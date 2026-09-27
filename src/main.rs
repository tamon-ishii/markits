use clap::{Parser, Subcommand};
use markits::{Scene, render_debug_from_json, render_from_json, render_with_layout_from_json};
use std::fs;
use std::io::{self, Read};
use std::process::ExitCode;

#[derive(Parser, Debug)]
#[command(
    name = "markits",
    about = "Semantic Annotation SVG Engine — renders clean annotation SVGs from intent JSON",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Render semantic annotation JSON into SVG
    Render {
        /// Path to JSON file, or '-' to read from standard input
        input: String,
        /// Emit SVG and positioned elements as JSON
        #[arg(long)]
        layout_json: bool,
        /// Overlay layout targets, candidates, scores, and selected positions
        #[arg(long)]
        debug: bool,
    },
    /// Validate semantic annotation JSON without rendering
    Validate {
        /// Path to JSON file, or '-' to read from standard input
        input: String,
    },
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
        Commands::Render { input, layout_json, debug } => {
            let json_content = read_input(&input)?;
            if layout_json {
                let mut result = render_with_layout_from_json(&json_content)?;
                if debug { result.svg = render_debug_from_json(&json_content)?; }
                println!("{}", serde_json::to_string_pretty(&result)?);
            } else if debug {
                print!("{}", render_debug_from_json(&json_content)?);
            } else {
                print!("{}", render_from_json(&json_content)?);
            }
        }
        Commands::Validate { input } => {
            Scene::from_json(&read_input(&input)?)?;
            println!("Valid annotation document");
        }
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
