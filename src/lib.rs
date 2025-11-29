pub mod error;
pub mod types;
pub mod storage;
pub mod execution;
pub mod cli;
pub mod ffi;
pub mod vector;

use error::Result;
use execution::Executor;
use storage::Pager;
use types::{Config, DocumentId};
use serde_json::Value;
use std::path::{Path, PathBuf};

// Vector database imports (internal use)
use vector::embedding::{EmbeddingProvider, EmbeddingConfig, create_provider};
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;
use std::fs;
use std::io::{Read, Write};
use serde::{Serialize, Deserialize};

/// Serialized vector data format for persistence
#[derive(Serialize, Deserialize)]
struct SerializedVectorData {
    version: u32,
    collections: Vec<Vec<u8>>,
}

/// Main database interface
pub struct Database {
    executor: Executor,
    /// Vector collections for similarity search
    vector_collections: RwLock<HashMap<String, vector::search::VectorCollection>>,
    /// Default embedding provider
    embedding_provider: Option<Arc<dyn EmbeddingProvider>>,
    /// Path to the database file (for vector persistence)
    db_path: PathBuf,
}

impl Database {
    /// Get the path to the vector data file
    fn vector_data_path(db_path: &Path) -> PathBuf {
        let mut path = db_path.to_path_buf();
        let stem = path.file_stem().unwrap_or_default().to_string_lossy();
        let ext = path.extension().unwrap_or_default().to_string_lossy();
        path.set_file_name(format!("{}.vectors.{}", stem, ext));
        path
    }

    /// Load vector collections from disk
    fn load_vector_collections(db_path: &Path) -> HashMap<String, vector::search::VectorCollection> {
        let vector_path = Self::vector_data_path(db_path);
        if !vector_path.exists() {
            return HashMap::new();
        }

        match fs::File::open(&vector_path) {
            Ok(mut file) => {
                let mut data = Vec::new();
                if let Err(e) = file.read_to_end(&mut data) {
                    eprintln!("Failed to read vector data file: {}", e);
                    return HashMap::new();
                }
                
                // Deserialize the collections
                match bincode::deserialize::<SerializedVectorData>(&data) {
                    Ok(serialized) => {
                        let mut collections = HashMap::new();
                        for coll_data in serialized.collections {
                            match vector::search::VectorCollection::from_bytes(&coll_data) {
                                Ok(coll) => {
                                    collections.insert(coll.name.clone(), coll);
                                }
                                Err(e) => {
                                    eprintln!("Failed to deserialize vector collection: {}", e);
                                }
                            }
                        }
                        collections
                    }
                    Err(e) => {
                        eprintln!("Failed to deserialize vector data: {}", e);
                        HashMap::new()
                    }
                }
            }
            Err(e) => {
                eprintln!("Failed to open vector data file: {}", e);
                HashMap::new()
            }
        }
    }

    /// Save vector collections to disk
    fn save_vector_collections(&self) -> Result<()> {
        let vector_path = Self::vector_data_path(&self.db_path);
        let collections = self.vector_collections.read();
        
        if collections.is_empty() {
            // Remove vector file if no collections
            let _ = fs::remove_file(&vector_path);
            return Ok(());
        }
        
        let mut coll_bytes = Vec::new();
        for coll in collections.values() {
            match coll.to_bytes() {
                Ok(bytes) => coll_bytes.push(bytes),
                Err(e) => {
                    return Err(error::KeraDBError::StorageError(
                        format!("Failed to serialize vector collection: {}", e)
                    ));
                }
            }
        }
        
        let serialized = SerializedVectorData {
            version: 1,
            collections: coll_bytes,
        };
        
        let data = bincode::serialize(&serialized).map_err(|e| {
            error::KeraDBError::StorageError(format!("Failed to serialize vector data: {}", e))
        })?;
        
        let mut file = fs::File::create(&vector_path).map_err(|e| {
            error::KeraDBError::StorageError(format!("Failed to create vector file: {}", e))
        })?;
        
        file.write_all(&data).map_err(|e| {
            error::KeraDBError::StorageError(format!("Failed to write vector data: {}", e))
        })?;
        
        file.sync_all().map_err(|e| {
            error::KeraDBError::StorageError(format!("Failed to sync vector file: {}", e))
        })?;
        
        Ok(())
    }

