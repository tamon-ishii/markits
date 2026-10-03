//! Converts named targets and high-level instructions into renderable annotations.
use crate::error::{MarkitsError, Result};
use crate::model::TargetRect;
use serde_json::{Map, Value, json};
use std::collections::{HashMap, HashSet};

pub struct PreparedInput {
    pub scene: Value,
    pub ids: Vec<String>,
}

fn invalid(message: impl Into<String>) -> MarkitsError {
    MarkitsError::Validation(message.into())
}

fn suggestion(value: &str, options: &[&str]) -> Option<String> {
    fn distance(a: &str, b: &str) -> usize {
        let mut costs: Vec<usize> = (0..=b.chars().count()).collect();
        for (i, ca) in a.chars().enumerate() {
            let mut previous = i;
            costs[0] = i + 1;
            for (j, cb) in b.chars().enumerate() {
                let old = costs[j + 1];
                costs[j + 1] = (costs[j + 1] + 1)
                    .min(costs[j] + 1)
                    .min(previous + usize::from(ca != cb));
                previous = old;
            }
        }
        costs[b.chars().count()]
    }
    options
        .iter()
        .map(|option| (*option, distance(value, option)))
        .min_by_key(|(_, score)| *score)
        .and_then(|(option, score)| (score <= 2).then(|| option.to_string()))
}

fn validate_rect(rect: &TargetRect, value: &Value, path: &str) -> Result<()> {
    let coordinate = |index: usize, field: &str| {
        if value.is_array() {
            format!("{path}[{index}]")
        } else {
            format!("{path}.{field}")
        }
    };
    for (index, field, number) in [
        (0, "x", rect.x),
        (1, "y", rect.y),
        (2, "width", rect.width),
        (3, "height", rect.height),
    ] {
        if !number.is_finite() {
            return Err(invalid(format!(
                "{} must be finite",
                coordinate(index, field)
            )));
        }
    }
    if rect.width <= 0.0 {
        return Err(invalid(format!("{} must be > 0", coordinate(2, "width"))));
    }
    if rect.height <= 0.0 {
        return Err(invalid(format!("{} must be > 0", coordinate(3, "height"))));
    }
    Ok(())
}

fn find_target_rect<'a>(name: &str, targets: &'a HashMap<String, TargetRect>) -> Option<&'a TargetRect> {
    // 1. Exact match
    if let Some(rect) = targets.get(name) {
        return Some(rect);
    }

    // 2. Case-insensitive exact match
    let lower_name = name.to_lowercase();
    for (k, rect) in targets {
        if k.to_lowercase() == lower_name {
            return Some(rect);
        }
    }

    // 3. Trimming common Japanese / English suffixes: "保存ボタン" -> "保存", "Submit button" -> "Submit"
    let stripped = name
        .trim_end_matches("ボタン")
        .trim_end_matches(" button")
        .trim_end_matches(" Button")
        .trim();
    if !stripped.is_empty() && stripped != name {
        if let Some(rect) = targets.get(stripped) {
            return Some(rect);
        }
        for (k, rect) in targets {
            if k.to_lowercase() == stripped.to_lowercase() {
                return Some(rect);
            }
        }
    }

    // 4. Substring / contains match (without role prefix)
    for (k, rect) in targets {
        if !k.contains(':') && !k.starts_with("ui-") && (k.contains(name) || name.contains(k.as_str())) {
            return Some(rect);
        }
    }

    // 5. Role-prefixed match (e.g. "button:保存")
    for (k, rect) in targets {
        if let Some((_role, el_name)) = k.split_once(':') {
            if el_name == name || el_name == stripped || el_name.contains(name) || name.contains(el_name) {
                return Some(rect);
            }
        }
    }

    None
}

fn named_target(value: &Value, targets: &HashMap<String, TargetRect>, path: &str) -> Result<Value> {
    if let Some(name) = value.as_str() {
        let rect = find_target_rect(name, targets).ok_or_else(|| {
            let mut known: Vec<&str> = targets.keys().map(String::as_str).collect();
            known.sort_unstable();
            let hint = suggestion(name, &known)
                .map(|candidate| format!(" Did you mean '{candidate}'?"))
                .unwrap_or_default();
            invalid(format!("{path}: unknown target '{name}'.{hint}"))
        })?;
        return Ok(serde_json::to_value(rect)?);
    }
    let rect: TargetRect = serde_json::from_value(value.clone())
        .map_err(|error| invalid(format!("{path}: {error}")))?;
    validate_rect(&rect, value, path)?;
    Ok(value.clone())
}

