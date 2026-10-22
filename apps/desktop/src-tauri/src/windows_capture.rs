//! Native desktop enumeration also works for applications without UI Automation (e.g. Java IDEs).
use super::DetectedUiElement;
use windows_sys::Win32::{
    Foundation::{HWND, LPARAM, RECT},
    Graphics::Dwm::{DWMWA_CLOAKED, DWMWA_EXTENDED_FRAME_BOUNDS, DwmGetWindowAttribute},
    UI::WindowsAndMessaging::{
        EnumWindows, GWL_EXSTYLE, GetClassNameW, GetWindowLongPtrW, GetWindowRect,
        GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId, IsIconic, IsWindowVisible,
        WS_EX_TOOLWINDOW,
    },
};

pub fn capture_windows(origin_x: i32, origin_y: i32) -> Vec<DetectedUiElement> {
    let mut windows = Vec::<DetectedUiElement>::new();
    // EnumWindows visits topmost first. The overlay expects bottommost first.
    unsafe {
        EnumWindows(Some(collect_window), &mut windows as *mut _ as LPARAM);
    }
    windows.reverse();
    for window in &mut windows {
        window.x -= origin_x as f64;
        window.y -= origin_y as f64;
    }
    windows
}

unsafe extern "system" fn collect_window(hwnd: HWND, data: LPARAM) -> i32 {
    // The callback is synchronous and `data` points to the vector owned by capture_windows.
    unsafe {
        if IsWindowVisible(hwnd) == 0 || IsIconic(hwnd) != 0 {
            return 1;
        }
        let mut pid = 0;
        GetWindowThreadProcessId(hwnd, &mut pid);
        if pid == std::process::id()
            || GetWindowLongPtrW(hwnd, GWL_EXSTYLE) & WS_EX_TOOLWINDOW as isize != 0
        {
            return 1;
        }
        let mut cloaked = 0u32;
        DwmGetWindowAttribute(hwnd, DWMWA_CLOAKED as u32, &mut cloaked as *mut _ as _, 4);
        if cloaked != 0 {
            return 1;
        }

        let mut rect = RECT::default();
        if DwmGetWindowAttribute(
            hwnd,
            DWMWA_EXTENDED_FRAME_BOUNDS as u32,
            &mut rect as *mut _ as _,
            std::mem::size_of::<RECT>() as u32,
        ) < 0
            && GetWindowRect(hwnd, &mut rect) == 0
        {
            return 1;
        }
        if rect.right <= rect.left || rect.bottom <= rect.top {
            return 1;
        }
        let length = GetWindowTextLengthW(hwnd);
        let mut title = vec![0u16; length.max(0) as usize + 1];
        let length = GetWindowTextW(hwnd, title.as_mut_ptr(), title.len() as i32);
        let name = String::from_utf16_lossy(&title[..length.max(0) as usize]);
        // Shell desktop surfaces must not become capture targets, even with localized titles.
        let mut class_name = [0u16; 256];
        let class_length = GetClassNameW(hwnd, class_name.as_mut_ptr(), class_name.len() as i32);
        let class_name = String::from_utf16_lossy(&class_name[..class_length.max(0) as usize]);
        if matches!(class_name.as_str(), "Progman" | "WorkerW") {
            return 1;
        }
        let windows = &mut *(data as *mut Vec<DetectedUiElement>);
        windows.push(DetectedUiElement {
            role: "window".into(),
            name: Some(name),
            window_id: Some(format!("hwnd:0x{:x}", hwnd as usize)),
            pid: Some(pid),
            x: rect.left as f64,
            y: rect.top as f64,
            width: (rect.right - rect.left) as f64,
            height: (rect.bottom - rect.top) as f64,
        });
    }
    1
}
