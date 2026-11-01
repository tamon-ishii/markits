//! Remove MarkIts edit data without decoding or re-encoding image pixels.
const PAYLOAD_START: &str = "<markits:payload>";
const PAYLOAD_END: &str = "</markits:payload>";

fn clean_xmp(data: &[u8]) -> Result<Vec<u8>, String> {
    if !data
        .windows(PAYLOAD_START.len())
        .any(|s| s == PAYLOAD_START.as_bytes())
    {
        return Ok(data.to_vec());
    }
    let mut xml = std::str::from_utf8(data)
        .map_err(|e| e.to_string())?
        .to_string();
    while let Some(start) = xml.find(PAYLOAD_START) {
        let end = xml[start..]
            .find(PAYLOAD_END)
            .ok_or("Incomplete MarkIts XMP payload")?
            + start
            + PAYLOAD_END.len();
        xml.replace_range(start..end, "");
    }
    Ok(xml.into_bytes())
}

/// Remove all `markits:*` PNG text chunks or MarkIts XMP payloads in WebP/JXL.
/// Other metadata (including color profiles) and compressed image data are kept.
pub fn strip_markits_metadata(bytes: &[u8]) -> Result<Vec<u8>, String> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return strip_png(bytes);
    }
    if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
        return strip_container(bytes, true);
    }
    if bytes.starts_with(b"\0\0\0\x0cJXL \x0d\x0a\x87\x0a") {
        return strip_container(bytes, false);
    }
    // A bare JPEG XL codestream has no metadata container to strip.
    if bytes.starts_with(b"\xff\x0a") {
        return Ok(bytes.to_vec());
    }
    Err("Metadata stripping supports PNG, WebP, and JPEG XL".into())
}

fn strip_png(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let mut output = bytes[..8].to_vec();
    let mut offset = 8usize;
    let mut ended = false;
    while offset < bytes.len() {
        let header = bytes
            .get(offset..offset.checked_add(8).ok_or("Invalid PNG offset")?)
            .ok_or("Truncated PNG chunk")?;
        let length = u32::from_be_bytes(header[..4].try_into().unwrap()) as usize;
        let end = offset
            .checked_add(12)
            .and_then(|n| n.checked_add(length))
            .filter(|&n| n <= bytes.len())
            .ok_or("Truncated PNG data")?;
        let kind = &header[4..];
        let data = &bytes[offset + 8..end - 4];
        let is_text = [b"tEXt", b"zTXt", b"iTXt"].contains(&kind.try_into().unwrap());
        let is_markits = is_text
            && data
                .split(|&b| b == 0)
                .next()
                .is_some_and(|key| key.starts_with(b"markits:"));
        if !is_markits {
            output.extend_from_slice(&bytes[offset..end]);
        }
        offset = end;
        if kind == b"IEND" {
            ended = true;
            break;
        }
    }
    if !ended || offset != bytes.len() {
        return Err("Invalid PNG end".into());
    }
    Ok(output)
}

fn strip_container(bytes: &[u8], webp: bool) -> Result<Vec<u8>, String> {
    let (mut offset, end) = if webp {
        let end = (u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize)
            .checked_add(8)
            .filter(|&n| n == bytes.len())
            .ok_or("Invalid WebP size")?;
        (12usize, end)
    } else {
        (0, bytes.len())
    };
    let mut output = if webp {
        bytes[..12].to_vec()
    } else {
        Vec::new()
    };
    while offset < end {
        let header = bytes
            .get(offset..offset.checked_add(8).ok_or("Invalid box offset")?)
            .filter(|_| offset + 8 <= end)
            .ok_or("Truncated box")?;
        let (kind, size, header_len) = if webp {
            (
                &header[..4],
                (u32::from_le_bytes(header[4..].try_into().unwrap()) as usize)
                    .checked_add(8)
                    .ok_or("Box too large")?,
                8,
            )
        } else {
            let size = u32::from_be_bytes(header[..4].try_into().unwrap());
            let (size, header_len) = match size {
                0 => (end - offset, 8),
                1 => {
                    let large = bytes
                        .get(offset + 8..offset + 16)
                        .filter(|_| offset + 16 <= end)
                        .ok_or("Truncated large box")?;
                    (
                        usize::try_from(u64::from_be_bytes(large.try_into().unwrap()))
                            .map_err(|_| "Box too large")?,
                        16,
                    )
                }
                n => (n as usize, 8),
            };
            (&header[4..], size, header_len)
        };
        let next = offset
            .checked_add(size)
            .filter(|&n| size >= header_len && n <= end)
            .ok_or("Invalid box size")?;
        let padded = next
            .checked_add(if webp { size % 2 } else { 0 })
            .filter(|&n| n <= end)
            .ok_or("Invalid padding")?;
        let data = &bytes[offset + header_len..next];
        if kind == if webp { b"XMP " } else { b"xml " } {
            let cleaned = clean_xmp(data)?;
            if cleaned == data {
                output.extend_from_slice(&bytes[offset..padded]);
            } else if webp {
                output.extend_from_slice(kind);
                output.extend_from_slice(
                    &u32::try_from(cleaned.len())
                        .map_err(|_| "XMP too large")?
                        .to_le_bytes(),
                );
                output.extend_from_slice(&cleaned);
                if cleaned.len() % 2 != 0 {
                    output.push(0);
                }
            } else {
                output.extend_from_slice(
                    &u32::try_from(cleaned.len() + 8)
                        .map_err(|_| "XMP too large")?
                        .to_be_bytes(),
                );
                output.extend_from_slice(kind);
                output.extend_from_slice(&cleaned);
            }
        } else {
            output.extend_from_slice(&bytes[offset..padded]);
        }
        offset = padded;
    }
    if webp {
        let size = u32::try_from(output.len() - 8).map_err(|_| "WebP too large")?;
        output[4..8].copy_from_slice(&size.to_le_bytes());
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn png_strips_every_markits_key_and_keeps_pixels_and_other_text() {
        let mut png = std::io::Cursor::new(Vec::new());
        image::DynamicImage::new_rgba8(5, 3)
            .write_to(&mut png, image::ImageFormat::Png)
            .unwrap();
        let original = png.into_inner();
        let mut tagged =
            crate::raster::embed_png_text_chunk(&original, "Author", "Keep me").unwrap();
        for key in [
            "markits:source_image",
            "markits:base_image",
            "markits:crop_info",
            "markits:operations",
            "markits:ui_elements",
            "markits:annotations",
            "markits:future",
        ] {
            tagged = crate::raster::embed_png_text_chunk(&tagged, key, "secret").unwrap();
        }
        let stripped = strip_markits_metadata(&tagged).unwrap();
        assert_eq!(
            crate::raster::extract_png_text_chunk(&stripped, "Author").as_deref(),
            Some("Keep me")
        );
        assert!(!stripped.windows(8).any(|s| s == b"markits:"));
        assert_eq!(
            image::load_from_memory(&stripped).unwrap().into_rgba8(),
            image::load_from_memory(&original).unwrap().into_rgba8()
        );
        assert_eq!(strip_markits_metadata(&stripped).unwrap(), stripped);
    }

    #[test]
    fn rejects_truncated_containers_without_panicking() {
        for bytes in [
            b"\x89PNG\r\n\x1a\n\0\0\0".as_slice(),
            b"RIFF\xff\xff\xff\xffWEBP",
            b"\0\0\0\x0cJXL \x0d\x0a\x87\x0a\0\0\0\x01xml ",
        ] {
            assert!(strip_markits_metadata(bytes).is_err());
        }
    }
}
