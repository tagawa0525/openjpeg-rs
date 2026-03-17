// Phase 600a: JP2 decoder
//
// Reads a JP2 file: parses boxes (JP, FTYP, JP2H, IHDR, COLR, BPCC, JP2C),
// then delegates J2K codestream decoding to J2kDecoder.

use crate::error::{Error, Result};
use crate::io::cio::{MemoryStream, read_bytes_be};
use crate::j2k::read::J2kDecoder;
use crate::jp2::{
    CdefEntry, CmapEntry, ColourMethod, JP2_BPCC, JP2_CDEF, JP2_CMAP, JP2_COLR, JP2_FTYP, JP2_IHDR,
    JP2_JP, JP2_JP2_BRAND, JP2_JP2C, JP2_JP2H, JP2_MAGIC, JP2_PCLR, Jp2Box, Jp2Colour, Jp2CompInfo,
    Jp2State, Pclr,
};
use crate::types::ColorSpace;

/// JP2 file decoder.
pub struct Jp2Decoder {
    /// Current decoder state.
    pub state: Jp2State,
    /// Embedded J2K decoder.
    pub j2k: J2kDecoder,
    /// Image width (from IHDR).
    pub width: u32,
    /// Image height (from IHDR).
    pub height: u32,
    /// Number of components (from IHDR).
    pub numcomps: u32,
    /// Bits per component (from IHDR; 255 = varies per component).
    pub bpc: u8,
    /// Compression type (from IHDR; should be 7).
    pub compression_type: u8,
    /// Colour specification.
    pub colour: Jp2Colour,
    /// Per-component bit depth info.
    pub comp_info: Vec<Jp2CompInfo>,
    /// Channel definition entries (from CDEF box).
    pub cdef: Option<Vec<CdefEntry>>,
    /// Palette data (from PCLR box).
    pub pclr: Option<Pclr>,
    /// Component mapping entries (from CMAP box).
    pub cmap: Option<Vec<CmapEntry>>,
    /// Whether IHDR was found.
    ihdr_found: bool,
    /// Whether COLR was found.
    colr_found: bool,
}

impl Default for Jp2Decoder {
    fn default() -> Self {
        Self::new()
    }
}

impl Jp2Decoder {
    /// Create a new JP2 decoder.
    pub fn new() -> Self {
        Self {
            state: Jp2State::None,
            j2k: J2kDecoder::new(),
            width: 0,
            height: 0,
            numcomps: 0,
            bpc: 0,
            compression_type: 0,
            colour: Jp2Colour::default(),
            comp_info: Vec::new(),
            cdef: None,
            pclr: None,
            cmap: None,
            ihdr_found: false,
            colr_found: false,
        }
    }

    /// Read JP2 file header (all boxes up to JP2C).
    ///
    /// After this call, the stream is positioned at the start of the J2K
    /// codestream inside the JP2C box. Call `read_codestream()` next.
    pub fn read_header(&mut self, stream: &mut MemoryStream) -> Result<()> {
        loop {
            let box_hdr = read_box_header(stream)?;

            // JP2C box: codestream found
            if box_hdr.box_type == JP2_JP2C {
                if self.state != Jp2State::Header {
                    return Err(Error::InvalidInput(
                        "JP2C box found before JP2 header".into(),
                    ));
                }
                self.state = Jp2State::Codestream;
                // Stream is now at start of J2K codestream
                return Ok(());
            }

            let payload_len = (box_hdr.length - box_hdr.header_len) as usize;
            if payload_len > stream.bytes_left() {
                return Err(Error::InvalidInput(format!(
                    "Box payload {} exceeds available data",
                    payload_len
                )));
            }

            match box_hdr.box_type {
                JP2_JP | JP2_FTYP | JP2_JP2H => {
                    let mut payload = vec![0u8; payload_len];
                    if payload_len > 0 && stream.read(&mut payload)? < payload_len {
                        return Err(Error::EndOfStream);
                    }
                    match box_hdr.box_type {
                        JP2_JP => self.read_jp(&payload)?,
                        JP2_FTYP => self.read_ftyp(&payload)?,
                        JP2_JP2H => self.read_jp2h(&payload)?,
                        _ => unreachable!(),
                    }
                }
                _ => {
                    // Skip unknown boxes without allocating payload
                    stream.skip(payload_len as i64)?;
                }
            }
        }
    }

    /// Read the J2K codestream from inside the JP2C box.
    ///
    /// Must be called after `read_header()`.
    pub fn read_codestream(&mut self, stream: &mut MemoryStream) -> Result<()> {
        if self.state != Jp2State::Codestream {
            return Err(Error::InvalidInput(
                "Must call read_header() before read_codestream()".into(),
            ));
        }

        self.j2k.read_header(stream)?;

        // Apply JP2 colour info to the image
        self.apply_colour();

        self.j2k.read_all_tiles(stream)?;

        // Post-decode: apply palette and channel definitions
        self.apply_pclr();
        self.apply_cdef();

        Ok(())
    }

    /// Read JP signature box payload.
    fn read_jp(&mut self, data: &[u8]) -> Result<()> {
        if self.state != Jp2State::None {
            return Err(Error::InvalidInput(
                "JP signature box must be first box".into(),
            ));
        }
        if data.len() != 4 {
            return Err(Error::InvalidInput(format!(
                "JP signature box payload must be 4 bytes, got {}",
                data.len()
            )));
        }
        let magic = read_bytes_be(data, 4);
        if magic != JP2_MAGIC {
            return Err(Error::InvalidInput(format!(
                "Bad JP2 magic number: 0x{magic:08X}"
            )));
        }
        self.state = Jp2State::Signature;
        Ok(())
    }

    /// Read FTYP (File Type) box payload.
    fn read_ftyp(&mut self, data: &[u8]) -> Result<()> {
        if self.state != Jp2State::Signature {
            return Err(Error::InvalidInput(
                "FTYP box must follow JP signature box".into(),
            ));
        }
        if data.len() < 8 {
            return Err(Error::InvalidInput(format!(
                "FTYP box too short: {} bytes",
                data.len()
            )));
        }
        let brand = read_bytes_be(data, 4);
        if brand != JP2_JP2_BRAND {
            return Err(Error::InvalidInput(format!(
                "Unsupported JP2 brand: 0x{brand:08X}"
            )));
        }
        // MinV (4 bytes) + compatibility list (4 bytes each) — skip for now
        self.state = Jp2State::FileType;
        Ok(())
    }

    /// Read JP2H (JP2 Header) super-box payload.
    fn read_jp2h(&mut self, data: &[u8]) -> Result<()> {
        if self.state != Jp2State::FileType {
            return Err(Error::InvalidInput("JP2H box must follow FTYP box".into()));
        }

        let mut offset = 0usize;
        let mut has_ihdr = false;

        while offset < data.len() {
            if offset + 8 > data.len() {
                return Err(Error::InvalidInput("JP2H: truncated sub-box header".into()));
            }
            let sub_lbox = read_bytes_be(&data[offset..], 4);
            let sub_type = read_bytes_be(&data[offset + 4..], 4);

            let (sub_hdr_len, sub_len): (usize, usize) = if sub_lbox == 1 {
                // Extended length
                if offset + 16 > data.len() {
                    return Err(Error::InvalidInput(
                        "JP2H: truncated extended-length sub-box header".into(),
                    ));
                }
                let xl_high = read_bytes_be(&data[offset + 8..], 4);
                if xl_high != 0 {
                    return Err(Error::InvalidInput("JP2H: sub-box size exceeds 4GB".into()));
                }
                let xl_low = read_bytes_be(&data[offset + 12..], 4) as usize;
                if xl_low < 16 {
                    return Err(Error::InvalidInput(format!(
                        "JP2H: invalid extended sub-box length {xl_low}"
                    )));
                }
                (16, xl_low)
            } else if sub_lbox == 0 {
                // Last sub-box: extends to end of JP2H payload
                (8, data.len() - offset)
            } else {
                let len = sub_lbox as usize;
                if len < 8 {
                    return Err(Error::InvalidInput(format!(
                        "JP2H: invalid sub-box length {len}"
                    )));
                }
                (8, len)
            };

            if offset + sub_len > data.len() {
                return Err(Error::InvalidInput(format!(
                    "JP2H: sub-box length {sub_len} exceeds available data"
                )));
            }

            let sub_payload = &data[offset + sub_hdr_len..offset + sub_len];

            match sub_type {
                JP2_IHDR => {
                    self.read_ihdr(sub_payload)?;
                    has_ihdr = true;
                }
                JP2_COLR => {
                    self.read_colr(sub_payload)?;
                }
                JP2_BPCC => {
                    self.read_bpcc(sub_payload)?;
                }
                JP2_CDEF => {
                    self.read_cdef(sub_payload)?;
                }
                JP2_PCLR => {
                    self.read_pclr(sub_payload)?;
                }
                JP2_CMAP => {
                    self.read_cmap(sub_payload)?;
                }
                _ => {
                    // Skip unknown sub-boxes
                }
            }

            offset += sub_len;
        }

        if !has_ihdr {
            return Err(Error::InvalidInput("JP2H: missing IHDR box".into()));
        }
        if !self.colr_found {
            return Err(Error::InvalidInput("JP2H: missing COLR box".into()));
        }

        self.state = Jp2State::Header;
        Ok(())
    }

