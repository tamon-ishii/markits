use clap::{Parser, Subcommand};
use markits::{Scene, render_debug_from_json, render_from_json, render_with_layout_from_json};
use std::fs;
use std::io::{self, Read};
use std::process::ExitCode;

mod raster;

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
    },
    /// Print PNG or JPEG dimensions as JSON
    Inspect {
        /// Path to a PNG or JPEG image
        image: std::path::PathBuf,
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
            image,
            output,
        } => {
            let json_content = read_input(&input)?;
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
        Commands::Validate { input, image } => {
            let json = read_input(&input)?;
            if let Some(path) = image {
                let info = raster::inspect_image(&path)?;
                let resolved = raster::with_image_canvas(&json, info.width, info.height)?;
                let scene = Scene::from_json(&resolved)?;
                raster::ensure_canvas_matches(&scene, &info)?;
            } else {
                Scene::from_json(&json)?;
            }
            println!("Valid annotation document");
        }
        Commands::Inspect { image } => {
            let info = raster::inspect_image(&image)?;
            println!(
                "{}",
                serde_json::json!({"width": info.width, "height": info.height, "format": info.format})
            );
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
