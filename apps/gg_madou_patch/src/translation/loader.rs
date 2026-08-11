use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranslationEntry {
    pub id: u16,
    #[serde(default)]
    pub jp: String,
    #[serde(default)]
    pub en: String,
    #[serde(default)]
    pub ko: String,
    #[serde(default)]
    pub skip: bool,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub notes: String,
    #[serde(default)]
    pub layout: Option<LayoutConstraint>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayoutConstraint {
    pub max_line_tiles: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BankTranslation {
    pub bank: u8,
    #[serde(default)]
    pub table: String,
    #[serde(default)]
    pub base: String,
    pub entries: Vec<TranslationEntry>,
}

/// Load all translation files from a directory.
/// Expects files named bank_03_main.json, bank_05_secondary.json, etc.
/// Multiple files for the same bank/table are merged.
pub fn load_translations(dir: &Path) -> Result<Vec<BankTranslation>> {
    let mut translations = Vec::new();

    // Scan dir and all subdirectories (raw/, in_progress/, needs_review/, complete/)
    let mut all_files = Vec::new();
    collect_json_files(dir, &mut all_files);
    all_files.sort();

    let files = all_files;

    for file_path in &files {
        let content = std::fs::read_to_string(file_path)?;
        let bank_trans: BankTranslation = serde_json::from_str(&content)?;
        translations.push(bank_trans);
    }

    let mut merged: std::collections::BTreeMap<(u8, String), BankTranslation> =
        std::collections::BTreeMap::new();
    for bt in &translations {
        let table = if bt.table.is_empty() {
            "main".to_string()
        } else {
            bt.table.clone()
        };
        let entry = merged
            .entry((bt.bank, table.clone()))
            .or_insert_with(|| BankTranslation {
                bank: bt.bank,
                table,
                base: bt.base.clone(),
                entries: Vec::new(),
            });
        entry.entries.extend(bt.entries.clone());
    }

    Ok(merged.into_values().collect())
}

/// Recursively collect bank_*.json files from a directory and subdirectories.
fn collect_json_files(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
    let Ok(read_dir) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in read_dir.filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.is_dir() {
            collect_json_files(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "json") {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with("bank_") && !name.ends_with(".bak") {
                out.push(path);
            }
        }
    }
}
