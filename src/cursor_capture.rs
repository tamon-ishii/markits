/// Current pointer in desktop coordinates, if it lies inside the capture target.
pub fn pointer_in_bounds(
    window: &crate::window_capture::WindowInfo,
) -> Result<Option<crate::recording::RecordedPointer>, String> {
    #[cfg(target_os = "windows")]
    {
        use windows_sys::Win32::{Foundation::POINT, UI::WindowsAndMessaging::GetCursorPos};
        let mut p = POINT { x: 0, y: 0 };
        if unsafe { GetCursorPos(&mut p) } == 0 {
            return Err(std::io::Error::last_os_error().to_string());
        }
        if p.x < window.x
            || p.y < window.y
            || p.x as f64 >= window.x as f64 + window.width as f64
            || p.y as f64 >= window.y as f64 + window.height as f64
        {
            return Ok(None);
        }
        Ok(Some(crate::recording::RecordedPointer {
            x: p.x as f64,
            y: p.y as f64,
            target: None,
            target_offset: None,
        }))
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = window;
        Ok(None)
    }
}

#[cfg(target_os = "windows")]
pub mod windows {
    use std::{
        mem::{size_of, zeroed},
        ptr::{copy_nonoverlapping, null_mut},
    };
    use windows_sys::Win32::{Graphics::Gdi::*, UI::WindowsAndMessaging::*};

    pub fn window_under_cursor() -> Result<String, String> {
        unsafe {
            let mut point = windows_sys::Win32::Foundation::POINT { x: 0, y: 0 };
            if GetCursorPos(&mut point) == 0 {
                return Err(error());
            }
            let window = GetAncestor(WindowFromPoint(point), GA_ROOT);
            if window.is_null() {
                return Err("No window under the cursor".into());
            }
            Ok(format!("0x{:x}", window as usize as u32))
        }
    }

    fn error() -> String {
        format!(
            "マウスポインタの撮影に失敗しました: {}",
            std::io::Error::last_os_error()
        )
    }

    pub fn release_overlay_capture(handle: usize) {
        use windows_sys::Win32::{Foundation::HWND, UI::Input::KeyboardAndMouse::ReleaseCapture};
        unsafe {
            let overlay = handle as HWND;
            let thread = GetWindowThreadProcessId(overlay, null_mut());
            let mut info: GUITHREADINFO = zeroed();
            info.cbSize = size_of::<GUITHREADINFO>() as u32;
            if GetGUIThreadInfo(thread, &mut info) != 0
                && !info.hwndCapture.is_null()
                && (GetAncestor(info.hwndCapture, GA_ROOT) == overlay
                    || IsChild(overlay, info.hwndCapture) != 0)
            {
                let _ = SendMessageTimeoutW(
                    info.hwndCapture,
                    WM_CANCELMODE,
                    0,
                    0,
                    SMTO_ABORTIFHUNG,
                    100,
                    null_mut(),
                );
            }
            ReleaseCapture();
        }
    }

    pub fn refresh_mouse_move() {
        unsafe {
            let mut point = windows_sys::Win32::Foundation::POINT { x: 0, y: 0 };
            if GetCursorPos(&mut point) != 0 {
                SetCursorPos(point.x, point.y);
            }
        }
    }

    pub fn refresh_cursor() {
        // Ask the window now under the pointer to choose its own cursor. Do not
        // force an arrow: text fields, links and resize borders need their shapes.
        unsafe {
            let mut point = windows_sys::Win32::Foundation::POINT { x: 0, y: 0 };
            if GetCursorPos(&mut point) == 0 {
                return;
            }
            let window = WindowFromPoint(point);
            if window.is_null() {
                return;
            }
            let location = ((point.y as u32 & 0xffff) << 16 | (point.x as u32 & 0xffff)) as isize;
            let mut hit = 0;
            if SendMessageTimeoutW(
                window,
                WM_NCHITTEST,
                0,
                location,
                SMTO_ABORTIFHUNG,
                100,
                &mut hit,
            ) == 0
            {
                return;
            }
            let reason = ((WM_MOUSEMOVE << 16) | (hit as u32 & 0xffff)) as isize;
            let _ = SendMessageTimeoutW(
                window,
                WM_SETCURSOR,
                window as usize,
                reason,
                SMTO_ABORTIFHUNG,
                100,
                null_mut(),
            );
        }
    }

