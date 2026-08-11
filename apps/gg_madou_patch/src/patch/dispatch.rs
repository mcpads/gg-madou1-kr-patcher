use z80::{
    AluOperation as AluOp, Assembler, AssemblerError, ByteOperand as Reg8, Condition as Cond,
    Instruction, Register16 as Reg16,
};

use super::constants::*;

// ---------------------------------------------------------------------------
// Dispatch patch — 7 bytes at $238A
// ---------------------------------------------------------------------------

/// Assemble the 7-byte patch at $238A that redirects to ko_dispatch.
///
/// Replaces the original `LD A,$54 / ADD A,D` sequence with
/// `JP $2889` plus NOP padding to fill the original 7-byte slot.
pub fn assemble_dispatch_patch() -> Vec<u8> {
    // JP nn = 0xC3, lo, hi (3 bytes)
    let target = KO_DISPATCH_ADDR as u16;
    let mut bytes = vec![0xC3, (target & 0xFF) as u8, (target >> 8) as u8];
    // NOP-pad to fill the original 7-byte region
    bytes.extend(vec![0x00; DISPATCH_PATCH_SIZE - 3]);
    bytes
}

// ---------------------------------------------------------------------------
// ko_dispatch — 64 bytes at $2889
// ---------------------------------------------------------------------------

/// Assemble ko_dispatch at $2889 (64 bytes: 32-byte NOP sled + dispatch logic).
///
/// Layout:
/// - $2889-$28A8: 32 × NOP (zero-init buffer used by the game's copy routine)
/// - $28A9+: dispatch logic
///   - `LD A,D`; `CP $80`; `JR C, en_path`   — D < $80 → EN path
///   - KO path: save slot2, read 2nd byte, switch to Bank 32, `JP $8000`
///   - EN path: `LD A,$54`; `ADD A,D`; `LD D,A`; `JP $23A6`
pub fn assemble_ko_dispatch() -> Result<Vec<u8>, AssemblerError> {
    let mut asm = Assembler::new();

    // 32-byte NOP sled at $2889-$28A8
    asm.emit_all(std::iter::repeat_n(Instruction::Nop, 32));

    // === Dispatch logic starts at $28A9 ===
    // Input: D = current text byte

    // LD A, D
    asm.emit(Instruction::LdRR(Reg8::A, Reg8::D));
    // CP $80
    asm.emit(Instruction::AluImm(AluOp::Cp, 0x80));
    // JR C, en_path  (D < $80 → EN)
    asm.jr_cond(Cond::C, "en_path");

    // === KO path (D >= $80) ===

    // Save slot 2 bank: LD A, ($FFFF); LD ($DFD1), A
    asm.emit(Instruction::LdAAddr(0xFFFF));
    asm.emit(Instruction::LdAddrA(RAM_SLOT2_SAVE));

    // Read 2nd byte (must happen before bank switch, text still mapped).
    // INC BC so BC points at the 2nd byte, then LD A, (BC).
    asm.emit(Instruction::IncRR(Reg16::Bc));
    asm.emit(Instruction::LdABC); // LD A, (BC) = 0x0A
    asm.emit(Instruction::LdAddrA(RAM_BYTE2)); // LD ($DFD8), A

    // Switch Slot 2 to Bank 32 (ko_render)
    asm.emit(Instruction::LdRImm(Reg8::A, BANK_KO_RENDER)); // LD A, 32
    asm.emit(Instruction::LdAddrA(0xFFFF)); // LD ($FFFF), A

    // JP $8000 (ko_render logical address)
    asm.emit(Instruction::Jp(KO_RENDER_LOGICAL));

    // === EN path ===
    asm.label("en_path");

    // Replicate the original $238A code: A = D + $54, put back in D
    asm.emit(Instruction::LdRImm(Reg8::A, 0x54)); // LD A, $54
    asm.emit(Instruction::AluR(AluOp::Add, Reg8::D)); // ADD A, D
    asm.emit(Instruction::LdRR(Reg8::D, Reg8::A)); // LD D, A

    // Fall through to $23A6 (EN bank-switch + VWF)
    asm.emit(Instruction::Jp(EN_BANK_SWITCH_ADDR)); // JP $23A6

    asm.assemble(KO_DISPATCH_ADDR as u16)
        .map(|program| program.into_bytes())
}
