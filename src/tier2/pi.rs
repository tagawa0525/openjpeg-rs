// Phase 400b: Packet Iterator (C: opj_pi_iterator_t)
//
// Iterates over packets in a tile following one of five JPEG 2000 progression
// orders: LRCP, RLCP, RPCL, PCRL, CPRL. Used by T2 to read/write packets
// in the correct order.

use crate::j2k::params::Poc;
use crate::types::ProgressionOrder;

// ---------------------------------------------------------------------------
// Data structures
// ---------------------------------------------------------------------------

/// Resolution-level information for a PI component (C: opj_pi_resolution_t).
#[derive(Debug, Default, Clone)]
pub struct PiResolution {
    /// Precinct width exponent (log2).
    pub pdx: u32,
    /// Precinct height exponent (log2).
    pub pdy: u32,
    /// Number of precincts in width.
    pub pw: u32,
    /// Number of precincts in height.
    pub ph: u32,
}

/// Component information for the PI (C: opj_pi_comp_t).
#[derive(Debug, Default, Clone)]
pub struct PiComp {
    /// Component sub-sampling factor X.
    pub dx: u32,
    /// Component sub-sampling factor Y.
    pub dy: u32,
    /// Number of resolution levels.
    pub numresolutions: u32,
    /// Per-resolution information.
    pub resolutions: Vec<PiResolution>,
}

/// Packet iterator state (C: opj_pi_iterator_t).
///
/// Tracks the current packet position (layno, resno, compno, precno) and
/// advances through packets according to the progression order.
#[derive(Debug, Clone)]
pub struct PiIterator {
    /// Tile-part generation enabled.
    pub tp_on: bool,
    /// Stride for layer dimension in include array.
    pub step_l: u32,
    /// Stride for resolution dimension.
    pub step_r: u32,
    /// Stride for component dimension.
    pub step_c: u32,
    /// Stride for precinct dimension.
    pub step_p: u32,
    /// Current component index.
    pub compno: u32,
    /// Current resolution index.
    pub resno: u32,
    /// Current precinct index.
    pub precno: u32,
    /// Current layer index.
    pub layno: u32,
    /// Whether this is the first call to next().
    pub first: bool,
    /// Progression order change parameters (bounds for iteration).
    pub poc: Poc,
    /// Number of components.
    pub numcomps: u32,
    /// Component information.
    pub comps: Vec<PiComp>,
    /// Tile boundaries.
    pub tx0: u32,
    pub ty0: u32,
    pub tx1: u32,
    pub ty1: u32,
    /// Current spatial position (for RPCL/PCRL/CPRL).
    pub x: u32,
    pub y: u32,
    /// Precinct spacing in image coordinates (for RPCL/PCRL/CPRL).
    pub dx: u32,
    pub dy: u32,
}

/// A set of packet iterators for a tile, with shared include array.
///
/// The include array tracks which packets have already been visited,
/// preventing duplicate processing across multiple POC entries.
pub struct PacketIterators {
    /// Per-POC iterators.
    pub iterators: Vec<PiIterator>,
    /// Shared include array (flattened: layno * step_l + resno * step_r + ...).
    pub include: Vec<i16>,
}

impl PacketIterators {
    /// Advance the iterator at index `pino` to the next unvisited packet.
    /// Returns `true` if a packet was found, `false` if exhausted.
    pub fn next(&mut self, pino: usize) -> bool {
        let Some(pi) = self.iterators.get_mut(pino) else {
            return false;
        };
        pi_next(pi, &mut self.include)
    }

    /// Returns the number of POC iterators.
    pub fn len(&self) -> usize {
        self.iterators.len()
    }

    /// Returns `true` if there are no iterators.
    pub fn is_empty(&self) -> bool {
        self.iterators.is_empty()
    }

    /// Returns a reference to the iterator at index `pino`.
    pub fn get(&self, pino: usize) -> &PiIterator {
        &self.iterators[pino]
    }
}

// ---------------------------------------------------------------------------
// Progression order functions
// ---------------------------------------------------------------------------

