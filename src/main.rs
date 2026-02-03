mod config;
mod pdf;

use anyhow::{anyhow, Context, Result};
use clap::Parser;
use lopdf::Document;
use std::path::PathBuf;

use config::{load_base_pdf, load_csv_data, load_settings_config};
use pdf::create_output_pdf;

/// Generate QR codes and place them on a PDF template.
#[derive(Parser, Debug)]
#[command(name = "qr_code_print")]
#[command(about = "Generate QR codes and place them on a PDF template.", long_about = None)]
struct Args {
    /// Target directory containing base.pdf, settings.json, and data.csv
    /// output.pdf will be saved in this directory
    #[arg(short, long, default_value = ".")]
    target_dir: PathBuf,
}

fn run(target_dir: PathBuf) -> Result<()> {
    // Verify the target directory exists
    if !target_dir.exists() {
        return Err(anyhow!("Target directory not found: {:?}", target_dir));
    }
    if !target_dir.is_dir() {
        return Err(anyhow!("Target path is not a directory: {:?}", target_dir));
    }

    // Define file paths
    let settings_json_path = target_dir.join("settings.json");
    let data_csv_path = target_dir.join("data.csv");
    let base_pdf_path = target_dir.join("base.pdf");
    let output_pdf_path = target_dir.join("output.pdf");

    // Check if required files exist
    for path in [&settings_json_path, &data_csv_path, &base_pdf_path].iter() {
        if !path.exists() {
            return Err(anyhow!("Required file not found: {:?}", path));
        }
    }

    println!("Loading configuration from settings.json...");
    let config = load_settings_config(&settings_json_path)?;

    println!("Loading data from data.csv...");
    let data_rows = load_csv_data(&data_csv_path)?;
    println!("Found {} rows in data.csv", data_rows.len());

    println!("Loading base.pdf...");
    let base_pdf_bytes = load_base_pdf(&base_pdf_path)?;
    let base_doc = Document::load_mem(&base_pdf_bytes)
        .with_context(|| "Failed to load base.pdf")?;

    println!("Generating output.pdf...");
    let mut output_doc = create_output_pdf(&base_doc, &data_rows, &config)?;

    output_doc.save(&output_pdf_path)?;
    println!("Successfully saved output.pdf with {} pages", data_rows.len());

    Ok(())
}

fn main() {
    let args = Args::parse();

    if let Err(e) = run(args.target_dir) {
        eprintln!("Error: {}", e);
        for cause in e.chain().skip(1) {
            eprintln!("Caused by: {}", cause);
        }
        std::process::exit(1);
    }
}
