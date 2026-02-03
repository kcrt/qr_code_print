# QR Code Print

A cross-platform Rust application that generates PDFs with QR codes and text based on CSV data and placement specifications.

## Features

- Works on both macOS and Windows
- Reads CSV data from `data.csv`
- Reads placement configuration from `place.json`
- Uses `base.pdf` as a template
- Generates QR codes or places text at specified positions
- Outputs to `output.pdf` (one page per CSV row)

## Dependencies

- **serde** & **serde_json**: JSON parsing
- **csv**: CSV file reading
- **qrcode**: QR code generation
- **image**: Image processing (PNG encoding)
- **lopdf**: PDF manipulation
- **anyhow**: Error handling
- **flate2**: Data compression for PDF images

## File Format

### place.json

```json
{
  "fields": {
    "URL": {
      "x": 50,
      "y": 50,
      "w": 100,
      "h": 100,
      "type": "QR"
    },
    "ID": {
      "x": 200,
      "y": 50,
      "w": 150,
      "h": 30,
      "type": "Text"
    }
  }
}
```

### data.csv

```csv
URL,ID,Name
https://example.com/item001,A001,Item One
https://example.com/item002,B002,Item Two
```

## Building

```bash
cargo build --release
```

## Running

```bash
cargo run
```

The application requires `base.pdf`, `data.csv`, and `place.json` to be in the same directory.

## Creating a base PDF

A sample base PDF can be created using the provided example:

```bash
cargo run --example create_base_pdf
```

## Coordinates

- `x, y`: Position from top-left corner (in PDF points)
- `w, h`: Width and height (in PDF points)
- `type`: Either "QR" for QR codes or "Text" for text rendering
