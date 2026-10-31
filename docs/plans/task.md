| Step | Task | Status |
| --- | --- | --- |
| 1 | Screen Enumeration and Screen Selection in src/capture.rs | Completed |
| 2 | Window Enumeration and Targeted Capture in src/capture.rs | Completed |
| 3 | High-DPI Coordinate Scaling and Scale-Factor Normalization | Completed |
| 4 | CLI Flags & Handlers in src/cli.rs and src/main.rs | Completed |
| 5 | Full Verification and Documentation Update | Completed |
| 6 | Configurable UI exploration depth: settings input, persistence, traversal limits; frontend build and 24 Rust tests passed | Completed |
| 7 | Studio handoff: closing the editor cancels instead of hiding; verify actual desktop cancellation | Complete |
| 8 | Merge Manual Studio integration and Windows warning fixes into upstream MarkIts for submodule use | Completed |
| 9 | Verify 56 library tests, desktop frontend build, crop browser regression, and Manual Studio cargo check | Passed |
| 10 | Apply pending crop before save, copy, and done editing; frontend TypeScript and Vite build passed | Completed |
| 11 | Show playback target image, window, recorded coordinates and failed step; frontend build and 6 playback tests passed | Completed |
| 12 | Display recorded click names, roles, matching mode and target thumbnails; frontend build and 6 playback tests passed | Completed |
| 13 | Filter pointer events by actual target HWND on Windows; stop recording and matching surrounding images, including legacy playback; frontend build, 2 recording tests and 6 playback tests passed | Completed |
| 14 | Remove playback close action, enlarge recording/playback windows to 480x760 and give completed operations a scrollable area; frontend build and cargo check passed | Completed |
| 15 | Replace recording window dropdown with capture overlay starting in recording selection mode; click target to start recording; frontend build and cargo check passed | Completed |
| 16 | Windows cursor capture: select window then use configured key to capture live pixels and current cursor; Escape cancels; native cursor rendering test passed | Completed |
| 17 | Capture hover states by keyboard during recording, hide controls during capture, update thumbnails and exclude capture chord from recording; 4 recording tests passed | Completed |
| 18 | Configure playback wait cap (default 300ms), display all operations before starting and highlight current step; cap regression passed | Completed |
| 19 | Probe hovered controls through Windows UIA, search up to 128 levels and timestamp completed scans; recording name bounds regression passed | Completed |
| 20 | Start recording directly from the edited image's source window; persist window identity through capture, export and reload; identity regression and 34 desktop tests passed | Completed |
| 21 | Rename buttons to record-icon 操作を記録 and 再撮影; frontend build passed | Completed |
| 22 | Present selection overlay before waiting for hidden WebView2 image decoding; reproduced hidden-window symptom, 34 desktop tests and executable build passed; post-fix UI verification denied by automatic app approval | Completed |
| 23 | Unify cursor capture under normal capture: choose window then capture method, remove standalone button, add recording cursor checkbox; frontend build passed | Completed |
| 24 | Persist pointer coordinates, optional UI name and offset for standalone and recorded captures; move pointer before replay snapshot; legacy recording, metadata and hover resolution regressions passed | Completed |
| 25 | Stack home hero buttons and settings vertically to preserve text width; frontend build, 36 desktop tests and 4 recording tests passed | Completed |
| 26 | Restore immediate normal capture; add opt-in cursor mode to bottom toolbar and a floating guide showing the actual capture key; hide guide before capture, support cancel and closing; frontend build, cargo check and 36 desktop tests passed | Completed |
| 27 | Remove home hero; retain capture/open in toolbar and move capture shortcut, exploration depth and playback wait settings into toolbar settings; frontend build and diff checks passed | Completed |
| 28 | Explicitly instruct users to move the pointer and press the displayed capture key again in the cursor guide and selection hint; frontend build passed | Completed |
| 29 | Release crosshair selection during cursor capture: use highlighted target immediately, hide native overlay and disable its mouse input, remove delayed re-presentation and request cursor refresh from the underlying app; frontend build and 36 desktop tests passed | Completed |
| 30 | Preserve cursor capture waiting and retry instead of raising home: tolerate denied foreground activation, reuse/hide guide, retain capture state through failures and cancellation checks, and avoid raising main before image processing; frontend build and 36 desktop tests passed, live app verification unavailable | Completed |
| 31 | Display configured shortcut names in selection instructions; release native/WebView mouse capture on UI thread, return focus after guide creation and refresh mouse movement for hover; frontend build and 36 desktop tests passed, live hover verification unavailable | Completed |
| 32 | Hide toolbar settings in editor view; keep settings available on home; frontend build passed | Completed |
| 33 | Recover cursor capture scan target from embedded source identity, translate/scale UI bounds into image coordinates, and merge async snap results into history without replacing annotations; 38 desktop tests passed including coordinate and history regressions | Completed |
| 34 | Make cursor inclusion an independent checkbox: changing it never chooses a hovered window or arms capture; preserve it when starting operation recording; selection handler regressions passed | Completed |
| 35 | Remove capture-overlay drag selection and immediate hovered-target handoff; click an actual window once, then capture immediately or wait for the configured key; cropping remains in editor | Completed |
| 36 | Keep fullscreen capture with cursor inclusion and recapture metadata, without requiring a single-window identity; fullscreen roundtrip regression passed | Completed |
| 37 | Document complete capture/record/replay/crop/snap contract in capture-workflow.md; frontend build, 7 selection-flow tests and 39 desktop tests passed; live hover verification remains unavailable | Completed |
