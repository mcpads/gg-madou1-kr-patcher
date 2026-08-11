#[cfg(test)]
mod tests {
    #![allow(clippy::module_inception)]

    use crate::patch::{constants, dispatch, guard, relocation, render};
    use z80::{ByteOperand as Reg8, Instruction, decode_bytes};

    // -----------------------------------------------------------------------
    // dispatch patch tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_dispatch_patch_size() {
        let patch = dispatch::assemble_dispatch_patch();
        assert_eq!(
            patch.len(),
            constants::DISPATCH_PATCH_SIZE,
            "dispatch patch must be exactly {} bytes",
            constants::DISPATCH_PATCH_SIZE
        );
    }

    #[test]
    fn test_dispatch_patch_jp_opcode() {
        let patch = dispatch::assemble_dispatch_patch();
        assert_eq!(patch[0], 0xC3, "first byte must be JP opcode 0xC3");
    }

    #[test]
    fn test_dispatch_patch_target_address() {
        let patch = dispatch::assemble_dispatch_patch();
        let lo = patch[1] as u16;
        let hi = patch[2] as u16;
        let addr = lo | (hi << 8);
        assert_eq!(
            addr,
            constants::KO_DISPATCH_ADDR as u16,
            "dispatch patch JP target must point to KO_DISPATCH_ADDR"
        );
    }

    #[test]
    fn test_dispatch_patch_nop_padding() {
        let patch = dispatch::assemble_dispatch_patch();
        for (i, byte) in patch.iter().enumerate().skip(3) {
            assert_eq!(*byte, 0x00, "byte {} of dispatch patch must be NOP", i);
        }
    }

    // -----------------------------------------------------------------------
    // ko_dispatch tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_ko_dispatch_fits_in_limit() {
        let code = dispatch::assemble_ko_dispatch().unwrap();
        assert!(
            code.len() <= constants::KO_DISPATCH_SIZE,
            "ko_dispatch is {} bytes, exceeds {} byte limit",
            code.len(),
            constants::KO_DISPATCH_SIZE
        );
    }

    #[test]
    fn test_ko_dispatch_nop_sled() {
        let code = dispatch::assemble_ko_dispatch().unwrap();
        for (i, byte) in code.iter().enumerate().take(32) {
            assert_eq!(
                *byte, 0x00,
                "ko_dispatch byte {i} must be NOP (part of 32-byte NOP sled)"
            );
        }
    }

    #[test]
    fn test_ko_dispatch_first_instruction_ld_a_d() {
        let code = dispatch::assemble_ko_dispatch().unwrap();
        // Disassemble starting after the NOP sled
        let decoded = decode_bytes(&code[32..]).expect("typed dispatch instruction");
        assert_eq!(
            decoded.instruction(),
            &Instruction::LdRR(Reg8::A, Reg8::D),
            "first instruction after NOP sled must be LD A,D"
        );
    }

    #[test]
    fn test_ko_dispatch_has_cp_80() {
        let code = dispatch::assemble_ko_dispatch().unwrap();
        // FE 80 = CP $80
        let has_cp80 = code.windows(2).any(|w| w == [0xFE, 0x80]);
        assert!(has_cp80, "ko_dispatch must contain CP $80");
    }

    #[test]
    fn test_ko_dispatch_has_ko_render_jump() {
        let code = dispatch::assemble_ko_dispatch().unwrap();
        // JP $8000 = C3 00 80
        let target_lo = (constants::KO_RENDER_LOGICAL & 0xFF) as u8;
        let target_hi = (constants::KO_RENDER_LOGICAL >> 8) as u8;
        let has_jp = code.windows(3).any(|w| w == [0xC3, target_lo, target_hi]);
        assert!(has_jp, "ko_dispatch must contain JP $8000 (ko_render)");
    }

    #[test]
    fn test_ko_dispatch_en_path_add_54() {
        let code = dispatch::assemble_ko_dispatch().unwrap();
        // LD A, $54 = 3E 54
        let has_ld_54 = code.windows(2).any(|w| w == [0x3E, 0x54]);
        assert!(has_ld_54, "ko_dispatch EN path must contain LD A,$54");
    }

    #[test]
    fn test_ko_dispatch_en_path_jp_23a6() {
        let code = dispatch::assemble_ko_dispatch().unwrap();
        // JP $23A6 = C3 A6 23
        let lo = (constants::EN_BANK_SWITCH_ADDR & 0xFF) as u8;
        let hi = (constants::EN_BANK_SWITCH_ADDR >> 8) as u8;
        let has_jp = code.windows(3).any(|w| w == [0xC3, lo, hi]);
        assert!(has_jp, "ko_dispatch EN path must contain JP $23A6");
    }

    // -----------------------------------------------------------------------
    // ko_render tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_ko_render_initializes_dfdc() {
        let code = render::assemble_ko_render().unwrap();
        // LD ($DFDC), A = 0x32 0xDC 0xDF — ko_render must initialize $DFDC
        // when VWF session is not active (bit7=0). Without this, the first KO
        // character in a text string uses stale $DFDC ($00 at boot), causing
        // glyph writes to overwrite game graphic tiles (face corruption).
        let has_dfdc_write = code.windows(3).any(|w| w == [0x32, 0xDC, 0xDF]);
        assert!(
            has_dfdc_write,
            "ko_render must initialize $DFDC to prevent tile range collision at boot"
        );
    }

    #[test]
    fn test_ko_render_resets_tile_counter() {
        let code = render::assemble_ko_render().unwrap();
        // LD ($DFE8), A = 0x32 0xE8 0xDF
        let has_counter_reset = code.windows(3).any(|w| w == [0x32, 0xE8, 0xDF]);
        assert!(has_counter_reset, "ko_render must reset $DFE8 tile counter");
    }

    #[test]
    fn test_ko_render_jumps_to_engine_return() {
        let code = render::assemble_ko_render().unwrap();
        // JP $23CA = C3 CA 23 (slot 2 restore via Bank 0 trampoline)
        let lo = (constants::ENGINE_RETURN_ADDR & 0xFF) as u8;
        let hi = (constants::ENGINE_RETURN_ADDR >> 8) as u8;
        let has_jp = code.windows(3).any(|w| w == [0xC3, lo, hi]);
        assert!(
            has_jp,
            "ko_render must end with JP $23CA (safe slot restore)"
        );
    }

    #[test]
    fn test_ko_render_contains_outi() {
        let code = render::assemble_ko_render().unwrap();
        // OUTI = ED A3
        let has_outi = code.windows(2).any(|w| w == [0xED, 0xA3]);
        assert!(has_outi, "ko_render must contain OUTI for VRAM transfer");
    }

    #[test]
    fn test_ko_render_sets_dfe6_c0() {
        let code = render::assemble_ko_render().unwrap();
        // LD A, $C0 = 3E C0; LD ($DFE6), A = 32 E6 DF
        // Mimics VWF BOUNDARY → next EN char goes to $B3BC (no DEC HL)
        let has_c0 = code.windows(5).any(|w| w == [0x3E, 0xC0, 0x32, 0xE6, 0xDF]);
        assert!(has_c0, "ko_render must set $DFE6=$C0 (BOUNDARY state)");
    }

    #[test]
    fn test_ko_render_pre_increments_tile_counter() {
        let code = render::assemble_ko_render().unwrap();
        // INC (HL) = 34 must appear before ADD A, (HL) = 86
        // This is the VWF pre-increment convention ($B4C5)
        let inc_pos = code.iter().position(|&b| b == 0x34);
        let add_pos = code.iter().position(|&b| b == 0x86);
        assert!(inc_pos.is_some(), "must have INC (HL)");
        assert!(add_pos.is_some(), "must have ADD A, (HL)");
        assert!(
            inc_pos.unwrap() < add_pos.unwrap(),
            "INC (HL) must come BEFORE ADD A, (HL) (pre-increment)"
        );
    }

    #[test]
    fn test_ko_render_no_inline_nametable_write() {
        let code = render::assemble_ko_render().unwrap();
        // LD (HL), D = 72 followed by INC HL = 23 should NOT appear
        // (nametable write is handled by $2391, not inline)
        let has_inline = code.windows(2).any(|w| w == [0x72, 0x23]);
        assert!(!has_inline, "ko_render must NOT write nametable inline");
    }

    #[test]
    fn test_ko_render_di_ei_pair() {
        let code = render::assemble_ko_render().unwrap();
        // DI = F3, EI = FB; both must be present for VDP timing
        let has_di = code.contains(&0xF3);
        let has_ei = code.contains(&0xFB);
        assert!(has_di, "ko_render must contain DI before VDP write");
        assert!(has_ei, "ko_render must contain EI after VDP write");
    }

    // -----------------------------------------------------------------------
    // EN guard tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_en_guard_fits_in_region() {
        let code = guard::assemble_en_guard().unwrap();
        assert!(
            code.len() <= constants::EN_GUARD_SIZE,
            "EN guard is {} bytes, exceeds {} byte region",
            code.len(),
            constants::EN_GUARD_SIZE
        );
    }

    #[test]
    fn test_en_guard_has_cp_80() {
        let code = guard::assemble_en_guard().unwrap();
        // CP $80 = FE 80
        let has_cp80 = code.windows(2).any(|w| w == [0xFE, 0x80]);
        assert!(
            has_cp80,
            "EN guard must contain CP $80 for Korean prefix check"
        );
    }

    #[test]
    fn test_en_guard_has_wide_space_replacement() {
        let code = guard::assemble_en_guard().unwrap();
        // LD A, $48 = 3E 48
        let has_wide_space = code.windows(2).any(|w| w == [0x3E, 0x48]);
        assert!(
            has_wide_space,
            "EN guard must replace Korean prefix with $48 (wide space)"
        );
    }

    #[test]
    fn test_en_guard_jumps_to_vwf_engine() {
        let code = guard::assemble_en_guard().unwrap();
        // JP $B2C0 = C3 C0 B2
        let lo = (constants::VWF_ENGINE_ADDR & 0xFF) as u8;
        let hi = (constants::VWF_ENGINE_ADDR >> 8) as u8;
        let has_jp = code.windows(3).any(|w| w == [0xC3, lo, hi]);
        assert!(has_jp, "EN guard must end with JP $B2C0 (VWF engine)");
    }

    #[test]
    fn test_en_guard_switches_to_bank_8() {
        let code = guard::assemble_en_guard().unwrap();
        // LD A, 8 = 3E 08; LD ($FFFF), A = 32 FF FF
        let has_bank8 = code.windows(5).any(|w| w == [0x3E, 0x08, 0x32, 0xFF, 0xFF]);
        assert!(has_bank8, "EN guard must switch to Bank 8 before JP VWF");
    }

    #[test]
    fn test_en_guard_saves_bc() {
        let code = guard::assemble_en_guard().unwrap();
        // LD ($DFE0), BC = ED 43 E0 DF
        let lo = (constants::RAM_TEXT_PTR & 0xFF) as u8;
        let hi = (constants::RAM_TEXT_PTR >> 8) as u8;
        let has_save = code.windows(4).any(|w| w == [0xED, 0x43, lo, hi]);
        assert!(has_save, "EN guard must save BC to $DFE0");
    }

    // -----------------------------------------------------------------------
    // Cross-module consistency tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_ko_render_ld_a_d_sub_80() {
        // Korean high byte processing: LD A,D; SUB $80
        let code = render::assemble_ko_render().unwrap();
        // LD A, D = 7A; SUB $80 = D6 80
        let has_seq = code.windows(3).any(|w| w == [0x7A, 0xD6, 0x80]);
        assert!(has_seq, "ko_render must extract syllable group via SUB $80");
    }

    #[test]
    fn test_ko_render_vdp_write_mode_or_40() {
        // VDP write address setup: OR $40
        let code = render::assemble_ko_render().unwrap();
        // OR $40 = F6 40
        let has_or40 = code.windows(2).any(|w| w == [0xF6, 0x40]);
        assert!(has_or40, "ko_render must set VDP write mode (OR $40)");
    }

    #[test]
    fn test_ko_render_always_base_9f() {
        let code = render::assemble_ko_render().unwrap();
        // ko_render must set $DFDC to $9F unconditionally (no $70/$CE branching).
        // $9F-$D6 = 56 safe tiles; base $70 only has 15 before face tile $80 collision.
        let has_ld_a_9f = code.windows(2).any(|w| w == [0x3E, 0x9F]);
        assert!(has_ld_a_9f, "ko_render must use tile base $9F");
        // Must NOT contain base $70 selection (removed)
        let has_cp_a6 = code.windows(2).any(|w| w == [0xFE, 0xA6]);
        assert!(
            !has_cp_a6,
            "ko_render must not branch on $A6 (base $70 removed)"
        );
    }

    #[test]
    fn test_en_guard_return_addr_at_23ca() {
        let code = guard::assemble_en_guard().unwrap();
        let offset = (constants::ENGINE_RETURN_ADDR - constants::EN_BANK_SWITCH_ADDR) as usize;
        // LD ($FFFF), A = 32 FF FF must be at the return address offset
        assert_eq!(
            code[offset..offset + 3],
            [0x32, 0xFF, 0xFF],
            "LD ($FFFF),A must be at offset {:#X} (address ${:04X})",
            offset,
            constants::ENGINE_RETURN_ADDR
        );
    }

    #[test]
    fn test_ko_render_size_reasonable() {
        let code = render::assemble_ko_render().unwrap();
        assert!(
            code.len() <= 200,
            "ko_render is {} bytes, expected ≤200",
            code.len()
        );
    }

    #[test]
    fn test_constants_address_sanity() {
        // Basic sanity: physical addresses match expected bank layout
        assert_eq!(constants::KO_RENDER_PHYSICAL, 0x80000, "Bank 32 = 0x80000");
        assert_eq!(constants::FONT_4BPP_PHYSICAL, 0x84000, "Bank 33 = 0x84000");
        assert_eq!(
            constants::KO_DISPATCH_ADDR,
            0x2889,
            "dispatch in Bank 0 free space"
        );
        assert_eq!(
            constants::DISPATCH_PATCH_ADDR,
            0x238A,
            "intercept at original tile mapping"
        );
    }

    // -----------------------------------------------------------------------
    // Phase 3b Task 2: relocation wiring (stubs + entry point redirects)
    // -----------------------------------------------------------------------

    #[test]
    fn test_stub_textwindow_bytes_spare_35() {
        // LD B,35; JP $22CC = 06 23 C3 CC 22
        let code = relocation::assemble_stub_textwindow(35).unwrap();
        assert_eq!(code, vec![0x06, 0x23, 0xC3, 0xCC, 0x22]);
        assert_eq!(code.len(), constants::RELOCATION_STUB_SIZE);
    }

    #[test]
    fn test_stub_textwindow_bytes_spare_36() {
        // LD B,36; JP $22CC = 06 24 C3 CC 22
        let code = relocation::assemble_stub_textwindow(36).unwrap();
        assert_eq!(code, vec![0x06, 0x24, 0xC3, 0xCC, 0x22]);
        assert_eq!(code.len(), constants::RELOCATION_STUB_SIZE);
    }

    #[test]
    fn test_stub_sprite_bytes_spare_35() {
        // LD B,35; JP $232D = 06 23 C3 2D 23
        let code = relocation::assemble_stub_sprite(35).unwrap();
        assert_eq!(code, vec![0x06, 0x23, 0xC3, 0x2D, 0x23]);
        assert_eq!(code.len(), constants::RELOCATION_STUB_SIZE);
    }

    #[test]
    fn test_stub_textwindow_bytes_spare_37() {
        // LD B,37; JP $22CC = 06 25 C3 CC 22 — stub D (bank3 "third",
        // Phase 3c).
        let code = relocation::assemble_stub_textwindow(37).unwrap();
        assert_eq!(code, vec![0x06, 0x25, 0xC3, 0xCC, 0x22]);
        assert_eq!(code.len(), constants::RELOCATION_STUB_SIZE);
    }

    #[test]
    fn test_stub_sprite_bytes_spare_37() {
        // LD B,37; JP $232D = 06 25 C3 2D 23 — stub E (bank3 "third",
        // Phase 3c).
        let code = relocation::assemble_stub_sprite(37).unwrap();
        assert_eq!(code, vec![0x06, 0x25, 0xC3, 0x2D, 0x23]);
        assert_eq!(code.len(), constants::RELOCATION_STUB_SIZE);
    }

    #[test]
    fn test_stub_targets_match_continuation_constants() {
        // The JP operand inside each stub must equal the documented
        // continuation address, independent of which spare bank is passed.
        let textwindow = relocation::assemble_stub_textwindow(35).unwrap();
        let target = (textwindow[3] as u16) | ((textwindow[4] as u16) << 8);
        assert_eq!(target, constants::TEXTWINDOW_CONTINUATION_ADDR);

        let sprite = relocation::assemble_stub_sprite(35).unwrap();
        let target = (sprite[3] as u16) | ((sprite[4] as u16) << 8);
        assert_eq!(target, constants::SPRITE_CONTINUATION_ADDR);
    }

    #[test]
    fn test_entry1_jr_displacement_targets_stub_a() {
        // $22C1 (byte after the displacement) + disp must land on stub A.
        let disp = relocation::entry1_new_byte();
        let next_instr_addr = constants::ENTRY1_BANK3_MAIN_TEXTWINDOW_JR_ADDR as i32 + 1;
        let target = next_instr_addr + (disp as i8 as i32);
        assert_eq!(target, constants::STUB_TEXTWINDOW_A_ADDR as i32);
        assert_eq!(
            disp, 0xDD,
            "displacement byte must match verified EN ROM value"
        );
    }

    #[test]
    fn test_entry3_targets_stub_a() {
        // bank3 "secondary" stays on stub A/spare 35 (only "third" split
        // out in Phase 3c).
        let expected = [
            (constants::STUB_TEXTWINDOW_A_ADDR & 0xFF) as u8,
            (constants::STUB_TEXTWINDOW_A_ADDR >> 8) as u8,
        ];
        assert_eq!(relocation::entry3_new_bytes(), expected);
    }

    #[test]
    fn test_entry2_targets_stub_d() {
        // Phase 3c: bank3 "third" text-window now targets stub D (spare 37),
        // no longer stub A (spare 35) — must diverge from entry #3
        // ("secondary", still stub A).
        let expected = [
            (constants::STUB_TEXTWINDOW_D_ADDR & 0xFF) as u8,
            (constants::STUB_TEXTWINDOW_D_ADDR >> 8) as u8,
        ];
        assert_eq!(relocation::entry2_new_bytes(), expected);
        assert_ne!(
            relocation::entry2_new_bytes(),
            relocation::entry3_new_bytes()
        );
    }

    #[test]
    fn test_entry4_targets_stub_b() {
        // bank3 "secondary" sprite stays on stub B/spare 35.
        let expected = [
            (constants::STUB_SPRITE_B_ADDR & 0xFF) as u8,
            (constants::STUB_SPRITE_B_ADDR >> 8) as u8,
        ];
        assert_eq!(relocation::entry4_new_bytes(), expected);
    }

    #[test]
    fn test_entry5_targets_stub_e() {
        // Phase 3c: bank3 "third" sprite now targets stub E (spare 37), no
        // longer stub B (spare 35) — must diverge from entry #4
        // ("secondary", still stub B).
        let expected = [
            (constants::STUB_SPRITE_E_ADDR & 0xFF) as u8,
            (constants::STUB_SPRITE_E_ADDR >> 8) as u8,
        ];
        assert_eq!(relocation::entry5_new_bytes(), expected);
        assert_ne!(
            relocation::entry5_new_bytes(),
            relocation::entry4_new_bytes()
        );
    }

    #[test]
    fn test_entry6_targets_stub_c() {
        let expected = [
            (constants::STUB_TEXTWINDOW_C_ADDR & 0xFF) as u8,
            (constants::STUB_TEXTWINDOW_C_ADDR >> 8) as u8,
        ];
        assert_eq!(relocation::entry6_new_bytes(), expected);
    }

    #[test]
    fn test_relocation_entry_bytes_match_design_doc() {
        // Cross-check every entry's new bytes against
        // docs/reverse_engineering/phase3b_z80_wiring.md's 표1 verbatim
        // (entries #2/#5 updated for Phase 3c's stub D/E split), so a
        // future edit to relocation.rs's target constants can't silently
        // drift from the reviewed design without a test failure.
        assert_eq!(relocation::entry1_new_byte(), 0xDD);
        assert_eq!(relocation::entry2_new_bytes(), [0xD6, 0x1A]); // -> stub D ($1AD6)
        assert_eq!(relocation::entry3_new_bytes(), [0x9E, 0x22]); // -> stub A ($229E)
        assert_eq!(relocation::entry4_new_bytes(), [0xC4, 0x22]); // -> stub B ($22C4)
        assert_eq!(relocation::entry5_new_bytes(), [0xDB, 0x1A]); // -> stub E ($1ADB)
        assert_eq!(relocation::entry6_new_bytes(), [0xAA, 0x26]); // -> stub C ($26AA)
    }

    #[test]
    fn test_stub_wiring_matches_relocation_map() {
        // Review Important #2: the stub selection this module hardcodes
        // (entryN_* -> stub A/B/C/D/E) must never drift from
        // `text::relocation::RELOCATION_MAP`, the single source of truth
        // `partition_translations` uses to decide where each table's DATA
        // actually lands. This mirrors exactly the derivation
        // `commands::build` Stage 7 performs via `spare_bank_for` — so a
        // future edit to RELOCATION_MAP that changes a table's spare bank
        // (or ungroups main/secondary) fails here instead of silently
        // wiring an entry point to the wrong bank.
        use crate::text::relocation::spare_bank_for;

        let bank3_main = spare_bank_for(3, "main").unwrap();
        let bank3_secondary = spare_bank_for(3, "secondary").unwrap();
        assert_eq!(
            bank3_main, bank3_secondary,
            "bank3 main/secondary must share one spare bank (stub A/B assumption)"
        );
        assert_eq!(bank3_main, 35);

        let bank3_third = spare_bank_for(3, "third").unwrap();
        assert_eq!(
            bank3_third, 37,
            "bank3 third must be split into spare 37 (Phase 3c)"
        );
        assert_ne!(
            bank3_third, bank3_main,
            "third must no longer share a spare bank with main/secondary"
        );

        let bank5_main = spare_bank_for(5, "main").unwrap();
        let bank5_secondary = spare_bank_for(5, "secondary").unwrap();
        assert_eq!(
            bank5_main, bank5_secondary,
            "bank5 main/secondary must share one spare bank (stub C assumption)"
        );
        assert_eq!(bank5_main, 36);

        // Each stub's `LD B,n` immediate must equal the map-derived spare
        // bank number for the group it serves.
        assert_eq!(
            relocation::assemble_stub_textwindow(bank3_main).unwrap()[1],
            bank3_main
        );
        assert_eq!(
            relocation::assemble_stub_sprite(bank3_main).unwrap()[1],
            bank3_main
        );
        assert_eq!(
            relocation::assemble_stub_textwindow(bank3_third).unwrap()[1],
            bank3_third
        );
        assert_eq!(
            relocation::assemble_stub_sprite(bank3_third).unwrap()[1],
            bank3_third
        );
        assert_eq!(
            relocation::assemble_stub_textwindow(bank5_main).unwrap()[1],
            bank5_main
        );

        // And the entry points for the split-out table (#2/#5, "third")
        // must land on stub D/E, never on stub A/B (which now serve only
        // main/secondary).
        assert_eq!(
            relocation::entry2_new_bytes(),
            [
                (constants::STUB_TEXTWINDOW_D_ADDR & 0xFF) as u8,
                (constants::STUB_TEXTWINDOW_D_ADDR >> 8) as u8,
            ]
        );
        assert_eq!(
            relocation::entry5_new_bytes(),
            [
                (constants::STUB_SPRITE_E_ADDR & 0xFF) as u8,
                (constants::STUB_SPRITE_E_ADDR >> 8) as u8,
            ]
        );
    }

    #[test]
    fn test_atomicity_stub_b_and_entry6_coupling() {
        // 원자성 불변식 (phase3b_z80_wiring.md): stub B ($22C4-$22C8) is
        // written on top of the OLD bank5 stub, whose 4 tail bytes lived at
        // $22C6-$22C9 (`06 05 18 02`, part of the original `18 65 06 05
        // 18` region). Once stub B is applied, address $22C6 holds stub B's
        // own tail bytes (`C3 2D 23` = `JP $232D`, no preceding `LD B,n`).
        // If entry #6 (bank5's caller at $763F) is NOT also redirected away
        // from $22C6 in the same build, $763E still calls $22C6 and falls
        // straight into that bare `JP $232D` with whatever B last held —
        // silent corruption. This test proves stub B's bytes at that old
        // offset are exactly the dangerous bare-JP sequence (confirming the
        // hazard is real), and that entry #6's computed target is never
        // $22C6, only ever stub C.
        let stub_b = relocation::assemble_stub_sprite(35).unwrap();
        // stub_b = [06, 23, C3, 2D, 23]; stub_b[2..] is what ends up at
        // $22C6 (STUB_SPRITE_B_ADDR + 2) once stub B is written.
        assert_eq!(
            &stub_b[2..],
            &[0xC3, 0x2D, 0x23],
            "stub B's tail at old $22C6 must be a bare JP $232D (the hazard entry #6 must route around)"
        );

        let entry6_target = u16::from_le_bytes(relocation::entry6_new_bytes());
        assert_ne!(
            entry6_target, 0x22C6,
            "entry #6 must never be left pointing at $22C6 (now inside stub B)"
        );
        assert_eq!(entry6_target, constants::STUB_TEXTWINDOW_C_ADDR as u16);
    }

    #[test]
    fn test_relocation_stage7_writes_apply_cleanly_on_synthetic_rom() {
        // Integration-style check of the exact write_expect sequence
        // commands::build's Stage 7 issues for relocation wiring, run
        // against a synthetic buffer seeded with the real EN ROM's
        // original bytes at all 11 offsets (verified via `xxd`/`disasm`
        // against the real ROM — see constants.rs doc comments) instead of
        // the actual 1MB ROM file, which is gitignored and not available in
        // CI.
        //
        // This exercises `TrackedRom::write_expect`'s own precondition
        // checks (any byte-value or size mismatch panics via `.unwrap()`)
        // and `TrackedRom`'s collision detector (any accidental region
        // overlap between the 5 stubs and 6 entry points also panics),
        // then asserts the brief's literal atomicity check: after all 11
        // writes, $763F-$7640 must no longer read the old bank5 stub
        // target `C6 22`.
        use gg_sms::rom::{Expect, TrackedRom};

        let mut seed = vec![0u8; 0x8000];
        seed[0x1AD6..0x1ADB].copy_from_slice(&[0xCD, 0xF3, 0x22, 0xCD, 0x25]); // stub D region
        seed[0x1ADB..0x1AE0].copy_from_slice(&[0x27, 0xCD, 0xF4, 0x2B, 0xC3]); // stub E region
        seed[0x229E..0x22A3].copy_from_slice(&[0xAF, 0x18, 0x02, 0x3E, 0x01]); // stub A region
        seed[0x22C4..0x22C9].copy_from_slice(&[0x18, 0x65, 0x06, 0x05, 0x18]); // stub B region
        seed[0x26AA..0x26AF].copy_from_slice(&[0x21, 0x3E, 0xC9, 0x18, 0x03]); // stub C region
        seed[0x22C0] = 0x09; // entry #1
        seed[0x2288..0x228A].copy_from_slice(&[0xCA, 0x22]); // entry #2
        seed[0x229A..0x229C].copy_from_slice(&[0xCA, 0x22]); // entry #3
        seed[0x22AC..0x22AE].copy_from_slice(&[0x2B, 0x23]); // entry #4
        seed[0x446C..0x446E].copy_from_slice(&[0x2B, 0x23]); // entry #5
        seed[0x763F..0x7641].copy_from_slice(&[0xC6, 0x22]); // entry #6

        let mut rom = TrackedRom::new(seed);

        rom.write_expect(
            "stub_a",
            constants::STUB_TEXTWINDOW_A_ADDR,
            &relocation::assemble_stub_textwindow(35).unwrap(),
            &Expect::Bytes(&[0xAF, 0x18, 0x02, 0x3E, 0x01]),
        )
        .unwrap();
        rom.write_expect(
            "stub_b",
            constants::STUB_SPRITE_B_ADDR,
            &relocation::assemble_stub_sprite(35).unwrap(),
            &Expect::Bytes(&[0x18, 0x65, 0x06, 0x05, 0x18]),
        )
        .unwrap();
        rom.write_expect(
            "stub_c",
            constants::STUB_TEXTWINDOW_C_ADDR,
            &relocation::assemble_stub_textwindow(36).unwrap(),
            &Expect::Bytes(&[0x21, 0x3E, 0xC9, 0x18, 0x03]),
        )
        .unwrap();
        rom.write_expect(
            "stub_d",
            constants::STUB_TEXTWINDOW_D_ADDR,
            &relocation::assemble_stub_textwindow(37).unwrap(),
            &Expect::Bytes(&[0xCD, 0xF3, 0x22, 0xCD, 0x25]),
        )
        .unwrap();
        rom.write_expect(
            "stub_e",
            constants::STUB_SPRITE_E_ADDR,
            &relocation::assemble_stub_sprite(37).unwrap(),
            &Expect::Bytes(&[0x27, 0xCD, 0xF4, 0x2B, 0xC3]),
        )
        .unwrap();
        rom.write_expect(
            "entry1",
            constants::ENTRY1_BANK3_MAIN_TEXTWINDOW_JR_ADDR,
            &[relocation::entry1_new_byte()],
            &Expect::Bytes(&[0x09]),
        )
        .unwrap();
        rom.write_expect(
            "entry2",
            constants::ENTRY2_BANK3_THIRD_TEXTWINDOW_CALL_ADDR,
            &relocation::entry2_new_bytes(),
            &Expect::Bytes(&[0xCA, 0x22]),
        )
        .unwrap();
        rom.write_expect(
            "entry3",
            constants::ENTRY3_BANK3_SECONDARY_TEXTWINDOW_CALL_ADDR,
            &relocation::entry3_new_bytes(),
            &Expect::Bytes(&[0xCA, 0x22]),
        )
        .unwrap();
        rom.write_expect(
            "entry4",
            constants::ENTRY4_BANK3_SECONDARY_SPRITE_CALL_ADDR,
            &relocation::entry4_new_bytes(),
            &Expect::Bytes(&[0x2B, 0x23]),
        )
        .unwrap();
        rom.write_expect(
            "entry5",
            constants::ENTRY5_BANK3_THIRD_SPRITE_CALL_ADDR,
            &relocation::entry5_new_bytes(),
            &Expect::Bytes(&[0x2B, 0x23]),
        )
        .unwrap();
        rom.write_expect(
            "entry6",
            constants::ENTRY6_BANK5_SHARED_CALL_ADDR,
            &relocation::entry6_new_bytes(),
            &Expect::Bytes(&[0xC6, 0x22]),
        )
        .unwrap();

        // The brief's literal atomicity check: post-patch, $763F-$7640 must
        // no longer be the old bank5 stub target.
        assert_ne!(rom.read(0x763F, 2).unwrap(), &[0xC6, 0x22]);
        assert_eq!(rom.read(0x763F, 2).unwrap(), &[0xAA, 0x26]);

        // And the corresponding stub is intact at its new address.
        assert_eq!(
            rom.read(constants::STUB_TEXTWINDOW_C_ADDR, 5).unwrap(),
            &[0x06, 0x24, 0xC3, 0xCC, 0x22]
        );

        // Phase 3c: entries #2/#5 ("third") now resolve to stub D/E, not
        // stub A/B — and stub D/E are intact at their reclaimed addresses.
        assert_eq!(rom.read(0x2288, 2).unwrap(), &[0xD6, 0x1A]);
        assert_eq!(rom.read(0x446C, 2).unwrap(), &[0xDB, 0x1A]);
        assert_eq!(
            rom.read(constants::STUB_TEXTWINDOW_D_ADDR, 5).unwrap(),
            &[0x06, 0x25, 0xC3, 0xCC, 0x22]
        );
        assert_eq!(
            rom.read(constants::STUB_SPRITE_E_ADDR, 5).unwrap(),
            &[0x06, 0x25, 0xC3, 0x2D, 0x23]
        );
    }

    #[test]
    fn test_stub_regions_do_not_overlap_entry_points() {
        // Every stub's 5-byte region must not overlap any of the 6 entry
        // point write regions (all within Bank 0's fixed slot0, <0x4000).
        let stubs = [
            (
                constants::STUB_TEXTWINDOW_A_ADDR,
                constants::RELOCATION_STUB_SIZE,
            ),
            (
                constants::STUB_SPRITE_B_ADDR,
                constants::RELOCATION_STUB_SIZE,
            ),
            (
                constants::STUB_TEXTWINDOW_C_ADDR,
                constants::RELOCATION_STUB_SIZE,
            ),
            (
                constants::STUB_TEXTWINDOW_D_ADDR,
                constants::RELOCATION_STUB_SIZE,
            ),
            (
                constants::STUB_SPRITE_E_ADDR,
                constants::RELOCATION_STUB_SIZE,
            ),
        ];
        let entries = [
            (constants::ENTRY1_BANK3_MAIN_TEXTWINDOW_JR_ADDR, 1),
            (constants::ENTRY2_BANK3_THIRD_TEXTWINDOW_CALL_ADDR, 2),
            (constants::ENTRY3_BANK3_SECONDARY_TEXTWINDOW_CALL_ADDR, 2),
            (constants::ENTRY4_BANK3_SECONDARY_SPRITE_CALL_ADDR, 2),
            // #5 and #6 are bank1 physical addresses (0x446C, 0x763F),
            // far outside the bank0 stub range — included for completeness.
            (constants::ENTRY5_BANK3_THIRD_SPRITE_CALL_ADDR, 2),
            (constants::ENTRY6_BANK5_SHARED_CALL_ADDR, 2),
        ];
        for &(stub_addr, stub_len) in &stubs {
            for &(entry_addr, entry_len) in &entries {
                let overlap =
                    entry_addr < stub_addr + stub_len && stub_addr < entry_addr + entry_len;
                assert!(
                    !overlap,
                    "stub [{:#X}, {:#X}) overlaps entry point [{:#X}, {:#X})",
                    stub_addr,
                    stub_addr + stub_len,
                    entry_addr,
                    entry_addr + entry_len
                );
            }
        }
    }

    #[test]
    fn test_stub_continuation_targets_outside_stub_regions() {
        // Stubs must jump to continuation code that lies OUTSIDE any stub's
        // own write region — otherwise a stub would clobber the shared
        // bank-switch routine it depends on.
        let stubs: &[(usize, usize)] = &[
            (
                constants::STUB_TEXTWINDOW_A_ADDR,
                constants::RELOCATION_STUB_SIZE,
            ),
            (
                constants::STUB_SPRITE_B_ADDR,
                constants::RELOCATION_STUB_SIZE,
            ),
            (
                constants::STUB_TEXTWINDOW_C_ADDR,
                constants::RELOCATION_STUB_SIZE,
            ),
            (
                constants::STUB_TEXTWINDOW_D_ADDR,
                constants::RELOCATION_STUB_SIZE,
            ),
            (
                constants::STUB_SPRITE_E_ADDR,
                constants::RELOCATION_STUB_SIZE,
            ),
        ];
        let continuations = [
            constants::TEXTWINDOW_CONTINUATION_ADDR as usize,
            constants::SPRITE_CONTINUATION_ADDR as usize,
        ];
        for &(stub_addr, stub_len) in stubs {
            for &cont in &continuations {
                assert!(
                    cont < stub_addr || cont >= stub_addr + stub_len,
                    "continuation {:#X} falls inside stub region [{:#X}, {:#X})",
                    cont,
                    stub_addr,
                    stub_addr + stub_len
                );
            }
        }
    }
}
