//! JP-native rebuild path (parallel to the EN-base build): raw expansion,
//! M1 renderer proof, and M2 translated text-bank relocation.

use anyhow::{Context, Result, bail};
use gg_sms::header::TmrSegaHeader;
use sha2::{Digest, Sha256};
use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::Path;

pub mod engine;
pub mod relocation;
pub mod render;
pub mod tables;

const JP_SIZE: usize = 0x8_0000; // 512 KiB
const TARGET_SIZE: usize = 0x10_0000; // 1 MiB
const BANK_SIZE: usize = 0x4000;
pub const JP_ROM_SHA256: [u8; 32] = [
    0x4A, 0x87, 0xF0, 0x2F, 0x35, 0x86, 0x88, 0xBC, 0x76, 0x80, 0xD0, 0xD3, 0x4F, 0x52, 0x7E, 0x10,
    0xA0, 0x87, 0xFE, 0xC9, 0x5D, 0xBF, 0x7E, 0xD1, 0x31, 0x24, 0x1D, 0x8F, 0xFE, 0x4C, 0x06, 0x54,
];

pub fn verify_supported_jp_rom(jp: &[u8]) -> Result<()> {
    if jp.len() != JP_SIZE {
        bail!("expected 512 KiB JP ROM, got {} bytes", jp.len());
    }
    let digest: [u8; 32] = Sha256::digest(jp).into();
    if digest != JP_ROM_SHA256 {
        bail!(
            "unsupported JP ROM SHA-256: expected {:02x?}, got {:02x?}",
            JP_ROM_SHA256,
            digest
        );
    }
    Ok(())
}

fn write_at(rom: &mut [u8], off: usize, data: &[u8]) {
    rom[off..off + data.len()].copy_from_slice(data);
}

fn expect_bytes(rom: &[u8], off: usize, expected: &[u8], label: &str) -> Result<()> {
    let Some(got) = rom.get(off..off + expected.len()) else {
        bail!(
            "{label} signature out of range at ${off:04X}: need {} bytes, ROM has {}",
            expected.len(),
            rom.len()
        );
    };
    if got != expected {
        bail!("{label} signature mismatch at ${off:04X}: expected {expected:02X?}, got {got:02X?}");
    }
    Ok(())
}

fn expect_sha256(
    rom: &[u8],
    off: usize,
    len: usize,
    expected: &[u8; 32],
    label: &str,
) -> Result<()> {
    let Some(got) = rom.get(off..off + len) else {
        bail!(
            "{label} signature out of range at ${off:04X}: need {len} bytes, ROM has {}",
            rom.len()
        );
    };
    let digest: [u8; 32] = Sha256::digest(got).into();
    if &digest != expected {
        bail!(
            "{label} SHA-256 mismatch at ${off:04X}: expected {expected:02X?}, got {digest:02X?}"
        );
    }
    Ok(())
}

/// M1 render-test build: JP raw expansion + JP-native renderer patches
/// (dispatch hook, ko_dispatch, ko_render) + Korean font + a **brute test
/// injection** that overwrites the first 2 bytes of every active bank 3/5 text
/// entry with `$F0 $00` (JP-direct KO syllable #0). This makes the first cell
/// of every dialogue render the Korean glyph #0 — a throwaway ROM to prove the
/// JP-native render path works end-to-end in the emulator. Not a shipping build.
pub fn build_render(jp: &[u8], ttf_path: &Path, translations_dir: &Path) -> Result<Vec<u8>> {
    verify_supported_jp_rom(jp)?;
    let checked_texts = tables::verify_reserved_ko_prefixes_unused(jp)
        .context("verify JP-direct KO prefixes are unused")?;
    println!("  JP KO prefix audit: $F0-$F2 absent from {checked_texts} native strings");
    let mut rom = expand_to_1mb(jp)?;

    apply_renderer(&mut rom)?;

    // --- Korean font (banks 33-34) ---
    let translations = crate::translation::loader::load_translations(translations_dir)
        .context("load translations for font")?;
    let glyphs = collect_jp_glyphs(&translations);
    write_jp_font(&mut rom, ttf_path, &glyphs)?;

    // --- Brute test injection: first 2 bytes of every active entry → $F0 $00 ---
    let mut injected = 0usize;
    for &bank in &[3u8, 5u8] {
        let config = crate::text::bank::config_for_bank(bank)
            .with_context(|| format!("no config for bank {bank}"))?;
        let base = config.physical_base;
        for table in config.tables {
            for id in 0..table.ptr_count as usize {
                let po = base + table.ptr_bank_offset as usize + id * 2;
                let ptr = u16::from_le_bytes([rom[po], rom[po + 1]]);
                if ptr == 0 {
                    continue;
                }
                let off = base + (ptr as usize + table.base_logical as usize - 0x8000);
                if off + 1 < base + BANK_SIZE {
                    rom[off] = render::JP_KO_PREFIX_START;
                    rom[off + 1] = 0x00;
                    injected += 1;
                }
            }
        }
    }
    println!("  KO test injection: {injected} entries → $F0 $00 (syllable #0)");

    TmrSegaHeader::update_checksum(&mut rom);
    Ok(rom)
}

