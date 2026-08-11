use anyhow::Result;
use std::collections::BTreeMap;
use std::path::Path;

pub struct FontOutput {
    pub font_1bpp: Vec<u8>,            // syllable_count × 8 bytes
    pub font_4bpp: Vec<u8>,            // syllable_count × 32 bytes
    pub syllable_map: BTreeMap<char, u16>, // char → index
}

/// Generate Korean font glyphs from a TTF file.
///
/// Each syllable is rendered at 8×8 pixels and converted to 1bpp and 4bpp formats.
/// The threshold parameter controls the binary cutoff for 1bpp (0-255, default 128).
pub fn generate_korean_font(
    ttf_path: &Path,
    font_size: f32,
    syllables: &[char],
    threshold: u8,
) -> Result<FontOutput> {
    let font_data = std::fs::read(ttf_path)?;
    let font = fontdue::Font::from_bytes(font_data, fontdue::FontSettings::default())
        .map_err(|e| anyhow::anyhow!("failed to load font: {}", e))?;

    let mut font_1bpp = Vec::new();
    let mut font_4bpp = Vec::new();
    let mut syllable_map = BTreeMap::new();

    // 문자부호는 1px 작게 렌더(8x8 셀에서 가장자리에 안 붙게). 중앙정렬은 rasterize_to_8x8이 처리.
    // '~'는 물결형(synth_tilde), ','는 서양식 쉼표(synth_comma)로 합성 — dalmoori 8px가
    // 각각 대시·가타카나 콤마(、)처럼 보이는 문제 회피.
    const SMALL_PUNCT: &[char] = &['!', '?'];
    // 마침표는 중앙이 아니라 베이스라인(하단) 정렬 — 중앙에 뜨면 나카구로처럼 어색.
    const BASELINE_PUNCT: &[char] = &['.'];

    for (idx, &ch) in syllables.iter().enumerate() {
        // '…'(U+2026)은 dalmoori에 글리프가 없다(폰트 cmap 미포함) → 프로시저럴 합성.
        // 베이스라인에 2×2 점 3개를 균등 배치(마침표와 동일 무게, 셀 가장자리 비접촉).
        let bpp1 = if ch == '…' {
            synth_ellipsis()
        } else if ch == '~' {
            synth_tilde()
        } else if ch == ',' {
            synth_comma()
        } else {
            // Rasterize at requested size (문자부호는 1px 작게)
            let size = if SMALL_PUNCT.contains(&ch) {
                (font_size - 1.0).max(1.0)
            } else {
                font_size
            };
            let (metrics, bitmap) = font.rasterize(ch, size);

            // Convert to 8×8 1bpp bitmap (마침표/쉼표는 하단 정렬)
            let align_bottom = BASELINE_PUNCT.contains(&ch);
            rasterize_to_8x8(&metrics, &bitmap, threshold, align_bottom)
        };

        // Convert to formats
        let bpp4 = super::tile::bpp1_to_bpp4(&bpp1);

        font_1bpp.extend_from_slice(&super::tile::bpp1_to_en_format(&bpp1, 8));
        font_4bpp.extend_from_slice(&bpp4);
        syllable_map.insert(ch, idx as u16);
    }

    Ok(FontOutput {
        font_1bpp,
        font_4bpp,
        syllable_map,
    })
}

/// 프로시저럴 '…'(생략부호) 8×8 1bpp. dalmoori에 U+2026 글리프가 없어 합성.
/// 베이스라인(행 7)에 1px 점 3개(cols 1,3,5) — 마침표('.'=1px)와 동일 무게,
/// 가운데 점이 '.'의 col3과 정렬, 양쪽 여백 확보. 비트: `0x80 >> col`.
fn synth_ellipsis() -> [u8; 8] {
    // 0x54 = 0101_0100 → cols {1} {3} {5}
    const ROW: u8 = 0b0101_0100;
    [0, 0, 0, 0, 0, 0, 0, ROW]
}

/// 프로시저럴 '~'(물결표) 8×8 1bpp. dalmoori 8px는 작은 대각선이라 대시처럼 보여
/// 물결형(crest-trough-crest)으로 합성. 셀 중앙(행 3-5)에 위-아래-위 굴곡. 비트: `0x80 >> col`.
fn synth_tilde() -> [u8; 8] {
    // row3: cols 1,2,7  = 0110_0001
    // row4: cols 0,3,6  = 1001_0010
    // row5: cols 4,5    = 0000_1100
    [0, 0, 0, 0b0110_0001, 0b1001_0010, 0b0000_1100, 0, 0]
}

/// 프로시저럴 ','(쉼표) 8×8 1bpp. dalmoori 8px는 중앙 2px 대각선이라 가타카나 콤마(、)처럼
/// 보여, 서양식 쉼표(2×2 머리 + 아래로 굽는 꼬리)로 합성. 좌하단(행 4-7)에 앉음. 비트: `0x80 >> col`.
fn synth_comma() -> [u8; 8] {
    // row4: cols 2,3  = 0011_0000
    // row5: cols 2,3  = 0011_0000
    // row6: col 2     = 0010_0000
    // row7: col 1     = 0100_0000
    [0, 0, 0, 0, 0b0011_0000, 0b0011_0000, 0b0010_0000, 0b0100_0000]
}

/// Rasterize a fontdue glyph into an 8×8 1bpp bitmap.
/// Centers the glyph within the 8×8 cell (align_bottom=true면 세로 하단 정렬 — 마침표/쉼표).
fn rasterize_to_8x8(
    metrics: &fontdue::Metrics,
    bitmap: &[u8],
    threshold: u8,
    align_bottom: bool,
) -> [u8; 8] {
    let mut result = [0u8; 8];

    let glyph_w = metrics.width;
    let glyph_h = metrics.height;

    if glyph_w == 0 || glyph_h == 0 {
        return result; // empty glyph (e.g., space)
    }

    // Center horizontally; vertically center or bottom-align(baseline)
    let offset_x = (8usize.saturating_sub(glyph_w)) / 2;
    let offset_y = if align_bottom {
        8usize.saturating_sub(glyph_h) // 하단(베이스라인) 정렬
    } else {
        (8usize.saturating_sub(glyph_h)) / 2
    };

    for gy in 0..glyph_h.min(8) {
        let dst_y = offset_y + gy;
        if dst_y >= 8 {
            break;
        }

        for gx in 0..glyph_w.min(8) {
            let dst_x = offset_x + gx;
            if dst_x >= 8 {
                continue;
            }

            let coverage = bitmap[gy * glyph_w + gx];
            if coverage >= threshold {
                result[dst_y] |= 0x80 >> dst_x;
            }
        }
    }

    result
}

#[cfg(test)]
#[path = "korean_tests.rs"]
mod tests;
