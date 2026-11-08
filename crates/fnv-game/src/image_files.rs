//! Pictures SUP's functions write (`crate::nvse_sup5`; docs/spec-notes/ext-sup5.md): the
//! screenshots the engine takes for CaptureScreenshot and CaptureScreenshotAlt
//! ([`ShotRequest`], `nvf_play::sup_shots`) and TextToQRCode's code, encoded here as PNG
//! (the `png` crate), BMP or DDS (written by hand: uncompressed, a fixed header).

/// Where a screenshot goes and how it is cut, for the engine.
#[derive(Debug, Clone, PartialEq)]
pub struct ShotRequest {
    /// The file: under the game's own folder (`root`, CaptureScreenshot) or under Data
    /// (CaptureScreenshotAlt's `textures\SUPScreenshots\…`). Either way into NVForever's
    /// layer (`crate::mod_files`), never the game's folder.
    pub path: String,
    pub root: bool,
    /// The format number [documented: CaptureScreenshot, 0 JPG … 5 DDS; CaptureScreenshotAlt,
    /// 6 a smaller DDS].
    pub format: u8,
    /// The part of the screen, `[x0, x1, y0, y1]`, in pixels when `game_res`, else in the
    /// UI's units (960 high) [documented: CaptureScreenshotAlt, "UI(XML) screen
    /// resolution is used" unless Use Game Res].
    pub crop: Option<[f32; 4]>,
    pub game_res: bool,
}

/// The file extension of a format number [documented: CaptureScreenshot,
/// CaptureScreenshotAlt formats]; anything else is JPG [hypothesis: the default].
pub fn extension(format: u8) -> &'static str {
    match format {
        1 => "bmp",
        2 => "gif",
        3 => "tiff",
        4 => "png",
        5 | 6 => "dds",
        _ => "jpg",
    }
}

/// The part of a `w`×`h` picture a crop takes, in pixels, clamped to the picture:
/// `(x, y, width, height)`; `None` when nothing is left. `scale` turns the crop's units
/// into pixels.
pub fn crop_rect(w: u32, h: u32, crop: [f32; 4], scale: f32) -> Option<(u32, u32, u32, u32)> {
    let px = |v: f32, max: u32| ((v * scale).round().max(0.0) as u32).min(max);
    let (x0, x1) = (px(crop[0].min(crop[1]), w), px(crop[0].max(crop[1]), w));
    let (y0, y1) = (px(crop[2].min(crop[3]), h), px(crop[2].max(crop[3]), h));
    (x1 > x0 && y1 > y0).then(|| (x0, y0, x1 - x0, y1 - y0))
}

/// `rgba` (`w`×`h`, 4 bytes a pixel, top row first) cut to a rectangle.
pub fn crop(rgba: &[u8], w: u32, (x, y, cw, ch): (u32, u32, u32, u32)) -> Vec<u8> {
    let mut out = Vec::with_capacity((cw * ch * 4) as usize);
    for r in y..y + ch {
        let o = ((r * w + x) * 4) as usize;
        out.extend_from_slice(&rgba[o..o + (cw * 4) as usize]);
    }
    out
}

/// The file's bytes for a format. JPG, GIF and TIFF have no encoder in NVForever: they
/// are written as PNG under the name asked for [decision: no image encoder added for
/// screenshots alone (2026-11-08 brief); follow-up in ext-sup5.md].
pub fn encode(format: u8, w: u32, h: u32, rgba: &[u8]) -> Option<Vec<u8>> {
    match format {
        1 => Some(bmp(w, h, rgba)),
        5 => Some(dds(w, h, rgba, false)),
        6 => Some(dds(w, h, rgba, true)),
        _ => png(w, h, rgba),
    }
}

pub fn png(w: u32, h: u32, rgba: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    {
        let mut e = png::Encoder::new(&mut out, w, h);
        e.set_color(png::ColorType::Rgba);
        e.set_depth(png::BitDepth::Eight);
        let mut wr = e.write_header().ok()?;
        wr.write_image_data(rgba).ok()?;
    }
    Some(out)
}

