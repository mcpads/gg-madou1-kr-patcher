//! Verify the JP ROM's bank 3/5 pointer tables against the (EN-derived)
//! MultiBankConfig structure. M0 discovery: confirms the configs hold on JP
//! and records the JP active-entry counts.

use anyhow::{Result, bail};

use crate::jp::render::{JP_KO_PREFIX_END, JP_KO_PREFIX_START};
use crate::text::bank::{TableTranslation, build_bank_with_tables, config_for_bank};

#[derive(Debug, Clone)]
pub struct TableReport {
    pub bank: u8,
    pub table: String,
    pub active: usize,         // non-zero pointers
    pub out_of_range: usize,   // resolved bank offset outside [0, 0x4000)
    pub first_bytes_ff: usize, // active entries whose first byte is 0xFF (suspicious)
}

const BANK_SIZE: usize = 0x4000;
const VAR_POINTER_TABLE_ADDR: usize = 0x243E;
const VAR_COUNT: usize = 13;

fn verify_text_has_no_reserved_prefix(
    text: &[u8],
    terminator: u8,
    terminator_required: bool,
    label: &str,
) -> Result<()> {
    let terminator_index = text.iter().position(|&byte| byte == terminator);
    if terminator_required && terminator_index.is_none() {
        bail!("{label}: missing ${terminator:02X} terminator");
    }
    // Match text::bank::entry_text_range: mixed JP entries without an FE
    // terminator own the remainder of their bank, so audit that full range.
    let end = terminator_index.unwrap_or(text.len());
    if let Some((index, &byte)) = text[..end]
        .iter()
        .enumerate()
        .find(|(_, byte)| (JP_KO_PREFIX_START..=JP_KO_PREFIX_END).contains(byte))
    {
        bail!("{label}: reserved JP KO prefix ${byte:02X} at text+{index:#X}");
    }
    Ok(())
}

/// Prove that the JP-direct KO prefix range is absent from every native text
/// source that reaches the character dispatcher: the five bank 3/5 pointer
/// tables and the 13 bank-0 `{VAR}` strings.
pub fn verify_reserved_ko_prefixes_unused(rom: &[u8]) -> Result<usize> {
    let mut checked = 0usize;

    for &bank in &[3u8, 5u8] {
        let config =
            config_for_bank(bank).ok_or_else(|| anyhow::anyhow!("no config for bank {bank}"))?;
        let base = config.physical_base;
        if base + BANK_SIZE > rom.len() {
            bail!("ROM too small for bank {bank} at {base:#07X}");
        }
        for table in config.tables {
            for id in 0..table.ptr_count as usize {
                let po = base + table.ptr_bank_offset as usize + id * 2;
                let ptr = u16::from_le_bytes([rom[po], rom[po + 1]]);
                if ptr == 0 {
                    continue;
                }
                let start = ptr as usize + table.base_logical as usize - 0x8000;
                if start >= BANK_SIZE {
                    bail!("bank{bank} {} id{id}: pointer outside bank", table.name);
                }
                verify_text_has_no_reserved_prefix(
                    &rom[base + start..base + BANK_SIZE],
                    0xFE,
                    false,
                    &format!("bank{bank} {} id{id}", table.name),
                )?;
                checked += 1;
            }
        }
    }

    let table_end = VAR_POINTER_TABLE_ADDR + VAR_COUNT * 2;
    if table_end > rom.len() {
        bail!("ROM too small for bank-0 VAR pointer table");
    }
    for id in 0..VAR_COUNT {
        let po = VAR_POINTER_TABLE_ADDR + id * 2;
        let ptr = u16::from_le_bytes([rom[po], rom[po + 1]]) as usize;
        if ptr >= rom.len() {
            bail!("bank0 VAR id{id}: pointer ${ptr:04X} outside ROM");
        }
        verify_text_has_no_reserved_prefix(&rom[ptr..], 0xFF, true, &format!("bank0 VAR id{id}"))?;
        checked += 1;
    }

    Ok(checked)
}

