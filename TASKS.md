# TASKS

## 現状（2026-05-02 時点 / branch `feat/v2-web`）

`feat/v2-web` で web 側の v2 化を一気通貫で実施中。現時点のコミット (新しい順):

1. `refactor(filter): hidden の URL 永続化を撤去、純粋に react state のみで管理`
2. `feat(cache): cacheComponents 移行 / 各 chart を 'use cache' 化`
3. `chore(deps): Next.js を 16.1.6 → 16.2.4 へアップデート`
4. `feat(legend): mini-legend のダブルクリック/タップで isolate / restore に対応`
5. `chore(deps): mini-legend 一本化で未使用になった @headlessui/react と @heroicons/react を削除`
6. `refactor: サイドバー / drawer / ハンバーガーを撤廃、mini-legend に一本化`
7. `feat(legend): mini-legend をクリック/タップで表示/非表示切替可能に`
8. `feat(chart): tooltip の当たり判定を拡張`
9. `fix(colors): Tableau 10 を正典順に並べ替え + 色割り当てを index-based に変更`
10. `feat: 各チャートに inline mini-legend を表示`
11. `refactor: Next.js を docker から外し host 起動 + prod 構成を Vercel 前提に整理`
12. `chore: web パッケージを 1.0.0 へ bump`
13. `refactor(types): Chart.js を declaration merging で拡張し as キャストを撤去`
14. `refactor: pnpm workspace + Tailwind 4 / daisyUI 5 + compose watch`
15. `feat: チャートカラーを Tableau 10 パレットに変更`
16. `docs: Claude Code 用ドキュメントと dev override 用 gitignore 追加`
17. `feat: コンテナ表示フィルタのサイドバー化`

到達した形:
- 凡例 + フィルタは **mini-legend (`MiniLegend.tsx`) に統合**。サイドバー / drawer / ハンバーガーは廃止
- mini-legend の操作: シングルクリックで toggle (250ms 遅延発火)、ダブルクリックで isolate / restore
- 色は Tableau 10 を **alphabetical index** で割当 (先頭 = blue、N <= 10 で衝突なし)
- Chart.js: `mode: 'nearest'` + `intersect: false` + `hitRadius: 10` で当たり判定広め、tooltip は 1 点
- フィルタ状態は **react state のみ** で管理 (URL 永続化なし)。リロードで全表示にリセット
- **cacheComponents 有効**: 各 chart server component を `'use cache'` 化 (`cacheTag('chart:cpu')` 等)、`fetchContainers` のみ fetcher 側でも cache。page は Partial Prerender (`◐ /`)
- 開発は `pnpm dev` 一発 (core を docker、web を host で並列起動)
- 本番は Vercel + 自前 VPS API (`api.faveo-systema.net/cephylas`) 構成

## 次にやる候補

### 1. PR 2: 並列化 + Rust スレッド化

**ゴール**: 4 × N の直列フェッチを並列化、ボトルネックの Rust 側もマルチスレッド受付に。`feat/v2-core` ブランチ予定。

実装方針:
- `fetchContainers()` を `react.cache()` でメモ化（リクエスト単位）
- 各 Chart の `for ... await` を `Promise.all` に
- もしくは `core` 側に `/containers/all/cpu` 系の bulk endpoint を追加（より理想的）
- `core/src/server.rs` の `for stream in listener.incoming()` を `std::thread::spawn` でハンドラを別スレッドへ。簡易 thread pool でも可
- 並列度に応じて `RwLock` の read 競合が増えるが、read 中心なので問題ないはず

### 2. prod 移行

`feat/v2-web` で `next/Dockerfile.prod` を削除済 (Vercel が build するので)。Vercel 側に必要な調整:
- Vercel project の Root Directory を `next` に設定 (monorepo 構造のため)
- Vercel の Env Vars で `API_URL=https://api.faveo-systema.net/cephylas` (or 想定 URL) を設定
- cacheComponents が有効なので、Vercel の Data Cache が自然に効く想定 (10s revalidate)

## 直近の小バグ・改善

- `Chart.tsx` の `datasets: any` 型は `AppDataset` でほぼ解決、残りも `TimedPoint` / `TimedSeries` で型付き
- `nginx.conf` の `/` ルート (Next.js プロキシ) は Vercel hosting に移行したので未使用。`/api/` も使わない設計 → `/cephylas/*` で core にルーティングする形に書き換える整理が必要 (本番側の都合に合わせて)