    // Capture what is visible now, including hover tooltips which may be separate
    // native windows. PrintWindow/XCap's offscreen rendering can omit those.
    pub fn screen_pixels(
        window: &crate::window_capture::WindowInfo,
    ) -> Result<image::RgbaImage, String> {
        unsafe {
            let screen = GetDC(null_mut());
            if screen.is_null() {
                return Err(error());
            }
            let dc = CreateCompatibleDC(screen);
            if dc.is_null() {
                ReleaseDC(null_mut(), screen);
                return Err(error());
            }
            let mut info: BITMAPINFO = zeroed();
            info.bmiHeader.biSize = size_of::<BITMAPINFOHEADER>() as u32;
            info.bmiHeader.biWidth = window.width as i32;
            info.bmiHeader.biHeight = -(window.height as i32);
            info.bmiHeader.biPlanes = 1;
            info.bmiHeader.biBitCount = 32;
            info.bmiHeader.biCompression = BI_RGB;
            let mut bits = null_mut();
            let bitmap = CreateDIBSection(dc, &info, DIB_RGB_COLORS, &mut bits, null_mut(), 0);
            if bitmap.is_null() {
                DeleteDC(dc);
                ReleaseDC(null_mut(), screen);
                return Err(error());
            }
            let old = SelectObject(dc, bitmap);
            let result = if BitBlt(
                dc,
                0,
                0,
                window.width as i32,
                window.height as i32,
                screen,
                window.x,
                window.y,
                SRCCOPY | CAPTUREBLT,
            ) == 0
            {
                Err(error())
            } else {
                GdiFlush();
                let mut image = image::RgbaImage::new(window.width, window.height);
                let pixels = std::slice::from_raw_parts(bits as *const u8, image.as_raw().len());
                for (pixel, raw) in image.pixels_mut().zip(pixels.chunks_exact(4)) {
                    *pixel = image::Rgba([raw[2], raw[1], raw[0], 255]);
                }
                Ok(image)
            };
            SelectObject(dc, old);
            DeleteObject(bitmap);
            DeleteDC(dc);
            ReleaseDC(null_mut(), screen);
            result
        }
    }

    pub fn draw_current_cursor(
        image: &mut image::RgbaImage,
        window: &crate::window_capture::WindowInfo,
    ) -> Result<(), String> {
        unsafe {
            let mut cursor: CURSORINFO = zeroed();
            cursor.cbSize = size_of::<CURSORINFO>() as u32;
            if GetCursorInfo(&mut cursor) == 0 {
                return Err(error());
            }
            if cursor.flags & CURSOR_SHOWING == 0 {
                return Ok(());
            }
            let icon = CopyIcon(cursor.hCursor);
            if icon.is_null() {
                return Err(error());
            }
            let mut info: ICONINFO = zeroed();
            if GetIconInfo(icon, &mut info) == 0 {
                DestroyIcon(icon);
                return Err(error());
            }
            let sx = image.width() as f64 / window.width.max(1) as f64;
            let sy = image.height() as f64 / window.height.max(1) as f64;
            let x = ((cursor.ptScreenPos.x as f64 - window.x as f64 - info.xHotspot as f64) * sx)
                .round() as i32;
            let y = ((cursor.ptScreenPos.y as f64 - window.y as f64 - info.yHotspot as f64) * sy)
                .round() as i32;
            let mut bitmap: BITMAP = zeroed();
            let source = if !info.hbmColor.is_null() {
                info.hbmColor
            } else {
                info.hbmMask
            };
            let result = if GetObjectW(
                source,
                size_of::<BITMAP>() as i32,
                &mut bitmap as *mut _ as _,
            ) == 0
            {
                Err(error())
            } else {
                let height = if info.hbmColor.is_null() {
                    bitmap.bmHeight / 2
                } else {
                    bitmap.bmHeight
                };
                draw_icon(
                    image,
                    icon,
                    x,
                    y,
                    (bitmap.bmWidth as f64 * sx).round() as i32,
                    (height as f64 * sy).round() as i32,
                )
            };
            if !info.hbmColor.is_null() {
                DeleteObject(info.hbmColor);
            }
            if !info.hbmMask.is_null() {
                DeleteObject(info.hbmMask);
            }
            DestroyIcon(icon);
            result
        }
    }

