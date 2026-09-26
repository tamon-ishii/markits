use markits::{render_from_json, Scene};

#[test]
fn test_complex_multi_annotation_scene() {
    let json = r#"{
      "canvas": {
        "width": 1920,
        "height": 1080
      },
      "annotations": [
        {
          "type": "spotlight",
          "target": [820, 640, 100, 32],
          "style": "primary"
        },
        {
          "type": "callout",
          "target": [820, 640, 100, 32],
          "text": "設定を保存します",
          "style": "primary"
        },
        {
          "type": "callout",
          "target": [500, 640, 120, 32],
          "text": "設定をキャンセルします",
          "style": "secondary"
        },
        {
          "type": "badge",
          "target": [820, 640, 100, 32],
          "step": 1,
          "style": "step",
          "position": "top-left"
        },
        {
          "type": "rounded-rect",
          "target": [500, 640, 120, 32],
          "rx": 6.0,
          "ry": 6.0,
          "style": "warning"
        }
      ]
    }"#;

    let scene = Scene::from_json(json).expect("Should parse multi-annotation json");
    assert_eq!(scene.annotations.len(), 5);

    let svg = scene.render_svg().expect("Should render SVG successfully");

    // Validate SVG root properties
    assert!(svg.starts_with(r#"<svg xmlns="http://www.w3.org/2000/svg" width="1920" height="1080" viewBox="0 0 1920 1080">"#));
    assert!(svg.ends_with("</svg>\n"));

    // Validate defs presence
    assert!(svg.contains("<filter id=\"markits-shadow\""));
    assert!(svg.contains("<marker id=\"arrowhead-primary\""));
    assert!(svg.contains("<marker id=\"arrowhead-secondary\""));
    assert!(svg.contains("<mask id=\"spotlight-mask-0\">"));

    // Validate rendered annotations content
    assert!(svg.contains("設定を保存します"));
    assert!(svg.contains("設定をキャンセルします"));
    assert!(svg.contains(">1</text>"));
    assert!(svg.contains("mask=\"url(#spotlight-mask-0)\""));
    assert!(svg.contains("stroke=\"#d97706\"")); // warning color
}

#[test]
fn test_render_from_json_helper() {
    let json = r#"{
      "canvas": { "width": 800, "height": 600 },
      "annotations": [
        {
          "type": "label",
          "target": {"x": 50, "y": 50, "width": 100, "height": 40},
          "text": "Header Element",
          "style": "info"
        }
      ]
    }"#;

    let svg = render_from_json(json).expect("Should render directly from JSON string");
    assert!(svg.contains("width=\"800\""));
    assert!(svg.contains("height=\"600\""));
    assert!(svg.contains("Header Element"));
    assert!(svg.contains("#0284c7")); // info color
}

#[test]
fn test_skitch_components_full_workflow() {
    let json = r#"{
      "canvas": { "width": 1200, "height": 800 },
      "shadow": true,
      "annotations": [
        {
          "type": "divider",
          "target": [50, 400, 1100, 4],
          "style": "pink"
        },
        {
          "type": "pin",
          "target": [250, 200, 60, 60],
          "icon": "?",
          "text": "これは何？",
          "style": "info",
          "position": "left",
          "shadow": true,
          "outline": true
        },
        {
          "type": "bullseye",
          "target": [600, 200, 60, 60],
          "style": "pink",
          "shadow": false
        },
        {
          "type": "label",
          "target": [600, 200, 60, 60],
          "text": "矢印の位置が変えられる",
          "style": "pink",
          "position": "right",
          "outline": true
        }
      ]
    }"#;

    let svg = render_from_json(json).expect("Should render Skitch components SVG");
    assert!(svg.contains("#ea1a65")); // Pink
    assert!(svg.contains("<polygon points=")); // Pin tip
    assert!(svg.contains("これは何？"));
    assert!(svg.contains("矢印の位置が変えられる"));
    assert!(svg.contains("paint-order=\"stroke fill\""));
    assert!(svg.contains("<line x1=\"50\"")); // Divider
}
