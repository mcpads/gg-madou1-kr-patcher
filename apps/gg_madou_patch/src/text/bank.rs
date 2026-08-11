use anyhow::Result;
use std::cmp::Reverse;
use std::collections::{HashMap, HashSet};

/// A pointer table within a bank.
///
/// Each bank can contain multiple pointer tables at different base addresses.
/// The game code selects the table by setting HL to the base_logical address,
/// then uses the common lookup routine at $22CA/$232B.
pub struct SubTable {
    /// Human-readable name (e.g., "main", "secondary", "third")
    pub name: &'static str,
    /// Logical base address when mapped to slot 2 ($8000-$BFFF).
    /// Pointer values are offsets relative to this base.
    pub base_logical: u16,
    /// Offset of the pointer table within the 16KB bank.
    pub ptr_bank_offset: u16,
    /// Number of pointer entries (always 256 for this game).
    pub ptr_count: u16,
}

/// Multi-table bank configuration.
///
/// Describes the complete layout of a bank including all pointer tables.
/// The text data is interleaved between and after the tables, with heavy
/// suffix sharing (multiple entries pointing into the same text stream).
pub struct MultiBankConfig {
    pub bank_number: u8,
    pub physical_base: usize,
    pub tables: &'static [SubTable],
    pub reserved_ranges: &'static [(usize, usize)],
}

/// Bank 3: 3 pointer tables
/// - $8000 (main): battle/status messages, Z80 callers at $22BC/$22B8/$22B1
/// - $8F00 (secondary): monster battle text, Z80 caller at $2290
/// - $9D40 (third): menu/UI/credits, Z80 caller at $2280
pub static BANK3_CONFIG: MultiBankConfig = MultiBankConfig {
    bank_number: 3,
    physical_base: 0x0C000,
    reserved_ranges: &[],
    tables: &[
        SubTable {
            name: "main",
            base_logical: 0x8000,
            ptr_bank_offset: 0x0000,
            ptr_count: 256,
        },
        SubTable {
            name: "secondary",
            base_logical: 0x8F00,
            ptr_bank_offset: 0x0F00,
            ptr_count: 256,
        },
        SubTable {
            name: "third",
            base_logical: 0x9D40,
            ptr_bank_offset: 0x1D40,
            ptr_count: 256,
        },
    ],
};

/// Bank 5: 2 pointer tables
/// - $8000 (main): story/dialogue, Z80 callers at $22C1/$22C6
/// - $A000 (secondary): events/lake/grimoire, Z80 caller at $7649
pub static BANK5_CONFIG: MultiBankConfig = MultiBankConfig {
    bank_number: 5,
    physical_base: 0x14000,
    reserved_ranges: &[(0x3000, 0x4000)],
    tables: &[
        SubTable {
            name: "main",
            base_logical: 0x8000,
            ptr_bank_offset: 0x0000,
            ptr_count: 256,
        },
        SubTable {
            name: "secondary",
            base_logical: 0xA000,
            ptr_bank_offset: 0x2000,
            ptr_count: 256,
        },
    ],
};

/// Static config lookup by text bank number. Single source of truth shared by
/// `commands::build`, `commands::check`, and `text::relocation` — keeping
/// this in one place is what lets `text::relocation::partition_translations`
/// serve both commands without their routing drifting apart.
pub fn config_for_bank(bank: u8) -> Option<&'static MultiBankConfig> {
    match bank {
        3 => Some(&BANK3_CONFIG),
        5 => Some(&BANK5_CONFIG),
        _ => None,
    }
}

pub struct EncodedEntry {
    pub id: u16,
    pub data: Vec<u8>, // encoded text bytes (with terminator)
    pub skip: bool,    // if true, keep original EN pointer and text
}

/// Translation data for a specific table within a bank.
pub struct TableTranslation {
    /// Index into MultiBankConfig::tables
    pub table_idx: usize,
    pub entries: Vec<EncodedEntry>,
}

