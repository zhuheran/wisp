use base64::{engine::general_purpose::STANDARD, Engine};
use image::{DynamicImage, GenericImageView, ImageFormat};
use serde::{Deserialize, Serialize};
use std::io::Cursor;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageCompressConfig {
    pub max_width: u32,
    pub max_height: u32,
    pub jpeg_quality: u8,
}

impl Default for ImageCompressConfig {
    fn default() -> Self {
        Self {
            max_width: 2048,
            max_height: 2048,
            jpeg_quality: 80,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageCompressResult {
    pub data: String,
    pub mime_type: String,
    pub original_size: usize,
    pub compressed_size: usize,
    pub was_compressed: bool,
}

#[tauri::command]
pub async fn compress_image(
    base64_data: String,
    mime_type: String,
    config: Option<ImageCompressConfig>,
) -> Result<ImageCompressResult, String> {
    let config = config.unwrap_or_default();

    let input_bytes = STANDARD
        .decode(&base64_data)
        .map_err(|e| format!("Failed to decode base64: {}", e))?;

    let original_size = input_bytes.len();

    let img_format = match mime_type.as_str() {
        "image/png" => ImageFormat::Png,
        "image/jpeg" | "image/jpg" => ImageFormat::Jpeg,
        "image/gif" => ImageFormat::Gif,
        "image/webp" => ImageFormat::WebP,
        "image/bmp" => ImageFormat::Bmp,
        "image/tiff" => ImageFormat::Tiff,
        _ => ImageFormat::Png,
    };

    let img = image::load_from_memory_with_format(&input_bytes, img_format)
        .map_err(|e| format!("Failed to load image: {}", e))?;

    let (width, height) = img.dimensions();
    let needs_resize = width > config.max_width || height > config.max_height;

    let processed_img = if needs_resize {
        img.resize(config.max_width, config.max_height, image::imageops::FilterType::Lanczos3)
    } else {
        img
    };

    let output_format = if mime_type == "image/png" {
        ImageFormat::Png
    } else {
        ImageFormat::Jpeg
    };

    let mut output_buffer = Cursor::new(Vec::new());

    match output_format {
        ImageFormat::Jpeg => {
            let encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(
                &mut output_buffer,
                config.jpeg_quality,
            );
            processed_img
                .write_with_encoder(encoder)
                .map_err(|e| format!("Failed to encode JPEG: {}", e))?;
        }
        ImageFormat::Png => {
            processed_img
                .write_to(&mut output_buffer, ImageFormat::Png)
                .map_err(|e| format!("Failed to encode PNG: {}", e))?;
        }
        _ => {
            processed_img
                .write_to(&mut output_buffer, output_format)
                .map_err(|e| format!("Failed to encode image: {}", e))?;
        }
    }

    let output_bytes = output_buffer.into_inner();
    let compressed_size = output_bytes.len();

    let output_mime_type = if output_format == ImageFormat::Jpeg {
        "image/jpeg".to_string()
    } else {
        "image/png".to_string()
    };

    if compressed_size >= original_size {
        return Ok(ImageCompressResult {
            data: base64_data,
            mime_type,
            original_size,
            compressed_size: original_size,
            was_compressed: false,
        });
    }

    let output_base64 = STANDARD.encode(&output_bytes);

    Ok(ImageCompressResult {
        data: output_base64,
        mime_type: output_mime_type,
        original_size,
        compressed_size,
        was_compressed: true,
    })
}

#[tauri::command]
pub async fn get_image_info(base64_data: String) -> Result<ImageInfo, String> {
    let input_bytes = STANDARD
        .decode(&base64_data)
        .map_err(|e| format!("Failed to decode base64: {}", e))?;

    let img = image::load_from_memory(&input_bytes)
        .map_err(|e| format!("Failed to load image: {}", e))?;

    let (width, height) = img.dimensions();

    let format = match img {
        DynamicImage::ImageLuma8(_) => "grayscale",
        DynamicImage::ImageLumaA8(_) => "grayscale-alpha",
        DynamicImage::ImageRgb8(_) => "rgb",
        DynamicImage::ImageRgba8(_) => "rgba",
        DynamicImage::ImageLuma16(_) => "grayscale16",
        DynamicImage::ImageLumaA16(_) => "grayscale-alpha16",
        DynamicImage::ImageRgb16(_) => "rgb16",
        DynamicImage::ImageRgba16(_) => "rgba16",
        DynamicImage::ImageRgb32F(_) => "rgb32f",
        DynamicImage::ImageRgba32F(_) => "rgba32f",
        _ => "unknown",
    };

    Ok(ImageInfo {
        width,
        height,
        format: format.to_string(),
        size_bytes: input_bytes.len(),
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageInfo {
    pub width: u32,
    pub height: u32,
    pub format: String,
    pub size_bytes: usize,
}
