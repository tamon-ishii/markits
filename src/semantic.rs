//! Resolve AI-facing input into the existing coordinate-based scene model.
use crate::error::{MarkitsError, Result};
use crate::model::{Scene, TargetRect};
use serde_json::{Map, Value, json};

pub struct ResolvedInput {
    pub scene: Scene,
    /// One source ID per expanded annotation, in drawing order.
    pub source_ids: Vec<String>,
}

fn invalid(message: impl Into<String>) -> MarkitsError {
    MarkitsError::Validation(message.into())
}

fn distance(a: &str, b: &str) -> usize {
    let mut row: Vec<usize> = (0..=b.chars().count()).collect();
    for (i, ac) in a.chars().enumerate() {
        let mut previous = row[0];
        row[0] = i + 1;
        for (j, bc) in b.chars().enumerate() {
            let old = row[j + 1];
            row[j + 1] = (row[j + 1] + 1)
                .min(row[j] + 1)
                .min(previous + usize::from(ac != bc));
            previous = old;
        }
    }
    row[b.chars().count()]
}

fn validate_fields(map: &Map<String, Value>, allowed: &[&str], path: &str) -> Result<()> {
    for key in map.keys() {
        if !allowed.contains(&key.as_str()) {
            let suggestion = allowed
                .iter()
                .min_by_key(|candidate| distance(key, candidate))
                .filter(|candidate| distance(key, candidate) <= 2)
                .map(|candidate| format!("; did you mean {candidate:?}?"))
                .unwrap_or_default();
            return Err(invalid(format!(
                "{path}: unknown field {key:?}{suggestion}"
            )));
        }
    }
    Ok(())
}

fn rect(value: &Value, path: &str) -> Result<TargetRect> {
    if let Some(map) = value.as_object() {
        validate_fields(map, &["x", "y", "width", "height", "w", "h"], path)?;
    }
    let parsed: TargetRect =
        serde_json::from_value(value.clone()).map_err(|e| invalid(format!("{path}: {e}")))?;
    for (name, number) in [
        ("x", parsed.x),
        ("y", parsed.y),
        ("width", parsed.width),
        ("height", parsed.height),
    ] {
        if !number.is_finite() {
            return Err(invalid(format!("{path}.{name} must be finite")));
        }
    }
    if parsed.width <= 0.0 {
        let field = if value.is_array() { "[2]" } else { ".width" };
        return Err(invalid(format!("{path}{field} must be > 0")));
    }
    if parsed.height <= 0.0 {
        let field = if value.is_array() { "[3]" } else { ".height" };
        return Err(invalid(format!("{path}{field} must be > 0")));
    }
    Ok(parsed)
}

fn resolve_target(value: &Value, targets: &Map<String, Value>, path: &str) -> Result<Value> {
    let value = match value.as_str() {
        Some(name) => targets
            .get(name)
            .ok_or_else(|| invalid(format!("{path}: unknown target {name:?}")))?,
        None => value,
    };
    let r = rect(value, path)?;
    Ok(json!([r.x, r.y, r.width, r.height]))
}

fn push_annotation(value: Value, id: &str, output: &mut Vec<Value>, ids: &mut Vec<String>) {
    output.push(value);
    ids.push(id.to_string());
}

