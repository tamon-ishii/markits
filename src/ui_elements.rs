use serde::{Deserialize, Serialize};
#[cfg(not(target_os = "linux"))]
use xa11y::{App, AppExt, Element, Rect, Role};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DetectedUiElement {
    pub role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub window_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pid: Option<u32>,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl DetectedUiElement {
    pub fn to_ui_element(&self) -> crate::UiElement {
        crate::UiElement::new(
            &self.role,
            self.name.as_deref().unwrap_or_default(),
            self.x,
            self.y,
            self.width,
            self.height,
        )
    }
}

impl From<DetectedUiElement> for crate::UiElement {
    fn from(el: DetectedUiElement) -> Self {
        el.to_ui_element()
    }
}

/// Collect top-level windows across the desktop (super fast, ~5ms).
pub fn capture_desktop_windows(screen_origin_x: i32, screen_origin_y: i32) -> Vec<DetectedUiElement> {
    let mut elements = Vec::new();
    let current_pid = std::process::id();

    // 1. Collect top-level windows via X11 on Linux (as in pymodulemgr)
    #[cfg(target_os = "linux")]
    {
        for win in linux_x11::list_x11_windows(current_pid) {
            elements.push(DetectedUiElement {
                role: "window".to_string(),
                name: Some(win.title),
                window_id: None,
                pid: win.pid,
                x: (win.x - screen_origin_x) as f64,
                y: (win.y - screen_origin_y) as f64,
                width: win.width as f64,
                height: win.height as f64,
            });
        }
    }

    // Fallback for non-Linux top-level windows
    #[cfg(not(target_os = "linux"))]
    {
        if let Ok(apps) = App::list() {
            for app in apps {
                if let Some(pid) = app.pid {
                    if pid == current_pid {
                        continue;
                    }
                }
                if let Ok(windows) = app.windows() {
                    for w in windows {
                        if let Some(b) = w.bounds {
                            elements.push(DetectedUiElement {
                                role: "window".to_string(),
                                name: w.name.clone(),
                                window_id: w.stable_id.clone(),
                                pid: w.pid.or(app.pid),
                                x: (b.x - screen_origin_x) as f64,
                                y: (b.y - screen_origin_y) as f64,
                                width: b.width as f64,
                                height: b.height as f64,
                            });
                        }
                    }
                }
            }
        }
    }

    elements
}

/// Collect detailed controls (buttons, inputs, tabs, links, etc.) via AT-SPI / Accessibility.
/// If `bounds_filter` is provided (e.g. cropped rectangle), windows/apps that do not intersect
/// with the rectangle are pruned immediately to speed up collection dramatically.
pub fn capture_desktop_detailed_elements(
    screen_origin_x: i32,
    screen_origin_y: i32,
    bounds_filter: Option<(f64, f64, f64, f64)>,
) -> Vec<DetectedUiElement> {
    #[cfg(target_os = "linux")]
    {
        let windows = capture_desktop_windows(screen_origin_x, screen_origin_y);
        let mut elements = Vec::new();
        for window in windows {
            if bounds_filter.is_some_and(|(x, y, w, h)| {
                window.x + window.width <= x || window.x >= x + w
                    || window.y + window.height <= y || window.y >= y + h
            }) { continue; }
            elements.extend(capture_desktop_detailed_elements_for_window(
                screen_origin_x, screen_origin_y, &window,
            ));
        }
        return elements;
    }
    #[cfg(not(target_os = "linux"))]
    collect_detailed_elements(screen_origin_x, screen_origin_y, bounds_filter, None)
}

pub fn capture_desktop_detailed_elements_for_window(
    screen_origin_x: i32,
    screen_origin_y: i32,
    target: &DetectedUiElement,
) -> Vec<DetectedUiElement> {
    #[cfg(target_os = "linux")]
    return linux_atspi::collect_window_elements(screen_origin_x, screen_origin_y, target);
    #[cfg(not(target_os = "linux"))]
    collect_detailed_elements(screen_origin_x, screen_origin_y, None, Some(target))
}

/// Identify the process behind a captured X11 window when available.
pub fn target_process_id(target: &DetectedUiElement) -> Option<u32> {
    #[cfg(target_os = "linux")]
    { target.pid.or_else(|| linux_x11::pid_for_target(target)) }
    #[cfg(not(target_os = "linux"))]
    { target.pid }
}

#[cfg(not(target_os = "linux"))]
fn collect_detailed_elements(
    screen_origin_x: i32,
    screen_origin_y: i32,
    bounds_filter: Option<(f64, f64, f64, f64)>,
    target: Option<&DetectedUiElement>,
) -> Vec<DetectedUiElement> {
    let mut elements = if let Some(target) = target {
        vec![target.clone()]
    } else {
        capture_desktop_windows(screen_origin_x, screen_origin_y)
    };
    let current_pid = std::process::id();

    #[cfg(not(target_os = "linux"))]
    let apps = if let Some(pid) = target.and_then(target_process_id) {
        App::by_pid(pid, std::time::Duration::ZERO).ok().into_iter().collect::<Vec<_>>()
    } else {
        App::list().unwrap_or_default()
    };

    {
        for app in apps {
            if let Some(pid) = app.pid {
                if pid == current_pid {
                    continue;
                }
            }

            // The process-specific root avoids a second desktop-wide enumeration.
            let windows = if target.is_some() {
                app.as_element().children()
            } else {
                app.windows()
            };
            let Ok(windows) = windows else {
                continue;
            };

            for window in windows {
                if target.is_none() && !matches!(window.role, Role::Window | Role::Dialog) {
                    continue;
                }
                if let Some(target) = target {
                    if !target_accepts_window(
                        target, window.stable_id.as_deref(), window.pid.or(app.pid),
                    ) {
                        continue;
                    }
                    // Accessibility bounds often describe the content area while X11
                    // describes the decorated window. Match substantial overlap too.
                    if !window.bounds.is_some_and(|b| {
                        window_overlaps_target(
                            (b.x - screen_origin_x) as f64,
                            (b.y - screen_origin_y) as f64,
                            b.width as f64,
                            b.height as f64,
                            target,
                        )
                    }) {
                        continue;
                    }
                }
                if let Some((fx, fy, fw, fh)) = bounds_filter {
                    if let Some(wb) = window.bounds {
                        let wx = (wb.x - screen_origin_x) as f64;
                        let wy = (wb.y - screen_origin_y) as f64;
                        let ww = wb.width as f64;
                        let wh = wb.height as f64;
                        let intersects = !(wx + ww <= fx || wx >= fx + fw || wy + wh <= fy || wy >= fy + fh);
                        if !intersects {
                            continue;
                        }
                    }
                }

                #[cfg(target_os = "linux")]
                let relative_position = target.and_then(|target| {
                    let bounds = window.bounds?;
                    if bounds.x == 0 && bounds.y == 0 && (target.x.abs() > 2.0 || target.y.abs() > 2.0) {
                        linux_atspi::connect().map(|connection| (connection, target.x, target.y))
                    } else {
                        None
                    }
                });
                #[cfg(not(target_os = "linux"))]
                let relative_position: Option<((), f64, f64)> = None;

                collect_elements_recursive(
                    &window, screen_origin_x, screen_origin_y, &mut elements, 0,
                    target.map(|target| (target.x, target.y, target.width, target.height)).or(bounds_filter),
                    relative_position.as_ref(),
                );
            }
        }
    }

    elements
}

#[cfg(any(test, not(target_os = "linux")))]
fn target_accepts_window(
    target: &DetectedUiElement, candidate_id: Option<&str>, candidate_pid: Option<u32>,
) -> bool {
    target.pid.is_none_or(|pid| candidate_pid == Some(pid))
        && target.window_id.as_deref().is_none_or(|id| candidate_id == Some(id))
}

fn window_overlaps_target(x: f64, y: f64, w: f64, h: f64, target: &DetectedUiElement) -> bool {
    if w <= 0.0 || h <= 0.0 || target.width <= 0.0 || target.height <= 0.0 {
        return false;
    }
    // Some GTK apps report their top-level AT-SPI bounds relative to the
    // window even when screen coordinates were requested.
    if x == 0.0 && y == 0.0 && (target.x.abs() > 2.0 || target.y.abs() > 2.0)
        && (w - target.width).abs() <= 8.0 && (h - target.height).abs() <= 8.0
    {
        return true;
    }
    let overlap_w = (x + w).min(target.x + target.width) - x.max(target.x);
    let overlap_h = (y + h).min(target.y + target.height) - y.max(target.y);
    if overlap_w <= 0.0 || overlap_h <= 0.0 {
        return false;
    }
    let overlap = overlap_w * overlap_h;
    let smaller_area = (w * h).min(target.width * target.height);
    let larger_area = (w * h).max(target.width * target.height);
    larger_area / smaller_area <= 1.6 && overlap / smaller_area >= 0.75
}

#[cfg(not(target_os = "linux"))]
type RelativePosition = ((), f64, f64);

#[cfg(not(target_os = "linux"))]
fn element_position(
    element: &Element,
    bounds: Rect,
    origin_x: i32,
    origin_y: i32,
    relative_position: Option<&RelativePosition>,
) -> (f64, f64, f64, f64) {
    #[cfg(target_os = "linux")]
    if let Some((connection, window_x, window_y)) = relative_position {
        if let Some((x, y, w, h)) = linux_atspi::window_extents(connection, element) {
            return (window_x + x as f64 - origin_x as f64,
                    window_y + y as f64 - origin_y as f64,
                    w as f64, h as f64);
        }
    }
    #[cfg(not(target_os = "linux"))]
    let _ = relative_position;
    (
        (bounds.x - origin_x) as f64,
        (bounds.y - origin_y) as f64,
        bounds.width as f64,
        bounds.height as f64,
    )
}

#[cfg(target_os = "linux")]
mod linux_atspi {
    use std::collections::VecDeque;
    use std::time::Duration;
    use xa11y::{App, AppExt};
    use zbus::blocking::{connection::Builder, Connection, Proxy};
    use zbus::zvariant::OwnedObjectPath;

    pub fn collect_window_elements(
        origin_x: i32, origin_y: i32, target: &super::DetectedUiElement,
    ) -> Vec<super::DetectedUiElement> {
        let mut elements = vec![target.clone()];
        let Some(pid) = super::target_process_id(target) else { return elements };
        let Ok(app) = App::by_pid(pid, Duration::ZERO) else { return elements };
        let root = app.as_element();
        let Some(bus_name) = root.raw.get("bus_name").and_then(|value| value.as_str()) else { return elements };
        let Some(path) = root.raw.get("object_path").and_then(|value| value.as_str()) else { return elements };
        let Some(connection) = connect() else { return elements };
        let mut queue = VecDeque::from([(bus_name.to_owned(), path.to_owned(), 0usize)]);
        let mut relative_coordinates = false;
        let mut visited = 0usize;

        // Read one property at a time. Some WebKitGTK accessibility bridges
        // abort when many nodes and attributes are queried concurrently.
        while let Some((bus, path, depth)) = queue.pop_front() {
            if visited >= 500 { break; }
            visited += 1;
            let Ok(accessible) = Proxy::new(&connection, bus.as_str(), path.as_str(), "org.a11y.atspi.Accessible") else { continue };
            let role: u32 = accessible.call("GetRole", &()).unwrap_or_default();
            let name: Option<String> = accessible.get_property::<String>("Name")
                .ok().map(|name| name.trim().to_owned()).filter(|name| !name.is_empty());
            let bounds = Proxy::new(&connection, bus.as_str(), path.as_str(), "org.a11y.atspi.Component")
                .ok().and_then(|proxy| proxy.call::<_, _, (i32, i32, i32, i32)>("GetExtents", &(0u32,)).ok());

            if depth == 1 {
                let Some((x, y, w, h)) = bounds else { continue };
                if !super::window_overlaps_target(
                    (x - origin_x) as f64, (y - origin_y) as f64, w as f64, h as f64, target,
                ) { continue; }
                relative_coordinates = x == 0 && y == 0
                    && (target.x.abs() > 2.0 || target.y.abs() > 2.0);
            }

            let position = if relative_coordinates && depth > 0 {
                Proxy::new(&connection, bus.as_str(), path.as_str(), "org.a11y.atspi.Component")
                    .ok().and_then(|proxy| proxy.call::<_, _, (i32, i32, i32, i32)>("GetExtents", &(1u32,)).ok())
                    .map(|(x, y, w, h)| (target.x + x as f64, target.y + y as f64, w as f64, h as f64))
            } else {
                bounds.map(|(x, y, w, h)| ((x - origin_x) as f64, (y - origin_y) as f64, w as f64, h as f64))
            };
            if let Some((x, y, w, h)) = position {
                if w <= 0.0 || h <= 0.0 || x + w <= target.x || x >= target.x + target.width
                    || y + h <= target.y || y >= target.y + target.height { continue; }
                if let Some(mapped_role) = map_role(role) {
                    if depth > 1 && w > 4.0 && h > 4.0 && w < 5000.0 && h < 5000.0
                        && !elements.iter().any(|item| item.role == mapped_role
                            && (item.x-x).abs() < 2.0
                            && (item.y-y).abs() < 2.0 && (item.width-w).abs() < 2.0
                            && (item.height-h).abs() < 2.0)
                    {
                        elements.push(super::DetectedUiElement {
                            role: mapped_role.to_owned(), name, window_id: None, pid: None,
                            x, y, width: w, height: h,
                        });
                    }
                }
            }
            if depth >= 32 { continue; }
            if let Ok(children) = accessible.call::<_, _, Vec<(String, OwnedObjectPath)>>("GetChildren", &()) {
                for (child_bus, child_path) in children {
                    queue.push_back((child_bus, child_path.to_string(), depth + 1));
                }
            }
        }
        elements
    }

    fn map_role(role: u32) -> Option<&'static str> {
        match role {
            7 | 8 => Some("checkbox"),
            10 | 47 | 56 | 57 | 58 => Some("cell"),
            11 => Some("combobox"),
            13 | 26 | 27 => Some("image"),
            20 | 39 | 85 | 87 | 99 => Some("group"),
            29 | 81 | 116 => Some("text"),
            31 | 65 | 98 => Some("list"),
            32 | 91 => Some("listitem"),
            35 | 45 | 59 => Some("menuitem"),
            37 => Some("tab"),
            40 | 60 | 61 | 78 | 79 => Some("input"),
            43 | 52 | 62 | 129 => Some("button"),
            44 => Some("radio"),
            55 | 66 => Some("table"),
            63 => Some("toolbar"),
            82 | 92..=96 => Some("webarea"),
            83 => Some("heading"),
            88 => Some("link"),
            110 => Some("navigation"),
            _ => None,
        }
    }

    pub fn connect() -> Option<Connection> {
        let session = Connection::session().ok()?;
        let bus = Proxy::new(&session, "org.a11y.Bus", "/org/a11y/bus", "org.a11y.Bus").ok()?;
        let address: String = bus.call("GetAddress", &()).ok()?;
        Builder::address(address.as_str()).ok()?.build().ok()
    }

}

