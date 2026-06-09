# EmveDB

このライブラリは、EmveDB という単一ファイルのベクトル検索 DB を提供します。
Rust 製のライブラリ（埋め込み型）で、外部サーバを必要としません。

---

## 1. 概要・設計目標

- **単一ファイル**: すべてのデータ（ヘッダ・ベクトル・ペイロード）を 1 つのファイルに保存する。
- **Append-only**: 追加・更新・削除はすべてファイル末尾への追記で表現し、既存領域の上書きを行わない。これにより書き込みが高速かつクラッシュ耐性が高い。
- **論理削除/論理更新**: 削除・更新は論理操作（トゥームストーン追記 / 新バージョン追記）で行い、物理的な領域回収は
  `compact()` で行う。
- **厳密な全探索**: v1 の検索は近似なしのブルートフォース（全件距離計算）。フォーマットは将来 ANN インデックスを追加できるよう拡張余地を残す。
- **埋め込み利用**: ライブラリとしてプロセス内に組み込んで使う。プロセス間はファイルロックで協調する。
- **`:memory:` 対応**: ファイルパスに `:memory:` を指定すると永続化せずインメモリで動作する。

### 非目標（v1 スコープ外）

- 近似最近傍探索（HNSW/IVF 等）。フォーマット上は拡張可能とするが、実装は v1 に含めない。
- 分散・レプリケーション・ネットワークプロトコル。
- ペイロード内容に対するサーバサイドのフィルタ DSL（インプロセスのコールバックフィルタのみ提供。§6.3）。
- f32 以外の要素型（f16/i8 量子化）。ヘッダに予約領域は確保するが実装は将来。

---

## 2. 用語

| 用語       | 意味                                             |
|----------|------------------------------------------------|
| レコード     | 1 件の `(id, vector, payload)` の組。               |
| フレーム     | ファイルに追記される 1 つの操作単位（Put / Delete）。             |
| トゥームストーン | 論理削除を表す Delete フレーム。                           |
| ライブ      | 同一 id の最新フレームが Put で、かつ Delete で打ち消されていない状態。   |
| ペイロード    | アプリが任意にシリアライズした不透明バイト列（blob）。EmveDB は内容を解釈しない。 |

---

## 3. データモデル

- **id**: ユーザ指定の `u64`。レコードの主キー。
- **vector**: `f32` の配列。長さは DB ごとに固定の `dimension`。
    - 値は有限（NaN / ±Inf を含まない）でなければならない。違反は `InvalidVector` エラー。
    - 要素型は v1 では `f32` 固定（ヘッダの `element_type` で表現、将来拡張用）。
- **payload**: 任意の不透明バイト列。長さ 0 を許可。最大長は既定 16 MiB（設定可、ハード上限はフレーム長 `u32` 由来で約 4 GiB
  未満）。EmveDB は内容を一切解釈しない。
- **dimension**: DB 作成時に固定。許容範囲は `1..=65535`（推奨）。以降その DB では変更不可。
- **metric**: DB 作成時に 1 つ固定（§6.1）。`Cosine` / `L2`(ユークリッド) / `Dot`(内積)。

### 3.1 upsert / 削除のセマンティクス

- `put(id, vector, payload)` は **upsert**。同一 id が既存なら論理更新（新しい Put フレームを追記し、以降その id
  は最新フレームを指す）。
- `delete(id)` は最新が Put のとき論理削除（Delete フレーム追記）。存在しなければ no-op。
- 同一 id について「最後に追記されたフレーム」が真の状態を決める（last-writer-wins）。
- 物理的な旧バージョン/トゥームストーンの回収は `compact()`（§7）まで行わない。

---

## 4. 公開 API（Rust）

シグネチャは確定仕様ではなく指針。エラーは `Result<T, EmveError>`（§11）。

