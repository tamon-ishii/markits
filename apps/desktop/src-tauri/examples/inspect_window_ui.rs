use markits_desktop_lib::ui_elements::{
    capture_desktop_detailed_elements_for_window, capture_desktop_windows,
};

fn main() {
    let windows = capture_desktop_windows(0, 0);
    let Some(window) = windows
        .iter()
        .find(|window| window.name.as_deref() == Some("アプリセンター"))
    else {
        println!("App Center window not found");
        return;
    };
    println!("Window: {window:?}");
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
