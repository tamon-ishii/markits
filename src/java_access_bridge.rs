//! Read Java accessibility on a dedicated, message-pumping thread. UIA does not
//! expose Swing controls, and the bridge must keep receiving window messages.
use crate::ui_elements::DetectedUiElement;
use std::{collections::HashMap, path::{Path, PathBuf}, sync::{Mutex, OnceLock, mpsc}, time::{Duration, Instant}};
use windows_sys::Win32::{
    Foundation::{CloseHandle, HMODULE, HWND},
    System::{LibraryLoader::{GetProcAddress, LoadLibraryExW, LOAD_LIBRARY_SEARCH_DEFAULT_DIRS, LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR},
        Threading::{OpenProcess, QueryFullProcessImageNameW, PROCESS_QUERY_LIMITED_INFORMATION}},
    UI::WindowsAndMessaging::{DispatchMessageW, GetWindowThreadProcessId, MSG, PM_REMOVE, PeekMessageW, TranslateMessage},
};

#[repr(C)]
struct ContextInfo {
    name: [u16; 1024], description: [u16; 1024],
    role: [u16; 256], role_en: [u16; 256], states: [u16; 256], states_en: [u16; 256],
    index: i32, children: i32, x: i32, y: i32, width: i32, height: i32,
    component: i32, action: i32, selection: i32, text: i32, interfaces: i32,
}

struct Bridge {
    _module: HMODULE,
    is_java: unsafe extern "C" fn(HWND) -> i32,
    context: unsafe extern "C" fn(HWND, *mut i32, *mut i64) -> i32,
    info: unsafe extern "C" fn(i32, i64, *mut ContextInfo) -> i32,
    child: unsafe extern "C" fn(i32, i64, i32) -> i64,
    release: unsafe extern "C" fn(i32, i64),
}

struct Request { target: DetectedUiElement, origin: (i32, i32), depth: usize, reply: mpsc::SyncSender<Vec<DetectedUiElement>> }
static WORKERS: OnceLock<Mutex<HashMap<PathBuf, mpsc::SyncSender<Request>>>> = OnceLock::new();

fn bridge_path(target: &DetectedUiElement) -> Option<PathBuf> {
    let pid = target.pid?;
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if process.is_null() { return None; }
    let mut buffer = [0u16; 32768];
    let mut length = buffer.len() as u32;
    let ok = unsafe { QueryFullProcessImageNameW(process, 0, buffer.as_mut_ptr(), &mut length) };
    unsafe { CloseHandle(process); }
    if ok == 0 { return None; }
    let executable = PathBuf::from(String::from_utf16_lossy(&buffer[..length as usize]));
    let bin = executable.parent()?;
    // JetBrains bundles its runtime next to bin; ordinary Java launchers live
    // in the runtime's bin directory. Never search the current directory/PATH.
    [bin.join("windowsaccessbridge-64.dll"), bin.parent()?.join("jbr/bin/windowsaccessbridge-64.dll")]
        .into_iter().find(|path| path.is_file())
}

fn window_handle(target: &DetectedUiElement) -> Option<HWND> {
    let id = target.window_id.as_deref()?.strip_prefix("hwnd:0x")?;
    let handle = usize::from_str_radix(id, 16).ok()? as HWND;
    let mut pid = 0;
    unsafe { GetWindowThreadProcessId(handle, &mut pid); }
    (target.pid == Some(pid)).then_some(handle)
}

