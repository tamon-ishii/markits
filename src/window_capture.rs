use serde::Serialize;

pub fn is_wayland_session() -> bool {
    #[cfg(target_os = "linux")]
    {
        match std::env::var("XDG_SESSION_TYPE").ok().as_deref() {
            Some(session) if session.eq_ignore_ascii_case("wayland") => true,
            Some(session) if session.eq_ignore_ascii_case("x11") => false,
            _ => std::env::var("WAYLAND_DISPLAY").is_ok_and(|display| !display.is_empty()),
        }
    }
    #[cfg(not(target_os = "linux"))]
    {
        false
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct WindowInfo {
    pub id: String,
    pub title: String,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

pub(crate) fn save_image_with_uimap(
    img: &image::DynamicImage,
    bounds: Option<(f64, f64, f64, f64)>,
    destination: &std::path::Path,
    include_uimap: bool,
    target_window_id: Option<&str>,
) -> Result<(), String> {
    let mut png_bytes = Vec::new();
    img.write_to(
        &mut std::io::Cursor::new(&mut png_bytes),
        image::ImageFormat::Png,
    )
    .map_err(|error| error.to_string())?;

    // Attempt UI element detection and embed image element metadata via MarkIts
    let detected = if let Some(window_id) = target_window_id {
        // Accessibility uses platform-specific identities (e.g. hwnd:0x... on
        // Windows). Share the exact-window lookup used for operation recording;
        // geometry matching fails on browser frames and can select another app.
        eprintln!("native capture: resolving accessibility window");
        let target = crate::semantic_target::window_context(window_id)?;
        eprintln!("native capture: reading accessibility elements");
        let elements = crate::ui_elements::capture_desktop_detailed_elements_for_window(0, 0, &target);
        eprintln!("native capture: accessibility elements read");
        elements
    } else if include_uimap {
        crate::ui_elements::capture_desktop_detailed_elements(0, 0, bounds)
    } else {
        Vec::new()
    };
    let final_bytes = if !detected.is_empty() {
        let ui_elements: Vec<crate::UiElement> = if let Some((bx, by, bw, bh)) = bounds {
            let mut elements = crate::ui_elements::filter_elements_for_crop(&detected, bx, by, bw, bh);
            scale_capture_elements(&mut elements, img.width(), img.height(), bw, bh);
            elements.into_iter().map(Into::into).collect()
        } else {
            detected.into_iter().map(Into::into).collect()
        };
        crate::raster::embed_png_uimap(&png_bytes, &ui_elements).unwrap_or(png_bytes)
    } else {
        png_bytes
    };
    eprintln!("native capture: writing PNG");
    std::fs::write(destination, final_bytes).map_err(|error| error.to_string())
}

#[cfg(target_os = "linux")]
mod linux {
    use super::{is_wayland_session, WindowInfo};
    use ashpd::desktop::screenshot::{AvailableTargets, Screenshot};
    use image::{ImageBuffer, Rgb};
    use std::ffi::{CStr, CString};
    use std::os::raw::{c_int, c_long, c_uchar, c_ulong, c_void};
    use std::path::Path;
    use std::ptr;
    use std::sync::{Mutex, MutexGuard};
    use std::thread;
    use std::time::Duration;
    use x11_dl::xlib;

    const PORTAL_WINDOW_ID: &str = "portal";

    type ErrorHandler = unsafe extern "C" fn(*mut xlib::Display, *mut xlib::XErrorEvent) -> c_int;
    // Xlib's handler is process-wide. Serialize our connections and delegate errors
    // on toolkit-owned displays to the previous handler.
    static CONNECTION_LOCK: Mutex<()> = Mutex::new(());
    static ERROR_HANDLER: Mutex<(usize, Option<ErrorHandler>)> = Mutex::new((0, None));

    unsafe extern "C" fn handle_error(
        display: *mut xlib::Display,
        event: *mut xlib::XErrorEvent,
    ) -> c_int {
        let (capture_display, previous) = *ERROR_HANDLER
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if display as usize == capture_display && (*event).error_code == xlib::BadWindow {
            // A client can disappear between reading _NET_CLIENT_LIST and
            // inspecting it. XGetWindowAttributes then returns zero.
            return 0;
        }
        previous.map_or(0, |handler| handler(display, event))
    }

    fn capture_portal(
        inset: u32,
        destination: &Path,
        include_uimap: bool,
    ) -> Result<WindowInfo, String> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|error| format!("Could not start screenshot portal: {error}"))?;
        let uri = runtime.block_on(async {
            let response = Screenshot::request()
                .interactive(true)
                .target(AvailableTargets::Window)
                .send()
                .await
                .map_err(|error| format!("Screenshot portal request failed: {error}"))?
                .response()
                .map_err(|error| format!("Screenshot was cancelled or denied: {error}"))?;
            Ok::<String, String>(response.uri().to_string())
        })?;
        let source = url::Url::parse(&uri)
            .map_err(|error| format!("Invalid screenshot URI: {error}"))?
            .to_file_path()
            .map_err(|_| "Screenshot portal did not return a local file")?;
        let image = image::open(&source)
            .map_err(|error| format!("Could not read portal screenshot: {error}"))?;
        let crop = inset.checked_mul(2).ok_or("Inset is too large")?;
        let width = image
            .width()
            .checked_sub(crop)
            .ok_or("Inset exceeds captured image width")?;
        let height = image
            .height()
            .checked_sub(crop)
            .ok_or("Inset exceeds captured image height")?;
        if width == 0 || height == 0 {
            return Err("Capture area is empty".into());
        }
        let cropped = image.crop_imm(inset, inset, width, height);
        super::save_image_with_uimap(&cropped, None, destination, include_uimap, None)?;
        Ok(WindowInfo {
            id: PORTAL_WINDOW_ID.into(),
            title: "Selected window".into(),
            x: 0,
            y: 0,
            width,
            height,
        })
    }

    struct DisplayConnection {
        api: xlib::Xlib,
        display: *mut xlib::Display,
        root: xlib::Window,
        previous_handler: Option<ErrorHandler>,
        _lock: MutexGuard<'static, ()>,
    }

    impl Drop for DisplayConnection {
        fn drop(&mut self) {
            unsafe {
                (self.api.XCloseDisplay)(self.display);
                (self.api.XSetErrorHandler)(self.previous_handler);
            }
            *ERROR_HANDLER
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner()) = (0, None);
        }
    }

    impl DisplayConnection {
        fn open() -> Result<Self, String> {
            let lock = CONNECTION_LOCK
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let api = xlib::Xlib::open().map_err(|error| format!("X11 is unavailable: {error}"))?;
            let display = unsafe { (api.XOpenDisplay)(ptr::null()) };
            if display.is_null() {
                return Err("Cannot open the X11 display. Window capture currently requires a Linux X11 session".into());
            }
            let root = unsafe { (api.XDefaultRootWindow)(display) };
            let mut handler_state = ERROR_HANDLER
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let previous_handler = unsafe { (api.XSetErrorHandler)(Some(handle_error)) };
            *handler_state = (display as usize, previous_handler);
            drop(handler_state);
            Ok(Self {
                api,
                display,
                root,
                previous_handler,
                _lock: lock,
            })
        }

        fn atom(&self, name: &str) -> xlib::Atom {
            let name = CString::new(name).expect("static atom name");
            unsafe { (self.api.XInternAtom)(self.display, name.as_ptr(), xlib::False) }
        }

        fn property(&self, window: xlib::Window, name: &str) -> Option<(c_int, Vec<u8>)> {
            let atom = self.atom(name);
            let mut actual_type = 0;
            let mut format = 0;
            let mut count = 0;
            let mut after = 0;
            let mut data: *mut c_uchar = ptr::null_mut();
            let status = unsafe {
                (self.api.XGetWindowProperty)(
                    self.display,
                    window,
                    atom,
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
                    unsafe {
                        (self.api.XFree)(data.cast::<c_void>());
                    }
                }
                return None;
            }
            let element_size = if format == 32 {
                std::mem::size_of::<c_ulong>()
            } else {
                1
            };
            let bytes =
                unsafe { std::slice::from_raw_parts(data, count as usize * element_size).to_vec() };
            unsafe {
                (self.api.XFree)(data.cast::<c_void>());
            }
            Some((format, bytes))
        }

        fn client_windows(&self) -> Result<Vec<xlib::Window>, String> {
            let (format, bytes) = self
                .property(self.root, "_NET_CLIENT_LIST")
                .ok_or("The window manager does not expose its window list")?;
            if format != 32 {
                return Err("Unexpected X11 window list format".into());
            }
            let width = std::mem::size_of::<c_ulong>();
            Ok(bytes
                .chunks_exact(width)
                .map(|chunk| {
                    let mut native = [0u8; std::mem::size_of::<c_ulong>()];
                    native.copy_from_slice(chunk);
                    c_ulong::from_ne_bytes(native)
                })
                .collect())
        }

        fn title(&self, window: xlib::Window) -> Option<String> {
            if let Some((8, bytes)) = self.property(window, "_NET_WM_NAME") {
                let text = String::from_utf8_lossy(&bytes)
                    .trim_end_matches('\0')
                    .to_string();
                if !text.is_empty() {
                    return Some(text);
                }
            }
            let mut raw = ptr::null_mut();
            let success = unsafe { (self.api.XFetchName)(self.display, window, &mut raw) };
            if success == 0 || raw.is_null() {
                return None;
            }
            let text = unsafe { CStr::from_ptr(raw).to_string_lossy().into_owned() };
            unsafe {
                (self.api.XFree)(raw.cast::<c_void>());
            }
            (!text.is_empty()).then_some(text)
        }

        fn info(&self, window: xlib::Window) -> Option<WindowInfo> {
            let mut attr = std::mem::MaybeUninit::<xlib::XWindowAttributes>::uninit();
            if unsafe { (self.api.XGetWindowAttributes)(self.display, window, attr.as_mut_ptr()) }
                == 0
            {
                return None;
            }
            let attr = unsafe { attr.assume_init() };
            if attr.map_state != xlib::IsViewable || attr.width < 1 || attr.height < 1 {
                return None;
            }
            let mut x = 0;
            let mut y = 0;
            let mut child = 0;
            if unsafe {
                (self.api.XTranslateCoordinates)(
                    self.display,
                    window,
                    self.root,
                    0,
                    0,
                    &mut x,
                    &mut y,
                    &mut child,
                )
            } == 0
            {
                return None;
            }
            Some(WindowInfo {
                id: format!("0x{window:x}"),
                title: self.title(window)?,
                x,
                y,
                width: attr.width as u32,
                height: attr.height as u32,
            })
        }

        fn send_message(&self, window: xlib::Window, name: &str, data: [c_long; 5]) {
            let message = xlib::XClientMessageEvent {
                type_: xlib::ClientMessage,
                serial: 0,
                send_event: xlib::True,
                display: self.display,
                window,
                message_type: self.atom(name),
                format: 32,
                data: xlib::ClientMessageData::from(data),
            };
            let mut event = xlib::XEvent {
                client_message: message,
            };
            unsafe {
                (self.api.XSendEvent)(
                    self.display,
                    self.root,
                    xlib::False,
                    xlib::SubstructureRedirectMask | xlib::SubstructureNotifyMask,
                    &mut event,
                );
                (self.api.XFlush)(self.display);
            }
        }

        fn set_above(&self, window: xlib::Window, add: bool) {
            self.send_message(
                window,
                "_NET_WM_STATE",
                [
                    if add { 1 } else { 0 },
                    self.atom("_NET_WM_STATE_ABOVE") as c_long,
                    0,
                    2,
                    0,
                ],
            );
        }

        fn activate(&self, window: xlib::Window) {
            self.send_message(window, "_NET_ACTIVE_WINDOW", [2, 0, 0, 0, 0]);
            unsafe {
                (self.api.XRaiseWindow)(self.display, window);
                (self.api.XSetInputFocus)(
                    self.display,
                    window,
                    xlib::RevertToParent,
                    xlib::CurrentTime,
                );
                (self.api.XFlush)(self.display);
            }
        }
    }

    fn channel(pixel: u64, mask: u64) -> u8 {
        if mask == 0 {
            return 0;
        }
        let value = (pixel & mask) >> mask.trailing_zeros();
        let max = mask >> mask.trailing_zeros();
        ((value * 255 + max / 2) / max) as u8
    }

    fn raw_ximage_pixel(
        data: &[u8],
        stride: usize,
        bytes_per_pixel: usize,
        byte_order: c_int,
        x: usize,
        y: usize,
    ) -> Option<u64> {
        if !(1..=4).contains(&bytes_per_pixel) {
            return None;
        }
        let start = y
            .checked_mul(stride)?
            .checked_add(x.checked_mul(bytes_per_pixel)?)?;
        let bytes = data.get(start..start.checked_add(bytes_per_pixel)?)?;
        Some(if byte_order == xlib::LSBFirst {
            bytes.iter().enumerate().fold(0u64, |value, (index, byte)| {
                value | (u64::from(*byte) << (index * 8))
            })
        } else if byte_order == xlib::MSBFirst {
            bytes
                .iter()
                .fold(0u64, |value, byte| (value << 8) | u64::from(*byte))
        } else {
            return None;
        })
    }

    pub fn list_windows() -> Result<Vec<WindowInfo>, String> {
        if is_wayland_session() {
            return Ok(vec![WindowInfo {
                id: PORTAL_WINDOW_ID.into(),
                title: "Choose a window in the system screenshot dialog".into(),
                x: 0,
                y: 0,
                width: 1,
                height: 1,
            }]);
        }
        let conn = DisplayConnection::open()?;
        let mut windows: Vec<_> = conn
            .client_windows()?
            .into_iter()
            .filter_map(|id| conn.info(id))
            .collect();
        windows.sort_by(|a, b| a.title.to_lowercase().cmp(&b.title.to_lowercase()));
        Ok(windows)
    }

    pub fn window_process_ids() -> Result<Vec<(String, u32)>, String> {
        let conn = DisplayConnection::open()?;
        Ok(conn.client_windows()?.into_iter().filter_map(|id| {
            let (format, bytes) = conn.property(id, "_NET_WM_PID")?;
            if format != 32 || bytes.len() < std::mem::size_of::<c_ulong>() { return None; }
            let mut value = [0u8; std::mem::size_of::<c_ulong>()];
            value.copy_from_slice(&bytes[..std::mem::size_of::<c_ulong>()]);
            Some((format!("0x{id:x}"), c_ulong::from_ne_bytes(value) as u32))
        }).collect())
    }

    pub fn is_window_active(window_id: &str) -> Result<bool, String> {
        let id = u64::from_str_radix(window_id.trim_start_matches("0x"), 16)
            .map_err(|_| format!("Invalid X11 window ID: {window_id}"))? as xlib::Window;
        let conn = DisplayConnection::open()?;
        let (format, bytes) = conn.property(conn.root, "_NET_ACTIVE_WINDOW")
            .ok_or("The window manager does not expose its active window")?;
        let size = std::mem::size_of::<c_ulong>();
        if format != 32 || bytes.len() < size { return Err("Unexpected active window format".into()); }
        let mut value = [0u8; std::mem::size_of::<c_ulong>()];
        value.copy_from_slice(&bytes[..size]);
        Ok(c_ulong::from_ne_bytes(value) == id)
    }

    pub fn activate_window(window_id: &str) -> Result<WindowInfo, String> {
        if is_wayland_session() {
            return Err("Wayland does not allow ModuleLoom to activate arbitrary windows".into());
        }
        let id = u64::from_str_radix(window_id.trim_start_matches("0x"), 16)
            .map_err(|_| format!("Invalid X11 window ID: {window_id}"))?
            as xlib::Window;
        let conn = DisplayConnection::open()?;
        if !conn.client_windows()?.contains(&id) {
            return Err("Selected window is no longer open".into());
        }
        conn.activate(id);
        thread::sleep(Duration::from_millis(150));
        conn.info(id)
            .ok_or("The selected window is not visible".into())
    }

    pub fn close_window(window_id: &str) -> Result<(), String> {
        if is_wayland_session() {
            return Ok(());
        }
        let id = u64::from_str_radix(window_id.trim_start_matches("0x"), 16)
            .map_err(|_| format!("Invalid X11 window ID: {window_id}"))?
            as xlib::Window;
        let conn = DisplayConnection::open()?;
        if conn.client_windows()?.contains(&id) {
            conn.send_message(id, "_NET_CLOSE_WINDOW", [0, 2, 0, 0, 0]);
            thread::sleep(Duration::from_millis(150));
        }
        Ok(())
    }

    pub fn capture_window(
        window_id: &str,
        inset: u32,
        destination: &Path,
        include_uimap: bool,
        target_window_id: Option<&str>,
    ) -> Result<WindowInfo, String> {
        if is_wayland_session() {
            if window_id != PORTAL_WINDOW_ID {
                return Err("Choose the portal window option on Wayland".into());
            }
            return capture_portal(inset, destination, include_uimap);
        }
        let id = u64::from_str_radix(window_id.trim_start_matches("0x"), 16)
            .map_err(|_| format!("Invalid X11 window ID: {window_id}"))?
            as xlib::Window;
        let conn = DisplayConnection::open()?;
        if !conn.client_windows()?.contains(&id) {
            return Err("Selected window is no longer open".into());
        }
        let state_before = conn.property(id, "_NET_WM_STATE");
        let above = conn.atom("_NET_WM_STATE_ABOVE");
        let already_above = state_before.as_ref().is_some_and(|(format, bytes)| {
            *format == 32
                && bytes
                    .chunks_exact(std::mem::size_of::<c_ulong>())
                    .any(|chunk| {
                        let mut native = [0u8; std::mem::size_of::<c_ulong>()];
                        native.copy_from_slice(chunk);
                        c_ulong::from_ne_bytes(native) == above
                    })
        });
        if !already_above {
            conn.set_above(id, true);
        }
        conn.activate(id);
        thread::sleep(Duration::from_millis(400));
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        let result = loop {
            let frame = capture_visible(&conn, id, inset);
            if frame.as_ref().is_err_and(|error| error == "Selected window has not finished painting")
                && std::time::Instant::now() < deadline {
                thread::sleep(Duration::from_millis(250));
                continue;
            }
            break frame;
        };
        if !already_above {
            conn.set_above(id, false);
        }
        // Metadata lookup opens another DisplayConnection. Release the X11
        // connection lock before resolving the exact accessibility window.
        drop(conn);
        let (info, picture) = result?;
        let margin = inset as i32;
        super::save_image_with_uimap(
            &picture,
            Some(((info.x + margin) as f64, (info.y + margin) as f64,
                picture.width() as f64, picture.height() as f64)),
            destination,
            include_uimap,
            target_window_id,
        )?;
        Ok(info)
    }

    // A compositor maintains a separate pixmap for each window. Reading it
    // avoids capturing whatever happens to overlap the target on the desktop.
    struct WindowPixmap { _connection: x11rb::rust_connection::RustConnection, drawable: u32 }

    impl WindowPixmap {
        fn open(window: u32) -> Option<Self> {
            use x11rb::connection::Connection;
            use x11rb::protocol::composite::ConnectionExt;
            let (connection, _) = x11rb::connect(None).ok()?;
            let drawable = connection.generate_id().ok()?;
            connection.composite_name_window_pixmap(window, drawable).ok()?.check().ok()?;
            Some(Self { _connection: connection, drawable })
        }
    }

    fn read_region_pixels(conn: &DisplayConnection, id: xlib::Window,
        offset_x: i32, offset_y: i32, width: u32, height: u32) -> Result<image::RgbImage, String> {
        let pixmap = WindowPixmap::open(id as u32);
        let drawable = pixmap.as_ref().map_or(id, |pixmap| pixmap.drawable as xlib::Drawable);
        let raw = unsafe {
            (conn.api.XGetImage)(
                conn.display,
                drawable,
                offset_x,
                offset_y,
                width,
                height,
                !0,
                xlib::ZPixmap,
            )
        };
        if raw.is_null() {
            return Err("Could not capture the window pixels".into());
        }
        let mut pixels = Vec::with_capacity(width as usize * height as usize * 3);
        // XGetImage on a pixmap has no visual and returns zero RGB masks.
        // Decode its pixels using the visual of the source window.
        let masks = unsafe {
            let mut attributes = std::mem::MaybeUninit::<xlib::XWindowAttributes>::uninit();
            if (conn.api.XGetWindowAttributes)(conn.display, id, attributes.as_mut_ptr()) != 0 {
                let visual = attributes.assume_init().visual;
                if !visual.is_null() {
                    ((*visual).red_mask as u64, (*visual).green_mask as u64, (*visual).blue_mask as u64)
                } else { ((*raw).red_mask as u64, (*raw).green_mask as u64, (*raw).blue_mask as u64) }
            } else { ((*raw).red_mask as u64, (*raw).green_mask as u64, (*raw).blue_mask as u64) }
        };
        let raw_layout = unsafe {
            let stride = usize::try_from((*raw).bytes_per_line).ok();
            let bytes_per_pixel = usize::try_from((*raw).bits_per_pixel)
                .ok()
                .filter(|bits| *bits > 0 && *bits % 8 == 0)
                .map(|bits| bits / 8);
            stride.zip(bytes_per_pixel).and_then(|(stride, bytes)| {
                stride.checked_mul(height as usize).and_then(|len| {
                    (!(*raw).data.is_null()).then(|| {
                        (
                            std::slice::from_raw_parts((*raw).data.cast::<u8>(), len),
                            stride,
                            bytes,
                            (*raw).byte_order,
                        )
                    })
                })
            })
        };
        if let Some((data, stride, 4, _)) = raw_layout.filter(|(_, _, _, order)|
            *order == xlib::LSBFirst && masks == (0xff0000, 0xff00, 0xff)) {
            for row in data.chunks_exact(stride).take(height as usize) {
                for pixel in row.chunks_exact(4).take(width as usize) {
                    pixels.extend_from_slice(&[pixel[2], pixel[1], pixel[0]]);
                }
            }
        } else {
        for row in 0..height {
            for col in 0..width {
                let pixel = raw_layout
                    .and_then(|(data, stride, bytes, order)| {
                        raw_ximage_pixel(data, stride, bytes, order, col as usize, row as usize)
                    })
                    .unwrap_or_else(|| unsafe {
                        (conn.api.XGetPixel)(raw, col as c_int, row as c_int)
                    } as u64);
                pixels.extend([
                    channel(pixel, masks.0),
                    channel(pixel, masks.1),
                    channel(pixel, masks.2),
                ]);
            }
        }
        }
        unsafe {
            (conn.api.XDestroyImage)(raw);
        }
        ImageBuffer::<Rgb<u8>, _>::from_raw(width, height, pixels).ok_or("Invalid screenshot dimensions".into())
    }

    pub fn read_window_pixels(window_id: &str, region: Option<(u32, u32, u32, u32)>) -> Result<image::RgbImage, String> {
        if is_wayland_session() { return Err("Wayland requires explicit portal capture".into()); }
        let id = u64::from_str_radix(window_id.trim_start_matches("0x"), 16)
            .map_err(|_| "Invalid window ID")? as xlib::Window;
        let conn = DisplayConnection::open()?;
        if !conn.client_windows()?.contains(&id) { return Err("Selected window is no longer open".into()); }
        let info = conn.info(id).ok_or("Selected window is not visible")?;
        let (x, y, width, height) = region.unwrap_or((0, 0, info.width, info.height));
        if width == 0 || height == 0 || x.checked_add(width).is_none_or(|right| right > info.width)
            || y.checked_add(height).is_none_or(|bottom| bottom > info.height) {
            return Err("Capture region is outside the selected window".into());
        }
        read_region_pixels(&conn, id, x as i32, y as i32, width, height)
    }

    fn capture_visible(
        conn: &DisplayConnection,
        id: xlib::Window,
        inset: u32,
    ) -> Result<(WindowInfo, image::DynamicImage), String> {
        let info = conn.info(id).ok_or("The selected window is not visible")?;
        let margin = inset as i32;
        let x = info.x + margin;
        let y = info.y + margin;
        let width = info
            .width
            .checked_sub(inset.saturating_mul(2))
            .ok_or("Inset exceeds window width")?;
        let height = info
            .height
            .checked_sub(inset.saturating_mul(2))
            .ok_or("Inset exceeds window height")?;
        if width == 0 || height == 0 {
            return Err("Capture area is empty".into());
        }
        let screen = unsafe { (conn.api.XDefaultScreen)(conn.display) };
        let screen_width = unsafe { (conn.api.XDisplayWidth)(conn.display, screen) };
        let screen_height = unsafe { (conn.api.XDisplayHeight)(conn.display, screen) };
        if x < 0 || y < 0 || x + width as i32 > screen_width || y + height as i32 > screen_height {
            return Err("The entire window must be visible on the screen before capture".into());
        }
        let picture = read_region_pixels(conn, id, margin, margin, width, height)?;
        let pixels = picture.as_raw();
        // GTK/WebKit can show a uniform background before its UI is ready.
        let background = &pixels[..3];
        if background.iter().max().unwrap() - background.iter().min().unwrap() <= 4
            && pixels.chunks_exact(3).all(|pixel| pixel == background) {
            return Err("Selected window has not finished painting".into());
        }
        Ok((info, image::DynamicImage::ImageRgb8(picture)))
    }

    #[cfg(test)]
    mod tests {
        use super::{
            channel, handle_error, raw_ximage_pixel, xlib, DisplayConnection, CONNECTION_LOCK,
            ERROR_HANDLER,
        };

        #[test]
        fn raw_pixel_reader_respects_x11_byte_order_and_stride() {
            let little_endian = [0x33, 0x22, 0x11, 0, 0xaa, 0xbb, 0xcc, 0];
            assert_eq!(
                raw_ximage_pixel(&little_endian, 8, 4, xlib::LSBFirst, 0, 0),
                Some(0x0011_2233)
            );
            assert_eq!(
                raw_ximage_pixel(&little_endian, 8, 4, xlib::LSBFirst, 1, 0),
                Some(0x00cc_bbaa)
            );
            let big_endian = [0, 0x11, 0x22, 0x33];
            assert_eq!(
                raw_ximage_pixel(&big_endian, 4, 4, xlib::MSBFirst, 0, 0),
                Some(0x0011_2233)
            );
        }

        #[test]
        fn error_handler_preserves_other_displays_and_unrelated_errors() {
            unsafe extern "C" fn previous(
                _: *mut xlib::Display,
                _: *mut xlib::XErrorEvent,
            ) -> std::os::raw::c_int {
                37
            }
            let _lock = CONNECTION_LOCK.lock().unwrap();
            // The callback compares this token without dereferencing Display.
            let mut display_token = 0u8;
            let capture = (&mut display_token as *mut u8).cast::<xlib::Display>();
            let original = *ERROR_HANDLER.lock().unwrap();
            *ERROR_HANDLER.lock().unwrap() = (capture as usize, Some(previous));
            let mut event: xlib::XErrorEvent = unsafe { std::mem::zeroed() };
            event.error_code = xlib::BadWindow;
            let stale_window = unsafe { handle_error(capture, &mut event) };
            let other_display = unsafe { handle_error(std::ptr::null_mut(), &mut event) };
            event.error_code = xlib::BadMatch;
            let unrelated_error = unsafe { handle_error(capture, &mut event) };
            *ERROR_HANDLER.lock().unwrap() = original;
            assert_eq!(stale_window, 0);
            assert_eq!(other_display, 37);
            assert_eq!(unrelated_error, 37);
        }

        #[test]
        #[ignore = "requires an X11 display"]
        fn destroyed_window_is_skipped_without_exiting() {
            let connection = DisplayConnection::open().unwrap();
            let window = unsafe {
                (connection.api.XCreateSimpleWindow)(
                    connection.display,
                    connection.root,
                    0,
                    0,
                    10,
                    10,
                    0,
                    0,
                    0,
                )
            };
            unsafe {
                (connection.api.XDestroyWindow)(connection.display, window);
                (connection.api.XSync)(connection.display, 0);
            }
            assert!(connection.info(window).is_none());
            assert!(connection.property(window, "_NET_WM_NAME").is_none());
        }

        #[test]
        fn capture_reads_target_pixels_when_another_window_covers_it() {
            if std::env::var_os("DISPLAY").is_none() { return; }
            let conn = DisplayConnection::open().unwrap();
            let dir = tempfile::tempdir().unwrap();
            let image_path = dir.path().join("target.png");
            let mut attributes: xlib::XSetWindowAttributes = unsafe { std::mem::zeroed() };
            attributes.override_redirect = xlib::True;
            let make_window = |color| unsafe {
                let window = (conn.api.XCreateSimpleWindow)(conn.display, conn.root, 24, 24, 64, 64, 0, 0, color);
                (conn.api.XChangeWindowAttributes)(conn.display, window, xlib::CWOverrideRedirect, &mut attributes);
                let title = std::ffi::CString::new("Capture regression fixture").unwrap();
                (conn.api.XStoreName)(conn.display, window, title.as_ptr());
                (conn.api.XMapRaised)(conn.display, window);
                (conn.api.XClearWindow)(conn.display, window);
                window
            };
            let mut make_window = make_window;
            let target = make_window(0xff0000);
            unsafe { (conn.api.XSync)(conn.display, xlib::False); }
            std::thread::sleep(std::time::Duration::from_millis(200));
            unsafe { (conn.api.XClearWindow)(conn.display, target); (conn.api.XSync)(conn.display, xlib::False); }
            let covering = make_window(0x0000ff);
            unsafe { (conn.api.XSync)(conn.display, xlib::False); }
            std::thread::sleep(std::time::Duration::from_millis(150));
            let result = super::capture_visible(&conn, target, 0);
            unsafe {
                (conn.api.XDestroyWindow)(conn.display, covering);
                (conn.api.XDestroyWindow)(conn.display, target);
                (conn.api.XSync)(conn.display, xlib::False);
            }
            drop(conn);
            let (_, picture) = result.unwrap();
            picture.save(&image_path).unwrap();
            let image = image::open(image_path).unwrap().to_rgb8();
            assert_eq!(image.get_pixel(32, 32).0, [255, 0, 0], "The target must be red even while a blue window covers it");
        }

        #[test]
        fn rgb_masks_decode_x11_pixel() {
            assert_eq!(channel(0x336699, 0xff0000), 0x33);
            assert_eq!(channel(0x336699, 0x00ff00), 0x66);
            assert_eq!(channel(0x336699, 0x0000ff), 0x99);
        }
    }
}