fn check_key(value: &str, path: &str, options: &[&str]) -> Result<()> {
    if options.contains(&value) {
        return Ok(());
    }
    let hint = suggestion(value, options)
        .map(|candidate| format!(" Did you mean '{candidate}'?"))
        .unwrap_or_default();
    Err(invalid(format!("{path}: unknown value '{value}'.{hint}")))
}

fn instruction_parts(
    object: &Map<String, Value>,
    path: &str,
    targets: &HashMap<String, TargetRect>,
) -> Result<Vec<Value>> {
    for key in object.keys() {
        if ![
            "type",
            "target",
            "destination",
            "action",
            "text",
            "style",
            "position",
            "max_width",
            "outline",
            "shadow",
        ]
        .contains(&key.as_str())
        {
            return Err(invalid(format!("{path}: unknown field '{key}'")));
        }
    }
    let action = object
        .get("action")
        .and_then(Value::as_str)
        .ok_or_else(|| invalid(format!("{path}.action: expected an action string")))?;
    check_key(
        action,
        &format!("{path}.action"),
        &[
            "click",
            "enter",
            "select",
            "drag",
            "attention",
            "warning",
            "compare",
        ],
    )?;
    let target = object
        .get("target")
        .cloned()
        .ok_or_else(|| invalid(format!("{path}.target: required")))?;
    let text = object
        .get("text")
        .and_then(Value::as_str)
        .ok_or_else(|| invalid(format!("{path}.text: expected a string")))?;
    let style = object
        .get("style")
        .and_then(Value::as_str)
        .unwrap_or(if action == "warning" {
            "warning"
        } else {
            "primary"
        });
    let position = object
        .get("position")
        .and_then(Value::as_str)
        .unwrap_or("auto");
    let mut parts = Vec::new();
    if matches!(action, "click" | "attention") {
        parts.push(json!({"type":"spotlight","target":target,"style":style}));
    } else if matches!(action, "enter" | "select" | "drag" | "warning" | "compare") {
        parts.push(json!({"type":"rounded-rect","target":target,"style":style}));
    }
    let mut callout_target = target.clone();
    if matches!(action, "drag" | "compare") {
        let destination = object.get("destination").ok_or_else(|| {
            invalid(format!(
                "{path}.destination: required for action '{action}'"
            ))
        })?;
        let source_value = named_target(&target, targets, &format!("{path}.target"))?;
        let destination_value = named_target(destination, targets, &format!("{path}.destination"))?;
        let source: TargetRect = serde_json::from_value(source_value)?;
        let dest: TargetRect = serde_json::from_value(destination_value)?;
        parts.push(json!({"type":"rounded-rect","target":destination,"style":style}));
        if action == "drag" {
            let start = [source.center_x(), source.center_y()];
            let end = [dest.center_x(), dest.center_y()];
            let control = [(start[0] + end[0]) / 2.0, (start[1] + end[1]) / 2.0 - 32.0];
            parts.push(json!({"type":"bezier-arrow","start":start,"control":control,"end":end,"style":style}));
        }
        callout_target = destination.clone();
    } else if object.contains_key("destination") {
        return Err(invalid(format!(
            "{path}.destination: only valid for drag or compare"
        )));
    }
    let mut callout = json!({"type":"callout","target":callout_target,"text":text,"style":style,"position":position});
    if let Some(width) = object.get("max_width") { callout["max_width"] = width.clone(); }
    if let Some(outline) = object.get("outline") { callout["outline"] = outline.clone(); }
    if let Some(shadow) = object.get("shadow") { callout["shadow"] = shadow.clone(); }
    parts.push(callout);
    Ok(parts)
}