/// Compute flattened include array index using u64 to avoid u32 overflow.
/// Returns `None` if the index is out of bounds.
#[inline]
fn include_index(
    pi: &PiIterator,
    layno: u32,
    resno: u32,
    compno: u32,
    precno: u32,
    include_len: usize,
) -> Option<usize> {
    let index = layno as u64 * pi.step_l as u64
        + resno as u64 * pi.step_r as u64
        + compno as u64 * pi.step_c as u64
        + precno as u64 * pi.step_p as u64;
    let index = index as usize;
    if index < include_len {
        Some(index)
    } else {
        None
    }
}

/// Dispatch to the appropriate progression order function.
pub fn pi_next(pi: &mut PiIterator, include: &mut [i16]) -> bool {
    match pi.poc.prg {
        ProgressionOrder::Lrcp => pi_next_lrcp(pi, include),
        ProgressionOrder::Rlcp => pi_next_rlcp(pi, include),
        ProgressionOrder::Rpcl => pi_next_rpcl(pi, include),
        ProgressionOrder::Pcrl => pi_next_pcrl(pi, include),
        ProgressionOrder::Cprl => pi_next_cprl(pi, include),
    }
}

/// LRCP: Layer → Resolution → Component → Precinct (C: opj_pi_next_lrcp).
fn pi_next_lrcp(pi: &mut PiIterator, include: &mut [i16]) -> bool {
    let mut resuming = !pi.first;
    if pi.first {
        pi.first = false;
    }

    let layno_start = if resuming { pi.layno } else { pi.poc.layno0 };
    for layno in layno_start..pi.poc.layno1 {
        pi.layno = layno;

        let resno_start = if resuming { pi.resno } else { pi.poc.resno0 };
        for resno in resno_start..pi.poc.resno1 {
            pi.resno = resno;

            let compno_start = if resuming { pi.compno } else { pi.poc.compno0 };
            for compno in compno_start..pi.poc.compno1 {
                pi.compno = compno;

                if compno as usize >= pi.comps.len() {
                    if resuming {
                        resuming = false;
                    }
                    continue;
                }
                let comp = &pi.comps[compno as usize];
                if resno >= comp.numresolutions {
                    if resuming {
                        resuming = false;
                    }
                    continue;
                }
                let res = &comp.resolutions[resno as usize];
                let precno1 = if !pi.tp_on {
                    res.pw * res.ph
                } else {
                    pi.poc.precno1
                };

                let precno_start = if resuming {
                    resuming = false;
                    pi.precno + 1
                } else {
                    pi.poc.precno0
                };

                for precno in precno_start..precno1 {
                    pi.precno = precno;
                    let Some(index) =
                        include_index(pi, layno, resno, compno, precno, include.len())
                    else {
                        return false;
                    };
                    if include[index] == 0 {
                        include[index] = 1;
                        return true;
                    }
                }
            }
        }
    }
    false
}

/// RLCP: Resolution → Layer → Component → Precinct (C: opj_pi_next_rlcp).
fn pi_next_rlcp(pi: &mut PiIterator, include: &mut [i16]) -> bool {
    let mut resuming = !pi.first;
    if pi.first {
        pi.first = false;
    }

    let resno_start = if resuming { pi.resno } else { pi.poc.resno0 };
    for resno in resno_start..pi.poc.resno1 {
        pi.resno = resno;

        let layno_start = if resuming { pi.layno } else { pi.poc.layno0 };
        for layno in layno_start..pi.poc.layno1 {
            pi.layno = layno;

            let compno_start = if resuming { pi.compno } else { pi.poc.compno0 };
            for compno in compno_start..pi.poc.compno1 {
                pi.compno = compno;

                if compno as usize >= pi.comps.len() {
                    if resuming {
                        resuming = false;
                    }
                    continue;
                }
                let comp = &pi.comps[compno as usize];
                if resno >= comp.numresolutions {
                    if resuming {
                        resuming = false;
                    }
                    continue;
                }
                let res = &comp.resolutions[resno as usize];
                let precno1 = if !pi.tp_on {
                    res.pw * res.ph
                } else {
                    pi.poc.precno1
                };

                let precno_start = if resuming {
                    resuming = false;
                    pi.precno + 1
                } else {
                    pi.poc.precno0
                };

                for precno in precno_start..precno1 {
                    pi.precno = precno;
                    let Some(index) =
                        include_index(pi, layno, resno, compno, precno, include.len())
                    else {
                        return false;
                    };
                    if include[index] == 0 {
                        include[index] = 1;
                        return true;
                    }
                }
            }
        }
    }
    false
}

