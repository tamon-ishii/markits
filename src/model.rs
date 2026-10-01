use crate::error::{MarkitsError, Result};
use serde::de::{self, Deserializer, MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Serialize};
use std::fmt;

/// Canvas dimensions defining the viewBox of the annotation SVG.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
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

fn default_ui_role() -> String {
    "control".to_string()
}

/// A detected UI element (button, textbox, window, etc.) in a UIMap.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UiElement {
    #[serde(default = "default_ui_role")]
    pub role: String,
    #[serde(default)]
    pub name: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl UiElement {
    pub fn new(role: impl Into<String>, name: impl Into<String>, x: f64, y: f64, width: f64, height: f64) -> Self {
        Self {
            role: role.into(),
            name: name.into(),
            x,
            y,
            width,
            height,
        }
    }

    pub fn rect(&self) -> TargetRect {
        TargetRect::new(self.x, self.y, self.width, self.height)
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

                if seq.next_element::<de::IgnoredAny>()?.is_some() {
                    return Err(de::Error::invalid_length(5, &"exactly 4 elements [x, y, width, height]"));
                }

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
                        _ => return Err(de::Error::unknown_field(&key, &["x", "y", "width", "height", "w", "h"])),
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

/// 2D point coordinates for vectors, curves, and anchors.
///
/// Can be deserialized from either an object `{"x": ..., "y": ...}`
/// or a 2-element array `[x, y]`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Point2D {
    pub x: f64,
    pub y: f64,
}

impl Point2D {
    pub fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
}

impl<'de> Deserialize<'de> for Point2D {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct Point2DVisitor;

        impl<'de> Visitor<'de> for Point2DVisitor {
            type Value = Point2D;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a 2-element array [x, y] or an object with x, y")
            }

            fn visit_seq<A>(self, mut seq: A) -> std::result::Result<Point2D, A::Error>
            where
                A: SeqAccess<'de>,
            {
                let x = seq
                    .next_element::<f64>()?
                    .ok_or_else(|| de::Error::invalid_length(0, &"2 elements [x, y]"))?;
                let y = seq
                    .next_element::<f64>()?
                    .ok_or_else(|| de::Error::invalid_length(1, &"2 elements [x, y]"))?;

                if seq.next_element::<de::IgnoredAny>()?.is_some() {
                    return Err(de::Error::invalid_length(3, &"exactly 2 elements [x, y]"));
                }

                Ok(Point2D { x, y })
            }

            fn visit_map<M>(self, mut map: M) -> std::result::Result<Point2D, M::Error>
            where
                M: MapAccess<'de>,
            {
                let mut x = None;
                let mut y = None;

                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        "x" => x = Some(map.next_value::<f64>()?),
                        "y" => y = Some(map.next_value::<f64>()?),
                        _ => return Err(de::Error::unknown_field(&key, &["x", "y"])),
                    }
                }

                let x = x.ok_or_else(|| de::Error::missing_field("x"))?;
                let y = y.ok_or_else(|| de::Error::missing_field("y"))?;

                Ok(Point2D { x, y })
            }
        }

        deserializer.deserialize_any(Point2DVisitor)
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
    Center,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum ArrowTextPlacement {
    #[default]
    #[serde(alias = "center", alias = "mid")]
    Middle,
    #[serde(alias = "tip", alias = "head", alias = "endpoint")]
    End,
}

fn default_true() -> bool {
    true
}

