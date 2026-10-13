# Cephylas

Docker container のリソース使用率を記録・可視化する Web アプリ。

このファイルは**開発ガイド**（使用する各コーディングエージェント共通）である。
**仕様の正典は [`prd/`](./prd/README.md)**。構成・API・UI の仕様はそちらに書き、ここには重複させない。

- **core/** — Rust 製ロガー兼 API サーバ。Docker ソケットから統計を 10 秒ごとに取得し、
  ファイルログとメモリキャッシュへ記録する。`:7878` で REST を返す。→ [prd/02-core.md](./prd/02-core.md)
- **next/** — Next.js 16 / React 19 のフロント。Server Component から core の API を叩いて
  Chart.js で描画する。pnpm workspace の唯一の TS package（package name は `web`）。→ [prd/03-web.md](./prd/03-web.md)
- **nginx.conf** — 本番 VPS で `<api-domain>/cephylas/*` を core に中継する。dev では使わない。

## まず読むもの

| 知りたいこと | 読む場所 |
|---|---|
| 何を作っているか・何をやらないか | [prd/00-product.md](./prd/00-product.md) |
| 構成と設計方針 | [prd/01-architecture.md](./prd/01-architecture.md) |
| 公開してよい情報の線・アクセス制限の計画 | [prd/04-access-and-privacy.md](./prd/04-access-and-privacy.md) |
| 開発環境・デプロイ | [prd/05-deploy.md](./prd/05-deploy.md) |
| 進行状況・次にやること | `.claude/local/TASKS.md`（gitignore 対象。**存在すればセッション開始時に読む**） |
| 本番の実値（ドメインなど） | `.claude/local/DEPLOY.md`（gitignore 対象。存在すれば参照する） |

## 守ること

仕様の詳細は PRD にある。作業中に踏みやすいものだけを挙げる。

1. **ブラウザから core を直接叩かない。** core の API を叩くのは Server Component だけ。
   `API_URL` に `NEXT_PUBLIC_` を付けない。
2. **API に載せてよいのはコンテナ名と使用率だけ。** Docker のレスポンスをそのまま外へ流さない。
3. **使用率の算出とダウンサンプリングは core で行う。** web に計算を戻さない。
   ダウンサンプリングを等間隔の間引きに置き換えない。
4. **core は軽量・依存最小を優先する**（方針であり禁止ではない。crate を足すなら理由を PR に書く）。
5. **`'use cache'` を付けた関数の中で `cookies()` / `headers()` / `searchParams` を呼ばない。**
   cache 越しに返す値はシリアライズできる形にする（`Error` インスタンスを返さない）。
6. **UI は light テーマのみ。** Chart.js がダークテーマを持たない。

## 公開リポジトリでの秘匿

このリポジトリは**公開**である。詳細は [prd/README.md](./prd/README.md) §秘匿方針。

- 🔒 `.env*` は**読まない・コミットしない**。
- 🔒 実ドメイン・VPS の接続先・本番で動いているコンテナ名を、文書にもコードのコメントにも書かない。
  `<frontend-domain>` / `<api-domain>` / `<root-domain>` を使う。
- 🔒 個人ローカルのメモは `.claude/local/` に置く（`.claude/local/.gitignore` の `*` で除外）。

## 開発コマンド

```bash
pnpm dev         # core を docker 起動 + Next.js を host で起動
pnpm dev:core    # core だけ起動
pnpm dev:web     # Next.js だけ起動 (host)
pnpm down        # docker compose down

pnpm typecheck   # 全 workspace の tsc --noEmit
pnpm build       # 全 workspace の build
pnpm audit       # 既知の脆弱性の確認
```

`pnpm dev` で起動後、http://localhost:3000 で確認。core API は http://localhost:7878。
`API_URL` が無い場合は `http://localhost:7878` にフォールバックする（`next/src/lib/fetchers.ts`）。

core 単体は `core/` で `cargo build` / `cargo run` できる。カレントディレクトリの `./log/log_daily` を
読み書きし、`/var/run/docker.sock` を読む。自動テストは `cargo test` で、現状は server
（`core/src/server.rs`）の分だけがある。🔒 テストで `7878` 番を bind しない（`127.0.0.1:0` を使う）。

### 個人 override

`compose.override.yml`（gitignore 対象）で開発機ごとの調整を入れる。**新しい開発機では作り直す必要がある。**

```yaml
services:
  cephylas:
    deploy:
      resources:
        limits:
          memory: 2G   # 本番 50M だと cargo build が SIGKILL されるので緩和
    volumes:
      - /var/run/docker.sock:/var/run/docker.sock:ro   # rootless Docker ではソケットのパスを合わせる
```

## 文書の置き場

| 種類 | 置き場 |
|---|---|
| 仕様（あるべき設計・実装状況・gap） | `prd/` |
| 仕様を決めた経緯 | `prd/_grilling/decisions.md` |
| 開発ガイド・作業上の注意 | この `AGENTS.md` |
| 進行状況・引き継ぎ | `.claude/local/TASKS.md` |
| 本番の実値・運用メモ | `.claude/local/` |

- **仕様を変える PR は、同じ PR で PRD も直す。** PRD が先行するのは構わないが、
  「実装済み」の記述が実装と食い違う状態を残さない。
- `.claude/local/` は worktree ごとに別である（git 管理外なので共有されない）。
  worktree を跨いで引き継ぐときは、必要なファイルを手で写す。

## レビュー

PR は `oculibis` レビュー bot に掛け、指摘ゼロになるまで追加コミットで対応する。
bot は `.github/review-bot.json` に従い、`prd/` とこのファイルを正典として読む。
マージの手法は、これまで merge commit を使っている。
