use color_eyre::eyre::{Result, WrapErr};
use regex::Regex;
use rusqlite::Connection;
use std::path::Path;
use std::sync::LazyLock;
use yaml_rust2::{Yaml, YamlLoader};

use crate::config::Config;
use crate::db;
use crate::modules::{InsertOp, Module};

/// What kind of file the materializer recognizes. Used by the watcher
/// dispatch and by sync/rebuild to pick the right parser.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileKind {
    DailyNote,
    NutritionDb,
}

/// Classify a path. Returns `None` for hidden, swap, or unrelated files.
pub fn materialized_file_kind(path: &Path) -> Option<FileKind> {
    let filename = path.file_name().and_then(|f| f.to_str())?;
    if filename.starts_with('.') || filename.starts_with('~') || filename.ends_with('~') {
        return None;
    }
    if RE_NOTE_FILE.is_match(filename) {
        return Some(FileKind::DailyNote);
    }
    if filename == "nutrition-db.md" {
        return Some(FileKind::NutritionDb);
    }
    None
}

// --- YAML Preprocessing ---

static RE_MISSING_SPACE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(\s*\w[\w\s]*):(\S)").unwrap());

/// Preprocess raw YAML to handle common human-written quirks.
/// Ported from agents/notes_materializer/parser.py:47-92.
pub fn preprocess_yaml(raw: &str) -> String {
    let mut lines: Vec<String> = Vec::new();

    for line in raw.lines() {
        let trimmed = line.trim();

        // Skip empty lines, pure comments, and list items
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with('-') {
            lines.push(line.to_string());
            continue;
        }

        let mut processed = line.to_string();

        // Fix missing space after colon: `key:value` → `key: value`
        if let Some(caps) = RE_MISSING_SPACE.captures(&processed) {
            let full = caps.get(0).unwrap();
            let key_part = caps.get(1).unwrap().as_str();
            let val_start = caps.get(2).unwrap().as_str();
            let replacement = format!("{}: {}", key_part, val_start);
            processed =
                format!("{}{}", &processed[..full.start()], replacement) + &processed[full.end()..];
        }

        // Strip inline comments BEFORE colon quoting
        let (value_part, comment_suffix) = strip_inline_comment(&processed);
        processed = value_part;

        // Auto-quote values containing colons (e.g., sleep times)
        processed = auto_quote_colons(&processed);

        // Restore inline comment
        if !comment_suffix.is_empty() {
            processed = format!("{processed} {comment_suffix}");
        }

        lines.push(processed);
    }

    lines.join("\n")
}

/// Strip inline comment (` #...`) from a line, returning (line_without_comment, comment).
/// Respects quoted strings.
fn strip_inline_comment(line: &str) -> (String, String) {
    let bytes = line.as_bytes();
    let mut in_quote = false;
    let mut quote_char = b' ';

    for i in 0..bytes.len() {
        if in_quote {
            if bytes[i] == quote_char {
                in_quote = false;
            }
            continue;
        }
        if bytes[i] == b'"' || bytes[i] == b'\'' {
            in_quote = true;
            quote_char = bytes[i];
            continue;
        }
        if bytes[i] == b'#' && i > 0 && bytes[i - 1] == b' ' {
            return (line[..i - 1].trim_end().to_string(), line[i..].to_string());
        }
    }

    (line.to_string(), String::new())
}

/// Auto-quote values that contain colons and aren't already quoted/bracketed.
fn auto_quote_colons(line: &str) -> String {
    // Only process lines that look like `key: value`
    let Some(colon_pos) = line.find(':') else {
        return line.to_string();
    };

    let after_colon = &line[colon_pos + 1..];
    if after_colon.is_empty() {
        return line.to_string();
    }

    let value = after_colon.trim_start();
    if value.is_empty() {
        return line.to_string();
    }

    // Already quoted or bracketed?
    if value.starts_with('"')
        || value.starts_with('\'')
        || value.starts_with('[')
        || value.starts_with('{')
    {
        return line.to_string();
    }

    // Does the value contain another colon?
    if !value.contains(':') {
        return line.to_string();
    }

    // Check this is a top-level or simple nested key (not a mapping start)
    let key_part = &line[..colon_pos];
    format!("{key_part}: \"{value}\"")
}

// --- Core Normalization ---

