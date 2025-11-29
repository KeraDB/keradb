use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

/// A document ID (UUID v4)
pub type DocumentId = String;

/// A document is a JSON value with a unique ID
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Document {
    #[serde(rename = "_id")]
    pub id: DocumentId,
    #[serde(flatten)]
    pub data: Value,
}

impl Document {
    /// Create a new document with an auto-generated ID
    pub fn new(data: Value) -> Self {
        let id = Uuid::new_v4().to_string();
        Self { id, data }
    }

    /// Create a document with a specific ID
    pub fn with_id(id: DocumentId, data: Value) -> Self {
        Self { id, data }
    }

    /// Get a field value from the document
    pub fn get(&self, field: &str) -> Option<Value> {
        if field == "_id" {
            Some(Value::String(self.id.clone()))
        } else {
            self.data.get(field).cloned()
        }
    }

    /// Set a field value in the document
    pub fn set(&mut self, field: &str, value: Value) {
        if field != "_id" {
            if let Value::Object(ref mut map) = self.data {
                map.insert(field.to_string(), value);
            }
        }
    }

    /// Convert to JSON value (includes _id)
    pub fn to_value(&self) -> Value {
        let mut obj = self.data.clone();
        if let Value::Object(ref mut map) = obj {
            map.insert("_id".to_string(), Value::String(self.id.clone()));
        }
        obj
    }
}

/// Metadata about a collection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectionMetadata {
    pub name: String,
    pub document_count: usize,
    pub created_at: i64,
    pub updated_at: i64,
}

impl CollectionMetadata {
    pub fn new(name: String) -> Self {
        let now = chrono::Utc::now().timestamp();
        Self {
            name,
            document_count: 0,
            created_at: now,
            updated_at: now,
        }
    }
}

/// Page types in the storage engine
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum PageType {
    Meta = 0,
    Data = 1,
    Index = 2,
    Free = 3,
    VectorData = 4,
    VectorIndex = 5,
}

impl TryFrom<u8> for PageType {
    type Error = crate::error::KeraDBError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(PageType::Meta),
            1 => Ok(PageType::Data),
            2 => Ok(PageType::Index),
            3 => Ok(PageType::Free),
            4 => Ok(PageType::VectorData),
            5 => Ok(PageType::VectorIndex),
            _ => Err(crate::error::KeraDBError::InvalidFormat(
                format!("Invalid page type: {}", value),
            )),
        }
    }
}

/// Database configuration
#[derive(Debug, Clone)]
pub struct Config {
    pub page_size: usize,
    pub cache_size: usize,
    pub auto_checkpoint: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            page_size: 4096,      // 4KB pages
            cache_size: 100,      // 100 pages in cache
            auto_checkpoint: true,
        }
    }
}

/// Query filter operators
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FilterOp {
    Eq(Value),
    Ne(Value),
    Gt(Value),
    Gte(Value),
    Lt(Value),
    Lte(Value),
    In(Vec<Value>),
    Nin(Vec<Value>),
}

/// Query filter
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Filter {
    pub field: String,
    pub op: FilterOp,
}
