//! JP 원본 GG 타이틀 로고의 source-derived 한글화.
//!
//! 원본 ROM의 138타일·20×9 맵을 화면으로 해제한 뒤 Imagen에서 한 덩어리로 만든
//! `魔導傳記` 인덱스 층으로 주 로고 전체를 바꾼다. 네 원형 독음은 Imagen 빈 원 시안을
//! GG 16색/16px 기하로 정규화하고, 글자는 MD판과 같은 Galmuri7 7px 자형을 별도 8×8
//! 마스크로 합성한다. 독음은 한자 하단을 겹쳐 부제 공간을 확보하고, 원본
//! `3つの魔導球` 자리는 Galmuri7의 `3개의 마도구`로 교체한다. 배경·좌우 보석·금장과
//! 원본 `I`는 source surface에서 보존한다.
//!
//! 새 독음 때문에 원본의 좌우반전 공유 138타일 예산을 넘으므로, 빌드가 이미 확보한
//! 1MiB 확장 ROM의 빈 bank 38에 20×9 고유 타일, 원본 보조 타일 42개, 새 맵을 함께
//! 둔다. bank 0의 업로드 블록은 checked Z80 assembler로 다시 조립한다.

use anyhow::{Context, Result, bail};
use sha2::{Digest, Sha256};
use std::path::Path;
use z80::{Assembler, ByteOperand as Reg8, Instruction, Register16 as Reg16};

pub const SOURCE_TILE_BASE: usize = 0x2_DEEC;
pub const SOURCE_TILE_BYTES: usize = 0x1140;
pub const SOURCE_TILE_COUNT: usize = SOURCE_TILE_BYTES / 32;
pub const SOURCE_MAP_BASE: usize = 0x2_F034;
pub const SOURCE_MAP_BYTES: usize = 20 * 9 * 2;
pub const SOURCE_SECOND_TILE_BASE: usize = 0x2_F19C;
pub const SOURCE_SECOND_TILE_BYTES: usize = 0x0540;

pub const UPLOAD_PATCH_BASE: usize = 0x0_BB6;
pub const UPLOAD_PATCH_END: usize = 0x0_BDD;

pub const RELOC_BANK: u8 = 38;
pub const RELOC_BASE: usize = RELOC_BANK as usize * 0x4000;
pub const RELOC_TITLE_OFFSET: usize = 0x0000;
pub const RELOC_SECOND_OFFSET: usize = 0x2000;
pub const RELOC_MAP_OFFSET: usize = 0x2600;
pub const RELOC_END: usize = RELOC_MAP_OFFSET + SOURCE_MAP_BYTES;
pub const RELOC_TITLE_BUTTON_BASE: usize = RELOC_BASE
    + RELOC_SECOND_OFFSET
    + (super::menu_tiles::TITLE_BUTTON_BASE - SOURCE_SECOND_TILE_BASE);

const SURFACE_WIDTH: usize = 160;
const SURFACE_HEIGHT: usize = 72;
const OUTPUT_TILE_COUNT: usize = 20 * 9;
const OUTPUT_TILE_BYTES: usize = OUTPUT_TILE_COUNT * 32;

const TITLE_LAYER_X: usize = 22;
const TITLE_LAYER_Y: usize = 0;
const TITLE_LAYER_WIDTH: usize = 101;
const TITLE_LAYER_HEIGHT: usize = 44;
const TITLE_LAYER_HEX: &str = include_str!("../../../../assets/graphics/title_logo_wordmark.hex");
const TITLE_LAYER_SHA256: [u8; 32] = [
    0xE1, 0xD1, 0x26, 0x83, 0xDF, 0x6B, 0x64, 0xF6, 0xFC, 0xC8, 0x16, 0x67, 0x4B, 0xDA, 0x76, 0xD4,
    0x09, 0x97, 0x91, 0x56, 0x9F, 0xF5, 0x15, 0xD9, 0x9B, 0x2B, 0xCD, 0xC9, 0x46, 0x33, 0x35, 0x0F,
];