    /// Read IHDR (Image Header) box payload.
    fn read_ihdr(&mut self, data: &[u8]) -> Result<()> {
        if self.ihdr_found {
            // Per spec, ignore duplicate IHDR boxes
            return Ok(());
        }
        if data.len() != 14 {
            return Err(Error::InvalidInput(format!(
                "IHDR box must be 14 bytes, got {}",
                data.len()
            )));
        }

        self.height = read_bytes_be(data, 4);
        self.width = read_bytes_be(&data[4..], 4);
        self.numcomps = read_bytes_be(&data[8..], 2);
        self.bpc = data[10];
        self.compression_type = data[11];
        // data[12] = UnkC, data[13] = IPR — not needed for basic decoding

        if self.compression_type != 7 {
            return Err(Error::InvalidInput(format!(
                "IHDR: unsupported compression type {} (expected 7)",
                self.compression_type
            )));
        }

        if self.height == 0 || self.width == 0 || self.numcomps == 0 {
            return Err(Error::InvalidInput(format!(
                "IHDR: invalid dimensions {}x{} with {} components",
                self.width, self.height, self.numcomps
            )));
        }
        if self.numcomps > 16384 {
            return Err(Error::InvalidInput(format!(
                "IHDR: too many components: {}",
                self.numcomps
            )));
        }

        // Initialize per-component info
        self.comp_info = vec![Jp2CompInfo::default(); self.numcomps as usize];
        if self.bpc != 255 {
            // Uniform BPC: decode to (prec, sgnd)
            let (prec, sgnd) = decode_bpc(self.bpc);
            for ci in &mut self.comp_info {
                ci.prec = prec;
                ci.sgnd = sgnd;
            }
        }

        self.ihdr_found = true;
        Ok(())
    }

    /// Read COLR (Colour Specification) box payload.
    fn read_colr(&mut self, data: &[u8]) -> Result<()> {
        // Per spec, ignore duplicate COLR boxes
        if self.colr_found {
            return Ok(());
        }
        if data.len() < 3 {
            return Err(Error::InvalidInput("COLR box too short".into()));
        }

        let meth = data[0];
        self.colour.precedence = data[1];
        self.colour.approx = data[2];

        match meth {
            1 => {
                // Enumerated colour space
                if data.len() < 7 {
                    return Err(Error::InvalidInput(
                        "COLR box too short for enumerated colour space".into(),
                    ));
                }
                self.colour.meth = ColourMethod::Enumerated;
                self.colour.enumcs = read_bytes_be(&data[3..], 4);
            }
            2 => {
                // ICC profile
                self.colour.meth = ColourMethod::Icc;
                self.colour.icc_profile = data[3..].to_vec();
            }
            _ => {
                return Err(Error::InvalidInput(format!(
                    "COLR: unsupported method {meth}"
                )));
            }
        }

        self.colr_found = true;
        Ok(())
    }

    /// Read BPCC (Bits Per Component) box payload.
    fn read_bpcc(&mut self, data: &[u8]) -> Result<()> {
        if !self.ihdr_found {
            return Err(Error::InvalidInput("BPCC box found before IHDR".into()));
        }
        if data.len() != self.numcomps as usize {
            return Err(Error::InvalidInput(format!(
                "BPCC: expected {} bytes, got {}",
                self.numcomps,
                data.len()
            )));
        }
        for (i, &b) in data.iter().enumerate() {
            let (prec, sgnd) = decode_bpc(b);
            self.comp_info[i].prec = prec;
            self.comp_info[i].sgnd = sgnd;
        }
        Ok(())
    }

    /// Map JP2 EnumCS to ColorSpace and apply to the J2K image.
    fn apply_colour(&mut self) {
        let cs = match self.colour.meth {
            ColourMethod::Enumerated => match self.colour.enumcs {
                16 => ColorSpace::Srgb,
                17 => ColorSpace::Gray,
                18 => ColorSpace::Sycc,
                24 => ColorSpace::Eycc,
                _ => ColorSpace::Unknown,
            },
            ColourMethod::Icc => {
                self.j2k.image.icc_profile = self.colour.icc_profile.clone();
                ColorSpace::Unknown
            }
        };
        self.j2k.image.color_space = cs;
    }

    /// Read CDEF (Channel Definition) box payload.
    ///
    /// Layout: N(2) + N × {Cn(2), Typ(2), Asoc(2)}.
    fn read_cdef(&mut self, data: &[u8]) -> Result<()> {
        if self.cdef.is_some() {
            return Err(Error::InvalidInput("Duplicate CDEF box".into()));
        }
        if data.len() < 2 {
            return Err(Error::InvalidInput("CDEF box too short".into()));
        }
        let n = read_bytes_be(data, 2) as usize;
        if n == 0 {
            return Err(Error::InvalidInput("CDEF: N must be > 0".into()));
        }
        if data.len() < 2 + n * 6 {
            return Err(Error::InvalidInput(format!(
                "CDEF: expected {} bytes, got {}",
                2 + n * 6,
                data.len()
            )));
        }
        let mut entries = Vec::with_capacity(n);
        for i in 0..n {
            let off = 2 + i * 6;
            let cn = read_bytes_be(&data[off..], 2) as u16;
            let typ = read_bytes_be(&data[off + 2..], 2) as u16;
            let asoc = read_bytes_be(&data[off + 4..], 2) as u16;
            entries.push(CdefEntry { cn, typ, asoc });
        }
        self.cdef = Some(entries);
        Ok(())
    }

    /// Read PCLR (Palette) box payload.
    ///
    /// Layout: NE(2) + NPC(1) + NPC × Bi(1) + NE × NPC × value(variable).
    fn read_pclr(&mut self, data: &[u8]) -> Result<()> {
        if self.pclr.is_some() {
            return Err(Error::InvalidInput("Duplicate PCLR box".into()));
        }
        if data.len() < 3 {
            return Err(Error::InvalidInput("PCLR box too short".into()));
        }
        let nr_entries = read_bytes_be(data, 2) as u16;
        if nr_entries == 0 || nr_entries > 1024 {
            return Err(Error::InvalidInput(format!(
                "PCLR: NE must be 1..1024, got {nr_entries}"
            )));
        }
        let nr_channels = data[2];
        if nr_channels == 0 {
            return Err(Error::InvalidInput("PCLR: NPC must be > 0".into()));
        }
        let header_len = 3 + nr_channels as usize;
        if data.len() < header_len {
            return Err(Error::InvalidInput(
                "PCLR: truncated bit depth table".into(),
            ));
        }

        let mut channel_sign = Vec::with_capacity(nr_channels as usize);
        let mut channel_size = Vec::with_capacity(nr_channels as usize);
        let mut bytes_per_col = Vec::with_capacity(nr_channels as usize);
        for i in 0..nr_channels as usize {
            let bi = data[3 + i];
            let prec = (bi & 0x7F) + 1;
            let sgnd = (bi & 0x80) != 0;
            channel_sign.push(sgnd);
            channel_size.push(prec);
            bytes_per_col.push((prec as usize).div_ceil(8));
        }

        let entry_bytes: usize = bytes_per_col.iter().sum();
        let expected = header_len + nr_entries as usize * entry_bytes;
        if data.len() < expected {
            return Err(Error::InvalidInput(format!(
                "PCLR: expected {} bytes, got {}",
                expected,
                data.len()
            )));
        }

        let mut entries = Vec::with_capacity(nr_entries as usize * nr_channels as usize);
        let mut off = header_len;
        for _ in 0..nr_entries {
            for &nbytes in &bytes_per_col {
                let mut val = 0u32;
                for b in &data[off..off + nbytes] {
                    val = (val << 8) | *b as u32;
                }
                entries.push(val);
                off += nbytes;
            }
        }

        self.pclr = Some(Pclr {
            entries,
            channel_sign,
            channel_size,
            nr_entries,
            nr_channels,
        });
        Ok(())
    }