/// Build a bank with multi-table awareness.
///
/// This preserves the EN ROM's text layout (including suffix sharing) and only
/// overwrites bytes that are exclusively referenced by translated entries.
///
/// Algorithm:
/// 1. Copy the entire EN bank (preserves all text, pointer tables, game data)
/// 2. Compute "protected" bytes (referenced by untranslated entries in ANY table)
/// 3. Build writable regions from unprotected bytes
/// 4. Pack translated entries' KO text into writable regions
/// 5. Update pointer tables for translated entries only
pub fn build_bank_with_tables(
    config: &MultiBankConfig,
    translations: &[TableTranslation],
    en_rom: &[u8],
) -> Result<Vec<u8>> {
    let bank_size = 0x4000usize;
    let mut bank = vec![0u8; bank_size];

    // Step 1: Copy entire EN bank
    let src = config.physical_base;
    bank.copy_from_slice(&en_rom[src..src + bank_size]);

    // Build a map of which entries are being translated
    // Key: (table_idx, entry_id), Value: true if this entry has new text
    let mut translated_set: HashSet<(usize, u16)> = HashSet::new();
    let mut ordered_translations: Vec<&TableTranslation> = translations.iter().collect();
    ordered_translations
        .sort_by_key(|tt| Reverse((config.tables[tt.table_idx].base_logical - 0x8000) as usize));

    for tt in ordered_translations {
        for entry in &tt.entries {
            if !entry.skip && !entry.data.is_empty() {
                translated_set.insert((tt.table_idx, entry.id));
            }
        }
    }

    // Step 2: Compute protected bytes
    let protected = compute_protected_bytes(config, en_rom, &translated_set);

    // Step 3: Build writable regions (fail-closed whitelist)
    let translated_bytes = compute_translated_text_bytes(config, en_rom, &translated_set);
    let writable = compute_writable_regions(&translated_bytes, &protected);
    let total_writable: usize = writable.iter().map(|(s, e)| e - s).sum();

    // Step 4: Allocate and write translated text with suffix-sharing optimization
    //
    // Suffix sharing: if entry B's encoded data is an exact tail portion of
    // entry A's data, B can share A's allocation (pointer = A_offset + delta).
    // This is critical for PAGE-chained entries (item lists, damage chains).
    // Precompute membership sets before `writable` is moved into the allocator.
    let writable_set: std::collections::HashSet<usize> =
        writable.iter().flat_map(|&(s, e)| s..e).collect();
    let mut ptr_set: std::collections::HashSet<usize> = std::collections::HashSet::new();
    for table in config.tables {
        let s = table.ptr_bank_offset as usize;
        for off in s..s + table.ptr_count as usize * 2 {
            ptr_set.insert(off);
        }
    }

    let mut alloc = RegionAllocator::new(writable);
    let mut total_written = 0usize;
    let mut allocated: Vec<(usize, usize)> = Vec::new(); // (bank_offset, data_len)

    let mut ordered_translations: Vec<&TableTranslation> = translations.iter().collect();
    ordered_translations
        .sort_by_key(|tt| Reverse((config.tables[tt.table_idx].base_logical - 0x8000) as usize));

    for tt in ordered_translations {
        let table = &config.tables[tt.table_idx];
        let min_offset = (table.base_logical - 0x8000) as usize;

        // Collect non-skip entries with data, sorted longest-first for suffix sharing
        let mut active: Vec<(u16, &[u8])> = tt
            .entries
            .iter()
            .filter(|e| !e.skip && !e.data.is_empty() && e.id < table.ptr_count)
            .map(|e| (e.id, e.data.as_slice()))
            .collect();
        active.sort_by_key(|entry| Reverse(entry.1.len()));

        for &(id, data) in &active {
            // Check if this entry's data is a suffix of any already-allocated entry
            let suffix_match = allocated.iter().find(|&&(parent_off, parent_len)| {
                if data.len() > parent_len {
                    return false;
                }
                let delta = parent_len - data.len();
                if parent_off + delta < min_offset {
                    return false;
                }
                let parent_data = &bank[parent_off..parent_off + parent_len];
                parent_data[delta..] == *data
            });

            let bank_offset = if let Some(&(parent_off, parent_len)) = suffix_match {
                // Reuse parent allocation — pointer into the middle of parent's data
                let delta = parent_len - data.len();
                parent_off + delta
            } else {
                // Allocate new space
                let off = alloc.alloc(data.len(), min_offset).map_err(|_| {
                    anyhow::anyhow!(
                        "bank {} table '{}' entry {}: no space for {} bytes ({} writable total, {} used)",
                        config.bank_number,
                        table.name,
                        id,
                        data.len(),
                        total_writable,
                        total_written,
                    )
                })?;
                bank[off..off + data.len()].copy_from_slice(data);
                total_written += data.len();
                allocated.push((off, data.len()));
                off
            };

            // Compute and write pointer
            let ptr_val = (bank_offset as u32 + 0x8000 - table.base_logical as u32) as u16;
            let ptr_offset = table.ptr_bank_offset as usize + id as usize * 2;
            let bytes = ptr_val.to_le_bytes();
            bank[ptr_offset] = bytes[0];
            bank[ptr_offset + 1] = bytes[1];
        }
    }

    // Report usage
    println!(
        "  Bank {}: {}/{} writable bytes used ({:.1}%)",
        config.bank_number,
        total_written,
        total_writable,
        if total_writable > 0 {
            total_written as f64 / total_writable as f64 * 100.0
        } else {
            0.0
        }
    );

    verify_bank_within_whitelist(
        &bank,
        &en_rom[src..src + bank_size],
        &writable_set,
        &ptr_set,
        config.bank_number,
    )?;

    Ok(bank)
}

