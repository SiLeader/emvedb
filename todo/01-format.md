# M1: フォーマット層（構造体 ⇄ バイト列の変換）

## このマイルストーンでやること

ファイルに保存する「**バイトの並べ方のルール（＝文法）**」をコードにします。具体的には:

- `Header`（ファイル先頭64バイトの情報）を **バイト列に変換 (encode)** したり、**バイト列から復元 (decode)** する
- `Frame`（追加 Put / 削除 Delete の1操作）を同じく encode / decode する
- 壊れ検出のための **CRC32C** を付ける／検証する

**重要**: このマイルストーンでは**ファイルには一切触りません**。相手は `&[u8]`（バイトの並び）と `Vec<u8>`
だけ。だから純粋な計算問題として、テストでガッチリ固められます。

## なぜ必要か

DB は最終的にバイト列としてファイルに書かれます。「構造体 → バイト列 → 構造体」と往復しても**1ビットも変わらず元に戻る**
ことが、データの信頼性の土台です。ここをテストで固めておくと、後のファイル I/O やクラッシュ回復のデバッグが劇的にラクになります。

## 出てくる言葉

| 言葉              | 意味                                                                                          |
|-----------------|---------------------------------------------------------------------------------------------|
| encode（直列化）     | 構造体 → バイト列。「保存できる形にする」。                                                                     |
| decode（復元）      | バイト列 → 構造体。「読み戻す」。                                                                          |
| リトルエンディアン       | 数値をバイトにする時の並び順。下位から先。Rust では `1234u32.to_le_bytes()` で得られる。読む時は `u32::from_le_bytes([..])`。 |
| CRC32C          | 4バイトのチェックサム。`crc32c::crc32c(&bytes)` で計算。保存時に末尾に付け、読む時に再計算して一致を確認 → 化けを検出。                  |
| magic（マジックナンバー） | 「これは確かに EmveDB のファイルだよ」という目印の固定バイト列。                                                        |
| ラウンドトリップ        | encode して decode したら元に戻ること。テストの定番。                                                          |

## 対象モジュール

- `src/format/mod.rs`, `src/format/header.rs`, `src/format/frame.rs`

## 前提

- M0 完了

---

## フォーマットの中身（SPEC §5）

> ⚠️ すべて**リトルエンディアン**。整合性チェックは**CRC32C**。

### ヘッダ：ファイル先頭の固定64バイト（§5.1）

ファイルの一番最初に1回だけ書かれる「DB全体の説明書き」です。下の表の通りにバイトを並べます。

| 位置(offset) | サイズ | フィールド            | 中身                                |
|------------|-----|------------------|-----------------------------------|
| 0          | 8   | `magic`          | ASCII `"EMVEDB\0\0"`（`\0`はヌル文字）   |
| 8          | 2   | `format_version` | `u16` = 1                         |
| 10         | 4   | `generation`     | `u32`。`compact()` するたびに +1 する世代番号 |
| 14         | 1   | `metric`         | `0=Cosine, 1=L2, 2=Dot`           |
| 15         | 1   | `element_type`   | `0=f32`（今は f32 固定）                |
| 16         | 4   | `dimension`      | `u32`。ベクトルの長さ                     |
| 20         | 4   | `flags`          | `u32`（将来用。今は0）                    |
| 24         | 36  | `reserved`       | 全部0で埋める（将来拡張用の空き）                 |
| 60         | 4   | `header_crc32c`  | **バイト0〜59**（前60バイト）の CRC32C       |

> ポイント: CRC は「最後の4バイトを**除いた**前60バイト」に対して計算し、その結果を最後の4バイトに書きます。読む時は、前60バイトから自分で
> CRC を再計算して、ファイルに書いてあった4バイトと一致するか見ます。

### フレーム：1操作ぶんのバイト列（§5.2）

追加や削除を1回するたびに、この形のかたまりをファイル末尾に足します。

```
[ record_len: u32 ][ body（中身）... ][ crc32c: u32 ]
```

- `record_len` = `body のバイト数 + 4`（後ろに続く body と crc を合わせたバイト数）
- `crc32c` = 「`record_len` の4バイト ＋ `body`」を**つなげたバイト列**の CRC32C
    - ※ `record_len` も CRC に含めるのは、長さフィールド自体の化けも検出するため
- 読む手順: まず `record_len`(4B) を読む → 続く `record_len` バイトを読む → その末尾4バイトが crc、その手前が body

**Put（追加/更新）の body**:

| サイズ             | フィールド         | 中身                |
|-----------------|---------------|-------------------|
| 1               | `type`        | `1`（Put の印）       |
| 1               | `flags`       | 予約（0）             |
| 8               | `id`          | `u64`             |
| 4               | `payload_len` | `u32`（ペイロードのバイト数） |
| `4 * dimension` | `vector`      | `f32` × dimension |
| `payload_len`   | `payload`     | おまけバイト列           |

→ `body_len = 14 + 4*dimension + payload_len`（1+1+8+4 = 14 が固定部分）

**Delete（削除）の body**:

| サイズ | フィールド   | 中身             |
|-----|---------|----------------|
| 1   | `type`  | `2`（Delete の印） |
| 1   | `flags` | 予約（0）          |
| 8   | `id`    | `u64`          |

→ `body_len = 10`

---

## 手順（タスク）

### `header.rs`

- [ ] 構造体を定義
  ```rust
  pub(crate) struct Header {
      pub format_version: u16,
      pub generation: u32,
      pub metric: Metric,
      pub element_type: u8,
      pub dimension: u32,
      pub flags: u32,
  }
  ```
- [ ] 定数を用意
    - `const MAGIC: [u8; 8] = *b"EMVEDB\0\0";`
    - `const HEADER_LEN: usize = 64;`
    - `const FORMAT_VERSION: u16 = 1;`
- [ ] `encode(&self) -> [u8; 64]`
    - 64バイトの配列を0で初期化 → 上の表の位置に各フィールドを `to_le_bytes()` で書き込む → `reserved` は0のまま →
      最後にバイト0〜59のCRCを計算して位置60に書く
- [ ] `decode(buf: &[u8; 64]) -> Result<Header>` … 検証しながら復元
    - magic が一致しない → `InvalidMagic`
    - 前60バイトの CRC を再計算 → 末尾4バイトと不一致 → `Corrupt`
    - `format_version` が想定外（メジャー不一致）→ `UnsupportedVersion`
    - `metric` / `element_type` が未知の値 → `Corrupt`
- [ ] `with_incremented_generation(&self) -> Header` … `generation` を +1 した新しい Header を返す（compact 用、M6 で使う）

### `frame.rs`

- [ ] 書き込み用の型（読み手に優しいゼロコピー寄り）
  ```rust
  pub(crate) enum Frame<'a> {
      Put { id: u64, vector: &'a [f32], payload: &'a [u8] },
      Delete { id: u64 },
  }
  ```
- [ ] スキャン結果用の型 … ファイルを読み進めたとき「種別＋各フィールドがファイルのどこ（offset）に何バイトあるか」を返す型
  ```rust
  pub(crate) enum FrameRef {
      Put { id: u64, vector_range: Range<u64>, payload_range: Range<u64> },
      Delete { id: u64 },
  }
  ```
    - なぜ range を返す？ → ペイロードは大きいことがあるので、ここでは丸ごと読まず「場所だけ」覚えて、`get()`
      の時に遅延ロードするため（SPEC §9.2）
- [ ] encode 関数（既存の `Vec<u8>` に追記する形にすると効率的）
    - [ ] `encode_put(id, vector, payload, out: &mut Vec<u8>)`
    - [ ] `encode_delete(id, out: &mut Vec<u8>)`
    - 中身の流れ（Put の例）: body を組み立てる → `record_len = body.len()+4` を計算 → `record_len` を `out` に書く → body
      を書く → 「record_len+body」のCRCを計算して書く
- [ ] `decode_body(record_len, body: &[u8], dimension: u32) -> Result<DecodedFrame>`
    - `type` が 1/2 以外 → `Corrupt`
    - 長さが合わない（Put なのに `body.len() != 14 + 4*dimension + payload_len` など）→ `Corrupt`
- [ ] CRC ヘルパ: `crc_of(record_len_le: [u8;4], body: &[u8]) -> u32`
- [ ] バイト長を計算するヘルパ（事前にサイズが分かると確保が一発で済む）
    - `put_frame_len(dim, payload_len) -> usize`
    - `delete_frame_len() -> usize`

### 共通

- [ ] エンディアン変換は、小さなヘルパ（`read_u32_le` 等）を作るか、`to_le_bytes`/`from_le_bytes` を直接使うか、**どちらかに統一
  **する（混在させない）

---

## 動作確認（受け入れ条件）

- [ ] **ラウンドトリップ**: `Header::decode(&Header::encode(&h))` が元の `h` と全フィールド一致する。
- [ ] Put/Delete フレームを encode → そのバイト列を decode して元の値に戻る。
- [ ] CRC のバイトを**1ビットだけ反転**させると、header は `Corrupt`、frame は検証で不一致を検出できる。
- [ ] 壊れ系の入力（短すぎる buf、`type=0`、dimension と長さが合わない）が `Corrupt` を返す。

## つまずきポイント / 実装メモ

- このモジュールは**ファイル I/O を一切持たない**。`&[u8]` / `Vec<u8>` だけを扱う（テストのしやすさと、後のクラッシュ回復実装の単純化のため）。
- `vector: &[f32]` をバイトにするのは、各要素を `f32::to_le_bytes()` してループで詰めるだけでOK。SIMD や `bytemuck` での高速化は
  M8 以降の課題（まずは素直に）。
- `dimension` はフレーム自身は持たない。decode する時に**外から渡す**（dimension はヘッダにしか書いていないので）。ここを忘れると
  Put の body の長さが計算できず詰まる。
- 困ったら「1件 encode してバイト列を `{:02x?}` で表示 → 表と手で照合」が一番速いデバッグ。
