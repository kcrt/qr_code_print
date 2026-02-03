mod config;
mod pdf;

use anyhow::{anyhow, Context, Result};
use lopdf::Document;

use config::{load_base_pdf, load_csv_data, load_settings_config};
use pdf::create_output_pdf;

fn run() -> Result<()> {
    // Get the current directory
    let current_dir = std::env::current_dir()?;

    // Define file paths
    let settings_json_path = current_dir.join("settings.json");
    let data_csv_path = current_dir.join("data.csv");
    let base_pdf_path = current_dir.join("base.pdf");
    let output_pdf_path = current_dir.join("output.pdf");

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
    if let Err(e) = run() {
        eprintln!("Error: {}", e);
        for cause in e.chain().skip(1) {
            eprintln!("Caused by: {}", cause);
        }
        std::process::exit(1);
    }
}