/// Compute minimum precinct spacing (dx, dy) across all components and
/// resolutions in the given range. Returns `(0, 0)` if no valid resolutions.
fn compute_precinct_spacing(
    pi: &PiIterator,
    resno0: u32,
    resno1: u32,
    compno0: u32,
    compno1: u32,
) -> (u32, u32) {
    let mut dx: u32 = 0;
    let mut dy: u32 = 0;
    for resno in resno0..resno1 {
        for compno in compno0..compno1 {
            if compno as usize >= pi.comps.len() {
                continue;
            }
            let comp = &pi.comps[compno as usize];
            if resno >= comp.numresolutions {
                continue;
            }
            let res = &comp.resolutions[resno as usize];
            let levelno = comp.numresolutions - 1 - resno;
            // Precinct spacing in image coordinates
            let dx_cur = comp
                .dx
                .saturating_mul(1u32.checked_shl(res.pdx + levelno).unwrap_or(u32::MAX));
            let dy_cur = comp
                .dy
                .saturating_mul(1u32.checked_shl(res.pdy + levelno).unwrap_or(u32::MAX));
            if dx == 0 || dx > dx_cur {
                dx = dx_cur;
            }
            if dy == 0 || dy > dy_cur {
                dy = dy_cur;
            }
        }
    }
    (dx, dy)
}

/// Compute precinct index from spatial coordinates (C: inline in pi_next_rpcl).
///
/// Returns `None` if the position doesn't align with a valid precinct boundary.
fn compute_precinct_index(
    comp: &PiComp,
    res: &PiResolution,
    resno: u32,
    x: u32,
    y: u32,
    tx0: u32,
    ty0: u32,
) -> Option<u32> {
    let levelno = comp.numresolutions - 1 - resno;
    let rpx = res.pdx + levelno;
    let rpy = res.pdy + levelno;

    // Precinct step in image coordinates for alignment checks
    let x_step = (comp.dx as u64).checked_shl(rpx).unwrap_or(0);
    let y_step = (comp.dy as u64).checked_shl(rpy).unwrap_or(0);
    if x_step == 0 || y_step == 0 {
        return None;
    }

    // Check y alignment
    if !(y as u64).is_multiple_of(y_step) && y != ty0 {
        return None;
    }

    // Check x alignment
    if !(x as u64).is_multiple_of(x_step) && x != tx0 {
        return None;
    }

    // Compute tile-relative coordinates at this resolution level
    let div_x = (comp.dx as u64) << levelno;
    let div_y = (comp.dy as u64) << levelno;
    if div_x == 0 || div_y == 0 {
        return None;
    }

    let trx0 = ((tx0 as u64).div_ceil(div_x)) as i64;
    let try0 = ((ty0 as u64).div_ceil(div_y)) as i64;

    let x_scaled = ((x as u64).div_ceil(div_x)) as i64;
    let y_scaled = ((y as u64).div_ceil(div_y)) as i64;

    let prci = (x_scaled >> res.pdx) - (trx0 >> res.pdx);
    let prcj = (y_scaled >> res.pdy) - (try0 >> res.pdy);

    if prci < 0 || prcj < 0 {
        return None;
    }

    let precno = prci as u64 + prcj as u64 * res.pw as u64;
    Some(precno as u32)
}

