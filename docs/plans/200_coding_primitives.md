# Phase 200: 符号化プリミティブ

Status: IMPLEMENTED

## 概要

Tier-1/Tier-2 符号化の基盤となるモジュール群を実装する。MQ算術コーダ、タグツリー、スパース配列。

## C版対応ファイル

| C版ファイル            | Rustモジュール           | 概要                                 |
| ---------------------- | ------------------------ | ------------------------------------ |
| `mqc.c/h`, `mqc_inl.h` | `coding/mqc.rs`          | MQ算術コーダ（エントロピー符号化器） |
| `tgt.c/h`              | `coding/tgt.rs`          | タグツリー（レート歪み最適化用）     |
| `sparse_array.c/h`     | `coding/sparse_array.rs` | スパース配列（メモリ効率化）         |

## モジュール詳細

### coding/mqc.rs

JPEG 2000 の MQ 算術コーダ。T1（Tier-1）がコードブロック係数の符号化/復号に使用する。

#### C版との対応

| C版関数/定数                     | Rust                            | 備考                          |
| -------------------------------- | ------------------------------- | ----------------------------- |
| `MQC_NUMCTXS` (19)               | `MQC_NUMCTXS: usize = 19`       |                               |
| `opj_mqc_state_t`                | `MqcState`（static配列の要素）  | ポインタ→インデックス         |
| `mqc_states[94]`                 | `MQC_STATES: [MqcState; 47]`    | MPS/LPS対を1エントリに統合    |
| `opj_mqc_t`                      | `Mqc`                           | コンテキスト→インデックス管理 |
| `opj_mqc_setcurctx`              | `Mqc::set_curctx()`             |                               |
| `opj_mqc_init_enc`               | `Mqc::init_enc()`               |                               |
| `opj_mqc_encode`                 | `Mqc::encode()`                 |                               |
| `opj_mqc_flush`                  | `Mqc::flush()`                  |                               |
| `opj_mqc_bypass_init_enc`        | `Mqc::bypass_init_enc()`        |                               |
| `opj_mqc_bypass_enc`             | `Mqc::bypass_enc()`             |                               |
| `opj_mqc_bypass_flush_enc`       | `Mqc::bypass_flush_enc()`       |                               |
| `opj_mqc_bypass_get_extra_bytes` | `Mqc::bypass_get_extra_bytes()` |                               |
| `opj_mqc_reset_enc`              | `Mqc::reset_enc()`              |                               |
| `opj_mqc_restart_init_enc`       | `Mqc::restart_init_enc()`       |                               |
| `opj_mqc_erterm_enc`             | `Mqc::erterm_enc()`             |                               |
| `opj_mqc_segmark_enc`            | `Mqc::segmark_enc()`            |                               |
| `opj_mqc_init_dec`               | `Mqc::init_dec()`               |                               |
| `opj_mqc_raw_init_dec`           | `Mqc::raw_init_dec()`           |                               |
| `opj_mqc_decode`                 | `Mqc::decode()`                 | マクロ→メソッド               |
| `opj_mqc_raw_decode`             | `Mqc::raw_decode()`             |                               |
| `opj_mqc_finish_dec`             | `Mqc::finish_dec()`             |                               |
| `opj_mqc_resetstates`            | `Mqc::reset_states()`           |                               |
| `opj_mqc_setstate`               | `Mqc::set_state()`              |                               |
| `opj_mqc_numbytes`               | `Mqc::num_bytes()`              |                               |

#### データ構造

```rust
/// MQ状態遷移テーブルのエントリ。
/// C版では MPS/LPS で 94 エントリだが、Rust版では対にして 47 エントリ。
struct MqcState {
    qeval: u32,          // LPS の推定確率
    nmps: usize,         // MPS 符号化後の次状態インデックス
    nlps: usize,         // LPS 符号化後の次状態インデックス
    switch_mps: bool,    // LPS 発生時に MPS を反転するか
}

/// MQ算術コーダ本体。
pub struct Mqc<'a> {
    buf: &'a mut [u8],   // 出力/入力バッファ
    bp: usize,           // バッファ内の現在位置
    c: u32,              // 符号語レジスタ
    a: u32,              // 確率区間
    ct: u32,             // ビットカウンタ
    start: usize,        // 開始位置（num_bytes計算用）
    ctxs: [usize; 19],   // 19コンテキスト（状態インデックス）
    ctxs_mps: [u8; 19],  // 各コンテキストの MPS 値
    curctx: usize,       // 現在のアクティブコンテキスト番号
    // デコーダ用
    end_of_byte_stream_counter: u32,
    backup: [u8; 2],     // 復元用バックアップ
}
```

#### 状態遷移テーブル

47 状態の静的テーブル `MQC_STATES`。C版 `mqc_states[94]` から MPS/LPS 対を統合。各エントリの `qeval` は ITU-T T.800 Table D.3 に準拠。

#### 主要アルゴリズム

**符号化**: `c` レジスタに確率区間 `a` を加減算し、正規化時にバイト出力。0xFF 後は 7 ビットスタッフィング。

