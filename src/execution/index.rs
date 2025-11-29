use crate::error::{KeraDBError, Result};
use crate::types::DocumentId;
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

/// Simple in-memory B-Tree index (primary key)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexEntry {
    pub doc_id: DocumentId,
    pub page_num: u32,
    pub offset: usize,
}

pub struct Index {
    // Collection name -> (doc_id -> IndexEntry)
    indexes: Arc<DashMap<String, HashMap<DocumentId, IndexEntry>>>,
}

impl Index {
    pub fn new() -> Self {
        Self {
            indexes: Arc::new(DashMap::new()),
        }
    }

    /// Insert an entry into the index
    pub fn insert(&self, collection: &str, doc_id: DocumentId, page_num: u32, offset: usize) -> Result<()> {
        let entry = IndexEntry {
            doc_id: doc_id.clone(),
            page_num,
            offset,
        };

        let mut indexes = self.indexes.entry(collection.to_string()).or_default();
        
        if indexes.contains_key(&doc_id) {
            return Err(KeraDBError::DuplicateKey(doc_id));
        }
        
        indexes.insert(doc_id, entry);
        Ok(())
    }

    /// Find an entry in the index
    pub fn find(&self, collection: &str, doc_id: &str) -> Option<IndexEntry> {
        self.indexes
            .get(collection)
            .and_then(|idx| idx.get(doc_id).cloned())
    }

    /// Remove an entry from the index
    pub fn remove(&self, collection: &str, doc_id: &str) -> Option<IndexEntry> {
        self.indexes
            .get_mut(collection)
            .and_then(|mut idx| idx.remove(doc_id))
    }

    /// Get all document IDs in a collection
    pub fn list_ids(&self, collection: &str) -> Vec<DocumentId> {
        self.indexes
            .get(collection)
            .map(|idx| idx.keys().cloned().collect())
            .unwrap_or_default()
    }

    /// Get count of documents in a collection
    pub fn count(&self, collection: &str) -> usize {
        self.indexes
            .get(collection)
            .map(|idx| idx.len())
            .unwrap_or(0)
    }

    /// List all collections
    pub fn list_collections(&self) -> Vec<String> {
        self.indexes.iter().map(|entry| entry.key().clone()).collect()
    }
}

impl Default for Index {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_index_operations() {
        let index = Index::new();
        
        index.insert("users", "id1".to_string(), 0, 0).unwrap();
        index.insert("users", "id2".to_string(), 0, 100).unwrap();
        
        assert_eq!(index.count("users"), 2);
        
        let entry = index.find("users", "id1").unwrap();
        assert_eq!(entry.doc_id, "id1");
        
        index.remove("users", "id1");
        assert_eq!(index.count("users"), 1);
    }
}
