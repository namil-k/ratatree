# ratatree 0.4.0 - 흔한 다이얼로그 3종 완성 설계

## 목표

ratatree의 직접 사용자는 ratatui 앱에 파일/폴더 선택 화면이 필요한 Rust 개발자다. crates.io의 역의존 크레이트 2개(txxxt, bibox)를 읽어 보면 실제 사용은 하나의 패턴으로 수렴한다. `builder().start_dir(home).mode(FilesOnly | DirsOnly).build()`로 만들고, `handle_event(Event::Key)`만 넘기고, `FilePicker::default()`를 작은 패널이나 전체 화면에 그리고, 결과가 나오면 state를 버린다. 트리 뷰, 검색, 테마, 필터, 마우스는 아무도 쓰지 않는다.

이번 릴리스의 완성 기준은 그 패턴이 담는 다이얼로그 3종이 앱 개발자가 손대지 않아도 실제 조건(50x18 패널, 긴 홈 경로, 수백 개 항목, 빈 폴더)에서 끝까지 되는 것이다.

1. 파일 열기: `PickerMode::FilesOnly`
2. 폴더 고르기: `PickerMode::DirsOnly`
3. 여러 파일 고르기: `PickerMode::Both` + `Space`

## 발견된 걸림돌

코드 워크스루(`src/state.rs`의 `confirm`, `toggle_select`, `is_selectable`, `src/widget.rs`의 `render_path_bar`, `render_status_bar`)에서 나온 것이다.

| 다이얼로그 | 걸림돌 | 원인 |
|---|---|---|
| 폴더 고르기 | 지금 들어와 있는 폴더를 고를 방법이 없다. 빈 폴더에서는 `(empty)`만 보이고 아무것도 못 고른다 | 목록에는 하위 항목만 있고 `confirm()`은 폴더 위에서 항상 진입한다 |
| 폴더 고르기 | 폴더를 고르려면 `Space` 뒤 `Enter`인데 그 힌트가 어디에도 없다 | 상태 바가 `0 selected \| hidden: off \| view: list`뿐이다 |
| 전부 | 좁은 패널에서 경로 바의 오른쪽이 잘려 지금 어느 폴더인지가 안 보인다 | `Paragraph`가 기본으로 오른쪽을 자른다 |
| 전부 | 항목이 수백 개면 지금 어디쯤인지 알 수 없다 | 스크롤바도 위치 숫자도 없다 |
| 전부 | 통합 패턴이 README 산문에만 있고 실행되는 예제가 없다 | 예제가 `basic.rs` 하나뿐이다 |

## 핵심 결정

| 항목 | 결정 | 대안과 이유 |
|---|---|---|
| 현재 폴더 선택 | 목록 맨 위에 `.` 항목. `DirsOnly`에서만 | 전용 키: 힌트 없이는 발견 불가, 커스텀 키맵 앱은 따로 바인딩 필요. `DirsOnly`에서 `Enter`를 선택으로 변경: 빈 폴더와 현재 폴더는 여전히 못 고르고 `Enter` 의미가 모드마다 달라짐. `.`은 셸 관례와 일치하고 새 키가 없으며 목록에 보이므로 힌트 자체가 됨. `Both`에도 넣는 안은 기각: `Both`가 빌더 기본값이라 `mode()`를 안 부른 모든 사용자의 목록 맨 위에 `./`가 생기고, README 목업과 기본 모드 테스트 대부분이 한 칸씩 밀림. 현재 폴더 선택이 필요한 다이얼로그는 `DirsOnly`뿐 |
| `.` 판별 | `entry.path == common.current_dir` | `EntryKind` 변형 추가는 `pub` enum이라 앱의 `match`를 깨는 breaking |
| `.` 삽입 위치 | `read_entries` 바깥, `refresh_entries`와 `build()` | `read_entries`는 트리 뷰 하위 폴더에도 쓰이는데 `.`은 루트에만 있어야 하고, 숨김 토글(`.`으로 시작하는 이름 제외)에 걸리면 안 됨 |
| 경로 바 | 왼쪽을 잘라 뒤쪽 보존, 컴포넌트 경계에서 자르고 `…` 접두 | 오른쪽 자르기(현재): 가장 정보가 많은 마지막 컴포넌트를 잃음. 가운데 생략: 구현 대비 이득 없음 |
| 위치 표시 | 상태 바 앞에 `커서/전체` | 스크롤바: `FilePickerTheme`에 필드가 늘어 breaking이 하나 더 생기고(테마는 `pub` 필드 구조체, README가 리터럴 생성을 안내), 50칸 패널에서 한 칸이 아까움. 숫자로 "어디쯤"은 해결됨 |
| 예제 | `dialogs.rs` 하나. 호스트 앱에서 `1`/`2`/`3`으로 세 다이얼로그를 모달로 띄움 | 다이얼로그마다 예제 파일: 통합 패턴(모달, 결과 수신, state 폐기)이 세 번 중복됨 |
| 버전 | 0.4.0 | API는 안 깨지지만 `DirsOnly`에서 `entries[0]`이 `.`이 되고 `Selected`가 `current_dir`를 돌려줄 수 있음. `entries`를 직접 순회하거나 인덱스로 접근하는 앱에는 관찰 가능한 변화라 patch로 내기엔 위험 |

