use crate::error::Result;
use crate::layout::{LayoutEngine, Point, ResolvedAnnotation, ResolvedScene};
use crate::model::{ArrowSkin, ArrowheadStyle, LineStyle, Scene, SemanticStyle};
use crate::theme::Theme;
use serde::Serialize;

fn arrow_dash(style: LineStyle) -> &'static str {
    match style { LineStyle::Solid => "", LineStyle::Dashed => " stroke-dasharray=\"9 6\"", LineStyle::Dotted => " stroke-dasharray=\"2 5\"" }
}

fn classic_arrow(points: &[Point], curve_control: Option<Point>, color: &str, width: f64, line_style: LineStyle, arrowhead: ArrowheadStyle, filter: &str) -> String {
    let Some(tip) = points.last() else { return String::new(); };
    let length: f64 = points.windows(2).map(|pair| pair[0].distance_to(&pair[1])).sum();
    if length < 5.0 {
        return format!("  <line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" stroke=\"{color}\" stroke-width=\"{width}\" stroke-linecap=\"round\"{filter}/>\n", points[0].x, points[0].y, tip.x, tip.y);
    }
    let head_length = (width * 3.5).clamp(18.0, 30.0);
    let head_half = (width * 2.3).clamp(10.0, 22.0);
    let stop_distance = match arrowhead {
        ArrowheadStyle::Filled => length - head_length + width.min(head_length * 0.25),
        ArrowheadStyle::Open => length - width * 0.5,
    }.clamp(0.0, length);
    let mut distance = 0.0;
    let mut stop_t = 1.0;
    let mut stop_point = *tip;
    for (index, pair) in points.windows(2).enumerate() {
        let segment = pair[0].distance_to(&pair[1]);
        if distance + segment >= stop_distance && segment > f64::EPSILON {
            let t = (stop_distance - distance) / segment;
            stop_t = (index as f64 + t) / (points.len() - 1) as f64;
            stop_point = Point::new(pair[0].x + (pair[1].x - pair[0].x) * t, pair[0].y + (pair[1].y - pair[0].y) * t);
            break;
        }
        distance += segment;
    }
    let shaft_shape = if let Some(control) = curve_control {
        let start = points[0];
        let control_x = start.x + (control.x - start.x) * stop_t;
        let control_y = start.y + (control.y - start.y) * stop_t;
        let inv = 1.0 - stop_t;
        let end_x = inv * inv * start.x + 2.0 * inv * stop_t * control.x + stop_t * stop_t * tip.x;
        let end_y = inv * inv * start.y + 2.0 * inv * stop_t * control.y + stop_t * stop_t * tip.y;
        format!("<path d=\"M {} {} Q {:.2} {:.2} {:.2} {:.2}\"", start.x, start.y, control_x, control_y, end_x, end_y)
    } else {
        format!("<line x1=\"{}\" y1=\"{}\" x2=\"{:.2}\" y2=\"{:.2}\"", points[0].x, points[0].y, stop_point.x, stop_point.y)
    };
    let shaft = format!("    {shaft_shape} class=\"classic-arrow-shaft\" fill=\"none\" stroke=\"{color}\" stroke-width=\"{width}\" stroke-linecap=\"round\"{} />\n", arrow_dash(line_style));
    let previous = points[points.len() - 2];
    let dx = tip.x - previous.x;
    let dy = tip.y - previous.y;
    let tangent_length = (dx * dx + dy * dy).sqrt().max(f64::EPSILON);
    let ux = dx / tangent_length;
    let uy = dy / tangent_length;
    let base_x = tip.x - ux * head_length;
    let base_y = tip.y - uy * head_length;
    let left_x = base_x - uy * head_half;
    let left_y = base_y + ux * head_half;
    let right_x = base_x + uy * head_half;
    let right_y = base_y - ux * head_half;
    let head_shape = format!("M {left_x:.2} {left_y:.2} L {:.2} {:.2} L {right_x:.2} {right_y:.2}", tip.x, tip.y);
    let head = match arrowhead {
        ArrowheadStyle::Filled => format!("    <path class=\"classic-arrow-head\" d=\"{head_shape} Z\" fill=\"{color}\"/>\n"),
        ArrowheadStyle::Open => format!("    <path class=\"classic-arrow-head\" d=\"{head_shape}\" fill=\"none\" stroke=\"{color}\" stroke-width=\"{}\" stroke-linecap=\"round\" stroke-linejoin=\"round\"/>\n", width.max(2.5)),
    };
    format!("  <g{filter}>\n{shaft}{head}  </g>\n")
}