#[cfg(target_os = "linux")]
pub use linux::{
    is_window_active, read_window_pixels,
    activate_window, capture_window as platform_capture_window, close_window, list_windows, window_process_ids,
};

#[cfg(any(target_os = "macos", target_os = "windows"))]
mod native {
    use super::WindowInfo;
    use std::path::Path;
    use std::thread;
    use std::time::Duration;
    use xcap::Window;

    fn info(window: &Window) -> Result<WindowInfo, String> {
        Ok(WindowInfo {
            id: format!("0x{:x}", window.id().map_err(|error| error.to_string())?),
            title: window.title().map_err(|error| error.to_string())?,
            x: window.x().map_err(|error| error.to_string())?,
            y: window.y().map_err(|error| error.to_string())?,
            width: window.width().map_err(|error| error.to_string())?,
            height: window.height().map_err(|error| error.to_string())?,
        })
    }

    pub fn list_windows() -> Result<Vec<WindowInfo>, String> {
        let mut windows = Vec::new();
        for window in Window::all().map_err(|error| error.to_string())? {
            if window.is_minimized().unwrap_or(true) {
                continue;
            }
            if let Ok(item) = info(&window) {
                if !item.title.trim().is_empty() && item.width > 0 && item.height > 0 {
                    windows.push(item);
                }
            }
        }
        windows.sort_by(|a, b| a.title.to_lowercase().cmp(&b.title.to_lowercase()));
        Ok(windows)
    }