/// Core annotation types supported by MarkIts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case", deny_unknown_fields)]
pub enum Annotation {
    Arrow {
        #[serde(default)]
        target: Option<TargetRect>,
        #[serde(default, alias = "from", alias = "p0", alias = "start_point")]
        start: Option<Point2D>,
        #[serde(default, alias = "to", alias = "p1", alias = "end_point")]
        end: Option<Point2D>,
        #[serde(default, alias = "stroke_width", alias = "width", alias = "thickness", alias = "line_width")]
        stroke_width: Option<f64>,
        #[serde(default)]
        step: Option<u32>,
        #[serde(default)]
        text: Option<String>,
        #[serde(
            default,
            alias = "text_placement",
            alias = "text_position",
            alias = "placement",
            alias = "text_pos",
            alias = "text_anchor"
        )]
        text_placement: Option<ArrowTextPlacement>,
        #[serde(default)]
        style: SemanticStyle,
        #[serde(default)]
        position: PositionHint,
        #[serde(default)]
        shadow: Option<bool>,
        #[serde(default)]
        outline: Option<bool>,
        #[serde(
            default,
            alias = "box",
            alias = "enclosure",
            alias = "frame",
            alias = "pill",
            alias = "badge",
            alias = "background"
        )]
        boxed: Option<bool>,
    },
    Rect {
        target: TargetRect,
        #[serde(default, alias = "stroke_width", alias = "width", alias = "thickness", alias = "line_width")]
        stroke_width: Option<f64>,
        #[serde(default)]
        style: SemanticStyle,
        #[serde(default)]
        shadow: Option<bool>,
    },
    #[serde(alias = "rounded_rect")]
    RoundedRect {
        target: TargetRect,
        #[serde(default, alias = "stroke_width", alias = "width", alias = "thickness", alias = "line_width")]
        stroke_width: Option<f64>,
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
        #[serde(default, alias = "stroke_width", alias = "width", alias = "thickness", alias = "line_width")]
        stroke_width: Option<f64>,
        #[serde(default)]
        style: SemanticStyle,
        #[serde(default)]
        shadow: Option<bool>,
    },
    Label {
        target: TargetRect,
        text: String,
        #[serde(default)]
        max_width: Option<f64>,
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
        max_width: Option<f64>,
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
        #[serde(default)]
        arrow: Option<bool>,
    },
    #[serde(
        alias = "step_arrow",
        alias = "number-arrow",
        alias = "number_arrow",
        alias = "numbered-arrow",
        alias = "numbered_arrow",
        alias = "arrow-badge",
        alias = "badge-arrow",
        alias = "step-pin"
    )]
    StepArrow {
        target: TargetRect,
        #[serde(default, alias = "stroke_width", alias = "width", alias = "thickness", alias = "line_width")]
        stroke_width: Option<f64>,
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
    #[serde(
        alias = "bezier_arrow",
        alias = "curved-arrow",
        alias = "curved_arrow",
        alias = "curve-arrow",
        alias = "curve_arrow",
        alias = "bezier"
    )]
    BezierArrow {
        #[serde(default)]
        target: Option<TargetRect>,
        #[serde(default, alias = "from", alias = "p0", alias = "start_point")]
        start: Option<Point2D>,
        #[serde(
            default,
            alias = "mid",
            alias = "middle",
            alias = "via",
            alias = "p1",
            alias = "intermediate",
            alias = "control_point"
        )]
        control: Option<Point2D>,
        #[serde(default, alias = "to", alias = "p2", alias = "end_point")]
        end: Option<Point2D>,
        #[serde(default, alias = "stroke_width", alias = "width", alias = "thickness", alias = "line_width")]
        stroke_width: Option<f64>,
        #[serde(default)]
        text: Option<String>,
        #[serde(default)]
        style: SemanticStyle,
        #[serde(default)]
        position: PositionHint,
        #[serde(
            default,
            alias = "text_placement",
            alias = "text_position",
            alias = "placement",
            alias = "text_pos",
            alias = "text_anchor"
        )]
        text_placement: Option<ArrowTextPlacement>,
        #[serde(
            default,
            alias = "gap",
            alias = "distance",
            alias = "text_offset",
            alias = "spacing"
        )]
        offset: Option<f64>,
        #[serde(default, alias = "ratio", alias = "progress", alias = "along")]
        t: Option<f64>,
        #[serde(default)]
        shadow: Option<bool>,
        #[serde(default)]
        outline: Option<bool>,
        #[serde(
            default,
            alias = "box",
            alias = "enclosure",
            alias = "frame",
            alias = "pill",
            alias = "badge",
            alias = "background"
        )]
        boxed: Option<bool>,
    },
}

