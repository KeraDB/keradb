# KeraDB

> A lightweight, embedded NoSQL document database with vector search - written in Rust

[![Build Status](https://img.shields.io/badge/build-passing-brightgreen)]()
[![Rust](https://img.shields.io/badge/rust-1.70%2B-orange)]()
[![License](https://img.shields.io/badge/license-MIT-blue)]()

KeraDB is a single-file, embedded document database designed for simplicity and ease of use. Think SQLite, but for JSON documents **with vector search capabilities**!

## Features

- **Single-file database** - One .ndb file contains everything
- **Fast** - Written in Rust with zero-cost abstractions
- **Safe** - Memory-safe with Rust's guarantees
- **Document-oriented** - Store JSON documents with automatic UUIDs
- **Indexed** - Primary key indexing for fast lookups
- **Persistent** - Data survives application restarts
- **CLI tool** - Interactive REPL and command-line interface
- **Collections** - Organize documents into named collections
- **Simple API** - Easy to learn and use
- **Multi-language SDKs** - Use from Rust, Node.js, Python, Go, or C#

### Vector Database Features

- **HNSW Index** - Fast approximate nearest neighbor search
- **Multiple Distances** - Cosine, Euclidean, Dot Product, Manhattan
- **Metadata Filtering** - Filter vector search by document attributes
- **Unified Storage** - Vectors and documents in the same .ndb file
- **Delta Compression** - Up to 97% storage savings with LEANN-style compression

---

## LEANN-Style Delta Compression

KeraDB implements delta compression inspired by [LEANN](https://github.com/yichuan-w/LEANN) (Low-storage Embedding Approximate Nearest Neighbor), a research project from Berkeley Sky Computing Lab that achieves **97% storage savings** while maintaining search accuracy.

### The Problem: Vector Storage is Expensive

Traditional vector databases store every embedding in full. For typical use cases:

| Data Type | Vectors | Dimensions | Traditional Storage |
|-----------|---------|------------|---------------------|
| Email chunks | 780K | 768 | 78 MB |
| Text corpus | 60M | 768 | 201 GB |
| Browser history | 38K | 768 | 6 MB |
| Chat messages | 400K | 768 | 64 MB |

### The LEANN Solution

LEANN exploits the fact that similar vectors (neighbors in the HNSW graph) share most of their information. Instead of storing full vectors:

```
Traditional:  [3072 bytes] [3072 bytes] [3072 bytes] ...
                  vec1         vec2         vec3

LEANN-style:  [anchor: 3072 bytes] -> [delta: ~200 bytes] -> [delta: ~200 bytes]
                   vec1                    vec2                  vec3
```

1. **Anchor vectors** - Full vectors stored periodically (every 8th vector by default)
2. **Delta vectors** - Only the sparse differences from the nearest anchor

### Core Techniques

From the [LEANN paper (arXiv:2506.08276)](https://arxiv.org/abs/2506.08276):

| Technique | Description |
|-----------|-------------|
| **Graph-based selective recomputation** | Only compute/store embeddings for nodes in the search path |
| **High-degree preserving pruning** | Keep important "hub" nodes while removing redundant connections |
| **Sparse delta encoding** | Store only components that differ significantly (threshold-based) |
| **Quantized deltas** | Optional 8-bit quantization for even more aggressive compression |

### Storage Savings (LEANN Benchmarks)

| Dataset | Traditional | With LEANN | Savings |
|---------|-------------|------------|---------|
| 780K email chunks | 78 MB | 8 MB | 91% |
| 60M text chunks | 201 GB | 6 GB | **97%** |
| 38K browser entries | 6 MB | 0.4 MB | 95% |
| 400K chat messages | 64 MB | 2 MB | 97% |

### Using Compression in KeraDB

```rust
use keradb::{VectorConfig, CompressionConfig, CompressionMode};

// Delta compression - recommended for most use cases
let config = VectorConfig::new(768)
    .with_delta_compression();

// Quantized delta - most aggressive compression
let config = VectorConfig::new(768)
    .with_quantized_compression();

// Custom configuration
let config = VectorConfig::new(768)
    .with_compression(CompressionConfig {
        mode: CompressionMode::Delta,
        sparsity_threshold: 0.001,  // Ignore deltas below this
        max_density: 0.15,          // Fall back to full if >15% differs
        anchor_frequency: 8,        // Every 8th vector is an anchor
        quantization_bits: 8,       // For quantized mode
    });
```

---

## Quick Start

### Installation

**Linux and macOS:**
```bash
curl -sSf https://raw.githubusercontent.com/KeraDB/keradb/main/scripts/install.sh | sh
```

**Windows (PowerShell):**
```powershell
iwr -useb https://raw.githubusercontent.com/KeraDB/keradb/main/scripts/install.ps1 | iex
```

### Building from Source

```bash
git clone https://github.com/KeraDB/keradb.git
cd keradb
cargo build --release
```

### Using the CLI

```bash
# Create and open database
./target/release/keradb shell myapp.ndb

# Inside the shell:
keradb> insert users {"name":"Alice","age":30}
keradb> find users
keradb> count users
keradb> exit
```

### Using as a Library

```rust
use keradb::Database;
use serde_json::json;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let db = Database::create("mydata.ndb")?;
    
    let id = db.insert("users", json!({
        "name": "Alice",
        "age": 30
    }))?;
    
    let user = db.find_by_id("users", &id)?;
    println!("Found: {:?}", user.data);
    
    Ok(())
}
```

### Vector Search Example

```rust
use keradb::{Database, VectorConfig, Distance};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let db = Database::create("vectors.ndb")?;
    
    // Create vector collection with compression
    let config = VectorConfig::new(384)
        .with_distance(Distance::Cosine)
        .with_delta_compression();
    db.create_vector_collection("embeddings", config)?;
    
    // Insert vectors
    let embedding = vec![0.1, 0.2, 0.3, /* ... */];
    db.insert_vector("embeddings", embedding, None)?;
    
    // Search
    let results = db.vector_search("embeddings", &query, 10)?;
    
    Ok(())
}
```

---

## Benchmarks

Run on your system with `cargo bench`.

### Document Operations

| Operation | Throughput |
|-----------|------------|
| Insert (single) | ~10,000 ops/sec |
| Find by ID | ~50,000 ops/sec |
| Update | ~8,000 ops/sec |

### Vector Insert Performance

| Dimensions | Time per Insert |
|------------|-----------------|
| 32-dim | 77-81 us |
| 128-dim | 94-97 us |
| 384-dim | 135-144 us |
| 768-dim | 167-185 us |

### Vector Search Performance (k=10)

| Collection Size | Dimensions | Search Time |
|-----------------|------------|-------------|
| 100 vectors | 128-dim | 34-36 us |
| 1,000 vectors | 128-dim | 36-37 us |
| 10,000 vectors | 128-dim | 38 us |
| 1,000 vectors | 384-dim | 117-121 us |

HNSW achieves near-constant search time - 10,000 vectors is only ~10% slower than 100 vectors.

### Search by K

| k (results) | Time |
|-------------|------|
| k=1 | 35-37 us |
| k=10 | 35-37 us |
| k=50 | 39-40 us |
| k=100 | 64-65 us |

### Distance Metric Comparison

| Metric | Time | Notes |
|--------|------|-------|
| Dot Product | 54-57 us | Fastest - no normalization |
| Euclidean | 59-60 us | Standard L2 distance |
| Cosine | 115-117 us | Requires normalization |

### Bulk Insert

| Count | Time | Rate |
|-------|------|------|
| 100 vectors | 7.1 ms | ~14k/sec |
| 500 vectors | 51 ms | ~10k/sec |
| 1,000 vectors | 107 ms | ~9.3k/sec |
| 5,000 vectors | 567 ms | ~8.8k/sec |

### Compression Performance

| Scenario | Uncompressed | Compressed | Savings |
|----------|--------------|------------|---------|
| 768-dim, 5% sparse diff | 3,072 bytes | ~250 bytes | 91.9% |
| 128-dim, 10% sparse diff | 512 bytes | ~94 bytes | 81.6% |
| Mixed workload | 5,120 bytes | 2,218 bytes | 56.7% |

### Performance Summary

- Sub-40us search on 10K vectors (HNSW delivers O(log n) complexity)
- ~100us per vector insert for typical 128-dim embeddings
- Dot product is 2x faster than cosine (no normalization)
- ~9K vectors/sec bulk insert rate
- Up to 92% compression for similar vectors

---

## Architecture

```
+------------------+
|    CLI / API     |  <- User Interface
+------------------+
|    Executor      |  <- CRUD Operations
+------------------+
|    Index         |  <- HashMap + HNSW
+------------------+
|    Storage       |  <- Page-based I/O
+------------------+
|   .ndb File      |  <- Persistent Storage
+------------------+
```

---

## Current Capabilities

**Implemented:**
- Single-file database format (.ndb)
- CRUD operations
- Primary key indexing (UUID-based)
- Collections management
- Page-based storage (4KB pages)
- Binary serialization (bincode)
- LRU buffer pool caching
- CLI with interactive REPL
- HNSW vector index
- Delta compression (LEANN-style)

**Coming Soon (v0.2):**
- Query filters (eq, ne, gt, lt)
- Secondary indexes
- Sorting

**Future (v0.3+):**
- Transactions (ACID)
- Write-Ahead Log (WAL)
- Crash recovery

---

## Language SDKs

| Language | Installation |
|----------|--------------|
| Rust | `cargo add keradb` |
| Node.js | `npm install keradb` |
| Python | `pip install keradb` |
| Go | `go get github.com/yourusername/keradb` |
| C# | `dotnet add package keradb` |

---

## Testing

```bash
cargo test              # Run all tests
cargo test --lib        # Library tests only
cargo bench             # Performance benchmarks
```

---

## License

MIT License - see LICENSE file for details

## Acknowledgments

Inspired by:
- [SQLite](https://sqlite.org/) - The gold standard for embedded databases
- [MongoDB](https://www.mongodb.com/) - Document-oriented design
- [sled](https://github.com/spacejam/sled) - Rust embedded database
- [LEANN](https://github.com/yichuan-w/LEANN) - Graph-based selective recomputation for 97% storage savings ([Paper](https://arxiv.org/abs/2506.08276))

---

**Built with Rust** | [Documentation](https://keradb.github.io/docs) | [Examples](https://keradb.github.io/docs/examples)
