//! Vector search functionality
//! 
//! Provides high-level search API with filtering, pagination, and result formatting.

use super::hnsw::HnswIndex;
use super::types::{
    Distance, Embedding, MetadataFilter, VectorConfig, VectorDocument, 
    VectorId, VectorSearchResult,
};
use super::embedding::EmbeddingProvider;
use crate::error::{KeraDBError, Result};

use parking_lot::RwLock;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;

/// A vector collection with search capabilities
pub struct VectorCollection {
    /// Collection name
    pub name: String,
    
    /// Configuration
    pub config: VectorConfig,
    
    /// HNSW index for ANN search
    index: HnswIndex,
    
    /// Document metadata storage (id -> metadata)
    metadata: RwLock<HashMap<VectorId, Value>>,
    
    /// Optional embedding provider for text-to-vector conversion
    embedding_provider: Option<Arc<dyn EmbeddingProvider>>,
}

impl VectorCollection {
    /// Create a new vector collection
    pub fn new(name: String, config: VectorConfig) -> Self {
        Self {
            name,
            config: config.clone(),
            index: HnswIndex::new(config),
            metadata: RwLock::new(HashMap::new()),
            embedding_provider: None,
        }
    }

    /// Create a collection with an embedding provider
    pub fn with_embedding_provider(
        name: String,
        config: VectorConfig,
        provider: Arc<dyn EmbeddingProvider>,
    ) -> Self {
        Self {
            name,
            config: config.clone(),
            index: HnswIndex::new(config),
            metadata: RwLock::new(HashMap::new()),
            embedding_provider: Some(provider),
        }
    }

    /// Insert a vector with optional metadata
    pub fn insert(&self, vector: Embedding, metadata: Option<Value>) -> Result<VectorId> {
        let id = self.index.insert(vector)?;
        
        if let Some(meta) = metadata {
            self.metadata.write().insert(id, meta);
        }
        
        Ok(id)
    }

    /// Insert text (requires embedding provider)
    pub fn insert_text(&self, text: &str, metadata: Option<Value>) -> Result<VectorId> {
        let provider = self.embedding_provider.as_ref().ok_or_else(|| {
            KeraDBError::InvalidFormat("No embedding provider configured".into())
        })?;
        
        let vector = provider.embed(text)?;
        let id = self.index.insert_with_metadata(vector, Some(text.to_string()), None)?;
        
        if let Some(meta) = metadata {
            self.metadata.write().insert(id, meta);
        }
        
        Ok(id)
    }

    /// Search by vector
    pub fn search(&self, query: &Embedding, k: usize) -> Result<Vec<VectorSearchResult>> {
        let results = self.index.search(query, k)?;
        
        self.build_search_results(results)
    }

    /// Search by text (requires embedding provider)
    pub fn search_text(&self, query: &str, k: usize) -> Result<Vec<VectorSearchResult>> {
        let provider = self.embedding_provider.as_ref().ok_or_else(|| {
            KeraDBError::InvalidFormat("No embedding provider configured".into())
        })?;
        
        let query_vector = provider.embed(query)?;
        self.search(&query_vector, k)
    }

    /// Search with metadata filtering
    pub fn search_filtered(
        &self,
        query: &Embedding,
        k: usize,
        filter: &MetadataFilter,
    ) -> Result<Vec<VectorSearchResult>> {
        // Over-fetch to account for filtering
        let fetch_k = k * 10;
        let results = self.index.search(query, fetch_k)?;
        
        let metadata = self.metadata.read();
        let filtered: Vec<_> = results
            .into_iter()
            .filter(|(id, _)| {
                metadata.get(id)
                    .map(|m| filter.matches(m))
                    .unwrap_or(true) // Include docs without metadata
            })
            .take(k)
            .collect();

        self.build_search_results(filtered)
    }

    /// Get a document by ID
    pub fn get(&self, id: VectorId) -> Option<VectorDocument> {
        self.index.get(id).map(|mut doc| {
            if let Some(meta) = self.metadata.read().get(&id) {
                doc.metadata = meta.clone();
            }
            doc
        })
    }

    /// Delete a document by ID
    pub fn delete(&self, id: VectorId) -> Result<bool> {
        self.metadata.write().remove(&id);
        self.index.delete(id)
    }

    /// Get the number of vectors in the collection
    pub fn len(&self) -> usize {
        self.index.len()
    }

    /// Check if the collection is empty
    pub fn is_empty(&self) -> bool {
        self.index.is_empty()
    }

    /// Build search results from raw (id, distance) pairs
    fn build_search_results(&self, results: Vec<(VectorId, f32)>) -> Result<Vec<VectorSearchResult>> {
        let metadata = self.metadata.read();
        
        Ok(results
            .into_iter()
            .enumerate()
            .filter_map(|(rank, (id, score))| {
                self.index.get(id).map(|mut doc| {
                    if let Some(meta) = metadata.get(&id) {
                        doc.metadata = meta.clone();
                    }
                    VectorSearchResult::new(doc, score, rank)
                })
            })
            .collect())
    }

    /// Serialize the collection to bytes
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        let index_bytes = self.index.to_bytes()?;
        let metadata = self.metadata.read().clone();
        
        // Serialize metadata as JSON string since bincode doesn't support serde_json::Value
        let metadata_json = serde_json::to_string(&metadata).map_err(|e| {
            KeraDBError::StorageError(format!("Failed to serialize metadata: {}", e))
        })?;
        
        let data = SerializedCollection {
            name: self.name.clone(),
            config: self.config.clone(),
            index_bytes,
            metadata_json,
        };
        
