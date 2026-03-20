# Fix: T1 encoder/decoder roundtrip bugs (d11437d regression)

**Status: IN_PROGRESS**

## Context

Commit d11437d ("fix(t1): remove FRACBITS from encoder to align with decoder bitplanes") was intended to fix
encoder/decoder alignment but introduced two bugs and weakened tests to hide the failures:

- **Symptom 1**: TCD roundtrip `[0,4,8,...,252]` → `[63,71,79,...,191]` (63/64 mismatch)
- **Symptom 2**: API roundtrip uniform pixel 100 → 113 (+13 offset)

Root cause analysis identifies **two independent bugs** and **three test regressions**.

## Bug Analysis

### Bug 1: copy_decoded_cblks_to_data reads wrong data layout

- **Encoder** (`enc_sigpass` etc.): accesses `data[datap], data[datap+1], data[datap+2], data[datap+3]` → **stripe-column**
- **Decoder** (`dec_sigpass_mqc` etc.): accesses `data[datap], data[datap+w], data[datap+2w], data[datap+3w]` → **row-major**
- `copy_decoded_cblks_to_data` reads stripe-column (d11437d changed from column-major to stripe-column), but decoder produces row-major
- **Result**: pixel positions scrambled

### Bug 2: IMSB encode formula introduces off-by-one in decoder numbps

Tag tree decode loop returns `i = imsb_value + 1`. Combined with decode formula
`numbps = (band_numbps + 1) - i`, the effective formula is `numbps = band_numbps - imsb_value`.

| Version   | Encode IMSB                       | Decoded numbps    | Correct? |
| --------- | --------------------------------- | ----------------- | -------- |
| d11437d前 | `band_numbps - cblk_numbps`       | `cblk_numbps`     | Yes      |
| d11437d後 | `(band_numbps + 1) - cblk_numbps` | `cblk_numbps - 1` | **No**   |

Decoder gets 1 fewer bitplane → reconstruction at ~half scale → explains +13 offset for pixel 100.

### Not a bug: FRACBITS removal

FRACBITS don't change the MQ bitstream content (same bit values at same logical positions).
Removal simplifies encoder and makes it symmetric with decoder. Keep as-is.

### Not a bug: Identity dequant (`val` instead of `val / 2`)

With correct IMSB (numbps match), decoder reconstructs at correct scale. Identity is correct.
The old `/2` compensated for the pre-existing off-by-one in T2 decode `(band_numbps + 1)` formula.

## Changes

### Files to modify

| File              | Change                                                    |
| ----------------- | --------------------------------------------------------- |
| `src/tcd.rs`      | Fix `copy_decoded_cblks_to_data` to read row-major layout |
| `src/tcd.rs`      | Fix unit test `copy_decoded_cblks_single_res` data layout |
| `src/tcd.rs`      | Strengthen `encode_decode_roundtrip_single_tile` test     |
| `src/tier2/t2.rs` | Revert IMSB encode to `band_numbps - cblk_numbps`         |
| `src/tier2/t2.rs` | Fix encode roundtrip test's IMSB formula                  |
| `src/api.rs`      | Strengthen `encode_decode_roundtrip_jp2_with_pixels` test |

### Commit plan (TDD)

1. **RED** `test(tcd): strengthen roundtrip test to verify per-pixel accuracy`
   - `encode_decode_roundtrip_single_tile`: assert each pixel within ±1 of original
   - Will fail due to layout + IMSB bugs

2. **RED** `test(api): verify exact decoded value for uniform pixel roundtrip`
   - `encode_decode_roundtrip_jp2_with_pixels`: assert decoded value within ±1 of 100
   - Will fail due to IMSB bug

3. **GREEN** `fix(tcd): read decoded cblk data in row-major layout matching T1 decoder`
   - `copy_decoded_cblks_to_data` (tcd.rs:738-759): change from stripe-column to row-major

     ```rust
     // Before (stripe-column - WRONG):
     let mut datap = 0usize;
     for stripe_start in (0..desc.cblk_h).step_by(4) { ... }

     // After (row-major - matches decoder output):
     for j in 0..desc.cblk_h {
         for i in 0..desc.cblk_w {
             let dst_off = (desc.buf_y + j) * comp_w + desc.buf_x + i;
             let val = decoded_data[j * desc.cblk_w + i];
             comp.data[dst_off] = if desc.qmfbid == 1 { val } else { ... };
         }
     }
     ```

   - Fix `copy_decoded_cblks_single_res` test: use row-major test data layout
   - Fix `copy_decoded_cblks_two_res_subband_offsets` test: use row-major + adjust expected values

4. **GREEN** `fix(t2): revert IMSB encode to band_numbps - cblk_numbps`
   - `t2_encode_packet` (t2.rs:610-615): revert to `band_numbps - cblk_numbps`

     ```rust
     // Before (d11437d - WRONG):
     imsb.set_value(cblkno, ((band_numbps as u32 + 1).saturating_sub(cblk.numbps)) as i32);

     // After (correct):
     imsb.set_value(cblkno, (band_numbps as u32).saturating_sub(cblk.numbps) as i32);
     ```

   - Fix `t2_encode_decode_roundtrip` test: revert IMSB formula

5. **VERIFY** All tests pass, clippy clean, fmt clean

## Verification

```bash
cargo test                                     # all tests pass
cargo test encode_decode_roundtrip -- --nocapture  # verify pixel accuracy
cargo clippy --all-targets -- -D warnings
cargo fmt -- --check
```

After fix, expected behavior:

- TCD roundtrip: each decoded pixel within ±1 of original (T1 reconstruction bias)
- API roundtrip: uniform 100 decodes to 99-101