/// A 24-bit BMP: a 54-byte header, rows bottom up, each padded to 4 bytes.
pub fn bmp(w: u32, h: u32, rgba: &[u8]) -> Vec<u8> {
    let row = (w * 3).div_ceil(4) * 4;
    let size = 54 + row * h;
    let mut out = Vec::with_capacity(size as usize);
    out.extend_from_slice(b"BM");
    out.extend_from_slice(&size.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&54u32.to_le_bytes());
    out.extend_from_slice(&40u32.to_le_bytes());
    out.extend_from_slice(&(w as i32).to_le_bytes());
    out.extend_from_slice(&(h as i32).to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&24u16.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&(row * h).to_le_bytes());
    out.extend_from_slice(&2835i32.to_le_bytes());
    out.extend_from_slice(&2835i32.to_le_bytes());
    out.extend_from_slice(&[0; 8]);
    for r in (0..h).rev() {
        let start = out.len();
        for c in 0..w {
            let o = ((r * w + c) * 4) as usize;
            out.extend_from_slice(&[rgba[o + 2], rgba[o + 1], rgba[o]]);
        }
        out.resize(start + row as usize, 0);
    }
    out
}

/// An uncompressed DDS: 32-bit BGRA, or 16-bit R5G6B5 for the smaller form
/// [documented: the DDS header of Microsoft's DirectX documentation; hypothesis: format 6
/// "smaller size" as 16 bits a pixel].
pub fn dds(w: u32, h: u32, rgba: &[u8], small: bool) -> Vec<u8> {
    let bpp = if small { 16u32 } else { 32 };
    let mut out = Vec::new();
    out.extend_from_slice(b"DDS ");
    let mut hd = [0u32; 31];
    hd[0] = 124;
    hd[1] = 0x1 | 0x2 | 0x4 | 0x8 | 0x1000; // caps, height, width, pitch, pixel format
    hd[2] = h;
    hd[3] = w;
    hd[4] = w * bpp / 8;
    // The pixel format at 72 bytes into the header (index 18).
    hd[18] = 32;
    hd[19] = if small { 0x40 } else { 0x40 | 0x1 }; // RGB, alpha pixels
    hd[21] = bpp;
    if small {
        (hd[22], hd[23], hd[24], hd[25]) = (0xF800, 0x07E0, 0x001F, 0);
    } else {
        (hd[22], hd[23], hd[24], hd[25]) = (0x00FF_0000, 0x0000_FF00, 0x0000_00FF, 0xFF00_0000);
    }
    hd[26] = 0x1000; // texture
    for v in hd {
        out.extend_from_slice(&v.to_le_bytes());
    }
    for p in rgba.as_chunks::<4>().0 {
        if small {
            let v = (u16::from(p[0] >> 3) << 11) | (u16::from(p[1] >> 2) << 5) | u16::from(p[2] >> 3);
            out.extend_from_slice(&v.to_le_bytes());
        } else {
            out.extend_from_slice(&[p[2], p[1], p[0], p[3]]);
        }
    }
    out
}

/// A QR code of `text` as a black-on-white picture: `scale` pixels a module and a quiet
/// zone of 4 modules [hypothesis: the page gives neither]. `None` when the text is too
/// long for a code.
pub fn qr_rgba(text: &str, scale: u32) -> Option<(u32, Vec<u8>)> {
    let code = qrcodegen::QrCode::encode_text(text, qrcodegen::QrCodeEcc::Medium).ok()?;
    let n = code.size() as u32;
    let side = (n + 8) * scale;
    let mut px = vec![255u8; (side * side * 4) as usize];
    for y in 0..side {
        for x in 0..side {
            let (mx, my) = ((x / scale) as i32 - 4, (y / scale) as i32 - 4);
            if code.get_module(mx, my) {
                let o = ((y * side + x) * 4) as usize;
                px[o..o + 3].copy_from_slice(&[0, 0, 0]);
            }
        }
    }
    Some((side, px))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn headers_and_crops() {
        let px: Vec<u8> = (0..4 * 3 * 2).map(|i| i as u8).collect();
        let b = bmp(3, 2, &px);
        assert_eq!(&b[..2], b"BM");
        assert_eq!(b.len(), 54 + 12 * 2);
        let d = dds(3, 2, &px, false);
        assert_eq!(&d[..4], b"DDS ");
        assert_eq!(d.len(), 128 + 24);
        assert_eq!(dds(3, 2, &px, true).len(), 128 + 12);
        assert_eq!(&png(3, 2, &px).unwrap()[1..4], b"PNG");
        assert_eq!(crop_rect(100, 50, [10.0, 20.0, 40.0, 90.0], 1.0), Some((10, 40, 10, 10)));
        assert_eq!(crop_rect(100, 50, [10.0, 10.0, 0.0, 10.0], 1.0), None);
        assert_eq!(crop(&px, 3, (1, 1, 2, 1)), px[16..24].to_vec());
        let (side, q) = qr_rgba("I love bacon", 2).unwrap();
        assert_eq!(side, (21 + 8) * 2);
        assert_eq!(q.len() as u32, side * side * 4);
    }
}
