//! Probe the hovered control before click delivery, independently of whole-tree scans.
use super::*;
use windows::{core::Interface, Win32::{Foundation::POINT, System::Com::*, UI::{Accessibility::*, WindowsAndMessaging::GetCursorPos}}};

pub(super) struct Probe {
    pub target: crate::semantic_target::Target,
    bounds: (f64, f64, f64, f64),
}
impl Probe {
    pub fn contains(&self, x: f64, y: f64) -> bool {
        let (left, top, right, bottom) = self.bounds;
        x >= left && x < right && y >= top && y < bottom
    }
}
type CachedTarget = Option<(Instant, Probe)>;

pub(super) fn probe_point(x: i32, y: i32) -> Option<(crate::semantic_target::Target, [f64; 4])> {
    unsafe {
        let initialized = CoInitializeEx(None, COINIT_MULTITHREADED).is_ok();
        let result = CoCreateInstance::<_, IUIAutomation>(&CUIAutomation8, None, CLSCTX_INPROC_SERVER).ok()
            .and_then(|automation| {
                if let Ok(options) = automation.cast::<IUIAutomation2>() {
                    let _ = options.SetConnectionTimeout(500);
                    let _ = options.SetTransactionTimeout(500);
                }
                at_point(&automation, POINT { x, y }).map(|probe| (probe.target, [probe.bounds.0, probe.bounds.1, probe.bounds.2, probe.bounds.3]))
            });
        if initialized { CoUninitialize(); }
        result
    }
}

pub(super) fn start(window: Option<String>) -> Arc<Mutex<CachedTarget>> {
    let cache = Arc::new(Mutex::new(None));
    let output = cache.clone();
    if let Some(id) = window {
        thread::spawn(move || unsafe {
            if CoInitializeEx(None, COINIT_MULTITHREADED).is_err() { return; }
            if let Ok(automation) = CoCreateInstance::<_, IUIAutomation>(&CUIAutomation8, None, CLSCTX_INPROC_SERVER) {
                if let Ok(options) = automation.cast::<IUIAutomation2>() {
                    let _ = options.SetConnectionTimeout(500);
                    let _ = options.SetTransactionTimeout(500);
                }
                loop {
                    let mut point = POINT::default();
                    let target = if GetCursorPos(&mut point).is_ok()
                        && crate::window_capture::pointer_targets_window(&id, point.x as f64, point.y as f64).unwrap_or(false) {
                        at_point(&automation, point).map(|target| (Instant::now(), target))
                    } else { None };
                    if let Ok(mut value) = output.lock() { *value = target; }
                    thread::sleep(Duration::from_millis(40));
                }
            }
            CoUninitialize();
        });
    }
    cache
}

unsafe fn at_point(automation: &IUIAutomation, point: POINT) -> Option<Probe> {
    // Raw view preserves unnamed text/image children so their named button parent
    // can be found even when it is many layers down in a web application.
    unsafe {
        let walker = automation.RawViewWalker().ok()?;
        let mut element = automation.ElementFromPoint(point).ok()?;
        for _ in 0..128 {
            let control = element.CurrentControlType().ok()?;
            if control == UIA_WindowControlTypeId { break; }
            if let Some(role) = role(control) {
                let name = element.CurrentName().map(|name| name.to_string()).unwrap_or_default();
                if !name.trim().is_empty() {
                    let bounds = element.CurrentBoundingRectangle().ok()?;
                    return Some(Probe {
                        target: crate::semantic_target::Target { role: role.into(), name: name.trim().into(), reveals: None },
                        bounds: (bounds.left as f64, bounds.top as f64, bounds.right as f64, bounds.bottom as f64),
                    });
                }
            }
            element = walker.GetParentElement(&element).ok()?;
        }
        None
    }
}

fn role(control: UIA_CONTROLTYPE_ID) -> Option<&'static str> {
    Some(if control == UIA_ButtonControlTypeId { "button" }
    else if control == UIA_CheckBoxControlTypeId { "checkbox" }
    else if control == UIA_RadioButtonControlTypeId { "radio" }
    else if control == UIA_EditControlTypeId { "input" }
    else if control == UIA_ComboBoxControlTypeId { "combobox" }
    else if control == UIA_MenuItemControlTypeId { "menuitem" }
    else if control == UIA_TabItemControlTypeId { "tab" }
    else if control == UIA_HyperlinkControlTypeId { "link" }
    else if control == UIA_ListItemControlTypeId || control == UIA_TreeItemControlTypeId { "listitem" }
    else if control == UIA_DataItemControlTypeId { "cell" }
    else if control == UIA_SpinnerControlTypeId { "button" }
    else { return None; })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hovered_name_is_reused_only_inside_the_same_control() {
        let probe = Probe {
            target: crate::semantic_target::Target { role: "button".into(), name: "Save".into(), reveals: None },
            bounds: (100.0, 200.0, 300.0, 240.0),
        };
        assert!(probe.contains(101.0, 201.0));
        assert!(probe.contains(299.0, 239.0));
        assert!(!probe.contains(300.0, 220.0));
        assert!(!probe.contains(150.0, 240.0));
        assert!(!probe.contains(99.0, 220.0));
    }
}
