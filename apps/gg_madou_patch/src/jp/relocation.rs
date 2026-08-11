//! JP-direct entry-point wiring for the relocated bank 3/5 text tables.
//!
//! M2 re-verified the six callers, five reclaimed stub regions, and the two
//! shared lookup continuations against the original JP ROM. Their addresses
//! happen to match the EN-base wiring, but this module owns JP-specific
//! signatures so the JP build fails closed if that assumption ever drifts.

use anyhow::{Context, Result, ensure};

use crate::patch::{constants as c, relocation as patch};

fn expect_bytes(rom: &[u8], off: usize, expected: &[u8], label: &str) -> Result<()> {
    let got = rom
        .get(off..off + expected.len())
        .with_context(|| format!("{label} signature out of range at ${off:05X}"))?;
    ensure!(
        got == expected,
        "{label} JP signature mismatch at ${off:05X}: expected {expected:02X?}, got {got:02X?}"
    );
    Ok(())
}

fn write_at(rom: &mut [u8], off: usize, data: &[u8]) {
    rom[off..off + data.len()].copy_from_slice(data);
}

/// Verify and install the complete five-stub/six-entry relocation graph.
/// All original signatures are checked before the first mutation.
pub fn apply(rom: &mut [u8]) -> Result<()> {
    // Shared lookup bodies: JP and EN are byte-identical at the bank-select
    // prologues and continuations. Check both before reusing the assemblers.
    expect_bytes(rom, 0x22CA, &[0x06, 0x03, 0x57], "JP text-window lookup")?;
    expect_bytes(rom, 0x232B, &[0x06, 0x03, 0x3A], "JP sprite lookup")?;

    let stub_signatures: &[(usize, &[u8], &str)] = &[
        (
            c::STUB_TEXTWINDOW_A_ADDR,
            &[0xAF, 0x18, 0x02, 0x3E, 0x01],
            "JP stub A region",
        ),
        (
            c::STUB_SPRITE_B_ADDR,
            &[0x18, 0x65, 0x06, 0x05, 0x18],
            "JP stub B region",
        ),
        (
            c::STUB_TEXTWINDOW_C_ADDR,
            &[0x21, 0x3E, 0xC9, 0x18, 0x03],
            "JP stub C region",
        ),
        (
            c::STUB_TEXTWINDOW_D_ADDR,
            &[0xCD, 0xF3, 0x22, 0xCD, 0x25],
            "JP stub D region",
        ),
        (
            c::STUB_SPRITE_E_ADDR,
            &[0x27, 0xCD, 0xF4, 0x2B, 0xC3],
            "JP stub E region",
        ),
    ];
    for &(off, expected, label) in stub_signatures {
        expect_bytes(rom, off, expected, label)?;
    }

    let entry_signatures: &[(usize, &[u8], &str)] = &[
        (
            c::ENTRY1_BANK3_MAIN_TEXTWINDOW_JR_ADDR,
            &[0x09],
            "JP entry 1",
        ),
        (
            c::ENTRY2_BANK3_THIRD_TEXTWINDOW_CALL_ADDR,
            &[0xCA, 0x22],
            "JP entry 2",
        ),
        (
            c::ENTRY3_BANK3_SECONDARY_TEXTWINDOW_CALL_ADDR,
            &[0xCA, 0x22],
            "JP entry 3",
        ),
        (
            c::ENTRY4_BANK3_SECONDARY_SPRITE_CALL_ADDR,
            &[0x2B, 0x23],
            "JP entry 4",
        ),
        (
            c::ENTRY5_BANK3_THIRD_SPRITE_CALL_ADDR,
            &[0x2B, 0x23],
            "JP entry 5",
        ),
        (
            c::ENTRY6_BANK5_SHARED_CALL_ADDR,
            &[0xC6, 0x22],
            "JP entry 6",
        ),
    ];
    for &(off, expected, label) in entry_signatures {
        expect_bytes(rom, off, expected, label)?;
    }

    let bank3_main = crate::text::relocation::spare_bank_for(3, "main")?;
    let bank3_secondary = crate::text::relocation::spare_bank_for(3, "secondary")?;
    ensure!(
        bank3_main == bank3_secondary,
        "JP stub A/B requires bank3 main+secondary to share one spare bank"
    );
    let bank3_third = crate::text::relocation::spare_bank_for(3, "third")?;
    let bank5_main = crate::text::relocation::spare_bank_for(5, "main")?;
    let bank5_secondary = crate::text::relocation::spare_bank_for(5, "secondary")?;
    ensure!(
        bank5_main == bank5_secondary,
        "JP stub C requires bank5 main+secondary to share one spare bank"
    );

    let stub_a = patch::assemble_stub_textwindow(bank3_main).context("assemble JP stub A")?;
    let stub_b = patch::assemble_stub_sprite(bank3_main).context("assemble JP stub B")?;
    let stub_c = patch::assemble_stub_textwindow(bank5_main).context("assemble JP stub C")?;
    let stub_d = patch::assemble_stub_textwindow(bank3_third).context("assemble JP stub D")?;
    let stub_e = patch::assemble_stub_sprite(bank3_third).context("assemble JP stub E")?;

    write_at(rom, c::STUB_TEXTWINDOW_A_ADDR, &stub_a);
    write_at(rom, c::STUB_SPRITE_B_ADDR, &stub_b);
    write_at(rom, c::STUB_TEXTWINDOW_C_ADDR, &stub_c);
    write_at(rom, c::STUB_TEXTWINDOW_D_ADDR, &stub_d);
    write_at(rom, c::STUB_SPRITE_E_ADDR, &stub_e);

    write_at(
        rom,
        c::ENTRY1_BANK3_MAIN_TEXTWINDOW_JR_ADDR,
        &[patch::entry1_new_byte()],
    );
    write_at(
        rom,
        c::ENTRY2_BANK3_THIRD_TEXTWINDOW_CALL_ADDR,
        &patch::entry2_new_bytes(),
    );
    write_at(
        rom,
        c::ENTRY3_BANK3_SECONDARY_TEXTWINDOW_CALL_ADDR,
        &patch::entry3_new_bytes(),
    );
    write_at(
        rom,
        c::ENTRY4_BANK3_SECONDARY_SPRITE_CALL_ADDR,
        &patch::entry4_new_bytes(),
    );
    write_at(
        rom,
        c::ENTRY5_BANK3_THIRD_SPRITE_CALL_ADDR,
        &patch::entry5_new_bytes(),
    );
    write_at(
        rom,
        c::ENTRY6_BANK5_SHARED_CALL_ADDR,
        &patch::entry6_new_bytes(),
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn jp_rom() -> Option<Vec<u8>> {
        let path = std::env::var("GG_MADOU1_JP_ROM").unwrap_or_else(|_| {
            "../../roms/Madou Monogatari I - 3-Tsu no Madoukyuu (Japan).gg".to_string()
        });
        std::fs::read(path).ok()
    }

    #[test]
    fn jp_original_accepts_complete_relocation_graph() {
        let Some(jp) = jp_rom() else {
            eprintln!("skip: JP ROM absent");
            return;
        };
        let mut rom = crate::jp::expand_to_1mb(&jp).expect("expand JP ROM");
        apply(&mut rom).expect("JP relocation wiring");
        assert_eq!(
            &rom[c::STUB_TEXTWINDOW_D_ADDR..c::STUB_TEXTWINDOW_D_ADDR + 5],
            &[0x06, 0x25, 0xC3, 0xCC, 0x22]
        );
        assert_eq!(
            &rom[c::ENTRY6_BANK5_SHARED_CALL_ADDR..c::ENTRY6_BANK5_SHARED_CALL_ADDR + 2],
            &[0xAA, 0x26]
        );
    }

    #[test]
    fn jp_relocation_rejects_signature_drift() {
        let Some(jp) = jp_rom() else {
            eprintln!("skip: JP ROM absent");
            return;
        };
        let mut rom = crate::jp::expand_to_1mb(&jp).expect("expand JP ROM");
        rom[c::ENTRY5_BANK3_THIRD_SPRITE_CALL_ADDR] ^= 0x01;
        let err = apply(&mut rom).expect_err("signature drift must fail");
        assert!(err.to_string().contains("JP entry 5"));
    }
}
