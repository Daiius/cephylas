# 03. web

web は Next.js（App Router）のアプリで、`next/` に置く。core の API からデータを取り、
Chart.js で描画する。

**実装済みの範囲**: 本章の内容はすべて実装済みである。

## スタック

- Next.js 16 / React 19 / Turbopack
- Chart.js 4 + chartjs-adapter-luxon（時間軸）
- Tailwind CSS 4 + daisyUI 5（設定は `globals.css` に CSS で書く）
- zod（API レスポンスの検証）

## ページ構成

| パス | 実装 | 描画 |
|---|---|---|
| `/` | `next/src/app/page.tsx` | Partial Prerender（外枠は静的、チャートはストリーミング） |
| `/about` | `next/src/app/about/page.tsx` | 静的 |

`/` は 4 つのチャートをそれぞれ `<Suspense>` で包む。フォールバックは `ChartSkeleton` である。
1 枚の取得が遅くても、他のチャートと外枠の表示を止めない。

全体を `FilterProvider` で包み、4 枚のチャートがフィルタの状態を共有する。

## データ取得

🔒 **core の API を叩くのは Server Component だけである**（[01](./01-architecture.md) §通信の向き）。

**チャート 1 枚につき 1 リクエスト**である。core の `/usages/{kind}` が全コンテナ分の系列を
まとめて返す（[02](./02-core.md) §REST API（現行））。1 回の表示で core に送るのは 4 本である。

- 取得関数は `next/src/lib/fetchers.ts` の `fetchUsages(kind)` の 1 つである。
  `kind` は `cpu` / `memory` / `io` / `net`。
- API の URL は環境変数 `API_URL` から読む。未設定なら `http://localhost:7878` を使う
  （dev で core を手元に立てたときの既定）。
- レスポンスは zod で検証する。系列の `t` と `v` の長さが違う応答は、検証で失敗として扱う。
- 戻り値は `FetchResult<T>`（`{ ok: true, data }` または `{ ok: false, error: { message } }`）で、
  例外は投げない。**error 側は `Error` インスタンスではなく、シリアライズできる形にする。**
  `'use cache'` の境界を越えて返すためである。

各チャートの Server Component（`CpuChart.tsx` など）は、受け取った全コンテナ分を
Chart.js のデータセットに変換する。

- **コンテナの並びは web 側で決める。** コンテナ名を**コードポイント順**に並べ替える
  （`sortedContainerNames`）。core の並び（[02](./02-core.md) §REST API（現行））と同じ結果になるが、
  JSON のキーの順序には依存しない。ロケールに依存する比較（`localeCompare`）は使わない。
- 点は `{ x: t[i], y: v[i] }` である。**`x` は数値**（エポックからのミリ秒）で、
  Chart.js の時間軸にそのまま渡す。
- CPU とメモリは 1 コンテナにつき 1 本、IO とネットワークは 1 コンテナにつき 2 本
  （read と write、recv と send）のデータセットを作る。1 本目が実線、2 本目が破線である。

取得に失敗したときは、チャートの代わりに「取得中...」という文言を出す。
⚠ 失敗と取得中を区別しておらず、失敗の理由も画面には出ない。

## キャッシュ構成

`next.config.ts` で `cacheComponents: true` を有効にしている。

- **各チャートの Server Component が cache の単位である。** 冒頭で `'use cache'` を宣言し、
  取得・整形・JSX の構築までをまとめて cache する。
- 寿命は `cacheLife({ revalidate: 10, expire: 60, stale: 10 })` である。core の収集間隔
  （10 秒）に合わせている。
- タグは `chart:cpu` / `chart:memory` / `chart:io` / `chart:net`。
- 取得関数（`fetchUsages`）は cache を宣言しない（チャート側の cache に含まれる）。
  4 枚のチャートが共有する取得は無く、`containers` というタグも無い。

🔒 **不変条件**: cache される関数の中で、リクエストごとに変わる API（`cookies()`、`headers()`、
`searchParams` の await など）を呼ばない。

