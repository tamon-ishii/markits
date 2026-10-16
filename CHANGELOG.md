# Changelog

## 0.7.2 - 2026-10-16

### Added

- Windows setup installer registers a per-user startup shortcut and removes it on uninstall. Login startup runs in the tray; normal launches open the editor.

### Fixed

- Fix Windows and macOS compilation for system window enumeration.
- Install Linux CLI build dependencies and build ARM64 on a native runner. Linux CLI archives target GNU libc; musl builds are omitted because capture dependencies require native system libraries.

## 0.7.1 - 2026-10-16

### Added

- Select common pixel resolutions from the crop toolbar to resize the crop selection exactly. Resolutions larger than the source image are disabled.

## 0.7.0 - 2026-10-03

### Added

- Capture a selected display, window, or process directly from the CLI. List available displays and windows before capture.
- Normalize UI element coordinates for high-DPI displays and reuse UIMap data embedded in PNG images.
- Apply annotations in batches, use JSON style templates, and capture a timed series of screenshots.
- Add desktop editor workflows for crop, output sizing, persistent UI metadata, and arrow skin selection.
- Add classic, sketch, and bold arrow skins, plus solid, dashed, and dotted classic shafts.

### Improved

- Place arrow labels using their measured bounds so centered labels clear the shaft and end labels clear the tail.
- Improve spotlight coverage at image edges, arrow sizing and selection, and desktop capture responsiveness.
- Expand CLI, desktop, and AI manual documentation for capture, UIMap, templates, cropping, and arrow labels.
