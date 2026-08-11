//! 베이크드 UI 버튼(bank 2·11)의 한글화 타일 — **빌드시 dalmoori 폰트에서 생성**.
//!
//! JP 원본의 필드 메뉴·상점·타이틀·파일슬롯·확인창 버튼과 방향석 東/西/南/北의
//! 둥근 프레임은 유지하고 내부 자형만 dalmoori 한글로 교체한다. nametable 직접
//! 배치 그래픽이라 텍스트 엔진·재번역으로 못 닿음 → 직접 교체.
//!
//! **KO 타일은 include_bytes가 아니라 fontdue로 빌드시 합성**(KO 텍스트 글리프와 동일 경로).
//! 프레임은 지원 JP ROM에서 직접 읽고 SHA-256을 검증한다
//! (ROM 슬라이스: menu `[0xA2B8..+1344]`, file `[0xA838..+384]`, yesno `[0xAFB8..+384]`,
//! shop `[0xB138..+512]`, title `[0x2F55C..+384]`). 방향석은 원본 자산을 별도 저장하지 않고
//! ROM `[0x2F6DC..+768]`의 SHA-256을 검증한 뒤 그 프레임을 사용한다. 파이프라인 전체가 Rust.
//! 위치: `docs/reverse_engineering/baked_text_inventory.md`.

use anyhow::{Context, Result};
use std::path::Path;

/// 첫 버튼 타일의 JP ROM 물리 오프셋(bank 2). 필드 메뉴 6개 + 배틀 RUN이 여기서부터 연속.
pub const MENU_BUTTON_BASE: usize = 0xA2B8;

/// 버튼당 바이트 수 (6 타일 3×2 × 32B 4bpp).
pub const MENU_BUTTON_BYTES: usize = 6 * 32;

/// 버튼 개수. 필드 메뉴 6개 + 배틀 RUN(0xA738, ROM 연속 7번째).
/// 배틀 커맨드 메뉴는 마도/물건(필드 MAGIC/ITEM 타일)을 재사용하고 RUN만 배틀 전용.
pub const MENU_BUTTON_COUNT: usize = 7;

/// 지원 JP 원본의 메뉴 버튼 블록 SHA-256.
pub const MENU_BUTTONS_JP_SHA256: [u8; 32] = [
    0x66, 0xFC, 0x77, 0xBF, 0xD0, 0x10, 0x6D, 0x37, 0x73, 0x8F, 0x5C, 0x60, 0x36, 0x4B, 0xFC, 0xD9,
    0x5E, 0x12, 0x00, 0x03, 0xDD, 0xDF, 0x07, 0x27, 0x9F, 0x5D, 0x1F, 0x54, 0xFA, 0xA8, 0x88, 0x7A,
];

/// 각 버튼의 한글 라벨(ROM 순서). 빌드시 이 라벨을 dalmoori로 합성.
pub const MENU_BUTTON_LABELS: [&str; MENU_BUTTON_COUNT] =
    ["지도", "음성", "저장", "로드", "마도", "물건", "도망"];

// --- 상점 버튼(BUY/SEL/GO) ---
// ROM 0xB138부터 16타일 연속(VRAM 402-417). SEL·GO가 우측 타일(404/407)을 공유(게임 dedup)
// 하지만, 세 라벨을 전부 '-기'로 끝맺어(사기/팔기/가기) 공유 타일이 양쪽 다 '기'의 우측절반으로
// 일치 → 네임테이블 코드 패치 없이 8px 중앙정렬 달성. 16타일 블록 통째 교체.

/// 상점 버튼 블록의 JP ROM 물리 오프셋(bank 2). 16타일 연속.
pub const SHOP_BUTTON_BASE: usize = 0xB138;

/// 상점 버튼 블록 바이트 수(16 타일 × 32B 4bpp).
pub const SHOP_BUTTON_BYTES: usize = 16 * 32;

