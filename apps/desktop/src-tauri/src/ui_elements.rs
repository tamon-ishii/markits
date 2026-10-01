use serde::{Deserialize, Serialize};
use xa11y::{App, AppExt, Element, Role};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DetectedUiElement {
    pub role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// Capture UI elements across accessible desktop windows.
/// Coordinates are offset relative to the specified screen origin.
pub fn capture_desktop_ui_elements(screen_origin_x: i32, screen_origin_y: i32) -> Vec<DetectedUiElement> {
    let mut elements = Vec::new();
    let current_pid = std::process::id();

    // 1. Collect top-level windows via X11 on Linux (as in pymodulemgr)
    #[cfg(target_os = "linux")]
    {
        for win in linux_x11::list_x11_windows(current_pid) {
            elements.push(DetectedUiElement {
                role: "window".to_string(),
                name: Some(win.title),
                x: (win.x - screen_origin_x) as f64,
                y: (win.y - screen_origin_y) as f64,
                width: win.width as f64,
                height: win.height as f64,
            });
        }
    }

    // 2. Collect granular controls (buttons, inputs, tabs, etc.) via AT-SPI (xa11y)
    if let Ok(apps) = App::list() {
        for app in apps {
            if let Some(pid) = app.pid {
                if pid == current_pid {
                    continue;
                }
            }

            let Ok(windows) = app.windows() else {
                continue;
            };

            for window in windows {
                collect_elements_recursive(&window, screen_origin_x, screen_origin_y, &mut elements, 0);
            }
        }
    }

    elements
}

fn collect_elements_recursive(
    element: &Element,
    origin_x: i32,
    origin_y: i32,
    out: &mut Vec<DetectedUiElement>,
    depth: usize,
) {
    if depth > 6 {
        return;
    }

    // Skip minimized windows and their children
    if element.states.minimized == Some(true) {
        return;
    }

    if let Some(bounds) = element.bounds {
        // Only keep reasonable UI elements
        if bounds.width > 4 && bounds.height > 4 && bounds.width < 5000 && bounds.height < 5000 {
            let role_str = match element.role {
                Role::Button => Some("button"),
                Role::CheckBox => Some("checkbox"),
                Role::RadioButton => Some("radio"),
                Role::TextField | Role::TextArea => Some("input"),
                Role::ComboBox => Some("combobox"),
                Role::MenuItem => Some("menuitem"),
                Role::Tab => Some("tab"),
                Role::Link => Some("link"),
                Role::SpinButton | Role::Switch => Some("button"),
                Role::Toolbar => Some("toolbar"),
                Role::Window | Role::Dialog => Some("window"),
                _ => None,
            };

            if let Some(r) = role_str {
                let name = element
                    .name
                    .as_deref()
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty());

                let el_x = (bounds.x - origin_x) as f64;
                let el_y = (bounds.y - origin_y) as f64;
                let el_w = bounds.width as f64;
                let el_h = bounds.height as f64;

                // Avoid exact duplicate window bounds if already collected from X11
                let is_dup = out.iter().any(|existing| {
                    (existing.x - el_x).abs() < 2.0
                        && (existing.y - el_y).abs() < 2.0
                        && (existing.width - el_w).abs() < 2.0
                        && (existing.height - el_h).abs() < 2.0
                });

                if !is_dup {
                    out.push(DetectedUiElement {
                        role: r.to_string(),
                        name,
                        x: el_x,
                        y: el_y,
                        width: el_w,
                        height: el_h,
                    });
                }
            }
        }
    }

    if let Ok(children) = element.children() {
        for child in children {
            collect_elements_recursive(&child, origin_x, origin_y, out, depth + 1);
        }
    }
}

#[cfg(target_os = "linux")]
mod linux_x11 {
    use std::ffi::{CStr, CString};
    use std::os::raw::{c_int, c_uchar, c_ulong, c_void};
    use std::ptr;
    use x11_dl::xlib;

    pub struct WindowBounds {
        pub title: String,
        pub x: i32,
        pub y: i32,
        pub width: u32,
        pub height: u32,
    }

