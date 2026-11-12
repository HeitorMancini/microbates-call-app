//! Pixel helpers: BGRA capture -> I420 (libyuv), scaling, RGBA previews and JPEG.

use livekit::webrtc::native::yuv_helper;
use livekit::webrtc::prelude::{I420Buffer, VideoBuffer};

/// Largest even size with the aspect ratio of `width`x`height` that fits the bounds
/// (never upscales).
pub fn fit_within(width: u32, height: u32, max_width: u32, max_height: u32) -> (u32, u32) {
    let (width, height) = (width.max(2), height.max(2));
    let scale = (f64::from(max_width) / f64::from(width))
        .min(f64::from(max_height) / f64::from(height))
        .min(1.0);
    let even = |v: f64| ((v.round() as u32) & !1).max(2);
    (
        even(f64::from(width) * scale),
        even(f64::from(height) * scale),
    )
}

/// Converts a BGRA image (libyuv "ARGB", `stride` bytes per row) to I420, cropping
/// to even dimensions.
pub fn bgra_to_i420(width: u32, height: u32, stride: u32, data: &[u8]) -> I420Buffer {
    let (w, h) = ((width & !1).max(2), (height & !1).max(2));
    assert!(
        width >= 2
            && height >= 2
            && stride >= width * 4
            && data.len() >= (stride * height) as usize,
        "invalid BGRA frame {width}x{height} stride {stride} len {}",
        data.len()
    );
    let mut buffer = I420Buffer::new(w, h);
    let (sy, su, sv) = buffer.strides();
    let (y, u, v) = buffer.data_mut();
    yuv_helper::argb_to_i420(data, stride, y, sy, u, su, v, sv, w as i32, h as i32);
    buffer
}

pub fn scale_i420(buffer: &mut I420Buffer, width: u32, height: u32) -> I420Buffer {
    buffer.scale(width as i32, height as i32)
}

/// Downscales to at most `max_width` wide and converts to packed RGBA.
pub fn i420_to_rgba_preview(buffer: &mut I420Buffer, max_width: u32) -> (u32, u32, Vec<u8>) {
    let (w, h) = fit_within(buffer.width(), buffer.height(), max_width, u32::MAX);
    let mut scaled;
    let src = if (w, h) == (buffer.width(), buffer.height()) {
        &*buffer
    } else {
        scaled = buffer.scale(w as i32, h as i32);
        &mut scaled
    };
    let mut rgba = vec![0u8; (w * h * 4) as usize];
    let (sy, su, sv) = src.strides();
    let (y, u, v) = src.data();
    // libyuv names formats by little-endian word order: "ABGR" is R,G,B,A in memory.
    yuv_helper::i420_to_abgr(y, sy, u, su, v, sv, &mut rgba, w * 4, w as i32, h as i32);
    (w, h, rgba)
}

/// Encodes packed RGBA as baseline JPEG.
pub fn encode_jpeg(width: u32, height: u32, rgba: &[u8], quality: u8) -> Result<Vec<u8>, String> {
    let w = u16::try_from(width).map_err(|_| format!("width {width} too large for JPEG"))?;
    let h = u16::try_from(height).map_err(|_| format!("height {height} too large for JPEG"))?;
    let mut out = Vec::with_capacity(rgba.len() / 8);
    jpeg_encoder::Encoder::new(&mut out, quality)
        .encode(rgba, w, h, jpeg_encoder::ColorType::Rgba)
        .map_err(|e| e.to_string())?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fit_keeps_aspect_and_never_upscales() {
        assert_eq!(fit_within(3840, 2160, 1920, 1080), (1920, 1080));
        assert_eq!(fit_within(2560, 1440, 1280, 720), (1280, 720));
        assert_eq!(fit_within(1000, 1000, 1920, 1080), (1000, 1000));
        assert_eq!(fit_within(3440, 1440, 1920, 1080), (1920, 804));
        assert_eq!(fit_within(641, 361, 1920, 1080), (640, 360));
    }

    #[test]
    fn bgra_round_trips_through_i420_and_jpeg() {
        let (w, h) = (64u32, 32u32);
        // Solid red in BGRA, with 16 bytes of row padding.
        let stride = w * 4 + 16;
        let mut bgra = vec![0u8; (stride * h) as usize];
        for row in bgra.chunks_mut(stride as usize) {
            for px in row[..(w * 4) as usize].chunks_mut(4) {
                px.copy_from_slice(&[0, 0, 255, 255]);
            }
        }
        let mut i420 = bgra_to_i420(w, h, stride, &bgra);
        let (pw, ph, rgba) = i420_to_rgba_preview(&mut i420, 32);
        assert_eq!((pw, ph), (32, 16));
        let px = &rgba[..4];
        assert!(
            px[0] > 200 && px[1] < 60 && px[2] < 60,
            "expected red, got {px:?}"
        );

        let jpeg = encode_jpeg(pw, ph, &rgba, 70).unwrap();
        assert_eq!(&jpeg[..2], &[0xFF, 0xD8]);
    }
}
