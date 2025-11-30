//! KeraDB vs SQLite Benchmark Suite
//! 
//! This benchmark compares KeraDB's performance against SQLite across various operations:
//! - Single inserts
//! - Bulk inserts
//! - Point queries (by ID)
//! - Range queries
//! - Updates
//! - Deletes
//! - JSON document operations

use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId, Throughput};
use keradb::Database;
use rusqlite::{Connection, params};
use serde_json::json;
use tempfile::tempdir;
use std::time::Duration;

// ============================================================
// Setup Helpers
// ============================================================

fn setup_keradb() -> (tempfile::TempDir, Database) {
    let dir = tempdir().unwrap();
    let path = dir.path().join("bench.ndb");
    let db = Database::create(&path).unwrap();
    (dir, db)
}

fn setup_sqlite() -> (tempfile::TempDir, Connection) {
    let dir = tempdir().unwrap();
    let path = dir.path().join("bench.db");
    let conn = Connection::open(&path).unwrap();
    
    // Create table with JSON storage pattern
    conn.execute(
        "CREATE TABLE documents (
            id TEXT PRIMARY KEY,
            collection TEXT NOT NULL,
            data TEXT NOT NULL,
            created_at TEXT DEFAULT CURRENT_TIMESTAMP
        )",
        [],
    ).unwrap();
    
    // Create index on collection
    conn.execute(
        "CREATE INDEX idx_collection ON documents(collection)",
        [],
    ).unwrap();
    
    (dir, conn)
}

fn setup_sqlite_with_json() -> (tempfile::TempDir, Connection) {
    let dir = tempdir().unwrap();
    let path = dir.path().join("bench.db");
    let conn = Connection::open(&path).unwrap();
    
    // Enable JSON1 extension features
    conn.execute(
        "CREATE TABLE users (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL,
            age INTEGER,
            email TEXT,
            data JSON
        )",
        [],
    ).unwrap();
    
    conn.execute("CREATE INDEX idx_name ON users(name)", []).unwrap();
    conn.execute("CREATE INDEX idx_age ON users(age)", []).unwrap();
    
    (dir, conn)
}

// ============================================================
// Insert Benchmarks
// ============================================================