/// Draws a single silhouette along the arrow's centre line. A taper and a wide head
/// are part of the same path, so curved arrows keep their shape around bends.
fn skin_arrow(points: &[Point], color: &str, width: f64, skin: ArrowSkin, key: &str, filter: &str) -> Option<String> {
    if points.len() < 2 { return None; }
    let mut distances = vec![0.0; points.len()];
    for i in 1..points.len() {
        distances[i] = distances[i - 1] + ((points[i].x - points[i - 1].x).powi(2) + (points[i].y - points[i - 1].y).powi(2)).sqrt();
    }
    let length = *distances.last()?;
    if length < 5.0 { return None; }
    let sketch_outline = (width * 0.2).clamp(1.0, 2.0);
    let (head_length, head_half) = match skin {
        ArrowSkin::Bold => ((length * 0.26).clamp(18.0, 90.0).min(length * 0.45), (length * 0.12).clamp(10.0, 65.0).min(length * 0.25)),
        ArrowSkin::Sketch => ((width * 3.5).clamp(18.0, 30.0).min(length * 0.45), ((width * 2.3).clamp(10.0, 22.0) - sketch_outline * 0.5).min(length * 0.26)),
        ArrowSkin::Classic => return None,
    };
    let neck_distance = length - head_length;
    let mut body = Vec::new();
    for i in 0..points.len() - 1 {
        if distances[i] < neck_distance { body.push((points[i], distances[i])); }
        if distances[i + 1] >= neck_distance {
            let segment = (distances[i + 1] - distances[i]).max(f64::EPSILON);
            let t = (neck_distance - distances[i]) / segment;
            body.push((Point::new(points[i].x + (points[i + 1].x - points[i].x) * t, points[i].y + (points[i + 1].y - points[i].y) * t), neck_distance));
            break;
        }
    }
    let mut left = Vec::with_capacity(body.len());
    let mut right = Vec::with_capacity(body.len());
    for i in 0..body.len() {
        let prev = if i == 0 { body[i].0 } else { body[i - 1].0 };
        let next = if i + 1 == body.len() { points[points.len() - 1] } else { body[i + 1].0 };
        let dx = next.x - prev.x;
        let dy = next.y - prev.y;
        let direction_length = (dx * dx + dy * dy).sqrt().max(f64::EPSILON);
        let nx = -dy / direction_length;
        let ny = dx / direction_length;
        let progress = (body[i].1 / neck_distance).clamp(0.0, 1.0);
        let half_width = match skin {
            ArrowSkin::Bold => 0.6 + ((width * 1.7).max(head_half * 0.36) - 0.6) * progress,
            ArrowSkin::Sketch => ((width - sketch_outline) * 0.5).max(0.5).min(head_half * 0.65),
            ArrowSkin::Classic => return None,
        };
        let p = body[i].0;
        left.push(Point::new(p.x + nx * half_width, p.y + ny * half_width));
        right.push(Point::new(p.x - nx * half_width, p.y - ny * half_width));
    }
    let neck = body.last()?.0;
    let tip = *points.last()?;
    let dx = tip.x - neck.x;
    let dy = tip.y - neck.y;
    let direction_length = (dx * dx + dy * dy).sqrt().max(f64::EPSILON);
    let nx = -dy / direction_length;
    let ny = dx / direction_length;
    let mut path = format!("M {:.2} {:.2}", left[0].x, left[0].y);
    for p in left.iter().skip(1) { path.push_str(&format!(" L {:.2} {:.2}", p.x, p.y)); }
    path.push_str(&format!(" L {:.2} {:.2} L {:.2} {:.2} L {:.2} {:.2}", neck.x + nx * head_half, neck.y + ny * head_half, tip.x, tip.y, neck.x - nx * head_half, neck.y - ny * head_half));
    for p in right.iter().rev() { path.push_str(&format!(" L {:.2} {:.2}", p.x, p.y)); }
    path.push_str(" Z");
    match skin {
        ArrowSkin::Bold => Some(format!("  <path d=\"{path}\" fill=\"{color}\"{filter}/>\n")),
        ArrowSkin::Sketch => Some(format!("  <path d=\"{path}\" fill=\"url(#arrow-hatch-{key})\" stroke=\"{color}\" stroke-width=\"{sketch_outline}\" stroke-linecap=\"round\" stroke-linejoin=\"round\"{filter}/>\n")),
        ArrowSkin::Classic => None,
    }
}
/// Geometry returned alongside the SVG for editor and automation clients.
#[derive(Debug, Clone, Serialize)]
pub struct LayoutElement {
    pub id: String,
    pub bounds: [f64; 4],
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arrow_path: Option<Vec<[f64; 2]>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RenderResult {
    pub svg: String,
    pub elements: Vec<LayoutElement>,
}

fn element_geometry(annotation: &ResolvedAnnotation) -> ([f64; 4], Option<Vec<[f64; 2]>>) {
    let rect = |r: &crate::model::TargetRect| [r.x, r.y, r.width, r.height];
    let point = |p: &crate::layout::Point| [p.x, p.y];
    let line_bounds = |points: &[crate::layout::Point]| {
        let min_x = points.iter().map(|p| p.x).fold(f64::INFINITY, f64::min);
        let max_x = points.iter().map(|p| p.x).fold(f64::NEG_INFINITY, f64::max);
        let min_y = points.iter().map(|p| p.y).fold(f64::INFINITY, f64::min);
        let max_y = points.iter().map(|p| p.y).fold(f64::NEG_INFINITY, f64::max);
        [min_x, min_y, max_x - min_x, max_y - min_y]
    };
    match annotation {
        ResolvedAnnotation::Rect { rect: r, .. } | ResolvedAnnotation::RoundedRect { rect: r, .. } => (rect(r), None),
        ResolvedAnnotation::Label { box_rect, .. } => (rect(box_rect), None),
        ResolvedAnnotation::Callout { box_rect, arrow_start, arrow_end, .. } =>
            (rect(box_rect), Some(vec![point(arrow_start), point(arrow_end)])),
        ResolvedAnnotation::Spotlight { target, .. } => (rect(target), None),
        ResolvedAnnotation::Circle { cx, cy, rx, ry, .. } => ([cx-rx, cy-ry, 2.0*rx, 2.0*ry], None),
        ResolvedAnnotation::Badge { center, radius, .. } | ResolvedAnnotation::StepArrow { center, radius, .. } => {
            let path = if let ResolvedAnnotation::StepArrow { arrow_start, arrow_end, .. } = annotation {
                Some(vec![point(arrow_start), point(arrow_end)])
            } else { None };
            ([center.x-radius, center.y-radius, 2.0*radius, 2.0*radius], path)
        }
        ResolvedAnnotation::Pin { head_center, head_radius, text_rect, .. } => {
            let mut bounds = [head_center.x-head_radius, head_center.y-head_radius, 2.0*head_radius, 2.0*head_radius];
            if let Some(text) = text_rect {
                let left = bounds[0].min(text.x);
                let top = bounds[1].min(text.y);
                let right = (bounds[0]+bounds[2]).max(text.right());
                let bottom = (bounds[1]+bounds[3]).max(text.bottom());
                bounds = [left, top, right-left, bottom-top];
            }
            (bounds, None)
        }
        ResolvedAnnotation::Bullseye { center, outer_radius, .. } =>
            ([center.x-outer_radius, center.y-outer_radius, 2.0*outer_radius, 2.0*outer_radius], None),
        ResolvedAnnotation::Arrow { start, end, .. } | ResolvedAnnotation::Divider { start, end, .. } =>
            (line_bounds(&[*start, *end]), Some(vec![point(start), point(end)])),
        ResolvedAnnotation::BezierArrow { start, control, end, text_rect, .. } => {
            let points = [*start, *control, *end];
            let mut bounds = line_bounds(&points);
            if let Some(text) = text_rect {
                let left = bounds[0].min(text.x);
                let top = bounds[1].min(text.y);
                let right = (bounds[0]+bounds[2]).max(text.right());
                let bottom = (bounds[1]+bounds[3]).max(text.bottom());
                bounds = [left, top, right-left, bottom-top];
            }
            (bounds, Some(points.iter().map(point).collect()))
        }
    }
}

fn multiline_text(text: &str, x: f64, font_size: f64) -> String {
    let lines: Vec<&str> = text.split('\n').collect();
    if lines.len() == 1 { return escape_xml(text); }
    let line_height = font_size * 1.35;
    lines.iter().enumerate().map(|(index, line)| {
        let dy = if index == 0 { -(lines.len() as f64 - 1.0) * line_height / 2.0 } else { line_height };
        format!("<tspan x=\"{x}\" dy=\"{dy}\">{}</tspan>", escape_xml(line))
    }).collect()
}

/// Escapes XML special characters.
pub fn escape_xml(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(c),
        }
    }
    out
}

