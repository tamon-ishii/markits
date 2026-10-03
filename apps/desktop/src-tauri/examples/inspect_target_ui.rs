use markits_desktop_lib::ui_elements::{
    capture_desktop_detailed_elements_for_window, capture_desktop_windows, target_process_id,
};
use std::time::Duration;
use xa11y::{App, AppExt};

fn main() {
    let query = std::env::args()
        .nth(1)
        .expect("usage: inspect_target_ui WINDOW_TITLE");
    let requested_pid = std::env::args()
        .nth(2)
        .and_then(|arg| arg.parse::<u32>().ok());
    let windows = capture_desktop_windows(0, 0);
    let Some(window) = windows.iter().find(|window| {
        window
            .name
            .as_deref()
            .is_some_and(|name| name.contains(&query))
            && requested_pid.is_none_or(|pid| target_process_id(window) == Some(pid))
    }) else {
        eprintln!("Window not found: {query}");
        std::process::exit(1);
    };
    println!("Window: {window:?}");
    if let Some(pid) = requested_pid {
        match App::by_pid(pid, Duration::ZERO) {
            Ok(app) => {
                println!("Accessibility app: {} ({:?})", app.name, app.pid);
                println!("Accessibility root: {:?}", app.as_element().raw);
                if let Ok(children) = app.as_element().children() {
                    for child in children {
                        println!(
                            "Root child: {:?} {:?} {:?}",
                            child.role, child.name, child.bounds
                        );
                    }
                }
            }
            Err(error) => println!("Accessibility app error: {error}"),
        }
    }
    let elements = capture_desktop_detailed_elements_for_window(0, 0, window);
    println!("Elements: {}", elements.len());
    for element in elements
        .iter()
        .filter(|element| element.role != "window")
        .take(20)
    {
        println!("{element:?}");
    }
}
