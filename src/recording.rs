//! Shared window operation recording engine.
#[cfg(target_os = "windows")]
mod windows_target;
use rdev::{Button, Event, EventType, Key, listen};
use serde::{Deserialize, Serialize};
use std::{
    fs::File,
    io::{BufWriter, Write},
    path::Path,
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RecordedEvent {
    Screenshot {
        elapsed_ms: u64,
        #[serde(default)]
        include_cursor: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pointer: Option<RecordedPointer>,
    },
    Click {
        elapsed_ms: u64,
        x: f64,
        y: f64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        target: Option<crate::semantic_target::Target>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        visual: Option<crate::visual_target::Target>,
    },
    Text {
        elapsed_ms: u64,
        value: String,
    },
    Key {
        elapsed_ms: u64,
        value: String,
    },
    Scroll {
        elapsed_ms: u64,
        x: f64,
        y: f64,
        dx: i64,
        dy: i64,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordedPointer {
    pub x: f64,
    pub y: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<crate::semantic_target::Target>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_offset: Option<[f64; 2]>,
}

#[cfg(target_os = "windows")]
pub fn named_target_at_point(x: i32, y: i32) -> Option<(crate::semantic_target::Target, [f64; 4])> {
    windows_target::probe_point(x, y)
}

pub fn run_helper(path: &Path) -> Result<(), String> {
    let pause_file = path.with_extension("pause");
    let capture_shortcut = std::env::var("MARKITS_RECORDING_CAPTURE_SHORTCUT")
        .unwrap_or_else(|_| "PrintScreen".into());
    let file = File::create(path).map_err(|error| error.to_string())?;
    let output = Arc::new(Mutex::new(BufWriter::new(file)));
    #[cfg(target_os = "linux")]
    let point_context = std::env::var("MUNIN_RECORDING_WINDOW")
        .ok()
        .and_then(|context| {
            serde_json::from_str::<crate::ui_elements::DetectedUiElement>(&context).ok()
        });
    let visual_window = std::env::var("MARKITS_RECORDING_WINDOW_ID")
        .ok()
        .or_else(|| {
            std::env::var("MUNIN_RECORDING_WINDOW")
                .ok()
                .and_then(|context| {
                    serde_json::from_str::<crate::ui_elements::DetectedUiElement>(&context).ok()
                })
                .and_then(|context| context.window_id)
                .and_then(|id| {
                    #[cfg(target_os = "linux")]
                    {
                        id.parse::<u64>().ok().map(|id| format!("0x{id:x}"))
                    }
                    #[cfg(not(target_os = "linux"))]
                    {
                        Some(id)
                    }
                })
        });
    let semantic_window = visual_window.clone();
    let target_window = visual_window;
    let target_only = std::env::var_os("MARKITS_RECORDING_TARGET_ONLY").is_some();
    #[cfg(target_os = "windows")]
    let point_target = windows_target::start(target_window.clone());
    let snapshot = Arc::new(Mutex::new((Instant::now(), Vec::new())));
    if let Some(id) = semantic_window {
        let snapshot = Arc::clone(&snapshot);
        let depth = std::env::var("MUNIN_UI_CAPTURE_DEPTH")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(128).clamp(1, 128);
        thread::spawn(move || {
            let _depth = crate::ui_elements::capture_depth_scope(depth);
            loop {
                let elements = crate::semantic_target::observe_window(&id).unwrap_or_default();
                if let Ok(mut cache) = snapshot.lock() {
                    *cache = (Instant::now(), elements);
                }
                thread::sleep(Duration::from_millis(200));
            }
        });
    }
    let started = Instant::now();
    let mut ctrl = false;
    let mut shift = false;
    let mut alt = false;
    let mut meta = false;
    let mut pointer = (0.0, 0.0);
    let writer = Arc::clone(&output);
    let callback = move |event: Event| {
        let elapsed_ms = started.elapsed().as_millis().min(u64::MAX as u128) as u64;
        let key_state = match event.event_type {
            EventType::KeyPress(key) => Some((key, true)),
            EventType::KeyRelease(key) => Some((key, false)),
            _ => None,
        };
        if let Some((key, down)) = key_state {
            match key {
                Key::ControlLeft | Key::ControlRight => ctrl = down,
                Key::ShiftLeft | Key::ShiftRight => shift = down,
                Key::Alt | Key::AltGr => alt = down,
                Key::MetaLeft | Key::MetaRight => meta = down,
                _ => {}
            }
            if down && ctrl && shift && key == Key::F10 {
                std::process::exit(0);
            }
            if down && ctrl && shift && key == Key::F9 {
                if pause_file.exists() {
                    let _ = std::fs::remove_file(&pause_file);
                } else {
                    let _ = std::fs::write(&pause_file, "paused");
                }
                return;
            }
            if is_capture_shortcut(&capture_shortcut, key, ctrl, shift, alt, meta) {
                return;
            }
            if !down || pause_file.exists() || is_modifier(key) {
                return;
            }
            if target_only
                && !target_window
                    .as_ref()
                    .is_some_and(|id| crate::window_capture::is_window_active(id).unwrap_or(false))
            {
                return;
            }
            let chord = [
                if ctrl { Some("Control") } else { None },
                if alt { Some("Alt") } else { None },
                if meta { Some("Meta") } else { None },
                if shift && (ctrl || alt || meta) { Some("Shift") } else { None },
            ]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();
            let special = key_name(key);
            let value = if let Some(name) = special {
                Some(if chord.is_empty() {
                    name.to_string()
                } else {
                    format!("{}+{}", chord.join("+"), name)
                })
            } else {
                event
                    .name
                    .as_deref()
                    .filter(|name| !name.is_empty() && name.chars().all(|c| !c.is_control()))
                    .map(|name| {
                        if chord.is_empty() {
                            name.to_string()
                        } else {
                            format!("{}+{}", chord.join("+"), name)
                        }
                    })
            };
            if let Some(value) = value {
                let event = if special.is_none() && chord.is_empty() {
                    RecordedEvent::Text { elapsed_ms, value }
                } else {
                    RecordedEvent::Key { elapsed_ms, value }
                };
                write_event(&writer, event);
            }
            return;
        }
        if let EventType::MouseMove { x, y } = event.event_type {
            pointer = (x, y);
            return;
        }
        if pause_file.exists() {
            return;
        }
        if target_only {
            let inside = target_window.as_ref().is_some_and(|id| {
                crate::window_capture::pointer_targets_window(id, pointer.0, pointer.1).unwrap_or(false)
            });
            if !inside {
                return;
            }
        }
        let recorded = match event.event_type {
            EventType::MouseMove { x, y } => {
                pointer = (x, y);
                None
            }
            EventType::ButtonPress(Button::Left) => Some(RecordedEvent::Click {
                elapsed_ms,
                x: pointer.0,
                y: pointer.1,
                visual: None,
                target: {
                    #[cfg(target_os = "windows")]
                    let direct = point_target.lock().ok().and_then(|cache| {
                        let (observed_at, probe) = cache.as_ref()?;
                        (observed_at.elapsed() < Duration::from_millis(800)
                            && probe.contains(pointer.0, pointer.1))
                            .then(|| probe.target.clone())
                    });
                    let cached = snapshot.lock().ok().and_then(|cache| {
                        (cache.0.elapsed() < Duration::from_millis(2200))
                            .then(|| {
                                crate::semantic_target::at_point(&cache.1, pointer.0, pointer.1)
                            })
                            .flatten()
                    });
                    #[cfg(target_os = "windows")]
                    let cached = direct.or(cached);
                    #[cfg(target_os = "linux")]
                    let cached = cached.or_else(|| {
                        point_context.as_ref().and_then(|context| {
                            let element = crate::ui_elements::desktop_target_at_point(
                                context,
                                pointer.0 as i32,
                                pointer.1 as i32,
                            )?;
                            Some(crate::semantic_target::Target {
                                role: element.role,
                                name: element.name?,
                                reveals: None,
                            })
                        })
                    });
                    cached
                },
            }),
            EventType::Wheel { delta_x, delta_y } => Some(RecordedEvent::Scroll {
                elapsed_ms,
                x: pointer.0,
                y: pointer.1,
                dx: delta_x,
                dy: delta_y,
            }),
            _ => None,
        };
        if let Some(event) = recorded {
            write_event(&writer, event);
        }
    };
    listen(callback).map_err(|error| format!("入力記録を開始できませんでした: {error:?}"))
}

fn write_event(writer: &Arc<Mutex<BufWriter<File>>>, event: RecordedEvent) {
    if let Ok(mut writer) = writer.lock() {
        if serde_json::to_writer(&mut *writer, &event).is_ok() {
            let _ = writer.write_all(b"\n");
            let _ = writer.flush();
        }
    }
}

fn is_modifier(key: Key) -> bool {
    matches!(
        key,
        Key::ControlLeft
            | Key::ControlRight
            | Key::ShiftLeft
            | Key::ShiftRight
            | Key::Alt
            | Key::AltGr
            | Key::MetaLeft
            | Key::MetaRight
    )
}

fn is_capture_shortcut(shortcut: &str, key: Key, ctrl: bool, shift: bool, alt: bool, meta: bool) -> bool {
    match shortcut {
        "PrintScreen" => key == Key::PrintScreen && !ctrl && !shift && !alt && !meta,
        "Alt+PrintScreen" => key == Key::PrintScreen && alt && !ctrl && !shift && !meta,
        "Control+Shift+S" => key == Key::KeyS && ctrl && shift && !alt && !meta,
        _ => false,
    }
}

fn key_name(key: Key) -> Option<&'static str> {
    Some(match key {
        Key::Return | Key::KpReturn => "Enter",
        Key::Escape => "Escape",
        Key::Tab => "Tab",
        Key::Backspace => "Backspace",
        Key::Delete => "Delete",
        Key::Space => "Space",
        Key::UpArrow => "Up",
        Key::DownArrow => "Down",
        Key::LeftArrow => "Left",
        Key::RightArrow => "Right",
        Key::Home => "Home",
        Key::End => "End",
        Key::PageUp => "PageUp",
        Key::PageDown => "PageDown",
        Key::F1 => "F1",
        Key::F2 => "F2",
        Key::F3 => "F3",
        Key::F4 => "F4",
        Key::F5 => "F5",
        Key::F6 => "F6",
        Key::F7 => "F7",
        Key::F8 => "F8",
        Key::F9 => "F9",
        Key::F10 => "F10",
        Key::F11 => "F11",
        Key::F12 => "F12",
        _ => return None,
    })
}

/// A recording owns its input helper, never the user's target application.
pub struct RecordingSession {
    child: Option<std::process::Child>,
    directory: std::path::PathBuf,
    event_file: std::path::PathBuf,
    window: crate::window_capture::WindowInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationRecording {
    pub version: u32,
    pub window: String,
    pub bounds: [i64; 4],
    pub events: Vec<RecordedEvent>,
}

#[derive(Serialize)]
pub struct RecordingStatus {
    pub running: bool,
    pub paused: bool,
    pub events: Vec<RecordedEvent>,
}

impl RecordingSession {
    pub fn window_id(&self) -> &str {
        &self.window.id
    }
    pub fn start(window_id: &str, executable: &Path) -> Result<Self, String> {
        Self::start_with_capture_shortcut(window_id, executable, "PrintScreen")
    }

    pub fn start_with_capture_shortcut(window_id: &str, executable: &Path, capture_shortcut: &str) -> Result<Self, String> {
        if crate::window_capture::is_wayland_session() {
            return Err("Operation recording requires an X11 session on Linux".into());
        }
        let window = crate::window_capture::list_windows()?
            .into_iter()
            .find(|window| window.id == window_id)
            .ok_or("Selected window no longer exists")?;
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_nanos();
        let directory =
            std::env::temp_dir().join(format!("markits-recording-{}-{nonce}", std::process::id()));
        std::fs::create_dir(&directory).map_err(|e| e.to_string())?;
        let event_file = directory.join("events.jsonl");
        let mut session = Self {
            child: None,
            directory,
            event_file,
            window,
        };
        let context = crate::semantic_target::window_context(window_id)?;
        let log = File::create(session.directory.join("helper.log")).map_err(|e| e.to_string())?;
        let mut command = std::process::Command::new(executable);
        command
            .arg("--manual-studio-record-input")
            .arg(&session.event_file)
            .env("MARKITS_RECORDING_TARGET_ONLY", "1")
            .env("MARKITS_RECORDING_WINDOW_ID", window_id)
            .env("MARKITS_RECORDING_CAPTURE_SHORTCUT", capture_shortcut)
            .env(
                "MUNIN_RECORDING_WINDOW",
                serde_json::to_string(&context).map_err(|e| e.to_string())?,
            )
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(log);
        #[cfg(target_os = "windows")]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000);
        }
        session.child = Some(command.spawn().map_err(|e| e.to_string())?);
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            session.check_helper()?;
            if session.event_file.is_file() {
                break;
            }
            if Instant::now() >= deadline {
                return Err("Input recorder did not become ready".into());
            }
            thread::sleep(Duration::from_millis(20));
        }
        crate::window_capture::activate_window(window_id)?;
        Ok(session)
    }

    fn check_helper(&mut self) -> Result<bool, String> {
        match self
            .child
            .as_mut()
            .ok_or("Recording already finished")?
            .try_wait()
            .map_err(|e| e.to_string())?
        {
            Some(status) if !status.success() => {
                let log =
                    std::fs::read_to_string(self.directory.join("helper.log")).unwrap_or_default();
                Err(format!("Input recorder failed ({status}): {log}"))
            }
            Some(_) => Ok(false),
            None => Ok(true),
        }
    }

    pub fn pause(&self, paused: bool) -> Result<(), String> {
        let file = self.event_file.with_extension("pause");
        if paused {
            std::fs::write(file, "paused").map_err(|e| e.to_string())
        } else {
            match std::fs::remove_file(file) {
                Ok(()) => Ok(()),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(e) => Err(e.to_string()),
            }
        }
    }

    pub fn status(&mut self, exclude: Option<[f64; 4]>) -> Result<RecordingStatus, String> {
        let running = self.check_helper()?;
        Ok(RecordingStatus {
            running,
            paused: self.event_file.with_extension("pause").exists(),
            events: self.events(exclude)?,
        })
    }

    fn events(&self, exclude: Option<[f64; 4]>) -> Result<Vec<RecordedEvent>, String> {
        let bytes = std::fs::read(&self.event_file).map_err(|e| e.to_string())?;
        let end = bytes.iter().rposition(|b| *b == b'\n').map_or(0, |i| i + 1);
        let mut events = Vec::new();
        for line in bytes[..end]
            .split(|b| *b == b'\n')
            .filter(|line| !line.is_empty())
        {
            let event: RecordedEvent = serde_json::from_slice(line).map_err(|e| e.to_string())?;
            let excluded = match (&event, exclude) {
                (
                    RecordedEvent::Click { x, y, .. } | RecordedEvent::Scroll { x, y, .. },
                    Some([left, top, width, height]),
                ) => *x >= left && *x < left + width && *y >= top && *y < top + height,
                _ => false,
            };
            if !excluded {
                events.push(event);
            }
        }
        Ok(events)
    }

    pub fn finish(mut self, exclude: Option<[f64; 4]>) -> Result<OperationRecording, String> {
        self.check_helper()?;
        self.stop();
        Ok(OperationRecording {
            version: 1,
            window: self.window.title.clone(),
            bounds: [
                self.window.x as i64,
                self.window.y as i64,
                self.window.width as i64,
                self.window.height as i64,
            ],
            events: self.events(exclude)?,
        })
    }

    fn stop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

impl Drop for RecordingSession {
    fn drop(&mut self) {
        self.stop();
        let _ = std::fs::remove_dir_all(&self.directory);
    }
}

#[cfg(test)]
fn event_in_window(
    event: &RecordedEvent,
    window: &crate::window_capture::WindowInfo,
    exclude: Option<[f64; 4]>,
) -> bool {
    let (x, y) = match event {
        RecordedEvent::Click { x, y, .. } | RecordedEvent::Scroll { x, y, .. } => (*x, *y),
        _ => return true,
    };
    let contains = |[left, top, width, height]: [f64; 4]| {
        x >= left && x < left + width && y >= top && y < top + height
    };
    contains([
        window.x as f64,
        window.y as f64,
        window.width as f64,
        window.height as f64,
    ]) && !exclude.is_some_and(contains)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_keys_are_excluded_without_excluding_normal_typing() {
        assert!(is_capture_shortcut("PrintScreen", Key::PrintScreen, false, false, false, false));
        assert!(is_capture_shortcut("Alt+PrintScreen", Key::PrintScreen, false, false, true, false));
        assert!(is_capture_shortcut("Control+Shift+S", Key::KeyS, true, true, false, false));
        assert!(!is_capture_shortcut("Control+Shift+S", Key::KeyS, false, false, false, false));
        assert!(!is_capture_shortcut("Control+Shift+S", Key::KeyS, true, false, false, false));
        assert!(!is_capture_shortcut("PrintScreen", Key::PrintScreen, false, false, true, false));
    }

    #[test]
    fn exports_only_target_clicks_and_excludes_controls() {
        let window = crate::window_capture::WindowInfo {
            id: "test".into(),
            title: "Test".into(),
            x: 10,
            y: 20,
            width: 100,
            height: 80,
        };
        let click = |x, y| RecordedEvent::Click {
            elapsed_ms: 1,
            x,
            y,
            target: None,
            visual: None,
        };
        assert!(event_in_window(&click(30., 40.), &window, None));
        assert!(!event_in_window(&click(0., 0.), &window, None));
        assert!(!event_in_window(
            &click(30., 40.),
            &window,
            Some([20., 30., 40., 40.])
        ));
        let json = serde_json::to_value(click(30., 40.)).unwrap();
        assert_eq!(json["kind"], "click");
        assert_eq!(json["elapsed_ms"], 1);
    }

    #[test]
    fn pause_partial_lines_and_cleanup() {
        let directory = tempfile::tempdir().unwrap().keep();
        let event_file = directory.join("events.jsonl");
        std::fs::write(
            &event_file,
            "{\"kind\":\"text\",\"elapsed_ms\":1,\"value\":\"hello\"}\n{unfinished",
        )
        .unwrap();
        let session = RecordingSession {
            child: None,
            directory: directory.clone(),
            event_file,
            window: crate::window_capture::WindowInfo {
                id: "test".into(),
                title: "Test".into(),
                x: 0,
                y: 0,
                width: 100,
                height: 100,
            },
        };
        session.pause(true).unwrap();
        assert!(session.event_file.with_extension("pause").exists());
        session.pause(false).unwrap();
        session.pause(false).unwrap();
        assert_eq!(session.events(None).unwrap().len(), 1);
        drop(session);
        assert!(!directory.exists());
    }
}
