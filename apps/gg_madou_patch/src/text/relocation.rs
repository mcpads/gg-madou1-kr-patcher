//! Relocation policy: maps overflowing (bank, table) pairs — identified by
//! Phase 3a's fail-closed whitelist (docs/reverse_engineering/
//! phase3a_whitelist_result.md) — to spare banks (35-63, confirmed blank
//! $FF space in the EN ROM) and builds the routing shared by
//! `commands::build` and `commands::check` so their behavior can't drift
//! (phase3b_result.md §3.4).
//!
//! Deliberately kept separate from `text::bank`'s `SubTable`/`MultiBankConfig`:
//! those describe the ROM's fixed hardware layout (almost never changes),
//! while "which table relocates where" is a policy that shifts as
//! translation coverage grows and spare-bank capacity is consumed.

use std::collections::BTreeMap;

use anyhow::{Context, Result};

use crate::text::bank::{
    self, EncodedEntry, MultiBankConfig, RelocatedTableSourceOwned, SubTable, TableTranslation,
};
use crate::text::control::encode_ko_text;
use crate::text::encoding::KoEncoding;
use crate::translation::loader::BankTranslation;

/// One (source bank, table) → spare bank relocation.
pub struct RelocationEntry {
    pub source_bank: u8,
    pub source_table_name: &'static str,
    pub spare_bank_number: u8,
}

/// Overflow tables (phase3a_whitelist_result.md §6, phase3b_result.md
/// §7): bank 3 main+secondary → spare bank 35, bank 5 main → spare bank 36.
///
/// `phase3b_result.md` §7 measured bank 3 "third" as fitting with
/// 0 overflow (3604B needed, 3604B allocated) and left it in place — but
/// that measurement ran all 5 tables against ONE shared writable pool
/// (`build_bank_with_tables` allocates from a single pool ordered by
/// `min_offset`, not per-table). "third" has the highest `min_offset` in
/// bank 3, so it got first pick of every writable byte at or above its
/// offset — including bytes that were only writable because main/secondary
/// were being translated too. Once main+secondary are excluded here (§3.3:
/// an excluded table's EN text becomes fully protected, not reclaimable),
/// "third" loses that borrowed headroom and overflows on its own (verified
/// empirically: real build failed with `bank 3 table 'third' entry 107: no
/// space for 25 bytes (2585 writable total, 2561 used)` once main+secondary
/// were excluded but third was not). So "third" was relocated too, initially
/// alongside main/secondary into spare bank 35 (Phase 3b Task 1).
///
/// The identical mechanism hit bank 5 "secondary" once bank 5 "main" was
/// excluded: it was measured at 0 overflow in §7 (2110B needed) under the
/// same 5-table shared pool, then failed for real once "main" was excluded
/// (`bank 5 table 'secondary' entry 27: no space for 29 bytes (1581
/// writable total, 1563 used)`) for the same reason — "secondary" has the
/// higher `min_offset` in bank 5 and had first pick of "main"'s
/// would-be-reclaimable bytes before "main" was pulled out of the pool. So
/// "secondary" is relocated too, alongside "main".
///
/// **Phase 3c**: sharing spare bank 35 among all three bank-3 tables left
/// only 88B free (99.4% saturated, phase3b_result.md §4) — not enough
/// headroom for bank 3 "third"'s remaining 94 untranslated entries once
/// they're translated. "third" is therefore split into its own spare bank
/// 37 (`SPARE_37_CONFIG`), leaving spare 35 with only main+secondary and
/// substantially more headroom. This is a pure capacity split — "third"
/// doesn't share a pool with main/secondary any more (each spare bank gets
/// its own `RegionAllocator`, see `build_relocated_bank`), so it no longer
/// needs to move in lockstep with them the way it did when bank 3 was still
/// built in place.
///
/// Net result: this map relocates every table whose original bank build
/// only fit while sharing a pool with tables that are *also* relocated —
/// i.e. both bank 3 and bank 5 move entirely to spare banks (35/36/37 as of
/// Phase 3c). This is a deviation from `phase3b_result.md`
/// §3.1's example map (which listed only 3 entries and assumed bank 3
/// "third"/bank 5 "secondary" would fit in place), made to satisfy the
/// actual overflow-free-build requirement — confirmed by real
/// `cargo run -- build` runs, not assumed.
pub static RELOCATION_MAP: &[RelocationEntry] = &[
    RelocationEntry {
        source_bank: 3,
        source_table_name: "main",
        spare_bank_number: 35,
    },
    RelocationEntry {
        source_bank: 3,
        source_table_name: "secondary",
        spare_bank_number: 35,
    },
    RelocationEntry {
        source_bank: 3,
        source_table_name: "third",
        spare_bank_number: 37,
    },
    RelocationEntry {
        source_bank: 5,
        source_table_name: "main",
        spare_bank_number: 36,
    },
    RelocationEntry {
        source_bank: 5,
        source_table_name: "secondary",
        spare_bank_number: 36,
    },
];

