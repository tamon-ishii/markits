#[test]
fn test_crop_rgba_buffer() {
    let mut img = image::RgbaImage::new(100, 100);
    img.put_pixel(10, 10, image::Rgba([255, 0, 0, 255]));
    let cropped = markits::capture::crop_rgba_image(&img, 10, 10, 20, 20).unwrap();
    assert_eq!(cropped.width(), 20);
    assert_eq!(cropped.height(), 20);
    assert_eq!(*cropped.get_pixel(0, 0), image::Rgba([255, 0, 0, 255]));
}
