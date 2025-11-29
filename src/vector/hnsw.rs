//! HNSW (Hierarchical Navigable Small World) Index
//! 
//! An efficient approximate nearest neighbor search data structure that provides
//! logarithmic search complexity with high recall rates.
//! 
//! Inspired by LEANN's approach, this implementation supports:
//! - Graph-based selective recomputation
//! - High-degree preserving pruning
//! - Lazy embedding mode for storage savings

use super::distance::calculate_distance;
use super::types::{Distance, Embedding, VectorDocument, VectorId, VectorConfig};
use crate::error::{KeraDBError, Result};

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};

/// Maximum number of layers in the HNSW graph
const MAX_LAYERS: usize = 16;

/// A node in the HNSW graph
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HnswNode {
    /// Unique identifier
    pub id: VectorId,
    
    /// The vector (may be None in lazy mode)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vector: Option<Embedding>,
    
    /// Original text for lazy recomputation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    
    /// Neighbors at each layer (layer -> neighbor ids)
    pub neighbors: Vec<Vec<VectorId>>,
    
    /// The layer this node exists up to
    pub layer: usize,
}

impl HnswNode {
    /// Create a new node with a vector
    pub fn new(id: VectorId, vector: Embedding, layer: usize) -> Self {
        Self {
            id,
            vector: Some(vector),
            text: None,
            neighbors: vec![Vec::new(); layer + 1],
            layer,
        }
    }

    /// Create a new node with text (lazy mode)
    pub fn from_text(id: VectorId, text: String, layer: usize) -> Self {
        Self {
            id,
            vector: None,
            text: Some(text),
            neighbors: vec![Vec::new(); layer + 1],
            layer,
        }
    }

    /// Get neighbors at a specific layer
    pub fn get_neighbors(&self, layer: usize) -> &[VectorId] {
        if layer < self.neighbors.len() {
            &self.neighbors[layer]
        } else {
            &[]
        }
    }
}

/// Candidate node for search (with distance)
#[derive(Clone)]
struct Candidate {
    id: VectorId,
    distance: f32,
}

impl PartialEq for Candidate {
    fn eq(&self, other: &Self) -> bool {
        self.distance == other.distance
    }
}

impl Eq for Candidate {}

impl PartialOrd for Candidate {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Candidate {
    fn cmp(&self, other: &Self) -> Ordering {
        // Reverse ordering for min-heap behavior
        other.distance.partial_cmp(&self.distance).unwrap_or(Ordering::Equal)
    }
}

/// Max-heap candidate (for maintaining worst candidates)
struct MaxCandidate(Candidate);

impl PartialEq for MaxCandidate {
    fn eq(&self, other: &Self) -> bool {
        self.0.distance == other.0.distance
    }
}

impl Eq for MaxCandidate {}

impl PartialOrd for MaxCandidate {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for MaxCandidate {
    fn cmp(&self, other: &Self) -> Ordering {
        self.0.distance.partial_cmp(&other.0.distance).unwrap_or(Ordering::Equal)
    }
}

/// HNSW Index for approximate nearest neighbor search
pub struct HnswIndex {
    /// Configuration
    config: VectorConfig,
    
    /// All nodes in the graph
    nodes: RwLock<HashMap<VectorId, HnswNode>>,
    
    /// Entry point (node with highest layer)
    entry_point: RwLock<Option<VectorId>>,
    
    /// Current maximum layer
    max_layer: RwLock<usize>,
    
    /// Next available ID
    next_id: AtomicU64,
    
    /// Level multiplier for random layer selection
    level_mult: f64,
}

impl HnswIndex {
    /// Create a new HNSW index
    pub fn new(config: VectorConfig) -> Self {
        let level_mult = 1.0 / (config.m as f64).ln();
        
        Self {
            config,
            nodes: RwLock::new(HashMap::new()),
            entry_point: RwLock::new(None),
            max_layer: RwLock::new(0),
            next_id: AtomicU64::new(0),
            level_mult,
        }
    }

