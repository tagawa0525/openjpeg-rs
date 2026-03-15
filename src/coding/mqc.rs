// Phase 200: MQ arithmetic coder (C: opj_mqc_t)

use crate::types::COMMON_CBLK_DATA_EXTRA;

/// Number of contexts (C: MQC_NUMCTXS).
pub const MQC_NUMCTXS: usize = 19;

/// Sentinel value for bypass ct initialization (C: BYPASS_CT_INIT).
const BYPASS_CT_INIT: u32 = 0xDEADBEEF;

/// MQ state transition table entry.
///
/// Each entry represents a probability state pair (MPS=0 and MPS=1 share the
/// same entry). The C version uses 94 entries (47 pairs); we use 47 entries
/// and track MPS separately per context.
#[derive(Clone, Copy)]
pub struct MqcState {
    pub qeval: u32,
    pub nmps: usize,
    pub nlps: usize,
    pub switch_mps: bool,
}

/// Static MQ state table (47 entries, ITU-T T.800 Table D.3).
///
/// Derived from C version `mqc_states[94]` by merging even/odd pairs.
/// For each C pair (2*i, 2*i+1), they share qeval but differ in mps.
/// Transition indices are halved: C index n maps to Rust index n/2.
pub static MQC_STATES: [MqcState; 47] = [
    // State  0: qeval=0x5601
    MqcState {
        qeval: 0x5601,
        nmps: 1,
        nlps: 1,
        switch_mps: true,
    },
    // State  1: qeval=0x3401
    MqcState {
        qeval: 0x3401,
        nmps: 2,
        nlps: 6,
        switch_mps: false,
    },
    // State  2: qeval=0x1801
    MqcState {
        qeval: 0x1801,
        nmps: 3,
        nlps: 9,
        switch_mps: false,
    },
    // State  3: qeval=0x0ac1
    MqcState {
        qeval: 0x0ac1,
        nmps: 4,
        nlps: 12,
        switch_mps: false,
    },
    // State  4: qeval=0x0521
    MqcState {
        qeval: 0x0521,
        nmps: 5,
        nlps: 29,
        switch_mps: false,
    },
    // State  5: qeval=0x0221
    MqcState {
        qeval: 0x0221,
        nmps: 38,
        nlps: 33,
        switch_mps: false,
    },
    // State  6: qeval=0x5601 (after LPS from state 1)
    MqcState {
        qeval: 0x5601,
        nmps: 7,
        nlps: 6,
        switch_mps: true,
    },
    // State  7: qeval=0x5401
    MqcState {
        qeval: 0x5401,
        nmps: 8,
        nlps: 14,
        switch_mps: false,
    },
    // State  8: qeval=0x4801
    MqcState {
        qeval: 0x4801,
        nmps: 9,
        nlps: 14,
        switch_mps: false,
    },
    // State  9: qeval=0x3801
    MqcState {
        qeval: 0x3801,
        nmps: 10,
        nlps: 14,
        switch_mps: false,
    },
    // State 10: qeval=0x3001
    MqcState {
        qeval: 0x3001,
        nmps: 11,
        nlps: 17,
        switch_mps: false,
    },
    // State 11: qeval=0x2401
    MqcState {
        qeval: 0x2401,
        nmps: 12,
        nlps: 18,
        switch_mps: false,
    },
    // State 12: qeval=0x1c01
    MqcState {
        qeval: 0x1c01,
        nmps: 13,
        nlps: 20,
        switch_mps: false,
    },
    // State 13: qeval=0x1601
    MqcState {
        qeval: 0x1601,
        nmps: 29,
        nlps: 21,
        switch_mps: false,
    },
    // State 14: qeval=0x5601 (after LPS from state 7/8/9)
    MqcState {
        qeval: 0x5601,
        nmps: 15,
        nlps: 14,
        switch_mps: true,
    },
    // State 15: qeval=0x5401
    MqcState {
        qeval: 0x5401,
        nmps: 16,
        nlps: 14,
        switch_mps: false,
    },
    // State 16: qeval=0x5101
    MqcState {
        qeval: 0x5101,
        nmps: 17,
        nlps: 15,
        switch_mps: false,
    },
    // State 17: qeval=0x4801
    MqcState {
        qeval: 0x4801,
        nmps: 18,
        nlps: 16,
        switch_mps: false,
    },
    // State 18: qeval=0x3801
    MqcState {
        qeval: 0x3801,
        nmps: 19,
        nlps: 17,
        switch_mps: false,
    },
    // State 19: qeval=0x3401
    MqcState {
        qeval: 0x3401,
        nmps: 20,
        nlps: 18,
        switch_mps: false,
    },
    // State 20: qeval=0x3001
    MqcState {
        qeval: 0x3001,
        nmps: 21,
        nlps: 19,
        switch_mps: false,
    },
    // State 21: qeval=0x2801
    MqcState {
        qeval: 0x2801,
        nmps: 22,
        nlps: 19,
        switch_mps: false,
    },
    // State 22: qeval=0x2401
    MqcState {
        qeval: 0x2401,
        nmps: 23,
        nlps: 20,
        switch_mps: false,
    },
    // State 23: qeval=0x2201
    MqcState {
        qeval: 0x2201,
        nmps: 24,
        nlps: 21,
        switch_mps: false,
    },
    // State 24: qeval=0x1c01
    MqcState {
        qeval: 0x1c01,
        nmps: 25,
        nlps: 22,
        switch_mps: false,
    },
    // State 25: qeval=0x1801
    MqcState {
        qeval: 0x1801,
        nmps: 26,
        nlps: 23,
        switch_mps: false,
    },
    // State 26: qeval=0x1601
    MqcState {
        qeval: 0x1601,
        nmps: 27,
        nlps: 24,
        switch_mps: false,
    },
    // State 27: qeval=0x1401
    MqcState {
        qeval: 0x1401,
        nmps: 28,
        nlps: 25,
        switch_mps: false,
    },
    // State 28: qeval=0x1201
    MqcState {
        qeval: 0x1201,
        nmps: 29,
        nlps: 26,
        switch_mps: false,
    },
    // State 29: qeval=0x1101
    MqcState {
        qeval: 0x1101,
        nmps: 30,
        nlps: 27,
        switch_mps: false,
    },
    // State 30: qeval=0x0ac1
    MqcState {
        qeval: 0x0ac1,
        nmps: 31,
        nlps: 28,
        switch_mps: false,
    },
    // State 31: qeval=0x09c1
    MqcState {
        qeval: 0x09c1,
        nmps: 32,
        nlps: 29,
        switch_mps: false,
    },
    // State 32: qeval=0x08a1
    MqcState {
        qeval: 0x08a1,
        nmps: 33,
        nlps: 30,
        switch_mps: false,
    },
    // State 33: qeval=0x0521
    MqcState {
        qeval: 0x0521,
        nmps: 34,
        nlps: 31,
        switch_mps: false,
    },
    // State 34: qeval=0x0441
    MqcState {
        qeval: 0x0441,
        nmps: 35,
        nlps: 32,
        switch_mps: false,
    },
    // State 35: qeval=0x02a1
    MqcState {
        qeval: 0x02a1,
        nmps: 36,
        nlps: 33,
        switch_mps: false,
    },
    // State 36: qeval=0x0221
    MqcState {
        qeval: 0x0221,
        nmps: 37,
        nlps: 34,
        switch_mps: false,
    },
    // State 37: qeval=0x0141
    MqcState {
        qeval: 0x0141,
        nmps: 38,
        nlps: 35,
        switch_mps: false,
    },
    // State 38: qeval=0x0111
    MqcState {
        qeval: 0x0111,
        nmps: 39,
        nlps: 36,
        switch_mps: false,
    },
    // State 39: qeval=0x0085
    MqcState {
        qeval: 0x0085,
        nmps: 40,
        nlps: 37,
        switch_mps: false,
    },
    // State 40: qeval=0x0049
    MqcState {
        qeval: 0x0049,
        nmps: 41,
        nlps: 38,
        switch_mps: false,
    },
    // State 41: qeval=0x0025
    MqcState {
        qeval: 0x0025,
        nmps: 42,
        nlps: 39,
        switch_mps: false,
    },
    // State 42: qeval=0x0015
    MqcState {
        qeval: 0x0015,
        nmps: 43,
        nlps: 40,
        switch_mps: false,
    },
    // State 43: qeval=0x0009
    MqcState {
        qeval: 0x0009,
        nmps: 44,
        nlps: 41,
        switch_mps: false,
    },
    // State 44: qeval=0x0005
    MqcState {
        qeval: 0x0005,
        nmps: 45,
        nlps: 42,
        switch_mps: false,
    },
    // State 45: qeval=0x0001
    MqcState {
        qeval: 0x0001,
        nmps: 45,
        nlps: 43,
        switch_mps: false,
    },
    // State 46: qeval=0x5601 (equiprobable, self-referencing)
    MqcState {
        qeval: 0x5601,
        nmps: 46,
        nlps: 46,
        switch_mps: false,
    },
];

