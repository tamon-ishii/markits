# Window operation recording

In Markits Desktop choose **ウインドウ操作を記録**, select an existing application window, and choose **記録を開始**. The editor hides and a floating control window displays recorded input. Use **一時停止 / 再開**, **終了してJSONを保存**, or **取消**. Closing the control window cancels the recording. The target application remains open.

Ctrl+Shift+F9 toggles pause. Ctrl+Shift+F10 stops the input helper; use the control window to save the resulting recording. Cancelling the save dialog retains the completed recording until you save again or cancel.

The JSON contains `version: 1`, the window title, initial desktop `bounds: [x,y,width,height]`, and `events`. Events use the same tagged format as Manual Studio's input recorder: `kind` is `click`, `text`, `key` or `scroll`; `elapsed_ms` is time since recording started. Clicks and scrolls contain desktop coordinates. Clicks can include semantic `target` and visual `visual` metadata. This is operation data rather than a video file. Pointer actions outside the selected window or over the floating controls are excluded.

Manual Studio uses the same Markits input helper and target types. Its existing launch profiles, scenario recipes, screenshot library registration and annotation handoff remain available. Standalone recordings do not require a Manual Studio project. Native input monitoring requires the relevant OS permissions; Linux recording currently requires X11.
