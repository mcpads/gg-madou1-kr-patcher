//! BPS 가변길이 정수(VLI) 인코드/디코드.

/// u64 값을 BPS VLI로 인코딩해 buf에 덧붙인다.
pub fn encode(buf: &mut Vec<u8>, mut data: u64) {
    loop {
        let x = (data & 0x7F) as u8;
        data >>= 7;
        if data == 0 {
            buf.push(0x80 | x);
            break;
        }
        buf.push(x);
        data -= 1;
    }
}

/// 바이트 슬라이스에서 BPS VLI를 디코딩한다. (value, 소비 바이트 수) 반환.
pub fn decode(data: &[u8]) -> Result<(u64, usize), &'static str> {
    let mut result: u64 = 0;
    let mut shift: u64 = 1;
    for (i, &byte) in data.iter().enumerate() {
        result += u64::from(byte & 0x7F) * shift;
        if byte & 0x80 != 0 {
            return Ok((result, i + 1));
        }
        shift <<= 7;
        result += shift;
    }
    Err("unexpected end of VLI data")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_range() {
        for n in [0u64, 1, 127, 128, 255, 16384, 1_048_576, u32::MAX as u64] {
            let mut buf = Vec::new();
            encode(&mut buf, n);
            let (val, consumed) = decode(&buf).unwrap();
            assert_eq!(val, n, "roundtrip failed for {n}");
            assert_eq!(consumed, buf.len());
        }
    }
}
