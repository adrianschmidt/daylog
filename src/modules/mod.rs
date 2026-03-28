pub mod climbing;
pub mod dashboard;
pub mod training;
pub mod trends;

use crate::config::Config;
use color_eyre::eyre::Result;
use ratatui::layout::Rect;
use ratatui::Frame;
use rusqlite::Connection;
use yaml_rust2::Yaml;

/// An operation returned by a module's normalize() method.
/// The core engine executes all ops in one transaction.
pub enum InsertOp {
    /// Insert into the core `metrics` table. Any module can contribute metrics.
    Metric { name: String, value: f64 },

    /// Insert into a module-owned table. The module provides the table name,
    /// column names, and values. Core executes blindly — the module owns the schema.
    Row {
        table: &'static str,
        columns: Vec<(&'static str, SqlValue)>,
    },
}

/// Subset of SQLite types that InsertOp can carry.
#[derive(Debug, Clone)]
pub enum SqlValue {
    Text(String),
    Integer(i64),
    Real(f64),
    Bool(bool),
    Null,
}

impl SqlValue {
    pub fn to_sql(&self) -> Box<dyn rusqlite::types::ToSql + '_> {
        match self {
            SqlValue::Text(s) => Box::new(s.clone()),
            SqlValue::Integer(i) => Box::new(*i),
            SqlValue::Real(f) => Box::new(*f),
            SqlValue::Bool(b) => Box::new(*b),
            SqlValue::Null => Box::new(Option::<String>::None),
        }
    }
}

/// Describes where a field lives in the YAML frontmatter.
/// Used by `daylog log` to route writes to the correct location.
pub enum YamlPath {
    /// Top-level scalar: `weight: 173.4`
    Scalar(String),
    /// Nested under a parent: `lifts:\n  pullup: BWx8`
    Nested(String, String),
    /// Append to a list: `sends:\n  - V5`
    ListAppend(String),
}

/// The core module trait. Modules are stateless — they hold config (set at
/// construction, immutable) but no mutable state.
///
/// `normalize()` is called on the watcher thread; `draw()` is called on the
/// TUI thread. If a module ever needs cached/derived state, it goes in SQLite.
pub trait Module: Send + Sync {
    /// Unique ID. Matches config key: [modules.climbing]
    fn id(&self) -> &str;

    /// Tab display name
    fn name(&self) -> &str;

    /// SQL CREATE TABLE statements. Run once at DB init.
    fn schema(&self) -> &str {
        ""
    }

    /// Extract domain rows from today's YAML.
    /// Core engine handles the INSERT transaction.
    fn normalize(&self, date: &str, yaml: &Yaml, config: &Config) -> Result<Vec<InsertOp>>;

    /// Render the tab. Module queries its own tables directly.
    /// Connection is opened SQLITE_OPEN_READ_ONLY — modules cannot write from draw().
    fn draw(&self, f: &mut Frame, area: Rect, conn: &Connection, config: &Config);

    /// Optional: per-tab keybinding labels (for help display).
    fn keybindings(&self) -> Vec<(char, &str)> {
        vec![]
    }

    /// Optional: handle a key press. Return true if consumed.
    fn handle_key(&self, _key: char, _conn: &Connection) -> bool {
        false
    }

    /// Optional: contribute fields to `daylog status --json`
    fn status_json(&self, _conn: &Connection, _config: &Config) -> Option<serde_json::Value> {
        None
    }

    /// Optional: map a `daylog log` field to a YAML path.
    /// e.g., training module maps ("lift", "pullup") → YamlPath::Nested("lifts", "pullup")
    fn log_field_path(&self, _field: &str, _subfield: &str) -> Option<YamlPath> {
        None
    }
}

/// Build the module registry based on config.
pub fn build_registry(config: &Config) -> Vec<Box<dyn Module>> {
    let mut m: Vec<Box<dyn Module>> = Vec::new();
    if config.is_enabled("dashboard") {
        m.push(Box::new(dashboard::Dashboard::new(config)));
    }
    if config.is_enabled("training") {
        m.push(Box::new(training::Training::new(config)));
    }
    if config.is_enabled("trends") {
        m.push(Box::new(trends::Trends::new(config)));
    }
    if config.is_enabled("climbing") {
        m.push(Box::new(climbing::Climbing::new(config)));
    }
    m
}

/// Validate that InsertOp table names match module schemas.
/// Called once at startup to prevent typos from causing runtime SQLite errors.
pub fn validate_module_tables(modules: &[Box<dyn Module>]) -> Result<()> {
    use std::collections::HashSet;

    let mut known_tables: HashSet<String> = HashSet::new();
    for module in modules {
        let schema = module.schema();
        // Extract table names from CREATE TABLE statements
        for line in schema.lines() {
            let line = line.trim().to_uppercase();
            if line.starts_with("CREATE TABLE") {
                if let Some(name) = line
                    .strip_prefix("CREATE TABLE IF NOT EXISTS ")
                    .or_else(|| line.strip_prefix("CREATE TABLE "))
                {
                    if let Some(name) = name.split_whitespace().next() {
                        known_tables.insert(name.trim_matches('(').to_lowercase());
                    }
                }
            }
        }
    }

    // Store for runtime validation
    KNOWN_MODULE_TABLES
        .lock()
        .map_err(|e| color_eyre::eyre::eyre!("Failed to lock module tables: {e}"))?
        .clone_from(&known_tables);

    Ok(())
}

/// Check if a table name is valid for a module InsertOp.
pub fn is_valid_module_table(table: &str) -> bool {
    KNOWN_MODULE_TABLES
        .lock()
        .map(|tables| tables.contains(table))
        .unwrap_or(false)
}

static KNOWN_MODULE_TABLES: std::sync::LazyLock<
    std::sync::Mutex<std::collections::HashSet<String>>,
> = std::sync::LazyLock::new(|| std::sync::Mutex::new(std::collections::HashSet::new()));

/// Parse a color name string into a ratatui Color.
/// Shared utility for modules that use config-driven colors.
pub fn parse_color(name: &str) -> ratatui::style::Color {
    use ratatui::style::Color;
    match name.to_lowercase().as_str() {
        "red" => Color::Red,
        "green" => Color::Green,
        "blue" => Color::Blue,
        "yellow" => Color::Yellow,
        "cyan" => Color::Cyan,
        "magenta" => Color::Magenta,
        "white" => Color::White,
        "gray" | "grey" => Color::Gray,
        _ => Color::White,
    }
}
