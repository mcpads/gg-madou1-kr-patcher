/// Convert 1bpp bitmap (8 bytes, 1 byte per row) to 4bpp tile (32 bytes).
/// In 4bpp: each row = 4 bytes (bitplanes 0-3).
/// For a 1bpp source, set all 4 bitplanes to the same value → color 0 or 15.
pub fn bpp1_to_bpp4(bpp1: &[u8; 8]) -> [u8; 32] {
    let mut bpp4 = [0u8; 32];
    for row in 0..8 {
        let bits = bpp1[row];
        // Color 15 = all bitplanes set
        bpp4[row * 4] = bits;     // bitplane 0
        bpp4[row * 4 + 1] = bits; // bitplane 1
        bpp4[row * 4 + 2] = bits; // bitplane 2
        bpp4[row * 4 + 3] = bits; // bitplane 3
    }
    bpp4
}

/// Convert 1bpp bitmap to the EN font format:
/// [width_byte] [row0] [row1] ... [row6] = 8 bytes total
/// (only 7 rows because first byte is width)
pub fn bpp1_to_en_format(bpp1: &[u8; 8], width: u8) -> [u8; 8] {
    let mut result = [0u8; 8];
    result[0] = width;
    result[1..8].copy_from_slice(&bpp1[0..7]);
    result
}

#[cfg(test)]
#[path = "tile_tests.rs"]
mod tests;
