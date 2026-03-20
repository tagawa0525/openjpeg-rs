// Phase 1100g: Roundtrip verification tests
//
// Verifies encode→decode roundtrip correctness:
// - 5-3 reversible: pixel-perfect match
// - 9-7 irreversible: PSNR above threshold

use openjpeg_rs::api::{CodecFormat, EncodeOptions, decode, encode_with_params};
use openjpeg_rs::image::{Image, ImageCompParam};
use openjpeg_rs::types::ColorSpace;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn build_gray_image(w: u32, h: u32, pixel_fn: impl FnMut(usize) -> i32) -> Image {
    let params = vec![ImageCompParam {
        dx: 1,
        dy: 1,
        w,
        h,
        x0: 0,
        y0: 0,
        prec: 8,
        sgnd: false,
    }];
    let mut image = Image::new(&params, ColorSpace::Gray);
    image.x0 = 0;
    image.y0 = 0;
    image.x1 = w;
    image.y1 = h;
    let n = (w * h) as usize;
    image.comps[0].data = (0..n).map(pixel_fn).collect();
    image
}

fn build_rgb_image(w: u32, h: u32, pixel_fns: [fn(usize) -> i32; 3]) -> Image {
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
    let mut image = Image::new(&params, ColorSpace::Srgb);
    image.x0 = 0;
    image.y0 = 0;
    image.x1 = w;
    image.y1 = h;
    let n = (w * h) as usize;
    for (c, pf) in pixel_fns.iter().enumerate() {
        image.comps[c].data = (0..n).map(pf).collect();
    }
    image
}

fn assert_pixels_exact(original: &Image, decoded: &Image) {
    assert_eq!(
        original.comps.len(),
        decoded.comps.len(),
        "component count mismatch"
    );
    for (c, (orig, dec)) in original.comps.iter().zip(decoded.comps.iter()).enumerate() {
        assert_eq!(
            orig.data.len(),
            dec.data.len(),
            "component {c}: pixel count mismatch"
        );
        for (i, (&o, &d)) in orig.data.iter().zip(dec.data.iter()).enumerate() {
            assert_eq!(o, d, "component {c}, pixel {i}: expected {o}, got {d}");
        }
    }
}

fn compute_psnr(original: &Image, decoded: &Image, max_val: f64) -> f64 {
    let mut mse_sum = 0.0f64;
    let mut total_pixels = 0usize;
    for (orig, dec) in original.comps.iter().zip(decoded.comps.iter()) {
        for (&o, &d) in orig.data.iter().zip(dec.data.iter()) {
            let diff = (o - d) as f64;
            mse_sum += diff * diff;
            total_pixels += 1;
        }
    }
    if total_pixels == 0 {
        return f64::INFINITY;
    }
    let mse = mse_sum / total_pixels as f64;
    if mse == 0.0 {
        return f64::INFINITY;
    }
    10.0 * (max_val * max_val / mse).log10()
}

fn lossless_options() -> EncodeOptions {
    EncodeOptions {
        qmfbid: 1,
        numresolutions: 1,
        mct: 0,
    }
}

// ---------------------------------------------------------------------------
// 5-3 lossless roundtrip tests
// ---------------------------------------------------------------------------

#[test]
fn roundtrip_53_lossless_gradient_gray() {
    let image = build_gray_image(16, 16, |i| (i % 256) as i32);
    let encoded = encode_with_params(&image, CodecFormat::J2k, &lossless_options()).unwrap();
    let decoded = decode(&encoded, CodecFormat::J2k).unwrap();
    assert_pixels_exact(&image, &decoded);
}

#[test]
fn roundtrip_53_lossless_random_gray() {
    // Deterministic pseudo-random via simple LCG
    let mut seed: u32 = 12345;
    let image = build_gray_image(32, 32, |_| {
        seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
        ((seed >> 16) % 256) as i32
    });
    let encoded = encode_with_params(&image, CodecFormat::J2k, &lossless_options()).unwrap();
    let decoded = decode(&encoded, CodecFormat::J2k).unwrap();
    assert_pixels_exact(&image, &decoded);
}

#[test]
fn roundtrip_53_lossless_edge_values() {
    let image = build_gray_image(8, 8, |i| {
        let row = i / 8;
        let col = i % 8;
        match (row < 4, col < 4) {
            (true, true) => 0,
            (true, false) => 127,
            (false, true) => 128,
            (false, false) => 255,
        }
    });
    let encoded = encode_with_params(&image, CodecFormat::J2k, &lossless_options()).unwrap();
    let decoded = decode(&encoded, CodecFormat::J2k).unwrap();
    assert_pixels_exact(&image, &decoded);
}

#[test]
fn roundtrip_53_lossless_multi_resolution() {
    let image = build_gray_image(32, 32, |i| (i % 256) as i32);
    let options = EncodeOptions {
        qmfbid: 1,
        numresolutions: 3,
        mct: 0,
    };
    let encoded = encode_with_params(&image, CodecFormat::J2k, &options).unwrap();
    let decoded = decode(&encoded, CodecFormat::J2k).unwrap();
    assert_pixels_exact(&image, &decoded);
}

#[test]
fn roundtrip_53_lossless_rgb() {
    let image = build_rgb_image(
        16,
        16,
        [
            |i| (i % 256) as i32,
            |i| ((i * 3 + 50) % 256) as i32,
            |i| ((i * 7 + 100) % 256) as i32,
        ],
    );
    let options = EncodeOptions {
        qmfbid: 1,
        numresolutions: 1,
        mct: 1,
    };
    let encoded = encode_with_params(&image, CodecFormat::J2k, &options).unwrap();
    let decoded = decode(&encoded, CodecFormat::J2k).unwrap();
    assert_pixels_exact(&image, &decoded);
}

#[test]
fn roundtrip_53_lossless_non_power_of_2() {
    let image = build_gray_image(13, 7, |i| (i % 256) as i32);
    let encoded = encode_with_params(&image, CodecFormat::J2k, &lossless_options()).unwrap();
    let decoded = decode(&encoded, CodecFormat::J2k).unwrap();
    assert_pixels_exact(&image, &decoded);
}

#[test]
fn roundtrip_jp2_format_lossless() {
    let image = build_gray_image(16, 16, |i| (i % 256) as i32);
    let encoded = encode_with_params(&image, CodecFormat::Jp2, &lossless_options()).unwrap();
    let decoded = decode(&encoded, CodecFormat::Jp2).unwrap();
    assert_pixels_exact(&image, &decoded);
}

// ---------------------------------------------------------------------------
// 9-7 lossy roundtrip test
// ---------------------------------------------------------------------------

#[test]
#[ignore = "9-7 irreversible quantization not yet implemented in encoder"]
fn roundtrip_97_lossy_psnr() {
    let image = build_gray_image(32, 32, |i| (i % 256) as i32);
    let options = EncodeOptions {
        qmfbid: 0,
        numresolutions: 1,
        mct: 0,
    };
    let encoded = encode_with_params(&image, CodecFormat::J2k, &options).unwrap();
    let decoded = decode(&encoded, CodecFormat::J2k).unwrap();
    let psnr = compute_psnr(&image, &decoded, 255.0);
    assert!(psnr >= 30.0, "PSNR {psnr:.1} dB is below 30 dB threshold");
}
