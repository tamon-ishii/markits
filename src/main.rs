use clap::{Parser, Subcommand};
use markits::{Scene, render_from_json, render_with_layout_from_json};
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
        /// Output format: svg or layout-json
        #[arg(long, default_value = "svg", value_parser = ["svg", "layout-json"])]
        format: String,
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
    /// Print the bundled Markdown manual for AI/LLM use
    Manual,
}

fn read_input(input: &str) -> Result<String, Box<dyn std::error::Error>> {
    if input == "-" {
        let mut buffer = String::new();
        io::stdin().read_to_string(&mut buffer)?;
        Ok(buffer)
    } else {
        Ok(fs::read_to_string(input)
            .map_err(|e| format!("Failed to read file '{}': {}", input, e))?)
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Render {
            input,
            format,
            image,
            output,
        } => {
            let json_content = read_input(&input)?;
            if let (Some(image), Some(output)) = (image, output) {
                if format != "svg" {
                    return Err("--format layout-json cannot be used with --image".into());
                }
                raster::render_png(&json_content, &image, &output)?;
            } else if format == "layout-json" {
                let result = render_with_layout_from_json(&json_content)?;
                println!("{}", serde_json::to_string_pretty(&result)?);
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
            println!("Valid");
        }
        Commands::Inspect { image } => {
            let info = raster::inspect_image(&image)?;
            println!(
                "{}",
                serde_json::json!({"width": info.width, "height": info.height, "format": info.format})
            );
        }
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