/// 지원 JP 원본의 상점 버튼 블록 SHA-256.
pub const SHOP_BUTTONS_JP_SHA256: [u8; 32] = [
    0x17, 0xD3, 0x1A, 0x01, 0x96, 0x8F, 0xA6, 0x90, 0xB9, 0xDD, 0x10, 0x66, 0x72, 0x13, 0x05, 0x29,
    0xBD, 0x16, 0x66, 0x52, 0x92, 0xAE, 0xC3, 0x55, 0x44, 0x31, 0x24, 0xA2, 0x8E, 0x1C, 0x6B, 0xA2,
];

/// 상점 라벨 — SEL·GO는 반드시 '기' 어미(공유 타일 정합).
const SHOP_SEL: &str = "팔기"; // tiles 402-407
const SHOP_BUY: &str = "사기"; // tiles 408-413
const SHOP_GO: &str = "가기"; // tiles 414,415,404,416,417,407

// --- 타이틀 버튼(NEW/LOAD) ---
// bank 11 ROM 0x2f55c부터 NEW(6타일)+LOAD(6타일) 연속(0x2f55c+192=0x2f61c). 타일은
// column-major 배열(bank 2 메뉴 버튼과 다름). 파일선택 FILE1~4는 아래 별도 블록에서 일기1~4로.

/// 타이틀 NEW 버튼의 JP ROM 물리 오프셋(bank 11). LOAD는 바로 뒤 0x2f61c.
pub const TITLE_BUTTON_BASE: usize = 0x2F55C;

/// 타이틀 버튼 개수(NEW, LOAD).
pub const TITLE_BUTTON_COUNT: usize = 2;

/// 지원 JP 원본의 타이틀 버튼 블록 SHA-256.
pub const TITLE_BUTTONS_JP_SHA256: [u8; 32] = [
    0x06, 0x5C, 0x30, 0x45, 0x12, 0xE2, 0x25, 0xCA, 0x2F, 0xCA, 0x57, 0xCC, 0x6B, 0xCA, 0x57, 0x72,
    0xC6, 0x2D, 0x11, 0x60, 0xF0, 0x4A, 0x9C, 0x8A, 0x00, 0x23, 0x92, 0x9A, 0xC1, 0xC2, 0x45, 0x7A,
];

/// 타이틀 라벨(ROM 순서: NEW, LOAD). 타이틀 LOAD는 "이어서 플레이" 의미라 "계속"
/// (시작/계속 = 새 게임/이어하기). 필드메뉴 LOAD는 저장과 짝이라 별도로 "로드" 유지.
const TITLE_LABELS: [&str; TITLE_BUTTON_COUNT] = ["시작", "계속"];

// --- 파일 슬롯 버튼(FILE1~4 → 일기1~4) ---
// 저장/로드 화면의 파일 슬롯. JP 원본은 "日記"(일기) — EN이 FILE로 바꾼 걸 JP 충실로 복원.
// ROM 0xA838부터 12타일(row-major). 4버튼이 왼쪽 2타일(일기)을 공유하고 우측 타일만 숫자로
// 다름 → 숫자를 우측 differing 타일(col 16+)에 두면 상점 '기' 트릭처럼 nametable 패치 불필요.

/// 파일 슬롯 버튼 블록의 JP ROM 물리 오프셋(bank 2). 12타일 연속.
pub const FILE_BUTTON_BASE: usize = 0xA838;

/// 파일 슬롯 블록 바이트 수(12 타일 × 32B).
pub const FILE_BUTTON_BYTES: usize = 12 * 32;

/// 지원 JP 원본의 파일 슬롯 버튼 블록 SHA-256.
pub const FILE_BUTTONS_JP_SHA256: [u8; 32] = [
    0xE3, 0x26, 0xD9, 0xD0, 0x25, 0x66, 0xFA, 0xC3, 0x6E, 0x04, 0xB9, 0x60, 0xDA, 0x20, 0x07, 0xD2,
    0xEC, 0xB4, 0x72, 0x5A, 0x6C, 0xCE, 0x04, 0x59, 0x09, 0xB9, 0xE5, 0xDF, 0xD3, 0x6A, 0x6E, 0xD3,
];

/// 일기 공유 접두 + 슬롯 숫자.
const FILE_PREFIX: &str = "일기";

