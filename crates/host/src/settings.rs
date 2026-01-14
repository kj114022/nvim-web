//! Settings persistence using `SQLite`
//!
//! Stores nvim-web settings in ~/.config/nvim-web/settings.db

use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::{Context, Result};
use rusqlite::{params, Connection};

/// Settings storage backed by `SQLite`
pub struct SettingsStore {
    conn: Connection,
}

impl SettingsStore {
    /// Create or open settings database
    ///
    /// Location: ~/.config/nvim-web/settings.db
    pub fn new() -> Result<Self> {
        let db_path = Self::db_path()?;

        // Ensure parent directory exists
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent).context("Failed to create config directory")?;
        }

        let conn = Connection::open(&db_path).context("Failed to open settings database")?;

        // Initialize schema
        conn.execute(
            "CREATE TABLE IF NOT EXISTS settings (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL,
                updated_at INTEGER DEFAULT (strftime('%s', 'now'))
            )",
            [],
        )?;

        Ok(Self { conn })
    }

    /// Get database path
    fn db_path() -> Result<PathBuf> {
        let config_dir = dirs::config_dir().context("Could not determine config directory")?;
        Ok(config_dir.join("nvim-web").join("settings.db"))
    }

    /// Get a single setting
    pub fn get(&self, key: &str) -> Option<String> {
        self.conn
            .query_row("SELECT value FROM settings WHERE key = ?", [key], |row| {
                row.get(0)
            })
            .ok()
    }

    /// Set a single setting
    pub fn set(&self, key: &str, value: &str) -> Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO settings (key, value, updated_at)
             VALUES (?, ?, strftime('%s', 'now'))",
            params![key, value],
        )?;
        Ok(())
    }

    /// Delete a setting
    pub fn delete(&self, key: &str) -> Result<()> {
        self.conn
            .execute("DELETE FROM settings WHERE key = ?", [key])?;
        Ok(())
    }

    /// Get all settings as a map
    pub fn get_all(&self) -> HashMap<String, String> {
        let mut map = HashMap::new();

        if let Ok(mut stmt) = self.conn.prepare("SELECT key, value FROM settings") {
            if let Ok(rows) = stmt.query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            }) {
                for row in rows.flatten() {
                    map.insert(row.0, row.1);
                }
            }
        }

        map
    }

    /// Set multiple settings at once
    pub fn set_all(&self, settings: &HashMap<String, String>) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;

        for (key, value) in settings {
            tx.execute(
                "INSERT OR REPLACE INTO settings (key, value, updated_at)
                 VALUES (?, ?, strftime('%s', 'now'))",
                params![key, value],
            )?;
        }

        tx.commit()?;
        Ok(())
    }
}

/// Default settings
pub fn defaults() -> HashMap<String, String> {
    let mut map = HashMap::new();
    map.insert("font_size".to_string(), "14".to_string());
    map.insert("font_family".to_string(), "monospace".to_string());
    map.insert("theme".to_string(), "dark".to_string());
    map.insert("cursor_blink".to_string(), "true".to_string());
    map
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn test_settings_crud() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let conn = Connection::open(&db_path).unwrap();

        conn.execute(
            "CREATE TABLE settings (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL,
                updated_at INTEGER
            )",
            [],
        )
        .unwrap();

        let store = SettingsStore { conn };

        // Set
        store.set("font_size", "16").unwrap();
        assert_eq!(store.get("font_size"), Some("16".to_string()));

        // Update
        store.set("font_size", "18").unwrap();
        assert_eq!(store.get("font_size"), Some("18".to_string()));

        // Delete
        store.delete("font_size").unwrap();
        assert_eq!(store.get("font_size"), None);
    }

    #[test]
    fn test_get_all() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let conn = Connection::open(&db_path).unwrap();

        conn.execute(
            "CREATE TABLE settings (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL,
                updated_at INTEGER
            )",
            [],
        )
        .unwrap();

        let store = SettingsStore { conn };

        store.set("a", "1").unwrap();
        store.set("b", "2").unwrap();

        let all = store.get_all();
        assert_eq!(all.get("a"), Some(&"1".to_string()));
        assert_eq!(all.get("b"), Some(&"2".to_string()));
    }
}
