//! Phase 3b Task 2 (+ Phase 3c): Z80 wiring for relocated text tables.
//!
//! Task 1 (`text::relocation`) moved bank 3's three tables and bank 5's two
//! tables into spare banks (pointer-table overflow fix, build green). The
//! game's text-window and sprite lookup routines still hard-code CALL/JR
//! targets that read banks 3/5 directly, so the relocated tables are never
//! seen at runtime until those entry points are redirected.
//!
//! This module implements the wiring from
//! docs/reverse_engineering/phase3b_z80_wiring.md, plus Phase 3c's stub D/E
//! addition (bank 3 "third" split into its own spare bank 37 for headroom —
//! see `text::relocation::RELOCATION_MAP`'s doc comment):
//! - Five 5-byte stubs (`assemble_stub_textwindow`/`assemble_stub_sprite`,
//!   generic over spare bank number) placed in reclaimed dead code, each
//!   selecting a spare bank and jumping into the original routine's
//!   bank-switch continuation.
//! - Six entry-point operand patches (`entryN_*`) that redirect existing
//!   CALL/JR instructions at the original six call sites to the new stubs,
//!   without changing instruction size. Entries #2/#5 (bank 3 "third") now
//!   target stub D/E instead of stub A/B.
//!
//! `commands::build`'s Stage 7 applies all eleven writes via
//! `TrackedRom::write_expect`, which re-verifies every original byte
//! against the real ROM at build time — the constants here only need to be
//! internally consistent, not authoritative on their own. Every stub's `LD
//! B,n` immediate is derived from `text::relocation::spare_bank_for`
//! (reading `RELOCATION_MAP`), not a hardcoded literal, so a future change
//! to the relocation map can't silently drift from the Z80 wiring.

use z80::{Assembler, AssemblerError, ByteOperand as Reg8, Instruction};

use super::constants::*;

// ---------------------------------------------------------------------------
// Stubs (design doc §Table 2): `LD B,<spare bank>; JP <continuation>`.
// ---------------------------------------------------------------------------
//
// The original $22CA (text-window) and $232B (sprite) routines both open
// with `LD B,3` (2 bytes) before falling into a shared bank-switch-and-
// restore sequence. Each stub replicates that `LD B,n` with the spare bank
// number, then jumps past it directly into the continuation — the
// continuation code itself is never touched, so its behavior (switching
// slot 2 to whatever is in B, doing the pointer lookup, switching back) is
// reused unmodified for the spare bank.

/// Text-window stub: `LD B,<spare>; JP $22CC` (5 bytes).
///
/// Used for:
/// - Stub A (spare 35) at `STUB_TEXTWINDOW_A_ADDR` ($229E) — bank3
///   main/secondary/third text-window entries (#1-#3).
/// - Stub C (spare 36) at `STUB_TEXTWINDOW_C_ADDR` ($26AA) — bank5
///   main+secondary shared-tail entry (#6).
pub fn assemble_stub_textwindow(spare_bank_number: u8) -> Result<Vec<u8>, AssemblerError> {
    assemble_bank_select_stub(spare_bank_number, TEXTWINDOW_CONTINUATION_ADDR)
}

/// Sprite stub: `LD B,<spare>; JP $232D` (5 bytes).
///
/// Used for Stub B (spare 35) at `STUB_SPRITE_B_ADDR` ($22C4) — bank3
/// secondary/third sprite entries (#4-#5).
pub fn assemble_stub_sprite(spare_bank_number: u8) -> Result<Vec<u8>, AssemblerError> {
    assemble_bank_select_stub(spare_bank_number, SPRITE_CONTINUATION_ADDR)
}

fn assemble_bank_select_stub(
    spare_bank_number: u8,
    continuation_addr: u16,
) -> Result<Vec<u8>, AssemblerError> {
    // The builder's base address is cosmetic here: both instructions are
    // emitted with absolute operands (`LdRImm`, `Jp(addr)`), not label
    // references, so it has no effect on the encoded bytes.
    let mut asm = Assembler::new();
    asm.emit(Instruction::LdRImm(Reg8::B, spare_bank_number));
    asm.emit(Instruction::Jp(continuation_addr));
    let bytes = asm.assemble(0x0000)?.into_bytes();
    debug_assert_eq!(bytes.len(), RELOCATION_STUB_SIZE);
    Ok(bytes)
}

// ---------------------------------------------------------------------------
// Entry point operand patches (design doc §Table 1) — size-preserving.
// ---------------------------------------------------------------------------

/// Little-endian bytes for a 2-byte CALL/JP address operand.
fn operand_bytes(addr: usize) -> [u8; 2] {
    let addr = addr as u16;
    [(addr & 0xFF) as u8, (addr >> 8) as u8]
}

/// Signed JR displacement byte for a JR whose displacement byte sits at
/// `disp_byte_addr` (the opcode is the byte immediately before it, and the
/// Z80 computes the jump relative to the address right after the
/// displacement byte), targeting `target_addr`.
fn jr_displacement(disp_byte_addr: usize, target_addr: usize) -> u8 {
    let next_instr_addr = disp_byte_addr as i32 + 1;
    (target_addr as i32 - next_instr_addr) as i8 as u8
}

/// #1: bank3 main text-window JR displacement @ $22C0 → stub A ($229E).
pub fn entry1_new_byte() -> u8 {
    jr_displacement(ENTRY1_BANK3_MAIN_TEXTWINDOW_JR_ADDR, STUB_TEXTWINDOW_A_ADDR)
}

/// #2: bank3 third text-window CALL operand @ $2288 → stub D ($1AD6,
/// spare 37 — Phase 3c split "third" out of stub A/spare 35).
pub fn entry2_new_bytes() -> [u8; 2] {
    operand_bytes(STUB_TEXTWINDOW_D_ADDR)
}

/// #3: bank3 secondary text-window CALL operand @ $229A → stub A ($229E).
pub fn entry3_new_bytes() -> [u8; 2] {
    operand_bytes(STUB_TEXTWINDOW_A_ADDR)
}

/// #4: bank3 secondary sprite CALL operand @ $22AC → stub B ($22C4).
pub fn entry4_new_bytes() -> [u8; 2] {
    operand_bytes(STUB_SPRITE_B_ADDR)
}

/// #5: bank3 third sprite CALL operand @ $446C (bank1 physical) → stub E
/// ($1ADB, spare 37 — Phase 3c split "third" out of stub B/spare 35).
pub fn entry5_new_bytes() -> [u8; 2] {
    operand_bytes(STUB_SPRITE_E_ADDR)
}

/// #6: bank5 main+secondary shared-tail CALL operand @ $763F (bank1
/// physical) → stub C ($26AA).
///
/// **Must be applied atomically with stub C** — see the atomicity note on
/// `ENTRY6_BANK5_SHARED_CALL_ADDR` in `constants.rs`.
pub fn entry6_new_bytes() -> [u8; 2] {
    operand_bytes(STUB_TEXTWINDOW_C_ADDR)
}
