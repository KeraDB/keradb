use crate::error::Result;
use crate::types::Document;
use serde::{Deserialize, Serialize};

pub struct Serializer;

impl Serializer {
    /// Serialize a document to bytes
    pub fn serialize(doc: &Document) -> Result<Vec<u8>> {
        // Convert to JSON string first, then serialize the string
        let json_str = serde_json::to_string(doc)?;
        Ok(bincode::serialize(&json_str)?)
    }

    /// Deserialize bytes to a document
    pub fn deserialize(bytes: &[u8]) -> Result<Document> {
        // Deserialize to JSON string first, then parse
        let json_str: String = bincode::deserialize(bytes)?;
        Ok(serde_json::from_str(&json_str)?)
    }

    /// Serialize any serializable value
    pub fn serialize_value<T: Serialize>(value: &T) -> Result<Vec<u8>> {
        let json_str = serde_json::to_string(value)?;
        Ok(bincode::serialize(&json_str)?)
    }

    /// Deserialize to any deserializable value
    pub fn deserialize_value<T: for<'de> Deserialize<'de>>(bytes: &[u8]) -> Result<T> {
        let json_str: String = bincode::deserialize(bytes)?;
        Ok(serde_json::from_str(&json_str)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_serialize_document() {
        let doc = Document::new(json!({"name": "Alice", "age": 30}));
        let bytes = Serializer::serialize(&doc).unwrap();
        let deserialized = Serializer::deserialize(&bytes).unwrap();
        
        assert_eq!(doc.id, deserialized.id);
        assert_eq!(doc.data, deserialized.data);
    }
}