impl Annotation {
    pub fn target(&self) -> TargetRect {
        match self {
            Annotation::Arrow { target, start, end, .. } => {
                if let Some(t) = target {
                    *t
                } else {
                    let s = start.unwrap_or(Point2D::new(0.0, 0.0));
                    let e = end.unwrap_or(Point2D::new(100.0, 100.0));
                    let min_x = s.x.min(e.x);
                    let max_x = s.x.max(e.x);
                    let min_y = s.y.min(e.y);
                    let max_y = s.y.max(e.y);
                    TargetRect::new(min_x, min_y, (max_x - min_x).max(1.0), (max_y - min_y).max(1.0))
                }
            }
            Annotation::Rect { target, .. }
            | Annotation::RoundedRect { target, .. }
            | Annotation::Circle { target, .. }
            | Annotation::Label { target, .. }
            | Annotation::Callout { target, .. }
            | Annotation::Badge { target, .. }
            | Annotation::Spotlight { target, .. }
            | Annotation::Pin { target, .. }
            | Annotation::Bullseye { target, .. }
            | Annotation::Divider { target, .. }
            | Annotation::StepArrow { target, .. } => *target,
            Annotation::BezierArrow { target, start, control, end, .. } => {
                if let Some(t) = target {
                    *t
                } else {
                    let s = start.unwrap_or(Point2D::new(0.0, 0.0));
                    let c = control.unwrap_or(Point2D::new(50.0, 50.0));
                    let e = end.unwrap_or(Point2D::new(100.0, 100.0));
                    let min_x = s.x.min(c.x).min(e.x);
                    let max_x = s.x.max(c.x).max(e.x);
                    let min_y = s.y.min(c.y).min(e.y);
                    let max_y = s.y.max(c.y).max(e.y);
                    TargetRect::new(min_x, min_y, (max_x - min_x).max(1.0), (max_y - min_y).max(1.0))
                }
            }
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
            | Annotation::Divider { style, .. }
            | Annotation::StepArrow { style, .. }
            | Annotation::BezierArrow { style, .. } => *style,
        }
    }

    pub fn position_hint(&self) -> PositionHint {
        match self {
            Annotation::Arrow { position, .. }
            | Annotation::Label { position, .. }
            | Annotation::Callout { position, .. }
            | Annotation::Badge { position, .. }
            | Annotation::Pin { position, .. }
            | Annotation::StepArrow { position, .. }
            | Annotation::BezierArrow { position, .. } => *position,
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
            | Annotation::Bullseye { shadow, .. }
            | Annotation::StepArrow { shadow, .. }
            | Annotation::BezierArrow { shadow, .. } => *shadow,
            Annotation::Spotlight { .. } | Annotation::Divider { .. } => None,
        }
    }

    pub fn outline_override(&self) -> Option<bool> {
        match self {
            Annotation::Label { outline, .. }
            | Annotation::Callout { outline, .. }
            | Annotation::Pin { outline, .. }
            | Annotation::Arrow { outline, .. }
            | Annotation::BezierArrow { outline, .. } => *outline,
            _ => None,
        }
    }
}

/// The top-level scene specification.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Scene {
    pub canvas: Canvas,
    #[serde(default = "default_true")]
    pub shadow: bool,
    #[serde(default)]
    pub annotations: Vec<Annotation>,
    #[serde(default, skip_serializing_if = "Option::is_none", alias = "ui_map", alias = "ui_elements")]
    pub uimap: Option<Vec<UiElement>>,
}