/// Parse and normalize a single note file, inserting into the database.
///
/// Dates are derived from filenames (YYYY-MM-DD.md), not from `effective_today()`.
/// This is intentional: the materializer processes historical notes whose dates are
/// already encoded in the filename, whereas `effective_today()` only applies to
/// determining which file to write to during live logging.
pub fn materialize_file(
    conn: &Connection,
    file_path: &Path,
    config: &Config,
    modules: &[Box<dyn Module>],
) -> Result<()> {
    let content = std::fs::read_to_string(file_path)
        .wrap_err_with(|| format!("Failed to read {}", file_path.display()))?;

    let date = file_path
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or_else(|| color_eyre::eyre::eyre!("Invalid filename: {}", file_path.display()))?;

    // Validate date format
    if !is_valid_date(date) {
        color_eyre::eyre::bail!("Invalid date format in filename: {}", date);
    }

    // Extract YAML frontmatter
    let yaml_str = extract_frontmatter(&content)?;
    let preprocessed = preprocess_yaml(&yaml_str);

    let docs = YamlLoader::load_from_str(&preprocessed)
        .wrap_err_with(|| format!("Failed to parse YAML in {}", file_path.display()))?;

    let yaml = docs.first().unwrap_or(&Yaml::Null);

    // Get file mtime
    let mtime = std::fs::metadata(file_path)?
        .modified()?
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64();

    // Core vitals extraction
    let sleep_data = yaml_str_field(yaml, "sleep")
        .as_deref()
        .and_then(crate::time::parse_sleep_range)
        .map(|(start, end)| {
            let hours = crate::time::sleep_hours(start, end);
            (
                crate::time::format_time(start, crate::config::TimeFormat::TwentyFourHour),
                crate::time::format_time(end, crate::config::TimeFormat::TwentyFourHour),
                hours,
            )
        });
    let weight = yaml_f64_field(yaml, "weight");
    let mood = yaml_i32_field(yaml, "mood");
    let energy = yaml_i32_field(yaml, "energy");
    let sleep_quality = yaml_i32_field(yaml, "sleep_quality");

    // Extract notes section from markdown body
    let notes = extract_notes_section(&content);

    // DELETE-then-INSERT in a transaction (auto-rollback on drop)
    let tx = conn.unchecked_transaction()?;

    db::delete_date(&tx, date)?;
    db::insert_day(
        &tx,
        date,
        sleep_data.as_ref().map(|(s, _, _)| s.as_str()),
        sleep_data.as_ref().map(|(_, e, _)| e.as_str()),
        sleep_data.as_ref().map(|(_, _, h)| *h),
        sleep_quality,
        mood,
        energy,
        weight,
        notes.as_deref(),
        mtime,
    )?;

    // Generic metrics from config
    for metric_name in config.metrics.keys() {
        if let Some(value) = yaml_f64_field(yaml, metric_name) {
            tx.execute(
                "INSERT OR REPLACE INTO metrics (date, name, value) VALUES (?1, ?2, ?3)",
                rusqlite::params![date, metric_name, value],
            )?;
        }
    }

    // Module normalization
    let mut all_ops: Vec<InsertOp> = Vec::new();
    for module in modules {
        match module.normalize(date, yaml, config) {
            Ok(ops) => all_ops.extend(ops),
            Err(e) => {
                eprintln!(
                    "Warning: module '{}' failed to normalize {}: {e}",
                    module.id(),
                    date
                );
            }
        }
    }

    db::execute_insert_ops(&tx, date, &all_ops)?;
    tx.commit()?;

    Ok(())
}

