pub mod bps;
mod cli;
mod commands;
pub mod font;
pub mod jp;
pub mod patch;
pub mod text;
pub mod translation;

use anyhow::Result;
use clap::Parser;

fn main() -> Result<()> {
    let cli = cli::Cli::parse();
    match cli.command {
        cli::Commands::Info { rom } => commands::info::run(&rom),
        cli::Commands::Disasm {
            rom,
            range,
            addr,
            bank,
        } => commands::disasm::run(&rom, range.as_deref(), addr.as_deref(), bank),
        cli::Commands::Bps {
            source,
            target,
            output,
        } => commands::bps::run(&source, &target, &output),
        cli::Commands::JpBuild {
            jp_rom,
            output,
            render,
            translated,
            font,
            translations,
        } => commands::jp_build::run(
            &jp_rom,
            &output,
            render,
            translated,
            font.as_deref(),
            translations.as_deref(),
        ),
        cli::Commands::JpVerify { jp_rom } => commands::jp_verify::run(&jp_rom),
    }
}