```rust
// --- 生成・オープン ---
// 新規作成。既存ファイルがあればエラー（AlreadyExists）。
Db::create(path: impl AsRef<Path>, opts: CreateOptions) -> Result<Db>;
// 既存をオープン。ヘッダを読み metric/dimension を取得。
Db::open(path: impl AsRef<Path>, opts: OpenOptions) -> Result<Db>;
// 無ければ create、有れば open。
Db::open_or_create(path: impl AsRef<Path>, opts: CreateOptions) -> Result<Db>;

// --- 書き込み（ReadWrite モードのみ）---
fn put(&self, id: u64, vector: &[f32], payload: &[u8]) -> Result<()>; // upsert
fn delete(&self, id: u64) -> Result<bool>; // 存在して削除したら true
fn flush(&self) -> Result<()>;             // fsync（SyncMode に依らず強制）
fn compact(&self) -> Result<()>;           // 物理再構築（§7）

// --- 読み取り ---
fn get(&self, id: u64) -> Result<Option<Record>>;            // Record{ id, vector, payload }
fn contains(&self, id: u64) -> Result<bool>;
fn search(&self, query: &[f32], k: usize, opts: SearchOptions) -> Result<Vec<SearchResult>>;
fn len(&self) -> usize;          // ライブ件数
fn is_empty(&self) -> bool;
fn metric(&self) -> Metric;
fn dimension(&self) -> u32;

// Drop 時に flush（ReadWrite のとき）＋ロック解放。
```

```rust
pub struct CreateOptions {
    pub dimension: u32,
    pub metric: Metric,                  // Cosine | L2 | Dot
    pub sync: SyncMode,                  // 既定 OnFlush
    pub max_payload_len: usize,          // 既定 16 MiB
}
pub struct OpenOptions {
    pub mode: OpenMode,                  // ReadOnly | ReadWrite
    pub lock_wait: bool,                 // ロック競合時に待つ(true) / 即エラー(false, 既定)
    pub sync: SyncMode,                  // ReadWrite 時のみ有効
}
pub enum SyncMode { Always, OnFlush /*既定*/, Never }
pub struct SearchResult {
    pub id: u64,
    pub score: f32,
    pub distance: f32
}
pub struct SearchOptions {
    pub filter: Option<Box<dyn Fn(u64) -> bool + Send + Sync>>, // §6.3
    pub min_score: Option<f32>,          // メトリックに応じた足切り
}
```

---

## 5. ファイルフォーマット

- バイト順は全フィールド **リトルエンディアン**。
- 構造は **64 バイトのヘッダ** ＋ **可変長フレームの追記列**。
- 整合性検出に **CRC32C（Castagnoli）** を用いる。

### 5.1 ヘッダ（先頭 64 バイト・固定）

| オフセット | サイズ | フィールド            | 内容                                         |
|-------|-----|------------------|--------------------------------------------|
| 0     | 8   | `magic`          | ASCII `"EMVEDB\0\0"`                       |
| 8     | 2   | `format_version` | `u16`。本仕様は `1`。                            |
| 10    | 4   | `generation`     | `u32`。DBファイル自体の世代管理。`compact()` 時にインクリメント。 |
| 14    | 1   | `metric`         | `0=Cosine, 1=L2, 2=Dot`                    |
| 15    | 1   | `element_type`   | `0=f32`（将来 `1=f16, 2=i8` 等）                |
| 16    | 4   | `dimension`      | `u32`                                      |
| 20    | 4   | `flags`          | `u32`（将来拡張。未使用ビットは 0）                      |
| 24    | 36  | `reserved`       | 0 埋め                                       |
| 60    | 4   | `header_crc32c`  | バイト `0..60` の CRC32C                       |

- オープン時に `magic` と `header_crc32c` を検証。`format_version` のメジャー不一致は `UnsupportedVersion`。
- ヘッダはファイル生成時に一度だけ書かれ、以後不変。

### 5.2 フレーム共通レイアウト

各フレームは次の並び:

```
[ record_len: u32 ][ body ... ][ crc32c: u32 ]
```

- `record_len` = `len(body) + 4`（後続バイト数 = body + crc）。
- `crc32c` は `record_len` フィールドと `body` を連結したバイト列の CRC32C（長さフィールドの破損も検出）。
- リーダは `record_len` を読み、続く `record_len` バイトを読み、末尾 4 バイトを crc として検証する。

#### Put フレーム（body）

| サイズ             | フィールド         | 内容                |
|-----------------|---------------|-------------------|
| 1               | `type`        | `1`               |
| 1               | `flags`       | 予約（0）             |
| 8               | `id`          | `u64`             |
| 4               | `payload_len` | `u32`             |
| `4 * dimension` | `vector`      | `f32` × dimension |
| `payload_len`   | `payload`     | 不透明バイト列           |

`body_len = 14 + 4*dimension + payload_len`

#### Delete フレーム（body）

| サイズ | フィールド   | 内容    |
|-----|---------|-------|
| 1   | `type`  | `2`   |
| 1   | `flags` | 予約（0） |
| 8   | `id`    | `u64` |

`body_len = 10`

