// Image format I/O for CLI tools.
//
// Supports PGM/PPM (Portable Graymap/Pixmap, binary P5/P6) and PGX
// (JPEG 2000 test format). No external dependencies required.

use openjpeg_rs::image::Image;
use std::fs::File;
use std::io::{self, BufWriter, Write};
use std::path::Path;

/// Supported output image formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageFormat {
    /// PGM (grayscale, P5 binary) or PPM (color, P6 binary).
    Pnm,
    /// PGX (JPEG 2000 single-component test format).
    Pgx,
}

/// Detect output format from file extension.
pub fn detect_output_format(path: &Path) -> Option<ImageFormat> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    match ext.as_str() {
        "pgm" | "ppm" | "pnm" => Some(ImageFormat::Pnm),
        "pgx" => Some(ImageFormat::Pgx),
        _ => None,
    }
}

/// Write image to file in the specified format.
pub fn write_image(image: &Image, path: &Path, format: ImageFormat) -> io::Result<()> {
    match format {
        ImageFormat::Pnm => write_pnm(image, path),
        ImageFormat::Pgx => write_pgx(image, path),
    }
}

/// Write image as PGM (1 component) or PPM (3 components).
///
/// PGM: P5 binary format (8-bit or 16-bit grayscale).
/// PPM: P6 binary format (8-bit or 16-bit RGB).
fn write_pnm(image: &Image, path: &Path) -> io::Result<()> {
    if image.comps.is_empty() {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "no components"));
    }

    let nc = image.comps.len();
    if nc != 1 && nc != 3 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("PNM requires 1 or 3 components, got {nc}"),
        ));
    }

    let w = image.comps[0].w as usize;
    let h = image.comps[0].h as usize;
    let prec = image.comps[0].prec;
    let maxval = (1u32 << prec) - 1;

    let file = File::create(path)?;
    let mut writer = BufWriter::new(file);

    // Header
    let magic = if nc == 1 { "P5" } else { "P6" };
    writeln!(writer, "{magic}")?;
    writeln!(writer, "{w} {h}")?;
    writeln!(writer, "{maxval}")?;

    // Pixel data
    let bytes_per_sample = if prec > 8 { 2 } else { 1 };

    for y in 0..h {
        for x in 0..w {
            for c in 0..nc {
                let idx = y * w + x;
                let val = if idx < image.comps[c].data.len() {
                    image.comps[c].data[idx].max(0) as u32
                } else {
                    0
                };
                let clamped = val.min(maxval);
                if bytes_per_sample == 2 {
                    writer.write_all(&(clamped as u16).to_be_bytes())?;
                } else {
                    writer.write_all(&[clamped as u8])?;
                }
            }
        }
    }

    writer.flush()
}

/// Write a single component as PGX format.
///
/// PGX format: text header line + raw pixel data (big-endian).
/// Header: "PG <endian> [+|-]<precision> <width> <height>"
/// If the image has multiple components, writes comp0 only.
fn write_pgx(image: &Image, path: &Path) -> io::Result<()> {
    if image.comps.is_empty() {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "no components"));
    }

    let comp = &image.comps[0];
    let w = comp.w as usize;
    let h = comp.h as usize;
    let prec = comp.prec;
    let sign_char = if comp.sgnd { '-' } else { '+' };

    let file = File::create(path)?;
    let mut writer = BufWriter::new(file);

    // Header
    writeln!(writer, "PG ML {sign_char}{prec} {w} {h}")?;

    // Pixel data (big-endian)
    let bytes_per_sample = if prec > 16 {
        4
    } else if prec > 8 {
        2
    } else {
        1
    };

    for y in 0..h {
        for x in 0..w {
            let idx = y * w + x;
            let val = if idx < comp.data.len() {
                comp.data[idx]
            } else {
                0
            };
            match bytes_per_sample {
                4 => writer.write_all(&(val as u32).to_be_bytes())?,
                2 => writer.write_all(&(val as u16).to_be_bytes())?,
                _ => writer.write_all(&[val as u8])?,
            }
        }
    }

    writer.flush()
}

