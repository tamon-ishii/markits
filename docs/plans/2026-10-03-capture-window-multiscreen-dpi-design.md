# Design: Window Capture, Multi-Screen Selection, and High-DPI Scaling

## 1. Overview & Context

When integrating MarkIts with desktop automation engines and automated documentation builders (`ModuleLoom`, `manual-core`, `desktop_scenario`), direct control over which screen or window is captured is essential.

Currently, MarkIts defaults to capturing the primary screen and lacks:
1. Native window/process-targeted capture without manual boundary computation.
2. Multi-monitor enumeration and display selection (`--screen <INDEX>`).
3. High-DPI / scale-factor normalization between physical image pixels and logical accessibility tree points.

This document specifies the design for adding these three capabilities to the MarkIts core library and CLI.

---

## 2. Requirements & Goals

### 2.1 Functional Requirements
- **Screen Enumeration (`--list-screens`)**:
  - List all connected displays with screen index, device name, geometry `(x, y, width, height)`, scale factor, and primary display flag.
  - Support human-readable terminal output and machine-readable `--json` output.
- **Window Enumeration (`--list-windows`)**:
  - List capturable top-level desktop windows with window ID, process ID (PID), window title, owning application name, and geometry.
  - Support human-readable terminal output and machine-readable `--json` output.
- **Targeted Screen Capture (`--screen <INDEX>`)**:
  - Capture a specified monitor by 0-indexed integer (e.g. `--screen 0`, `--screen 1`) or `'primary'`.
- **Targeted Window Capture (`--window <QUERY>` / `--pid <PID>`)**:
  - Capture a window directly by title substring (case-insensitive) or window ID.
  - Capture the primary window associated with a given operating system PID.
  - Filter and offset detected UI elements to align with the captured window's local coordinate space `(0, 0)`.
- **High-DPI / Scale Factor Normalization**:
  - Retrieve the target display's `scale_factor` (e.g. `1.0`, `1.25`, `1.5`, `2.0`).
  - Scale logical accessibility bounding boxes (`x`, `y`, `width`, `height`) by `scale_factor` so that annotations align with physical pixel positions in the screenshot.
  - Embed `markits:scale_factor` into the PNG metadata tEXt chunks.

---

## 3. Data Structures & Core API (`src/capture.rs`)

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

#[derive(Debug, Clone)]
pub enum WindowQuery {
    TitleOrId(String),
    Pid(u32),
}

/// List all connected screens.
pub fn list_screens() -> Result<Vec<ScreenInfo>, CaptureError>;

/// List all visible, capturable windows.
pub fn list_windows() -> Result<Vec<WindowInfo>, CaptureError>;

/// Capture a specific screen by index.
pub fn capture_screen(screen_index: usize) -> Result<CapturedImage, CaptureError>;

/// Capture a window by query (title substring, ID, or PID).
/// Prioritizes native window buffer capture; falls back to locating the window on its
/// display and cropping the screen buffer if native capture fails.
pub fn capture_window_by_query(query: &WindowQuery) -> Result<CapturedImage, CaptureError>;
```

---

## 4. High-DPI Scaling & Coordinate Alignment

### 4.1 Problem
On high-DPI displays (such as 4K or Retina displays with 150% or 200% scaling):
- Screenshot image buffer: Physical pixels (e.g. $3840 \times 2160$).
- OS Accessibility API (AT-SPI on Linux, Accessibility APIs on macOS/Windows): Logical points (e.g. $1920 \times 1080$).

### 4.2 Normalization Algorithm
When capturing a screen or window:
1. Determine `scale_factor = screen.display_info.scale_factor` (default `1.0` if unavailable or invalid).
2. For each detected UI element:
   $$\text{elem.x} = (\text{logical\_x} - \text{origin\_x}) \times \text{scale\_factor}$$
   $$\text{elem.y} = (\text{logical\_y} - \text{origin\_y}) \times \text{scale\_factor}$$
   $$\text{elem.width} = \text{logical\_width} \times \text{scale\_factor}$$
   $$\text{elem.height} = \text{logical\_height} \times \text{scale\_factor}$$
3. Embed `markits:scale_factor` into the PNG metadata tEXt chunk.

---

## 5. CLI Interface Specifications

### 5.1 Discovery Commands
```bash
# Human readable listing
markits capture --list-screens
markits capture --list-windows

# Machine readable JSON output for automation
markits capture --list-screens --json
markits capture --list-windows --json
```

### 5.2 Capture Commands
```bash
# Capture secondary monitor
markits capture --screen 1 -o screen1.png

# Capture specific window by title (case-insensitive substring)
markits capture --window "Firefox" -o firefox.png

# Capture window by PID
markits capture --pid 12345 -o process_window.png

# Combined with auto-crop and annotations
markits capture --window "Firefox" -i "検索バーをクリック" --crop --crop-margin 32 -o annotated.png
```

---

## 6. Testing Strategy

1. **Unit Tests**:
   - `list_screens()` returns at least one screen with valid dimensions and non-zero scale factor.
   - `WindowQuery` matching algorithms (case-insensitive title matching, substring matching, numeric ID parsing).
   - High-DPI scaling calculations with fractional scaling factors (`1.0`, `1.25`, `1.5`, `2.0`).
2. **Integration / CLI Tests**:
   - `markits capture --list-screens` and `markits capture --list-screens --json`.
   - `markits capture --list-windows` and `markits capture --list-windows --json`.
   - Screen index selection bounds checking (e.g. `--screen 999` returns clean error).
3. **Workspace Verification**:
   - `cargo test --workspace` passes cleanly.