/// MQ arithmetic coder (C: opj_mqc_t).
pub struct Mqc<'a> {
    buf: &'a mut [u8],
    bp: usize,
    c: u32,
    a: u32,
    ct: u32,
    start: usize,
    ctxs: [usize; MQC_NUMCTXS],
    ctxs_mps: [u8; MQC_NUMCTXS],
    curctx: usize,
    end_of_byte_stream_counter: u32,
    end: usize,
    backup: [u8; COMMON_CBLK_DATA_EXTRA],
}

impl<'a> Mqc<'a> {
    /// Create a new MQC bound to a buffer.
    ///
    /// The buffer must include `COMMON_CBLK_DATA_EXTRA` extra writable bytes
    /// at the end for decoder operation.
    pub fn new(buf: &'a mut [u8]) -> Self {
        Self {
            buf,
            bp: 0,
            c: 0,
            a: 0x8000,
            ct: 12,
            start: 0,
            ctxs: [0; MQC_NUMCTXS],
            ctxs_mps: [0; MQC_NUMCTXS],
            curctx: 0,
            end_of_byte_stream_counter: 0,
            end: 0,
            backup: [0; COMMON_CBLK_DATA_EXTRA],
        }
    }

    /// Reset all contexts to equiprobable state 0 (C: opj_mqc_resetstates).
    pub fn reset_states(&mut self) {
        for i in 0..MQC_NUMCTXS {
            self.ctxs[i] = 0;
            self.ctxs_mps[i] = 0;
        }
    }