fn apply_renderer(rom: &mut [u8]) -> Result<()> {
    engine::verify_hook(rom).context("verify JP dispatch hook signature")?;
    let clear_hook_off = render::JP_WINDOW_CLEAR_HOOK_ADDR as usize;
    expect_bytes(
        rom,
        clear_hook_off,
        &render::JP_WINDOW_CLEAR_ORIG,
        "JP window-clear hook",
    )?;
    expect_bytes(
        rom,
        render::JP_WINDOW_RESET_STUB_ADDR as usize,
        &render::JP_WINDOW_RESET_STUB_ORIG,
        "JP window-reset stub",
    )?;
    expect_bytes(
        rom,
        render::JP_RETURN_STUB_ADDR as usize,
        &render::JP_RETURN_STUB_ORIG,
        "JP return stub",
    )?;
    expect_bytes(
        rom,
        render::JP_NATIVE_DISPATCH_STUB_ADDR as usize,
        &render::JP_NATIVE_DISPATCH_STUB_ORIG,
        "JP native dispatch stub",
    )?;

    let reset_stub = render::assemble_jp_window_reset_stub()
        .map_err(|e| anyhow::anyhow!("window-reset stub asm: {e}"))?;
    let ko_dispatch =
        render::assemble_jp_ko_dispatch().map_err(|e| anyhow::anyhow!("ko_dispatch asm: {e}"))?;
    let ko_render =
        render::assemble_jp_ko_render().map_err(|e| anyhow::anyhow!("ko_render asm: {e}"))?;
    let native_dispatch_stub = render::assemble_jp_native_dispatch_stub()
        .map_err(|e| anyhow::anyhow!("native dispatch stub asm: {e}"))?;

    let dispatch_free = vec![0x00; ko_dispatch.len()];
    expect_bytes(
        rom,
        render::KO_DISPATCH_JP_ADDR as usize,
        &dispatch_free,
        "JP ko_dispatch region",
    )?;
    let render_free = vec![0xFF; ko_render.len()];
    expect_bytes(
        rom,
        crate::patch::constants::KO_RENDER_PHYSICAL,
        &render_free,
        "JP ko_render region",
    )?;

    // Every destination signature is known at this point; mutate only after
    // the complete renderer patch set has passed its preconditions.
    write_at(rom, 0x0238A, &render::assemble_jp_dispatch_patch());
    write_at(
        rom,
        clear_hook_off,
        &render::assemble_jp_window_clear_patch(),
    );
    write_at(rom, render::JP_WINDOW_RESET_STUB_ADDR as usize, &reset_stub);
    write_at(rom, render::KO_DISPATCH_JP_ADDR as usize, &ko_dispatch);
    write_at(rom, crate::patch::constants::KO_RENDER_PHYSICAL, &ko_render);
    write_at(
        rom,
        render::JP_RETURN_STUB_ADDR as usize,
        &render::assemble_jp_return_stub(),
    );
    write_at(
        rom,
        render::JP_NATIVE_DISPATCH_STUB_ADDR as usize,
        &native_dispatch_stub,
    );
    println!(
        "  Patches: dispatch(5B @$238A→$28A9) + window_reset({}B @${:04X} via $2194) + ko_dispatch({}B @$28A9) + ko_render({}B @$80000) + return_stub(7B @${:04X}) + native_stub({}B @${:04X})",
        reset_stub.len(),
        render::JP_WINDOW_RESET_STUB_ADDR,
        ko_dispatch.len(),
        ko_render.len(),
        render::JP_RETURN_STUB_ADDR,
        native_dispatch_stub.len(),
        render::JP_NATIVE_DISPATCH_STUB_ADDR,
    );

    Ok(())
}

fn collect_jp_glyphs(translations: &[crate::translation::loader::BankTranslation]) -> Vec<char> {
    let all: Vec<(String, bool)> = translations
        .iter()
        .flat_map(|bt| bt.entries.iter().map(|e| (e.ko.clone(), e.skip)))
        .collect();
    crate::text::encoding::collect_korean_syllables(&all)
}

fn write_jp_font(rom: &mut [u8], ttf_path: &Path, glyphs: &[char]) -> Result<()> {
    let font = crate::font::korean::generate_korean_font(ttf_path, 8.0, glyphs, 128)
        .context("generate korean font")?;
    let font_free = vec![0xFF; font.font_4bpp.len()];
    expect_bytes(
        rom,
        crate::patch::constants::FONT_4BPP_PHYSICAL,
        &font_free,
        "JP font region",
    )?;
    write_at(
        rom,
        crate::patch::constants::FONT_4BPP_PHYSICAL,
        &font.font_4bpp,
    );
    println!(
        "  Font: {} glyphs, {}B 4bpp @$84000",
        glyphs.len(),
        font.font_4bpp.len()
    );
    Ok(())
}

