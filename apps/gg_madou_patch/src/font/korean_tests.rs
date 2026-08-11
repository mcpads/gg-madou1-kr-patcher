use super::*;

#[test]
fn test_rasterize_empty() {
    let metrics = fontdue::Metrics {
        width: 0,
        height: 0,
        xmin: 0,
        ymin: 0,
        advance_width: 0.0,
        advance_height: 0.0,
        bounds: fontdue::OutlineBounds {
            xmin: 0.0,
            ymin: 0.0,
            width: 0.0,
            height: 0.0,
        },
    };
    let bitmap = vec![];
    let result = rasterize_to_8x8(&metrics, &bitmap, 128, false);
    assert_eq!(result, [0u8; 8]);
}

#[test]
fn test_rasterize_single_pixel() {
    let metrics = fontdue::Metrics {
        width: 1,
        height: 1,
        xmin: 0,
        ymin: 0,
        advance_width: 1.0,
        advance_height: 1.0,
        bounds: fontdue::OutlineBounds {
            xmin: 0.0,
            ymin: 0.0,
            width: 1.0,
            height: 1.0,
        },
    };
    let bitmap = vec![255]; // full coverage
    let result = rasterize_to_8x8(&metrics, &bitmap, 128, false);
    // Centered at (3, 3) → bit 4 of row 3
    // offset_x = (8-1)/2 = 3, offset_y = (8-1)/2 = 3
    // 0x80 >> 3 = 0x10
    assert_eq!(result[3], 0x10);
}

#[test]
fn test_font_output_sizes() {
    // If we can't load a TTF, at least test the structures
    let enc = super::super::tile::bpp1_to_bpp4(&[0u8; 8]);
    assert_eq!(enc.len(), 32);
}

#[test]
fn test_synth_ellipsis_three_dots() {
    // '…'는 폰트에 없어 합성 — 비어있지 않고 정확히 3개 점 컬럼(1,3,5)이어야 함
    let g = synth_ellipsis();
    assert_eq!(g, [0, 0, 0, 0, 0, 0, 0, 0b0101_0100]);
    let row7 = g[7];
    let cols: Vec<usize> = (0..8).filter(|c| row7 & (0x80 >> c) != 0).collect();
    assert_eq!(cols, vec![1, 3, 5], "생략부호는 cols 1,3,5에 점 3개");
}
