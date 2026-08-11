use super::*;

#[test]
fn test_bpp1_to_bpp4_empty() {
    let bpp1 = [0u8; 8];
    let bpp4 = bpp1_to_bpp4(&bpp1);
    assert_eq!(bpp4, [0u8; 32]);
}

#[test]
fn test_bpp1_to_bpp4_full() {
    let bpp1 = [0xFF; 8];
    let bpp4 = bpp1_to_bpp4(&bpp1);
    assert_eq!(bpp4, [0xFF; 32]);
}

#[test]
fn test_bpp1_to_bpp4_pattern() {
    let mut bpp1 = [0u8; 8];
    bpp1[0] = 0x80; // top-left pixel
    let bpp4 = bpp1_to_bpp4(&bpp1);
    assert_eq!(bpp4[0], 0x80);
    assert_eq!(bpp4[1], 0x80);
    assert_eq!(bpp4[2], 0x80);
    assert_eq!(bpp4[3], 0x80);
    assert_eq!(bpp4[4], 0x00); // second row all zero
}

#[test]
fn test_en_format() {
    let bpp1 = [0xFF; 8];
    let en = bpp1_to_en_format(&bpp1, 8);
    assert_eq!(en[0], 8); // width
    assert_eq!(en[1], 0xFF); // row 0
    assert_eq!(en[7], 0xFF); // row 6
}
