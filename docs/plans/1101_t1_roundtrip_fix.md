# Fix: T1 encoder/decoder roundtrip bugs

Status: IMPLEMENTED

## Context

T1 encoder/decoder roundtrip test failures:

- **Symptom 1**: TCD roundtrip `[0,4,8,...,252]` → scrambled values (pixel positions mixed)
- **Symptom 2**: API roundtrip uniform pixel 100 → 114 (+14 offset)

Root cause analysis identified **three independent bugs**.

## Bug Analysis

### Bug 1: Encoder data preparation uses wrong layout (root cause)

The T1 encoder scan pattern (`enc_sigpass`, `enc_refpass`, `enc_clnpass`) reads data
sequentially with `datap += 4` per column within 4-row stripes — **stripe-column** order.

But `t1_encode_cblks` prepared data in **column-major** layout (`col * h + row`).
These layouts only match for single-stripe blocks (h ≤ 4). For taller blocks, the encoder
read coefficients from wrong spatial positions, producing a corrupted MQ bitstream.

**Example (8×8 block):**

| Index | Column-major (wrong)           | Stripe-column (correct) |
| ----- | ------------------------------ | ----------------------- |
| 0-3   | col0, rows 0-7                 | col0, rows 0-3          |
| 4-7   | col0, rows 4-7 (still col0!)   | col1, rows 0-3          |
| 8-11  | col1, rows 0-3                 | col2, rows 0-3          |

### Bug 2: copy_decoded_cblks_to_data reads wrong layout

`copy_decoded_cblks_to_data` read decoded data as column-major (`data[col * h + row]`),
but the T1 decoder (`dec_sigpass_mqc` etc.) writes in row-major (`data[row * w + col]`).

### Bug 3: Unnecessary /2 dequantization

`copy_decoded_cblks_to_data` applied `val / 2` for reversible mode (qmfbid==1).
With correct IMSB formula (`band_numbps - cblk_numbps`), encoder and decoder numbps
match, so the decoder already reconstructs at the correct scale. The `/2` halved
the output unnecessarily.

### Not a bug: IMSB formula

The IMSB encode formula `band_numbps - cblk_numbps` and decode formula
`(band_numbps + 1) - i` (where tag tree decode returns `i = value + 1`)
correctly roundtrip to produce `decoded_numbps = cblk_numbps`. No change needed.

### Not a bug: FRACBITS

The encoder shifts input by `T1_NMSEDEC_FRACBITS` (6 bits) and subtracts FRACBITS
from numbps. The decoder uses this adjusted numbps and reconstructs at the original
(unshifted) scale. This asymmetry is intentional and correct.

## Changes

### Files modified

| File               | Change                                                              |
| ------------------ | ------------------------------------------------------------------- |
| `src/coding/t1.rs` | Fix data preparation: column-major → stripe-column layout           |
| `src/tcd.rs`       | Fix `copy_decoded_cblks_to_data`: column-major → row-major read     |
| `src/tcd.rs`       | Fix dequantization: `val / 2` → `val` (identity) for reversible     |
| `src/tcd.rs`       | Fix unit tests for new data layouts                                 |
| `src/tcd.rs`       | Strengthen `encode_decode_roundtrip_single_tile` (per-pixel ±1)     |
| `src/tcd.rs`       | Add `encode_decode_pipeline_stages` regression test                 |
| `src/api.rs`       | Strengthen `encode_decode_roundtrip_jp2_with_pixels` (per-pixel ±1) |

### Commits

1. **RED** `test(tcd): strengthen roundtrip test to verify per-pixel accuracy`
2. **GREEN** `fix(t1,tcd): fix encoder/decoder data layout and dequantization`
3. `test(tcd): add pipeline stage regression test`

## Verification

```bash
cargo test --lib                               # 429 passed
cargo clippy --all-targets -- -D warnings       # clean
cargo fmt -- --check                            # clean
```

After fix:

- TCD roundtrip: each decoded pixel within ±1 of original
- API roundtrip: uniform 100 decodes to 99-101