/// Source data for one relocated table: the active source ROM bank's
/// `physical_base` (needed to recover untranslated entries) plus this table's
/// encoded entries (KO translations, or `skip` markers).
pub struct RelocatedTableSource<'a> {
    pub source_physical_base: usize,
    pub entries: &'a [EncodedEntry],
}

/// Owned variant of [`RelocatedTableSource`] — used where the entries must
/// outlive the loop that produced them (e.g. `text::relocation::
/// partition_translations`, which collects sources across all banks before
/// any spare bank is built).
pub struct RelocatedTableSourceOwned {
    pub source_physical_base: usize,
    pub entries: Vec<EncodedEntry>,
}

impl RelocatedTableSourceOwned {
    pub fn as_ref(&self) -> RelocatedTableSource<'_> {
        RelocatedTableSource {
            source_physical_base: self.source_physical_base,
            entries: &self.entries,
        }
    }
}

/// Build a spare bank containing one or more relocated pointer tables from
/// scratch — a blank $FF canvas (spare banks 35-63 are unused space, no EN
/// bank content to preserve; see phase3a_whitelist_result.md §6).
///
/// `dest.tables[i]` must be an exact duplicate of the corresponding source
/// bank's `SubTable` (same `base_logical`/`ptr_bank_offset`/`ptr_count`) —
/// only `dest.physical_base`/`dest.bank_number` differ, because the game's
/// pointer-lookup routine hardcodes `base_logical` as both the ptr-table
/// address and the text base (phase3b_result.md §1).
/// `sources[i]` supplies the entries for `dest.tables[i]` (parallel arrays,
/// matched by index — see `relocation_config_matches_source` for the drift
/// guard on the config side).
///
/// Untranslated entries are copied byte-for-byte from the active source ROM
/// (no re-encoding). EN-base builds therefore preserve EN bytes; JP-direct
/// builds preserve JP-native bytes and rely on their collision-free prefix
/// audit. Callers must pass the same expanded ROM whose spare-bank region is
/// being populated, so whitelist comparison is against the real destination.
///
/// Each table gets its own suffix-sharing allocation ledger — deliberately
/// *not* shared across tables in the same spare bank, so suffix sharing
/// can't accidentally cross a table boundary (which would tie two logically
/// unrelated pointer tables' text together).
pub fn build_relocated_bank(
    dest: &MultiBankConfig,
    sources: &[RelocatedTableSource],
    source_rom: &[u8],
) -> Result<Vec<u8>> {
    anyhow::ensure!(
        sources.len() == dest.tables.len(),
        "build_relocated_bank: sources.len()={} != dest.tables.len()={} (spare bank {})",
        sources.len(),
        dest.tables.len(),
        dest.bank_number,
    );

    let bank_size = 0x4000usize;
    let mut bank = vec![0xFFu8; bank_size];

    // Step 1: pointer-table regions, zero-initialized (null pointer = unused
    // slot). The bank starts blank, so — unlike build_bank_with_tables —
    // these must be explicitly zeroed rather than inherited from an EN copy.
    let mut ptr_regions: Vec<(usize, usize)> = Vec::new();
    for table in dest.tables {
        let s = table.ptr_bank_offset as usize;
        let e = s + table.ptr_count as usize * 2;
        bank[s..e].fill(0x00);
        ptr_regions.push((s, e));
    }

    // Step 2: text candidate regions = whole bank minus pointer regions.
    let candidate_regions = regions_excluding(bank_size, &ptr_regions);
    let writable_set: HashSet<usize> = candidate_regions.iter().flat_map(|&(s, e)| s..e).collect();
    let mut ptr_set: HashSet<usize> = HashSet::new();
    for &(s, e) in &ptr_regions {
        for off in s..e {
            ptr_set.insert(off);
        }
    }
    let total_writable = writable_set.len();

    // Step 3: single allocator shared by all tables in this bank (mirrors
    // build_bank_with_tables' pattern: tables with a higher min_offset claim
    // their region first, lower-min_offset tables backfill what's left).
    let mut alloc = RegionAllocator::new(candidate_regions);
    let mut total_written = 0usize;

    let mut order: Vec<usize> = (0..dest.tables.len()).collect();
    order.sort_by_key(|&i| Reverse((dest.tables[i].base_logical - 0x8000) as usize));

    for i in order {
        let table = &dest.tables[i];
        let source = &sources[i];
        let min_offset = (table.base_logical - 0x8000) as usize;

        let mut translated_map: HashMap<u16, &[u8]> = HashMap::new();
        for e in source.entries {
            if !e.skip && !e.data.is_empty() {
                translated_map.insert(e.id, e.data.as_slice());
            }
        }

        // Resolve data for every id: translated KO bytes, or a raw source copy
        // for untranslated-but-active ids. Ids with no EN pointer at all
        // (inactive slots) are skipped — their ptr stays $0000.
        let mut resolved: Vec<(u16, Vec<u8>)> = Vec::new();
        for id in 0..table.ptr_count {
            if let Some(&data) = translated_map.get(&id) {
                resolved.push((id, data.to_vec()));
            } else if let Some(range) =
                entry_text_range(source_rom, source.source_physical_base, table, id)
            {
                let start = source.source_physical_base + range.start;
                let end = source.source_physical_base + range.end;
                resolved.push((id, source_rom[start..end].to_vec()));
            }
        }

        // Longest-first, for suffix sharing.
        resolved.sort_by_key(|(_, data)| Reverse(data.len()));

        // Table-local allocation ledger — never shared across tables.
        let mut allocated: Vec<(usize, usize)> = Vec::new();

        for (id, data) in &resolved {
            let data: &[u8] = data.as_slice();
            let suffix_match = allocated.iter().find(|&&(parent_off, parent_len)| {
                if data.len() > parent_len {
                    return false;
                }
                let delta = parent_len - data.len();
                if parent_off + delta < min_offset {
                    return false;
                }
                let parent_data = &bank[parent_off..parent_off + parent_len];
                parent_data[delta..] == *data
            });

            let bank_offset = if let Some(&(parent_off, parent_len)) = suffix_match {
                let delta = parent_len - data.len();
                parent_off + delta
            } else {
                let off = alloc.alloc(data.len(), min_offset).map_err(|_| {
                    anyhow::anyhow!(
                        "spare bank {} table '{}' entry {}: no space for {} bytes ({} writable total, {} used)",
                        dest.bank_number,
                        table.name,
                        id,
                        data.len(),
                        total_writable,
                        total_written,
                    )
                })?;
                bank[off..off + data.len()].copy_from_slice(data);
                total_written += data.len();
                allocated.push((off, data.len()));
                off
            };

            let ptr_val = (bank_offset as u32 + 0x8000 - table.base_logical as u32) as u16;
            let ptr_offset = table.ptr_bank_offset as usize + *id as usize * 2;
            let bytes = ptr_val.to_le_bytes();
            bank[ptr_offset] = bytes[0];
            bank[ptr_offset + 1] = bytes[1];
        }
    }

    println!(
        "  Spare bank {}: {}/{} writable bytes used ({:.1}%)",
        dest.bank_number,
        total_written,
        total_writable,
        if total_writable > 0 {
            total_written as f64 / total_writable as f64 * 100.0
        } else {
            0.0
        }
    );

    let source_blank = &source_rom[dest.physical_base..dest.physical_base + bank_size];
    verify_bank_within_whitelist(
        &bank,
        source_blank,
        &writable_set,
        &ptr_set,
        dest.bank_number,
    )?;

    Ok(bank)
}

