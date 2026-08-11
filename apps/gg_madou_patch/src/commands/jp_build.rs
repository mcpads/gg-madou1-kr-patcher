use anyhow::{Context, Result, bail};
use std::path::Path;

/// JP-native build selector. Raw is M0; `render` is the M1 brute-injection
/// proof; `translated` is the M2 relocated full-text candidate.
pub fn run(
    jp_rom_path: &Path,
    output_path: &Path,
    render: bool,
    translated: bool,
    font: Option<&Path>,
    translations: Option<&Path>,
) -> Result<()> {
    if render && translated {
        bail!("--render and --translated are mutually exclusive");
    }
    let mode = if translated {
        "translated (M2)"
    } else if render {
        "render (M1)"
    } else {
        "raw (M0)"
    };
    println!("=== JP-native {mode} build ===");
    let jp = std::fs::read(jp_rom_path)
        .with_context(|| format!("failed to read JP ROM: {}", jp_rom_path.display()))?;
    println!("  Loaded {} bytes ({} banks)", jp.len(), jp.len() / 0x4000);

    let out = if render || translated {
        let option = if translated {
            "--translated"
        } else {
            "--render"
        };
        let font = font.ok_or_else(|| anyhow::anyhow!("{option} requires --font"))?;
        let translations =
            translations.ok_or_else(|| anyhow::anyhow!("{option} requires --translations"))?;
        if !font.exists() {
            bail!("font file not found: {}", font.display());
        }
        if translated {
            crate::jp::build_translated(&jp, font, translations)
                .context("failed M2 translated build")?
        } else {
            crate::jp::build_render(&jp, font, translations).context("failed M1 render build")?
        }
    } else {
        crate::jp::expand_to_1mb(&jp).context("failed to expand JP ROM to 1MiB")?
    };
    println!(
        "  Output {} bytes ({} banks)",
        out.len(),
        out.len() / 0x4000
    );

    if let Some(parent) = output_path.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create output dir: {}", parent.display()))?;
    }
    std::fs::write(output_path, &out)
        .with_context(|| format!("failed to write output: {}", output_path.display()))?;
    println!("  Written to {}", output_path.display());
    Ok(())
}