/// One-shot sync: parse notes newer than last sync.
pub fn sync_all(
    conn: &Connection,
    notes_dir: &Path,
    config: &Config,
    modules: &[Box<dyn Module>],
) -> Result<(u32, u32)> {
    let last_sync = db::get_last_sync(conn)?.unwrap_or(0.0);
    let threshold = last_sync - 1.0; // 1s buffer for filesystem mtime resolution

    let mut synced = 0u32;
    let mut errors = 0u32;

    for entry in note_files(notes_dir)? {
        let mtime = std::fs::metadata(&entry)?
            .modified()?
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs_f64();

        if mtime < threshold {
            continue;
        }

        match materialize_file(conn, &entry, config, modules) {
            Ok(()) => synced += 1,
            Err(e) => {
                eprintln!("Error parsing {}: {e}", entry.display());
                errors += 1;
            }
        }
    }

    let nutrition_path = notes_dir.join("nutrition-db.md");
    if nutrition_path.exists() {
        let mtime = std::fs::metadata(&nutrition_path)
            .ok()
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs_f64())
            .unwrap_or(0.0);
        if mtime >= threshold {
            match crate::materializer::nutrition::materialize_nutrition_db(
                conn,
                &nutrition_path,
                config,
            ) {
                Ok(_n) => synced += 1,
                Err(e) => {
                    eprintln!("Error parsing nutrition-db.md: {e}");
                    errors += 1;
                }
            }
        }
    }

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64();
    db::set_last_sync(conn, now)?;

    Ok((synced, errors))
}

/// Full rebuild: parse ALL notes regardless of mtime.
pub fn rebuild_all(
    conn: &Connection,
    notes_dir: &Path,
    config: &Config,
    modules: &[Box<dyn Module>],
) -> Result<(u32, u32)> {
    let mut synced = 0u32;
    let mut errors = 0u32;

    for entry in note_files(notes_dir)? {
        match materialize_file(conn, &entry, config, modules) {
            Ok(()) => synced += 1,
            Err(e) => {
                eprintln!("Error parsing {}: {e}", entry.display());
                errors += 1;
            }
        }
    }

    let nutrition_path = notes_dir.join("nutrition-db.md");
    if nutrition_path.exists() {
        match crate::materializer::nutrition::materialize_nutrition_db(
            conn,
            &nutrition_path,
            config,
        ) {
            Ok(_n) => synced += 1,
            Err(e) => {
                eprintln!("Error parsing nutrition-db.md: {e}");
                errors += 1;
            }
        }
    }

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64();
    db::set_last_sync(conn, now)?;

    Ok((synced, errors))
}

// --- File Watcher ---

/// Start the file watcher on a separate thread.
/// Returns a handle that can be used to stop the watcher.
pub fn start_watcher(
    notes_dir: std::path::PathBuf,
    db_path: std::path::PathBuf,
    config: Config,
    modules: std::sync::Arc<Vec<Box<dyn Module>>>,
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
) -> Result<std::thread::JoinHandle<()>> {
    use notify::{
        Config as NotifyConfig, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher,
    };
    use std::sync::mpsc;
    use std::time::{Duration, Instant};

    let (tx, rx) = mpsc::channel();

    let mut watcher = RecommendedWatcher::new(
        move |res: notify::Result<Event>| {
            if let Ok(event) = res {
                let _ = tx.send(event);
            }
        },
        NotifyConfig::default(),
    )?;

    watcher.watch(&notes_dir, RecursiveMode::NonRecursive)?;

    let handle = std::thread::spawn(move || {
        let _watcher = watcher; // Keep alive
        let mut last_process = Instant::now();
        let debounce = Duration::from_millis(500);
        let mut pending_files: std::collections::HashSet<std::path::PathBuf> =
            std::collections::HashSet::new();

        let mut current_config = config;

        // Open DB connection once; reconnect on error
        let mut conn_opt: Option<rusqlite::Connection> = match db::open_rw(&db_path) {
            Ok(c) => Some(c),
            Err(e) => {
                eprintln!("Watcher: failed to open database: {e}");
                None
            }
        };

        loop {
            if stop.load(std::sync::atomic::Ordering::Relaxed) {
                break;
            }

            match rx.recv_timeout(Duration::from_millis(250)) {
                Ok(event) => match event.kind {
                    EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_) => {
                        for path in &event.paths {
                            if path
                                .file_name()
                                .map(|f| f == "config.toml")
                                .unwrap_or(false)
                            {
                                current_config = Config::load_or_keep(&current_config);
                                eprintln!("Config reloaded");
                                continue;
                            }

                            if materialized_file_kind(path).is_some() {
                                pending_files.insert(path.clone());
                            }
                        }
                    }
                    _ => {}
                },
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }

            // Process pending files after debounce
            if !pending_files.is_empty() && last_process.elapsed() >= debounce {
                // Reconnect if connection was lost
                if conn_opt.is_none() {
                    match db::open_rw(&db_path) {
                        Ok(c) => {
                            eprintln!("Watcher: reconnected to database");
                            conn_opt = Some(c);
                        }
                        Err(e) => {
                            eprintln!("Watcher: reconnect failed: {e}");
                            last_process = Instant::now();
                            continue;
                        }
                    }
                }

                let conn = conn_opt.as_ref().unwrap();
                let mut conn_failed = false;
                for path in pending_files.drain() {
                    if !path.exists() {
                        match materialized_file_kind(&path) {
                            Some(FileKind::DailyNote) => {
                                if let Some(date) = path.file_stem().and_then(|s| s.to_str()) {
                                    if is_valid_date(date) {
                                        let _ = db::delete_date(conn, date);
                                    }
                                }
                            }
                            Some(FileKind::NutritionDb) => {
                                // Spec: deletion is a no-op; foods table retained.
                            }
                            None => {}
                        }
                        continue;
                    }
                    let result = match materialized_file_kind(&path) {
                        Some(FileKind::DailyNote) => {
                            materialize_file(conn, &path, &current_config, &modules)
                        }
                        Some(FileKind::NutritionDb) => {
                            crate::materializer::nutrition::materialize_nutrition_db(
                                conn,
                                &path,
                                &current_config,
                            )
                            .map(|_| ())
                        }
                        None => continue,
                    };
                    if let Err(e) = result {
                        let err_str = e.to_string();
                        eprintln!("Warning: failed to parse {}: {e}", path.display());
                        if err_str.contains("disk I/O error")
                            || err_str.contains("database is locked")
                            || err_str.contains("unable to open")
                        {
                            conn_failed = true;
                            break;
                        }
                        let _ = conn.execute(
                            "INSERT OR REPLACE INTO sync_meta (key, value) VALUES ('last_error', ?1)",
                            [format!("{}: {e}", path.display())],
                        );
                    }
                }
                if conn_failed {
                    eprintln!("Watcher: database connection lost, will reconnect");
                    conn_opt = None;
                }
                last_process = Instant::now();
            }
        }
    });

    Ok(handle)
}