/// Regions of `[0, bank_size)` not covered by any range in `exclude`.
/// Used by `build_relocated_bank` to derive the text candidate area as the
/// complement of the pointer-table regions (there's no "protected graphics"
/// concept in a blank spare bank — see design doc §2.2 step 3).
fn regions_excluding(bank_size: usize, exclude: &[(usize, usize)]) -> Vec<(usize, usize)> {
    let excluded: HashSet<usize> = exclude.iter().flat_map(|&(s, e)| s..e).collect();
    let mut regions = Vec::new();
    let mut run_start: Option<usize> = None;
    for off in 0..bank_size {
        if excluded.contains(&off) {
            if let Some(s) = run_start.take() {
                regions.push((s, off));
            }
        } else if run_start.is_none() {
            run_start = Some(off);
        }
    }
    if let Some(s) = run_start {
        regions.push((s, bank_size));
    }
    regions
}

/// Verify every byte differing from EN lies in a writable region or a pointer-
/// table region. A change elsewhere means a graphic/data block was clobbered.
fn verify_bank_within_whitelist(
    bank: &[u8],
    en_bank: &[u8],
    writable_set: &std::collections::HashSet<usize>,
    ptr_set: &std::collections::HashSet<usize>,
    bank_number: u8,
) -> Result<()> {
    for off in 0..bank.len().min(en_bank.len()) {
        if bank[off] != en_bank[off] && !writable_set.contains(&off) && !ptr_set.contains(&off) {
            return Err(anyhow::anyhow!(
                "bank {} byte ${:04X} changed but is outside writable/pointer regions \
                 (graphic/data clobbered — whitelist bug)",
                bank_number,
                off,
            ));
        }
    }
    Ok(())
}

