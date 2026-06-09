# M4: Db コア（open/create + 基本操作の配線）

## このマイルストーンでやること

ここまで作った部品を合体させ、**実際に使える DB の入り口**を作ります。

- ストレージ（M2）と索引（M3）を1つの `Db` 構造体にまとめる
- `create` / `open` / `open_or_create`（DBを作る・開く）
- `put`（追加/更新）/ `get`（取得）/ `delete`（削除）/ `flush`（確実保存）

検索（search）と compact は次の M5 / M6 でやるので、ここでは入れません。

## なぜ必要か

部品単体では「DB」になりません。「ファイルを開く → 索引を作る → 書く → 読む」という**一連の流れ（配線）**をここで通して、初めて
`let db = Db::create(...)？; db.put(...)?; db.get(...)?;` という普通の使い方ができるようになります。

## データの流れ（put したとき何が起きるか）

```
db.put(id, vector, payload)
   │
   ├─ 1. 入力チェック（dimension / NaN・Inf / payload上限）
   ├─ 2. Put フレームを encode（M1）
   ├─ 3. storage.append() で末尾に書く → 開始オフセットが返る（M2）
   ├─ 4. そのフレーム内の payload の絶対位置を計算
   ├─ 5. index.put(id, vector, payload_offset, payload_len)（M3）
   └─ 6. SyncMode が Always なら storage.sync()
```

## 出てくる言葉

| 言葉               | 意味                                               |
|------------------|--------------------------------------------------|
| upsert           | 「あれば更新、なければ追加」。`put` の挙動。                        |
| 論理削除             | 実データは消さず「消した印（トゥームストーン）」を追記する削除。                 |
| last-writer-wins | 同じ id は「最後に書いた人の勝ち」。追記オンリーなので自然にこうなる。            |
| 遅延ロード            | ペイロードを最初から全部メモリに載せず、`get()` で必要になった時にファイルから読むこと。 |
| ロックガード           | M2 で取ったファイルロックの持ち手。`Db` が持っている間ロックが効く。           |

## 対象モジュール

- `src/db.rs`（必要なら `src/lib.rs` の re-export も更新）

## 前提

- M2, M3 完了

## 仕様の要点（SPEC §3.1, §4, §9.2）

- `put` は upsert、`delete` は論理削除（トゥームストーン追記）、last-writer-wins。
- ベクトル長 ≠ dimension → `DimensionMismatch`、NaN/Inf → `InvalidVector`、payload 超過 → `PayloadTooLarge`。
- `SyncMode`: `Always`=毎回fsync / `OnFlush`=flush時 / `Never`=しない。
- 読み取り専用（`ReadOnly`）ハンドルへの書き込みは `ReadOnly` エラー。

---

## 手順（タスク）

### 1. `Db` の内部状態を設計

- [ ] `backend: Box<dyn Storage>` … ファイル版かメモリ版（トレイトで抽象化済み）
- [ ] `index: InMemoryIndex`
- [ ] ヘッダ由来の情報: `metric`, `dimension`, `generation`, `max_payload_len`, `sync`, `mode`
- [ ] ロックガード（RW のときだけ保持）
- [ ] 並行性のラップ（`RwLock`）は M7 で入れる。**今は単純な所有でOK**だが、M7 で内部可変にすることを頭の片隅に置いておく

### 2. `Db::create(path, CreateOptions)`

- [ ] `dimension` の範囲チェック（`1..=65535`、§12）、`:memory:` かどうかで分岐
- [ ] ファイル版: 既に存在したら `AlreadyExists`、ヘッダを書き、排他ロックを取る
- [ ] 空の index を初期化

### 3. `Db::open(path, OpenOptions)`

- [ ] ロック取得（RW=排他 / RO=共有、`lock_wait` に従う）
- [ ] ヘッダを読んで検証 → `metric` / `dimension` を確定
- [ ] 残っている `.compact` 一時ファイルがあれば削除（§5.3）
- [ ] `scan_frames` で全フレームを読み、`index.build_from_frames` で索引を構築
    - torn write 回復は storage 側がやってくれている（RW なら既に truncate 済み）

### 4. `Db::open_or_create(path, CreateOptions)`

- [ ] ファイルがあれば `open`、なければ `create`（実装はこの2つを呼び分けるだけ）

### 5. 書き込み系

- [ ] `put(&self, id, vector, payload)`:
    - [ ] `ReadOnly` なら `ReadOnly` エラーで弾く
    - [ ] 入力検証: 長さ ≠ dimension → `DimensionMismatch` / NaN・Inf 混入 → `InvalidVector` / payload が
      `max_payload_len` 超え → `PayloadTooLarge`
    - [ ] Put フレームを encode → `append`（開始オフセット取得）→ index 更新（payload_offset = フレーム内 payload の**絶対
      **オフセット）
    - [ ] `SyncMode::Always` なら `sync`
- [ ] `delete(&self, id) -> bool`:
    - [ ] index に無ければ false（**推奨: その場合は何も書かない**。存在する時だけトゥームストーンを追記）
    - [ ] 存在すれば Delete フレームを append → `index.delete` → `Always` なら sync

### 6. 読み取り系

- [ ] `get(&self, id) -> Option<Record>`:
    - [ ] index から `Entry` を引く → ベクトルはアリーナからコピー、payload は `read_at(payload_offset, payload_len)`
      で遅延ロード
    - [ ] `Record { id, vector: Vec<f32>, payload: Vec<u8> }` を返す
- [ ] `contains` / `len` / `is_empty` / `metric` / `dimension`

### 7. その他

- [ ] `flush(&self)`: backend を sync（§4: SyncMode に関係なく強制的に fsync する）
- [ ] `Drop`: RW なら flush してからロック解放（flush 失敗時にどう扱うか＝ログに出す/握り潰すを決めておく）

---

## 動作確認（受け入れ条件）

- [ ] create → put 数件 → get で**同じ vector / payload** が返る。
- [ ] 同じ id を put で更新 → get が新しい値、`len` は変わらない。
- [ ] delete → get が None、`len` が減る。ファイル版は**再 open してもライブ集合が一致**する。
- [ ] `ReadOnly` で open して put/delete すると `ReadOnly` エラー。
- [ ] 不正入力（dim 不一致 / NaN / payload 超過）がそれぞれ正しいエラーになる。
- [ ] `Always` でも `OnFlush` でも、途中でプロセスを kill した後の再 open が一貫している（本格テストは M8）。

## つまずきポイント / 実装メモ

- **payload_offset の計算がハマりやすい**。`append` が返す「Put フレーム先頭のオフセット」に、「フレーム先頭から payload
  までの相対距離」を足すと絶対位置になる。この相対距離を計算するヘルパを `frame.rs` に置くと安全（
  `record_len(4) + type(1) + flags(1) + id(8) + payload_len(4) + vector(4*dim)` の合計）。
- `:memory:` でも**まったく同じ経路**を通す（`read_at` がメモリ版から返ってくるだけ）。分岐を増やさないのがコツ。
- delete で「存在しない id には何も書かない」設計にすると、追記オンリーでも冪等で無駄が減る（推奨）。SPEC §3.1 の「存在しなければ
  no-op」とも一致。