/// Helper to render semantic style string key for marker IDs.
fn style_key(style: SemanticStyle) -> &'static str {
    match style {
        SemanticStyle::Primary => "primary",
        SemanticStyle::Secondary => "secondary",
        SemanticStyle::Warning => "warning",
        SemanticStyle::Danger => "danger",
        SemanticStyle::Info => "info",
        SemanticStyle::Step => "step",
        SemanticStyle::Pink => "pink",
    }
}

/// SVG Renderer responsible for generating vector XML markup.
pub struct SvgRenderer {
    pub theme: Theme,
}

impl Default for SvgRenderer {
    fn default() -> Self {
        Self {
            theme: Theme::default(),
        }
    }
}

impl SvgRenderer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_theme(theme: Theme) -> Self {
        Self { theme }
    }

    pub fn render_scene(&self, scene: &ResolvedScene) -> String {
        let mut svg = String::with_capacity(4096);
        let w = scene.canvas.width;
        let h = scene.canvas.height;

        // Root SVG element with transparent canvas
        svg.push_str(&format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="{}" height="{}" viewBox="0 0 {} {}">"#,
            w, h, w, h
        ));
        svg.push('\n');

        // <defs> section
        svg.push_str("  <defs>\n");

        // Conditionally emit drop shadow filter only if at least one annotation uses it
        if scene.has_any_shadow() {
            svg.push_str(r#"    <filter id="markits-shadow" x="-20%" y="-20%" width="140%" height="140%">"#);
            svg.push('\n');
            svg.push_str(r##"      <feDropShadow dx="0" dy="2" stdDeviation="3" flood-color="#000000" flood-opacity="0.25"/>"##);
            svg.push('\n');
            svg.push_str("    </filter>\n");
        }

        // Arrowhead markers for each semantic style
        const ALL_STYLES: [SemanticStyle; 7] = [
            SemanticStyle::Primary,
            SemanticStyle::Secondary,
            SemanticStyle::Warning,
            SemanticStyle::Danger,
            SemanticStyle::Info,
            SemanticStyle::Step,
            SemanticStyle::Pink,
        ];

        for &style in &ALL_STYLES {
            let tokens = self.theme.tokens_for(style);
            let key = style_key(style);
            svg.push_str(&format!(
                r#"    <marker id="arrowhead-{}" viewBox="0 0 10 10" refX="7" refY="5" markerWidth="6" markerHeight="6" orient="auto-start-reverse">
      <path d="M 0 1.5 L 8 5 L 0 8.5 z" fill="{}" />
    </marker>
"#,
                key, tokens.stroke_color
            ));
            svg.push_str(&format!(
                "    <marker id=\"arrowhead-open-{}\" viewBox=\"0 0 10 10\" refX=\"7\" refY=\"5\" markerWidth=\"6\" markerHeight=\"6\" orient=\"auto-start-reverse\"><path d=\"M 0 1.5 L 8 5 L 0 8.5\" fill=\"none\" stroke=\"{}\" stroke-width=\"1.6\"/></marker>\n",
                key, tokens.stroke_color
            ));
            svg.push_str(&format!(
                "    <pattern id=\"arrow-hatch-{}\" width=\"12\" height=\"12\" patternUnits=\"userSpaceOnUse\" patternTransform=\"rotate(35)\"><path d=\"M 0 0 V 12\" stroke=\"{}\" stroke-width=\"2.2\"/></pattern>\n",
                key, tokens.stroke_color
            ));
        }

        // Spotlight masks
        // Bleed past the SVG viewport so antialiasing cannot expose a light strip
        // where the dimming layer meets the editor canvas edge.
        let spotlight_bleed = 2;
        let mask_width = w + spotlight_bleed * 2;
        let mask_height = h + spotlight_bleed * 2;
        for (idx, ann) in scene.annotations.iter().enumerate() {
            if let ResolvedAnnotation::Spotlight { target, .. } = ann {
                svg.push_str(&format!(
                    r#"    <mask id="spotlight-mask-{}" maskUnits="userSpaceOnUse" maskContentUnits="userSpaceOnUse" x="-{}" y="-{}" width="{}" height="{}">
      <rect x="-{}" y="-{}" width="{}" height="{}" fill="white"/>
      <rect x="{}" y="{}" width="{}" height="{}" rx="{}" ry="{}" fill="black"/>
    </mask>
"#,
                    idx, spotlight_bleed, spotlight_bleed, mask_width, mask_height,
                    spotlight_bleed, spotlight_bleed, mask_width, mask_height,
                    target.x, target.y, target.width, target.height, self.theme.corner_radius, self.theme.corner_radius
                ));
            }
        }

        svg.push_str("  </defs>\n");

        // Layer 1: Spotlights (bottom overlay)
        for (idx, ann) in scene.annotations.iter().enumerate() {
            if let ResolvedAnnotation::Spotlight { target, style } = ann {
                let tokens = self.theme.tokens_for(*style);
                svg.push_str(&format!(
                    r#"  <!-- Spotlight -->
  <rect x="-{}" y="-{}" width="{}" height="{}" fill="{}" opacity="{}" mask="url(#spotlight-mask-{})"/>
  <rect x="{}" y="{}" width="{}" height="{}" rx="{}" ry="{}" fill="none" stroke="{}" stroke-width="{}" stroke-dasharray="4 3"/>
"#,
                    spotlight_bleed, spotlight_bleed, mask_width, mask_height,
                    self.theme.spotlight_backdrop,
                    self.theme.spotlight_opacity,
                    idx,
                    target.x,
                    target.y,
                    target.width,
                    target.height,
                    self.theme.corner_radius,
                    self.theme.corner_radius,
                    tokens.stroke_color,
                    tokens.stroke_width
                ));
            }
        }

        // Layer 2: Dividers, Rectangles, Circles, Arrows, Bullseyes
        for ann in &scene.annotations {
            match ann {
                ResolvedAnnotation::Divider { start, end, style, stroke_width } => {
                    let tokens = self.theme.tokens_for(*style);
                    svg.push_str(&format!(
                        r#"  <line x1="{}" y1="{}" x2="{}" y2="{}" stroke="{}" stroke-width="{}" stroke-linecap="round"/>
"#,
                        start.x, start.y, end.x, end.y, tokens.stroke_color, stroke_width
                    ));
                }
                ResolvedAnnotation::Rect { rect, style, stroke_width, .. } => {
                    let tokens = self.theme.tokens_for(*style);
                    let sw = stroke_width.unwrap_or(tokens.stroke_width);
                    svg.push_str(&format!(
                        r#"  <rect x="{}" y="{}" width="{}" height="{}" fill="{}" stroke="{}" stroke-width="{}"/>
"#,
                        rect.x, rect.y, rect.width, rect.height, tokens.light_fill, tokens.stroke_color, sw
                    ));
                }
                ResolvedAnnotation::RoundedRect { rect, rx, ry, style, stroke_width, .. } => {
                    let tokens = self.theme.tokens_for(*style);
                    let sw = stroke_width.unwrap_or(tokens.stroke_width);
                    svg.push_str(&format!(
                        r#"  <rect x="{}" y="{}" width="{}" height="{}" rx="{}" ry="{}" fill="{}" stroke="{}" stroke-width="{}"/>
"#,
                        rect.x, rect.y, rect.width, rect.height, rx, ry, tokens.light_fill, tokens.stroke_color, sw
                    ));
                }
                ResolvedAnnotation::Circle { cx, cy, rx, ry, style, stroke_width, .. } => {
                    let tokens = self.theme.tokens_for(*style);
                    let sw = stroke_width.unwrap_or(tokens.stroke_width);
                    svg.push_str(&format!(
                        r#"  <ellipse cx="{}" cy="{}" rx="{}" ry="{}" fill="{}" stroke="{}" stroke-width="{}"/>
"#,
                        cx, cy, rx, ry, tokens.light_fill, tokens.stroke_color, sw
                    ));
                }
                ResolvedAnnotation::Bullseye { center, outer_radius, inner_radius, dot_radius, style, shadow } => {
                    let tokens = self.theme.tokens_for(*style);
                    let filter_attr = if *shadow { r#" filter="url(#markits-shadow)""# } else { "" };
                    svg.push_str(&format!(
                        r#"  <g{}>
    <circle cx="{}" cy="{}" r="{}" fill="none" stroke="{}" stroke-width="3.5"/>
    <circle cx="{}" cy="{}" r="{}" fill="none" stroke="{}" stroke-width="1.8"/>
    <circle cx="{}" cy="{}" r="{}" fill="{}"/>
  </g>
"#,
                        filter_attr,
                        center.x, center.y, outer_radius, tokens.stroke_color,
                        center.x, center.y, inner_radius, tokens.stroke_color,
                        center.x, center.y, dot_radius, tokens.stroke_color
                    ));
                }
                ResolvedAnnotation::Arrow { start, end, text, style, shadow, stroke_width, line_style, arrowhead, arrow_skin, boxed, outline, text_placement } => {
                    let tokens = self.theme.tokens_for(*style);
                    let sw = stroke_width.unwrap_or(4.0);
                    let key = style_key(*style);
                    let filter_attr = if *shadow { r#" filter="url(#markits-shadow)""# } else { "" };
                    svg.push_str(&skin_arrow(&[*start, *end], &tokens.stroke_color, sw, *arrow_skin, key, filter_attr)
                        .unwrap_or_else(|| classic_arrow(&[*start, *end], None, &tokens.stroke_color, sw, *line_style, *arrowhead, filter_attr)));

                    if let Some(txt) = text {
                        if !txt.trim().is_empty() {
                            let escaped = escape_xml(txt);
                            let dx = end.x - start.x;
                            let dy = end.y - start.y;
                            let len = (dx * dx + dy * dy).sqrt().max(0.001);
                            let font_size = self.theme.font_size;
                            let text_dim = crate::layout::estimate_text_dimensions(txt, font_size);
                            let pill_w = text_dim.width + 16.0;
                            let pill_h = text_dim.height + 8.0;
                            let rx = self.theme.corner_radius;
                            let ry = self.theme.corner_radius;
                            let (base_x, base_y) = ((start.x + end.x) / 2.0, (start.y + end.y) / 2.0);
                            let clearance = sw / 2.0 + 6.0;
                            let (cx, cy) = match text_placement {
                                crate::model::ArrowTextPlacement::End => {
                                    // Place the label beyond the tail, accounting for its full bounds.
                                    let ux = -dx / len;
                                    let uy = -dy / len;
                                    let extent = pill_w / 2.0 * ux.abs() + pill_h / 2.0 * uy.abs();
                                    (start.x + ux * (extent + clearance), start.y + uy * (extent + clearance))
                                }
                                crate::model::ArrowTextPlacement::Middle => {
                                    // Put labels beside the shaft: above/below horizontal arrows,
                                    // and to the right of vertical arrows.
                                    if dx.abs() >= dy.abs() {
                                        let side = if dx >= 0.0 { -1.0 } else { 1.0 };
                                        (base_x, base_y + side * (pill_h / 2.0 + clearance))
                                    } else {
                                        (base_x + pill_w / 2.0 + clearance, base_y)
                                    }
                                }
                            };
                            let box_x = cx - pill_w / 2.0;
                            let box_y = cy - pill_h / 2.0;

                            if *boxed {
                                let stroke_color = if *outline { "#ffffff" } else { tokens.stroke_color };
                                svg.push_str(&format!(
                                    r#"  <g{}>
    <rect x="{}" y="{}" width="{}" height="{}" rx="{}" ry="{}" fill="{}" stroke="{}" stroke-width="1.5"/>
    <text x="{}" y="{}" fill="{}" font-family="{}" font-size="{}" font-weight="600" text-anchor="middle" dominant-baseline="central">{}</text>
  </g>
"#,
                                    filter_attr,
                                    box_x, box_y, pill_w, pill_h, rx, ry,
                                    tokens.fill_color, stroke_color,
                                    cx, cy,
                                    tokens.text_color, self.theme.font_family, font_size,
                                    escaped
                                ));
                            } else {
                                let outline_attr = if *outline {
                                    r##" stroke="#ffffff" stroke-width="1.8" stroke-linejoin="round" paint-order="stroke fill""##
                                } else {
                                    ""
                                };
                                svg.push_str(&format!(
                                    r#"  <g{}>
    <text x="{}" y="{}" fill="{}" font-family="{}" font-size="{}" font-weight="600" text-anchor="middle" dominant-baseline="central"{}>{}</text>
  </g>
"#,
                                    filter_attr,
                                    cx, cy,
                                    tokens.stroke_color, self.theme.font_family, font_size,
                                    outline_attr,
                                    escaped
                                ));
                            }
                        }
                    }
                }
                ResolvedAnnotation::BezierArrow { start, control, end, style, shadow, stroke_width, line_style, arrowhead, arrow_skin, .. } => {
                    let tokens = self.theme.tokens_for(*style);
                    let sw = stroke_width.unwrap_or(4.0);
                    let key = style_key(*style);
                    let filter_attr = if *shadow { r#" filter="url(#markits-shadow)""# } else { "" };
                    let points: Vec<Point> = (0..=40).map(|step| {
                        let t = step as f64 / 40.0;
                        let inv = 1.0 - t;
                        Point::new(inv * inv * start.x + 2.0 * inv * t * control.x + t * t * end.x,
                                   inv * inv * start.y + 2.0 * inv * t * control.y + t * t * end.y)
                    }).collect();
                    svg.push_str(&skin_arrow(&points, &tokens.stroke_color, sw, *arrow_skin, key, filter_attr)
                        .unwrap_or_else(|| classic_arrow(&points, Some(*control), &tokens.stroke_color, sw, *line_style, *arrowhead, filter_attr)));
                }
                _ => {}
            }
        }

        // Layer 3: Labels, Callouts, Badges, Pins (Foremost layer)
        for ann in &scene.annotations {
            match ann {
                ResolvedAnnotation::Label { box_rect, text, style, shadow, outline } => {
                    let tokens = self.theme.tokens_for(*style);
                    let escaped = multiline_text(text, box_rect.center_x(), self.theme.font_size);
                    let filter_attr = if *shadow { r#" filter="url(#markits-shadow)""# } else { "" };
                    let stroke_color = if *outline { "#ffffff" } else { tokens.stroke_color };

                    svg.push_str(&format!(
                        r#"  <g{}>
    <rect x="{}" y="{}" width="{}" height="{}" rx="{}" ry="{}" fill="{}" stroke="{}" stroke-width="1.5"/>
    <text x="{}" y="{}" fill="{}" font-family="{}" font-size="{}" font-weight="600" text-anchor="middle" dominant-baseline="central">{}</text>
  </g>
"#,
                        filter_attr,
                        box_rect.x, box_rect.y, box_rect.width, box_rect.height,
                        self.theme.corner_radius, self.theme.corner_radius,
                        tokens.fill_color, stroke_color,
                        box_rect.center_x(), box_rect.center_y(),
                        tokens.text_color, self.theme.font_family, self.theme.font_size,
                        escaped
                    ));
                }
                ResolvedAnnotation::Callout { box_rect, text, arrow_start, arrow_end, style, shadow, outline } => {
                    let tokens = self.theme.tokens_for(*style);
                    let key = style_key(*style);
                    let escaped = multiline_text(text, box_rect.center_x(), self.theme.font_size);
                    let filter_attr = if *shadow { r#" filter="url(#markits-shadow)""# } else { "" };
                    let stroke_color = if *outline { "#ffffff" } else { tokens.stroke_color };

                    // Pointer arrow line
                    svg.push_str(&format!(
                        r#"  <line x1="{}" y1="{}" x2="{}" y2="{}" stroke="{}" stroke-width="{}" stroke-linecap="round" marker-end="url(#arrowhead-{})"/>
"#,
                        arrow_start.x, arrow_start.y, arrow_end.x, arrow_end.y, tokens.stroke_color, tokens.stroke_width, key
                    ));

                    // Label Box and Text
                    svg.push_str(&format!(
                        r#"  <g{}>
    <rect x="{}" y="{}" width="{}" height="{}" rx="{}" ry="{}" fill="{}" stroke="{}" stroke-width="1.5"/>
    <text x="{}" y="{}" fill="{}" font-family="{}" font-size="{}" font-weight="600" text-anchor="middle" dominant-baseline="central">{}</text>
  </g>
"#,
                        filter_attr,
                        box_rect.x, box_rect.y, box_rect.width, box_rect.height,
                        self.theme.corner_radius, self.theme.corner_radius,
                        tokens.fill_color, stroke_color,
                        box_rect.center_x(), box_rect.center_y(),
                        tokens.text_color, self.theme.font_family, self.theme.font_size,
                        escaped
                    ));
                }
                ResolvedAnnotation::Badge { center, radius, label, style, shadow } => {
                    let tokens = self.theme.tokens_for(*style);
                    let escaped = escape_xml(label);
                    let filter_attr = if *shadow { r#" filter="url(#markits-shadow)""# } else { "" };

                    svg.push_str(&format!(
                        r##"  <g{}>
    <circle cx="{}" cy="{}" r="{}" fill="{}" stroke="#ffffff" stroke-width="2"/>
    <text x="{}" y="{}" fill="{}" font-family="{}" font-size="{}" font-weight="bold" text-anchor="middle" dominant-baseline="central">{}</text>
  </g>
"##,
                        filter_attr,
                        center.x, center.y, radius,
                        tokens.fill_color,
                        center.x, center.y,
                        tokens.text_color, self.theme.font_family, self.theme.font_size * 0.9,
                        escaped
                    ));
                }
                ResolvedAnnotation::StepArrow {
                    center,
                    radius,
                    label,
                    arrow_start,
                    arrow_end,
                    style,
                    shadow,
                    stroke_width,
                } => {
                    let tokens = self.theme.tokens_for(*style);
                    let sw = stroke_width.unwrap_or(tokens.stroke_width);
                    let key = style_key(*style);
                    let escaped = escape_xml(label);
                    let filter_attr = if *shadow { r#" filter="url(#markits-shadow)""# } else { "" };

                    // 1. Pointer arrow line connecting badge to target
                    svg.push_str(&format!(
                        r#"  <line x1="{}" y1="{}" x2="{}" y2="{}" stroke="{}" stroke-width="{}" stroke-linecap="round" marker-end="url(#arrowhead-{})"/>
"#,
                        arrow_start.x, arrow_start.y, arrow_end.x, arrow_end.y, tokens.stroke_color, sw, key
                    ));

                    // 2. Circular step badge
                    svg.push_str(&format!(
                        r##"  <g{}>
    <circle cx="{}" cy="{}" r="{}" fill="{}" stroke="#ffffff" stroke-width="2"/>
    <text x="{}" y="{}" fill="{}" font-family="{}" font-size="{}" font-weight="bold" text-anchor="middle" dominant-baseline="central">{}</text>
  </g>
"##,
                        filter_attr,
                        center.x, center.y, radius,
                        tokens.fill_color,
                        center.x, center.y,
                        tokens.text_color, self.theme.font_family, self.theme.font_size * 0.9,
                        escaped
                    ));
                }
                ResolvedAnnotation::Pin { head_center, head_radius, tip, icon, text, text_rect, style, shadow, outline } => {
                    let tokens = self.theme.tokens_for(*style);
                    let filter_attr = if *shadow { r#" filter="url(#markits-shadow)""# } else { "" };

                    // Compute triangular pointer base perpendicular to the (head_center -> tip) direction
                    let dx = tip.x - head_center.x;
                    let dy = tip.y - head_center.y;
                    let len = (dx * dx + dy * dy).sqrt().max(0.001);
                    let ux = dx / len;
                    let uy = dy / len;
                    let perp_x = -uy;
                    let perp_y = ux;
                    let base_half = head_radius * 0.65;

                    let p1_x = head_center.x + perp_x * base_half;
                    let p1_y = head_center.y + perp_y * base_half;
                    let p2_x = head_center.x - perp_x * base_half;
                    let p2_y = head_center.y - perp_y * base_half;

                    svg.push_str(&format!(r#"  <g{}>"#, filter_attr));
                    svg.push('\n');

                    // 1. Pointer triangle
                    svg.push_str(&format!(
                        r#"    <polygon points="{},{} {},{} {},{}" fill="{}" stroke="{}" stroke-width="1.5"/>
"#,
                        p1_x, p1_y, p2_x, p2_y, tip.x, tip.y, tokens.fill_color, tokens.stroke_color
                    ));

                    // 2. Circular pin head
                    svg.push_str(&format!(
                        r##"    <circle cx="{}" cy="{}" r="{}" fill="{}" stroke="#ffffff" stroke-width="2.5"/>
"##,
                        head_center.x, head_center.y, head_radius, tokens.fill_color
                    ));

                    // 3. Icon / Symbol inside the pin head
                    if let Some(ic) = icon {
                        let escaped_icon = escape_xml(ic);
                        svg.push_str(&format!(
                            r#"    <text x="{}" y="{}" fill="{}" font-family="{}" font-size="{}" font-weight="bold" text-anchor="middle" dominant-baseline="central">{}</text>
"#,
                            head_center.x, head_center.y, tokens.text_color, self.theme.font_family, head_radius * 0.95, escaped_icon
                        ));
                    }

                    // 4. Linked text pill
                    if let (Some(txt), Some(tr)) = (text, text_rect) {
                        let escaped_text = escape_xml(txt);
                        let stroke_color = if *outline { "#ffffff" } else { tokens.stroke_color };
                        svg.push_str(&format!(
                            r##"    <rect x="{}" y="{}" width="{}" height="{}" rx="{}" ry="{}" fill="#1e293b" stroke="{}" stroke-width="1.5"/>
    <text x="{}" y="{}" fill="#ffffff" font-family="{}" font-size="{}" font-weight="600" text-anchor="middle" dominant-baseline="central">{}</text>
"##,
                            tr.x, tr.y, tr.width, tr.height, self.theme.corner_radius, self.theme.corner_radius,
                            stroke_color,
                            tr.center_x(), tr.center_y(), self.theme.font_family, self.theme.font_size,
                            escaped_text
                        ));
                    }

                    svg.push_str("  </g>\n");
                }
                ResolvedAnnotation::BezierArrow { text, text_rect, style, shadow, outline, boxed, .. } => {
                    if let (Some(txt), Some(tr)) = (text, text_rect) {
                        let tokens = self.theme.tokens_for(*style);
                        let escaped = escape_xml(txt);
                        let filter_attr = if *shadow { r#" filter="url(#markits-shadow)""# } else { "" };

                        if *boxed {
                            let stroke_color = if *outline { "#ffffff" } else { tokens.stroke_color };
                            svg.push_str(&format!(
                                r#"  <g{}>
    <rect x="{}" y="{}" width="{}" height="{}" rx="{}" ry="{}" fill="{}" stroke="{}" stroke-width="1.5"/>
    <text x="{}" y="{}" fill="{}" font-family="{}" font-size="{}" font-weight="600" text-anchor="middle" dominant-baseline="central">{}</text>
  </g>
"#,
                                filter_attr,
                                tr.x, tr.y, tr.width, tr.height,
                                self.theme.corner_radius, self.theme.corner_radius,
                                tokens.fill_color, stroke_color,
                                tr.center_x(), tr.center_y(),
                                tokens.text_color, self.theme.font_family, self.theme.font_size,
                                escaped
                            ));
                        } else {
                            let outline_attr = if *outline {
                                r##" stroke="#ffffff" stroke-width="1.8" stroke-linejoin="round" paint-order="stroke fill""##
                            } else {
                                ""
                            };
                            svg.push_str(&format!(
                                r#"  <g{}>
    <text x="{}" y="{}" fill="{}" font-family="{}" font-size="{}" font-weight="600" text-anchor="middle" dominant-baseline="central"{}>{}</text>
  </g>
"#,
                                filter_attr,
                                tr.center_x(), tr.center_y(),
                                tokens.stroke_color, self.theme.font_family, self.theme.font_size,
                                outline_attr,
                                escaped
                            ));
                        }
                    }
                }
                _ => {}
            }
        }

        svg.push_str("</svg>\n");
        svg
    }
}

impl Scene {
    /// Renders the scene to an SVG string using the default layout engine and renderer.
    pub fn render_svg(&self) -> Result<String> {
        self.validate()?;
        let visible = self.visible_scene();
        let layout_engine = LayoutEngine::new();
        let resolved = layout_engine.layout_scene(&visible);
        let renderer = SvgRenderer::new();
        Ok(renderer.render_scene(&resolved))
    }
}

/// Convenience function to render SVG directly from a JSON string.
pub fn render_from_json(json_str: &str) -> Result<String> {
    let scene = Scene::from_json(json_str)?;
    scene.render_svg()
}

/// Renders the final layout with target, candidate, collision and selected boxes overlaid.
pub fn render_debug_from_json(json_str: &str) -> Result<String> {
    let scene = Scene::from_json(json_str)?;
    let scene = scene.visible_scene();
    let engine = LayoutEngine::new();
    let resolved = engine.layout_scene(&scene);
    let mut svg = SvgRenderer::new().render_scene(&resolved);
    let mut overlay = String::from("  <g id=\"markits-layout-debug\" font-family=\"monospace\" pointer-events=\"none\">\n");
    for (index, ann) in resolved.annotations.iter().enumerate() {
        if let ResolvedAnnotation::Label { box_rect, .. } | ResolvedAnnotation::Callout { box_rect, .. } = ann {
            overlay.push_str(&format!("    <rect class=\"collision-box\" data-annotation=\"{index}\" x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"none\" stroke=\"#a855f7\" stroke-dasharray=\"3 3\"/>\n", box_rect.x, box_rect.y, box_rect.width, box_rect.height));
        }
    }
    for callout in engine.debug_callouts(&scene, &resolved) {
        let t = callout.target;
        overlay.push_str(&format!("    <rect class=\"target-box\" data-annotation=\"{}\" x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"#3b82f6\" fill-opacity=\"0.08\" stroke=\"#2563eb\" stroke-dasharray=\"5 3\"/>\n", callout.annotation_index, t.x, t.y, t.width, t.height));
        for (index, candidate) in callout.candidates.iter().enumerate() {
            let r = candidate.rect;
            let color = if candidate.selected { "#16a34a" } else { "#ef4444" };
            let opacity = if candidate.selected { 0.18 } else { 0.025 };
            overlay.push_str(&format!("    <rect class=\"candidate-box{}\" data-annotation=\"{}\" data-candidate=\"{}\" data-anchor=\"{:?}\" data-score=\"{:.2}\" x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"{}\" fill-opacity=\"{}\" stroke=\"{}\" stroke-opacity=\"0.45\" stroke-dasharray=\"2 3\"><title>callout {}: {:?}, score {:.2}{}</title></rect>\n", if candidate.selected { " selected" } else { " rejected" }, callout.annotation_index, index, candidate.anchor, candidate.score, r.x, r.y, r.width, r.height, color, opacity, color, callout.annotation_index, candidate.anchor, candidate.score, if candidate.selected { " (selected)" } else { "" }));
        }
    }
    overlay.push_str("  </g>\n");
    if let Some(index) = svg.rfind("</svg>") { svg.insert_str(index, &overlay); }
    Ok(svg)
}

/// Renders SVG and exposes the positioned annotation geometry.
pub fn render_with_layout_from_json(json_str: &str) -> Result<RenderResult> {
    let prepared = crate::semantic::prepare(json_str)?;
    let scene: Scene = serde_json::from_value(prepared.scene)?;
    scene.validate()?;
    let ids = prepared.ids.into_iter().enumerate()
        .filter(|(index, _)| !scene.hidden_annotations.contains(index))
        .map(|(_, id)| id);
    let resolved = LayoutEngine::new().layout_scene(&scene.visible_scene());
    let svg = SvgRenderer::new().render_scene(&resolved);
    let elements = resolved.annotations.iter().zip(ids).map(|(annotation, id)| {
        let (bounds, arrow_path) = element_geometry(annotation);
        LayoutElement { id, bounds, arrow_path }
    }).collect();
    Ok(RenderResult { svg, elements })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arrow_line_and_head_styles_render() {
        let json = r#"{"canvas":{"width":100,"height":80},"annotations":[
          {"type":"arrow","start":[5,5],"end":[50,30],"line_style":"dashed","arrowhead":"open"},
          {"type":"bezier-arrow","start":[5,40],"control":[30,10],"end":[60,40],"line_style":"dotted"}
        ]}"#;
        let svg = render_from_json(json).unwrap();
        assert!(svg.contains("stroke-dasharray=\"9 6\""));
        assert!(svg.contains("stroke-dasharray=\"2 5\""));
        assert!(svg.contains("class=\"classic-arrow-head\""));
        assert!(svg.lines().any(|line| line.contains("class=\"classic-arrow-head\"") && line.contains("fill=\"none\"")));
    }

    #[test]
    fn arrow_skins_render_for_straight_and_curved_arrows() {
        for arrow_type in ["arrow", "bezier-arrow"] {
            let geometry = if arrow_type == "arrow" { "\"start\":[5,5],\"end\":[50,30]" } else { "\"start\":[5,5],\"control\":[20,35],\"end\":[50,30]" };
            for (skin, expected) in [
                ("classic", "class=\"classic-arrow-head\""),
                ("sketch", "fill=\"url(#arrow-hatch-primary)\""),
                ("bold", "fill=\"#"),
            ] {
                let json = format!("{{\"canvas\":{{\"width\":100,\"height\":80}},\"annotations\":[{{\"type\":\"{arrow_type}\",{geometry},\"arrow_skin\":\"{skin}\"}}]}}");
                let svg = render_from_json(&json).unwrap();
                assert!(svg.contains(expected), "{arrow_type} {skin}");
                if skin == "sketch" {
                    assert!(svg.contains("<pattern id=\"arrow-hatch-primary\""));
                }
                if skin != "classic" {
                    let arrow_layer = svg.split("<!-- Layer 3").next().unwrap_or(&svg);
                    assert!(arrow_layer.contains("<path d=\"M "), "{arrow_type} {skin}");
                    assert!(!arrow_layer.contains("marker-end=\"url(#arrowhead-bold"));
                }
            }
        }
    }

    #[test]
    fn thick_classic_arrow_shaft_stops_inside_filled_head() {
        let svg = render_from_json(r#"{"canvas":{"width":790,"height":602},"annotations":[{"type":"arrow","start":[165,286],"end":[370,254],"stroke_width":11}]}"#).unwrap();
        let shaft = svg.lines().find(|line| line.contains("classic-arrow-shaft")).unwrap();
        let shaft_end_x: f64 = shaft.split("x2=\"").nth(1).unwrap().split('"').next().unwrap().parse().unwrap();
        assert!(shaft_end_x < 350.0);
        assert!(svg.contains("L 370.00 254.00"));
    }
    use crate::model::Canvas;

    #[test]
    fn test_escape_xml() {
        assert_eq!(escape_xml("Save & Close <1>"), "Save &amp; Close &lt;1&gt;");
    }

    #[test]
    fn test_svg_root_and_viewbox() {
        let scene = Scene {
            canvas: Canvas { width: 1920, height: 1080 },
            shadow: true,
            annotations: vec![],
            hidden_annotations: vec![],
            uimap: None,
        };
        let svg = scene.render_svg().unwrap();
        assert!(svg.starts_with(r#"<svg xmlns="http://www.w3.org/2000/svg" width="1920" height="1080" viewBox="0 0 1920 1080">"#));
        assert!(svg.ends_with("</svg>\n"));
    }

    #[test]
    fn test_render_callout() {
        let json = r#"{
            "canvas": {"width": 1920, "height": 1080},
            "annotations": [
                {
                    "type": "callout",
                    "target": [820, 640, 100, 32],
                    "text": "設定を保存します",
                    "style": "primary"
                }
            ]
        }"#;

        let svg = render_from_json(json).unwrap();
        assert!(svg.contains("設定を保存します"));
        assert!(svg.contains("marker-end=\"url(#arrowhead-primary)\""));
        assert!(svg.contains("<filter id=\"markits-shadow\""));
        assert!(svg.contains("<rect"));
    }

    #[test]
    fn test_render_spotlight_and_badge() {
        let json = r#"{
            "canvas": {"width": 800, "height": 600},
            "annotations": [
                {
                    "type": "spotlight",
                    "target": [200, 150, 100, 50],
                    "style": "primary"
                },
                {
                    "type": "badge",
                    "target": [200, 150, 100, 50],
                    "step": 1,
                    "style": "step"
                }
            ]
        }"#;

        let svg = render_from_json(json).unwrap();
        assert!(svg.contains("mask=\"url(#spotlight-mask-0)\""));
        assert!(svg.contains("<circle"));
        assert!(svg.contains(">1</text>"));
    }

    #[test]
    fn spotlight_mask_and_overlay_bleed_past_all_canvas_edges() {
        let svg = render_from_json(r#"{"canvas":{"width":100,"height":80},"annotations":[{"type":"spotlight","target":[30,20,20,20]}]}"#).unwrap();
        assert!(svg.contains("maskUnits=\"userSpaceOnUse\" maskContentUnits=\"userSpaceOnUse\" x=\"-2\" y=\"-2\" width=\"104\" height=\"84\""));
        assert!(svg.contains("<rect x=\"-2\" y=\"-2\" width=\"104\" height=\"84\" fill=\"white\"/>"));
        assert!(svg.contains("<rect x=\"-2\" y=\"-2\" width=\"104\" height=\"84\" fill=\"#000000\" opacity="));
    }

    #[test]
    fn test_render_shapes() {
        let json = r#"{
            "canvas": {"width": 500, "height": 500},
            "annotations": [
                {"type": "rect", "target": [10, 10, 50, 50]},
                {"type": "rounded-rect", "target": [70, 10, 50, 50], "rx": 6},
                {"type": "circle", "target": [130, 10, 50, 50]},
                {"type": "arrow", "target": [190, 10, 50, 50]}
            ]
        }"#;

        let svg = render_from_json(json).unwrap();
        assert!(svg.contains("<rect"));
        assert!(svg.contains("<ellipse"));
        assert!(svg.contains("<line"));
    }

    #[test]
    fn test_render_skitch_components() {
        let json = r#"{
            "canvas": {"width": 1000, "height": 800},
            "shadow": false,
            "annotations": [
                {
                    "type": "pin",
                    "target": [200, 200, 50, 50],
                    "icon": "♡",
                    "text": "Like Button",
                    "style": "pink"
                },
                {
                    "type": "bullseye",
                    "target": [400, 400, 60, 60],
                    "style": "pink"
                },
                {
                    "type": "divider",
                    "target": [0, 300, 1000, 4],
                    "style": "pink"
                }
            ]
        }"#;

        let svg = render_from_json(json).unwrap();
        // Shadow filter definition should be omitted when all shadow is false
        assert!(!svg.contains(r#"<filter id="markits-shadow""#));
        assert!(svg.contains("<polygon points="));
        assert!(svg.contains(">♡</text>"));
        assert!(svg.contains("Like Button"));
        assert!(svg.contains("#ea1a65")); // Pink
    }

    #[test]
    fn test_render_step_arrow() {
        let json = r#"{
            "canvas": {"width": 800, "height": 600},
            "annotations": [
                {
                    "type": "step-arrow",
                    "target": [200, 150, 100, 50],
                    "step": 1,
                    "style": "step",
                    "position": "bottom"
                }
            ]
        }"#;

        let svg = render_from_json(json).unwrap();
        assert!(svg.contains("marker-end=\"url(#arrowhead-step)\""));
        assert!(svg.contains("<circle"));
        assert!(svg.contains(">1</text>"));
        assert!(svg.contains("#7c3aed")); // Step color
    }

    #[test]
    fn test_render_bezier_arrow() {
        let json = r#"{
            "canvas": {"width": 1000, "height": 600},
            "annotations": [
                {
                    "type": "bezier-arrow",
                    "start": [100, 400],
                    "control": [300, 150],
                    "end": [500, 400],
                    "text": "データ同期",
                    "style": "primary",
                    "shadow": true,
                    "outline": true
                }
            ]
        }"#;

        let svg = render_from_json(json).unwrap();
        assert!(svg.contains("<path d=\"M 100 400 Q "));
        assert!(svg.contains("class=\"classic-arrow-head\""));
        assert!(svg.contains("fill=\"none\""));
        assert!(svg.contains("データ同期"));
        assert!(svg.contains("<filter id=\"markits-shadow\""));
        assert!(svg.contains("<rect")); // Boxed by default
        assert!(svg.contains("#2563eb")); // primary color
    }

    #[test]
    fn test_render_bezier_arrow_unboxed() {
        let json = r#"{
            "canvas": {"width": 800, "height": 600},
            "annotations": [
                {
                    "type": "bezier-arrow",
                    "start": [100, 300],
                    "control": [250, 100],
                    "end": [400, 300],
                    "text": "枠なしテキスト",
                    "style": "pink",
                    "box": false,
                    "outline": true
                }
            ]
        }"#;

        let svg = render_from_json(json).unwrap();
        assert!(svg.contains("<path d=\"M 100 300 Q "));
        assert!(svg.contains("枠なしテキスト"));
        // When box is false, NO rect should be rendered for text
        assert!(!svg.contains("<rect"));
        // Text should have paint-order stroke fill
        assert!(svg.contains("paint-order=\"stroke fill\""));
        assert!(svg.contains("#ea1a65")); // Pink
    }
}
