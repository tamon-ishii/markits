use crate::model::{Annotation, Canvas, PositionHint, Scene, SemanticStyle, TargetRect};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

impl Point {
    pub fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    pub fn distance_to(&self, other: &Point) -> f64 {
        ((self.x - other.x).powi(2) + (self.y - other.y).powi(2)).sqrt()
    }
}

/// Estimated box dimensions for rendering text or badges.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Dimensions {
    pub width: f64,
    pub height: f64,
}

impl Dimensions {
    pub fn new(width: f64, height: f64) -> Self {
        Self { width, height }
    }
}

/// Estimates bounding box dimensions for text annotations based on character metrics.
pub fn estimate_text_dimensions(text: &str, font_size: f64) -> Dimensions {
    let padding_x = 12.0;
    let padding_y = 6.0;
    let line_height = font_size * 1.35;

    let lines: Vec<&str> = text.lines().collect();
    let num_lines = lines.len().max(1) as f64;

    let mut max_line_width: f64 = 0.0;
    for line in lines {
        let mut line_w: f64 = 0.0;
        for c in line.chars() {
            if c.is_ascii() {
                line_w += font_size * 0.60;
            } else {
                // Wider for CJK and multibyte characters
                line_w += font_size * 1.05;
            }
        }
        if line_w > max_line_width {
            max_line_width = line_w;
        }
    }

    let min_width = 36.0;
    let min_height = 24.0;

    let total_width = (max_line_width + padding_x * 2.0).max(min_width);
    let total_height = (num_lines * line_height + padding_y * 2.0).max(min_height);

    Dimensions::new(total_width, total_height)
}

/// 8 surrounding candidate anchors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnchorPosition {
    Top,
    Bottom,
    Left,
    Right,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

impl AnchorPosition {
    pub const ALL: [AnchorPosition; 8] = [
        AnchorPosition::Top,
        AnchorPosition::Bottom,
        AnchorPosition::Right,
        AnchorPosition::Left,
        AnchorPosition::TopRight,
        AnchorPosition::TopLeft,
        AnchorPosition::BottomRight,
        AnchorPosition::BottomLeft,
    ];

    pub fn matches_hint(&self, hint: PositionHint) -> bool {
        match (self, hint) {
            (AnchorPosition::Top, PositionHint::Top) => true,
            (AnchorPosition::Bottom, PositionHint::Bottom) => true,
            (AnchorPosition::Left, PositionHint::Left) => true,
            (AnchorPosition::Right, PositionHint::Right) => true,
            (AnchorPosition::TopLeft, PositionHint::TopLeft) => true,
            (AnchorPosition::TopRight, PositionHint::TopRight) => true,
            (AnchorPosition::BottomLeft, PositionHint::BottomLeft) => true,
            (AnchorPosition::BottomRight, PositionHint::BottomRight) => true,
            _ => false,
        }
    }