    /// Create a new database file
    pub fn create<P: AsRef<Path>>(path: P) -> Result<Self> {
        let config = Config::default();
        Self::create_with_config(path, config)
    }

    /// Create a new database with custom configuration
    pub fn create_with_config<P: AsRef<Path>>(path: P, config: Config) -> Result<Self> {
        let path = path.as_ref();
        let pager = Pager::create(path, config.page_size)?;
        let executor = Executor::new(pager, config.cache_size);
        
        Ok(Self { 
            executor,
            vector_collections: RwLock::new(HashMap::new()),
            embedding_provider: None,
            db_path: path.to_path_buf(),
        })
    }

    /// Open an existing database file
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let config = Config::default();
        Self::open_with_config(path, config)
    }

    /// Open an existing database with custom configuration
    pub fn open_with_config<P: AsRef<Path>>(path: P, config: Config) -> Result<Self> {
        let path = path.as_ref();
        let pager = Pager::open(path)?;
        let executor = Executor::new(pager, config.cache_size);
        
        // Load vector collections from disk
        let vector_collections = Self::load_vector_collections(path);
        
        Ok(Self { 
            executor,
            vector_collections: RwLock::new(vector_collections),
            embedding_provider: None,
            db_path: path.to_path_buf(),
        })
    }

    /// Insert a document into a collection
    /// 
    /// # Example
    /// ```ignore
    /// let doc = json!({"name": "Alice", "age": 30});
    /// let id = db.insert("users", doc)?;
    /// ```
    pub fn insert(&self, collection: &str, data: Value) -> Result<DocumentId> {
        self.executor.insert(collection, data)
    }

    /// Find a document by ID
    /// 
    /// # Example
    /// ```ignore
    /// let doc = db.find_by_id("users", "abc123")?;
    /// ```
    pub fn find_by_id(&self, collection: &str, doc_id: &str) -> Result<types::Document> {
        self.executor.find_by_id(collection, doc_id)
    }

    /// Update a document
    /// 
    /// # Example
    /// ```ignore
    /// db.update("users", "abc123", json!({"age": 31}))?;
    /// ```
    pub fn update(&self, collection: &str, doc_id: &str, data: Value) -> Result<types::Document> {
        self.executor.update(collection, doc_id, data)
    }

    /// Delete a document
    /// 
    /// # Example
    /// ```ignore
    /// db.delete("users", "abc123")?;
    /// ```
    pub fn delete(&self, collection: &str, doc_id: &str) -> Result<types::Document> {
        self.executor.delete(collection, doc_id)
    }

    /// Find all documents in a collection
    /// 
    /// # Example
    /// ```ignore
    /// let docs = db.find_all("users", None, None)?;
    /// let page = db.find_all("users", Some(10), Some(20))?; // limit 10, skip 20
    /// ```
    pub fn find_all(&self, collection: &str, limit: Option<usize>, skip: Option<usize>) -> Result<Vec<types::Document>> {
        self.executor.find_all(collection, limit, skip)
    }

    /// Count documents in a collection
    /// 
    /// # Example
    /// ```ignore
    /// let count = db.count("users");
    /// ```
    pub fn count(&self, collection: &str) -> usize {
        self.executor.count(collection)
    }

    /// List all collections with document counts
    /// 
    /// # Example
    /// ```ignore
    /// let collections = db.list_collections();
    /// for (name, count) in collections {
    ///     println!("{}: {} documents", name, count);
    /// }
    /// ```
    pub fn list_collections(&self) -> Vec<(String, usize)> {
        self.executor.list_collections()
    }

    /// Sync all changes to disk (including vector data)
    pub fn sync(&self) -> Result<()> {
        // Sync document data
        self.executor.sync()?;
        
        // Sync vector collections
        self.save_vector_collections()?;
        
        Ok(())
    }

    // ============================================================
    // Vector Database API
    // ============================================================

    /// Create a vector collection for similarity search
    /// 
    /// # Example
    /// ```ignore
    /// use keradb::vector::VectorConfig;
    /// 
    /// let db = Database::create("mydata.ndb")?;
    /// db.create_vector_collection("embeddings", VectorConfig::new(384))?;
    /// ```
    pub fn create_vector_collection(&self, name: &str, config: vector::VectorConfig) -> Result<()> {
        let mut collections = self.vector_collections.write();
        
        if collections.contains_key(name) {
            return Err(error::KeraDBError::CollectionExists(name.to_string()));
        }
        
        let collection = if let Some(ref provider) = self.embedding_provider {
            vector::search::VectorCollection::with_embedding_provider(
                name.to_string(),
                config,
                provider.clone(),
            )
        } else {
            vector::search::VectorCollection::new(name.to_string(), config)
        };
        
        collections.insert(name.to_string(), collection);
        drop(collections); // Release the lock before saving
        
        // Auto-save vector collections
        self.save_vector_collections()?;
        
        Ok(())
    }

    /// Insert a vector into a collection
    /// 
    /// # Example
    /// ```ignore
    /// let vector = vec![0.1, 0.2, 0.3, ...]; // 384 dimensions
    /// let id = db.insert_vector("embeddings", vector, Some(json!({"source": "doc1"})))?;
    /// ```
    pub fn insert_vector(
        &self,
        collection: &str,
        vector: Embedding,
        metadata: Option<Value>,
    ) -> Result<VectorId> {
        let id = {
            let collections = self.vector_collections.read();
            let coll = collections.get(collection).ok_or_else(|| {
                error::KeraDBError::CollectionNotFound(collection.to_string())
            })?;
            coll.insert(vector, metadata)?
        };
        
        // Auto-save vector collections after insert
        self.save_vector_collections()?;
        
        Ok(id)
    }

    /// Insert text into a vector collection (requires embedding provider)
    /// 
    /// # Example
    /// ```ignore
    /// db.insert_text("documents", "Machine learning is fascinating", 
    ///     Some(json!({"category": "tech"})))?;
    /// ```
    pub fn insert_text(
        &self,
        collection: &str,
        text: &str,
        metadata: Option<Value>,
    ) -> Result<VectorId> {
        let id = {
            let collections = self.vector_collections.read();
            let coll = collections.get(collection).ok_or_else(|| {
                error::KeraDBError::CollectionNotFound(collection.to_string())
            })?;
            coll.insert_text(text, metadata)?
        };
        
        // Auto-save vector collections after insert
        self.save_vector_collections()?;
        
        Ok(id)
    }

    /// Search for similar vectors
    /// 
    /// # Example
    /// ```ignore
    /// let query = vec![0.1, 0.2, 0.3, ...];
    /// let results = db.vector_search("embeddings", &query, 10)?;
    /// for result in results {
    ///     println!("ID: {}, Score: {}", result.document.id, result.score);
    /// }
    /// ```
    pub fn vector_search(
        &self,
        collection: &str,
        query: &Embedding,
        k: usize,
    ) -> Result<Vec<VectorSearchResult>> {
        let collections = self.vector_collections.read();
        let coll = collections.get(collection).ok_or_else(|| {
            error::KeraDBError::CollectionNotFound(collection.to_string())
        })?;
        coll.search(query, k)
    }

    /// Search for similar vectors by text query
    /// 
    /// # Example
    /// ```ignore
    /// let results = db.vector_search_text("documents", "artificial intelligence", 10)?;
    /// ```
    pub fn vector_search_text(
        &self,
        collection: &str,
        query: &str,
        k: usize,
    ) -> Result<Vec<VectorSearchResult>> {
        let collections = self.vector_collections.read();
        let coll = collections.get(collection).ok_or_else(|| {
            error::KeraDBError::CollectionNotFound(collection.to_string())
        })?;
        coll.search_text(query, k)
    }

    /// Search with metadata filtering
    /// 
    /// # Example
    /// ```ignore
    /// let filter = MetadataFilter::new().eq("category", json!("tech"));
    /// let results = db.vector_search_filtered("documents", &query, 10, &filter)?;
    /// ```
    pub fn vector_search_filtered(
        &self,
        collection: &str,
        query: &Embedding,
        k: usize,
        filter: &MetadataFilter,
    ) -> Result<Vec<VectorSearchResult>> {
        let collections = self.vector_collections.read();
        let coll = collections.get(collection).ok_or_else(|| {
            error::KeraDBError::CollectionNotFound(collection.to_string())
        })?;
        coll.search_filtered(query, k, filter)
    }

    /// Get a vector document by ID
    pub fn get_vector(&self, collection: &str, id: VectorId) -> Result<Option<vector::VectorDocument>> {
        let collections = self.vector_collections.read();
        let coll = collections.get(collection).ok_or_else(|| {
            error::KeraDBError::CollectionNotFound(collection.to_string())
        })?;
        Ok(coll.get(id))
    }

    /// Delete a vector by ID
    pub fn delete_vector(&self, collection: &str, id: VectorId) -> Result<bool> {
        let result = {
            let collections = self.vector_collections.read();
            let coll = collections.get(collection).ok_or_else(|| {
                error::KeraDBError::CollectionNotFound(collection.to_string())
            })?;
            coll.delete(id)?
        };
        
        // Auto-save vector collections after delete
        self.save_vector_collections()?;
        
        Ok(result)
    }

    /// List all vector collections
    pub fn list_vector_collections(&self) -> Vec<(String, usize)> {
        self.vector_collections
            .read()
            .iter()
            .map(|(name, coll)| (name.clone(), coll.len()))
            .collect()
    }

    /// Drop a vector collection
    pub fn drop_vector_collection(&self, name: &str) -> Result<bool> {
        let removed = self.vector_collections.write().remove(name).is_some();
        
        // Auto-save vector collections after drop
        self.save_vector_collections()?;
        
        Ok(removed)
    }

    /// Set the default embedding provider for text-to-vector conversion
    pub fn set_embedding_provider(&mut self, config: EmbeddingConfig) -> Result<()> {
        self.embedding_provider = Some(create_provider(config)?);
        Ok(())
    }

    /// Get vector collection statistics
    pub fn vector_stats(&self, collection: &str) -> Result<vector::VectorCollectionStats> {
        let collections = self.vector_collections.read();
        let coll = collections.get(collection).ok_or_else(|| {
            error::KeraDBError::CollectionNotFound(collection.to_string())
        })?;
        
        Ok(vector::VectorCollectionStats {
            name: coll.name.clone(),
            vector_count: coll.len(),
            dimensions: coll.config.dimensions,
            distance: coll.config.distance,
            memory_bytes: coll.len() * coll.config.dimensions * 4, // Approximate
            hnsw_layers: coll.config.m,
            lazy_embedding: coll.config.lazy_embedding,
            compression_mode: coll.config.compression.mode,
            compression_ratio: 0.0, // TODO: Get actual compression ratio from store
            anchor_count: 0,
            delta_count: 0,
        })
    }
}

