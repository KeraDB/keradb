//! LEANN-style Delta Compression for Vector Storage
//! 
//! This module implements a delta-based compression scheme inspired by LEANN
//! (Graph-Based Selective Recomputation) that can achieve up to 97% storage savings
//! by exploiting the similarity structure of the HNSW graph.
//! 
//! # Key Concepts
//! 
//! **Full Vectors (Anchors)**: Some nodes store complete vectors
//! **Delta Vectors**: Other nodes store only the difference from a neighbor
//! 
//! # How It Works
//! 
//! 1. When inserting a vector, we check if it's similar enough to an existing neighbor
//! 2. If yes, we store only the difference (delta) and a reference to the base vector
//! 3. During search, we reconstruct vectors on-the-fly from deltas
//! 
//! # Storage Savings
//! 
//! For typical embeddings (e.g., 768-dim OpenAI):
//! - Full vector: 768 × 4 bytes = 3,072 bytes
//! - Delta (sparse): ~50-100 significant differences = ~200-400 bytes
//! - Savings: ~87-93% per compressed vector

use serde::{Deserialize, Serialize};
use super::types::{Embedding, VectorId};

/// Compression mode for vector storage
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum CompressionMode {
    /// No compression - store full vectors
    #[default]
    None,
    /// Delta compression - store differences from neighbors
    Delta,
    /// Quantized delta - store quantized differences (more aggressive)
    QuantizedDelta,
}

/// Configuration for delta compression
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompressionConfig {
    /// Compression mode to use
    pub mode: CompressionMode,
    
    /// Threshold for considering a delta component as "significant"
    /// Components with absolute delta < threshold are treated as zero
    /// Default: 0.001
    pub sparsity_threshold: f32,
    
    /// Maximum percentage of components to store (for sparse deltas)
    /// If more components exceed threshold, fall back to full storage
    /// Default: 0.15 (15%)
    pub max_density: f32,
    
    /// How often to force a full vector (anchor) in the graph
    /// 1 = every vector is an anchor, 10 = every 10th vector is an anchor
    /// Default: 8
    pub anchor_frequency: usize,
    
    /// Number of quantization bits (for QuantizedDelta mode)
    /// Default: 8 (256 levels)
    pub quantization_bits: u8,
}

impl Default for CompressionConfig {
    fn default() -> Self {
        Self {
            mode: CompressionMode::Delta,
            sparsity_threshold: 0.001,
            max_density: 0.15,
            anchor_frequency: 8,
            quantization_bits: 8,
        }
    }
}

impl CompressionConfig {
    /// Create config with no compression
    pub fn none() -> Self {
        Self {
            mode: CompressionMode::None,
            ..Default::default()
        }
    }
    
    /// Create config with delta compression
    pub fn delta() -> Self {
        Self::default()
    }
    
    /// Create config with aggressive quantized delta compression
    pub fn quantized() -> Self {
        Self {
            mode: CompressionMode::QuantizedDelta,
            sparsity_threshold: 0.01,
            max_density: 0.10,
            anchor_frequency: 16,
            quantization_bits: 8,
        }
    }
    
    /// Set sparsity threshold
    pub fn with_threshold(mut self, threshold: f32) -> Self {
        self.sparsity_threshold = threshold;
        self
    }
    
    /// Set anchor frequency
    pub fn with_anchor_frequency(mut self, freq: usize) -> Self {
        self.anchor_frequency = freq.max(1);
        self
    }
}

/// A compressed vector representation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CompressedVector {
    /// Full vector (anchor)
    Full(Embedding),
    
    /// Sparse delta from a base vector
    Delta {
        /// ID of the base (anchor) vector
        base_id: VectorId,
        /// Sparse representation: (index, delta_value) pairs
        deltas: Vec<(u16, f32)>,
        /// Original vector norm (for fast distance approximation)
        norm: f32,
    },
    
    /// Quantized delta (more aggressive compression)
    QuantizedDelta {
        /// ID of the base vector
        base_id: VectorId,
        /// Quantized sparse deltas: (index, quantized_value)
        deltas: Vec<(u16, i8)>,
        /// Scale factor for dequantization
        scale: f32,
        /// Original vector norm
        norm: f32,
    },
}

impl CompressedVector {
    /// Check if this is a full (anchor) vector
    pub fn is_anchor(&self) -> bool {
        matches!(self, CompressedVector::Full(_))
    }
    
