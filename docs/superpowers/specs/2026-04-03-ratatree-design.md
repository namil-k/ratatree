# ratatree — ratatui 파일/디렉토리 피커 위젯 설계

## 개요

ratatui 기반 TUI 애플리케이션에서 사용할 수 있는 파일/디렉토리 피커 위젯 크레이트.
다른 프로젝트에서 `ratatree`를 의존성으로 추가하면 바로 사용 가능.

## 핵심 결정사항

| 항목 | 결정 | 대안 및 이유 |
|---|---|---|
| 뷰 | 리스트 뷰 기본 + 트리 뷰 런타임 토글 (Tab) | 초기 설정 고정 → 유연성 부족, trait 기반 분리 → dynamic dispatch 불필요 |
| 선택 | 다중 선택 (`Vec<PathBuf>`) | 단일 선택만 → 범용성 부족 |
| 이벤트 | `handle_event(Event)` 패턴 | 개별 메서드 방식 → 사용자 코드 복잡, handle_event이 ratatui 생태계 표준 |
| 필터 | 콜백 `Fn(&Path) -> bool` + 실시간 fuzzy find | 확장자만 → 제한적, glob → 콜백이 상위호환 |
| 키 바인딩 | Vim 모션 + 화살표키 병행 | 한쪽만 → 사용자층 제한 |
| 마우스 | 기본 마우스 (클릭 선택 + 스크롤) | 없음 → 접근성 부족, 풀 마우스 → 복잡도 대비 효용 낮음 |
| 숨김 파일 | `.` 키로 토글, 기본 숨김 | vim `.`과 충돌 없음 (파일 피커에서 반복 명령 불필요) |
| 심볼릭 링크 | Follow + 순환 감지 (visited_dirs 추적) | 표시만 → UX 제한, follow만 → 무한루프 위험 |
| 스타일 | `FilePickerTheme` 구조체로 ratatui Style 주입 | 기본 테마만 → 커스터마이징 불가, 아이콘 추가 → Nerd Font 의존 |
| 아키텍처 | Enum 뷰 + 공유 상태 (C안) | 단일 State → 뷰별 로직 분기 복잡, trait 기반 → 불필요한 동적 디스패치 |

## Public API

### 핵심 타입

```rust
// 사용자가 직접 다루는 타입들
FilePicker          // StatefulWidget, 렌더링 담당
FilePickerState     // 모든 상태, handle_event() 제공
FilePickerTheme     // Style 커스터마이징
PickerMode          // FilesOnly | DirsOnly | Both
ViewMode            // List | Tree
PickerResult        // Pending | Selected(Vec<PathBuf>) | Cancelled
```

### 사용 예시

```rust
let mut state = FilePickerState::builder()
    .start_dir("~/projects")
    .mode(PickerMode::Both)
    .view(ViewMode::List)
    .filter(|path| {
        path.extension()
            .map(|e| e == "rs" || e == "toml")
            .unwrap_or(true)
    })
    .theme(my_custom_theme)
    .build();

// 이벤트 루프
loop {
    terminal.draw(|f| {
        f.render_stateful_widget(FilePicker::default(), f.area(), &mut state);
    })?;

    if let Event::Key(key) = crossterm::event::read()? {
        state.handle_event(Event::Key(key));
    }

    match state.result() {
        PickerResult::Selected(paths) => return Ok(paths),
        PickerResult::Cancelled => return Ok(vec![]),
        PickerResult::Pending => {}
    }
}
```

## 내부 아키텍처

### 타입 구조

```
FilePickerState
├── common: CommonState
│   ├── current_dir: PathBuf
│   ├── entries: Vec<Entry>
│   ├── selected: HashSet<PathBuf>
│   ├── show_hidden: bool
│   ├── search_query: Option<String>
│   ├── visited_dirs: HashSet<PathBuf>    // 순환 감지
│   ├── input_mode: InputMode             // Normal | Search
│   ├── pending_key: Option<(char, Instant)>  // gg 시퀀스
│   └── error_message: Option<String>     // 상태 바 표시용
└── view: ViewState
    ├── List(ListViewState)   // cursor, scroll_offset
    └── Tree(TreeViewState)   // cursor, scroll_offset, expanded: HashSet<PathBuf>
```

### 데이터 흐름

```
crossterm::Event
      │
      ▼
state.handle_event(event)
      │
      ├─ Normal모드 → 키/마우스 → 네비게이션/선택/토글
      ├─ Search모드 → 텍스트입력 → fuzzy 필터링
      └─ Enter → PickerResult::Selected 반환

frame.render_stateful_widget(picker, area, &mut state)
      │
      ├─ 경로 바 렌더링
      ├─ ViewState에 따라 List/Tree 렌더링
      └─ 상태 바 렌더링 (선택 수, 검색 쿼리, 에러 등)

state.result() → PickerResult 확인
```

