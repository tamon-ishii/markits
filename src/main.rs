use clap::{Parser, Subcommand};
use markits::render_from_json;
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
    },
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Render { input } => {
            let json_content = if input == "-" {
                let mut buffer = String::new();
                io::stdin().read_to_string(&mut buffer)?;
                buffer
            } else {
                fs::read_to_string(&input)
                    .map_err(|e| format!("Failed to read file '{}': {}", input, e))?
            };

            let svg = render_from_json(&json_content)?;
            print!("{}", svg);
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
