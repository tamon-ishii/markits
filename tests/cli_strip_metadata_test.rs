use std::{fs, process::Command};

#[test]
fn strip_metadata_removes_edit_data_without_modifying_the_original() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("private.png");
    let output = dir.path().join("share.png");
    let mut png = std::io::Cursor::new(Vec::new());
    image::DynamicImage::new_rgb8(16, 12).write_to(&mut png, image::ImageFormat::Png).unwrap();
    let original = png.into_inner();
    let tagged = markits::raster::embed_png_text_chunk(&original, "markits:base_image", "hidden original").unwrap();
    let tagged = markits::raster::embed_png_text_chunk(&tagged, "markits:operations", "private input").unwrap();
    fs::write(&input, &tagged).unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_markits")).args(["strip-metadata"]).arg(&input).arg("-o").arg(&output).output().unwrap();
    assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
    assert_eq!(fs::read(&output).unwrap(), original);
    assert_eq!(fs::read(&input).unwrap(), tagged);
    let result = Command::new(env!("CARGO_BIN_EXE_markits")).arg("strip-metadata").arg(&input).arg("-o").arg(&input).output().unwrap();
    assert!(!result.status.success());
    assert_eq!(fs::read(&input).unwrap(), tagged);
}
