use crate::error::{MarkitsError, Result};
use serde::de::{self, Deserializer, MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Serialize};
use std::fmt;

/// Canvas dimensions defining the viewBox of the annotation SVG.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Canvas {
    pub width: u32,
    pub height: u32,
}

/// Target bounding box coordinates and dimensions.
///
/// Can be deserialized from either an object `{"x": ..., "y": ..., "width": ..., "height": ...}`
/// or a 4-element array `[x, y, width, height]`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct TargetRect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl TargetRect {
    pub fn new(x: f64, y: f64, width: f64, height: f64) -> Self {
        Self { x, y, width, height }
    }

    pub fn right(&self) -> f64 {
        self.x + self.width
    }

    pub fn bottom(&self) -> f64 {
        self.y + self.height
    }

    pub fn center_x(&self) -> f64 {
        self.x + self.width / 2.0
    }

    pub fn center_y(&self) -> f64 {
        self.y + self.height / 2.0
    }

    pub fn intersects(&self, other: &TargetRect) -> bool {
        self.x < other.right()
            && self.right() > other.x
            && self.y < other.bottom()
            && self.bottom() > other.y
    }
}

impl<'de> Deserialize<'de> for TargetRect {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct TargetRectVisitor;

        impl<'de> Visitor<'de> for TargetRectVisitor {
            type Value = TargetRect;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a 4-element array [x, y, width, height] or an object with x, y, width, height")
            }

            fn visit_seq<A>(self, mut seq: A) -> std::result::Result<TargetRect, A::Error>
            where
                A: SeqAccess<'de>,
            {
                let x = seq
                    .next_element::<f64>()?
                    .ok_or_else(|| de::Error::invalid_length(0, &"4 elements [x, y, width, height]"))?;
                let y = seq
                    .next_element::<f64>()?
                    .ok_or_else(|| de::Error::invalid_length(1, &"4 elements [x, y, width, height]"))?;
                let width = seq
                    .next_element::<f64>()?
                    .ok_or_else(|| de::Error::invalid_length(2, &"4 elements [x, y, width, height]"))?;
                let height = seq
                    .next_element::<f64>()?
                    .ok_or_else(|| de::Error::invalid_length(3, &"4 elements [x, y, width, height]"))?;

                Ok(TargetRect { x, y, width, height })
            }

            fn visit_map<M>(self, mut map: M) -> std::result::Result<TargetRect, M::Error>
            where
                M: MapAccess<'de>,
            {
                let mut x = None;
                let mut y = None;
                let mut width = None;
                let mut height = None;

                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        "x" => x = Some(map.next_value::<f64>()?),
                        "y" => y = Some(map.next_value::<f64>()?),
                        "width" | "w" => width = Some(map.next_value::<f64>()?),
                        "height" | "h" => height = Some(map.next_value::<f64>()?),
                        _ => {
                            let _ = map.next_value::<de::IgnoredAny>()?;
                        }
                    }
                }

                let x = x.ok_or_else(|| de::Error::missing_field("x"))?;
                let y = y.ok_or_else(|| de::Error::missing_field("y"))?;
                let width = width.ok_or_else(|| de::Error::missing_field("width"))?;
                let height = height.ok_or_else(|| de::Error::missing_field("height"))?;

                Ok(TargetRect { x, y, width, height })
            }
        }

        deserializer.deserialize_any(TargetRectVisitor)
    }
}

/// Semantic styling intent used to derive colors and strokes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum SemanticStyle {
    #[default]
    Primary,
    Secondary,
    Warning,
    Danger,
    Info,
    Step,
    Pink,
}

/// Position placement hint for annotations relative to target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum PositionHint {
    #[default]
    Auto,
    Top,
    Bottom,
    Left,
    Right,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

fn default_true() -> bool {
    true
}

