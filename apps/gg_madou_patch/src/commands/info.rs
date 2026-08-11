use anyhow::Result;
use gg_sms::header::TmrSegaHeader;
use std::path::Path;

pub fn run(rom_path: &Path) -> Result<()> {
    let data = std::fs::read(rom_path)?;
    let header = TmrSegaHeader::parse(&data)?;

    println!("ROM: {}", rom_path.display());
    println!("Size: {} bytes ({} banks)", data.len(), data.len() / 0x4000);
    println!("Product code: {}", header.product_code);
    println!("Version: {}", header.version);
    println!("Region: {:#04X}", header.region);
    println!("ROM size field: {:#04X}", header.rom_size);
    println!("Checksum: {:#06X}", header.checksum);

    let computed = TmrSegaHeader::compute_checksum(&data);
    if computed == header.checksum {
        println!("Checksum verified: OK");
    } else {
        println!("Checksum MISMATCH: computed {:#06X}", computed);
    }

    Ok(())
}