/// JP 원본의 베이크드 UI 타일을 한글 타일로 교체한다. 각 블록은 지원 ROM에서 직접 읽고
/// SHA-256을 검증한 뒤 프레임 소스로 사용한다.
fn apply_jp_baked_ui(rom: &mut [u8], ttf_path: &Path) -> Result<()> {
    use crate::patch::{menu_tiles, title_logo};

    let money_addr = crate::patch::constants::MONEY_KANJI_GLYPH_ADDR;
    let money_jp = [0x10, 0x28, 0x7C, 0x92, 0x7C, 0x10, 0x54, 0xFE];
    let money_ko = [0x00, 0x7C, 0x04, 0xFE, 0x00, 0x7C, 0x44, 0x7C];

    // 전체 JP 시그니처를 먼저 검증해 일부 블록만 적용된 중간 상태를 만들지 않는다.
    expect_bytes(rom, money_addr, &money_jp, "JP money kanji glyph")?;
    expect_sha256(
        rom,
        menu_tiles::MENU_BUTTON_BASE,
        menu_tiles::MENU_BUTTON_COUNT * menu_tiles::MENU_BUTTON_BYTES,
        &menu_tiles::MENU_BUTTONS_JP_SHA256,
        "JP menu buttons",
    )?;
    expect_sha256(
        rom,
        menu_tiles::SHOP_BUTTON_BASE,
        menu_tiles::SHOP_BUTTON_BYTES,
        &menu_tiles::SHOP_BUTTONS_JP_SHA256,
        "JP shop buttons",
    )?;
    expect_sha256(
        rom,
        menu_tiles::FILE_BUTTON_BASE,
        menu_tiles::FILE_BUTTON_BYTES,
        &menu_tiles::FILE_BUTTONS_JP_SHA256,
        "JP file buttons",
    )?;
    expect_sha256(
        rom,
        menu_tiles::YESNO_BUTTON_BASE,
        2 * menu_tiles::MENU_BUTTON_BYTES,
        &menu_tiles::YESNO_BUTTONS_JP_SHA256,
        "JP yes/no buttons",
    )?;
    expect_sha256(
        rom,
        menu_tiles::DIRECTION_BUTTON_BASE,
        menu_tiles::DIRECTION_BUTTON_BYTES,
        &menu_tiles::DIRECTION_BUTTONS_JP_SHA256,
        "JP direction-stone buttons",
    )?;
    let menu_jp = rom[menu_tiles::MENU_BUTTON_BASE
        ..menu_tiles::MENU_BUTTON_BASE
            + menu_tiles::MENU_BUTTON_COUNT * menu_tiles::MENU_BUTTON_BYTES]
        .to_vec();
    let shop_jp = rom[menu_tiles::SHOP_BUTTON_BASE
        ..menu_tiles::SHOP_BUTTON_BASE + menu_tiles::SHOP_BUTTON_BYTES]
        .to_vec();
    let file_jp = rom[menu_tiles::FILE_BUTTON_BASE
        ..menu_tiles::FILE_BUTTON_BASE + menu_tiles::FILE_BUTTON_BYTES]
        .to_vec();
    let yesno_jp = rom[menu_tiles::YESNO_BUTTON_BASE
        ..menu_tiles::YESNO_BUTTON_BASE + 2 * menu_tiles::MENU_BUTTON_BYTES]
        .to_vec();
    let direction_jp = rom[menu_tiles::DIRECTION_BUTTON_BASE
        ..menu_tiles::DIRECTION_BUTTON_BASE + menu_tiles::DIRECTION_BUTTON_BYTES]
        .to_vec();

    let menu_ko = menu_tiles::generate_menu_buttons_jp(ttf_path, &menu_jp)
        .context("generate JP-based Hangul menu buttons")?;
    let shop_ko = menu_tiles::generate_shop_block_jp(ttf_path, &shop_jp)
        .context("generate JP-based Hangul shop buttons")?;
    let file_ko = menu_tiles::generate_file_block_jp(ttf_path, &file_jp)
        .context("generate JP-based Hangul file buttons")?;
    let yesno_ko = menu_tiles::generate_yesno_buttons_jp(ttf_path, &yesno_jp)
        .context("generate JP-based Hangul yes/no buttons")?;
    let direction_ko = menu_tiles::generate_direction_buttons_jp(ttf_path, &direction_jp)
        .context("generate JP-based Hangul direction-stone buttons")?;
    let title_screen_patch = title_logo::prepare_jp(rom, ttf_path)
        .context("prepare atomic JP-source Korean title screen")?;

    let title_screen = title_logo::apply_prepared(rom, title_screen_patch)
        .context("apply atomic JP-source Korean title screen")?;
    write_at(rom, money_addr, &money_ko);
    write_at(rom, menu_tiles::MENU_BUTTON_BASE, &menu_ko);
    write_at(rom, menu_tiles::SHOP_BUTTON_BASE, &shop_ko);
    write_at(rom, menu_tiles::FILE_BUTTON_BASE, &file_ko);
    write_at(rom, menu_tiles::YESNO_BUTTON_BASE, &yesno_ko);
    write_at(rom, menu_tiles::DIRECTION_BUTTON_BASE, &direction_ko);

    println!(
        "  JP baked UI: logo {} tiles/{} px → bank {}, menu {}B, shop {}B, title {}B, file {}B, yes/no {}B, direction {}B, 金→금 8B",
        title_screen.title_tiles,
        title_screen.changed_pixels,
        title_screen.bank,
        menu_ko.len(),
        shop_ko.len(),
        title_screen.title_button_bytes,
        file_ko.len(),
        yesno_ko.len(),
        direction_ko.len()
    );
    Ok(())
}

#[derive(Debug, PartialEq, Eq)]
struct CorpusStats {
    translated: usize,
    source_copy: usize,
    non_text: usize,
    active: usize,
}

/// Find one control byte in a JP-direct stream while skipping the second byte
/// of `$F0-$F2` Korean glyph encodings.
fn jp_direct_control_index(data: &[u8], control: u8) -> Option<usize> {
    let mut offset = 0usize;
    while offset < data.len() {
        let byte = data[offset];
        if (render::JP_KO_PREFIX_START..=render::JP_KO_PREFIX_END).contains(&byte) {
            offset = offset.saturating_add(2);
            continue;
        }
        if byte == control {
            return Some(offset);
        }
        offset += 1;
    }
    None
}

fn count_jp_direct_controls(data: &[u8], control: u8) -> usize {
    let mut count = 0usize;
    let mut remaining = data;
    while let Some(offset) = jp_direct_control_index(remaining, control) {
        count += 1;
        remaining = &remaining[offset + 1..];
    }
    count
}

fn normalize_display_name(text: &str) -> String {
    text.chars()
        .filter(|ch| !matches!(ch, '\n' | '□' | ' '))
        .collect()
}

/// Bank 3/third spell and item descriptions are the runtime name authority.
/// Layout-only newlines and source padding do not change the canonical name;
/// every other string that names the same JP entry must use that name instead
/// of an EN-derived alias.
fn validate_menu_name_surfaces(
    translations: &[crate::translation::loader::BankTranslation],
) -> Result<()> {
    let mut canonical_names = Vec::new();

    for bt in translations {
        if bt.bank != 3 || bt.table != "third" {
            continue;
        }
        for entry in &bt.entries {
            if !(11..=45).contains(&entry.id) {
                continue;
            }
            let Some(jp_title) = entry
                .jp
                .strip_prefix('♪')
                .and_then(|text| text.split_once('↓').map(|(title, _)| title))
            else {
                continue;
            };
            let ko_title = entry
                .ko
                .strip_prefix('♪')
                .and_then(|text| text.split_once('↓').map(|(title, _)| title))
                .with_context(|| {
                    format!(
                        "name description id {} has no KO title terminator",
                        entry.id
                    )
                })?;
            if ko_title.contains('□') {
                bail!(
                    "name description id {} retains source padding: {ko_title:?}",
                    entry.id
                );
            }
            canonical_names.push((
                normalize_display_name(jp_title),
                normalize_display_name(ko_title),
            ));
        }
    }

    for bt in translations {
        for entry in &bt.entries {
            let jp = normalize_display_name(&entry.jp);
            let ko = normalize_display_name(&entry.ko);
            for (jp_name, ko_name) in &canonical_names {
                if jp.contains(jp_name) && !ko.contains(ko_name) {
                    bail!(
                        "menu name mismatch at bank {} {} id {}: JP {jp_name:?} requires KO {ko_name:?}",
                        bt.bank,
                        bt.table,
                        entry.id
                    );
                }
            }
        }
    }

    Ok(())
}

