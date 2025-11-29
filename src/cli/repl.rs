use crate::Database;
use crate::vector::{VectorConfig, Distance};
use rustyline::error::ReadlineError;
use rustyline::DefaultEditor;
use std::path::Path;

pub struct Repl {
    db: Database,
    editor: DefaultEditor,
}

impl Repl {
    pub fn new<P: AsRef<Path>>(path: P) -> anyhow::Result<Self> {
        let db = if path.as_ref().exists() {
            Database::open(path)?
        } else {
            Database::create(path)?
        };

        let editor = DefaultEditor::new()?;

        Ok(Self { db, editor })
    }

    pub fn run(&mut self) -> anyhow::Result<()> {
        println!("NoSQLite Interactive Shell");
        println!("Type 'help' for commands, 'exit' to quit\n");

        loop {
            let readline = self.editor.readline("nosqlite> ");
            match readline {
                Ok(line) => {
                    let line = line.trim();
                    if line.is_empty() {
                        continue;
                    }

                    self.editor.add_history_entry(line)?;

                    if let Err(e) = self.execute_command(line) {
                        eprintln!("Error: {}", e);
                    }
                }
                Err(ReadlineError::Interrupted) => {
                    println!("^C");
                    continue;
                }
                Err(ReadlineError::Eof) => {
                    println!("exit");
                    break;
                }
                Err(err) => {
                    eprintln!("Error: {:?}", err);
                    break;
                }
            }
        }

        Ok(())
    }

    fn execute_command(&self, line: &str) -> anyhow::Result<()> {
        let parts: Vec<&str> = line.split_whitespace().collect();
        
        if parts.is_empty() {
            return Ok(());
        }

        match parts[0] {
            "help" => self.show_help(),
            "exit" | "quit" => std::process::exit(0),
            "collections" => self.list_collections()?,
            "insert" => self.insert(&parts[1..])?,
            "find" => self.find(&parts[1..])?,
            "update" => self.update(&parts[1..])?,
            "delete" => self.delete(&parts[1..])?,
            "count" => self.count(&parts[1..])?,
            "sync" => {
                self.db.sync()?;
                println!("Database synced to disk");
            }
            // Vector database commands
            "vcreate" => self.vector_create(&parts[1..])?,
            "vinsert" => self.vector_insert(&parts[1..])?,
            "vsearch" => self.vector_search(&parts[1..])?,
            "vcollections" => self.list_vector_collections()?,
            "vstats" => self.vector_stats(&parts[1..])?,
            "vdrop" => self.vector_drop(&parts[1..])?,
            _ => {
                println!("Unknown command: {}. Type 'help' for available commands.", parts[0]);
            }
        }

        Ok(())
    }

    fn show_help(&self) {
        println!("Available commands:");
        println!();
        println!("  === Document Database ===");
        println!("  help                                  - Show this help message");
        println!("  collections                           - List all collections");
        println!("  insert <collection> <json>            - Insert a document");
        println!("  find <collection> [id]                - Find document(s)");
        println!("  update <collection> <id> <json>       - Update a document");
        println!("  delete <collection> <id>              - Delete a document");
        println!("  count <collection>                    - Count documents in collection");
        println!("  sync                                  - Sync database to disk");
        println!();
        println!("  === Vector Database ===");
        println!("  vcreate <name> <dims> [distance]      - Create vector collection");
        println!("  vinsert <collection> <vector> [json]  - Insert a vector");
        println!("  vsearch <collection> <vector> [k]     - Search for similar vectors");
        println!("  vcollections                          - List vector collections");
        println!("  vstats <collection>                   - Show vector collection stats");
        println!("  vdrop <collection>                    - Drop vector collection");
        println!();
        println!("  exit/quit                             - Exit the shell");
        println!();
        println!("Examples:");
        println!("  insert users {{\"name\":\"Alice\",\"age\":30}}");
        println!("  find users abc123");
        println!();
        println!("  vcreate embeddings 384 cosine");
        println!("  vinsert embeddings [0.1,0.2,0.3,...] {{\"source\":\"doc1\"}}");
        println!("  vsearch embeddings [0.1,0.2,0.3,...] 10");
    }

