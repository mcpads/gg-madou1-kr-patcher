use anyhow::{Result, bail};

use crate::text::encoding::{collapse_ellipsis, normalize_glyph_char};

use super::loader::BankTranslation;

/// Return every violation of an entry's declared runtime layout constraint.
///
/// A constraint is attached only after the entry's actual consumer has been
/// confirmed to use that window width. Unclassified dialogue remains in the
/// advisory global scan instead of inheriting an unsafe project-wide limit.
pub fn declared_layout_issues(translations: &[BankTranslation]) -> Result<Vec<String>> {
    let mut issues = Vec::new();

    for bank in translations {
        for entry in &bank.entries {
            let Some(layout) = &entry.layout else {
                continue;
            };
            if layout.max_line_tiles == 0 {
                bail!(
                    "bank {} {} id {} declares max_line_tiles=0",
                    bank.bank,
                    bank.table,
                    entry.id
                );
            }
            for (line_no, line) in display_lines(&entry.ko) {
                let width_half_tiles = visible_half_tiles(&line);
                let max_half_tiles = layout.max_line_tiles * 2;
                if width_half_tiles > max_half_tiles {
                    issues.push(format!(
                        "bank {} {} id {} line {}: {} > {} tiles: {}",
                        bank.bank,
                        bank.table,
                        entry.id,
                        line_no,
                        format_half_tiles(width_half_tiles),
                        layout.max_line_tiles,
                        line
                    ));
                }
            }
        }
    }

    Ok(issues)
}

/// Fail closed on every runtime-confirmed layout constraint.
pub fn validate_declared_layout_constraints(translations: &[BankTranslation]) -> Result<()> {
    let issues = declared_layout_issues(translations)?;
    if !issues.is_empty() {
        bail!(
            "declared layout constraint failed with {} issue(s): {}",
            issues.len(),
            issues.join("; ")
        );
    }
    Ok(())
}

/// Measure a rendered line in half-tile units.
///
/// The static checker and the hard per-consumer gate share this function so
/// advisory candidates cannot drift from the runtime-confirmed width model.
pub(crate) fn visible_half_tiles(line: &str) -> usize {
    collapse_ellipsis(line)
        .chars()
        .filter_map(normalize_glyph_char)
        .map(|ch| match ch {
            '□' | '◎' => 1,
            '-' => 1,
            _ if ch.is_ascii_alphanumeric() => 1,
            _ => 2,
        })
        .sum()
}

pub(crate) fn display_lines(text: &str) -> Vec<(usize, String)> {
    let mut lines = Vec::new();
    let mut current = String::new();
    let mut line_no = 1usize;
    let mut chars = text.chars().peekable();

    while let Some(ch) = chars.next() {
        match ch {
            '\n' => push_line(&mut lines, &mut current, &mut line_no),
            '{' => {
                let tag: String = chars.by_ref().take_while(|&c| c != '}').collect();
                match tag.as_str() {
                    "FF" | "PAGE" | "WAIT" | "END" => {
                        push_line(&mut lines, &mut current, &mut line_no);
                    }
                    _ if tag.starts_with("VAR:") => current.push('#'),
                    _ => {}
                }
            }
            _ => current.push(ch),
        }
    }
    push_line(&mut lines, &mut current, &mut line_no);
    lines
}

fn push_line(lines: &mut Vec<(usize, String)>, current: &mut String, line_no: &mut usize) {
    if !current.is_empty() {
        lines.push((*line_no, std::mem::take(current)));
    }
    *line_no += 1;
}

pub(crate) fn format_half_tiles(half_tiles: usize) -> String {
    if half_tiles.is_multiple_of(2) {
        (half_tiles / 2).to_string()
    } else {
        format!("{}.5", half_tiles / 2)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn width_model_matches_confirmed_runtime_rules() {
        assert_eq!(visible_half_tiles("멈출 수가 없어~"), 18);
        assert_eq!(visible_half_tiles("멈출 수 없어"), 14);
        assert_eq!(visible_half_tiles("가나다라마바사"), 14);
        assert_eq!(visible_half_tiles("가나다라마바사□"), 15);
        assert_eq!(visible_half_tiles("□□□주머니↓"), 11);
        assert_eq!(visible_half_tiles("ABC12"), 5);
        assert_eq!(visible_half_tiles("안녕..."), 6);
    }
}
