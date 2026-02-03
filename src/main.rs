use anyhow::{anyhow, Context, Result};
use csv::ReaderBuilder;
use image::{ImageBuffer, Luma};
use lopdf::{Dictionary, Document, Object, Stream};
use qrcode::QrCode;
use serde::Deserialize;
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufReader, Read, Write};
use std::path::Path;

#[derive(Debug, Deserialize, Clone)]
struct FieldSpec {
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    #[serde(rename = "type")]
    output_type: String,
}

#[derive(Debug, Deserialize)]
struct PlaceConfig {
    fields: HashMap<String, FieldSpec>,
}

struct DataRow {
    data: HashMap<String, String>,
}

fn load_place_config(path: &Path) -> Result<PlaceConfig> {
    let file = File::open(path)
        .with_context(|| format!("Failed to open place.json at {:?}", path))?;
    let reader = BufReader::new(file);
    let config: PlaceConfig = serde_json::from_reader(reader)
        .with_context(|| "Failed to parse place.json")?;
    Ok(config)
}

fn load_csv_data(path: &Path) -> Result<Vec<DataRow>> {
    let file = File::open(path)
        .with_context(|| format!("Failed to open data.csv at {:?}", path))?;
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

fn load_base_pdf(path: &Path) -> Result<Document> {
    let file = File::open(path)
        .with_context(|| format!("Failed to open base.pdf at {:?}", path))?;
    let mut buf = Vec::new();
    let mut reader = BufReader::new(file);
    reader.read_to_end(&mut buf)?;
    Document::load_mem(&buf).with_context(|| "Failed to load base.pdf")
}

fn escape_pdf_string(s: &str) -> String {
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

fn compress_data(data: &[u8]) -> Result<Vec<u8>> {
    let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    encoder.write_all(data)?;
    Ok(encoder.finish()?)
}

fn generate_qr_code(data: &str, width: u32, height: u32) -> Result<ImageBuffer<Luma<u8>, Vec<u8>>> {
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

fn create_output_pdf(
    base_doc: &Document,
    data_rows: &[DataRow],
    config: &PlaceConfig,
) -> Result<Document> {
    // Clone the base document to preserve all its content
    let mut output_doc = base_doc.clone();

    // Get the first page from base document
    let base_page_id = *base_doc.get_pages().iter().next()
        .ok_or_else(|| anyhow!("No pages in base.pdf"))?.1;

    let base_page = base_doc.get_object(base_page_id)?.as_dict()?;

    // Get page dimensions from base page
    let media_box = base_page.get(b"MediaBox")?.as_array()?;
    let page_height = (media_box[3].as_float()? - media_box[1].as_float()?) as f64;

    // Get or create base page's resources (unused but kept for future reference)
    let _base_resources_id: Option<(u32, u16)> = base_page.get(b"Resources")
        .ok()
        .and_then(|r| r.as_reference().ok());

    // Create a standard font (Helvetica) for our new text
    let font_id = {
        let mut font_dict = Dictionary::new();
        font_dict.set("Type", "Font");
        font_dict.set("Subtype", "Type1");
        font_dict.set("BaseFont", "Helvetica");
        output_doc.add_object(Object::Dictionary(font_dict))
    };

    // Create additional pages for each row (beyond the first)
    let mut additional_page_ids = Vec::new();

    for row in data_rows.iter().skip(1) {
        // Clone the base page for this row
        let page_dict = base_page.clone();

        // Add the cloned page to the document
        // Note: The content streams are already in output_doc from the initial clone
        let page_id = output_doc.add_object(Object::Dictionary(page_dict));

        // Build overlay content for this row
        let mut content_parts = Vec::new();
        let mut xobject_dict = Dictionary::new();

        for (field_name, field_spec) in &config.fields {
            let value = row.data.get(field_name).map(|s| s.as_str()).unwrap_or("");

            match field_spec.output_type.as_str() {
                "QR" => {
                    // Generate QR code
                    let qr_size = 200;
                    let qr_img = generate_qr_code(value, qr_size, qr_size)?;

                    // Convert grayscale image to raw bytes (8-bit per pixel)
                    let mut raw_bytes = Vec::new();
                    for pixel in qr_img.pixels() {
                        raw_bytes.push(pixel[0]);
                    }

                    // Compress the image data
                    let compressed_bytes = compress_data(&raw_bytes)?;

                    // Create image XObject
                    let mut img_dict = Dictionary::new();
                    img_dict.set("Type", "XObject");
                    img_dict.set("Subtype", "Image");
                    img_dict.set("Width", qr_size as i64);
                    img_dict.set("Height", qr_size as i64);
                    img_dict.set("ColorSpace", "DeviceGray");
                    img_dict.set("BitsPerComponent", 8_i64);
                    img_dict.set("Filter", "FlateDecode");

                    let img_stream = Stream::new(img_dict, compressed_bytes);
                    let img_id = output_doc.add_object(img_stream);

                    let img_name = format!("Im{}", img_id.0);
                    xobject_dict.set(img_name.clone(), Object::Reference(img_id));

                    let x = field_spec.x;
                    let y = page_height - field_spec.y - field_spec.h;
                    let w = field_spec.w;
                    let h = field_spec.h;

                    content_parts.push(format!(
                        "q {} 0 0 {} {} {} cm /{} Do Q ",
                        w, h, x, y, img_name
                    ));
                }
                "Text" => {
                    let x = field_spec.x;
                    let y = page_height - field_spec.y;
                    let font_size = field_spec.h.min(field_spec.w * 0.5) as f64;
                    let escaped_value = escape_pdf_string(value);

                    content_parts.push(format!(
                        "q BT /F1 {} Tf {} {} Td ({}) Tj ET Q ",
                        font_size, x, y - font_size, escaped_value
                    ));
                }
                _ => {
                    return Err(anyhow!("Unknown output type: {}", field_spec.output_type));
                }
            }
        }

        // Append overlay content to the cloned page (preserving base content)
        let overlay_bytes = content_parts.join("").as_bytes().to_vec();
        output_doc.add_page_contents(page_id, overlay_bytes)?;

        // Get the page's resources and add our resources
        let resources_id = output_doc.get_object(page_id)
            .ok()
            .and_then(|page| page.as_dict().ok())
            .and_then(|dict| dict.get(b"Resources").ok())
            .and_then(|r| r.as_reference().ok());

        if let Some(res_id) = resources_id {
            // Get the existing font dictionary first (before mutable borrow)
            let font_dict_to_clone = if let Ok(res) = output_doc.get_dictionary(res_id) {
                match res.get(b"Font") {
                    Ok(Object::Reference(font_dict_id)) => {
                        output_doc.get_dictionary(*font_dict_id)
                            .map(|d| d.clone())
                            .ok()
                    }
                    Ok(Object::Dictionary(d)) => Some(d.clone()),
                    _ => None,
                }
            } else {
                None
            };

            // Now modify the resources
            if let Ok(res) = output_doc.get_dictionary_mut(res_id) {
                let mut font_resources = font_dict_to_clone.unwrap_or_else(|| Dictionary::new());
                font_resources.set("F1", Object::Reference(font_id));
                res.set("Font", Object::Dictionary(font_resources));

                // Add XObject resources
                if !xobject_dict.is_empty() {
                    let mut xobject_resources = if let Ok(xobj) = res.get(b"XObject").and_then(|x| x.as_dict()) {
                        xobj.clone()
                    } else {
                        Dictionary::new()
                    };
                    for (key, value) in xobject_dict.iter() {
                        xobject_resources.set(key.to_vec(), value.clone());
                    }
                    res.set("XObject", Object::Dictionary(xobject_resources));
                }
            }
        }

        additional_page_ids.push(page_id);
    }

    // Now add content to the first page (base page) for the first row
    if let Some(first_row) = data_rows.first() {
        let mut content_parts = Vec::new();
        let mut xobject_dict = Dictionary::new();

        for (field_name, field_spec) in &config.fields {
            let value = first_row.data.get(field_name).map(|s| s.as_str()).unwrap_or("");

            match field_spec.output_type.as_str() {
                "QR" => {
                    let qr_size = 200;
                    let qr_img = generate_qr_code(value, qr_size, qr_size)?;

                    let mut raw_bytes = Vec::new();
                    for pixel in qr_img.pixels() {
                        raw_bytes.push(pixel[0]);
                    }

                    // Compress the image data
                    let compressed_bytes = compress_data(&raw_bytes)?;

                    let mut img_dict = Dictionary::new();
                    img_dict.set("Type", "XObject");
                    img_dict.set("Subtype", "Image");
                    img_dict.set("Width", qr_size as i64);
                    img_dict.set("Height", qr_size as i64);
                    img_dict.set("ColorSpace", "DeviceGray");
                    img_dict.set("BitsPerComponent", 8_i64);
                    img_dict.set("Filter", "FlateDecode");

                    let img_stream = Stream::new(img_dict, compressed_bytes);
                    let img_id = output_doc.add_object(img_stream);

                    let img_name = format!("Im{}", img_id.0);
                    xobject_dict.set(img_name.clone(), Object::Reference(img_id));

                    let x = field_spec.x;
                    let y = page_height - field_spec.y - field_spec.h;
                    let w = field_spec.w;
                    let h = field_spec.h;

                    content_parts.push(format!(
                        "q {} 0 0 {} {} {} cm /{} Do Q ",
                        w, h, x, y, img_name
                    ));
                }
                "Text" => {
                    let x = field_spec.x;
                    let y = page_height - field_spec.y;
                    let font_size = field_spec.h.min(field_spec.w * 0.5) as f64;
                    let escaped_value = escape_pdf_string(value);

                    content_parts.push(format!(
                        "q BT /F1 {} Tf {} {} Td ({}) Tj ET Q ",
                        font_size, x, y - font_size, escaped_value
                    ));
                }
                _ => {
                    return Err(anyhow!("Unknown output type: {}", field_spec.output_type));
                }
            }
        }

        // Append new content to the base page
        let new_content = content_parts.join("").as_bytes().to_vec();

        // Add new content to the first page
        if let Ok(first_page_id) = output_doc.get_pages().values().next().ok_or_else(|| anyhow!("No pages")) {
            output_doc.add_page_contents(*first_page_id, new_content)?;

            // Get the page's current resources
            let resources_id = output_doc.get_object(*first_page_id)
                .ok()
                .and_then(|page| page.as_dict().ok())
                .and_then(|dict| dict.get(b"Resources").ok())
                .and_then(|r| r.as_reference().ok());

            if let Some(res_id) = resources_id {
                // Get the existing font dictionary first (before mutable borrow)
                let font_dict_to_clone = if let Ok(res) = output_doc.get_dictionary(res_id) {
                    match res.get(b"Font") {
                        Ok(Object::Reference(font_dict_id)) => {
                            output_doc.get_dictionary(*font_dict_id)
                                .map(|d| d.clone())
                                .ok()
                        }
                        Ok(Object::Dictionary(d)) => Some(d.clone()),
                        _ => None,
                    }
                } else {
                    None
                };

                // Now modify the resources
                if let Ok(res) = output_doc.get_dictionary_mut(res_id) {
                    let mut font_resources = font_dict_to_clone.unwrap_or_else(|| Dictionary::new());
                    font_resources.set("F1", Object::Reference(font_id));
                    res.set("Font", Object::Dictionary(font_resources));

                    if !xobject_dict.is_empty() {
                        let mut xobject_resources = if let Ok(xobj) = res.get(b"XObject").and_then(|x| x.as_dict()) {
                            xobj.clone()
                        } else {
                            Dictionary::new()
                        };
                        for (key, value) in xobject_dict.iter() {
                            xobject_resources.set(key.to_vec(), value.clone());
                        }
                        res.set("XObject", Object::Dictionary(xobject_resources));
                    }
                }
            }
        }
    }

    // Update the pages dictionary to include all new pages
    if !additional_page_ids.is_empty() {
        if let Ok(pages) = output_doc.catalog()?.get(b"Pages") {
            if let Ok(pages_id) = pages.as_reference() {
                // Get current kids array before mutable borrow
                let current_kids = output_doc.get_dictionary(pages_id)
                    .ok()
                    .and_then(|d| d.get(b"Kids").ok())
                    .and_then(|k| k.as_array().ok())
                    .map(|arr| arr.clone())
                    .unwrap_or_default();

                let mut new_kids = current_kids;
                for page_id in additional_page_ids {
                    new_kids.push(Object::Reference(page_id));
                }
                let new_count = new_kids.len();

                if let Ok(pages_mut) = output_doc.get_dictionary_mut(pages_id) {
                    pages_mut.set("Kids", new_kids);
                    pages_mut.set("Count", new_count as i64);
                }
            }
        }
    }

    Ok(output_doc)
}

fn run() -> Result<()> {
    // Get the current directory
    let current_dir = std::env::current_dir()?;

    // Define file paths
    let place_json_path = current_dir.join("place.json");
    let data_csv_path = current_dir.join("data.csv");
    let base_pdf_path = current_dir.join("base.pdf");
    let output_pdf_path = current_dir.join("output.pdf");

    // Check if required files exist
    for path in [&place_json_path, &data_csv_path, &base_pdf_path].iter() {
        if !path.exists() {
            return Err(anyhow!("Required file not found: {:?}", path));
        }
    }

    println!("Loading configuration from place.json...");
    let config = load_place_config(&place_json_path)?;

    println!("Loading data from data.csv...");
    let data_rows = load_csv_data(&data_csv_path)?;
    println!("Found {} rows in data.csv", data_rows.len());

    println!("Loading base.pdf...");
    let base_doc = load_base_pdf(&base_pdf_path)?;

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
