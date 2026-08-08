English | [简体中文](README_ZH.md)

# PDF2Image

A cross-platform command-line utility that converts PDF pages to WebP, PNG, or JPG images. Release packages are self-contained single executables and do not require a separate installation of PDFium, Poppler, or libwebp.

## Downloads

| Platform | Architecture | Package | Hash |
| --- | --- | --- | --- |
| Linux | x86_64 | [Download](https://github.com/Joey-Kot/PDF2Image/releases/download/Latest/pdf2image-linux-x86_64.tar.gz) | [sha256](https://github.com/Joey-Kot/PDF2Image/releases/download/Latest/pdf2image-linux-x86_64.tar.gz.sha256) |
| Linux | arm64 | [Download](https://github.com/Joey-Kot/PDF2Image/releases/download/Latest/pdf2image-linux-arm64.tar.gz) | [sha256](https://github.com/Joey-Kot/PDF2Image/releases/download/Latest/pdf2image-linux-arm64.tar.gz.sha256) |
| Windows | x86_64 | [Download](https://github.com/Joey-Kot/PDF2Image/releases/download/Latest/pdf2image-windows-x86_64.zip) | [sha256](https://github.com/Joey-Kot/PDF2Image/releases/download/Latest/pdf2image-windows-x86_64.zip.sha256) |
| Windows | arm64 | [Download](https://github.com/Joey-Kot/PDF2Image/releases/download/Latest/pdf2image-windows-arm64.zip) | [sha256](https://github.com/Joey-Kot/PDF2Image/releases/download/Latest/pdf2image-windows-arm64.zip.sha256) |
| macOS | x86_64 | [Download](https://github.com/Joey-Kot/PDF2Image/releases/download/Latest/pdf2image-macos-x86_64.tar.gz) | [sha256](https://github.com/Joey-Kot/PDF2Image/releases/download/Latest/pdf2image-macos-x86_64.tar.gz.sha256) |
| macOS | arm64 | [Download](https://github.com/Joey-Kot/PDF2Image/releases/download/Latest/pdf2image-macos-arm64.tar.gz) | [sha256](https://github.com/Joey-Kot/PDF2Image/releases/download/Latest/pdf2image-macos-arm64.tar.gz.sha256) |

## Usage

```text
pdf2image --file <PDF> --ratio <MAX_WIDTH:MAX_HEIGHT> [--page <PAGES>] [--format <FORMAT>] [--quality <QUALITY>]
```

- `--file`: Required. The input PDF file.
- `--ratio`: Required. The maximum pixel width and height allowed for each page image. The accepted formats are `width:height`, `width*height`, and `width,height`, for example `2048:1024`. The program preserves the page's aspect ratio and scales it to fit within these bounds.
- `--page`: Optional. The page numbers to export. Single-page lists, continuous ranges, and combinations of both are supported, for example `'1,3,5'`, `'1-5'`, or `'1-3,5'`. Page numbering starts at `1`; all pages are exported when this option is omitted.
- `--format`: Optional. Supported formats are `webp`, `png`, and `jpg`. The default is `webp`; `jpeg` is accepted as an alias for `jpg`.
- `--quality`: Optional. A value from `0.0` to `1.0`, with a default of `0.85`. This option controls the lossy encoding quality for WebP and JPG. PNG is lossless, so this option does not affect PNG image quality.

Examples:

```bash
pdf2image --file document.pdf --ratio '2048:1024'
pdf2image --file document.pdf --ratio '2048*2048' --page '1,3,5' --format png
pdf2image --file document.pdf --ratio '2048:2048' --page '1-5' --format png
pdf2image --file document.pdf --ratio '1920,1080' --page '1-3,5' --format jpg --quality 0.9
```

Run `pdf2image --help` to view the command-line help.

## Output Rules

For an input file at `/path/document.pdf`, the program creates `/path/document/` in the same directory and writes one image per page:

```text
document/
├── 001.webp
├── 002.webp
└── 003.webp
```

File names are padded to at least three digits and automatically use more digits for documents with more than 999 pages. Transparent PDF backgrounds are composited onto a white background.

When `--page` selects only some pages, output file names retain the original PDF page numbers. For example, `--page '1,3,5'` produces `001.webp`, `003.webp`, and `005.webp`.

If an output directory with the same name already exists and is not empty, the program reports an error and stops to avoid overwriting existing files. An existing empty directory can be reused.

## Building Locally

Rust `1.92` or later is required:

```bash
cargo build --release
```

The resulting executable is located at `target/release/pdf2image`, or `target/release/pdf2image.exe` on Windows.