/// RPCL: Resolution → Position → Component → Layer (C: opj_pi_next_rpcl).
fn pi_next_rpcl(pi: &mut PiIterator, include: &mut [i16]) -> bool {
    let mut resuming = !pi.first;
    if pi.first {
        let (dx, dy) = compute_precinct_spacing(
            pi,
            pi.poc.resno0,
            pi.poc.resno1,
            pi.poc.compno0,
            pi.poc.compno1,
        );
        if dx == 0 || dy == 0 {
            return false;
        }
        pi.dx = dx;
        pi.dy = dy;
        pi.first = false;
    }

    let resno_start = if resuming { pi.resno } else { pi.poc.resno0 };
    for resno in resno_start..pi.poc.resno1 {
        pi.resno = resno;

        let y_start = if resuming { pi.y } else { pi.poc.ty0 as u32 };
        let mut y = y_start;
        while y < pi.poc.ty1 as u32 {
            pi.y = y;

            let x_start = if resuming { pi.x } else { pi.poc.tx0 as u32 };
            let mut x = x_start;
            while x < pi.poc.tx1 as u32 {
                pi.x = x;

                let compno_start = if resuming { pi.compno } else { pi.poc.compno0 };
                for compno in compno_start..pi.poc.compno1 {
                    pi.compno = compno;

                    if compno as usize >= pi.comps.len() {
                        if resuming {
                            resuming = false;
                        }
                        continue;
                    }
                    let comp = &pi.comps[compno as usize];
                    if resno >= comp.numresolutions {
                        if resuming {
                            resuming = false;
                        }
                        continue;
                    }
                    let res = &comp.resolutions[resno as usize];

                    let precno =
                        match compute_precinct_index(comp, res, resno, x, y, pi.tx0, pi.ty0) {
                            Some(p) if p < res.pw * res.ph => p,
                            _ => {
                                if resuming {
                                    resuming = false;
                                }
                                continue;
                            }
                        };
                    pi.precno = precno;

                    let layno_start = if resuming {
                        resuming = false;
                        pi.layno + 1
                    } else {
                        pi.poc.layno0
                    };

                    for layno in layno_start..pi.poc.layno1 {
                        pi.layno = layno;
                        let Some(index) =
                            include_index(pi, layno, resno, compno, precno, include.len())
                        else {
                            return false;
                        };
                        if include[index] == 0 {
                            include[index] = 1;
                            return true;
                        }
                    }
                }

                x += pi.dx - (x % pi.dx);
            }

            y += pi.dy - (y % pi.dy);
        }
    }
    false
}

/// PCRL: Position → Component → Resolution → Layer (C: opj_pi_next_pcrl).
fn pi_next_pcrl(pi: &mut PiIterator, include: &mut [i16]) -> bool {
    let mut resuming = !pi.first;
    if pi.first {
        let (dx, dy) = compute_precinct_spacing(
            pi,
            pi.poc.resno0,
            pi.poc.resno1,
            pi.poc.compno0,
            pi.poc.compno1,
        );
        if dx == 0 || dy == 0 {
            return false;
        }
        pi.dx = dx;
        pi.dy = dy;
        pi.first = false;
    }

    let y_start = if resuming { pi.y } else { pi.poc.ty0 as u32 };
    let mut y = y_start;
    while y < pi.poc.ty1 as u32 {
        pi.y = y;

        let x_start = if resuming { pi.x } else { pi.poc.tx0 as u32 };
        let mut x = x_start;
        while x < pi.poc.tx1 as u32 {
            pi.x = x;

            let compno_start = if resuming { pi.compno } else { pi.poc.compno0 };
            for compno in compno_start..pi.poc.compno1 {
                pi.compno = compno;

                if compno as usize >= pi.comps.len() {
                    if resuming {
                        resuming = false;
                    }
                    continue;
                }
                let comp = &pi.comps[compno as usize];

                let resno_start = if resuming { pi.resno } else { pi.poc.resno0 };
                for resno in resno_start..pi.poc.resno1 {
                    pi.resno = resno;

                    if resno >= comp.numresolutions {
                        if resuming {
                            resuming = false;
                        }
                        continue;
                    }
                    let res = &comp.resolutions[resno as usize];

                    let precno =
                        match compute_precinct_index(comp, res, resno, x, y, pi.tx0, pi.ty0) {
                            Some(p) if p < res.pw * res.ph => p,
                            _ => {
                                if resuming {
                                    resuming = false;
                                }
                                continue;
                            }
                        };
                    pi.precno = precno;

                    let layno_start = if resuming {
                        resuming = false;
                        pi.layno + 1
                    } else {
                        pi.poc.layno0
                    };

                    for layno in layno_start..pi.poc.layno1 {
                        pi.layno = layno;
                        let Some(index) =
                            include_index(pi, layno, resno, compno, precno, include.len())
                        else {
                            return false;
                        };
                        if include[index] == 0 {
                            include[index] = 1;
                            return true;
                        }
                    }
                }
            }

            x += pi.dx - (x % pi.dx);
        }

        y += pi.dy - (y % pi.dy);
    }
    false
}

