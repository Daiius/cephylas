# Cephylas

Docker container のリソース使用率を記録・可視化する Web アプリ。

- **core/** — Rust 製ロガー兼 API サーバ。`/var/run/docker.sock` から `/containers/{id}/stats` を 10 秒ごとに取得 → ファイルログ＋メモリキャッシュへ記録。`:7878` で REST を返す。
- **next/** — Next.js 16 / React 19 のフロント。Server Component から `core` API を叩いて Chart.js で描画。クライアントは `core` に直接アクセスしない。
- **nginx.conf** — `/` → `cephylas-nextjs:3000`、`/api/` → `cephylas:7878`。本番のみ前段に立つ。
- **本番**: https://cephylas.faveo-systema.net （`docker-compose.prod.yml`）

## 維持すべき設計判断

`README.md` / `core/README.md` にもあるが要点:

1. **クライアントは Next.js のみと通信**。`core` は internal network からのみアクセス可能。`API_URL=http://cephylas:7878` (compose 内 DNS)。
2. **疑似 LTTB ダウンサンプリングは Rust 側で実施**（`core/src/log_cache.rs` の `UsageCacheMap::downsample`）。デフォルト `nsample=512`、log は最大 `MAX_LOG_LENGTH=8640` 件（24 時間 × 10 秒間隔）。
3. **`core` は外部 crate 最小**（`json` のみ）。HTTP サーバも手書き（`core/src/server.rs`）。memory 50MB 制限。
4. **データ点数の感覚維持**: LTTB は「人間が見て情報が落ちた感じが少ない」点群を選ぶアルゴリズム。素朴な等間隔間引きにしない。

## 開発コマンド

```bash
# dev (compose watch なし、bind mount で hot reload)
docker compose up

# ログサンプル: log/log_daily に入れておくと起動時に読み込む
# Rust 側: cargo run が CMD（core/Dockerfile）
# Next 側: pnpm run dev が CMD（next/Dockerfile）

# 本番イメージ
docker compose -f docker-compose.yml -f docker-compose.prod.yml up
```

## 既知の落とし穴・気をつける点

- **`next/src/app/page.tsx:4` の `dynamic = 'force-dynamic'`**: Next.js キャッシュを完全に無効化している。cacheComponents 移行時はここを外す。
- **`fetchContainers()` が 4 チャートで個別に呼ばれている**（`CpuChart.tsx:18` など）。`core` の単一スレッドサーバには負荷。`react.cache()` か並列化したい。
- **N+1 直列フェッチ**: `for (const c of containers) { await fetchXxx(c) }` が CPU/Mem/IO/Net 全部にある。`Promise.all` 化推奨。
- **`Chart.tsx` の `mounted` 二段レンダ**（`useState(false)` → effect で `setMounted(true)` → 再 effect）は古いパターン。React 19 では不要。
- **エラー時 UI のコピペバグ**: `MemoryChart.tsx`, `IoChart.tsx`, `NetChart.tsx` の失敗時 fallback がすべて「CPU使用率取得中...」のまま。
- **`core/src/server.rs:378`** の TCP listener はメインスレッドで逐次処理。並列フェッチ化するならここをスレッド化必須。
- **`limited_convert_time_string_to_f32`**（`server.rs:135`）は日付を捨てて時刻だけを秒に変換している。日次ローテ前提。日跨ぎでバグる可能性あり。
- **`core/src/log.rs:380`** の Docker API レスポンスで `time == "0001-01-01T00:00:00Z"` のケースが実在する。`break` で無視している。

## Git worktree 運用メモ

- このリポジトリは bare clone (`/Users/daiji/sources/cephylas/.bare`) を中心に worktree を切る運用。
- `claude --worktree <name>` で worktree 作成 + 新セッション起動が可能。Claude Code のセッションは cwd ごとに分離される（`~/.claude/projects/-Users-daiji-sources-cephylas-<branch>/`）。
- ただし auto-memory は `.bare` 配下に集約されるため、worktree 間で共有される。
- worktree を跨いで context を引き継ぎたい場合は **`TASKS.md` 経由が推奨**。セッション直接共有はしない（cwd が違うため別セッション扱い）。
- `.env.development` は post-checkout hook で worktree に自動コピーされる（コメントに記載あり）。
