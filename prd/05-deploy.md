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
| `pnpm down` | dev の core を止める |
| `pnpm typecheck` | 全 workspace の `tsc --noEmit` |
| `pnpm build` | 全 workspace の build |

- 画面は `http://localhost:3000`、core の API は `http://localhost:7878` である。
- `API_URL` が未設定なら、web は `http://localhost:7878` を叩く。
- dev の core は `core/` をマウントして `cargo run` する（`core/Dockerfile`）。
  ソースを変えたらコンテナを再起動する。
- `log/` をマウントし、`log/log_daily` を読み書きする。

### dev 用の compose

**リポジトリを取ってきただけで、dev の収集まで動く。** `pnpm dev` / `pnpm dev:core` / `pnpm down` は、
`compose.yml` に **`compose.dev.yml`** を重ねて読む。

`compose.dev.yml` は dev に必要な差分だけを持つ。

- **メモリ制限を 2GB に緩める。** `compose.yml` の 50MB は本番向けの値で、
  そのままだと dev の `cargo build` が強制終了される。
- **Docker ソケットを読み取り専用でマウントする。** core は `/var/run/docker.sock` を読む。
  rootless Docker など、ホスト側のソケットの場所が違う環境では、環境変数 `DOCKER_SOCK` で
  場所を指定する（未設定なら `/var/run/docker.sock`）。

🔒 **dev の都合を `compose.yml` に入れない。** `compose.yml` は本番も読む。
dev だけに要る設定は `compose.dev.yml` に置く。本番は `compose.dev.yml` を読まない。

⚠ ファイルを明示して読むので、`compose.override.yml` は自動では読まれない。
開発機ごとの調整は、環境変数（`DOCKER_SOCK`）で行う。

### リポジトリ内のログ

`log/log_daily` は追跡対象で、過去に本番で採ったログがそのまま入っている。

- **dev では、このログのグラフは起動から約 10 秒で消える。** キャッシュの窓は現在時刻から
  24 時間なので（[02](./02-core.md) §キャッシュ）、古い時刻の点は最初の tick で窓の外になる。
  以後は、開発機で実際に動いているコンテナの値が出る。
- dev の core はこのファイルに追記するので、起動すると作業ツリーに差分が出る。**コミットしない。**
- ⚠ **このファイルを追跡対象から外す・差し替える変更は、本番の配置手順と合わせて行う。**
  本番でも同じパスに稼働中のログを書いており、リポジトリ側でこのファイルを変えると、
  本番で更新を取り込むときに衝突するか、稼働中のログを失う。実際のコンテナ名が入っている点
  （[04](./04-access-and-privacy.md) §gap）と合わせて、**検討中**である。

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