const SOURCE_TILES_SHA256: [u8; 32] = [
    0xA7, 0xBB, 0x7E, 0x1A, 0x35, 0x1D, 0x7B, 0x0D, 0x73, 0x87, 0x00, 0x71, 0x74, 0x01, 0x57, 0x2F,
    0xF9, 0x82, 0x59, 0x25, 0xB3, 0x0D, 0x57, 0x70, 0xBA, 0x2B, 0xA3, 0x8D, 0xBE, 0x49, 0x76, 0x61,
];
const SOURCE_MAP_SHA256: [u8; 32] = [
    0xF9, 0x79, 0xF1, 0xAE, 0x04, 0x8B, 0xE2, 0x16, 0xAC, 0xA7, 0x7B, 0x30, 0x98, 0x37, 0x5C, 0xCD,
    0xFB, 0x46, 0x39, 0x03, 0x15, 0x07, 0x52, 0x1B, 0x5F, 0x44, 0x35, 0xD0, 0x7B, 0x3B, 0x12, 0x7D,
];
const SOURCE_SECOND_TILES_SHA256: [u8; 32] = [
    0xD9, 0x01, 0x61, 0xF8, 0xD9, 0x37, 0x9C, 0x10, 0x54, 0xEE, 0x0E, 0x20, 0xE0, 0x55, 0x73, 0x40,
    0x8B, 0xEB, 0x50, 0x89, 0x8D, 0x0F, 0xD7, 0xA4, 0xCF, 0x8D, 0x74, 0xC4, 0x69, 0xA5, 0xA1, 0x7E,
];

/// Galmuri7 v2.403 BDF의 7px 자형을 MD 승인 폭 보정까지 적용한 8×8 행 마스크.
const PRONUNCIATION_GLYPHS: [[u8; 8]; 4] = [
    [0xE8, 0xA8, 0xA8, 0xAC, 0xA8, 0xE8, 0x08, 0x00], // 마
    [0x7C, 0x40, 0x40, 0x7C, 0x10, 0xFE, 0x00, 0x00], // 도
    [0x72, 0x2E, 0x52, 0x52, 0x02, 0x20, 0x3E, 0x00], // 전
    [0xE8, 0x28, 0x28, 0x48, 0x48, 0x88, 0x08, 0x00], // 기
];
const PRONUNCIATION_CENTERS: [usize; 4] = [33, 59, 86, 110];
const PRONUNCIATION_PAINT_OFFSETS_X: [isize; 4] = [1, 0, 0, 1];
const MEDALLION_TOP_Y: usize = 34;
const PRONUNCIATION_TOP_Y: usize = 38;

/// Galmuri7 v2.403 BDF 원형. `3개의 마도구` 6글자를 8px cell로 조판한다.
const SUBTITLE_GLYPHS: [[u8; 8]; 6] = [
    [0xE0, 0x10, 0x10, 0x60, 0x10, 0x10, 0xE0, 0x00], // 3
    [0xEA, 0x2A, 0x2A, 0x4E, 0x4A, 0x8A, 0x0A, 0x00], // 개
    [0x64, 0x94, 0x94, 0x64, 0x04, 0xFC, 0x04, 0x00], // 의
    [0xF4, 0x94, 0x94, 0x96, 0x94, 0xF4, 0x04, 0x00], // 마
    [0x7C, 0x40, 0x40, 0x7C, 0x10, 0xFE, 0x00, 0x00], // 도
    [0x7C, 0x04, 0x04, 0xFE, 0x10, 0x10, 0x10, 0x00], // 구
];
const SUBTITLE_X: usize = 54;
const SUBTITLE_Y: usize = 50;

#[derive(Debug)]
pub struct PreparedTitleLogo {
    upload_code: Vec<u8>,
    title_tiles: Vec<u8>,
    second_tiles: Vec<u8>,
    title_map: Vec<u8>,
    source_title_buttons: Vec<u8>,
    title_buttons: Vec<u8>,
    changed_pixels: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TitleLogoSummary {
    pub title_tiles: usize,
    pub title_button_bytes: usize,
    pub changed_pixels: usize,
    pub bank: u8,
}

fn expect_sha256(bytes: &[u8], expected: &[u8; 32], label: &str) -> Result<()> {
    let actual: [u8; 32] = Sha256::digest(bytes).into();
    if &actual != expected {
        bail!("{label} SHA-256 mismatch: expected {expected:02X?}, got {actual:02X?}");
    }
    Ok(())
}

fn source_range<'a>(rom: &'a [u8], offset: usize, len: usize, label: &str) -> Result<&'a [u8]> {
    rom.get(offset..offset + len).with_context(|| {
        format!(
            "{label} outside ROM at ${offset:06X}: need {len} bytes, ROM has {}",
            rom.len()
        )
    })
}