/// 2글자 라벨 중앙정렬(cols [4,12]).
const CENTER2: &[usize] = &[4, 12];

/// 1글자 라벨 중앙정렬(col 8 = 가운데 타일).
const CENTER1: &[usize] = &[8];

// --- YES/NO 확인 버튼(→ 응/아니) ---
// 상점 구매/흥정 확인창의 YES/NO. VRAM은 shop 버튼과 같은 슬롯(402+)을 쓰나 ROM 소스는
// 별도: bank 2 0xAFB8부터 YES(6타일)+NO(6타일) 연속(0xAFB8+192=0xB078, +192=0xB138 shop).

/// YES 버튼의 JP ROM 물리 오프셋(bank 2). NO는 바로 뒤 0xB078.
pub const YESNO_BUTTON_BASE: usize = 0xAFB8;

/// 지원 JP 원본의 YES/NO 버튼 블록 SHA-256.
pub const YESNO_BUTTONS_JP_SHA256: [u8; 32] = [
    0xE7, 0xF5, 0x15, 0xCD, 0xAD, 0x34, 0x96, 0x26, 0x5F, 0x65, 0xF2, 0xD8, 0x16, 0x33, 0x08, 0x8D,
    0x24, 0xDE, 0xC8, 0x01, 0x1A, 0x70, 0x6A, 0xA5, 0x22, 0x9B, 0x52, 0x4E, 0x17, 0x47, 0x96, 0x37,
];

/// FILE 공유버튼: 일기(왼쪽 2타일 걸침) + 숫자(우측 differing 타일). dalmoori는 네이티브
/// 8px라 7px 다운스케일 시 fontdue가 픽셀정렬을 깨 뭉개짐 → 8px 유지. 라벨을 우측 2px 이동
/// ([3,10,18])해 좌측 프레임에서 떼고 균형. 숫자는 col18(우측 differing 타일)이라 공유 유지.
const FILE_XS: &[usize] = &[3, 10, 18];

// --- 방향석(東/西/南/北 → 동/서/남/북) ---
// bank 11 ROM 0x2F6DC부터 6타일 버튼 4개가 연속한다. 각 버튼은 타이틀 버튼과 같은
// column-major 배열이다. ROM 그룹 순서는 화면 위치 기준 아래(北), 오른쪽(西), 위(南), 왼쪽(東).

/// 방향석 버튼 블록의 JP ROM 물리 오프셋(bank 11).
pub const DIRECTION_BUTTON_BASE: usize = 0x2F6DC;

/// 방향석 버튼 개수(北, 西, 南, 東).
pub const DIRECTION_BUTTON_COUNT: usize = 4;

/// 방향석 블록 바이트 수(4버튼 × 6타일 × 32B).
pub const DIRECTION_BUTTON_BYTES: usize = DIRECTION_BUTTON_COUNT * MENU_BUTTON_BYTES;

/// 지원 JP 원본의 방향석 768바이트 SHA-256.
pub const DIRECTION_BUTTONS_JP_SHA256: [u8; 32] = [
    0xC5, 0x0E, 0xD1, 0xF9, 0x7D, 0xF6, 0xF8, 0x2C, 0x5F, 0x5B, 0x2E, 0xBE, 0xBC, 0xAF, 0x9F, 0x79,
    0x12, 0xE5, 0xB5, 0x3B, 0x31, 0x8F, 0x91, 0xC7, 0x57, 0x0B, 0x95, 0xFE, 0xA3, 0xCD, 0xC2, 0x2E,
];

/// ROM 그룹 순서에 대응하는 한글 라벨(北, 西, 南, 東).
const DIRECTION_LABELS: [&str; DIRECTION_BUTTON_COUNT] = ["북", "서", "남", "동"];

// ---------------------------------------------------------------------------
// 타일 합성 (fontdue) — KO 텍스트 글리프와 동일한 dalmoori 렌더 경로.
// ---------------------------------------------------------------------------

