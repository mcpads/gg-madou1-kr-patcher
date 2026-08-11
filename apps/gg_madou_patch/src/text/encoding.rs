use std::collections::HashMap;

// KO 글리프(dalmoori 생성)로 렌더할 기호. 金은 텍스트에서 '금'(한글)으로 치환하므로 제외.
// 문자부호(!,~,?,,,.)와 EN 문자(A-Z,a-z)도 KO 글리프로 옮겨 EN VWF 폰트 의존 제거
// → 폰트 생성으로 일괄 제어(문자부호는 1px 작게, 마침표/쉼표는 베이스라인 정렬). §is_ko_glyph_char
// '.'은 EN 폰트의 좌하단 작은 점(garbage처럼 보임) 대신 dalmoori로 렌더. ☆/・는 '.'으로 정규화.
const KO_GLYPH_SYMBOLS: &[char] = &['♪', '↓', '↑', '♥', '▼', '&', '!', '~', '?', ',', '.', '…'];

/// 생략부호 정규화: `.`/`・` 3개 이상 연속 런 → 단일 `…` 글리프(1타일, 컴팩트).
/// 1~2개는 `.`으로 유지. `…`(이미 단일)는 그대로 통과.
/// collect_korean_syllables와 encode_ko_text가 **동일하게** 적용해야
/// (빈도 수집 ↔ 실제 인코딩) `…` 글리프 누락으로 인한 bail을 막는다.
pub fn collapse_ellipsis(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '.' || c == '・' {
            let mut n = 1;
            while matches!(chars.peek(), Some('.') | Some('・')) {
                chars.next();
                n += 1;
            }
            if n >= 3 {
                out.push('…');
            } else {
                for _ in 0..n {
                    out.push('.');
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// 글리프 문자 정규화. `None` = 렌더에서 제거.
/// `☆`: JP 원본 byte 0x7F = 온점 `。`을 구 디코더가 ☆로 오독한 것(jp_charmap.md).
///   번역자가 문말 `~` 등을 이미 넣어, ☆(=。)를 마침표로 렌더하면 `~.` 2중 종결 → **제거**.
/// `・`: 나카구로 → 마침표 `.`으로 통일.
pub fn normalize_glyph_char(ch: char) -> Option<char> {
    match ch {
        '☆' => None,
        '・' => Some('.'),
        _ => Some(ch),
    }
}

/// Korean syllable → 2-byte encoding mapping
pub struct KoEncoding {
    char_to_bytes: HashMap<char, [u8; 2]>,
    literal_overrides: HashMap<char, u8>,
    index_to_char: Vec<char>,
}

impl KoEncoding {
    /// Build from a list of syllables sorted by frequency.
    ///
    /// Returns an error if the syllable list exceeds 768 entries (the
    /// maximum supported by the 3-prefix-byte scheme: 0x80/0x81/0x82 × 256).
    pub fn from_syllables(syllables: &[char]) -> Result<Self, anyhow::Error> {
        Self::from_syllables_with_prefixes(syllables, [0x80, 0x81, 0x82])
    }

    /// Build an encoding with caller-selected prefix bytes.
    ///
    /// The legacy EN-base path uses `$80-$82`. The JP-direct path must use
    /// `$F0-$F2`, because `$80-$82` are live native JP characters. Keeping
    /// the prefix set in the encoding object makes the text encoder shared
    /// while letting each ROM base select collision-free bytes explicitly.
    pub fn from_syllables_with_prefixes(
        syllables: &[char],
        prefixes: [u8; 3],
    ) -> Result<Self, anyhow::Error> {
        if prefixes[0].wrapping_add(1) != prefixes[1] || prefixes[1].wrapping_add(1) != prefixes[2]
        {
            anyhow::bail!("KO prefix bytes must be consecutive, got {:02X?}", prefixes);
        }
        if syllables.len() > 768 {
            anyhow::bail!(
                "too many syllables: {} (max 768 for 3 prefix bytes {:02X?})",
                syllables.len(),
                prefixes,
            );
        }

        let mut char_to_bytes = HashMap::new();
        let mut index_to_char = Vec::new();

        for (i, &ch) in syllables.iter().enumerate() {
            let prefix = prefixes[i / 256];
            let sub_index = (i % 256) as u8;
            char_to_bytes.insert(ch, [prefix, sub_index]);
            index_to_char.push(ch);
        }

        Ok(KoEncoding {
            char_to_bytes,
            literal_overrides: HashMap::new(),
            index_to_char,
        })
    }

    /// Override selected one-byte literals for a ROM-specific native
    /// charmap. This is used by the JP-direct path for space (`$00`, handled
    /// by the engine as a blank cell) and hyphen (`$42`, native long dash).
    pub fn with_literal_overrides(mut self, overrides: &[(char, u8)]) -> Self {
        self.literal_overrides.extend(overrides.iter().copied());
        self
    }

    pub fn encode_char(&self, ch: char) -> Option<[u8; 2]> {
        self.char_to_bytes.get(&ch).copied()
    }

    pub fn literal_byte(&self, ch: char) -> Option<u8> {
        self.literal_overrides.get(&ch).copied()
    }

    pub fn syllable_count(&self) -> usize {
        self.index_to_char.len()
    }
}

/// Collect all unique characters rendered by ko_render, sorted by frequency.
///
/// ASCII and safe EN text-engine symbols remain 1-byte EN characters. Hangul and
/// JP-only symbols whose byte values collide with the Korean prefix range are
/// rendered through the Korean 2-byte glyph path.
pub fn collect_korean_syllables(entries: &[(String, bool)]) -> Vec<char> {
    // entries: (ko_text, skip)
    let mut freq: HashMap<char, usize> = HashMap::new();
    for (text, skip) in entries {
        if *skip {
            continue;
        }
        // encode_ko_text와 동일한 생략부호 정규화 후 빈도 집계
        let text = collapse_ellipsis(text);
        for ch in text.chars() {
            let Some(ch) = normalize_glyph_char(ch) else {
                continue; // ☆ 등 제거 문자
            };
            if is_ko_glyph_char(ch) {
                *freq.entry(ch).or_default() += 1;
            }
        }
    }
    let mut syllables: Vec<(char, usize)> = freq.into_iter().collect();
    syllables.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    syllables.into_iter().map(|(ch, _)| ch).collect()
}

pub fn is_hangul_syllable(ch: char) -> bool {
    let code = ch as u32;
    (0xAC00..=0xD7A3).contains(&code)
}

pub fn is_ko_glyph_char(ch: char) -> bool {
    // 한글 + EN 문자(A-Z,a-z) + 기호/문자부호 → KO 글리프(dalmoori). 숫자·공백·□·마침표류는 EN 유지.
    is_hangul_syllable(ch) || ch.is_ascii_alphabetic() || KO_GLYPH_SYMBOLS.contains(&ch)
}

#[cfg(test)]
#[path = "encoding_tests.rs"]
mod tests;
