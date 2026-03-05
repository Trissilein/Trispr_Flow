use chrono::{Datelike, TimeZone, Utc};
use serde::Serialize;
use std::collections::{BTreeMap, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};
use tracing::warn;

use crate::state::HistoryEntry;

// ---------------------------------------------------------------------------
// PartitionKey
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct PartitionKey {
    pub(crate) year: u16,
    pub(crate) month: u8,
}

impl PartitionKey {
    /// Derive the partition key from an epoch-millisecond timestamp.
    pub(crate) fn from_timestamp_ms(ts: u64) -> Self {
        let secs = (ts / 1000) as i64;
        let dt = Utc.timestamp_opt(secs, 0).single().unwrap_or_else(Utc::now);
        Self {
            year: dt.year() as u16,
            month: dt.month() as u8,
        }
    }

    /// The partition key for the current wall-clock month.
    pub(crate) fn current() -> Self {
        let now = Utc::now();
        Self {
            year: now.year() as u16,
            month: now.month() as u8,
        }
    }

    /// Filename for this partition, e.g. `"2026-03.json"`.
    pub(crate) fn filename(&self) -> String {
        format!("{:04}-{:02}.json", self.year, self.month)
    }

    /// Human-readable label, e.g. `"March 2026"`.
    pub(crate) fn display_label(&self) -> String {
        let month_name = match self.month {
            1 => "January",
            2 => "February",
            3 => "March",
            4 => "April",
            5 => "May",
            6 => "June",
            7 => "July",
            8 => "August",
            9 => "September",
            10 => "October",
            11 => "November",
            12 => "December",
            _ => "Unknown",
        };
        format!("{} {}", month_name, self.year)
    }

    /// Parse a key string like `"2026-03"` into a `PartitionKey`.
    pub(crate) fn parse(s: &str) -> Result<Self, String> {
        let parts: Vec<&str> = s.split('-').collect();
        if parts.len() != 2 {
            return Err(format!("Invalid partition key '{}': expected YYYY-MM", s));
        }
        let year: u16 = parts[0]
            .parse()
            .map_err(|_| format!("Invalid year in partition key '{}'", s))?;
        let month: u8 = parts[1]
            .parse()
            .map_err(|_| format!("Invalid month in partition key '{}'", s))?;
        if !(1..=12).contains(&month) {
            return Err(format!("Month out of range in partition key '{}'", s));
        }
        Ok(Self { year, month })
    }

    /// String representation without the `.json` extension, e.g. `"2026-03"`.
    pub(crate) fn as_key_string(&self) -> String {
        format!("{:04}-{:02}", self.year, self.month)
    }
}

// ---------------------------------------------------------------------------
// PartitionInfo  (serialized to the frontend)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub(crate) struct PartitionInfo {
    pub(crate) key: String,
    pub(crate) label: String,
    pub(crate) entry_count: usize,
    pub(crate) size_bytes: u64,
    pub(crate) is_active: bool,
}

// ---------------------------------------------------------------------------
// PartitionedHistory
// ---------------------------------------------------------------------------

pub(crate) struct PartitionedHistory {
    pub(crate) active: VecDeque<HistoryEntry>,
    pub(crate) active_key: PartitionKey,
    pub(crate) base_dir: PathBuf,
}

impl PartitionedHistory {
    /// Load an existing partitioned history or migrate from a legacy monolithic
    /// JSON file.
    ///
    /// Migration steps:
    ///   1. Ensure `base_dir` exists.
    ///   2. If a legacy file exists **and** base_dir contains no `.json` files
    ///      yet, split entries by month, write each partition atomically, and
    ///      rename the legacy file to `*.migrated`.
    ///   3. Load the current month partition into RAM.
    pub(crate) fn load_or_migrate(base_dir: PathBuf, legacy_path: Option<&Path>) -> Self {
        let _ = fs::create_dir_all(&base_dir);

        let has_json_files = fs::read_dir(&base_dir)
            .map(|entries| {
                entries.flatten().any(|e| {
                    e.path()
                        .extension()
                        .and_then(|ext| ext.to_str())
                        .map(|ext| ext == "json")
                        .unwrap_or(false)
                })
            })
            .unwrap_or(false);

        // Migrate legacy file if applicable
        if let Some(legacy) = legacy_path {
            if legacy.exists() && !has_json_files {
                if let Ok(raw) = fs::read_to_string(legacy) {
                    let entries: Vec<HistoryEntry> =
                        serde_json::from_str(&raw).unwrap_or_default();
                    if !entries.is_empty() {
                        // Group entries by month
                        let mut groups: BTreeMap<PartitionKey, Vec<HistoryEntry>> =
                            BTreeMap::new();
                        for entry in entries {
                            let key = PartitionKey::from_timestamp_ms(entry.timestamp_ms);
                            groups.entry(key).or_default().push(entry);
                        }
                        // Write each partition atomically
                        for (key, group) in &groups {
                            let path = base_dir.join(key.filename());
                            if let Err(e) = save_entries_to_path(&path, group) {
                                warn!(
                                    "Failed to write migrated partition {}: {}",
                                    key.filename(),
                                    e
                                );
                            }
                        }
                    }
                    // Rename legacy file to .migrated
                    let migrated = legacy.with_extension("migrated");
                    if let Err(e) = fs::rename(legacy, &migrated) {
                        warn!(
                            "Failed to rename legacy history file to .migrated: {}",
                            e
                        );
                    }
                }
            }
        }

        // Load the current month partition
        let active_key = PartitionKey::current();
        let active_path = base_dir.join(active_key.filename());
        let active = match fs::read_to_string(&active_path) {
            Ok(raw) => {
                let entries: Vec<HistoryEntry> =
                    serde_json::from_str(&raw).unwrap_or_default();
                VecDeque::from(entries)
            }
            Err(_) => VecDeque::new(),
        };

        Self {
            active,
            active_key,
            base_dir,
        }
    }