    /// Read CMAP (Component Mapping) box payload.
    ///
    /// Layout: NPC × {CMP(2), MTYP(1), PCOL(1)}.
    /// Requires PCLR to be read first.
    fn read_cmap(&mut self, data: &[u8]) -> Result<()> {
        if self.cmap.is_some() {
            return Err(Error::InvalidInput("Duplicate CMAP box".into()));
        }
        let pclr = self
            .pclr
            .as_ref()
            .ok_or_else(|| Error::InvalidInput("CMAP box found before PCLR".into()))?;
        let nr_channels = pclr.nr_channels as usize;
        if data.len() < nr_channels * 4 {
            return Err(Error::InvalidInput(format!(
                "CMAP: expected {} bytes, got {}",
                nr_channels * 4,
                data.len()
            )));
        }
        let mut entries = Vec::with_capacity(nr_channels);
        for i in 0..nr_channels {
            let off = i * 4;
            let cmp = read_bytes_be(&data[off..], 2) as u16;
            let mtyp = data[off + 2];
            let pcol = data[off + 3];
            if mtyp > 1 {
                return Err(Error::InvalidInput(format!(
                    "CMAP[{i}]: invalid MTYP {mtyp} (must be 0 or 1)"
                )));
            }
            if mtyp == 1 && pcol >= pclr.nr_channels {
                return Err(Error::InvalidInput(format!(
                    "CMAP[{i}]: PCOL {pcol} >= palette channels {}",
                    pclr.nr_channels
                )));
            }
            if cmp as u32 >= self.numcomps {
                return Err(Error::InvalidInput(format!(
                    "CMAP[{i}]: CMP {cmp} >= image components {}",
                    self.numcomps
                )));
            }
            entries.push(CmapEntry { cmp, mtyp, pcol });
        }
        self.cmap = Some(entries);
        Ok(())
    }

    /// Apply CDEF channel definitions to the image.
    ///
    /// Sets alpha flags and reorders components based on colour associations.
    pub fn apply_cdef(&mut self) {
        let cdef = match self.cdef.take() {
            Some(c) => c,
            None => return,
        };
        let comps = &mut self.j2k.image.comps;
        let nr = comps.len();

        // First pass: mark alpha/opacity channels
        for entry in &cdef {
            let cn = entry.cn as usize;
            if cn >= nr {
                continue;
            }
            if entry.typ == 1 || entry.typ == 2 {
                // 1 = opacity, 2 = premultiplied opacity
                comps[cn].alpha = entry.typ;
            }
        }

        // Second pass: reorder colour components based on asoc
        // Build a target position map: for colour channels (typ==0),
        // asoc-1 is the desired position.
        let mut order: Vec<Option<usize>> = vec![None; nr];
        for entry in &cdef {
            let cn = entry.cn as usize;
            if cn >= nr || entry.typ != 0 {
                continue;
            }
            let asoc = entry.asoc;
            if asoc == 0 || asoc == 0xFFFF {
                continue;
            }
            let target = (asoc - 1) as usize;
            if target < nr {
                order[target] = Some(cn);
            }
        }

        // Apply swaps: place component order[i] into position i
        for i in 0..nr {
            if let Some(src) = order[i] {
                if src == i {
                    continue;
                }
                comps.swap(i, src);
                // Update remaining order entries to reflect the swap
                for slot in &mut order[(i + 1)..] {
                    if *slot == Some(i) {
                        *slot = Some(src);
                    }
                }
            }
        }
    }

    /// Apply PCLR+CMAP palette expansion to the image.
    ///
    /// For palette-mapped channels (mtyp=1), replaces index data with
    /// palette-looked-up colour values. For direct channels (mtyp=0),
    /// copies the component data unchanged.
    pub fn apply_pclr(&mut self) {
        let pclr = match self.pclr.take() {
            Some(p) => p,
            None => return,
        };
        let cmap = match self.cmap.take() {
            Some(c) => c,
            None => return,
        };

        let old_comps = &self.j2k.image.comps;
        let nr_channels = cmap.len();
        let nr_entries = pclr.nr_entries as usize;
        let max_idx = nr_entries.saturating_sub(1) as i32;

        let mut new_comps = Vec::with_capacity(nr_channels);

        for (ch, entry) in cmap.iter().enumerate() {
            let src_idx = entry.cmp as usize;
            if src_idx >= old_comps.len() {
                continue;
            }
            let src = &old_comps[src_idx];

            if entry.mtyp == 0 {
                // Direct mapping: copy component as-is
                new_comps.push(src.clone());
            } else {
                // Palette mapping: lookup each pixel
                let pcol = entry.pcol as usize;
                let nr_ch = pclr.nr_channels as usize;
                let mut data = Vec::with_capacity(src.data.len());
                for &idx in &src.data {
                    let k = idx.clamp(0, max_idx) as usize;
                    data.push(pclr.entries[k * nr_ch + pcol] as i32);
                }
                let mut comp = src.clone();
                comp.data = data;
                comp.prec = pclr.channel_size[pcol] as u32;
                comp.sgnd = pclr.channel_sign[pcol];
                new_comps.push(comp);
            }
        }

        self.j2k.image.comps = new_comps;
    }
}

/// Decode a raw BPC/BPCC byte into (precision, signed).
///
/// JP2 encoding: bit 7 = signedness, bits 0-6 = (precision - 1).
fn decode_bpc(raw: u8) -> (u8, bool) {
    let prec = (raw & 0x7F) + 1;
    let sgnd = (raw & 0x80) != 0;
    (prec, sgnd)
}