    pub fn baseline_bias(&self) -> f64 {
        match self {
            AnchorPosition::Top => 0.0,
            AnchorPosition::Right => 1.0,
            AnchorPosition::Bottom => 2.0,
            AnchorPosition::Left => 3.0,
            AnchorPosition::TopRight => 5.0,
            AnchorPosition::TopLeft => 6.0,
            AnchorPosition::BottomRight => 7.0,
            AnchorPosition::BottomLeft => 8.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Candidate {
    pub anchor: AnchorPosition,
    pub rect: TargetRect,
}

/// Generates candidate placement boxes surrounding the target rectangle.
pub fn generate_candidates(
    target: &TargetRect,
    box_dim: Dimensions,
    offset: f64,
) -> Vec<Candidate> {
    let bw = box_dim.width;
    let bh = box_dim.height;

    AnchorPosition::ALL
        .iter()
        .map(|&anchor| {
            let (x, y) = match anchor {
                AnchorPosition::Top => (target.center_x() - bw / 2.0, target.y - offset - bh),
                AnchorPosition::Bottom => (target.center_x() - bw / 2.0, target.bottom() + offset),
                AnchorPosition::Left => (target.x - offset - bw, target.center_y() - bh / 2.0),
                AnchorPosition::Right => (target.right() + offset, target.center_y() - bh / 2.0),
                AnchorPosition::TopLeft => (target.x - offset - bw, target.y - offset - bh),
                AnchorPosition::TopRight => (target.right() + offset, target.y - offset - bh),
                AnchorPosition::BottomLeft => (target.x - offset - bw, target.bottom() + offset),
                AnchorPosition::BottomRight => (target.right() + offset, target.bottom() + offset),
            };
            Candidate {
                anchor,
                rect: TargetRect::new(x, y, bw, bh),
            }
        })
        .collect()
}

/// Computes penalty score for a candidate placement. Lower is better.
pub fn calculate_score(
    candidate: &Candidate,
    target: &TargetRect,
    canvas: &Canvas,
    hint: PositionHint,
    occupied_rects: &[TargetRect],
) -> f64 {
    let mut penalty = 0.0;
    let c = &candidate.rect;

    // 1. Canvas Boundary Penalty
    let canvas_w = canvas.width as f64;
    let canvas_h = canvas.height as f64;

    let overflow_left = (-c.x).max(0.0);
    let overflow_top = (-c.y).max(0.0);
    let overflow_right = (c.right() - canvas_w).max(0.0);
    let overflow_bottom = (c.bottom() - canvas_h).max(0.0);

    let overflow_distance = overflow_left + overflow_top + overflow_right + overflow_bottom;
    if overflow_distance > 0.0 {
        penalty += 10000.0 + overflow_distance * 100.0;
    }

    // 2. Target Overlap Penalty
    if c.intersects(target) {
        let overlap_w = (c.right().min(target.right()) - c.x.max(target.x)).max(0.0);
        let overlap_h = (c.bottom().min(target.bottom()) - c.y.max(target.y)).max(0.0);
        penalty += 50000.0 + (overlap_w * overlap_h) * 10.0;
    }

    // 3. Peer Overlap Penalty
    for occ in occupied_rects {
        if c.intersects(occ) {
            let overlap_w = (c.right().min(occ.right()) - c.x.max(occ.x)).max(0.0);
            let overlap_h = (c.bottom().min(occ.bottom()) - c.y.max(occ.y)).max(0.0);
            penalty += 20000.0 + (overlap_w * overlap_h) * 10.0;
        }
    }

    // 4. Position Hint Weighting
    if hint != PositionHint::Auto {
        if candidate.anchor.matches_hint(hint) {
            penalty -= 200.0; // Strong bonus for requested hint
        } else {
            penalty += 300.0; // Penalty for deviation
        }
    } else {
        penalty += candidate.anchor.baseline_bias();
    }

    // 5. Offset Distance
    let center_dist = Point::new(c.center_x(), c.center_y())
        .distance_to(&Point::new(target.center_x(), target.center_y()));
    penalty += center_dist * 0.05;

    penalty
}

/// Deterministically selects the best candidate for placement.
pub fn select_best_candidate(
    candidates: &[Candidate],
    target: &TargetRect,
    canvas: &Canvas,
    hint: PositionHint,
    occupied_rects: &[TargetRect],
) -> Candidate {
    let mut scored: Vec<(&Candidate, f64)> = candidates
        .iter()
        .map(|c| {
            let score = calculate_score(c, target, canvas, hint, occupied_rects);
            (c, score)
        })
        .collect();

    // Deterministic sort: lower score first. On tie, stable order by baseline_bias
    scored.sort_by(|a, b| {
        a.1.total_cmp(&b.1)
            .then_with(|| a.0.anchor.baseline_bias().total_cmp(&b.0.anchor.baseline_bias()))
    });

    scored.first().map(|(c, _)| (*c).clone()).unwrap_or_else(|| {
        candidates
            .first()
            .cloned()
            .unwrap_or_else(|| Candidate {
                anchor: AnchorPosition::Top,
                rect: *target,
            })
    })
}

/// Computes connecting arrow start (from label/callout box) and end (to target edge) points.
pub fn calculate_arrow_connection(
    callout_box: &TargetRect,
    target: &TargetRect,
    anchor: AnchorPosition,
) -> (Point, Point) {
    match anchor {
        AnchorPosition::Top => (
            Point::new(callout_box.center_x(), callout_box.bottom()),
            Point::new(target.center_x(), target.y),
        ),
        AnchorPosition::Bottom => (
            Point::new(callout_box.center_x(), callout_box.y),
            Point::new(target.center_x(), target.bottom()),
        ),
        AnchorPosition::Left => (
            Point::new(callout_box.right(), callout_box.center_y()),
            Point::new(target.x, target.center_y()),
        ),
        AnchorPosition::Right => (
            Point::new(callout_box.x, callout_box.center_y()),
            Point::new(target.right(), target.center_y()),
        ),
        AnchorPosition::TopLeft => (
            Point::new(callout_box.right(), callout_box.bottom()),
            Point::new(target.x, target.y),
        ),
        AnchorPosition::TopRight => (
            Point::new(callout_box.x, callout_box.bottom()),
            Point::new(target.right(), target.y),
        ),
        AnchorPosition::BottomLeft => (
            Point::new(callout_box.right(), callout_box.y),
            Point::new(target.x, target.bottom()),
        ),
        AnchorPosition::BottomRight => (
            Point::new(callout_box.x, callout_box.y),
            Point::new(target.right(), target.bottom()),
        ),
    }
}

/// Layout-resolved annotation with exact absolute geometry ready for SVG emission.
#[derive(Debug, Clone, PartialEq)]
pub enum ResolvedAnnotation {
    Arrow {
        start: Point,
        end: Point,
        text: Option<String>,
        style: SemanticStyle,
        shadow: bool,
    },
    Rect {
        rect: TargetRect,
        style: SemanticStyle,
        shadow: bool,
    },
    RoundedRect {
        rect: TargetRect,
        rx: f64,
        ry: f64,
        style: SemanticStyle,
        shadow: bool,
    },
    Circle {
        cx: f64,
        cy: f64,
        rx: f64,
        ry: f64,
        style: SemanticStyle,
        shadow: bool,
    },
    Label {
        box_rect: TargetRect,
        text: String,
        style: SemanticStyle,
        shadow: bool,
        outline: bool,
    },
    Callout {
        box_rect: TargetRect,
        text: String,
        arrow_start: Point,
        arrow_end: Point,
        style: SemanticStyle,
        shadow: bool,
        outline: bool,
    },
    Badge {
        center: Point,
        radius: f64,
        label: String,
        style: SemanticStyle,
        shadow: bool,
    },
    Spotlight {
        target: TargetRect,
        style: SemanticStyle,
    },
    Pin {
        head_center: Point,
        head_radius: f64,
        tip: Point,
        icon: Option<String>,
        text: Option<String>,
        text_rect: Option<TargetRect>,
        style: SemanticStyle,
        shadow: bool,
        outline: bool,
    },
    Bullseye {
        center: Point,
        outer_radius: f64,
        inner_radius: f64,
        dot_radius: f64,
        style: SemanticStyle,
        shadow: bool,
    },
    Divider {
        start: Point,
        end: Point,
        style: SemanticStyle,
        stroke_width: f64,
    },
}

/// Resolved scene containing all positioned annotations and the canvas.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedScene {
    pub canvas: Canvas,
    pub annotations: Vec<ResolvedAnnotation>,
}

impl ResolvedScene {
    pub fn has_any_shadow(&self) -> bool {
        self.annotations.iter().any(|ann| match ann {
            ResolvedAnnotation::Arrow { shadow, .. }
            | ResolvedAnnotation::Rect { shadow, .. }
            | ResolvedAnnotation::RoundedRect { shadow, .. }
            | ResolvedAnnotation::Circle { shadow, .. }
            | ResolvedAnnotation::Label { shadow, .. }
            | ResolvedAnnotation::Callout { shadow, .. }
            | ResolvedAnnotation::Badge { shadow, .. }
            | ResolvedAnnotation::Pin { shadow, .. }
            | ResolvedAnnotation::Bullseye { shadow, .. } => *shadow,
            _ => false,
        })
    }
}

/// Engine to execute layout on a Scene.
pub struct LayoutEngine {
    pub font_size: f64,
    pub callout_offset: f64,
    pub badge_radius: f64,
    pub pin_head_radius: f64,
}

impl Default for LayoutEngine {
    fn default() -> Self {
        Self {
            font_size: 14.0,
            callout_offset: 16.0,
            badge_radius: 14.0,
            pin_head_radius: 18.0,
        }
    }
}

impl LayoutEngine {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn layout_scene(&self, scene: &Scene) -> ResolvedScene {
        let mut occupied_rects: Vec<TargetRect> = Vec::new();
        let mut resolved: Vec<ResolvedAnnotation> = Vec::new();

        for ann in &scene.annotations {
            let shadow = ann.shadow_override().unwrap_or(scene.shadow);
            let outline = ann.outline_override().unwrap_or(true);

            match ann {
                Annotation::Rect { target, style, .. } => {
                    resolved.push(ResolvedAnnotation::Rect {
                        rect: *target,
                        style: *style,
                        shadow,
                    });
                    occupied_rects.push(*target);
                }
                Annotation::RoundedRect {
                    target,
                    rx,
                    ry,
                    style,
                    ..
                } => {
                    let rx_val = rx.unwrap_or(8.0);
                    let ry_val = ry.unwrap_or(8.0);
                    resolved.push(ResolvedAnnotation::RoundedRect {
                        rect: *target,
                        rx: rx_val,
                        ry: ry_val,
                        style: *style,
                        shadow,
                    });
                    occupied_rects.push(*target);
                }
                Annotation::Circle { target, style, .. } => {
                    resolved.push(ResolvedAnnotation::Circle {
                        cx: target.center_x(),
                        cy: target.center_y(),
                        rx: target.width / 2.0,
                        ry: target.height / 2.0,
                        style: *style,
                        shadow,
                    });
                    occupied_rects.push(*target);
                }
                Annotation::Spotlight { target, style } => {
                    resolved.push(ResolvedAnnotation::Spotlight {
                        target: *target,
                        style: *style,
                    });
                }
                Annotation::Label {
                    target,
                    text,
                    style,
                    position,
                    ..
                } => {
                    let dim = estimate_text_dimensions(text, self.font_size);
                    let candidates = generate_candidates(target, dim, self.callout_offset);
                    let best = select_best_candidate(
                        &candidates,
                        target,
                        &scene.canvas,
                        *position,
                        &occupied_rects,
                    );
                    occupied_rects.push(best.rect);
                    resolved.push(ResolvedAnnotation::Label {
                        box_rect: best.rect,
                        text: text.clone(),
                        style: *style,
                        shadow,
                        outline,
                    });
                }
                Annotation::Callout {
                    target,
                    text,
                    style,
                    position,
                    ..
                } => {
                    let dim = estimate_text_dimensions(text, self.font_size);
                    let candidates = generate_candidates(target, dim, self.callout_offset);
                    let best = select_best_candidate(
                        &candidates,
                        target,
                        &scene.canvas,
                        *position,
                        &occupied_rects,
                    );
                    let (arrow_start, arrow_end) =
                        calculate_arrow_connection(&best.rect, target, best.anchor);
                    occupied_rects.push(best.rect);
                    resolved.push(ResolvedAnnotation::Callout {
                        box_rect: best.rect,
                        text: text.clone(),
                        arrow_start,
                        arrow_end,
                        style: *style,
                        shadow,
                        outline,
                    });
                }
                Annotation::Badge {
                    target,
                    step,
                    text,
                    style,
                    position,
                    ..
                } => {
                    let badge_label = if let Some(s) = step {
                        s.to_string()
                    } else if let Some(t) = text {
                        t.clone()
                    } else {
                        "1".to_string()
                    };

                    let dim = Dimensions::new(self.badge_radius * 2.0, self.badge_radius * 2.0);
                    let candidates = generate_candidates(target, dim, 8.0);
                    let best = select_best_candidate(
                        &candidates,
                        target,
                        &scene.canvas,
                        *position,
                        &occupied_rects,
                    );
                    let center = Point::new(best.rect.center_x(), best.rect.center_y());
                    occupied_rects.push(best.rect);
                    resolved.push(ResolvedAnnotation::Badge {
                        center,
                        radius: self.badge_radius,
                        label: badge_label,
                        style: *style,
                        shadow,
                    });
                }
                Annotation::Arrow {
                    target,
                    text,
                    style,
                    position,
                    ..
                } => {
                    let dim = Dimensions::new(32.0, 32.0);
                    let candidates = generate_candidates(target, dim, self.callout_offset);
                    let best = select_best_candidate(
                        &candidates,
                        target,
                        &scene.canvas,
                        *position,
                        &occupied_rects,
                    );
                    let (start, end) = calculate_arrow_connection(&best.rect, target, best.anchor);
                    resolved.push(ResolvedAnnotation::Arrow {
                        start,
                        end,
                        text: text.clone(),
                        style: *style,
                        shadow,
                    });
                }
                Annotation::Pin {
                    target,
                    icon,
                    text,
                    style,
                    position,
                    ..
                } => {
                    let r = self.pin_head_radius;
                    let tip_offset = 6.0;

                    let (head_center, tip) = match position {
                        PositionHint::Bottom => (
                            Point::new(target.center_x(), target.bottom() + tip_offset + r),
                            Point::new(target.center_x(), target.bottom()),
                        ),
                        PositionHint::Left => (
                            Point::new(target.x - tip_offset - r, target.center_y()),
                            Point::new(target.x, target.center_y()),
                        ),
                        PositionHint::Right => (
                            Point::new(target.right() + tip_offset + r, target.center_y()),
                            Point::new(target.right(), target.center_y()),
                        ),
                        _ => (
                            // Default Top
                            Point::new(target.center_x(), target.y - tip_offset - r),
                            Point::new(target.center_x(), target.y),
                        ),
                    };

                    let text_rect = if let Some(txt) = text {
                        let text_dim = estimate_text_dimensions(txt, self.font_size);
                        let pill_x = if head_center.x + r + text_dim.width > scene.canvas.width as f64 {
                            head_center.x - r - 4.0 - text_dim.width
                        } else {
                            head_center.x + r + 4.0
                        };
                        let pill_y = head_center.y - text_dim.height / 2.0;
                        Some(TargetRect::new(pill_x, pill_y, text_dim.width, text_dim.height))
                    } else {
                        None
                    };

                    resolved.push(ResolvedAnnotation::Pin {
                        head_center,
                        head_radius: r,
                        tip,
                        icon: icon.clone(),
                        text: text.clone(),
                        text_rect,
                        style: *style,
                        shadow,
                        outline,
                    });
                }
                Annotation::Bullseye { target, style, .. } => {
                    let center = Point::new(target.center_x(), target.center_y());
                    let outer_radius = (target.width.min(target.height) / 2.0).clamp(16.0, 36.0);
                    let inner_radius = outer_radius * 0.55;
                    let dot_radius = (outer_radius * 0.22).max(3.0);

                    resolved.push(ResolvedAnnotation::Bullseye {
                        center,
                        outer_radius,
                        inner_radius,
                        dot_radius,
                        style: *style,
                        shadow,
                    });
                }
                Annotation::Divider { target, style } => {
                    let start;
                    let end;
                    let stroke_width;
                    if target.width >= target.height {
                        start = Point::new(target.x, target.center_y());
                        end = Point::new(target.right(), target.center_y());
                        stroke_width = target.height.clamp(3.0, 8.0);
                    } else {
                        start = Point::new(target.center_x(), target.y);
                        end = Point::new(target.center_x(), target.bottom());
                        stroke_width = target.width.clamp(3.0, 8.0);
                    }

                    resolved.push(ResolvedAnnotation::Divider {
                        start,
                        end,
                        style: *style,
                        stroke_width,
                    });
                }
            }
        }

        ResolvedScene {
            canvas: scene.canvas,
            annotations: resolved,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_estimate_text_dimensions() {
        let dim = estimate_text_dimensions("Save", 14.0);
        assert!(dim.width > 24.0);
        assert!(dim.height >= 24.0);

        let longer = estimate_text_dimensions("Click the button to save all your project settings", 14.0);
        assert!(longer.width > dim.width);
    }

    #[test]
    fn test_candidate_generation() {
        let target = TargetRect::new(100.0, 100.0, 50.0, 50.0);
        let dim = Dimensions::new(40.0, 20.0);
        let candidates = generate_candidates(&target, dim, 10.0);

        assert_eq!(candidates.len(), 8);

        // Top candidate should be positioned above target.y
        let top = candidates.iter().find(|c| c.anchor == AnchorPosition::Top).unwrap();
        assert_eq!(top.rect.bottom(), target.y - 10.0);

        // Right candidate should be positioned right of target.right()
        let right = candidates.iter().find(|c| c.anchor == AnchorPosition::Right).unwrap();
        assert_eq!(right.rect.x, target.right() + 10.0);
    }

    #[test]
    fn test_canvas_overflow_penalty() {
        let target = TargetRect::new(10.0, 10.0, 50.0, 50.0);
        let canvas = Canvas { width: 100, height: 100 };
        let dim = Dimensions::new(40.0, 20.0);
        let candidates = generate_candidates(&target, dim, 10.0);

        // Top candidate overflows canvas (y < 0)
        let top = candidates.iter().find(|c| c.anchor == AnchorPosition::Top).unwrap();
        let score_top = calculate_score(top, &target, &canvas, PositionHint::Auto, &[]);

        // Bottom candidate is inside canvas (10+50+10=70 < 100)
        let bottom = candidates.iter().find(|c| c.anchor == AnchorPosition::Bottom).unwrap();
        let score_bottom = calculate_score(bottom, &target, &canvas, PositionHint::Auto, &[]);

        assert!(score_top > score_bottom);
    }

    #[test]
    fn test_position_hint_weighting() {
        let target = TargetRect::new(500.0, 500.0, 50.0, 50.0);
        let canvas = Canvas { width: 1000, height: 1000 };
        let dim = Dimensions::new(40.0, 20.0);
        let candidates = generate_candidates(&target, dim, 10.0);

        let best = select_best_candidate(&candidates, &target, &canvas, PositionHint::Right, &[]);
        assert_eq!(best.anchor, AnchorPosition::Right);
    }

    #[test]
    fn test_deterministic_selection() {
        let target = TargetRect::new(500.0, 500.0, 50.0, 50.0);
        let canvas = Canvas { width: 1000, height: 1000 };
        let dim = Dimensions::new(40.0, 20.0);
        let candidates = generate_candidates(&target, dim, 10.0);

        let sel1 = select_best_candidate(&candidates, &target, &canvas, PositionHint::Auto, &[]);
        let sel2 = select_best_candidate(&candidates, &target, &canvas, PositionHint::Auto, &[]);

        assert_eq!(sel1.anchor, sel2.anchor);
        assert_eq!(sel1.rect, sel2.rect);
    }

    #[test]
    fn test_arrow_connection_routing() {
        let target = TargetRect::new(100.0, 100.0, 50.0, 50.0);
        let callout = TargetRect::new(100.0, 20.0, 50.0, 30.0);
        let (start, end) = calculate_arrow_connection(&callout, &target, AnchorPosition::Top);

        assert_eq!(start.x, 125.0);
        assert_eq!(start.y, 50.0);
        assert_eq!(end.x, 125.0);
        assert_eq!(end.y, 100.0);
    }

    #[test]
    fn test_pin_and_bullseye_layout() {
        let scene = Scene {
            canvas: Canvas { width: 1000, height: 1000 },
            shadow: true,
            annotations: vec![
                Annotation::Pin {
                    target: TargetRect::new(200.0, 200.0, 40.0, 40.0),
                    icon: Some("♡".to_string()),
                    text: Some("Like".to_string()),
                    style: SemanticStyle::Pink,
                    position: PositionHint::Top,
                    shadow: None,
                    outline: None,
                },
                Annotation::Bullseye {
                    target: TargetRect::new(500.0, 500.0, 60.0, 60.0),
                    style: SemanticStyle::Primary,
                    shadow: Some(false),
                },
            ],
        };

        let engine = LayoutEngine::new();
        let resolved = engine.layout_scene(&scene);

        assert_eq!(resolved.annotations.len(), 2);
        if let ResolvedAnnotation::Pin { tip, text_rect, shadow, .. } = &resolved.annotations[0] {
            assert_eq!(tip.x, 220.0);
            assert_eq!(tip.y, 200.0);
            assert!(text_rect.is_some());
            assert!(*shadow);
        } else {
            panic!("Expected ResolvedAnnotation::Pin");
        }

        if let ResolvedAnnotation::Bullseye { center, shadow, .. } = &resolved.annotations[1] {
            assert_eq!(center.x, 530.0);
            assert_eq!(center.y, 530.0);
            assert!(!*shadow);
        } else {
            panic!("Expected ResolvedAnnotation::Bullseye");
        }
    }
}
