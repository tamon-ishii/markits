use markits::ui_elements::capture_desktop_detailed_elements;

fn main() {
    let elements = capture_desktop_detailed_elements(0, 0, None);
    println!("Total elements: {}", elements.len());
    for (i, el) in elements.iter().enumerate() {
        println!("[{}] role: {}, name: {:?}, bounds: ({}, {}, {}, {})", i, el.role, el.name, el.x, el.y, el.width, el.height);
    }
}
