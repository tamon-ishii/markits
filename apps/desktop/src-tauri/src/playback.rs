use crate::{history, metadata};
use base64::Engine;
use markits::recording::{OperationRecording, RecordedEvent};
use serde::{Deserialize, Serialize};
use std::{
    io::Cursor,
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};
use tauri::{Emitter, Manager};

pub const OPERATIONS_KEY: &str = "markits:operations";
pub const GROUP_KEY: &str = "markits:operation_group";

pub fn copy_metadata(target: &[u8], source: &[u8]) -> Result<Vec<u8>, metadata::MetadataError> {
    let mut result = target.to_vec();
    if source.starts_with(b"\x89PNG\r\n\x1a\n") {
        for key in [OPERATIONS_KEY, GROUP_KEY] {
            if let Some(value) = metadata::extract_text_chunk(source, key)? {
                result = metadata::embed_text_chunk(&result, key, &value)?;
            }
        }
    }
    Ok(result)
}

#[derive(Clone, Serialize, Deserialize)]
pub struct SavedRecording {
    pub recording: OperationRecording,
    pub window_id: String,
    pub pid: u32,
}

#[derive(Default)]
pub struct PlaybackState(pub AtomicBool, pub AtomicBool, pub Mutex<PlaybackProgress>);

#[derive(Default, Clone, Serialize)]
pub struct PlaybackProgress {
    running: bool,
    step: usize,
    total: usize,
    message: String,
    completed: Vec<String>,
    captures: usize,
    error: Option<String>,
}

#[tauri::command]
pub fn cmd_playback_status(
    state: tauri::State<'_, PlaybackState>,
) -> Result<PlaybackProgress, String> {
    state
        .2
        .lock()
        .map(|value| value.clone())
        .map_err(|e| e.to_string())
}

fn progress(app: &tauri::AppHandle, update: impl FnOnce(&mut PlaybackProgress)) {
    if let Ok(mut value) = app.state::<PlaybackState>().2.lock() {
        update(&mut value);
    }
}

fn describe(event: &RecordedEvent) -> String {
    match event {
        RecordedEvent::Click { x, y, target, .. } => target
            .as_ref()
            .map(|target| format!("クリック：{}", target.name))
            .unwrap_or_else(|| format!("クリック：({x:.0}, {y:.0})")),
        RecordedEvent::Text { value, .. } => {
            format!("文字入力：{}", value.chars().take(60).collect::<String>())
        }
        RecordedEvent::Key { value, .. } => format!("キー：{value}"),
        RecordedEvent::Scroll { dx, dy, .. } => format!("スクロール：横 {dx} / 縦 {dy}"),
        RecordedEvent::Screenshot { .. } => "キャプチャ：現在のウィンドウを新しく撮影".into(),
    }
}

// Hide the progress window while sending pointer input or taking a screenshot,
// so it neither intercepts the click nor appears in the captured pixels.
fn without_popup<T>(
    app: &tauri::AppHandle,
    action: impl FnOnce() -> Result<T, String>,
) -> Result<T, String> {
    let popup = app.get_webview_window("playback");
    if let Some(popup) = &popup {
        popup.hide().map_err(|e| e.to_string())?;
        thread::sleep(Duration::from_millis(80));
    }
    let result = action();
    if let Some(popup) = popup {
        let _ = popup.show();
    }
    result
}

pub fn extract(bytes: &[u8]) -> Result<Option<SavedRecording>, String> {
    if !bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Ok(None);
    }
    // Exported PNGs keep the original image, including its recording metadata.
    let mut bytes = bytes.to_vec();
    for _ in 0..4 {
        if let Some(json) =
            metadata::extract_text_chunk(&bytes, OPERATIONS_KEY).map_err(|e| e.to_string())?
        {
            return serde_json::from_str(&json)
                .map(Some)
                .map_err(|e| e.to_string());
        }
        let Some(source) = metadata::extract_text_chunk(&bytes, metadata::MARKITS_SOURCE_KEYWORD)
            .map_err(|e| e.to_string())?
        else {
            return Ok(None);
        };
        bytes = decode(&source)?;
    }
    Ok(None)
}

fn decode(url: &str) -> Result<Vec<u8>, String> {
    base64::engine::general_purpose::STANDARD
        .decode(url.split_once(',').ok_or("Invalid image data")?.1)
        .map_err(|e| e.to_string())
}