    fn list_collections(&self) -> anyhow::Result<()> {
        let collections = self.db.list_collections();
        
        if collections.is_empty() {
            println!("No collections found");
            return Ok(());
        }

        println!("Collections:");
        for (name, count) in collections {
            println!("  {} ({} documents)", name, count);
        }
        
        Ok(())
    }

    fn insert(&self, args: &[&str]) -> anyhow::Result<()> {
        if args.len() < 2 {
            println!("Usage: insert <collection> <json>");
            return Ok(());
        }

        let collection = args[0];
        let json_str = args[1..].join(" ");
        let data: serde_json::Value = serde_json::from_str(&json_str)?;

        let id = self.db.insert(collection, data)?;
        println!("Inserted document with ID: {}", id);

        Ok(())
    }

    fn find(&self, args: &[&str]) -> anyhow::Result<()> {
        if args.is_empty() {
            println!("Usage: find <collection> [id]");
            return Ok(());
        }

        let collection = args[0];

        if args.len() == 1 {
            // Find all
            let docs = self.db.find_all(collection, Some(10), None)?;
            
            if docs.is_empty() {
                println!("No documents found");
                return Ok(());
            }

            println!("Found {} document(s):", docs.len());
            for doc in docs {
                let json = serde_json::to_string_pretty(&doc.to_value())?;
                println!("{}", json);
            }
        } else {
            // Find by ID
            let doc_id = args[1];
            let doc = self.db.find_by_id(collection, doc_id)?;
            let json = serde_json::to_string_pretty(&doc.to_value())?;
            println!("{}", json);
        }

        Ok(())
    }

    fn update(&self, args: &[&str]) -> anyhow::Result<()> {
        if args.len() < 3 {
            println!("Usage: update <collection> <id> <json>");
            return Ok(());
        }

        let collection = args[0];
        let doc_id = args[1];
        let json_str = args[2..].join(" ");
        let data: serde_json::Value = serde_json::from_str(&json_str)?;

        let updated = self.db.update(collection, doc_id, data)?;
        println!("Updated document:");
        let json = serde_json::to_string_pretty(&updated.to_value())?;
        println!("{}", json);

        Ok(())
    }

    fn delete(&self, args: &[&str]) -> anyhow::Result<()> {
        if args.len() < 2 {
            println!("Usage: delete <collection> <id>");
            return Ok(());
        }

        let collection = args[0];
        let doc_id = args[1];

        let deleted = self.db.delete(collection, doc_id)?;
        println!("Deleted document:");
        let json = serde_json::to_string_pretty(&deleted.to_value())?;
        println!("{}", json);

        Ok(())
    }

    fn count(&self, args: &[&str]) -> anyhow::Result<()> {
        if args.is_empty() {
            println!("Usage: count <collection>");
            return Ok(());
        }

        let collection = args[0];
        let count = self.db.count(collection);
        println!("{} documents in collection '{}'", count, collection);

        Ok(())
    }

    // ============================================================
    // Vector Database Commands
    // ============================================================

    fn vector_create(&self, args: &[&str]) -> anyhow::Result<()> {
        if args.len() < 2 {
            println!("Usage: vcreate <name> <dimensions> [distance]");
            println!("  distance: cosine (default), euclidean, dot_product, manhattan");
            return Ok(());
        }

        let name = args[0];
        let dimensions: usize = args[1].parse()
            .map_err(|_| anyhow::anyhow!("Invalid dimensions: {}", args[1]))?;

        let distance = if args.len() > 2 {
            match args[2].to_lowercase().as_str() {
                "cosine" => Distance::Cosine,
                "euclidean" | "l2" => Distance::Euclidean,
                "dot" | "dot_product" | "inner" => Distance::DotProduct,
                "manhattan" | "l1" => Distance::Manhattan,
                _ => {
                    println!("Unknown distance metric: {}. Using cosine.", args[2]);
                    Distance::Cosine
                }
            }
        } else {
            Distance::Cosine
        };

        let config = VectorConfig::new(dimensions).with_distance(distance);
        self.db.create_vector_collection(name, config)?;
        
        println!("Created vector collection '{}' with {} dimensions ({} distance)", 
                 name, dimensions, distance.name());

        Ok(())
    }

