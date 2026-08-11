//! BPS 패치 적용 (source + patch → target). 자체검증 및 `bps` 명령용.

use super::vli;

const BPS_MAGIC: &[u8; 4] = b"BPS1";

pub fn apply(source: &[u8], patch: &[u8]) -> Result<Vec<u8>, String> {
    if patch.len() < 16 {
        return Err("patch too small".into());
    }
    if &patch[..4] != BPS_MAGIC {
        return Err("not a BPS patch (invalid magic)".into());
    }

    // patch CRC 검증
    let patch_body = &patch[..patch.len() - 4];
    let stored_patch_crc = u32::from_le_bytes(patch[patch.len() - 4..].try_into().unwrap());
    if stored_patch_crc != crc32fast::hash(patch_body) {
        return Err("patch CRC mismatch".into());
    }

    let mut pos = 4;
    let (source_size, n) = vli::decode(&patch[pos..]).map_err(str::to_string)?;
    pos += n;
    let (target_size, n) = vli::decode(&patch[pos..]).map_err(str::to_string)?;
    pos += n;
    if target_size > 64 * 1024 * 1024 {
        return Err(format!("target size too large: {target_size}"));
    }
    let (metadata_size, n) = vli::decode(&patch[pos..]).map_err(str::to_string)?;
    pos += n;
    pos += metadata_size as usize;
    if pos > patch.len() {
        return Err("metadata extends beyond patch".into());
    }
    if source.len() as u64 != source_size {
        return Err(format!(
            "source size mismatch (expected {source_size}, got {})",
            source.len()
        ));
    }

    // source CRC 검증 (잘못된 ROM이면 여기서 실패)
    let footer_start = patch.len() - 12;
    let stored_source_crc = u32::from_le_bytes(patch[footer_start..footer_start + 4].try_into().unwrap());
    if stored_source_crc != crc32fast::hash(source) {
        return Err(format!(
            "source CRC mismatch — wrong ROM? (expected {stored_source_crc:08X}, got {:08X})",
            crc32fast::hash(source)
        ));
    }

    let mut target = vec![0u8; target_size as usize];
    let mut output_offset: usize = 0;
    let mut source_relative_offset: i64 = 0;
    let mut target_relative_offset: i64 = 0;
    let action_end = footer_start;

    while pos < action_end {
        let (data, n) = vli::decode(&patch[pos..]).map_err(str::to_string)?;
        pos += n;
        let action = data & 3;
        let length = ((data >> 2) + 1) as usize;

        match action {
            0 => {
                // SourceRead
                for i in 0..length {
                    let idx = output_offset + i;
                    target[idx] = if idx < source.len() { source[idx] } else { 0 };
                }
                output_offset += length;
            }
            1 => {
                // TargetRead
                if pos + length > action_end {
                    return Err("TargetRead extends beyond patch data".into());
                }
                target[output_offset..output_offset + length].copy_from_slice(&patch[pos..pos + length]);
                pos += length;
                output_offset += length;
            }
            2 => {
                // SourceCopy
                let (off, n) = vli::decode(&patch[pos..]).map_err(str::to_string)?;
                pos += n;
                let sign = if off & 1 != 0 { -1i64 } else { 1i64 };
                source_relative_offset += sign * (off >> 1) as i64;
                for _ in 0..length {
                    let idx = source_relative_offset as usize;
                    if idx >= source.len() {
                        return Err(format!("SourceCopy out of bounds: {idx}"));
                    }
                    target[output_offset] = source[idx];
                    output_offset += 1;
                    source_relative_offset += 1;
                }
            }
            3 => {
                // TargetCopy
                let (off, n) = vli::decode(&patch[pos..]).map_err(str::to_string)?;
                pos += n;
                let sign = if off & 1 != 0 { -1i64 } else { 1i64 };
                target_relative_offset += sign * (off >> 1) as i64;
                for _ in 0..length {
                    let idx = target_relative_offset as usize;
                    if idx >= output_offset {
                        return Err(format!("TargetCopy out of bounds: {idx}"));
                    }
                    target[output_offset] = target[idx];
                    output_offset += 1;
                    target_relative_offset += 1;
                }
            }
            _ => unreachable!(),
        }
        if output_offset > target.len() {
            return Err("output exceeds target size".into());
        }
    }

    // target CRC 검증
    let stored_target_crc = u32::from_le_bytes(patch[footer_start + 4..footer_start + 8].try_into().unwrap());
    if stored_target_crc != crc32fast::hash(&target) {
        return Err("target CRC mismatch".into());
    }
    Ok(target)
}