/// 4bpp SMS 타일(32B) → 8×8 색인 그리드.
fn decode_tile(t: &[u8]) -> [[u8; 8]; 8] {
    let mut g = [[0u8; 8]; 8];
    for row in 0..8 {
        let planes = [t[row * 4], t[row * 4 + 1], t[row * 4 + 2], t[row * 4 + 3]];
        for (c, cell) in g[row].iter_mut().enumerate() {
            let bit = 0x80u8 >> c;
            let mut v = 0u8;
            for (p, plane) in planes.iter().enumerate() {
                if plane & bit != 0 {
                    v |= 1 << p;
                }
            }
            *cell = v;
        }
    }
    g
}

/// 24×16 그리드의 (gx,gy) 8×8 영역 → 4bpp 타일(32B).
fn encode_tile(grid: &[[u8; 24]; 16], gx: usize, gy: usize) -> [u8; 32] {
    let mut out = [0u8; 32];
    for row in 0..8 {
        let mut planes = [0u8; 4];
        for c in 0..8 {
            let v = grid[gy + row][gx + c];
            let bit = 0x80u8 >> c;
            for (p, plane) in planes.iter_mut().enumerate() {
                if v & (1 << p) != 0 {
                    *plane |= bit;
                }
            }
        }
        out[row * 4..row * 4 + 4].copy_from_slice(&planes);
    }
    out
}

/// 6타일 블록의 타일 인덱스 i → 24×16 그리드 내 (gx,gy) 픽셀 원점.
/// bank 2 버튼(메뉴·상점)은 row-major(위3+아래3), bank 11 타이틀 버튼은 column-major
/// (열마다 위·아래) — nametable 배치가 다름(baked_text_inventory.md).
fn tile_origin(i: usize, col_major: bool) -> (usize, usize) {
    if col_major {
        ((i / 2) * 8, (i % 2) * 8)
    } else {
        ((i % 3) * 8, (i / 3) * 8)
    }
}

/// 원본 6타일(192B) 프레임 + 라벨 → KO 버튼 타일(192B). 프레임 유지, 내부만 한글 삽입.
/// 글자는 `size`px, `xs`의 각 열 원점·`ypos`행에 배치. 획 = 색인 15.
/// fontdue 렌더는 dalmoori tight bbox로 PIL top-align과 픽셀 일치(검증됨).
/// `col_major`: ROM 타일 배열 순서(위 `tile_origin`). `xs`: 글자별 시작 열(2글자 [4,12] 중앙,
/// FILE 공유버튼 [1,8,16]=숫자를 우측 differing 타일에). `size`/`ypos`: 폰트 px·세로 시작행
/// (전부 8.0/row4 — dalmoori 네이티브 8px).
struct ButtonLayout<'a> {
    col_major: bool,
    xs: &'a [usize],
    size: f32,
    ypos: usize,
    clear_x: std::ops::Range<usize>,
}

fn compose_button_with_layout(
    source192: &[u8],
    label: &str,
    font: &fontdue::Font,
    layout: &ButtonLayout<'_>,
) -> [u8; 192] {
    let mut grid = [[0u8; 24]; 16];
    for i in 0..6 {
        let t = decode_tile(&source192[i * 32..i * 32 + 32]);
        let (tx, ty) = tile_origin(i, layout.col_major);
        for r in 0..8 {
            for c in 0..8 {
                grid[ty + r][tx + c] = t[r][c];
            }
        }
    }
    // 내부 텍스트 영역(rows 3-12, 호출자 지정 cols) → 버튼 면(색인 1)으로 클리어.
    for row in grid.iter_mut().take(13).skip(3) {
        for cell in row
            .iter_mut()
            .take(layout.clear_x.end)
            .skip(layout.clear_x.start)
        {
            *cell = 1;
        }
    }
    // 라벨 글자 스탬프(호출자가 준 열 원점 `xs`·폰트크기 `size`·세로 시작 `ypos`에).
    for (ch, &ox) in label.chars().zip(layout.xs.iter()) {
        let (m, bmp) = font.rasterize(ch, layout.size);
        for gy in 0..m.height {
            for gx in 0..m.width {
                if bmp[gy * m.width + gx] >= 128 {
                    let (rr, cc) = (layout.ypos + gy, ox + gx);
                    if rr < 16 && cc < 24 {
                        grid[rr][cc] = 15;
                    }
                }
            }
        }
    }
    let mut out = [0u8; 192];
    for i in 0..6 {
        let (gx, gy) = tile_origin(i, layout.col_major);
        out[i * 32..i * 32 + 32].copy_from_slice(&encode_tile(&grid, gx, gy));
    }
    out
}