    /// Get the base ID if this is a delta vector
    pub fn base_id(&self) -> Option<VectorId> {
        match self {
            CompressedVector::Full(_) => None,
            CompressedVector::Delta { base_id, .. } => Some(*base_id),
            CompressedVector::QuantizedDelta { base_id, .. } => Some(*base_id),
        }
    }
    
    /// Get the stored norm
    pub fn norm(&self) -> f32 {
        match self {
            CompressedVector::Full(v) => {
                v.iter().map(|x| x * x).sum::<f32>().sqrt()
            }
            CompressedVector::Delta { norm, .. } => *norm,
            CompressedVector::QuantizedDelta { norm, .. } => *norm,
        }
    }
    
    /// Estimate storage size in bytes
    pub fn storage_bytes(&self) -> usize {
        match self {
            CompressedVector::Full(v) => v.len() * 4 + 8, // 4 bytes per f32 + overhead
            CompressedVector::Delta { deltas, .. } => {
                deltas.len() * 6 + 16 // 2 bytes index + 4 bytes value + overhead
            }
            CompressedVector::QuantizedDelta { deltas, .. } => {
                deltas.len() * 3 + 20 // 2 bytes index + 1 byte value + overhead
            }
        }
    }
}

/// Delta encoder/decoder for LEANN-style compression
pub struct DeltaCompressor {
    config: CompressionConfig,
}

impl DeltaCompressor {
    /// Create a new delta compressor
    pub fn new(config: CompressionConfig) -> Self {
        Self { config }
    }
    
    /// Compress a vector relative to a base vector
    /// Returns None if the delta is too dense (should store full vector instead)
    pub fn compress(&self, vector: &Embedding, base: &Embedding) -> Option<CompressedVector> {
        if self.config.mode == CompressionMode::None {
            return Some(CompressedVector::Full(vector.clone()));
        }
        
        if vector.len() != base.len() {
            return Some(CompressedVector::Full(vector.clone()));
        }
        
        let dim = vector.len();
        let norm = vector.iter().map(|x| x * x).sum::<f32>().sqrt();
        
        // Compute sparse delta
        let mut deltas = Vec::new();
        for (i, (v, b)) in vector.iter().zip(base.iter()).enumerate() {
            let delta = v - b;
            if delta.abs() > self.config.sparsity_threshold {
                deltas.push((i as u16, delta));
            }
        }
        
        // Check if delta is sparse enough
        let density = deltas.len() as f32 / dim as f32;
        if density > self.config.max_density {
            // Too dense, store full vector
            return None;
        }
        
        match self.config.mode {
            CompressionMode::None => Some(CompressedVector::Full(vector.clone())),
            CompressionMode::Delta => Some(CompressedVector::Delta {
                base_id: 0, // Will be set by caller
                deltas,
                norm,
            }),
            CompressionMode::QuantizedDelta => {
                // Find scale for quantization
                let max_abs = deltas.iter()
                    .map(|(_, d)| d.abs())
                    .fold(0.0f32, f32::max);
                
                if max_abs < 1e-10 {
                    // All deltas are essentially zero
                    return Some(CompressedVector::QuantizedDelta {
                        base_id: 0,
                        deltas: Vec::new(),
                        scale: 1.0,
                        norm,
                    });
                }
                
                let scale = max_abs / 127.0; // Quantize to i8 range [-127, 127]
                let quantized: Vec<(u16, i8)> = deltas
                    .iter()
                    .map(|(i, d)| (*i, (d / scale).round() as i8))
                    .filter(|(_, q)| *q != 0)
                    .collect();
                
                Some(CompressedVector::QuantizedDelta {
                    base_id: 0,
                    deltas: quantized,
                    scale,
                    norm,
                })
            }
        }
    }
    
    /// Decompress a delta vector given the base vector
    pub fn decompress(&self, compressed: &CompressedVector, get_base: impl Fn(VectorId) -> Option<Embedding>) -> Option<Embedding> {
        match compressed {
            CompressedVector::Full(v) => Some(v.clone()),
            
            CompressedVector::Delta { base_id, deltas, .. } => {
                let mut base = get_base(*base_id)?;
                for (i, delta) in deltas {
                    if (*i as usize) < base.len() {
                        base[*i as usize] += delta;
                    }
                }
                Some(base)
            }
            
            CompressedVector::QuantizedDelta { base_id, deltas, scale, .. } => {
                let mut base = get_base(*base_id)?;
                for (i, quantized) in deltas {
                    if (*i as usize) < base.len() {
                        base[*i as usize] += (*quantized as f32) * scale;
                    }
                }
                Some(base)
            }
        }
    }
    