    pub fn is_window_active(window_id: &str) -> Result<bool, String> {
        let id = u32::from_str_radix(window_id.trim_start_matches("0x"), 16).map_err(|e| e.to_string())?;
        #[cfg(target_os = "windows")]
        {
            #[link(name = "user32")]
            unsafe extern "system" { fn GetForegroundWindow() -> isize; }
            return Ok(unsafe { GetForegroundWindow() } as u32 == id);
        }
        #[cfg(target_os = "macos")]
        {
            Ok(Window::all().map_err(|e| e.to_string())?.iter().any(|w| w.id().ok() == Some(id) && w.is_focused().unwrap_or(false)))
        }
    }

    pub fn activate_window(window_id: &str) -> Result<WindowInfo, String> {
        let id = u32::from_str_radix(window_id.trim_start_matches("0x"), 16)
            .map_err(|_| format!("Invalid window ID: {window_id}"))?;
        let window = Window::all()
            .map_err(|error| error.to_string())?
            .into_iter()
            .find(|item| item.id().is_ok_and(|candidate| candidate == id))
            .ok_or("Selected window is no longer open")?;
        #[cfg(target_os = "windows")]
        {
            #[link(name = "user32")]
            unsafe extern "system" {
                fn SetForegroundWindow(window: isize) -> i32;
            }
            // xcap exposes the low 32 bits of HWND; Windows sign-extends them on 64-bit systems.
            if unsafe { SetForegroundWindow((id as i32) as isize) } == 0 {
                return Err("Windows denied activation of the selected window".into());
            }
        }
        #[cfg(target_os = "macos")]
        {
            accessible_window(&window)?.activate().map_err(|error| {
                format!("Could not activate selected window; grant Accessibility permission: {error}")
            })?;
        }
        thread::sleep(Duration::from_millis(150));
        info(&window)
    }