/// Parses and resolves named targets and instructions before normal Scene parsing.
pub fn resolve_json(json_str: &str) -> Result<ResolvedInput> {
    let mut root: Value = serde_json::from_str(json_str)?;
    let object = root
        .as_object_mut()
        .ok_or_else(|| invalid("scene must be an object"))?;
    validate_fields(
        object,
        &["canvas", "shadow", "targets", "annotations"],
        "scene",
    )?;
    if let Some(canvas) = object.get("canvas").and_then(Value::as_object) {
        validate_fields(canvas, &["width", "height"], "canvas")?;
    }
    let targets = object.remove("targets").unwrap_or_else(|| json!({}));
    let targets = targets
        .as_object()
        .ok_or_else(|| invalid("targets must be an object"))?;
    for (name, value) in targets {
        rect(value, &format!("targets.{name}"))?;
    }
    let annotations = object
        .entry("annotations")
        .or_insert_with(|| json!([]))
        .as_array_mut()
        .ok_or_else(|| invalid("annotations must be an array"))?;
    let mut expanded = Vec::new();
    let mut ids = Vec::new();
    let mut seen_ids = std::collections::HashSet::new();
    for (index, annotation) in annotations.drain(..).enumerate() {
        let mut ann = annotation
            .as_object()
            .cloned()
            .ok_or_else(|| invalid(format!("annotations[{index}] must be an object")))?;
        let kind = ann
            .get("type")
            .and_then(Value::as_str)
            .ok_or_else(|| invalid(format!("annotations[{index}].type is required")))?
            .to_string();
        let common = ["type", "id", "target", "style"];
        let extra: &[&str] = match kind.as_str() {
            "callout" | "label" => &["text", "position", "outline", "shadow"],
            "pin" | "pin-callout" | "pin_callout" => {
                &["text", "icon", "position", "outline", "shadow"]
            }
            "badge" => &["step", "text", "position", "arrow", "shadow"],
            "step-arrow" | "step_arrow" | "number-arrow" | "number_arrow" | "numbered-arrow"
            | "numbered_arrow" | "arrow-badge" | "badge-arrow" | "step-pin" | "arrow" => {
                &["step", "text", "position", "shadow"]
            }
            "rounded-rect" | "rounded_rect" => &["rx", "ry", "shadow"],
            "bezier-arrow" | "bezier_arrow" | "curved-arrow" | "curved_arrow" | "curve-arrow"
            | "curve_arrow" | "bezier" => &[
                "start",
                "from",
                "p0",
                "start_point",
                "control",
                "mid",
                "middle",
                "via",
                "p1",
                "intermediate",
                "control_point",
                "end",
                "to",
                "p2",
                "end_point",
                "text",
                "position",
                "text_position",
                "offset",
                "gap",
                "distance",
                "text_offset",
                "spacing",
                "t",
                "ratio",
                "progress",
                "along",
                "outline",
                "boxed",
                "box",
                "enclosure",
                "frame",
                "pill",
                "badge",
                "background",
                "shadow",
            ],
            "instruction" => &[
                "action", "text", "position", "outline", "to", "with", "shadow",
            ],
            "rect" | "circle" | "ellipse" | "bullseye" => &["shadow"],
            "spotlight" | "divider" => &[],
            _ => {
                return Err(invalid(format!(
                    "annotations[{index}].type: unknown type {kind:?}"
                )));
            }
        };
        let mut allowed = common.to_vec();
        allowed.extend_from_slice(extra);
        validate_fields(&ann, &allowed, &format!("annotations[{index}]"))?;
        if kind != "bezier-arrow"
            && !matches!(
                kind.as_str(),
                "bezier_arrow"
                    | "curved-arrow"
                    | "curved_arrow"
                    | "curve-arrow"
                    | "curve_arrow"
                    | "bezier"
            )
            && !ann.contains_key("target")
        {
            return Err(invalid(format!("annotations[{index}].target is required")));
        }
        if matches!(kind.as_str(), "callout" | "label") && !ann.contains_key("text") {
            return Err(invalid(format!("annotations[{index}].text is required")));
        }
        for key in [
            "start",
            "from",
            "p0",
            "start_point",
            "control",
            "mid",
            "middle",
            "via",
            "p1",
            "intermediate",
            "control_point",
            "end",
            "to",
            "p2",
            "end_point",
        ] {
            if kind != "instruction" {
                if let Some(value) = ann.get(key) {
                    if let Some(map) = value.as_object() {
                        validate_fields(map, &["x", "y"], &format!("annotations[{index}].{key}"))?;
                    }
                }
            }
        }
        for key in ["rx", "ry", "t"] {
            if let Some(value) = ann.get(key) {
                let number = value.as_f64().ok_or_else(|| {
                    invalid(format!("annotations[{index}].{key} must be a number"))
                })?;
                if number < 0.0 || (key == "t" && number > 1.0) {
                    return Err(invalid(format!(
                        "annotations[{index}].{key} is out of range"
                    )));
                }
            }
        }
        if let Some(style) = ann.get("style").and_then(Value::as_str) {
            const STYLES: &[&str] = &[
                "primary",
                "secondary",
                "warning",
                "danger",
                "info",
                "step",
                "pink",
            ];
            if !STYLES.contains(&style) {
                let suggestion = STYLES
                    .iter()
                    .min_by_key(|candidate| distance(style, candidate))
                    .filter(|candidate| distance(style, candidate) <= 2)
                    .map(|candidate| format!(" Did you mean {candidate:?}?"))
                    .unwrap_or_default();
                return Err(invalid(format!(
                    "annotations[{index}].style: unknown style {style:?}.{suggestion}"
                )));
            }
        }
        let id = ann
            .remove("id")
            .map(|v| {
                v.as_str()
                    .map(str::to_owned)
                    .ok_or_else(|| invalid(format!("annotations[{index}].id must be a string")))
            })
            .transpose()?
            .unwrap_or_else(|| format!("annotation-{index}"));
        if id.is_empty() || !seen_ids.insert(id.clone()) {
            return Err(invalid(format!(
                "annotations[{index}].id must be nonempty and unique"
            )));
        }
        if let Some(target) = ann.get("target") {
            ann.insert(
                "target".into(),
                resolve_target(target, targets, &format!("annotations[{index}].target"))?,
            );
        }
        if kind == "instruction" {
            for key in ["to", "with"] {
                if let Some(value) = ann.get(key) {
                    ann.insert(
                        key.into(),
                        resolve_target(value, targets, &format!("annotations[{index}].{key}"))?,
                    );
                }
            }
        }
        if kind == "instruction" {
            let action = ann
                .get("action")
                .and_then(Value::as_str)
                .ok_or_else(|| invalid(format!("annotations[{index}].action is required")))?;
            let target = ann
                .get("target")
                .ok_or_else(|| invalid(format!("annotations[{index}].target is required")))?
                .clone();
            let text = ann.get("text").and_then(Value::as_str).unwrap_or(action);
            let style = ann.get("style").cloned().unwrap_or_else(|| {
                if action == "warning" {
                    json!("warning")
                } else {
                    json!("primary")
                }
            });
            let position = ann
                .get("position")
                .cloned()
                .unwrap_or_else(|| json!("auto"));
            let shadow = ann.get("shadow").cloned().unwrap_or(Value::Null);
            let outline = ann.get("outline").cloned().unwrap_or(Value::Null);
            match action {
                "click" | "attention" | "warning" => {
                    push_annotation(
                        json!({"type":"spotlight","target":target,"style":style}),
                        &id,
                        &mut expanded,
                        &mut ids,
                    );
                }
                "select" => {
                    push_annotation(
                        json!({"type":"rect","target":target,"style":style}),
                        &id,
                        &mut expanded,
                        &mut ids,
                    );
                }
                "drag" => {
                    let to = ann.get("to").ok_or_else(|| {
                        invalid(format!("annotations[{index}].to is required for drag"))
                    })?;
                    let from_rect = rect(&target, &format!("annotations[{index}].target"))?;
                    let to_rect = rect(to, &format!("annotations[{index}].to"))?;
                    let start = [from_rect.center_x(), from_rect.center_y()];
                    let end = [to_rect.center_x(), to_rect.center_y()];
                    let control = [(start[0] + end[0]) / 2.0, (start[1] + end[1]) / 2.0 - 40.0];
                    push_annotation(
                        json!({"type":"bezier-arrow","start":start,"control":control,"end":end,"style":style}),
                        &id,
                        &mut expanded,
                        &mut ids,
                    );
                }
                "compare" => {
                    let with = ann.get("with").ok_or_else(|| {
                        invalid(format!("annotations[{index}].with is required for compare"))
                    })?;
                    push_annotation(
                        json!({"type":"callout","target":with,"text":text,"style":style,"position":position}),
                        &id,
                        &mut expanded,
                        &mut ids,
                    );
                }
                "enter" => {}
                _ => {
                    return Err(invalid(format!(
                        "annotations[{index}].action: unknown action {action:?}"
                    )));
                }
            }
            push_annotation(
                json!({"type":"callout","target":target,"text":text,"style":style,
                "position":position,"shadow":shadow,"outline":outline}),
                &id,
                &mut expanded,
                &mut ids,
            );
        } else {
            push_annotation(Value::Object(ann), &id, &mut expanded, &mut ids);
        }
    }
    *annotations = expanded;
    let scene: Scene = serde_json::from_value(root)?;
    scene.validate()?;
    Ok(ResolvedInput {
        scene,
        source_ids: ids,
    })
}
