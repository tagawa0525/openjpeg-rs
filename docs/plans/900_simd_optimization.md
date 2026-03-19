# Phase 900: SIMD最適化

Status: IMPLEMENTED

## Context

Phase 800（マルチスレッド）完了後の次フェーズ。DWT・MCTのホットループに`std::arch`によるSIMD最適化を追加する。

- **i32系（RCT, DWT 5-3）**: SSE2（128-bit, 4×i32）/ AVX2（256-bit, 8×i32）
- **f32系（ICT, DWT 9-7）**: SSE（128-bit, 4×f32）/ AVX（256-bit, 8×f32）

C版はSSE2/AVX2/AVX-512のコンパイル時分岐で実装。Rust版ではランタイムCPU検出 + `#[target_feature]`によるディスパッチで、スカラフォールバックを常に維持する。

Phase 900が完了すると、MCT変換・DWT垂直マルチカラムパスがSIMD加速される（マスタープランM6の一部）。

**unsafe方針**: SIMD intrinsicsは`std::arch`によるunsafeを許容する唯一のケース（マスタープラン記載）。各unsafe関数に`# Safety`ドキュメントを必須とし、スカラ版との一致をテストで検証する。

## C版対応コード

| C版ファイル/関数                      | SIMD対象         | 使用ISA    | 概要                        |
| ------------------------------------- | ---------------- | ---------- | --------------------------- |
| `dwt.c: opj_idwt53_v_cas0_mcols_*()`  | 垂直マルチカラム | SSE2, AVX2 | 16/32列を同時にリフティング |
| `dwt.c: opj_idwt53_h_cas0()`          | 水平リフティング | SSE2, AVX2 | 8/16サンプル同時処理（※）   |
| `mct.c: opj_mct_encode/decode()`      | RCT (i32)        | SSE2, AVX2 | 4/8サンプル/iter            |
| `mct.c: opj_mct_encode/decode_real()` | ICT (f32)        | SSE2/AVX   | 4/8サンプル/iter            |
| `t1.c` (量子化ループ)                 | 係数→float変換   | SSE2       | cvtepi32_ps + 乗算          |

※ DWT水平リフティングは本Phase 900のスコープ外。C版の参考情報として記載。

Rust版ではSSE2/AVX2をターゲットとし、AVX-512は延期する（普及率が低く、テスト環境の確保が困難）。

## SIMD最適化対象と優先順位

```text
優先度  対象                         期待高速化  根拠
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
 P1    MCT RCT/ICT (i32/f32)        2-4x       単純ループ、実装容易、効果確実
 P2    DWT 5-3 垂直マルチカラム     4-8x       最大ボトルネック、C版の主要最適化
 P3    DWT 9-7 垂直マルチカラム     4-8x       P2と同パターン（f32版）
```

### スコープ外（延期）

| 項目                        | 理由                                          |
| --------------------------- | --------------------------------------------- |
| AVX-512                     | 普及率低、テスト環境確保困難                  |
| AArch64 NEON                | x86_64を先行、ARM対応は将来                   |
| T1量子化SIMD                | 効果が限定的（MQが支配的）                    |
| DWT水平リフティングSIMD     | rayonによる行並列で十分な高速化が得られている |
| DWT interleave/deinterleave | 効果が限定的                                  |

## サブPR構成

| PR   | ブランチ        | スコープ                               | 推定LOC | 依存      |
| ---- | --------------- | -------------------------------------- | ------- | --------- |
| 900a | `feat/simd-mct` | SIMD基盤 + MCT RCT/ICT SSE2/AVX2       | ~400    | Phase 800 |
| 900b | `feat/simd-dwt` | DWT 5-3/9-7 垂直マルチカラム SSE2/AVX2 | ~600    | 900a      |

マージ順: 900a → 900b

## 設計判断

### Feature flag

SIMDはfeature flagを設けない。理由:

1. `std::arch`のintrinsicsは`#[target_feature(enable = "...")]`で保護される
2. ランタイムCPU検出で自動ディスパッチ（`is_x86_feature_detected!`）
3. SIMD非対応CPUでは自動的にスカラフォールバック
4. ユーザーがflag管理する必要がない

```rust
pub fn mct_encode(c0: &mut [i32], c1: &mut [i32], c2: &mut [i32]) {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        if is_x86_feature_detected!("avx2") {
            // Safety: AVX2 checked above
            unsafe { mct_encode_avx2(c0, c1, c2) };
            return;
        }
        if is_x86_feature_detected!("sse2") {
            // Safety: SSE2 checked above
            unsafe { mct_encode_sse2(c0, c1, c2) };
            return;
        }
    }
    mct_encode_scalar(c0, c1, c2);
}
```

### SIMDコードの配置

各モジュール内にサブモジュールを追加:

