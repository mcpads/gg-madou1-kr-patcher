//! JP-native renderer re-base (M1). Adapts the EN-based ko_dispatch/ko_render
//! (`patch/dispatch.rs`, `patch/render.rs`) to hook the JP native text engine
//! instead of the EN VWF engine. See
//! `docs/reverse_engineering/jp_native_renderer_findings.md`.
//!
//! Key JP-vs-EN differences (findings §4, verified in-emulator):
//! - Hook at $238A → `JP $28A9` (ko_dispatch). Code lives at $28A9, NOT $2889:
//!   the game's copy routine ($2541/$260D) reads $2889-$28A8 as data and DMAs
//!   it to VRAM (a read breakpoint confirmed this), so $2889-$28A8 must stay the
//!   original NOP buffer. ko_dispatch reserves $F0-$F2 for KO and sends every
//!   other byte through the original JP fixed-tile/dakuten split. JP text uses
//!   the native $00-$EF range, including $80-$82, so those old KO prefixes are
//!   not safe on the JP-direct path.
//! - ko_render owns $DFDC/$DFE6/$DFE8 (JP game does not touch them); no EN VWF
//!   coupling. Drops the $DF90 compose-buffer clear (EN-only), sets $DFE6=$80
//!   (session bit only).
//! - Return path: ko_render does NOT restore slot 2 itself — it runs in slot 2
//!   (bank 32), so `LD ($FFFF),A` there would unmap it mid-execution → crash.
//!   It jumps to a bank-0 return stub ($0028, in JP's unused RST-vector NOPs)
//!   which restores slot 2, loads the tile, and `JP $2392` (JP native nametable
//!   write `LD (HL),A; INC HL; LD (HL),$10; INC HL; INC BC`).

use z80::{
    AluOperation as AluOp, Assembler, AssemblerError, ByteOperand as Reg8, Condition as Cond,
    IndexRegister as IndexReg, Instruction, Register16 as Reg16, StackRegister as Reg16Stack,
};

use crate::patch::constants::*;

/// JP native basic-char render entry ($238F: `LD A,$54; ADD A,D; ...`).
pub const JP_NATIVE_BASIC_ADDR: u16 = 0x238F;
/// JP native dakuten-compositing entry for character bytes >= $86.
pub const JP_NATIVE_DAKUTEN_ADDR: u16 = 0x23A6;
/// JP native nametable write ($2392: `LD (HL),A; INC HL; LD (HL),$10; INC HL; INC BC`).
pub const JP_NAMETABLE_WRITE_ADDR: u16 = 0x2392;
/// JP-direct KO prefixes. The active bank 3/5 JP text corpus contains none of
/// $F0-$F2, while $80-$82 are common native character bytes.
pub const JP_KO_PREFIX_START: u8 = 0xF0;
pub const JP_KO_PREFIX_END: u8 = 0xF2;
/// KO dynamic tile base hypothesis (EN precedent; verified empirically).
pub const KO_TILE_BASE: u8 = 0x9F;
/// Battle shared-window allocation used when nametable low byte is $CE.
pub const KO_SHARED_TILE_BASE: u8 = 0x70;
pub const KO_SHARED_TILE_COUNTER: u8 = 0x0E;

/// Shared window-clear body used by both the simple ($218C) and speech-bubble
/// ($2191) entry points. The original first three bytes are
/// `PUSH AF; PUSH BC; PUSH DE`; replacing them with a JP lets one reset hook
/// cover both window types.
pub const JP_WINDOW_CLEAR_HOOK_ADDR: u16 = 0x2194;
pub const JP_WINDOW_CLEAR_CONTINUE_ADDR: u16 = 0x2197;
pub const JP_WINDOW_CLEAR_ORIG: [u8; 3] = [0xF5, 0xC5, 0xD5];