pub fn snapshot(window_id: &str) -> Result<Vec<u8>, String> {
    let pixels = markits::window_capture::read_window_pixels(window_id, None)?;
    let mut png = Cursor::new(Vec::new());
    pixels
        .write_to(&mut png, image::ImageFormat::Png)
        .map_err(|e| e.to_string())?;
    Ok(png.into_inner())
}

pub fn save_snapshots(
    saved: &SavedRecording,
    pictures: Vec<Vec<u8>>,
) -> Result<Vec<metadata::LoadedImageResult>, String> {
    let group = format!(
        "operations_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_nanos()
    );
    let json = serde_json::to_string(saved).map_err(|e| e.to_string())?;
    let mut results = Vec::new();
    for bytes in pictures {
        let bytes =
            metadata::embed_text_chunk(&bytes, OPERATIONS_KEY, &json).map_err(|e| e.to_string())?;
        let bytes =
            metadata::embed_text_chunk(&bytes, GROUP_KEY, &group).map_err(|e| e.to_string())?;
        let item =
            history::save_capture_to_history(&bytes, None, None).map_err(|e| e.to_string())?;
        let mut loaded = metadata::load_image_with_metadata(&bytes).map_err(|e| e.to_string())?;
        loaded.history_id = Some(item.id);
        results.push(loaded);
    }
    Ok(results)
}

#[tauri::command]
pub fn cmd_has_operations(image_data_url: String) -> Result<bool, String> {
    Ok(extract(&decode(&image_data_url)?)?.is_some())
}

#[tauri::command]
pub fn cmd_playback_cancel(state: tauri::State<'_, PlaybackState>) {
    state.1.store(true, Ordering::SeqCst);
}

#[tauri::command]
pub async fn cmd_playback(
    app: tauri::AppHandle,
    image_data_url: String,
) -> Result<Vec<metadata::LoadedImageResult>, String> {
    let saved = extract(&decode(&image_data_url)?)?.ok_or("この画像には操作記録がありません")?;
    app.state::<crate::recording::RecordingState>()
        .claim_playback(&app.state::<PlaybackState>())?;
    app.state::<PlaybackState>()
        .1
        .store(false, Ordering::SeqCst);
    progress(&app, |value| {
        *value = PlaybackProgress {
            running: true,
            total: saved.recording.events.len(),
            message: "対象ウィンドウを確認中…".into(),
            ..Default::default()
        }
    });
    if let Some(old) = app.get_webview_window("playback") {
        let _ = old.destroy();
    }
    let popup = tauri::WebviewWindowBuilder::new(
        &app,
        "playback",
        tauri::WebviewUrl::App("index.html".into()),
    )
    .title("MarkIts — 再生・再撮影")
    .inner_size(420.0, 320.0)
    .always_on_top(true)
    .focused(false)
    .focusable(false)
    .build();
    if let Err(error) = popup {
        app.state::<PlaybackState>()
            .0
            .store(false, Ordering::SeqCst);
        progress(&app, |value| {
            value.running = false;
            value.error = Some(error.to_string());
        });
        return Err(error.to_string());
    }
    let host = app.clone();
    let result = tauri::async_runtime::spawn_blocking(move || replay(&host, saved))
        .await
        .map_err(|e| e.to_string())
        .and_then(|r| r);
    app.state::<PlaybackState>()
        .0
        .store(false, Ordering::SeqCst);
    progress(&app, |value| {
        value.running = false;
        match &result {
            Ok(images) => {
                value.captures = images.len();
                value.message = format!("完了：新しく {} 枚を撮影しました", images.len());
            }
            Err(error) => {
                value.message = "再生を停止しました".into();
                value.error = Some(error.clone());
            }
        }
    });
    if let Some(popup) = app.get_webview_window("playback") {
        let _ = popup.set_focusable(true);
    }
    if let Some(main) = app.get_webview_window("main") {
        crate::commands::force_raise_window(&main);
    }
    result
}