// --- Helper Functions ---

/// List all YYYY-MM-DD.md files in the notes directory.
fn note_files(dir: &Path) -> Result<Vec<std::path::PathBuf>> {
    let mut files = Vec::new();
    if !dir.exists() {
        return Ok(files);
    }
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if is_note_file(&path) {
            files.push(path);
        }
    }
    files.sort();
    Ok(files)
}

static RE_NOTE_FILE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\d{4}-\d{2}-\d{2}\.md$").unwrap());

/// Check if a path is a valid note file (YYYY-MM-DD.md, not hidden/swap).
fn is_note_file(path: &Path) -> bool {
    let Some(filename) = path.file_name().and_then(|f| f.to_str()) else {
        return false;
    };

    // Skip hidden/swap files
    if filename.starts_with('.') || filename.starts_with('~') || filename.ends_with('~') {
        return false;
    }

    RE_NOTE_FILE.is_match(filename)
}

fn is_valid_date(s: &str) -> bool {
    RE_NOTE_FILE.is_match(&format!("{s}.md"))
}

/// Extract YAML frontmatter from file content (between first pair of `---`).
fn extract_frontmatter(content: &str) -> Result<String> {
    let content = content
        .strip_prefix('\u{feff}') // Strip BOM
        .unwrap_or(content)
        .replace("\r\n", "\n"); // Normalize line endings

    let lines: Vec<&str> = content.lines().collect();

    if lines.is_empty() || lines[0].trim() != "---" {
        color_eyre::eyre::bail!("No YAML frontmatter found (file should start with ---)");
    }

    for (i, line) in lines.iter().enumerate().skip(1) {
        if line.trim() == "---" {
            return Ok(lines[1..i].join("\n"));
        }
    }

    color_eyre::eyre::bail!("No closing --- found for YAML frontmatter");
}

/// Extract the ## Notes section from the markdown body.
fn extract_notes_section(content: &str) -> Option<String> {
    static RE_NOTES: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?m)^## Notes\s*\n([\s\S]*?)(?:\n## |\z)").unwrap());

    RE_NOTES.captures(content).map(|caps| {
        caps.get(1)
            .map(|m| m.as_str().trim().to_string())
            .unwrap_or_default()
    })
}