### 5.3 整合性とリカバリ（torn write）

- オープン時、ヘッダ検証後にフレームを先頭から順次スキャンして検証する。
- **末尾の不完全フレーム / crc 不一致**（残りバイト不足、または最終フレームのみ crc NG）は、書き込み途中のクラッシュ（torn
  write）とみなす:
    - `ReadWrite`: 最後に成功した有効フレーム末尾までファイルを **truncate** して回復する。
    - `ReadOnly`: 末尾以降を無視して読み込む（ファイルは変更しない）。
- **中間フレームの crc 不一致**（末尾でない位置の破損）は回復不能とみなし `Corrupt` エラー。
- `.compact` 一時ファイルが残存していれば未完了の compaction とみなし削除する（§7）。

---

## 6. 検索

### 6.1 メトリックとスコア

DB 作成時に固定。`search` は結果を「良い順（best-first）」に返す。

| metric   | distance の定義     | ランキング       | score の意味            |
|----------|------------------|-------------|----------------------|
| `Cosine` | コサイン距離 `1 - cos` | score 降順    | `cos`（−1..1、大きいほど類似） |
| `L2`     | ユークリッド距離         | distance 昇順 | `-distance`（大きいほど類似） |
| `Dot`    | —                | score 降順    | 内積（大きいほど類似）          |

- `SearchResult.score` はメトリックに依らず「大きいほど良い」値に正規化して返す。`distance` は人間が解釈しやすい生の距離（Dot
  では `score` と同値）。
- Cosine は内部で正規化を行う。ベクトルのノルムは追記時に前計算してインメモリに保持し、クエリ側は検索時に正規化する。*
  *ゼロノルムベクトル**は Cosine では類似度を定義できないため score を `-inf`（最下位）として扱う。
- L2 はランキングに二乗距離を用い、最終的な `distance` のみ平方根を取る。

### 6.2 アルゴリズム

- ライブな全レコードに対し距離を計算し、サイズ `k` の有界ヒープで上位 `k` を選ぶ（`O(N·dimension)` 時間、`O(k)` 追加メモリ）。
- クエリ長が `dimension` と不一致なら `DimensionMismatch`。

### 6.3 フィルタ

- `SearchOptions.filter` にインプロセスのコールバック `Fn(u64) -> bool` を渡すと、`false` の id をスコアリング対象から除外できる。
- 不透明ペイロードに対する条件付き検索が必要な場合は、アプリ側が id→属性の対応を保持してこのコールバックで判定する（EmveDB
  はペイロードを解釈しない）。

---

## 7. compact()

物理的な領域回収（旧バージョン・トゥームストーンの除去）を行う。

- 実行中は DB を排他ロックし、他の書き込み・compact をブロックする（読み取りも一貫性のためブロック対象。§8）。
- DB ファイルと同じディレクトリに `<DBファイル名>.compact` という一時ファイルを作る。
- 手順:
    1. 一時ファイルに新しいヘッダを書く（metric/dimension は元と同一）。
    2. ライブなレコードのみを Put フレームとして書き出す（id ごとに最新の 1 件、トゥームストーンは除外）。
    3. 一時ファイルを fsync。
    4. 一時ファイルを元ファイル名へ **アトミックに rename**（同一ディレクトリ内 → POSIX で原子的に置換）。
    5. 親ディレクトリを fsync（rename の永続化）。
    6. インメモリインデックスのオフセットを新ファイルに合わせて更新。
- **クラッシュ安全性**: rename は原子的なので、任意の時点で「旧ファイルが完全」か「新ファイルが完全」のいずれか。途中でクラッシュした場合、次回オープン時に残存する
  `.compact` を削除すれば元ファイルでそのまま回復できる。
- `:memory:` の compact はファイルを使わず、インメモリ構造から旧バージョン/トゥームストーンを取り除いて再構築する。

---

## 8. 並行性とロック

### 8.1 プロセス間（ファイルロック）

OS の **アドバイザリファイルロック**（POSIX `flock`/`fcntl`、Windows `LockFileEx`）を用いる。

- `ReadWrite` オープン → **排他ロック**を取得。
- `ReadOnly` オープン → **共有ロック**を取得。
- よって: 書き込みは 1 プロセスのみ。読み取りは複数プロセス同時可。**ただし書き込みオープン中は読み取りオープン不可**
  （排他ロックが共有ロック取得をブロック）。
