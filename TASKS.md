# TASKS

## 現状（2026-05-01 時点 / branch `feat/legend-sidebar`）

PR 1 (凡例サイドバー化) を中心とした **複合変更が完了**。コミット履歴:

1. `feat: コンテナ表示フィルタのサイドバー化` — Chart.js デフォルト凡例を消し、サイドバー化。FilterContext / Sidebar 新設。URL `?hidden=` で永続化
2. `docs: Claude Code 用ドキュメントと dev override 用 gitignore 追加`
3. `feat: チャートカラーを Tableau 10 パレットに変更`
4. (これから) `refactor: pnpm workspace + Tailwind 4 / daisyUI 5 / compose watch` 系の大規模 modernization

未着手:
- PR 2 (並列化 + Rust スレッド化)
- PR 3 (cacheComponents 移行)
- prod の `next/Dockerfile.prod` 更新 (monorepo 構造に追従、outputFileTracingRoot 設定)

## 次にやる候補

### PR 2: 並列化 + Rust スレッド化

**ゴール**: 4 × N の直列フェッチを並列化、ボトルネックの Rust 側もマルチスレッド受付に。

実装方針:
- `fetchContainers()` を `react.cache()` でメモ化（リクエスト単位）。
- 各 Chart の `for ... await` を `Promise.all` に。
- もしくは `core` 側に `/containers/all/cpu` 系の bulk endpoint を追加（より理想的）。
- `core/src/server.rs:378` の `for stream in listener.incoming()` を `std::thread::spawn` でハンドラを別スレッドへ。簡易 thread pool でも可。
- 並列度に応じて `RwLock` の read 競合が増えるが、read 中心なので問題ないはず。

### PR 3: cacheComponents 移行

**ゴール**: `dynamic = 'force-dynamic'` を撤廃し、`'use cache'` + `cacheLife({ revalidate: 10 })` でキャッシュ。

実装方針:
- `next.config.ts` に `experimental.cacheComponents: true` を追加。
- `lib/fetchers.ts` の各 fetcher に `'use cache'` ディレクティブを追加。
- `cacheLife({ revalidate: 10 })` — log 投入間隔と一致。
- `cacheTag('containers')` / `cacheTag(\`cpu:${name}\`)` で revalidation key を整理。
- `app/page.tsx` の `dynamic = 'force-dynamic'` を削除。
- `<Suspense fallback={<ChartSkeleton />}>` は維持。

### prod 移行 (おそらく PR 3 と同時)

monorepo 化したので `next/Dockerfile.prod` が古い。現在 `compose.prod.yml` は事前ビルド済みイメージ前提なので、次に push する際は build context を root にして:

```dockerfile
FROM node:22-slim AS build
WORKDIR /workspace
RUN corepack enable && corepack prepare pnpm@10.33.2 --activate
COPY package.json pnpm-lock.yaml pnpm-workspace.yaml ./
COPY next/package.json ./next/
RUN pnpm install --frozen-lockfile
COPY next ./next
RUN pnpm --filter web build
# standalone output @ /workspace/next/.next/standalone (要 outputFileTracingRoot)
...
```

`next.config.ts` に `outputFileTracingRoot: path.resolve(__dirname, '..')` も必要になる。

## 直近の小バグ・改善

- `Chart.tsx` の `datasets: any` 型 → AppDataset で部分的に対応済み（残り型の精緻化）
- 凡例非表示で hover tooltip だけが手がかりになった。tooltip の表示順をデフォルトのまま放置 — 大量 dataset で見づらいかも
- `next/Dockerfile.prod` が flat monorepo 構造に未対応（上述）
- `next/package.json` に `@headlessui/react`, `@heroicons/react` が残っているが現状未使用 → 次の機会に削除
