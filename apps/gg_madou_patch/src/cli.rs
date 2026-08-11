use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "gg-madou-patch")]
#[command(about = "Game Gear Madou Monogatari I Korean patch tool")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Display ROM header information
    Info {
        /// Path to ROM file
        rom: PathBuf,
    },
    /// Disassemble Z80 code
    Disasm {
        /// Path to ROM file
        rom: PathBuf,
        /// Physical ROM offset range (e.g., 0x238A-0x23FF)
        #[arg(long)]
        range: Option<String>,
        /// Logical Z80 address (use with --bank)
        #[arg(long)]
        addr: Option<String>,
        /// Bank number (for --addr mode)
        #[arg(long)]
        bank: Option<u8>,
    },
    /// Create a distribution BPS patch from a supported source ROM and target ROM
    Bps {
        /// Supported source ROM (JP original for the JP→KR release)
        #[arg(long)]
        source: PathBuf,
        /// Target ROM (Korean-patched build output)
        #[arg(long)]
        target: PathBuf,
        /// Output BPS patch file
        #[arg(long)]
        output: PathBuf,
    },
    /// JP-native build: M0 raw, M1 render proof, or translated release build
    JpBuild {
        /// Path to JP original ROM (512KB)
        jp_rom: PathBuf,
        /// Output ROM path
        #[arg(long, default_value = "out/jp_native_raw.gg")]
        output: PathBuf,
        /// M1 render test: apply JP-native renderer patches + font + brute
        /// KO injection (requires --font and --translations)
        #[arg(long)]
        render: bool,
        /// Translated build: relocate all JP text tables and encode KO with
        /// $F0-$F2 (requires --font and --translations)
        #[arg(long)]
        translated: bool,
        /// TTF font path (for --render or --translated)
        #[arg(long)]
        font: Option<PathBuf>,
        /// Translations dir (for --render font generation or --translated)
        #[arg(long)]
        translations: Option<PathBuf>,
    },
    /// JP-native structure verify (M0): hook signature + pointer tables
    JpVerify {
        /// Path to JP original ROM (512KB)
        jp_rom: PathBuf,
    },
}
