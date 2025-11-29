# Contributing to KeraDB

Thank you for your interest in contributing to KeraDB! This document provides guidelines and information about the project structure.

## 📁 Project Architecture

### Core Database Engine (`src/`)

The main database engine is located in the `src/` directory and is written in Rust:

```
src/
├── lib.rs              # Library API entry point
├── main.rs             # CLI binary entry point
├── types.rs            # Core type definitions
├── error.rs            # Error handling
├── ffi.rs              # Foreign Function Interface for C bindings
├── cli/
│   ├── mod.rs          # CLI module
│   └── repl.rs         # Interactive REPL implementation
├── storage/
│   ├── mod.rs          # Storage layer module
│   ├── pager.rs        # Page-based file I/O
│   ├── buffer.rs       # Buffer pool management
│   └── serializer.rs   # Document serialization
└── execution/
    ├── mod.rs          # Execution layer module
    ├── executor.rs     # Query executor
    └── index.rs        # Primary key indexing
```

#### Key Components

1. **Storage Layer** (`src/storage/`)
   - **Pager**: Manages page-based file I/O with 4KB pages
   - **Buffer Pool**: In-memory caching of pages
   - **Serializer**: Converts documents to/from binary format

2. **Execution Layer** (`src/execution/`)
   - **Executor**: Handles CRUD operations
   - **Index**: Maintains primary key indexes for fast lookups

3. **CLI** (`src/cli/`)
   - Interactive REPL for database operations
   - Command-line interface for database management

### SDKs (`sdks/`)

Multi-language bindings for KeraDB:

```
sdks/
├── rust/               # Native Rust SDK (wrapper)
├── nodejs/             # Node.js/TypeScript SDK
├── python/             # Python SDK
├── go/                 # Go SDK
└── csharp/             # C# SDK
```

Each SDK provides language-idiomatic bindings to the core database engine through FFI.

### Examples and Tests

- `examples/` - Example `.ndb` database files and demonstration scripts
- `tests/` - Integration tests for the database engine
- `benches/` - Performance benchmarks using Criterion

### Documentation (`docs/`)

```
docs/
├── getting-started.md      # Quick start guide
├── installation/           # Installation guides and methods
├── development/            # Implementation and development docs
└── planning/               # Original design documents
```

### Scripts (`scripts/`)

Installation and utility scripts:
- `install.sh` - Unix/Linux installation
- `install.ps1` - Windows PowerShell installation
- `install-wsl.sh` - WSL-specific installation
- `demo.sh` - Demo script
- `test.sh` - Test runner

### Example Application (`KeraDB-labs/`)

A complete web application demonstrating KeraDB usage:
- `backend/` - Rust API server using KeraDB
- `frontend/` - React/TypeScript web interface

## 🔧 Development Setup

### Prerequisites

- Rust 1.70 or later
- Cargo (comes with Rust)

### Building from Source

```bash
# Clone the repository
git clone https://github.com/yourusername/KeraDB.git
cd KeraDB

# Build in development mode
cargo build

# Build in release mode (optimized)
cargo build --release

# Run tests
cargo test

# Run benchmarks
cargo bench
```

### Running the CLI

```bash
# Using cargo run
cargo run -- create test.ndb
cargo run -- shell test.ndb

# Or use the compiled binary
./target/release/KeraDB shell test.ndb
```

## 🧪 Testing

### Running Tests

```bash
# Run all tests
cargo test

# Run tests with output
cargo test -- --nocapture

# Run specific test
cargo test test_name

# Run integration tests
cargo test --test integration_tests
```

### Adding Tests

- Unit tests: Add `#[cfg(test)]` modules in the same file
- Integration tests: Add files to `tests/` directory

## 📝 Code Style

- Follow Rust standard formatting: `cargo fmt`
- Run clippy for linting: `cargo clippy`
- Write clear comments for complex logic
- Add documentation comments (`///`) for public APIs

## 🚀 Contributing Guidelines

### Reporting Bugs

1. Check if the issue already exists
2. Create a new issue with:
   - Clear description
   - Steps to reproduce
   - Expected vs actual behavior
   - System information (OS, Rust version)

### Suggesting Features

1. Open an issue describing the feature
2. Explain the use case and benefits
3. Discuss implementation approach

### Submitting Pull Requests

1. Fork the repository
2. Create a feature branch (`git checkout -b feature/my-feature`)
3. Make your changes
4. Add tests for new functionality
5. Ensure all tests pass (`cargo test`)
6. Format code (`cargo fmt`)
7. Run clippy (`cargo clippy`)
8. Commit with clear messages
9. Push to your fork
10. Create a pull request

### Commit Messages

Use clear, descriptive commit messages:

```
Add support for bulk insert operations

- Implement batch insert method in executor
- Add tests for bulk operations
- Update documentation
```

## 🎯 Areas for Contribution

### High Priority

- **Query Filters**: Add WHERE clause support for filtering
- **Secondary Indexes**: Support indexing on non-ID fields
- **Write-Ahead Log (WAL)**: Improve durability
- **Transaction Support**: ACID guarantees
- **Compression**: Reduce file size

### Medium Priority

- **Backup/Restore**: Database backup utilities
- **Replication**: Master-slave replication
- **Performance**: Optimization opportunities
- **Documentation**: More examples and tutorials

### Good First Issues

- Add more example applications
- Improve error messages
- Write additional tests
- Update documentation
- Fix compiler warnings

## 📚 Learning Resources

If you're new to database development:

1. Read the source code starting with `src/lib.rs`
2. Check out `docs/development/IMPLEMENTATION.md`
3. Review existing tests in `tests/`
4. Look at benchmark code in `benches/`

### Recommended Reading

- "Database Internals" by Alex Petrov
- SQLite documentation and source code
- Rust embedded database projects (sled, redb)

## 🤝 Code of Conduct

- Be respectful and inclusive
- Welcome newcomers
- Provide constructive feedback
- Focus on what is best for the community

## 📄 License

By contributing, you agree that your contributions will be licensed under the MIT License.

## 💬 Getting Help

- Open an issue for questions
- Check existing documentation
- Review closed issues for similar problems

---

Thank you for contributing to KeraDB! 🎉