    pub fn read_window_pixels(window_id: &str, region: Option<(u32, u32, u32, u32)>) -> Result<image::RgbImage, String> {
        let id = u32::from_str_radix(window_id.trim_start_matches("0x"), 16).map_err(|_| "Invalid window ID")?;
        let window = Window::all().map_err(|error| error.to_string())?.into_iter()
            .find(|window| window.id().is_ok_and(|candidate| candidate == id)).ok_or("Selected window is no longer open")?;
        let details = info(&window)?;
        let image = window.capture_image().map_err(|error| error.to_string())?;
        let (x, y, width, height) = region.unwrap_or((0, 0, details.width, details.height));
        if width == 0 || height == 0 || x.checked_add(width).is_none_or(|right| right > details.width)
            || y.checked_add(height).is_none_or(|bottom| bottom > details.height) { return Err("Capture region is outside the selected window".into()); }
        let sx = image.width() as f64 / details.width as f64;
        let sy = image.height() as f64 / details.height as f64;
        let (x, y, width, height) = ((x as f64 * sx) as u32, (y as f64 * sy) as u32,
            (width as f64 * sx) as u32, (height as f64 * sy) as u32);
        let cropped = image::imageops::crop_imm(&image, x, y, width, height).to_image();
        Ok(image::DynamicImage::ImageRgba8(cropped).to_rgb8())
    }