## 키 바인딩

### Normal 모드 — 네비게이션

| 키 | 동작 |
|---|---|
| `j` / `↓` | 커서 아래로 |
| `k` / `↑` | 커서 위로 |
| `l` / `→` | 디렉토리 진입 (파일에서는 무시) |
| `h` / `←` / `Backspace` | 상위 디렉토리로 |
| `gg` | 맨 위로 (2-키 시퀀스, 500ms 타임아웃) |
| `G` | 맨 아래로 |
| `Ctrl+D` | 반 페이지 아래로 |
| `Ctrl+U` | 반 페이지 위로 |

### Normal 모드 — 액션

| 키 | 동작 |
|---|---|
| `Space` | 다중 선택 토글 |
| `Enter` | 디렉토리면 진입, 파일이면 확정. 다중 선택이 있으면 선택된 목록 반환, 없으면 커서 위치 항목 반환 |
| `Esc` / `q` | 취소 (Cancelled 반환) |
| `Tab` | 리스트 ↔ 트리 뷰 전환 |
| `.` | 숨김 파일 표시 토글 |
| `/` / `Ctrl+F` | Search 모드 진입 |
| `~` | 홈 디렉토리로 이동 |

### Search 모드

- 타이핑 → 현재 디렉토리 항목을 실시간 fuzzy 매칭
- `Enter` → 필터 유지하며 Normal로 복귀
- `Esc` → 필터 해제하며 Normal로 복귀
- `j/k`, `↑/↓` → 검색 결과 내에서 커서 이동

### InputMode 전환

```
Normal ──( / 또는 Ctrl+F )──▶ Search
Search ──( Esc 또는 Enter )──▶ Normal
```

## 에러 처리

핵심 원칙: **위젯은 panic하지 않는다.** `handle_event()`와 `render()`는 항상 성공.

| 상황 | 처리 |
|---|---|
| 권한 없는 디렉토리 진입 | 진입 안 됨 + 상태 바에 "Permission denied" 표시 |
| 삭제된 경로 접근 | 상위 디렉토리로 자동 이동 |
| 빈 디렉토리 | "(empty)" 표시, 네비게이션 정상 동작 |
| 심볼릭 링크 순환 감지 | 진입 차단 + "Circular symlink" 표시 |
| 읽기 실패한 항목 | 해당 항목 건너뜀 (목록에서 제외) |

## 파일 구조

```
src/
├── lib.rs          — pub re-exports
├── entry.rs        — Entry 타입 + 디렉토리 읽기
├── state.rs        — FilePickerState, CommonState, builder 패턴
├── widget.rs       — impl StatefulWidget for FilePicker
├── event.rs        — handle_event: 키/마우스 디스패치, gg 시퀀스, InputMode
├── theme.rs        — FilePickerTheme, Default 구현
├── search.rs       — fuzzy matching 로직
└── view/
    ├── mod.rs      — ViewState enum, 공통 렌더 헬퍼
    ├── list.rs     — ListViewState, 리스트 렌더링/네비게이션
    └── tree.rs     — TreeViewState, 트리 렌더링/네비게이션

examples/
└── basic.rs        — 기본 사용 예제
```

## 의존성

```toml
[dependencies]
ratatui = "0.29"
crossterm = "0.28"
dirs = "5"

[dev-dependencies]
tempfile = "3"
```

- fuzzy search는 외부 크레이트 없이 직접 구현 (간단한 substring + 스코어링)
- 추후 필요시 `fuzzy-matcher` 크레이트로 교체 가능

## 테스트 전략

1. **유닛 테스트** (각 모듈)
   - Entry 생성, 정렬, 필터링
   - Fuzzy search 매칭 로직
   - 커서 이동 (경계 조건: 맨 위/아래, 빈 목록)
   - gg 시퀀스 파싱, 타임아웃
   - 숨김 파일 토글

2. **통합 테스트** (임시 디렉토리)
   - tempdir로 파일 구조 생성 → State로 탐색
   - 다중 선택 → result() 검증
   - 뷰 전환 후 상태 유지 검증
   - 심볼릭 링크 순환 감지 검증

3. **렌더링 스냅샷** (선택)
   - ratatui TestBackend로 렌더링 결과 확인
   - 리스트/트리 뷰 출력 비교

## 레이아웃

```
┌─────────────────────────────────────────┐
│ 📂 ~/projects/ratatree/src              │  ← 경로 바
│─────────────────────────────────────────│
│   📁 widgets/                           │
│ ▸ 📄 lib.rs                             │  ← 커서
│   ✓ 📄 mod.rs                           │  ← 선택됨
│   🔗 config → ../config                 │
│─────────────────────────────────────────│
│ 2 selected | . hidden | / search        │  ← 상태 바
└─────────────────────────────────────────┘
```