    /// Compute approximate distance without full decompression
    /// Uses the cached norm and sparse delta for faster computation
    pub fn approximate_distance(
        &self,
        compressed: &CompressedVector,
        query: &Embedding,
        query_norm: f32,
    ) -> f32 {
        match compressed {
            CompressedVector::Full(v) => {
                // Exact distance
                super::distance::cosine_distance(v, query)
            }
            CompressedVector::Delta { norm, .. } | CompressedVector::QuantizedDelta { norm, .. } => {
                // Approximation using norms (fast but less accurate)
                // For cosine: approximate as 1 - (norm_ratio)
                // This is a rough upper bound, actual search will refine
                let norm_ratio = norm / query_norm;
                (1.0 - norm_ratio).abs().min(2.0)
            }
        }
    }
}

/// Compressed vector storage with anchor management
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompressedVectorStore {
    /// Configuration
    config: CompressionConfig,
    
    /// Compressed vectors by ID
    vectors: std::collections::HashMap<VectorId, CompressedVector>,
    
    /// Set of anchor (full) vector IDs
    anchors: std::collections::HashSet<VectorId>,
    
    /// Total vectors inserted
    total_count: usize,
    
    /// Dimensions
    dimensions: usize,
}

impl CompressedVectorStore {
    /// Create a new compressed vector store
    pub fn new(dimensions: usize, config: CompressionConfig) -> Self {
        Self {
            config,
            vectors: std::collections::HashMap::new(),
            anchors: std::collections::HashSet::new(),
            total_count: 0,
            dimensions,
        }
    }
    
    /// Insert a vector, potentially compressing it
    pub fn insert(&mut self, id: VectorId, vector: Embedding, neighbor_id: Option<VectorId>) -> bool {
        if vector.len() != self.dimensions {
            return false;
        }
        
        self.total_count += 1;
        
        // Decide if this should be an anchor
        let should_anchor = self.config.mode == CompressionMode::None
            || self.anchors.is_empty()
            || self.total_count % self.config.anchor_frequency == 0;
        
        if should_anchor {
            self.vectors.insert(id, CompressedVector::Full(vector));
            self.anchors.insert(id);
            return true;
        }
        
        // Try to compress relative to a neighbor
        if let Some(neighbor_id) = neighbor_id {
            if let Some(base_vector) = self.get_full(neighbor_id) {
                let compressor = DeltaCompressor::new(self.config.clone());
                if let Some(mut compressed) = compressor.compress(&vector, &base_vector) {
                    // Set the base ID
                    match &mut compressed {
                        CompressedVector::Delta { base_id, .. } => *base_id = neighbor_id,
                        CompressedVector::QuantizedDelta { base_id, .. } => *base_id = neighbor_id,
                        _ => {}
                    }
                    self.vectors.insert(id, compressed);
                    return true;
                }
            }
        }
        
        // Fallback: store as anchor
        self.vectors.insert(id, CompressedVector::Full(vector));
        self.anchors.insert(id);
        true
    }
    
    /// Get a full (decompressed) vector by ID
    pub fn get_full(&self, id: VectorId) -> Option<Embedding> {
        let compressed = self.vectors.get(&id)?;
        
        match compressed {
            CompressedVector::Full(v) => Some(v.clone()),
            CompressedVector::Delta { base_id, deltas, .. } => {
                // Recursive decompression (anchors are guaranteed to exist)
                let mut base = self.get_full(*base_id)?;
                for (i, delta) in deltas {
                    if (*i as usize) < base.len() {
                        base[*i as usize] += delta;
                    }
                }
                Some(base)
            }
            CompressedVector::QuantizedDelta { base_id, deltas, scale, .. } => {
                let mut base = self.get_full(*base_id)?;
                for (i, quantized) in deltas {
                    if (*i as usize) < base.len() {
                        base[*i as usize] += (*quantized as f32) * scale;
                    }
                }
                Some(base)
            }
        }
    }
    
    /// Get compressed vector (for storage/serialization)
    pub fn get_compressed(&self, id: VectorId) -> Option<&CompressedVector> {
        self.vectors.get(&id)
    }
    