/// Production corpus gate for the JP-direct path.
///
/// Every active JP pointer must have exactly one translation record. Empty KO
/// text is only accepted when explicitly marked as a source-copy (`skip` or
/// `status: "skip"`) or a verified `non_text` sentinel, so a missing
/// translation cannot silently ship as JP.
fn validate_translation_corpus(
    jp: &[u8],
    translations: &[crate::translation::loader::BankTranslation],
) -> Result<CorpusStats> {
    validate_menu_name_surfaces(translations).context("JP menu-name consistency gate")?;
    crate::translation::layout::validate_declared_layout_constraints(translations)
        .context("JP declared layout gate")?;

    let mut seen_tables = HashSet::new();
    let mut seen_ids: HashMap<(u8, String), HashSet<u16>> = HashMap::new();
    let mut translated = 0usize;
    let mut source_copy = 0usize;
    let mut non_text = 0usize;

    for bt in translations {
        let config = crate::text::bank::config_for_bank(bt.bank)
            .with_context(|| format!("unknown translation bank {}", bt.bank))?;
        let table = config
            .tables
            .iter()
            .find(|table| table.name == bt.table)
            .with_context(|| format!("unknown translation table {} {}", bt.bank, bt.table))?;
        let table_key = (bt.bank, bt.table.clone());
        if !seen_tables.insert(table_key.clone()) {
            bail!(
                "duplicate merged translation table {} {}",
                bt.bank,
                bt.table
            );
        }
        let ids = seen_ids.entry(table_key).or_default();

        for entry in &bt.entries {
            if entry.id >= table.ptr_count {
                bail!(
                    "translation bank {} {} id {} exceeds pointer count {}",
                    bt.bank,
                    bt.table,
                    entry.id,
                    table.ptr_count
                );
            }
            if !ids.insert(entry.id) {
                bail!(
                    "duplicate translation bank {} {} id {}",
                    bt.bank,
                    bt.table,
                    entry.id
                );
            }
            let ptr_off =
                config.physical_base + table.ptr_bank_offset as usize + entry.id as usize * 2;
            let ptr = u16::from_le_bytes([jp[ptr_off], jp[ptr_off + 1]]);
            if ptr == 0 {
                bail!(
                    "translation bank {} {} id {} targets an inactive JP pointer",
                    bt.bank,
                    bt.table,
                    entry.id
                );
            }

            let copies_source = entry.skip || entry.ko.is_empty();
            if copies_source {
                if !entry.ko.is_empty() {
                    bail!(
                        "translation bank {} {} id {} has skip=true with non-empty KO text",
                        bt.bank,
                        bt.table,
                        entry.id
                    );
                }
                if entry.status == "non_text" {
                    if !entry.skip {
                        bail!(
                            "translation bank {} {} id {} has status=non_text without skip=true",
                            bt.bank,
                            bt.table,
                            entry.id
                        );
                    }
                    if bt.bank != 3 || bt.table != "third" || entry.id != 255 {
                        bail!(
                            "translation bank {} {} id {} is not an approved non-text sentinel",
                            bt.bank,
                            bt.table,
                            entry.id
                        );
                    }
                    non_text += 1;
                } else if !entry.skip && entry.status != "skip" {
                    bail!(
                        "translation bank {} {} id {} has empty KO text without explicit skip status",
                        bt.bank,
                        bt.table,
                        entry.id
                    );
                } else {
                    source_copy += 1;
                }
            } else {
                if entry.status == "skip" || entry.status == "non_text" {
                    bail!(
                        "translation bank {} {} id {} has status={} with non-empty KO text",
                        bt.bank,
                        bt.table,
                        entry.id,
                        entry.status
                    );
                }
                if entry.status != "complete" {
                    bail!(
                        "translation bank {} {} id {} has release-ineligible status={}",
                        bt.bank,
                        bt.table,
                        entry.id,
                        entry.status
                    );
                }
                translated += 1;
            }
        }
    }

    let mut active = 0usize;
    for &bank in &[3u8, 5u8] {
        let config = crate::text::bank::config_for_bank(bank).expect("known JP text bank");
        for table in config.tables {
            let key = (bank, table.name.to_string());
            let ids = seen_ids
                .get(&key)
                .with_context(|| format!("missing translation table {bank} {}", table.name))?;
            for id in 0..table.ptr_count {
                let ptr_off =
                    config.physical_base + table.ptr_bank_offset as usize + id as usize * 2;
                let ptr = u16::from_le_bytes([jp[ptr_off], jp[ptr_off + 1]]);
                if ptr != 0 {
                    active += 1;
                    if !ids.contains(&id) {
                        bail!(
                            "active JP entry bank {bank} {} id {id} has no translation record",
                            table.name
                        );
                    }
                }
            }
        }
    }

    if translated + source_copy + non_text != active {
        bail!(
            "translation corpus cardinality mismatch: {} translated + {} source-copy + {} non-text != {} active JP entries",
            translated,
            source_copy,
            non_text,
            active
        );
    }

    Ok(CorpusStats {
        translated,
        source_copy,
        non_text,
        active,
    })
}