**復号**: バッファからバイト入力し `c` レジスタを更新。`c >> 16` と閾値を比較して MPS/LPS を判定。

**バイパスモード**: 算術符号化を使わず直接ビットを読み書き。一様分布のビットに使用。

### coding/tgt.rs

タグツリー。Tier-2 がレート歪み最適化のためにコードブロックの包含/ゼロビットプレーン情報を階層的に符号化する。

#### C版との対応

| C版関数            | Rust                   | 備考             |
| ------------------ | ---------------------- | ---------------- |
| `opj_tgt_create`   | `TagTree::new()`       |                  |
| `opj_tgt_init`     | `TagTree::reset()`     | 既存ツリー再利用 |
| `opj_tgt_destroy`  | `Drop`                 | 自動解放         |
| `opj_tgt_reset`    | `TagTree::reset()`     |                  |
| `opj_tgt_setvalue` | `TagTree::set_value()` |                  |
| `opj_tgt_encode`   | `TagTree::encode()`    | `Bio` を使用     |
| `opj_tgt_decode`   | `TagTree::decode()`    | `Bio` を使用     |

#### データ構造

```rust
/// タグツリーノード。
struct TgtNode {
    parent: Option<usize>,  // 親ノードのインデックス（ルートは None）
    value: i32,             // ノード値
    low: i32,               // 処理済み最低閾値
    known: bool,            // 値が確定したか
}

/// タグツリー。
pub struct TagTree {
    numleafsh: u32,         // リーフの横幅
    numleafsv: u32,         // リーフの縦幅
    nodes: Vec<TgtNode>,    // 全ノード（リーフ先頭、親が後続）
}
```

#### 主要アルゴリズム

**ツリー構築**: リーフ層 (numleafsh × numleafsv) から各レベルで ceil(w/2) × ceil(h/2) に縮小し、ルート（1ノード）まで。全ノードをフラット `Vec` に格納。

**符号化**: リーフからルートまでスタックに積み、ルートから降りて各ノードの `low` から `threshold` までビットを出力。値に達すると 1 ビット出力。

**復号**: 同様にスタックベースでルートから降り、ビットを読みながら値を確定。

### coding/sparse_array.rs

スパース配列。DWT（逆ウェーブレット変換）が係数格納に使用する。必要なブロックのみメモリ確保。

#### C版との対応

| C版関数                            | Rust                             | 備考     |
| ---------------------------------- | -------------------------------- | -------- |
| `opj_sparse_array_int32_create`    | `SparseArray::new()`             |          |
| `opj_sparse_array_int32_free`      | `Drop`                           | 自動解放 |
| `opj_sparse_array_is_region_valid` | `SparseArray::is_region_valid()` |          |
| `opj_sparse_array_int32_read`      | `SparseArray::read_region()`     |          |
| `opj_sparse_array_int32_write`     | `SparseArray::write_region()`    |          |

#### データ構造

```rust
/// ブロックベースのスパース i32 配列。
pub struct SparseArray {
    width: u32,
    height: u32,
    block_width: u32,
    block_height: u32,
    block_count_hor: u32,
    block_count_ver: u32,
    data_blocks: Vec<Option<Vec<i32>>>,  // 未割当ブロックは None（暗黙的にゼロ）
}
```

#### 主要アルゴリズム

**読み取り**: 指定矩形領域に重なるブロックを走査。割当済みならコピー、未割当ならゼロ埋め。ストライド付き出力対応。

**書き込み**: 指定矩形領域に重なるブロックを走査。未割当なら確保（ゼロ初期化）してコピー。ストライド付き入力対応。

## テスト方針

### mqc.rs

- 状態テーブル: 全 47 エントリの `qeval`、遷移先、`switch_mps` を検証
- エンコード/デコードラウンドトリップ: 既知のビット列を符号化→復号して一致確認
- 0xFF スタッフィング: 0xFF バイト後のビット数が 7 になることを確認
- バイパスモード: encode→decode ラウンドトリップ
- `reset_states`: 全コンテキストが等確率状態にリセットされることを確認
- `set_state`: 個別コンテキストの状態設定
- `num_bytes`: 書き込みバイト数の正確性
- `segmark_enc`: セグメントマーカーの符号化

### tgt.rs

- ツリー構築: 各種サイズ (1×1, 2×2, 4×4, 3×5) でノード数と親子関係を検証
- `set_value` + encode/decode ラウンドトリップ
- 閾値ベースの段階的符号化/復号
- リセット後の状態

### sparse_array.rs

- 領域バリデーション: 範囲外・空領域の拒否
- 書き込み→読み取りラウンドトリップ
- 未割当ブロックの読み取りがゼロを返すこと
- 複数ブロックにまたがる領域の読み書き
- ストライド付きの読み書き
- ブロック境界でのパーシャル読み書き

## 依存関係

- `coding/mqc.rs`: `crate::types`（定数 `COMMON_CBLK_DATA_EXTRA`）
- `coding/tgt.rs`: `crate::io::bio::Bio`、`crate::types`（`uint_ceildiv`）
- `coding/sparse_array.rs`: `crate::types`（`uint_ceildiv`）
