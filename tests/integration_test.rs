use markits::{render_debug_from_json, render_from_json, render_with_layout_from_json, Scene};

#[test]
fn instruction_can_disable_text_outline() {
    let json = r#"{"canvas":{"width":160,"height":100},"annotations":[
        {"type":"instruction","action":"click","target":[80,50,40,20],
         "text":"Click Save","outline":false}]}"#;
    let svg = render_from_json(json).unwrap();
    assert!(svg.contains("Click "));
    assert!(svg.contains("Save"));
    assert!(!svg.contains("paint-order=\"stroke fill\""));
}

#[test]
fn debug_svg_and_multiline_japanese_text() {
    let input = r#"{
      "canvas":{"width":420,"height":280},
      "annotations":[
        {"type":"callout","target":[150,120,60,32],"text":"設定が完了したら、このボタンをクリックしてください & <確認>","max_width":140},
        {"type":"callout","target":[270,120,60,32],"text":"保存します","max_width":100}
      ]
    }"#;
    let svg = render_from_json(input).unwrap();
    assert!(svg.contains("<tspan"));
    assert!(svg.contains("&amp;"));
    assert!(svg.contains("&lt;"));
    assert!(svg.contains("&gt;"));
    let debug = render_debug_from_json(input).unwrap();
    assert!(debug.contains("id=\"markits-layout-debug\""));
    assert_eq!(debug.matches("class=\"target-box\"").count(), 2);
    assert_eq!(debug.matches("class=\"candidate-box selected\"").count(), 2);
    assert_eq!(debug.matches("class=\"candidate-box rejected\"").count(), 46);
    assert!(debug.contains("data-score="));
    assert!(!svg.contains("markits-layout-debug"));
}

#[test]
fn invalid_max_width_has_clear_error() {
    let input = r#"{"canvas":{"width":400,"height":300},"annotations":[{"type":"callout","target":[100,100,40,30],"text":"x","max_width":20}]}"#;
    assert!(Scene::from_json(input).unwrap_err().to_string().contains("annotations[0].max_width"));
}

#[test]
fn instruction_passes_max_width_to_callout() {
    let input = r#"{"canvas":{"width":400,"height":300},"annotations":[{"type":"instruction","action":"click","target":[180,140,50,30],"text":"設定が完了したら保存してください","max_width":100}]}"#;
    let result = render_with_layout_from_json(input).unwrap();
    assert!(result.elements[1].bounds[2] <= 100.0);
    assert!(result.svg.contains("<tspan"));
}

#[test]
fn named_target_instruction_and_layout_result() {
    let input = r#"{
      "canvas": {"width": 400, "height": 300},
      "targets": {"save-button": [240, 180, 100, 36]},
      "annotations": [
        {"id": "step1", "type": "instruction", "target": "save-button", "action": "click", "text": "保存をクリック"}
      ]
    }"#;
    let result = render_with_layout_from_json(input).unwrap();
    assert!(result.svg.contains("保存をクリック"));
    assert_eq!(result.elements.len(), 2);
    assert_eq!(result.elements[0].id, "step1:focus");
    assert_eq!(result.elements[1].id, "step1:callout");
    assert_eq!(result.elements[0].bounds, [240.0, 180.0, 100.0, 36.0]);
    assert_eq!(result.elements[1].arrow_path.as_ref().unwrap().len(), 2);
}

