// === Patch insertion points (physical ROM offsets) ===

/// JP ko_dispatch replaces `LD A,$54; ADD A,D` at dispatch point.
pub const DISPATCH_PATCH_ADDR: usize = 0x238A;
/// Original bytes: `3E 54 82 57 18 16 00` — 7 bytes.
pub const DISPATCH_PATCH_SIZE: usize = 7;

/// EN read-ahead guard + bank switch code region.
pub const EN_GUARD_ADDR: usize = 0x23A6;
/// Size of the guard region ($23A6-$23D2 = 45 bytes).
pub const EN_GUARD_SIZE: usize = 45;

/// ko_dispatch is placed in free space in Bank 0.
pub const KO_DISPATCH_ADDR: usize = 0x2889;
/// 32-byte NOP sled + up to 32 bytes of dispatch code.
pub const KO_DISPATCH_SIZE: usize = 64;

/// Physical ROM offset for ko_render (Bank 32 start).
pub const KO_RENDER_PHYSICAL: usize = 0x80000;
/// Logical address of ko_render in Slot 2 ($8000-$BFFF).
pub const KO_RENDER_LOGICAL: u16 = 0x8000;

/// Return address for VWF engine: LD ($FFFF),A; JP $2391.
/// This sequence is emitted by assemble_en_guard() at the end of the guard region.
/// A must contain the slot 2 bank number to restore.
pub const ENGINE_RETURN_ADDR: u16 = 0x23CA;

/// Logical address of the EN bank-switch + VWF jump code.
pub const EN_BANK_SWITCH_ADDR: u16 = 0x23A6;
/// Logical address of the Bank 8 VWF proportional-width engine.
pub const VWF_ENGINE_ADDR: u16 = 0xB2C0;

/// Physical address of the 4bpp Korean glyph data (Bank 33 start).
pub const FONT_4BPP_PHYSICAL: usize = 0x84000;

/// Bank 8 return patch: one-byte change JP $23C8 → JP $23CA.
/// Points to the LD ($FFFF),A; JP $2391 emitted by EN guard.
pub const BANK8_RETURN_PATCH: usize = 0x234F5;

/// Bank 8 $B41D: `LD (HL), $00` (2 bytes: 36 00) — resets $DFE8 during VWF init.
/// NOP'd to prevent $DFE8 reset when KO and EN tiles share the counter.
/// Physical: 0x20000 + ($B41D - $8000) = 0x2341D.
pub const BANK8_DFE8_RESET_PATCH: usize = 0x2341D;

/// Bank 9 $8EB0 (물리 0x24EB0): 상점/HUD 돈 단위 `金`(한자) 글리프.
/// 1bpp 8바이트(`00 28 7c 92 7c 10 54 fe`). 게임이 1bpp→4bpp 확장해 VRAM tile 215로
/// 직접 업로드(대사 char 경로 아님, HUD 그래픽). BP 추적으로 소스 확정 —
/// 업로드 코드 $2842(OUTI, C=$BE), HL이 이 주소를 가리킴.
/// 한글 `금`으로 교체(0법칙: 한글 패치에 JP 한자 노출 방지).
/// docs/reverse_engineering/shop_money_kanji_blocker.md.
pub const MONEY_KANJI_GLYPH_ADDR: usize = 0x24EB0;

/// $23E4: newline($FD) 핸들러($23D6)의 `$23E3 LD BC,$00XX; $23E6 ADD IX,BC` stride 저바이트.
/// JP 원본=$50(80바이트=나메테이블 2행, 큰 줄간격), EN이 $28(40=1행)로 촘촘히 packing.
/// 한글은 JP처럼 2행 간격이 가독성·게임 하드코딩 가격 슬롯 정렬에 맞음 → $50으로 복원.
/// 전제: 페이지당 ≤3줄(2행×3=6행이 박스에 들어감). docs/qa_backlog.md.
pub const NEWLINE_STRIDE_ADDR: usize = 0x23E4;

/// $0020: `JP $26DC` (3 bytes) — end of $0018 VBlank sync setup.
/// Redirected to $0032 trampoline that clears $DFE8 before JP $26DC.
pub const TEXT_SYNC_JP_ADDR: usize = 0x0020;

/// $0032-$0037: 6-byte free space between newline handler and RST $38.
/// Used as trampoline: `LD ($DFE8), A; JP $26DC`.
pub const DFE8_TRAMPOLINE_ADDR: usize = 0x0032;
pub const DFE8_TRAMPOLINE_LOGICAL: u16 = 0x0032;

/// Original $0020 JP target — VBlank buffer sync function.
pub const VBLANK_SYNC_ADDR: u16 = 0x26DC;

// === RAM addresses ===