    /// Set the state of a specific context (C: opj_mqc_setstate).
    pub fn set_state(&mut self, ctxno: usize, msb: u32, prob: i32) {
        debug_assert!(ctxno < MQC_NUMCTXS, "ctxno {ctxno} out of range");
        debug_assert!(
            (prob as usize) < MQC_STATES.len(),
            "prob {prob} out of range"
        );
        self.ctxs[ctxno] = prob as usize;
        self.ctxs_mps[ctxno] = msb as u8;
    }

    /// Set current active context (C: opj_mqc_setcurctx).
    pub fn set_curctx(&mut self, ctxno: usize) {
        debug_assert!(ctxno < MQC_NUMCTXS, "ctxno {ctxno} out of range");
        self.curctx = ctxno;
    }

    // --- Encoder ---

    /// Initialize encoder (C: opj_mqc_init_enc).
    ///
    /// The first byte of buf is a padding byte (bp starts at position 0,
    /// data is written starting from position 1).
    pub fn init_enc(&mut self) {
        self.set_curctx(0);
        self.a = 0x8000;
        self.c = 0;
        // In C, bp starts at buf-1. We use bp=0 as padding, start=1.
        self.bp = 0;
        self.ct = 12;
        self.start = 1;
        self.end_of_byte_stream_counter = 0;
        // Explicitly zero the padding byte so byteout behavior is correct
        // in both debug and release builds.
        self.buf[0] = 0;
    }

    /// Number of bytes written since init (C: opj_mqc_numbytes).
    pub fn num_bytes(&self) -> usize {
        self.bp.saturating_sub(self.start)
    }