fn decode_tile(bytes: &[u8]) -> Result<[u8; 64]> {
    if bytes.len() != 32 {
        bail!("GG title tile must be 32 bytes, got {}", bytes.len());
    }
    let mut pixels = [0u8; 64];
    for y in 0..8 {
        for x in 0..8 {
            let bit = 0x80 >> x;
            for plane in 0..4 {
                if bytes[y * 4 + plane] & bit != 0 {
                    pixels[y * 8 + x] |= 1 << plane;
                }
            }
        }
    }
    Ok(pixels)
}

fn encode_tile(pixels: &[u8]) -> Result<[u8; 32]> {
    if pixels.len() != 64 || pixels.iter().any(|&pixel| pixel >= 16) {
        bail!("GG title tile must contain exactly 64 palette indices below 16");
    }
    let mut bytes = [0u8; 32];
    for y in 0..8 {
        for x in 0..8 {
            let bit = 0x80 >> x;
            let pixel = pixels[y * 8 + x];
            for plane in 0..4 {
                if pixel & (1 << plane) != 0 {
                    bytes[y * 4 + plane] |= bit;
                }
            }
        }
    }
    Ok(bytes)
}

fn render_surface(tiles: &[u8], map: &[u8]) -> Result<Vec<u8>> {
    if !tiles.len().is_multiple_of(32) || map.len() != SOURCE_MAP_BYTES {
        bail!(
            "invalid GG title tile/map sizes: {} / {}",
            tiles.len(),
            map.len()
        );
    }
    let decoded: Vec<_> = tiles
        .chunks_exact(32)
        .map(decode_tile)
        .collect::<Result<_>>()?;
    let mut surface = vec![0u8; SURFACE_WIDTH * SURFACE_HEIGHT];
    for position in 0..OUTPUT_TILE_COUNT {
        let word = u16::from_le_bytes([map[position * 2], map[position * 2 + 1]]);
        let tile_index = (word & 0x01FF) as usize;
        let tile = decoded.get(tile_index).with_context(|| {
            format!(
                "title map tile {tile_index} outside {} tiles",
                decoded.len()
            )
        })?;
        let hflip = word & 0x0200 != 0;
        let vflip = word & 0x0400 != 0;
        let tile_x = position % 20;
        let tile_y = position / 20;
        for y in 0..8 {
            for x in 0..8 {
                let source_x = if hflip { 7 - x } else { x };
                let source_y = if vflip { 7 - y } else { y };
                surface[(tile_y * 8 + y) * SURFACE_WIDTH + tile_x * 8 + x] =
                    tile[source_y * 8 + source_x];
            }
        }
    }
    Ok(surface)
}

fn title_layer() -> Result<Vec<u8>> {
    expect_sha256(
        TITLE_LAYER_HEX.as_bytes(),
        &TITLE_LAYER_SHA256,
        "GG title 傳記 layer",
    )?;
    let pixels: Vec<_> = TITLE_LAYER_HEX
        .bytes()
        .filter(|byte| !byte.is_ascii_whitespace())
        .map(|byte| match byte {
            b'0'..=b'9' => Ok(byte - b'0'),
            b'A'..=b'F' => Ok(byte - b'A' + 10),
            _ => bail!("invalid GG title layer nibble {byte:#04X}"),
        })
        .collect::<Result<_>>()?;
    if pixels.len() != TITLE_LAYER_WIDTH * TITLE_LAYER_HEIGHT {
        bail!(
            "GG title layer has {} pixels, expected {}",
            pixels.len(),
            TITLE_LAYER_WIDTH * TITLE_LAYER_HEIGHT
        );
    }
    Ok(pixels)
}

fn set_pixel(surface: &mut [u8], mutable: &mut [bool], x: usize, y: usize, value: u8) {
    let offset = y * SURFACE_WIDTH + x;
    surface[offset] = value;
    mutable[offset] = true;
}

