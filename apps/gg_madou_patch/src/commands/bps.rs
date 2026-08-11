use anyhow::{Context, Result, bail};
use std::path::Path;

use crate::bps;

/// 지원 원본→KR BPS 패치를 생성하고 자체검증(적용 round-trip)한 뒤 저장한다.
/// 배포 절차는 docs/release_process.md.
pub fn run(source: &Path, target: &Path, output: &Path) -> Result<()> {
    let source_data = std::fs::read(source)
        .with_context(|| format!("failed to read source ROM: {}", source.display()))?;
    let target_data = std::fs::read(target)
        .with_context(|| format!("failed to read target ROM: {}", target.display()))?;

    let patch = bps::create(&source_data, &target_data);

    // 자체검증: 패치를 소스에 적용해 타깃이 그대로 재구성되는지 확인
    match bps::apply(&source_data, &patch) {
        Ok(result) if result == target_data => {}
        Ok(_) => bail!("self-check failed: round-trip 결과가 target과 불일치"),
        Err(e) => bail!("self-check failed: {e}"),
    }

    std::fs::write(output, &patch)
        .with_context(|| format!("failed to write patch: {}", output.display()))?;

    println!(
        "BPS 패치 생성: {} bytes → {}",
        patch.len(),
        output.display()
    );
    println!("  source crc {:08X}", crc32fast::hash(&source_data));
    println!("  target crc {:08X}", crc32fast::hash(&target_data));
    println!("  round-trip 자체검증 통과");
    Ok(())
}