/// Source text byte range for one entry: [start, end) following the pointer to
/// the FE terminator (inclusive). None if the pointer is null. Single source of
/// truth for both protected-walk and translated-walk.
fn entry_text_range(
    source_rom: &[u8],
    physical_base: usize,
    table: &SubTable,
    id: u16,
) -> Option<std::ops::Range<usize>> {
    let ptr_phys = physical_base + table.ptr_bank_offset as usize + id as usize * 2;
    let ptr_val = u16::from_le_bytes([source_rom[ptr_phys], source_rom[ptr_phys + 1]]);
    if ptr_val == 0 {
        return None;
    }
    let base_offset = (table.base_logical - 0x8000) as usize;
    let start = ptr_val as usize + base_offset;
    let mut end = start;
    // Scan for the $FE terminator up to the bank boundary — NOT an arbitrary
    // smaller cap. A hardcoded `0..2000` here previously truncated long
    // PAGE-chained entries (e.g. a bank 3 "third"/credits entry whose real
    // $FE terminator is at +2315B) to an incomplete, non-terminated range.
    // That silently under-protected/mis-copied text; verified via a real
    // build failure once build_relocated_bank started copying (rather than
    // just marking-protected) these ranges.
    for j in 0..0x4000usize.saturating_sub(start) {
        if start + j >= 0x4000 || physical_base + start + j >= source_rom.len() {
            break;
        }
        end = start + j + 1;
        if source_rom[physical_base + start + j] == 0xFE {
            break;
        }
    }
    Some(start..end)
}