#[test]
fn scene_without_annotations_remains_valid() {
    let scene = Scene::from_json(r#"{"canvas":{"width":100,"height":100}}"#).unwrap();
    assert!(scene.annotations.is_empty());
}

#[test]
fn validation_explains_typo_and_bad_target() {
    let typo = r#"{"canvas":{"width":400,"height":300},"annotations":[{"type":"callout","target":[10,20,30,40],"text":"x","style":"primari"}]}"#;
    assert!(Scene::from_json(typo).unwrap_err().to_string().contains("Did you mean 'primary'?"));
    let missing = r#"{"canvas":{"width":400,"height":300},"targets":{"save-button":[10,20,30,40]},"annotations":[{"type":"callout","target":"save-buttn","text":"x"}]}"#;
    assert!(Scene::from_json(missing).unwrap_err().to_string().contains("Did you mean 'save-button'?"));
    let bad_size = r#"{"canvas":{"width":400,"height":300},"annotations":[{"type":"callout","target":[10,20,0,40],"text":"x"}]}"#;
    assert!(Scene::from_json(bad_size).unwrap_err().to_string().contains("annotations[0].target"));
}

#[test]
fn drag_instruction_draws_path_between_named_targets() {
    let input = r#"{
      "canvas": {"width": 500, "height": 300},
      "targets": {"item": [40, 80, 60, 30], "drop-zone": [340, 180, 90, 50]},
      "annotations": [
        {"id":"move", "type":"instruction", "action":"drag", "target":"item", "destination":"drop-zone", "text":"ここへ移動"}
      ]
    }"#;
    let result = render_with_layout_from_json(input).unwrap();
    assert_eq!(result.elements.len(), 4);
    let path = result.elements.iter().find(|element| element.id == "move:path").unwrap();
    let points = path.arrow_path.as_ref().unwrap();
    assert_eq!(points[0], [70.0, 95.0]);
    assert_eq!(points[2], [385.0, 205.0]);
}

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
    assert!(svg.contains("<mask id=\"spotlight-mask-0\" maskUnits=\"userSpaceOnUse\""));

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
    assert!(svg.contains("stroke=\"#ffffff\""));
    assert!(svg.contains("<line x1=\"50\"")); // Divider
}

#[test]
fn test_step_arrow_full_workflow() {
    let json = r#"{
      "canvas": { "width": 800, "height": 600 },
      "annotations": [
        {
          "type": "step-arrow",
          "target": [300, 200, 150, 50],
          "step": 1,
          "style": "step",
          "position": "left"
        },
        {
          "type": "badge",
          "target": [300, 350, 150, 50],
          "step": 2,
          "arrow": true,
          "style": "primary",
          "position": "bottom"
        },
        {
          "type": "arrow",
          "target": [550, 200, 150, 50],
          "step": 3,
          "style": "danger",
          "position": "top"
        }
      ]
    }"#;

    let svg = render_from_json(json).expect("Should render step arrow SVG");
    assert!(svg.contains("marker-end=\"url(#arrowhead-step)\""));
    assert!(svg.contains("marker-end=\"url(#arrowhead-primary)\""));
    assert!(svg.contains("marker-end=\"url(#arrowhead-danger)\""));
    assert!(svg.contains(">1</text>"));
    assert!(svg.contains(">2</text>"));
    assert!(svg.contains(">3</text>"));
}

#[test]
fn test_bezier_arrow_full_workflow() {
    let json = r#"{
      "canvas": { "width": 1200, "height": 800 },
      "annotations": [
        {
          "type": "bezier-arrow",
          "start": [100, 300],
          "control": [350, 100],
          "end": [600, 300],
          "text": "非同期キュー転送",
          "style": "info"
        },
        {
          "type": "curved-arrow",
          "from": [650, 300],
          "via": [850, 500],
          "to": [1050, 300],
          "text": "DB書き込み",
          "style": "pink",
          "box": false,
          "offset": 15
        }
      ]
    }"#;

    let svg = render_from_json(json).expect("Should render bezier arrow SVG");
    assert!(svg.contains("<path d=\"M 100 300 Q "));
    assert!(svg.contains("<path d=\"M 650 300 Q "));
    assert_eq!(svg.matches("class=\"classic-arrow-head\"").count(), 2);
    assert!(svg.contains("非同期キュー転送"));
    assert!(svg.contains("DB書き込み"));
    assert!(svg.contains("paint-order=\"stroke fill\"")); // for unboxed text
}

