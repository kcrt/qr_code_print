use lopdf::{Dictionary, Document, Object};

/// Update a page's resources dictionary with fonts and XObjects
///
/// This function handles the pattern of:
/// 1. Getting the existing font dictionary (if any)
/// 2. Adding our F1 font reference
/// 3. Merging any XObject resources
pub fn update_page_resources(
    doc: &mut Document,
    page_id: (u32, u16),
    font_id: (u32, u16),
    xobject_dict: &Dictionary,
) {
    // Get the page's resources
    let resources_id = doc.get_object(page_id)
        .ok()
        .and_then(|page| page.as_dict().ok())
        .and_then(|dict| dict.get(b"Resources").ok())
        .and_then(|r| r.as_reference().ok());

    if let Some(res_id) = resources_id {
        // Get the existing font dictionary first (before mutable borrow)
        let font_dict_to_clone = if let Ok(res) = doc.get_dictionary(res_id) {
            match res.get(b"Font") {
                Ok(Object::Reference(font_dict_id)) => {
                    doc.get_dictionary(*font_dict_id).cloned().ok()
                }
                Ok(Object::Dictionary(d)) => Some(d.clone()),
                _ => None,
            }
        } else {
            None
        };

        // Now modify the resources
        if let Ok(res) = doc.get_dictionary_mut(res_id) {
            let mut font_resources = font_dict_to_clone.unwrap_or_else(Dictionary::new);
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
}