/// 일반 24×16 버튼 합성. 내부 텍스트 영역 x=3..20을 지운다.
fn compose_button(
    source192: &[u8],
    label: &str,
    font: &fontdue::Font,
    col_major: bool,
    xs: &[usize],
    size: f32,
    ypos: usize,
) -> [u8; 192] {
    compose_button_with_layout(
        source192,
        label,
        font,
        &ButtonLayout {
            col_major,
            xs,
            size,
            ypos,
            clear_x: 3..21,
        },
    )
}

/// dalmoori 폰트 로드.
fn load_font(ttf_path: &Path) -> Result<fontdue::Font> {
    let data = std::fs::read(ttf_path)
        .with_context(|| format!("메뉴 타일용 폰트 로드 실패: {}", ttf_path.display()))?;
    fontdue::Font::from_bytes(data, fontdue::FontSettings::default())
        .map_err(|e| anyhow::anyhow!("폰트 파싱 실패: {e}"))
}

fn generate_menu_buttons_from(ttf_path: &Path, source: &[u8]) -> Result<Vec<u8>> {
    anyhow::ensure!(
        source.len() == MENU_BUTTON_COUNT * MENU_BUTTON_BYTES,
        "메뉴 버튼 소스 블록 크기 불일치"
    );
    let font = load_font(ttf_path)?;
    let mut out = Vec::with_capacity(MENU_BUTTON_COUNT * MENU_BUTTON_BYTES);
    for i in 0..MENU_BUTTON_COUNT {
        let frame = &source[i * MENU_BUTTON_BYTES..(i + 1) * MENU_BUTTON_BYTES];
        out.extend_from_slice(&compose_button(
            frame,
            MENU_BUTTON_LABELS[i],
            &font,
            false,
            CENTER2,
            8.0,
            4,
        ));
    }
    Ok(out)
}

/// JP 기반 필드 메뉴 7버튼(RUN 포함) KO 타일 블록 생성.
pub fn generate_menu_buttons_jp(ttf_path: &Path, source: &[u8]) -> Result<Vec<u8>> {
    generate_menu_buttons_from(ttf_path, source)
}

/// 상점 16타일 블록 KO 생성 (`SHOP_BUTTON_BYTES`B). SEL(402-407)+BUY(408-413)+GO 고유 4타일.
/// GO는 우측 타일 404/407을 SEL과 공유하므로 '기' 어미 정합을 assert로 검증.
fn generate_shop_block_from(ttf_path: &Path, source: &[u8]) -> Result<Vec<u8>> {
    anyhow::ensure!(
        source.len() == SHOP_BUTTON_BYTES,
        "상점 버튼 소스 블록 크기 불일치"
    );
    let font = load_font(ttf_path)?;
    let tile = |i: usize| &source[i * 32..i * 32 + 32]; // i = tile index 0..15 (= VRAM 402+i)

    let sel = compose_button(&source[0..192], SHOP_SEL, &font, false, CENTER2, 8.0, 4); // tiles 402-407
    let buy = compose_button(&source[192..384], SHOP_BUY, &font, false, CENTER2, 8.0, 4); // tiles 408-413
    // GO 프레임 = tiles [414,415,404,416,417,407] = 블록 인덱스 [12,13,2,14,15,5]
    let mut go_frame = Vec::with_capacity(192);
    for &i in &[12usize, 13, 2, 14, 15, 5] {
        go_frame.extend_from_slice(tile(i));
    }
    let go = compose_button(&go_frame, SHOP_GO, &font, false, CENTER2, 8.0, 4);

    // 공유 타일 정합: SEL의 404(위치2)·407(위치5) == GO가 재사용하는 '기' 우측절반.
    anyhow::ensure!(
        sel[64..96] == go[64..96],
        "상점 SEL/GO 공유 타일 404 불일치 — 어미가 '기'가 아님?"
    );
    anyhow::ensure!(
        sel[160..192] == go[160..192],
        "상점 SEL/GO 공유 타일 407 불일치"
    );

    // ko 블록: 타일 402-417 순서 = SEL(6) + BUY(6) + GO 고유 4타일(414,415,416,417).
    let mut out = Vec::with_capacity(SHOP_BUTTON_BYTES);
    out.extend_from_slice(&sel);
    out.extend_from_slice(&buy);
    out.extend_from_slice(&go[0..64]); // tiles 414,415
    out.extend_from_slice(&go[96..160]); // tiles 416,417
    anyhow::ensure!(out.len() == SHOP_BUTTON_BYTES, "상점 블록 크기 불일치");
    Ok(out)
}

