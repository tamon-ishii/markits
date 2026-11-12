use clap::{Parser, Subcommand};
use markits::{Scene, render_debug_from_json, render_from_json, render_with_layout_from_json};
use std::fs;
use std::io::{self, Read};
use std::process::ExitCode;

use markits::{cli_support, raster};
mod cli_automation;
mod cli_commands;

#[derive(Parser, Debug)]
#[command(
    name = "markits",
    about = "Capture screenshots, annotate UI targets, export editable images, and replay window operations",
    after_help = "AI/LLM usage guide: run `markits manual` to print the bundled Markdown manual.",
    version
)]
struct Cli {
    /// Preserve editable source, annotations, crop and operation metadata in image output
    #[arg(long, global = true)]
    editing_metadata: bool,
    /// UI exploration depth (1..128)
    #[arg(long, global = true, default_value_t = 64, value_parser = clap::value_parser!(u32).range(1..=128))]
    ui_depth: u32,
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Extract a capture recipe as JSON without executing its script or operations
    Recipe(cli_automation::RecipeArgs),
    /// Launch an app/script, operate its window, capture and embed a reproducible recipe
    CaptureRun(cli_automation::CaptureRunArgs),
    /// Launch and recapture using the recipe embedded in an image (executes saved script/input)
    Recapture(cli_automation::RecaptureArgs),
    /// Convert/export PNG, JPEG, WebP or JPEG XL; flattened unless --editing-metadata
    Export {
        image: std::path::PathBuf,
        #[arg(short, long)]
        output: std::path::PathBuf,
    },
    /// List, load, save and delete desktop capture history
    History {
        #[command(subcommand)]
        command: cli_commands::HistoryCommand,
    },
    /// Record window input; control an active session from another CLI invocation
    Record {
        #[command(subcommand)]
        command: cli_commands::RecordCommand,
    },
    /// Replay recorded operations and capture screenshots (actually operates the target window)
    Replay(cli_commands::ReplayArgs),
    #[command(hide = true)]
    RecordWorker { session: std::path::PathBuf },
    /// Remove MarkIts edit metadata from PNG, WebP, or JPEG XL without changing pixels
    StripMetadata {
        /// Image containing MarkIts metadata
        image: std::path::PathBuf,
        /// Destination image (same format, different file)
        #[arg(short, long)]
        output: std::path::PathBuf,
    },
    /// Render annotation JSON into SVG or an annotated PNG, WebP, or JPEG XL
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
        /// PNG, JPEG, WebP, or JPEG XL image to annotate (requires --output)
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
    /// Print image dimensions and metadata as JSON
    Inspect {
        /// Path to a PNG, JPEG, WebP, or JPEG XL image
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
        /// Mark: pin, rect, rounded-rect, circle, callout, spotlight, bullseye, arrow, bezier-arrow, step-arrow, badge, label, mosaic, divider
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
        /// Output image path (.png, .webp, .jxl)
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
    /// Cut a rectangular region from an image and save the actual pixels
    Crop {
        /// Source PNG, JPEG, WebP, or JPEG XL image
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
        /// Destination image (.png, .webp, .jxl)
        #[arg(long)]
        output: std::path::PathBuf,
    },
    /// Capture screenshot of the primary screen or region with optional UI detection and annotation
    Capture {
        /// Include the current mouse cursor (Windows only)
        #[arg(long)]
        include_cursor: bool,
        /// Destination image (.png, .webp, .jxl)
        output: Option<std::path::PathBuf>,
        /// Destination image (.png, .webp, .jxl) (alternative to positional argument)
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
            eprintln!(
                "Warning: UIMap source is {}x{}, but target image is {}x{}; targets may be misplaced.",
                info.width, info.height, width, height
            );
        }
    }
}

