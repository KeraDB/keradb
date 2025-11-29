use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId};
use keradb::{Database, VectorConfig, Distance, CompressionConfig, CompressionMode};
use serde_json::json;
use tempfile::tempdir;

fn benchmark_insert(c: &mut Criterion) {
    let dir = tempdir().unwrap();
    let path = dir.path().join("bench.ndb");
    let db = Database::create(&path).unwrap();

    c.bench_function("insert_single", |b| {
        let mut counter = 0;
        b.iter(|| {
            counter += 1;
            db.insert("users", black_box(json!({
                "name": format!("User {}", counter),
                "age": 30,
                "email": format!("user{}@example.com", counter)
            }))).unwrap();
        });
    });
}

fn benchmark_find_by_id(c: &mut Criterion) {
    let dir = tempdir().unwrap();
    let path = dir.path().join("bench.ndb");
    let db = Database::create(&path).unwrap();

    // Pre-populate with documents
    let mut ids = Vec::new();
    for i in 0..1000 {
        let id = db.insert("users", json!({
            "name": format!("User {}", i),
            "age": 30 + (i % 50)
        })).unwrap();
        ids.push(id);
    }

    c.bench_function("find_by_id", |b| {
        let mut idx = 0;
        b.iter(|| {
            idx = (idx + 1) % ids.len();
            black_box(db.find_by_id("users", &ids[idx]).unwrap());
        });
    });
}

fn benchmark_update(c: &mut Criterion) {
    let dir = tempdir().unwrap();
    let path = dir.path().join("bench.ndb");
    let db = Database::create(&path).unwrap();

    // Pre-populate
    let mut ids = Vec::new();
    for i in 0..100 {
        let id = db.insert("users", json!({
            "name": format!("User {}", i),
            "age": 30
        })).unwrap();
        ids.push(id);
    }

    c.bench_function("update", |b| {
        let mut idx = 0;
        b.iter(|| {
            idx = (idx + 1) % ids.len();
            db.update("users", &ids[idx], black_box(json!({
                "name": format!("User {}", idx),
                "age": 31
            }))).unwrap();
        });
    });
}

fn benchmark_bulk_insert(c: &mut Criterion) {
    let mut group = c.benchmark_group("bulk_insert");
    
    for size in [10, 100, 1000].iter() {
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, &size| {
            b.iter(|| {
                let dir = tempdir().unwrap();
                let path = dir.path().join("bench.ndb");
                let db = Database::create(&path).unwrap();

                for i in 0..size {
                    db.insert("items", black_box(json!({
                        "index": i,
                        "value": format!("item_{}", i)
                    }))).unwrap();
                }
            });
        });
    }
    
    group.finish();
}

fn benchmark_find_all(c: &mut Criterion) {
    let dir = tempdir().unwrap();
    let path = dir.path().join("bench.ndb");
    let db = Database::create(&path).unwrap();

    // Pre-populate
    for i in 0..1000 {
        db.insert("users", json!({
            "name": format!("User {}", i),
            "age": 30 + (i % 50)
        })).unwrap();
    }

    c.bench_function("find_all_limit_10", |b| {
        b.iter(|| {
            black_box(db.find_all("users", Some(10), None).unwrap());
        });
    });

    c.bench_function("find_all_limit_100", |b| {
        b.iter(|| {
            black_box(db.find_all("users", Some(100), None).unwrap());
        });
    });
}

// ============================================================
// Vector Database Benchmarks
// ============================================================

fn random_vector(dim: usize) -> Vec<f32> {
    (0..dim).map(|i| ((i * 7 + 13) % 100) as f32 / 100.0).collect()
}

fn random_vector_seeded(dim: usize, seed: usize) -> Vec<f32> {
    (0..dim).map(|i| (((i + seed) * 17 + 31) % 100) as f32 / 100.0).collect()
}

fn benchmark_vector_insert(c: &mut Criterion) {
    let mut group = c.benchmark_group("vector_insert");
    
    for dim in [32, 128, 384, 768].iter() {
        group.bench_with_input(BenchmarkId::new("dims", dim), dim, |b, &dim| {
            let dir = tempdir().unwrap();
            let path = dir.path().join("bench.ndb");
            let db = Database::create(&path).unwrap();
            
            let config = VectorConfig::new(dim).with_distance(Distance::Cosine);
            db.create_vector_collection("vectors", config).unwrap();
            
            let mut counter = 0;
            b.iter(|| {
                counter += 1;
                let vec = random_vector_seeded(dim, counter);
                db.insert_vector("vectors", black_box(vec), None).unwrap();
            });
        });
    }
    
    group.finish();
}

