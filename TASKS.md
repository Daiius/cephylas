# TASKS

## 現状（2026-05-02 時点）

`feat/v2-web` は **PR #40 で main に merged 済**、**Vercel 本番反映済** (`cephylas.faveo-systema.net`)。次フェーズは `feat/v2-core` で core (Rust) 側の改修。

### web (v2) で到達した形

- 凡例 + フィルタは **mini-legend (`MiniLegend.tsx`) に統合**。サイドバー / drawer / ハンバーガーは廃止
- mini-legend の操作: シングルクリックで toggle (250ms 遅延発火)、ダブルクリックで isolate / restore
- 色は Tableau 10 を **alphabetical index** で割当 (先頭 = blue、N <= 10 で衝突なし)
- Chart.js: `mode: 'nearest'` + `intersect: false` + `hitRadius: 10` で当たり判定広め、tooltip は 1 点
- フィルタ状態は **react state のみ** で管理 (URL 永続化なし)。リロードで全表示にリセット
- **cacheComponents 有効**: 各 chart server component を `'use cache'` 化 (`cacheTag('chart:cpu')` 等)、`fetchContainers` のみ fetcher 側でも cache。page は Partial Prerender (`◐ /`)
- 各 chart 内の N+1 は **`Promise.all` で並列化済** (core 側がスレッド化されれば自動的に効果が出る)
- 開発は `pnpm dev` 一発 (core を docker、web を host で並列起動)
- 本番は Vercel + 自前 VPS API (`api.faveo-systema.net/cephylas`) 構成。Vercel 側で `x-vercel-cache: HIT` / `x-nextjs-prerender: 1` を確認済

## 次にやる: core 側の TCP listener スレッド化 (`feat/v2-core`)

**ゴール**: 単一スレッド逐次処理になっている `core/src/server.rs` をマルチスレッド受付化し、web 側で既に発射されている並列フェッチを実効化する。

### 現在のボトルネック

- web 側は `Promise.all` で 4 chart 内の N+1 を並列化済 (`feat(cache): cacheComponents 移行` と `perf(charts): Promise.all 化` の 2 commit)
- `fetchContainers` だけ fetcher 側で `'use cache'` & `cacheTag('containers')` のため 4 chart で共有 (request 内 dedup + 10s revalidate)
- しかし `core/src/server.rs` の `for stream in listener.incoming()` がメインスレッドで逐次処理 → HTTP 並列接続が受付段階で serialize される

### 実装方針

- A 案 (素直): `for stream in listener.incoming()` のハンドラを `std::thread::spawn` で別スレッドへ。簡易 thread pool 化も可
- B 案 (より理想): core に `/containers/all/cpu` 系の **bulk endpoint** を追加。1 リクエストで全コンテナ分の cpu 配列を返す。要求発射数が `4 + 4*N` から `4` に落ちる
- 並列度に応じて `RwLock` の read 競合が増えるが、read 中心なので問題ないはず
- core は外部 crate 最小 (`json` のみ)、HTTP も手書きの方針を維持。`std::thread::spawn` レベルで足りるはず
- メモリ 50MB 制限は維持

### 留意点

- web 側の API 呼び方を変えるなら `next/src/lib/fetchers.ts` と各 `XxxChart.tsx` の修正も必要。bulk endpoint 採用時は web 側コミットも併せて
- `limited_convert_time_string_to_f32`（`server.rs:135`）は日付を捨てて時刻だけを秒に変換しているので、スレッド化と独立に日跨ぎバグの整理候補

## 残課題 (優先度低)

- **`nginx.conf` の整理**: `/` ルート (Next.js プロキシ) は Vercel hosting に移行したので未使用。`/api/` も使わない設計 → `/cephylas/*` で core にルーティングする形に書き換える整理が必要 (本番側の都合に合わせて)
- **`Chart.tsx` の最後の `datasets` 依存**: `datasets` を依存配列に入れているため毎回 destroy/create される。new array で来るので本来は安定化したいが PR で動作優先で許容中 (`Chart.tsx:88-92` の eslint-disable コメント参照)