impl Scene {
    pub fn from_json(json_str: &str) -> Result<Self> {
        let prepared = crate::semantic::prepare(json_str)?;
        let scene: Self = serde_json::from_value(prepared.scene)?;
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
            if let Annotation::Arrow { target, start, end, .. } = annotation {
                if target.is_none() && (start.is_none() || end.is_none()) {
                    return Err(MarkitsError::Validation(format!(
                        "annotations[{idx}]: arrow requires either 'target' or both 'start' and 'end'"
                    )));
                }
            }
            if let Annotation::Label { max_width: Some(width), .. } | Annotation::Callout { max_width: Some(width), .. } = annotation {
                if !width.is_finite() || *width < 40.0 {
                    return Err(MarkitsError::Validation(format!("annotations[{idx}].max_width must be a finite number >= 40")));
                }
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

    #[test]
    fn test_step_arrow_deserialization() {
        let json = r#"{
            "canvas": {"width": 800, "height": 600},
            "annotations": [
                {
                    "type": "step-arrow",
                    "target": [100, 100, 200, 50],
                    "step": 1,
                    "style": "step",
                    "position": "bottom"
                },
                {
                    "type": "number-arrow",
                    "target": [200, 200, 100, 50],
                    "step": 2
                },
                {
                    "type": "badge",
                    "target": [300, 300, 100, 50],
                    "step": 3,
                    "arrow": true
                }
            ]
        }"#;

        let scene = Scene::from_json(json).expect("Failed to parse step arrow scene");
        assert_eq!(scene.annotations.len(), 3);
        match &scene.annotations[0] {
            Annotation::StepArrow { step, style, position, .. } => {
                assert_eq!(*step, Some(1));
                assert_eq!(*style, SemanticStyle::Step);
                assert_eq!(*position, PositionHint::Bottom);
            }
            _ => panic!("Expected StepArrow"),
        }
    }

    #[test]
    fn test_bezier_arrow_deserialization() {
        let json = r#"{
            "canvas": {"width": 1000, "height": 800},
            "annotations": [
                {
                    "type": "bezier-arrow",
                    "start": [100, 200],
                    "control": [200, 100],
                    "end": [300, 200],
                    "text": "データ連携",
                    "style": "primary"
                },
                {
                    "type": "curved-arrow",
                    "from": {"x": 50, "y": 60},
                    "via": {"x": 150, "y": 10},
                    "to": {"x": 250, "y": 60},
                    "text": "処理完了",
                    "style": "pink",
                    "shadow": true
                }
            ]
        }"#;

        let scene = Scene::from_json(json).expect("Failed to parse bezier arrow scene");
        assert_eq!(scene.annotations.len(), 2);

        if let Annotation::BezierArrow { start, control, end, text, style, .. } = &scene.annotations[0] {
            assert_eq!(*start, Some(Point2D::new(100.0, 200.0)));
            assert_eq!(*control, Some(Point2D::new(200.0, 100.0)));
            assert_eq!(*end, Some(Point2D::new(300.0, 200.0)));
            assert_eq!(text.as_deref(), Some("データ連携"));
            assert_eq!(*style, SemanticStyle::Primary);
        } else {
            panic!("Expected BezierArrow");
        }

        if let Annotation::BezierArrow { start, control, end, text, style, shadow, boxed, position, .. } = &scene.annotations[1] {
            assert_eq!(*start, Some(Point2D::new(50.0, 60.0)));
            assert_eq!(*control, Some(Point2D::new(150.0, 10.0)));
            assert_eq!(*end, Some(Point2D::new(250.0, 60.0)));
            assert_eq!(text.as_deref(), Some("処理完了"));
            assert_eq!(*style, SemanticStyle::Pink);
            assert_eq!(*shadow, Some(true));
            assert_eq!(*boxed, None);
            assert_eq!(*position, PositionHint::Auto);
        } else {
            panic!("Expected CurvedArrow");
        }

        let json_unboxed = r#"{
            "canvas": {"width": 500, "height": 500},
            "annotations": [
                {
                    "type": "bezier-arrow",
                    "start": [10, 10],
                    "end": [100, 100],
                    "text": "No Frame",
                    "box": false,
                    "position": "top"
                }
            ]
        }"#;
        let scene_unboxed = Scene::from_json(json_unboxed).unwrap();
        if let Annotation::BezierArrow { boxed, position, .. } = &scene_unboxed.annotations[0] {
            assert_eq!(*boxed, Some(false));
            assert_eq!(*position, PositionHint::Top);
        } else {
            panic!("Expected unboxed BezierArrow");
        }
    }

    #[test]
    fn test_arrow_with_explicit_start_end() {
        let json = r#"{
            "canvas": {"width": 800, "height": 600},
            "annotations": [
                {
                    "type": "arrow",
                    "start": [50, 60],
                    "end": [200, 300],
                    "style": "primary"
                }
            ]
        }"#;
        let scene = Scene::from_json(json).unwrap();
        if let Annotation::Arrow { start, end, style, .. } = &scene.annotations[0] {
            assert_eq!(*start, Some(Point2D::new(50.0, 60.0)));
            assert_eq!(*end, Some(Point2D::new(200.0, 300.0)));
            assert_eq!(*style, SemanticStyle::Primary);
        } else {
            panic!("Expected Arrow");
        }
    }
}
