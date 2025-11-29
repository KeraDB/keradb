use clap::{Parser, Subcommand};
use keradb::{Database, cli::{Repl, TuiApp}};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "keradb")]
#[command(version = env!("CARGO_PKG_VERSION"))]
#[command(author = "KeraDB Contributors")]
#[command(about = "KeraDB - A lightweight embedded NoSQL database", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Create a new database
    Create {
        /// Path to the database file
        path: PathBuf,
    },
    
    /// Open interactive shell (basic REPL)
    Shell {
        /// Path to the database file
        path: PathBuf,
    },

    /// Open interactive TUI (Terminal User Interface)
    Tui {
        /// Optional: Path to database file (opens connection manager if not provided)
        path: Option<PathBuf>,
    },
    
    /// Show database statistics
    Stats {
        /// Path to the database file
        path: PathBuf,
    },
    
    /// Execute a single query
    Query {
        /// Path to the database file
        path: PathBuf,
        
        /// Query to execute
        query: String,
    },
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let cli = Cli::parse();

    // If no command provided, launch TUI directly
    let command = cli.command.unwrap_or(Commands::Tui { path: None });

    match command {
        Commands::Create { path } => {
            if path.exists() {
                eprintln!("Error: Database file already exists: {}", path.display());
                std::process::exit(1);
            }

            Database::create(&path)?;
            println!("Created database: {}", path.display());
        }

        Commands::Shell { path } => {
            let mut repl = Repl::new(&path)?;
            repl.run()?;
        }

        Commands::Tui { path } => {
            let mut tui = match path {
                Some(p) => TuiApp::with_database(&p)?,
                None => TuiApp::new()?,
            };
            tui.run()?;
        }

        Commands::Stats { path } => {
            let db = Database::open(&path)?;
            
            let collections = db.list_collections();
            let total_docs: usize = collections.iter().map(|(_, count)| count).sum();
            
            let file_size = std::fs::metadata(&path)?.len();
            let file_size_mb = file_size as f64 / 1024.0 / 1024.0;
            
            println!("Database: {}", path.display());
            println!("Size: {:.2} MB", file_size_mb);
            println!("Collections: {}", collections.len());
            println!("Total Documents: {}", total_docs);
            println!();
            
            if !collections.is_empty() {
                println!("Collections:");
                for (name, count) in collections {
                    println!("  {} - {} documents", name, count);
                }
            }
        }

        Commands::Query { path, query } => {
            let db = Database::open(&path)?;
            
            // Simple query parser: "find <collection> [id]"
            let parts: Vec<&str> = query.split_whitespace().collect();
            
            if parts.is_empty() {
                eprintln!("Error: Empty query");
                std::process::exit(1);
            }

            match parts[0] {
                "find" => {
                    if parts.len() < 2 {
                        eprintln!("Usage: find <collection> [id]");
                        std::process::exit(1);
                    }
                    
                    let collection = parts[1];
                    
                    if parts.len() == 2 {
                        // Find all
                        let docs = db.find_all(collection, Some(10), None)?;
                        let json = serde_json::to_string_pretty(&docs)?;
                        println!("{}", json);
                    } else {
                        // Find by ID
                        let doc = db.find_by_id(collection, parts[2])?;
                        let json = serde_json::to_string_pretty(&doc.to_value())?;
                        println!("{}", json);
                    }
                }
                "count" => {
                    if parts.len() < 2 {
                        eprintln!("Usage: count <collection>");
                        std::process::exit(1);
                    }
                    
                    let collection = parts[1];
                    let count = db.count(collection);
                    println!("{}", count);
                }
                _ => {
                    eprintln!("Unknown query command: {}", parts[0]);
                    std::process::exit(1);
                }
            }
        }
    }

    Ok(())
}
