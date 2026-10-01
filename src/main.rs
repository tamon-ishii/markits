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
        #[arg(long, requires = "image")]
        output: Option<std::path::PathBuf>,
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
        /// Semantic style: primary, secondary, warning, danger, info, step, pink
        #[arg(long, default_value = "primary")]
        style: String,
        /// Position hint: auto, top, bottom, left, right
        #[arg(long, default_value = "auto")]
        position: String,
        /// Optional path to an external UIMap JSON file
        #[arg(long)]
        uimap: Option<std::path::PathBuf>,
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
        } => {
            let mut json_content = read_input(&input)?;
            if let Some(uimap_path) = uimap {
                let uimap_str = fs::read_to_string(uimap_path)?;
                let elements: Vec<markits::UiElement> = serde_json::from_str(&uimap_str)?;
                json_content = raster::with_image_canvas_and_uimap(&json_content, 0, 0, Some(&elements))?;
            }
            if let (Some(image), Some(output)) = (image, output) {
                if layout_json || debug {
                    return Err("--image cannot be combined with --layout-json or --debug".into());
                }
                raster::render_png(&json_content, &image, &output)?;
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
        Commands::Validate { input, image, uimap } => {
            let json = read_input(&input)?;
            let mut external_uimap = None;
            if let Some(uimap_path) = uimap {
                let uimap_str = fs::read_to_string(uimap_path)?;
                let elements: Vec<markits::UiElement> = serde_json::from_str(&uimap_str)?;
                external_uimap = Some(elements);
            }
            if let Some(path) = image {
                let info = raster::inspect_image(&path)?;
                let effective_uimap = external_uimap.as_deref().or(info.uimap.as_deref());
                let resolved = raster::with_image_canvas_and_uimap(&json, info.width, info.height, effective_uimap)?;
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
        Commands::Uimap { image, json, filter } => {
            let info = raster::inspect_image(&image)?;
            let elements = info.uimap.unwrap_or_default();
            let filtered: Vec<_> = elements
                .into_iter()
                .filter(|el| {
                    if let Some(ref f) = filter {
                        el.role.eq_ignore_ascii_case(f)
                    } else {
                        true
                    }
                })
                .collect();

            if json {
                println!("{}", serde_json::to_string_pretty(&filtered)?);
            } else if filtered.is_empty() {
                println!("No UI elements detected in {}", image.display());
            } else {
                println!("UI Map in {} ({} elements detected):", image.display(), filtered.len());
                for (i, el) in filtered.iter().enumerate() {
                    let name_str = if el.name.is_empty() { "(unnamed)" } else { &el.name };
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
            style,
            position,
            uimap,
            output,
        } => {
            let info = raster::inspect_image(&image)?;
            let mut external_uimap = None;
            if let Some(uimap_path) = uimap {
                let uimap_content = fs::read_to_string(&uimap_path)?;
                let parsed: Vec<markits::UiElement> = serde_json::from_str(&uimap_content)?;
                external_uimap = Some(parsed);
            }
            let effective_uimap = external_uimap.as_deref().or(info.uimap.as_deref());

            let target_val: serde_json::Value = if target.starts_with('[') {
                serde_json::from_str(&target).map_err(|e| format!("Invalid target coordinate array: {e}"))?
            } else {
                serde_json::Value::String(target)
            };

            let mut anno_obj = serde_json::Map::new();
            anno_obj.insert("type".to_string(), serde_json::Value::String(mark));
            anno_obj.insert("target".to_string(), target_val);
            anno_obj.insert("style".to_string(), serde_json::Value::String(style));
            anno_obj.insert("position".to_string(), serde_json::Value::String(position));
            if let Some(t) = text {
                anno_obj.insert("text".to_string(), serde_json::Value::String(t));
            }

            let mut scene_obj = serde_json::Map::new();
            scene_obj.insert("canvas".to_string(), serde_json::json!({"width": info.width, "height": info.height}));
            if let Some(elements) = effective_uimap {
                scene_obj.insert("uimap".to_string(), serde_json::to_value(elements)?);
            }
            scene_obj.insert(
                "annotations".to_string(),
                serde_json::Value::Array(vec![serde_json::Value::Object(anno_obj)]),
            );

            let scene_json = serde_json::to_string(&serde_json::Value::Object(scene_obj))?;
            raster::render_png(&scene_json, &image, &output)?;
            println!("Successfully annotated and saved to {}", output.display());
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