/// Spare bank number that Z80 entry-point wiring (`patch::relocation`,
/// `commands::build` Stage 7) must target for a given (source_bank,
/// table_name) — derived directly from `RELOCATION_MAP`, the single source
/// of truth for the data-side routing (`partition_translations` uses the
/// same `target_for` lookup). Callers use this instead of reading a spare
/// config's `bank_number` field directly, so the `LD B,n` immediate baked
/// into a relocation stub can never silently drift from where
/// `build_relocated_bank` actually placed the table's data.
pub fn spare_bank_for(source_bank: u8, table_name: &str) -> Result<u8> {
    target_for(source_bank, table_name)
        .map(|e| e.spare_bank_number)
        .with_context(|| {
            format!("RELOCATION_MAP has no entry for bank {source_bank} table '{table_name}'")
        })
}

/// Spare bank 35's layout duplicates `BANK3_CONFIG`'s main/secondary
/// `SubTable` values verbatim (`base_logical`/`ptr_bank_offset`/`ptr_count`)
/// — only `bank_number`/`physical_base` point at the spare bank instead.
/// This duplication is not optional: the game's shared pointer-lookup
/// routine hardcodes `base_logical` as both the ptr-table address and the
/// text base (design doc §1). The `relocation_config_matches_source` test
/// (text/bank_tests.rs) guards against this drifting from BANK3_CONFIG.
///
/// Phase 3c: "third" used to live here too (Phase 3b), but sharing spare 35
/// among all three bank-3 tables left only 88B free (99.4% saturated) — not
/// enough headroom for "third"'s remaining untranslated entries. "third"
/// now has its own spare bank (`SPARE_37_CONFIG`); this config keeps only
/// main+secondary.
pub static SPARE_35_CONFIG: MultiBankConfig = MultiBankConfig {
    bank_number: 35,
    physical_base: 35 * 0x4000,
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
    ],
};

/// Spare bank 37's layout duplicates `BANK3_CONFIG`'s "third" `SubTable`
/// verbatim (same rationale as spare bank 35 above). Phase 3c: split out of
/// spare 35 to give "third" its own capacity pool, independent of
/// main/secondary's usage in spare 35.
pub static SPARE_37_CONFIG: MultiBankConfig = MultiBankConfig {
    bank_number: 37,
    physical_base: 37 * 0x4000,
    reserved_ranges: &[],
    tables: &[SubTable {
        name: "third",
        base_logical: 0x9D40,
        ptr_bank_offset: 0x1D40,
        ptr_count: 256,
    }],
};

/// Spare bank 36's layout duplicates `BANK5_CONFIG`'s "main"/"secondary"
/// `SubTable` values verbatim (same rationale as spare bank 35 above).
pub static SPARE_36_CONFIG: MultiBankConfig = MultiBankConfig {
    bank_number: 36,
    physical_base: 36 * 0x4000,
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
            base_logical: 0xA000,
            ptr_bank_offset: 0x2000,
            ptr_count: 256,
        },
    ],
};

pub fn spare_config_for(spare_bank_number: u8) -> Option<&'static MultiBankConfig> {
    match spare_bank_number {
        35 => Some(&SPARE_35_CONFIG),
        36 => Some(&SPARE_36_CONFIG),
        37 => Some(&SPARE_37_CONFIG),
        _ => None,
    }
}

/// Relocation target for a (source_bank, table_name) pair, if that table is
/// overflow-relocated. `None` means it builds in place, in its original bank.
pub fn target_for(source_bank: u8, table_name: &str) -> Option<&'static RelocationEntry> {
    RELOCATION_MAP
        .iter()
        .find(|e| e.source_bank == source_bank && e.source_table_name == table_name)
}

