# M0: プロジェクトの土台を作る

## このマイルストーンでやること

実装を始める前の「下ごしらえ」です。使う外部ライブラリを決め、空のソースファイルを並べ、**全モジュールで共通して使う型**
（エラー・メトリック・設定）を用意します。
この段階ではロジックはほぼ書きません。「箱だけ先に作る」フェーズです。

## なぜ必要か

- エラー型（`EmveError`）や設定型（`CreateOptions` など）は、この後のすべてのコードが使います。先に決めておかないと、後から全部書き直しになります。
- 空でもいいので全ファイルと `mod` 宣言を置いておくと、以降「ファイルを足す」ではなく「中身を書く」だけで済み、迷子になりません。

## 出てくる言葉

| 言葉             | 意味                                                 |
|----------------|----------------------------------------------------|
| クレート (crate)   | Rust のパッケージ／ライブラリの単位。この `emvedb` 全体が1クレート。         |
| 依存クレート         | 自分のコードが使う外部ライブラリ。`Cargo.toml` に書く。                 |
| トレイト (trait)   | 「この機能を持っている」という約束（他言語の interface に近い）。             |
| enum           | 「いくつかの種類のどれか1つ」を表す型。`Metric` は Cosine/L2/Dot のどれか。 |
| `Result<T, E>` | 「成功（値 T）か失敗（エラー E）か」を表す Rust の基本型。                 |

## 対象モジュール

- `Cargo.toml`
- `src/lib.rs`, `src/error.rs`, `src/metric.rs`, `src/options.rs`

## 前提

- なし（一番最初に着手）

## 手順（タスク）

### 1. 使うライブラリを `Cargo.toml` に追加

それぞれ「何のために使うか」とセットで覚えると良いです。

- [ ] `crc32c` … データ化け検出の計算（CRC32C）。自前で書くのは大変なので借りる。SPEC §5。
- [ ] `fs4` … OS をまたいで使えるファイルロック。SPEC §8.1。
- [ ] `thiserror` … エラー型 `EmveError` の定義をラクにするマクロ。手書きでも書けるが面倒なので使う。
- [ ] dev-dependency（テスト時だけ使う）:
    - [ ] `tempfile` … テストで一時的な DB ファイルを作る
    - [ ] `proptest` … ランダム入力を大量に試す「プロパティテスト」
    - [ ] `criterion` … 速度を測るベンチマーク（任意・後回し可）

> 💡 すでに `Cargo.toml` に入っているものは確認だけでOK。

### 2. 空のモジュール骨格を作る

- [ ] README の「モジュール構成」に従って空ファイルを作り、`mod xxx;` 宣言を書く
    - 例: `src/format/mod.rs` に `pub(crate) mod header;` と `pub(crate) mod frame;`、`src/lib.rs` に `mod format;` など
    - 中身は空でもコンパイルが通ればOK

### 3. `error.rs`：エラーの種類を全部用意（SPEC §11）

- [ ] `EmveError` という enum を作り、起こりうる失敗を列挙する。`thiserror` を使うと各バリアントにメッセージを付けられる。
    - [ ] `Io(std::io::Error)` … ファイル読み書きの失敗
    - [ ] `Corrupt` … データが壊れていて回復不能
    - [ ] `UnsupportedVersion` … 知らないフォーマットバージョン
    - [ ] `InvalidMagic` … そもそも EmveDB のファイルではない
    - [ ] `DimensionMismatch` … ベクトルの長さが dimension と違う
    - [ ] `InvalidVector` … NaN/Inf（無限大）が混ざっている
    - [ ] `PayloadTooLarge` … ペイロードが上限超え
    - [ ] `Locked` … 他が使用中でロックが取れない
    - [ ] `AlreadyExists` … `create` したらファイルが既にあった
    - [ ] `ReadOnly` … 読み取り専用なのに書こうとした
- [ ] `pub type Result<T> = std::result::Result<T, EmveError>;` を定義（毎回 `EmveError` と書かずに済む）
- [ ] `From<std::io::Error> for EmveError` を実装
    - これがあると、I/O 関数の `?` 演算子で自動的に `Io(...)` に変換される。`thiserror` の `#[from]` 属性で自動生成できる。

### 4. `metric.rs`：メトリックの“型だけ”用意（中身は M5 で実装）

- [ ] `pub enum Metric { Cosine, L2, Dot }`
- [ ] `to_u8()` / `from_u8()` … ヘッダに保存する数値表現の変換。`0=Cosine, 1=L2, 2=Dot`（SPEC §5.1）
    - 例: `Metric::L2.to_u8() == 1`、`Metric::from_u8(2) == Some(Metric::Dot)`
- [ ] 距離・スコア計算関数は**シグネチャ（型）だけ**書いて `todo!()` で中身は空に。実装は M5。

### 5. `options.rs`：設定の型を用意（SPEC §4, §12）

- [ ] `pub enum OpenMode { ReadOnly, ReadWrite }` … 読み取り専用か読み書きか
- [ ] `pub enum SyncMode { Always, OnFlush, Never }`
    - 意味: `Always`=毎回ディスクに確実書き込み（安全だが遅い）/ `OnFlush`=`flush()`時だけ / `Never`=OS任せ
    - [ ] `Default` を `OnFlush` にする
- [ ] `CreateOptions { dimension: u32, metric: Metric, sync: SyncMode, max_payload_len: usize }`
    - [ ] `Default` で `max_payload_len` を 16 MiB（`16 * 1024 * 1024`）にする
- [ ] `OpenOptions { mode: OpenMode, lock_wait: bool, sync: SyncMode }`
    - [ ] `lock_wait` の既定は `false`（ロックが取れなければ待たずに即エラー）

### 6. `lib.rs`：公開する型をまとめる

- [ ] クレートの説明コメント（`//! EmveDB は ...`）を書く
- [ ] 外から使う型を `pub use` で re-export
    - 目標: 利用者が `emvedb::{Metric, OpenMode, SyncMode, CreateOptions, OpenOptions, EmveError}` で参照できる

### 7. ビルドが通ることを確認

- [ ] `cargo build` / `cargo clippy -- -D warnings` / `cargo fmt --check` が全部緑

## 動作確認（受け入れ条件）

- 中身が空（`todo!()`）でも**コンパイルが通る**。
- 別の場所から `use emvedb::{Metric, OpenMode, SyncMode, CreateOptions, OpenOptions, EmveError};` がエラーなく書ける。

## つまずきポイント / 実装メモ

- `edition` は 2024（既存 `Cargo.toml` のまま）。最低 Rust バージョン（MSRV）を決めるなら README に書く。
- `EmveError` は `Send + Sync + 'static` である必要がある（後の M7 で `Db` を複数スレッドから使うため）。`thiserror`
  を使えば自然に満たせる。
- ライブラリ選定はあくまで提案。`fs4` の代わりに `fs2`、`crc32c` の代わりに自前テーブルでも動く。決める前に「そのクレートが今もメンテされているか」を確認すると安心。