    pub fn list_x11_windows(exclude_pid: u32) -> Vec<WindowBounds> {
        let Ok(api) = xlib::Xlib::open() else {
            return Vec::new();
        };
        let display = unsafe { (api.XOpenDisplay)(ptr::null()) };
        if display.is_null() {
            return Vec::new();
        }
        let root = unsafe { (api.XDefaultRootWindow)(display) };

        let atom = |name: &str| -> xlib::Atom {
            let c_name = CString::new(name).unwrap();
            unsafe { (api.XInternAtom)(display, c_name.as_ptr(), xlib::False) }
        };

        let get_property = |window: xlib::Window, prop: xlib::Atom| -> Option<(c_int, Vec<u8>)> {
            let mut actual_type = 0;
            let mut format = 0;
            let mut count = 0;
            let mut after = 0;
            let mut data: *mut c_uchar = ptr::null_mut();
            let status = unsafe {
                (api.XGetWindowProperty)(
                    display,
                    window,
                    prop,
                    0,
                    4096,
                    xlib::False,
                    0,
                    &mut actual_type,
                    &mut format,
                    &mut count,
                    &mut after,
                    &mut data,
                )
            };
            if status != 0 || data.is_null() || !matches!(format, 8 | 32) {
                if !data.is_null() {
                    unsafe { (api.XFree)(data.cast::<c_void>()) };
                }
                return None;
            }
            let el_size = if format == 32 {
                std::mem::size_of::<c_ulong>()
            } else {
                1
            };
            let bytes = unsafe { std::slice::from_raw_parts(data, count as usize * el_size).to_vec() };
            unsafe { (api.XFree)(data.cast::<c_void>()) };
            Some((format, bytes))
        };

        let client_list_atom = atom("_NET_CLIENT_LIST");
        let name_atom = atom("_NET_WM_NAME");
        let pid_atom = atom("_NET_WM_PID");

        let mut windows = Vec::new();
        if let Some((32, bytes)) = get_property(root, client_list_atom) {
            let chunk_size = std::mem::size_of::<c_ulong>();
            for chunk in bytes.chunks_exact(chunk_size) {
                let mut native = [0u8; std::mem::size_of::<c_ulong>()];
                native.copy_from_slice(chunk);
                let win = c_ulong::from_ne_bytes(native) as xlib::Window;

                // Check PID
                if let Some((32, pid_bytes)) = get_property(win, pid_atom) {
                    if pid_bytes.len() >= chunk_size {
                        let mut pnative = [0u8; std::mem::size_of::<c_ulong>()];
                        pnative.copy_from_slice(&pid_bytes[..chunk_size]);
                        let win_pid = c_ulong::from_ne_bytes(pnative) as u32;
                        if win_pid == exclude_pid {
                            continue;
                        }
                    }
                }

                // Check attributes & visibility
                let mut attr = std::mem::MaybeUninit::<xlib::XWindowAttributes>::uninit();
                if unsafe { (api.XGetWindowAttributes)(display, win, attr.as_mut_ptr()) } == 0 {
                    continue;
                }
                let attr = unsafe { attr.assume_init() };
                if attr.map_state != xlib::IsViewable || attr.width < 10 || attr.height < 10 {
                    continue;
                }

                // Get coordinates relative to root
                let mut x = 0;
                let mut y = 0;
                let mut child = 0;
                if unsafe {
                    (api.XTranslateCoordinates)(
                        display,
                        win,
                        root,
                        0,
                        0,
                        &mut x,
                        &mut y,
                        &mut child,
                    )
                } == 0
                {
                    continue;
                }

                // Get title
                let title = if let Some((8, name_bytes)) = get_property(win, name_atom) {
                    String::from_utf8_lossy(&name_bytes).trim_end_matches('\0').to_string()
                } else {
                    let mut raw = ptr::null_mut();
                    if unsafe { (api.XFetchName)(display, win, &mut raw) } != 0 && !raw.is_null() {
                        let text = unsafe { CStr::from_ptr(raw).to_string_lossy().into_owned() };
                        unsafe { (api.XFree)(raw.cast::<c_void>()) };
                        text
                    } else {
                        String::new()
                    }
                };

                if !title.is_empty() {
                    windows.push(WindowBounds {
                        title,
                        x,
                        y,
                        width: attr.width as u32,
                        height: attr.height as u32,
                    });
                }
            }
        }

        unsafe {
            (api.XCloseDisplay)(display);
        }
        windows
    }
}