fn compose_korean_surface(source: &[u8]) -> Result<(Vec<u8>, usize)> {
    if source.len() != SURFACE_WIDTH * SURFACE_HEIGHT {
        bail!("GG title surface size mismatch: {}", source.len());
    }
    let mut output = source.to_vec();
    let mut mutable = vec![false; source.len()];
    let layer = title_layer()?;

    // 네 한자의 획·팔레트·선명도가 갈라지지 않도록 Imagen 통합 魔導傳記 층으로 교체한다.
    for y in 0..TITLE_LAYER_HEIGHT {
        for x in 0..TITLE_LAYER_WIDTH {
            set_pixel(
                &mut output,
                &mut mutable,
                TITLE_LAYER_X + x,
                TITLE_LAYER_Y + y,
                layer[y * TITLE_LAYER_WIDTH + x],
            );
        }
    }

    // 원본 부제의 비금장 픽셀만 지운다. 하단 금장·보석 팔레트는 그대로 보존한다.
    for y in 46..59 {
        for x in 45..118 {
            let source_pixel = source[y * SURFACE_WIDTH + x];
            if !matches!(source_pixel, 0 | 3 | 4 | 5 | 6 | 10) {
                set_pixel(&mut output, &mut mutable, x, y, 0);
            }
        }
    }

    // Imagen 빈 원 시안을 GG 정수 기하로 정규화: 16px 외경, 금색 bevel, 적색 면.
    for center_x in PRONUNCIATION_CENTERS {
        for y in 0..16usize {
            for x in 0..16usize {
                let dx2 = x as i32 * 2 - 15;
                let dy2 = y as i32 * 2 - 15;
                let distance4 = dx2 * dx2 + dy2 * dy2;
                if distance4 > 256 {
                    continue;
                }
                let value = if distance4 >= 208 {
                    1
                } else if distance4 >= 128 {
                    if dy2 <= -4 || dx2 <= -8 {
                        10
                    } else if dy2 >= 6 || dx2 >= 10 {
                        3
                    } else {
                        6
                    }
                } else if distance4 >= 100 {
                    1
                } else {
                    9
                };
                set_pixel(
                    &mut output,
                    &mut mutable,
                    center_x - 8 + x,
                    MEDALLION_TOP_Y + y,
                    value,
                );
            }
        }
    }

    // 글자는 원과 독립된 8×8 의미층. 원 테두리의 가장 밝은 금색(index 10)으로 찍는다.
    for ((&center_x, &paint_offset_x), glyph) in PRONUNCIATION_CENTERS
        .iter()
        .zip(PRONUNCIATION_PAINT_OFFSETS_X.iter())
        .zip(PRONUNCIATION_GLYPHS.iter())
    {
        for (y, &row) in glyph.iter().enumerate() {
            for x in 0..8 {
                if row & (1 << (7 - x)) != 0 {
                    let paint_x = (center_x as isize - 4 + paint_offset_x + x as isize) as usize;
                    set_pixel(
                        &mut output,
                        &mut mutable,
                        paint_x,
                        PRONUNCIATION_TOP_Y + y,
                        10,
                    );
                }
            }
        }
    }

    // 원본 정식 부제를 의미 그대로 보존한다. 먼저 1px 우하단 shadow를 찍고 흰 자형을 올린다.
    let mut subtitle_x = SUBTITLE_X;
    for (index, glyph) in SUBTITLE_GLYPHS.iter().enumerate() {
        if index == 3 {
            subtitle_x += 4;
        }
        for (y, &row) in glyph.iter().enumerate() {
            for x in 0..8 {
                if row & (1 << (7 - x)) != 0 {
                    set_pixel(
                        &mut output,
                        &mut mutable,
                        subtitle_x + x + 1,
                        SUBTITLE_Y + y + 1,
                        9,
                    );
                }
            }
        }
        subtitle_x += 8;
    }
    subtitle_x = SUBTITLE_X;
    for (index, glyph) in SUBTITLE_GLYPHS.iter().enumerate() {
        if index == 3 {
            subtitle_x += 4;
        }
        for (y, &row) in glyph.iter().enumerate() {
            for x in 0..8 {
                if row & (1 << (7 - x)) != 0 {
                    set_pixel(
                        &mut output,
                        &mut mutable,
                        subtitle_x + x,
                        SUBTITLE_Y + y,
                        15,
                    );
                }
            }
        }
        subtitle_x += 8;
    }

    for (offset, (&before, &after)) in source.iter().zip(output.iter()).enumerate() {
        if !mutable[offset] && before != after {
            bail!("GG title protected pixel changed at surface offset {offset}");
        }
        if after >= 16 {
            bail!("GG title emitted invalid palette index {after} at surface offset {offset}");
        }
    }
    let changed_pixels = source
        .iter()
        .zip(output.iter())
        .filter(|(before, after)| before != after)
        .count();
    Ok((output, changed_pixels))
}