/// Identify a single-click window capture by its exact crop bounds.
/// A dragged region or full-screen capture has no single target application.
pub fn select_window_for_capture(
    elements: &[DetectedUiElement],
    crop: Option<(f64, f64, f64, f64)>,
) -> Option<DetectedUiElement> {
    let (x, y, w, h) = crop?;
    elements.iter().rev().find(|el| {
        el.role == "window"
            && (el.x - x).abs() <= 1.0 && (el.y - y).abs() <= 1.0
            && (el.width - w).abs() <= 1.0 && (el.height - h).abs() <= 1.0
    }).cloned()
}

/// Capture UI elements across accessible desktop windows.
/// Coordinates are offset relative to the specified screen origin.
pub fn capture_desktop_ui_elements(screen_origin_x: i32, screen_origin_y: i32) -> Vec<DetectedUiElement> {
    capture_desktop_detailed_elements(screen_origin_x, screen_origin_y, None)
}

#[cfg(not(target_os = "linux"))]
fn collect_elements_recursive(
    element: &Element,
    origin_x: i32,
    origin_y: i32,
    out: &mut Vec<DetectedUiElement>,
    depth: usize,
    bounds_filter: Option<(f64, f64, f64, f64)>,
    relative_position: Option<&RelativePosition>,
) {
    if depth > 16 || out.len() >= 1000 {
        return;
    }

    // Minimized state is meaningful on the top-level window. Reading it for every
    // nested panel performs unnecessary accessibility calls on some applications.
    if depth == 0 && element.states.minimized == Some(true) {
        return;
    }

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
        Role::StaticText => Some("text"),
        Role::Heading => Some("heading"),
        Role::Image => Some("image"),
        Role::List => Some("list"),
        Role::ListItem | Role::TreeItem => Some("listitem"),
        Role::Table => Some("table"),
        Role::TableCell => Some("cell"),
        Role::WebArea => Some("webarea"),
        Role::Navigation => Some("navigation"),
        Role::Group if element.name.as_ref().is_some_and(|name| !name.trim().is_empty()) => Some("group"),
        Role::Window | Role::Dialog => Some("window"),
        _ => None,
    };

    if let (Some(role_str), Some(bounds)) = (role_str, element.bounds) {
        let (el_x, el_y, el_w, el_h) = element_position(element, bounds, origin_x, origin_y, relative_position);

        // A container with stale bounds can still have visible children.
        let is_ghost = el_x.abs() < 1.0 && el_y.abs() < 1.0
            && (element.role == Role::MenuItem || element.name.is_none());

        if let Some((fx, fy, fw, fh)) = bounds_filter {
            let intersects = !(el_x + el_w <= fx || el_x >= fx + fw || el_y + el_h <= fy || el_y >= fy + fh);
            if !intersects && !is_ghost {
                return;
            }
        }

        // Only keep reasonable UI elements
        if !is_ghost && el_w > 4.0 && el_h > 4.0 && el_w < 5000.0 && el_h < 5000.0 {
            let name = element
                .name
                .as_deref()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());

            // Avoid exact duplicate window bounds if already collected from X11
            let is_dup = out.iter().any(|existing| {
                (existing.x - el_x).abs() < 2.0
                    && (existing.y - el_y).abs() < 2.0
                    && (existing.width - el_w).abs() < 2.0
                    && (existing.height - el_h).abs() < 2.0
            });

            if !is_dup {
                out.push(DetectedUiElement {
                    role: role_str.to_string(),
                    name,
                    window_id: None,
                    pid: None,
                    x: el_x,
                    y: el_y,
                    width: el_w,
                    height: el_h,
                });
            }
        }
    }

    if let Ok(children) = element.children() {
        for child in children {
            collect_elements_recursive(&child, origin_x, origin_y, out, depth + 1, bounds_filter, relative_position);
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
        pub pid: Option<u32>,
        pub x: i32,
        pub y: i32,
        pub width: u32,
        pub height: u32,
    }

    pub fn pid_for_target(target: &super::DetectedUiElement) -> Option<u32> {
        let windows = list_x11_windows(0);
        windows.iter().find(|window| {
            target.name.as_deref() == Some(window.title.as_str())
                && (window.x as f64 - target.x).abs() <= 1.0
                && (window.y as f64 - target.y).abs() <= 1.0
                && (window.width as f64 - target.width).abs() <= 1.0
                && (window.height as f64 - target.height).abs() <= 1.0
        }).or_else(|| windows.iter().find(|window| {
            target.name.as_deref() == Some(window.title.as_str())
                && super::window_overlaps_target(
                    window.x as f64, window.y as f64,
                    window.width as f64, window.height as f64, target,
                )
        })).and_then(|window| window.pid)
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

        let stacking_atom = atom("_NET_CLIENT_LIST_STACKING");
        let client_list_atom = atom("_NET_CLIENT_LIST");
        let name_atom = atom("_NET_WM_NAME");
        let pid_atom = atom("_NET_WM_PID");
        let frame_extents_atom = atom("_NET_FRAME_EXTENTS");
        let gtk_frame_extents_atom = atom("_GTK_FRAME_EXTENTS");
        let motif_hints_atom = atom("_MOTIF_WM_HINTS");

        let mut windows = Vec::new();
        let list_data = get_property(root, stacking_atom).or_else(|| get_property(root, client_list_atom));

        if let Some((32, bytes)) = list_data {
            let chunk_size = std::mem::size_of::<c_ulong>();
            for chunk in bytes.chunks_exact(chunk_size) {
                let mut native = [0u8; std::mem::size_of::<c_ulong>()];
                native.copy_from_slice(chunk);
                let win = c_ulong::from_ne_bytes(native) as xlib::Window;

                // Check PID
                let mut window_pid = None;
                if let Some((32, pid_bytes)) = get_property(win, pid_atom) {
                    if pid_bytes.len() >= chunk_size {
                        let mut pnative = [0u8; std::mem::size_of::<c_ulong>()];
                        pnative.copy_from_slice(&pid_bytes[..chunk_size]);
                        let win_pid = c_ulong::from_ne_bytes(pnative) as u32;
                        window_pid = Some(win_pid);
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

                // Check if the window explicitly disables decorations (e.g. CSD, borderless, or merged titlebars like JetBrains / Chrome)
                let has_no_decorations = if let Some((32, mbytes)) = get_property(win, motif_hints_atom) {
                    if mbytes.len() >= chunk_size * 3 {
                        let mut flags_buf = [0u8; std::mem::size_of::<c_ulong>()];
                        flags_buf.copy_from_slice(&mbytes[0..chunk_size]);
                        let flags = c_ulong::from_ne_bytes(flags_buf);

                        let mut decor_buf = [0u8; std::mem::size_of::<c_ulong>()];
                        decor_buf.copy_from_slice(&mbytes[chunk_size * 2..chunk_size * 3]);
                        let decorations = c_ulong::from_ne_bytes(decor_buf);

                        // MWM_HINTS_DECORATIONS = (1 << 1)
                        (flags & (1 << 1) != 0) && decorations == 0
                    } else {
                        false
                    }
                } else {
                    false
                };

                // Get frame extents (titlebar and window manager border decoration) if decorations are enabled
                let (ext_left, ext_right, ext_top, ext_bottom) = if !has_no_decorations {
                    if let Some((32, ext_bytes)) = get_property(win, frame_extents_atom) {
                        if ext_bytes.len() >= chunk_size * 4 {
                            let mut vals = [0u32; 4];
                            for (i, val) in vals.iter_mut().enumerate() {
                                let mut buf = [0u8; std::mem::size_of::<c_ulong>()];
                                buf.copy_from_slice(&ext_bytes[i * chunk_size..(i + 1) * chunk_size]);
                                *val = c_ulong::from_ne_bytes(buf) as u32;
                            }
                            (vals[0], vals[1], vals[2], vals[3])
                        } else {
                            (0, 0, 0, 0)
                        }
                    } else {
                        (0, 0, 0, 0)
                    }
                } else {
                    (0, 0, 0, 0)
                };

                let mut win_x = x - ext_left as i32;
                let mut win_y = y - ext_top as i32;
                let mut win_w = attr.width as u32 + ext_left + ext_right;
                let mut win_h = attr.height as u32 + ext_top + ext_bottom;

                // GTK client-side decorations include transparent drop-shadow pixels in
                // the X11 geometry. _GTK_FRAME_EXTENTS describes those invisible margins.
                if let Some((32, bytes)) = get_property(win, gtk_frame_extents_atom) {
                    if bytes.len() >= chunk_size * 4 {
                        let mut inset = [0u32; 4];
                        for (i, value) in inset.iter_mut().enumerate() {
                            let mut buf = [0u8; std::mem::size_of::<c_ulong>()];
                            buf.copy_from_slice(&bytes[i * chunk_size..(i + 1) * chunk_size]);
                            *value = c_ulong::from_ne_bytes(buf) as u32;
                        }
                        if inset[0] + inset[1] < win_w / 2 && inset[2] + inset[3] < win_h / 2 {
                            win_x += inset[0] as i32;
                            win_y += inset[2] as i32;
                            win_w -= inset[0] + inset[1];
                            win_h -= inset[2] + inset[3];
                        }
                    }
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

                let display_title = if !title.is_empty() {
                    title
                } else if attr.width >= 50 && attr.height >= 50 {
                    "Window".to_string()
                } else {
                    String::new()
                };

                if !display_title.is_empty() {
                    windows.push(WindowBounds {
                        title: display_title,
                        pid: window_pid,
                        x: win_x,
                        y: win_y,
                        width: win_w,
                        height: win_h,
                    });
                }
            }
        }

        unsafe {
            (api.XCloseDisplay)(display);
        }
        windows
    }

    pub fn activate_x11_window_for_current_process(expected_title: Option<&str>) {
        let current_pid = std::process::id();
        let Ok(api) = xlib::Xlib::open() else {
            return;
        };
        let display = unsafe { (api.XOpenDisplay)(ptr::null()) };
        if display.is_null() {
            return;
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
        let pid_atom = atom("_NET_WM_PID");
        let active_win_atom = atom("_NET_ACTIVE_WINDOW");

        let name_atom = atom("_NET_WM_NAME");

        if let Some((32, bytes)) = get_property(root, client_list_atom) {
            let chunk_size = std::mem::size_of::<c_ulong>();
            for chunk in bytes.chunks_exact(chunk_size) {
                let mut native = [0u8; std::mem::size_of::<c_ulong>()];
                native.copy_from_slice(chunk);
                let win = c_ulong::from_ne_bytes(native) as xlib::Window;

                if let Some((32, pid_bytes)) = get_property(win, pid_atom) {
                    if pid_bytes.len() >= chunk_size {
                        let mut pnative = [0u8; std::mem::size_of::<c_ulong>()];
                        pnative.copy_from_slice(&pid_bytes[..chunk_size]);
                        let win_pid = c_ulong::from_ne_bytes(pnative) as u32;
                        if win_pid == current_pid {
                            if let Some(target) = expected_title {
                                let title = if let Some((8, name_bytes)) = get_property(win, name_atom) {
                                    String::from_utf8_lossy(&name_bytes).trim_end_matches('\0').to_string()
                                } else {
                                    String::new()
                                };
                                if !title.contains(target) {
                                    continue;
                                }
                            }

                            let mut xev: xlib::XClientMessageEvent = unsafe { std::mem::zeroed() };
                            xev.type_ = xlib::ClientMessage;
                            xev.window = win;
                            xev.message_type = active_win_atom;
                            xev.format = 32;
                            xev.data.set_long(0, 2); // 2 = source: pager or direct user action
                            xev.data.set_long(1, 0); // timestamp
                            xev.data.set_long(2, 0);

                            let mut event: xlib::XEvent = xev.into();
                            let mask = xlib::SubstructureRedirectMask | xlib::SubstructureNotifyMask;
                            unsafe {
                                (api.XSendEvent)(display, root, xlib::False, mask, &mut event);
                                (api.XMapRaised)(display, win);
                                (api.XFlush)(display);
                                (api.XCloseDisplay)(display);
                            }
                            return;
                        }
                    }
                }
            }
        }

        unsafe {
            (api.XCloseDisplay)(display);
        }
    }
}

#[cfg(target_os = "linux")]
pub use linux_x11::activate_x11_window_for_current_process as activate_x11_window;

#[cfg(not(target_os = "linux"))]
pub fn activate_x11_window(_expected_title: Option<&str>) {}

/// Filter and translate UI elements relative to a cropped region.
pub fn filter_elements_for_crop(
    elements: &[DetectedUiElement],
    crop_x: f64,
    crop_y: f64,
    crop_w: f64,
    crop_h: f64,
) -> Vec<DetectedUiElement> {
    elements
        .iter()
        .filter_map(|el| {

            // Exclude unmapped ghost elements that had absolute (0, 0) coordinates
            if el.x.abs() < 1.0 && el.y.abs() < 1.0 && (el.role == "menuitem" || el.name.is_none()) {
                return None;
            }

            // Calculate translated coordinates inside the cropped image
            let new_x = el.x - crop_x;
            let new_y = el.y - crop_y;

            // Element must be inside the cropped canvas
            if new_x < -4.0 || new_y < -4.0 || new_x >= crop_w || new_y >= crop_h {
                return None;
            }

            // Exclude elements that are almost entirely outside the crop
            let inter_w = (new_x + el.width).min(crop_w) - new_x.max(0.0);
            let inter_h = (new_y + el.height).min(crop_h) - new_y.max(0.0);
            if inter_w < 6.0 || inter_h < 6.0 {
                return None;
            }

            // Exclude root background bounds covering full crop
            if inter_w >= crop_w - 4.0 && inter_h >= crop_h - 4.0 {
                return None;
            }

            Some(DetectedUiElement {
                role: el.role.clone(),
                name: el.name.clone(),
                window_id: None,
                pid: None,
                x: new_x.max(0.0),
                y: new_y.max(0.0),
                width: inter_w,
                height: inter_h,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use xa11y::{App, AppExt};

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
                window_id: None, pid: None,
                x: 80.0,
                y: 80.0,
                width: 100.0,
                height: 100.0,
            },
            DetectedUiElement {
                role: "button".into(),
                name: Some("OK".into()),
                window_id: None, pid: None,
                x: 100.0,
                y: 100.0,
                width: 50.0,
                height: 30.0,
            },
            DetectedUiElement {
                role: "button".into(),
                name: Some("Cancel".into()),
                window_id: None, pid: None,
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

    #[test]
    fn matches_accessibility_content_inside_decorated_window() {
        let target = DetectedUiElement {
            role: "window".into(),
            name: Some("アプリセンター".into()),
            window_id: None, pid: None,
            x: 24.0,
            y: 27.0,
            width: 1384.0,
            height: 904.0,
        };
        assert!(window_overlaps_target(50.0, 50.0, 1332.0, 852.0, &target));
        assert!(!window_overlaps_target(0.0, 0.0, 1920.0, 1080.0, &target));
        assert!(!window_overlaps_target(1500.0, 50.0, 300.0, 300.0, &target));
        let calendar = DetectedUiElement { x: 100.0, y: 100.0, width: 646.0, height: 600.0, ..target };
        assert!(window_overlaps_target(0.0, 0.0, 646.0, 600.0, &calendar));
    }

    #[test]
    fn window_capture_selects_only_exact_target() {
        let background = DetectedUiElement {
            role: "window".into(), name: Some("background".into()),
            window_id: None, pid: None,
            x: 0.0, y: 0.0, width: 1200.0, height: 900.0,
        };
        let foreground = DetectedUiElement {
            role: "window".into(), name: Some("calendar".into()),
            window_id: None, pid: None,
            x: 100.0, y: 100.0, width: 646.0, height: 600.0,
        };
        let windows = vec![background, foreground.clone()];
        assert_eq!(select_window_for_capture(&windows, Some((100.0, 100.0, 646.0, 600.0))), Some(foreground));
        assert_eq!(select_window_for_capture(&windows, Some((110.0, 110.0, 300.0, 300.0))), None);
        assert_eq!(select_window_for_capture(&windows, None), None);
    }

    #[test]
    fn captured_window_identity_rejects_another_window_in_same_process() {
        let target = DetectedUiElement {
            role: "window".into(), name: Some("Editor".into()),
            window_id: Some("hwnd:0x1234".into()), pid: Some(42),
            x: 100.0, y: 100.0, width: 800.0, height: 600.0,
        };
        assert!(target_accepts_window(&target, Some("hwnd:0x1234"), Some(42)));
        assert!(!target_accepts_window(&target, Some("hwnd:0x5678"), Some(42)));
        assert!(!target_accepts_window(&target, Some("hwnd:0x1234"), Some(43)));
    }

    #[test]
    fn test_capture_desktop_windows() {
        let windows = capture_desktop_windows(0, 0);
        assert!(!windows.is_empty(), "Should capture at least one desktop window");

        // Verify that root window matching full canvas is excluded, but sub-windows are retained
        let full_canvas_window = vec![DetectedUiElement {
            role: "window".into(),
            name: Some("Root Screen Window".into()),
            window_id: None, pid: None,
            x: 0.0,
            y: 0.0,
            width: 1920.0,
            height: 1080.0,
        }];
        let cropped = filter_elements_for_crop(&full_canvas_window, 0.0, 0.0, 1920.0, 1080.0);
        assert!(cropped.is_empty(), "Exact full canvas root window must be excluded");
    }
}
