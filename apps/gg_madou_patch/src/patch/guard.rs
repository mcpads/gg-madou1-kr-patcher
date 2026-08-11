use z80::{
    AluOperation as AluOp, Assembler, AssemblerError, ByteOperand as Reg8, Condition as Cond,
    Instruction, Register16 as Reg16,
};

use super::constants::*;

// ---------------------------------------------------------------------------
// EN read-ahead guard — 45 bytes at $23A6
// ---------------------------------------------------------------------------

/// Assemble the EN read-ahead guard at $23A6.
///
/// The guard replaces the original bank-switch + VWF-jump code ($23A6-$23D2).
/// It intercepts the VWF engine's read-ahead byte and replaces any Korean
/// prefix byte (≥ $80) with $48 (wide space, width=8).  This forces the VWF
/// engine to finish any pending compositing before the next character is
/// processed as Korean, preventing corrupted font look-ups.
///
/// Sequence:
/// 1. Save BC (text pointer) to $DFE0.
/// 2. INC BC; LD A,(BC) — peek next byte.
/// 3. CP $80; JR C, normal_peek — if ≥ $80, LD A,$48 (wide space).
/// 4. Save peek byte to $DFD8.
/// 5. Save HL to $DFE2.
/// 6. Save DE to $DFE4.
/// 7. LD HL,($FFFE) + LD ($DFD0),HL — save slot 1 & 2 banks in one shot.
/// 8. LD A,8; LD ($FFFF),A — switch slot 2 to Bank 8 (VWF engine).
/// 9. JP $B2C0 — enter VWF engine.
///
/// The VWF engine returns to $23CA which does `LD ($FFFF),A; JP $2391`.
pub fn assemble_en_guard() -> Result<Vec<u8>, AssemblerError> {
    let mut asm = Assembler::new();

    // Step 1: save BC (text pointer)
    // LD ($DFE0), BC                                     [ED 43 E0 DF] = 4B
    asm.emit(Instruction::LdAddrRR(RAM_TEXT_PTR, Reg16::Bc));

    // Step 2: peek next byte
    // INC BC                                             [03]          = 1B
    asm.emit(Instruction::IncRR(Reg16::Bc));
    // LD A, (BC)  = 0x0A                                 [0A]          = 1B
    asm.emit(Instruction::LdABC);

    // Step 3: Korean prefix guard
    // CP $80                                             [FE 80]       = 2B
    asm.emit(Instruction::AluImm(AluOp::Cp, 0x80));
    // JR C, normal_peek  (A < $80 → OK)
    asm.jr_cond(Cond::C, "normal_peek");
    // LD A, $48  — replace with wide-space               [3E 48]       = 2B
    asm.emit(Instruction::LdRImm(Reg8::A, 0x48));

    asm.label("normal_peek");

    // Step 4: save peek byte
    // LD ($DFD8), A                                      [32 D8 DF]    = 3B
    asm.emit(Instruction::LdAddrA(RAM_BYTE2));

    // Step 5: save HL
    // LD ($DFE2), HL                                     [22 E2 DF]    = 3B
    asm.emit(Instruction::LdAddrHL(RAM_HL_SAVE));

    // Step 6: save DE
    // LD ($DFE4), DE                                     [ED 53 E4 DF] = 4B
    asm.emit(Instruction::LdAddrRR(RAM_DE_SAVE, Reg16::De));

    // Step 7: save slot 1 and slot 2 banks atomically.
    // LD HL, ($FFFE) — L = mem[$FFFE]=slot1, H = mem[$FFFF]=slot2
    // LD ($DFD0), HL — stores L→$DFD0, H→$DFD1
    // LD HL, ($FFFE)                                     [2A FE FF]    = 3B
    asm.emit(Instruction::LdHLAddr(0xFFFE));
    // LD ($DFD0), HL                                     [22 D0 DF]    = 3B
    asm.emit(Instruction::LdAddrHL(RAM_SLOT1_SAVE));

    // Step 8: switch slot 2 to Bank 8
    // LD A, 8                                            [3E 08]       = 2B
    asm.emit(Instruction::LdRImm(Reg8::A, BANK_VWF_ENGINE));
    // LD ($FFFF), A                                      [32 FF FF]    = 3B
    asm.emit(Instruction::LdAddrA(0xFFFF));

    // Step 9: jump to VWF engine
    // JP $B2C0                                           [C3 C0 B2]    = 3B
    asm.emit(Instruction::Jp(VWF_ENGINE_ADDR));

    // Step 10: return sequence (VWF engine and ko_render both return here)
    // This MUST be at the address ENGINE_RETURN_ADDR ($23CA).
    // LD ($FFFF), A — restore slot 2 bank                [32 FF FF]    = 3B
    asm.emit(Instruction::LdAddrA(0xFFFF));
    // JP $2391 — resume text engine                      [C3 91 23]    = 3B
    asm.emit(Instruction::Jp(0x2391));

    asm.assemble(EN_BANK_SWITCH_ADDR)
        .map(|program| program.into_bytes())
}
