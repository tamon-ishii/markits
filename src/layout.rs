use crate::model::{Annotation, ArrowSkin, ArrowTextPlacement, ArrowheadStyle, Canvas, LineStyle, PositionHint, Scene, SemanticStyle, TargetRect};

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

fn char_width(c: char, font_size: f64) -> f64 {
    if c.is_ascii() { font_size * 0.60 } else { font_size * 1.05 }
}

/// Wraps Latin text at spaces and CJK text between characters. Explicit newlines are preserved.
pub fn wrap_text(text: &str, font_size: f64, max_width: f64) -> String {
    let content_width = (max_width - 24.0).max(font_size);
    let mut output = Vec::new();
    for paragraph in text.split('\n') {
        let mut line = String::new();
        let mut width = 0.0;
        let mut word = String::new();
        let mut word_width = 0.0;
        let flush_word = |line: &mut String, width: &mut f64, word: &mut String, word_width: &mut f64, output: &mut Vec<String>| {
            if word.is_empty() { return; }
            if !line.is_empty() && *width + *word_width > content_width {
                output.push(std::mem::take(line));
                *width = 0.0;
            }
            for c in word.chars() {
                let cw = char_width(c, font_size);
                if !line.is_empty() && *width + cw > content_width {
                    output.push(std::mem::take(line));
                    *width = 0.0;
                }
                line.push(c);
                *width += cw;
            }
            word.clear();
            *word_width = 0.0;
        };
        for c in paragraph.chars() {
            if c.is_whitespace() {
                flush_word(&mut line, &mut width, &mut word, &mut word_width, &mut output);
                let cw = char_width(' ', font_size);
                if !line.is_empty() && width + cw <= content_width {
                    line.push(' ');
                    width += cw;
                }
            } else if !c.is_ascii() {
                flush_word(&mut line, &mut width, &mut word, &mut word_width, &mut output);
                let cw = char_width(c, font_size);
                if !line.is_empty() && width + cw > content_width {
                    output.push(std::mem::take(&mut line));
                    width = 0.0;
                }
                line.push(c);
                width += cw;
            } else {
                word.push(c);
                word_width += char_width(c, font_size);
            }
        }
        flush_word(&mut line, &mut width, &mut word, &mut word_width, &mut output);
        output.push(line.trim_end().to_string());
    }
    output.join("\n")
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

fn orientation(a: Point, b: Point, c: Point) -> f64 {
    (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x)
}

fn segments_cross(a: (Point, Point), b: (Point, Point)) -> bool {
    let left = orientation(a.0, a.1, b.0) * orientation(a.0, a.1, b.1);
    let right = orientation(b.0, b.1, a.0) * orientation(b.0, b.1, a.1);
    left < -1e-8 && right < -1e-8
}

fn segment_intersects_rect(segment: (Point, Point), rect: &TargetRect) -> bool {
    let (a, b) = segment;
    if (a.x > rect.x && a.x < rect.right() && a.y > rect.y && a.y < rect.bottom())
        || (b.x > rect.x && b.x < rect.right() && b.y > rect.y && b.y < rect.bottom()) {
        return true;
    }
    let tl = Point::new(rect.x, rect.y);
    let tr = Point::new(rect.right(), rect.y);
    let bl = Point::new(rect.x, rect.bottom());
    let br = Point::new(rect.right(), rect.bottom());
    [(tl, tr), (tr, br), (br, bl), (bl, tl)]
        .into_iter().any(|edge| segments_cross(segment, edge))
}

fn select_callout_candidate(
    target: &TargetRect,
    dimensions: Dimensions,
    offset: f64,
    canvas: &Canvas,
    hint: PositionHint,
    occupied_rects: &[TargetRect],
    occupied_arrows: &[(Point, Point)],
) -> Candidate {
    let candidates: Vec<Candidate> = [offset, offset + 24.0, offset + 48.0]
        .into_iter().flat_map(|distance| generate_candidates(target, dimensions, distance)).collect();
    candidates.into_iter().min_by(|a, b| {
        let score = |candidate: &Candidate| {
            let arrow = calculate_arrow_connection(&candidate.rect, target, candidate.anchor);
            let crossings = occupied_arrows.iter().filter(|&&other| segments_cross(arrow, other)).count() as f64;
            let over_labels = occupied_rects.iter().filter(|rect| segment_intersects_rect(arrow, rect)).count() as f64;
            calculate_score(candidate, target, canvas, hint, occupied_rects)
                + crossings * 30_000.0 + over_labels * 15_000.0
        };
        score(a).total_cmp(&score(b)).then_with(|| a.anchor.baseline_bias().total_cmp(&b.anchor.baseline_bias()))
    }).expect("candidate set is never empty")
}

fn callout_candidates(target: &TargetRect, dimensions: Dimensions, offset: f64) -> Vec<Candidate> {
    [offset, offset + 24.0, offset + 48.0]
        .into_iter()
        .flat_map(|distance| generate_candidates(target, dimensions, distance))
        .collect()
}

struct CalloutChoice {
    annotation_index: usize,
    target: TargetRect,
    hint: PositionHint,
    candidates: Vec<Candidate>,
}

fn pair_penalty(a: &Candidate, a_target: &TargetRect, b: &Candidate, b_target: &TargetRect) -> f64 {
    let a_arrow = calculate_arrow_connection(&a.rect, a_target, a.anchor);
    let b_arrow = calculate_arrow_connection(&b.rect, b_target, b.anchor);
    let mut score = 0.0;
    if a.rect.intersects(&b.rect) {
        let width = (a.rect.right().min(b.rect.right()) - a.rect.x.max(b.rect.x)).max(0.0);
        let height = (a.rect.bottom().min(b.rect.bottom()) - a.rect.y.max(b.rect.y)).max(0.0);
        score += 20_000.0 + width * height * 10.0;
    }
    if segments_cross(a_arrow, b_arrow) { score += 30_000.0; }
    if segment_intersects_rect(a_arrow, &b.rect) { score += 15_000.0; }
    if segment_intersects_rect(b_arrow, &a.rect) { score += 15_000.0; }
    score
}

fn total_callout_score(choices: &[CalloutChoice], selected: &[usize], scene: &Scene, fixed: &[TargetRect]) -> f64 {
    let mut score = 0.0;
    for (i, choice) in choices.iter().enumerate() {
        let candidate = &choice.candidates[selected[i]];
        let other_fixed: Vec<_> = fixed.iter().copied().filter(|r| *r != choice.target).collect();
        score += calculate_score(candidate, &choice.target, &scene.canvas, choice.hint, &other_fixed);
        let arrow = calculate_arrow_connection(&candidate.rect, &choice.target, candidate.anchor);
        score += other_fixed.iter().filter(|r| segment_intersects_rect(arrow, r)).count() as f64 * 15_000.0;
        for j in 0..i {
            score += pair_penalty(candidate, &choice.target, &choices[j].candidates[selected[j]], &choices[j].target);
        }
    }
    score
}

#[derive(Debug, Clone)]
pub struct DebugCandidate {
    pub rect: TargetRect,
    pub anchor: AnchorPosition,
    pub score: f64,
    pub selected: bool,
}

#[derive(Debug, Clone)]
pub struct DebugCallout {
    pub annotation_index: usize,
    pub target: TargetRect,
    pub candidates: Vec<DebugCandidate>,
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
        stroke_width: Option<f64>,
        line_style: LineStyle,
        arrowhead: ArrowheadStyle,
        arrow_skin: ArrowSkin,
        boxed: bool,
        outline: bool,
        text_placement: ArrowTextPlacement,
    },
    Rect {
        rect: TargetRect,
        style: SemanticStyle,
        shadow: bool,
        stroke_width: Option<f64>,
    },
    RoundedRect {
        rect: TargetRect,
        rx: f64,
        ry: f64,
        style: SemanticStyle,
        shadow: bool,
        stroke_width: Option<f64>,
    },
    Circle {
        cx: f64,
        cy: f64,
        rx: f64,
        ry: f64,
        style: SemanticStyle,
        shadow: bool,
        stroke_width: Option<f64>,
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
    StepArrow {
        center: Point,
        radius: f64,
        label: String,
        arrow_start: Point,
        arrow_end: Point,
        style: SemanticStyle,
        shadow: bool,
        stroke_width: Option<f64>,
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
    BezierArrow {
        start: Point,
        control: Point,
        end: Point,
        text: Option<String>,
        text_rect: Option<TargetRect>,
        style: SemanticStyle,
        shadow: bool,
        outline: bool,
        boxed: bool,
        stroke_width: Option<f64>,
        line_style: LineStyle,
        arrowhead: ArrowheadStyle,
        arrow_skin: ArrowSkin,
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
            | ResolvedAnnotation::StepArrow { shadow, .. }
            | ResolvedAnnotation::Pin { shadow, .. }
            | ResolvedAnnotation::Bullseye { shadow, .. }
            | ResolvedAnnotation::BezierArrow { shadow, .. } => *shadow,
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

    fn callout_problem(&self, scene: &Scene, resolved: &ResolvedScene) -> (Vec<CalloutChoice>, Vec<TargetRect>, Vec<usize>) {
        let mut fixed: Vec<TargetRect> = scene.annotations.iter().map(Annotation::target).collect();
        let mut choices = Vec::new();
        let mut selected = Vec::new();
        for (index, (input, output)) in scene.annotations.iter().zip(&resolved.annotations).enumerate() {
            match (input, output) {
                (Annotation::Callout { target, position, .. }, ResolvedAnnotation::Callout { box_rect, .. }) => {
                    let dim = Dimensions::new(box_rect.width, box_rect.height);
                    let candidates = callout_candidates(target, dim, self.callout_offset);
                    selected.push(candidates.iter().position(|c| c.rect == *box_rect).unwrap_or(0));
                    choices.push(CalloutChoice { annotation_index: index, target: *target, hint: *position, candidates });
                }
                (_, ResolvedAnnotation::Label { box_rect, .. }) => fixed.push(*box_rect),
                (_, ResolvedAnnotation::Pin { text_rect: Some(rect), .. })
                | (_, ResolvedAnnotation::BezierArrow { text_rect: Some(rect), .. }) => fixed.push(*rect),
                _ => {}
            }
        }
        (choices, fixed, selected)
    }

    fn optimize_callouts(&self, scene: &Scene, resolved: &mut ResolvedScene) {
        let (choices, fixed, greedy) = self.callout_problem(scene, resolved);
        if choices.len() < 2 { return; }
        let mut starts = vec![greedy];
        starts.push(vec![0; choices.len()]);
        let mut best_score = f64::INFINITY;
        let mut best = Vec::new();
        for mut selected in starts {
            for sweep in 0..12 {
                let mut changed = false;
                let order: Vec<usize> = if sweep % 2 == 0 { (0..choices.len()).collect() } else { (0..choices.len()).rev().collect() };
                for i in order {
                    let previous = selected[i];
                    let mut local_best = total_callout_score(&choices, &selected, scene, &fixed);
                    let mut local_choice = previous;
                    for candidate_index in 0..choices[i].candidates.len() {
                        selected[i] = candidate_index;
                        let score = total_callout_score(&choices, &selected, scene, &fixed);
                        if score + 1e-8 < local_best {
                            local_best = score;
                            local_choice = candidate_index;
                        }
                    }
                    selected[i] = local_choice;
                    changed |= selected[i] != previous;
                }
                if !changed { break; }
            }
            let score = total_callout_score(&choices, &selected, scene, &fixed);
            if score < best_score { best_score = score; best = selected; }
        }
        for (choice, index) in choices.iter().zip(best) {
            let candidate = &choice.candidates[index];
            if let ResolvedAnnotation::Callout { box_rect, arrow_start, arrow_end, .. } = &mut resolved.annotations[choice.annotation_index] {
                *box_rect = candidate.rect;
                (*arrow_start, *arrow_end) = calculate_arrow_connection(&candidate.rect, &choice.target, candidate.anchor);
            }
        }
    }

    pub fn debug_callouts(&self, scene: &Scene, resolved: &ResolvedScene) -> Vec<DebugCallout> {
        let (choices, fixed, selected) = self.callout_problem(scene, resolved);
        choices.iter().enumerate().map(|(i, choice)| {
            let candidates = choice.candidates.iter().enumerate().map(|(index, candidate)| {
                let mut trial = selected.clone();
                trial[i] = index;
                DebugCandidate { rect: candidate.rect, anchor: candidate.anchor, score: total_callout_score(&choices, &trial, scene, &fixed), selected: index == selected[i] }
            }).collect();
            DebugCallout { annotation_index: choice.annotation_index, target: choice.target, candidates }
        }).collect()
    }

    pub fn layout_scene(&self, scene: &Scene) -> ResolvedScene {
        let mut occupied_rects: Vec<TargetRect> = Vec::new();
        for target in scene.annotations.iter().map(Annotation::target) {
            if !occupied_rects.contains(&target) { occupied_rects.push(target); }
        }
        let mut occupied_arrows: Vec<(Point, Point)> = Vec::new();
        let mut resolved: Vec<ResolvedAnnotation> = Vec::new();

        for ann in &scene.annotations {
            let shadow = ann.shadow_override().unwrap_or(scene.shadow);
            let outline = ann.outline_override().unwrap_or(true);

            match ann {
                Annotation::Rect { target, style, stroke_width, .. } => {
                    resolved.push(ResolvedAnnotation::Rect {
                        rect: *target,
                        style: *style,
                        shadow,
                        stroke_width: *stroke_width,
                    });
                    occupied_rects.push(*target);
                }
                Annotation::RoundedRect {
                    target,
                    rx,
                    ry,
                    style,
                    stroke_width,
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
                        stroke_width: *stroke_width,
                    });
                    occupied_rects.push(*target);
                }
                Annotation::Circle { target, style, stroke_width, .. } => {
                    resolved.push(ResolvedAnnotation::Circle {
                        cx: target.center_x(),
                        cy: target.center_y(),
                        rx: target.width / 2.0,
                        ry: target.height / 2.0,
                        style: *style,
                        shadow,
                        stroke_width: *stroke_width,
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
                    max_width,
                    style,
                    position,
                    ..
                } => {
                    let wrapped = wrap_text(text, self.font_size, max_width.unwrap_or((scene.canvas.width as f64 * 0.55).min(320.0)));
                    let dim = estimate_text_dimensions(&wrapped, self.font_size);
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
                        text: wrapped,
                        style: *style,
                        shadow,
                        outline,
                    });
                }
                Annotation::Callout {
                    target,
                    text,
                    max_width,
                    style,
                    position,
                    ..
                } => {
                    let wrapped = wrap_text(text, self.font_size, max_width.unwrap_or((scene.canvas.width as f64 * 0.55).min(320.0)));
                    let dim = estimate_text_dimensions(&wrapped, self.font_size);
                    let best = select_callout_candidate(
                        target,
                        dim,
                        self.callout_offset,
                        &scene.canvas,
                        *position,
                        &occupied_rects,
                        &occupied_arrows,
                    );
                    let (arrow_start, arrow_end) =
                        calculate_arrow_connection(&best.rect, target, best.anchor);
                    occupied_rects.push(best.rect);
                    occupied_arrows.push((arrow_start, arrow_end));
                    resolved.push(ResolvedAnnotation::Callout {
                        box_rect: best.rect,
                        text: wrapped,
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
                    arrow,
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
                    if arrow == &Some(true) {
                        let offset = self.callout_offset + 8.0;
                        let candidates = generate_candidates(target, dim, offset);
                        let best = select_best_candidate(
                            &candidates,
                            target,
                            &scene.canvas,
                            *position,
                            &occupied_rects,
                        );
                        let center = Point::new(best.rect.center_x(), best.rect.center_y());
                        let (_, arrow_end) =
                            calculate_arrow_connection(&best.rect, target, best.anchor);
                        let dx = arrow_end.x - center.x;
                        let dy = arrow_end.y - center.y;
                        let len = (dx * dx + dy * dy).sqrt().max(0.001);
                        let arrow_start = Point::new(
                            center.x + (dx / len) * self.badge_radius,
                            center.y + (dy / len) * self.badge_radius,
                        );
                        occupied_rects.push(best.rect);
                        resolved.push(ResolvedAnnotation::StepArrow {
                            center,
                            radius: self.badge_radius,
                            label: badge_label,
                            arrow_start,
                            arrow_end,
                            style: *style,
                            shadow,
                            stroke_width: None,
                        });
                    } else {
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
                }
                Annotation::StepArrow {
                    target,
                    stroke_width,
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
                    let offset = self.callout_offset + 8.0;
                    let candidates = generate_candidates(target, dim, offset);
                    let best = select_best_candidate(
                        &candidates,
                        target,
                        &scene.canvas,
                        *position,
                        &occupied_rects,
                    );
                    let center = Point::new(best.rect.center_x(), best.rect.center_y());
                    let (_, arrow_end) =
                        calculate_arrow_connection(&best.rect, target, best.anchor);
                    let dx = arrow_end.x - center.x;
                    let dy = arrow_end.y - center.y;
                    let len = (dx * dx + dy * dy).sqrt().max(0.001);
                    let arrow_start = Point::new(
                        center.x + (dx / len) * self.badge_radius,
                        center.y + (dy / len) * self.badge_radius,
                    );
                    occupied_rects.push(best.rect);
                    resolved.push(ResolvedAnnotation::StepArrow {
                        center,
                        radius: self.badge_radius,
                        label: badge_label,
                        arrow_start,
                        arrow_end,
                        style: *style,
                        shadow,
                        stroke_width: *stroke_width,
                    });
                }
                Annotation::Arrow {
                    target,
                    start: explicit_start,
                    end: explicit_end,
                    stroke_width,
                    line_style,
                    arrowhead,
                    arrow_skin,
                    step,
                    text,
                    text_placement,
                    style,
                    position,
                    boxed,
                    outline,
                    ..
                } => {
                    let arrow_boxed = boxed.unwrap_or(true);
                    let arrow_outline = outline.unwrap_or(true);
                    let placement = text_placement.unwrap_or_default();
                    if let (Some(s), Some(e)) = (explicit_start, explicit_end) {
                        let arrow_start = Point::new(s.x, s.y);
                        let arrow_end = Point::new(e.x, e.y);
                        if let Some(s_num) = step {
                            resolved.push(ResolvedAnnotation::StepArrow {
                                center: arrow_start,
                                radius: self.badge_radius,
                                label: s_num.to_string(),
                                arrow_start,
                                arrow_end,
                                style: *style,
                                shadow,
                                stroke_width: *stroke_width,
                            });
                        } else {
                            resolved.push(ResolvedAnnotation::Arrow {
                                start: arrow_start,
                                end: arrow_end,
                                text: text.clone(),
                                style: *style,
                                shadow,
                                stroke_width: *stroke_width,
                                line_style: *line_style,
                                arrowhead: *arrowhead,
                                arrow_skin: *arrow_skin,
                                boxed: arrow_boxed,
                                outline: arrow_outline,
                                text_placement: placement,
                            });
                        }
                    } else if let Some(target) = target {
                        if step.is_some() || text.is_some() {
                            let badge_label = if let Some(s) = step {
                                s.to_string()
                            } else if let Some(t) = text {
                                t.clone()
                            } else {
                                "1".to_string()
                            };

                            let dim = Dimensions::new(self.badge_radius * 2.0, self.badge_radius * 2.0);
                            let offset = self.callout_offset + 8.0;
                            let candidates = generate_candidates(target, dim, offset);
                            let best = select_best_candidate(
                                &candidates,
                                target,
                                &scene.canvas,
                                *position,
                                &occupied_rects,
                            );
                            let center = Point::new(best.rect.center_x(), best.rect.center_y());
                            let (_, arrow_end) =
                                calculate_arrow_connection(&best.rect, target, best.anchor);
                            let dx = arrow_end.x - center.x;
                            let dy = arrow_end.y - center.y;
                            let len = (dx * dx + dy * dy).sqrt().max(0.001);
                            let arrow_start = Point::new(
                                center.x + (dx / len) * self.badge_radius,
                                center.y + (dy / len) * self.badge_radius,
                            );
                            occupied_rects.push(best.rect);
                            resolved.push(ResolvedAnnotation::StepArrow {
                                center,
                                radius: self.badge_radius,
                                label: badge_label,
                                arrow_start,
                                arrow_end,
                                style: *style,
                                shadow,
                                stroke_width: *stroke_width,
                            });
                        } else {
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
                                stroke_width: *stroke_width,
                                line_style: *line_style,
                                arrowhead: *arrowhead,
                                arrow_skin: *arrow_skin,
                                boxed: arrow_boxed,
                                outline: arrow_outline,
                                text_placement: placement,
                            });
                        }
                    }
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
                Annotation::BezierArrow {
                    target,
                    start,
                    control,
                    end,
                    text,
                    text_placement,
                    stroke_width,
                    line_style,
                    arrowhead,
                    arrow_skin,
                    style,
                    position,
                    offset,
                    t,
                    boxed,
                    ..
                } => {
                    let is_boxed = boxed.unwrap_or(true);
                    let (s, e, c) = match (start, end, control, target) {
                        (Some(sp), Some(ep), Some(cp), _) => (
                            Point::new(sp.x, sp.y),
                            Point::new(ep.x, ep.y),
                            Point::new(cp.x, cp.y),
                        ),
                        (Some(sp), Some(ep), None, _) => {
                            let sp = Point::new(sp.x, sp.y);
                            let ep = Point::new(ep.x, ep.y);
                            let cp = Point::new((sp.x + ep.x) / 2.0, (sp.y + ep.y) / 2.0 - 50.0);
                            (sp, ep, cp)
                        }
                        _ => {
                            if let Some(t) = target {
                                let sp = start
                                    .map(|p| Point::new(p.x, p.y))
                                    .unwrap_or_else(|| Point::new(t.x, t.bottom()));
                                let ep = end
                                    .map(|p| Point::new(p.x, p.y))
                                    .unwrap_or_else(|| Point::new(t.right(), t.bottom()));
                                let cp = control
                                    .map(|p| Point::new(p.x, p.y))
                                    .unwrap_or_else(|| Point::new(t.center_x(), t.y - 30.0));
                                (sp, ep, cp)
                            } else {
                                let sp = start
                                    .map(|p| Point::new(p.x, p.y))
                                    .unwrap_or_else(|| Point::new(50.0, 150.0));
                                let ep = end
                                    .map(|p| Point::new(p.x, p.y))
                                    .unwrap_or_else(|| Point::new(250.0, 150.0));
                                let cp = control
                                    .map(|p| Point::new(p.x, p.y))
                                    .unwrap_or_else(|| Point::new((sp.x + ep.x) / 2.0, (sp.y + ep.y) / 2.0 - 50.0));
                                (sp, ep, cp)
                            }
                        }
                    };

                    let default_t = match text_placement {
                        // The label's "矢印の終端" is the tail without the arrowhead.
                        Some(ArrowTextPlacement::End) => 0.0,
                        _ => 0.5,
                    };
                    let param_t = if *text_placement == Some(ArrowTextPlacement::End) {
                        0.0
                    } else {
                        t.unwrap_or(default_t).clamp(0.0, 1.0)
                    };
                    let one_minus_t = 1.0 - param_t;
                    let b0 = one_minus_t * one_minus_t;
                    let b1 = 2.0 * one_minus_t * param_t;
                    let b2 = param_t * param_t;
                    let mid_x = b0 * s.x + b1 * c.x + b2 * e.x;
                    let mid_y = b0 * s.y + b1 * c.y + b2 * e.y;

                    let (text_rect, text_val) = match text {
                        Some(txt) if !txt.trim().is_empty() => {
                            let dim = estimate_text_dimensions(txt, self.font_size);
                            let chord_y = (s.y + e.y) / 2.0;
                            let vy = c.y - chord_y;

                            // Distance gap between curve and text
                            let gap = offset.unwrap_or(if is_boxed { 8.0 } else { 6.0 });
                            let half_h = dim.height / 2.0;
                            let half_w = dim.width / 2.0;

                            let at_tail = *text_placement == Some(ArrowTextPlacement::End);
                            let label_anchor = if at_tail {
                                (s.x, s.y)
                            } else {
                                (mid_x, mid_y)
                            };
                            let (anchor_x, anchor_y) = label_anchor;
                            let (center_x, center_y) = if at_tail {
                                // Move outside the tail along the reverse initial tangent.
                                let mut tx = s.x - c.x;
                                let mut ty = s.y - c.y;
                                let tangent_len = (tx * tx + ty * ty).sqrt();
                                if tangent_len < 0.001 {
                                    tx = s.x - e.x;
                                    ty = s.y - e.y;
                                }
                                let tangent_len = (tx * tx + ty * ty).sqrt().max(0.001);
                                tx /= tangent_len;
                                ty /= tangent_len;
                                let extent = half_w * tx.abs() + half_h * ty.abs();
                                let distance = extent + gap + 4.0;
                                (s.x + tx * distance, s.y + ty * distance)
                            } else {
                                match position {
                                PositionHint::Center => {
                                    // Use the curve tangent to choose a clear side, and include
                                    // the label's projected bounds in the separation distance.
                                    let mut tx = 2.0 * (1.0 - param_t) * (c.x - s.x)
                                        + 2.0 * param_t * (e.x - c.x);
                                    let mut ty = 2.0 * (1.0 - param_t) * (c.y - s.y)
                                        + 2.0 * param_t * (e.y - c.y);
                                    let tangent_len = (tx * tx + ty * ty).sqrt().max(0.001);
                                    tx /= tangent_len;
                                    ty /= tangent_len;
                                    let distance = if tx.abs() >= ty.abs() {
                                        half_h + gap
                                    } else {
                                        half_w + gap
                                    };
                                    if tx.abs() >= ty.abs() {
                                        let side = if tx >= 0.0 { -1.0 } else { 1.0 };
                                        (anchor_x, anchor_y + side * distance)
                                    } else {
                                        (anchor_x + distance, anchor_y)
                                    }
                                }
                                PositionHint::Top => (anchor_x, anchor_y - (half_h + gap)),
                                PositionHint::Bottom => (anchor_x, anchor_y + (half_h + gap)),
                                PositionHint::Left => (anchor_x - (half_w + gap), anchor_y),
                                PositionHint::Right => (anchor_x + (half_w + gap), anchor_y),
                                _ => {
                                    // Auto: place on the convex (outer) side of the curve
                                    if vy < -5.0 {
                                        // Arches upwards -> place above curve
                                        (anchor_x, anchor_y - (half_h + gap))
                                    } else if vy > 5.0 {
                                        // Arches downwards -> place below curve
                                        (anchor_x, anchor_y + (half_h + gap))
                                    } else {
                                        // Mostly flat or vertical -> default above curve
                                        (anchor_x, anchor_y - (half_h + gap))
                                    }
                                }
                                }
                            };

                            let tr = TargetRect::new(
                                center_x - half_w,
                                center_y - half_h,
                                dim.width,
                                dim.height,
                            );
                            occupied_rects.push(tr);
                            (Some(tr), Some(txt.clone()))
                        }
                        _ => (None, None),
                    };

                    let min_x = s.x.min(c.x).min(e.x);
                    let max_x = s.x.max(c.x).max(e.x);
                    let min_y = s.y.min(c.y).min(e.y);
                    let max_y = s.y.max(c.y).max(e.y);
                    occupied_rects.push(TargetRect::new(
                        min_x,
                        min_y,
                        (max_x - min_x).max(1.0),
                        (max_y - min_y).max(1.0),
                    ));

                    resolved.push(ResolvedAnnotation::BezierArrow {
                        start: s,
                        control: c,
                        end: e,
                        text: text_val,
                        text_rect,
                        style: *style,
                        shadow,
                        outline,
                        boxed: is_boxed,
                        stroke_width: *stroke_width,
                        line_style: *line_style,
                        arrowhead: *arrowhead,
                        arrow_skin: *arrow_skin,
                    });
                }
            }
        }

        let mut result = ResolvedScene {
            canvas: scene.canvas,
            annotations: resolved,
        };
        self.optimize_callouts(scene, &mut result);
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn japanese_text_wraps_within_width() {
        let wrapped = wrap_text("設定が完了したら、このボタンをクリックしてください", 14.0, 120.0);
        assert!(wrapped.contains('\n'));
        for line in wrapped.split('\n') {
            assert!(estimate_text_dimensions(line, 14.0).width <= 120.0);
        }
        assert_eq!(wrapped.replace('\n', ""), "設定が完了したら、このボタンをクリックしてください");
    }

    #[test]
    fn global_layout_improves_joint_candidate_score() {
        let scene = Scene::from_json(r#"{
          "canvas":{"width":420,"height":260},
          "annotations":[
            {"type":"callout","target":[120,80,40,28],"text":"最初の設定を保存します","max_width":120},
            {"type":"callout","target":[190,95,40,28],"text":"次の設定を選択します","max_width":120},
            {"type":"callout","target":[250,120,40,28],"text":"最後に確認します","max_width":120}
          ]
        }"#).unwrap();
        let engine = LayoutEngine::new();
        let resolved = engine.layout_scene(&scene);
        let (choices, fixed, selected) = engine.callout_problem(&scene, &resolved);
        let initial = total_callout_score(&choices, &vec![0; choices.len()], &scene, &fixed);
        let final_score = total_callout_score(&choices, &selected, &scene, &fixed);
        assert!(final_score < initial);
        assert_eq!(engine.debug_callouts(&scene, &resolved).len(), 3);
    }

    #[test]
    fn auto_callout_avoids_existing_arrow_crossing() {
        let target = TargetRect::new(280.0, 200.0, 40.0, 30.0);
        let other_arrow = (Point::new(200.0, 190.0), Point::new(400.0, 190.0));
        let best = select_callout_candidate(
            &target, Dimensions::new(100.0, 30.0), 16.0,
            &Canvas { width: 600, height: 400 }, PositionHint::Auto,
            &[], &[other_arrow],
        );
        let selected_arrow = calculate_arrow_connection(&best.rect, &target, best.anchor);
        assert!(!segments_cross(selected_arrow, other_arrow));
    }

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
            hidden_annotations: vec![],
            uimap: None,
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

    #[test]
    fn test_bezier_arrow_layout() {
        use crate::model::Point2D;

        let scene = Scene {
            canvas: Canvas { width: 1000, height: 800 },
            shadow: true,
            annotations: vec![
                Annotation::BezierArrow {
                    target: None,
                    start: Some(Point2D::new(100.0, 300.0)),
                    control: Some(Point2D::new(300.0, 100.0)),
                    end: Some(Point2D::new(500.0, 300.0)),
                    stroke_width: None,
                    line_style: LineStyle::Solid,
                    arrowhead: ArrowheadStyle::Filled,
                    arrow_skin: ArrowSkin::Classic,
                    text: Some("Midpoint Text".to_string()),
                    style: SemanticStyle::Primary,
                    position: PositionHint::Center,
                    offset: None,
                    t: None,
                    shadow: Some(true),
                    outline: Some(true),
                    boxed: Some(false),
                    text_placement: None,
                },
                Annotation::BezierArrow {
                    target: None,
                    start: Some(Point2D::new(100.0, 300.0)),
                    control: Some(Point2D::new(300.0, 100.0)),
                    end: Some(Point2D::new(500.0, 300.0)),
                    stroke_width: None,
                    line_style: LineStyle::Solid,
                    arrowhead: ArrowheadStyle::Filled,
                    arrow_skin: ArrowSkin::Classic,
                    text: Some("Offset Text".to_string()),
                    style: SemanticStyle::Primary,
                    position: PositionHint::Top,
                    offset: Some(20.0),
                    t: Some(0.5),
                    shadow: Some(false),
                    outline: Some(true),
                    boxed: Some(true),
                    text_placement: None,
                },
            ],
            hidden_annotations: vec![],
            uimap: None,
        };

        let engine = LayoutEngine::new();
        let resolved = engine.layout_scene(&scene);

        assert_eq!(resolved.annotations.len(), 2);
        if let ResolvedAnnotation::BezierArrow { start, control, end, text, text_rect, boxed, .. } = &resolved.annotations[0] {
            assert_eq!(start.x, 100.0);
            assert_eq!(start.y, 300.0);
            assert_eq!(control.x, 300.0);
            assert_eq!(control.y, 100.0);
            assert_eq!(end.x, 500.0);
            assert_eq!(end.y, 300.0);
            assert_eq!(text.as_deref(), Some("Midpoint Text"));
            assert!(!boxed);

            // Anchored at t=0.5, then moved above the horizontal tangent so the
            // label bounds clear the curve.
            let tr = text_rect.as_ref().expect("text_rect should be Some");
            assert!((tr.center_x() - 300.0).abs() < 1e-4);
            assert!(tr.bottom() < 200.0);
        } else {
            panic!("Expected ResolvedAnnotation::BezierArrow");
        }

        if let ResolvedAnnotation::BezierArrow { text, text_rect, .. } = &resolved.annotations[1] {
            assert_eq!(text.as_deref(), Some("Offset Text"));
            let tr = text_rect.as_ref().expect("text_rect should be Some");
            // Midpoint is y=200. With position: Top and offset: 20.0,
            // center_y = 200.0 - (tr.height / 2.0 + 20.0)
            // bottom of box = center_y + tr.height / 2.0 = 200.0 - 20.0 = 180.0
            assert!((tr.bottom() - 180.0).abs() < 1e-4);
        } else {
            panic!("Expected second BezierArrow");
        }
    }
}
