use clap::{Parser, Subcommand};
use markits::{Scene, render_debug_from_json, render_from_json, render_with_layout_from_json};
use std::fs;
use std::io::{self, Read};
use std::process::ExitCode;

use markits::raster;

#[derive(Parser, Debug)]
#[command(
    name = "markits",
    about = "Semantic Annotation SVG Engine — renders clean annotation SVGs from intent JSON",
    after_help = "AI/LLM usage guide: run `markits manual` to print the bundled Markdown manual.",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Render annotation JSON into SVG or an annotated PNG
    Render {
        /// Path to JSON file, or '-' to read from standard input
        input: String,
        /// Emit SVG and positioned elements as JSON
        #[arg(long)]
        layout_json: bool,
        /// Overlay layout targets, candidates, scores, and selected positions
        #[arg(long)]
        debug: bool,
        /// Optional path to external UIMap JSON file
        #[arg(long)]
        uimap: Option<std::path::PathBuf>,
        /// PNG or JPEG image to annotate (requires --output)
        #[arg(long, requires = "output")]
        image: Option<std::path::PathBuf>,
        /// Path for the annotated PNG (requires --image)
        #[arg(short, long, requires = "image")]
        output: Option<std::path::PathBuf>,
        /// Crop output image to bounding box of annotations
        #[arg(long, requires = "image")]
        crop: bool,
        /// Margin in pixels around annotation bounding box when cropping
        #[arg(long, default_value = "32")]
        crop_margin: u32,
    },
    /// Validate semantic annotation JSON without rendering
    Validate {
        /// Path to JSON file, or '-' to read from standard input
        input: String,
        /// Image to supply canvas dimensions when JSON omits canvas
        #[arg(long)]
        image: Option<std::path::PathBuf>,
        /// Optional path to external UIMap JSON file
        #[arg(long)]
        uimap: Option<std::path::PathBuf>,
    },
    /// Print PNG or JPEG dimensions and metadata as JSON
    Inspect {
        /// Path to a PNG or JPEG image
        image: std::path::PathBuf,
        /// Include full detected UIMap elements in JSON output
        #[arg(long)]
        uimap: bool,
    },
    /// Present/extract UI elements map (UIMap) from an image
    Uimap {
        /// Path to a PNG image containing embedded UIMap
        image: std::path::PathBuf,
        /// Output as raw JSON (recommended for AI / automated pipelines)
        #[arg(long)]
        json: bool,
        /// Write the JSON UIMap to a file instead of standard output
        #[arg(long, value_name = "PATH")]
        output: Option<std::path::PathBuf>,
        /// Filter by UI element role (e.g. button, textbox, menu)
        #[arg(long)]
        filter: Option<String>,
    },
    /// Quick command to add a mark to a specific UI target without writing JSON
    Annotate {
        /// Base image to annotate
        image: std::path::PathBuf,
        /// Target UI element name (e.g. "保存", "検索") or rectangle [x, y, w, h]
        #[arg(long)]
        target: String,
        /// Annotation mark type: pin, rect, rounded-rect, circle, callout, spotlight, bullseye, arrow
        #[arg(long, default_value = "pin")]
        mark: String,
        /// Label or description text for the mark
        #[arg(long)]
        text: Option<String>,
        /// Numeric step shown by badge/step-arrow marks
        #[arg(long)]
        step: Option<u32>,
        /// Semantic style: primary, secondary, warning, danger, info, step, pink
        #[arg(long, default_value = "primary")]
        style: String,
        /// Position hint: auto, top, bottom, left, right
        #[arg(long, default_value = "auto")]
        position: String,
        /// Optional path to an external UIMap JSON file
        #[arg(long)]
        uimap: Option<std::path::PathBuf>,
        /// Crop output image to bounding box of annotations
        #[arg(long)]
        crop: bool,
        /// Margin in pixels around annotation bounding box when cropping
        #[arg(long, default_value = "32")]
        crop_margin: u32,
        /// Output image path (.png)
        #[arg(short, long)]
        output: std::path::PathBuf,
    },
    /// Apply several marks from a JSON array in one pass
    AnnotateBatch {
        image: std::path::PathBuf,
        marks: std::path::PathBuf,
        #[arg(short, long)]
        output: std::path::PathBuf,
        #[arg(long)]
        uimap: Option<std::path::PathBuf>,
        /// JSON object of default style, position, shadow, outline and stroke_width
        #[arg(long)]
        template: Option<std::path::PathBuf>,
    },
    /// Capture numbered screenshots after an optional delay
    CaptureSeries {
        /// Base PNG path; files are written as name_001.png, name_002.png, ...
        output: std::path::PathBuf,
        #[arg(long, default_value_t = 1)]
        count: u32,
        #[arg(long, default_value_t = 0)]
        delay_ms: u64,
        #[arg(long, default_value_t = 1000)]
        interval_ms: u64,
        #[arg(long)]
        screen: Option<usize>,
    },
    /// Cut a rectangular region from a PNG or JPEG and save the actual pixels
    Crop {
        /// Source PNG or JPEG image
        image: std::path::PathBuf,
        /// Left edge in source-image pixels
        #[arg(long)]
        x: u32,
        /// Top edge in source-image pixels
        #[arg(long)]
        y: u32,
        /// Output width in pixels
        #[arg(long)]
        width: u32,
        /// Output height in pixels
        #[arg(long)]
        height: u32,
        /// Destination PNG file
        #[arg(long)]
        output: std::path::PathBuf,
    },
    /// Capture screenshot of the primary screen or region with optional UI detection and annotation
    Capture {
        /// Destination PNG file
        output: Option<std::path::PathBuf>,
        /// Destination PNG file (alternative to positional argument)
        #[arg(short, long = "output")]
        output_flag: Option<std::path::PathBuf>,
        /// List all connected screens and exit
        #[arg(long)]
        list_screens: bool,
        /// List all capturable top-level windows and exit
        #[arg(long)]
        list_windows: bool,
        /// Screen/Monitor index to capture (0-indexed)
        #[arg(long)]
        screen: Option<usize>,
        /// Target window title substring or ID to capture directly
        #[arg(long)]
        window: Option<String>,
        /// Target process ID (PID) to capture directly
        #[arg(long)]
        pid: Option<u32>,
        /// Output listings in JSON format
        #[arg(long)]
        json: bool,
        /// Detect desktop UI elements and embed UIMap metadata in the output PNG
        #[arg(long)]
        detect_ui: bool,
        /// Reuse UIMap from an existing PNG image (extracted from metadata) or a JSON file
        #[arg(long)]
        uimap: Option<std::path::PathBuf>,
        /// Subregion left coordinate (pixels)
        #[arg(long)]
        x: Option<u32>,
        /// Subregion top coordinate (pixels)
        #[arg(long)]
        y: Option<u32>,
        /// Subregion width (pixels)
        #[arg(long)]
        width: Option<u32>,
        /// Subregion height (pixels)
        #[arg(long)]
        height: Option<u32>,
        /// Target UI element name (e.g. "保存", "保存ボタン") or rectangle [x, y, w, h] to annotate immediately
        #[arg(long)]
        target: Option<String>,
        /// Annotation mark type: rect, rounded-rect, pin, circle, callout, spotlight, bullseye, arrow
        #[arg(long, default_value = "rect")]
        mark: String,
        /// Label or description text for the mark
        #[arg(long)]
        text: Option<String>,
        /// Numeric step shown by badge/step-arrow marks
        #[arg(long)]
        step: Option<u32>,
        /// Semantic style: primary, secondary, warning, danger, info, step, pink
        #[arg(long, default_value = "primary")]
        style: String,
        /// Position hint: auto, top, bottom, left, right
        #[arg(long, default_value = "auto")]
        position: String,
        /// Crop output image to bounding box of annotations
        #[arg(long)]
        crop: bool,
        /// Margin in pixels around annotation bounding box when cropping
        #[arg(long, default_value = "32")]
        crop_margin: u32,
    },
    /// Print the bundled Markdown manual for AI/LLM use
    Manual,
}

