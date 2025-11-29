//! Distance/similarity functions for vector operations
//! 
//! Provides efficient implementations of various distance metrics used in
//! approximate nearest neighbor search.

use super::types::{Distance, Embedding};

/// Calculate distance between two vectors using the specified metric
#[inline]
pub fn calculate_distance(a: &Embedding, b: &Embedding, metric: Distance) -> f32 {
    debug_assert_eq!(a.len(), b.len(), "Vector dimensions must match");
    
    match metric {
        Distance::Cosine => cosine_distance(a, b),
        Distance::Euclidean => euclidean_distance(a, b),
        Distance::DotProduct => dot_product_distance(a, b),
        Distance::Manhattan => manhattan_distance(a, b),
    }
}

/// Cosine distance: 1 - cosine_similarity
/// Range: [0, 2], where 0 = identical, 2 = opposite
#[inline]
pub fn cosine_distance(a: &Embedding, b: &Embedding) -> f32 {
    let dot = dot_product(a, b);
    let norm_a = norm(a);
    let norm_b = norm(b);
    
    if norm_a == 0.0 || norm_b == 0.0 {
        return 1.0; // Undefined, return neutral distance
    }
    
    let similarity = dot / (norm_a * norm_b);
    // Clamp to handle floating point errors
    1.0 - similarity.clamp(-1.0, 1.0)
}

/// Cosine similarity: dot(a, b) / (||a|| * ||b||)
/// Range: [-1, 1], where 1 = identical, -1 = opposite
#[inline]
pub fn cosine_similarity(a: &Embedding, b: &Embedding) -> f32 {
    1.0 - cosine_distance(a, b)
}

/// Euclidean (L2) distance
#[inline]
pub fn euclidean_distance(a: &Embedding, b: &Embedding) -> f32 {
    euclidean_distance_squared(a, b).sqrt()
}

/// Squared Euclidean distance (faster, no sqrt)
#[inline]
pub fn euclidean_distance_squared(a: &Embedding, b: &Embedding) -> f32 {
    a.iter()
        .zip(b.iter())
        .map(|(x, y)| {
            let diff = x - y;
            diff * diff
        })
        .sum()
}

/// Dot product distance (negative dot product for ranking)
/// Lower is better (more similar)
#[inline]
pub fn dot_product_distance(a: &Embedding, b: &Embedding) -> f32 {
    -dot_product(a, b)
}

/// Manhattan (L1) distance
#[inline]
pub fn manhattan_distance(a: &Embedding, b: &Embedding) -> f32 {
    a.iter()
        .zip(b.iter())
        .map(|(x, y)| (x - y).abs())
        .sum()
}

/// Dot product of two vectors
#[inline]
pub fn dot_product(a: &Embedding, b: &Embedding) -> f32 {
    a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
}

/// L2 norm (magnitude) of a vector
#[inline]
pub fn norm(v: &Embedding) -> f32 {
    v.iter().map(|x| x * x).sum::<f32>().sqrt()
}

/// Normalize a vector to unit length
pub fn normalize(v: &mut Embedding) {
    let n = norm(v);
    if n > 0.0 {
        for x in v.iter_mut() {
            *x /= n;
        }
    }
}

/// Create a normalized copy of a vector
pub fn normalized(v: &Embedding) -> Embedding {
    let mut result = v.clone();
    normalize(&mut result);
    result
}

/// SIMD-optimized distance functions (when available)
#[cfg(target_arch = "x86_64")]
pub mod simd {
    use super::*;

    /// SIMD-optimized dot product (uses auto-vectorization hints)
    #[inline]
    pub fn dot_product_simd(a: &Embedding, b: &Embedding) -> f32 {
        // Process in chunks of 8 for better vectorization
        let chunks = a.len() / 8;
        let mut sum = 0.0f32;
        
        for i in 0..chunks {
            let offset = i * 8;
            let mut chunk_sum = 0.0f32;
            for j in 0..8 {
                chunk_sum += a[offset + j] * b[offset + j];
            }
            sum += chunk_sum;
        }
        
        // Handle remainder
        for i in (chunks * 8)..a.len() {
            sum += a[i] * b[i];
        }
        
        sum
    }

    /// SIMD-optimized squared Euclidean distance
    #[inline]
    pub fn euclidean_squared_simd(a: &Embedding, b: &Embedding) -> f32 {
        let chunks = a.len() / 8;
        let mut sum = 0.0f32;
        
        for i in 0..chunks {
            let offset = i * 8;
            let mut chunk_sum = 0.0f32;
            for j in 0..8 {
                let diff = a[offset + j] - b[offset + j];
                chunk_sum += diff * diff;
            }
            sum += chunk_sum;
        }
        
        for i in (chunks * 8)..a.len() {
            let diff = a[i] - b[i];
            sum += diff * diff;
        }
        
        sum
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cosine_distance() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        assert!((cosine_distance(&a, &b) - 0.0).abs() < 1e-6);

        let c = vec![0.0, 1.0, 0.0];
        assert!((cosine_distance(&a, &c) - 1.0).abs() < 1e-6);

        let d = vec![-1.0, 0.0, 0.0];
        assert!((cosine_distance(&a, &d) - 2.0).abs() < 1e-6);
    }

    #[test]
    fn test_euclidean_distance() {
        let a = vec![0.0, 0.0, 0.0];
        let b = vec![3.0, 4.0, 0.0];
        assert!((euclidean_distance(&a, &b) - 5.0).abs() < 1e-6);
    }

    #[test]
    fn test_normalize() {
        let mut v = vec![3.0, 4.0];
        normalize(&mut v);
        assert!((norm(&v) - 1.0).abs() < 1e-6);
        assert!((v[0] - 0.6).abs() < 1e-6);
        assert!((v[1] - 0.8).abs() < 1e-6);
    }

    #[test]
    fn test_manhattan_distance() {
        let a = vec![1.0, 2.0, 3.0];
        let b = vec![4.0, 6.0, 8.0];
        assert!((manhattan_distance(&a, &b) - 12.0).abs() < 1e-6);
    }
}
