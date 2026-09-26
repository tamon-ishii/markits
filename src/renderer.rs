use crate::error::Result;
use crate::layout::{LayoutEngine, ResolvedAnnotation, ResolvedScene};
use crate::model::{Scene, SemanticStyle};
use crate::theme::Theme;

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
        }

        // Spotlight masks
        for (idx, ann) in scene.annotations.iter().enumerate() {
            if let ResolvedAnnotation::Spotlight { target, .. } = ann {
                svg.push_str(&format!(
                    r#"    <mask id="spotlight-mask-{}">
      <rect width="100%" height="100%" fill="white"/>
      <rect x="{}" y="{}" width="{}" height="{}" rx="{}" ry="{}" fill="black"/>
    </mask>
"#,
                    idx, target.x, target.y, target.width, target.height, self.theme.corner_radius, self.theme.corner_radius
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
  <rect width="100%" height="100%" fill="{}" opacity="{}" mask="url(#spotlight-mask-{})"/>
  <rect x="{}" y="{}" width="{}" height="{}" rx="{}" ry="{}" fill="none" stroke="{}" stroke-width="{}" stroke-dasharray="4 3"/>
"#,
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
                ResolvedAnnotation::Rect { rect, style, .. } => {
                    let tokens = self.theme.tokens_for(*style);
                    svg.push_str(&format!(
                        r#"  <rect x="{}" y="{}" width="{}" height="{}" fill="{}" stroke="{}" stroke-width="{}"/>
"#,
                        rect.x, rect.y, rect.width, rect.height, tokens.light_fill, tokens.stroke_color, tokens.stroke_width
                    ));
                }
                ResolvedAnnotation::RoundedRect { rect, rx, ry, style, .. } => {
                    let tokens = self.theme.tokens_for(*style);
                    svg.push_str(&format!(
                        r#"  <rect x="{}" y="{}" width="{}" height="{}" rx="{}" ry="{}" fill="{}" stroke="{}" stroke-width="{}"/>
"#,
                        rect.x, rect.y, rect.width, rect.height, rx, ry, tokens.light_fill, tokens.stroke_color, tokens.stroke_width
                    ));
                }
                ResolvedAnnotation::Circle { cx, cy, rx, ry, style, .. } => {
                    let tokens = self.theme.tokens_for(*style);
                    svg.push_str(&format!(
                        r#"  <ellipse cx="{}" cy="{}" rx="{}" ry="{}" fill="{}" stroke="{}" stroke-width="{}"/>
"#,
                        cx, cy, rx, ry, tokens.light_fill, tokens.stroke_color, tokens.stroke_width
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
                ResolvedAnnotation::Arrow { start, end, style, shadow, .. } => {
                    let tokens = self.theme.tokens_for(*style);
                    let key = style_key(*style);
                    let filter_attr = if *shadow { r#" filter="url(#markits-shadow)""# } else { "" };
                    svg.push_str(&format!(
                        r#"  <line x1="{}" y1="{}" x2="{}" y2="{}" stroke="{}" stroke-width="{}" stroke-linecap="round" marker-end="url(#arrowhead-{})"{}/>
"#,
                        start.x, start.y, end.x, end.y, tokens.stroke_color, tokens.stroke_width, key, filter_attr
                    ));
                }
                _ => {}
            }
        }

        // Layer 3: Labels, Callouts, Badges, Pins (Foremost layer)
        for ann in &scene.annotations {
            match ann {
                ResolvedAnnotation::Label { box_rect, text, style, shadow, outline } => {
                    let tokens = self.theme.tokens_for(*style);
                    let escaped = escape_xml(text);
                    let filter_attr = if *shadow { r#" filter="url(#markits-shadow)""# } else { "" };
                    let outline_attr = if *outline {
                        r##" stroke="#ffffff" stroke-width="3.5" stroke-linejoin="round" paint-order="stroke fill""##
                    } else {
                        ""
                    };

                    svg.push_str(&format!(
                        r#"  <g{}>
    <rect x="{}" y="{}" width="{}" height="{}" rx="{}" ry="{}" fill="{}" stroke="{}" stroke-width="1.5"/>
    <text x="{}" y="{}" fill="{}" font-family="{}" font-size="{}" font-weight="600" text-anchor="middle" dominant-baseline="central"{}>{}</text>
  </g>
"#,
                        filter_attr,
                        box_rect.x, box_rect.y, box_rect.width, box_rect.height,
                        self.theme.corner_radius, self.theme.corner_radius,
                        tokens.fill_color, tokens.stroke_color,
                        box_rect.center_x(), box_rect.center_y(),
                        tokens.text_color, self.theme.font_family, self.theme.font_size,
                        outline_attr,
                        escaped
                    ));
                }
                ResolvedAnnotation::Callout { box_rect, text, arrow_start, arrow_end, style, shadow, outline } => {
                    let tokens = self.theme.tokens_for(*style);
                    let key = style_key(*style);
                    let escaped = escape_xml(text);
                    let filter_attr = if *shadow { r#" filter="url(#markits-shadow)""# } else { "" };
                    let outline_attr = if *outline {
                        r##" stroke="#ffffff" stroke-width="3.5" stroke-linejoin="round" paint-order="stroke fill""##
                    } else {
                        ""
                    };

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
    <text x="{}" y="{}" fill="{}" font-family="{}" font-size="{}" font-weight="600" text-anchor="middle" dominant-baseline="central"{}>{}</text>
  </g>
"#,
                        filter_attr,
                        box_rect.x, box_rect.y, box_rect.width, box_rect.height,
                        self.theme.corner_radius, self.theme.corner_radius,
                        tokens.fill_color, tokens.stroke_color,
                        box_rect.center_x(), box_rect.center_y(),
                        tokens.text_color, self.theme.font_family, self.theme.font_size,
                        outline_attr,
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
                ResolvedAnnotation::Pin { head_center, head_radius, tip, icon, text, text_rect, style, shadow, outline } => {
                    let tokens = self.theme.tokens_for(*style);
                    let filter_attr = if *shadow { r#" filter="url(#markits-shadow)""# } else { "" };
                    let outline_attr = if *outline {
                        r##" stroke="#ffffff" stroke-width="3" stroke-linejoin="round" paint-order="stroke fill""##
                    } else {
                        ""
                    };

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
                        svg.push_str(&format!(
                            r##"    <rect x="{}" y="{}" width="{}" height="{}" rx="{}" ry="{}" fill="#1e293b" stroke="#ffffff" stroke-width="1.5"/>
    <text x="{}" y="{}" fill="#ffffff" font-family="{}" font-size="{}" font-weight="600" text-anchor="middle" dominant-baseline="central"{}>{}</text>
"##,
                            tr.x, tr.y, tr.width, tr.height, self.theme.corner_radius, self.theme.corner_radius,
                            tr.center_x(), tr.center_y(), self.theme.font_family, self.theme.font_size,
                            outline_attr,
                            escaped_text
                        ));
                    }

                    svg.push_str("  </g>\n");
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
        let layout_engine = LayoutEngine::new();
        let resolved = layout_engine.layout_scene(self);
        let renderer = SvgRenderer::new();
        Ok(renderer.render_scene(&resolved))
    }
}

/// Convenience function to render SVG directly from a JSON string.
pub fn render_from_json(json_str: &str) -> Result<String> {
    let scene = Scene::from_json(json_str)?;
    scene.render_svg()
}

#[cfg(test)]
mod tests {
    use super::*;
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
        assert!(svg.contains("paint-order=\"stroke fill\""));
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
}