    /// Remove a vector
    pub fn remove(&mut self, id: VectorId) -> bool {
        // Note: Removing an anchor that other vectors depend on is complex
        // For now, we don't allow removing anchors that have dependents
        if self.anchors.contains(&id) {
            // Check if any other vector depends on this anchor
            let has_dependents = self.vectors.values().any(|v| v.base_id() == Some(id));
            if has_dependents {
                return false; // Cannot remove anchor with dependents
            }
            self.anchors.remove(&id);
        }
        self.vectors.remove(&id).is_some()
    }
    
    /// Get storage statistics
    pub fn stats(&self) -> CompressionStats {
        let total_vectors = self.vectors.len();
        let anchor_count = self.anchors.len();
        let delta_count = total_vectors - anchor_count;
        
        let compressed_bytes: usize = self.vectors.values().map(|v| v.storage_bytes()).sum();
        let uncompressed_bytes = total_vectors * self.dimensions * 4;
        
        let compression_ratio = if uncompressed_bytes > 0 {
            1.0 - (compressed_bytes as f64 / uncompressed_bytes as f64)
        } else {
            0.0
        };
        
        CompressionStats {
            total_vectors,
            anchor_count,
            delta_count,
            compressed_bytes,
            uncompressed_bytes,
            compression_ratio,
            avg_delta_size: if delta_count > 0 {
                (compressed_bytes - anchor_count * (self.dimensions * 4 + 8)) / delta_count
            } else {
                0
            },
        }
    }
    
    /// Check if a vector is an anchor
    pub fn is_anchor(&self, id: VectorId) -> bool {
        self.anchors.contains(&id)
    }
    
    /// Get number of vectors
    pub fn len(&self) -> usize {
        self.vectors.len()
    }
    
    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.vectors.is_empty()
    }
}

/// Statistics about compression effectiveness
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompressionStats {
    /// Total number of vectors
    pub total_vectors: usize,
    
    /// Number of anchor (full) vectors
    pub anchor_count: usize,
    
    /// Number of delta-compressed vectors
    pub delta_count: usize,
    
    /// Total storage used (bytes)
    pub compressed_bytes: usize,
    
    /// Storage without compression (bytes)
    pub uncompressed_bytes: usize,
    
    /// Compression ratio (0.0 = no savings, 0.97 = 97% savings)
    pub compression_ratio: f64,
    
    /// Average size of delta vectors (bytes)
    pub avg_delta_size: usize,
}