fn benchmark_single_insert(c: &mut Criterion) {
    let mut group = c.benchmark_group("single_insert");
    group.measurement_time(Duration::from_secs(5));
    group.sample_size(50);
    
    // KeraDB single insert
    group.bench_function("keradb", |b| {
        let (_dir, db) = setup_keradb();
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
    
    // SQLite single insert (JSON as TEXT)
    group.bench_function("sqlite_json_text", |b| {
        let (_dir, conn) = setup_sqlite();
        let mut counter = 0;
        b.iter(|| {
            counter += 1;
            let id = format!("user_{}", counter);
            let data = serde_json::to_string(&json!({
                "name": format!("User {}", counter),
                "age": 30,
                "email": format!("user{}@example.com", counter)
            })).unwrap();
            conn.execute(
                "INSERT INTO documents (id, collection, data) VALUES (?1, ?2, ?3)",
                params![id, "users", data],
            ).unwrap();
        });
    });
    
    // SQLite single insert (structured columns)
    group.bench_function("sqlite_structured", |b| {
        let (_dir, conn) = setup_sqlite_with_json();
        let mut counter = 0;
        b.iter(|| {
            counter += 1;
            conn.execute(
                "INSERT INTO users (name, age, email) VALUES (?1, ?2, ?3)",
                params![format!("User {}", counter), 30, format!("user{}@example.com", counter)],
            ).unwrap();
        });
    });
    
    group.finish();
}

fn benchmark_bulk_insert(c: &mut Criterion) {
    let mut group = c.benchmark_group("bulk_insert");
    group.measurement_time(Duration::from_secs(10));
    group.sample_size(30);
    
    for size in [100, 1000, 5000].iter() {
        group.throughput(Throughput::Elements(*size as u64));
        
        // KeraDB bulk insert
        group.bench_with_input(BenchmarkId::new("keradb", size), size, |b, &size| {
            b.iter(|| {
                let (_dir, db) = setup_keradb();
                for i in 0..size {
                    db.insert("users", black_box(json!({
                        "name": format!("User {}", i),
                        "age": 25 + (i % 50),
                        "email": format!("user{}@example.com", i),
                        "active": i % 2 == 0
                    }))).unwrap();
                }
            });
        });
        
        // SQLite bulk insert with transaction
        group.bench_with_input(BenchmarkId::new("sqlite_transaction", size), size, |b, &size| {
            b.iter(|| {
                let (_dir, conn) = setup_sqlite();
                conn.execute("BEGIN TRANSACTION", []).unwrap();
                for i in 0..size {
                    let id = format!("user_{}", i);
                    let data = serde_json::to_string(&json!({
                        "name": format!("User {}", i),
                        "age": 25 + (i % 50),
                        "email": format!("user{}@example.com", i),
                        "active": i % 2 == 0
                    })).unwrap();
                    conn.execute(
                        "INSERT INTO documents (id, collection, data) VALUES (?1, ?2, ?3)",
                        params![id, "users", data],
                    ).unwrap();
                }
                conn.execute("COMMIT", []).unwrap();
            });
        });
    }
    
    group.finish();
}

// ============================================================
// Query Benchmarks
// ============================================================

fn benchmark_point_query(c: &mut Criterion) {
    let mut group = c.benchmark_group("point_query");
    group.measurement_time(Duration::from_secs(5));
    group.sample_size(50);
    
    // Pre-populate KeraDB
    let (_kera_dir, kera_db) = setup_keradb();
    let mut kera_ids = Vec::new();
    for i in 0..5000 {
        let id = kera_db.insert("users", json!({
            "name": format!("User {}", i),
            "age": 25 + (i % 50),
            "email": format!("user{}@example.com", i)
        })).unwrap();
        kera_ids.push(id);
    }
    
    // Pre-populate SQLite
    let (_sqlite_dir, sqlite_conn) = setup_sqlite();
    let mut sqlite_ids = Vec::new();
    sqlite_conn.execute("BEGIN TRANSACTION", []).unwrap();
    for i in 0..5000 {
        let id = format!("user_{}", i);
        let data = serde_json::to_string(&json!({
            "name": format!("User {}", i),
            "age": 25 + (i % 50),
            "email": format!("user{}@example.com", i)
        })).unwrap();
        sqlite_conn.execute(
            "INSERT INTO documents (id, collection, data) VALUES (?1, ?2, ?3)",
            params![&id, "users", data],
        ).unwrap();
        sqlite_ids.push(id);
    }
    sqlite_conn.execute("COMMIT", []).unwrap();
    
    // KeraDB point query
    group.bench_function("keradb", |b| {
        let mut idx = 0;
        b.iter(|| {
            idx = (idx + 1) % kera_ids.len();
            black_box(kera_db.find_by_id("users", &kera_ids[idx]).unwrap());
        });
    });
    
    // SQLite point query
    group.bench_function("sqlite", |b| {
        let mut idx = 0;
        let mut stmt = sqlite_conn.prepare_cached(
            "SELECT data FROM documents WHERE id = ?1 AND collection = ?2"
        ).unwrap();
        b.iter(|| {
            idx = (idx + 1) % sqlite_ids.len();
            let result: String = stmt.query_row(
                params![&sqlite_ids[idx], "users"],
                |row| row.get(0)
            ).unwrap();
            black_box(result);
        });
    });
    
    group.finish();
}

fn benchmark_range_query(c: &mut Criterion) {
    let mut group = c.benchmark_group("range_query");
    group.measurement_time(Duration::from_secs(5));
    group.sample_size(50);
    
    // Pre-populate KeraDB
    let (_kera_dir, kera_db) = setup_keradb();
    for i in 0..5000 {
        kera_db.insert("users", json!({
            "name": format!("User {}", i),
            "age": 25 + (i % 50),
            "email": format!("user{}@example.com", i)
        })).unwrap();
    }
    
    // Pre-populate SQLite with structured data
    let (_sqlite_dir, sqlite_conn) = setup_sqlite_with_json();
    sqlite_conn.execute("BEGIN TRANSACTION", []).unwrap();
    for i in 0..5000 {
        sqlite_conn.execute(
            "INSERT INTO users (name, age, email) VALUES (?1, ?2, ?3)",
            params![format!("User {}", i), 25 + (i as i32 % 50), format!("user{}@example.com", i)],
        ).unwrap();
    }
    sqlite_conn.execute("COMMIT", []).unwrap();
    
    for limit in [10, 100, 1000].iter() {
        // KeraDB find_all with limit
        group.bench_with_input(BenchmarkId::new("keradb", limit), limit, |b, &limit| {
            b.iter(|| {
                black_box(kera_db.find_all("users", Some(limit), None).unwrap());
            });
        });
        
        // SQLite SELECT with LIMIT
        group.bench_with_input(BenchmarkId::new("sqlite", limit), limit, |b, &limit| {
            let query = format!("SELECT * FROM users LIMIT {}", limit);
            b.iter(|| {
                let mut stmt = sqlite_conn.prepare_cached(&query).unwrap();
                let results: Vec<(i32, String, i32, String)> = stmt
                    .query_map([], |row| {
                        Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
                    })
                    .unwrap()
                    .filter_map(|r| r.ok())
                    .collect();
                black_box(results);
            });
        });
    }
    
    group.finish();
}

// ============================================================
// Update Benchmarks
// ============================================================

fn benchmark_update(c: &mut Criterion) {
    let mut group = c.benchmark_group("update");
    group.measurement_time(Duration::from_secs(5));
    group.sample_size(50);
    
    // Pre-populate KeraDB
    let (_kera_dir, kera_db) = setup_keradb();
    let mut kera_ids = Vec::new();
    for i in 0..1000 {
        let id = kera_db.insert("users", json!({
            "name": format!("User {}", i),
            "age": 25,
            "email": format!("user{}@example.com", i)
        })).unwrap();
        kera_ids.push(id);
    }
    
    // Pre-populate SQLite
    let (_sqlite_dir, sqlite_conn) = setup_sqlite();
    let mut sqlite_ids = Vec::new();
    sqlite_conn.execute("BEGIN TRANSACTION", []).unwrap();
    for i in 0..1000 {
        let id = format!("user_{}", i);
        let data = serde_json::to_string(&json!({
            "name": format!("User {}", i),
            "age": 25,
            "email": format!("user{}@example.com", i)
        })).unwrap();
        sqlite_conn.execute(
            "INSERT INTO documents (id, collection, data) VALUES (?1, ?2, ?3)",
            params![&id, "users", data],
        ).unwrap();
        sqlite_ids.push(id);
    }
    sqlite_conn.execute("COMMIT", []).unwrap();
    
    // KeraDB update
    group.bench_function("keradb", |b| {
        let mut idx = 0;
        let mut age = 25;
        b.iter(|| {
            idx = (idx + 1) % kera_ids.len();
            age = (age + 1) % 100;
            kera_db.update("users", &kera_ids[idx], black_box(json!({
                "name": format!("User {}", idx),
                "age": age,
                "email": format!("user{}@example.com", idx)
            }))).unwrap();
        });
    });
    
    // SQLite update
    group.bench_function("sqlite", |b| {
        let mut idx = 0;
        let mut age = 25;
        b.iter(|| {
            idx = (idx + 1) % sqlite_ids.len();
            age = (age + 1) % 100;
            let data = serde_json::to_string(&json!({
                "name": format!("User {}", idx),
                "age": age,
                "email": format!("user{}@example.com", idx)
            })).unwrap();
            sqlite_conn.execute(
                "UPDATE documents SET data = ?1 WHERE id = ?2",
                params![data, &sqlite_ids[idx]],
            ).unwrap();
        });
    });
    
    group.finish();
}

// ============================================================
// Delete Benchmarks
// ============================================================

fn benchmark_delete(c: &mut Criterion) {
    let mut group = c.benchmark_group("delete");
    group.measurement_time(Duration::from_secs(5));
    group.sample_size(20);
    
    for size in [100, 500].iter() {
        // KeraDB delete
        group.bench_with_input(BenchmarkId::new("keradb", size), size, |b, &size| {
            b.iter_custom(|iters| {
                let mut total = Duration::ZERO;
                for _ in 0..iters {
                    let (_dir, db) = setup_keradb();
                    let mut ids = Vec::new();
                    for i in 0..size {
                        let id = db.insert("users", json!({
                            "name": format!("User {}", i),
                            "age": 25
                        })).unwrap();
                        ids.push(id);
                    }
                    
                    let start = std::time::Instant::now();
                    for id in ids {
                        db.delete("users", &id).unwrap();
                    }
                    total += start.elapsed();
                }
                total
            });
        });
        
        // SQLite delete
        group.bench_with_input(BenchmarkId::new("sqlite", size), size, |b, &size| {
            b.iter_custom(|iters| {
                let mut total = Duration::ZERO;
                for _ in 0..iters {
                    let (_dir, conn) = setup_sqlite();
                    conn.execute("BEGIN TRANSACTION", []).unwrap();
                    let mut ids = Vec::new();
                    for i in 0..size {
                        let id = format!("user_{}", i);
                        let data = serde_json::to_string(&json!({
                            "name": format!("User {}", i),
                            "age": 25
                        })).unwrap();
                        conn.execute(
                            "INSERT INTO documents (id, collection, data) VALUES (?1, ?2, ?3)",
                            params![&id, "users", data],
                        ).unwrap();
                        ids.push(id);
                    }
                    conn.execute("COMMIT", []).unwrap();
                    
                    let start = std::time::Instant::now();
                    conn.execute("BEGIN TRANSACTION", []).unwrap();
                    for id in ids {
                        conn.execute("DELETE FROM documents WHERE id = ?1", params![id]).unwrap();
                    }
                    conn.execute("COMMIT", []).unwrap();
                    total += start.elapsed();
                }
                total
            });
        });
    }
    
    group.finish();
}

// ============================================================
// JSON Document Operations
// ============================================================

fn benchmark_json_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("json_operations");
    group.measurement_time(Duration::from_secs(5));
    group.sample_size(50);
    
    // Complex nested JSON document
    let complex_doc = json!({
        "user": {
            "profile": {
                "name": "John Doe",
                "age": 30,
                "contact": {
                    "email": "john@example.com",
                    "phone": "+1-555-0123",
                    "addresses": [
                        {"type": "home", "street": "123 Main St", "city": "NYC"},
                        {"type": "work", "street": "456 Office Ave", "city": "Boston"}
                    ]
                }
            },
            "preferences": {
                "theme": "dark",
                "notifications": true,
                "language": "en"
            },
            "tags": ["premium", "verified", "active"]
        },
        "metadata": {
            "created": "2024-01-01T00:00:00Z",
            "updated": "2024-06-15T12:30:00Z",
            "version": 3
        }
    });
    
    // KeraDB complex document insert
    group.bench_function("keradb_complex_insert", |b| {
        let (_dir, db) = setup_keradb();
        let mut counter = 0;
        b.iter(|| {
            counter += 1;
            let mut doc = complex_doc.clone();
            doc["user"]["profile"]["name"] = json!(format!("User {}", counter));
            db.insert("profiles", black_box(doc)).unwrap();
        });
    });
    
    // SQLite complex document insert (JSON as TEXT)
    group.bench_function("sqlite_complex_insert", |b| {
        let (_dir, conn) = setup_sqlite();
        let mut counter = 0;
        b.iter(|| {
            counter += 1;
            let mut doc = complex_doc.clone();
            doc["user"]["profile"]["name"] = json!(format!("User {}", counter));
            let id = format!("profile_{}", counter);
            let data = serde_json::to_string(&doc).unwrap();
            conn.execute(
                "INSERT INTO documents (id, collection, data) VALUES (?1, ?2, ?3)",
                params![id, "profiles", data],
            ).unwrap();
        });
    });
    
    group.finish();
}

// ============================================================
// Concurrent Access Benchmarks
// ============================================================

fn benchmark_read_heavy_workload(c: &mut Criterion) {
    let mut group = c.benchmark_group("read_heavy_workload");
    group.measurement_time(Duration::from_secs(5));
    group.sample_size(30);
    
    // Pre-populate KeraDB with 2000 documents
    let (_kera_dir, kera_db) = setup_keradb();
    let mut kera_ids = Vec::new();
    for i in 0..2000 {
        let id = kera_db.insert("users", json!({
            "name": format!("User {}", i),
            "age": 25 + (i % 50),
            "email": format!("user{}@example.com", i),
            "score": i * 10
        })).unwrap();
        kera_ids.push(id);
    }
    
    // Pre-populate SQLite
    let (_sqlite_dir, sqlite_conn) = setup_sqlite_with_json();
    sqlite_conn.execute("BEGIN TRANSACTION", []).unwrap();
    for i in 0..2000 {
        sqlite_conn.execute(
            "INSERT INTO users (name, age, email, data) VALUES (?1, ?2, ?3, ?4)",
            params![
                format!("User {}", i),
                25 + (i as i32 % 50),
                format!("user{}@example.com", i),
                json!({"score": i * 10}).to_string()
            ],
        ).unwrap();
    }
    sqlite_conn.execute("COMMIT", []).unwrap();
    
    // 80% reads, 20% writes workload
    group.bench_function("keradb_80_20", |b| {
        let mut counter = 0;
        b.iter(|| {
            counter += 1;
            if counter % 5 == 0 {
                // Write operation (20%)
                kera_db.insert("users", json!({
                    "name": format!("NewUser {}", counter),
                    "age": 30,
                    "email": format!("new{}@example.com", counter)
                })).unwrap();
            } else {
                // Read operation (80%)
                let idx = counter % kera_ids.len();
                black_box(kera_db.find_by_id("users", &kera_ids[idx]).unwrap());
            }
        });
    });
    
    group.bench_function("sqlite_80_20", |b| {
        let mut counter = 0;
        let mut read_stmt = sqlite_conn.prepare_cached(
            "SELECT * FROM users WHERE id = ?1"
        ).unwrap();
        b.iter(|| {
            counter += 1;
            if counter % 5 == 0 {
                // Write operation (20%)
                sqlite_conn.execute(
                    "INSERT INTO users (name, age, email) VALUES (?1, ?2, ?3)",
                    params![
                        format!("NewUser {}", counter),
                        30,
                        format!("new{}@example.com", counter)
                    ],
                ).unwrap();
            } else {
                // Read operation (80%)
                let idx = counter % 2000;
                let result: Result<(i32, String, i32, String), _> = read_stmt.query_row(
                    params![idx],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
                );
                black_box(result);
            }
        });
    });
    
    group.finish();
}

// ============================================================
// Memory & Storage Efficiency
// ============================================================

fn benchmark_storage_efficiency(c: &mut Criterion) {
    let mut group = c.benchmark_group("storage_efficiency");
    group.measurement_time(Duration::from_secs(10));
    group.sample_size(10);
    
    // Measure storage for 5000 documents
    let size = 5000;
    
    group.bench_function("keradb_5k_docs", |b| {
        b.iter_custom(|iters| {
            let mut total = Duration::ZERO;
            for _ in 0..iters {
                let dir = tempdir().unwrap();
                let path = dir.path().join("bench.ndb");
                let db = Database::create(&path).unwrap();
                
                let start = std::time::Instant::now();
                for i in 0..size {
                    db.insert("users", json!({
                        "name": format!("User {}", i),
                        "age": 25 + (i % 50),
                        "email": format!("user{}@example.com", i),
                        "bio": "Lorem ipsum dolor sit amet, consectetur adipiscing elit."
                    })).unwrap();
                }
                total += start.elapsed();
                
                // Force sync
                drop(db);
            }
            total
        });
    });
    
    group.bench_function("sqlite_5k_docs", |b| {
        b.iter_custom(|iters| {
            let mut total = Duration::ZERO;
            for _ in 0..iters {
                let dir = tempdir().unwrap();
                let path = dir.path().join("bench.db");
                let conn = Connection::open(&path).unwrap();
                
                conn.execute(
                    "CREATE TABLE documents (id TEXT PRIMARY KEY, collection TEXT, data TEXT)",
                    [],
                ).unwrap();
                
                let start = std::time::Instant::now();
                conn.execute("BEGIN TRANSACTION", []).unwrap();
                for i in 0..size {
                    let id = format!("user_{}", i);
                    let data = serde_json::to_string(&json!({
                        "name": format!("User {}", i),
                        "age": 25 + (i % 50),
                        "email": format!("user{}@example.com", i),
                        "bio": "Lorem ipsum dolor sit amet, consectetur adipiscing elit."
                    })).unwrap();
                    conn.execute(
                        "INSERT INTO documents (id, collection, data) VALUES (?1, ?2, ?3)",
                        params![id, "users", data],
                    ).unwrap();
                }
                conn.execute("COMMIT", []).unwrap();
                total += start.elapsed();
            }
            total
        });
    });
    
    group.finish();
}

criterion_group!(
    comparison_benches,
    benchmark_single_insert,
    benchmark_bulk_insert,
    benchmark_point_query,
    benchmark_range_query,
    benchmark_update,
    benchmark_delete,
    benchmark_json_operations,
    benchmark_read_heavy_workload,
    benchmark_storage_efficiency
);

criterion_main!(comparison_benches);