/// Core annotation types supported by MarkIts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum Annotation {
    Arrow {
        target: TargetRect,
        #[serde(default)]
        text: Option<String>,
        #[serde(default)]
        style: SemanticStyle,
        #[serde(default)]
        position: PositionHint,
        #[serde(default)]
        shadow: Option<bool>,
    },
    Rect {
        target: TargetRect,
        #[serde(default)]
        style: SemanticStyle,
        #[serde(default)]
        shadow: Option<bool>,
    },
    #[serde(alias = "rounded_rect")]
    RoundedRect {
        target: TargetRect,
        #[serde(default)]
        rx: Option<f64>,
        #[serde(default)]
        ry: Option<f64>,
        #[serde(default)]
        style: SemanticStyle,
        #[serde(default)]
        shadow: Option<bool>,
    },
    #[serde(alias = "ellipse")]
    Circle {
        target: TargetRect,
        #[serde(default)]
        style: SemanticStyle,
        #[serde(default)]
        shadow: Option<bool>,
    },
    Label {
        target: TargetRect,
        text: String,
        #[serde(default)]
        style: SemanticStyle,
        #[serde(default)]
        position: PositionHint,
        #[serde(default)]
        shadow: Option<bool>,
        #[serde(default)]
        outline: Option<bool>,
    },
    Callout {
        target: TargetRect,
        text: String,
        #[serde(default)]
        style: SemanticStyle,
        #[serde(default)]
        position: PositionHint,
        #[serde(default)]
        shadow: Option<bool>,
        #[serde(default)]
        outline: Option<bool>,
    },
    Badge {
        target: TargetRect,
        #[serde(default)]
        step: Option<u32>,
        #[serde(default)]
        text: Option<String>,
        #[serde(default)]
        style: SemanticStyle,
        #[serde(default)]
        position: PositionHint,
        #[serde(default)]
        shadow: Option<bool>,
    },
    Spotlight {
        target: TargetRect,
        #[serde(default)]
        style: SemanticStyle,
    },
    #[serde(alias = "pin_callout", alias = "pin-callout")]
    Pin {
        target: TargetRect,
        #[serde(default)]
        icon: Option<String>,
        #[serde(default)]
        text: Option<String>,
        #[serde(default)]
        style: SemanticStyle,
        #[serde(default)]
        position: PositionHint,
        #[serde(default)]
        shadow: Option<bool>,
        #[serde(default)]
        outline: Option<bool>,
    },
    Bullseye {
        target: TargetRect,
        #[serde(default)]
        style: SemanticStyle,
        #[serde(default)]
        shadow: Option<bool>,
    },
    Divider {
        target: TargetRect,
        #[serde(default)]
        style: SemanticStyle,
    },
}

impl Annotation {
    pub fn target(&self) -> &TargetRect {
        match self {
            Annotation::Arrow { target, .. }
            | Annotation::Rect { target, .. }
            | Annotation::RoundedRect { target, .. }
            | Annotation::Circle { target, .. }
            | Annotation::Label { target, .. }
            | Annotation::Callout { target, .. }
            | Annotation::Badge { target, .. }
            | Annotation::Spotlight { target, .. }
            | Annotation::Pin { target, .. }
            | Annotation::Bullseye { target, .. }
            | Annotation::Divider { target, .. } => target,
        }
    }

    pub fn style(&self) -> SemanticStyle {
        match self {
            Annotation::Arrow { style, .. }
            | Annotation::Rect { style, .. }
            | Annotation::RoundedRect { style, .. }
            | Annotation::Circle { style, .. }
            | Annotation::Label { style, .. }
            | Annotation::Callout { style, .. }
            | Annotation::Badge { style, .. }
            | Annotation::Spotlight { style, .. }
            | Annotation::Pin { style, .. }
            | Annotation::Bullseye { style, .. }
            | Annotation::Divider { style, .. } => *style,
        }
    }

    pub fn position_hint(&self) -> PositionHint {
        match self {
            Annotation::Arrow { position, .. }
            | Annotation::Label { position, .. }
            | Annotation::Callout { position, .. }
            | Annotation::Badge { position, .. }
            | Annotation::Pin { position, .. } => *position,
            _ => PositionHint::Auto,
        }
    }

    pub fn shadow_override(&self) -> Option<bool> {
        match self {
            Annotation::Arrow { shadow, .. }
            | Annotation::Rect { shadow, .. }
            | Annotation::RoundedRect { shadow, .. }
            | Annotation::Circle { shadow, .. }
            | Annotation::Label { shadow, .. }
            | Annotation::Callout { shadow, .. }
            | Annotation::Badge { shadow, .. }
            | Annotation::Pin { shadow, .. }
            | Annotation::Bullseye { shadow, .. } => *shadow,
            Annotation::Spotlight { .. } | Annotation::Divider { .. } => None,
        }
    }

    pub fn outline_override(&self) -> Option<bool> {
        match self {
            Annotation::Label { outline, .. }
            | Annotation::Callout { outline, .. }
            | Annotation::Pin { outline, .. } => *outline,
            _ => None,
        }
    }
}

/// The top-level scene specification.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Scene {
    pub canvas: Canvas,
    #[serde(default = "default_true")]
    pub shadow: bool,
    #[serde(default)]
    pub annotations: Vec<Annotation>,
}

impl Scene {
    pub fn from_json(json_str: &str) -> Result<Self> {
        let scene: Self = serde_json::from_str(json_str)?;
        scene.validate()?;
        Ok(scene)
    }