/// Normalize the bank3/third records whose JP pointer ranges cross from the
/// ending-credit stream into adjacent system data.
///
/// JP id 133 starts a fixed-page ending/credit stream with no `$FE`. Id 255
/// points exactly at the following non-text system-data suffix. Treating both
/// as ordinary unterminated strings copies the rest of bank 3 (4 KiB+) into
/// spare 37 and cannot fit. Id 133 is the translated suffix of the full ending
/// chain at id 160; id 255 becomes a defensive immediate END in the relocated
/// *text* table. The original bank-3 system data is never modified.
fn normalize_jp_mixed_third_entries(
    jp: &[u8],
    partitioned: &mut crate::text::relocation::PartitionedTranslations,
) -> Result<()> {
    const CREDITS_ID: u16 = 133;
    const DATA_SENTINEL_ID: u16 = 255;
    const ENDING_CHAIN_ID: u16 = 160;

    let source_config = crate::text::bank::config_for_bank(3).expect("bank3 config");
    let source_table = source_config
        .tables
        .iter()
        .find(|table| table.name == "third")
        .expect("bank3 third config");
    let pointer = |id: u16| {
        let off =
            source_config.physical_base + source_table.ptr_bank_offset as usize + id as usize * 2;
        u16::from_le_bytes([jp[off], jp[off + 1]])
    };
    let bank_offset = |ptr: u16| ptr as usize + (source_table.base_logical as usize - 0x8000);
    let credits_start = bank_offset(pointer(CREDITS_ID));
    let data_start = bank_offset(pointer(DATA_SENTINEL_ID));
    if credits_start >= data_start || data_start > BANK_SIZE {
        bail!(
            "JP mixed third boundary invalid: id133=${credits_start:04X}, id255=${data_start:04X}"
        );
    }
    let credits =
        &jp[source_config.physical_base + credits_start..source_config.physical_base + data_start];
    if credits.len() != 274 {
        bail!(
            "JP mixed third credits boundary drift: expected 274 bytes, got {}",
            credits.len()
        );
    }
    if credits.last() != Some(&crate::text::control::CTRL_PAGE) {
        bail!(
            "JP mixed third credits boundary drift: expected id133 prefix to end in PAGE, got {:02X?}",
            credits.last()
        );
    }

    let sources = partitioned
        .by_spare_bank
        .get_mut(&37)
        .context("JP mixed third normalization requires spare bank 37")?;
    if sources.len() != 1 {
        bail!(
            "JP spare bank 37 must contain exactly one source table, got {}",
            sources.len()
        );
    }
    let entries = &mut sources[0].entries;
    let credits_idx = entries
        .iter()
        .position(|entry| entry.id == CREDITS_ID)
        .context("JP mixed third credits entry 133 missing")?;
    let ending_idx = entries
        .iter()
        .position(|entry| entry.id == ENDING_CHAIN_ID)
        .context("JP mixed third ending chain entry 160 missing")?;
    if entries[credits_idx].skip || entries[credits_idx].data.is_empty() {
        bail!("JP mixed third entry 133 must be an explicit translation");
    }
    if !entries[ending_idx]
        .data
        .ends_with(&entries[credits_idx].data)
    {
        bail!("JP mixed third entry 133 must match the translated id160 suffix");
    }
    let source_page_count = credits
        .iter()
        .filter(|&&byte| byte == crate::text::control::CTRL_PAGE)
        .count();
    let translated_page_count =
        count_jp_direct_controls(&entries[credits_idx].data, crate::text::control::CTRL_PAGE);
    if translated_page_count != source_page_count {
        bail!(
            "JP mixed third entry 133 PAGE count drift: source boundary has {source_page_count}, translated suffix has {translated_page_count}"
        );
    }
    if !entries[credits_idx].data.ends_with(&[
        crate::text::control::CTRL_PAGE,
        crate::text::control::CTRL_CLOSE,
    ]) {
        bail!("JP mixed third entry 133 must preserve the final credit PAGE before END");
    }
    let translated_credits_len = entries[credits_idx].data.len();

    let data_entry = entries
        .iter_mut()
        .find(|entry| entry.id == DATA_SENTINEL_ID)
        .context("JP mixed third data sentinel entry 255 missing")?;
    if !data_entry.skip || !data_entry.data.is_empty() {
        bail!("JP mixed third entry 255 is no longer an explicit source-copy");
    }
    data_entry.data = vec![crate::text::control::CTRL_CLOSE];
    data_entry.skip = false;

    println!(
        "  Mixed third: id133 translated suffix {}B (source boundary {}B); id255 isolated as non-text",
        translated_credits_len,
        credits.len(),
    );
    Ok(())
}