/// Filter and translate UI elements relative to a cropped region.
pub fn filter_elements_for_crop(
    elements: &[DetectedUiElement],
    crop_x: f64,
    crop_y: f64,
    crop_w: f64,
    crop_h: f64,
) -> Vec<DetectedUiElement> {
    let crop_r = crop_x + crop_w;
    let crop_b = crop_y + crop_h;

    elements
        .iter()
        .filter_map(|el| {
            let el_r = el.x + el.width;
            let el_b = el.y + el.height;

            // Check if element intersects the crop rectangle
            let inter_x1 = el.x.max(crop_x);
            let inter_y1 = el.y.max(crop_y);
            let inter_x2 = el_r.min(crop_r);
            let inter_y2 = el_b.min(crop_b);

            if inter_x1 < inter_x2 && inter_y1 < inter_y2 {
                // If it significantly overlaps, translate coordinates
                let new_x = el.x - crop_x;
                let new_y = el.y - crop_y;

                // When user crops a window, the window's outer boundary matches the full crop region.
                // In the editor, snapping to the full canvas borders disrupts snapping to controls inside,
                // so exclude full-window boundaries that span the entire crop rectangle.
                if el.role == "window"
                    && new_x.abs() <= 8.0
                    && new_y.abs() <= 8.0
                    && (el.width - crop_w).abs() <= 16.0
                    && (el.height - crop_h).abs() <= 16.0
                {
                    return None;
                }

                Some(DetectedUiElement {
                    role: el.role.clone(),
                    name: el.name.clone(),
                    x: new_x,
                    y: new_y,
                    width: el.width,
                    height: el.height,
                })
            } else {
                None
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_collect_elements() {
        if let Ok(apps) = App::list() {
            println!("Discovered apps count: {}", apps.len());
            for app in &apps {
                println!("App: name={:?}, pid={:?}", app.name, app.pid);
                if let Ok(windows) = app.windows() {
                    println!("  Windows count: {}", windows.len());
                    for w in &windows {
                        println!("    Window: role={:?}, name={:?}, bounds={:?}", w.role, w.name, w.bounds);
                    }
                }
            }
        }

        let elements = capture_desktop_ui_elements(0, 0);
        println!("Total detected elements: {}", elements.len());

        let mut role_counts = std::collections::HashMap::new();
        for el in &elements {
            *role_counts.entry(el.role.clone()).or_insert(0) += 1;
        }
        println!("Role counts: {:?}", role_counts);

        let buttons: Vec<_> = elements.iter().filter(|el| el.role == "button").collect();
        println!("Button count: {}", buttons.len());
        for btn in buttons.iter().take(20) {
            println!("  [button] {:?} at ({}, {}) {}x{}", btn.name, btn.x, btn.y, btn.width, btn.height);
        }
    }

    #[test]
    fn test_filter_crop() {
        let list = vec![
            DetectedUiElement {
                role: "window".into(),
                name: Some("App Window".into()),
                x: 80.0,
                y: 80.0,
                width: 100.0,
                height: 100.0,
            },
            DetectedUiElement {
                role: "button".into(),
                name: Some("OK".into()),
                x: 100.0,
                y: 100.0,
                width: 50.0,
                height: 30.0,
            },
            DetectedUiElement {
                role: "button".into(),
                name: Some("Cancel".into()),
                x: 500.0,
                y: 500.0,
                width: 50.0,
                height: 30.0,
            },
        ];

        let cropped = filter_elements_for_crop(&list, 80.0, 80.0, 100.0, 100.0);
        // The window element (80,80 100x100) exactly matches crop region and must be excluded!
        assert_eq!(cropped.len(), 1);
        assert_eq!(cropped[0].name.as_deref(), Some("OK"));
        assert_eq!(cropped[0].x, 20.0);
        assert_eq!(cropped[0].y, 20.0);
    }
}
