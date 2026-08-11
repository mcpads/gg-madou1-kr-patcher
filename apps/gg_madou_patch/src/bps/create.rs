//! 지원 원본→번역 ROM의 BPS 패치 생성.
//!
//! SourceRead(변경 없는 구간)/TargetRead(변경 구간 리터럴)만 쓴다. 짧은 우연 일치로
//! 리터럴이 잘게 쪼개지지 않도록, 다시 SourceRead로 전환하려면 최소 `MIN_MATCH`개
//! 연속 일치를 요구한다.

use super::vli;

const BPS_MAGIC: &[u8; 4] = b"BPS1";
/// 리터럴 도중 SourceRead로 되돌아가는 데 필요한 최소 연속 일치 길이.
const MIN_MATCH: usize = 6;

pub fn create(source: &[u8], target: &[u8]) -> Vec<u8> {
    let mut patch = Vec::new();
    patch.extend_from_slice(BPS_MAGIC);
    vli::encode(&mut patch, source.len() as u64);
    vli::encode(&mut patch, target.len() as u64);
    vli::encode(&mut patch, 0); // metadata 없음

    let (tlen, slen) = (target.len(), source.len());
    let mut out = 0;
    while out < tlen {
        if out < slen && target[out] == source[out] {
            // SourceRead: 일치 구간 길이만큼
            let start = out;
            while out < tlen && out < slen && target[out] == source[out] {
                out += 1;
            }
            vli::encode(&mut patch, (((out - start) as u64) - 1) << 2);
        } else {
            // TargetRead: MIN_MATCH개 연속 일치가 나올 때까지 리터럴
            let start = out;
            out += 1;
            while out < tlen {
                if out < slen && target[out] == source[out] {
                    let mut m = 0;
                    while out + m < tlen
                        && out + m < slen
                        && target[out + m] == source[out + m]
                        && m < MIN_MATCH
                    {
                        m += 1;
                    }
                    if m >= MIN_MATCH {
                        break;
                    }
                }
                out += 1;
            }
            vli::encode(&mut patch, ((((out - start) as u64) - 1) << 2) | 1);
            patch.extend_from_slice(&target[start..out]);
        }
    }

    let source_crc = crc32fast::hash(source);
    let target_crc = crc32fast::hash(target);
    patch.extend_from_slice(&source_crc.to_le_bytes());
    patch.extend_from_slice(&target_crc.to_le_bytes());
    let patch_crc = crc32fast::hash(&patch);
    patch.extend_from_slice(&patch_crc.to_le_bytes());
    patch
}

#[cfg(test)]
mod tests {
    use super::super::apply;
    use super::*;

    #[test]
    fn roundtrip_inplace() {
        let source = vec![0x11u8; 4096];
        let mut target = source.clone();
        for i in (100..3000).step_by(37) {
            target[i] = 0xAB;
        }
        let patch = create(&source, &target);
        assert_eq!(&patch[..4], b"BPS1");
        assert_eq!(apply::apply(&source, &patch).unwrap(), target);
    }

    #[test]
    fn roundtrip_identical() {
        let source = vec![7u8; 1024];
        let patch = create(&source, &source);
        assert_eq!(apply::apply(&source, &patch).unwrap(), source);
    }
}
