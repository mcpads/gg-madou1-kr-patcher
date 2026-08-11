use super::encoding::{KoEncoding, collapse_ellipsis, is_ko_glyph_char, normalize_glyph_char};

/// Control codes in the text stream
pub const CTRL_END: u8 = 0x00; // end of text (actual zero byte)
pub const CTRL_PAGE: u8 = 0xFF; // [PAGE]
pub const CTRL_CLOSE: u8 = 0xFE; // [END]
pub const CTRL_NEWLINE: u8 = 0xFD; // newline
pub const CTRL_WAIT: u8 = 0xFC; // [WAIT]
pub const CTRL_VAR: u8 = 0xF8; // [VAR:XX] (2 bytes)

/// EN encoding: basic characters 0x0B-0x4A + symbols
/// Maps ASCII, digits, and special symbols to their text engine byte values.
pub fn en_char_to_byte(ch: char) -> Option<u8> {
    match ch {
        ' ' => Some(0x0B),
        'A'..='Z' => Some(0x0C + (ch as u8 - b'A')),
        'a'..='z' => Some(0x26 + (ch as u8 - b'a')),
        ',' => Some(0x40),
        '\'' => Some(0x41),
        '?' => Some(0x42),
        '!' => Some(0x43),
        '.' => Some(0x44),
        '☆' => Some(0x44),
        '・' => Some(0x44),
        '©' => Some(0x45),
        '®' => Some(0x46),
        '~' => Some(0x47),
        '-' => Some(0x49),
        '？' => Some(0x42),
        // Digits: EN VWF engine renders these via tile = byte + 0x54
        '0' => Some(0x02),
        '1' => Some(0x03),
        '2' => Some(0x04),
        '3' => Some(0x05),
        '4' => Some(0x06),
        '5' => Some(0x07),
        '6' => Some(0x08),
        '7' => Some(0x09),
        '8' => Some(0x0A),
        // Special symbols in the safe range (< 0x80, won't trigger KO dispatch)
        '□' => Some(0x00),
        '◎' => Some(0x01),
        _ => None,
    }
}

/// Encode a Korean text string to bytes.
/// Supports: Korean syllables (2-byte), EN chars (1-byte), control tags
/// Control tags: {FF} {WAIT} {END} {PAGE} {VAR:XX} \n
pub fn encode_ko_text(text: &str, ko_enc: &KoEncoding) -> anyhow::Result<Vec<u8>> {
    // 생략부호 정규화: '.'/'・' 3+ 연속 → 단일 '…' 글리프(1타일). '…'는 이제
    // KO 글리프이므로 안전 렌더. collect_korean_syllables와 동일 정규화.
    let text = collapse_ellipsis(text);
    let mut bytes = Vec::new();
    let mut chars = text.chars().peekable();

    while let Some(ch) = chars.next() {
        match ch {
            '\n' => bytes.push(CTRL_NEWLINE),
            '{' => {
                // Parse control tag
                let tag: String = chars.by_ref().take_while(|&c| c != '}').collect();
                match tag.as_str() {
                    "FF" | "PAGE" => bytes.push(CTRL_PAGE),
                    "WAIT" => bytes.push(CTRL_WAIT),
                    "END" => bytes.push(CTRL_CLOSE),
                    _ if tag.starts_with("VAR:") => {
                        bytes.push(CTRL_VAR);
                        let hex = &tag[4..];
                        let val = u8::from_str_radix(hex, 16)
                            .map_err(|_| anyhow::anyhow!("invalid VAR hex: {}", hex))?;
                        bytes.push(val);
                    }
                    _ => anyhow::bail!("unknown control tag: {{{}}}", tag),
                }
            }
            _ => {
                // ☆ 제거 / ・ → '.' 정규화 후 KO 글리프/EN 바이트로
                let Some(ch) = normalize_glyph_char(ch) else {
                    continue; // ☆(JP 온점 오독) 제거
                };
                // Prefer an explicit KO mapping, then a ROM-specific native
                // literal override. The JP-direct charmap uses different
                // one-byte values for space/hyphen than the EN-base path.
                if let Some(enc) = ko_enc.encode_char(ch) {
                    bytes.push(enc[0]);
                    bytes.push(enc[1]);
                } else if is_ko_glyph_char(ch) {
                    anyhow::bail!("glyph character not in encoding: {}", ch);
                } else if let Some(b) = ko_enc.literal_byte(ch).or_else(|| en_char_to_byte(ch)) {
                    bytes.push(b);
                } else {
                    anyhow::bail!("unsupported character '{}' (U+{:04X})", ch, ch as u32);
                }
            }
        }
    }

    // Terminate with 0xFE [END] — the game's text engine end marker
    // Avoid double-terminator if the translation already ends with {END}
    if bytes.last() != Some(&CTRL_CLOSE) {
        bytes.push(CTRL_CLOSE);
    }

    Ok(bytes)
}

#[cfg(test)]
#[path = "control_tests.rs"]
mod tests;