fn benchmark_vector_search(c: &mut Criterion) {
    let mut group = c.benchmark_group("vector_search");
    
    // Test with different collection sizes
    for (size, dim) in [(100, 128), (1000, 128), (10000, 128), (1000, 384)].iter() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("bench.ndb");
        let db = Database::create(&path).unwrap();
        
        let config = VectorConfig::new(*dim).with_distance(Distance::Cosine);
        db.create_vector_collection("vectors", config).unwrap();
        
        // Pre-populate
        for i in 0..*size {
            let vec = random_vector_seeded(*dim, i);
            db.insert_vector("vectors", vec, None).unwrap();
        }
        
        let query = random_vector(*dim);
        
        group.bench_with_input(
            BenchmarkId::new(format!("size_{}_dim_{}", size, dim), "k10"),
            &query,
            |b, query| {
                b.iter(|| {
                    black_box(db.vector_search("vectors", query, 10).unwrap());
                });
            },
        );
    }
    
    group.finish();
}

fn benchmark_vector_search_varying_k(c: &mut Criterion) {
    let mut group = c.benchmark_group("vector_search_k");
    
    let dir = tempdir().unwrap();
    let path = dir.path().join("bench.ndb");
    let db = Database::create(&path).unwrap();
    
    let dim = 128;
    let config = VectorConfig::new(dim).with_distance(Distance::Cosine);
    db.create_vector_collection("vectors", config).unwrap();
    
    // Pre-populate with 5000 vectors
    for i in 0..5000 {
        let vec = random_vector_seeded(dim, i);
        db.insert_vector("vectors", vec, None).unwrap();
    }
    
    let query = random_vector(dim);
    
    for k in [1, 5, 10, 25, 50, 100].iter() {
        group.bench_with_input(BenchmarkId::from_parameter(k), k, |b, &k| {
            b.iter(|| {
                black_box(db.vector_search("vectors", &query, k).unwrap());
            });
        });
    }
    
    group.finish();
}

fn benchmark_vector_distance_metrics(c: &mut Criterion) {
    let mut group = c.benchmark_group("vector_distance");
    
    let dim = 384;
    let size = 1000;
    
    for distance in [Distance::Cosine, Distance::Euclidean, Distance::DotProduct].iter() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("bench.ndb");
        let db = Database::create(&path).unwrap();
        
        let config = VectorConfig::new(dim).with_distance(*distance);
        db.create_vector_collection("vectors", config).unwrap();
        
        for i in 0..size {
            let vec = random_vector_seeded(dim, i);
            db.insert_vector("vectors", vec, None).unwrap();
        }
        
        let query = random_vector(dim);
        
        group.bench_with_input(
            BenchmarkId::new("metric", distance.name()),
            &query,
            |b, query| {
                b.iter(|| {
                    black_box(db.vector_search("vectors", query, 10).unwrap());
                });
            },
        );
    }
    
    group.finish();
}

fn benchmark_vector_bulk_insert(c: &mut Criterion) {
    let mut group = c.benchmark_group("vector_bulk_insert");
    
    for size in [100, 500, 1000, 5000].iter() {
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, &size| {
            b.iter(|| {
                let dir = tempdir().unwrap();
                let path = dir.path().join("bench.ndb");
                let db = Database::create(&path).unwrap();
                
                let config = VectorConfig::new(128).with_distance(Distance::Cosine);
                db.create_vector_collection("vectors", config).unwrap();
                
                for i in 0..size {
                    let vec = random_vector_seeded(128, i);
                    db.insert_vector("vectors", black_box(vec), None).unwrap();
                }
            });
        });
    }
    
    group.finish();
}

// ============================================
// Compression Benchmarks
// ============================================