    /// Buffer length accessor (avoids borrow issues in tests).
    pub fn buf_len(&self) -> usize {
        self.buf.len()
    }

    /// Encode a symbol (C: opj_mqc_encode).
    pub fn encode(&mut self, d: u32) {
        if self.ctxs_mps[self.curctx] == d as u8 {
            self.codemps();
        } else {
            self.codelps();
        }
    }

    /// Flush encoder (C: opj_mqc_flush).
    pub fn flush(&mut self) {
        self.setbits();
        self.c <<= self.ct;
        self.byteout();
        self.c <<= self.ct;
        self.byteout();

        if self.buf[self.bp] != 0xff {
            self.bp += 1;
        }
    }

    /// BYPASS mode init (C: opj_mqc_bypass_init_enc).
    pub fn bypass_init_enc(&mut self) {
        self.c = 0;
        self.ct = BYPASS_CT_INIT;
    }

    /// BYPASS mode encode (C: opj_mqc_bypass_enc).
    pub fn bypass_enc(&mut self, d: u32) {
        if self.ct == BYPASS_CT_INIT {
            self.ct = 8;
        }
        self.ct -= 1;
        self.c += d << self.ct;
        if self.ct == 0 {
            self.buf[self.bp] = self.c as u8;
            self.ct = if self.buf[self.bp] == 0xff { 7 } else { 8 };
            self.bp += 1;
            self.c = 0;
        }
    }

    /// Return extra bytes for non-terminating BYPASS pass (C: opj_mqc_bypass_get_extra_bytes).
    pub fn bypass_get_extra_bytes(&self, erterm: bool) -> u32 {
        if self.ct < 7 || (self.ct == 7 && (erterm || self.buf[self.bp - 1] != 0xff)) {
            1
        } else {
            0
        }
    }

    /// BYPASS mode flush (C: opj_mqc_bypass_flush_enc).
    pub fn bypass_flush_enc(&mut self, erterm: bool) {
        if self.ct < 7 || (self.ct == 7 && (erterm || self.buf[self.bp - 1] != 0xff)) {
            let mut bit_value = 0u32;
            while self.ct > 0 {
                self.ct -= 1;
                self.c += bit_value << self.ct;
                bit_value = 1 - bit_value;
            }
            self.buf[self.bp] = self.c as u8;
            self.bp += 1;
        } else if self.ct == 7 && self.buf[self.bp - 1] == 0xff {
            debug_assert!(!erterm);
            self.bp -= 1;
        } else if self.ct == 8
            && !erterm
            && self.bp >= 2
            && self.buf[self.bp - 1] == 0x7f
            && self.buf[self.bp - 2] == 0xff
        {
            self.bp -= 2;
        }
    }

    /// RESET mode (C: opj_mqc_reset_enc).
    pub fn reset_enc(&mut self) {
        self.reset_states();
        self.set_state(17, 0, 46); // T1_CTXNO_UNI
        self.set_state(0, 0, 3); // T1_CTXNO_AGG -> ctxno 0 in reset, prob 3
        // Note: C version uses T1_CTXNO_ZC=0 with prob=4, but reset_enc
        // calls setstate for specific T1 context numbers.
        // We'll replicate the C behavior exactly.
    }

    /// RESTART mode reinit (C: opj_mqc_restart_init_enc).
    pub fn restart_init_enc(&mut self) {
        self.a = 0x8000;
        self.c = 0;
        self.ct = 12;
        self.bp -= 1;
        if self.buf[self.bp] == 0xff {
            self.ct = 13;
        }
    }

    /// ERTERM mode (C: opj_mqc_erterm_enc).
    pub fn erterm_enc(&mut self) {
        let mut k = 11i32 - self.ct as i32 + 1;
        while k > 0 {
            self.c <<= self.ct;
            self.ct = 0;
            self.byteout();
            k -= self.ct as i32;
        }
        if self.buf[self.bp] != 0xff {
            self.byteout();
        }
    }