/// CPRL: Component → Position → Resolution → Layer (C: opj_pi_next_cprl).
fn pi_next_cprl(pi: &mut PiIterator, include: &mut [i16]) -> bool {
    let mut resuming = !pi.first;
    if pi.first {
        pi.first = false;
    }

    let compno_start = if resuming { pi.compno } else { pi.poc.compno0 };
    for compno in compno_start..pi.poc.compno1 {
        pi.compno = compno;

        if compno as usize >= pi.comps.len() {
            if resuming {
                resuming = false;
            }
            continue;
        }

        // Compute dx/dy for this component
        let (dx, dy) =
            compute_precinct_spacing(pi, pi.poc.resno0, pi.poc.resno1, compno, compno + 1);
        if dx == 0 || dy == 0 {
            if resuming {
                resuming = false;
            }
            continue;
        }
        pi.dx = dx;
        pi.dy = dy;

        let y_start = if resuming { pi.y } else { pi.poc.ty0 as u32 };
        let mut y = y_start;
        while y < pi.poc.ty1 as u32 {
            pi.y = y;

            let x_start = if resuming { pi.x } else { pi.poc.tx0 as u32 };
            let mut x = x_start;
            while x < pi.poc.tx1 as u32 {
                pi.x = x;

                let resno_start = if resuming { pi.resno } else { pi.poc.resno0 };
                for resno in resno_start..pi.poc.resno1 {
                    pi.resno = resno;

                    let comp = &pi.comps[compno as usize];
                    if resno >= comp.numresolutions {
                        if resuming {
                            resuming = false;
                        }
                        continue;
                    }
                    let res = &comp.resolutions[resno as usize];

                    let precno =
                        match compute_precinct_index(comp, res, resno, x, y, pi.tx0, pi.ty0) {
                            Some(p) if p < res.pw * res.ph => p,
                            _ => {
                                if resuming {
                                    resuming = false;
                                }
                                continue;
                            }
                        };
                    pi.precno = precno;

                    let layno_start = if resuming {
                        resuming = false;
                        pi.layno + 1
                    } else {
                        pi.poc.layno0
                    };

                    for layno in layno_start..pi.poc.layno1 {
                        pi.layno = layno;
                        let Some(index) =
                            include_index(pi, layno, resno, compno, precno, include.len())
                        else {
                            return false;
                        };
                        if include[index] == 0 {
                            include[index] = 1;
                            return true;
                        }
                    }
                }

                x += pi.dx - (x % pi.dx);
            }

            y += pi.dy - (y % pi.dy);
        }
    }
    false
}

// ---------------------------------------------------------------------------
// Packet iterator creation
// ---------------------------------------------------------------------------

/// Create packet iterators for decoding (C: opj_pi_create_decode).
///
/// Builds PacketIterators from image and coding parameters for a given tile.
pub fn pi_create_decode(
    _image: &crate::image::Image,
    _cp: &crate::j2k::params::CodingParameters,
    _tileno: u32,
) -> crate::error::Result<PacketIterators> {
    todo!("Phase 1100b: pi_create_decode")
}

// ---------------------------------------------------------------------------
// Public helper functions
// ---------------------------------------------------------------------------

