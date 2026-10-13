# 05. 開発環境とデプロイ

本章は**姿勢のみ**を書く。ドメイン・接続先・設定値・手順の実物は書かない
（[README](./README.md) §秘匿方針）。実値は gitignore 対象の `.claude/local/` に置く。

**実装済みの範囲**: §開発環境と §本番の形は現状のとおりである。

## 開発環境

**core は Docker、web はホストで動かす。**

| コマンド | 内容 |
|---|---|
| `pnpm dev` | core を Docker で起動し、Next.js をホストで起動する |
| `pnpm dev:core` | core だけ起動する |
| `pnpm dev:web` | Next.js だけ起動する |
| `pnpm down` | `docker compose down` |
| `pnpm typecheck` | 全 workspace の `tsc --noEmit` |
| `pnpm build` | 全 workspace の build |

- 画面は `http://localhost:3000`、core の API は `http://localhost:7878` である。
- `API_URL` が未設定なら、web は `http://localhost:7878` を叩く。
- dev の core は `core/` をマウントして `cargo run` する（`core/Dockerfile`）。
  ソースを変えたらコンテナを再起動する。
- `log/` をマウントし、`log/log_daily` を読み書きする。リポジトリにはサンプルのログを置いている。

### 個人ごとの上書き

`compose.override.yml`（gitignore 対象）で、開発機ごとの調整を入れる。

- **メモリ制限の緩和が必要である。** `compose.yml` の 50MB は本番向けの値で、
  このままだと dev の `cargo build` が強制終了される。
- ⚠ **`compose.yml` は Docker ソケットをマウントしていない。** core は
  `/var/run/docker.sock` を読むので、統計を取るにはソケットのマウントを足す必要がある。
  rootless Docker など、ソケットの場所が違う環境ではそのパスに合わせる。
  現状はこの設定が override 任せになっており、リポジトリを取ってきただけでは収集が動かない。
  **dev の compose だけで収集まで動く**のがあるべき姿である。

## 本番の形

| 要素 | 置き場所 |
|---|---|
| web | Vercel（`https://<frontend-domain>`） |
| core | 作者の VPS 上のコンテナ |
| nginx | 同じ VPS 上のコンテナ。`https://<api-domain>/cephylas/*` を core へ中継する |

- **web は `main` への push で Vercel が自動でデプロイする。** Vercel 側で `API_URL` を
  API の公開 URL に設定する。
- **core の更新を自動で配置する仕組みはリポジトリに無い。** VPS 上で `compose.yml` に
  `compose.prod.yml` を重ねて起動する想定である。本番向けのイメージ定義は
  `core/Dockerfile.prod`（release ビルドを distroless に載せる）にある。
  ⚠ リポジトリ内の `compose.yml` は dev 用の `core/Dockerfile` を指したままで、
  本番が実際にどの定義で動いているかはリポジトリからは分からない（要確認）。
- **web が新しい API を必要とする変更は、core の配置を先に行う。** web は `main` への
  マージで自動的に配置されるが、core は手で更新するまで古いままである。順序を逆にすると、
  web が叩く経路が本番の core に無く、チャートが表示されない。
  - web は core の `/usages/*` を必要とする（[03](./03-web.md) §データ取得）。
    この経路を持つ core を先に配置してから、web を配置する。
  - core は以前の経路（コンテナ別の API）を残しているので、core を先に更新しても
    以前の web は動き続ける（[02](./02-core.md) §REST API（現行））。
  - このため、core の変更と、それを必要とする web の変更は、別々にマージできる単位に分ける。
- **nginx は core への中継だけを行う**（`nginx.conf`）。web は Vercel にあるので、
  nginx から Next.js へは中継しない。`nginx.conf` は 80 番で待ち受けるだけで TLS を扱わない。
  TLS の終端は前段で行う想定である（要確認）。
- **core にはメモリ 50MB の制限を掛ける。** 同居する他のコンテナを圧迫しないためである
  （[01](./01-architecture.md) §設計方針）。
- **core は Docker ソケットを読める必要がある。** core が送るのは GET だけだが、
  ソケットに届くプロセスは Docker を操作できる。core のコンテナをインターネットに
  直接さらさない理由の 1 つである。⚠ ソケットのマウントはリポジトリ内のどの compose にも
  書かれておらず、本番での与え方は運用側の設定である（要確認）。

### ベースイメージ

`core/Dockerfile` / `core/Dockerfile.prod` と `compose.prod.yml` は、ベースイメージを
`latest` タグで指している。**再ビルドするまで更新されず、ビルドのたびに中身が変わりうる。**
タグを固定するかは検討中である。

## ログのローテーション

**ホスト側の logrotate が、24 時間ごとに `log/log_daily` を回す。** core はローテーションを
行わない（[02](./02-core.md) §ログの保持）。

- core は書き込みのたびにファイルを開き直すので、ローテーション後に core へ通知する必要は無い。
- ローテーション直後に core を再起動すると、復元できるデータは新しいファイルにある分だけになる。
- ローテーションが遅れてファイルが 24 時間分より長くなっても、キャッシュに復元されるのは
  最後の行から 24 時間以内の分だけである（[02](./02-core.md) §キャッシュ）。
- logrotate の設定はリポジトリに置いていない。保持する世代数は運用側で決める。

## 依存の更新

- GitHub の **Dependabot alerts を有効にしている。** 自動で更新 PR を出す設定
  （`dependabot.yml`）は置いていない。
- 更新したら `pnpm audit`、`pnpm typecheck`、`pnpm build` を通す。
- `pnpm-workspace.yaml` で `sharp` のビルドスクリプトを許可している（Next.js の画像処理が依存する）。

## レビュー

PR は `oculibis` レビュー bot に掛ける。bot は `.github/review-bot.json` の指定に従い、
`prd/` と `AGENTS.md` を仕様の正典として読む。