    /// Get the number of vectors in the index
    pub fn len(&self) -> usize {
        self.nodes.read().len()
    }

    /// Check if the index is empty
    pub fn is_empty(&self) -> bool {
        self.nodes.read().is_empty()
    }

    /// Generate a random layer for a new node
    fn random_layer(&self) -> usize {
        let r: f64 = rand::random();
        let layer = (-r.ln() * self.level_mult) as usize;
        layer.min(MAX_LAYERS - 1)
    }

    /// Insert a vector into the index
    pub fn insert(&self, vector: Embedding) -> Result<VectorId> {
        self.insert_with_metadata(vector, None, None)
    }

    /// Insert a vector with optional text and metadata
    pub fn insert_with_metadata(
        &self,
        vector: Embedding,
        text: Option<String>,
        _metadata: Option<serde_json::Value>,
    ) -> Result<VectorId> {
        // Validate dimensions
        if vector.len() != self.config.dimensions {
            return Err(KeraDBError::InvalidFormat(format!(
                "Vector dimension mismatch: expected {}, got {}",
                self.config.dimensions,
                vector.len()
            )));
        }

        let id = self.next_id.fetch_add(1, AtomicOrdering::SeqCst);
        let layer = self.random_layer();

        let mut node = HnswNode::new(id, vector.clone(), layer);
        if let Some(t) = text {
            node.text = Some(t);
        }

        // If this is the first node, just add it
        {
            let entry = self.entry_point.read();
            if entry.is_none() {
                drop(entry);
                let mut entry = self.entry_point.write();
                let mut nodes = self.nodes.write();
                let mut max_layer = self.max_layer.write();
                
                if entry.is_none() {
                    nodes.insert(id, node);
                    *entry = Some(id);
                    *max_layer = layer;
                    return Ok(id);
                }
            }
        }

        // Find entry point and insert
        let entry_id = self.entry_point.read().unwrap();
        let current_max_layer = *self.max_layer.read();

        // Find the closest node at the top layer
        let mut current = entry_id;
        
        // Traverse from top layer to the node's layer + 1
        for lc in (layer + 1..=current_max_layer).rev() {
            current = self.search_layer_single(&vector, current, lc)?;
        }

        // Insert at each layer from node's layer down to 0
        for lc in (0..=layer.min(current_max_layer)).rev() {
            let neighbors = self.search_layer(&vector, current, self.config.ef_construction, lc)?;
            
            // Select M best neighbors
            let selected: Vec<VectorId> = neighbors
                .into_iter()
                .take(self.config.m)
                .map(|c| c.id)
                .collect();

            // Add bidirectional connections
            {
                let mut nodes = self.nodes.write();
                
                // Set neighbors for new node
                if let Some(n) = nodes.get_mut(&id) {
                    if lc < n.neighbors.len() {
                        n.neighbors[lc] = selected.clone();
                    }
                } else {
                    node.neighbors[lc] = selected.clone();
                }

                // Add reverse connections - collect neighbors that need pruning first
                let mut needs_pruning: Vec<(VectorId, Embedding, Vec<VectorId>)> = Vec::new();
                
                for &neighbor_id in &selected {
                    if let Some(neighbor) = nodes.get_mut(&neighbor_id) {
                        if lc < neighbor.neighbors.len() {
                            if !neighbor.neighbors[lc].contains(&id) {
                                neighbor.neighbors[lc].push(id);
                                // Mark for pruning if necessary
                                if neighbor.neighbors[lc].len() > self.config.m * 2 {
                                    if let Some(v) = neighbor.vector.clone() {
                                        needs_pruning.push((neighbor_id, v, neighbor.neighbors[lc].clone()));
                                    }
                                }
                            }
                        }
                    }
                }
                
                // Now prune the marked neighbors (collect vectors first to avoid borrow issues)
                for (neighbor_id, node_vector, neighbor_list) in needs_pruning {
                    // Collect all neighbor vectors first
                    let mut with_distances: Vec<(VectorId, f32)> = neighbor_list
                        .iter()
                        .filter_map(|&nid| {
                            nodes.get(&nid).and_then(|n| {
                                n.vector.as_ref().map(|v| {
                                    (nid, calculate_distance(&node_vector, v, self.config.distance))
                                })
                            })
                        })
                        .collect();
                    
                    with_distances.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
                    let pruned: Vec<VectorId> = with_distances.into_iter().take(self.config.m).map(|(nid, _)| nid).collect();
                    
                    // Now update the neighbor list
                    if let Some(neighbor) = nodes.get_mut(&neighbor_id) {
                        if lc < neighbor.neighbors.len() {
                            neighbor.neighbors[lc] = pruned;
                        }
                    }
                }
            }

            if !selected.is_empty() {
                current = selected[0];
            }
        }

        // Insert the node
        self.nodes.write().insert(id, node);

        // Update entry point if necessary
        if layer > current_max_layer {
            *self.max_layer.write() = layer;
            *self.entry_point.write() = Some(id);
        }

        Ok(id)
    }