        bincode::serialize(&data).map_err(|e| {
            KeraDBError::StorageError(format!("Failed to serialize collection: {}", e))
        })
    }

    /// Deserialize a collection from bytes
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        let data: SerializedCollection = bincode::deserialize(bytes).map_err(|e| {
            KeraDBError::StorageError(format!("Failed to deserialize collection: {}", e))
        })?;
        
        let index = HnswIndex::from_bytes(&data.index_bytes)?;
        
        // Deserialize metadata from JSON string
        let metadata: HashMap<VectorId, Value> = serde_json::from_str(&data.metadata_json).map_err(|e| {
            KeraDBError::StorageError(format!("Failed to deserialize metadata: {}", e))
        })?;
        
        Ok(Self {
            name: data.name,
            config: data.config,
            index,
            metadata: RwLock::new(metadata),
            embedding_provider: None,
        })
    }
}

/// Serializable collection data
#[derive(serde::Serialize, serde::Deserialize)]
struct SerializedCollection {
    name: String,
    config: VectorConfig,
    index_bytes: Vec<u8>,
    metadata_json: String, // JSON string for metadata to avoid bincode issues
}

/// High-level vector searcher that manages multiple collections
pub struct VectorSearcher {
    collections: RwLock<HashMap<String, VectorCollection>>,
}

impl VectorSearcher {
    /// Create a new vector searcher
    pub fn new() -> Self {
        Self {
            collections: RwLock::new(HashMap::new()),
        }
    }

    /// Create a new collection
    pub fn create_collection(&self, name: &str, config: VectorConfig) -> Result<()> {
        let mut collections = self.collections.write();
        
        if collections.contains_key(name) {
            return Err(KeraDBError::CollectionExists(name.to_string()));
        }
        
        let collection = VectorCollection::new(name.to_string(), config);
        collections.insert(name.to_string(), collection);
        
        Ok(())
    }

    /// Get a collection by name
    pub fn get_collection(&self, name: &str) -> Option<&VectorCollection> {
        // Note: This is tricky with RwLock, might need RefCell pattern
        // For now, we provide methods that operate on collections directly
        None // Placeholder
    }

    /// Insert a vector into a collection
    pub fn insert(
        &self,
        collection: &str,
        vector: Embedding,
        metadata: Option<Value>,
    ) -> Result<VectorId> {
        let collections = self.collections.read();
        let coll = collections.get(collection).ok_or_else(|| {
            KeraDBError::CollectionNotFound(collection.to_string())
        })?;
        coll.insert(vector, metadata)
    }

    /// Search in a collection
    pub fn search(
        &self,
        collection: &str,
        query: &Embedding,
        k: usize,
    ) -> Result<Vec<VectorSearchResult>> {
        let collections = self.collections.read();
        let coll = collections.get(collection).ok_or_else(|| {
            KeraDBError::CollectionNotFound(collection.to_string())
        })?;
        coll.search(query, k)
    }

    /// List all collections
    pub fn list_collections(&self) -> Vec<String> {
        self.collections.read().keys().cloned().collect()
    }

    /// Drop a collection
    pub fn drop_collection(&self, name: &str) -> Result<bool> {
        Ok(self.collections.write().remove(name).is_some())
    }
}

impl Default for VectorSearcher {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vector::embedding::MockEmbeddingProvider;

    fn random_vector(dim: usize) -> Embedding {
        (0..dim).map(|_| rand::random::<f32>()).collect()
    }

    #[test]
    fn test_collection_basic() {
        let config = VectorConfig::new(64);
        let coll = VectorCollection::new("test".to_string(), config);

        // Insert vectors with metadata
        let id1 = coll.insert(
            random_vector(64),
            Some(serde_json::json!({"category": "A"})),
        ).unwrap();
        
        let id2 = coll.insert(
            random_vector(64),
            Some(serde_json::json!({"category": "B"})),
        ).unwrap();

        assert_eq!(coll.len(), 2);

        // Get by ID
        let doc = coll.get(id1).unwrap();
        assert_eq!(doc.metadata["category"], "A");
    }

    #[test]
    fn test_filtered_search() {
        let config = VectorConfig::new(32);
        let coll = VectorCollection::new("test".to_string(), config);

        // Insert vectors with different categories
        for i in 0..50 {
            let category = if i % 2 == 0 { "even" } else { "odd" };
            coll.insert(
                random_vector(32),
                Some(serde_json::json!({"category": category, "index": i})),
            ).unwrap();
        }

        // Search with filter
        let query = random_vector(32);
        let filter = MetadataFilter::new()
            .eq("category", serde_json::json!("even"));
        
        let results = coll.search_filtered(&query, 10, &filter).unwrap();
        
        // All results should have category "even"
        for r in &results {
            assert_eq!(r.document.metadata["category"], "even");
        }
    }

    #[test]
    fn test_text_search() {
        let config = VectorConfig::new(384);
        let provider = Arc::new(MockEmbeddingProvider::new(384));
        let coll = VectorCollection::with_embedding_provider(
            "docs".to_string(),
            config,
            provider,
        );

        // Insert text documents
        coll.insert_text("machine learning is a subset of artificial intelligence", None).unwrap();
        coll.insert_text("deep learning uses neural networks", None).unwrap();
        coll.insert_text("rust is a systems programming language", None).unwrap();

        // Search by text
        let results = coll.search_text("AI and machine learning", 2).unwrap();
        assert_eq!(results.len(), 2);
    }
}