/// Saved Slot 1 bank number (during Korean render).
pub const RAM_SLOT1_SAVE: u16 = 0xDFD0;
/// Saved Slot 2 bank number (during Korean render).
pub const RAM_SLOT2_SAVE: u16 = 0xDFD1;
/// Second text byte (Korean low byte or EN read-ahead peek).
pub const RAM_BYTE2: u16 = 0xDFD8;
/// Saved BC (text pointer) — 2 bytes.
pub const RAM_TEXT_PTR: u16 = 0xDFE0;
/// Saved HL — 2 bytes.
pub const RAM_HL_SAVE: u16 = 0xDFE2;
/// Saved DE — 2 bytes.
pub const RAM_DE_SAVE: u16 = 0xDFE4;
/// VWF compositing state: bit7=session active, bit6=compositing, bit5=shift.
pub const VWF_STATE: u16 = 0xDFE6;
/// VWF output tile counter (index within current session).
pub const VWF_TILE_COUNTER: u16 = 0xDFE8;
/// VWF output tile base ($70 for line 1, $9F for line 2+) — set by EN engine.
pub const VWF_TILE_BASE: u16 = 0xDFDC;
/// VWF compositing output buffer (32 bytes).
pub const VWF_COMPOSE_BUF: u16 = 0xDF90;

// === VDP ports ===

/// VDP data port.
pub const VDP_DATA: u8 = 0xBE;
/// VDP control port.
pub const VDP_CTRL: u8 = 0xBF;

// === Bank numbers ===

/// Bank 8: contains the EN proportional-width VWF engine.
pub const BANK_VWF_ENGINE: u8 = 8;
/// Bank 32: contains ko_render.
pub const BANK_KO_RENDER: u8 = 32;
/// Bank 33: first bank of 4bpp Korean glyph data.
pub const BANK_FONT_START: u8 = 33;

// === Phase 3b relocation wiring (Task 2) ===
//
// Task 1 (`text::relocation`) moved bank 3's three tables and bank 5's two
// tables into spare banks 35/36 to fix pointer-table overflow. The game's
// text-window/sprite lookup routines still hard-code CALL/JR targets that
// read banks 3/5 directly; these constants wire six such entry points
// through three small stubs that redirect into the spare banks instead.
// See docs/reverse_engineering/phase3b_z80_wiring.md for the byte-level
// design — every original-byte value below was re-verified against the
// real EN ROM (`xxd`) before being hard-coded here and as an
// `Expect::Bytes` precondition in `commands::build`.

/// Stub A: `LD B,35; JP $22CC` (5B) — bank3 main/secondary/third
/// text-window redirect (entries #1-#3). Placed in reclaimed dead code
/// (original bytes `AF 18 02 3E 01` — unreachable in the EN ROM, no
/// absolute or relative caller found by exhaustive scan).
pub const STUB_TEXTWINDOW_A_ADDR: usize = 0x229E;
/// Stub B: `LD B,35; JP $232D` (5B) — bank3 secondary/third sprite
/// redirect (entries #4-#5). Placed over the old bank5 stub's tail
/// (original bytes `18 65 06 05 18`); that old stub's only caller ($763E)
/// is repointed by entry #6, so this region is dead once #6 is also
/// applied — see the atomicity note on `ENTRY6_BANK5_SHARED_CALL_ADDR`.
pub const STUB_SPRITE_B_ADDR: usize = 0x22C4;
/// Stub C: `LD B,36; JP $22CC` (5B) — bank5 main+secondary shared-tail
/// redirect (entry #6). Placed in reclaimed dead code (original bytes
/// `21 3E C9 18 03`).
pub const STUB_TEXTWINDOW_C_ADDR: usize = 0x26AA;

// Phase 3c: bank 3 "third" split out of spare 35 into its own spare bank
// 37 (headroom fix — see `text::relocation::RELOCATION_MAP` doc comment).
// Stubs D/E redirect "third"'s two entry points (#2 text-window, #5
// sprite) there instead of stub A/B.
//
// Both are placed in a second reclaimed dead-code region at
// `$1AD6-$1AE1` (12B, only 10B used): four back-to-back 3-byte
// instructions (`CALL $22F3; CALL $2725; CALL $2BF4; JP $218C`)
// immediately preceded by a `RET` (`$1AD5`, so not fallthrough-reachable)
// and followed by an unrelated `LDIR`-setup routine at `$1AE2` (`LD
// HL,$C6C0`, confirmed via `cargo run -- disasm --range 0x1AD0-0x1AF0`).
// A full-ROM scan for absolute `CALL/JP $1AD6` and in-bank `JR`/`DJNZ`
// targeting `$1AD6-$1AE1` found zero hits — same "no static XREF, indirect
// jump not excluded" caveat as `$229E`/`$22C4`/`$26AA` above; final
// confirmation is the emulator playtest, not this scan alone.

