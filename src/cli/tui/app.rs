use crate::Database;
use crate::vector::{VectorConfig, Distance};
use crate::cli::system_db::{SystemDatabase, DatabaseConnection};
use anyhow::Result;
use crossterm::{
    event::{KeyCode, KeyEvent, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen, Clear, ClearType},
};
use ratatui::{
    backend::CrosstermBackend,
    Terminal,
};
use std::path::Path;
use std::time::Duration;

use super::events::{AppEvent, EventHandler, is_quit_key};
use super::ui;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppMode {
    Normal,
    Insert,
    Command,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusedPanel {
    Connections,
    Collections,
    Query,
    Results,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppScreen {
    ConnectionManager,
    DatabaseExplorer,
}

pub struct TuiApp {
    // System database for connection history
    pub system_db: SystemDatabase,
    
    // Current database connection (optional)
    pub db: Option<Database>,
    pub db_path: Option<String>,
    
    // UI state
    pub screen: AppScreen,
    pub mode: AppMode,
    pub focused: FocusedPanel,
    
    // Input
    pub input: String,
    pub cursor_position: usize,
    pub command_history: Vec<String>,
    pub history_index: Option<usize>,
    
    // Results
    pub results: Vec<String>,
    pub results_scroll: usize,
    
    // Collections
    pub collections: Vec<(String, usize)>,
    pub selected_collection: usize,
    
    // Connection history
    pub connections: Vec<DatabaseConnection>,
    pub selected_connection: usize,
    
    // UI
    pub show_help: bool,
    pub status_message: String,
    pub should_quit: bool,
}

impl TuiApp {
    /// Create a new TUI app without connecting to any database
    pub fn new() -> Result<Self> {
        let system_db = SystemDatabase::init()?;
        let connections = system_db.list_connections().unwrap_or_default();

        Ok(Self {
            system_db,
            db: None,
            db_path: None,
            screen: AppScreen::ConnectionManager,
            mode: AppMode::Normal,
            focused: FocusedPanel::Connections,
            input: String::new(),
            cursor_position: 0,
            command_history: Vec::new(),
            history_index: None,
            results: vec![
                "╔══════════════════════════════════════════════════════════╗".to_string(),
                "║         Welcome to KeraDB Terminal User Interface        ║".to_string(),
                "╚══════════════════════════════════════════════════════════╝".to_string(),
                String::new(),
                "Select a database from history or create/open a new one:".to_string(),
                String::new(),
                "  [n] Create new database".to_string(),
                "  [o] Open existing database".to_string(),
                "  [Enter] Connect to selected".to_string(),
                "  [d] Remove from history".to_string(),
                "  [?] Show help".to_string(),
            ],
            results_scroll: 0,
            collections: Vec::new(),
            selected_collection: 0,
            connections,
            selected_connection: 0,
            show_help: false,
            status_message: "[n] New  [o] Open  [Enter] Connect  [d] Delete  [?] Help  [Ctrl+Q] Quit".into(),
            should_quit: false,
        })
    }

    /// Create a new TUI app and connect to a specific database
    pub fn with_database<P: AsRef<Path>>(path: P) -> Result<Self> {
        let mut app = Self::new()?;
        app.connect_to_database(path.as_ref().to_string_lossy().to_string())?;
        Ok(app)
    }

    /// Connect to a database
    pub fn connect_to_database(&mut self, path: String) -> Result<()> {
        // Open or create the database
        let db = if Path::new(&path).exists() {
            Database::open(&path)?
        } else {
            Database::create(&path)?
        };

        // Register in system database
        self.system_db.register_connection(&path)?;

        // Update stats
        let collections = db.list_collections();
        let total_docs: usize = collections.iter().map(|(_, c)| c).sum();
        self.system_db.update_connection_stats(&path, collections.len(), total_docs)?;

        // Update app state
        self.collections = collections;
        self.db = Some(db);
        self.db_path = Some(path.clone());
        self.screen = AppScreen::DatabaseExplorer;
        self.focused = FocusedPanel::Query;
        self.selected_collection = 0;
        
        self.results.push(String::new());
        self.results.push(format!("✓ Connected to: {}", path));
        self.results.push(format!("  {} collections, {} total documents", 
            self.collections.len(), total_docs));
        self.results.push(String::new());
        self.results.push("Type commands or press [i] to enter insert mode.".to_string());
        
        self.status_message = format!("Connected: {} | [Esc] Disconnect | [?] Help", 
            Path::new(&path).file_name().unwrap_or_default().to_string_lossy());
        
        // Refresh connections list
        self.connections = self.system_db.list_connections().unwrap_or_default();
        
        Ok(())
    }

    /// Disconnect from current database
    pub fn disconnect(&mut self) {
        if let Some(ref path) = self.db_path {
            self.results.push(format!("✓ Disconnected from: {}", path));
        }
        
        self.db = None;
        self.db_path = None;
        self.collections.clear();
        self.screen = AppScreen::ConnectionManager;
        self.focused = FocusedPanel::Connections;
        self.status_message = "[n] New  [o] Open  [Enter] Connect  [d] Delete  [?] Help".into();
        
        // Refresh connections
        self.connections = self.system_db.list_connections().unwrap_or_default();
    }

    pub fn run(&mut self) -> Result<()> {
        // Setup terminal
        enable_raw_mode()?;
        let mut stdout = std::io::stdout();
        execute!(stdout, EnterAlternateScreen, Clear(ClearType::All))?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;
        terminal.clear()?;

        // Event handler
        let events = EventHandler::new(Duration::from_millis(100));

        // Main loop
        while !self.should_quit {
            // Draw UI
            terminal.draw(|frame| ui::render(self, frame))?;

            // Handle events
            match events.next()? {
                AppEvent::Key(key) => self.handle_key_event(key),
                AppEvent::Tick => {}
            }
        }

        // Restore terminal
        disable_raw_mode()?;
        execute!(
            terminal.backend_mut(),
            LeaveAlternateScreen
        )?;
        terminal.show_cursor()?;

        Ok(())
    }

    fn handle_key_event(&mut self, key: KeyEvent) {
        // Global quit
        if is_quit_key(key) {
            self.should_quit = true;
            return;
        }

        match self.mode {
            AppMode::Normal => self.handle_normal_mode(key),
            AppMode::Insert => self.handle_insert_mode(key),
            AppMode::Command => self.handle_command_mode(key),
        }
    }

    fn handle_normal_mode(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('?') | KeyCode::F(1) => {
                self.show_help = !self.show_help;
            }
            KeyCode::Tab => {
                self.focused = match self.screen {
                    AppScreen::ConnectionManager => match self.focused {
                        FocusedPanel::Connections => FocusedPanel::Results,
                        _ => FocusedPanel::Connections,
                    },
                    AppScreen::DatabaseExplorer => match self.focused {
                        FocusedPanel::Collections => FocusedPanel::Query,
                        FocusedPanel::Query => FocusedPanel::Results,
                        FocusedPanel::Results => FocusedPanel::Collections,
                        _ => FocusedPanel::Query,
                    },
                };
            }
            KeyCode::BackTab => {
                self.focused = match self.screen {
                    AppScreen::ConnectionManager => match self.focused {
                        FocusedPanel::Connections => FocusedPanel::Results,
                        _ => FocusedPanel::Connections,
                    },
                    AppScreen::DatabaseExplorer => match self.focused {
                        FocusedPanel::Collections => FocusedPanel::Results,
                        FocusedPanel::Query => FocusedPanel::Collections,
                        FocusedPanel::Results => FocusedPanel::Query,
                        _ => FocusedPanel::Query,
                    },
                };
            }
            // Connection manager specific keys
            KeyCode::Char('n') if self.screen == AppScreen::ConnectionManager => {
                self.mode = AppMode::Insert;
                self.input = "new ".to_string();
                self.cursor_position = self.input.len();
                self.status_message = "Enter path for new database (e.g., new mydb.ndb)".into();
            }
            KeyCode::Char('o') if self.screen == AppScreen::ConnectionManager => {
                self.mode = AppMode::Insert;
                self.input = "open ".to_string();
                self.cursor_position = self.input.len();
                self.status_message = "Enter path to open (e.g., open mydb.ndb)".into();
            }
            KeyCode::Enter if self.screen == AppScreen::ConnectionManager && self.focused == FocusedPanel::Connections => {
                if !self.connections.is_empty() {
                    let conn = &self.connections[self.selected_connection];
                    let path = conn.path.clone();
                    if let Err(e) = self.connect_to_database(path) {
                        self.results.push(format!("✗ Error: {}", e));
                        self.status_message = format!("Failed to connect: {}", e);
                    }
                }
            }
            KeyCode::Char('d') if self.screen == AppScreen::ConnectionManager && self.focused == FocusedPanel::Connections => {
                // Delete connection from history
                if !self.connections.is_empty() {
                    let conn = &self.connections[self.selected_connection];
                    let path = conn.path.clone();
                    if self.system_db.remove_connection(&path).is_ok() {
                        self.results.push(format!("✓ Removed from history: {}", path));
                        self.connections = self.system_db.list_connections().unwrap_or_default();
                        if self.selected_connection >= self.connections.len() && !self.connections.is_empty() {
                            self.selected_connection = self.connections.len() - 1;
                        }
                    }
                }
            }
            // Database explorer specific keys
            KeyCode::Char('i') | KeyCode::Enter if self.focused == FocusedPanel::Query && self.screen == AppScreen::DatabaseExplorer => {
                self.mode = AppMode::Insert;
                self.status_message = "-- INSERT MODE -- (Esc to exit, Enter to execute)".into();
            }
            KeyCode::Char(':') => {
                self.mode = AppMode::Command;
                self.input.clear();
                self.cursor_position = 0;
                self.status_message = ":".into();
            }
            KeyCode::Char('j') | KeyCode::Down => self.handle_down(),
            KeyCode::Char('k') | KeyCode::Up => self.handle_up(),
            KeyCode::Char('g') => {
                match self.focused {
                    FocusedPanel::Connections => self.selected_connection = 0,
                    FocusedPanel::Collections => self.selected_collection = 0,
                    FocusedPanel::Results => self.results_scroll = 0,
                    _ => {}
                }
            }
            KeyCode::Char('G') => {
                match self.focused {
                    FocusedPanel::Connections if !self.connections.is_empty() => {
                        self.selected_connection = self.connections.len() - 1;
                    }
                    FocusedPanel::Collections if !self.collections.is_empty() => {
                        self.selected_collection = self.collections.len() - 1;
                    }
                    FocusedPanel::Results if !self.results.is_empty() => {
                        self.results_scroll = self.results.len().saturating_sub(1);
                    }
                    _ => {}
                }
            }
            KeyCode::Char('r') => {
                self.refresh();
            }
            KeyCode::Esc if self.screen == AppScreen::DatabaseExplorer => {
                self.disconnect();
            }
            _ => {}
        }
    }

    fn handle_insert_mode(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                self.mode = AppMode::Normal;
                self.input.clear();
                self.cursor_position = 0;
                self.update_status_for_screen();
            }
            KeyCode::Enter => {
                self.execute_input();
            }
            KeyCode::Backspace => {
                if self.cursor_position > 0 {
                    self.cursor_position -= 1;
                    self.input.remove(self.cursor_position);
                }
            }
            KeyCode::Delete => {
                if self.cursor_position < self.input.len() {
                    self.input.remove(self.cursor_position);
                }
            }
            KeyCode::Left => {
                self.cursor_position = self.cursor_position.saturating_sub(1);
            }
            KeyCode::Right => {
                if self.cursor_position < self.input.len() {
                    self.cursor_position += 1;
                }
            }
            KeyCode::Home => {
                self.cursor_position = 0;
            }
            KeyCode::End => {
                self.cursor_position = self.input.len();
            }
            KeyCode::Up => {
                if !self.command_history.is_empty() {
                    self.history_index = Some(match self.history_index {
                        Some(i) => i.saturating_sub(1),
                        None => self.command_history.len() - 1,
                    });
                    if let Some(idx) = self.history_index {
                        self.input = self.command_history[idx].clone();
                        self.cursor_position = self.input.len();
                    }
                }
            }
            KeyCode::Down => {
                if let Some(idx) = self.history_index {
                    if idx < self.command_history.len() - 1 {
                        self.history_index = Some(idx + 1);
                        self.input = self.command_history[self.history_index.unwrap()].clone();
                    } else {
                        self.history_index = None;
                        self.input.clear();
                    }
                    self.cursor_position = self.input.len();
                }
            }
            KeyCode::Char(c) => {
                self.input.insert(self.cursor_position, c);
                self.cursor_position += 1;
            }
            _ => {}
        }
    }

    fn handle_command_mode(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                self.mode = AppMode::Normal;
                self.input.clear();
                self.update_status_for_screen();
            }
            KeyCode::Enter => {
                let cmd = self.input.trim().to_string();
                self.input.clear();
                self.mode = AppMode::Normal;
                self.execute_command(&cmd);
            }
            KeyCode::Backspace => {
                if self.cursor_position > 0 {
                    self.cursor_position -= 1;
                    self.input.remove(self.cursor_position);
                }
            }
            KeyCode::Char(c) => {
                self.input.insert(self.cursor_position, c);
                self.cursor_position += 1;
            }
            _ => {}
        }
    }

    fn handle_down(&mut self) {
        match self.focused {
            FocusedPanel::Connections if !self.connections.is_empty() => {
                self.selected_connection = (self.selected_connection + 1) % self.connections.len();
            }
            FocusedPanel::Collections if !self.collections.is_empty() => {
                self.selected_collection = (self.selected_collection + 1) % self.collections.len();
            }
            FocusedPanel::Results if !self.results.is_empty() => {
                if self.results_scroll < self.results.len() - 1 {
                    self.results_scroll += 1;
                }
            }
            _ => {}
        }
    }

    fn handle_up(&mut self) {
        match self.focused {
            FocusedPanel::Connections if !self.connections.is_empty() => {
                self.selected_connection = self.selected_connection
                    .checked_sub(1)
                    .unwrap_or(self.connections.len() - 1);
            }
            FocusedPanel::Collections if !self.collections.is_empty() => {
                self.selected_collection = self.selected_collection
                    .checked_sub(1)
                    .unwrap_or(self.collections.len() - 1);
            }
            FocusedPanel::Results => {
                self.results_scroll = self.results_scroll.saturating_sub(1);
            }
            _ => {}
        }
    }

    fn update_status_for_screen(&mut self) {
        self.status_message = match self.screen {
            AppScreen::ConnectionManager => {
                "[n] New  [o] Open  [Enter] Connect  [d] Delete  [?] Help".into()
            }
            AppScreen::DatabaseExplorer => {
                format!("Connected: {} | [Esc] Disconnect | [i] Insert | [?] Help", 
                    self.db_path.as_ref()
                        .and_then(|p| Path::new(p).file_name())
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_else(|| "unknown".to_string()))
            }
        };
    }

    fn execute_input(&mut self) {
        let input = self.input.trim().to_string();
        if input.is_empty() {
            return;
        }

        // Add to history
        self.command_history.push(input.clone());
        self.history_index = None;

        // Parse and execute
        self.results.push(format!("→ {}", input));
        
        let result = self.process_command(&input);
        match result {
            Ok(output) => {
                for line in output.lines() {
                    self.results.push(line.to_string());
                }
                if !output.is_empty() {
                    self.status_message = "Command executed".into();
                }
            }
            Err(e) => {
                self.results.push(format!("✗ Error: {}", e));
                self.status_message = format!("Error: {}", e);
            }
        }

        self.results.push(String::new());
        self.input.clear();
        self.cursor_position = 0;

        // Auto-scroll to bottom
        if !self.results.is_empty() {
            self.results_scroll = self.results.len().saturating_sub(1);
        }
        
        self.mode = AppMode::Normal;
        self.update_status_for_screen();
    }

    fn process_command(&mut self, input: &str) -> Result<String> {
        let parts: Vec<&str> = input.split_whitespace().collect();
        
        if parts.is_empty() {
            return Ok(String::new());
        }

        match parts[0].to_lowercase().as_str() {
            // Connection commands (work in both screens)
            "new" | "create" => {
                if parts.len() < 2 {
                    return Ok("Usage: new <path>".into());
                }
                let path = parts[1..].join(" ");
                if Path::new(&path).exists() {
                    return Ok(format!("Database already exists: {}. Use 'open' instead.", path));
                }
                self.connect_to_database(path)?;
                Ok(String::new())
            }
            "open" | "connect" => {
                if parts.len() < 2 {
                    return Ok("Usage: open <path>".into());
                }
                let path = parts[1..].join(" ");
                self.connect_to_database(path)?;
                Ok(String::new())
            }
            "disconnect" | "close" => {
                self.disconnect();
                Ok(String::new())
            }
            "connections" | "history" => {
                self.connections = self.system_db.list_connections().unwrap_or_default();
                if self.connections.is_empty() {
                    Ok("No connection history".into())
                } else {
                    let mut output = String::from("Recent Connections:\n");
                    for (i, conn) in self.connections.iter().enumerate() {
                        let marker = if Some(&conn.path) == self.db_path.as_ref() { "→ " } else { "  " };
                        output.push_str(&format!(
                            "{}[{}] {} ({} colls, {} docs) - {}\n",
                            marker, i + 1, conn.name, 
                            conn.collections_count, conn.total_documents,
                            conn.format_last_accessed()
                        ));
                    }
                    Ok(output)
                }
            }
            
            // Database commands (require connection)
            "help" => Ok(self.get_help_text()),
            "collections" | "list" => {
                if self.db.is_none() {
                    return Ok("Not connected. Use 'open <path>' or 'new <path>' first.".into());
                }
                self.refresh_collections();
                if self.collections.is_empty() {
                    Ok("No collections found".into())
                } else {
                    let mut output = String::from("Collections:\n");
                    for (name, count) in &self.collections {
                        output.push_str(&format!("  • {} ({} documents)\n", name, count));
                    }
                    Ok(output)
                }
            }
            "insert" => {
                let db = self.db.as_ref().ok_or_else(|| anyhow::anyhow!("Not connected"))?;
                if parts.len() < 3 {
                    return Ok("Usage: insert <collection> <json>".into());
                }
                let collection = parts[1];
                let json_str = parts[2..].join(" ");
                let doc: serde_json::Value = serde_json::from_str(&json_str)?;
                let id = db.insert(collection, doc)?;
                self.refresh_collections();
                self.update_system_db_stats();
                Ok(format!("✓ Inserted with id: {}", id))
            }
            "find" => {
                let db = self.db.as_ref().ok_or_else(|| anyhow::anyhow!("Not connected"))?;
                if parts.len() < 2 {
                    return Ok("Usage: find <collection> [id]".into());
                }
                let collection = parts[1];
                if parts.len() >= 3 {
                    let id = parts[2];
                    match db.find_by_id(collection, id) {
                        Ok(doc) => Ok(serde_json::to_string_pretty(&doc.to_value())?),
                        Err(_) => Ok(format!("Document not found: {}", id)),
                    }
                } else {
                    let docs = db.find_all(collection, Some(20), None)?;
                    if docs.is_empty() {
                        Ok("No documents found".into())
                    } else {
                        let mut output = serde_json::to_string_pretty(&docs)?;
                        if db.count(collection) > 20 {
                            output.push_str("\n... (showing first 20)");
                        }
                        Ok(output)
                    }
                }
            }
            "delete" => {
                let db = self.db.as_ref().ok_or_else(|| anyhow::anyhow!("Not connected"))?;
                if parts.len() < 3 {
                    return Ok("Usage: delete <collection> <id>".into());
                }
                let collection = parts[1];
                let id = parts[2];
                db.delete(collection, id)?;
                self.refresh_collections();
                self.update_system_db_stats();
                Ok(format!("✓ Deleted: {}", id))
            }
            "count" => {
                let db = self.db.as_ref().ok_or_else(|| anyhow::anyhow!("Not connected"))?;
                if parts.len() < 2 {
                    return Ok("Usage: count <collection>".into());
                }
                let collection = parts[1];
                let count = db.count(collection);
                Ok(format!("{} documents in '{}'", count, collection))
            }
            "sync" => {
                let db = self.db.as_ref().ok_or_else(|| anyhow::anyhow!("Not connected"))?;
                db.sync()?;
                Ok("✓ Database synced to disk".into())
            }
            "clear" => {
                self.results.clear();
                Ok(String::new())
            }
            "vcreate" => {
                let db = self.db.as_ref().ok_or_else(|| anyhow::anyhow!("Not connected"))?;
                if parts.len() < 3 {
                    return Ok("Usage: vcreate <name> <dimensions> [distance]".into());
                }
                let name = parts[1];
                let dims: usize = parts[2].parse()?;
                let distance = if parts.len() > 3 {
                    match parts[3].to_lowercase().as_str() {
                        "euclidean" => Distance::Euclidean,
                        "cosine" => Distance::Cosine,
                        "dot" => Distance::DotProduct,
                        _ => Distance::Cosine,
                    }
                } else {
                    Distance::Cosine
                };
                let config = VectorConfig::new(dims).with_distance(distance);
                db.create_vector_collection(name, config)?;
                Ok(format!("✓ Created vector collection: {} ({} dims)", name, dims))
            }
            "vcollections" => {
                let db = self.db.as_ref().ok_or_else(|| anyhow::anyhow!("Not connected"))?;
                let collections = db.list_vector_collections();
                if collections.is_empty() {
                    Ok("No vector collections found".into())
                } else {
                    let mut output = String::from("Vector Collections:\n");
                    for (name, count) in collections {
                        output.push_str(&format!("  • {} ({} vectors)\n", name, count));
                    }
                    Ok(output)
                }
            }
            _ => Ok(format!("Unknown command: '{}'. Type 'help' for commands.", parts[0])),
        }
    }

    fn execute_command(&mut self, cmd: &str) {
        match cmd {
            "q" | "quit" | "exit" => {
                self.should_quit = true;
            }
            "w" | "write" | "sync" => {
                if let Some(ref db) = self.db {
                    if let Err(e) = db.sync() {
                        self.status_message = format!("Error syncing: {}", e);
                    } else {
                        self.status_message = "✓ Database synced".into();
                    }
                } else {
                    self.status_message = "Not connected to any database".into();
                }
            }
            "wq" => {
                if let Some(ref db) = self.db {
                    let _ = db.sync();
                }
                self.should_quit = true;
            }
            "help" => {
                self.show_help = true;
            }
            "disconnect" | "close" | "disc" => {
                self.disconnect();
            }
            _ => {
                // Try to parse as open command
                if cmd.starts_with("e ") || cmd.starts_with("open ") {
                    let path = cmd.split_whitespace().skip(1).collect::<Vec<_>>().join(" ");
                    if !path.is_empty() {
                        if let Err(e) = self.connect_to_database(path) {
                            self.status_message = format!("Error: {}", e);
                        }
                        return;
                    }
                }
                self.status_message = format!("Unknown command: :{}", cmd);
            }
        }
    }

    fn refresh(&mut self) {
        self.connections = self.system_db.list_connections().unwrap_or_default();
        if self.db.is_some() {
            self.refresh_collections();
        }
        self.status_message = "✓ Refreshed".into();
    }

    fn refresh_collections(&mut self) {
        if let Some(ref db) = self.db {
            self.collections = db.list_collections();
        }
    }

    fn update_system_db_stats(&mut self) {
        if let (Some(ref path), Some(ref db)) = (&self.db_path, &self.db) {
            let collections = db.list_collections();
            let total_docs: usize = collections.iter().map(|(_, c)| c).sum();
            let _ = self.system_db.update_connection_stats(path, collections.len(), total_docs);
        }
    }

    pub fn get_help_text(&self) -> String {
        r#"KeraDB TUI Commands
═══════════════════

CONNECTION
  new <path>          Create new database
  open <path>         Open existing database  
  disconnect          Disconnect current db
  history             Show connection history

DOCUMENTS
  collections         List all collections
  insert <coll> <json>  Insert document
  find <coll> [id]    Find documents
  delete <coll> <id>  Delete document
  count <coll>        Count documents
  sync                Sync to disk

VECTORS
  vcreate <name> <dims> [dist]  Create vector collection
  vcollections        List vector collections

NAVIGATION
  Tab         Switch panels
  j/k ↑/↓     Navigate lists
  g/G         Top/bottom
  i/Enter     Insert mode (in query)
  Esc         Exit mode / Disconnect
  ?           Toggle help
  :q          Quit
  Ctrl+Q      Quit immediately

HOME SCREEN
  n           New database
  o           Open database
  Enter       Connect to selected
  d           Remove from history"#.into()
    }
}
