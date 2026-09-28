use crate::error::Result;
use crate::layout::{LayoutEngine, Point, ResolvedAnnotation};
use crate::model::{Scene, TargetRect};
use crate::renderer::SvgRenderer;
use crate::semantic::resolve_json;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct LayoutElement {
    pub id: String,
    pub source_id: String,
    pub kind: String,
    pub bounds: [f64; 4],
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arrow_path: Option<Vec<[f64; 2]>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RenderOutput {
    pub svg: String,
    pub elements: Vec<LayoutElement>,
}

fn box_bounds(r: &TargetRect) -> [f64; 4] {
    [r.x, r.y, r.width, r.height]
}

fn point_bounds(points: &[Point]) -> [f64; 4] {
    let min_x = points.iter().map(|p| p.x).fold(f64::INFINITY, f64::min);
    let max_x = points.iter().map(|p| p.x).fold(f64::NEG_INFINITY, f64::max);
    let min_y = points.iter().map(|p| p.y).fold(f64::INFINITY, f64::min);
    let max_y = points.iter().map(|p| p.y).fold(f64::NEG_INFINITY, f64::max);
    [min_x, min_y, max_x - min_x, max_y - min_y]
}

fn bezier_bounds(start: Point, control: Point, end: Point) -> [f64; 4] {
    let mut points = vec![start, end];
    for (a, b, c) in [(start.x, control.x, end.x), (start.y, control.y, end.y)] {
        let denominator = a - 2.0 * b + c;
        if denominator.abs() > 1e-12 {
            let t = (a - b) / denominator;
            if (0.0..1.0).contains(&t) {
                let u = 1.0 - t;
                points.push(Point::new(
                    u * u * start.x + 2.0 * u * t * control.x + t * t * end.x,
                    u * u * start.y + 2.0 * u * t * control.y + t * t * end.y,
                ));
            }
        }
    }
    point_bounds(&points)
}

fn path(points: &[Point]) -> Option<Vec<[f64; 2]>> {
    Some(points.iter().map(|p| [p.x, p.y]).collect())
}

fn element(id: String, source_id: String, ann: &ResolvedAnnotation) -> LayoutElement {
    let (kind, bounds, arrow_path) = match ann {
        ResolvedAnnotation::Rect { rect, .. } => ("rect", box_bounds(rect), None),
        ResolvedAnnotation::RoundedRect { rect, .. } => ("rounded-rect", box_bounds(rect), None),
        ResolvedAnnotation::Circle { cx, cy, rx, ry, .. } => {
            ("circle", [cx - rx, cy - ry, rx * 2.0, ry * 2.0], None)
        }
        ResolvedAnnotation::Label { box_rect, .. } => ("label", box_bounds(box_rect), None),
        ResolvedAnnotation::Callout {
            box_rect,
            arrow_start,
            arrow_end,
            ..
        } => (
            "callout",
            box_bounds(box_rect),
            path(&[*arrow_start, *arrow_end]),
        ),
        ResolvedAnnotation::Badge { center, radius, .. } => (
            "badge",
            [
                center.x - radius,
                center.y - radius,
                radius * 2.0,
                radius * 2.0,
            ],
            None,
        ),
        ResolvedAnnotation::StepArrow {
            center,
            radius,
            arrow_start,
            arrow_end,
            ..
        } => (
            "step-arrow",
            [
                center.x - radius,
                center.y - radius,
                radius * 2.0,
                radius * 2.0,
            ],
            path(&[*arrow_start, *arrow_end]),
        ),
        ResolvedAnnotation::Spotlight { target, .. } => ("spotlight", box_bounds(target), None),
        ResolvedAnnotation::Arrow { start, end, .. } => (
            "arrow",
            point_bounds(&[*start, *end]),
            path(&[*start, *end]),
        ),
        ResolvedAnnotation::Pin {
            head_center,
            head_radius,
            tip,
            text_rect,
            ..
        } => {
            let mut b = [
                head_center.x - head_radius,
                head_center.y - head_radius,
                head_radius * 2.0,
                head_radius * 2.0,
            ];
            let include = |b: &mut [f64; 4], x: f64, y: f64| {
                let right = (b[0] + b[2]).max(x);
                let bottom = (b[1] + b[3]).max(y);
                b[0] = b[0].min(x);
                b[1] = b[1].min(y);
                b[2] = right - b[0];
                b[3] = bottom - b[1];
            };
            include(&mut b, tip.x, tip.y);
            if let Some(r) = text_rect {
                include(&mut b, r.x, r.y);
                include(&mut b, r.right(), r.bottom());
            }
            ("pin", b, None)
        }
        ResolvedAnnotation::Bullseye {
            center,
            outer_radius,
            ..
        } => (
            "bullseye",
            [
                center.x - outer_radius,
                center.y - outer_radius,
                outer_radius * 2.0,
                outer_radius * 2.0,
            ],
            None,
        ),
        ResolvedAnnotation::Divider { start, end, .. } => {
            ("divider", point_bounds(&[*start, *end]), None)
        }
        ResolvedAnnotation::BezierArrow {
            start,
            control,
            end,
            text_rect,
            ..
        } => {
            let mut b = bezier_bounds(*start, *control, *end);
            if let Some(r) = text_rect {
                let right = (b[0] + b[2]).max(r.right());
                let bottom = (b[1] + b[3]).max(r.bottom());
                b[0] = b[0].min(r.x);
                b[1] = b[1].min(r.y);
                b[2] = right - b[0];
                b[3] = bottom - b[1];
            }
            ("bezier-arrow", b, path(&[*start, *control, *end]))
        }
    };
    LayoutElement {
        id,
        source_id,
        kind: kind.to_string(),
        bounds,
        arrow_path,
    }
}

fn render_scene_with_ids(scene: &Scene, source_ids: &[String]) -> Result<RenderOutput> {
    scene.validate()?;
    let resolved = LayoutEngine::new().layout_scene(scene);
    let svg = SvgRenderer::new().render_scene(&resolved);
    let mut totals = std::collections::HashMap::<String, usize>::new();
    for source_id in source_ids {
        *totals.entry(source_id.clone()).or_default() += 1;
    }
    let mut source_counts = std::collections::HashMap::<String, usize>::new();
    let mut used_ids = std::collections::HashSet::<String>::new();
    let elements = resolved
        .annotations
        .iter()
        .enumerate()
        .map(|(index, ann)| {
            let source_id = source_ids
                .get(index)
                .cloned()
                .unwrap_or_else(|| format!("annotation-{index}"));
            let total = totals.get(&source_id).copied().unwrap_or(1);
            let part = source_counts.entry(source_id.clone()).or_default();
            let mut id = if total == 1 {
                source_id.clone()
            } else {
                format!("{source_id}:{part}")
            };
            *part += 1;
            if !used_ids.insert(id.clone()) {
                id = format!("{id}:{index}");
                while !used_ids.insert(id.clone()) {
                    id.push('_');
                }
            }
            element(id, source_id, ann)
        })
        .collect();
    Ok(RenderOutput { svg, elements })
}

impl Scene {
    /// Renders SVG and returns geometry for each resolved annotation.
    pub fn render_with_layout(&self) -> Result<RenderOutput> {
        let ids: Vec<String> = (0..self.annotations.len())
            .map(|index| format!("annotation-{index}"))
            .collect();
        render_scene_with_ids(self, &ids)
    }
}

/// Renders JSON once and returns SVG plus geometry for each resolved annotation.
pub fn render_with_layout_from_json(json_str: &str) -> Result<RenderOutput> {
    let input = resolve_json(json_str)?;
    render_scene_with_ids(&input.scene, &input.source_ids)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quadratic_bounds_use_curve_extremum() {
        assert_eq!(
            bezier_bounds(
                Point::new(0.0, 0.0),
                Point::new(50.0, 100.0),
                Point::new(100.0, 0.0)
            ),
            [0.0, 0.0, 100.0, 50.0]
        );
    }
}