## 1. `.` 항목

### 동작

- `refresh_entries`와 `FilePickerBuilder::build`에서 목록 읽기에 성공한 뒤, `mode == DirsOnly`이면 인덱스 0에 `Entry { name: ".", path: current_dir.clone(), kind: EntryKind::Directory, depth: 0 }`를 삽입한다. 읽기에 실패하면 삽입하지 않는다. 존재하지 않는 `start_dir`나 권한 없는 폴더를 "선택"해 앱에 돌려주는 일이 없어야 하고, 그 경우 화면은 0.3.0대로 `(empty)`와 `read_error`다.
- 트리 뷰에서도 루트에만 한 번 들어간다. 하위 폴더 목록에는 없다.
- `confirm()`: 커서가 `.`이면 `Selected(vec![current_dir])`. `selected`가 비어 있지 않으면 기존대로 `selected`를 반환한다.
- `toggle_select()`: `.`도 다른 폴더처럼 `selected`에 `current_dir`를 넣고 뺀다. 여러 폴더를 고르는 흐름에서 "여기도 포함"이 자연스럽게 된다.
- `descend()`, `enter_directory()`, `expand_current()`, `collapse_current()`, `toggle_expand_current()`: 커서가 `.`이면 아무것도 하지 않는다. 트리 뷰에서 `.`을 펼치면 `current_dir`를 다시 읽어 목록이 중복되므로 반드시 막는다.
- 검색: 이름이 `.`이라 사용자가 `.`을 치지 않는 한 결과에 안 나온다. 별도 처리 없음.
- 숨김 토글: 영향 없음. `.`은 항상 보인다.
- 폴더 진입 후 커서는 0이라 `.` 위에 놓인다. 폴더 고르기에서는 진입 직후 `Enter`가 곧 "여기"다.
- 렌더링: 다른 폴더와 같은 스타일(`theme.directory`)과 `/` 접미로 `./`으로 그린다. 별도 스타일 없음.
- `toggle_view`와 `reset`의 커서 경로 유지는 `.`의 path가 `current_dir`라 그대로 동작한다.

### 문서

- `PickerMode`의 rustdoc에 "`DirsOnly`에서는 목록 맨 위에 현재 폴더를 뜻하는 `.` 항목이 있다"를 적는다.
- `CommonState::entries`의 rustdoc에 같은 내용을 적어, `entries`를 직접 순회하는 앱이 알게 한다.
- README 키 바인딩 표와 Features에 한 줄.

### 테스트

- `DirsOnly`로 빌드하면 `entries[0].name == "."`이고 path가 `current_dir`다. `FilesOnly`와 `Both`에서는 없다.
- `.` 위에서 `confirm()`하면 `Selected([current_dir])`.
- 빈 폴더에서 `DirsOnly`로 빌드해도 `.`이 있고 고를 수 있다.
- `.` 위에서 `Space` 뒤 `Enter`하면 `Selected([current_dir])`. 다른 폴더에서 `.`을 또 고르면 둘 다 들어 있다.
- `.` 위에서 `descend()`/`enter_directory()`는 `current_dir`와 `entries`를 바꾸지 않는다.
- 트리 뷰에서 `.` 위에서 `expand_current()`해도 `entries` 길이가 그대로다. 펼친 하위 폴더 목록에는 `.`이 없다.
- 숨김 토글을 해도 `.`은 남는다.
- 폴더에 진입하면 커서가 `.`에 있다.
- 읽기 실패 시에는 `.`이 없다.

