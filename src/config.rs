//! Configuration loading and parsing.
//!
//! This module handles:
//! - Loading and parsing settings.json (field placements, fonts)
//! - Loading and parsing data.csv (content data)
//! - Unit conversion for dimensions (mm, cm, in, pt)
//! - Dimension type with flexible deserialization

use anyhow::{Context, Result};
use csv::ReaderBuilder;
use serde::{Deserialize, Deserializer};
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;

/// Dimension value that can be specified as:
/// - A number (interpreted as points)
/// - A string with unit: e.g., "100 mm", "10 cm", "1 in" (inches)
#[derive(Debug, Clone, Copy)]
pub struct Dimension(pub f64);

impl Dimension {
    /// Convert to points (internal PDF unit)
    pub fn as_points(&self) -> f64 {
        self.0
    }
}

impl<'de> Deserialize<'de> for Dimension {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct DimensionVisitor;

        impl serde::de::Visitor<'_> for DimensionVisitor {
            type Value = Dimension;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a number or a string with unit (e.g., \"100 mm\", \"10 cm\", \"1 in\")")
            }

            fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                Ok(Dimension(value as f64))
            }

            fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                Ok(Dimension(value as f64))
            }

            fn visit_f64<E>(self, value: f64) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                Ok(Dimension(value))
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                let value = value.trim();
                let (num_str, unit) = value.split_at(
                    value
                        .find(|c: char| c.is_whitespace() || c == 'm' || c == 'c' || c == 'i')
                        .unwrap_or(value.len()),
                );
                let num_str = num_str.trim();
                let unit = unit.trim().to_lowercase();

                let num: f64 = num_str.parse().map_err(|_| {
                    serde::de::Error::custom(format!("invalid number in dimension: {}", num_str))
                })?;

                // 1 inch = 72 points (PDF default unit)
                let points = match unit.as_str() {
                    "" | "pt" | "point" | "points" => num,
                    "mm" => num * 72.0 / 25.4,
                    "cm" => num * 72.0 / 2.54,
                    "in" | "inch" | "inches" => num * 72.0,
                    _ => {
                        return Err(serde::de::Error::custom(format!(
                            "unknown unit '{}'. Supported: mm, cm, in, pt",
                            unit
                        )))
                    }
                };

                Ok(Dimension(points))
            }
        }

        deserializer.deserialize_any(DimensionVisitor)
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct FieldSpec {
    pub x: Dimension,
    pub y: Dimension,
    pub w: Dimension,
    pub h: Dimension,
    #[serde(rename = "type")]
    pub output_type: String,
    #[serde(default)]
    pub font_size: Option<Dimension>,
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_dimension_from_number() {
        let json = json!(100);
        let dim: Dimension = serde_json::from_value(json).unwrap();
        assert_eq!(dim.as_points(), 100.0);
    }

    #[test]
    fn test_dimension_from_f64() {
        let json = json!(12.5);
        let dim: Dimension = serde_json::from_value(json).unwrap();
        assert_eq!(dim.as_points(), 12.5);
    }

    #[test]
    fn test_dimension_from_mm() {
        let json = json!("100 mm");
        let dim: Dimension = serde_json::from_value(json).unwrap();
        // 100 mm = 100 * 72 / 25.4 points ≈ 283.46
        assert!((dim.as_points() - 283.46).abs() < 0.01);
    }

    #[test]
    fn test_dimension_from_cm() {
        let json = json!("10 cm");
        let dim: Dimension = serde_json::from_value(json).unwrap();
        // 10 cm = 10 * 72 / 2.54 points ≈ 283.46
        assert!((dim.as_points() - 283.46).abs() < 0.01);
    }

    #[test]
    fn test_dimension_from_inch() {
        let json = json!("1 in");
        let dim: Dimension = serde_json::from_value(json).unwrap();
        assert_eq!(dim.as_points(), 72.0);
    }

    #[test]
    fn test_dimension_from_inches() {
        let json = json!("1 inches");
        let dim: Dimension = serde_json::from_value(json).unwrap();
        assert_eq!(dim.as_points(), 72.0);
    }

    #[test]
    fn test_dimension_from_inch_full_word() {
        let json = json!("1 inch");
        let dim: Dimension = serde_json::from_value(json).unwrap();
        assert_eq!(dim.as_points(), 72.0);
    }

    #[test]
    fn test_dimension_default_unit() {
        let json = json!(100);
        let dim: Dimension = serde_json::from_value(json).unwrap();
        assert_eq!(dim.as_points(), 100.0);
    }

    #[test]
    fn test_dimension_from_pt() {
        let json = json!("100 pt");
        let dim: Dimension = serde_json::from_value(json).unwrap();
        assert_eq!(dim.as_points(), 100.0);
    }

    #[test]
    fn test_dimension_from_points() {
        let json = json!("100 points");
        let dim: Dimension = serde_json::from_value(json).unwrap();
        assert_eq!(dim.as_points(), 100.0);
    }

    #[test]
    fn test_dimension_whitespace_handling() {
        let json = json!("  100  mm  ");
        let dim: Dimension = serde_json::from_value(json).unwrap();
        assert!((dim.as_points() - 283.46).abs() < 0.01);
    }

    #[test]
    fn test_dimension_lowercase_unit() {
        let json = json!("100 MM");
        let dim: Dimension = serde_json::from_value(json).unwrap();
        // Unit is lowercased in parsing, so this should work
        assert!((dim.as_points() - 283.46).abs() < 0.01);
    }

    #[test]
    fn test_dimension_invalid_unit() {
        let json = json!("100 foo");
        let result: Result<Dimension, _> = serde_json::from_value(json);
        assert!(result.is_err());
    }

    #[test]
    fn test_dimension_invalid_number() {
        let json = json!("abc mm");
        let result: Result<Dimension, _> = serde_json::from_value(json);
        assert!(result.is_err());
    }

    #[test]
    fn test_field_spec_with_units() {
        let json = json!({
            "x": "50 mm",
            "y": "10 cm",
            "w": "1 in",
            "h": "50 pt",
            "type": "QR"
        });
        let spec: FieldSpec = serde_json::from_value(json).unwrap();
        assert!((spec.x.as_points() - 141.73).abs() < 0.01);  // 50 mm
        assert!((spec.y.as_points() - 283.46).abs() < 0.01);  // 10 cm
        assert_eq!(spec.w.as_points(), 72.0);                 // 1 inch
        assert_eq!(spec.h.as_points(), 50.0);                 // 50 pt
        assert!(spec.font_size.is_none());
    }

    #[test]
    fn test_field_spec_with_font_size() {
        let json = json!({
            "x": "50 mm",
            "y": "10 cm",
            "w": "1 in",
            "h": "50 pt",
            "type": "Text",
            "font_size": "12 pt"
        });
        let spec: FieldSpec = serde_json::from_value(json).unwrap();
        assert_eq!(spec.font_size.unwrap().as_points(), 12.0);
    }

    #[test]
    fn test_field_spec_with_font_size_mm() {
        let json = json!({
            "x": "50 mm",
            "y": "10 cm",
            "w": "1 in",
            "h": "50 pt",
            "type": "Text",
            "font_size": "5 mm"
        });
        let spec: FieldSpec = serde_json::from_value(json).unwrap();
        // 5 mm = 5 * 72 / 25.4 points ≈ 14.17
        assert!((spec.font_size.unwrap().as_points() - 14.17).abs() < 0.01);
    }
}
