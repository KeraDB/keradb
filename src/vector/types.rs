//! Vector types and data structures

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use super::compression::{CompressionConfig, CompressionMode};

/// A vector embedding (array of f32 values)
pub type Embedding = Vec<f32>;

/// Unique identifier for a vector entry
pub type VectorId = u64;

/// Distance metric for similarity calculations
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Distance {
    /// Cosine similarity (1 - cosine_sim), range [0, 2]
    #[default]
    Cosine,
    /// Euclidean (L2) distance
    Euclidean,
    /// Dot product (negative for similarity ranking)
    DotProduct,
    /// Manhattan (L1) distance
    Manhattan,
}

impl Distance {
    /// Returns the name of the distance metric
    pub fn name(&self) -> &'static str {
        match self {
            Distance::Cosine => "cosine",
            Distance::Euclidean => "euclidean",
            Distance::DotProduct => "dot_product",
            Distance::Manhattan => "manhattan",
        }
    }
}

/// Configuration for a vector collection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorConfig {
    /// Dimensionality of vectors
    pub dimensions: usize,
    
    /// Distance metric to use
    #[serde(default)]
    pub distance: Distance,
    
    /// HNSW M parameter: number of connections per node
    /// Higher = better recall, more memory. Default: 16
    #[serde(default = "default_m")]
    pub m: usize,
    
    /// HNSW ef_construction: size of dynamic candidate list during construction
    /// Higher = better index quality, slower build. Default: 200
    #[serde(default = "default_ef_construction")]
    pub ef_construction: usize,
    
    /// HNSW ef_search: size of dynamic candidate list during search
    /// Higher = better recall, slower search. Default: 50
    #[serde(default = "default_ef_search")]
    pub ef_search: usize,
    
    /// Enable lazy embedding mode (LEANN-style)
    /// If true, store text and recompute embeddings on-demand
    #[serde(default)]
    pub lazy_embedding: bool,
    
    /// Embedding model name (for lazy embedding mode)
    #[serde(default)]
    pub embedding_model: Option<String>,
    
    /// LEANN-style compression configuration
    #[serde(default)]
    pub compression: CompressionConfig,
}

fn default_m() -> usize { 16 }
fn default_ef_construction() -> usize { 200 }
fn default_ef_search() -> usize { 50 }

impl Default for VectorConfig {
    fn default() -> Self {
        Self {
            dimensions: 384, // Default for all-MiniLM-L6-v2
            distance: Distance::Cosine,
            m: 16,
            ef_construction: 200,
            ef_search: 50,
            lazy_embedding: false,
            embedding_model: None,
            compression: CompressionConfig::default(),
        }
    }
}

impl VectorConfig {
    /// Create a new vector config with specified dimensions
    pub fn new(dimensions: usize) -> Self {
        Self {
            dimensions,
            ..Default::default()
        }
    }

    /// Set the distance metric
    pub fn with_distance(mut self, distance: Distance) -> Self {
        self.distance = distance;
        self
    }

    /// Set HNSW M parameter
    pub fn with_m(mut self, m: usize) -> Self {
        self.m = m;
        self
    }

    /// Enable lazy embedding mode
    pub fn with_lazy_embedding(mut self, model: &str) -> Self {
        self.lazy_embedding = true;
        self.embedding_model = Some(model.to_string());
        self
    }
    
    /// Enable LEANN-style delta compression
    pub fn with_compression(mut self, config: CompressionConfig) -> Self {
        self.compression = config;
        self
    }
    
    /// Enable delta compression with default settings (up to 97% savings)
    pub fn with_delta_compression(mut self) -> Self {
        self.compression = CompressionConfig::delta();
        self
    }
    
    /// Enable aggressive quantized compression
    pub fn with_quantized_compression(mut self) -> Self {
        self.compression = CompressionConfig::quantized();
        self
    }
}

/// A vector document with optional metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorDocument {
    /// Unique identifier
    pub id: VectorId,
    
    /// The vector embedding (may be None in lazy mode)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub embedding: Option<Embedding>,
    
    /// Original text (for lazy embedding recomputation)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    
    /// Associated metadata (JSON object)
    #[serde(default)]
    pub metadata: Value,
}

impl VectorDocument {
    /// Create a new vector document with an embedding
    pub fn new(id: VectorId, embedding: Embedding) -> Self {
        Self {
            id,
            embedding: Some(embedding),
            text: None,
            metadata: Value::Null,
        }
    }

    /// Create a new vector document with text (for lazy embedding)
    pub fn from_text(id: VectorId, text: String) -> Self {
        Self {
            id,
            embedding: None,
            text: Some(text),
            metadata: Value::Null,
        }
    }

    /// Add metadata to the document
    pub fn with_metadata(mut self, metadata: Value) -> Self {
        self.metadata = metadata;
        self
    }

    /// Check if this document has an embedding
    pub fn has_embedding(&self) -> bool {
        self.embedding.is_some()
    }
}