    pub fn validate(&self) -> Result<()> {
        if self.canvas.width == 0 || self.canvas.height == 0 {
            return Err(MarkitsError::Validation(
                "Canvas dimensions must be greater than zero".to_string(),
            ));
        }

        for (idx, annotation) in self.annotations.iter().enumerate() {
            let target = annotation.target();
            if target.width <= 0.0 || target.height <= 0.0 {
                return Err(MarkitsError::Validation(format!(
                    "Annotation {} has invalid non-positive target dimensions (width: {}, height: {})",
                    idx, target.width, target.height
                )));
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_canvas_deserialization() {
        let json = r#"{"width": 1920, "height": 1080}"#;
        let canvas: Canvas = serde_json::from_str(json).unwrap();
        assert_eq!(canvas.width, 1920);
        assert_eq!(canvas.height, 1080);
    }

    #[test]
    fn test_target_rect_from_object() {
        let json = r#"{"x": 100.5, "y": 200.0, "width": 50.0, "height": 30.0}"#;
        let rect: TargetRect = serde_json::from_str(json).unwrap();
        assert_eq!(rect.x, 100.5);
        assert_eq!(rect.y, 200.0);
        assert_eq!(rect.width, 50.0);
        assert_eq!(rect.height, 30.0);
    }

    #[test]
    fn test_target_rect_from_array() {
        let json = r#"[820, 640, 100, 32]"#;
        let rect: TargetRect = serde_json::from_str(json).unwrap();
        assert_eq!(rect.x, 820.0);
        assert_eq!(rect.y, 640.0);
        assert_eq!(rect.width, 100.0);
        assert_eq!(rect.height, 32.0);
    }

    #[test]
    fn test_semantic_style_and_position_defaults() {
        let json = r#"{
            "type": "callout",
            "target": [10, 20, 30, 40],
            "text": "test"
        }"#;
        let ann: Annotation = serde_json::from_str(json).unwrap();
        assert_eq!(ann.style(), SemanticStyle::Primary);
        assert_eq!(ann.position_hint(), PositionHint::Auto);
    }

    #[test]
    fn test_explicit_style_and_position() {
        let json = r#"{
            "type": "callout",
            "target": [10, 20, 30, 40],
            "text": "test",
            "style": "warning",
            "position": "top-right"
        }"#;
        let ann: Annotation = serde_json::from_str(json).unwrap();
        assert_eq!(ann.style(), SemanticStyle::Warning);
        assert_eq!(ann.position_hint(), PositionHint::TopRight);
    }

    #[test]
    fn test_all_annotation_types_deserialization() {
        let json = r#"{
            "canvas": {"width": 1920, "height": 1080},
            "annotations": [
                {"type": "arrow", "target": [10, 20, 30, 40]},
                {"type": "rect", "target": [10, 20, 30, 40]},
                {"type": "rounded-rect", "target": [10, 20, 30, 40], "rx": 5.0},
                {"type": "circle", "target": [10, 20, 30, 40]},
                {"type": "label", "target": [10, 20, 30, 40], "text": "Label"},
                {"type": "callout", "target": [10, 20, 30, 40], "text": "Callout"},
                {"type": "badge", "target": [10, 20, 30, 40], "step": 1},
                {"type": "spotlight", "target": [10, 20, 30, 40]}
            ]
        }"#;

        let scene = Scene::from_json(json).expect("Failed to parse valid scene");
        assert_eq!(scene.annotations.len(), 8);
    }

    #[test]
    fn test_validation_rejects_zero_canvas() {
        let json = r#"{"canvas": {"width": 0, "height": 1080}, "annotations": []}"#;
        let res = Scene::from_json(json);
        assert!(res.is_err());
    }

    #[test]
    fn test_validation_rejects_negative_target() {
        let json = r#"{
            "canvas": {"width": 1920, "height": 1080},
            "annotations": [
                {"type": "rect", "target": [10, 20, -30, 40]}
            ]
        }"#;
        let res = Scene::from_json(json);
        assert!(res.is_err());
    }

    #[test]
    fn test_skitch_components_and_shadow_deserialization() {
        let json = r#"{
            "canvas": {"width": 1200, "height": 800},
            "shadow": false,
            "annotations": [
                {
                    "type": "pin",
                    "target": [100, 200, 50, 50],
                    "icon": "♡",
                    "text": "お気に入り",
                    "style": "pink",
                    "position": "bottom",
                    "shadow": true,
                    "outline": true
                },
                {
                    "type": "bullseye",
                    "target": [300, 300, 60, 60],
                    "style": "primary"
                },
                {
                    "type": "divider",
                    "target": [0, 400, 1200, 4],
                    "style": "pink"
                }
            ]
        }"#;

        let scene = Scene::from_json(json).expect("Failed to parse Skitch components");
        assert!(!scene.shadow);
        assert_eq!(scene.annotations.len(), 3);

        if let Annotation::Pin { icon, text, shadow, outline, .. } = &scene.annotations[0] {
            assert_eq!(icon.as_deref(), Some("♡"));
            assert_eq!(text.as_deref(), Some("お気に入り"));
            assert_eq!(*shadow, Some(true));
            assert_eq!(*outline, Some(true));
        } else {
            panic!("Expected Pin annotation");
        }
    }
}
