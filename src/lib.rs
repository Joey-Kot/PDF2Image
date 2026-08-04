use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, ensure};
use clap::{Parser, ValueEnum};
use hayro::hayro_interpret::InterpreterSettings;
use hayro::hayro_syntax::Pdf;
use hayro::vello_cpu::Pixmap;
use hayro::vello_cpu::color::palette::css::WHITE;
use hayro::{RenderCache, RenderSettings, render};
use image::codecs::jpeg::JpegEncoder;
use image::codecs::png::{CompressionType, FilterType, PngEncoder};
use image::{ExtendedColorType, ImageEncoder};
use webp::Encoder as WebpEncoder;

const DEFAULT_QUALITY: f32 = 0.85;
const MIN_PADDING_WIDTH: usize = 3;
const MAX_RENDER_DIMENSION: u32 = u16::MAX as u32;
const MAX_WEBP_DIMENSION: u16 = 16_383;

/// Convert PDF pages into images.
#[derive(Debug, Parser)]
#[command(
    name = "pdf2image",
    version,
    about = "将 PDF 页面转换为 WebP、PNG 或 JPG 图像",
    long_about = None,
    after_help = "用法示例：\n  pdf2image --file document.pdf --ratio '2048:1024'\n  pdf2image --file document.pdf --ratio '2048*2048' --page '1,3,5' --format png\n  pdf2image --file document.pdf --ratio '2048:2048' --page '1-5' --format png\n  pdf2image --file document.pdf --ratio '1920,1080' --page '1-3,5' --format jpg --quality 0.9"
)]
struct Cli {
    /// 要转换的 PDF 文件
    #[arg(long, value_name = "PDF")]
    file: PathBuf,

    /// 输出图像的最大宽高，例如 '2048:1024'、'2048*1024' 或 '2048,1024'
    #[arg(long, value_name = "最大宽:最大高", value_parser = parse_max_size)]
    ratio: MaxSize,

    /// 要导出的页码，例如 '1,3,5'、'1-5' 或 '1-3,5'；默认导出全部页面
    #[arg(long, value_name = "页码", value_parser = parse_page_selection)]
    page: Option<PageSelection>,

    /// 输出格式
    #[arg(
        long,
        value_name = "格式",
        value_enum,
        ignore_case = true,
        default_value = "webp"
    )]
    format: OutputFormat,

    /// 图像质量，范围为 0.0 到 1.0；PNG 为无损格式，该参数不改变画质
    #[arg(long, value_name = "质量", default_value_t = DEFAULT_QUALITY, value_parser = parse_quality)]
    quality: f32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum OutputFormat {
    Webp,
    Png,
    #[value(alias = "jpeg")]
    Jpg,
}