/// Stub D: `LD B,37; JP $22CC` (5B) — bank3 "third" text-window redirect
/// (entry #2). Original bytes `CD F3 22 CD 25` (first 5B of the reclaimed
/// $1AD6 region).
pub const STUB_TEXTWINDOW_D_ADDR: usize = 0x1AD6;
/// Stub E: `LD B,37; JP $232D` (5B) — bank3 "third" sprite redirect (entry
/// #5). Original bytes `27 CD F4 2B C3` (last 5B of the reclaimed $1AD6
/// region; the final 2 bytes at $1AE0-$1AE1, `8C 21`, are left untouched —
/// orphaned tail of the old `JP $218C` operand, unreferenced since nothing
/// points into the middle of dead code).
pub const STUB_SPRITE_E_ADDR: usize = 0x1ADB;

/// All relocation stubs are 5 bytes: `LD B,n` (2B) + `JP nn` (3B).
pub const RELOCATION_STUB_SIZE: usize = 5;

/// Text-window bank-switch continuation. The original $22CA routine opens
/// with `LD B,3` (2B) before falling into a shared bank-switch-and-restore
/// sequence; $22CC is the byte immediately after that `LD B,3`. Stubs A and
/// C jump here after setting B to their own spare bank number, so the
/// continuation code runs completely unmodified.
pub const TEXTWINDOW_CONTINUATION_ADDR: u16 = 0x22CC;
/// Sprite bank-switch continuation — same relationship as
/// `TEXTWINDOW_CONTINUATION_ADDR`, but for the original $232B routine's
/// `LD B,3` prologue. Stub B jumps here.
pub const SPRITE_CONTINUATION_ADDR: u16 = 0x232D;

/// #1: bank3 main text-window JR displacement byte @ $22C0 (opcode `18` @
/// $22BF). Original displacement `09` (target $22CA); redirected to stub A.
pub const ENTRY1_BANK3_MAIN_TEXTWINDOW_JR_ADDR: usize = 0x22C0;
/// #2: bank3 third text-window CALL operand @ $2288 (opcode `CD` @ $2287).
/// Original operand `CA 22` ($22CA); redirected to stub D (spare 37,
/// Phase 3c — "third" no longer shares stub A/spare 35 with main/secondary).
pub const ENTRY2_BANK3_THIRD_TEXTWINDOW_CALL_ADDR: usize = 0x2288;
/// #3: bank3 secondary text-window CALL operand @ $229A (opcode `CD` @
/// $2299). Original operand `CA 22` ($22CA); redirected to stub A.
pub const ENTRY3_BANK3_SECONDARY_TEXTWINDOW_CALL_ADDR: usize = 0x229A;
/// #4: bank3 secondary sprite CALL operand @ $22AC (opcode `CD` @ $22AB).
/// Original operand `2B 23` ($232B); redirected to stub B.
pub const ENTRY4_BANK3_SECONDARY_SPRITE_CALL_ADDR: usize = 0x22AC;
/// #5: bank3 third sprite CALL operand @ $446C (opcode `CD` @ $446B).
/// Physically resides in bank1 (fixed code bank) — bank3 is only a slot2
/// *data* bank here; `0x446C` is a bank1 physical address, not evidence of
/// a bank3 code page (the coincidence that `0x446C / 0x4000 == 1` is just
/// that: a coincidence). Original operand `2B 23` ($232B); redirected to
/// stub E (spare 37, Phase 3c — see entry #2's doc comment).
pub const ENTRY5_BANK3_THIRD_SPRITE_CALL_ADDR: usize = 0x446C;
/// #6: bank5 main+secondary shared-tail CALL operand @ $763F (opcode `CD` @
/// $763E). Physically resides in bank1 (fixed code bank) — bank5 is only
/// ever a slot2 *data* bank, never a code bank; `0x763F` is a bank1
/// physical address. Original operand `C6 22` (the OLD bank5 stub at
/// $22C6); redirected to stub C.
///
/// **Atomicity**: this entry and stub C (`STUB_TEXTWINDOW_C_ADDR`) MUST be
/// written together. Stub B (`STUB_SPRITE_B_ADDR`, $22C4-$22C8) overlaps
/// the old bank5 stub at $22C6 — if entry #6 is skipped while stub B is
/// applied, $763E still calls $22C6, which now contains stub B's tail
/// bytes (`C3 2D 23` = `JP $232D`) with no preceding `LD B,n`, silently
/// switching slot 2 to whatever B last held (silent corruption). See
/// docs/reverse_engineering/phase3b_z80_wiring.md's 원자성 불변식.
pub const ENTRY6_BANK5_SHARED_CALL_ADDR: usize = 0x763F;