fn replay(
    app: &tauri::AppHandle,
    saved: SavedRecording,
) -> Result<Vec<metadata::LoadedImageResult>, String> {
    if saved.recording.version != 1 {
        return Err("未対応の操作記録バージョンです".into());
    }
    let windows = markits::window_capture::list_windows()?;
    let owners = markits::window_capture::window_process_ids()?;
    let original = windows.iter().find(|w| {
        w.id == saved.window_id
            && owners
                .iter()
                .any(|(id, pid)| id == &w.id && *pid == saved.pid)
    });
    let window = if let Some(window) = original {
        window
    } else {
        let matches: Vec<_> = windows
            .iter()
            .filter(|w| w.title == saved.recording.window)
            .collect();
        if matches.len() != 1 {
            return Err(
                "対象ウインドウを一意に特定できません。対象を開いてから再生してください".into(),
            );
        }
        matches[0]
    };
    // Validate all keyboard events before executing any input.
    for event in &saved.recording.events {
        if let RecordedEvent::Key { value, .. } = event {
            parse_chord(value)?;
        }
    }
    if let Some(main) = app.get_webview_window("main") {
        main.hide().map_err(|e| e.to_string())?;
    }
    markits::window_capture::activate_window(&window.id)?;
    let check = || {
        if app.state::<PlaybackState>().1.load(Ordering::SeqCst) {
            return Err("再生を停止しました".into());
        }
        if !markits::window_capture::is_window_active(&window.id)? {
            return Err("対象ウインドウからフォーカスが移動したため再生を停止しました".into());
        }
        Ok(())
    };
    let mut pictures = Vec::new();
    play_events(
        &saved.recording.events,
        |mut delay, index| {
            progress(app, |value| {
                value.step = index + 1;
                value.message = if delay > 0 {
                    format!(
                        "待機中（{:.1} 秒）：{}",
                        delay as f64 / 1000.0,
                        describe(&saved.recording.events[index])
                    )
                } else {
                    describe(&saved.recording.events[index])
                };
            });
            while delay > 0 {
                check()?;
                let chunk = delay.min(50);
                thread::sleep(Duration::from_millis(chunk));
                delay -= chunk;
            }
            check()?;
            let _ = app.emit("playback-progress", index + 1);
            Ok(())
        },
        |event| {
            progress(app, |value| {
                value.message = format!("実行中：{}", describe(event))
            });
            match event {
                RecordedEvent::Screenshot { .. } => {
                    pictures.push(without_popup(app, || snapshot(&window.id))?)
                }
                RecordedEvent::Click {
                    x,
                    y,
                    target,
                    visual,
                    ..
                } => {
                    let current = markits::window_capture::list_windows()?
                        .into_iter()
                        .find(|w| w.id == window.id)
                        .ok_or("対象ウインドウが閉じられました")?;
                    let point = if let Some(target) = target {
                        let elements = markits::semantic_target::observe_window(&window.id)?;
                        let element = target
                            .resolve(&elements)?
                            .ok_or_else(|| format!("操作対象が見つかりません: {}", target.name))?;
                        (
                            element.x + element.width / 2.0,
                            element.y + element.height / 2.0,
                        )
                    } else if let Some(visual) = visual {
                        let pixels = without_popup(app, || {
                            markits::window_capture::read_window_pixels(&window.id, None)
                        })?;
                        let (x, y) = markits::visual_target::locate(visual, &pixels, check)?
                            .ok_or("操作対象の画像が見つかりません")?;
                        (current.x as f64 + x, current.y as f64 + y)
                    } else {
                        relative_point(*x, *y, &saved.recording, &current)?
                    };
                    check()?;
                    without_popup(app, || {
                        send(rdev::EventType::MouseMove {
                            x: point.0,
                            y: point.1,
                        })?;
                        send(rdev::EventType::ButtonPress(rdev::Button::Left))?;
                        send(rdev::EventType::ButtonRelease(rdev::Button::Left))?;
                        Ok(())
                    })?;
                }
                RecordedEvent::Text { value, .. } => paste(value)?,
                RecordedEvent::Key { value, .. } => chord(&parse_chord(value)?)?,
                RecordedEvent::Scroll { x, y, dx, dy, .. } => {
                    let current = markits::window_capture::list_windows()?
                        .into_iter()
                        .find(|w| w.id == window.id)
                        .ok_or("対象ウインドウが閉じられました")?;
                    let (x, y) = relative_point(*x, *y, &saved.recording, &current)?;
                    without_popup(app, || {
                        send(rdev::EventType::MouseMove { x, y })?;
                        send(rdev::EventType::Wheel {
                            delta_x: *dx,
                            delta_y: *dy,
                        })?;
                        Ok(())
                    })?;
                }
            }
            progress(app, |value| {
                value.completed.push(describe(event));
                if matches!(event, RecordedEvent::Screenshot { .. }) {
                    value.captures += 1;
                }
            });
            Ok(())
        },
    )?;
    if pictures.is_empty() {
        check()?;
        thread::sleep(Duration::from_millis(250));
        progress(app, |value| {
            value.message = "キャプチャ：現在のウィンドウを新しく撮影".into()
        });
        pictures.push(without_popup(app, || snapshot(&window.id))?);
    }
    save_snapshots(&saved, pictures)
}

