# Window Capture, Multi-Screen Selection, and High-DPI Scaling Implementation Plan

> **For Antigravity:** REQUIRED WORKFLOW: Use `.agent/workflows/execute-plan.md` to execute this plan in single-flow mode.

**Goal:** Enable MarkIts to capture specific windows or processes directly, select from multiple monitors, and automatically normalize High-DPI coordinate scaling for precision annotation.

**Architecture:** Extend `src/capture.rs` with `ScreenInfo`, `WindowInfo`, and `WindowQuery` APIs backed by `screenshots::Screen` and `screenshots::Window`, with window boundary screen-crop fallback; normalize logical UI element bounds by screen `scale_factor` and embed `markits:scale_factor`; expose discovery and capture flags in CLI (`src/cli.rs` and `src/main.rs`).

**Tech Stack:** Rust, `screenshots` crate, `image` crate, `clap`, `serde`, `serde_json`.

---

### Task 1: Screen Enumeration and Screen Selection in `src/capture.rs`

**Files:**
- Modify: `src/capture.rs`
- Test: `src/capture.rs` (unit tests)

**Step 1: Write the failing unit tests for screen enumeration and screen capture**

```rust
#[test]
fn test_list_screens_returns_valid_info() {
    let screens = list_screens().expect("list_screens should succeed");
    assert!(!screens.is_empty(), "should detect at least one screen");
    let primary = screens.iter().find(|s| s.is_primary).or(screens.first()).unwrap();
    assert!(primary.width > 0);
    assert!(primary.height > 0);
    assert!(primary.scale_factor > 0.0);
}

#[test]
fn test_capture_screen_out_of_bounds_fails() {
    let result = capture_screen(9999);
    assert!(result.is_err());
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test --lib capture::tests::test_list_screens`
Expected: FAIL (functions not defined)

**Step 3: Implement `ScreenInfo`, `list_screens`, and `capture_screen`**

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ScreenInfo {
    pub index: usize,
    pub name: String,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub scale_factor: f64,
    pub is_primary: bool,
}

pub fn list_screens() -> Result<Vec<ScreenInfo>, CaptureError> {
    let screens = Screen::all().map_err(|e| CaptureError::CaptureFailed(e.to_string()))?;
    if screens.is_empty() {
        return Err(CaptureError::NoScreensFound);
    }
    let mut list = Vec::new();
    for (i, s) in screens.iter().enumerate() {
        list.push(ScreenInfo {
            index: i,
            name: format!("Screen {}", i),
            x: s.display_info.x,
            y: s.display_info.y,
            width: s.display_info.width,
            height: s.display_info.height,
            scale_factor: s.display_info.scale_factor as f64,
            is_primary: s.display_info.is_primary,
        });
    }
    Ok(list)
}