/// Search result with score
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorSearchResult {
    /// The matched document
    pub document: VectorDocument,
    
    /// Distance/similarity score (lower is more similar for distance metrics)
    pub score: f32,
    
    /// Rank in the result set (0-indexed)
    pub rank: usize,
}

impl VectorSearchResult {
    pub fn new(document: VectorDocument, score: f32, rank: usize) -> Self {
        Self { document, score, rank }
    }
}

/// Metadata filter for vector search
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetadataFilter {
    pub filters: HashMap<String, FilterCondition>,
}

/// Filter condition for a single field
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FilterCondition {
    /// Equality check
    Eq(Value),
    /// Not equal
    Ne(Value),
    /// Greater than
    Gt(Value),
    /// Greater than or equal
    Gte(Value),
    /// Less than
    Lt(Value),
    /// Less than or equal
    Lte(Value),
    /// Value is in array
    In(Vec<Value>),
    /// Value is not in array
    NotIn(Vec<Value>),
    /// String contains substring
    Contains(String),
    /// String starts with prefix
    StartsWith(String),
    /// String ends with suffix
    EndsWith(String),
}

impl MetadataFilter {
    /// Create a new empty filter
    pub fn new() -> Self {
        Self {
            filters: HashMap::new(),
        }
    }

    /// Add an equality filter
    pub fn eq(mut self, field: &str, value: Value) -> Self {
        self.filters.insert(field.to_string(), FilterCondition::Eq(value));
        self
    }

    /// Add a greater-than filter
    pub fn gt(mut self, field: &str, value: Value) -> Self {
        self.filters.insert(field.to_string(), FilterCondition::Gt(value));
        self
    }

    /// Check if a document's metadata matches this filter
    pub fn matches(&self, metadata: &Value) -> bool {
        for (field, condition) in &self.filters {
            let value = metadata.get(field);
            if !condition.matches(value) {
                return false;
            }
        }
        true
    }
}

impl Default for MetadataFilter {
    fn default() -> Self {
        Self::new()
    }
}

impl FilterCondition {
    /// Check if a value matches this condition
    pub fn matches(&self, value: Option<&Value>) -> bool {
        match (self, value) {
            (FilterCondition::Eq(expected), Some(actual)) => expected == actual,
            (FilterCondition::Ne(expected), Some(actual)) => expected != actual,
            (FilterCondition::Gt(expected), Some(actual)) => {
                compare_values(actual, expected) == Some(std::cmp::Ordering::Greater)
            }
            (FilterCondition::Gte(expected), Some(actual)) => {
                matches!(compare_values(actual, expected), Some(std::cmp::Ordering::Greater | std::cmp::Ordering::Equal))
            }
            (FilterCondition::Lt(expected), Some(actual)) => {
                compare_values(actual, expected) == Some(std::cmp::Ordering::Less)
            }
            (FilterCondition::Lte(expected), Some(actual)) => {
                matches!(compare_values(actual, expected), Some(std::cmp::Ordering::Less | std::cmp::Ordering::Equal))
            }
            (FilterCondition::In(values), Some(actual)) => values.contains(actual),
            (FilterCondition::NotIn(values), Some(actual)) => !values.contains(actual),
            (FilterCondition::Contains(substr), Some(Value::String(s))) => s.contains(substr),
            (FilterCondition::StartsWith(prefix), Some(Value::String(s))) => s.starts_with(prefix),
            (FilterCondition::EndsWith(suffix), Some(Value::String(s))) => s.ends_with(suffix),
            _ => false,
        }
    }
}

/// Compare two JSON values
fn compare_values(a: &Value, b: &Value) -> Option<std::cmp::Ordering> {
    match (a, b) {
        (Value::Number(a), Value::Number(b)) => {
            let a = a.as_f64()?;
            let b = b.as_f64()?;
            a.partial_cmp(&b)
        }
        (Value::String(a), Value::String(b)) => Some(a.cmp(b)),
        _ => None,
    }
}

/// Statistics about a vector collection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorCollectionStats {
    /// Name of the collection
    pub name: String,
    
    /// Number of vectors in the collection
    pub vector_count: usize,
    
    /// Dimensions of vectors
    pub dimensions: usize,
    
    /// Distance metric used
    pub distance: Distance,
    
    /// Memory usage in bytes (approximate)
    pub memory_bytes: usize,
    
    /// Number of HNSW layers
    pub hnsw_layers: usize,
    
    /// Whether lazy embedding is enabled
    pub lazy_embedding: bool,
    
    /// Compression mode in use
    pub compression_mode: CompressionMode,
    
    /// Compression ratio (0.0 = no savings, 0.97 = 97% savings)
    pub compression_ratio: f64,
    
    /// Number of anchor vectors (full storage)
    pub anchor_count: usize,
    
    /// Number of delta-compressed vectors
    pub delta_count: usize,
}