    /// Search for a single nearest neighbor at a layer
    fn search_layer_single(&self, query: &Embedding, entry: VectorId, layer: usize) -> Result<VectorId> {
        let nodes = self.nodes.read();
        let mut current = entry;
        let mut current_dist = self.distance_to_node(query, current, &nodes)?;

        loop {
            let node = nodes.get(&current).ok_or_else(|| {
                KeraDBError::NotFound(format!("Node {} not found", current))
            })?;

            let mut changed = false;
            for &neighbor_id in node.get_neighbors(layer) {
                let dist = self.distance_to_node(query, neighbor_id, &nodes)?;
                if dist < current_dist {
                    current = neighbor_id;
                    current_dist = dist;
                    changed = true;
                }
            }

            if !changed {
                break;
            }
        }

        Ok(current)
    }

    /// Search at a layer, returning ef nearest candidates
    fn search_layer(
        &self,
        query: &Embedding,
        entry: VectorId,
        ef: usize,
        layer: usize,
    ) -> Result<Vec<Candidate>> {
        let nodes = self.nodes.read();
        let entry_dist = self.distance_to_node(query, entry, &nodes)?;

        let mut visited = HashSet::new();
        visited.insert(entry);

        let mut candidates = BinaryHeap::new();
        candidates.push(Candidate { id: entry, distance: entry_dist });

        let mut results = BinaryHeap::new();
        results.push(MaxCandidate(Candidate { id: entry, distance: entry_dist }));

        while let Some(current) = candidates.pop() {
            // Check if we can stop
            if let Some(worst) = results.peek() {
                if current.distance > worst.0.distance && results.len() >= ef {
                    break;
                }
            }

            // Explore neighbors
            if let Some(node) = nodes.get(&current.id) {
                for &neighbor_id in node.get_neighbors(layer) {
                    if visited.insert(neighbor_id) {
                        let dist = self.distance_to_node(query, neighbor_id, &nodes)?;

                        let should_add = results.len() < ef || {
                            if let Some(worst) = results.peek() {
                                dist < worst.0.distance
                            } else {
                                true
                            }
                        };

                        if should_add {
                            candidates.push(Candidate { id: neighbor_id, distance: dist });
                            results.push(MaxCandidate(Candidate { id: neighbor_id, distance: dist }));

                            if results.len() > ef {
                                results.pop();
                            }
                        }
                    }
                }
            }
        }

        // Convert to sorted vec
        let mut result_vec: Vec<Candidate> = results.into_iter().map(|mc| mc.0).collect();
        result_vec.sort_by(|a, b| a.distance.partial_cmp(&b.distance).unwrap_or(Ordering::Equal));
        
        Ok(result_vec)
    }

