use anyhow::{Context, Result};
use csv::ReaderBuilder;
use serde::Deserialize;
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;

#[derive(Debug, Deserialize, Clone)]
pub struct FieldSpec {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    #[serde(rename = "type")]
    pub output_type: String,
}

#[derive(Debug, Deserialize)]
pub struct PlaceConfig {
    pub fields: HashMap<String, FieldSpec>,
    pub settings: SettingsSection,
}

#[derive(Debug, Deserialize)]
pub struct SettingsSection {
    #[serde(default)]
    pub font: Option<String>,
}

pub struct DataRow {
    pub data: HashMap<String, String>,
}

/// Helper function to open a file with consistent error context
fn open_file_with_context(path: &Path, description: &str) -> Result<File> {
    File::open(path)
        .with_context(|| format!("Failed to open {} at {:?}", description, path))
}

pub fn load_settings_config(path: &Path) -> Result<PlaceConfig> {
    let file = open_file_with_context(path, "settings.json")?;
    let reader = BufReader::new(file);
    let config: PlaceConfig = serde_json::from_reader(reader)
        .with_context(|| "Failed to parse settings.json")?;
    Ok(config)
}

pub fn load_csv_data(path: &Path) -> Result<Vec<DataRow>> {
    let file = open_file_with_context(path, "data.csv")?;
    let mut rdr = ReaderBuilder::new().from_reader(file);
    let headers = rdr.headers()?.clone();

    let mut rows = Vec::new();
    for result in rdr.records() {
        let record = result?;
        let mut data = HashMap::new();
        for (i, header) in headers.iter().enumerate() {
            data.insert(header.to_string(), record.get(i).unwrap_or("").to_string());
        }
        rows.push(DataRow { data });
    }
    Ok(rows)
}

pub fn load_base_pdf(path: &Path) -> Result<Vec<u8>> {
    let file = open_file_with_context(path, "base.pdf")?;
    let mut buf = Vec::new();
    let mut reader = BufReader::new(file);
    reader.read_to_end(&mut buf)
        .with_context(|| "Failed to read base.pdf")?;
    Ok(buf)
}
