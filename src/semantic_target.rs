//! Named desktop targets. Coordinates are resolved again for each replay.
use crate::ui_elements::DetectedUiElement;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Target {
    pub role: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reveals: Option<Box<Target>>,
}

impl Target {
    pub fn validate(&self) -> Result<(), String> {
        if self.role.trim().is_empty() || self.name.trim().is_empty() {
            return Err("クリック対象には空でない種類と名前が必要です。".into());
        }
        if let Some(target) = &self.reveals {
            target.validate()?;
        }
        Ok(())
    }

    pub fn needs_click(&self, elements: &[DetectedUiElement]) -> Result<bool, String> {
        match &self.reveals {
            Some(target) => Ok(target.resolve(elements)?.is_none()),
            None => Ok(true),
        }
    }

    pub fn resolve<'a>(
        &self,
        elements: &'a [DetectedUiElement],
    ) -> Result<Option<&'a DetectedUiElement>, String> {
        let mut matches = elements.iter().filter(|element| {
            element.role == self.role
                && element.name.as_deref() == Some(self.name.as_str())
                && element.width > 0.0
                && element.height > 0.0
        });
        let found = matches.next();
        if matches.next().is_some() {
            return Err(format!(
                "クリック対象が複数あります：{}。操作を中止しました。",
                self.name
            ));
        }
        Ok(found)
    }
}

/// Use the smallest named control under the pointer, only if its identity is unique.
pub fn at_point(elements: &[DetectedUiElement], x: f64, y: f64) -> Option<Target> {
    let element = elements
        .iter()
        .filter(|element| {
            (matches!(
                element.role.as_str(),
                "button"
                    | "checkbox"
                    | "radio"
                    | "input"
                    | "combobox"
                    | "menuitem"
                    | "tab"
                    | "link"
                    | "listitem"
                    | "cell"
            ) || (element.role == "unknown" && element.height <= 64.0))
                && element
                    .name
                    .as_ref()
                    .is_some_and(|name| !name.trim().is_empty())
                && x >= element.x
                && y >= element.y
                && x < element.x + element.width
                && y < element.y + element.height
        })
        .min_by(|a, b| (a.width * a.height).total_cmp(&(b.width * b.height)))?;
    let target = Target {
        role: element.role.clone(),
        name: element.name.clone()?,
        reveals: None,
    };
    target.resolve(elements).ok().flatten()?;
    Some(target)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn button(name: &str, x: f64) -> DetectedUiElement {
        DetectedUiElement {
            role: "button".into(),
            name: Some(name.into()),
            window_id: None,
            pid: None,
            x,
            y: 10.0,
            width: 80.0,
            height: 30.0,
        }
    }
    #[test]
    fn resolves_identity_after_control_moves() {
        let target = at_point(&[button("Open", 10.0)], 20.0, 20.0).unwrap();
        let moved = [button("Open", 200.0)];
        assert_eq!(target.resolve(&moved).unwrap().unwrap().x, 200.0);
        assert!(target.resolve(&[]).unwrap().is_none());
    }
    #[test]
    fn named_containers_do_not_replace_an_unnamed_input() {
        let mut container = button("Project settings", 10.0);
        container.role = "group".into();
        container.width = 400.0;
        container.height = 300.0;
        let mut input = button("", 20.0);
        input.role = "input".into();
        assert!(at_point(&[container, input], 30.0, 20.0).is_none());
    }
    #[test]
    fn expanding_an_already_visible_child_does_not_collapse_its_parent() {
        let child = Target {
            role: "button".into(),
            name: "detail.md".into(),
            reveals: None,
        };
        let parent = Target {
            role: "unknown".into(),
            name: "Chapter フォルダー".into(),
            reveals: Some(Box::new(child)),
        };
        assert!(parent.needs_click(&[]).unwrap());
        assert!(!parent.needs_click(&[button("detail.md", 20.0)]).unwrap());
        assert!(parent
            .needs_click(&[button("detail.md", 20.0), button("detail.md", 200.0)])
            .is_err());
    }
    #[test]
    fn duplicate_names_are_never_guessed() {
        let elements = [button("Open", 10.0), button("Open", 200.0)];
        assert!(at_point(&elements, 20.0, 20.0).is_none());
        assert!(Target {
            role: "button".into(),
            name: "Open".into(),
            reveals: None,
        }
        .resolve(&elements)
        .is_err());
    }
}

/// Observe only the selected native window, using the same geometry as recording.
pub fn window_context(id: &str) -> Result<DetectedUiElement, String> {
    let window = super::window_capture::list_windows()?
        .into_iter()
        .find(|window| window.id == id)
        .ok_or("対象ウィンドウが閉じられました。")?;
    #[cfg(target_os = "linux")]
    let context = {
        let native_id = u64::from_str_radix(id.trim_start_matches("0x"), 16)
            .map_err(|error| error.to_string())?
            .to_string();
        crate::ui_elements::capture_desktop_windows(0, 0)
            .into_iter()
            .find(|candidate| candidate.window_id.as_deref() == Some(native_id.as_str()))
            .ok_or("対象ウィンドウが閉じられました。")?
    };
    #[cfg(not(target_os = "linux"))]
    let pid = super::window_capture::window_process_ids()?
        .into_iter()
        .find(|(candidate, _)| candidate == id)
        .map(|(_, pid)| pid)
        .ok_or("対象ウィンドウのアプリを確認できません。")?;
    #[cfg(target_os = "windows")]
    let accessible_id = {
        let handle = u32::from_str_radix(id.trim_start_matches("0x"), 16)
            .map_err(|error| error.to_string())?;
        Some(format!("hwnd:{:#x}", (handle as i32 as isize) as usize))
    };
    #[cfg(target_os = "macos")]
    let accessible_id = None;
    #[cfg(not(target_os = "linux"))]
    let context = DetectedUiElement {
        role: "window".into(),
        name: Some(window.title),
        window_id: accessible_id,
        pid: Some(pid),
        x: window.x as f64,
        y: window.y as f64,
        width: window.width as f64,
        height: window.height as f64,
    };
    #[cfg(target_os = "linux")]
    let _ = window;
    Ok(context)
}

pub fn observe_window(id: &str) -> Result<Vec<DetectedUiElement>, String> {
    Ok(crate::ui_elements::observe_desktop_targets(
        &window_context(id)?,
    ))
}

pub fn input_value(id: &str, target: &Target) -> Result<Option<String>, String> {
    crate::ui_elements::desktop_input_value(&window_context(id)?, &target.name)
}