    pub fn capture_window(
        window_id: &str,
        inset: u32,
        destination: &Path,
        include_uimap: bool,
        target_window_id: Option<&str>,
    ) -> Result<WindowInfo, String> {
        let id = u32::from_str_radix(window_id.trim_start_matches("0x"), 16)
            .map_err(|_| format!("Invalid window ID: {window_id}"))?;
        let window = Window::all()
            .map_err(|error| error.to_string())?
            .into_iter()
            .find(|item| item.id().is_ok_and(|candidate| candidate == id))
            .ok_or("Selected window is no longer open")?;
        if window.is_minimized().unwrap_or(true) {
            return Err("Selected window is minimized".into());
        }
        let details = info(&window)?;
        let image = window
            .capture_image()
            .map_err(|error| format!("Could not capture window: {error}"))?;
        let crop = inset.checked_mul(2).ok_or("Inset is too large")?;
        let width = image
            .width()
            .checked_sub(crop)
            .ok_or("Inset exceeds captured image width")?;
        let height = image
            .height()
            .checked_sub(crop)
            .ok_or("Inset exceeds captured image height")?;
        if width == 0 || height == 0 {
            return Err("Capture area is empty".into());
        }
        let cropped = image::imageops::crop_imm(&image, inset, inset, width, height).to_image();
        super::save_image_with_uimap(
            &image::DynamicImage::ImageRgba8(cropped),
            Some((
                details.x as f64 + inset as f64 * details.width as f64 / image.width() as f64,
                details.y as f64 + inset as f64 * details.height as f64 / image.height() as f64,
                width as f64 * details.width as f64 / image.width() as f64,
                height as f64 * details.height as f64 / image.height() as f64,
            )),
            destination,
            include_uimap,
            target_window_id,
        )?;
        Ok(details)
    }
    pub fn window_process_ids() -> Result<Vec<(String, u32)>, String> {
        Ok(Window::all().map_err(|e| e.to_string())?.into_iter().filter_map(|window| {
            Some((format!("0x{:x}", window.id().ok()?), window.pid().ok()?))
        }).collect())
    }