#[cfg(test)]
mod tests {
    use super::*;
    use openjpeg_rs::image::{Image, ImageCompParam};
    use openjpeg_rs::types::ColorSpace;
    use std::io::Read;

    fn make_gray_image(w: u32, h: u32, prec: u32, data: Vec<i32>) -> Image {
        let params = vec![ImageCompParam {
            dx: 1,
            dy: 1,
            w,
            h,
            x0: 0,
            y0: 0,
            prec,
            sgnd: false,
        }];
        let mut img = Image::new(&params, ColorSpace::Gray);
        img.x1 = w;
        img.y1 = h;
        img.comps[0].data = data;
        img
    }

    fn make_rgb_image(w: u32, h: u32, r: Vec<i32>, g: Vec<i32>, b: Vec<i32>) -> Image {
        let params: Vec<_> = (0..3)
            .map(|_| ImageCompParam {
                dx: 1,
                dy: 1,
                w,
                h,
                x0: 0,
                y0: 0,
                prec: 8,
                sgnd: false,
            })
            .collect();
        let mut img = Image::new(&params, ColorSpace::Srgb);
        img.x1 = w;
        img.y1 = h;
        img.comps[0].data = r;
        img.comps[1].data = g;
        img.comps[2].data = b;
        img
    }

    #[test]
    fn detect_format_pgm() {
        assert_eq!(
            detect_output_format(Path::new("test.pgm")),
            Some(ImageFormat::Pnm)
        );
        assert_eq!(
            detect_output_format(Path::new("test.ppm")),
            Some(ImageFormat::Pnm)
        );
        assert_eq!(
            detect_output_format(Path::new("test.pgx")),
            Some(ImageFormat::Pgx)
        );
        assert_eq!(detect_output_format(Path::new("test.jpg")), None);
    }

    #[test]
    fn write_pgm_2x2() {
        let img = make_gray_image(2, 2, 8, vec![10, 20, 30, 40]);
        let dir = std::env::temp_dir();
        let path = dir.join("test_write_pgm_2x2.pgm");

        write_image(&img, &path, ImageFormat::Pnm).unwrap();

        let mut data = Vec::new();
        File::open(&path).unwrap().read_to_end(&mut data).unwrap();
        let text = String::from_utf8_lossy(&data);
        assert!(text.starts_with("P5\n2 2\n255\n"));
        // After header: 4 bytes of pixel data
        let header_end = text.find("\n255\n").unwrap() + 5;
        assert_eq!(&data[header_end..], &[10, 20, 30, 40]);

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn write_ppm_2x1() {
        let img = make_rgb_image(2, 1, vec![255, 0], vec![0, 255], vec![0, 0]);
        let dir = std::env::temp_dir();
        let path = dir.join("test_write_ppm_2x1.ppm");

        write_image(&img, &path, ImageFormat::Pnm).unwrap();

        let mut data = Vec::new();
        File::open(&path).unwrap().read_to_end(&mut data).unwrap();
        let text = String::from_utf8_lossy(&data);
        assert!(text.starts_with("P6\n2 1\n255\n"));
        let header_end = text.find("\n255\n").unwrap() + 5;
        // pixel 0: R=255, G=0, B=0  pixel 1: R=0, G=255, B=0
        assert_eq!(&data[header_end..], &[255, 0, 0, 0, 255, 0]);

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn write_pgx_2x2() {
        let img = make_gray_image(2, 2, 8, vec![10, 20, 30, 40]);
        let dir = std::env::temp_dir();
        let path = dir.join("test_write_pgx_2x2.pgx");

        write_image(&img, &path, ImageFormat::Pgx).unwrap();

        let mut data = Vec::new();
        File::open(&path).unwrap().read_to_end(&mut data).unwrap();
        let text = String::from_utf8_lossy(&data);
        assert!(text.starts_with("PG ML +8 2 2\n"));
        let header_end = text.find("\n").unwrap() + 1;
        assert_eq!(&data[header_end..], &[10, 20, 30, 40]);

        std::fs::remove_file(&path).ok();
    }
}
