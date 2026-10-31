//! Read-only timing probe: no activation, simulated input, or saved captures.
use markits_desktop_lib::{playback, playback_trace::{Run, Span, measure, step}};

fn main() -> Result<(), String> {
    let path = std::env::args().nth(1).ok_or("Pass the path to a recorded PNG")?;
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    let recording = playback::extract(&bytes)?.ok_or("No recording metadata")?;
    let windows = markits::window_capture::list_windows()?;
    let owners = markits::window_capture::window_process_ids()?;
    let window = windows.iter().find(|w| w.id == recording.window_id
        && owners.iter().any(|(id, pid)| id == &w.id && *pid == recording.pid))
        .or_else(|| {
            let mut matching = windows.iter().filter(|w| w.title == recording.recording.window);
            let first = matching.next();
            if matching.next().is_none() { first } else { None }
        }).ok_or("Target window is closed or ambiguous")?;
    let _run = Run::start();
    let _total = Span::new("diagnostic.read_only");
    let _depth = markits::ui_elements::capture_depth_scope(128);
    for index in 1..=2 {
        step(index);
        let elements = measure("ui.observe_click_target", || markits::semantic_target::observe_window(&window.id))?;
        eprintln!("Probe {index}: {} accessible elements", elements.len());
        let png = measure("snapshot.with_ui", || playback::snapshot(&window.id))?;
        eprintln!("Probe {index}: {} snapshot bytes (not saved)", png.len());
    }
    Ok(())
}