## 2. 경로 바 왼쪽 잘라내기

### 동작

- 경로의 표시 폭이 영역 폭 이하이면 그대로 그린다. 폭은 `ratatui::text::Span::raw(s).width()`로 잰다. ratatui가 `unicode-width`를 내부에서 쓰지만 재수출하지는 않으므로, `Span::width`를 통해 같은 계산을 얻고 새 의존성은 넣지 않는다.
- 초과하면 앞쪽 컴포넌트를 하나씩 떼면서 `…` + 구분자 + 남은 컴포넌트들(구분자로 연결)이 폭에 들어갈 때까지 줄인다. 구분자는 `std::path::MAIN_SEPARATOR`다. 루트(`/`)와 Windows 접두(`C:`)도 떼어지는 컴포넌트다. 예: `/Users/namilkim/Library/Application Support/app` → `…/Application Support/app`.
- 마지막 컴포넌트 하나만 남아도 안 들어가면 그 컴포넌트를 앞에서 문자 단위로 잘라 `…` 뒤에 붙인다.
- 폭이 1 이하이면 `…`만 그린다.

### 테스트 (`TestBackend` 행 비교)

- 짧은 경로는 그대로.
- `/Users/namilkim/Library/Application Support/app`을 30칸에 그리면 `…/Application Support/app` 계열로 뒤가 보존된다.
- 마지막 컴포넌트가 폭보다 길면 앞이 잘린다.
- 한글 컴포넌트가 섞여도 폭 계산이 맞아 넘치지 않는다.

## 3. 상태 바 위치 표시

### 동작

- 일반 모드 상태 바를 `{cursor+1}/{visible_count} | {n} selected | hidden: {on|off} | view: {list|tree}`로 바꾼다. 목록이 비어 있으면 `0/0`.
- 검색 모드 상태 바는 이미 `(N matches)`가 있어 바꾸지 않는다.
- `error_message`, `read_error` 우선순위는 0.3.0 그대로.

### 테스트

- 5개 항목에서 커서 2에 있으면 `3/5 | ...`로 시작한다.
- 빈 폴더(`FilesOnly`)에서는 `0/0 | ...`.

## 4. 예제 `examples/dialogs.rs`

- 호스트 앱: 화면 가운데에 안내(`1: open file  2: choose folder  3: pick files  q: quit`)와 마지막 결과를 보여준다.
- `1`: `FilesOnly` + `.filter(ext == "pdf" || ext == "md")` + `Block::bordered().title(" Open file ")`
- `2`: `DirsOnly` + `Block::bordered().title(" Choose folder ")`
- `3`: `Both` + `Block::bordered().title(" Pick files ")`
- 모달은 txxxt처럼 **50x18**로 가운데에 띄우고 `Clear`로 배경을 지운다. 좁은 패널에서 2번, 3번이 실제로 보이게 하기 위해서다.
- 피커가 열려 있으면 이벤트를 피커에 넘기고, `Selected`/`Cancelled`가 나오면 결과를 호스트에 기록하고 state를 버린다. README "Integration with Your App"과 같은 구조.
- `Cargo.toml`에 `[[example]] name = "dialogs"` 추가. README "Running the Example"에 한 줄.
- `basic.rs`는 그대로 둔다.

## 5. 릴리스

- `Cargo.toml` 0.4.0.
- CHANGELOG `[0.4.0] - unreleased`: Added(`.` 항목, 위치 표시, `dialogs` 예제), Changed(경로 바 잘림 방향), 그리고 "`entries`를 직접 순회하거나 `entries[0]`에 접근하는 앱은 `DirsOnly`에서 `.`을 보게 된다"를 Migration 절에 명시.
- README: `ratatree = "0.4"`, Features와 키 바인딩 표에 `.` 항목, 예제 실행법.
- 검증 세트는 0.3.0과 동일: `cargo test --locked`, `cargo +1.88.0 test --locked`, clippy/rustdoc `-D warnings`, `cargo fmt --check`, `cargo package --locked`. push 후 CI 6/6 확인, 배포는 별도 확인.

## 제외

경로 직접 입력("Go to"), 스크롤바, 정렬 옵션, 새 폴더 만들기, 백엔드 독립 입력(termion/termwiz), 트리 → 리스트 전환 시 커서 폴더로 이동. 세 다이얼로그가 되는 데 필요하지 않다. 트리 전환 동작은 사용자가 현행 유지를 택했다.
