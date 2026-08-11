use z80::{
    AluOperation as AluOp, Assembler, AssemblerError, ByteOperand as Reg8, Condition as Cond,
    IndexRegister as IndexReg, Instruction, Register16 as Reg16, StackRegister as Reg16Stack,
};

use super::constants::*;

// ---------------------------------------------------------------------------
// ko_render — placed at Bank 32 logical $8000
// ---------------------------------------------------------------------------

/// Assemble ko_render at logical address $8000 (Bank 32, physical $80000).
///
/// Renders one Korean syllable glyph to VRAM, mimicking a VWF width=8
/// character that hits the BOUNDARY case.  This design ensures seamless
/// interleaving with the EN VWF engine:
///
/// - Phase 2: session init — resets $DFE8 and initializes $DFDC=$9F when bit7=0
/// - Phase 3: pre-increments $DFE8 (VWF convention), tile = $DFDC + $DFE8
/// - Phase 6: 32-byte 4bpp glyph VRAM transfer (OUTI loop)
/// - Phase 7: clears $DF90, sets $DFE6=$C0 (SESSION + INIT/OVFL)
/// - Phase 8: restores registers, returns via JP $23CA → $2391
///
/// $DFE6=$C0 tells the EN VWF engine "tile just completed, start new one"
/// via the $B3BC (first_tile) path, which does NOT do DEC HL×2.
///
/// Entry:
/// - D  = high byte of Korean encoding (0x80–0x82)
/// - $DFD8 = low byte (second text byte), saved by ko_dispatch
/// - $DFD1 = slot-2 save, saved by ko_dispatch
/// - BC = text pointer (advanced past 2nd byte by ko_dispatch INC BC)
///
/// Exit:
/// - D  = allocated output tile index
/// - $DFE6 = $C0 (next EN char → $B3BC first_tile, no DEC HL)
/// - Returns via JP $23CA → $2391 (standard nametable write + INC HL/BC)
pub fn assemble_ko_render() -> Result<Vec<u8>, AssemblerError> {
    let mut asm = Assembler::new();

    // ------------------------------------------------------------------
    // Phase 1: save registers + slot 1
    // ------------------------------------------------------------------
    asm.emit(Instruction::LdAddrRR(RAM_TEXT_PTR, Reg16::Bc)); // LD ($DFE0), BC
    asm.emit(Instruction::LdAddrHL(RAM_HL_SAVE)); // LD ($DFE2), HL
    asm.emit(Instruction::LdAddrRR(RAM_DE_SAVE, Reg16::De)); // LD ($DFE4), DE
    asm.emit(Instruction::LdAAddr(0xFFFE)); // LD A, ($FFFE)
    asm.emit(Instruction::LdAddrA(RAM_SLOT1_SAVE)); // LD ($DFD0), A

    // ------------------------------------------------------------------
    // Phase 2: VWF session check + tile base init
    // ------------------------------------------------------------------
    // On session start (bit7=0) select the tile base + counter from the
    // nametable low byte (L), replicating the game's $B416 init table. This
    // is what keeps coexisting windows on DISTINCT tiles.
    //
    // Battle draws two text windows in one turn — Arle's spell name (right,
    // nametable low $CE) and the monster's reaction (left, low $BC). $B416
    // maps $CE → base $70 / counter $0E ("shared window"), everything else →
    // base $9F / counter 0. Forcing $9F for BOTH (the old behavior) collapsed
    // them onto the same $A0+ tiles, so the message stomped the spell name's
    // glyph data → the spell-name box showed the message's prefix (bleed).
    // Matching $B416 puts the spell name on its own $70-based tiles ($7F+),
    // away from the $9F message tiles. Non-$CE positions keep $9F, so KO
    // dialogue (which needs the 56-tile $9F..$D6 range) is unaffected.
    asm.emit(Instruction::LdAAddr(VWF_STATE)); // LD A, ($DFE6)
    asm.emit(Instruction::Bit(7, Reg8::A)); // BIT 7, A
    asm.jr_cond(Cond::Nz, "session_ok");

    // Session start: pick base+counter from nametable low byte.
    asm.emit(Instruction::LdRR(Reg8::A, Reg8::L)); // LD A, L
    asm.emit(Instruction::AluImm(AluOp::Cp, 0xCE)); // CP $CE
    asm.jr_cond(Cond::Nz, "base_9f");

    // $CE shared window (battle spell-name box): base $70, counter $0E.
    asm.emit(Instruction::LdRImm(Reg8::A, 0x0E)); // LD A, $0E
    asm.emit(Instruction::LdAddrA(VWF_TILE_COUNTER)); // LD ($DFE8), A
    asm.emit(Instruction::LdRImm(Reg8::A, 0x70)); // LD A, $70
    asm.emit(Instruction::LdAddrA(VWF_TILE_BASE)); // LD ($DFDC), A
    asm.jr("session_ok");

    // Default: base $9F, counter 0 ($9F-$D6 = 56 safe empty tiles;
    // fits max KO text of 21 tiles well within range).
    asm.label("base_9f");
    asm.emit(Instruction::AluR(AluOp::Xor, Reg8::A)); // XOR A
    asm.emit(Instruction::LdAddrA(VWF_TILE_COUNTER)); // LD ($DFE8), A
    asm.emit(Instruction::LdRImm(Reg8::A, 0x9F)); // LD A, $9F
    asm.emit(Instruction::LdAddrA(VWF_TILE_BASE)); // LD ($DFDC), A

    asm.label("session_ok");

    // ------------------------------------------------------------------
    // Phase 3: allocate tile (VWF pre-increment convention)
    // ------------------------------------------------------------------
    // INC $DFE8 first, then tile = $DFDC + $DFE8
    // This matches VWF $B3BC/B4C5: INC before use.
    asm.emit(Instruction::LdRRImm(Reg16::Hl, VWF_TILE_COUNTER)); // LD HL, $DFE8
    asm.emit(Instruction::IncR(Reg8::IndirectHl)); // INC (HL)
    asm.emit(Instruction::LdAAddr(VWF_TILE_BASE)); // LD A, ($DFDC)
    asm.emit(Instruction::AluR(AluOp::Add, Reg8::IndirectHl)); // ADD A, (HL)
    asm.emit(Instruction::Push(Reg16Stack::Af)); // PUSH AF

    // ------------------------------------------------------------------
    // Phase 4: VRAM address = tile_index × 32
    // ------------------------------------------------------------------
    asm.emit(Instruction::LdRImm(Reg8::H, 0)); // LD H, 0
    asm.emit(Instruction::LdRR(Reg8::L, Reg8::A)); // LD L, A
    for _ in 0..5 {
        asm.emit(Instruction::AddHLRR(Reg16::Hl)); // ADD HL, HL (×32)
    }
    asm.emit(Instruction::Push(Reg16Stack::Hl)); // PUSH HL (VRAM addr)

    // ------------------------------------------------------------------
    // Phase 5: glyph source = syllable_number × 32
    // ------------------------------------------------------------------
    asm.emit(Instruction::LdRR(Reg8::A, Reg8::D)); // LD A, D
    asm.emit(Instruction::AluImm(AluOp::Sub, 0x80)); // SUB $80
    asm.emit(Instruction::LdRR(Reg8::H, Reg8::A)); // LD H, A
    asm.emit(Instruction::LdAAddr(RAM_BYTE2)); // LD A, ($DFD8)
    asm.emit(Instruction::LdRR(Reg8::L, Reg8::A)); // LD L, A
    for _ in 0..5 {
        asm.emit(Instruction::AddHLRR(Reg16::Hl)); // ADD HL, HL (×32)
    }

    // Bank selection: bit6 of H → bank 33 or 34
    asm.emit(Instruction::LdRImm(Reg8::A, BANK_FONT_START));
    asm.emit(Instruction::Bit(6, Reg8::H));
    asm.jr_cond(Cond::Z, "bank_ok");
    asm.emit(Instruction::LdRImm(Reg8::A, BANK_FONT_START + 1));
    asm.emit(Instruction::Res(6, Reg8::H));
    asm.label("bank_ok");
    asm.emit(Instruction::LdAddrA(0xFFFE)); // LD ($FFFE), A
    asm.emit(Instruction::Set(6, Reg8::H)); // SET 6, H ($4000 base)

    // ------------------------------------------------------------------
    // Phase 6: VRAM transfer (32 bytes)
    // ------------------------------------------------------------------
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
    asm.emit(Instruction::PushIx(IndexReg::Ix)); // PUSH IX (delay)
    asm.emit(Instruction::PopIx(IndexReg::Ix)); // POP IX  (delay)
    asm.emit(Instruction::Outi);
    asm.jr_cond(Cond::Nz, "outi_loop");
    asm.emit(Instruction::Ei);

    // ------------------------------------------------------------------
    // Phase 7: VWF state update
    // ------------------------------------------------------------------
    // Clear $DF90 compositing buffer (prevents stale data leaking to EN VWF)
    asm.emit(Instruction::AluR(AluOp::Xor, Reg8::A));
    asm.emit(Instruction::LdRRImm(Reg16::Hl, VWF_COMPOSE_BUF));
    asm.emit(Instruction::LdRImm(Reg8::B, 32));
    asm.label("clear_loop");
    asm.emit(Instruction::LdRR(Reg8::IndirectHl, Reg8::A));
    asm.emit(Instruction::IncRR(Reg16::Hl));
    asm.djnz("clear_loop");

    // $DFE6 = $C0: SESSION(bit7) + INIT/OVFL(bit6)
    // Mimics VWF BOUNDARY result. Next char:
    //   EN → $B3BC (first_tile): INC $DFE8, fresh composite, NO DEC HL
    //   KO → session_ok, Phase 3 pre-increment → correct tile
    asm.emit(Instruction::LdRImm(Reg8::A, 0xC0));
    asm.emit(Instruction::LdAddrA(VWF_STATE));

    // ------------------------------------------------------------------
    // Phase 8: restore and return (standard path)
    // ------------------------------------------------------------------
    asm.emit(Instruction::Pop(Reg16Stack::Af)); // POP AF (tile index)
    asm.emit(Instruction::LdRR(Reg8::D, Reg8::A)); // LD D, A

    asm.emit(Instruction::LdAAddr(RAM_DE_SAVE)); // LD A, ($DFE4)
    asm.emit(Instruction::LdRR(Reg8::E, Reg8::A)); // LD E, A
    asm.emit(Instruction::LdRRAddr(Reg16::Bc, RAM_TEXT_PTR)); // LD BC, ($DFE0)
    asm.emit(Instruction::LdHLAddr(RAM_HL_SAVE)); // LD HL, ($DFE2)

    asm.emit(Instruction::LdAAddr(RAM_SLOT1_SAVE)); // LD A, ($DFD0)
    asm.emit(Instruction::LdAddrA(0xFFFE)); // LD ($FFFE), A

    asm.emit(Instruction::LdAAddr(RAM_SLOT2_SAVE)); // LD A, ($DFD1)
    asm.emit(Instruction::Jp(ENGINE_RETURN_ADDR)); // JP $23CA

    asm.assemble(KO_RENDER_LOGICAL)
        .map(|program| program.into_bytes())
}