fn benchmark_compression_ratio(c: &mut Criterion) {
    use keradb::vector::compression::{CompressedVectorStore, DeltaCompressor};
    
    let mut group = c.benchmark_group("compression_ratio");
    
    // Test with different vector dimensions
    for dims in [128, 384, 768, 1536] {
        group.bench_with_input(BenchmarkId::new("dims", dims), &dims, |b, &dims| {
            b.iter(|| {
                let config = CompressionConfig {
                    mode: CompressionMode::Delta,
                    sparsity_threshold: 0.001,
                    max_density: 0.15,
                    anchor_frequency: 8,
                    quantization_bits: 8,
                };
                let mut store = CompressedVectorStore::new(dims, config);
                
                // Create base vector
                let base: Vec<f32> = (0..dims).map(|i| (i as f32 * 0.01).sin()).collect();
                store.insert(0, base.clone(), None);
                
                // Insert 100 similar vectors
                for i in 1..100 {
                    let mut v = base.clone();
                    // Modify ~5% of components (sparse difference like real embeddings)
                    for j in (0..dims).step_by(20) {
                        v[j] += 0.1 * (i as f32 * 0.01);
                    }
                    store.insert(i, black_box(v), Some(0));
                }
                
                black_box(store.stats())
            });
        });
    }
    
    group.finish();
}

fn benchmark_compression_decompression(c: &mut Criterion) {
    use keradb::vector::compression::{CompressedVectorStore, CompressionConfig, CompressionMode};
    
    let mut group = c.benchmark_group("compression_decompress");
    
    // Setup: create a compressed store
    let dims = 768;
    let config = CompressionConfig {
        mode: CompressionMode::Delta,
        sparsity_threshold: 0.001,
        max_density: 0.15,
        anchor_frequency: 8,
        quantization_bits: 8,
    };
    let mut store = CompressedVectorStore::new(dims, config);
    
    let base: Vec<f32> = (0..dims).map(|i| (i as f32 * 0.01).sin()).collect();
    store.insert(0, base.clone(), None);
    
    for i in 1..100 {
        let mut v = base.clone();
        for j in (0..dims).step_by(20) {
            v[j] += 0.1;
        }
        store.insert(i, v, Some(0));
    }
    
    // Benchmark decompression
    group.bench_function("decompress_768dim", |b| {
        let mut idx = 1;
        b.iter(|| {
            idx = (idx % 99) + 1;
            black_box(store.get_full(idx as u64))
        });
    });
    
    group.finish();
}

fn benchmark_compressed_vs_uncompressed(c: &mut Criterion) {
    let mut group = c.benchmark_group("compressed_vs_uncompressed");
    group.sample_size(50);
    
    let dims = 768;
    
    // Benchmark uncompressed
    group.bench_function("uncompressed_insert_1000", |b| {
        b.iter(|| {
            let dir = tempdir().unwrap();
            let path = dir.path().join("bench.ndb");
            let db = Database::create(&path).unwrap();
            
            let config = VectorConfig::new(dims)
                .with_distance(Distance::Cosine);
            db.create_vector_collection("vectors", config).unwrap();
            
            let base: Vec<f32> = (0..dims).map(|i| (i as f32 * 0.01).sin()).collect();
            for i in 0..1000 {
                let mut v = base.clone();
                for j in (0..dims).step_by(20) {
                    v[j] += 0.1 * (i as f32 * 0.001);
                }
                db.insert_vector("vectors", black_box(v), None).unwrap();
            }
        });
    });
    
    // Benchmark compressed
    group.bench_function("compressed_insert_1000", |b| {
        b.iter(|| {
            let dir = tempdir().unwrap();
            let path = dir.path().join("bench.ndb");
            let db = Database::create(&path).unwrap();
            
            let config = VectorConfig::new(dims)
                .with_distance(Distance::Cosine)
                .with_delta_compression();
            db.create_vector_collection("vectors", config).unwrap();
            
            let base: Vec<f32> = (0..dims).map(|i| (i as f32 * 0.01).sin()).collect();
            for i in 0..1000 {
                let mut v = base.clone();
                for j in (0..dims).step_by(20) {
                    v[j] += 0.1 * (i as f32 * 0.001);
                }
                db.insert_vector("vectors", black_box(v), None).unwrap();
            }
        });
    });
    
    group.finish();
}

criterion_group!(
    benches,
    benchmark_insert,
    benchmark_find_by_id,
    benchmark_update,
    benchmark_bulk_insert,
    benchmark_find_all
);

criterion_group!(
    vector_benches,
    benchmark_vector_insert,
    benchmark_vector_search,
    benchmark_vector_search_varying_k,
    benchmark_vector_distance_metrics,
    benchmark_vector_bulk_insert
);

criterion_group!(
    compression_benches,
    benchmark_compression_ratio,
    benchmark_compression_decompression,
    benchmark_compressed_vs_uncompressed
);

criterion_main!(benches, vector_benches, compression_benches);
