use anyhow::{Context, Result};
use lopdf::{Dictionary, Document, Object, Stream};
use std::fs;
use std::path::{Path, PathBuf};
use fontdb::Database;
use ttf_parser::Face;

/// Standard PDF Type1 fonts
#[derive(Debug, Clone, Copy)]
pub enum StandardFont {
    Helvetica,
    HelveticaBold,
    HelveticaOblique,
    HelveticaBoldOblique,
    TimesRoman,
    TimesBold,
    TimesItalic,
    TimesBoldItalic,
    Courier,
    CourierBold,
    CourierOblique,
    CourierBoldOblique,
}

impl StandardFont {
    /// Get the PDF BaseFont name for this font
    pub fn base_font_name(&self) -> &'static str {
        match self {
            StandardFont::Helvetica => "Helvetica",
            StandardFont::HelveticaBold => "Helvetica-Bold",
            StandardFont::HelveticaOblique => "Helvetica-Oblique",
            StandardFont::HelveticaBoldOblique => "Helvetica-BoldOblique",
            StandardFont::TimesRoman => "Times-Roman",
            StandardFont::TimesBold => "Times-Bold",
            StandardFont::TimesItalic => "Times-Italic",
            StandardFont::TimesBoldItalic => "Times-BoldItalic",
            StandardFont::Courier => "Courier",
            StandardFont::CourierBold => "Courier-Bold",
            StandardFont::CourierOblique => "Courier-Oblique",
            StandardFont::CourierBoldOblique => "Courier-BoldOblique",
        }
    }

    /// Parse a font name into a StandardFont
    pub fn from_name(name: &str) -> Option<StandardFont> {
        let name_lower = name.to_lowercase();
        match name_lower.as_str() {
            "helvetica" => Some(StandardFont::Helvetica),
            "helvetica-bold" => Some(StandardFont::HelveticaBold),
            "helvetica-oblique" => Some(StandardFont::HelveticaOblique),
            "helvetica-boldoblique" => Some(StandardFont::HelveticaBoldOblique),
            "times" | "times-roman" => Some(StandardFont::TimesRoman),
            "times-bold" => Some(StandardFont::TimesBold),
            "times-italic" => Some(StandardFont::TimesItalic),
            "times-bolditalic" => Some(StandardFont::TimesBoldItalic),
            "courier" => Some(StandardFont::Courier),
            "courier-bold" => Some(StandardFont::CourierBold),
            "courier-oblique" => Some(StandardFont::CourierOblique),
            "courier-boldoblique" => Some(StandardFont::CourierBoldOblique),
            _ => None,
        }
    }
}

/// Create a font in the PDF document
///
/// Returns the font object ID and the base font name for use in content streams
pub fn create_font(doc: &mut Document, font: StandardFont) -> Result<((u32, u16), String)> {
    let base_font_name = font.base_font_name().to_string();

    // Try to find a system font file for non-standard fonts
    // For now, we only support standard Type1 fonts
    let mut font_dict = Dictionary::new();
    font_dict.set("Type", "Font");
    font_dict.set("Subtype", "Type1");
    font_dict.set("BaseFont", base_font_name.clone());

    let font_id = doc.add_object(Object::Dictionary(font_dict));

    Ok((font_id, base_font_name))
}

/// Embed a TrueType font in the PDF document
///
/// This allows using custom fonts like "Meiryo UI"
#[allow(dead_code)]
pub fn create_true_type_font(
    doc: &mut Document,
    font_path: &Path,
    font_name: &str,
) -> Result<((u32, u16), String)> {
    let font_data = fs::read(font_path)
        .with_context(|| format!("Failed to read font file: {:?}", font_path))?;

    embed_true_type_font_data(doc, &font_data, font_name)
}