fn encode_surface(surface: &[u8], source_map: &[u8]) -> Result<(Vec<u8>, Vec<u8>)> {
    if surface.len() != SURFACE_WIDTH * SURFACE_HEIGHT || source_map.len() != SOURCE_MAP_BYTES {
        bail!("GG title encode input size mismatch");
    }
    let mut tiles = Vec::with_capacity(OUTPUT_TILE_BYTES);
    let mut map = Vec::with_capacity(SOURCE_MAP_BYTES);
    for position in 0..OUTPUT_TILE_COUNT {
        let tile_x = position % 20;
        let tile_y = position / 20;
        let mut pixels = [0u8; 64];
        for y in 0..8 {
            pixels[y * 8..y * 8 + 8].copy_from_slice(
                &surface[(tile_y * 8 + y) * SURFACE_WIDTH + tile_x * 8
                    ..(tile_y * 8 + y) * SURFACE_WIDTH + tile_x * 8 + 8],
            );
        }
        tiles.extend_from_slice(&encode_tile(&pixels)?);

        let source_word =
            u16::from_le_bytes([source_map[position * 2], source_map[position * 2 + 1]]);
        let output_word = (source_word & 0xF800) | position as u16;
        map.extend_from_slice(&output_word.to_le_bytes());
    }
    Ok((tiles, map))
}

fn assemble_upload_block(
    bank: u8,
    title_source: u16,
    title_bytes: u16,
    second_source: u16,
    map_source: u16,
) -> Result<Vec<u8>> {
    let mut asm = Assembler::new();
    asm.emit(Instruction::LdRImm(Reg8::A, bank));
    asm.emit(Instruction::LdAddrA(0xFFFF));
    asm.emit(Instruction::LdRRImm(Reg16::Hl, title_source));
    asm.emit(Instruction::LdRRImm(Reg16::De, 0x0000));
    asm.emit(Instruction::LdRRImm(Reg16::Bc, title_bytes));
    asm.emit(Instruction::Call(0x2813));
    asm.emit(Instruction::LdRRImm(Reg16::Hl, second_source));
    asm.emit(Instruction::LdRRImm(Reg16::De, 0x2000));
    asm.emit(Instruction::LdRRImm(
        Reg16::Bc,
        SOURCE_SECOND_TILE_BYTES as u16,
    ));
    asm.emit(Instruction::Call(0x2813));
    asm.emit(Instruction::Ei);
    asm.emit(Instruction::LdRRImm(Reg16::Hl, map_source));
    asm.emit(Instruction::LdRRImm(Reg16::De, 0xC928));
    asm.emit(Instruction::LdRRImm(Reg16::Bc, SOURCE_MAP_BYTES as u16));
    let bytes = asm.assemble(UPLOAD_PATCH_BASE as u16)?.into_bytes();
    if bytes.len() != UPLOAD_PATCH_END - UPLOAD_PATCH_BASE {
        bail!(
            "GG title upload block assembled to {} bytes, expected {}",
            bytes.len(),
            UPLOAD_PATCH_END - UPLOAD_PATCH_BASE
        );
    }
    Ok(bytes)
}