/// Encoded translations, classified into original-bank tables (built in
/// place with `bank::build_bank_with_tables`) and relocated tables (built
/// standalone into a spare bank with `bank::build_relocated_bank`).
///
/// This is the single source of truth for `commands::build`/`commands::check`
/// routing (design doc §3.4): both commands call `partition_translations`
/// instead of each re-deriving the routing, so they can't silently disagree
/// about which tables are relocated.
pub struct PartitionedTranslations {
    pub by_original_bank: BTreeMap<u8, Vec<TableTranslation>>,
    pub by_spare_bank: BTreeMap<u8, Vec<RelocatedTableSourceOwned>>,
}

pub fn partition_translations(
    translations: &[BankTranslation],
    ko_enc: &KoEncoding,
) -> Result<PartitionedTranslations> {
    let mut by_bank: BTreeMap<u8, Vec<&BankTranslation>> = BTreeMap::new();
    for bt in translations {
        by_bank.entry(bt.bank).or_default().push(bt);
    }

    let mut by_original_bank: BTreeMap<u8, Vec<TableTranslation>> = BTreeMap::new();
    // spare_bank -> dest table index -> source. Indexed so completeness
    // (every dest table has a source) can be validated before flattening to
    // the parallel Vec that `build_relocated_bank` requires.
    let mut by_spare_bank_idx: BTreeMap<u8, BTreeMap<usize, RelocatedTableSourceOwned>> =
        BTreeMap::new();

    for (source_bank, bank_translations) in &by_bank {
        let Some(config) = bank::config_for_bank(*source_bank) else {
            eprintln!("  Warning: unknown bank {source_bank}, skipping");
            continue;
        };

        for bt in bank_translations {
            let Some(table_idx) = config.tables.iter().position(|t| t.name == bt.table) else {
                eprintln!(
                    "  Warning: unknown table '{}' in bank {}, skipping",
                    bt.table, bt.bank
                );
                continue;
            };

            let mut encoded_entries = Vec::with_capacity(bt.entries.len());
            for entry in &bt.entries {
                if entry.skip || entry.ko.is_empty() {
                    encoded_entries.push(EncodedEntry {
                        id: entry.id,
                        data: Vec::new(),
                        skip: true,
                    });
                } else {
                    let data = encode_ko_text(&entry.ko, ko_enc).with_context(|| {
                        format!(
                            "failed to encode entry bank={} table={} id={}",
                            bt.bank, bt.table, entry.id
                        )
                    })?;
                    encoded_entries.push(EncodedEntry {
                        id: entry.id,
                        data,
                        skip: false,
                    });
                }
            }

            match target_for(*source_bank, &bt.table) {
                Some(target) => {
                    let spare_config = spare_config_for(target.spare_bank_number)
                        .with_context(|| {
                            format!("unmapped spare bank {}", target.spare_bank_number)
                        })?;
                    let dest_table_idx = spare_config
                        .tables
                        .iter()
                        .position(|t| t.name == bt.table)
                        .with_context(|| {
                            format!(
                                "spare bank {} config missing table '{}' (source bank {})",
                                target.spare_bank_number, bt.table, source_bank
                            )
                        })?;
                    by_spare_bank_idx
                        .entry(target.spare_bank_number)
                        .or_default()
                        .insert(
                            dest_table_idx,
                            RelocatedTableSourceOwned {
                                source_physical_base: config.physical_base,
                                entries: encoded_entries,
                            },
                        );
                }
                None => {
                    by_original_bank
                        .entry(*source_bank)
                        .or_default()
                        .push(TableTranslation {
                            table_idx,
                            entries: encoded_entries,
                        });
                }
            }
        }
    }

    let mut by_spare_bank: BTreeMap<u8, Vec<RelocatedTableSourceOwned>> = BTreeMap::new();
    for (spare_bank, mut by_idx) in by_spare_bank_idx {
        let dest = spare_config_for(spare_bank)
            .with_context(|| format!("unmapped spare bank {spare_bank}"))?;
        let mut sources = Vec::with_capacity(dest.tables.len());
        for idx in 0..dest.tables.len() {
            let source = by_idx.remove(&idx).with_context(|| {
                format!(
                    "spare bank {} missing translation source for table '{}' (index {})",
                    spare_bank, dest.tables[idx].name, idx
                )
            })?;
            sources.push(source);
        }
        by_spare_bank.insert(spare_bank, sources);
    }

    Ok(PartitionedTranslations {
        by_original_bank,
        by_spare_bank,
    })
}