    // Draw onto the actual background, preserving monochrome cursors' AND/XOR masks.
    fn draw_icon(
        image: &mut image::RgbaImage,
        icon: HICON,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
    ) -> Result<(), String> {
        unsafe {
            let dc = CreateCompatibleDC(null_mut());
            if dc.is_null() {
                return Err(error());
            }
            let mut info: BITMAPINFO = zeroed();
            info.bmiHeader.biSize = size_of::<BITMAPINFOHEADER>() as u32;
            info.bmiHeader.biWidth = image.width() as i32;
            info.bmiHeader.biHeight = -(image.height() as i32);
            info.bmiHeader.biPlanes = 1;
            info.bmiHeader.biBitCount = 32;
            info.bmiHeader.biCompression = BI_RGB;
            let mut bits = null_mut();
            let bitmap = CreateDIBSection(dc, &info, DIB_RGB_COLORS, &mut bits, null_mut(), 0);
            if bitmap.is_null() {
                DeleteDC(dc);
                return Err(error());
            }
            let old = SelectObject(dc, bitmap);
            let mut bgra = image.as_raw().clone();
            for pixel in bgra.chunks_exact_mut(4) {
                pixel.swap(0, 2);
            }
            copy_nonoverlapping(bgra.as_ptr(), bits as *mut u8, bgra.len());
            let drawn = DrawIconEx(dc, x, y, icon, width, height, 0, null_mut(), DI_NORMAL);
            GdiFlush();
            if drawn != 0 {
                copy_nonoverlapping(bits as *const u8, bgra.as_mut_ptr(), bgra.len());
                for (pixel, raw) in image.pixels_mut().zip(bgra.chunks_exact(4)) {
                    *pixel = image::Rgba([raw[2], raw[1], raw[0], 255]);
                }
            }
            SelectObject(dc, old);
            DeleteObject(bitmap);
            DeleteDC(dc);
            if drawn == 0 { Err(error()) } else { Ok(()) }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        #[test]
        fn cursor_shapes_and_clipping_are_rendered_on_the_background() {
            let background =
                image::RgbaImage::from_pixel(96, 96, image::Rgba([120, 180, 220, 255]));
            let arrow = unsafe { LoadCursorW(null_mut(), IDC_ARROW) };
            let beam = unsafe { LoadCursorW(null_mut(), IDC_IBEAM) };
            assert!(!arrow.is_null() && !beam.is_null());
            let mut a = background.clone();
            let mut b = background.clone();
            draw_icon(&mut a, arrow, 20, 20, 32, 32).unwrap();
            draw_icon(&mut b, beam, 20, 20, 32, 32).unwrap();
            assert_ne!(a, background);
            assert_ne!(b, background);
            assert_ne!(a, b);
            assert!(a.pixels().all(|p| p[3] == 255));
            let mut outside = background.clone();
            draw_icon(&mut outside, arrow, -100, -100, 32, 32).unwrap();
            assert_eq!(outside, background);
        }
    }
}