/// Bank offsets referenced by TRANSLATED entries' EN text (whitelist source).
fn compute_translated_text_bytes(
    config: &MultiBankConfig,
    en_rom: &[u8],
    translated_set: &HashSet<(usize, u16)>,
) -> HashSet<usize> {
    let mut bytes = HashSet::new();
    for (tidx, table) in config.tables.iter().enumerate() {
        for id in 0..table.ptr_count {
            if !translated_set.contains(&(tidx, id)) {
                continue;
            }
            if let Some(range) = entry_text_range(en_rom, config.physical_base, table, id) {
                for off in range {
                    bytes.insert(off);
                }
            }
        }
    }
    bytes
}

/// Compute which bank offsets are "protected" — cannot be overwritten.
///
/// Protected bytes include:
/// - Pointer table areas for all tables
/// - Text bytes referenced by any non-translated entry in any table
fn compute_protected_bytes(
    config: &MultiBankConfig,
    en_rom: &[u8],
    translated_set: &HashSet<(usize, u16)>,
) -> HashSet<usize> {
    let mut protected = HashSet::new();

    // Protect known non-text/game-data regions.
    for &(start, end) in config.reserved_ranges {
        for off in start..end.min(0x4000) {
            protected.insert(off);
        }
    }

    // Protect all pointer table areas
    for table in config.tables {
        let start = table.ptr_bank_offset as usize;
        let end = start + table.ptr_count as usize * 2;
        for off in start..end {
            protected.insert(off);
        }
    }

    // For each table, protect text bytes referenced by non-translated entries
    for (tidx, table) in config.tables.iter().enumerate() {
        for id in 0..table.ptr_count {
            if translated_set.contains(&(tidx, id)) {
                continue; // Translated entry — its bytes can be reclaimed
            }
            if let Some(range) = entry_text_range(en_rom, config.physical_base, table, id) {
                for off in range {
                    protected.insert(off);
                }
            }
        }
    }

    protected
}

/// Fail-closed whitelist: writable = bytes referenced by TRANSLATED entries,
/// minus protected (pointer tables, reserved, untranslated-text).
/// Graphics/data (referenced by no text pointer) are never writable.
fn compute_writable_regions(
    translated: &HashSet<usize>,
    protected: &HashSet<usize>,
) -> Vec<(usize, usize)> {
    let mut regions = Vec::new();
    let mut run_start: Option<usize> = None;
    for off in 0..0x4000usize {
        let writable = translated.contains(&off) && !protected.contains(&off);
        if writable {
            if run_start.is_none() {
                run_start = Some(off);
            }
        } else if let Some(s) = run_start.take() {
            regions.push((s, off));
        }
    }
    if let Some(s) = run_start {
        regions.push((s, 0x4000));
    }
    regions.retain(|&(s, e)| e - s >= 2);
    regions
}

/// Sequential allocator across multiple memory regions.
struct RegionAllocator {
    regions: Vec<(usize, usize)>,
}

impl RegionAllocator {
    fn new(regions: Vec<(usize, usize)>) -> Self {
        RegionAllocator { regions }
    }

    /// Allocate `size` bytes at a bank offset >= `min_offset`.
    ///
    /// `min_offset` ensures the allocated address produces a valid (non-negative)
    /// pointer value relative to the table's base_logical address.
    fn alloc(&mut self, size: usize, min_offset: usize) -> Result<usize, ()> {
        for idx in 0..self.regions.len() {
            let (start, end) = self.regions[idx];
            let addr = start.max(min_offset);
            if addr + size > end {
                continue;
            }

            let mut replacement = Vec::new();
            if start < addr {
                replacement.push((start, addr));
            }
            if addr + size < end {
                replacement.push((addr + size, end));
            }
            self.regions.splice(idx..=idx, replacement);
            return Ok(addr);
        }
        Err(())
    }
}

#[cfg(test)]
#[path = "bank_tests.rs"]
mod tests;
