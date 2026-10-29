//! Preflight mirrors the host formats without trusting MIME as proof of decoded content.
use std::io::Cursor;
mod svg;

pub(super) fn validate(media_type: &str, bytes: &[u8]) -> Result<(), String> {
    match media_type {
        "image/svg+xml" => svg::validate(bytes),
        "font/ttf" | "font/otf" => {
            let faces = ttf_parser::fonts_in_collection(bytes).unwrap_or(1);
            if faces == 0 || faces > 64 {
                return Err("font collection face budget exceeded".into());
            }
            for index in 0..faces {
                ttf_parser::Face::parse(bytes, index)
                    .map_err(|error| format!("font decode failed: {error:?}"))?;
            }
            Ok(())
        }
        "font/woff" | "font/woff2" => {
            Err("Native compressed fonts require a bounded WOFF decoder adapter".into())
        }
        _ => {
            let format = match media_type {
                "image/png" => image::ImageFormat::Png,
                "image/jpeg" => image::ImageFormat::Jpeg,
                "image/webp" => image::ImageFormat::WebP,
                "image/gif" => image::ImageFormat::Gif,
                _ => return Err("Native asset decoder is not registered for the media type".into()),
            };
            let mut limits = image::Limits::default();
            limits.max_image_width = Some(8192);
            limits.max_image_height = Some(8192);
            limits.max_alloc = Some(256 * 1024 * 1024);
            let mut reader = image::ImageReader::with_format(Cursor::new(bytes), format);
            reader.limits(limits);
            let decoded = reader
                .decode()
                .map_err(|error| format!("image decode failed: {error}"))?;
            if u64::from(decoded.width()) * u64::from(decoded.height()) > 8192 * 8192 {
                return Err("decoded image pixel budget exceeded".into());
            }
            Ok(())
        }
    }
}