    #[cfg(target_os = "macos")]
    fn accessible_window(window: &Window) -> Result<xa11y::Element, String> {
        use xa11y::{App, AppExt};
        let pid = window.pid().map_err(|e| e.to_string())?;
        let details = info(window)?;
        let mut matches = Vec::new();
        for app in App::list().map_err(|e| e.to_string())? {
            if app.pid != Some(pid) { continue; }
            for candidate in app.windows().map_err(|e| e.to_string())? {
                if candidate.bounds.is_some_and(|bounds| {
                    (bounds.x - details.x).abs() <= 2 && (bounds.y - details.y).abs() <= 2
                        && (bounds.width as i64 - details.width as i64).abs() <= 2
                        && (bounds.height as i64 - details.height as i64).abs() <= 2
                }) { matches.push(candidate); }
            }
        }
        if matches.len() != 1 {
            return Err("Cannot uniquely identify the selected window; grant Accessibility permission".into());
        }
        Ok(matches.remove(0))
    }

    pub fn close_window(window_id: &str) -> Result<(), String> {
        let id = u32::from_str_radix(window_id.trim_start_matches("0x"), 16)
            .map_err(|_| format!("Invalid window ID: {window_id}"))?;
        let window = Window::all().map_err(|e| e.to_string())?.into_iter()
            .find(|window| window.id().is_ok_and(|candidate| candidate == id))
            .ok_or("Selected window is no longer open")?;
        #[cfg(target_os = "windows")]
        {
            #[link(name = "user32")]
            unsafe extern "system" { fn PostMessageW(window: isize, message: u32, wparam: usize, lparam: isize) -> i32; }
            let _ = window;
            if unsafe { PostMessageW((id as i32) as isize, 0x0010, 0, 0) } == 0 {
                return Err("Windows denied closing the selected window".into());
            }
            Ok(())
        }
        #[cfg(target_os = "macos")]
        { accessible_window(&window)?.close().map_err(|e| format!("Could not close selected window: {e}")) }
    }

}

