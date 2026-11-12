//! Versioned, self-contained launch and window-operation recipe.
use crate::{metadata, recording::OperationRecording};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EmbeddedScript {
    pub name: String,
    pub content: String,
    /// Interpreter executable; inferred from extension when omitted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runner: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LaunchSpec {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub program: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub script: Option<EmbeddedScript>,
    #[serde(default)]
    pub args: Vec<String>,
    pub cwd: PathBuf,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WindowBounds {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CaptureRecipe {
    pub version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub launch: Option<LaunchSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub window: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub window_bounds: Option<WindowBounds>,
    #[serde(default = "default_timeout")]
    pub timeout_ms: u64,
    #[serde(default = "default_settle")]
    pub settle_ms: u64,
    #[serde(default)]
    pub include_cursor: bool,
    #[serde(default = "default_depth")]
    pub ui_depth: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recording: Option<OperationRecording>,
    /// Screenshot point represented by this image; omitted to generate all points.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capture_index: Option<usize>,
}
fn default_timeout() -> u64 {
    30000
}
fn default_settle() -> u64 {
    1000
}
fn default_depth() -> usize {
    64
}

impl CaptureRecipe {
    pub fn validate(&self) -> Result<(), String> {
        if self.version != 1 {
            return Err("Unsupported capture recipe version".into());
        }
        if self.timeout_ms == 0 || self.timeout_ms > 3600000 || self.settle_ms > 3600000 {
            return Err("Timeout must be 1..3600000 ms; settle time must be 0..3600000 ms".into());
        }
        if !(1..=128).contains(&self.ui_depth) {
            return Err("Recipe UI depth must be 1..128".into());
        }
        if let Some(index) = self.capture_index {
            let count = self.recording.as_ref().map_or(0, |r| {
                r.events
                    .iter()
                    .filter(|e| matches!(e, crate::recording::RecordedEvent::Screenshot { .. }))
                    .count()
            });
            if index >= count.max(1) {
                return Err("Recipe capture_index is outside screenshot points".into());
            }
        }
        if self.window.as_deref().is_some_and(|s| s.trim().is_empty()) {
            return Err("Window selector cannot be empty".into());
        }
        if let Some(bounds) = self.window_bounds {
            if bounds.width == 0
                || bounds.height == 0
                || bounds.width > i32::MAX as u32
                || bounds.height > i32::MAX as u32
            {
                return Err("Invalid window bounds".into());
            }
        }
        if let Some(launch) = &self.launch {
            if launch.program.is_some() == launch.script.is_some() {
                return Err("Specify exactly one of launch.program and launch.script".into());
            }
            let cwd = launch.cwd.to_string_lossy();
            let windows_absolute = cwd.starts_with("\\\\")
                || (cwd.as_bytes().get(1) == Some(&b':')
                    && cwd.as_bytes().first().is_some_and(u8::is_ascii_alphabetic)
                    && cwd
                        .as_bytes()
                        .get(2)
                        .is_some_and(|c| *c == b'\\' || *c == b'/'));
            if !launch.cwd.is_absolute() && !windows_absolute {
                return Err("Launch cwd must be an absolute path".into());
            }
            if launch
                .program
                .as_deref()
                .is_some_and(|p| p.trim().is_empty() || p.contains('\0'))
                || launch.args.iter().any(|a| a.contains('\0'))
            {
                return Err("Invalid executable or argument".into());
            }
            if let Some(script) = &launch.script {
                if script.name.is_empty()
                    || script.name.contains(['/', '\\'])
                    || script.name == "."
                    || script.name == ".."
                    || script.name.contains(':')
                    || script.content.contains('\0')
                {
                    return Err("Invalid embedded script name or content".into());
                }
                if script
                    .runner
                    .as_deref()
                    .is_some_and(|s| s.trim().is_empty() || s.contains('\0'))
                {
                    return Err("Invalid script runner".into());
                }
            }
        } else if self.window.is_none() {
            return Err("A launch specification or window selector is required".into());
        }
        Ok(())
    }
}

pub fn extract(bytes: &[u8]) -> Result<CaptureRecipe, String> {
    let png = crate::image_export::to_editable_png(bytes)?;
    let json = metadata::extract_text_chunk(&png, crate::recording_metadata::RECIPE_KEY)
        .map_err(|e| e.to_string())?
        .ok_or("Image has no capture recipe")?;
    let recipe: CaptureRecipe = serde_json::from_str(&json).map_err(|e| e.to_string())?;
    recipe.validate()?;
    Ok(recipe)
}
pub fn embed(png: &[u8], recipe: &CaptureRecipe) -> Result<Vec<u8>, String> {
    recipe.validate()?;
    metadata::embed_text_chunk(
        png,
        crate::recording_metadata::RECIPE_KEY,
        &serde_json::to_string(recipe).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())
}