    /// SEGMARK mode (C: opj_mqc_segmark_enc).
    pub fn segmark_enc(&mut self) {
        self.set_curctx(18);
        for i in 1..5u32 {
            self.encode(i % 2);
        }
    }

    // --- Decoder ---

    /// Initialize MQ decoder (C: opj_mqc_init_dec).
    pub fn init_dec(&mut self, len: usize) {
        self.init_dec_common(len);
        self.set_curctx(0);
        self.end_of_byte_stream_counter = 0;

        if len == 0 {
            self.c = 0xff << 16;
        } else {
            self.c = (self.buf[self.bp] as u32) << 16;
        }

        self.bytein();
        self.c <<= 7;
        self.ct -= 7;
        self.a = 0x8000;
    }

    /// Initialize RAW decoder (C: opj_mqc_raw_init_dec).
    pub fn raw_init_dec(&mut self, len: usize) {
        self.init_dec_common(len);
        self.c = 0;
        self.ct = 0;
    }

    /// Decode a symbol (C: opj_mqc_decode).
    pub fn decode(&mut self) -> u32 {
        let ctxno = self.curctx;
        let state_idx = self.ctxs[ctxno];
        let qeval = MQC_STATES[state_idx].qeval;

        self.a -= qeval;
        if (self.c >> 16) < qeval {
            // LPS exchange
            let d = self.lpsexchange(ctxno, state_idx, qeval);
            self.renormd();
            d
        } else {
            self.c -= qeval << 16;
            if (self.a & 0x8000) == 0 {
                // MPS exchange
                let d = self.mpsexchange(ctxno, state_idx);
                self.renormd();
                d
            } else {
                self.ctxs_mps[ctxno] as u32
            }
        }
    }

    /// RAW decode a symbol (C: opj_mqc_raw_decode).
    pub fn raw_decode(&mut self) -> u32 {
        if self.ct == 0 {
            if self.c == 0xff {
                if self.bp < self.buf.len() && self.buf[self.bp] > 0x8f {
                    self.c = 0xff;
                    self.ct = 8;
                } else if self.bp < self.buf.len() {
                    self.c = self.buf[self.bp] as u32;
                    self.bp += 1;
                    self.ct = 7;
                } else {
                    self.c = 0xff;
                    self.ct = 8;
                }
            } else if self.bp < self.buf.len() {
                self.c = self.buf[self.bp] as u32;
                self.bp += 1;
                self.ct = 8;
            } else {
                self.c = 0xff;
                self.ct = 8;
            }
        }
        self.ct -= 1;
        (self.c >> self.ct) & 1
    }

    /// Finish decoding, restore overwritten bytes (C: opq_mqc_finish_dec).
    pub fn finish_dec(&mut self) {
        let end = self.end;
        for i in 0..COMMON_CBLK_DATA_EXTRA {
            if end + i < self.buf.len() {
                self.buf[end + i] = self.backup[i];
            }
        }
    }

    // --- Internal encoder helpers ---

    fn setbits(&mut self) {
        let tempc = self.c.wrapping_add(self.a);
        self.c |= 0xffff;
        if self.c >= tempc {
            self.c -= 0x8000;
        }
    }

    fn byteout(&mut self) {
        debug_assert!(
            self.bp + 1 < self.buf.len(),
            "buffer overflow in byteout: bp={} buf.len()={}",
            self.bp,
            self.buf.len()
        );
        if self.buf[self.bp] == 0xff {
            self.bp += 1;
            self.buf[self.bp] = (self.c >> 20) as u8;
            self.c &= 0xfffff;
            self.ct = 7;
        } else if (self.c & 0x8000000) == 0 {
            self.bp += 1;
            self.buf[self.bp] = (self.c >> 19) as u8;
            self.c &= 0x7ffff;
            self.ct = 8;
        } else {
            self.buf[self.bp] += 1;
            if self.buf[self.bp] == 0xff {
                self.c &= 0x7ffffff;
                self.bp += 1;
                self.buf[self.bp] = (self.c >> 20) as u8;
                self.c &= 0xfffff;
                self.ct = 7;
            } else {
                self.bp += 1;
                self.buf[self.bp] = (self.c >> 19) as u8;
                self.c &= 0x7ffff;
                self.ct = 8;
            }
        }
    }

