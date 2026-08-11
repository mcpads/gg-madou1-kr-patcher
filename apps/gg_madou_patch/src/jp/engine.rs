//! JP-native text-engine hook facts (verified by disasm of the JP ROM).
//!
//! At $238A the JP engine does `LD A,$85; CP D; JR C,$17`: D <= $85 renders a
//! fixed tile (`tile = D + $54`) directly; D >= $86 branches to $23A6 dakuten
//! compositing. The KR path will hook here so that `D >= $80` routes to the
//! Korean renderer while `D < $80` keeps the native fixed-tile path.

use anyhow::{Result, bail};

pub const HOOK_ADDR: usize = 0x238A;
pub const HOOK_SIGNATURE: [u8; 5] = [0x3E, 0x85, 0xBA, 0x38, 0x17];

pub fn verify_hook(rom: &[u8]) -> Result<()> {
    if rom.len() < HOOK_ADDR + HOOK_SIGNATURE.len() {
        bail!(
            "ROM too small for JP hook check: need {} bytes, got {}",
            HOOK_ADDR + HOOK_SIGNATURE.len(),
            rom.len()
        );
    }
    let got = &rom[HOOK_ADDR..HOOK_ADDR + HOOK_SIGNATURE.len()];
    if got != HOOK_SIGNATURE {
        bail!(
            "JP hook signature mismatch at {HOOK_ADDR:#07X}: expected {HOOK_SIGNATURE:02X?}, got {got:02X?}"
        );
    }
    Ok(())
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
    fn jp_hook_signature_present() {
        let Some(rom) = jp_rom() else {
            eprintln!("skip: JP ROM absent");
            return;
        };
        verify_hook(&rom).expect("JP $238A hook signature");
    }

    #[test]
    fn jp_hook_is_not_en_signature() {
        // EN's $238A is `LD A,$54` (3E 54...). The JP ROM must read $85 here,
        // not $54 — a real per-byte check guarding against pointing the JP
        // path at an EN ROM.
        let Some(rom) = jp_rom() else {
            eprintln!("skip: JP ROM absent");
            return;
        };
        assert_eq!(
            rom[HOOK_ADDR + 1],
            0x85,
            "JP $238A+1 must be $85 (LD A,$85), not EN's $54"
        );
    }

    #[test]
    fn verify_hook_errors_on_short_rom() {
        let short = vec![0u8; HOOK_ADDR]; // one byte too short for the 5-byte read
        assert!(verify_hook(&short).is_err(), "short ROM must Err, not panic");
    }
}
