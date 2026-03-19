// opj_decompress: JPEG 2000 decompression tool
//
// Decodes J2K/JP2 files to PGM/PPM/PGX image formats.
// Equivalent to C version's opj_decompress.

mod image_io;

use clap::Parser;
use openjpeg_rs::api::{self, CodecFormat};
use std::path::PathBuf;
use std::process;

/// JPEG 2000 image decompressor.
#[derive(Parser, Debug)]
#[command(name = "opj_decompress", about = "Decode JPEG 2000 images")]
struct Args {
    /// Input JPEG 2000 file (.j2k, .j2c, .jp2)
    #[arg(short = 'i', long = "input")]
    input: PathBuf,

    /// Output image file (.pgm, .ppm, .pgx)
    #[arg(short = 'o', long = "output")]
    output: PathBuf,

    /// Verbose output
    #[arg(short = 'v', long = "verbose")]
    verbose: bool,
}

fn main() {
    let args = Args::parse();

    if let Err(e) = run(&args) {
        eprintln!("error: {e}");
        process::exit(1);
    }
}

fn run(args: &Args) -> Result<(), Box<dyn std::error::Error>> {
    // Detect output format
    let out_format = image_io::detect_output_format(&args.output)
        .ok_or_else(|| format!("unsupported output format: {}", args.output.display()))?;

    // Read input file
    if args.verbose {
        eprintln!("reading {}", args.input.display());
    }
    let data = std::fs::read(&args.input)?;

    // Detect codec format
    let codec_format = api::detect_format(&data)
        .ok_or_else(|| format!("cannot detect JPEG 2000 format: {}", args.input.display()))?;

    if args.verbose {
        let fmt_name = match codec_format {
            CodecFormat::J2k => "J2K",
            CodecFormat::Jp2 => "JP2",
        };
        eprintln!("detected format: {fmt_name}");
    }

    // Decode
    if args.verbose {
        eprintln!("decoding...");
    }
    let image = api::decode_owned(data, codec_format)?;

    if args.verbose {
        let w = image.x1 - image.x0;
        let h = image.y1 - image.y0;
        let nc = image.comps.len();
        let prec = image.comps.first().map_or(0, |c| c.prec);
        eprintln!("image: {w}x{h}, {nc} component(s), {prec}-bit");
    }

    // Write output
    if args.verbose {
        eprintln!("writing {}", args.output.display());
    }
    image_io::write_image(&image, &args.output, out_format)?;

    if args.verbose {
        eprintln!("done");
    }

    Ok(())
}