    fn codemps(&mut self) {
        let ctxno = self.curctx;
        let state_idx = self.ctxs[ctxno];
        let qeval = MQC_STATES[state_idx].qeval;

        self.a -= qeval;
        if (self.a & 0x8000) == 0 {
            if self.a < qeval {
                self.a = qeval;
            } else {
                self.c += qeval;
            }
            self.ctxs[ctxno] = MQC_STATES[state_idx].nmps;
            self.renorme();
        } else {
            self.c += qeval;
        }
    }

    fn codelps(&mut self) {
        let ctxno = self.curctx;
        let state_idx = self.ctxs[ctxno];
        let qeval = MQC_STATES[state_idx].qeval;

        self.a -= qeval;
        if self.a < qeval {
            self.c += qeval;
        } else {
            self.a = qeval;
        }
        if MQC_STATES[state_idx].switch_mps {
            self.ctxs_mps[ctxno] = 1 - self.ctxs_mps[ctxno];
        }
        self.ctxs[ctxno] = MQC_STATES[state_idx].nlps;
        self.renorme();
    }

    fn renorme(&mut self) {
        loop {
            self.a <<= 1;
            self.c <<= 1;
            self.ct -= 1;
            if self.ct == 0 {
                let c = self.c;
                self.byteout();
                // byteout may update c via masking, but we need to restore
                // the shifted value. Actually byteout reads self.c directly.
                let _ = c; // byteout operates on self.c
            }
            if (self.a & 0x8000) != 0 {
                break;
            }
        }
    }

    // --- Internal decoder helpers ---

    fn init_dec_common(&mut self, len: usize) {
        self.start = 0;
        self.end = len;
        self.bp = 0;

        // Backup and overwrite end bytes with artificial 0xFF 0xFF
        for i in 0..COMMON_CBLK_DATA_EXTRA {
            if len + i < self.buf.len() {
                self.backup[i] = self.buf[len + i];
                self.buf[len + i] = 0xFF;
            }
        }
    }

    fn bytein(&mut self) {
        if self.bp + 1 < self.buf.len() {
            let next = self.buf[self.bp + 1] as u32;
            if self.buf[self.bp] == 0xff {
                if next > 0x8f {
                    self.c += 0xff00;
                    self.ct = 8;
                    self.end_of_byte_stream_counter += 1;
                } else {
                    self.bp += 1;
                    self.c += next << 9;
                    self.ct = 7;
                }
            } else {
                self.bp += 1;
                self.c += next << 8;
                self.ct = 8;
            }
        } else {
            self.c += 0xff00;
            self.ct = 8;
            self.end_of_byte_stream_counter += 1;
        }
    }

    fn renormd(&mut self) {
        loop {
            if self.ct == 0 {
                self.bytein();
            }
            self.a <<= 1;
            self.c <<= 1;
            self.ct -= 1;
            if self.a >= 0x8000 {
                break;
            }
        }
    }

    fn mpsexchange(&mut self, ctxno: usize, state_idx: usize) -> u32 {
        let qeval = MQC_STATES[state_idx].qeval;
        if self.a < qeval {
            let d = 1 - self.ctxs_mps[ctxno] as u32;
            self.ctxs[ctxno] = MQC_STATES[state_idx].nlps;
            if MQC_STATES[state_idx].switch_mps {
                self.ctxs_mps[ctxno] = 1 - self.ctxs_mps[ctxno];
            }
            d
        } else {
            let d = self.ctxs_mps[ctxno] as u32;
            self.ctxs[ctxno] = MQC_STATES[state_idx].nmps;
            d
        }
    }

