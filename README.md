# 마도물어 I (게임기어) 한글 패처

게임기어용 《마도물어 I 3개의 마도구》 일본판에 한글 패치를 적용하는 Rust 코드입니다. 일본판 원본 검증, 텍스트 인코딩·재배치, 한글 폰트 생성, 베이크드 UI와 Z80 훅 적용, BPS 생성·적용 기능을 제공합니다.

버튼 프레임은 사용자가 제공한 지원 일본판 ROM의 고정 구간에서 직접 읽고 SHA-256을 검증한 뒤 한글 UI 합성에 사용합니다. 추출한 JP/EN 버튼 fixture는 Git에 포함하지 않습니다. `assets/graphics/title_logo_wordmark.hex`만 패치가 새로 적용하는 타이틀 로고 레이어로 제공합니다. 과거 영어판 기반 빌드 입력과 명령도 공개 범위에 포함하지 않습니다.

배포용 BPS와 적용 방법은 [마도물어 시리즈 한글 번역 프로젝트](https://github.com/mcpads/madou-monogatari-kr-patch#마도물어-i--게임기어)에서 제공합니다.

## 제공하지 않는 파일

이 저장소는 다음 파일을 제공하지 않습니다.

- 원본·패치 적용 ROM과 BPS 산출물
- 대사 JSON을 비롯한 번역 자산
- 한글 폰트 파일
- 내부 조사 문서, 작업 기록, 스크린샷과 런타임 증거

정당한 원본과 프로젝트 입력을 가진 사용자가 아래 경로에 필요한 파일을 직접 배치해야 합니다.

```text
assets/
├── fonts/
│   └── dalmoori.ttf
└── translations/
    └── complete/
```

지원 일본판 ROM은 524,288바이트이며 SHA-256은 다음과 같습니다.

```text
4a87f02f358688bc7680d0d34f527e10a087fec95dbf7ed131241d8ffe4c0654
```

## 빌드와 검사

외부 입력을 배치한 뒤 다음 명령을 실행합니다.

```bash
cargo build --release -p gg-madou-patch

GG_MADOU1_JP_ROM="<일본판 ROM 경로>" cargo test --workspace

cargo run --release -p gg-madou-patch -- jp-verify \
  "<일본판 ROM 경로>"

cargo run --release -p gg-madou-patch -- jp-build \
  "<일본판 ROM 경로>" \
  --translated \
  --font assets/fonts/dalmoori.ttf \
  --translations assets/translations/complete \
  --output out/gg-madou1-kr.gg
```

컴파일 자체에는 원본 ROM·폰트·번역 자산이 필요하지 않습니다. 실제 한글 ROM 생성과 원본 의존 통합 검사에는 해당 외부 입력이 필요합니다. 원본이나 번역 자산 없이도 사용할 수 있는 일반 기능은 ROM 헤더 처리, Game Gear 주소 변환, BPS 처리와 합성 입력 기반 단위 검사입니다.

## 라이선스

이 저장소의 소스 코드는 [MIT License](LICENSE)로 제공합니다. 원작 게임, 사용자가 별도로 준비하는 폰트·번역 입력과 원본에서 파생되는 자료의 권리는 각 권리자에게 있습니다.
