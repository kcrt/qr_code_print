use anyhow::{anyhow, Context, Result};
use lopdf::{Dictionary, Document, Object};
use crate::config::{DataRow, PlaceConfig};
use super::content::ContentBuilder;
use super::resources::update_page_resources_with_fonts;
use super::fonts::{create_font, StandardFont, find_cid_font, embed_cid_font};

/// Check if the text requires CID font (non-ASCII characters)
fn needs_cid_font(text: &str) -> bool {
    // Check for any characters outside ASCII range
    text.chars().any(|c| c > '\u{7F}')
}

/// Collect all text from data rows to check if CID font is needed
fn should_use_cid_font(data_rows: &[DataRow], config: &PlaceConfig) -> bool {
    for row in data_rows {
        for (field_name, _field_spec) in &config.fields {
            if let Some(value) = row.data.get(field_name) {
                if needs_cid_font(value) {
                    return true;
                }
            }
        }
    }
    false
}

/// Font references for use in content generation
struct FontRefs {
    regular_id: (u32, u16),
    regular_name: String,
    cid_id: Option<(u32, u16)>,
    cid_name: Option<String>,
}

/// Create a single page with content for a given data row
fn create_page_for_row(
    output_doc: &mut Document,
    base_page: &Dictionary,
    row: &DataRow,
    config: &PlaceConfig,
    page_height: f64,
    fonts: &FontRefs,
) -> Result<(u32, u16)> {
    // Clone the base page for this row
    let page_dict = base_page.clone();

    // Add the cloned page to the document
    let page_id = output_doc.add_object(Object::Dictionary(page_dict));

    // Build overlay content for this row
    let mut builder = if let Some(ref cid_name) = fonts.cid_name {
        ContentBuilder::new_with_cid_font(fonts.regular_name.clone(), cid_name.clone())
    } else {
        ContentBuilder::new(fonts.regular_name.clone())
    };

    for (field_name, field_spec) in &config.fields {
        let value = row.data.get(field_name).map(|s| s.as_str()).unwrap_or("");
        builder.add_field(field_name, value, field_spec, page_height, output_doc)?;
    }

    // Append overlay content to the cloned page
    let overlay_bytes = builder.build_content_bytes();
    output_doc.add_page_contents(page_id, overlay_bytes)?;

    // Update the page's resources with fonts and XObjects
    update_page_resources_with_fonts(
        output_doc,
        page_id,
        fonts.regular_id,
        &fonts.regular_name,
        fonts.cid_id,
        fonts.cid_name.as_deref(),
        &builder.xobjects,
    );

    Ok(page_id)
}

/// Update the Pages dictionary to include all new pages in the Kids array
fn update_pages_dictionary(doc: &mut Document, additional_page_ids: &[(u32, u16)]) -> Result<()> {
    if additional_page_ids.is_empty() {
        return Ok(());
    }

    let pages = doc.catalog()?.get(b"Pages")
        .with_context(|| "Failed to get Pages from catalog")?;

    let pages_id = pages.as_reference()
        .with_context(|| "Pages is not a reference")?;

    // Get current kids array before mutable borrow
    let current_kids = doc.get_dictionary(pages_id)
        .ok()
        .and_then(|d| d.get(b"Kids").ok())
        .and_then(|k| k.as_array().ok())
        .cloned()
        .unwrap_or_default();

    let mut new_kids = current_kids;
    for page_id in additional_page_ids {
        new_kids.push(Object::Reference(*page_id));
    }
    let new_count = new_kids.len();

    if let Ok(pages_mut) = doc.get_dictionary_mut(pages_id) {
        pages_mut.set("Kids", new_kids);
        pages_mut.set("Count", new_count as i64);
    }

    Ok(())
}

/// Create the output PDF with all data rows
pub fn create_output_pdf(
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

    // Determine the fonts to use
    // Always create a regular font for ASCII text
    let regular_font = StandardFont::from_name("Helvetica").unwrap_or(StandardFont::Helvetica);
    let (regular_font_id, regular_font_name) = create_font(&mut output_doc, regular_font)?;

    // Create a CID font if non-ASCII text is detected
    let (cid_font_id, cid_font_name) = if should_use_cid_font(data_rows, config) {
        if let Some((font_data, font_name)) = find_cid_font() {
            let (fid, fname) = embed_cid_font(&mut output_doc, &font_data, &font_name)
                .with_context(|| "Failed to embed CID font")?;
            (Some(fid), Some(fname))
        } else {
            (None, None)
        }
    } else {
        (None, None)
    };

    let fonts = FontRefs {
        regular_id: regular_font_id,
        regular_name: regular_font_name,
        cid_id: cid_font_id,
        cid_name: cid_font_name,
    };

    // Create additional pages for each row (beyond the first)
    let mut additional_page_ids = Vec::new();

    for row in data_rows.iter().skip(1) {
        let page_id = create_page_for_row(
            &mut output_doc,
            base_page,
            row,
            config,
            page_height,
            &fonts,
        )?;
        additional_page_ids.push(page_id);
    }

    // Add content to the first page (base page) for the first row
    if let Some(first_row) = data_rows.first() {
        let mut builder = if let Some(ref cid_name) = fonts.cid_name {
            ContentBuilder::new_with_cid_font(fonts.regular_name.clone(), cid_name.clone())
        } else {
            ContentBuilder::new(fonts.regular_name.clone())
        };

        for (field_name, field_spec) in &config.fields {
            let value = first_row.data.get(field_name).map(|s| s.as_str()).unwrap_or("");
            builder.add_field(field_name, value, field_spec, page_height, &mut output_doc)?;
        }

        // Append new content to the base page
        let new_content = builder.build_content_bytes();

        // Add new content to the first page
        let first_page_id = *output_doc.get_pages().values().next()
            .ok_or_else(|| anyhow!("No pages"))?;

        output_doc.add_page_contents(first_page_id, new_content)?;

        // Update the first page's resources
        update_page_resources_with_fonts(
            &mut output_doc,
            first_page_id,
            fonts.regular_id,
            &fonts.regular_name,
            fonts.cid_id,
            fonts.cid_name.as_deref(),
            &builder.xobjects,
        );
    }

    // Update the pages dictionary to include all new pages
    update_pages_dictionary(&mut output_doc, &additional_page_ids)?;

    Ok(output_doc)
}
