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
                if !matches!(mark.as_str(), "rect" | "rounded-rect" | "spotlight" | "circle") {
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
        Commands::Capture {
            output,
            output_flag,
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
            let output_path = output
                .or(output_flag)
                .ok_or("Output destination path (.png) is required")?;

            let captured = if let (Some(x), Some(y), Some(w), Some(h)) = (x, y, width, height) {
                markits::capture::capture_region(x, y, w, h)?
            } else {
                markits::capture::capture_primary_screen()?
            };

            let mut effective_uimap = None;
            if let Some(uimap_path) = uimap {
                effective_uimap = Some(raster::load_uimap_from_path(&uimap_path)?);
            } else if detect_ui {
                let bounds = if let (Some(x), Some(y), Some(w), Some(h)) = (x, y, width, height) {
                    Some((x as f64, y as f64, w as f64, h as f64))
                } else {
                    None
                };
                let detected = markits::ui_elements::capture_desktop_detailed_elements(0, 0, bounds);
                let elements: Vec<markits::UiElement> = if let (Some(x), Some(y), Some(w), Some(h)) = (x, y, width, height) {
                    markits::ui_elements::filter_elements_for_crop(&detected, x as f64, y as f64, w as f64, h as f64)
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
                    if !matches!(mark.as_str(), "rect" | "rounded-rect" | "spotlight" | "circle") {
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
                println!("Screenshot captured, annotated, and saved to {}", output_path.display());
            } else {
                if crop {
                    eprintln!("Warning: --crop was specified, but no --target annotation was provided. Outputting full image.");
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