    /// Push a new entry.  If the calendar month has changed since the last
    /// active key, flush the current partition to disk and switch to the new
    /// month.
    pub(crate) fn push_entry(&mut self, entry: HistoryEntry) {
        let entry_key = PartitionKey::from_timestamp_ms(entry.timestamp_ms);
        if entry_key != self.active_key {
            // Flush the old month to disk before switching
            if let Err(e) = self.flush_to_disk() {
                warn!("Failed to flush partition before month switch: {}", e);
            }
            self.active_key = entry_key;
            // Load existing data for the new month (there might already be entries)
            let path = self.base_dir.join(self.active_key.filename());
            self.active = match fs::read_to_string(&path) {
                Ok(raw) => {
                    let entries: Vec<HistoryEntry> =
                        serde_json::from_str(&raw).unwrap_or_default();
                    VecDeque::from(entries)
                }
                Err(_) => VecDeque::new(),
            };
        }
        self.active.push_front(entry);
    }

    /// Persist the active partition to disk atomically (.tmp + rename).
    pub(crate) fn flush_to_disk(&self) -> Result<(), String> {
        let path = self.base_dir.join(self.active_key.filename());
        let entries: Vec<&HistoryEntry> = self.active.iter().collect();
        let raw = serde_json::to_string_pretty(&entries).map_err(|e| e.to_string())?;
        let tmp_path = path.with_extension("json.tmp");
        fs::write(&tmp_path, &raw).map_err(|e| e.to_string())?;
        fs::rename(&tmp_path, &path).map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Scan the base directory and return metadata for every partition file.
    pub(crate) fn list_partitions(&self) -> Vec<PartitionInfo> {
        let mut result = Vec::new();
        let entries = match fs::read_dir(&self.base_dir) {
            Ok(e) => e,
            Err(_) => return result,
        };

        for dir_entry in entries.flatten() {
            let path = dir_entry.path();
            let ext_match = path
                .extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| ext == "json")
                .unwrap_or(false);
            if !ext_match {
                continue;
            }
            let stem = match path.file_stem().and_then(|s| s.to_str()) {
                Some(s) => s.to_string(),
                None => continue,
            };
            let pk = match PartitionKey::parse(&stem) {
                Ok(k) => k,
                Err(_) => continue,
            };

            let is_active = pk == self.active_key;

            let (entry_count, size_bytes) = if is_active {
                // Use in-memory data for the active partition
                let size = path.metadata().map(|m| m.len()).unwrap_or(0);
                (self.active.len(), size)
            } else {
                let size = path.metadata().map(|m| m.len()).unwrap_or(0);
                // For archived partitions, estimate entry count from file or read
                let count = match fs::read_to_string(&path) {
                    Ok(raw) => {
                        let parsed: Vec<HistoryEntry> =
                            serde_json::from_str(&raw).unwrap_or_default();
                        parsed.len()
                    }
                    Err(_) => 0,
                };
                (count, size)
            };

            result.push(PartitionInfo {
                key: pk.as_key_string(),
                label: pk.display_label(),
                entry_count,
                size_bytes,
                is_active,
            });
        }

        // Sort by key descending so newest month is first
        result.sort_by(|a, b| b.key.cmp(&a.key));
        result
    }

    /// Load a specific partition on-demand.  If `key` matches the active
    /// month, return in-memory data; otherwise read from disk.
    pub(crate) fn load_partition(&self, key: &PartitionKey) -> Vec<HistoryEntry> {
        if *key == self.active_key {
            return self.active.iter().cloned().collect();
        }
        let path = self.base_dir.join(key.filename());
        match fs::read_to_string(&path) {
            Ok(raw) => serde_json::from_str(&raw).unwrap_or_default(),
            Err(_) => Vec::new(),
        }
    }

    /// Wrapper around `VecDeque::retain` for the active partition (needed by
    /// cluster-flush logic in `transcription.rs`).
    #[cfg(target_os = "windows")]
    pub(crate) fn retain_active<F: FnMut(&HistoryEntry) -> bool>(&mut self, f: F) {
        self.active.retain(f);
    }
}

// ---------------------------------------------------------------------------
// Standalone helpers
// ---------------------------------------------------------------------------

/// Write a slice of entries to the given path atomically (.tmp + rename).
pub(crate) fn save_entries_to_path(path: &Path, entries: &[HistoryEntry]) -> Result<(), String> {
    let raw = serde_json::to_string_pretty(entries).map_err(|e| e.to_string())?;
    let tmp_path = path.with_extension("json.tmp");
    fs::write(&tmp_path, &raw).map_err(|e| e.to_string())?;
    fs::rename(&tmp_path, path).map_err(|e| e.to_string())?;
    Ok(())
}