fn play_events(
    events: &[RecordedEvent],
    mut before: impl FnMut(u64, usize) -> Result<(), String>,
    mut action: impl FnMut(&RecordedEvent) -> Result<(), String>,
) -> Result<(), String> {
    let mut previous = 0;
    for (index, event) in events.iter().enumerate() {
        let elapsed = match event {
            RecordedEvent::Click { elapsed_ms, .. }
            | RecordedEvent::Text { elapsed_ms, .. }
            | RecordedEvent::Key { elapsed_ms, .. }
            | RecordedEvent::Scroll { elapsed_ms, .. }
            | RecordedEvent::Screenshot { elapsed_ms } => *elapsed_ms,
        };
        before(elapsed.saturating_sub(previous), index)?;
        action(event)?;
        previous = elapsed;
    }
    Ok(())
}

fn relative_point(
    x: f64,
    y: f64,
    recording: &OperationRecording,
    window: &markits::window_capture::WindowInfo,
) -> Result<(f64, f64), String> {
    let [left, top, width, height] = recording.bounds;
    if width <= 0
        || height <= 0
        || !x.is_finite()
        || !y.is_finite()
        || x < left as f64
        || y < top as f64
        || x >= (left + width) as f64
        || y >= (top + height) as f64
    {
        return Err("操作座標が対象ウインドウの外にあります".into());
    }
    Ok((
        window.x as f64 + (x - left as f64) * window.width as f64 / width as f64,
        window.y as f64 + (y - top as f64) * window.height as f64 / height as f64,
    ))
}

fn send(event: rdev::EventType) -> Result<(), String> {
    rdev::simulate(&event).map_err(|e| format!("入力を再生できません: {e:?}"))?;
    thread::sleep(Duration::from_millis(15));
    Ok(())
}

fn chord(keys: &[rdev::Key]) -> Result<(), String> {
    let mut pressed = Vec::new();
    let mut result = Ok(());
    for &key in keys {
        if let Err(error) = send(rdev::EventType::KeyPress(key)) {
            result = Err(error);
            break;
        }
        pressed.push(key);
    }
    for key in pressed.into_iter().rev() {
        let release = send(rdev::EventType::KeyRelease(key));
        if result.is_ok() {
            result = release;
        }
    }
    result
}

fn paste(value: &str) -> Result<(), String> {
    let mut clipboard = arboard::Clipboard::new().map_err(|e| e.to_string())?;
    let previous = clipboard.get_text().ok();
    clipboard
        .set_text(value.to_string())
        .map_err(|e| e.to_string())?;
    #[cfg(target_os = "macos")]
    let modifier = rdev::Key::MetaLeft;
    #[cfg(not(target_os = "macos"))]
    let modifier = rdev::Key::ControlLeft;
    let result = chord(&[modifier, rdev::Key::KeyV]);
    thread::sleep(Duration::from_millis(150));
    if let Some(previous) = previous {
        let _ = clipboard.set_text(previous);
    }
    result
}

