use chrono::{DateTime, Utc};
use crate::Database;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

const SYSTEM_DB_NAME: &str = ".keradb_system.db";
const CONNECTIONS_COLLECTION: &str = "connections";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseConnection {
    pub id: String,
    pub path: String,
    pub name: String,
    pub created_at: DateTime<Utc>,
    pub last_accessed: DateTime<Utc>,
    pub access_count: u64,
    pub collections_count: usize,
    pub total_documents: usize,
}

impl DatabaseConnection {
    pub fn format_last_accessed(&self) -> String {
        let now = Utc::now();
        let diff = now.signed_duration_since(self.last_accessed);
        
        let minutes = diff.num_minutes();
        let hours = diff.num_hours();
        let days = diff.num_days();

        if minutes < 1 {
            "Just now".to_string()
        } else if minutes < 60 {
            format!("{}m ago", minutes)
        } else if hours < 24 {
            format!("{}h ago", hours)
        } else if days < 7 {
            format!("{}d ago", days)
        } else {
            self.last_accessed.format("%Y-%m-%d").to_string()
        }
    }
}

pub struct SystemDatabase {
    db: Database,
    path: PathBuf,
}

impl SystemDatabase {
    /// Get the system database path in an OS-agnostic way
    fn get_system_db_path() -> anyhow::Result<PathBuf> {
        let home = if cfg!(target_os = "windows") {
            std::env::var("USERPROFILE")
                .or_else(|_| std::env::var("HOMEDRIVE")
                    .and_then(|drive| std::env::var("HOMEPATH")
                        .map(|path| format!("{}{}", drive, path))))
                .map_err(|_| anyhow::anyhow!("Could not determine user home directory"))?
        } else {
            std::env::var("HOME")
                .map_err(|_| anyhow::anyhow!("Could not determine user home directory"))?
        };

        let mut path = PathBuf::from(home);
        path.push(".keradb");
        
        // Create the directory if it doesn't exist
        if !path.exists() {
            std::fs::create_dir_all(&path)?;
        }
        
        path.push(SYSTEM_DB_NAME);
        Ok(path)
    }

    /// Initialize or open the system database
    pub fn init() -> anyhow::Result<Self> {
        let db_path = Self::get_system_db_path()?;
        
        let db = if db_path.exists() {
            Database::open(&db_path)?
        } else {
            Database::create(&db_path)?
        };

        Ok(Self { db, path: db_path })
    }

    /// Get the system database path
    pub fn path(&self) -> &PathBuf {
        &self.path
    }

    /// Register a new database connection or update existing
    pub fn register_connection(&self, path: &str) -> anyhow::Result<String> {
        let now = Utc::now();
        
        // Extract database name from path
        let name = std::path::Path::new(path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string();
        
        // Check if connection already exists
        if let Ok((id, existing)) = self.find_connection_by_path(path) {
            // Update existing connection
            let updated = DatabaseConnection {
                id: id.clone(),
                path: path.to_string(),
                name,
                created_at: existing.created_at,
                last_accessed: now,
                access_count: existing.access_count + 1,
                collections_count: existing.collections_count,
                total_documents: existing.total_documents,
            };
            
            let doc = serde_json::to_value(&updated)?;
            self.db.update(CONNECTIONS_COLLECTION, &id, doc)?;
            self.db.sync()?;
            return Ok(id);
        }

        // Create new connection
        let connection = DatabaseConnection {
            id: String::new(),
            path: path.to_string(),
            name,
            created_at: now,
            last_accessed: now,
            access_count: 1,
            collections_count: 0,
            total_documents: 0,
        };

        let doc = serde_json::to_value(&connection)?;
        let id = self.db.insert(CONNECTIONS_COLLECTION, doc)?;
        self.db.sync()?;
        
        Ok(id)
    }

    /// Update database statistics
    pub fn update_connection_stats(
        &self,
        path: &str,
        collections_count: usize,
        total_documents: usize,
    ) -> anyhow::Result<()> {
        if let Ok((id, mut conn)) = self.find_connection_by_path(path) {
            conn.collections_count = collections_count;
            conn.total_documents = total_documents;
            conn.last_accessed = Utc::now();
            
            let doc = serde_json::to_value(&conn)?;
            self.db.update(CONNECTIONS_COLLECTION, &id, doc)?;
            self.db.sync()?;
        }
        Ok(())
    }

    /// Find connection by database path
    fn find_connection_by_path(&self, path: &str) -> anyhow::Result<(String, DatabaseConnection)> {
        let docs = self.db.find_all(CONNECTIONS_COLLECTION, None, None)?;
        
        for doc in docs {
            let doc_value = doc.to_value();
            
            let doc_id = doc_value.get("_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("Missing _id field"))?
                .to_string();
            
            let conn: DatabaseConnection = serde_json::from_value(doc_value)?;
            
            if conn.path == path {
                return Ok((doc_id, conn));
            }
        }
        
        Err(anyhow::anyhow!("Connection not found"))
    }

    /// Get all registered connections
    pub fn list_connections(&self) -> anyhow::Result<Vec<DatabaseConnection>> {
        let docs = self.db.find_all(CONNECTIONS_COLLECTION, None, None)?;
        
        let mut connections = Vec::new();
        for doc in docs {
            let doc_value = doc.to_value();
            
            if let Ok(conn) = serde_json::from_value::<DatabaseConnection>(doc_value) {
                connections.push(conn);
            }
        }
        
        // Sort by last accessed (most recent first)
        connections.sort_by(|a, b| b.last_accessed.cmp(&a.last_accessed));
        
        Ok(connections)
    }

    /// Remove a connection from the system database
    pub fn remove_connection(&self, path: &str) -> anyhow::Result<()> {
        if let Ok((id, _conn)) = self.find_connection_by_path(path) {
            self.db.delete(CONNECTIONS_COLLECTION, &id)?;
            self.db.sync()?;
        }
        Ok(())
    }

    /// Get the most recently used connection
    pub fn get_last_connection(&self) -> anyhow::Result<Option<DatabaseConnection>> {
        let connections = self.list_connections()?;
        Ok(connections.into_iter().next())
    }
}