#[cfg(any(target_os = "macos", target_os = "windows"))]
pub use native::{is_window_active, read_window_pixels, activate_window, capture_window as platform_capture_window, close_window, list_windows, window_process_ids};

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
pub fn list_windows() -> Result<Vec<WindowInfo>, String> {
    Err("Window capture is unsupported on this platform".into())
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
pub fn platform_capture_window(
    _window_id: &str,
    _inset: u32,
    _destination: &std::path::Path,
    _include_uimap: bool,
    _target_window_id: Option<&str>,
) -> Result<WindowInfo, String> {
    Err("Window capture is unsupported on this platform".into())
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
pub fn close_window(_window_id: &str) -> Result<(), String> {
    Ok(())
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
pub fn activate_window(_window_id: &str) -> Result<WindowInfo, String> {
    Err("Window activation is unsupported on this platform".into())
}

pub use crate::capture_lifecycle::with_capture_preparation;

pub fn capture_window(
    window_id: &str,
    inset: u32,
    destination: &std::path::Path,
    include_uimap: bool,
    target_window_id: Option<&str>,
) -> Result<WindowInfo, String> {
    crate::capture_lifecycle::run_pre_capture_hook()?;
    platform_capture_window(
        window_id,
        inset,
        destination,
        include_uimap,
        target_window_id,
    )
}

fn scale_capture_elements(elements: &mut [crate::ui_elements::DetectedUiElement], pixel_width: u32, pixel_height: u32, width: f64, height: f64) {
    if width <= 0.0 || height <= 0.0 { return; }
    let scale_x = pixel_width as f64 / width;
    let scale_y = pixel_height as f64 / height;
    for element in elements {
        element.x *= scale_x;
        element.y *= scale_y;
        element.width *= scale_x;
        element.height *= scale_y;
    }
}

#[cfg(test)]
mod capture_scale_tests {
    use super::*;
    #[cfg(target_os = "windows")]
    #[test]
    #[ignore = "requires an open Chromium browser web page"]
    fn browser_page_buttons_are_detected_beyond_browser_toolbar() {
        use xa11y::{App, AppExt, Role};
        let window = list_windows().unwrap().into_iter().find(|window|
            window.title.contains("Google Chrome") || window.title.contains("Microsoft") && window.title.contains("Edge")
        ).expect("Open a web page in Chrome or Edge");
        let context = crate::semantic_target::window_context(&window.id).unwrap();
        let app = App::by_pid(context.pid.unwrap(), std::time::Duration::ZERO).unwrap();
        let root = app.as_element().children().unwrap().into_iter()
            .find(|element| element.stable_id == context.window_id).unwrap();
        let mut queue = std::collections::VecDeque::from([(root, 0usize, false)]);
        let mut visited = 0;
        let mut buttons = Vec::new();
        while let Some((element, depth, in_page)) = queue.pop_front() {
            visited += 1;
            if visited > 10000 || depth > 64 { break; }
            let in_page = in_page || element.role == Role::WebArea;
            if in_page && element.role == Role::Button {
                if let (Some(name), Some(bounds)) = (&element.name, element.bounds) {
                    if bounds.width > 4 && bounds.height > 4
                        && bounds.x as f64 + bounds.width as f64 > context.x
                        && (bounds.x as f64) < context.x + context.width
                        && bounds.y as f64 + bounds.height as f64 > context.y
                        && (bounds.y as f64) < context.y + context.height {
                        buttons.push((name.trim().to_owned(), bounds, depth));
                    }
                }
            }
            if let Ok(children) = element.children() {
                queue.extend(children.into_iter().map(|child| (child, depth + 1, in_page)));
            }
        }
        eprintln!("Browser accessibility: {visited} nodes, {} page buttons, button depths {:?}",
            buttons.len(), buttons.iter().map(|(_,_,depth)| depth).collect::<Vec<_>>());
        assert!(!buttons.is_empty(), "Browser accessibility exposes no page buttons");
        {
            let _depth = crate::ui_elements::capture_depth_scope(16);
            let shallow = crate::ui_elements::capture_desktop_detailed_elements_for_window(0, 0, &context);
            assert!(!buttons.iter().filter(|(_,_,depth)| *depth > 16).any(|(name,bounds,_)|
                shallow.iter().any(|item| item.role == "button" && item.name.as_ref() == Some(name)
                    && (item.x - bounds.x as f64).abs() < 2.0 && (item.y - bounds.y as f64).abs() < 2.0)));
        }
        let _depth = crate::ui_elements::capture_depth_scope(64);
        let detected = crate::ui_elements::capture_desktop_detailed_elements_for_window(0, 0, &context);
        let found = buttons.iter().filter(|(name,bounds,_)| detected.iter().any(|item|
            item.role == "button" && item.name.as_ref() == Some(name)
                && (item.x - bounds.x as f64).abs() < 2.0 && (item.y - bounds.y as f64).abs() < 2.0
        )).count();
        eprintln!("Detected {found}/{} page buttons", buttons.len());
        assert_eq!(found, buttons.len(), "Capture must include web page buttons, not just browser chrome");
        let directory = tempfile::tempdir().unwrap();
        let output = directory.path().join("browser.png");
        platform_capture_window(&window.id, 0, &output, false, Some(&window.id)).unwrap();
        let captured = crate::raster::extract_png_uimap(&std::fs::read(output).unwrap()).unwrap();
        assert!(buttons.iter().all(|(name,_,_)| captured.iter().any(|item|
            item.role == "button" && item.name == *name)),
            "Web page buttons must also be embedded in the captured PNG for MarkIts");
    }
    #[cfg(target_os = "windows")]
    #[test]
    #[ignore = "requires an open YouTube browser window"]
    fn youtube_capture_retains_accessible_controls() {
        let window = list_windows().unwrap().into_iter()
            .find(|window| window.title.contains("YouTube")).expect("Open YouTube in a browser");
        let directory = tempfile::tempdir().unwrap();
        let output = directory.path().join("youtube.png");
        platform_capture_window(&window.id, 0, &output, false, Some(&window.id)).unwrap();
        let bytes = std::fs::read(output).unwrap();
        let controls = crate::raster::extract_png_uimap(&bytes).unwrap_or_default();
        assert!(controls.iter().any(|element| element.role == "button"),
            "YouTube capture must retain browser/page controls, got {} elements", controls.len());
    }
    #[cfg(any(target_os = "windows", target_os = "macos"))]
    #[test]
    fn native_window_ids_can_be_joined_to_their_processes() {
        let windows = list_windows().unwrap();
        if windows.is_empty() { return; } // No interactive desktop in some CI environments.
        let owners = window_process_ids().unwrap();
        assert!(windows.iter().any(|window| owners.iter().any(|(id, pid)| id == &window.id && *pid > 0)),
            "Native window enumeration and process ownership must use the same ID format");
    }
    #[test]
    fn logical_coordinates_are_scaled_to_capture_pixels() {
        let mut elements = vec![crate::ui_elements::DetectedUiElement {
            role: "button".into(), name: None, window_id: None, pid: None,
            x: 10.0, y: 20.0, width: 30.0, height: 40.0,
        }];
        scale_capture_elements(&mut elements, 1600, 900, 800.0, 600.0);
        assert_eq!((elements[0].x, elements[0].y, elements[0].width, elements[0].height), (20.0, 30.0, 60.0, 60.0));
    }
}
