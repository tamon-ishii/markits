//! Metadata shared by capture, history, and operation playback.
use crate::metadata;

pub const OPERATIONS_KEY: &str = "markits:operations";
pub const GROUP_KEY: &str = "markits:operation_group";
pub const RECIPE_KEY: &str = "markits:capture_recipe";

pub fn copy_metadata(target: &[u8], source: &[u8]) -> Result<Vec<u8>, metadata::MetadataError> {
    let mut result = target.to_vec();
    if source.starts_with(b"\x89PNG\r\n\x1a\n") {
        for key in [
            OPERATIONS_KEY,
            GROUP_KEY,
            RECIPE_KEY,
            metadata::MARKITS_CAPTURE_TARGET_KEYWORD,
        ] {
            if let Some(value) = metadata::extract_text_chunk(source, key)? {
                result = metadata::embed_text_chunk(&result, key, &value)?;
            }
        }
    }
    Ok(result)
}