/// Read a JP2 box header (8 bytes: length + type) from the stream.
///
/// Returns a `Jp2Box` with `header_len` set to the number of bytes consumed
/// by the header (8 for normal, 16 for extended-length boxes).
fn read_box_header(stream: &mut MemoryStream) -> Result<Jp2Box> {
    let mut buf = [0u8; 8];
    if stream.read(&mut buf)? < 8 {
        return Err(Error::EndOfStream);
    }
    let length = read_bytes_be(&buf[0..], 4);
    let box_type = read_bytes_be(&buf[4..], 4);

    if length == 0 {
        // Last box: extends to end of stream
        let remaining = stream.bytes_left() as u32;
        return Ok(Jp2Box {
            length: remaining + 8,
            box_type,
            header_len: 8,
        });
    }

    if length == 1 {
        // Extended length (8 bytes) — we only support up to 4GB
        let mut xlbuf = [0u8; 8];
        if stream.read(&mut xlbuf)? < 8 {
            return Err(Error::EndOfStream);
        }
        let xl_high = read_bytes_be(&xlbuf[0..], 4);
        if xl_high != 0 {
            return Err(Error::InvalidInput(
                "Box size exceeds 4GB, not supported".into(),
            ));
        }
        let xl_low = read_bytes_be(&xlbuf[4..], 4);
        if xl_low < 16 {
            return Err(Error::InvalidInput(format!(
                "Extended-length box too short: {xl_low}"
            )));
        }
        return Ok(Jp2Box {
            length: xl_low,
            box_type,
            header_len: 16,
        });
    }

    if length < 8 {
        return Err(Error::InvalidInput(format!(
            "Box length {length} is less than minimum header size 8"
        )));
    }

    Ok(Jp2Box {
        length,
        box_type,
        header_len: 8,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::image::ImageComp;
    use crate::io::cio::MemoryStream;

    // -----------------------------------------------------------------------
    // Helper: build a minimal valid JP2 file
    // -----------------------------------------------------------------------

    /// Build a JP2 signature box (12 bytes).
    fn build_jp_box() -> Vec<u8> {
        let mut b = Vec::new();
        b.extend_from_slice(&12u32.to_be_bytes()); // length
        b.extend_from_slice(&JP2_JP.to_be_bytes()); // type
        b.extend_from_slice(&JP2_MAGIC.to_be_bytes()); // magic
        b
    }

    /// Build a FTYP box (20 bytes).
    fn build_ftyp_box() -> Vec<u8> {
        let mut b = Vec::new();
        b.extend_from_slice(&20u32.to_be_bytes()); // length
        b.extend_from_slice(&JP2_FTYP.to_be_bytes()); // type
        b.extend_from_slice(&JP2_JP2_BRAND.to_be_bytes()); // brand
        b.extend_from_slice(&0u32.to_be_bytes()); // minversion
        b.extend_from_slice(&JP2_JP2_BRAND.to_be_bytes()); // CL[0]
        b
    }

    /// Build an IHDR sub-box (22 bytes).
    fn build_ihdr_box(w: u32, h: u32, nc: u16, bpc: u8) -> Vec<u8> {
        let mut b = Vec::new();
        b.extend_from_slice(&22u32.to_be_bytes()); // length
        b.extend_from_slice(&JP2_IHDR.to_be_bytes()); // type
        b.extend_from_slice(&h.to_be_bytes()); // HEIGHT
        b.extend_from_slice(&w.to_be_bytes()); // WIDTH
        b.extend_from_slice(&nc.to_be_bytes()); // NC
        b.push(bpc); // BPC
        b.push(7); // C = 7 (JPEG 2000)
        b.push(0); // UnkC
        b.push(0); // IPR
        b
    }

    /// Build a COLR sub-box with enumerated colour space.
    fn build_colr_enumcs_box(enumcs: u32) -> Vec<u8> {
        let mut b = Vec::new();
        b.extend_from_slice(&15u32.to_be_bytes()); // length = 8 + 7
        b.extend_from_slice(&JP2_COLR.to_be_bytes()); // type
        b.push(1); // METH = enumerated
        b.push(0); // PREC
        b.push(0); // APPROX
        b.extend_from_slice(&enumcs.to_be_bytes()); // EnumCS
        b
    }

    /// Build a JP2H super-box containing IHDR and COLR.
    fn build_jp2h_box(w: u32, h: u32, nc: u16, bpc: u8, enumcs: u32) -> Vec<u8> {
        let ihdr = build_ihdr_box(w, h, nc, bpc);
        let colr = build_colr_enumcs_box(enumcs);
        let total_len = 8 + ihdr.len() + colr.len();
        let mut b = Vec::new();
        b.extend_from_slice(&(total_len as u32).to_be_bytes()); // length
        b.extend_from_slice(&JP2_JP2H.to_be_bytes()); // type
        b.extend_from_slice(&ihdr);
        b.extend_from_slice(&colr);
        b
    }

    /// Build a minimal J2K codestream (SOC + SIZ + COD + QCD + SOT + SOD + data + EOC).
    fn build_minimal_j2k(w: u32, h: u32, nc: u16) -> Vec<u8> {
        let mut buf = Vec::new();

        // SOC
        buf.extend_from_slice(&[0xFF, 0x4F]);

        // SIZ marker
        let mut siz_payload = Vec::new();
        siz_payload.extend_from_slice(&0u16.to_be_bytes()); // Rsiz
        siz_payload.extend_from_slice(&w.to_be_bytes()); // Xsiz
        siz_payload.extend_from_slice(&h.to_be_bytes()); // Ysiz
        siz_payload.extend_from_slice(&0u32.to_be_bytes()); // X0siz
        siz_payload.extend_from_slice(&0u32.to_be_bytes()); // Y0siz
        siz_payload.extend_from_slice(&w.to_be_bytes()); // XTsiz
        siz_payload.extend_from_slice(&h.to_be_bytes()); // YTsiz
        siz_payload.extend_from_slice(&0u32.to_be_bytes()); // XT0siz
        siz_payload.extend_from_slice(&0u32.to_be_bytes()); // YT0siz
        siz_payload.extend_from_slice(&nc.to_be_bytes()); // Csiz
        for _ in 0..nc {
            siz_payload.push(0x07); // Ssiz: 8-bit unsigned
            siz_payload.push(0x01); // XRsiz
            siz_payload.push(0x01); // YRsiz
        }
        let siz_len = (siz_payload.len() + 2) as u16;
        buf.extend_from_slice(&[0xFF, 0x51]);
        buf.extend_from_slice(&siz_len.to_be_bytes());
        buf.extend_from_slice(&siz_payload);

        // COD marker
        let mut cod_payload = Vec::new();
        cod_payload.push(0x00); // Scod
        cod_payload.push(0x00); // LRCP
        cod_payload.extend_from_slice(&1u16.to_be_bytes()); // 1 layer
        cod_payload.push(0x00); // no MCT
        cod_payload.push(0x00); // 0 decomp → 1 resolution
        cod_payload.push(0x04); // cblkw
        cod_payload.push(0x04); // cblkh
        cod_payload.push(0x00); // cblksty
        cod_payload.push(0x01); // 5-3 reversible
        let cod_len = (cod_payload.len() + 2) as u16;
        buf.extend_from_slice(&[0xFF, 0x52]);
        buf.extend_from_slice(&cod_len.to_be_bytes());
        buf.extend_from_slice(&cod_payload);

        // QCD marker
        let qcd_payload = vec![0x40, 0x40]; // NOQNT numgbits=2, band0 expn=8
        let qcd_len = (qcd_payload.len() + 2) as u16;
        buf.extend_from_slice(&[0xFF, 0x5C]);
        buf.extend_from_slice(&qcd_len.to_be_bytes());
        buf.extend_from_slice(&qcd_payload);

        // SOT marker (tile 0)
        let tile_data = vec![0u8; 4];
        let psot: u32 = 12 + 2 + tile_data.len() as u32;
        buf.extend_from_slice(&[0xFF, 0x90]);
        buf.extend_from_slice(&10u16.to_be_bytes());
        buf.extend_from_slice(&0u16.to_be_bytes());
        buf.extend_from_slice(&psot.to_be_bytes());
        buf.push(0x00);
        buf.push(0x01);

        // SOD
        buf.extend_from_slice(&[0xFF, 0x93]);
        buf.extend_from_slice(&tile_data);

        // EOC
        buf.extend_from_slice(&[0xFF, 0xD9]);

        buf
    }

    /// Build a JP2C box wrapping a J2K codestream.
    fn build_jp2c_box(j2k: &[u8]) -> Vec<u8> {
        let total_len = 8 + j2k.len();
        let mut b = Vec::new();
        b.extend_from_slice(&(total_len as u32).to_be_bytes());
        b.extend_from_slice(&JP2_JP2C.to_be_bytes());
        b.extend_from_slice(j2k);
        b
    }

    /// Build a complete minimal JP2 file.
    fn build_minimal_jp2() -> Vec<u8> {
        let mut file = Vec::new();
        file.extend_from_slice(&build_jp_box());
        file.extend_from_slice(&build_ftyp_box());
        file.extend_from_slice(&build_jp2h_box(8, 8, 1, 0x07, 17)); // 8x8 gray, 8-bit
        file.extend_from_slice(&build_jp2c_box(&build_minimal_j2k(8, 8, 1)));
        file
    }

    // -----------------------------------------------------------------------
    // Tests: box header reading
    // -----------------------------------------------------------------------

    #[test]
    fn read_box_header_basic() {
        let mut data = Vec::new();
        data.extend_from_slice(&22u32.to_be_bytes());
        data.extend_from_slice(&JP2_IHDR.to_be_bytes());
        data.extend_from_slice(&[0u8; 14]); // dummy payload

        let mut stream = MemoryStream::new_input(data);
        let hdr = read_box_header(&mut stream).unwrap();
        assert_eq!(hdr.length, 22);
        assert_eq!(hdr.box_type, JP2_IHDR);
    }

    #[test]
    fn read_box_header_extended_length() {
        let mut data = Vec::new();
        data.extend_from_slice(&1u32.to_be_bytes()); // length=1 → extended
        data.extend_from_slice(&JP2_JP2C.to_be_bytes());
        data.extend_from_slice(&0u32.to_be_bytes()); // XLBox high (0)
        data.extend_from_slice(&100u32.to_be_bytes()); // XLBox low (100)
        data.extend_from_slice(&[0u8; 84]); // payload (100 - 16 = 84)

        let mut stream = MemoryStream::new_input(data);
        let hdr = read_box_header(&mut stream).unwrap();
        assert_eq!(hdr.length, 100);
        assert_eq!(hdr.box_type, JP2_JP2C);
        assert_eq!(hdr.header_len, 16);
    }

    #[test]
    fn read_box_header_extended_length_too_short() {
        let mut data = Vec::new();
        data.extend_from_slice(&1u32.to_be_bytes()); // length=1 → extended
        data.extend_from_slice(&JP2_JP2C.to_be_bytes());
        data.extend_from_slice(&0u32.to_be_bytes()); // XLBox high
        data.extend_from_slice(&10u32.to_be_bytes()); // XLBox low < 16
        let mut stream = MemoryStream::new_input(data);
        assert!(read_box_header(&mut stream).is_err());
    }

    #[test]
    fn read_box_header_length_too_short() {
        let mut data = Vec::new();
        data.extend_from_slice(&5u32.to_be_bytes()); // length < 8
        data.extend_from_slice(&JP2_IHDR.to_be_bytes());
        let mut stream = MemoryStream::new_input(data);
        assert!(read_box_header(&mut stream).is_err());
    }

    #[test]
    fn read_box_header_last_box() {
        // length=0 means "extends to end of stream"
        let mut data = Vec::new();
        data.extend_from_slice(&0u32.to_be_bytes()); // length=0
        data.extend_from_slice(&JP2_JP2C.to_be_bytes());
        data.extend_from_slice(&[0xAA; 20]); // 20 bytes of data

        let mut stream = MemoryStream::new_input(data);
        let hdr = read_box_header(&mut stream).unwrap();
        assert_eq!(hdr.box_type, JP2_JP2C);
        assert_eq!(hdr.length, 20 + 8); // remaining + header
    }

    // -----------------------------------------------------------------------
    // Tests: individual box readers
    // -----------------------------------------------------------------------

    #[test]
    fn read_jp_signature_valid() {
        let mut dec = Jp2Decoder::new();
        dec.read_jp(&JP2_MAGIC.to_be_bytes()).unwrap();
        assert_eq!(dec.state, Jp2State::Signature);
    }

    #[test]
    fn read_jp_signature_bad_magic() {
        let mut dec = Jp2Decoder::new();
        let result = dec.read_jp(&0xDEADBEEFu32.to_be_bytes());
        assert!(result.is_err());
    }

    #[test]
    fn read_jp_signature_wrong_state() {
        let mut dec = Jp2Decoder::new();
        dec.state = Jp2State::Signature; // already past initial state
        let result = dec.read_jp(&JP2_MAGIC.to_be_bytes());
        assert!(result.is_err());
    }

    #[test]
    fn read_ftyp_valid() {
        let mut dec = Jp2Decoder::new();
        dec.state = Jp2State::Signature;
        let mut data = Vec::new();
        data.extend_from_slice(&JP2_JP2_BRAND.to_be_bytes()); // brand
        data.extend_from_slice(&0u32.to_be_bytes()); // minversion
        data.extend_from_slice(&JP2_JP2_BRAND.to_be_bytes()); // CL[0]
        dec.read_ftyp(&data).unwrap();
        assert_eq!(dec.state, Jp2State::FileType);
    }

    #[test]
    fn read_ftyp_wrong_state() {
        let mut dec = Jp2Decoder::new();
        // state is None, not Signature
        let data = vec![0u8; 12];
        let result = dec.read_ftyp(&data);
        assert!(result.is_err());
    }

    #[test]
    fn read_ihdr_valid() {
        let mut dec = Jp2Decoder::new();
        let data = {
            let mut d = Vec::new();
            d.extend_from_slice(&100u32.to_be_bytes()); // HEIGHT
            d.extend_from_slice(&200u32.to_be_bytes()); // WIDTH
            d.extend_from_slice(&3u16.to_be_bytes()); // NC
            d.push(0x07); // BPC: 8-bit unsigned (raw encoding: (prec-1) | sign<<7)
            d.push(7); // C
            d.push(0); // UnkC
            d.push(0); // IPR
            d
        };
        dec.read_ihdr(&data).unwrap();
        assert_eq!(dec.height, 100);
        assert_eq!(dec.width, 200);
        assert_eq!(dec.numcomps, 3);
        assert_eq!(dec.bpc, 0x07);
        assert_eq!(dec.compression_type, 7);
        assert!(dec.ihdr_found);
        assert_eq!(dec.comp_info.len(), 3);
        // Uniform BPC 0x07 decodes to 8-bit unsigned
        for ci in &dec.comp_info {
            assert_eq!(ci.prec, 8);
            assert!(!ci.sgnd);
        }
    }

    #[test]
    fn read_ihdr_zero_dimensions_fails() {
        let mut dec = Jp2Decoder::new();
        let mut data = Vec::new();
        data.extend_from_slice(&0u32.to_be_bytes()); // HEIGHT=0
        data.extend_from_slice(&200u32.to_be_bytes());
        data.extend_from_slice(&3u16.to_be_bytes());
        data.push(8);
        data.push(7);
        data.push(0);
        data.push(0);
        assert!(dec.read_ihdr(&data).is_err());
    }

    #[test]
    fn read_ihdr_bad_compression_type_fails() {
        let mut dec = Jp2Decoder::new();
        let mut data = Vec::new();
        data.extend_from_slice(&100u32.to_be_bytes()); // HEIGHT
        data.extend_from_slice(&200u32.to_be_bytes()); // WIDTH
        data.extend_from_slice(&3u16.to_be_bytes()); // NC
        data.push(0x07); // BPC
        data.push(5); // C = 5 (not 7)
        data.push(0); // UnkC
        data.push(0); // IPR
        assert!(dec.read_ihdr(&data).is_err());
    }

    #[test]
    fn read_ihdr_bad_size_fails() {
        let mut dec = Jp2Decoder::new();
        let data = vec![0u8; 10]; // too short
        assert!(dec.read_ihdr(&data).is_err());
    }

    #[test]
    fn read_colr_enumerated() {
        let mut dec = Jp2Decoder::new();
        let mut data = Vec::new();
        data.push(1); // METH = enumerated
        data.push(0); // PREC
        data.push(0); // APPROX
        data.extend_from_slice(&16u32.to_be_bytes()); // EnumCS = sRGB
        dec.read_colr(&data).unwrap();
        assert_eq!(dec.colour.meth, ColourMethod::Enumerated);
        assert_eq!(dec.colour.enumcs, 16);
        assert!(dec.colr_found);
    }

    #[test]
    fn read_colr_icc() {
        let mut dec = Jp2Decoder::new();
        let mut data = Vec::new();
        data.push(2); // METH = ICC
        data.push(0); // PREC
        data.push(0); // APPROX
        data.extend_from_slice(&[0xDE, 0xAD, 0xBE, 0xEF]); // fake ICC
        dec.read_colr(&data).unwrap();
        assert_eq!(dec.colour.meth, ColourMethod::Icc);
        assert_eq!(dec.colour.icc_profile, vec![0xDE, 0xAD, 0xBE, 0xEF]);
    }

    #[test]
    fn read_colr_duplicate_ignored() {
        let mut dec = Jp2Decoder::new();
        let mut data = Vec::new();
        data.push(1);
        data.push(0);
        data.push(0);
        data.extend_from_slice(&16u32.to_be_bytes());
        dec.read_colr(&data).unwrap();
        assert_eq!(dec.colour.enumcs, 16);

        // Second COLR should be ignored
        let mut data2 = Vec::new();
        data2.push(1);
        data2.push(0);
        data2.push(0);
        data2.extend_from_slice(&17u32.to_be_bytes());
        dec.read_colr(&data2).unwrap();
        assert_eq!(dec.colour.enumcs, 16); // still 16, not 17
    }

    #[test]
    fn read_bpcc_valid() {
        let mut dec = Jp2Decoder::new();
        // First set up IHDR with bpc=255 (varies)
        let mut ihdr = Vec::new();
        ihdr.extend_from_slice(&10u32.to_be_bytes());
        ihdr.extend_from_slice(&10u32.to_be_bytes());
        ihdr.extend_from_slice(&3u16.to_be_bytes());
        ihdr.push(255); // BPC=255 → per-component
        ihdr.push(7);
        ihdr.push(0);
        ihdr.push(0);
        dec.read_ihdr(&ihdr).unwrap();

        // Now read BPCC (raw values: 0x07=8bit unsigned, 0x89=10bit signed, 0x0B=12bit unsigned)
        let bpcc_data = vec![0x07, 0x89, 0x0B];
        dec.read_bpcc(&bpcc_data).unwrap();
        assert_eq!(dec.comp_info[0].prec, 8);
        assert!(!dec.comp_info[0].sgnd);
        assert_eq!(dec.comp_info[1].prec, 10);
        assert!(dec.comp_info[1].sgnd);
        assert_eq!(dec.comp_info[2].prec, 12);
        assert!(!dec.comp_info[2].sgnd);
    }

    #[test]
    fn read_bpcc_before_ihdr_fails() {
        let mut dec = Jp2Decoder::new();
        let data = vec![8, 10, 12];
        assert!(dec.read_bpcc(&data).is_err());
    }

    #[test]
    fn read_bpcc_wrong_size_fails() {
        let mut dec = Jp2Decoder::new();
        let mut ihdr = Vec::new();
        ihdr.extend_from_slice(&10u32.to_be_bytes());
        ihdr.extend_from_slice(&10u32.to_be_bytes());
        ihdr.extend_from_slice(&3u16.to_be_bytes());
        ihdr.push(255);
        ihdr.push(7);
        ihdr.push(0);
        ihdr.push(0);
        dec.read_ihdr(&ihdr).unwrap();

        let data = vec![8, 10]; // 2 bytes for 3 components
        assert!(dec.read_bpcc(&data).is_err());
    }

    // -----------------------------------------------------------------------
    // Tests: JP2H super-box parsing
    // -----------------------------------------------------------------------

    #[test]
    fn read_jp2h_with_ihdr_and_colr() {
        let mut dec = Jp2Decoder::new();
        dec.state = Jp2State::FileType;

        let ihdr = build_ihdr_box(8, 8, 1, 0x07);
        let colr = build_colr_enumcs_box(17); // gray
        let mut payload = Vec::new();
        payload.extend_from_slice(&ihdr);
        payload.extend_from_slice(&colr);

        dec.read_jp2h(&payload).unwrap();
        assert_eq!(dec.state, Jp2State::Header);
        assert_eq!(dec.width, 8);
        assert_eq!(dec.height, 8);
        assert_eq!(dec.numcomps, 1);
        assert!(dec.colr_found);
    }

    #[test]
    fn read_jp2h_missing_colr_fails() {
        let mut dec = Jp2Decoder::new();
        dec.state = Jp2State::FileType;

        // JP2H with only IHDR, no COLR
        let ihdr = build_ihdr_box(8, 8, 1, 0x07);
        assert!(dec.read_jp2h(&ihdr).is_err());
    }

    #[test]
    fn read_jp2h_missing_ihdr_fails() {
        let mut dec = Jp2Decoder::new();
        dec.state = Jp2State::FileType;

        // JP2H with only COLR, no IHDR
        let colr = build_colr_enumcs_box(17);
        assert!(dec.read_jp2h(&colr).is_err());
    }

    // -----------------------------------------------------------------------
    // Tests: full JP2 header reading
    // -----------------------------------------------------------------------

    #[test]
    fn read_jp2_header_minimal() {
        let jp2_data = build_minimal_jp2();
        let mut stream = MemoryStream::new_input(jp2_data);
        let mut dec = Jp2Decoder::new();

        dec.read_header(&mut stream).unwrap();

        assert_eq!(dec.state, Jp2State::Codestream);
        assert_eq!(dec.width, 8);
        assert_eq!(dec.height, 8);
        assert_eq!(dec.numcomps, 1);
        assert_eq!(dec.bpc, 0x07);
        assert!(dec.ihdr_found);
        assert!(dec.colr_found);
    }

    #[test]
    fn read_jp2_header_then_codestream() {
        let jp2_data = build_minimal_jp2();
        let mut stream = MemoryStream::new_input(jp2_data);
        let mut dec = Jp2Decoder::new();

        dec.read_header(&mut stream).unwrap();
        dec.read_codestream(&mut stream).unwrap();

        // J2K decoder should have parsed the codestream
        assert_eq!(dec.j2k.image.x1, 8);
        assert_eq!(dec.j2k.image.y1, 8);
        assert_eq!(dec.j2k.image.comps.len(), 1);
        assert_eq!(dec.j2k.tile_data.len(), 1);
    }

    #[test]
    fn read_jp2_colour_applied_to_image() {
        let jp2_data = build_minimal_jp2(); // gray (enumcs=17)
        let mut stream = MemoryStream::new_input(jp2_data);
        let mut dec = Jp2Decoder::new();

        dec.read_header(&mut stream).unwrap();
        dec.read_codestream(&mut stream).unwrap();

        assert_eq!(dec.j2k.image.color_space, ColorSpace::Gray);
    }

    #[test]
    fn read_jp2_srgb_colour() {
        let mut file = Vec::new();
        file.extend_from_slice(&build_jp_box());
        file.extend_from_slice(&build_ftyp_box());
        file.extend_from_slice(&build_jp2h_box(8, 8, 3, 0x07, 16)); // sRGB, 8-bit
        file.extend_from_slice(&build_jp2c_box(&build_minimal_j2k(8, 8, 3)));

        let mut stream = MemoryStream::new_input(file);
        let mut dec = Jp2Decoder::new();
        dec.read_header(&mut stream).unwrap();
        dec.read_codestream(&mut stream).unwrap();

        assert_eq!(dec.j2k.image.color_space, ColorSpace::Srgb);
        assert_eq!(dec.j2k.image.comps.len(), 3);
    }

    #[test]
    fn read_jp2_jp2c_before_header_fails() {
        // JP2C without JP/FTYP/JP2H
        let j2k = build_minimal_j2k(8, 8, 1);
        let mut file = Vec::new();
        file.extend_from_slice(&build_jp_box());
        file.extend_from_slice(&build_ftyp_box());
        // Skip JP2H, go straight to JP2C
        file.extend_from_slice(&build_jp2c_box(&j2k));

        let mut stream = MemoryStream::new_input(file);
        let mut dec = Jp2Decoder::new();
        assert!(dec.read_header(&mut stream).is_err());
    }

    #[test]
    fn read_codestream_before_header_fails() {
        let jp2_data = build_minimal_jp2();
        let mut stream = MemoryStream::new_input(jp2_data);
        let mut dec = Jp2Decoder::new();
        // Don't call read_header first
        assert!(dec.read_codestream(&mut stream).is_err());
    }

    // -----------------------------------------------------------------------
    // Helpers: optional box builders
    // -----------------------------------------------------------------------

    /// Build a CDEF sub-box.
    fn build_cdef_box(entries: &[(u16, u16, u16)]) -> Vec<u8> {
        let payload_len = 2 + entries.len() * 6;
        let total_len = 8 + payload_len;
        let mut b = Vec::new();
        b.extend_from_slice(&(total_len as u32).to_be_bytes());
        b.extend_from_slice(&JP2_CDEF.to_be_bytes());
        b.extend_from_slice(&(entries.len() as u16).to_be_bytes());
        for &(cn, typ, asoc) in entries {
            b.extend_from_slice(&cn.to_be_bytes());
            b.extend_from_slice(&typ.to_be_bytes());
            b.extend_from_slice(&asoc.to_be_bytes());
        }
        b
    }

    /// Build a PCLR sub-box.
    /// `bpc_raw` contains raw bit-depth values per column (same encoding as BPC).
    /// `entries` is `[entry0_col0, entry0_col1, ..., entry1_col0, ...]`.
    fn build_pclr_box(
        nr_entries: u16,
        nr_channels: u8,
        bpc_raw: &[u8],
        entries: &[u32],
    ) -> Vec<u8> {
        // Compute bytes per column value
        let bytes_per: Vec<usize> = bpc_raw
            .iter()
            .map(|&b| {
                let bits = (b & 0x7F) as usize + 1;
                bits.div_ceil(8)
            })
            .collect();
        let entry_size: usize = bytes_per.iter().sum();
        let payload_len = 3 + nr_channels as usize + nr_entries as usize * entry_size;
        let total_len = 8 + payload_len;
        let mut b = Vec::new();
        b.extend_from_slice(&(total_len as u32).to_be_bytes());
        b.extend_from_slice(&JP2_PCLR.to_be_bytes());
        b.extend_from_slice(&nr_entries.to_be_bytes());
        b.push(nr_channels);
        b.extend_from_slice(bpc_raw);
        for (idx, &val) in entries.iter().enumerate() {
            let col = idx % nr_channels as usize;
            let nbytes = bytes_per[col];
            for i in (0..nbytes).rev() {
                b.push((val >> (i * 8)) as u8);
            }
        }
        b
    }

    /// Build a CMAP sub-box.
    fn build_cmap_box(entries: &[(u16, u8, u8)]) -> Vec<u8> {
        let payload_len = entries.len() * 4;
        let total_len = 8 + payload_len;
        let mut b = Vec::new();
        b.extend_from_slice(&(total_len as u32).to_be_bytes());
        b.extend_from_slice(&JP2_CMAP.to_be_bytes());
        for &(cmp, mtyp, pcol) in entries {
            b.extend_from_slice(&cmp.to_be_bytes());
            b.push(mtyp);
            b.push(pcol);
        }
        b
    }

    /// Build a JP2H box with custom sub-boxes.
    #[allow(dead_code)]
    fn build_jp2h_box_custom(sub_boxes: &[&[u8]]) -> Vec<u8> {
        let content_len: usize = sub_boxes.iter().map(|b| b.len()).sum();
        let total_len = 8 + content_len;
        let mut b = Vec::new();
        b.extend_from_slice(&(total_len as u32).to_be_bytes());
        b.extend_from_slice(&JP2_JP2H.to_be_bytes());
        for sub in sub_boxes {
            b.extend_from_slice(sub);
        }
        b
    }

    // -----------------------------------------------------------------------
    // Tests: CDEF box reading
    // -----------------------------------------------------------------------

    #[test]

    fn read_cdef_valid_rgb_with_alpha() {
        let mut dec = Jp2Decoder::new();
        dec.numcomps = 4;
        dec.ihdr_found = true;
        // 4 channels: R(colour,1), G(colour,2), B(colour,3), A(opacity,0)
        let data = {
            let mut d = Vec::new();
            d.extend_from_slice(&4u16.to_be_bytes()); // N=4
            // cn=0, typ=0(colour), asoc=1(R)
            d.extend_from_slice(&0u16.to_be_bytes());
            d.extend_from_slice(&0u16.to_be_bytes());
            d.extend_from_slice(&1u16.to_be_bytes());
            // cn=1, typ=0(colour), asoc=2(G)
            d.extend_from_slice(&1u16.to_be_bytes());
            d.extend_from_slice(&0u16.to_be_bytes());
            d.extend_from_slice(&2u16.to_be_bytes());
            // cn=2, typ=0(colour), asoc=3(B)
            d.extend_from_slice(&2u16.to_be_bytes());
            d.extend_from_slice(&0u16.to_be_bytes());
            d.extend_from_slice(&3u16.to_be_bytes());
            // cn=3, typ=1(opacity), asoc=0(whole image)
            d.extend_from_slice(&3u16.to_be_bytes());
            d.extend_from_slice(&1u16.to_be_bytes());
            d.extend_from_slice(&0u16.to_be_bytes());
            d
        };
        dec.read_cdef(&data).unwrap();
        let cdef = dec.cdef.as_ref().unwrap();
        assert_eq!(cdef.len(), 4);
        assert_eq!(
            cdef[0],
            CdefEntry {
                cn: 0,
                typ: 0,
                asoc: 1
            }
        );
        assert_eq!(
            cdef[3],
            CdefEntry {
                cn: 3,
                typ: 1,
                asoc: 0
            }
        );
    }

    #[test]

    fn read_cdef_too_short_fails() {
        let mut dec = Jp2Decoder::new();
        dec.numcomps = 1;
        dec.ihdr_found = true;
        let data = vec![0u8; 1]; // too short: needs at least 2 bytes for N
        assert!(dec.read_cdef(&data).is_err());
    }

    #[test]

    fn read_cdef_zero_count_fails() {
        let mut dec = Jp2Decoder::new();
        dec.numcomps = 1;
        dec.ihdr_found = true;
        let data = 0u16.to_be_bytes().to_vec(); // N=0
        assert!(dec.read_cdef(&data).is_err());
    }

    #[test]

    fn read_cdef_truncated_entries_fails() {
        let mut dec = Jp2Decoder::new();
        dec.numcomps = 2;
        dec.ihdr_found = true;
        let mut data = Vec::new();
        data.extend_from_slice(&2u16.to_be_bytes()); // N=2
        // Only provide 1 entry (6 bytes) instead of 2
        data.extend_from_slice(&[0u8; 6]);
        assert!(dec.read_cdef(&data).is_err());
    }

    #[test]

    fn read_cdef_duplicate_fails() {
        let mut dec = Jp2Decoder::new();
        dec.numcomps = 1;
        dec.ihdr_found = true;
        let mut data = Vec::new();
        data.extend_from_slice(&1u16.to_be_bytes());
        data.extend_from_slice(&0u16.to_be_bytes()); // cn
        data.extend_from_slice(&0u16.to_be_bytes()); // typ
        data.extend_from_slice(&1u16.to_be_bytes()); // asoc
        dec.read_cdef(&data).unwrap();
        // Second CDEF should fail
        assert!(dec.read_cdef(&data).is_err());
    }

    // -----------------------------------------------------------------------
    // Tests: PCLR box reading
    // -----------------------------------------------------------------------

    #[test]

    fn read_pclr_valid() {
        let mut dec = Jp2Decoder::new();
        dec.ihdr_found = true;
        // 4 entries, 3 columns (RGB), 8-bit each
        let mut data = Vec::new();
        data.extend_from_slice(&4u16.to_be_bytes()); // NE=4
        data.push(3); // NPC=3
        data.push(0x07); // Bi[0]: 8-bit unsigned
        data.push(0x07); // Bi[1]: 8-bit unsigned
        data.push(0x07); // Bi[2]: 8-bit unsigned
        // 4 entries × 3 columns, each 1 byte
        let palette = [
            255, 0, 0, // entry 0: red
            0, 255, 0, // entry 1: green
            0, 0, 255, // entry 2: blue
            255, 255, 255, // entry 3: white
        ];
        data.extend_from_slice(&palette);

        dec.read_pclr(&data).unwrap();
        let pclr = dec.pclr.as_ref().unwrap();
        assert_eq!(pclr.nr_entries, 4);
        assert_eq!(pclr.nr_channels, 3);
        assert_eq!(pclr.channel_size, vec![8, 8, 8]);
        assert_eq!(pclr.channel_sign, vec![false, false, false]);
        // entry 0: (255, 0, 0)
        assert_eq!(pclr.entries[0], 255);
        assert_eq!(pclr.entries[1], 0);
        assert_eq!(pclr.entries[2], 0);
        // entry 2: (0, 0, 255)
        assert_eq!(pclr.entries[6], 0);
        assert_eq!(pclr.entries[7], 0);
        assert_eq!(pclr.entries[8], 255);
    }

    #[test]

    fn read_pclr_too_short_fails() {
        let mut dec = Jp2Decoder::new();
        dec.ihdr_found = true;
        let data = vec![0u8; 2]; // need at least 3 bytes
        assert!(dec.read_pclr(&data).is_err());
    }

    #[test]

    fn read_pclr_zero_entries_fails() {
        let mut dec = Jp2Decoder::new();
        dec.ihdr_found = true;
        let mut data = Vec::new();
        data.extend_from_slice(&0u16.to_be_bytes()); // NE=0
        data.push(1); // NPC=1
        data.push(0x07);
        assert!(dec.read_pclr(&data).is_err());
    }

    #[test]

    fn read_pclr_duplicate_fails() {
        let mut dec = Jp2Decoder::new();
        dec.ihdr_found = true;
        let mut data = Vec::new();
        data.extend_from_slice(&1u16.to_be_bytes()); // NE=1
        data.push(1); // NPC=1
        data.push(0x07);
        data.push(128); // 1 entry, 1 column
        dec.read_pclr(&data).unwrap();
        assert!(dec.read_pclr(&data).is_err());
    }

    // -----------------------------------------------------------------------
    // Tests: CMAP box reading
    // -----------------------------------------------------------------------

    #[test]

    fn read_cmap_valid() {
        let mut dec = Jp2Decoder::new();
        dec.ihdr_found = true;
        dec.numcomps = 1;
        // Set up PCLR with 3 channels first
        dec.pclr = Some(Pclr {
            entries: vec![0; 12],
            channel_sign: vec![false; 3],
            channel_size: vec![8; 3],
            nr_entries: 4,
            nr_channels: 3,
        });
        // CMAP: 3 entries (one per palette column)
        // comp 0 → palette col 0 (mtyp=1)
        // comp 0 → palette col 1 (mtyp=1)
        // comp 0 → palette col 2 (mtyp=1)
        let mut data = Vec::new();
        for pcol in 0u8..3 {
            data.extend_from_slice(&0u16.to_be_bytes()); // CMP=0
            data.push(1); // MTYP=1 (palette)
            data.push(pcol); // PCOL
        }
        dec.read_cmap(&data).unwrap();
        let cmap = dec.cmap.as_ref().unwrap();
        assert_eq!(cmap.len(), 3);
        assert_eq!(
            cmap[0],
            CmapEntry {
                cmp: 0,
                mtyp: 1,
                pcol: 0
            }
        );
        assert_eq!(
            cmap[2],
            CmapEntry {
                cmp: 0,
                mtyp: 1,
                pcol: 2
            }
        );
    }

    #[test]

    fn read_cmap_without_pclr_fails() {
        let mut dec = Jp2Decoder::new();
        dec.ihdr_found = true;
        dec.numcomps = 1;
        let mut data = Vec::new();
        data.extend_from_slice(&0u16.to_be_bytes());
        data.push(0);
        data.push(0);
        assert!(dec.read_cmap(&data).is_err());
    }

    #[test]

    fn read_cmap_wrong_size_fails() {
        let mut dec = Jp2Decoder::new();
        dec.ihdr_found = true;
        dec.numcomps = 1;
        dec.pclr = Some(Pclr {
            entries: vec![0; 3],
            channel_sign: vec![false; 3],
            channel_size: vec![8; 3],
            nr_entries: 1,
            nr_channels: 3,
        });
        // Only 2 entries instead of 3
        let mut data = Vec::new();
        for _ in 0..2 {
            data.extend_from_slice(&0u16.to_be_bytes());
            data.push(1);
            data.push(0);
        }
        assert!(dec.read_cmap(&data).is_err());
    }

    // -----------------------------------------------------------------------
    // Tests: CDEF application
    // -----------------------------------------------------------------------

    #[test]

    fn apply_cdef_marks_alpha() {
        let mut dec = Jp2Decoder::new();
        dec.state = Jp2State::Codestream;
        // Set up a 4-component image (RGBA)
        dec.j2k.image.comps = vec![
            ImageComp {
                prec: 8,
                ..ImageComp::default()
            },
            ImageComp {
                prec: 8,
                ..ImageComp::default()
            },
            ImageComp {
                prec: 8,
                ..ImageComp::default()
            },
            ImageComp {
                prec: 8,
                ..ImageComp::default()
            },
        ];
        // CDEF: channels 0-2 = colour, channel 3 = opacity
        dec.cdef = Some(vec![
            CdefEntry {
                cn: 0,
                typ: 0,
                asoc: 1,
            },
            CdefEntry {
                cn: 1,
                typ: 0,
                asoc: 2,
            },
            CdefEntry {
                cn: 2,
                typ: 0,
                asoc: 3,
            },
            CdefEntry {
                cn: 3,
                typ: 1,
                asoc: 0,
            }, // alpha
        ]);
        dec.apply_cdef();
        assert_eq!(dec.j2k.image.comps[3].alpha, 1);
        assert_eq!(dec.j2k.image.comps[0].alpha, 0);
    }

    #[test]

    fn apply_cdef_swaps_components() {
        let mut dec = Jp2Decoder::new();
        dec.state = Jp2State::Codestream;
        // Set up a 3-component image with data to verify swap
        dec.j2k.image.comps = vec![
            ImageComp {
                prec: 8,
                data: vec![10],
                ..ImageComp::default()
            },
            ImageComp {
                prec: 8,
                data: vec![20],
                ..ImageComp::default()
            },
            ImageComp {
                prec: 8,
                data: vec![30],
                ..ImageComp::default()
            },
        ];
        // CDEF maps: channel 0→asoc 3(B), channel 1→asoc 1(R), channel 2→asoc 2(G)
        // So file order is BGR, should be reordered to RGB
        dec.cdef = Some(vec![
            CdefEntry {
                cn: 0,
                typ: 0,
                asoc: 3,
            }, // channel 0 is B
            CdefEntry {
                cn: 1,
                typ: 0,
                asoc: 1,
            }, // channel 1 is R
            CdefEntry {
                cn: 2,
                typ: 0,
                asoc: 2,
            }, // channel 2 is G
        ]);
        dec.apply_cdef();
        // After swap: comp[0]=R(20), comp[1]=G(30), comp[2]=B(10)
        assert_eq!(dec.j2k.image.comps[0].data, vec![20]);
        assert_eq!(dec.j2k.image.comps[1].data, vec![30]);
        assert_eq!(dec.j2k.image.comps[2].data, vec![10]);
    }

    // -----------------------------------------------------------------------
    // Tests: PCLR + CMAP application
    // -----------------------------------------------------------------------

    #[test]

    fn apply_pclr_expands_palette() {
        let mut dec = Jp2Decoder::new();
        dec.state = Jp2State::Codestream;
        // 1 component with palette indices: [0, 1, 2, 3]
        dec.j2k.image.comps = vec![ImageComp {
            prec: 8,
            w: 2,
            h: 2,
            data: vec![0, 1, 2, 3],
            ..ImageComp::default()
        }];
        dec.j2k.image.x1 = 2;
        dec.j2k.image.y1 = 2;
        // Palette: 4 entries, 3 columns (RGB)
        dec.pclr = Some(Pclr {
            entries: vec![
                255, 0, 0, // entry 0: red
                0, 255, 0, // entry 1: green
                0, 0, 255, // entry 2: blue
                255, 255, 255, // entry 3: white
            ],
            channel_sign: vec![false, false, false],
            channel_size: vec![8, 8, 8],
            nr_entries: 4,
            nr_channels: 3,
        });
        // CMAP: all palette mapping from component 0
        dec.cmap = Some(vec![
            CmapEntry {
                cmp: 0,
                mtyp: 1,
                pcol: 0,
            },
            CmapEntry {
                cmp: 0,
                mtyp: 1,
                pcol: 1,
            },
            CmapEntry {
                cmp: 0,
                mtyp: 1,
                pcol: 2,
            },
        ]);
        dec.apply_pclr();

        // Should now have 3 components
        assert_eq!(dec.j2k.image.comps.len(), 3);
        // Component 0 (R): [255, 0, 0, 255]
        assert_eq!(dec.j2k.image.comps[0].data, vec![255, 0, 0, 255]);
        // Component 1 (G): [0, 255, 0, 255]
        assert_eq!(dec.j2k.image.comps[1].data, vec![0, 255, 0, 255]);
        // Component 2 (B): [0, 0, 255, 255]
        assert_eq!(dec.j2k.image.comps[2].data, vec![0, 0, 255, 255]);
    }

    #[test]

    fn apply_pclr_clamps_index() {
        let mut dec = Jp2Decoder::new();
        dec.state = Jp2State::Codestream;
        // Indices include out-of-range value
        dec.j2k.image.comps = vec![ImageComp {
            prec: 8,
            w: 2,
            h: 1,
            data: vec![-1, 10], // -1 clamps to 0, 10 clamps to 1 (max index)
            ..ImageComp::default()
        }];
        dec.pclr = Some(Pclr {
            entries: vec![100, 200],
            channel_sign: vec![false],
            channel_size: vec![8],
            nr_entries: 2,
            nr_channels: 1,
        });
        dec.cmap = Some(vec![CmapEntry {
            cmp: 0,
            mtyp: 1,
            pcol: 0,
        }]);
        dec.apply_pclr();

        assert_eq!(dec.j2k.image.comps[0].data, vec![100, 200]);
    }

    #[test]

    fn apply_pclr_direct_mapping() {
        let mut dec = Jp2Decoder::new();
        dec.state = Jp2State::Codestream;
        // 2 components: comp 0 has palette indices, comp 1 is direct (e.g., alpha)
        dec.j2k.image.comps = vec![
            ImageComp {
                prec: 8,
                w: 2,
                h: 1,
                data: vec![0, 1],
                ..ImageComp::default()
            },
            ImageComp {
                prec: 8,
                w: 2,
                h: 1,
                data: vec![128, 255],
                ..ImageComp::default()
            },
        ];
        dec.pclr = Some(Pclr {
            entries: vec![10, 20],
            channel_sign: vec![false],
            channel_size: vec![8],
            nr_entries: 2,
            nr_channels: 1,
        });
        // CMAP: channel 0 = palette from comp 0, channel 1 = direct from comp 1
        dec.cmap = Some(vec![
            CmapEntry {
                cmp: 0,
                mtyp: 1,
                pcol: 0,
            },
            CmapEntry {
                cmp: 1,
                mtyp: 0,
                pcol: 0,
            },
        ]);
        dec.apply_pclr();

        assert_eq!(dec.j2k.image.comps.len(), 2);
        assert_eq!(dec.j2k.image.comps[0].data, vec![10, 20]); // palette-expanded
        assert_eq!(dec.j2k.image.comps[1].data, vec![128, 255]); // direct pass-through
    }

    // -----------------------------------------------------------------------
    // Tests: JP2H integration with optional boxes
    // -----------------------------------------------------------------------

    #[test]

    fn read_jp2h_with_cdef() {
        let mut dec = Jp2Decoder::new();
        dec.state = Jp2State::FileType;

        let ihdr = build_ihdr_box(8, 8, 3, 0x07);
        let colr = build_colr_enumcs_box(16); // sRGB
        let cdef = build_cdef_box(&[
            (0, 0, 1), // R
            (1, 0, 2), // G
            (2, 0, 3), // B
        ]);
        let mut payload = Vec::new();
        payload.extend_from_slice(&ihdr);
        payload.extend_from_slice(&colr);
        payload.extend_from_slice(&cdef);

        dec.read_jp2h(&payload).unwrap();
        assert!(dec.cdef.is_some());
        assert_eq!(dec.cdef.as_ref().unwrap().len(), 3);
    }

    #[test]

    fn read_jp2h_with_pclr_and_cmap() {
        let mut dec = Jp2Decoder::new();
        dec.state = Jp2State::FileType;

        let ihdr = build_ihdr_box(8, 8, 1, 0x07);
        let colr = build_colr_enumcs_box(16);
        let pclr = build_pclr_box(
            2,
            3,
            &[0x07, 0x07, 0x07],
            &[
                255, 0, 0, // entry 0
                0, 255, 0, // entry 1
            ],
        );
        let cmap = build_cmap_box(&[
            (0, 1, 0), // comp 0 → palette col 0
            (0, 1, 1), // comp 0 → palette col 1
            (0, 1, 2), // comp 0 → palette col 2
        ]);
        let mut payload = Vec::new();
        payload.extend_from_slice(&ihdr);
        payload.extend_from_slice(&colr);
        payload.extend_from_slice(&pclr);
        payload.extend_from_slice(&cmap);

        dec.read_jp2h(&payload).unwrap();
        assert!(dec.pclr.is_some());
        assert!(dec.cmap.is_some());
    }
}