/// JP 기반 상점 버튼 블록 생성.
pub fn generate_shop_block_jp(ttf_path: &Path, source: &[u8]) -> Result<Vec<u8>> {
    generate_shop_block_from(ttf_path, source)
}

/// 타이틀 NEW/LOAD 2버튼 KO 타일 블록 생성 (`TITLE_BUTTON_COUNT * 192`B).
/// column-major 타일 배열. NEW→시작, LOAD→로드.
fn generate_title_buttons_from(ttf_path: &Path, source: &[u8]) -> Result<Vec<u8>> {
    anyhow::ensure!(
        source.len() == TITLE_BUTTON_COUNT * MENU_BUTTON_BYTES,
        "타이틀 버튼 소스 블록 크기 불일치"
    );
    let font = load_font(ttf_path)?;
    let mut out = Vec::with_capacity(TITLE_BUTTON_COUNT * MENU_BUTTON_BYTES);
    for i in 0..TITLE_BUTTON_COUNT {
        let frame = &source[i * MENU_BUTTON_BYTES..(i + 1) * MENU_BUTTON_BYTES];
        out.extend_from_slice(&compose_button(
            frame,
            TITLE_LABELS[i],
            &font,
            true,
            CENTER2,
            8.0,
            4,
        ));
    }
    Ok(out)
}

/// JP 기반 타이틀 버튼 생성.
pub fn generate_title_buttons_jp(ttf_path: &Path, source: &[u8]) -> Result<Vec<u8>> {
    generate_title_buttons_from(ttf_path, source)
}

/// 파일 슬롯 12타일 블록 KO 생성(`FILE_BUTTON_BYTES`B). FILE1~4 → 일기1~4.
/// 4버튼이 왼쪽 2타일(일기)을 공유하므로: FILE1 프레임으로 일기1~4를 합성해 일기 타일 정합을
/// assert로 검증하고, 일기1의 6타일 + 일기2~4의 우측(differing) 숫자 타일만 조립.
fn generate_file_block_from(ttf_path: &Path, source: &[u8]) -> Result<Vec<u8>> {
    anyhow::ensure!(
        source.len() == FILE_BUTTON_BYTES,
        "파일 버튼 소스 블록 크기 불일치"
    );
    let font = load_font(ttf_path)?;
    let frame = &source[0..192]; // 첫 슬롯 6타일 = 모든 슬롯 공통 프레임 소스
    let btn = |n: char| -> [u8; 192] {
        compose_button(
            frame,
            &format!("{FILE_PREFIX}{n}"),
            &font,
            false,
            FILE_XS,
            8.0,
            4,
        )
    };
    let (b1, b2, b3, b4) = (btn('1'), btn('2'), btn('3'), btn('4'));

    // 공유 타일 정합: 일기(row-major 인덱스 0·1 top, 3·4 bot)가 슬롯마다 동일해야 함
    // (숫자가 우측 differing 타일 col16+에 갇혀 일기 타일 미침범).
    for &i in &[0usize, 1, 3, 4] {
        anyhow::ensure!(
            b1[i * 32..i * 32 + 32] == b2[i * 32..i * 32 + 32]
                && b1[i * 32..i * 32 + 32] == b3[i * 32..i * 32 + 32]
                && b1[i * 32..i * 32 + 32] == b4[i * 32..i * 32 + 32],
            "FILE 공유 타일 {i} 불일치 — 일기가 우측 숫자칸 침범?"
        );
    }

    // 12타일 조립(ROM 0xA838 순서): 일기1 6타일(192-197) + 일기2~4의 우측 타일
    // (인덱스 2=숫자 top, 5=숫자 bot) → 198,199 / 19a,19b / 19c,19d.
    let mut out = Vec::with_capacity(FILE_BUTTON_BYTES);
    out.extend_from_slice(&b1);
    for b in [&b2, &b3, &b4] {
        out.extend_from_slice(&b[64..96]); // 인덱스 2 = 숫자 top
        out.extend_from_slice(&b[160..192]); // 인덱스 5 = 숫자 bot
    }
    anyhow::ensure!(out.len() == FILE_BUTTON_BYTES, "FILE 블록 크기 불일치");
    Ok(out)
}