fn pump_messages() {
    unsafe {
        let mut message: MSG = std::mem::zeroed();
        while PeekMessageW(&mut message, std::ptr::null_mut(), 0, 0, PM_REMOVE) != 0 {
            TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
}

impl Bridge {
    unsafe fn load(path: &Path) -> Option<Self> {
        let path: Vec<u16> = path.as_os_str().to_string_lossy().encode_utf16().chain(Some(0)).collect();
        let module = unsafe { LoadLibraryExW(path.as_ptr(), std::ptr::null_mut(), LOAD_LIBRARY_SEARCH_DEFAULT_DIRS | LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR) };
        if module.is_null() { return None; }
        macro_rules! symbol {
            ($name:literal, $ty:ty) => {
                unsafe { std::mem::transmute::<unsafe extern "system" fn() -> isize, $ty>(GetProcAddress(module, $name.as_ptr().cast())?) }
            };
        }
        let run = symbol!(c"Windows_run", unsafe extern "C" fn());
        let bridge = Self {
            _module: module,
            is_java: symbol!(c"isJavaWindow", unsafe extern "C" fn(HWND) -> i32),
            context: symbol!(c"getAccessibleContextFromHWND", unsafe extern "C" fn(HWND, *mut i32, *mut i64) -> i32),
            info: symbol!(c"getAccessibleContextInfo", unsafe extern "C" fn(i32, i64, *mut ContextInfo) -> i32),
            child: symbol!(c"getAccessibleChildFromContext", unsafe extern "C" fn(i32, i64, i32) -> i64),
            release: symbol!(c"releaseJavaObject", unsafe extern "C" fn(i32, i64)),
        };
        unsafe { run(); }
        Some(bridge)
    }

    fn collect(&self, request: &Request) -> Vec<DetectedUiElement> {
        let Some(hwnd) = window_handle(&request.target) else { return Vec::new(); };
        let ready_by = Instant::now() + Duration::from_millis(500);
        while unsafe { (self.is_java)(hwnd) } == 0 {
            if Instant::now() >= ready_by { return Vec::new(); }
            pump_messages();
            std::thread::sleep(Duration::from_millis(5));
        }
        let mut vm = 0;
        let mut context = 0;
        if unsafe { (self.context)(hwnd, &mut vm, &mut context) } == 0 || context == 0 { return Vec::new(); }
        let mut output = Vec::new();
        let mut visited = 0;
        self.walk(vm, context, 0, request, Instant::now() + Duration::from_millis(1500), &mut visited, &mut output);
        unsafe { (self.release)(vm, context); }
        output
    }

    fn walk(&self, vm: i32, context: i64, depth: usize, request: &Request, deadline: Instant, visited: &mut usize, output: &mut Vec<DetectedUiElement>) {
        if depth > request.depth || *visited >= 5000 || output.len() >= 1000 || Instant::now() >= deadline { return; }
        *visited += 1;
        pump_messages();
        let mut info: ContextInfo = unsafe { std::mem::zeroed() };
        if unsafe { (self.info)(vm, context, &mut info) } == 0 { return; }
        let name = utf16(&info.name);
        if let Some(role) = role_name(&utf16(&info.role_en), !name.is_empty()) {
            let x = info.x as f64 - request.origin.0 as f64;
            let y = info.y as f64 - request.origin.1 as f64;
            let target = &request.target;
            if info.width > 4 && info.height > 4 && x < target.x + target.width && y < target.y + target.height
                && x + info.width as f64 > target.x && y + info.height as f64 > target.y
                && !utf16(&info.states_en).split(',').any(|state| matches!(state.trim(), "invisible" | "offscreen"))
            {
                output.push(DetectedUiElement { role: role.into(), name: (!name.is_empty()).then_some(name),
                    window_id: None, pid: request.target.pid, x, y, width: info.width as f64, height: info.height as f64 });
            }
        }
        if depth >= request.depth { return; }
        for index in 0..info.children.clamp(0, 5000) {
            if Instant::now() >= deadline || *visited >= 5000 || output.len() >= 1000 { break; }
            let child = unsafe { (self.child)(vm, context, index) };
            if child != 0 {
                self.walk(vm, child, depth + 1, request, deadline, visited, output);
                unsafe { (self.release)(vm, child); }
            }
        }
    }
}

fn utf16(value: &[u16]) -> String {
    String::from_utf16_lossy(&value[..value.iter().position(|&unit| unit == 0).unwrap_or(value.len())]).trim().to_owned()
}

fn role_name(role: &str, named: bool) -> Option<&'static str> {
    Some(match role {
        "push button" | "toggle button" => "button", "check box" => "checkbox", "radio button" => "radio",
        "text" | "password text" => "input", "combo box" => "combobox", "menu item" | "menu" => "menuitem",
        "page tab" => "tab", "label" => "text", "icon" => "image", "list" | "tree" => "list",
        "list item" => "listitem", "table" => "table", "tool bar" => "toolbar",
        "panel" if named => "group", _ => return None,
    })
}

pub fn collect(target: &DetectedUiElement, origin_x: i32, origin_y: i32, depth: usize) -> Option<Vec<DetectedUiElement>> {
    let path = bridge_path(target)?;
    let workers = WORKERS.get_or_init(Default::default);
    let Ok(mut workers) = workers.lock() else { return Some(Vec::new()); };
    let sender = workers.entry(path.clone()).or_insert_with(|| {
        let (sender, receiver) = mpsc::sync_channel::<Request>(1);
        let _ = std::thread::Builder::new().name("java-accessibility".into()).spawn(move || {
            let Some(bridge) = (unsafe { Bridge::load(&path) }) else { return; };
            loop {
                pump_messages();
                match receiver.try_recv() {
                    Ok(request) => { let _ = request.reply.send(bridge.collect(&request)); }
                    Err(mpsc::TryRecvError::Disconnected) => break,
                    Err(mpsc::TryRecvError::Empty) => std::thread::sleep(Duration::from_millis(5)),
                }
            }
        });
        sender
    }).clone();
    drop(workers);
    let (reply, receiver) = mpsc::sync_channel(1);
    if sender.try_send(Request { target: target.clone(), origin: (origin_x, origin_y), depth, reply }).is_err() { return Some(Vec::new()); }
    Some(receiver.recv_timeout(Duration::from_millis(2200)).unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bridge_layout_and_roles_match_java_abi() {
        assert_eq!(std::mem::size_of::<ContextInfo>(), 6188);
        assert_eq!(role_name("push button", false), Some("button"));
        assert_eq!(role_name("page tab", true), Some("tab"));
        assert_eq!(role_name("panel", false), None);
        assert_eq!(role_name("panel", true), Some("group"));
        assert_eq!(utf16(&[65, 66, 0, 67]), "AB");
    }
}
