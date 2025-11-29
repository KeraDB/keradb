//! Embedding providers for vector generation
//! 
//! Supports multiple embedding backends:
//! - Local models via ONNX Runtime (candle/ort)
//! - OpenAI API
//! - Custom embedding functions

use super::types::Embedding;
use crate::error::{KeraDBError, Result};

use std::sync::Arc;

/// Trait for embedding providers
pub trait EmbeddingProvider: Send + Sync {
    /// Generate embedding for a single text
    fn embed(&self, text: &str) -> Result<Embedding>;
    
    /// Generate embeddings for multiple texts (batched)
    fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Embedding>> {
        texts.iter().map(|t| self.embed(t)).collect()
    }
    
    /// Get the dimensionality of embeddings
    fn dimensions(&self) -> usize;
    
    /// Get the model name
    fn model_name(&self) -> &str;
}

/// Mock embedding provider for testing (generates random normalized vectors)
pub struct MockEmbeddingProvider {
    dimensions: usize,
}

impl MockEmbeddingProvider {
    pub fn new(dimensions: usize) -> Self {
        Self { dimensions }
    }
}

impl EmbeddingProvider for MockEmbeddingProvider {
    fn embed(&self, _text: &str) -> Result<Embedding> {
        // Generate deterministic "random" vector based on text hash
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        
        let mut hasher = DefaultHasher::new();
        _text.hash(&mut hasher);
        let seed = hasher.finish();
        
        let mut rng_state = seed;
        let mut vector: Embedding = (0..self.dimensions)
            .map(|_| {
                // Simple LCG random number generator
                rng_state = rng_state.wrapping_mul(6364136223846793005).wrapping_add(1);
                ((rng_state >> 33) as f32) / (u32::MAX as f32) * 2.0 - 1.0
            })
            .collect();
        
        // Normalize
        let norm: f32 = vector.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            for x in &mut vector {
                *x /= norm;
            }
        }
        
        Ok(vector)
    }
    
    fn dimensions(&self) -> usize {
        self.dimensions
    }
    
    fn model_name(&self) -> &str {
        "mock"
    }
}

/// Simple term-frequency embedding (for basic semantic search without ML)
pub struct TfIdfEmbeddingProvider {
    dimensions: usize,
}

impl TfIdfEmbeddingProvider {
    pub fn new(dimensions: usize) -> Self {
        Self { dimensions }
    }
    
    /// Simple hash-based feature extraction
    fn text_to_features(&self, text: &str) -> Embedding {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        
        let mut vector = vec![0.0f32; self.dimensions];
        
        // Tokenize and hash each word to a dimension
        for word in text.split_whitespace() {
            let word = word.to_lowercase();
            let mut hasher = DefaultHasher::new();
            word.hash(&mut hasher);
            let idx = (hasher.finish() as usize) % self.dimensions;
            vector[idx] += 1.0;
        }
        
        // L2 normalize
        let norm: f32 = vector.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            for x in &mut vector {
                *x /= norm;
            }
        }
        
        vector
    }
}

impl EmbeddingProvider for TfIdfEmbeddingProvider {
    fn embed(&self, text: &str) -> Result<Embedding> {
        Ok(self.text_to_features(text))
    }
    
    fn dimensions(&self) -> usize {
        self.dimensions
    }
    
    fn model_name(&self) -> &str {
        "tfidf-hash"
    }
}

/// Configuration for embedding providers
#[derive(Debug, Clone)]
pub enum EmbeddingConfig {
    /// Mock provider for testing
    Mock { dimensions: usize },
    
    /// Simple TF-IDF hash-based embeddings
    TfIdf { dimensions: usize },
    
    /// OpenAI API
    #[cfg(feature = "openai")]
    OpenAI { 
        api_key: String,
        model: String,
    },
    
    /// Local ONNX model
    #[cfg(feature = "onnx")]
    Onnx {
        model_path: String,
        dimensions: usize,
    },
}

impl Default for EmbeddingConfig {
    fn default() -> Self {
        EmbeddingConfig::TfIdf { dimensions: 384 }
    }
}

/// Create an embedding provider from configuration
pub fn create_provider(config: EmbeddingConfig) -> Result<Arc<dyn EmbeddingProvider>> {
    match config {
        EmbeddingConfig::Mock { dimensions } => {
            Ok(Arc::new(MockEmbeddingProvider::new(dimensions)))
        }
        EmbeddingConfig::TfIdf { dimensions } => {
            Ok(Arc::new(TfIdfEmbeddingProvider::new(dimensions)))
        }
        #[cfg(feature = "openai")]
        EmbeddingConfig::OpenAI { api_key, model } => {
            // OpenAI implementation would go here
            Err(KeraDBError::NotImplemented("OpenAI embedding not yet implemented".into()))
        }
        #[cfg(feature = "onnx")]
        EmbeddingConfig::Onnx { model_path, dimensions } => {
            // ONNX implementation would go here
            Err(KeraDBError::NotImplemented("ONNX embedding not yet implemented".into()))
        }
    }
}

/// Utility: Normalize a vector to unit length
pub fn normalize_embedding(embedding: &mut Embedding) {
    let norm: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        for x in embedding.iter_mut() {
            *x /= norm;
        }
    }
}

/// Utility: Check if an embedding is normalized
pub fn is_normalized(embedding: &Embedding) -> bool {
    let norm: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
    (norm - 1.0).abs() < 1e-5
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mock_provider() {
        let provider = MockEmbeddingProvider::new(384);
        
        let e1 = provider.embed("hello world").unwrap();
        let e2 = provider.embed("hello world").unwrap();
        let e3 = provider.embed("goodbye world").unwrap();
        
        // Same text should give same embedding
        assert_eq!(e1, e2);
        
        // Different text should give different embedding
        assert_ne!(e1, e3);
        
        // Embeddings should be normalized
        assert!(is_normalized(&e1));
        assert!(is_normalized(&e3));
    }

    #[test]
    fn test_tfidf_provider() {
        let provider = TfIdfEmbeddingProvider::new(256);
        
        let e1 = provider.embed("machine learning is great").unwrap();
        let e2 = provider.embed("deep learning is awesome").unwrap();
        
        assert_eq!(e1.len(), 256);
        assert!(is_normalized(&e1));
        
        // Similar texts should have some similarity
        let dot: f32 = e1.iter().zip(&e2).map(|(a, b)| a * b).sum();
        assert!(dot > 0.0); // Should share some words
    }
}