/// Parses the public semantic document while retaining one ID per rendered annotation.
pub fn prepare(json_str: &str) -> Result<PreparedInput> {
    let mut root: Value = serde_json::from_str(json_str)?;
    let object = root
        .as_object_mut()
        .ok_or_else(|| invalid("scene: expected a JSON object"))?;
    for key in object.keys() {
        if !["canvas", "shadow", "annotations", "hidden_annotations", "targets", "uimap", "ui_map", "ui_elements"].contains(&key.as_str()) {
            return Err(invalid(format!("scene: unknown field '{key}'")));
        }
    }
    let raw_targets = object.remove("targets").unwrap_or_else(|| json!({}));
    let raw_targets = raw_targets
        .as_object()
        .ok_or_else(|| invalid("targets: expected an object"))?;
    let mut targets = HashMap::new();
    for (name, value) in raw_targets {
        if name.is_empty() {
            return Err(invalid("targets: names must not be empty"));
        }
        let rect: TargetRect = serde_json::from_value(value.clone())
            .map_err(|error| invalid(format!("targets.{name}: {error}")))?;
        validate_rect(&rect, value, &format!("targets.{name}"))?;
        targets.insert(name.clone(), rect);
    }

    let raw_uimap = object
        .remove("uimap")
        .or_else(|| object.remove("ui_map"))
        .or_else(|| object.remove("ui_elements"));
    let mut parsed_uimap = None;
    if let Some(uimap_val) = raw_uimap {
        let elements: Vec<crate::model::UiElement> = serde_json::from_value(uimap_val)
            .map_err(|error| invalid(format!("uimap: {error}")))?;
        for (idx, el) in elements.iter().enumerate() {
            let rect = el.rect();
            let trimmed = el.name.trim();
            if !trimmed.is_empty() {
                targets.entry(trimmed.to_string()).or_insert(rect);
                targets.entry(format!("{}:{}", el.role, trimmed)).or_insert(rect);
            }
            targets.entry(format!("{}:{}", el.role, idx + 1)).or_insert(rect);
            targets.entry(format!("ui-{}", idx + 1)).or_insert(rect);
        }
        parsed_uimap = Some(elements);
    }
    if let Some(ref elements) = parsed_uimap {
        if !elements.is_empty() {
            object.insert("uimap".to_owned(), serde_json::to_value(elements).unwrap_or(Value::Null));
        }
    }
    let raw_annotations = match object.get("annotations") {
        None => Vec::new(),
        Some(Value::Array(items)) => items.clone(),
        Some(_) => return Err(invalid("annotations: expected an array")),
    };
    let mut annotations = Vec::new();
    let mut ids = Vec::new();
    let mut used_ids = HashSet::new();
    for (index, item) in raw_annotations.iter().enumerate() {
        let path = format!("annotations[{index}]");
        let mut ann = item
            .as_object()
            .cloned()
            .ok_or_else(|| invalid(format!("{path}: expected an object")))?;
        let id = ann
            .remove("id")
            .map(|value| {
                value
                    .as_str()
                    .map(str::to_owned)
                    .ok_or_else(|| invalid(format!("{path}.id: expected a string")))
            })
            .transpose()?
            .unwrap_or_else(|| format!("annotation-{index}"));
        if id.is_empty() || !used_ids.insert(id.clone()) {
            return Err(invalid(format!("{path}.id: must be nonempty and unique")));
        }
        let is_instruction = ann
            .get("type")
            .and_then(Value::as_str)
            .ok_or_else(|| invalid(format!("{path}.type: expected a string")))?
            == "instruction";
        let expanded = if is_instruction {
            instruction_parts(&ann, &path, &targets)?
        } else {
            vec![Value::Object(ann)]
        };
        for (part_index, mut part) in expanded.into_iter().enumerate() {
            let part_obj = part.as_object_mut().expect("annotation parts are objects");
            if let Some(target) = part_obj.get("target") {
                part_obj.insert(
                    "target".to_owned(),
                    named_target(target, &targets, &format!("{path}.target"))?,
                );
            }
            if let Some(style) = part_obj.get("style").and_then(Value::as_str) {
                check_key(
                    style,
                    &format!("{path}.style"),
                    &[
                        "primary",
                        "secondary",
                        "warning",
                        "danger",
                        "info",
                        "step",
                        "pink",
                    ],
                )?;
            }
            if let Some(position) = part_obj.get("position").and_then(Value::as_str) {
                check_key(
                    position,
                    &format!("{path}.position"),
                    &[
                        "auto",
                        "top",
                        "bottom",
                        "left",
                        "right",
                        "top-left",
                        "top-right",
                        "bottom-left",
                        "bottom-right",
                        "center",
                    ],
                )?;
            }
            ids.push(if is_instruction {
                let suffix = match part.get("type").and_then(Value::as_str) {
                    Some("callout") => "callout".to_owned(),
                    Some("bezier-arrow") => "path".to_owned(),
                    _ if part_index == 0 => "focus".to_owned(),
                    _ => format!("focus-{}", part_index + 1),
                };
                format!("{id}:{suffix}")
            } else {
                id.clone()
            });
            annotations.push(part);
        }
    }
    object.insert("annotations".to_owned(), Value::Array(annotations));
    Ok(PreparedInput { scene: root, ids })
}