/// M2 production-candidate build: JP-native renderer plus `$F0-$F2` encoded
/// KO text, all five JP pointer tables relocated to spare banks 35/36/37,
/// and JP-verified lookup entry-point wiring. Unlike `build_render`, this
/// does not mutate source text in place or inject a test glyph.
pub fn build_translated(jp: &[u8], ttf_path: &Path, translations_dir: &Path) -> Result<Vec<u8>> {
    verify_supported_jp_rom(jp)?;
    let checked_texts = tables::verify_reserved_ko_prefixes_unused(jp)
        .context("verify JP-direct KO prefixes are unused")?;
    println!("  JP KO prefix audit: $F0-$F2 absent from {checked_texts} native strings");

    let translations = crate::translation::loader::load_translations(translations_dir)
        .context("load JP-direct translations")?;
    let stats = validate_translation_corpus(jp, &translations)
        .context("JP-direct translation corpus gate")?;
    println!(
        "  Corpus: {}/{} text translated, {} source-copy, {} non-text sentinel",
        stats.translated,
        stats.active - stats.non_text,
        stats.source_copy,
        stats.non_text
    );

    let glyphs = collect_jp_glyphs(&translations);
    let ko_enc = crate::text::encoding::KoEncoding::from_syllables_with_prefixes(
        &glyphs,
        [
            render::JP_KO_PREFIX_START,
            render::JP_KO_PREFIX_START + 1,
            render::JP_KO_PREFIX_END,
        ],
    )
    .context("build JP-direct $F0-$F2 encoding")?
    // `$00` is intercepted before JP character dispatch and writes a blank
    // cell. `$42` is the native long dash tile; the EN bytes `$0B/$49`
    // would render live kana (`あ`/`オ`) on the JP base.
    .with_literal_overrides(&[(' ', 0x00), ('-', 0x42)]);
    println!("  Encoding: {} glyphs in $F0-$F2", ko_enc.syllable_count());

    let mut partitioned = crate::text::relocation::partition_translations(&translations, &ko_enc)
        .context("partition JP-direct translations")?;
    normalize_jp_mixed_third_entries(jp, &mut partitioned)
        .context("normalize JP ending/system-data boundary")?;
    if !partitioned.by_original_bank.is_empty() {
        bail!(
            "JP-direct M2 requires all text tables relocated; in-place banks remain: {:?}",
            partitioned.by_original_bank.keys().collect::<Vec<_>>()
        );
    }
    let actual_spares: BTreeSet<u8> = partitioned.by_spare_bank.keys().copied().collect();
    let expected_spares = BTreeSet::from([35u8, 36u8, 37u8]);
    if actual_spares != expected_spares {
        bail!(
            "JP-direct relocation set mismatch: expected {expected_spares:?}, got {actual_spares:?}"
        );
    }

    let mut rom = expand_to_1mb(jp)?;
    for (spare_bank, sources_owned) in &partitioned.by_spare_bank {
        let dest = crate::text::relocation::spare_config_for(*spare_bank)
            .with_context(|| format!("unmapped spare bank {spare_bank}"))?;
        let sources: Vec<_> = sources_owned.iter().map(|source| source.as_ref()).collect();
        let bank_data = crate::text::bank::build_relocated_bank(dest, &sources, &rom)
            .with_context(|| format!("build JP-direct spare bank {spare_bank}"))?;
        write_at(&mut rom, dest.physical_base, &bank_data);
    }

    apply_renderer(&mut rom)?;
    relocation::apply(&mut rom).context("install JP-direct relocation wiring")?;
    write_jp_font(&mut rom, ttf_path, &glyphs)?;
    apply_jp_baked_ui(&mut rom, ttf_path)?;

    TmrSegaHeader::update_checksum(&mut rom);
    Ok(rom)
}