/// Reset stub address in JP's unused $0018-$0037 RST-vector NOP region.
/// Keeping the original dakuten renderer at $23A6 intact lets untranslated JP
/// text coexist with brute-test or future selectively translated KR entries.
pub const JP_WINDOW_RESET_STUB_ADDR: u16 = 0x0018;
pub const JP_WINDOW_RESET_STUB_ORIG: [u8; 15] = [0x00; 15];

/// Patch at $2194: jump from the shared window-clear body to the reset stub.
pub fn assemble_jp_window_clear_patch() -> Vec<u8> {
    let target = JP_WINDOW_RESET_STUB_ADDR;
    vec![0xC3, (target & 0xFF) as u8, (target >> 8) as u8]
}

/// Reset $DFE6/$DFE8 once whenever a text window is cleared, then replay the
/// three overwritten PUSH instructions and continue at $2197.
///
/// AF is saved around XOR/LD so the clear routine sees exactly the register
/// and flag state it saw before the hook. BC/DE are not touched before their
/// original pushes are replayed.
pub fn assemble_jp_window_reset_stub() -> Result<Vec<u8>, AssemblerError> {
    let mut asm = Assembler::new();

    asm.emit(Instruction::Push(Reg16Stack::Af));
    asm.emit(Instruction::AluR(AluOp::Xor, Reg8::A));
    asm.emit(Instruction::LdAddrA(VWF_STATE));
    asm.emit(Instruction::LdAddrA(VWF_TILE_COUNTER));
    asm.emit(Instruction::Pop(Reg16Stack::Af));

    // Replay the original $2194-$2196 instructions.
    asm.emit(Instruction::Push(Reg16Stack::Af));
    asm.emit(Instruction::Push(Reg16Stack::Bc));
    asm.emit(Instruction::Push(Reg16Stack::De));
    asm.emit(Instruction::Jp(JP_WINDOW_CLEAR_CONTINUE_ADDR));

    asm.assemble(JP_WINDOW_RESET_STUB_ADDR)
        .map(|program| program.into_bytes())
}

/// Return stub address in the same unused JP RST-vector NOP region. It is
/// aligned to the $0028 vector boundary and stays below the live $0038 IRQ
/// handler.
/// ko_render must restore slot 2 from **bank 0** (always mapped) — restoring it
/// inside ko_render (bank 32, slot 2) would unmap ko_render mid-execution.
pub const JP_RETURN_STUB_ADDR: u16 = 0x0028;
pub const JP_RETURN_STUB_ORIG: [u8; 7] = [0x00; 7];
/// Original JP fixed-tile/dakuten split, moved to the final 9 bytes before the
/// live $0038 IRQ handler so ko_dispatch still fits in $28A9-$28C8.
pub const JP_NATIVE_DISPATCH_STUB_ADDR: u16 = 0x002F;
pub const JP_NATIVE_DISPATCH_STUB_ORIG: [u8; 9] = [0x00; 9];

/// Bank-0 return stub (7 bytes) at $0028: restore slot 2, load tile, JP $2392.
/// Entry: A = slot-2 bank value, D = tile index.
pub fn assemble_jp_return_stub() -> Vec<u8> {
    // LD ($FFFF),A ; LD A,D ; JP $2392
    vec![0x32, 0xFF, 0xFF, 0x7A, 0xC3, 0x92, 0x23]
}

/// Bank-0 native fallback (9 bytes) at $002F. Entry: D = JP character byte.
pub fn assemble_jp_native_dispatch_stub() -> Result<Vec<u8>, AssemblerError> {
    let mut asm = Assembler::new();
    asm.emit(Instruction::LdRImm(Reg8::A, 0x85));
    asm.emit(Instruction::AluR(AluOp::Cp, Reg8::D));
    asm.emit(Instruction::JpCond(Cond::C, JP_NATIVE_DAKUTEN_ADDR));
    asm.emit(Instruction::Jp(JP_NATIVE_BASIC_ADDR));
    asm.assemble(JP_NATIVE_DISPATCH_STUB_ADDR)
        .map(|program| program.into_bytes())
}