impl OutputFormat {
    const fn extension(self) -> &'static str {
        match self {
            Self::Webp => "webp",
            Self::Png => "png",
            Self::Jpg => "jpg",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct MaxSize {
    width: u16,
    height: u16,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct PageSelection {
    ranges: Vec<PageRange>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PageRange {
    start: usize,
    end: usize,
}

#[derive(Clone, Copy, Debug)]
struct PageRenderPlan {
    width: u16,
    height: u16,
    scale: f32,
}

/// Parse command-line arguments and run the conversion.
pub fn run() -> Result<()> {
    convert(Cli::parse())
}

fn convert(cli: Cli) -> Result<()> {
    validate_pdf_path(&cli.file)?;

    let pdf_bytes =
        fs::read(&cli.file).with_context(|| format!("无法读取 PDF 文件 {}", cli.file.display()))?;
    let pdf = Pdf::new(pdf_bytes)
        .map_err(|error| anyhow::anyhow!("无法解析 PDF 文件 {}：{error:?}", cli.file.display()))?;

    let page_count = pdf.pages().len();
    ensure!(page_count > 0, "PDF 不包含可转换的页面");
    let selected_pages = resolve_page_selection(cli.page.as_ref(), page_count)?;
    let selected_page_count = selected_pages.len();

    let plans = selected_pages
        .iter()
        .map(|&page_number| {
            let page = &pdf.pages()[page_number - 1];
            let (page_width, page_height) = page.render_dimensions();
            let plan = fit_dimensions(page_width, page_height, cli.ratio)
                .with_context(|| format!("无法计算第 {page_number} 页的输出尺寸"))?;
            validate_format_dimensions(cli.format, plan.width, plan.height)
                .with_context(|| format!("第 {page_number} 页的输出尺寸不受所选格式支持"))?;
            Ok(plan)
        })
        .collect::<Result<Vec<_>>>()?;

    let output_dir = output_directory(&cli.file)?;
    prepare_output_directory(&output_dir)?;

    println!(
        "开始转换：{} 页，格式 {}，输出目录 {}",
        selected_page_count,
        cli.format.extension(),
        output_dir.display()
    );

    let cache = RenderCache::new();
    let interpreter_settings = InterpreterSettings::default();
    let padding_width = sequence_padding_width(page_count);

    for (index, (&page_number, plan)) in selected_pages.iter().zip(plans).enumerate() {
        let page = &pdf.pages()[page_number - 1];
        let render_settings = RenderSettings {
            x_scale: plan.scale,
            y_scale: plan.scale,
            width: Some(plan.width),
            height: Some(plan.height),
            bg_color: WHITE,
        };
        let pixmap = render(page, &cache, &interpreter_settings, &render_settings);
        let encoded = encode_pixmap(&pixmap, cli.format, cli.quality)
            .with_context(|| format!("第 {page_number} 页图像编码失败"))?;
        let file_name = sequence_file_name(page_number, padding_width, cli.format.extension());
        let output_path = output_dir.join(file_name);
        fs::write(&output_path, encoded)
            .with_context(|| format!("无法写入图像 {}", output_path.display()))?;

        println!(
            "[{}/{}] {}（{}x{}）",
            index + 1,
            selected_page_count,
            output_path.display(),
            plan.width,
            plan.height
        );
    }

    println!("转换完成：{}", output_dir.display());
    Ok(())
}

fn validate_pdf_path(path: &Path) -> Result<()> {
    ensure!(
        path.is_file(),
        "PDF 文件不存在或不是普通文件：{}",
        path.display()
    );
    let is_pdf = path
        .extension()
        .and_then(OsStr::to_str)
        .is_some_and(|extension| extension.eq_ignore_ascii_case("pdf"));
    ensure!(is_pdf, "输入文件必须使用 .pdf 扩展名：{}", path.display());
    Ok(())
}

fn output_directory(pdf_path: &Path) -> Result<PathBuf> {
    let stem = pdf_path
        .file_stem()
        .context("无法从 PDF 文件名确定输出目录名称")?;
    let parent = pdf_path.parent().unwrap_or_else(|| Path::new("."));
    Ok(parent.join(stem))
}

fn prepare_output_directory(path: &Path) -> Result<()> {
    match fs::metadata(path) {
        Ok(metadata) => {
            ensure!(
                metadata.is_dir(),
                "输出路径已存在但不是文件夹：{}",
                path.display()
            );
            let mut entries = fs::read_dir(path)
                .with_context(|| format!("无法检查输出文件夹 {}", path.display()))?;
            ensure!(
                entries.next().transpose()?.is_none(),
                "同名输出文件夹已存在且非空，已停止以避免覆盖：{}",
                path.display()
            );
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            fs::create_dir(path)
                .with_context(|| format!("无法创建输出文件夹 {}", path.display()))?;
        }
        Err(error) => {
            return Err(error).with_context(|| format!("无法访问输出路径 {}", path.display()));
        }
    }

    Ok(())
}

fn parse_max_size(value: &str) -> std::result::Result<MaxSize, String> {
    let (width, height) = value.split_once([':', '*', ',']).ok_or_else(|| {
        "最大宽高必须使用“宽:高”、“宽*高”或“宽,高”格式，例如 2048:1024".to_owned()
    })?;
    let width = parse_dimension(width, "宽")?;
    let height = parse_dimension(height, "高")?;
    Ok(MaxSize { width, height })
}

fn parse_page_selection(value: &str) -> std::result::Result<PageSelection, String> {
    if value.trim().is_empty() {
        return Err("页码不能为空".to_owned());
    }

    let mut ranges = Vec::new();
    for item in value.split(',') {
        let item = item.trim();
        if item.is_empty() {
            return Err("页码必须使用“1,3,5”、“1-5”或“1-3,5”格式".to_owned());
        }

        let range = if let Some((start, end)) = item.split_once('-') {
            if end.contains('-') {
                return Err(format!("页码范围格式无效：{item}"));
            }
            let start = parse_page_number(start.trim())?;
            let end = parse_page_number(end.trim())?;
            if start > end {
                return Err(format!("页码范围起始值不能大于结束值：{item}"));
            }
            PageRange { start, end }
        } else {
            let page = parse_page_number(item)?;
            PageRange {
                start: page,
                end: page,
            }
        };
        ranges.push(range);
    }

    Ok(PageSelection { ranges })
}

fn parse_page_number(value: &str) -> std::result::Result<usize, String> {
    let page = value
        .parse::<usize>()
        .map_err(|_| format!("页码必须是正整数：{value}"))?;
    if page == 0 {
        return Err("页码必须大于 0".to_owned());
    }
    Ok(page)
}

fn resolve_page_selection(
    selection: Option<&PageSelection>,
    page_count: usize,
) -> Result<Vec<usize>> {
    let Some(selection) = selection else {
        return Ok((1..=page_count).collect());
    };

    let mut selected = vec![false; page_count];
    for range in &selection.ranges {
        ensure!(
            range.end <= page_count,
            "所选页码 {} 超出 PDF 总页数 {}",
            range.end,
            page_count
        );
        for page_number in range.start..=range.end {
            selected[page_number - 1] = true;
        }
    }

    Ok(selected
        .into_iter()
        .enumerate()
        .filter_map(|(index, is_selected)| is_selected.then_some(index + 1))
        .collect())
}

fn parse_dimension(value: &str, name: &str) -> std::result::Result<u16, String> {
    let parsed = value
        .parse::<u32>()
        .map_err(|_| format!("最大{name}必须是正整数"))?;
    if parsed == 0 || parsed > MAX_RENDER_DIMENSION {
        return Err(format!("最大{name}必须在 1 到 {MAX_RENDER_DIMENSION} 之间"));
    }
    Ok(parsed as u16)
}

fn parse_quality(value: &str) -> std::result::Result<f32, String> {
    let quality = value
        .parse::<f32>()
        .map_err(|_| "质量必须是 0.0 到 1.0 之间的数字".to_owned())?;
    if !quality.is_finite() || !(0.0..=1.0).contains(&quality) {
        return Err("质量必须在 0.0 到 1.0 之间".to_owned());
    }
    Ok(quality)
}

fn fit_dimensions(page_width: f32, page_height: f32, max_size: MaxSize) -> Result<PageRenderPlan> {
    ensure!(
        page_width.is_finite() && page_height.is_finite() && page_width > 0.0 && page_height > 0.0,
        "PDF 页面尺寸无效：{page_width}x{page_height}"
    );

    let width_scale = f64::from(max_size.width) / f64::from(page_width);
    let height_scale = f64::from(max_size.height) / f64::from(page_height);
    let scale = width_scale.min(height_scale);
    ensure!(scale.is_finite() && scale > 0.0, "计算得到的缩放比例无效");

    let width = (f64::from(page_width) * scale)
        .round()
        .clamp(1.0, f64::from(max_size.width)) as u16;
    let height = (f64::from(page_height) * scale)
        .round()
        .clamp(1.0, f64::from(max_size.height)) as u16;

    Ok(PageRenderPlan {
        width,
        height,
        scale: scale as f32,
    })
}

fn validate_format_dimensions(format: OutputFormat, width: u16, height: u16) -> Result<()> {
    if format == OutputFormat::Webp {
        ensure!(
            width <= MAX_WEBP_DIMENSION && height <= MAX_WEBP_DIMENSION,
            "WebP 的单边尺寸不能超过 {MAX_WEBP_DIMENSION} 像素，当前为 {width}x{height}"
        );
    }
    Ok(())
}

fn encode_pixmap(pixmap: &Pixmap, format: OutputFormat, quality: f32) -> Result<Vec<u8>> {
    let width = u32::from(pixmap.width());
    let height = u32::from(pixmap.height());
    let rgba = pixmap.data_as_u8_slice();

    match format {
        OutputFormat::Webp => encode_webp(rgba, width, height, quality),
        OutputFormat::Png => encode_png(rgba, width, height),
        OutputFormat::Jpg => encode_jpeg(rgba, width, height, quality),
    }
}

fn encode_webp(rgba: &[u8], width: u32, height: u32, quality: f32) -> Result<Vec<u8>> {
    let encoded = WebpEncoder::from_rgba(rgba, width, height)
        .encode_simple(false, quality * 100.0)
        .map_err(|error| anyhow::anyhow!("WebP 编码器返回错误：{error:?}"))?;
    Ok(encoded.to_vec())
}

fn encode_png(rgba: &[u8], width: u32, height: u32) -> Result<Vec<u8>> {
    let mut output = Vec::new();
    PngEncoder::new_with_quality(&mut output, CompressionType::Default, FilterType::Adaptive)
        .write_image(rgba, width, height, ExtendedColorType::Rgba8)
        .context("PNG 编码器返回错误")?;
    Ok(output)
}

fn encode_jpeg(rgba: &[u8], width: u32, height: u32, quality: f32) -> Result<Vec<u8>> {
    let mut rgb = Vec::with_capacity((width as usize) * (height as usize) * 3);
    for pixel in rgba.chunks_exact(4) {
        rgb.extend_from_slice(&pixel[..3]);
    }

    let jpeg_quality = ((quality * 100.0).round() as u8).clamp(1, 100);
    let mut output = Vec::new();
    JpegEncoder::new_with_quality(&mut output, jpeg_quality)
        .write_image(&rgb, width, height, ExtendedColorType::Rgb8)
        .context("JPG 编码器返回错误")?;
    Ok(output)
}

fn sequence_padding_width(page_count: usize) -> usize {
    page_count.to_string().len().max(MIN_PADDING_WIDTH)
}

fn sequence_file_name(index: usize, padding_width: usize, extension: &str) -> String {
    format!("{index:0padding_width$}.{extension}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;
    use std::fmt::Write as _;

    #[test]
    fn parses_max_size() {
        let expected = MaxSize {
            width: 2048,
            height: 1024,
        };
        assert_eq!(parse_max_size("2048:1024").unwrap(), expected);
        assert_eq!(parse_max_size("2048*1024").unwrap(), expected);
        assert_eq!(parse_max_size("2048,1024").unwrap(), expected);
        assert!(parse_max_size("2048x1024").is_err());
        assert!(parse_max_size("2048:1024,768").is_err());
        assert!(parse_max_size("0:1024").is_err());
        assert!(parse_max_size("65536:1024").is_err());
    }

    #[test]
    fn parses_quality() {
        assert_eq!(parse_quality("0.85").unwrap(), 0.85);
        assert_eq!(parse_quality("0").unwrap(), 0.0);
        assert_eq!(parse_quality("1").unwrap(), 1.0);
        assert!(parse_quality("-0.1").is_err());
        assert!(parse_quality("1.01").is_err());
        assert!(parse_quality("NaN").is_err());
    }

    #[test]
    fn parses_and_resolves_page_selection() {
        let list = parse_page_selection("1,3,5").unwrap();
        assert_eq!(resolve_page_selection(Some(&list), 6).unwrap(), [1, 3, 5]);

        let range = parse_page_selection("1-5").unwrap();
        assert_eq!(
            resolve_page_selection(Some(&range), 6).unwrap(),
            [1, 2, 3, 4, 5]
        );

        let combined = parse_page_selection("1-3,5").unwrap();
        assert_eq!(
            resolve_page_selection(Some(&combined), 6).unwrap(),
            [1, 2, 3, 5]
        );

        let overlapping = parse_page_selection("3-5,1-3").unwrap();
        assert_eq!(
            resolve_page_selection(Some(&overlapping), 6).unwrap(),
            [1, 2, 3, 4, 5]
        );
        assert_eq!(resolve_page_selection(None, 3).unwrap(), [1, 2, 3]);
    }

    #[test]
    fn rejects_invalid_page_selection() {
        assert!(parse_page_selection("").is_err());
        assert!(parse_page_selection("0").is_err());
        assert!(parse_page_selection("1,,3").is_err());
        assert!(parse_page_selection("5-3").is_err());
        assert!(parse_page_selection("1-3-5").is_err());
        assert!(parse_page_selection("page").is_err());

        let selection = parse_page_selection("1-7").unwrap();
        let error = resolve_page_selection(Some(&selection), 6).unwrap_err();
        assert!(error.to_string().contains("超出 PDF 总页数 6"));
    }

    #[test]
    fn command_line_defaults_to_webp_and_point_eighty_five_quality() {
        let cli = Cli::try_parse_from([
            "pdf2image",
            "--file",
            "document.pdf",
            "--ratio",
            "2048:1024",
        ])
        .unwrap();
        assert_eq!(cli.format, OutputFormat::Webp);
        assert_eq!(cli.quality, DEFAULT_QUALITY);
        assert!(cli.page.is_none());
    }

    #[test]
    fn help_includes_quoted_ratio_and_page_examples() {
        let help = Cli::command().render_help().to_string();
        assert!(help.contains("--ratio '2048:1024'"));
        assert!(help.contains("--ratio '2048*2048'"));
        assert!(help.contains("--ratio '1920,1080'"));
        assert!(help.contains("--page '1,3,5'"));
        assert!(help.contains("--page '1-5'"));
        assert!(help.contains("--page '1-3,5'"));
    }

    #[test]
    fn fits_page_inside_bounds_without_changing_aspect_ratio() {
        let portrait = fit_dimensions(
            612.0,
            792.0,
            MaxSize {
                width: 2048,
                height: 1024,
            },
        )
        .unwrap();
        assert_eq!((portrait.width, portrait.height), (791, 1024));

        let landscape = fit_dimensions(
            1600.0,
            900.0,
            MaxSize {
                width: 1000,
                height: 1000,
            },
        )
        .unwrap();
        assert_eq!((landscape.width, landscape.height), (1000, 563));

        let a4 = fit_dimensions(
            612.0,
            792.0,
            MaxSize {
                width: 2000,
                height: 2000,
            },
        )
        .unwrap();
        assert_eq!((a4.width, a4.height), (1545, 2000));
    }

    #[test]
    fn creates_zero_padded_names() {
        assert_eq!(sequence_padding_width(7), 3);
        assert_eq!(sequence_padding_width(1_234), 4);
        assert_eq!(sequence_file_name(1, 3, "webp"), "001.webp");
        assert_eq!(sequence_file_name(42, 4, "jpg"), "0042.jpg");
    }

    #[test]
    fn rejects_non_empty_output_directory() {
        let temp = tempfile::tempdir().unwrap();
        let output = temp.path().join("document");
        fs::create_dir(&output).unwrap();
        fs::write(output.join("existing.txt"), b"keep me").unwrap();

        let error = prepare_output_directory(&output).unwrap_err();
        assert!(error.to_string().contains("非空"));
    }

    #[test]
    fn encodes_png_and_jpeg() {
        let mut pixmap = Pixmap::new(2, 3);
        for pixel in pixmap.data_as_u8_slice_mut().chunks_exact_mut(4) {
            pixel.copy_from_slice(&[240, 128, 64, 255]);
        }

        let png = encode_pixmap(&pixmap, OutputFormat::Png, DEFAULT_QUALITY).unwrap();
        assert!(png.starts_with(b"\x89PNG\r\n\x1a\n"));
        let decoded_png =
            image::load_from_memory_with_format(&png, image::ImageFormat::Png).unwrap();
        assert_eq!((decoded_png.width(), decoded_png.height()), (2, 3));

        let jpeg = encode_pixmap(&pixmap, OutputFormat::Jpg, DEFAULT_QUALITY).unwrap();
        assert!(jpeg.starts_with(&[0xff, 0xd8]));
        let decoded_jpeg =
            image::load_from_memory_with_format(&jpeg, image::ImageFormat::Jpeg).unwrap();
        assert_eq!((decoded_jpeg.width(), decoded_jpeg.height()), (2, 3));
    }

    #[test]
    fn converts_a_minimal_pdf_to_webp() {
        let temp = tempfile::tempdir().unwrap();
        let pdf_path = temp.path().join("sample.pdf");
        fs::write(&pdf_path, minimal_pdf(100, 200)).unwrap();

        convert(Cli {
            file: pdf_path,
            ratio: MaxSize {
                width: 100,
                height: 100,
            },
            page: None,
            format: OutputFormat::Webp,
            quality: DEFAULT_QUALITY,
        })
        .unwrap();

        let image_path = temp.path().join("sample").join("001.webp");
        let bytes = fs::read(image_path).unwrap();
        let image = image::load_from_memory_with_format(&bytes, image::ImageFormat::WebP)
            .unwrap()
            .into_rgba8();
        assert_eq!(image.dimensions(), (50, 100));
        let center = image.get_pixel(25, 50).0;
        let center_rgb = &center[..3];
        assert!(
            center_rgb[0] > 200 && center_rgb[1] < 80 && center_rgb[2] < 80,
            "expected a red center pixel, got {center_rgb:?}"
        );
    }

    #[test]
    fn converts_only_selected_pages_and_preserves_page_numbers() {
        let temp = tempfile::tempdir().unwrap();
        let pdf_path = temp.path().join("selected.pdf");
        fs::write(
            &pdf_path,
            minimal_pdf_with_pages(&[(100, 100), (100, 100), (100, 100), (100, 100), (100, 100)]),
        )
        .unwrap();

        convert(Cli {
            file: pdf_path,
            ratio: MaxSize {
                width: 100,
                height: 100,
            },
            page: Some(parse_page_selection("1,3,5").unwrap()),
            format: OutputFormat::Webp,
            quality: DEFAULT_QUALITY,
        })
        .unwrap();

        let output_dir = temp.path().join("selected");
        let mut file_names = fs::read_dir(output_dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect::<Vec<_>>();
        file_names.sort();
        assert_eq!(file_names, ["001.webp", "003.webp", "005.webp"]);
    }

    fn minimal_pdf(width: u32, height: u32) -> Vec<u8> {
        minimal_pdf_with_pages(&[(width, height)])
    }

    fn minimal_pdf_with_pages(dimensions: &[(u32, u32)]) -> Vec<u8> {
        assert!(!dimensions.is_empty());

        let first_page_object = 3;
        let first_content_object = first_page_object + dimensions.len();
        let kids = (0..dimensions.len())
            .map(|index| format!("{} 0 R", first_page_object + index))
            .collect::<Vec<_>>()
            .join(" ");
        let mut objects = Vec::with_capacity(2 + dimensions.len() * 2);
        objects.push("<< /Type /Catalog /Pages 2 0 R >>".to_owned());
        objects.push(format!(
            "<< /Type /Pages /Kids [{kids}] /Count {} >>",
            dimensions.len()
        ));

        for (index, &(width, height)) in dimensions.iter().enumerate() {
            let content_object = first_content_object + index;
            objects.push(format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {width} {height}] /Contents {content_object} 0 R >>"
            ));
        }

        for &(width, height) in dimensions {
            let content = format!("q\n1 0 0 rg\n0 0 {width} {height} re\nf\nQ\n");
            objects.push(format!(
                "<< /Length {} >>\nstream\n{}endstream",
                content.len(),
                content
            ));
        }

        let mut pdf = String::from("%PDF-1.4\n");
        let mut offsets = Vec::new();
        for (index, object) in objects.iter().enumerate() {
            offsets.push(pdf.len());
            writeln!(&mut pdf, "{} 0 obj\n{}\nendobj", index + 1, object).unwrap();
        }

        let xref_offset = pdf.len();
        write!(&mut pdf, "xref\n0 {}\n", objects.len() + 1).unwrap();
        pdf.push_str("0000000000 65535 f \n");
        for offset in offsets {
            writeln!(&mut pdf, "{offset:010} 00000 n ").unwrap();
        }
        write!(
            &mut pdf,
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref_offset}\n%%EOF\n",
            objects.len() + 1
        )
        .unwrap();
        pdf.into_bytes()
    }
}