/// Expand a 512 KiB JP ROM to a valid 1 MiB image: preserve the low half
/// byte-for-byte except the header size code + checksum, fill the high half
/// with 0xFF free space.
pub fn expand_to_1mb(jp: &[u8]) -> Result<Vec<u8>> {
    verify_supported_jp_rom(jp)?;
    let mut out = jp.to_vec();
    out.resize(TARGET_SIZE, 0xFF);
    TmrSegaHeader::set_rom_size_code(&mut out, 0x2); // 1 MiB, region preserved
    TmrSegaHeader::update_checksum(&mut out);
    Ok(out)
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

    fn project_path(relative: &str) -> std::path::PathBuf {
        let from_package = std::path::Path::new("../..").join(relative);
        if from_package.exists() {
            from_package
        } else {
            std::path::PathBuf::from(relative)
        }
    }

    #[test]
    fn expand_produces_valid_1mb_image() {
        let Some(jp) = jp_rom() else {
            eprintln!("skip: JP ROM absent");
            return;
        };
        let out = expand_to_1mb(&jp).expect("expand");
        assert_eq!(out.len(), 0x10_0000, "must be 1 MiB");
        // low half identical except header size byte (0x7FFF) + checksum (0x7FFA-B)
        for i in 0..0x8_0000 {
            if i == 0x7FFF || i == 0x7FFA || i == 0x7FFB {
                continue;
            }
            assert_eq!(out[i], jp[i], "low-half byte {i:#07X} changed");
        }
        // high half all 0xFF
        assert!(
            out[0x8_0000..].iter().all(|&b| b == 0xFF),
            "high half not free"
        );
        // header valid + size code set, region preserved
        let hdr = TmrSegaHeader::parse(&out).expect("header parses");
        assert_eq!(hdr.rom_size, 0x2);
        assert_eq!(hdr.region, 0x5, "GG Japan region preserved");
    }

    #[test]
    fn expand_rejects_wrong_size() {
        assert!(expand_to_1mb(&vec![0u8; 0x4_0000]).is_err());
    }

    #[test]
    fn expect_bytes_fails_closed() {
        let rom = [0x10, 0x20, 0x30];
        expect_bytes(&rom, 1, &[0x20, 0x30], "test").expect("matching signature");
        assert!(expect_bytes(&rom, 1, &[0x20, 0x31], "test").is_err());
        assert!(expect_bytes(&rom, 2, &[0x30, 0x40], "test").is_err());
    }

    #[test]
    fn expect_sha256_fails_closed() {
        let rom = [0x10, 0x20, 0x30];
        let expected: [u8; 32] = Sha256::digest(rom).into();
        expect_sha256(&rom, 0, rom.len(), &expected, "test").expect("matching hash");
        assert!(expect_sha256(&rom, 1, 2, &expected, "test").is_err());
        assert!(expect_sha256(&rom, 2, 2, &expected, "test").is_err());
    }

    #[test]
    fn jp_direct_control_scan_skips_glyph_subindices() {
        let stream = [
            render::JP_KO_PREFIX_START,
            crate::text::control::CTRL_PAGE,
            crate::text::control::CTRL_PAGE,
            render::JP_KO_PREFIX_END,
            crate::text::control::CTRL_CLOSE,
            crate::text::control::CTRL_CLOSE,
        ];
        assert_eq!(
            count_jp_direct_controls(&stream, crate::text::control::CTRL_PAGE),
            1
        );
        assert_eq!(
            jp_direct_control_index(&stream, crate::text::control::CTRL_CLOSE),
            Some(5)
        );
    }

    #[test]
    fn current_translation_corpus_covers_every_active_jp_pointer() {
        let Some(jp) = jp_rom() else {
            eprintln!("skip: JP ROM absent");
            return;
        };
        let translations = crate::translation::loader::load_translations(&project_path(
            "assets/translations/complete",
        ))
        .expect("load current translations");
        let stats = validate_translation_corpus(&jp, &translations).expect("corpus gate");
        assert_eq!(
            stats,
            CorpusStats {
                translated: 1028,
                source_copy: 0,
                non_text: 1,
                active: 1029,
            }
        );
    }

    #[test]
    fn current_translation_corpus_has_canonical_menu_names() {
        let translations = crate::translation::loader::load_translations(&project_path(
            "assets/translations/complete",
        ))
        .expect("load current translations");
        validate_menu_name_surfaces(&translations).expect("menu-name consistency gate");
    }

    #[test]
    fn current_translation_declared_layout_constraints() {
        let mut translations = crate::translation::loader::load_translations(&project_path(
            "assets/translations/complete",
        ))
        .expect("load current translations");
        crate::translation::layout::validate_declared_layout_constraints(&translations)
            .expect("declared layout gate");

        let entry = translations
            .iter_mut()
            .find(|bank| bank.bank == 5 && bank.table == "secondary")
            .and_then(|bank| bank.entries.iter_mut().find(|entry| entry.id == 84))
            .expect("bank 5 secondary id 84");
        entry.ko = "주르륵!\n미끄러져서\n멈출 수가 없어~".to_string();
        let error = crate::translation::layout::validate_declared_layout_constraints(&translations)
            .expect_err("9-tile line must fail the declared 7-tile window");
        assert!(error.to_string().contains("bank 5 secondary id 84"));
    }

    #[test]
    fn current_ending_credit_rows_and_final_page_are_safe() {
        let translations = crate::translation::loader::load_translations(&project_path(
            "assets/translations/complete",
        ))
        .expect("load current translations");
        let third = translations
            .iter()
            .find(|bank| bank.bank == 3 && bank.table == "third")
            .expect("bank 3 third");

        let credit_rows: Vec<_> = third
            .entries
            .iter()
            .filter(|entry| (178..=198).contains(&entry.id))
            .collect();
        assert_eq!(credit_rows.len(), 21, "staff roll must keep all 21 rows");
        for entry in credit_rows {
            let row = entry
                .ko
                .split("{PAGE}")
                .next()
                .expect("credit row before PAGE");
            assert!(
                !row.contains('\n'),
                "staff-roll row {} must remain one line",
                entry.id
            );
            let cells = crate::text::encoding::collapse_ellipsis(row)
                .chars()
                .filter_map(crate::text::encoding::normalize_glyph_char)
                .count();
            assert_eq!(
                cells, 17,
                "staff-roll row {} must preserve the original 17-cell width",
                entry.id
            );
        }

        let ending_suffixes: Vec<_> = third
            .entries
            .iter()
            .filter(|entry| entry.id == 133 || (160..=254).contains(&entry.id))
            .collect();
        assert_eq!(
            ending_suffixes.len(),
            93,
            "ending chain active-suffix count drift"
        );
        for entry in ending_suffixes {
            assert!(
                entry.ko.ends_with("{PAGE}"),
                "ending suffix {} must scroll the final credit before END",
                entry.id
            );
        }
    }

    #[test]
    fn current_momomo_shop_voice_has_no_regional_dialect() {
        let translations = crate::translation::loader::load_translations(&project_path(
            "assets/translations/complete",
        ))
        .expect("load current translations");
        let momomo = translations
            .iter()
            .find(|bank| bank.bank == 5 && bank.table == "secondary")
            .expect("bank 5 secondary");
        let dialect_markers = [
            "라예",
            "랍니꺼",
            "입니더",
            "심더",
            "그라모",
            "그라믄",
            "갑네예",
            "아뿝시더",
        ];

        for entry in momomo
            .entries
            .iter()
            .filter(|entry| (30..=64).contains(&entry.id))
        {
            for marker in dialect_markers {
                assert!(
                    !entry.ko.contains(marker),
                    "bank 5 secondary id {} reintroduces regional dialect {marker:?}",
                    entry.id
                );
            }
        }
    }

    #[test]
    fn jp_baked_ui_rejects_signature_drift() {
        let Some(jp) = jp_rom() else {
            eprintln!("skip: JP ROM absent");
            return;
        };
        let mut rom = expand_to_1mb(&jp).expect("expand");
        rom[crate::patch::menu_tiles::DIRECTION_BUTTON_BASE] ^= 0x01;
        assert!(
            apply_jp_baked_ui(&mut rom, &project_path("assets/fonts/dalmoori.ttf")).is_err(),
            "JP 방향석 원본 시그니처가 바뀌면 fail-closed 해야 한다"
        );
    }

    #[test]
    fn translated_build_relocates_text_without_touching_source_banks() {
        let Some(jp) = jp_rom() else {
            eprintln!("skip: JP ROM absent");
            return;
        };
        let out = build_translated(
            &jp,
            &project_path("assets/fonts/dalmoori.ttf"),
            &project_path("assets/translations/complete"),
        )
        .expect("M2 translated build");
        assert_eq!(out.len(), TARGET_SIZE);
        assert_eq!(
            &out[0x0C000..0x10000],
            &jp[0x0C000..0x10000],
            "JP bank3 source text/system data must remain byte-identical"
        );
        assert_eq!(
            &out[0x14000..0x18000],
            &jp[0x14000..0x18000],
            "JP bank5 source text must remain byte-identical"
        );
        assert!(
            out[35 * BANK_SIZE..38 * BANK_SIZE]
                .iter()
                .any(|byte| (render::JP_KO_PREFIX_START..=render::JP_KO_PREFIX_END).contains(byte)),
            "relocated text banks must contain JP-direct KO prefixes"
        );

        let ttf = project_path("assets/fonts/dalmoori.ttf");
        use crate::patch::menu_tiles as tiles;
        let menu = tiles::generate_menu_buttons_jp(
            &ttf,
            &jp[tiles::MENU_BUTTON_BASE
                ..tiles::MENU_BUTTON_BASE + tiles::MENU_BUTTON_COUNT * tiles::MENU_BUTTON_BYTES],
        )
        .expect("JP menu buttons");
        let shop = tiles::generate_shop_block_jp(
            &ttf,
            &jp[tiles::SHOP_BUTTON_BASE..tiles::SHOP_BUTTON_BASE + tiles::SHOP_BUTTON_BYTES],
        )
        .expect("JP shop buttons");
        let title = tiles::generate_title_buttons_jp(
            &ttf,
            &jp[tiles::TITLE_BUTTON_BASE
                ..tiles::TITLE_BUTTON_BASE + tiles::TITLE_BUTTON_COUNT * tiles::MENU_BUTTON_BYTES],
        )
        .expect("JP title buttons");
        let file = tiles::generate_file_block_jp(
            &ttf,
            &jp[tiles::FILE_BUTTON_BASE..tiles::FILE_BUTTON_BASE + tiles::FILE_BUTTON_BYTES],
        )
        .expect("JP file buttons");
        let yesno = tiles::generate_yesno_buttons_jp(
            &ttf,
            &jp[tiles::YESNO_BUTTON_BASE..tiles::YESNO_BUTTON_BASE + 2 * tiles::MENU_BUTTON_BYTES],
        )
        .expect("JP yes/no buttons");
        let direction = crate::patch::menu_tiles::generate_direction_buttons_jp(
            &ttf,
            &jp[crate::patch::menu_tiles::DIRECTION_BUTTON_BASE
                ..crate::patch::menu_tiles::DIRECTION_BUTTON_BASE
                    + crate::patch::menu_tiles::DIRECTION_BUTTON_BYTES],
        )
        .expect("JP direction-stone buttons");
        assert_eq!(
            &out[crate::patch::menu_tiles::MENU_BUTTON_BASE
                ..crate::patch::menu_tiles::MENU_BUTTON_BASE + menu.len()],
            menu
        );
        assert_eq!(
            &out[crate::patch::menu_tiles::SHOP_BUTTON_BASE
                ..crate::patch::menu_tiles::SHOP_BUTTON_BASE + shop.len()],
            shop
        );
        assert_eq!(
            &out[crate::patch::menu_tiles::TITLE_BUTTON_BASE
                ..crate::patch::menu_tiles::TITLE_BUTTON_BASE + title.len()],
            title
        );
        assert_eq!(
            &out[crate::patch::title_logo::RELOC_TITLE_BUTTON_BASE
                ..crate::patch::title_logo::RELOC_TITLE_BUTTON_BASE + title.len()],
            title,
            "the title upload uses the relocated Korean START/CONTINUE tiles"
        );
        assert_eq!(
            &out[crate::patch::menu_tiles::FILE_BUTTON_BASE
                ..crate::patch::menu_tiles::FILE_BUTTON_BASE + file.len()],
            file
        );
        assert_eq!(
            &out[crate::patch::menu_tiles::YESNO_BUTTON_BASE
                ..crate::patch::menu_tiles::YESNO_BUTTON_BASE + yesno.len()],
            yesno
        );
        assert_eq!(
            &out[crate::patch::menu_tiles::DIRECTION_BUTTON_BASE
                ..crate::patch::menu_tiles::DIRECTION_BUTTON_BASE + direction.len()],
            direction
        );
        assert_eq!(
            &out[crate::patch::constants::MONEY_KANJI_GLYPH_ADDR
                ..crate::patch::constants::MONEY_KANJI_GLYPH_ADDR + 8],
            &[0x00, 0x7C, 0x04, 0xFE, 0x00, 0x7C, 0x44, 0x7C]
        );

        let third = crate::text::relocation::spare_config_for(37).expect("spare37 config");
        let table = &third.tables[0];
        let entry_start = |id: usize| {
            let ptr_off = third.physical_base + table.ptr_bank_offset as usize + id * 2;
            let ptr = u16::from_le_bytes([out[ptr_off], out[ptr_off + 1]]);
            third.physical_base + ptr as usize + (table.base_logical as usize - 0x8000)
        };
        let credits_start = entry_start(133);
        let ending_start = entry_start(160);
        let shared_credits_start = entry_start(237);
        assert!(
            credits_start > ending_start,
            "translated id133 must point inside the earlier id160 ending chain"
        );
        assert_eq!(
            credits_start, shared_credits_start,
            "translated id133 must share the exact id237 ending suffix"
        );
        assert!(
            (render::JP_KO_PREFIX_START..=render::JP_KO_PREFIX_END).contains(&out[credits_start]),
            "translated id133 must use JP-direct KO prefixes"
        );
        let credits_end =
            jp_direct_control_index(&out[credits_start..], crate::text::control::CTRL_CLOSE)
                .map(|offset| credits_start + offset)
                .expect("translated id133 END");
        assert_eq!(
            count_jp_direct_controls(
                &out[credits_start..credits_end],
                crate::text::control::CTRL_PAGE,
            ),
            18,
            "translated id133 must preserve all 18 source PAGE advances"
        );
        assert_eq!(
            out[credits_end - 1],
            crate::text::control::CTRL_PAGE,
            "translated id133 must scroll the final credit before END"
        );

        let active_ending_ids: Vec<usize> = (160..=254)
            .filter(|&id| {
                let ptr_off = third.physical_base + table.ptr_bank_offset as usize + id * 2;
                out[ptr_off] != 0 || out[ptr_off + 1] != 0
            })
            .collect();
        for ids in active_ending_ids.windows(2) {
            let current = entry_start(ids[0]);
            let next = entry_start(ids[1]);
            assert!(
                current < next,
                "ending suffix pointers must advance: {} -> {}",
                ids[0],
                ids[1]
            );
            assert_eq!(
                out[next - 1],
                crate::text::control::CTRL_PAGE,
                "ending suffix pointer {} must start after PAGE",
                ids[1]
            );
        }

        let ptr_off = third.physical_base + table.ptr_bank_offset as usize + 255 * 2;
        let ptr = u16::from_le_bytes([out[ptr_off], out[ptr_off + 1]]);
        let text_off = third.physical_base + ptr as usize + (table.base_logical as usize - 0x8000);
        assert_eq!(
            out[text_off],
            crate::text::control::CTRL_CLOSE,
            "non-text id255 must be isolated from relocated text"
        );
    }
}