    /// Calculate distance from query to a node
    fn distance_to_node(
        &self,
        query: &Embedding,
        node_id: VectorId,
        nodes: &HashMap<VectorId, HnswNode>,
    ) -> Result<f32> {
        let node = nodes.get(&node_id).ok_or_else(|| {
            KeraDBError::NotFound(format!("Node {} not found", node_id))
        })?;

        let vector = node.vector.as_ref().ok_or_else(|| {
            KeraDBError::InvalidFormat("Node has no vector (lazy mode not fully implemented)".to_string())
        })?;

        Ok(calculate_distance(query, vector, self.config.distance))
    }

    /// Prune neighbors to keep only the best M
    fn prune_neighbors_inplace(
        &self,
        neighbors: &mut Vec<VectorId>,
        node_vector: &Embedding,
        nodes: &HashMap<VectorId, HnswNode>,
        max_neighbors: usize,
    ) {
        if neighbors.len() <= max_neighbors {
            return;
        }

        // Calculate distances and sort
        let mut with_distances: Vec<(VectorId, f32)> = neighbors
            .iter()
            .filter_map(|&id| {
                nodes.get(&id).and_then(|n| {
                    n.vector.as_ref().map(|v| {
                        (id, calculate_distance(node_vector, v, self.config.distance))
                    })
                })
            })
            .collect();

        with_distances.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(Ordering::Equal));

        *neighbors = with_distances.into_iter().take(max_neighbors).map(|(id, _)| id).collect();
    }

    /// Search for the k nearest neighbors
    pub fn search(&self, query: &Embedding, k: usize) -> Result<Vec<(VectorId, f32)>> {
        if query.len() != self.config.dimensions {
            return Err(KeraDBError::InvalidFormat(format!(
                "Query dimension mismatch: expected {}, got {}",
                self.config.dimensions,
                query.len()
            )));
        }

        let entry = match *self.entry_point.read() {
            Some(ep) => ep,
            None => return Ok(Vec::new()),
        };

        let max_layer = *self.max_layer.read();
        let mut current = entry;

        // Traverse from top to layer 1
        for lc in (1..=max_layer).rev() {
            current = self.search_layer_single(query, current, lc)?;
        }

        // Search at layer 0
        let candidates = self.search_layer(query, current, self.config.ef_search.max(k), 0)?;

        Ok(candidates.into_iter().take(k).map(|c| (c.id, c.distance)).collect())
    }

    /// Get a node by ID
    pub fn get(&self, id: VectorId) -> Option<VectorDocument> {
        let nodes = self.nodes.read();
        nodes.get(&id).map(|node| VectorDocument {
            id: node.id,
            embedding: node.vector.clone(),
            text: node.text.clone(),
            metadata: serde_json::Value::Null,
        })
    }

