//! PDF content stream generation for QR codes and text.
//!
//! This module provides:
//! - QR code generation and embedding
//! - Text rendering with standard and CID fonts
//! - PDF content stream building
//! - String encoding for PDF (ASCII and UTF-16BE)

use anyhow::{anyhow, Context, Result};
use crate::config::FieldSpec;
use image::{ImageBuffer, Luma};
use lopdf::{Dictionary, Document, Object, Stream};
use qrcode::QrCode;
use std::io::Write;

/// QR code size constant
const QR_SIZE: u32 = 200;

/// Builder for generating PDF content streams and associated XObjects
pub struct ContentBuilder {
    pub content_parts: Vec<String>,
    pub xobjects: Dictionary,
    font_name: String,
    cid_font_name: Option<String>,
}

impl ContentBuilder {
    /// Create a new ContentBuilder with the given font name
    pub fn new(font_name: String) -> Self {
        Self {
            content_parts: Vec::new(),
            xobjects: Dictionary::new(),
            font_name,
            cid_font_name: None,
        }
    }

    /// Create a new ContentBuilder with CID font support
    pub fn new_with_cid_font(font_name: String, cid_font_name: String) -> Self {
        Self {
            content_parts: Vec::new(),
            xobjects: Dictionary::new(),
            font_name,
            cid_font_name: Some(cid_font_name),
        }
    }

    /// Add a QR code field to the content
    pub fn add_qr_code(
        &mut self,
        value: &str,
        spec: &FieldSpec,
        page_height: f64,
        doc: &mut Document,
    ) -> Result<()> {
        // Generate QR code image
        let qr_img = generate_qr_code(value, QR_SIZE, QR_SIZE)?;

        // Convert grayscale image to raw bytes (8-bit per pixel)
        let raw_bytes: Vec<u8> = qr_img.pixels().map(|pixel| pixel[0]).collect();

        // Compress the image data
        let compressed_bytes = compress_data(&raw_bytes)?;

        // Create image XObject
        let mut img_dict = Dictionary::new();
        img_dict.set("Type", "XObject");
        img_dict.set("Subtype", "Image");
        img_dict.set("Width", QR_SIZE as i64);
        img_dict.set("Height", QR_SIZE as i64);
        img_dict.set("ColorSpace", "DeviceGray");
        img_dict.set("BitsPerComponent", 8_i64);
        img_dict.set("Filter", "FlateDecode");

        let img_stream = Stream::new(img_dict, compressed_bytes);
        let img_id = doc.add_object(img_stream);

        let img_name = format!("Im{}", img_id.0);
        self.xobjects.set(img_name.clone(), Object::Reference(img_id));

        // Calculate PDF coordinates (flip Y axis)
        let x = spec.x.as_points();
        let y = page_height - spec.y.as_points() - spec.h.as_points();
        let w = spec.w.as_points();
        let h = spec.h.as_points();

        // Add content stream commands for drawing the image
        self.content_parts.push(format!(
            "q {} 0 0 {} {} {} cm /{} Do Q ",
            w, h, x, y, img_name
        ));

        Ok(())
    }

    /// Add a text field to the content
    pub fn add_text(&mut self, value: &str, spec: &FieldSpec, page_height: f64) {
        let x = spec.x.as_points();
        let y = page_height - spec.y.as_points();
        let font_size = spec.font_size
            .map(|d| d.as_points())
            .unwrap_or_else(|| spec.h.as_points().min(spec.w.as_points() * 0.5));

        // Check if the text requires CID font (non-ASCII)
        let needs_cid = value.chars().any(|c| c > '\u{7F}');

        if needs_cid {
            if let Some(cid_font_name) = &self.cid_font_name {
                // Use CID font with hex encoding for non-ASCII text
                let hex_value = encode_cid_text(value);
                self.content_parts.push(format!(
                    "q BT 0 g /{} {} Tf {} {} Td <{}> Tj ET Q ",
                    cid_font_name, font_size, x, y - font_size, hex_value
                ));
            } else {
                // Fallback to regular font (may not display correctly)
                let escaped_value = escape_pdf_string(value);
                self.content_parts.push(format!(
                    "q BT 0 g /{} {} Tf {} {} Td ({}) Tj ET Q ",
                    self.font_name, font_size, x, y - font_size, escaped_value
                ));
            }
        } else {
            // Use regular font with escaped text for ASCII-only text
            let escaped_value = escape_pdf_string(value);
            self.content_parts.push(format!(
                "q BT 0 g /{} {} Tf {} {} Td ({}) Tj ET Q ",
                self.font_name, font_size, x, y - font_size, escaped_value
            ));
        }
    }