impl std::fmt::Display for CompressionStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Vectors: {} ({} anchors, {} deltas)\n\
             Storage: {} bytes (uncompressed: {} bytes)\n\
             Compression: {:.1}% savings\n\
             Avg delta size: {} bytes",
            self.total_vectors,
            self.anchor_count,
            self.delta_count,
            self.compressed_bytes,
            self.uncompressed_bytes,
            self.compression_ratio * 100.0,
            self.avg_delta_size
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    fn random_vector(dim: usize) -> Embedding {
        (0..dim).map(|i| (i as f32 * 0.1).sin()).collect()
    }
    
    fn similar_vector(base: &Embedding, noise: f32) -> Embedding {
        base.iter().enumerate().map(|(i, x)| {
            // Only add noise to ~10% of components (sparse difference)
            if i % 10 == 0 {
                x + noise
            } else {
                *x
            }
        }).collect()
    }
    
    #[test]
    fn test_delta_compression() {
        // Use relaxed config for small test vectors
        let config = CompressionConfig {
            mode: CompressionMode::Delta,
            sparsity_threshold: 0.001,
            max_density: 0.5, // Allow 50% for small vectors
            anchor_frequency: 8,
            quantization_bits: 8,
        };
        let compressor = DeltaCompressor::new(config);
        
        // Base vector and a similar one with a few significant differences
        let base: Embedding = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let similar: Embedding = vec![1.1, 2.0, 3.0, 4.0, 5.2]; // Two significant differences
        
        let compressed = compressor.compress(&similar, &base);
        assert!(compressed.is_some(), "Compression should succeed");
        
        if let Some(CompressedVector::Delta { deltas, .. }) = compressed {
            // Should only have the two significantly different components
            assert!(deltas.len() <= 2, "Expected <= 2 deltas, got {}", deltas.len());
        }
    }
    
    #[test]
    fn test_compression_roundtrip() {
        // Use relaxed config for small test vectors
        let config = CompressionConfig {
            mode: CompressionMode::Delta,
            sparsity_threshold: 0.001,
            max_density: 0.6, // Allow 60% for 5-element vector
            anchor_frequency: 8,
            quantization_bits: 8,
        };
        let compressor = DeltaCompressor::new(config);
        
        let base: Embedding = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let original: Embedding = vec![1.1, 2.0, 3.05, 4.0, 5.2]; // Sparse differences (3 out of 5)
        
        let compressed = compressor.compress(&original, &base);
        assert!(compressed.is_some(), "Compression should succeed");
        
        if let Some(mut compressed) = compressed {
            // Set base_id
            if let CompressedVector::Delta { base_id, .. } = &mut compressed {
                *base_id = 0;
            }
            
            let decompressed = compressor.decompress(&compressed, |_| Some(base.clone()));
            assert!(decompressed.is_some());
            
            let decompressed = decompressed.unwrap();
            for (o, d) in original.iter().zip(decompressed.iter()) {
                assert!((o - d).abs() < 0.01, "Decompression error too large");
            }
        }
    }
    
    #[test]
    fn test_compressed_store() {
        // Use a more aggressive config to ensure compression happens
        let config = CompressionConfig {
            mode: CompressionMode::Delta,
            sparsity_threshold: 0.001,
            max_density: 0.5, // Allow up to 50% non-zero deltas
            anchor_frequency: 4, // Every 4th vector is an anchor
            quantization_bits: 8,
        };
        let mut store = CompressedVectorStore::new(128, config);
        
        // Insert first vector (will be anchor)
        let v1 = random_vector(128);
        store.insert(0, v1.clone(), None);
        assert!(store.is_anchor(0));
        
        // Insert similar vectors with sparse differences (should be compressed)
        for i in 1..10 {
            let v = similar_vector(&v1, 0.1); // ~10% of components differ by 0.1
            store.insert(i, v, Some(0));
        }
        
        let stats = store.stats();
        println!("{}", stats);
        
        // With anchor_frequency=4, we expect some anchors and some deltas
        // Vectors 0, 4, 8 should be anchors (3 anchors)
        // Vectors 1, 2, 3, 5, 6, 7, 9 should be deltas (7 deltas) if compression works
        assert!(stats.delta_count > 0, "Expected some delta-compressed vectors");
    }
    
    #[test]
    fn test_quantized_compression() {
        // Use relaxed config for test
        let config = CompressionConfig {
            mode: CompressionMode::QuantizedDelta,
            sparsity_threshold: 0.001,
            max_density: 0.2, // ~10% is fine, allow up to 20%
            anchor_frequency: 16,
            quantization_bits: 8,
        };
        let compressor = DeltaCompressor::new(config);
        
        // Create vectors where only ~10% of components differ
        let base: Embedding = vec![1.0; 128];
        let mut similar: Embedding = vec![1.0; 128];
        // Set ~10% of components to be different
        for i in (0..128).step_by(10) {
            similar[i] = 1.5;
        }
        
        let compressed = compressor.compress(&similar, &base);
        assert!(compressed.is_some(), "Quantized compression should succeed");
        
        if let Some(CompressedVector::QuantizedDelta { deltas, .. }) = &compressed {
            println!("Quantized delta has {} non-zero components", deltas.len());
            // Should have ~13 non-zero components (128/10)
            assert!(deltas.len() <= 20, "Expected sparse quantized delta");
        }
    }
    
    #[test]
    fn test_high_compression_scenario() {
        // Simulate real embedding scenario: most components are very similar
        let config = CompressionConfig::delta();
        let compressor = DeltaCompressor::new(config);
        
        // 768-dim vector (like OpenAI embeddings)
        let base: Embedding = (0..768).map(|i| (i as f32 * 0.01).sin()).collect();
        
        // Only 5% of components differ significantly
        let mut similar = base.clone();
        for i in (0..768).step_by(20) {
            similar[i] += 0.1;
        }
        
        let compressed = compressor.compress(&similar, &base);
        assert!(compressed.is_some());
        
        if let Some(CompressedVector::Delta { deltas, .. }) = &compressed {
            let density = deltas.len() as f32 / 768.0;
            println!("Density: {:.1}% ({} deltas)", density * 100.0, deltas.len());
            assert!(density < 0.10, "Expected < 10% density");
            
            // Calculate storage savings
            let full_size = 768 * 4; // 3072 bytes
            let delta_size = deltas.len() * 6 + 16; // 6 bytes per delta + overhead
            let savings = 1.0 - (delta_size as f32 / full_size as f32);
            println!("Storage savings: {:.1}%", savings * 100.0);
            assert!(savings > 0.5, "Expected > 50% storage savings");
        }
    }
}