    fn vector_insert(&self, args: &[&str]) -> anyhow::Result<()> {
        if args.len() < 2 {
            println!("Usage: vinsert <collection> <vector_json> [metadata_json]");
            println!("  Example: vinsert embeddings [0.1,0.2,0.3] {{\"source\":\"doc1\"}}");
            return Ok(());
        }

        let collection = args[0];
        let vector: Vec<f32> = serde_json::from_str(args[1])
            .map_err(|e| anyhow::anyhow!("Invalid vector JSON: {}", e))?;

        let metadata = if args.len() > 2 {
            let json_str = args[2..].join(" ");
            Some(serde_json::from_str(&json_str)?)
        } else {
            None
        };

        let id = self.db.insert_vector(collection, vector, metadata)?;
        println!("Inserted vector with ID: {}", id);

        Ok(())
    }

    fn vector_search(&self, args: &[&str]) -> anyhow::Result<()> {
        if args.len() < 2 {
            println!("Usage: vsearch <collection> <query_vector_json> [k]");
            println!("  Example: vsearch embeddings [0.1,0.2,0.3] 10");
            return Ok(());
        }

        let collection = args[0];
        let query: Vec<f32> = serde_json::from_str(args[1])
            .map_err(|e| anyhow::anyhow!("Invalid query vector JSON: {}", e))?;

        let k: usize = if args.len() > 2 {
            args[2].parse().unwrap_or(10)
        } else {
            10
        };

        let results = self.db.vector_search(collection, &query, k)?;

        if results.is_empty() {
            println!("No results found");
            return Ok(());
        }

        println!("Found {} result(s):", results.len());
        for result in results {
            println!("  ID: {}, Score: {:.6}", result.document.id, result.score);
            if result.document.metadata != serde_json::Value::Null {
                println!("    Metadata: {}", result.document.metadata);
            }
        }

        Ok(())
    }

    fn list_vector_collections(&self) -> anyhow::Result<()> {
        let collections = self.db.list_vector_collections();

        if collections.is_empty() {
            println!("No vector collections found");
            return Ok(());
        }

        println!("Vector Collections:");
        for (name, count) in collections {
            println!("  {} ({} vectors)", name, count);
        }

        Ok(())
    }

    fn vector_stats(&self, args: &[&str]) -> anyhow::Result<()> {
        if args.is_empty() {
            println!("Usage: vstats <collection>");
            return Ok(());
        }

        let collection = args[0];
        let stats = self.db.vector_stats(collection)?;

        println!("Vector Collection: {}", stats.name);
        println!("  Vectors:      {}", stats.vector_count);
        println!("  Dimensions:   {}", stats.dimensions);
        println!("  Distance:     {}", stats.distance.name());
        println!("  Memory (est): {} KB", stats.memory_bytes / 1024);
        println!("  HNSW M:       {}", stats.hnsw_layers);
        println!("  Lazy Mode:    {}", stats.lazy_embedding);

        Ok(())
    }

    fn vector_drop(&self, args: &[&str]) -> anyhow::Result<()> {
        if args.is_empty() {
            println!("Usage: vdrop <collection>");
            return Ok(());
        }

        let collection = args[0];
        if self.db.drop_vector_collection(collection)? {
            println!("Dropped vector collection '{}'", collection);
        } else {
            println!("Vector collection '{}' not found", collection);
        }

        Ok(())
    }
}