/// ko_dispatch code address. $2889-$28A8 (32B) is read as **data** by the game
/// (copy routine at $2541/$260D DMAs it to VRAM — a read breakpoint confirmed
/// this), so code must live at $28A9+ (not read by that routine), leaving
/// $2889-$28A8 as the original NOP buffer.
pub const KO_DISPATCH_JP_ADDR: u16 = 0x28A9;

/// Dispatch patch at $238A (5 bytes): `JP $28A9` + 2× NOP.
/// Replaces the JP `LD A,$85; CP D; JR C,$17` (`3E 85 BA 38 17`).
pub fn assemble_jp_dispatch_patch() -> Vec<u8> {
    let target = KO_DISPATCH_JP_ADDR;
    vec![0xC3, (target & 0xFF) as u8, (target >> 8) as u8, 0x00, 0x00]
}

/// Original 5 bytes at $238A on the JP ROM (for `Expect::Bytes`).
pub const JP_DISPATCH_ORIG: [u8; 5] = [0x3E, 0x85, 0xBA, 0x38, 0x17];

/// ko_dispatch at $28A9 (no sled — $2889-$28A8 is a game-read data buffer).
///
/// - D in $F0-$F2 → KO path (save slot2, read 2nd byte, bank 32, JP $8000).
///   Those bytes do not occur in the active JP bank 3/5 corpus.
/// - all other D → reproduce the original JP split: D <= $85 uses the fixed
///   tile path at $238F; D >= $86 uses dakuten compositing at $23A6.
pub fn assemble_jp_ko_dispatch() -> Result<Vec<u8>, AssemblerError> {
    let mut asm = Assembler::new();

    // === classify D ===
    asm.emit(Instruction::LdRR(Reg8::A, Reg8::D)); // LD A, D
    asm.emit(Instruction::AluImm(AluOp::Sub, JP_KO_PREFIX_START));
    asm.emit(Instruction::AluImm(
        AluOp::Cp,
        JP_KO_PREFIX_END - JP_KO_PREFIX_START + 1,
    ));
    asm.emit(Instruction::JpCond(Cond::Nc, JP_NATIVE_DISPATCH_STUB_ADDR));

    // === KO path ($F0-$F2) ===
    asm.emit(Instruction::LdAAddr(0xFFFF)); // LD A, ($FFFF)
    asm.emit(Instruction::LdAddrA(RAM_SLOT2_SAVE)); // LD ($DFD1), A
    asm.emit(Instruction::IncRR(Reg16::Bc)); // INC BC
    asm.emit(Instruction::LdABC); // LD A, (BC)
    asm.emit(Instruction::LdAddrA(RAM_BYTE2)); // LD ($DFD8), A
    asm.emit(Instruction::LdRImm(Reg8::A, BANK_KO_RENDER)); // LD A, 32
    asm.emit(Instruction::LdAddrA(0xFFFF)); // LD ($FFFF), A
    asm.emit(Instruction::Jp(KO_RENDER_LOGICAL)); // JP $8000

    asm.assemble(KO_DISPATCH_JP_ADDR)
        .map(|program| program.into_bytes())
}

