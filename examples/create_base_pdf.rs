use printpdf::{Mm, PdfDocument};
use std::io::BufWriter;

fn main() {
    let (mut doc, page1, layer1) = PdfDocument::new("QR Code Print Template", Mm(210.0), Mm(297.0), "Layer 1");
    let current_layer = doc.get_page(page1).get_layer(layer1);

    // Add built-in fonts to the document
    let font_bold_ref = doc.add_builtin_font(printpdf::BuiltinFont::HelveticaBold).unwrap();
    let font_ref = doc.add_builtin_font(printpdf::BuiltinFont::Helvetica).unwrap();

    // Add a title
    current_layer.use_text("QR Code Print Template", 24.0, Mm(50.0), Mm(250.0), &font_bold_ref);

    let text2 = "Place items below:";
    current_layer.use_text(text2, 14.0, Mm(50.0), Mm(230.0), &font_ref);

    let file = std::fs::File::create("base.pdf").unwrap();
    let mut writer = BufWriter::new(file);
    doc.save(&mut writer).unwrap();
    println!("Created base.pdf");
}
