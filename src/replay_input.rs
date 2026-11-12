//! Native input primitives shared by CLI playback.
use std::{thread, time::Duration};
pub fn send(event: rdev::EventType) -> Result<(), String> {
    rdev::simulate(&event).map_err(|e| format!("Cannot replay input: {e:?}"))?;
    thread::sleep(Duration::from_millis(15));
    Ok(())
}

pub fn chord(keys: &[rdev::Key]) -> Result<(), String> {
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

pub fn paste(value: &str) -> Result<(), String> {
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

pub fn parse_chord(value: &str) -> Result<Vec<rdev::Key>, String> {
    use rdev::Key::*;
    value
        .split('+')
        .map(|part| {
            Ok(match part.trim().to_lowercase().as_str() {
                "control" | "ctrl" => ControlLeft,
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
                _ => return Err(format!("Unsupported key operation: {value}")),
            })
        })
        .collect()
}