/// ko_render at $8000 (Bank 32). JP-native re-base of `patch/render.rs`.
pub fn assemble_jp_ko_render() -> Result<Vec<u8>, AssemblerError> {
    let mut asm = Assembler::new();

    // --- Phase 1: save registers + slot 1 ---
    asm.emit(Instruction::LdAddrRR(RAM_TEXT_PTR, Reg16::Bc)); // LD ($DFE0), BC
    asm.emit(Instruction::LdAddrHL(RAM_HL_SAVE)); // LD ($DFE2), HL
    asm.emit(Instruction::LdAddrRR(RAM_DE_SAVE, Reg16::De)); // LD ($DFE4), DE
    asm.emit(Instruction::LdAAddr(0xFFFE)); // LD A, ($FFFE)
    asm.emit(Instruction::LdAddrA(RAM_SLOT1_SAVE)); // LD ($DFD0), A

    // --- Phase 2: session check + window-specific tile allocation ---
    asm.emit(Instruction::LdAAddr(VWF_STATE)); // LD A, ($DFE6)
    asm.emit(Instruction::Bit(7, Reg8::A)); // BIT 7, A
    asm.jr_cond(Cond::Nz, "session_ok");

    // JP battle runtime uses nametable L=$CE for the shared action/reaction
    // window, matching the EN engine's allocation contract. Keep that window
    // on base $70 after the 14 pre-existing tiles; all other windows use $9F.
    asm.emit(Instruction::LdRR(Reg8::A, Reg8::L));
    asm.emit(Instruction::AluImm(AluOp::Cp, 0xCE));
    asm.jr_cond(Cond::Nz, "base_9f");
    asm.emit(Instruction::LdRImm(Reg8::A, KO_SHARED_TILE_COUNTER));
    asm.emit(Instruction::LdAddrA(VWF_TILE_COUNTER));
    asm.emit(Instruction::LdRImm(Reg8::A, KO_SHARED_TILE_BASE));
    asm.emit(Instruction::LdAddrA(VWF_TILE_BASE));
    asm.jr("session_ok");

    asm.label("base_9f");
    asm.emit(Instruction::AluR(AluOp::Xor, Reg8::A)); // XOR A
    asm.emit(Instruction::LdAddrA(VWF_TILE_COUNTER)); // LD ($DFE8), A  (counter 0)
    asm.emit(Instruction::LdRImm(Reg8::A, KO_TILE_BASE)); // LD A, $9F
    asm.emit(Instruction::LdAddrA(VWF_TILE_BASE)); // LD ($DFDC), A
    asm.label("session_ok");

    // --- Phase 3: allocate tile (pre-increment) ---
    // A single JP text session can span many PAGE-delimited prologue screens.
    // If base+counter carries, the 8-bit tile index would otherwise wrap into
    // fixed graphics: in third:212, base $70 + counter $90 wrote the `유`
    // glyph into tile $00, turning every native $00 blank cell into `유`.
    // Recycle the current window's dynamic band instead. A visible text page
    // is much shorter than either band, so old page tiles are no longer live
    // when a long session reaches this boundary.
    asm.emit(Instruction::LdRRImm(Reg16::Hl, VWF_TILE_COUNTER)); // LD HL, $DFE8
    asm.label("allocate_tile");
    asm.emit(Instruction::IncR(Reg8::IndirectHl)); // INC (HL)
    asm.emit(Instruction::LdAAddr(VWF_TILE_BASE)); // LD A, ($DFDC)
    asm.emit(Instruction::AluR(AluOp::Add, Reg8::IndirectHl)); // ADD A, (HL)
    asm.jr_cond(Cond::Nc, "tile_ready");

    // Carry means the next tile would be $00. Restore the counter value used
    // immediately before each band's first allocation, then retry the normal
    // pre-increment path: $0E -> $7F for base $70, 0 -> $A0 for base $9F.
    asm.emit(Instruction::LdAAddr(VWF_TILE_BASE));
    asm.emit(Instruction::AluImm(AluOp::Cp, KO_SHARED_TILE_BASE));
    asm.jr_cond(Cond::Z, "reset_shared_counter");
    asm.emit(Instruction::AluR(AluOp::Xor, Reg8::A)); // default counter 0
    asm.jr("reset_tile_counter");
    asm.label("reset_shared_counter");
    asm.emit(Instruction::LdRImm(Reg8::A, KO_SHARED_TILE_COUNTER));
    asm.label("reset_tile_counter");
    asm.emit(Instruction::LdRR(Reg8::IndirectHl, Reg8::A));
    asm.jr("allocate_tile");

    asm.label("tile_ready");
    asm.emit(Instruction::Push(Reg16Stack::Af)); // PUSH AF (tile index)

    // --- Phase 4: VRAM address = tile × 32 ---
    asm.emit(Instruction::LdRImm(Reg8::H, 0)); // LD H, 0
    asm.emit(Instruction::LdRR(Reg8::L, Reg8::A)); // LD L, A
    for _ in 0..5 {
        asm.emit(Instruction::AddHLRR(Reg16::Hl)); // ADD HL, HL ×5
    }
    asm.emit(Instruction::Push(Reg16Stack::Hl)); // PUSH HL (VRAM addr)

    // --- Phase 5: glyph source = syllable × 32, pick bank 33/34 ---
    asm.emit(Instruction::LdRR(Reg8::A, Reg8::D)); // LD A, D
    asm.emit(Instruction::AluImm(AluOp::Sub, JP_KO_PREFIX_START));
    asm.emit(Instruction::LdRR(Reg8::H, Reg8::A)); // LD H, A
    asm.emit(Instruction::LdAAddr(RAM_BYTE2)); // LD A, ($DFD8)
    asm.emit(Instruction::LdRR(Reg8::L, Reg8::A)); // LD L, A
    for _ in 0..5 {
        asm.emit(Instruction::AddHLRR(Reg16::Hl)); // ADD HL, HL ×5
    }
    asm.emit(Instruction::LdRImm(Reg8::A, BANK_FONT_START)); // LD A, 33
    asm.emit(Instruction::Bit(6, Reg8::H));
    asm.jr_cond(Cond::Z, "bank_ok");
    asm.emit(Instruction::LdRImm(Reg8::A, BANK_FONT_START + 1)); // LD A, 34
    asm.emit(Instruction::Res(6, Reg8::H));
    asm.label("bank_ok");
    asm.emit(Instruction::LdAddrA(0xFFFE)); // LD ($FFFE), A  (slot1 → font bank)
    asm.emit(Instruction::Set(6, Reg8::H)); // SET 6, H  ($4000 base)

    // --- Phase 6: VRAM transfer (32 bytes) ---
    asm.emit(Instruction::Pop(Reg16Stack::De)); // POP DE (VRAM addr)
    asm.emit(Instruction::Push(Reg16Stack::Hl)); // PUSH HL (glyph src)
    asm.emit(Instruction::LdRR(Reg8::H, Reg8::D));
    asm.emit(Instruction::LdRR(Reg8::L, Reg8::E));
    asm.emit(Instruction::Di);
    asm.emit(Instruction::LdRR(Reg8::A, Reg8::L));
    asm.emit(Instruction::OutPortA(VDP_CTRL));
    asm.emit(Instruction::LdRR(Reg8::A, Reg8::H));
    asm.emit(Instruction::AluImm(AluOp::Or, 0x40));
    asm.emit(Instruction::OutPortA(VDP_CTRL));
    asm.emit(Instruction::Pop(Reg16Stack::Hl)); // POP HL (glyph src)
    asm.emit(Instruction::LdRImm(Reg8::B, 32));
    asm.emit(Instruction::LdRImm(Reg8::C, VDP_DATA));
    asm.label("outi_loop");
    asm.emit(Instruction::PushIx(IndexReg::Ix)); // delay
    asm.emit(Instruction::PopIx(IndexReg::Ix)); // delay
    asm.emit(Instruction::Outi);
    asm.jr_cond(Cond::Nz, "outi_loop");
    asm.emit(Instruction::Ei);

    // --- Phase 7: session state (JP: no $DF90 clear; session bit only) ---
    asm.emit(Instruction::LdRImm(Reg8::A, 0x80)); // LD A, $80  (SESSION)
    asm.emit(Instruction::LdAddrA(VWF_STATE)); // LD ($DFE6), A

    // --- Phase 8: restore + hand off to bank-0 return stub ---
    // slot 2 is NOT restored here: doing so would unmap ko_render (bank 32,
    // slot 2) mid-execution. Instead jump to the $0028 stub (bank 0) which
    // restores slot 2 and writes the tile, all from an always-mapped bank.
    asm.emit(Instruction::Pop(Reg16Stack::Af)); // POP AF (tile index)
    asm.emit(Instruction::LdRR(Reg8::D, Reg8::A)); // LD D, A (tile)
    asm.emit(Instruction::LdAAddr(RAM_DE_SAVE)); // LD A, ($DFE4)
    asm.emit(Instruction::LdRR(Reg8::E, Reg8::A)); // LD E, A
    asm.emit(Instruction::LdRRAddr(Reg16::Bc, RAM_TEXT_PTR)); // LD BC, ($DFE0)
    asm.emit(Instruction::LdHLAddr(RAM_HL_SAVE)); // LD HL, ($DFE2)
    asm.emit(Instruction::LdAAddr(RAM_SLOT1_SAVE)); // LD A, ($DFD0)
    asm.emit(Instruction::LdAddrA(0xFFFE)); // LD ($FFFE), A  (restore slot1 — safe, we run in slot2)
    asm.emit(Instruction::LdAAddr(RAM_SLOT2_SAVE)); // slot2 value for the stub
    asm.emit(Instruction::Jp(JP_RETURN_STUB_ADDR)); // JP $0028 (bank-0 stub)

    asm.assemble(KO_RENDER_LOGICAL)
        .map(|program| program.into_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dispatch_patch_targets_28a9() {
        let p = assemble_jp_dispatch_patch();
        assert_eq!(p, vec![0xC3, 0xA9, 0x28, 0x00, 0x00]); // JP $28A9 + NOP NOP
        assert_eq!(p.len(), JP_DISPATCH_ORIG.len(), "must fill the 5-byte slot");
    }

    #[test]
    fn ko_dispatch_fits_free_region() {
        // Code lives at $28A9; the game reads $2889-$28A8 as data, and $28C9+
        // has data, so the code must fit in $28A9-$28C8 (32 bytes).
        let d = assemble_jp_ko_dispatch().expect("assemble");
        assert!(
            d.len() <= 0x28C9 - (KO_DISPATCH_JP_ADDR as usize),
            "ko_dispatch overruns $28A9-$28C8: {} bytes",
            d.len()
        );
        assert_eq!(d[0], 0x7A, "code should start with LD A,D");
    }

    #[test]
    fn ko_render_ends_with_bank0_return_stub() {
        let r = assemble_jp_ko_render().expect("assemble");
        // Must hand off via JP $0028 (C3 28 00) — restoring slot2 inside
        // ko_render (bank 32) would unmap it mid-execution.
        assert_eq!(
            &r[r.len() - 3..],
            &[0xC3, 0x28, 0x00],
            "must end with JP $0028"
        );
    }

    #[test]
    fn ko_render_has_battle_shared_window_allocation() {
        let r = assemble_jp_ko_render().expect("assemble");
        assert!(
            r.windows(10).any(|w| {
                w == [
                    0x3E, 0x0E, // LD A,$0E
                    0x32, 0xE8, 0xDF, // LD ($DFE8),A
                    0x3E, 0x70, // LD A,$70
                    0x32, 0xDC, 0xDF, // LD ($DFDC),A
                ]
            }),
            "$CE battle window must use base $70 with counter $0E"
        );
        assert!(
            r.windows(9).any(|w| {
                w == [
                    0xAF, // XOR A
                    0x32, 0xE8, 0xDF, // LD ($DFE8),A
                    0x3E, 0x9F, // LD A,$9F
                    0x32, 0xDC, 0xDF, // LD ($DFDC),A
                ]
            }),
            "non-$CE windows must use base $9F with counter 0"
        );
    }

    #[test]
    fn ko_render_wraps_before_tile_zero() {
        let r = assemble_jp_ko_render().expect("assemble");

        assert!(
            r.windows(6).any(|w| {
                w[0] == 0x86 // ADD A,(HL)
                    && w[1] == 0x30 // JR NC,tile_ready
                    && w[3..] == [0x3A, 0xDC, 0xDF] // LD A,($DFDC)
            }),
            "tile allocation must branch around the wrap handler when no carry occurs"
        );
        assert!(
            r.windows(12).any(|w| {
                w[0..2] == [0xFE, 0x70] // CP $70
                    && w[2] == 0x28 // JR Z,reset_shared_counter
                    && w[4] == 0xAF // XOR A (default counter 0)
                    && w[5] == 0x18 // JR reset_tile_counter
                    && w[7..9] == [0x3E, 0x0E] // LD A,$0E
                    && w[9] == 0x77 // LD (HL),A
                    && w[10] == 0x18 // JR allocate_tile
            }),
            "carry handler must recycle base $70 at counter $0E and base $9F at counter 0"
        );
    }

    #[test]
    fn return_stub_is_slot2_restore_then_write() {
        // LD ($FFFF),A ; LD A,D ; JP $2392
        assert_eq!(
            assemble_jp_return_stub(),
            vec![0x32, 0xFF, 0xFF, 0x7A, 0xC3, 0x92, 0x23]
        );
    }

    #[test]
    fn ko_dispatch_reserves_f0_f2_and_preserves_native_dakuten() {
        let d = assemble_jp_ko_dispatch().expect("assemble");
        assert!(
            d.windows(8)
                .any(|w| w == [0x7A, 0xD6, 0xF0, 0xFE, 0x03, 0xD2, 0x2F, 0x00]),
            "dispatch must classify only $F0-$F2 as KO"
        );
        let native = assemble_jp_native_dispatch_stub().expect("assemble");
        assert!(
            native
                .windows(6)
                .any(|w| w == [0x3E, 0x85, 0xBA, 0xDA, 0xA6, 0x23]),
            "non-KO bytes must retain the original D >= $86 dakuten branch"
        );
        assert_eq!(native.len(), JP_NATIVE_DISPATCH_STUB_ORIG.len());
    }

    #[test]
    fn window_clear_patch_targets_reset_stub() {
        let p = assemble_jp_window_clear_patch();
        assert_eq!(p, vec![0xC3, 0x18, 0x00]);
        assert_eq!(p.len(), JP_WINDOW_CLEAR_ORIG.len());
    }

    #[test]
    fn window_reset_stub_preserves_state_and_replays_overwrite() {
        let stub = assemble_jp_window_reset_stub().expect("assemble");
        assert_eq!(
            stub,
            vec![
                0xF5, // PUSH AF (preserve caller state)
                0xAF, // XOR A
                0x32, 0xE6, 0xDF, // LD ($DFE6),A
                0x32, 0xE8, 0xDF, // LD ($DFE8),A
                0xF1, // POP AF
                0xF5, 0xC5, 0xD5, // replay PUSH AF/BC/DE
                0xC3, 0x97, 0x21, // JP $2197
            ]
        );
        assert!(
            JP_WINDOW_RESET_STUB_ADDR as usize + stub.len() <= JP_RETURN_STUB_ADDR as usize,
            "reset stub must not overlap the $0028 return stub"
        );
        assert_eq!(stub.len(), JP_WINDOW_RESET_STUB_ORIG.len());
        assert_eq!(JP_RETURN_STUB_ADDR, 0x0028);
        assert!(
            JP_RETURN_STUB_ADDR as usize + JP_RETURN_STUB_ORIG.len()
                <= JP_NATIVE_DISPATCH_STUB_ADDR as usize,
            "return stub must not overlap the native dispatch stub"
        );
        assert_eq!(JP_NATIVE_DISPATCH_STUB_ADDR, 0x002F);
        assert_eq!(
            JP_NATIVE_DISPATCH_STUB_ADDR as usize + JP_NATIVE_DISPATCH_STUB_ORIG.len(),
            0x0038,
            "native dispatch stub must end exactly before the live IRQ handler"
        );
    }
}