fn series_path(base: &std::path::Path, index: u32) -> std::path::PathBuf {
    let stem = base
        .file_stem()
        .and_then(|v| v.to_str())
        .unwrap_or("capture");
    let name = format!("{stem}_{index:03}.png");
    base.with_file_name(name)
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::args().nth(1).as_deref() == Some("--manual-studio-record-input") {
        let path = std::env::args_os()
            .nth(2)
            .ok_or("Missing input recorder event path")?;
        return markits::recording::run_helper(std::path::Path::new(&path)).map_err(Into::into);
    }
    let cli = Cli::parse();
    let _depth = markits::ui_elements::capture_depth_scope(cli.ui_depth as usize);
    let editing_metadata = cli.editing_metadata;

    match cli.command {
        Commands::Recipe(args) => cli_automation::recipe(args)?,
        Commands::CaptureRun(args) => cli_automation::capture_run(args)?,
        Commands::Recapture(args) => cli_automation::recapture(args)?,
        Commands::Export { image, output } => {
            cli_commands::export(&image, &output, editing_metadata)?
        }
        Commands::History { command } => cli_commands::history(command)?,
        Commands::Record { command } => cli_commands::record(command)?,
        Commands::Replay(args) => cli_commands::replay(args)?,
        Commands::RecordWorker { session } => cli_commands::record_worker(&session)?,
        Commands::StripMetadata { image, output } => {
            if image
                .canonicalize()
                .ok()
                .zip(output.canonicalize().ok())
                .is_some_and(|(a, b)| a == b)
            {
                return Err("Choose an output path different from the original image".into());
            }
            let source = fs::read(&image)?;
            let stripped = markits::image_metadata::strip_markits_metadata(&source)?;
            fs::write(&output, stripped)?;
            println!("Saved image without MarkIts metadata: {}", output.display());
        }
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
                let png_bytes = cli_support::render(
                    &json_content,
                    &image_bytes,
                    if crop { Some(crop_margin) } else { None },
                    editing_metadata,
                )?;
                cli_support::write_image(&output, &png_bytes)?;
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
            let (bytes, info) = raster::read_image(&image)?;
            let mut obj = serde_json::json!({
                "width": info.width,
                "height": info.height,
                "format": info.format,
                "has_uimap": info.uimap.is_some(),
                "has_capture_recipe": raster::extract_png_text_chunk(&bytes, markits::recording_metadata::RECIPE_KEY).is_some(),
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
            let png_bytes = cli_support::render(
                &scene_json,
                &image_bytes,
                if crop { Some(crop_margin) } else { None },
                editing_metadata,
            )?;
            cli_support::write_image(&output, &png_bytes)?;
            println!("Successfully annotated and saved to {}", output.display());
        }
        Commands::AnnotateBatch {
            image,
            marks,
            output,
            uimap,
            template,
        } => {
            let info = raster::inspect_image(&image)?;
            let marks_value: serde_json::Value =
                serde_json::from_str(&fs::read_to_string(&marks)?)?;
            let mut annotations = if let Some(items) = marks_value.as_array() {
                items.clone()
            } else {
                marks_value
                    .get("annotations")
                    .and_then(|v| v.as_array())
                    .ok_or("Marks file must be a JSON array or contain annotations")?
                    .clone()
            };
            if annotations.is_empty() {
                return Err("Marks file contains no annotations".into());
            }
            if let Some(path) = template {
                let defaults: serde_json::Value = serde_json::from_str(&fs::read_to_string(path)?)?;
                let defaults = defaults
                    .as_object()
                    .ok_or("Template must be a JSON object")?;
                for mark in &mut annotations {
                    let object = mark
                        .as_object_mut()
                        .ok_or("Every mark must be a JSON object")?;
                    let mark_type = object
                        .get("type")
                        .and_then(|value| value.as_str())
                        .unwrap_or("")
                        .to_string();
                    for key in [
                        "style",
                        "position",
                        "shadow",
                        "outline",
                        "stroke_width",
                        "line_style",
                        "arrowhead",
                        "arrow_skin",
                    ] {
                        let supported = match key {
                            "position" => matches!(
                                mark_type.as_str(),
                                "arrow"
                                    | "bezier-arrow"
                                    | "callout"
                                    | "pin"
                                    | "label"
                                    | "badge"
                                    | "step-arrow"
                            ),
                            "shadow" => !matches!(mark_type.as_str(), "spotlight" | "divider"),
                            "outline" => matches!(
                                mark_type.as_str(),
                                "arrow" | "bezier-arrow" | "callout" | "pin" | "label"
                            ),
                            "stroke_width" => matches!(
                                mark_type.as_str(),
                                "arrow"
                                    | "bezier-arrow"
                                    | "rect"
                                    | "rounded-rect"
                                    | "circle"
                                    | "step-arrow"
                            ),
                            "line_style" | "arrowhead" | "arrow_skin" => {
                                matches!(mark_type.as_str(), "arrow" | "bezier-arrow")
                            }
                            _ => true,
                        };
                        if !supported {
                            continue;
                        }
                        if !object.contains_key(key) {
                            if let Some(value) = defaults.get(key) {
                                object.insert(key.to_string(), value.clone());
                            }
                        }
                    }
                }
            }
            let effective_uimap = if let Some(path) = uimap {
                warn_uimap_if_stale(&path, info.width, info.height);
                Some(raster::load_uimap_from_path(&path)?)
            } else {
                info.uimap
            };
            let mut scene = serde_json::json!({
                "canvas": {"width": info.width, "height": info.height},
                "annotations": annotations
            });
            if let Some(elements) = effective_uimap {
                scene["uimap"] = serde_json::to_value(elements)?;
            }
            let (source, _) = raster::read_image(&image)?;
            let png = cli_support::render(
                &serde_json::to_string(&scene)?,
                &source,
                None,
                editing_metadata,
            )?;
            cli_support::write_image(&output, &png)?;
        }
        Commands::CaptureSeries {
            output,
            count,
            delay_ms,
            interval_ms,
            screen,
        } => {
            if count == 0 || count > 1000 {
                return Err("--count must be between 1 and 1000".into());
            }
            if delay_ms > 3_600_000 || interval_ms > 3_600_000 {
                return Err("Delay and interval must be at most one hour".into());
            }
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
                if index < count {
                    std::thread::sleep(std::time::Duration::from_millis(interval_ms));
                }
            }
        }
        Commands::Capture {
            include_cursor,
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
                let windows = markits::window_capture::list_windows()?;
                let listing:Vec<_>=windows.iter().map(|w| {
                    let pid=markits::semantic_target::window_context(&w.id).ok().and_then(|c|c.pid);
                    serde_json::json!({"id":w.id,"pid":pid,"title":w.title,"x":w.x,"y":w.y,"width":w.width,"height":w.height})
                }).collect();
                if json {
                    println!("{}", serde_json::to_string_pretty(&listing)?);
                } else {
                    println!("{:<18} {:<10} {}", "Window ID", "PID", "Title");
                    for item in listing {
                        println!(
                            "{:<18} {:<10} {}",
                            item["id"].as_str().unwrap_or(""),
                            item["pid"],
                            item["title"].as_str().unwrap_or("")
                        );
                    }
                }
                return Ok(());
            }

            let output_path = output
                .or(output_flag)
                .ok_or("Output destination path (.png, .webp, .jxl) is required")?;

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
            if include_cursor && !cfg!(target_os = "windows") {
                return Err("--include-cursor is supported only on Windows".into());
            }
            let is_window_capture = window.is_some() || pid.is_some();
            let native_window = if is_window_capture {
                Some(cli_support::native_window(window.as_deref(), pid)?)
            } else {
                None
            };
            let captured = if let Some(ref native) = native_window {
                cli_commands::capture_native(native, detect_ui && uimap.is_none(), include_cursor)?
            } else {
                let captured = if let (Some(x), Some(y), Some(w), Some(h)) = (x, y, width, height) {
                    markits::capture::capture_region_on_screen(screen, x, y, w, h)?
                } else if let Some(index) = screen {
                    markits::capture::capture_screen(index)?
                } else {
                    markits::capture::capture_primary_screen()?
                };
                if include_cursor {
                    cli_commands::capture_screen_cursor(captured, screen, x, y)?
                } else {
                    captured
                }
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
            } else if detect_ui && native_window.is_some() {
                effective_uimap = Some(captured.to_ui_elements());
            } else if detect_ui {
                let bounds = if let (Some(x), Some(y), Some(w), Some(h)) = (x, y, width, height) {
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
                    if let (Some(x), Some(y), Some(w), Some(h)) = (x, y, width, height) {
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
                let rendered_bytes = cli_support::render(
                    &scene_json,
                    &png_bytes,
                    if crop { Some(crop_margin) } else { None },
                    editing_metadata,
                )?;
                cli_support::write_image(&output_path, &rendered_bytes)?;
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
                cli_support::write_image(&output_path, &png_bytes)?;
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
        } => cli_commands::crop(&image, &output, x, y, width, height, editing_metadata)?,
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