## チャート

`next/src/components/Chart.tsx`（Client Component）が Chart.js を包む。

- 折れ線で、点は通常は描かない（ホバー時だけ出す）。アニメーションは無効。
- **横軸は時刻**。目盛の単位は Chart.js に自動で選ばせ、目盛の境界に合わせて範囲を広げる
  （`bounds: 'ticks'`）。24 時間なら 1 時間刻み、短い範囲なら分刻みで、
  キリのよい時刻に目盛が来る。表示は `HH:mm` である。
- **縦軸は 0 始まり**で、上限は自動である。
- **ツールチップはホバー位置に最も近い 1 点だけ**を出す（`mode: 'nearest'`、`intersect: false`、
  `hitRadius: 10`）。全コンテナ分を並べると読めないためである。
- データセットには `containerName` を持たせ、フィルタと凡例の対応付けに使う。
  Chart.js の型は `next/src/types/chartjs.d.ts` の declaration merging で拡張している。
- Chart.js 標準の凡例とタイトルは使わない。タイトルは HTML の `<h3>` で出し、
  その下に自前の凡例を置く。
- 時刻はブラウザのタイムゾーンで表示される（core が返すのはエポックからのミリ秒）。

⚠ データセットが変わるたびに Chart.js のインスタンスを作り直している。
フィルタの切り替えだけは作り直さず、`update('none')` で反映する。

## 凡例兼フィルタ

`next/src/components/MiniLegend.tsx` が、各チャートの上端に**色の点とコンテナ名のチップ**を並べる。
凡例とフィルタを兼ねる。

| 操作 | 結果 |
|---|---|
| クリック / タップ | そのコンテナの表示・非表示を切り替える |
| ダブルクリック / ダブルタップ | そのコンテナだけを表示する。すでに単独表示なら全表示に戻す |

- ダブルの判定は「同じチップへの 250ms 以内の 2 回目」である。この判定のため、
  **シングルクリックの反映は 250ms 遅れる**。
- IO やネットワークのように 1 コンテナに 2 系列あるチャートでも、チップは 1 コンテナにつき 1 つである。
- チップは `<button aria-pressed>` で、非表示のものは薄く、打ち消し線を付ける。
- iOS のダブルタップズームを抑えるため `touch-manipulation` を付ける。

### フィルタの状態

`next/src/components/FilterContext.tsx` が、非表示のコンテナ名の集合（`hidden`）を
**唯一の状態**として持つ。

```ts
const { hidden, toggle, setAll, isolate, clear } = useFilter();
toggle(name);             // 1 件の表示・非表示を切り替える
setAll(names);            // hidden を names に置き換える
isolate(name, allNames);  // name 以外を非表示にする
clear();                  // 全表示に戻す
```

**状態は React の state だけで持ち、URL にも保存先にも残さない。** リロードすると全表示に戻る。

## 色

`next/src/lib/colors.ts` が、**コンテナ名のアルファベット順の位置**に Tableau 10 の配色を
順に割り当てる。並びは web 側の並べ替え（§データ取得）で決まり、core が返す順序には
依存しない。

- 並べ替えはコードポイント順なので、大文字で始まる名前は小文字で始まる名前より前に来る。
- コンテナが 10 個以下なら色は衝突しない。11 個目からは同じ色が巡回する。
- 同じコンテナは 4 枚のチャートで同じ色になる。
- ⚠ コンテナが増減してアルファベット順の位置が変わると、既存のコンテナの色もずれる。
  これは受け入れている。

## テーマ

**light のみ**である。`globals.css` で daisyUI のテーマを `light` に固定し、
`<html data-theme="light">` を指定している。Chart.js がダークテーマを持たないためである。

## gap / 検討中

- `next.config.ts` の `output: 'standalone'` は、Vercel へ配置する現在の構成では使っていない。
  残す理由があるかは要確認。
- `package.json` の `lint` スクリプト（`next lint`）は、現行の Next.js で動くか要確認。