/// 타이틀 화면 전체를 하나의 결과물로 준비한다. 로고 업로드가 보조 타일 블록을 bank 38로
/// 옮기므로, START/CONTINUE 번역도 여기서 합성해 원주소와 재배치 사본을 함께 소유한다.
pub fn prepare_jp(rom: &[u8], ttf_path: &Path) -> Result<PreparedTitleLogo> {
    let source_tiles = source_range(rom, SOURCE_TILE_BASE, SOURCE_TILE_BYTES, "JP title tiles")?;
    let source_map = source_range(rom, SOURCE_MAP_BASE, SOURCE_MAP_BYTES, "JP title map")?;
    let second_tiles = source_range(
        rom,
        SOURCE_SECOND_TILE_BASE,
        SOURCE_SECOND_TILE_BYTES,
        "JP title secondary tiles",
    )?;
    expect_sha256(source_tiles, &SOURCE_TILES_SHA256, "JP title tiles")?;
    expect_sha256(source_map, &SOURCE_MAP_SHA256, "JP title map")?;
    expect_sha256(
        second_tiles,
        &SOURCE_SECOND_TILES_SHA256,
        "JP title secondary tiles",
    )?;
    let source_title_buttons = source_range(
        rom,
        super::menu_tiles::TITLE_BUTTON_BASE,
        super::menu_tiles::TITLE_BUTTON_COUNT * super::menu_tiles::MENU_BUTTON_BYTES,
        "JP title buttons",
    )?
    .to_vec();
    expect_sha256(
        &source_title_buttons,
        &super::menu_tiles::TITLE_BUTTONS_JP_SHA256,
        "JP title buttons",
    )?;
    let title_buttons =
        super::menu_tiles::generate_title_buttons_jp(ttf_path, &source_title_buttons)
            .context("generate Korean START/CONTINUE tiles for relocated title screen")?;
    let title_button_offset = super::menu_tiles::TITLE_BUTTON_BASE
        .checked_sub(SOURCE_SECOND_TILE_BASE)
        .context("title buttons precede the relocated secondary tile block")?;
    if title_button_offset + title_buttons.len() > second_tiles.len() {
        bail!("title buttons exceed the relocated secondary tile block");
    }

    let original_code = assemble_upload_block(11, 0x9EEC, 0x1140, 0xB19C, 0xB034)
        .context("assemble original JP title upload block")?;
    let actual_code = source_range(
        rom,
        UPLOAD_PATCH_BASE,
        original_code.len(),
        "JP title upload block",
    )?;
    if actual_code != original_code {
        bail!("JP title upload block signature mismatch at ${UPLOAD_PATCH_BASE:04X}");
    }
    let free = source_range(rom, RELOC_BASE, RELOC_END, "JP title relocation bank")?;
    if free.iter().any(|&byte| byte != 0xFF) {
        bail!("JP title relocation bank 38 is not free at ${RELOC_BASE:06X}");
    }

    let source_surface = render_surface(source_tiles, source_map)?;
    let (korean_surface, changed_pixels) = compose_korean_surface(&source_surface)?;
    let (title_tiles, title_map) = encode_surface(&korean_surface, source_map)?;
    let roundtrip = render_surface(&title_tiles, &title_map)?;
    if roundtrip != korean_surface {
        bail!("GG title relocated tile/map round-trip differs from composed surface");
    }
    let upload_code =
        assemble_upload_block(RELOC_BANK, 0x8000, OUTPUT_TILE_BYTES as u16, 0xA000, 0xA600)
            .context("assemble relocated GG title upload block")?;

    let mut relocated_second_tiles = second_tiles.to_vec();
    relocated_second_tiles[title_button_offset..title_button_offset + title_buttons.len()]
        .copy_from_slice(&title_buttons);

    Ok(PreparedTitleLogo {
        upload_code,
        title_tiles,
        second_tiles: relocated_second_tiles,
        title_map,
        source_title_buttons,
        title_buttons,
        changed_pixels,
    })
}

