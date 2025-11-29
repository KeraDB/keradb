use crate::error::{KeraDBError, Result};
use crate::execution::Index;
use crate::storage::{BufferPool, Pager, Serializer};
use crate::types::{CollectionMetadata, Document, DocumentId, PageType};
use parking_lot::RwLock;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;

/// Executor handles CRUD operations
pub struct Executor {
    pager: Arc<RwLock<Pager>>,
    buffer_pool: BufferPool,
    index: Index,
    collections: Arc<RwLock<HashMap<String, CollectionMetadata>>>,
}

impl Executor {
    pub fn new(pager: Pager, cache_size: usize) -> Self {
        let executor = Self {
            pager: Arc::new(RwLock::new(pager)),
            buffer_pool: BufferPool::new(cache_size),
            index: Index::new(),
            collections: Arc::new(RwLock::new(HashMap::new())),
        };
        
        // Rebuild index from existing pages
        if let Err(e) = executor.rebuild_index() {
            eprintln!("Warning: Failed to rebuild index: {}", e);
        }
        
        executor
    }
    
    /// Rebuild the index by scanning all pages in the database
    fn rebuild_index(&self) -> Result<()> {
        let pager = self.pager.read();
        let page_count = pager.page_count();
        drop(pager);
        
        for page_num in 0..page_count {
            let mut pager = self.pager.write();
            let page = match pager.read_page(page_num) {
                Ok(p) => p,
                Err(_) => continue, // Skip invalid pages
            };
            drop(pager);
            
            if page.page_type != PageType::Data {
                continue;
            }
            
            // Try to extract document from page
            if let Ok(doc) = self.extract_document_from_page(&page) {
                // Extract collection name from document metadata
                // For now, we'll use a simple approach: store collection in a special field
                if let Some(collection_name) = doc.data.get("_collection").and_then(|v| v.as_str()) {
                    self.index.insert(collection_name, doc.id.clone(), page_num, 0)?;
                    self.update_collection_metadata(collection_name, 1);
                }
            }
        }
        
        Ok(())
    }

    /// Insert a document into a collection
    pub fn insert(&self, collection: &str, mut data: Value) -> Result<DocumentId> {
        // Ensure data is an object
        if !data.is_object() {
            return Err(KeraDBError::InvalidDocument(
                "Document must be a JSON object".to_string(),
            ));
        }

        // Add collection name to data for persistence
        if let Value::Object(ref mut map) = data {
            map.insert("_collection".to_string(), Value::String(collection.to_string()));
        }

        // Create document with auto-generated ID if not provided
        let doc = if let Some(id_val) = data.get("_id") {
            let id = id_val.as_str()
                .ok_or_else(|| KeraDBError::InvalidDocument("_id must be a string".to_string()))?
                .to_string();
            
            // Remove _id from data
            if let Value::Object(ref mut map) = data {
                map.remove("_id");
            }
            
            Document::with_id(id, data)
        } else {
            Document::new(data)
        };

        // Serialize document
        let doc_bytes = Serializer::serialize(&doc)?;

        // Allocate page and write document
        let mut pager = self.pager.write();
        let page_num = pager.allocate_page(PageType::Data)?;
        
        let mut page = pager.read_page(page_num)?;
        
        // Simple storage: write length + data
        let len_bytes = (doc_bytes.len() as u32).to_le_bytes();
        page.data[0..4].copy_from_slice(&len_bytes);
        
        if doc_bytes.len() + 4 > page.data.len() {
            return Err(KeraDBError::StorageError(
                "Document too large for page".to_string(),
            ));
        }
        
        page.data[4..4 + doc_bytes.len()].copy_from_slice(&doc_bytes);
        page.checksum = crc32fast::hash(&page.data);
        
        pager.write_page(&page)?;
        drop(pager);

        // Update index
        self.index.insert(collection, doc.id.clone(), page_num, 0)?;

        // Update collection metadata
        self.update_collection_metadata(collection, 1);

        // Cache the page
        self.buffer_pool.put(page);

        Ok(doc.id)
    }

    /// Find a document by ID
    pub fn find_by_id(&self, collection: &str, doc_id: &str) -> Result<Document> {
        // Look up in index
        let entry = self.index.find(collection, doc_id)
            .ok_or_else(|| KeraDBError::DocumentNotFound(doc_id.to_string()))?;

        // Check cache first
        if let Some(page) = self.buffer_pool.get(entry.page_num) {
            return self.extract_document_from_page(&page);
        }

        // Read from disk
        let mut pager = self.pager.write();
        let page = pager.read_page(entry.page_num)?;
        drop(pager);

        // Cache the page
        self.buffer_pool.put(page.clone());

        self.extract_document_from_page(&page)
    }

