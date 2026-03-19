// opj_dump: JPEG 2000 codestream information tool
//
// Displays header and metadata from J2K/JP2 files.
// Equivalent to C version's opj_dump.

use clap::Parser;
use openjpeg_rs::api::{self, CodecFormat};
use openjpeg_rs::io::cio::MemoryStream;
use openjpeg_rs::j2k::read::J2kDecoder;
use openjpeg_rs::jp2::read::Jp2Decoder;
use std::io::Write;
use std::path::PathBuf;
use std::process;

/// JPEG 2000 codestream information tool.
#[derive(Parser, Debug)]
#[command(name = "opj_dump", about = "Display JPEG 2000 codestream information")]
struct Args {
    /// Input JPEG 2000 file (.j2k, .j2c, .jp2)
    #[arg(short = 'i', long = "input")]
    input: PathBuf,

    /// Output file (default: stdout)
    #[arg(short = 'o', long = "output")]
    output: Option<PathBuf>,
}

fn main() {
    let args = Args::parse();

    if let Err(e) = run(&args) {
        eprintln!("error: {e}");
        process::exit(1);
    }
}

fn run(args: &Args) -> Result<(), Box<dyn std::error::Error>> {
    let data = std::fs::read(&args.input)?;

    let codec_format = api::detect_format(&data)
        .ok_or_else(|| format!("cannot detect JPEG 2000 format: {}", args.input.display()))?;

    // Open output
    let mut out: Box<dyn Write> = if let Some(ref path) = args.output {
        Box::new(std::fs::File::create(path)?)
    } else {
        Box::new(std::io::stdout().lock())
    };

    writeln!(out, "File: {}", args.input.display())?;

    match codec_format {
        CodecFormat::J2k => {
            writeln!(out, "Format: J2K (raw codestream)")?;
            let mut stream = MemoryStream::new_input(data);
            let mut dec = J2kDecoder::new();
            dec.read_header(&mut stream)?;
            dump_image_info(&mut out, &dec.image)?;
            dump_coding_params(&mut out, &dec.cp)?;
        }
        CodecFormat::Jp2 => {
            writeln!(out, "Format: JP2 (file format)")?;
            let mut stream = MemoryStream::new_input(data);
            let mut dec = Jp2Decoder::new();
            dec.read_header(&mut stream)?;
            writeln!(out, "JP2 Header:")?;
            writeln!(out, "  Dimensions: {}x{}", dec.width, dec.height)?;
            writeln!(out, "  Components: {}", dec.numcomps)?;
            writeln!(out, "  BPC: {}", dec.bpc)?;
            dump_image_info(&mut out, &dec.j2k.image)?;
            dump_coding_params(&mut out, &dec.j2k.cp)?;
        }
    }

    Ok(())
}

fn dump_image_info(out: &mut dyn Write, image: &openjpeg_rs::image::Image) -> std::io::Result<()> {
    let w = image.x1 - image.x0;
    let h = image.y1 - image.y0;
    writeln!(out, "Image:")?;
    writeln!(out, "  Origin: ({}, {})", image.x0, image.y0)?;
    writeln!(out, "  Size: {w}x{h}")?;
    writeln!(out, "  Components: {}", image.comps.len())?;
    writeln!(out, "  Color space: {:?}", image.color_space)?;
    for (i, comp) in image.comps.iter().enumerate() {
        writeln!(
            out,
            "  Component {i}: {}x{}, {}-bit, {}signed, dx={}, dy={}",
            comp.w,
            comp.h,
            comp.prec,
            if comp.sgnd { "" } else { "un" },
            comp.dx,
            comp.dy,
        )?;
    }
    Ok(())
}

fn dump_coding_params(
    out: &mut dyn Write,
    cp: &openjpeg_rs::j2k::params::CodingParameters,
) -> std::io::Result<()> {
    writeln!(out, "Coding parameters:")?;
    writeln!(out, "  Tile grid origin: ({}, {})", cp.tx0, cp.ty0)?;
    writeln!(out, "  Tile size: {}x{}", cp.tdx, cp.tdy)?;
    writeln!(out, "  Tiles: {}x{} = {}", cp.tw, cp.th, cp.tw * cp.th)?;
    if !cp.tcps.is_empty() {
        let tcp = &cp.tcps[0];
        writeln!(out, "  Quality layers: {}", tcp.numlayers)?;
        if !tcp.tccps.is_empty() {
            let tccp = &tcp.tccps[0];
            writeln!(out, "  Resolutions: {}", tccp.numresolutions)?;
            writeln!(
                out,
                "  Code-block size: {}x{}",
                1u32 << tccp.cblkw,
                1u32 << tccp.cblkh
            )?;
            let dwt_name = if tccp.qmfbid == 1 {
                "5-3 reversible"
            } else {
                "9-7 irreversible"
            };
            writeln!(out, "  Wavelet: {dwt_name}")?;
        }
    }
    Ok(())
}
