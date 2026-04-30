# TASKS

## 現状（2026-05-01 時点 / branch `feat/v2-web`）

`feat/v2-web` で web 側の v2 化を一気通貫で実施中。現時点のコミット (新しい順):

1. `refactor: サイドバー / drawer / ハンバーガーを撤廃、mini-legend に一本化`
2. `feat(legend): mini-legend をクリック/タップで表示/非表示切替可能に`
3. `feat(chart): tooltip の当たり判定を拡張`
4. `fix(colors): Tableau 10 を正典順に並べ替え + 色割り当てを index-based に変更`
5. `feat: 各チャートに inline mini-legend を表示`
6. `refactor: Next.js を docker から外し host 起動 + prod 構成を Vercel 前提に整理`
7. `chore: web パッケージを 1.0.0 へ bump`
8. `refactor(types): Chart.js を declaration merging で拡張し as キャストを撤去`
9. `refactor: pnpm workspace + Tailwind 4 / daisyUI 5 + compose watch`
10. `feat: チャートカラーを Tableau 10 パレットに変更`
11. `docs: Claude Code 用ドキュメントと dev override 用 gitignore 追加`
12. `feat: コンテナ表示フィルタのサイドバー化`

到達した形:
- 凡例 + フィルタは **mini-legend (`MiniLegend.tsx`) に統合**。サイドバー / drawer / ハンバーガーは廃止
- 色は Tableau 10 を **alphabetical index** で割当 (先頭 = blue、N <= 10 で衝突なし)
- Chart.js: `mode: 'nearest'` + `intersect: false` + `hitRadius: 10` で当たり判定広め、tooltip は 1 点
- フィルタ状態は URL `?hidden=a,b,c` で永続化、`history.replaceState` で navigation を回避
- 開発は `pnpm dev` 一発 (core を docker、web を host で並列起動)
- 本番は Vercel + 自前 VPS API (`api.faveo-systema.net/cephylas`) 構成

## 次にやる候補

### 1. mini-legend のダブルクリック/タップで isolate / restore (検討中)

「複数表示中にダブル → そのチップだけ表示 (他全部 hidden)」「単独表示中にダブル → 全部表示 (hidden を空に)」という Grafana / Tableau / Excel 的な定番パターン。

**実装方針 (本人と合意済み)**:
- pointerup のタイムスタンプ比較で 250-300ms 以内の 2 連打を double と判定 (mouse / touch 統一)
- シングルクリック動作 (`toggle`) は double 検知のため少し遅延 (~250ms) させる
- `FilterContext` に `isolate(name, allNames)` を追加 (`setHidden(new Set(allNames.filter(n => n !== name)))`)
- 「自分以外全部 hidden」の状態判定は MiniLegend 側で `uniq` から計算

**ライブラリ選定 (これから決める)**:
- DIY (~20 行で書ける) — 依存ゼロ、この用途以外に汎用しないなら最善
- `use-double-tap` (https://www.npmjs.com/package/use-double-tap) — react hook、~1KB、pointer events で mouse/touch 統一、threshold 設定可。専用なので手戻りなし
- `@use-gesture/react` (旧 `react-use-gesture`) — gesture 全般 (drag, pinch, double-tap)。~10KB。将来 chart の pan/zoom や pinch-zoom 入れる予定があればこちらが効く
- `react-use` の `useDebounce` 等の汎用 hook 系 — 既存依存に react-use があるならアリだが現状なし

**おすすめ**: 当面 DIY、将来 chart に pan/zoom が欲しくなったら `@use-gesture/react` 採用検討

**留意点**:
- iOS Safari の double-tap zoom と競合しないよう `touch-action: manipulation` を当てる必要 (daisyUI の btn が暗黙に持つ可能性あり、要検証)
- キーボード操作で isolate はできない (双クリック相当のキー操作が標準化されていないため。Shift+Enter で代替する案もあり)

### 2. PR 2: 並列化 + Rust スレッド化

**ゴール**: 4 × N の直列フェッチを並列化、ボトルネックの Rust 側もマルチスレッド受付に。`feat/v2-core` ブランチ予定。

実装方針:
- `fetchContainers()` を `react.cache()` でメモ化（リクエスト単位）
- 各 Chart の `for ... await` を `Promise.all` に
- もしくは `core` 側に `/containers/all/cpu` 系の bulk endpoint を追加（より理想的）
- `core/src/server.rs` の `for stream in listener.incoming()` を `std::thread::spawn` でハンドラを別スレッドへ。簡易 thread pool でも可
- 並列度に応じて `RwLock` の read 競合が増えるが、read 中心なので問題ないはず

### 3. PR 3: cacheComponents 移行

**ゴール**: `dynamic = 'force-dynamic'` を撤廃し、`'use cache'` + `cacheLife({ revalidate: 10 })` でキャッシュ。

実装方針:
- `next.config.ts` に `experimental.cacheComponents: true` を追加
- `lib/fetchers.ts` の各 fetcher に `'use cache'` ディレクティブを追加
- `cacheLife({ revalidate: 10 })` — log 投入間隔と一致
- `cacheTag('containers')` / `cacheTag(\`cpu:${name}\`)` で revalidation key を整理
- `app/page.tsx` の `dynamic = 'force-dynamic'` を削除
- `<Suspense fallback={<ChartSkeleton />}>` は維持

### 4. prod 移行 (おそらく PR 3 と同時)

`feat/v2-web` で `next/Dockerfile.prod` を削除済 (Vercel が build するので)。Vercel 側に必要な調整:
- Vercel project の Root Directory を `next` に設定 (monorepo 構造のため)
- Vercel の Env Vars で `API_URL=https://api.faveo-systema.net/cephylas` (or 想定 URL) を設定

## 直近の小バグ・改善

- `next/package.json` に `@headlessui/react`, `@heroicons/react` が残っているが mini-legend 一本化で完全未使用 → 次のコミットで削除
- `Chart.tsx` の `datasets: any` 型は `AppDataset` でほぼ解決、残りも `TimedPoint` / `TimedSeries` で型付き
- `nginx.conf` の `/` ルート (Next.js プロキシ) は Vercel hosting に移行したので未使用。`/api/` も使わない設計 → `/cephylas/*` で core にルーティングする形に書き換える整理が必要 (本番側の都合に合わせて)