/// Get a string field from YAML, handling various types.
pub fn yaml_str_field(yaml: &Yaml, key: &str) -> Option<String> {
    match &yaml[key] {
        Yaml::String(s) => Some(s.clone()),
        Yaml::Integer(i) => Some(i.to_string()),
        Yaml::Real(r) => Some(r.clone()),
        Yaml::Boolean(b) => Some(b.to_string()),
        _ => None,
    }
}

/// Get an f64 field from YAML.
pub fn yaml_f64_field(yaml: &Yaml, key: &str) -> Option<f64> {
    match &yaml[key] {
        Yaml::Real(r) => r.parse().ok(),
        Yaml::Integer(i) => Some(*i as f64),
        Yaml::String(s) => {
            // Handle strings that are numbers (e.g., after preprocessing)
            s.trim().parse().ok()
        }
        _ => None,
    }
}

/// Get an i32 field from YAML.
pub fn yaml_i32_field(yaml: &Yaml, key: &str) -> Option<i32> {
    match &yaml[key] {
        Yaml::Integer(i) => Some(*i as i32),
        Yaml::Real(r) => r.parse::<f64>().ok().map(|f| f as i32),
        Yaml::String(s) => s.trim().parse().ok(),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- Preprocessing tests ---

    #[test]
    fn test_preprocess_missing_space() {
        let result = preprocess_yaml("mood:4\nenergy:3");
        assert!(result.contains("mood: 4"));
        assert!(result.contains("energy: 3"));
    }

    #[test]
    fn test_preprocess_colon_quoting() {
        let result = preprocess_yaml("sleep: 10:30pm-6:15am");
        assert!(result.contains("\"10:30pm-6:15am\""));
    }

    #[test]
    fn test_preprocess_already_quoted() {
        let input = "sleep: \"10:30pm-6:15am\"";
        let result = preprocess_yaml(input);
        assert_eq!(result, input);
    }

    #[test]
    fn test_preprocess_inline_comment() {
        let result = preprocess_yaml("weight: 173.4 # morning");
        assert!(result.contains("weight: 173.4"));
        assert!(result.contains("# morning"));
    }

    #[test]
    fn test_preprocess_list_items_untouched() {
        let input = "  - V5\n  - V4 x2";
        let result = preprocess_yaml(input);
        assert_eq!(result, input);
    }

    #[test]
    fn test_preprocess_bom_and_crlf() {
        // BOM and CRLF are handled by extract_frontmatter, not preprocess_yaml
        // But preprocess_yaml should handle normal content
        let result = preprocess_yaml("key: value");
        assert_eq!(result, "key: value");
    }

    #[test]
    fn test_preprocess_nested_key_no_false_quote() {
        let result = preprocess_yaml("lifts:\n  squat: 185x5");
        assert!(result.contains("lifts:"));
        assert!(result.contains("  squat: 185x5"));
        // squat value has no colon, should NOT be quoted
        assert!(!result.contains("\"185x5\""));
    }

    // --- Frontmatter extraction tests ---

    #[test]
    fn test_extract_frontmatter() {
        let content = "---\ndate: 2026-03-28\nweight: 173.4\n---\n\n## Notes\n";
        let yaml = extract_frontmatter(content).unwrap();
        assert!(yaml.contains("date: 2026-03-28"));
        assert!(yaml.contains("weight: 173.4"));
    }

    #[test]
    fn test_extract_frontmatter_no_opening() {
        let content = "Just markdown\n---\n";
        assert!(extract_frontmatter(content).is_err());
    }

    #[test]
    fn test_extract_frontmatter_with_bom() {
        let content = "\u{feff}---\ndate: 2026-03-28\n---\n";
        let yaml = extract_frontmatter(content).unwrap();
        assert!(yaml.contains("date: 2026-03-28"));
    }

    // --- Notes extraction tests ---

    #[test]
    fn test_extract_notes_section() {
        let content =
            "---\ndate: 2026-03-28\n---\n\n## Notes\n\nGood session today.\n\n## Other\n\nStuff\n";
        let notes = extract_notes_section(content).unwrap();
        assert_eq!(notes, "Good session today.");
    }

    #[test]
    fn test_extract_notes_section_end_of_file() {
        let content = "---\ndate: 2026-03-28\n---\n\n## Notes\n\nGood session.\n";
        let notes = extract_notes_section(content).unwrap();
        assert_eq!(notes, "Good session.");
    }

    // --- YAML field helpers ---

    #[test]
    fn test_yaml_f64_field() {
        let docs = YamlLoader::load_from_str("weight: 173.4\nmood: 4").unwrap();
        let yaml = &docs[0];
        assert_eq!(yaml_f64_field(yaml, "weight"), Some(173.4));
        assert_eq!(yaml_f64_field(yaml, "mood"), Some(4.0));
        assert_eq!(yaml_f64_field(yaml, "missing"), None);
    }

    #[test]
    fn test_yaml_i32_field() {
        let docs = YamlLoader::load_from_str("mood: 4\nrpe: 7.5").unwrap();
        let yaml = &docs[0];
        assert_eq!(yaml_i32_field(yaml, "mood"), Some(4));
        assert_eq!(yaml_i32_field(yaml, "rpe"), Some(7));
    }

    // --- Integration test: demo data round-trip ---

    #[test]
    fn test_materialize_demo_note() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "PRAGMA foreign_keys = ON;
             PRAGMA journal_mode = WAL;
             CREATE TABLE IF NOT EXISTS days (
                 date TEXT PRIMARY KEY, sleep_start TEXT, sleep_end TEXT,
                 sleep_hours REAL, sleep_quality INTEGER, mood INTEGER,
                 energy INTEGER, weight REAL, notes TEXT, file_mtime REAL, parsed_at TEXT
             );
             CREATE TABLE IF NOT EXISTS metrics (
                 date TEXT NOT NULL REFERENCES days(date) ON DELETE CASCADE,
                 name TEXT NOT NULL, value REAL NOT NULL, PRIMARY KEY (date, name)
             );
             CREATE TABLE IF NOT EXISTS sync_meta (key TEXT PRIMARY KEY, value TEXT);",
        )
        .unwrap();

        let dir = tempfile::TempDir::new().unwrap();
        let note_path = dir.path().join("2026-03-28.md");
        std::fs::write(
            &note_path,
            "---\ndate: 2026-03-28\nsleep: \"10:30pm-6:15am\"\nweight: 173.4\nmood: 4\nenergy: 3\nresting_hr: 52\n---\n\n## Notes\n\nGreat day.\n",
        )
        .unwrap();

        let config_str = "notes_dir = \"/tmp\"\n[modules]\n[exercises]\n[metrics]\nresting_hr = { display = \"Resting HR\", color = \"red\", unit = \"bpm\" }\n";
        let config: Config = toml::from_str(config_str).unwrap();
        let modules: Vec<Box<dyn Module>> = vec![];

        materialize_file(&conn, &note_path, &config, &modules).unwrap();

        // Verify core data
        let today = db::load_today(&conn, "2026-03-28").unwrap().unwrap();
        assert_eq!(today["weight"], 173.4);
        assert_eq!(today["mood"], 4);
        assert_eq!(today["energy"], 3);
        assert_eq!(today["sleep_start"], "22:30");
        assert_eq!(today["sleep_end"], "06:15");
        assert!((today["sleep_hours"].as_f64().unwrap() - 7.75).abs() < 0.01);
        assert_eq!(today["notes"], "Great day.");

        // Verify metric
        let metrics = db::load_metrics(&conn, "2026-03-28").unwrap();
        assert_eq!(metrics.len(), 1);
        assert_eq!(metrics[0].0, "resting_hr");
        assert_eq!(metrics[0].1, 52.0);
    }

    #[test]
    fn materialize_normalizes_12h_sleep_to_24h() {
        let dir = tempfile::TempDir::new().unwrap();
        let notes_dir = dir.path();
        let db_path = notes_dir.join(".daylog.db");
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        conn.execute_batch(
            "CREATE TABLE days (
                 date TEXT PRIMARY KEY, sleep_start TEXT, sleep_end TEXT,
                 sleep_hours REAL, sleep_quality INTEGER, mood INTEGER,
                 energy INTEGER, weight REAL, notes TEXT,
                 file_mtime REAL, parsed_at TEXT);
             CREATE TABLE metrics (
                 date TEXT, name TEXT, value REAL,
                 PRIMARY KEY (date, name));",
        )
        .unwrap();

        let file = notes_dir.join("2026-04-26.md");
        std::fs::write(
            &file,
            "---\ndate: 2026-04-26\nsleep: \"10:30pm-6:15am\"\n---\n",
        )
        .unwrap();

        let cfg: Config = toml::from_str(&format!(
            "notes_dir = '{}'\n",
            notes_dir.display().to_string().replace('\\', "/")
        ))
        .unwrap();
        materialize_file(&conn, &file, &cfg, &[]).unwrap();

        let (start, end, hours): (String, String, f64) = conn
            .query_row(
                "SELECT sleep_start, sleep_end, sleep_hours FROM days WHERE date = ?1",
                ["2026-04-26"],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert_eq!(start, "22:30");
        assert_eq!(end, "06:15");
        assert!((hours - 7.75).abs() < 0.01);
    }

    #[test]
    fn materialize_normalizes_24h_sleep_to_24h() {
        let dir = tempfile::TempDir::new().unwrap();
        let notes_dir = dir.path();
        let db_path = notes_dir.join(".daylog.db");
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        conn.execute_batch(
            "CREATE TABLE days (
                 date TEXT PRIMARY KEY, sleep_start TEXT, sleep_end TEXT,
                 sleep_hours REAL, sleep_quality INTEGER, mood INTEGER,
                 energy INTEGER, weight REAL, notes TEXT,
                 file_mtime REAL, parsed_at TEXT);
             CREATE TABLE metrics (
                 date TEXT, name TEXT, value REAL,
                 PRIMARY KEY (date, name));",
        )
        .unwrap();

        let file = notes_dir.join("2026-04-26.md");
        std::fs::write(
            &file,
            "---\ndate: 2026-04-26\nsleep: \"22:30-06:15\"\n---\n",
        )
        .unwrap();

        let cfg: Config = toml::from_str(&format!(
            "notes_dir = '{}'\n",
            notes_dir.display().to_string().replace('\\', "/")
        ))
        .unwrap();
        materialize_file(&conn, &file, &cfg, &[]).unwrap();

        let (start, end): (String, String) = conn
            .query_row(
                "SELECT sleep_start, sleep_end FROM days WHERE date = ?1",
                ["2026-04-26"],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(start, "22:30");
        assert_eq!(end, "06:15");
    }

    use std::path::PathBuf;

    fn p(s: &str) -> PathBuf {
        PathBuf::from(s)
    }

    #[test]
    fn file_kind_classifies_daily_note() {
        assert_eq!(
            materialized_file_kind(&p("2026-04-29.md")),
            Some(FileKind::DailyNote)
        );
        assert_eq!(
            materialized_file_kind(&p("/tmp/notes/2026-04-29.md")),
            Some(FileKind::DailyNote)
        );
    }

    #[test]
    fn file_kind_classifies_nutrition_db() {
        assert_eq!(
            materialized_file_kind(&p("nutrition-db.md")),
            Some(FileKind::NutritionDb)
        );
        assert_eq!(
            materialized_file_kind(&p("/tmp/notes/nutrition-db.md")),
            Some(FileKind::NutritionDb)
        );
    }

    #[test]
    fn file_kind_rejects_hidden_and_swap() {
        assert_eq!(materialized_file_kind(&p(".2026-04-29.md")), None);
        assert_eq!(materialized_file_kind(&p("~nutrition-db.md")), None);
        assert_eq!(materialized_file_kind(&p("nutrition-db.md~")), None);
    }

    #[test]
    fn file_kind_rejects_unrelated() {
        assert_eq!(materialized_file_kind(&p("README.md")), None);
        assert_eq!(materialized_file_kind(&p("notes.txt")), None);
        assert_eq!(materialized_file_kind(&p("food.md")), None);
    }

    #[test]
    fn watcher_dispatch_recognizes_both_kinds() {
        let daily = std::path::Path::new("/notes/2026-04-29.md");
        let nutrition = std::path::Path::new("/notes/nutrition-db.md");
        let other = std::path::Path::new("/notes/scratch.md");
        assert_eq!(materialized_file_kind(daily), Some(FileKind::DailyNote));
        assert_eq!(
            materialized_file_kind(nutrition),
            Some(FileKind::NutritionDb)
        );
        assert!(materialized_file_kind(other).is_none());
    }
}