/// JP 기반 파일 슬롯 버튼 생성.
pub fn generate_file_block_jp(ttf_path: &Path, source: &[u8]) -> Result<Vec<u8>> {
    generate_file_block_from(ttf_path, source)
}

/// YES/NO 확인 버튼 KO 타일 블록 생성(2 * 192B). YES→응(1글자 중앙), NO→아니(2글자).
fn generate_yesno_buttons_from(ttf_path: &Path, source: &[u8]) -> Result<Vec<u8>> {
    anyhow::ensure!(
        source.len() == 2 * MENU_BUTTON_BYTES,
        "YES/NO 버튼 소스 블록 크기 불일치"
    );
    let font = load_font(ttf_path)?;
    let yes = &source[0..192];
    let no = &source[192..384];
    let mut out = Vec::with_capacity(2 * MENU_BUTTON_BYTES);
    out.extend_from_slice(&compose_button(yes, "응", &font, false, CENTER1, 8.0, 4));
    out.extend_from_slice(&compose_button(no, "아니", &font, false, CENTER2, 8.0, 4));
    Ok(out)
}

/// JP 기반 YES/NO 확인 버튼 생성.
pub fn generate_yesno_buttons_jp(ttf_path: &Path, source: &[u8]) -> Result<Vec<u8>> {
    generate_yesno_buttons_from(ttf_path, source)
}