pub fn capture_screen(screen_index: usize) -> Result<CapturedImage, CaptureError> {
    let screens = Screen::all().map_err(|e| CaptureError::CaptureFailed(e.to_string()))?;
    let screen = screens.get(screen_index).ok_or_else(|| {
        CaptureError::CaptureFailed(format!("Screen index {} not found (total screens: {})", screen_index, screens.len()))
    })?;

    let raw_sc = screen.capture().map_err(|e| CaptureError::CaptureFailed(e.to_string()))?;
    let width = raw_sc.width();
    let height = raw_sc.height();
    let raw_bytes = raw_sc.into_raw();

    let image = RgbaImage::from_raw(width, height, raw_bytes)
        .ok_or_else(|| CaptureError::CaptureFailed("Failed to construct image from screen buffer".to_string()))?;

    rgba_to_captured_image(&image)
}
```

**Step 4: Run test to verify it passes**

Run: `cargo test --lib capture::tests::test_list_screens`
Expected: PASS

**Step 5: Commit**

```bash
git add src/capture.rs
git commit -m "feat(capture): add screen enumeration and index-based capture"
```

---

### Task 2: Window Enumeration and Targeted Capture in `src/capture.rs`

**Files:**
- Modify: `src/capture.rs`
- Test: `src/capture.rs` (unit tests)

**Step 1: Write the failing unit tests for window query and matching**

```rust
#[test]
fn test_window_query_matching() {
    let win = WindowInfo {
        id: 101,
        pid: Some(1234),
        title: "Mozilla Firefox - MarkIts".to_string(),
        app_name: "Firefox".to_string(),
        x: 100,
        y: 100,
        width: 800,
        height: 600,
        is_minimized: false,
    };

    assert!(win.matches_query(&WindowQuery::TitleOrId("firefox".to_string())));
    assert!(win.matches_query(&WindowQuery::TitleOrId("101".to_string())));
    assert!(!win.matches_query(&WindowQuery::TitleOrId("Chrome".to_string())));
    assert!(win.matches_query(&WindowQuery::Pid(1234)));
    assert!(!win.matches_query(&WindowQuery::Pid(9999)));
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test --lib capture::tests::test_window_query_matching`
Expected: FAIL (methods not defined)

**Step 3: Implement `WindowInfo`, `WindowQuery`, `list_windows`, and `capture_window_by_query`**

```rust
use screenshots::Window;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WindowInfo {
    pub id: u32,
    pub pid: Option<u32>,
    pub title: String,
    pub app_name: String,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub is_minimized: bool,
}

impl WindowInfo {
    pub fn matches_query(&self, query: &WindowQuery) -> bool {
        match query {
            WindowQuery::TitleOrId(q) => {
                if let Ok(id) = q.trim().parse::<u32>() {
                    if self.id == id {
                        return true;
                    }
                }
                let q_lower = q.to_lowercase();
                self.title.to_lowercase().contains(&q_lower) || self.app_name.to_lowercase().contains(&q_lower)
            }
            WindowQuery::Pid(target_pid) => self.pid == Some(*target_pid),
        }
    }
}

#[derive(Debug, Clone)]
pub enum WindowQuery {
    TitleOrId(String),
    Pid(u32),
}

pub fn list_windows() -> Result<Vec<WindowInfo>, CaptureError> {
    let windows = Window::all().map_err(|e| CaptureError::CaptureFailed(e.to_string()))?;
    let mut list = Vec::new();
    for w in windows {
        let title = w.title().unwrap_or_default();
        let app_name = w.app_name().unwrap_or_default();
        if title.is_empty() && app_name.is_empty() {
            continue;
        }
        list.push(WindowInfo {
            id: w.id(),
            pid: None,
            title,
            app_name,
            x: w.x(),
            y: w.y(),
            width: w.width(),
            height: w.height(),
            is_minimized: w.is_minimized(),
        });
    }
    Ok(list)
}

pub fn capture_window_by_query(query: &WindowQuery) -> Result<CapturedImage, CaptureError> {
    let windows = Window::all().map_err(|e| CaptureError::CaptureFailed(e.to_string()))?;
    let matched_window = windows.into_iter().find(|w| {
        let title = w.title().unwrap_or_default();
        let app_name = w.app_name().unwrap_or_default();
        match query {
            WindowQuery::TitleOrId(q) => {
                if let Ok(id) = q.trim().parse::<u32>() {
                    if w.id() == id { return true; }
                }
                let q_lower = q.to_lowercase();
                title.to_lowercase().contains(&q_lower) || app_name.to_lowercase().contains(&q_lower)
            }
            WindowQuery::Pid(_p) => false,
        }
    }).ok_or_else(|| CaptureError::CaptureFailed(format!("No window matching query: {:?}", query)))?;

    // Attempt direct window buffer capture
    if let Ok(raw_sc) = matched_window.capture() {
        let width = raw_sc.width();
        let height = raw_sc.height();
        let raw_bytes = raw_sc.into_raw();
        if let Some(image) = RgbaImage::from_raw(width, height, raw_bytes) {
            return rgba_to_captured_image(&image);
        }
    }

    // Fallback: Locate screen and crop window rect
    let wx = matched_window.x().max(0) as u32;
    let wy = matched_window.y().max(0) as u32;
    let ww = matched_window.width();
    let wh = matched_window.height();
    capture_region(wx, wy, ww, wh)
}
```

**Step 4: Run test to verify it passes**

Run: `cargo test --lib capture::tests::test_window_query_matching`
Expected: PASS

**Step 5: Commit**

```bash
git add src/capture.rs
git commit -m "feat(capture): add window enumeration and query-based capture"
```

---

### Task 3: High-DPI Coordinate Scaling and Scale-Factor Normalization

**Files:**
- Modify: `src/capture.rs`
- Modify: `src/ui_elements.rs`
- Test: `src/capture.rs` / `src/ui_elements.rs`

**Step 1: Write failing unit test for DPI scaling of UI elements**

```rust
#[test]
fn test_scale_ui_elements_for_dpi() {
    let mut elements = vec![
        DetectedUiElement {
            role: "button".into(),
            name: Some("Submit".into()),
            window_id: None,
            pid: None,
            x: 100.0,
            y: 50.0,
            width: 80.0,
            height: 30.0,
        }
    ];

    scale_ui_elements(&mut elements, 2.0, 0, 0);
    assert_eq!(elements[0].x, 200.0);
    assert_eq!(elements[0].y, 100.0);
    assert_eq!(elements[0].width, 160.0);
    assert_eq!(elements[0].height, 60.0);
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test --lib ui_elements::tests::test_scale_ui_elements_for_dpi`
Expected: FAIL (`scale_ui_elements` undefined)

**Step 3: Implement `scale_ui_elements` and add `scale_factor` to `CapturedImage`**

```rust
pub fn scale_ui_elements(
    elements: &mut [DetectedUiElement],
    scale_factor: f64,
    origin_x: i32,
    origin_y: i32,
) {
    if (scale_factor - 1.0).abs() < 1e-4 && origin_x == 0 && origin_y == 0 {
        return;
    }
    for el in elements.iter_mut() {
        el.x = ((el.x - origin_x as f64) * scale_factor).round();
        el.y = ((el.y - origin_y as f64) * scale_factor).round();
        el.width = (el.width * scale_factor).round();
        el.height = (el.height * scale_factor).round();
    }
}
```

**Step 4: Run test to verify it passes**

Run: `cargo test --lib ui_elements::tests::test_scale_ui_elements_for_dpi`
Expected: PASS

**Step 5: Commit**

```bash
git add src/ui_elements.rs src/capture.rs
git commit -m "feat(ui_elements): add DPI coordinate scaling and normalization"
```

---

### Task 4: CLI Flags & Handlers in `src/cli.rs` and `src/main.rs`

**Files:**
- Modify: `src/cli.rs`
- Modify: `src/main.rs`
- Test: `tests/cli_capture_test.rs`

**Step 1: Write failing CLI integration test for `--list-screens` and `--list-windows`**

```rust
#[test]
fn test_cli_list_screens_and_windows() {
    let mut cmd = Command::cargo_bin("markits").unwrap();
    cmd.args(["capture", "--list-screens", "--json"]);
    cmd.assert().success();

    let mut cmd2 = Command::cargo_bin("markits").unwrap();
    cmd2.args(["capture", "--list-windows", "--json"]);
    cmd2.assert().success();
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test --test cli_capture_test test_cli_list_screens_and_windows`
Expected: FAIL (unrecognized arguments)

**Step 3: Add CLI options in `src/cli.rs` and handlers in `src/main.rs`**

In `src/cli.rs`:
```rust
#[derive(clap::Args, Debug)]
pub struct CaptureArgs {
    // Existing fields...

    /// List all connected screens and exit
    #[arg(long)]
    pub list_screens: bool,

    /// List all capturable top-level windows and exit
    #[arg(long)]
    pub list_windows: bool,

    /// Screen/Monitor index to capture (0-indexed)
    #[arg(long)]
    pub screen: Option<usize>,

    /// Target window title substring or ID to capture directly
    #[arg(long)]
    pub window: Option<String>,

    /// Target process ID (PID) to capture directly
    #[arg(long)]
    pub pid: Option<u32>,

    /// Output listings in JSON format
    #[arg(long)]
    pub json: bool,
}
```

In `src/main.rs`:
Handle `--list-screens`, `--list-windows`, `--screen`, `--window`, and `--pid` in `run_capture`.

**Step 4: Run test to verify it passes**

Run: `cargo test --test cli_capture_test test_cli_list_screens_and_windows`
Expected: PASS

**Step 5: Commit**

```bash
git add src/cli.rs src/main.rs tests/cli_capture_test.rs
git commit -m "feat(cli): add --list-screens, --list-windows, --screen, --window, and --pid flags"
```

---

### Task 5: Full Verification and Documentation Update

**Files:**
- Modify: `README.md`
- Run: `cargo test --workspace`
- Run: `npm run build` in `apps/desktop`

**Step 1: Update README.md with CLI options and examples**

Add `--list-screens`, `--list-windows`, `--screen`, `--window`, and `--pid` to `README.md`.

**Step 2: Run full workspace test suite**

Run: `cargo test --workspace`
Expected: ALL PASS

**Step 3: Run desktop build verification**

Run: `npm run build` in `apps/desktop`
Expected: PASS

**Step 4: Commit**

```bash
git add README.md docs/plans/task.md
git commit -m "docs: document window capture, multi-screen, and DPI scaling features"
```
