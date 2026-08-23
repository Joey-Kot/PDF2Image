[English](README.md) | 简体中文

# PDF2Image

一个跨平台命令行程序，将 PDF 页面转换为 WebP、PNG 或 JPG 图像。发布包为自包含的单个可执行程序，不需要另外安装 PDFium、Poppler 或 libwebp。

## 下载

| Platform | Architecture | Package | Hash |
| --- | --- | --- | --- |
| Linux | x86_64 | [Download](https://github.com/Joey-Kot/PDF2Image/releases/download/Latest/pdf2image-linux-x86_64.tar.gz) | [sha256](https://github.com/Joey-Kot/PDF2Image/releases/download/Latest/pdf2image-linux-x86_64.tar.gz.sha256) |
| Linux | arm64 | [Download](https://github.com/Joey-Kot/PDF2Image/releases/download/Latest/pdf2image-linux-arm64.tar.gz) | [sha256](https://github.com/Joey-Kot/PDF2Image/releases/download/Latest/pdf2image-linux-arm64.tar.gz.sha256) |
| Windows | x86_64 | [Download](https://github.com/Joey-Kot/PDF2Image/releases/download/Latest/pdf2image-windows-x86_64.zip) | [sha256](https://github.com/Joey-Kot/PDF2Image/releases/download/Latest/pdf2image-windows-x86_64.zip.sha256) |
| Windows | arm64 | [Download](https://github.com/Joey-Kot/PDF2Image/releases/download/Latest/pdf2image-windows-arm64.zip) | [sha256](https://github.com/Joey-Kot/PDF2Image/releases/download/Latest/pdf2image-windows-arm64.zip.sha256) |
| macOS | x86_64 | [Download](https://github.com/Joey-Kot/PDF2Image/releases/download/Latest/pdf2image-macos-x86_64.tar.gz) | [sha256](https://github.com/Joey-Kot/PDF2Image/releases/download/Latest/pdf2image-macos-x86_64.tar.gz.sha256) |
| macOS | arm64 | [Download](https://github.com/Joey-Kot/PDF2Image/releases/download/Latest/pdf2image-macos-arm64.tar.gz) | [sha256](https://github.com/Joey-Kot/PDF2Image/releases/download/Latest/pdf2image-macos-arm64.tar.gz.sha256) |

## 使用方法

```text
pdf2image --file <PDF> --ratio <最大宽:最大高> [--page <页码>] [--text] [--format <格式>] [--quality <质量>]
```

- `--file`：必填，输入 PDF 文件。
- `--ratio`：必填，每页图像允许的最大像素宽高，支持 `宽:高`、`宽*高` 和 `宽,高` 三种格式，例如 `2048:1024`。程序保持页面宽高比，并缩放到该边界内。
- `--page`：可选，要导出的页码，支持单页列表、连续范围及两者组合，例如 `'1,3,5'`、`'1-5'` 或 `'1-3,5'`。页码从 `1` 开始；不指定时导出全部页面。
- `--text`：可选，同时将所选页面的文本层导出为文本文件，文件位于输出目录内，名称与输入 PDF 同名、扩展名为 `.txt`；每页内容由页码标题分隔。
- `--format`：可选，支持 `webp`、`png`、`jpg`，默认为 `webp`。`jpeg` 可作为 `jpg` 的别名。
- `--quality`：可选，范围为 `0.0` 到 `1.0`，默认为 `0.85`。该参数控制 WebP 和 JPG 的有损编码质量；PNG 为无损格式，因此不改变 PNG 画质。

示例：

```bash
pdf2image --file document.pdf --ratio '2048:1024'
pdf2image --file document.pdf --ratio '2048*2048' --page '1,3,5' --format png
pdf2image --file document.pdf --ratio '2048:2048' --page '1-5' --format png
pdf2image --file document.pdf --ratio '1920,1080' --page '1-3,5' --format jpg --quality 0.9
pdf2image --file document.pdf --ratio '2048:2048' --page '1,3,10' --text
```

也可以运行 `pdf2image --help` 查看帮助。

## 输出规则

输入文件为 `/path/document.pdf` 时，程序会在原目录创建 `/path/document/`，并按页输出：

```text
document/
├── 001.webp
├── 002.webp
├── 003.webp
└── document.txt  # 仅在指定 --text 时生成
```

文件名至少补足三位；超过 999 页时会自动增加位数。PDF 透明背景会合成到白色背景上。

使用 `--page` 选择部分页面时，输出文件名保留原 PDF 页码。例如 `--page '1,3,5'` 会生成 `001.webp`、`003.webp` 和 `005.webp`。

指定 `--text` 后，文本文件只包含 `--page` 最终选中的页面，并按页码顺序排列。例如选择 `1,3,10` 页时，文件中依次含有 `--------- Page 1 ---------`、`--------- Page 3 ---------` 和 `--------- Page 10 ---------` 标题。此功能提取 PDF 已有的文本层，不进行 OCR；扫描件若没有文本层，对应页面内容会为空。

如果同名输出文件夹已经存在且非空，程序会报错并停止，避免覆盖已有文件。已经存在的空文件夹可以继续使用。

## 本地编译

需要 Rust `1.92` 或更高版本：

```bash
cargo build --release
```

编译结果位于 `target/release/pdf2image`；Windows 下为 `target/release/pdf2image.exe`。