pub fn verify_all(rom: &[u8]) -> Result<Vec<TableReport>> {
    let mut reports = Vec::new();
    for &bank in &[3u8, 5u8] {
        let config =
            config_for_bank(bank).ok_or_else(|| anyhow::anyhow!("no config for bank {bank}"))?;
        let base = config.physical_base;
        if base + BANK_SIZE > rom.len() {
            bail!("ROM too small for bank {bank} at {base:#07X}");
        }
        let bank_bytes = &rom[base..base + BANK_SIZE];
        for table in config.tables {
            let mut active = 0;
            let mut out_of_range = 0;
            let mut first_bytes_ff = 0;
            for id in 0..table.ptr_count as usize {
                let po = table.ptr_bank_offset as usize + id * 2;
                let ptr = u16::from_le_bytes([bank_bytes[po], bank_bytes[po + 1]]);
                if ptr == 0 {
                    continue; // empty slot
                }
                active += 1;
                // ptr_val = bank_offset + 0x8000 - base_logical  (build_bank_with_tables)
                let off = ptr as i64 + table.base_logical as i64 - 0x8000;
                if !(0..BANK_SIZE as i64).contains(&off) {
                    out_of_range += 1;
                } else if bank_bytes[off as usize] == 0xFF {
                    first_bytes_ff += 1;
                }
            }
            reports.push(TableReport {
                bank,
                table: table.name.to_string(),
                active,
                out_of_range,
                first_bytes_ff,
            });
        }
    }
    Ok(reports)
}

/// Run `build_bank_with_tables` with zero translations. Because the builder
/// copies the source bank and only overwrites *translated* entries, an empty
/// translation set must reproduce the input bank byte-for-byte. This proves
/// the machine (bank copy + whitelist verify) accepts JP input without
/// corrupting it — the raw reinsertion gate before any KR text lands.
pub fn noop_rebuild_bank(rom: &[u8], bank: u8) -> Result<Vec<u8>> {
    let config =
        config_for_bank(bank).ok_or_else(|| anyhow::anyhow!("no config for bank {bank}"))?;
    let empty: Vec<TableTranslation> = Vec::new();
    build_bank_with_tables(config, &empty, rom)
        .map_err(|e| anyhow::anyhow!("bank {bank} no-op rebuild failed: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn jp_rom() -> Option<Vec<u8>> {
        let path = std::env::var("GG_MADOU1_JP_ROM").unwrap_or_else(|_| {
            "../../roms/Madou Monogatari I - 3-Tsu no Madoukyuu (Japan).gg".to_string()
        });
        std::fs::read(&path).ok()
    }

    #[test]
    fn jp_tables_resolve_in_range() {
        let Some(rom) = jp_rom() else {
            eprintln!("skip: JP ROM absent");
            return;
        };
        let reports = verify_all(&rom).expect("verify");
        assert_eq!(reports.len(), 5, "3 bank3 tables + 2 bank5 tables");
        for r in &reports {
            eprintln!(
                "  bank{} {}: active={} oor={} ff={}",
                r.bank, r.table, r.active, r.out_of_range, r.first_bytes_ff
            );
            assert!(
                r.active > 0,
                "bank{} {} has no active entries",
                r.bank,
                r.table
            );
            assert_eq!(
                r.out_of_range, 0,
                "bank{} {} has {} out-of-range pointers — config invalid on JP",
                r.bank, r.table, r.out_of_range
            );
        }
    }

    #[test]
    fn noop_rebuild_is_byte_identical() {
        let Some(rom) = jp_rom() else {
            eprintln!("skip: JP ROM absent");
            return;
        };
        for &bank in &[3u8, 5u8] {
            let rebuilt = noop_rebuild_bank(&rom, bank).expect("rebuild");
            let config = config_for_bank(bank).unwrap();
            let src = config.physical_base;
            assert_eq!(
                rebuilt,
                &rom[src..src + 0x4000],
                "bank {bank} no-op rebuild is not byte-identical — build machine mutates JP input"
            );
        }
    }

    #[test]
    fn jp_ko_prefixes_are_unused_in_active_text() {
        let Some(rom) = jp_rom() else {
            eprintln!("skip: JP ROM absent");
            return;
        };

        let checked = verify_reserved_ko_prefixes_unused(&rom).expect("prefix audit");
        assert_eq!(checked, 1_042, "1029 bank3/5 entries + 13 VAR strings");
    }
}
