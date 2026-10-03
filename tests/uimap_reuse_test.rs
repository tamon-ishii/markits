use markits::{UiElement, raster};

#[test]
fn test_load_uimap_from_png_and_json() {
    let elements = vec![UiElement::new("button", "保存", 10.0, 20.0, 80.0, 30.0)];
    let tmp_dir = std::env::temp_dir();
    let json_path = tmp_dir.join("test_uimap_reuse.json");
    std::fs::write(&json_path, serde_json::to_string(&elements).unwrap()).unwrap();

    let loaded_json = raster::load_uimap_from_path(&json_path).unwrap();
    assert_eq!(loaded_json.len(), 1);
    assert_eq!(loaded_json[0].name, "保存");

    // Create a 10x10 PNG with embedded metadata
    let img = image::RgbaImage::new(10, 10);
    let mut png_bytes = Vec::new();
    img.write_to(&mut std::io::Cursor::new(&mut png_bytes), image::ImageFormat::Png).unwrap();
    let embedded = raster::embed_png_uimap(&png_bytes, &elements).unwrap();
    let png_path = tmp_dir.join("test_uimap_reuse.png");
    std::fs::write(&png_path, embedded).unwrap();

    let loaded_png = raster::load_uimap_from_path(&png_path).unwrap();
    assert_eq!(loaded_png.len(), 1);
    assert_eq!(loaded_png[0].name, "保存");

    // Test a PNG without embedded metadata
    let clean_png_path = tmp_dir.join("test_uimap_clean.png");
    std::fs::write(&clean_png_path, png_bytes).unwrap();
    assert!(raster::load_uimap_from_path(&clean_png_path).is_err());
}