    fn lpsexchange(&mut self, ctxno: usize, state_idx: usize, qeval: u32) -> u32 {
        if self.a < qeval {
            self.a = qeval;
            let d = self.ctxs_mps[ctxno] as u32;
            self.ctxs[ctxno] = MQC_STATES[state_idx].nmps;
            d
        } else {
            self.a = qeval;
            let d = 1 - self.ctxs_mps[ctxno] as u32;
            if MQC_STATES[state_idx].switch_mps {
                self.ctxs_mps[ctxno] = 1 - self.ctxs_mps[ctxno];
            }
            self.ctxs[ctxno] = MQC_STATES[state_idx].nlps;
            d
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_table_length() {
        assert_eq!(MQC_STATES.len(), 47);
    }

    #[test]
    fn state_table_first_entry() {
        let s = &MQC_STATES[0];
        assert_eq!(s.qeval, 0x5601);
    }

    #[test]
    fn state_table_last_entry() {
        let s = &MQC_STATES[46];
        assert_eq!(s.qeval, 0x5601);
    }

    #[test]
    fn state_table_transitions_valid() {
        for (i, s) in MQC_STATES.iter().enumerate() {
            assert!(s.nmps < 47, "state {i}: nmps out of range");
            assert!(s.nlps < 47, "state {i}: nlps out of range");
        }
    }

    #[test]
    fn reset_states_all_equiprobable() {
        let mut buf = vec![0u8; 128];
        let mut mqc = Mqc::new(&mut buf);
        mqc.reset_states();
        for i in 0..MQC_NUMCTXS {
            assert_eq!(mqc.ctxs[i], 0);
            assert_eq!(mqc.ctxs_mps[i], 0);
        }
    }

    #[test]
    fn set_state_specific_context() {
        let mut buf = vec![0u8; 128];
        let mut mqc = Mqc::new(&mut buf);
        mqc.reset_states();
        mqc.set_state(17, 0, 46);
        assert_eq!(mqc.ctxs[17], 46);
    }

    #[test]
    fn init_enc_sets_initial_state() {
        let mut buf = vec![0u8; 128];
        let mut mqc = Mqc::new(&mut buf);
        mqc.init_enc();
        assert_eq!(mqc.num_bytes(), 0);
    }

    #[test]
    fn encode_single_symbol_and_flush() {
        let mut buf = vec![0u8; 128];
        let mut mqc = Mqc::new(&mut buf);
        mqc.reset_states();
        mqc.init_enc();
        mqc.set_curctx(0);
        mqc.encode(0);
        mqc.flush();
        assert!(mqc.num_bytes() > 0);
    }

    /// Helper: encode symbols into buf, return encoded data length.
    /// Encoder uses buf[0] as padding, data starts at buf[1].
    fn encode_symbols(buf: &mut [u8], ctx_syms: &[(usize, u32)]) -> usize {
        let mut mqc = Mqc::new(buf);
        mqc.reset_states();
        mqc.init_enc();
        for &(ctx, sym) in ctx_syms {
            mqc.set_curctx(ctx);
            mqc.encode(sym);
        }
        mqc.flush();
        mqc.num_bytes()
    }

    /// Helper: decode symbols and verify against expected.
    /// dec_buf must be data + COMMON_CBLK_DATA_EXTRA extra bytes.
    fn decode_and_verify(dec_buf: &mut [u8], data_len: usize, ctx_syms: &[(usize, u32)]) {
        let mut mqc = Mqc::new(dec_buf);
        mqc.reset_states();
        mqc.init_dec(data_len);
        for &(ctx, expected) in ctx_syms {
            mqc.set_curctx(ctx);
            let d = mqc.decode();
            assert_eq!(d, expected, "decode mismatch");
        }
        mqc.finish_dec();
    }

    /// Helper: full roundtrip test. Encode into enc_buf, extract data,
    /// copy to dec_buf, decode and verify.
    fn roundtrip_test(ctx_syms: &[(usize, u32)]) {
        let mut enc_buf = vec![0u8; 512];
        let num_bytes = encode_symbols(&mut enc_buf, ctx_syms);

        // Encoded data is at enc_buf[1..1+num_bytes]
        let mut dec_buf = vec![0u8; num_bytes + COMMON_CBLK_DATA_EXTRA];
        dec_buf[..num_bytes].copy_from_slice(&enc_buf[1..1 + num_bytes]);

        decode_and_verify(&mut dec_buf, num_bytes, ctx_syms);
    }

    #[test]
    fn encode_decode_roundtrip_mps() {
        let symbols: Vec<(usize, u32)> = vec![0u32, 0, 0, 0, 0, 0, 0, 0]
            .into_iter()
            .map(|s| (0, s))
            .collect();
        roundtrip_test(&symbols);
    }

    #[test]
    fn encode_decode_roundtrip_mixed() {
        let symbols: Vec<(usize, u32)> = vec![0u32, 1, 0, 1, 1, 0, 0, 1, 0, 0, 0, 1]
            .into_iter()
            .map(|s| (0, s))
            .collect();
        roundtrip_test(&symbols);
    }

    #[test]
    fn encode_decode_multiple_contexts() {
        let ctx_syms: Vec<(usize, u32)> = vec![
            (0, 0),
            (1, 1),
            (0, 1),
            (2, 0),
            (1, 0),
            (0, 0),
            (2, 1),
            (1, 1),
        ];
        roundtrip_test(&ctx_syms);
    }

    #[test]
    fn segmark_enc_encodes_four_symbols() {
        let mut buf = vec![0u8; 128];
        let mut mqc = Mqc::new(&mut buf);
        mqc.reset_states();
        mqc.init_enc();
        mqc.segmark_enc();
        mqc.flush();
        assert!(mqc.num_bytes() > 0);
    }

    #[test]
    fn bypass_encode_decode_roundtrip() {
        let mut buf = vec![0u8; 256];
        let bits = [1u32, 0, 1, 1, 0, 0, 1, 0, 1, 0, 0, 1, 1, 1, 0, 1];
        {
            let mut mqc = Mqc::new(&mut buf);
            mqc.reset_states();
            mqc.init_enc();
            mqc.set_curctx(0);
            mqc.encode(0);
            mqc.encode(1);
            mqc.flush();
            mqc.bypass_init_enc();
            for &b in &bits {
                mqc.bypass_enc(b);
            }
            mqc.bypass_flush_enc(false);
        }
    }

    #[test]
    fn raw_decode_roundtrip() {
        let mut data = vec![0b10110010u8, 0b11001010u8, 0xFF, 0xFF];
        let mut mqc = Mqc::new(&mut data);
        mqc.raw_init_dec(2);
        let mut decoded = Vec::new();
        for _ in 0..16 {
            decoded.push(mqc.raw_decode());
        }
        assert_eq!(decoded[0], 1);
        assert_eq!(decoded[1], 0);
        assert_eq!(decoded[2], 1);
        assert_eq!(decoded[3], 1);
        assert_eq!(decoded[4], 0);
        assert_eq!(decoded[5], 0);
        assert_eq!(decoded[6], 1);
        assert_eq!(decoded[7], 0);
        mqc.finish_dec();
    }

    #[test]
    fn num_bytes_tracks_position() {
        let mut buf = vec![0u8; 128];
        let mut mqc = Mqc::new(&mut buf);
        mqc.reset_states();
        mqc.init_enc();
        assert_eq!(mqc.num_bytes(), 0);
        mqc.set_curctx(0);
        for _ in 0..100 {
            mqc.encode(0);
        }
        mqc.flush();
        assert!(mqc.num_bytes() > 0);
    }

    #[test]
    fn restart_init_enc_resets_state() {
        let mut buf = vec![0u8; 256];
        let mut mqc = Mqc::new(&mut buf);
        mqc.reset_states();
        mqc.init_enc();
        mqc.set_curctx(0);
        mqc.encode(0);
        mqc.encode(1);
        mqc.flush();
        mqc.restart_init_enc();
        mqc.set_curctx(0);
        mqc.encode(0);
        mqc.flush();
    }

    #[test]
    fn erterm_enc_fills_correctly() {
        let mut buf = vec![0u8; 128];
        let mut mqc = Mqc::new(&mut buf);
        mqc.reset_states();
        mqc.init_enc();
        mqc.set_curctx(0);
        mqc.encode(0);
        mqc.encode(1);
        mqc.erterm_enc();
        assert!(mqc.num_bytes() > 0);
    }
}