```text
src/transform/
├── mct.rs             # ディスパッチ + スカラ実装
├── mct_simd.rs        # SSE2/AVX2実装
├── dwt.rs             # ディスパッチ + スカラ実装
└── dwt_simd.rs        # SSE2/AVX2実装
```

### MCT SIMD (900a)

#### RCT (i32) — SSE2: 4サンプル/iter, AVX2: 8サンプル/iter

```rust
/// # Safety
///
/// Caller must ensure the CPU supports AVX2 (`is_x86_feature_detected!("avx2")`).
#[target_feature(enable = "avx2")]
unsafe fn mct_encode_avx2(c0: &mut [i32], c1: &mut [i32], c2: &mut [i32]) {
    let n = c0.len().min(c1.len()).min(c2.len());
    let chunks = n / 8;
    for i in 0..chunks {
        let off = i * 8;
        let r = _mm256_loadu_si256(c0[off..].as_ptr() as *const __m256i);
        let g = _mm256_loadu_si256(c1[off..].as_ptr() as *const __m256i);
        let b = _mm256_loadu_si256(c2[off..].as_ptr() as *const __m256i);
        // Y = (R + 2G + B) >> 2
        let y = _mm256_srai_epi32(
            _mm256_add_epi32(_mm256_add_epi32(r, _mm256_slli_epi32(g, 1)), b),
            2,
        );
        let cb = _mm256_sub_epi32(b, g);
        let cr = _mm256_sub_epi32(r, g);
        _mm256_storeu_si256(c0[off..].as_mut_ptr() as *mut __m256i, y);
        _mm256_storeu_si256(c1[off..].as_mut_ptr() as *mut __m256i, cb);
        _mm256_storeu_si256(c2[off..].as_mut_ptr() as *mut __m256i, cr);
    }
    // Scalar remainder
    let processed = chunks * 8;
    mct_encode_scalar(&mut c0[processed..], &mut c1[processed..], &mut c2[processed..]);
}
```

#### ICT (f32) — SSE: 4サンプル/iter, AVX: 8サンプル/iter

同パターン。`_mm256_mul_ps`, `_mm256_add_ps`使用。

### DWT垂直マルチカラムSIMD (900b)

C版の`opj_idwt53_v_cas0_mcols_SSE2_OR_AVX2()`に相当。

**基本アイデア**: 1列ずつgather→transform→scatterする代わりに、連続する8列（AVX2）をまとめてSIMDレジスタにロードし、リフティングステップを列方向にベクトル化する。

```text
現在のスカラ版:
  for col in 0..rw:           // 列ごとに逐次
    gather column → tmp[]
    dwt_1d(tmp)
    scatter tmp → column

SIMD版 (AVX2, 8列同時):
  for col_base in (0..rw).step_by(8):
    // 各行から連続8列をロード（メモリ上は連続）
    for row in 0..rh:
      v[row] = _mm256_loadu_si256(&data[row * stride + col_base])
    // リフティングステップをベクトル化（8列同時）
    dwt_1d_vertical_avx2(v, sn, dn)
    // 結果を書き戻し
    for row in 0..rh:
      _mm256_storeu_si256(&data[row * stride + col_base], v[row])
```

連続8列のロードはメモリ上連続なので、キャッシュ効率も向上する。

### テスト戦略

各SIMD関数に対して:

1. スカラ版とSIMD版の出力一致テスト（ランダムデータ）
2. 端数処理テスト（サンプル数がSIMD幅の倍数でないケース）
3. 既存のラウンドトリップテスト（SIMD有効でも結果同一を確認）

```rust
#[test]
fn mct_encode_simd_matches_scalar() {
    let mut c0_scalar = vec![...]; // ランダムデータ
    let mut c0_simd = c0_scalar.clone();
    // ... c1, c2 も同様
    mct_encode_scalar(&mut c0_scalar, &mut c1_scalar, &mut c2_scalar);
    mct_encode(&mut c0_simd, &mut c1_simd, &mut c2_simd);  // SIMDディスパッチ
    assert_eq!(c0_scalar, c0_simd);
}
```

## 検証

各PR完了時:

```bash
cargo test
cargo test --features parallel
cargo clippy --all-targets -- -D warnings
cargo clippy --all-targets --features parallel -- -D warnings
cargo fmt -- --check
```

SIMD版は`is_x86_feature_detected!`により自動選択されるため、特別なfeature flagは不要。テストはターゲットCPUのSIMD機能に応じて自動的にSIMD版またはスカラ版を実行する。

## リファレンスファイル

- `reference/openjpeg/src/lib/openjp2/dwt.c` — C版DWT SIMD（SSE2/AVX2/AVX-512）
- `reference/openjpeg/src/lib/openjp2/mct.c` — C版MCT SIMD（SSE2）
- `reference/openjpeg/src/lib/openjp2/t1.c` — C版T1量子化SIMD（参考）