pub fn apply_prepared(rom: &mut [u8], prepared: PreparedTitleLogo) -> Result<TitleLogoSummary> {
    let original_code = assemble_upload_block(11, 0x9EEC, 0x1140, 0xB19C, 0xB034)?;
    if source_range(
        rom,
        UPLOAD_PATCH_BASE,
        original_code.len(),
        "JP title upload block",
    )? != original_code
    {
        bail!("JP title upload block changed after preparation");
    }
    if source_range(rom, RELOC_BASE, RELOC_END, "JP title relocation bank")?
        .iter()
        .any(|&byte| byte != 0xFF)
    {
        bail!("JP title relocation bank changed after preparation");
    }
    if source_range(
        rom,
        super::menu_tiles::TITLE_BUTTON_BASE,
        prepared.source_title_buttons.len(),
        "JP title buttons",
    )? != prepared.source_title_buttons
    {
        bail!("JP title buttons changed after preparation");
    }

    // 모든 검증을 마친 뒤 타이틀 화면 소유 영역을 한 번에 쓴다.
    rom[UPLOAD_PATCH_BASE..UPLOAD_PATCH_END].copy_from_slice(&prepared.upload_code);
    rom[RELOC_BASE + RELOC_TITLE_OFFSET
        ..RELOC_BASE + RELOC_TITLE_OFFSET + prepared.title_tiles.len()]
        .copy_from_slice(&prepared.title_tiles);
    rom[RELOC_BASE + RELOC_SECOND_OFFSET
        ..RELOC_BASE + RELOC_SECOND_OFFSET + prepared.second_tiles.len()]
        .copy_from_slice(&prepared.second_tiles);
    rom[RELOC_BASE + RELOC_MAP_OFFSET..RELOC_BASE + RELOC_MAP_OFFSET + prepared.title_map.len()]
        .copy_from_slice(&prepared.title_map);
    rom[super::menu_tiles::TITLE_BUTTON_BASE
        ..super::menu_tiles::TITLE_BUTTON_BASE + prepared.title_buttons.len()]
        .copy_from_slice(&prepared.title_buttons);

    Ok(TitleLogoSummary {
        title_tiles: OUTPUT_TILE_COUNT,
        title_button_bytes: prepared.title_buttons.len(),
        changed_pixels: prepared.changed_pixels,
        bank: RELOC_BANK,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::patch::menu_tiles;

    fn jp_rom() -> Option<Vec<u8>> {
        std::fs::read("../../roms/Madou Monogatari I - 3-Tsu no Madoukyuu (Japan).gg").ok()
    }

    fn dalmoori_font() -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/fonts/dalmoori.ttf")
    }

    #[test]
    fn title_layer_contract_is_exact() {
        let layer = title_layer().expect("title layer");
        assert_eq!(layer.len(), 101 * 44);
        assert!(layer.iter().all(|&pixel| pixel < 16));
    }

    #[test]
    fn source_tiles_round_trip() {
        let Some(jp) = jp_rom() else {
            eprintln!("skip: JP ROM absent");
            return;
        };
        for tile in jp[SOURCE_TILE_BASE..SOURCE_TILE_BASE + SOURCE_TILE_BYTES].chunks_exact(32) {
            let pixels = decode_tile(tile).expect("decode");
            assert_eq!(encode_tile(&pixels).expect("encode").as_slice(), tile);
        }
    }

    #[test]
    fn prepared_title_is_source_protected_and_reencodable() {
        let Some(jp) = jp_rom() else {
            eprintln!("skip: JP ROM absent");
            return;
        };
        let source_tiles = &jp[SOURCE_TILE_BASE..SOURCE_TILE_BASE + SOURCE_TILE_BYTES];
        let source_map = &jp[SOURCE_MAP_BASE..SOURCE_MAP_BASE + SOURCE_MAP_BYTES];
        let source_surface = render_surface(source_tiles, source_map).expect("source surface");
        let (target, changed) = compose_korean_surface(&source_surface).expect("compose");
        assert!(changed > 2_000, "title edit unexpectedly small: {changed}");

        // 주 로고 바깥 장식과 원본 I는 보호한다.
        for y in 0..43 {
            assert_eq!(
                &target[y * 160..y * 160 + 22],
                &source_surface[y * 160..y * 160 + 22]
            );
            assert_eq!(
                &target[y * 160 + 123..y * 160 + 140],
                &source_surface[y * 160 + 123..y * 160 + 140]
            );
        }
        for ((&center, &paint_offset_x), glyph) in PRONUNCIATION_CENTERS
            .iter()
            .zip(PRONUNCIATION_PAINT_OFFSETS_X.iter())
            .zip(PRONUNCIATION_GLYPHS.iter())
        {
            let painted = glyph
                .iter()
                .map(|row| row.count_ones() as usize)
                .sum::<usize>();
            let actual = (0..8)
                .flat_map(|y| (0..8).map(move |x| (x, y)))
                .filter(|&(x, y)| {
                    let paint_x = (center as isize - 4 + paint_offset_x + x as isize) as usize;
                    target[(PRONUNCIATION_TOP_Y + y) * 160 + paint_x] == 10
                })
                .count();
            assert!(
                actual >= painted,
                "pronunciation glyph at x={center} lost pixels"
            );
        }
        let mut subtitle_white = 0;
        for y in SUBTITLE_Y..SUBTITLE_Y + 8 {
            for x in SUBTITLE_X..=106 {
                subtitle_white += usize::from(target[y * 160 + x] == 15);
            }
        }
        assert!(subtitle_white >= 100, "Korean subtitle lost pixels");

        let (tiles, map) = encode_surface(&target, source_map).expect("encode target");
        assert_eq!(tiles.len(), 180 * 32);
        assert_eq!(render_surface(&tiles, &map).expect("render target"), target);
    }

    #[test]
    fn title_prepare_and_apply_are_fail_closed() {
        let Some(jp) = jp_rom() else {
            eprintln!("skip: JP ROM absent");
            return;
        };
        let mut expanded = jp.clone();
        expanded.resize(0x10_0000, 0xFF);
        let prepared = prepare_jp(&expanded, &dalmoori_font()).expect("prepare title");
        let before = expanded.clone();
        let summary = apply_prepared(&mut expanded, prepared).expect("apply title");
        assert_eq!(summary.title_tiles, 180);
        assert_eq!(summary.title_button_bytes, 384);
        assert_eq!(summary.bank, 38);
        assert_ne!(
            &expanded[UPLOAD_PATCH_BASE..UPLOAD_PATCH_END],
            &before[UPLOAD_PATCH_BASE..UPLOAD_PATCH_END]
        );
        let mut expected_second = jp
            [SOURCE_SECOND_TILE_BASE..SOURCE_SECOND_TILE_BASE + SOURCE_SECOND_TILE_BYTES]
            .to_vec();
        let button_offset = menu_tiles::TITLE_BUTTON_BASE - SOURCE_SECOND_TILE_BASE;
        let title_source = &jp[menu_tiles::TITLE_BUTTON_BASE
            ..menu_tiles::TITLE_BUTTON_BASE
                + menu_tiles::TITLE_BUTTON_COUNT * menu_tiles::MENU_BUTTON_BYTES];
        let title_buttons = menu_tiles::generate_title_buttons_jp(&dalmoori_font(), title_source)
            .expect("Korean title buttons");
        expected_second[button_offset..button_offset + title_buttons.len()]
            .copy_from_slice(&title_buttons);
        assert_eq!(
            &expanded[RELOC_BASE + RELOC_SECOND_OFFSET
                ..RELOC_BASE + RELOC_SECOND_OFFSET + SOURCE_SECOND_TILE_BYTES],
            expected_second
        );
        assert_eq!(
            &expanded[menu_tiles::TITLE_BUTTON_BASE
                ..menu_tiles::TITLE_BUTTON_BASE + title_buttons.len()],
            title_buttons
        );

        for offset in 0..expanded.len() {
            if expanded[offset] == before[offset] {
                continue;
            }
            assert!(
                (UPLOAD_PATCH_BASE..UPLOAD_PATCH_END).contains(&offset)
                    || (RELOC_BASE..RELOC_BASE + OUTPUT_TILE_BYTES).contains(&offset)
                    || (RELOC_BASE + RELOC_SECOND_OFFSET
                        ..RELOC_BASE + RELOC_SECOND_OFFSET + SOURCE_SECOND_TILE_BYTES)
                        .contains(&offset)
                    || (RELOC_BASE + RELOC_MAP_OFFSET
                        ..RELOC_BASE + RELOC_MAP_OFFSET + SOURCE_MAP_BYTES)
                        .contains(&offset)
                    || (menu_tiles::TITLE_BUTTON_BASE
                        ..menu_tiles::TITLE_BUTTON_BASE + title_buttons.len())
                        .contains(&offset),
                "unexpected title write at {offset:#08X}"
            );
        }

        let mut drift = before;
        drift[SOURCE_TILE_BASE] ^= 1;
        assert!(prepare_jp(&drift, &dalmoori_font()).is_err());
        drift[SOURCE_TILE_BASE] ^= 1;
        drift[RELOC_BASE] = 0;
        assert!(prepare_jp(&drift, &dalmoori_font()).is_err());
    }
}