/// Embed a TrueType font from raw data
///
/// This allows using custom fonts loaded from memory
#[allow(dead_code)]
pub fn embed_true_type_font_data(
    doc: &mut Document,
    font_data: &[u8],
    font_name: &str,
) -> Result<((u32, u16), String)> {
    // Create font dictionary
    let mut font_dict = Dictionary::new();
    font_dict.set("Type", "Font");
    font_dict.set("Subtype", "TrueType");
    font_dict.set("BaseFont", font_name);

    // Create font descriptor
    let mut font_descriptor = Dictionary::new();
    font_descriptor.set("Type", "FontDescriptor");
    font_descriptor.set("FontName", font_name);

    // Estimate font flags (for simplicity, using symbolic font flags)
    font_descriptor.set("Flags", 4i64); // Symbolic

    // Font bounding box - using conservative defaults
    font_descriptor.set("FontBBox", vec![0i64, 0i64, 1000i64, 1000i64].into_iter().map(Object::Integer).collect::<Vec<_>>());

    // Italic angle
    font_descriptor.set("ItalicAngle", 0i64);

    // Ascent and descent (typical values)
    font_descriptor.set("Ascent", 1000i64);
    font_descriptor.set("Descent", -200i64);

    // Cap height
    font_descriptor.set("CapHeight", 700i64);

    // Stem width (average width)
    font_descriptor.set("StemV", 80i64);

    let descriptor_id = doc.add_object(Object::Dictionary(font_descriptor));
    font_dict.set("FontDescriptor", Object::Reference(descriptor_id));

    // Embed the font program
    let mut font_stream_dict = Dictionary::new();
    font_stream_dict.set("Length1", font_data.len() as i64);

    let font_stream = Stream::new(font_stream_dict, font_data.to_vec());
    let font_stream_id = doc.add_object(font_stream);

    // Set the font file in the descriptor
    if let Ok(descriptor) = doc.get_dictionary_mut(descriptor_id) {
        descriptor.set("FontFile2", Object::Reference(font_stream_id));
    }

    let font_id = doc.add_object(Object::Dictionary(font_dict));

    Ok((font_id, font_name.to_string()))
}

/// Embed a CID-keyed font for CJK characters
///
/// This creates a Type0 font with a CIDFont descendant for proper CJK rendering
pub fn embed_cid_font(
    doc: &mut Document,
    font_data: &[u8],
    font_name: &str,
) -> Result<((u32, u16), String)> {
    // Create CIDFont dictionary
    let mut cid_font = Dictionary::new();
    cid_font.set("Type", "Font");
    cid_font.set("Subtype", "CIDFontType2"); // TrueType-based CID font
    cid_font.set("BaseFont", font_name);
    cid_font.set("CIDSystemInfo", {
        let mut cid_system = Dictionary::new();
        cid_system.set("Registry", "Adobe");
        cid_system.set("Ordering", "Identity");
        cid_system.set("Supplement", 0i64);
        Object::Dictionary(cid_system)
    });

    // Create font descriptor
    let mut font_descriptor = Dictionary::new();
    font_descriptor.set("Type", "FontDescriptor");
    font_descriptor.set("FontName", font_name);
    font_descriptor.set("Flags", 4i64); // Symbolic
    font_descriptor.set("FontBBox", vec![0i64, 0i64, 1000i64, 1000i64].into_iter().map(Object::Integer).collect::<Vec<_>>());
    font_descriptor.set("ItalicAngle", 0i64);
    font_descriptor.set("Ascent", 1000i64);
    font_descriptor.set("Descent", -200i64);
    font_descriptor.set("CapHeight", 700i64);
    font_descriptor.set("StemV", 80i64);

    let descriptor_id = doc.add_object(Object::Dictionary(font_descriptor));
    cid_font.set("FontDescriptor", Object::Reference(descriptor_id));

    // Embed the font program
    let mut font_stream_dict = Dictionary::new();
    font_stream_dict.set("Length1", font_data.len() as i64);

    let font_stream = Stream::new(font_stream_dict, font_data.to_vec());
    let font_stream_id = doc.add_object(font_stream);

    // Set the font file in the descriptor
    if let Ok(descriptor) = doc.get_dictionary_mut(descriptor_id) {
        descriptor.set("FontFile2", Object::Reference(font_stream_id));
    }

    let cid_font_id = doc.add_object(Object::Dictionary(cid_font));

    // Create Type0 font dictionary
    let mut type0_font = Dictionary::new();
    type0_font.set("Type", "Font");
    type0_font.set("Subtype", "Type0");
    type0_font.set("BaseFont", font_name);
    type0_font.set("Encoding", "Identity-H"); // Use Identity-H encoding for UCS-2
    type0_font.set("DescendantFonts", vec![Object::Reference(cid_font_id)].into_iter().collect::<Vec<_>>());

    let type0_font_id = doc.add_object(Object::Dictionary(type0_font));

    // Return a name that won't have spaces (for use in content stream)
    let safe_font_name = font_name.replace(' ', "-");

    Ok((type0_font_id, safe_font_name))
}

