# Playback timing diagnostics

## Delay modes

The playback dialog has a compact **Shorten waits (1 s per operation)** checkbox.
It is enabled by default. Change it before playing or while playback is paused.
The selection is kept when restarting playback in the same app session.

Every click waits one second immediately after releasing the mouse button,
before showing the control window or advancing to another event, in either
mode. After named link clicks, the app additionally compares a small cursor-free
sample with the pre-click image. It requires a visible change and then at least
600 ms of settled drawing; an unchanged old screen cannot pass this check.
After 10 seconds without confirmation, playback pauses for the user to inspect
the target and retry, rather than saving the old screen.

Every screenshot also checks that drawing has settled. The comparison is a
visual heuristic, not a browser network-idle signal: constantly animated content
can require user intervention, and later asynchronous content can still change.
With shortening enabled, recorded inter-operation delays are skipped and
key, text, and scroll operations also wait one second after execution.
Screenshots reuse this completed settling wait instead of adding a second one.
A first or standalone screenshot still waits one second; pointer repositioning
retains its hover settling delay. This also applies to fullscreen and fallback
captures. The countdown bar displays the active settling wait.

When a click is immediately followed by a screenshot, the control window stays
hidden between them, avoiding a show/hide animation in the captured image. It
is restored after the capture, on failure, or at playback completion.

Unchecking restores recorded or edited inter-operation delays and the original
first-operation skip, while retaining the safety wait immediately after clicks.

UI lookup, screenshot processing, and saving still take their actual processing
time; shortening waits does not remove these operations or their metadata.

## Timing logs

On Windows, the compact **Include pointer** checkbox next to the delay option
can override cursor inclusion for recapture. Its initial value comes from the
recording; an explicit choice is kept for the app session. Change it before
playing or while paused. Captured PNGs store each frame's actual cursor mode,
so reopening and replaying them preserves the new choice. Recorded hover
positions are retained even when the pointer itself is not drawn. When enabling
pointer capture on a frame without a recorded position, the current pointer
position is recorded. Window, fullscreen, and fallback captures use the option.

Playback emits timing-only JSON lines to stderr with the `[playback-timing]`
prefix and appends the same records to:

`%LOCALAPPDATA%\markits\logs\playback-timing.jsonl`

Override the file with `MARKITS_PLAYBACK_TIMING_LOG`. Each record includes a run
ID, step number, phase, begin/end event, timestamp and duration in milliseconds.
`delay.countdown` also records the requested delay as `expected_ms` so it can be
compared with actual elapsed time. Input text, window titles, target names and
image contents are not logged.

Useful phases:

| Phase | Measures |
| --- | --- |
| `frontend.ready` | Waiting for the operation list to become ready |
| `control.wait_for_play` | Explicit pause / waiting for Play |
| `window.activate` | Activating the target window |
| `delay.countdown` | The configured inter-operation delay |
| `ui.observe_click_target` | Accessibility lookup before a named click |
| `ui.observe_pointer_target` | Accessibility lookup before pointer positioning |
| `popup.hide` | Hiding the control window and its settling delay |
| `snapshot.read_pixels` | Reading window pixels |
| `screen.wait_for_transition` | Waiting for visible transition / settled drawing |
| `snapshot.encode_png` | PNG encoding and initial data URL generation |
| `snapshot.ui_scan_slot` | Waiting for another UI scan to release its slot |
| `snapshot.ui_scan_result` | Waiting for the captured frame's UI scan |
| `snapshot.embed_ui_metadata` | Normalizing coordinates and embedding metadata |
| `history.save_snapshots` | Persisting results and thumbnails |
| `step.prepare` / `step.execute` / `step.total` | Step totals; nested phases are included |

A `begin` without a matching `end` identifies a currently blocked phase. Nested
phase durations must not be summed with their parent durations.

## Read-only probe

From the repository root:

```powershell
$env:MARKITS_PLAYBACK_TIMING_LOG = Join-Path $pwd 'target/playback-timing.jsonl'
cargo run -p markits-desktop --example playback-timings -- 'C:\path\to\recorded-capture.png'
```

The probe identifies the recording's live target and measures the same UI lookup
and snapshot functions used by playback, twice. It does not activate windows,
send input, save images, or replay the recording. Keep the target open. These
results measure the component costs in the current window state; they are not
an end-to-end replay measurement.
