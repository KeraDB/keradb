use thiserror::Error;

#[derive(Error, Debug)]
pub enum KeraDBError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("Database not found: {0}")]
    DatabaseNotFound(String),

    #[error("Collection not found: {0}")]
    CollectionNotFound(String),

    #[error("Document not found: {0}")]
    DocumentNotFound(String),

    #[error("Invalid database format: {0}")]
    InvalidFormat(String),

    #[error("Database version mismatch: expected {expected}, got {actual}")]
    VersionMismatch { expected: u32, actual: u32 },

    #[error("Checksum mismatch: data may be corrupted")]
    ChecksumMismatch,

    #[error("Database is locked by another process")]
    DatabaseLocked,

    #[error("Invalid query: {0}")]
    InvalidQuery(String),

    #[error("Invalid document: {0}")]
    InvalidDocument(String),

    #[error("Duplicate key: {0}")]
    DuplicateKey(String),

    #[error("Index error: {0}")]
    IndexError(String),

    #[error("Transaction error: {0}")]
    TransactionError(String),

    #[error("Storage error: {0}")]
    StorageError(String),

    #[error("Parse error: {0}")]
    ParseError(String),

    #[error("Collection already exists: {0}")]
    CollectionExists(String),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Not implemented: {0}")]
    NotImplemented(String),

    #[error("Vector error: {0}")]
    VectorError(String),

    #[error("Embedding error: {0}")]
    EmbeddingError(String),
}

pub type Result<T> = std::result::Result<T, KeraDBError>;

impl From<bincode::Error> for KeraDBError {
    fn from(err: bincode::Error) -> Self {
        KeraDBError::Serialization(err.to_string())
    }
}

impl From<serde_json::Error> for KeraDBError {
    fn from(err: serde_json::Error) -> Self {
        KeraDBError::Serialization(err.to_string())
    }
}
