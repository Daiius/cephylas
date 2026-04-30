# TASKS

## 現状（2026-04-30 時点）

- main ブランチ最新は `3c55973` (fix-nextjs-dvh マージ済み)。作業ブランチ未作成。
- コードレビュー＋本番画面確認済み（playwright で `https://cephylas.faveo-systema.net` を確認）。
  - スクショ: `.playwright-mcp/cephylas-current.png`
  - 凡例が IO/Net で爆発しており、グラフの縦領域を圧迫しているのが視認できる。
- 実装変更は **未着手**。これから手をつける。

## 計画（3 段の PR に分割）

### PR 1: 凡例サイドバー化（優先・ここから着手）

**ゴール**: Chart.js デフォルト凡例（`plugins.legend.display = false`）を消し、サイドバー（PC）/ ドロワー（モバイル）でコンテナ単位のチェックボックス UI を提供する。4 チャート横断で選択状態を共有。

実装方針:
- 状態は **URL search params** (`?containers=a,b,c`) に保持。RSC 互換 / 共有可 / リロード耐性 / 後の cacheComponents の cache key になる。
- サイドバーは Client Component。`useRouter().replace(url, { scroll: false })` で URL 更新 → 該当 RSC が再生成。
- コンテナ名一覧は `fetchContainers()` を SSR で取得しサイドバーに props で渡す。
- 色割り当ては `containerName` のハッシュ → palette index でクライアント側に固定化（IO/Net は同色で borderDash で read/write を区別）。
- IO/Net の凡例は「コンテナ単位 1 行」に圧縮（read/write 別行をやめる）。
- レイアウト: `app/layout.tsx` を `flex` 2 ペインに変更。モバイルは `<details>` か `@headlessui/react` の `Disclosure`（依存関係に既にある）。

触るファイル:
- `next/src/app/page.tsx` — サイドバー差し込み
- `next/src/app/layout.tsx` — 2 ペイン化
- `next/src/components/ContainerFilter.tsx`（新規）
- `next/src/components/{Cpu,Memory,Io,Net}Chart.tsx` — `searchParams` 受け取り、フィルタ
- `next/src/components/Chart.tsx` — `plugins.legend.display = false`、useEffect の `mounted` ガード削除

### PR 2: 並列化 + Rust スレッド化

**ゴール**: 4 × N の直列フェッチを並列化し、ボトルネックになる Rust 側もマルチスレッド受付に。

実装方針:
- `fetchContainers()` を `react.cache()` でメモ化（リクエスト単位）。
- 各 Chart の `for ... await` を `Promise.all` に。
- もしくは `core` 側に `/containers/all/cpu` 系の bulk endpoint を追加（より理想的）。
- `core/src/server.rs:378` の `for stream in listener.incoming()` を `std::thread::spawn` でハンドラを別スレッドへ。簡易 thread pool でも可（外部 crate 制約あり）。
- 並列度に応じて `RwLock` の read 競合が増えるが、read 中心なので問題ないはず。

### PR 3: cacheComponents 移行

**ゴール**: `dynamic = 'force-dynamic'` を撤廃し、Next.js 16 の `'use cache'` + `cacheLife({ revalidate: 10 })` で適切にキャッシュ。

実装方針:
- `next/next.config.ts` に `experimental.cacheComponents: true` を追加。
- `lib/fetchers.ts` の各 fetcher に `'use cache'` ディレクティブを追加。
- `cacheLife({ revalidate: 10 })` — log 投入間隔と一致。
- `cacheTag('containers')` / `cacheTag(\`cpu:${name}\`)` で revalidation key を整理。
- `app/page.tsx` の `dynamic = 'force-dynamic'` を削除。
- `<Suspense fallback={<ChartSkeleton />}>` は維持（ストリーミング SSR）。
- 静的シェル（Header, Sidebar 骨格）は static rendering で即時返却される。

## 着手の指針

PR 1 から。視覚的改善が最大、ユーザがすぐ恩恵を感じられる。PR 2/3 はその後。

## 直近の作業

- ブランチ作成: `git switch -c feat/legend-sidebar`
- 作業はこの main worktree で実施し、コミット → push → PR の流れ。
- 別タスクと並列で進めたくなったら `claude --worktree <branch-name>` で別 worktree を切る。

## レビュー時に発見した小バグ（PR で巻き取るか別 PR にするか要判断）

- `Memory/Io/NetChart.tsx` のエラー時 fallback がすべて「CPU使用率取得中...」になっている → 各々 "Memory取得中" / "IO取得中" / "Net取得中" に修正
- `Chart.tsx` の `datasets: any` 型 → 適切な union 型に
- `IoChart.tsx` / `NetChart.tsx` で `borderColors` 配列が両ファイルにコピペ → `lib/colors.ts` に寄せる（PR 1 で色割り当てロジック作るならそこで自然に統合）