    /// Update a document
    pub fn update(&self, collection: &str, doc_id: &str, mut data: Value) -> Result<Document> {
        // Ensure data is an object
        if !data.is_object() {
            return Err(KeraDBError::InvalidDocument(
                "Document must be a JSON object".to_string(),
            ));
        }

        // Check if document exists
        let entry = self.index.find(collection, doc_id)
            .ok_or_else(|| KeraDBError::DocumentNotFound(doc_id.to_string()))?;

        // Add collection name to data for persistence (needed for index rebuild on reopen)
        if let Value::Object(ref mut map) = data {
            map.insert("_collection".to_string(), Value::String(collection.to_string()));
        }

        // Create updated document
        let doc = Document::with_id(doc_id.to_string(), data);

        // Serialize document
        let doc_bytes = Serializer::serialize(&doc)?;

        // Write to same page (simple approach - no overflow handling yet)
        let mut pager = self.pager.write();
        let mut page = pager.read_page(entry.page_num)?;
        
        let len_bytes = (doc_bytes.len() as u32).to_le_bytes();
        page.data[0..4].copy_from_slice(&len_bytes);
        
        if doc_bytes.len() + 4 > page.data.len() {
            return Err(KeraDBError::StorageError(
                "Updated document too large for page".to_string(),
            ));
        }
        
        page.data[4..4 + doc_bytes.len()].copy_from_slice(&doc_bytes);
        page.checksum = crc32fast::hash(&page.data);
        
        pager.write_page(&page)?;
        drop(pager);

        // Invalidate cache
        self.buffer_pool.remove(entry.page_num);

        Ok(doc)
    }

    /// Delete a document
    pub fn delete(&self, collection: &str, doc_id: &str) -> Result<Document> {
        // Get the document first
        let doc = self.find_by_id(collection, doc_id)?;

        // Remove from index
        let entry = self.index.remove(collection, doc_id)
            .ok_or_else(|| KeraDBError::DocumentNotFound(doc_id.to_string()))?;

        // Mark page as free (simple approach)
        let mut pager = self.pager.write();
        let mut page = pager.read_page(entry.page_num)?;
        page.page_type = PageType::Free;
        page.data = vec![0u8; page.data.len()];
        page.checksum = crc32fast::hash(&page.data);
        pager.write_page(&page)?;
        drop(pager);

        // Invalidate cache
        self.buffer_pool.remove(entry.page_num);

        // Update collection metadata
        self.update_collection_metadata(collection, -1);

        Ok(doc)
    }

    /// Find all documents in a collection
    pub fn find_all(&self, collection: &str, limit: Option<usize>, skip: Option<usize>) -> Result<Vec<Document>> {
        let doc_ids = self.index.list_ids(collection);
        
        let skip = skip.unwrap_or(0);
        let limit = limit.unwrap_or(usize::MAX);
        
        let mut documents = Vec::new();
        
        for doc_id in doc_ids.iter().skip(skip).take(limit) {
            if let Ok(doc) = self.find_by_id(collection, doc_id) {
                documents.push(doc);
            }
        }

        Ok(documents)
    }

    /// Count documents in a collection
    pub fn count(&self, collection: &str) -> usize {
        self.index.count(collection)
    }

    /// List all collections
    pub fn list_collections(&self) -> Vec<(String, usize)> {
        let collections = self.index.list_collections();
        collections
            .into_iter()
            .map(|name| {
                let count = self.index.count(&name);
                (name, count)
            })
            .collect()
    }

    /// Sync data to disk
    pub fn sync(&self) -> Result<()> {
        let mut pager = self.pager.write();
        pager.sync()?;
        Ok(())
    }

    // Helper methods

    fn extract_document_from_page(&self, page: &crate::storage::pager::Page) -> Result<Document> {
        if page.data.len() < 4 {
            return Err(KeraDBError::StorageError(
                "Invalid page data".to_string(),
            ));
        }

        let len = u32::from_le_bytes([
            page.data[0],
            page.data[1],
            page.data[2],
            page.data[3],
        ]) as usize;

        if len == 0 || len + 4 > page.data.len() {
            return Err(KeraDBError::StorageError(
                "Invalid document length".to_string(),
            ));
        }

        let doc_bytes = &page.data[4..4 + len];
        Serializer::deserialize(doc_bytes)
    }

    fn update_collection_metadata(&self, collection: &str, delta: i32) {
        let mut collections = self.collections.write();
        let metadata = collections
            .entry(collection.to_string())
            .or_insert_with(|| CollectionMetadata::new(collection.to_string()));
        
        if delta > 0 {
            metadata.document_count += delta as usize;
        } else {
            metadata.document_count = metadata.document_count.saturating_sub((-delta) as usize);
        }
        
        metadata.updated_at = chrono::Utc::now().timestamp();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tempfile::tempdir;

    #[test]
    fn test_insert_and_find() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.ndb");
        
        let pager = Pager::create(&path, 4096).unwrap();
        let executor = Executor::new(pager, 10);

        let doc_data = json!({"name": "Alice", "age": 30});
        let doc_id = executor.insert("users", doc_data.clone()).unwrap();

        let found = executor.find_by_id("users", &doc_id).unwrap();
        assert_eq!(found.data.get("name").unwrap(), "Alice");
    }

    #[test]
    fn test_update() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.ndb");
        
        let pager = Pager::create(&path, 4096).unwrap();
        let executor = Executor::new(pager, 10);

        let doc_id = executor.insert("users", json!({"name": "Alice", "age": 30})).unwrap();
        
        executor.update("users", &doc_id, json!({"name": "Alice", "age": 31})).unwrap();
        
        let found = executor.find_by_id("users", &doc_id).unwrap();
        assert_eq!(found.data.get("age").unwrap(), 31);
    }

    #[test]
    fn test_delete() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.ndb");
        
        let pager = Pager::create(&path, 4096).unwrap();
        let executor = Executor::new(pager, 10);

        let doc_id = executor.insert("users", json!({"name": "Alice"})).unwrap();
        
        executor.delete("users", &doc_id).unwrap();
        
        assert!(executor.find_by_id("users", &doc_id).is_err());
    }
}