fn parse_chord(value: &str) -> Result<Vec<rdev::Key>, String> {
    use rdev::Key::*;
    value
        .split('+')
        .map(|part| {
            Ok(match part.to_lowercase().as_str() {
                "control" => ControlLeft,
                "alt" => Alt,
                "meta" => MetaLeft,
                "shift" => ShiftLeft,
                "enter" => Return,
                "escape" => Escape,
                "tab" => Tab,
                "backspace" => Backspace,
                "delete" => Delete,
                "space" => Space,
                "up" => UpArrow,
                "down" => DownArrow,
                "left" => LeftArrow,
                "right" => RightArrow,
                "home" => Home,
                "end" => End,
                "pageup" => PageUp,
                "pagedown" => PageDown,
                "f1" => F1,
                "f2" => F2,
                "f3" => F3,
                "f4" => F4,
                "f5" => F5,
                "f6" => F6,
                "f7" => F7,
                "f8" => F8,
                "f9" => F9,
                "f10" => F10,
                "f11" => F11,
                "f12" => F12,
                "a" => KeyA,
                "b" => KeyB,
                "c" => KeyC,
                "d" => KeyD,
                "e" => KeyE,
                "f" => KeyF,
                "g" => KeyG,
                "h" => KeyH,
                "i" => KeyI,
                "j" => KeyJ,
                "k" => KeyK,
                "l" => KeyL,
                "m" => KeyM,
                "n" => KeyN,
                "o" => KeyO,
                "p" => KeyP,
                "q" => KeyQ,
                "r" => KeyR,
                "s" => KeyS,
                "t" => KeyT,
                "u" => KeyU,
                "v" => KeyV,
                "w" => KeyW,
                "x" => KeyX,
                "y" => KeyY,
                "z" => KeyZ,
                "0" => Num0,
                "1" => Num1,
                "2" => Num2,
                "3" => Num3,
                "4" => Num4,
                "5" => Num5,
                "6" => Num6,
                "7" => Num7,
                "8" => Num8,
                "9" => Num9,
                _ => return Err(format!("未対応のキー操作です: {value}")),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replay_captures_intermediate_states_instead_of_only_the_final_state() {
        let events = vec![
            RecordedEvent::Text {
                elapsed_ms: 10,
                value: "first".into(),
            },
            RecordedEvent::Screenshot { elapsed_ms: 30 },
            RecordedEvent::Text {
                elapsed_ms: 60,
                value: "second".into(),
            },
            RecordedEvent::Screenshot { elapsed_ms: 90 },
        ];
        let mut state = String::new();
        let mut pictures = Vec::new();
        let mut waits = Vec::new();
        play_events(
            &events,
            |delay, _| {
                waits.push(delay);
                Ok(())
            },
            |event| {
                match event {
                    RecordedEvent::Text { value, .. } => state = value.clone(),
                    RecordedEvent::Screenshot { .. } => pictures.push(state.clone()),
                    _ => unreachable!(),
                }
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(pictures, ["first", "second"]);
        assert_eq!(waits, [10, 20, 30, 30]);
    }

    #[test]
    fn replay_stops_at_a_failed_step_without_taking_later_screenshots() {
        let events = vec![
            RecordedEvent::Screenshot { elapsed_ms: 0 },
            RecordedEvent::Screenshot { elapsed_ms: 1 },
        ];
        let mut captures = 0;
        let result = play_events(
            &events,
            |_, index| {
                if index == 1 {
                    Err("cancelled".into())
                } else {
                    Ok(())
                }
            },
            |_| {
                captures += 1;
                Ok(())
            },
        );
        assert!(result.is_err());
        assert_eq!(captures, 1);
    }

    #[test]
    fn recording_survives_export_and_reload_with_multiple_capture_points() {
        let mut png = Cursor::new(Vec::new());
        image::RgbaImage::new(16, 16)
            .write_to(&mut png, image::ImageFormat::Png)
            .unwrap();
        let saved = SavedRecording {
            recording: OperationRecording {
                version: 1,
                window: "Test".into(),
                bounds: [0, 0, 100, 100],
                events: vec![
                    RecordedEvent::Screenshot { elapsed_ms: 10 },
                    RecordedEvent::Screenshot { elapsed_ms: 20 },
                ],
            },
            window_id: "0x123".into(),
            pid: 123,
        };
        let source = metadata::embed_text_chunk(
            &png.get_ref(),
            OPERATIONS_KEY,
            &serde_json::to_string(&saved).unwrap(),
        )
        .unwrap();
        let url = format!(
            "data:image/png;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(source)
        );
        let exported =
            metadata::embed_text_chunk(&png.get_ref(), metadata::MARKITS_SOURCE_KEYWORD, &url)
                .unwrap();
        let restored = extract(&exported).unwrap().unwrap();
        assert_eq!(restored.recording.events.len(), 2);
        assert_eq!(restored.window_id, "0x123");
        assert!(extract(&png.get_ref()).unwrap().is_none());
    }

    #[test]
    fn multiple_captures_have_distinct_history_items_and_one_thumbnail_group() {
        let mut png = Cursor::new(Vec::new());
        image::RgbaImage::new(16, 16)
            .write_to(&mut png, image::ImageFormat::Png)
            .unwrap();
        let saved = SavedRecording {
            recording: OperationRecording {
                version: 1,
                window: "Test capture batch".into(),
                bounds: [0, 0, 16, 16],
                events: vec![
                    RecordedEvent::Screenshot { elapsed_ms: 0 },
                    RecordedEvent::Screenshot { elapsed_ms: 10 },
                ],
            },
            window_id: "0x123".into(),
            pid: 123,
        };
        let images =
            save_snapshots(&saved, vec![png.get_ref().clone(), png.get_ref().clone()]).unwrap();
        assert_eq!(images.len(), 2);
        assert_ne!(images[0].history_id, images[1].history_id);
        let groups: Vec<_> = images
            .iter()
            .map(|image| {
                let loaded =
                    history::load_history_item(image.history_id.as_ref().unwrap()).unwrap();
                let bytes = decode(&loaded.image_data_url).unwrap();
                assert_eq!(extract(&bytes).unwrap().unwrap().recording.events.len(), 2);
                metadata::extract_text_chunk(&bytes, GROUP_KEY)
                    .unwrap()
                    .unwrap()
            })
            .collect();
        for image in &images {
            history::delete_history_item(image.history_id.as_ref().unwrap()).unwrap();
        }
        assert_eq!(groups[0], groups[1]);
    }

    #[test]
    fn cropped_export_reloads_the_recording_with_the_editable_background() {
        let mut png = Cursor::new(Vec::new());
        image::RgbaImage::new(16, 16)
            .write_to(&mut png, image::ImageFormat::Png)
            .unwrap();
        let saved = SavedRecording {
            recording: OperationRecording {
                version: 1,
                window: "Test".into(),
                bounds: [0, 0, 16, 16],
                events: vec![RecordedEvent::Screenshot { elapsed_ms: 10 }],
            },
            window_id: "0x123".into(),
            pid: 123,
        };
        let original = metadata::embed_text_chunk(
            png.get_ref(),
            OPERATIONS_KEY,
            &serde_json::to_string(&saved).unwrap(),
        )
        .unwrap();
        let cropped_url = format!(
            "data:image/png;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(png.get_ref())
        );
        let base_url = format!(
            "data:image/png;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(original)
        );
        let path = std::env::temp_dir().join(format!(
            "markits-recording-export-{}.png",
            std::process::id()
        ));
        crate::commands::cmd_compose_and_save(
            cropped_url,
            r#"{"canvas":{"width":16,"height":16},"annotations":[]}"#.into(),
            path.to_string_lossy().into(),
            None,
            None,
            None,
            false,
            Some(base_url),
            None,
        )
        .unwrap();
        let loaded = crate::commands::cmd_load_image(path.to_string_lossy().into()).unwrap();
        std::fs::remove_file(path).unwrap();
        assert_eq!(
            extract(&decode(&loaded.image_data_url).unwrap())
                .unwrap()
                .unwrap()
                .recording
                .events
                .len(),
            1
        );
    }

    #[test]
    fn moved_window_coordinates_and_keyboard_shortcuts_are_resolved() {
        let recording = OperationRecording {
            version: 1,
            window: "Test".into(),
            bounds: [100, 100, 200, 100],
            events: vec![],
        };
        let window = markits::window_capture::WindowInfo {
            id: "1".into(),
            title: "Test".into(),
            x: 300,
            y: 400,
            width: 400,
            height: 200,
        };
        assert_eq!(
            relative_point(150., 125., &recording, &window).unwrap(),
            (400., 450.)
        );
        assert!(relative_point(50., 125., &recording, &window).is_err());
        assert_eq!(
            parse_chord("Control+Shift+S").unwrap(),
            vec![
                rdev::Key::ControlLeft,
                rdev::Key::ShiftLeft,
                rdev::Key::KeyS
            ]
        );
        assert!(parse_chord("Control+Unsupported").is_err());
    }
}