/// Find a font file in the system
///
/// Searches common font directories for the given font name
#[allow(dead_code)]
pub fn find_system_font(font_name: &str) -> Option<String> {
    // Common font directories on different platforms
    let font_dirs: Vec<PathBuf> = if cfg!(target_os = "macos") {
        vec![
            PathBuf::from("/System/Library/Fonts"),
            PathBuf::from("/Library/Fonts"),
            PathBuf::from(std::env::var("HOME").ok()?).join("Library/Fonts"),
        ]
    } else if cfg!(target_os = "windows") {
        vec![
            PathBuf::from("C:\\Windows\\Fonts"),
        ]
    } else if cfg!(target_os = "linux") {
        vec![
            PathBuf::from("/usr/share/fonts"),
            PathBuf::from("/usr/local/share/fonts"),
            PathBuf::from(std::env::var("HOME").ok()?).join(".fonts"),
            PathBuf::from(std::env::var("HOME").ok()?).join(".local/share/fonts"),
        ]
    } else {
        return None;
    };

    // Common font file extensions
    let extensions = [".ttf", ".ttc", ".otf"];

    // Try to find the font file
    for font_dir in font_dirs {
        if !font_dir.exists() {
            continue;
        }

        // Try exact match with extensions
        for ext in &extensions {
            let font_path = font_dir.join(format!("{}{}", font_name, ext));
            if font_path.exists() {
                return font_path.to_str().map(String::from);
            }
        }

        // Try case-insensitive search
        if let Ok(entries) = fs::read_dir(&font_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|s| s.to_str()).map_or(false, |e| {
                    extensions.contains(&e.to_lowercase().as_str())
                }) {
                    let stem = path.file_stem()?.to_str()?;
                    if stem.eq_ignore_ascii_case(font_name) {
                        return path.to_str().map(String::from);
                    }
                }
            }
        }
    }

    None
}

/// Find a CID font that supports Unicode text
///
/// Searches for CJK fonts in the system that can render non-ASCII text
pub fn find_cid_font() -> Option<(Vec<u8>, String)> {
    let mut db = Database::new();

    // Load system fonts
    if cfg!(target_os = "macos") {
        db.load_system_fonts();
    } else if cfg!(target_os = "windows") {
        if let Ok(_) = std::env::var("WINDIR") {
            let font_dir = std::path::PathBuf::from("C:\\Windows\\Fonts");
            db.load_fonts_dir(font_dir);
        }
    } else if cfg!(target_os = "linux") {
        for path in &[
            "/usr/share/fonts",
            "/usr/local/share/fonts",
        ] {
            db.load_fonts_dir(std::path::PathBuf::from(path));
        }
        // Load user fonts
        if let Ok(home) = std::env::var("HOME") {
            for subpath in &[".fonts", ".local/share/fonts"] {
                let font_dir = std::path::PathBuf::from(&home).join(subpath);
                db.load_fonts_dir(font_dir);
            }
        }
    }

    // Common Japanese font family names to try
    let font_families = [
        "Hiragino Kaku Gothic Pro",
        "Hiragino Kaku Gothic ProN",
        "Hiragino Sans",
        "Hiragino Sans GB",
        "Hiragino Mincho ProN",
        "Noto Sans CJK JP",
        "Noto Sans JP",
        "Source Han Sans",
        "IPA Gothic",
        "IPA Mincho",
        "Yu Gothic",
        "Yu Mincho",
        "Meiryo",
        "MS Gothic",
        "MS Mincho",
    ];

    for family in &font_families {
        let family_ref = fontdb::Family::Name(family);
        let query = fontdb::Query {
            families: &[family_ref],
            ..Default::default()
        };

        if let Some(id) = db.query(&query) {
            if let Some((source, index)) = db.face_source(id) {
                match source {
                    fontdb::Source::File(path) => {
                        // Try to read the font file
                        if let Ok(data) = fs::read(&path) {
                            // For TTC files, we need to extract the correct font
                            if path.extension().and_then(|s| s.to_str()) == Some("ttc") {
                                // Try to find the correct font in the collection
                                if let Some(font_data) = extract_from_ttc(&data, index) {
                                    return Some((font_data, family.to_string()));
                                }
                            } else {
                                return Some((data, family.to_string()));
                            }
                        }
                    }
                    fontdb::Source::Binary(data) => {
                        // Convert Arc to Vec
                        let data_vec: Vec<u8> = data.as_ref().as_ref().to_vec();
                        return Some((data_vec, family.to_string()));
                    }
                    _ => {}
                }
            }
        }
    }

    None
}

/// Extract a single font from a TrueType Collection file
///
/// Returns the font data
fn extract_from_ttc(ttc_data: &[u8], index: u32) -> Option<Vec<u8>> {
    // Try to parse the font at the given index
    if let Ok(_face) = Face::parse(ttc_data, index) {
        // For TTC files, we can extract the specific font
        // However, for PDF embedding, we need the full TTC data
        // and specify the index in the font descriptor
        // For simplicity, return the whole TTC file
        return Some(ttc_data.to_vec());
    }

    None
}