#[test]
fn test_explicit_arrow_render() {
    let json = r#"{
      "canvas": { "width": 800, "height": 600 },
      "annotations": [
        {
          "type": "arrow",
          "start": [120, 150],
          "end": [450, 300],
          "style": "warning"
        },
        {
          "type": "arrow",
          "from": [50, 50],
          "to": [200, 100],
          "step": 3,
          "style": "step"
        }
      ]
    }"#;

    let svg = render_from_json(json).expect("Should render explicit arrow SVG");
    assert!(svg.contains("<line x1=\"120\" y1=\"150\""));
    assert!(svg.contains("class=\"classic-arrow-head\""));
    assert!(svg.contains("<line x1=\"50\" y1=\"50\""));
    assert!(svg.contains(">3</text>"));
}

#[test]
fn test_arrow_and_bezier_arrow_text_placement_middle_and_end() {
    let json = r#"{
      "canvas": { "width": 800, "height": 600 },
      "annotations": [
        {
          "type": "arrow",
          "start": [100, 100],
          "end": [500, 100],
          "text": "中間ラベル",
          "text_placement": "middle"
        },
        {
          "type": "arrow",
          "start": [100, 250],
          "end": [500, 250],
          "text": "終点ラベル",
          "text_placement": "end"
        },
        {
          "type": "bezier-arrow",
          "start": [100, 400],
          "control": [300, 300],
          "end": [500, 400],
          "text": "ベジェ終点",
          "text_placement": "end"
        }
      ]
    }"#;

    let svg = render_from_json(json).expect("Should render arrows with middle and end text");
    assert!(svg.contains("中間ラベル"));
    assert!(svg.contains("終点ラベル"));
    assert!(svg.contains("ベジェ終点"));
    let label_x = |label: &str| -> f64 {
        svg.lines()
            .find(|line| line.contains(label) && line.contains("<text x="))
            .and_then(|line| line.split("x=\"").nth(1))
            .and_then(|value| value.split('"').next())
            .unwrap()
            .parse()
            .unwrap()
    };
    assert!((label_x("中間ラベル") - 300.0).abs() < 1e-6);
    // End labels are outside the tail (the endpoint without the arrowhead).
    assert!(label_x("終点ラベル") < 100.0);
    assert!(label_x("ベジェ終点") < 100.0);
}

#[test]
fn test_uimap_and_named_ui_targets() {
    let input = r#"{
      "canvas": { "width": 800, "height": 600 },
      "uimap": [
        { "role": "button", "name": "保存", "x": 120, "y": 80, "width": 80, "height": 32 },
        { "role": "button", "name": "キャンセル", "x": 220, "y": 80, "width": 90, "height": 32 },
        { "role": "textbox", "name": "検索ワード", "x": 400, "y": 70, "width": 200, "height": 40 }
      ],
      "annotations": [
        { "type": "pin", "target": "保存", "text": "ここをクリック" },
        { "type": "rect", "target": "保存ボタン", "style": "warning" },
        { "type": "callout", "target": "検索ワード", "text": "キーワード入力" },
        { "type": "spotlight", "target": "button:2" }
      ]
    }"#;

    let result = render_with_layout_from_json(input).expect("Should render annotations targeting UIMap elements");
    assert!(result.svg.contains("ここをクリック"));
    assert!(result.svg.contains("キーワード入力"));
    // rect element should exactly match the target rectangle
    let rect_elem = result.elements.iter().find(|e| e.id.contains("annotation-1")).unwrap();
    assert_eq!(rect_elem.bounds, [120.0, 80.0, 80.0, 32.0]);
    // spotlight targeting "button:2" (キャンセル)
    let spot_elem = result.elements.iter().find(|e| e.id.contains("annotation-3")).unwrap();
    assert_eq!(spot_elem.bounds, [220.0, 80.0, 90.0, 32.0]);
}