/// Returns the total number of packets for a tile.
///
/// packet_count = numlayers * max_prec * numcomps * max_res
pub fn get_encoding_packet_count(
    numlayers: u32,
    numcomps: u32,
    max_res: u32,
    max_prec: u32,
) -> u32 {
    numlayers
        .saturating_mul(max_prec)
        .saturating_mul(numcomps)
        .saturating_mul(max_res)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Create a simple PI for testing: 1 component, 2 layers, 2 resolution levels.
    /// Res 0: 2x2 = 4 precincts (pdx=1,pdy=1,pw=2,ph=2)
    /// Res 1: 1x1 = 1 precinct  (pdx=1,pdy=1,pw=1,ph=1)
    fn create_test_pi(prg: ProgressionOrder) -> (PiIterator, Vec<i16>) {
        let resolutions = vec![
            PiResolution {
                pdx: 1,
                pdy: 1,
                pw: 2,
                ph: 2,
            },
            PiResolution {
                pdx: 1,
                pdy: 1,
                pw: 1,
                ph: 1,
            },
        ];
        let comp = PiComp {
            dx: 1,
            dy: 1,
            numresolutions: 2,
            resolutions,
        };

        let numlayers = 2u32;
        let numcomps = 1u32;
        let max_res = 2u32;
        let max_prec = 4u32;

        let step_p = 1u32;
        let step_c = max_prec * step_p;
        let step_r = numcomps * step_c;
        let step_l = max_res * step_r;

        let include_size = numlayers * step_l;
        let include = vec![0i16; include_size as usize];

        let poc = Poc {
            layno1: numlayers,
            resno1: max_res,
            compno1: numcomps,
            precno1: max_prec,
            prg,
            tx1: 4,
            ty1: 4,
            ..Default::default()
        };

        let pi = PiIterator {
            tp_on: false,
            step_l,
            step_r,
            step_c,
            step_p,
            compno: 0,
            resno: 0,
            precno: 0,
            layno: 0,
            first: true,
            poc,
            numcomps,
            comps: vec![comp],
            tx0: 0,
            ty0: 0,
            tx1: 4,
            ty1: 4,
            x: 0,
            y: 0,
            dx: 0,
            dy: 0,
        };

        (pi, include)
    }

    fn collect_packets(pi: &mut PiIterator, include: &mut [i16]) -> Vec<(u32, u32, u32, u32)> {
        let mut packets = Vec::new();
        while pi_next(pi, include) {
            packets.push((pi.layno, pi.resno, pi.compno, pi.precno));
        }
        packets
    }

    /// Create a single-resolution PI suitable for spatial progression orders.
    /// 1 component, 2 layers, 1 resolution, 2x2 = 4 precincts.
    fn create_spatial_test_pi(prg: ProgressionOrder) -> (PiIterator, Vec<i16>) {
        let resolutions = vec![PiResolution {
            pdx: 1,
            pdy: 1,
            pw: 2,
            ph: 2,
        }];
        let comp = PiComp {
            dx: 1,
            dy: 1,
            numresolutions: 1,
            resolutions,
        };

        let numlayers = 2u32;
        let numcomps = 1u32;
        let max_res = 1u32;
        let max_prec = 4u32;

        let step_p = 1u32;
        let step_c = max_prec * step_p;
        let step_r = numcomps * step_c;
        let step_l = max_res * step_r;

        let include_size = numlayers * step_l;
        let include = vec![0i16; include_size as usize];

        let poc = Poc {
            layno1: numlayers,
            resno1: max_res,
            compno1: numcomps,
            precno1: max_prec,
            prg,
            tx1: 4,
            ty1: 4,
            ..Default::default()
        };

        let pi = PiIterator {
            tp_on: false,
            step_l,
            step_r,
            step_c,
            step_p,
            compno: 0,
            resno: 0,
            precno: 0,
            layno: 0,
            first: true,
            poc,
            numcomps,
            comps: vec![comp],
            tx0: 0,
            ty0: 0,
            tx1: 4,
            ty1: 4,
            x: 0,
            y: 0,
            dx: 0,
            dy: 0,
        };

        (pi, include)
    }

    // --- Structure tests ---

    #[test]
    fn pi_resolution_default() {
        let res = PiResolution::default();
        assert_eq!(res.pdx, 0);
        assert_eq!(res.pw, 0);
    }

    #[test]
    fn pi_comp_default() {
        let comp = PiComp::default();
        assert_eq!(comp.dx, 0);
        assert_eq!(comp.numresolutions, 0);
        assert!(comp.resolutions.is_empty());
    }

    #[test]
    fn packet_iterators_basic() {
        let (pi, include) = create_test_pi(ProgressionOrder::Lrcp);
        let mut pis = PacketIterators {
            iterators: vec![pi],
            include,
        };
        assert_eq!(pis.len(), 1);
        assert!(!pis.is_empty());
        assert!(pis.next(0)); // Should find at least one packet
    }

    // --- LRCP tests ---

    #[test]
    fn lrcp_packet_count() {
        let (mut pi, mut include) = create_test_pi(ProgressionOrder::Lrcp);
        let packets = collect_packets(&mut pi, &mut include);
        // 2 layers * (4 precincts @ res0 + 1 precinct @ res1) = 10
        assert_eq!(packets.len(), 10);
    }

    #[test]
    fn lrcp_ordering() {
        let (mut pi, mut include) = create_test_pi(ProgressionOrder::Lrcp);
        let packets = collect_packets(&mut pi, &mut include);
        // LRCP: Layer outermost. Within each layer: R then C then P.
        // Layer 0:
        assert_eq!(packets[0], (0, 0, 0, 0));
        assert_eq!(packets[1], (0, 0, 0, 1));
        assert_eq!(packets[2], (0, 0, 0, 2));
        assert_eq!(packets[3], (0, 0, 0, 3));
        assert_eq!(packets[4], (0, 1, 0, 0)); // Res 1, 1 precinct
        // Layer 1:
        assert_eq!(packets[5], (1, 0, 0, 0));
        assert_eq!(packets[9], (1, 1, 0, 0));
    }

    #[test]
    fn lrcp_layers_are_outermost() {
        let (mut pi, mut include) = create_test_pi(ProgressionOrder::Lrcp);
        let packets = collect_packets(&mut pi, &mut include);
        // All layer 0 packets come before all layer 1 packets
        let first_l1 = packets.iter().position(|p| p.0 == 1).unwrap();
        assert!(packets[..first_l1].iter().all(|p| p.0 == 0));
    }

    // --- RLCP tests ---

    #[test]
    fn rlcp_packet_count() {
        let (mut pi, mut include) = create_test_pi(ProgressionOrder::Rlcp);
        let packets = collect_packets(&mut pi, &mut include);
        assert_eq!(packets.len(), 10);
    }

    #[test]
    fn rlcp_ordering() {
        let (mut pi, mut include) = create_test_pi(ProgressionOrder::Rlcp);
        let packets = collect_packets(&mut pi, &mut include);
        // RLCP = Res → Lay → Comp → Prec
        // Res 0, L0: C0, P=[0..3] → 4 packets
        // Res 0, L1: C0, P=[0..3] → 4 packets
        // Res 1, L0: C0, P=[0]   → 1 packet
        // Res 1, L1: C0, P=[0]   → 1 packet
        assert_eq!(packets[0], (0, 0, 0, 0));
        assert_eq!(packets[3], (0, 0, 0, 3));
        assert_eq!(packets[4], (1, 0, 0, 0)); // L1, Res 0
        assert_eq!(packets[7], (1, 0, 0, 3));
        assert_eq!(packets[8], (0, 1, 0, 0)); // Res 1, L0
        assert_eq!(packets[9], (1, 1, 0, 0)); // Res 1, L1
    }

    #[test]
    fn rlcp_resolutions_are_outermost() {
        let (mut pi, mut include) = create_test_pi(ProgressionOrder::Rlcp);
        let packets = collect_packets(&mut pi, &mut include);
        // All res 0 packets come before all res 1 packets
        let first_r1 = packets.iter().position(|p| p.1 == 1).unwrap();
        assert!(packets[..first_r1].iter().all(|p| p.1 == 0));
    }

    // --- Include array ---

    #[test]
    fn include_prevents_duplicates() {
        let (mut pi, mut include) = create_test_pi(ProgressionOrder::Lrcp);
        let packets1 = collect_packets(&mut pi, &mut include);
        // Reset iterator but keep include array
        pi.first = true;
        let packets2 = collect_packets(&mut pi, &mut include);
        // Second pass should find no packets (all already visited)
        assert!(!packets1.is_empty());
        assert!(packets2.is_empty());
    }

    // --- RPCL tests ---

    #[test]
    fn rpcl_packet_count() {
        let (mut pi, mut include) = create_spatial_test_pi(ProgressionOrder::Rpcl);
        let packets = collect_packets(&mut pi, &mut include);
        // 1 res × 4 precincts × 2 layers = 8
        assert_eq!(packets.len(), 8);
    }

    // --- PCRL tests ---

    #[test]
    fn pcrl_packet_count() {
        let (mut pi, mut include) = create_spatial_test_pi(ProgressionOrder::Pcrl);
        let packets = collect_packets(&mut pi, &mut include);
        assert_eq!(packets.len(), 8);
    }

    // --- CPRL tests ---

    #[test]
    fn cprl_packet_count() {
        let (mut pi, mut include) = create_spatial_test_pi(ProgressionOrder::Cprl);
        let packets = collect_packets(&mut pi, &mut include);
        assert_eq!(packets.len(), 8);
    }

    // --- pi_next dispatch ---

    #[test]
    fn pi_next_dispatches_correctly() {
        // LRCP and RLCP with multi-res config
        for prg in [ProgressionOrder::Lrcp, ProgressionOrder::Rlcp] {
            let (mut pi, mut include) = create_test_pi(prg);
            let packets = collect_packets(&mut pi, &mut include);
            assert_eq!(packets.len(), 10, "Expected 10 packets for {:?}", prg);
        }
        // Spatial orders with single-res config
        for prg in [
            ProgressionOrder::Rpcl,
            ProgressionOrder::Pcrl,
            ProgressionOrder::Cprl,
        ] {
            let (mut pi, mut include) = create_spatial_test_pi(prg);
            let packets = collect_packets(&mut pi, &mut include);
            assert_eq!(packets.len(), 8, "Expected 8 packets for {:?}", prg);
        }
    }

    // --- Packet count helper ---

    #[test]
    fn encoding_packet_count() {
        assert_eq!(get_encoding_packet_count(2, 1, 2, 4), 16);
        assert_eq!(get_encoding_packet_count(1, 3, 5, 2), 30);
        assert_eq!(get_encoding_packet_count(0, 1, 1, 1), 0);
    }

    // --- Empty tile ---

    // --- pi_create_decode ---

    #[test]
    #[ignore = "not yet implemented"]
    fn pi_create_decode_basic() {
        use crate::image::{Image, ImageCompParam};
        use crate::j2k::params::{
            CodingParamMode, CodingParameters, DecodingParam, TileCodingParameters,
            TileCompCodingParameters,
        };
        use crate::types::ColorSpace;

        // 1 component, 64x64, 2 resolutions, 1 layer, LRCP
        let params = vec![ImageCompParam {
            dx: 1,
            dy: 1,
            w: 64,
            h: 64,
            x0: 0,
            y0: 0,
            prec: 8,
            sgnd: false,
        }];
        let mut image = Image::new(&params, ColorSpace::Gray);
        image.x1 = 64;
        image.y1 = 64;

        let tccp = TileCompCodingParameters {
            numresolutions: 2,
            cblkw: 6,
            cblkh: 6,
            qmfbid: 1,
            ..Default::default()
        };
        let tcp = TileCodingParameters {
            numlayers: 1,
            prg: ProgressionOrder::Lrcp,
            tccps: vec![tccp],
            ..Default::default()
        };
        let cp = CodingParameters {
            tx0: 0,
            ty0: 0,
            tdx: 64,
            tdy: 64,
            tw: 1,
            th: 1,
            tcps: vec![tcp],
            mode: CodingParamMode::Decoder(DecodingParam::default()),
            ..CodingParameters::new_encoder()
        };

        let pis = pi_create_decode(&image, &cp, 0).unwrap();
        assert!(!pis.is_empty());
        // Should have at least 1 iterator (for the default POC)
        assert_eq!(pis.len(), 1);
        // The iterator should produce packets (1 layer * (precincts@res0 + precincts@res1))
        let mut pis = pis;
        let mut count = 0;
        while pis.next(0) {
            count += 1;
        }
        assert!(count > 0);
    }

    #[test]
    fn empty_tile_returns_false() {
        let comp = PiComp {
            dx: 1,
            dy: 1,
            numresolutions: 1,
            resolutions: vec![PiResolution {
                pdx: 0,
                pdy: 0,
                pw: 0,
                ph: 0,
            }],
        };
        let poc = Poc {
            layno1: 1,
            resno1: 1,
            compno1: 1,
            prg: ProgressionOrder::Lrcp,
            ..Default::default()
        };

        let mut pi = PiIterator {
            tp_on: false,
            step_l: 1,
            step_r: 1,
            step_c: 1,
            step_p: 1,
            compno: 0,
            resno: 0,
            precno: 0,
            layno: 0,
            first: true,
            poc,
            numcomps: 1,
            comps: vec![comp],
            tx0: 0,
            ty0: 0,
            tx1: 0,
            ty1: 0,
            x: 0,
            y: 0,
            dx: 0,
            dy: 0,
        };
        let mut include = vec![0i16; 1];
        assert!(!pi_next(&mut pi, &mut include));
    }
}