/// JP 방향석 버튼 4개를 원본 ROM 프레임에서 한글로 합성한다.
///
/// 원본 블록은 호출자가 `DIRECTION_BUTTONS_JP_SHA256`으로 검증한 뒤 전달한다. 새 원본 그래픽
/// fixture를 저장소에 복제하지 않으면서도 프레임·팔레트와 column-major 타일 배열을 보존한다.
pub fn generate_direction_buttons_jp(ttf_path: &Path, source: &[u8]) -> Result<Vec<u8>> {
    anyhow::ensure!(
        source.len() == DIRECTION_BUTTON_BYTES,
        "방향석 버튼 소스 블록 크기 불일치"
    );
    let font = load_font(ttf_path)?;
    let mut out = Vec::with_capacity(DIRECTION_BUTTON_BYTES);
    for (i, label) in DIRECTION_LABELS.iter().enumerate() {
        let frame = &source[i * MENU_BUTTON_BYTES..(i + 1) * MENU_BUTTON_BYTES];
        // 방향석 프레임은 일반 버튼보다 안쪽(x=3·20)에 세로 테두리가 있어 x=4..19만 지운다.
        out.extend_from_slice(&compose_button_with_layout(
            frame,
            label,
            &font,
            &ButtonLayout {
                col_major: true,
                xs: CENTER1,
                size: 8.0,
                ypos: 4,
                clear_x: 4..20,
            },
        ));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const TTF: &str = "../../assets/fonts/dalmoori.ttf";

    fn jp_rom() -> Option<Vec<u8>> {
        let path = std::env::var("GG_MADOU1_JP_ROM").unwrap_or_else(|_| {
            "../../roms/Madou Monogatari I - 3-Tsu no Madoukyuu (Japan).gg".to_string()
        });
        std::fs::read(path).ok()
    }

    #[test]
    fn jp_fixture_button_generation() {
        let Some(jp) = jp_rom() else {
            eprintln!("skip: JP ROM absent");
            return;
        };
        let menu_source =
            &jp[MENU_BUTTON_BASE..MENU_BUTTON_BASE + MENU_BUTTON_COUNT * MENU_BUTTON_BYTES];
        let shop_source = &jp[SHOP_BUTTON_BASE..SHOP_BUTTON_BASE + SHOP_BUTTON_BYTES];
        let title_source =
            &jp[TITLE_BUTTON_BASE..TITLE_BUTTON_BASE + TITLE_BUTTON_COUNT * MENU_BUTTON_BYTES];
        let file_source = &jp[FILE_BUTTON_BASE..FILE_BUTTON_BASE + FILE_BUTTON_BYTES];
        let yesno_source = &jp[YESNO_BUTTON_BASE..YESNO_BUTTON_BASE + 2 * MENU_BUTTON_BYTES];

        let menu = generate_menu_buttons_jp(Path::new(TTF), menu_source).unwrap();
        let shop = generate_shop_block_jp(Path::new(TTF), shop_source).unwrap();
        let title = generate_title_buttons_jp(Path::new(TTF), title_source).unwrap();
        let file = generate_file_block_jp(Path::new(TTF), file_source).unwrap();
        let yesno = generate_yesno_buttons_jp(Path::new(TTF), yesno_source).unwrap();

        assert_ne!(menu.as_slice(), menu_source);
        assert_ne!(shop.as_slice(), shop_source);
        assert_ne!(title.as_slice(), title_source);
        assert_ne!(file.as_slice(), file_source);
        assert_ne!(yesno.as_slice(), yesno_source);

        // 자형은 바뀌어도 첫 버튼의 상단 프레임 경계는 JP 원본 그대로다.
        assert_eq!(menu[0..4], menu_source[0..4]);
        assert_eq!(title[0..4], title_source[0..4]);
    }

    #[test]
    fn direction_button_generation_preserves_frames_and_order() {
        let mut grid = [[0u8; 24]; 16];
        for row in grid.iter_mut().take(13).skip(3) {
            row[3] = 7;
            row[20] = 7;
        }
        let mut frame = Vec::with_capacity(MENU_BUTTON_BYTES);
        for i in 0..6 {
            let (gx, gy) = tile_origin(i, true);
            frame.extend_from_slice(&encode_tile(&grid, gx, gy));
        }
        let source = frame.repeat(DIRECTION_BUTTON_COUNT);
        let ko = generate_direction_buttons_jp(Path::new(TTF), &source).unwrap();
        assert_eq!(ko.len(), DIRECTION_BUTTON_BYTES);
        for i in 0..DIRECTION_BUTTON_COUNT {
            let start = i * MENU_BUTTON_BYTES;
            assert_eq!(
                ko[start..start + 4],
                source[start..start + 4],
                "방향석 버튼 {i}: 상단 프레임 변경됨"
            );
            for y in 3..13 {
                for x in [3usize, 20] {
                    let tile_index = (x / 8) * 2 + (y / 8);
                    let tile_start = start + tile_index * 32;
                    let tile = decode_tile(&ko[tile_start..tile_start + 32]);
                    assert_eq!(
                        tile[y % 8][x % 8],
                        7,
                        "방향석 버튼 {i}: x={x}, y={y} 세로 테두리 변경됨"
                    );
                }
            }
        }
        for i in 1..DIRECTION_BUTTON_COUNT {
            assert_ne!(
                ko[0..MENU_BUTTON_BYTES],
                ko[i * MENU_BUTTON_BYTES..(i + 1) * MENU_BUTTON_BYTES],
                "방향석 버튼 {i}: 라벨 또는 ROM 그룹 순서가 구분되지 않음"
            );
        }
    }
}