    /// Add a field based on its type
    pub fn add_field(
        &mut self,
        _field_name: &str,
        value: &str,
        spec: &FieldSpec,
        page_height: f64,
        doc: &mut Document,
    ) -> Result<()> {
        match spec.output_type.as_str() {
            "QR" => {
                self.add_qr_code(value, spec, page_height, doc)?;
            }
            "Text" => {
                self.add_text(value, spec, page_height);
            }
            _ => {
                return Err(anyhow!("Unknown output type: {}", spec.output_type));
            }
        }
        Ok(())
    }

    /// Build the final content bytes
    pub fn build_content_bytes(&self) -> Vec<u8> {
        self.content_parts.join("").as_bytes().to_vec()
    }
}

impl Default for ContentBuilder {
    fn default() -> Self {
        Self::new("F1".to_string())
    }
}

/// Escape special characters in PDF strings
pub fn escape_pdf_string(s: &str) -> String {
    let mut result = String::new();
    for c in s.chars() {
        match c {
            '(' => result.push_str(r"\("),
            ')' => result.push_str(r"\)"),
            '\\' => result.push_str(r"\\"),
            '\n' => result.push_str(r"\n"),
            '\r' => result.push_str(r"\r"),
            '\t' => result.push_str(r"\t"),
            _ => result.push(c),
        }
    }
    result
}

/// Encode text for CID font (Identity-H encoding)
///
/// Converts text to UTF-16BE and returns hex representation
pub fn encode_cid_text(s: &str) -> String {
    let mut utf16be: Vec<u8> = Vec::new();

    // Add BOM (Byte Order Mark) for UTF-16BE if needed
    // Actually for Identity-H, we don't need BOM, just the raw UTF-16BE bytes

    for c in s.chars() {
        let code = c as u32;
        if code <= 0xFFFF {
            // BMP character - directly encode as UTF-16BE
            utf16be.push((code >> 8) as u8);
            utf16be.push((code & 0xFF) as u8);
        } else {
            // Supplementary character - needs surrogate pair
            let code = code - 0x10000;
            let high_surrogate = 0xD800 + ((code >> 10) & 0x3FF);
            let low_surrogate = 0xDC00 + (code & 0x3FF);
            utf16be.push((high_surrogate >> 8) as u8);
            utf16be.push((high_surrogate & 0xFF) as u8);
            utf16be.push((low_surrogate >> 8) as u8);
            utf16be.push((low_surrogate & 0xFF) as u8);
        }
    }

    // Convert to hex string
    utf16be.iter()
        .map(|b| format!("{:02X}", b))
        .collect::<Vec<_>>()
        .join("")
}

/// Compress data using zlib/flate2
pub fn compress_data(data: &[u8]) -> Result<Vec<u8>> {
    let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    encoder.write_all(data)?;
    Ok(encoder.finish()?)
}

/// Generate a QR code as an image buffer
pub fn generate_qr_code(data: &str, width: u32, height: u32) -> Result<ImageBuffer<Luma<u8>, Vec<u8>>> {
    let qr_code = QrCode::new(data)
        .with_context(|| format!("Failed to generate QR code for data: {}", data))?;

    // Render QR code with light=255 (white) and dark=0 (black)
    let img = qr_code
        .render::<Luma<u8>>()
        .light_color(Luma([255u8]))
        .dark_color(Luma([0u8]))
        .build();

    // Scale the image to the requested size
    let scaled = image::imageops::resize(
        &img,
        width,
        height,
        image::imageops::FilterType::Nearest,
    );
    Ok(scaled)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_escape_pdf_string() {
        assert_eq!(escape_pdf_string("hello"), "hello");
        assert_eq!(escape_pdf_string("(hello)"), r"\(hello\)");
        assert_eq!(escape_pdf_string("hello\\world"), r"hello\\world");
        assert_eq!(escape_pdf_string("line1\nline2"), r"line1\nline2");
    }

    #[test]
    fn test_content_builder_new() {
        let builder = ContentBuilder::new("F1".to_string());
        assert!(builder.content_parts.is_empty());
        assert!(builder.xobjects.is_empty());
    }

    #[test]
    fn test_content_builder_add_text() {
        let mut builder = ContentBuilder::new("F1".to_string());
        let spec = FieldSpec {
            x: crate::config::Dimension(100.0),
            y: crate::config::Dimension(200.0),
            w: crate::config::Dimension(50.0),
            h: crate::config::Dimension(12.0),
            output_type: "Text".to_string(),
            font_size: None,
        };

        builder.add_text("Hello", &spec, 800.0);

        assert_eq!(builder.content_parts.len(), 1);
        assert!(builder.content_parts[0].contains("Hello"));
        assert!(builder.xobjects.is_empty());
    }
}