fn read_input(input: &str) -> Result<String, Box<dyn std::error::Error>> {
    if input == "-" {
        let mut buffer = String::new();
        io::stdin().read_to_string(&mut buffer)?;
        Ok(buffer)
    } else {
        Ok(fs::read_to_string(input).map_err(|e| format!("Failed to read file '{input}': {e}"))?)
    }
}

fn warn_uimap_if_stale(path: &std::path::Path, width: u32, height: u32) {
    if let Ok(info) = raster::inspect_image(path) {
        if info.width != width || info.height != height {
            eprintln!("Warning: UIMap source is {}x{}, but target image is {}x{}; targets may be misplaced.",
                info.width, info.height, width, height);
        }
    }
}

fn series_path(base: &std::path::Path, index: u32) -> std::path::PathBuf {
    let stem = base.file_stem().and_then(|v| v.to_str()).unwrap_or("capture");
    let name = format!("{stem}_{index:03}.png");
    base.with_file_name(name)
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Render {
            input,
            layout_json,
            debug,
            uimap,
            image,
            output,
            crop,
            crop_margin,
        } => {
            let mut json_content = read_input(&input)?;
            if let Some(uimap_path) = uimap {
                if let Some(ref image_path) = image {
                    let info = raster::inspect_image(image_path)?;
                    warn_uimap_if_stale(&uimap_path, info.width, info.height);
                }
                let elements = raster::load_uimap_from_path(&uimap_path)?;
                json_content =
                    raster::with_image_canvas_and_uimap(&json_content, 0, 0, Some(&elements))?;
            }
            if let (Some(image), Some(output)) = (image, output) {
                if layout_json || debug {
                    return Err("--image cannot be combined with --layout-json or --debug".into());
                }
                let (image_bytes, _info) = raster::read_image(&image)?;
                let png_bytes = raster::render_composed_png_bytes_with_crop(
                    &json_content,
                    &image_bytes,
                    if crop { Some(crop_margin) } else { None },
                )?;
                fs::write(output, png_bytes)?;
            } else if layout_json {
                let mut result = render_with_layout_from_json(&json_content)?;
                if debug {
                    result.svg = render_debug_from_json(&json_content)?;
                }
                println!("{}", serde_json::to_string_pretty(&result)?);
            } else if debug {
                print!("{}", render_debug_from_json(&json_content)?);
            } else {
                print!("{}", render_from_json(&json_content)?);
            }
        }
        Commands::Validate {
            input,
            image,
            uimap,
        } => {
            let json = read_input(&input)?;
            let mut external_uimap = None;
            if let Some(uimap_path) = uimap {
                let elements = raster::load_uimap_from_path(&uimap_path)?;
                external_uimap = Some(elements);
            }
            if let Some(path) = image {
                let info = raster::inspect_image(&path)?;
                let effective_uimap = external_uimap.as_deref().or(info.uimap.as_deref());
                let resolved = raster::with_image_canvas_and_uimap(
                    &json,
                    info.width,
                    info.height,
                    effective_uimap,
                )?;
                let scene = Scene::from_json(&resolved)?;
                raster::ensure_canvas_matches(&scene, &info)?;
            } else {
                let resolved = if let Some(elements) = external_uimap {
                    raster::with_image_canvas_and_uimap(&json, 0, 0, Some(&elements))?
                } else {
                    json
                };
                Scene::from_json(&resolved)?;
            }
            println!("Valid annotation document");
        }
        Commands::Inspect { image, uimap } => {
            let info = raster::inspect_image(&image)?;
            let mut obj = serde_json::json!({
                "width": info.width,
                "height": info.height,
                "format": info.format,
                "has_uimap": info.uimap.is_some(),
                "uimap_elements_count": info.uimap.as_ref().map(|v| v.len()).unwrap_or(0),
            });
            if uimap {
                obj["uimap"] = serde_json::to_value(info.uimap.unwrap_or_default())?;
            }
            println!("{}", serde_json::to_string_pretty(&obj)?);
        }
        Commands::Uimap {
            image,
            json,
            output,
            filter,
        } => {
            let info = raster::inspect_image(&image)?;
            let elements = info.uimap.unwrap_or_default();
            let filtered: Vec<_> = elements
                .iter()
                .enumerate()
                .filter(|(_, el)| {
                    if let Some(ref f) = filter {
                        el.role.eq_ignore_ascii_case(f)
                    } else {
                        true
                    }
                })
                .map(|(_, el)| el.clone())
                .collect();

            if let Some(path) = output {
                fs::write(&path, serde_json::to_string_pretty(&filtered)?)?;
                println!(
                    "UIMap written to {} ({} elements)",
                    path.display(),
                    filtered.len()
                );
            } else if json {
                println!("{}", serde_json::to_string_pretty(&filtered)?);
            } else if filtered.is_empty() {
                println!("No UI elements detected in {}", image.display());
            } else {
                println!(
                    "UI Map in {} ({} elements detected):",
                    image.display(),
                    filtered.len()
                );
                for (i, el) in elements.iter().enumerate().filter(|(_, el)| {
                    filter
                        .as_ref()
                        .is_none_or(|f| el.role.eq_ignore_ascii_case(f))
                }) {
                    let name_str = if el.name.is_empty() {
                        "(unnamed)"
                    } else {
                        &el.name
                    };
                    println!(
                        "  #{:<2} [{:<8}] \"{}\" at [x: {}, y: {}, w: {}, h: {}]",
                        i + 1,
                        el.role,
                        name_str,
                        el.x as i64,
                        el.y as i64,
                        el.width as i64,
                        el.height as i64
                    );
                }
            }
        }
        Commands::Annotate {
            image,
            target,
            mark,
            text,
            step,
            style,
            position,
            uimap,
            crop,
            crop_margin,
            output,
        } => {
            let info = raster::inspect_image(&image)?;
            let mut external_uimap = None;
            if let Some(uimap_path) = uimap {
                warn_uimap_if_stale(&uimap_path, info.width, info.height);
                let parsed = raster::load_uimap_from_path(&uimap_path)?;
                external_uimap = Some(parsed);
            }
            let effective_uimap = external_uimap.as_deref().or(info.uimap.as_deref());

            let target_val: serde_json::Value = if target.starts_with('[') {
                serde_json::from_str(&target)
                    .map_err(|e| format!("Invalid target coordinate array: {e}"))?
            } else {
                serde_json::Value::String(target)
            };

            let mut anno_obj = serde_json::Map::new();
            anno_obj.insert("type".to_string(), serde_json::Value::String(mark.clone()));
            anno_obj.insert("target".to_string(), target_val);
            anno_obj.insert("style".to_string(), serde_json::Value::String(style));
            if matches!(
                mark.as_str(),
                "arrow" | "label" | "callout" | "badge" | "pin" | "step-arrow" | "bezier-arrow"
            ) {
                anno_obj.insert("position".to_string(), serde_json::Value::String(position));
            }
            if let Some(t) = text {
                if !matches!(
                    mark.as_str(),
                    "rect" | "rounded-rect" | "spotlight" | "circle"
                ) {
                    anno_obj.insert("text".to_string(), serde_json::Value::String(t));
                }
            }
            if let Some(value) = step {
                if matches!(mark.as_str(), "badge" | "step-arrow") {
                    anno_obj.insert("step".to_string(), serde_json::json!(value));
                }
            }

            let mut scene_obj = serde_json::Map::new();
            scene_obj.insert(
                "canvas".to_string(),
                serde_json::json!({"width": info.width, "height": info.height}),
            );
            if let Some(elements) = effective_uimap {
                scene_obj.insert("uimap".to_string(), serde_json::to_value(elements)?);
            }
            scene_obj.insert(
                "annotations".to_string(),
                serde_json::Value::Array(vec![serde_json::Value::Object(anno_obj)]),
            );

            let scene_json = serde_json::to_string(&serde_json::Value::Object(scene_obj))?;
            let (image_bytes, _info) = raster::read_image(&image)?;
            let png_bytes = raster::render_composed_png_bytes_with_crop(
                &scene_json,
                &image_bytes,
                if crop { Some(crop_margin) } else { None },
            )?;
            fs::write(&output, png_bytes)?;
            println!("Successfully annotated and saved to {}", output.display());
        }
        Commands::AnnotateBatch { image, marks, output, uimap, template } => {
            let info = raster::inspect_image(&image)?;
            let marks_value: serde_json::Value = serde_json::from_str(&fs::read_to_string(&marks)?)?;
            let mut annotations = if let Some(items) = marks_value.as_array() {
                items.clone()
            } else {
                marks_value.get("annotations").and_then(|v| v.as_array())
                    .ok_or("Marks file must be a JSON array or contain annotations")?.clone()
            };
            if annotations.is_empty() { return Err("Marks file contains no annotations".into()); }
            if let Some(path) = template {
                let defaults: serde_json::Value = serde_json::from_str(&fs::read_to_string(path)?)?;
                let defaults = defaults.as_object().ok_or("Template must be a JSON object")?;
                for mark in &mut annotations {
                    let object = mark.as_object_mut().ok_or("Every mark must be a JSON object")?;
                    let mark_type = object.get("type").and_then(|value| value.as_str()).unwrap_or("").to_string();
                    for key in ["style", "position", "shadow", "outline", "stroke_width", "line_style", "arrowhead", "arrow_skin"] {
                        let supported = match key {
                            "position" => matches!(mark_type.as_str(), "arrow" | "bezier-arrow" | "callout" | "pin" | "label" | "badge" | "step-arrow"),
                            "shadow" => !matches!(mark_type.as_str(), "spotlight" | "divider"),
                            "outline" => matches!(mark_type.as_str(), "arrow" | "bezier-arrow" | "callout" | "pin" | "label"),
                            "stroke_width" => matches!(mark_type.as_str(), "arrow" | "bezier-arrow" | "rect" | "rounded-rect" | "circle" | "step-arrow"),
                            "line_style" | "arrowhead" | "arrow_skin" => matches!(mark_type.as_str(), "arrow" | "bezier-arrow"),
                            _ => true,
                        };
                        if !supported { continue; }
                        if !object.contains_key(key) {
                            if let Some(value) = defaults.get(key) { object.insert(key.to_string(), value.clone()); }
                        }
                    }
                }
            }
            let effective_uimap = if let Some(path) = uimap {
                warn_uimap_if_stale(&path, info.width, info.height);
                Some(raster::load_uimap_from_path(&path)?)
            } else { info.uimap };
            let mut scene = serde_json::json!({
                "canvas": {"width": info.width, "height": info.height},
                "annotations": annotations
            });
            if let Some(elements) = effective_uimap { scene["uimap"] = serde_json::to_value(elements)?; }
            let source = fs::read(&image)?;
            let png = raster::render_composed_png_bytes(&serde_json::to_string(&scene)?, &source)?;
            fs::write(output, png)?;
        }
        Commands::CaptureSeries { output, count, delay_ms, interval_ms, screen } => {
            if count == 0 || count > 1000 { return Err("--count must be between 1 and 1000".into()); }
            if delay_ms > 3_600_000 || interval_ms > 3_600_000 { return Err("Delay and interval must be at most one hour".into()); }
            std::thread::sleep(std::time::Duration::from_millis(delay_ms));
            for index in 1..=count {
                let captured = if let Some(selected) = screen {
                    markits::capture::capture_screen(selected)?
                } else {
                    markits::capture::capture_primary_screen()?
                };
                let path = series_path(&output, index);
                fs::write(&path, captured.raw_png)?;
                println!("{}", path.display());
                if index < count { std::thread::sleep(std::time::Duration::from_millis(interval_ms)); }
            }
        }
        Commands::Capture {
            output,
            output_flag,
            list_screens,
            list_windows,
            screen,
            window,
            pid,
            json,
            detect_ui,
            uimap,
            x,
            y,
            width,
            height,
            target,
            mark,
            text,
            step,
            style,
            position,
            crop,
            crop_margin,
        } => {
            if list_screens {
                let screens = markits::capture::list_screens()?;
                if json {
                    println!("{}", serde_json::to_string_pretty(&screens)?);
                } else {
                    println!(
                        "{:<7} {:<12} {:<12} {:<14} {:<8}",
                        "Index", "Resolution", "Offset", "Scale Factor", "Primary"
                    );
                    println!("{}", "-".repeat(58));
                    for s in screens {
                        println!(
                            "{:<7} {:<12} {:<12} {:<14.2} {:<8}",
                            s.index,
                            format!("{}x{}", s.width, s.height),
                            format!("{},{}", s.x, s.y),
                            s.scale_factor,
                            if s.is_primary { "yes" } else { "no" }
                        );
                    }
                }
                return Ok(());
            }

            if list_windows {
                let windows = markits::capture::list_windows()?;
                if json {
                    println!("{}", serde_json::to_string_pretty(&windows)?);
                } else {
                    println!(
                        "{:<10} {:<8} {:<16} {:<16} {:<25}",
                        "Window ID", "PID", "App Name", "Bounds", "Title"
                    );
                    println!("{}", "-".repeat(82));
                    for w in windows {
                        let pid_str = w
                            .pid
                            .map(|p| p.to_string())
                            .unwrap_or_else(|| "-".to_string());
                        let bounds_str = format!("{},{} {}x{}", w.x, w.y, w.width, w.height);
                        let truncated_title = if w.title.chars().count() > 25 {
                            format!("{}...", w.title.chars().take(22).collect::<String>())
                        } else {
                            w.title.clone()
                        };
                        println!(
                            "{:<10} {:<8} {:<16} {:<16} {:<25}",
                            w.id, pid_str, w.app_name, bounds_str, truncated_title
                        );
                    }
                }
                return Ok(());
            }

            let output_path = output
                .or(output_flag)
                .ok_or("Output destination path (.png) is required")?;

            let region_args = [x.is_some(), y.is_some(), width.is_some(), height.is_some()];
            if region_args.iter().any(|present| *present)
                && !region_args.iter().all(|present| *present)
            {
                return Err("--x, --y, --width and --height must be provided together".into());
            }
            if (window.is_some() as u8 + pid.is_some() as u8 + region_args[0] as u8) > 1 {
                return Err("--window, --pid and region coordinates cannot be combined".into());
            }
            if screen.is_some() && (window.is_some() || pid.is_some()) {
                return Err("--screen cannot be combined with --window or --pid".into());
            }
            let is_window_capture = window.is_some() || pid.is_some();
            let window_for_uimap = if detect_ui && uimap.is_none() && is_window_capture {
                let query = if let Some(ref title_or_id) = window {
                    markits::capture::WindowQuery::TitleOrId(title_or_id.clone())
                } else {
                    markits::capture::WindowQuery::Pid(pid.expect("window capture has a query"))
                };
                markits::capture::list_windows()?
                    .into_iter()
                    .find(|item| item.matches_query(&query))
            } else {
                None
            };

            let captured = if let Some(query_str) = window {
                markits::capture::capture_window_by_query(
                    &markits::capture::WindowQuery::TitleOrId(query_str),
                )?
            } else if let Some(target_pid) = pid {
                markits::capture::capture_window_by_query(&markits::capture::WindowQuery::Pid(
                    target_pid,
                ))?
            } else if let (Some(x), Some(y), Some(w), Some(h)) = (x, y, width, height) {
                markits::capture::capture_region_on_screen(screen, x, y, w, h)?
            } else if let Some(screen_idx) = screen {
                markits::capture::capture_screen(screen_idx)?
            } else {
                markits::capture::capture_primary_screen()?
            };

            let screen_info = if !is_window_capture {
                let screens = markits::capture::list_screens()?;
                let selected = screen
                    .unwrap_or_else(|| screens.iter().position(|s| s.is_primary).unwrap_or(0));
                screens.get(selected).cloned()
            } else {
                None
            };

            let mut effective_uimap = None;
            if let Some(uimap_path) = uimap {
                warn_uimap_if_stale(&uimap_path, captured.width, captured.height);
                effective_uimap = Some(raster::load_uimap_from_path(&uimap_path)?);
            } else if detect_ui {
                let bounds = if let Some(ref selected_window) = window_for_uimap {
                    Some((
                        selected_window.x as f64,
                        selected_window.y as f64,
                        selected_window.width as f64,
                        selected_window.height as f64,
                    ))
                } else if let (Some(x), Some(y), Some(w), Some(h)) = (x, y, width, height) {
                    Some((x as f64, y as f64, w as f64, h as f64))
                } else {
                    None
                };
                let origin_x = screen_info.as_ref().map_or(0, |s| s.x);
                let origin_y = screen_info.as_ref().map_or(0, |s| s.y);
                let detected = markits::ui_elements::capture_desktop_detailed_elements(
                    origin_x, origin_y, bounds,
                );
                let elements: Vec<markits::UiElement> =
                    if let Some(ref selected_window) = window_for_uimap {
                        let mut cropped = markits::ui_elements::filter_elements_for_crop(
                            &detected,
                            selected_window.x as f64,
                            selected_window.y as f64,
                            selected_window.width as f64,
                            selected_window.height as f64,
                        );
                        let sx = captured.width as f64 / selected_window.width.max(1) as f64;
                        let sy = captured.height as f64 / selected_window.height.max(1) as f64;
                        for element in &mut cropped {
                            element.x *= sx;
                            element.y *= sy;
                            element.width *= sx;
                            element.height *= sy;
                        }
                        cropped.into_iter().map(Into::into).collect()
                    } else if let (Some(x), Some(y), Some(w), Some(h)) = (x, y, width, height) {
                        markits::ui_elements::filter_elements_for_crop(
                            &detected, x as f64, y as f64, w as f64, h as f64,
                        )
                        .into_iter()
                        .map(Into::into)
                        .collect()
                    } else {
                        detected.into_iter().map(Into::into).collect()
                    };
                effective_uimap = Some(elements);
            }

            // Embed UIMap in PNG bytes if we have one
            let png_bytes = if let Some(ref elements) = effective_uimap {
                raster::embed_png_uimap(&captured.raw_png, elements)?
            } else {
                captured.raw_png
            };

            if let Some(target_str) = target {
                let target_val: serde_json::Value = if target_str.starts_with('[') {
                    serde_json::from_str(&target_str)
                        .map_err(|e| format!("Invalid target coordinate array: {e}"))?
                } else {
                    serde_json::Value::String(target_str)
                };

                let mut anno_obj = serde_json::Map::new();
                anno_obj.insert("type".to_string(), serde_json::Value::String(mark.clone()));
                anno_obj.insert("target".to_string(), target_val);
                anno_obj.insert("style".to_string(), serde_json::Value::String(style));
                if matches!(
                    mark.as_str(),
                    "arrow" | "label" | "callout" | "badge" | "pin" | "step-arrow" | "bezier-arrow"
                ) {
                    anno_obj.insert("position".to_string(), serde_json::Value::String(position));
                }
                if let Some(t) = text {
                    if !matches!(
                        mark.as_str(),
                        "rect" | "rounded-rect" | "spotlight" | "circle"
                    ) {
                        anno_obj.insert("text".to_string(), serde_json::Value::String(t));
                    }
                }
                if let Some(value) = step {
                    if matches!(mark.as_str(), "badge" | "step-arrow") {
                        anno_obj.insert("step".to_string(), serde_json::json!(value));
                    }
                }

                let mut scene_obj = serde_json::Map::new();
                scene_obj.insert(
                    "canvas".to_string(),
                    serde_json::json!({"width": captured.width, "height": captured.height}),
                );
                if let Some(elements) = effective_uimap {
                    scene_obj.insert("uimap".to_string(), serde_json::to_value(elements)?);
                }
                scene_obj.insert(
                    "annotations".to_string(),
                    serde_json::Value::Array(vec![serde_json::Value::Object(anno_obj)]),
                );

                let scene_json = serde_json::to_string(&serde_json::Value::Object(scene_obj))?;
                let rendered_bytes = raster::render_composed_png_bytes_with_crop(
                    &scene_json,
                    &png_bytes,
                    if crop { Some(crop_margin) } else { None },
                )?;
                fs::write(&output_path, rendered_bytes)?;
                println!(
                    "Screenshot captured, annotated, and saved to {}",
                    output_path.display()
                );
            } else {
                if crop {
                    eprintln!(
                        "Warning: --crop was specified, but no --target annotation was provided. Outputting full image."
                    );
                }
                fs::write(&output_path, png_bytes)?;
                if let Some(ref elements) = effective_uimap {
                    println!(
                        "Screenshot captured with {} UI elements to {}",
                        elements.len(),
                        output_path.display()
                    );
                } else {
                    println!("Screenshot captured and saved to {}", output_path.display());
                }
            }
        }
        Commands::Crop {
            image,
            x,
            y,
            width,
            height,
            output,
        } => raster::crop_file(&image, &output, x, y, width, height)?,
        Commands::Manual => print!("{}", include_str!("../docs/AI_MANUAL.md")),
    }

    Ok(())
}

fn main() -> ExitCode {
    if let Err(err) = run() {
        eprintln!("Error: {}", err);
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}