- ロック競合時の挙動は `OpenOptions.lock_wait`: `false`（既定）なら即 `Locked` エラー、`true` ならロック取得まで待機。
- ロックは DB ハンドルの生存期間中保持し、`Drop` で解放する。

### 8.2 プロセス内（スレッド間）

- `Db` は `Send + Sync`。内部で `RwLock` 相当により協調する。
- `get` / `search` は読み取りロック（複数同時実行可）。`put` / `delete` / `flush` / `compact` は書き込みロック（直列化）。
- `compact` 実行中は読み取りもブロックされる（オフセット再配置の一貫性のため）。
- ハンドルは `Arc<Db>` 等で複数スレッド共有できる。

---

## 9. 永続化とメモリモデル

### 9.1 インメモリインデックス

- オープン時に全フレームをスキャンして `HashMap<u64, Entry>` を構築する（最新フレーム勝ち、トゥームストーンは除去）。
- ベクトルは連続した `f32` アリーナ（`Vec<f32>`）にキャッシュフレンドリに保持し、ブルートフォースを高速化する。`Entry`
  はアリーナ内オフセット、ペイロードのファイル内オフセット/長さ、Cosine 用の前計算ノルムを持つ。
- upsert/delete でアリーナ内の旧スロットは「死んだ」状態になり、`compact()` または次回オープン時に回収される。

### 9.2 ディスク I/O と耐久性

- ペイロードはインメモリに常駐させず、`get()` 時にファイルから遅延ロードする（メモリ節約）。`:memory:` では全てメモリ保持。
- 書き込みはファイル末尾追記。`SyncMode`:
    - `Always`: 各 `put`/`delete` ごとに fsync（最も安全・最も低速）。
    - `OnFlush`（既定）: `flush()`/`close`/`compact` 時に fsync。
    - `Never`: 明示 fsync なし（OS 任せ）。
- `OnFlush`/`Never` でクラッシュしても、ファイル自体は torn write 回復（§5.3）で一貫状態に戻る。失われ得るのは直近の未 fsync
  追記のみ。

---

## 10. `:memory:` モード

- パスに `:memory:` を指定するとファイルを生成せず、すべてプロセスメモリ上で動作する。
- ファイルロックは行わない（プロセス内のスレッド協調のみ）。
- 永続化・`flush`・`compact` のディスク操作は no-op 相当（`compact` はインメモリ再構築のみ）。ハンドル破棄でデータは消える。

---

## 11. エラー処理

`EmveError`（非網羅）:

| バリアント                | 契機                          |
|----------------------|-----------------------------|
| `Io(std::io::Error)` | 下層 I/O                      |
| `Corrupt`            | 中間フレーム破損・ヘッダ CRC 不一致など回復不能  |
| `UnsupportedVersion` | `format_version` メジャー不一致    |
| `InvalidMagic`       | `magic` 不一致（EmveDB ファイルでない） |
| `DimensionMismatch`  | ベクトル/クエリ長が `dimension` と不一致 |
| `InvalidVector`      | NaN/Inf を含む                 |
| `PayloadTooLarge`    | `max_payload_len` 超過        |
| `Locked`             | ロック競合（`lock_wait=false`）    |
| `AlreadyExists`      | `create` で既存ファイル            |
| `ReadOnly`           | `ReadOnly` ハンドルへの書き込み操作     |

---

## 12. 制限・既定値

| 項目           | 値                                  |
|--------------|------------------------------------|
| 要素型          | `f32` 固定（v1）                       |
| dimension    | `1..=65535`（推奨）                    |
| 1 フレーム最大長    | `record_len` が `u32` のため約 4 GiB 未満 |
| payload 既定上限 | 16 MiB（`max_payload_len` で変更可）     |
| ファイル/オフセット   | 64bit（実質サイズ上限なし）                   |
| 既定 SyncMode  | `OnFlush`                          |
| 既定ロック競合挙動    | 即 `Locked`（`lock_wait=false`）      |

---

## 13. バージョニングと将来拡張

- 互換性はヘッダ `format_version`（メジャー）で管理。未知メジャーは拒否。
- 予約領域（`flags`、`reserved`、`element_type`、フレームの `flags`）により、後方互換な拡張余地を確保:
    - ANN インデックス（HNSW 等）の付帯セグメント／サイドカー。
    - f16/i8 量子化要素型。
    - 追加フレーム種別（例: メタ操作）。
- 既存フレーム種別 `1=Put` / `2=Delete` は不変とし、新種別は新しい `type` 値で追加する。
