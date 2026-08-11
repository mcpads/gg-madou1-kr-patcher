use anyhow::{Result, bail};
use gg_sms::mapper::SegaMapper;
use std::path::Path;

pub fn run(
    rom_path: &Path,
    range: Option<&str>,
    addr: Option<&str>,
    bank: Option<u8>,
) -> Result<()> {
    let data = std::fs::read(rom_path)?;

    let (start, end, logical_start) = if let Some(range_str) = range {
        let (start, end) = parse_range(range_str)?;
        (start, end, None)
    } else if let (Some(addr_str), Some(bank_num)) = (addr, bank) {
        let logical_addr = parse_hex_u16(addr_str)?;
        let phys = SegaMapper::logical_to_physical(logical_addr, bank_num, bank_num);
        (phys, phys + 0x100, Some(logical_addr)) // default 256 bytes
    } else {
        bail!("provide --range (physical offsets) or --addr with --bank (logical address)");
    };

    if end > data.len() {
        bail!(
            "range {:#X}-{:#X} exceeds ROM size {:#X}",
            start,
            end,
            data.len()
        );
    }

    let mut offset = start;
    while offset < end && offset < data.len() {
        let bytes = &data[offset..];
        match z80::decode_bytes(bytes) {
            Ok(decoded) => {
                let len = decoded.length();
                let raw: Vec<String> = bytes[..len].iter().map(|b| format!("{b:02X}")).collect();
                let (bank_num, bank_offset) = SegaMapper::physical_to_bank(offset);
                let logical = if let Some(base) = logical_start {
                    base.wrapping_add((offset - start) as u16)
                } else if bank_num == 0 {
                    bank_offset
                } else {
                    0x8000 + bank_offset
                };
                println!(
                    "{offset:#07X} (${logical:04X} B{bank_num:02}): {:<12} {}",
                    raw.join(" "),
                    decoded.instruction()
                );
                offset += len;
            }
            Err(e) => {
                println!("{offset:#07X}: {:02X}           ; error: {e}", data[offset]);
                offset += 1;
            }
        }
    }
    Ok(())
}

fn parse_range(s: &str) -> Result<(usize, usize)> {
    let parts: Vec<&str> = s.split('-').collect();
    if parts.len() != 2 {
        bail!("range format: 0xSTART-0xEND");
    }
    let start = parse_hex_usize(parts[0])?;
    let end = parse_hex_usize(parts[1])?;
    Ok((start, end))
}

fn parse_hex_usize(s: &str) -> Result<usize> {
    let s = s.trim_start_matches("0x").trim_start_matches("0X");
    usize::from_str_radix(s, 16).map_err(|e| anyhow::anyhow!("invalid hex '{}': {}", s, e))
}

fn parse_hex_u16(s: &str) -> Result<u16> {
    let s = s.trim_start_matches("0x").trim_start_matches("0X");
    u16::from_str_radix(s, 16).map_err(|e| anyhow::anyhow!("invalid hex '{}': {}", s, e))
}
