use super::*;

#[test]
fn test_ko_encoding_basic() {
    let syllables = vec!['다', '이', '가'];
    let enc = KoEncoding::from_syllables(&syllables).unwrap();
    assert_eq!(enc.encode_char('다'), Some([0x80, 0x00]));
    assert_eq!(enc.encode_char('이'), Some([0x80, 0x01]));
    assert_eq!(enc.encode_char('가'), Some([0x80, 0x02]));
    assert_eq!(enc.encode_char('나'), None); // not in encoding
}

#[test]
fn test_ko_encoding_multi_prefix() {
    let syllables: Vec<char> = (0..300)
        .map(|i| char::from_u32(0xAC00 + i).unwrap())
        .collect();
    let enc = KoEncoding::from_syllables(&syllables).unwrap();
    // Index 0 → 0x80 0x00
    assert_eq!(
        enc.encode_char(char::from_u32(0xAC00).unwrap()),
        Some([0x80, 0x00])
    );
    // Index 256 → 0x81 0x00
    assert_eq!(
        enc.encode_char(char::from_u32(0xAC00 + 256).unwrap()),
        Some([0x81, 0x00])
    );
}

#[test]
fn test_collect_syllables() {
    let entries = vec![
        ("다다다이가".to_string(), false),
        ("".to_string(), true), // skipped
    ];
    let syllables = collect_korean_syllables(&entries);
    assert_eq!(syllables[0], '다'); // most frequent (3)
    assert_eq!(syllables.len(), 3);
}

#[test]
fn test_ko_encoding_boundary_255_256() {
    let syllables: Vec<char> = (0..300)
        .map(|i| char::from_u32(0xAC00 + i).unwrap())
        .collect();
    let enc = KoEncoding::from_syllables(&syllables).unwrap();
    // Index 255 → 0x80 0xFF
    assert_eq!(
        enc.encode_char(char::from_u32(0xAC00 + 255).unwrap()),
        Some([0x80, 0xFF])
    );
    // Index 256 → 0x81 0x00
    assert_eq!(
        enc.encode_char(char::from_u32(0xAC00 + 256).unwrap()),
        Some([0x81, 0x00])
    );
}

#[test]
fn test_ko_encoding_boundary_511_512() {
    let syllables: Vec<char> = (0..520)
        .map(|i| char::from_u32(0xAC00 + i).unwrap())
        .collect();
    let enc = KoEncoding::from_syllables(&syllables).unwrap();
    // Index 511 → 0x81 0xFF
    assert_eq!(
        enc.encode_char(char::from_u32(0xAC00 + 511).unwrap()),
        Some([0x81, 0xFF])
    );
    // Index 512 → 0x82 0x00
    assert_eq!(
        enc.encode_char(char::from_u32(0xAC00 + 512).unwrap()),
        Some([0x82, 0x00])
    );
}

#[test]
fn test_ko_encoding_empty() {
    let enc = KoEncoding::from_syllables(&[]).unwrap();
    assert_eq!(enc.syllable_count(), 0);
}

#[test]
fn test_ko_encoding_too_many_syllables() {
    let syllables: Vec<char> = (0..769)
        .map(|i| char::from_u32(0xAC00 + i).unwrap())
        .collect();
    let result = KoEncoding::from_syllables(&syllables);
    assert!(result.is_err());
}

#[test]
fn test_jp_direct_prefixes() {
    let syllables: Vec<char> = (0..520)
        .map(|i| char::from_u32(0xAC00 + i).unwrap())
        .collect();
    let enc = KoEncoding::from_syllables_with_prefixes(&syllables, [0xF0, 0xF1, 0xF2])
        .expect("JP-direct encoding");
    assert_eq!(enc.encode_char(syllables[0]), Some([0xF0, 0x00]));
    assert_eq!(enc.encode_char(syllables[255]), Some([0xF0, 0xFF]));
    assert_eq!(enc.encode_char(syllables[256]), Some([0xF1, 0x00]));
    assert_eq!(enc.encode_char(syllables[512]), Some([0xF2, 0x00]));
}

#[test]
fn test_custom_prefixes_must_be_consecutive() {
    let err = match KoEncoding::from_syllables_with_prefixes(&['가'], [0xF0, 0xF2, 0xF3]) {
        Ok(_) => panic!("non-consecutive prefix set must fail"),
        Err(err) => err,
    };
    assert!(err.to_string().contains("must be consecutive"));
}
