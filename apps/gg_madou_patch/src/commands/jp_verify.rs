use anyhow::{Context, Result};
use std::path::Path;

/// M0 structure verification: JP text-engine hook signature + bank 3/5
/// pointer-table validity. Prints active-entry counts for the record.
pub fn run(jp_rom_path: &Path) -> Result<()> {
    println!("=== JP-native structure verify (M0) ===");
    let rom = std::fs::read(jp_rom_path)
        .with_context(|| format!("failed to read JP ROM: {}", jp_rom_path.display()))?;

    crate::jp::verify_supported_jp_rom(&rom).context("supported JP ROM identity")?;

    println!("\n[1] Text-engine hook ($238A)");
    crate::jp::engine::verify_hook(&rom).context("JP hook signature")?;
    println!("  OK: {:02X?}", crate::jp::engine::HOOK_SIGNATURE);

    println!("\n[2] Pointer tables (bank 3/5)");
    let reports = crate::jp::tables::verify_all(&rom).context("pointer tables")?;
    for r in &reports {
        println!(
            "  bank{} {:<10} active={:<4} out_of_range={} first_byte_ff={}",
            r.bank, r.table, r.active, r.out_of_range, r.first_bytes_ff
        );
    }
    let bad: usize = reports.iter().map(|r| r.out_of_range).sum();
    if bad > 0 {
        anyhow::bail!("{bad} out-of-range pointer(s) — EN-derived config invalid on JP");
    }
    println!("\n=== Verify OK ===");
    Ok(())
}
