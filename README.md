# wterm

시리얼 · SSH · Telnet · 로컬 셸을 하나의 창에서 타일로 나눠 쓰는 터미널 앱입니다.
Tauri 2 (Rust) + SvelteKit + xterm.js 로 만들어졌습니다.

## 기능

**연결**

- **시리얼** — 포트 자동 감지(사용 중인 포트는 회색 처리), baud rate 선택 또는 직접 입력
- **SSH** — `~/.ssh` 키 인증을 먼저 시도하고, 실패하면 비밀번호로 전환 (비밀번호 없는 계정도 접속 가능). 비밀번호는 저장하지 않습니다
- **Telnet** — IAC 옵션 협상(ECHO/SGA/NAWS/BINARY/TERMINAL-TYPE) 지원, 로그인 프롬프트에 사용자 ID 자동 입력
- **로컬** — 설치된 WSL 배포판, PowerShell, 명령 프롬프트를 ConPTY로 실행. PowerShell/cmd는 시작 위치를 지정할 수 있습니다

**화면**

- 타일을 좌우/상하로 2~4등분 분할, 창 전체 기준으로 네 방향에 타일 추가
- 분할선 드래그로 크기 조절, 타이틀 바를 드래그해 타일 위치 교환
- 타일 확대/축소(선택한 타일만 전체 화면), 타일별 닫기
- 다크 / 라이트 테마, 색상 직접 지정(배경·글자·커서·강조), 한국어 / 영어 UI

**편의 기능**

- **연결 이력** — 접속 성공 시 자동 저장, 클릭 한 번으로 재접속. 연결 종류별로 마지막 설정을 기억합니다
- **로그 저장** — 타일별로 세션 출력을 파일로 기록. 파일명은 `{종류}-{대상}_{yyyymmdd-hhmmss.sss}.log`
  형식으로 자동 생성되며, 줄마다 수신 시각 기록 / 제어 문자 제거(순수 텍스트) 옵션을 제공합니다
- **멀티 입력** — 여러 타일을 선택해 같은 명령을 한 번에 전송. 보낸 명령은 이력으로 저장되어
  ↑/↓ 키나 이력 버튼으로 다시 꺼내 쓸 수 있고, 항목별 삭제와 전체 삭제를 지원합니다
- **클립보드** — 드래그로 선택하면 자동 복사, 우클릭으로 붙여넣기 (PuTTY 방식)

## 폰트

기본 폰트는 **D2Coding** 입니다. 한글과 영문의 폭이 잘 맞는 고정폭 폰트라 터미널에서 표가 깨지지 않습니다.

설치되어 있지 않으면 폴백 폰트(Consolas)로 표시되므로, 아래에서 내려받아 설치하세요.

- 다운로드: https://github.com/naver/d2-coding-font

설치 후 앱을 재시작하면 자동으로 적용됩니다. 다른 폰트를 쓰려면 툴바의 **설정 → 폰트**에서
시스템에 설치된 고정폭 폰트 목록에서 선택할 수 있고, 크기와 굵기도 함께 지정할 수 있습니다.

## 설정 파일

모든 설정(언어·테마·색상·폰트·로그 폴더·연결 이력)이 `wterm-settings.json` 한 파일에 저장됩니다.
저장 위치는 **실행 파일과 같은 폴더**이므로, 폴더째로 USB 등에 옮기면 설정도 함께 따라갑니다.

## 단축키

| 키 | 동작 |
| --- | --- |
| `Ctrl+Shift+D` | 오른쪽에 타일 추가 |
| `Ctrl+Shift+E` | 아래에 타일 추가 |
| `Ctrl+Shift+W` | 활성 타일 닫기 |

## 개발

필요 환경: Node.js + pnpm, Rust 툴체인, WebView2 (Windows 11 기본 포함)

```bash
pnpm install     # 의존성 설치 (xterm 패치 자동 적용)
pnpm tauri dev   # 개발 모드 실행
pnpm tauri build # 배포본 빌드
pnpm check       # 타입 검사
```

`pnpm install` 시 `patches/@xterm__xterm@6.0.0.patch` 가 적용됩니다. 한글 IME 입력 시 글자가
중복 입력되는 xterm.js 버그(업스트림 PR #6041)를 수정하는 패치이므로 제거하지 마세요.

창 제목에는 버전과 git 리비전이 표시됩니다 (`wterm v0.1.0 (eacd377)`). 커밋되지 않은 변경이
있는 상태로 빌드하면 리비전 뒤에 `+` 가 붙습니다.

구조와 라이브러리 선정 근거는 [docs/architecture.html](docs/architecture.html) 에 정리해 두었습니다.

## 권장 IDE 설정

[VS Code](https://code.visualstudio.com/) + [Svelte](https://marketplace.visualstudio.com/items?itemName=svelte.svelte-vscode) + [Tauri](https://marketplace.visualstudio.com/items?itemName=tauri-apps.tauri-vscode) + [rust-analyzer](https://marketplace.visualstudio.com/items?itemName=rust-lang.rust-analyzer)
