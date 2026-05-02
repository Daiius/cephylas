# Cephylas

Docker container のリソース使用率を記録・可視化する Web アプリ。

- **core/** — Rust 製ロガー兼 API サーバ。`/var/run/docker.sock` から `/containers/{id}/stats` を 10 秒ごとに取得 → ファイルログ＋メモリキャッシュへ記録。`:7878` で REST を返す。
- **next/** — Next.js 16 / React 19 のフロント。Server Component から `core` API を叩いて Chart.js で描画。クライアントは `core` に直接アクセスしない。pnpm workspace の唯一の TS package（package name は `web`）。
- **nginx.conf** — 本番 VPS の `api.faveo-systema.net` 配下のルータ。ここで `/cephylas/*` を core にプロキシする想定。dev では使わない。

## プロダクション構成

- **frontend**: Vercel 上の `cephylas.faveo-systema.net` (Next.js)
- **API**: 自前の VPS の `api.faveo-systema.net/cephylas` (Rust core, nginx 越し)

dev では Next.js を host で直接動かす方針。docker は core (Rust) のみ。

## ディレクトリ構成

```
cephylas/
├── core/                 # Rust API + logger (workspace 外)
├── next/                 # Next.js (pnpm workspace の単一 TS package, name: "web")
├── log/                  # log_daily を置く / dev のサンプルもここ
├── nginx.conf            # 本番 VPS の nginx 設定 (dev では未使用)
├── compose.yml           # dev (core のみ起動)
├── compose.prod.yml      # prod (core + nginx)
├── compose.override.yml  # gitignored — 個人ローカルの dev 上書き (memory 制限緩和等)
├── package.json          # workspace root
├── pnpm-workspace.yaml   # packages: ["next"]
├── pnpm-lock.yaml        # workspace root の単一 lockfile
├── CLAUDE.md
└── TASKS.md
```

flat 構造（`packages/web/` ではなく `next/`）にしている理由: TS package が 1 個しかないので中間ディレクトリは冗長。複数になったら `packages/` へ昇格すれば良い。

## 維持すべき設計判断

`README.md` / `core/README.md` にもあるが要点:

1. **クライアントは Next.js のみと通信**。`core` は internal network からのみアクセス可能。
2. **疑似 LTTB ダウンサンプリングは Rust 側で実施**（`core/src/log_cache.rs` の `UsageCacheMap::downsample`）。デフォルト `nsample=512`、log は最大 `MAX_LOG_LENGTH=8640` 件（24 時間 × 10 秒間隔）。
3. **`core` は外部 crate 最小**（`json` のみ）。HTTP サーバも手書き（`core/src/server.rs`）。memory 50MB 制限。
4. **データ点数の感覚維持**: LTTB は「人間が見て情報が落ちた感じが少ない」点群を選ぶアルゴリズム。素朴な等間隔間引きにしない。
5. **chart.js が dark theme を持たない** ため、daisyUI も `light` のみで運用（`globals.css` の `@plugin "daisyui"` で `themes: light --default`、`<html data-theme="light">`）。

## 開発コマンド

```bash
pnpm dev         # core を docker 起動 + Next.js を host で起動
pnpm dev:core    # core だけ起動
pnpm dev:web     # Next.js だけ起動 (host)
pnpm down        # docker compose down

pnpm typecheck   # 全 workspace の tsc --noEmit
pnpm build       # 全 workspace の build
```

`pnpm dev` で起動後、http://localhost:3000 で確認。core API は http://localhost:7878。

`API_URL` env var が無い場合は `http://localhost:7878` をフォールバックにする (`fetchers.ts`)。Vercel 本番デプロイ時は `API_URL=https://api.faveo-systema.net/cephylas` 等を Vercel 側で設定。

### 個人 override

`compose.override.yml`（gitignored）で開発 PC ごとの調整を入れる。よくあるやつ:
```yaml
services:
  cephylas:
    deploy:
      resources:
        limits:
          memory: 2G   # 本番 50M だと cargo build が SIGKILL されるので緩和
```

## スタック詳細

- Tailwind CSS 4 + daisyUI 5（CSS-based config @ `globals.css`）
- Next.js 16 + React 19 + Turbopack
- Chart.js 4 + chartjs-adapter-luxon
- Chart.js 型は `next/src/types/chartjs.d.ts` で declaration merging 拡張（`containerName` を dataset に持たせる）

## UI 構成

- **`Header.tsx`**: cephylas アイコン中央寄せのみ。サイドバー / ハンバーガーは廃止 (mini-legend が凡例兼フィルタを兼ねるため不要に)。
- **`MiniLegend.tsx`**: 各チャート上端に表示する**凡例兼フィルタ**。色ドット + コンテナ名のチップを `<button aria-pressed>` で並べる。
  - シングルクリック/タップ: `FilterContext.toggle` を呼んで表示/非表示切替。double 検知のため 250ms 遅延発火
  - ダブルクリック/タップ: 通常状態 → そのチップだけ表示 (`isolate`)、単独表示中 → 全表示に戻す (`clear`)。同チップへの 250ms 以内 2 連打を `pendingRef` のタイムスタンプ比較で判定
  - フィルタ状態は **react state のみ** で管理 (URL 永続化なし)。リロードすると全表示にリセットされる
  - iOS の double-tap zoom 抑止のため `touch-manipulation` クラスを付与
- **`Chart.tsx`**: Chart.js のデフォルト凡例 (`plugins.legend.display`) は無効、tooltip も `interaction.mode = 'nearest'` + `intersect: false` + `hitRadius: 10` でホバー位置に最も近い 1 点だけ表示する。タイトルは Chart.js ではなく HTML `<h3>` 側で出してミニ凡例の上に置く。
- **色割り当て**: `borderColorFor(index)` でアルファベット順 index に Tableau 10 を順に当てる。N <= 10 なら衝突なし、先頭 = blue。コンテナ追加で alphabetical 位置が変わると色がシフトする点は妥協。

## FilterContext API

`next/src/components/FilterContext.tsx` の Provider が表示/非表示状態の単一の真実 (`hidden: ReadonlySet<string>`) を持つ。永続化はせず、リロードで全表示にリセットされる。

```ts
const { hidden, toggle, setAll, isolate, clear } = useFilter();
toggle(name);                   // 1 件 on/off
setAll(['a','b']);              // hidden を ['a','b'] に置換 (= a,b を非表示)
isolate(name, allNames);        // name 以外を hidden (= name だけ表示)
clear();                        // hidden を空に (= 全表示)
```

## キャッシュ構成 (cacheComponents)

- `next.config.ts` で `cacheComponents: true` を有効化
- 各 chart server component (`CpuChart.tsx` 等) の冒頭で `'use cache'` + `cacheLife({ revalidate: 10, expire: 60, stale: 10 })` + `cacheTag('chart:cpu')` 等を宣言。fetch + データ整形 + JSX 構築までまるごと cache 単位
- 4 chart で共有される `fetchContainers` のみ fetcher 側でも `'use cache'` (`cacheTag('containers')`)。dedup 兼共有 cache
- それ以外の `fetchCpuStatus` 等は cache 指定なし (chart 側 cache に内包される)
- `FetchResult` の error 側は `{ message: string }` (serializable)。Error インスタンスを返すと `'use cache'` の境界で serialize できないため
- `app/page.tsx` は `dynamic` 指定なし。build 結果で Partial Prerender (`◐ /`) になる
- 不変条件: cache された関数内では server-only な dynamic API (`cookies()`, `headers()`, `searchParams` の await 等) を呼ばない

## 既知の落とし穴・気をつける点

- **`fetchContainers()` が 4 チャートで個別に呼ばれている**（`CpuChart.tsx` など）。`'use cache'` 化で dedup されるので問題は薄まったが、コア側の負荷は依然として 1 リクエストにつき N。
- **コア側は単一スレッド**: web 側は `Promise.all` で並列フェッチを発射しているが、`core/src/server.rs` の TCP listener がシングルスレッド逐次処理なので実際には serialize される。core をスレッド化すれば web 側無修正で並列が効く。
- **`core/src/server.rs` の TCP listener はメインスレッドで逐次処理**。並列フェッチ化するならここをスレッド化必須。
- **`limited_convert_time_string_to_f32`**（`server.rs:135`）は日付を捨てて時刻だけを秒に変換している。日次ローテ前提。日跨ぎでバグる可能性あり。
- **`core/src/log.rs`** の Docker API レスポンスで `time == "0001-01-01T00:00:00Z"` のケースが実在する。`break` で無視している。

## Git worktree 運用メモ

- このリポジトリは bare clone (`/Users/daiji/sources/cephylas/.bare`) を中心に worktree を切る運用。
- `claude --worktree <name>` で worktree 作成 + 新セッション起動が可能。Claude Code のセッションは cwd ごとに分離される（`~/.claude/projects/-Users-daiji-sources-cephylas-<branch>/`）。
- ただし auto-memory は `.bare` 配下に集約されるため、worktree 間で共有される。
- worktree を跨いで context を引き継ぎたい場合は **`TASKS.md` 経由が推奨**。セッション直接共有はしない（cwd が違うため別セッション扱い）。
