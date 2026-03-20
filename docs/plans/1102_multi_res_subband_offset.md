# Fix: T1エンコーダのサブバンドオフセット欠落

Status: PLANNED

## Context

numresolutions > 1 のエンコード→デコードラウンドトリップで、デコード値が大幅にずれる（pixel 0: expected 0, got 87）。numresolutions=1（DWTなし）では pixel-perfect。

## Root Cause

`t1_encode_cblks`がタイルバッファからコードブロックのデータを抽出する際、サブバンドオフセットを加算していない。

C版（`t1.c` L2237-2244）:

```c
x = cblk->x0 - band->x0;
if (band->bandno & 1) x += pres[resno-1].x1 - pres[resno-1].x0;
if (band->bandno & 2) y += pres[resno-1].y1 - pres[resno-1].y0;
tiledp = &tilec->data[y * tile_w + x];
```

Rust版（`t1.rs` L1736-1737）:

```rust
let cblk_x0 = (cblk.x0 - comp.x0) as usize;  // オフセットなし
let cblk_y0 = (cblk.y0 - comp.y0) as usize;
```

numresolutions=1ではDWTが適用されず、サブバンドは1つ（LL）のみで全体をカバーするため問題が発生しない。numresolutions > 1では、forward DWT後のタイルバッファ内でサブバンドが特定の位置に配置されるが、エンコーダがオフセットなしで読み出すため、異なるサブバンドのデータを誤って符号化する。

## Changes

| File                 | Change                                                  |
| -------------------- | ------------------------------------------------------- |
| `src/coding/t1.rs`   | `t1_encode_cblks`: サブバンドオフセット計算を追加       |
| `tests/roundtrip.rs` | `roundtrip_53_lossless_multi_resolution` のignore除去   |

## Commits

1. RED: `roundtrip_53_lossless_multi_resolution` の `#[ignore]` 除去
2. GREEN: `t1_encode_cblks` にサブバンドオフセットを追加
