//! Vector database extension for KeraDB
//! 
//! This module provides lightweight vector storage and similarity search capabilities,
//! inspired by LEANN's graph-based selective recomputation approach for 97% storage savings.
//! 
//! # Features
//! 
//! - **HNSW Index**: Hierarchical Navigable Small World graph for fast ANN search
//! - **Lazy Embeddings**: Store text, compute embeddings on-demand (LEANN-style)
//! - **Multiple Distance Metrics**: Cosine, Euclidean, Dot Product
//! - **Metadata Filtering**: Filter vector search results by document metadata
//! - **Single-file Storage**: Vectors stored in same .ndb file as documents
//! 
//! # Example
//! 
//! ```ignore
//! use keradb::Database;
//! use keradb::vector::{VectorConfig, Distance};
//! 
//! let db = Database::create("mydata.ndb")?;
//! 
//! // Create a vector-enabled collection
//! db.create_vector_collection("documents", VectorConfig {
//!     dimensions: 384,
//!     distance: Distance::Cosine,
//!     m: 16,
//!     ef_construction: 200,
//! })?;
//! 
//! // Insert vectors
//! db.insert_vector("documents", vec![0.1, 0.2, ...], json!({"source": "readme"}))?;
//! 
//! // Search
//! let results = db.vector_search("documents", &query_vector, 10)?;
//! ```

pub mod types;
pub mod distance;
pub mod hnsw;
pub mod embedding;
pub mod search;
pub mod compression;

pub use types::*;
pub use distance::*;
pub use hnsw::HnswIndex;
pub use embedding::EmbeddingProvider;
pub use search::VectorSearcher;
pub use compression::{CompressionConfig, CompressionMode, CompressedVector, CompressionStats};
