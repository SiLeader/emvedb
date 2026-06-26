# EmveDB

EmveDB は Rust アプリケーションに組み込んで使う、単一ファイル完結の
ベクトルデータベースです。

固定次元の `f32` ベクトルと不透明なペイロード bytes を、1 つのローカル
DB ファイルに保存します。サーバプロセスや外部サービスは不要です。現在の検索は
ライブなレコードに対する厳密な全探索です。

## 特徴

- ベクトルとペイロードを単一ファイルに保存
- `EmveDb` を中心にした組み込み用 Rust API
- テストや一時インデックス向けの `:memory:` モード
- upsert、delete、get、厳密 top-k search、compaction に対応
- cosine similarity、L2 distance、dot product の 3 メトリック
- CRC32C 付きフレームによる append-only 書き込み
- read-only / read-write open のためのファイルロック
- ペイロードは不透明な bytes として扱うため、任意のシリアライズ済みメタデータを保存可能

## クイックスタート

このリポジトリを直接使う場合は、`Cargo.toml` に追加します。

```toml
[dependencies]
emvedb = { path = "." }
```

DB を作成し、ベクトルを追加して検索します。

```rust
use emvedb::{CreateOptions, EmveDb, Metric, SearchOptions};

fn main() -> emvedb::Result<()> {
    let db = EmveDb::create(
        "example.emve",
        &CreateOptions {
            dimension: 3,
            metric: Metric::Cosine,
            ..CreateOptions::default()
        },
    )?;

    db.put(1, &[1.0, 0.0, 0.0], br#"{"label":"red"}"#)?;
    db.put(2, &[0.0, 1.0, 0.0], br#"{"label":"green"}"#)?;
    db.put(3, &[0.8, 0.2, 0.0], br#"{"label":"orange"}"#)?;

    let results = db.search(&[1.0, 0.0, 0.0], 2, &SearchOptions::default())?;

    for item in results {
        println!(
            "id={} score={} distance={}",
            item.id, item.score, item.distance
        );
    }

    db.flush()?;
    Ok(())
}
```

テストや一時インデックスには `:memory:` を使えます。

```rust
use emvedb::{CreateOptions, EmveDb, Metric};

let db = EmveDb::create(
    ":memory:",
    &CreateOptions {
        dimension: 2,
        metric: Metric::Dot,
        ..CreateOptions::default()
    },
)?;
```

## データモデル

各レコードは次の要素を持ちます。

- `id`: ユーザーが指定する `u64` 主キー
- `vector`: DB の次元数と一致する固定長の `f32` slice
- `payload`: 任意の bytes。EmveDB は内容を解析せず、インデックスもしません

`put` は upsert です。同じ `id` に再度書き込むと、ライブな値が論理的に
置き換わります。`delete` はライブなレコードを論理削除します。古いバージョン
や tombstone は append-only ファイルに残り、`compact()` がライブなレコード
だけでストレージを再構築するまで保持されます。

ベクトル値は有限である必要があります。`NaN`、`+Inf`、`-Inf` は拒否されます。

## DB の作成とオープン

新しい DB には `create`、既存 DB には `open`、どちらでもよい場合は
`open_or_create` を使います。

```rust
use emvedb::{EmveDb, OpenMode, OpenOptions};

let db = EmveDb::open(
    "example.emve",
    &OpenOptions {
        mode: OpenMode::ReadOnly,
        ..OpenOptions::default()
    },
)?;
```

デフォルト値:

- `CreateOptions::default()` は `Metric::Cosine`、`SyncMode::OnFlush`、
  payload 上限 16 MiB を使います。有効な `dimension` は必ず指定してください。
- `OpenOptions::default()` は `ReadWrite` mode、ロック競合時に待たない設定、
  `SyncMode::OnFlush` を使います。

## 検索

検索はライブな全レコードに対する厳密 top-k search です。

```rust
use emvedb::SearchOptions;

let options = SearchOptions::default()
    .with_filter(|id| id != 42)
    .with_min_score(0.25);

let results = db.search(&query_vector, 10, &options)?;
```

結果は良い順に返ります。`SearchResultItem::score` は常に「大きいほど良い」
値です。`distance` はメトリックごとの値です。

| Metric | 並び順 | `score` | `distance` |
| --- | --- | --- | --- |
| `Metric::Cosine` | cosine similarity が高い順 | cosine similarity | `1 - cosine` |
| `Metric::L2` | distance が低い順 | negative distance | Euclidean distance |
| `Metric::Dot` | dot product が高い順 | dot product | dot product |

`with_filter` でスコア計算前に id で絞り込み、`with_result_filter` で計算済みの
検索結果を使って絞り込めます。

## API 概要

```rust
EmveDb::create(path, &create_options)?;
EmveDb::open(path, &open_options)?;
EmveDb::open_or_create(path, &create_options)?;

db.put(id, vector, payload)?;
db.delete(id)?;
db.get(id)?;
db.contains(id)?;
db.search(query, k, &search_options)?;
db.len()?;
db.is_empty()?;
db.flush()?;
db.compact()?;

db.dimension();
db.metric();
```

`get` は `Record` を返します。`record.id()`、`record.vector()`、
`record.payload()` で各フィールドにアクセスできます。

## 永続化とロック

ファイル backed な DB は、データを単一の `.emve` ファイルに保存します。
書き込みはファイル末尾への追記で行われ、`flush()` は storage backend に
データ同期を要求します。

`SyncMode` は同期動作を制御します。

- `Always`: 各 `put` / `delete` の後に sync
- `OnFlush`: `flush()` で sync。デフォルト値です
- `Never`: sync を OS に任せます

ファイルを開くとき、EmveDB は advisory file lock を使います。

- `ReadWrite` は排他ロックを取得します
- `ReadOnly` は共有ロックを取得します
- `lock_wait: false` では競合時にすぐ `EmveError::Locked` を返します
- `lock_wait: true` ではロック取得まで待ちます

## Compaction

append-only 書き込みのため、更新や削除によって古いデータがファイルに残ることがあります。
read-write DB で `compact()` を呼ぶと、ライブなレコードだけを使ってストレージを
再構築します。

```rust
db.compact()?;
db.flush()?;
```

`compact()` は書き込み操作なので、`ReadOnly` ハンドルでは使えません。

## 制限

- 要素型は現在 `f32` のみ
- DB の次元数は `1..=65535`
- payload のデフォルト上限は 1 レコードあたり 16 MiB
- 検索は厳密な全探索で、計算量は `O(number_of_records * dimension)`
- `:memory:` DB は `open` で再オープンできません。新しい in-memory DB を作成してください

## 開発

テストを実行します。

```sh
cargo test
```

format と lint を確認します。

```sh
cargo fmt --check
cargo clippy --workspace --all-targets --all-features
```

ファイルフォーマットと詳細設計は [SPEC.md](SPEC.md) を参照してください。

## License

Apache License, Version 2.0 で公開されています。詳細は [LICENSE](LICENSE) を参照してください。