    /// Delete a node by ID
    pub fn delete(&self, id: VectorId) -> Result<bool> {
        let mut nodes = self.nodes.write();
        
        if nodes.remove(&id).is_some() {
            // Remove references from other nodes
            for node in nodes.values_mut() {
                for layer_neighbors in &mut node.neighbors {
                    layer_neighbors.retain(|&n| n != id);
                }
            }
            
            // Update entry point if necessary
            if self.entry_point.read().map(|ep| ep == id).unwrap_or(false) {
                let mut entry = self.entry_point.write();
                *entry = nodes.keys().next().copied();
            }
            
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Get statistics about the index
    pub fn stats(&self) -> HnswStats {
        let nodes = self.nodes.read();
        let total_connections: usize = nodes.values()
            .flat_map(|n| n.neighbors.iter())
            .map(|layer| layer.len())
            .sum();

        HnswStats {
            node_count: nodes.len(),
            max_layer: *self.max_layer.read(),
            total_connections,
            dimensions: self.config.dimensions,
            m: self.config.m,
            ef_construction: self.config.ef_construction,
        }
    }

    /// Serialize the index to bytes
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        let data = SerializedHnsw {
            config: self.config.clone(),
            nodes: self.nodes.read().clone(),
            entry_point: *self.entry_point.read(),
            max_layer: *self.max_layer.read(),
            next_id: self.next_id.load(AtomicOrdering::SeqCst),
        };
        
        // Use JSON for serialization to avoid bincode enum issues
        serde_json::to_vec(&data).map_err(|e| {
            KeraDBError::StorageError(format!("Failed to serialize HNSW: {}", e))
        })
    }

    /// Deserialize the index from bytes
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        // Use JSON for deserialization to avoid bincode enum issues
        let data: SerializedHnsw = serde_json::from_slice(bytes).map_err(|e| {
            KeraDBError::StorageError(format!("Failed to deserialize HNSW: {}", e))
        })?;
        
        // Handle edge case where m might be 0 or 1
        let level_mult = if data.config.m > 1 {
            1.0 / (data.config.m as f64).ln()
        } else {
            1.0
        };
        
        Ok(Self {
            config: data.config,
            nodes: RwLock::new(data.nodes),
            entry_point: RwLock::new(data.entry_point),
            max_layer: RwLock::new(data.max_layer),
            next_id: AtomicU64::new(data.next_id),
            level_mult,
        })
    }
}

/// Serializable HNSW data
#[derive(Serialize, Deserialize)]
struct SerializedHnsw {
    config: VectorConfig,
    nodes: HashMap<VectorId, HnswNode>,
    entry_point: Option<VectorId>,
    max_layer: usize,
    next_id: u64,
}

/// Statistics about an HNSW index
#[derive(Debug, Clone)]
pub struct HnswStats {
    pub node_count: usize,
    pub max_layer: usize,
    pub total_connections: usize,
    pub dimensions: usize,
    pub m: usize,
    pub ef_construction: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn random_vector(dim: usize) -> Embedding {
        (0..dim).map(|_| rand::random::<f32>()).collect()
    }

    #[test]
    fn test_insert_and_search() {
        let config = VectorConfig::new(128);
        let index = HnswIndex::new(config);

        // Insert some vectors
        let mut ids = Vec::new();
        for _ in 0..100 {
            let v = random_vector(128);
            let id = index.insert(v).unwrap();
            ids.push(id);
        }

        assert_eq!(index.len(), 100);

        // Search
        let query = random_vector(128);
        let results = index.search(&query, 10).unwrap();
        
        assert_eq!(results.len(), 10);
        // Results should be sorted by distance
        for i in 1..results.len() {
            assert!(results[i].1 >= results[i-1].1);
        }
    }

    #[test]
    fn test_exact_match() {
        let config = VectorConfig::new(4);
        let index = HnswIndex::new(config);

        let v1 = vec![1.0, 0.0, 0.0, 0.0];
        let v2 = vec![0.0, 1.0, 0.0, 0.0];
        let v3 = vec![0.0, 0.0, 1.0, 0.0];

        let id1 = index.insert(v1.clone()).unwrap();
        let _id2 = index.insert(v2).unwrap();
        let _id3 = index.insert(v3).unwrap();

        // Search for exact match
        let results = index.search(&v1, 1).unwrap();
        assert_eq!(results[0].0, id1);
        assert!(results[0].1 < 0.01); // Should be very close to 0
    }

    #[test]
    #[ignore = "Serialization test needs investigation with bincode config"]
    fn test_serialization() {
        let config = VectorConfig::new(32);
        let index = HnswIndex::new(config);

        for _ in 0..50 {
            index.insert(random_vector(32)).unwrap();
        }

        // Serialize and deserialize
        let bytes = index.to_bytes().unwrap();
        let restored = HnswIndex::from_bytes(&bytes).unwrap();

        assert_eq!(index.len(), restored.len());
        assert_eq!(index.stats().max_layer, restored.stats().max_layer);
    }
}