// Re-export commonly used types
pub use error::KeraDBError;
pub use types::Document;

// Re-export vector types for public API
pub use vector::{
    VectorConfig, VectorDocument, VectorSearchResult, 
    Embedding, VectorId, Distance, MetadataFilter, VectorCollectionStats,
    CompressionConfig, CompressionMode, CompressionStats,
};
pub use vector::search::VectorCollection;

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tempfile::tempdir;

    #[test]
    fn test_database_create_and_open() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.ndb");

        // Create
        let db = Database::create(&path).unwrap();
        let id = db.insert("users", json!({"name": "Alice"})).unwrap();
        db.sync().unwrap();
        drop(db);

        // Open
        let db = Database::open(&path).unwrap();
        let doc = db.find_by_id("users", &id).unwrap();
        assert_eq!(doc.data.get("name").unwrap(), "Alice");
    }

    #[test]
    fn test_full_crud_cycle() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.ndb");
        let db = Database::create(&path).unwrap();

        // Insert
        let id1 = db.insert("users", json!({"name": "Alice", "age": 30})).unwrap();
        let id2 = db.insert("users", json!({"name": "Bob", "age": 25})).unwrap();

        // Find
        let alice = db.find_by_id("users", &id1).unwrap();
        assert_eq!(alice.data.get("name").unwrap(), "Alice");

        // Update
        db.update("users", &id1, json!({"name": "Alice", "age": 31})).unwrap();
        let updated = db.find_by_id("users", &id1).unwrap();
        assert_eq!(updated.data.get("age").unwrap(), 31);

        // Count
        assert_eq!(db.count("users"), 2);

        // Find all
        let all = db.find_all("users", None, None).unwrap();
        assert_eq!(all.len(), 2);

        // Delete
        db.delete("users", &id2).unwrap();
        assert_eq!(db.count("users"), 1);

        // List collections
        let collections = db.list_collections();
        assert_eq!(collections.len(), 1);
        assert_eq!(collections[0].0, "users");
        assert_eq!(collections[0].1, 1);
    }
}
