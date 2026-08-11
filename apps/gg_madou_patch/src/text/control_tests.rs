use super::*;

#[test]
fn test_encode_no_double_terminator() {
    let syllables = vec!['가'];
    let ko_enc = KoEncoding::from_syllables(&syllables).unwrap();
    let result = encode_ko_text("가{END}", &ko_enc).unwrap();
    // Should end with single 0xFE, not double
    assert_eq!(*result.last().unwrap(), 0xFE);
    // The byte before 0xFE should NOT be 0xFE (no double)
    assert_ne!(result[result.len() - 2], 0xFE);
}

#[test]
fn test_en_char_encoding() {
    assert_eq!(en_char_to_byte(' '), Some(0x0B));
    assert_eq!(en_char_to_byte('A'), Some(0x0C));
    assert_eq!(en_char_to_byte('Z'), Some(0x25));
    assert_eq!(en_char_to_byte('a'), Some(0x26));
    assert_eq!(en_char_to_byte('.'), Some(0x44));
}

#[test]
fn test_encode_ko_text() {
    // '!','.' 모두 KO 글리프(2바이트). ☆(JP 온점 0x7F 오독)는 제거. 숫자는 EN(1바이트).
    let enc = KoEncoding::from_syllables(&['다', '이', '!', '.']).unwrap();
    let bytes = encode_ko_text("다이!.☆5", &enc).unwrap();
    // 다=80 00, 이=80 01, !=80 02, .=80 03, ☆→제거, 5=07(EN), FE=[END]
    assert_eq!(
        bytes,
        vec![0x80, 0x00, 0x80, 0x01, 0x80, 0x02, 0x80, 0x03, 0x07, 0xFE]
    );
}

#[test]
fn test_ellipsis_single_glyph_and_collapse() {
    // '…'(U+2026)는 단일 KO 글리프. '...'(3+ 연속 점)·'・・・'도 단일 '…'로 축약.
    let enc = KoEncoding::from_syllables(&['…', '.']).unwrap();
    // '…' at index0 → [80 00], '.' at index1 → [80 01]
    assert_eq!(encode_ko_text("…", &enc).unwrap(), vec![0x80, 0x00, 0xFE]);
    assert_eq!(encode_ko_text("...", &enc).unwrap(), vec![0x80, 0x00, 0xFE]);
    assert_eq!(
        encode_ko_text("・・・・・", &enc).unwrap(),
        vec![0x80, 0x00, 0xFE]
    );
    // 1~2개 점은 '.'(글리프 index1) 유지
    assert_eq!(
        encode_ko_text("..", &enc).unwrap(),
        vec![0x80, 0x01, 0x80, 0x01, 0xFE]
    );
}

#[test]
fn test_encode_control_codes() {
    let enc = KoEncoding::from_syllables(&['다']).unwrap();
    let bytes = encode_ko_text("다\n다{WAIT}", &enc).unwrap();
    // 다=80 00, \n=FD, 다=80 00, {WAIT}=FC, FE=[END]
    assert_eq!(bytes, vec![0x80, 0x00, 0xFD, 0x80, 0x00, 0xFC, 0xFE]);
}

#[test]
fn test_encode_var() {
    let enc = KoEncoding::from_syllables(&[]).unwrap();
    let bytes = encode_ko_text("{VAR:01}", &enc).unwrap();
    // {VAR:01}=F8 01, FE=[END]
    assert_eq!(bytes, vec![0xF8, 0x01, 0xFE]);
}

#[test]
fn test_encode_symbol_as_ko_glyph() {
    let enc = KoEncoding::from_syllables(&['♪']).unwrap();
    let bytes = encode_ko_text("♪", &enc).unwrap();
    assert_eq!(bytes, vec![0x80, 0x00, 0xFE]);
}

#[test]
fn test_unsupported_character_errors() {
    let enc = KoEncoding::from_syllables(&[]).unwrap();
    let err = encode_ko_text("€", &enc).unwrap_err();
    assert!(err.to_string().contains("unsupported character"));
}

#[test]
fn test_jp_literal_overrides_replace_en_bytes() {
    let enc = KoEncoding::from_syllables_with_prefixes(&[], [0xF0, 0xF1, 0xF2])
        .expect("JP-direct encoding")
        .with_literal_overrides(&[(' ', 0x00), ('-', 0x42)]);
    assert_eq!(encode_ko_text(" -", &enc).unwrap(), vec![0x00, 0x42, 0xFE]);
}
