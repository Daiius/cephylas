# PRD: Cephylas

> **この PRD は「実装予定の仕様（あるべき設計）」を書く正典である。** 現在の実装と乖離していても、
> それは**実装がまだ追いついていない gap** であり、PRD の誤りではない
> （理想を本文に書き、現状との差は「gap」「計画中」と明示する）。
> 読み手・レビューは、PRD が現行コードより先行している箇所を**欠陥ではなく実装 TODO** として扱うこと。
>
> **ただし次は現状に忠実であること**（ここが食い違えば PRD の誤り）: ①各章の**「実装済み」記述**は
> 現状を正確に表す、②**既存の API パス・レスポンスの形・ログの形式・環境変数名**は漏らさず載せる、
> ③**数値の意味論**（使用率の算出式・単位・時刻の扱い）は実装と一致させる。
> 未確定の論点は「要確認」と明記する。
>
> 🔒 **この PRD に GitHub Issue 番号は書かない。** 未実装の項目は「未実装」「計画中」「検討中」という
> **状態だけ**を書き、個別の課題は GitHub Issues 側で追う。
> 決定の経緯は [`_grilling/decisions.md`](./_grilling/decisions.md)。

## 目的

**Docker コンテナのリソース使用率を記録し、グラフで見る**ための小さな Web アプリである。
作者の本番 VPS で動くコンテナ群について、CPU・メモリ・ディスク IO・ネットワークの
直近 24 時間の推移を 1 画面で確認できる。

同時に、**少ない依存で低レベルから作ること**自体が題材である。Rust 製の core は
Docker Engine API を Unix ソケット越しに自前の HTTP で叩き、REST API も自前で返す。
フロントは Next.js の Server Component とキャッシュ機構を試す場を兼ねる。

**目的・利用者の範囲・非目標は [00-product.md](./00-product.md) に集約している。**

## スコープ

**機能の実装状況（実装済み / 計画中 / 検討中 / 要確認）は
[00-product.md](./00-product.md) §機能の実装状況にまとめている。**

要点だけ挙げる。

- **実装済み**: 10 秒間隔の収集 / JSONL ログ / メモリ上の 24 時間キャッシュ / ダウンサンプリング /
  全コンテナ分を 1 回で返す **bulk endpoint** とコンテナ別の REST API（[02](./02-core.md)） /
  4 種のチャート / 凡例兼フィルタ / Access のサービストークンのヘッダを付ける web 側の準備。
- **計画中**: **Cloudflare Access の導入による閲覧と API の制限**（[04](./04-access-and-privacy.md)）。
- **検討中**: 週・月単位へのログ要約。約束はしない。

## アーキ概観

**ブラウザが話す相手は Next.js だけ**である。core の API を叩くのは Next.js のサーバ側だけで、
ブラウザは core に直接アクセスしない。

```
[Docker Engine] ◄── unix socket ── [core (Rust) on VPS] ── append ──► [log/log_daily]
                                        │  メモリ上に 24 時間分をキャッシュ
                                        │  REST :7878
                                        ▲
                     [nginx on VPS]  https://<api-domain>/cephylas/*
                                        ▲
                                        │  Server Component から fetch（ブラウザは踏まない）
                     [Next.js on Vercel]  https://<frontend-domain>
                                        ▲
                                        │  HTTPS
                                    [Browser]
```

pnpm workspace は `next/` の 1 パッケージだけを含む。`core/` は workspace の外にある
Rust プロジェクトである。詳細は [01-architecture.md](./01-architecture.md)。

## 公開リポジトリでの秘匿方針

本リポジトリは**公開**である。

- 🔒 **秘密情報は文書にもコードにも書かない。** `.env*` の値、Cloudflare Access の
  サービストークンなどの資格情報。`.env*` は**読まない・コミットしない**。
  環境変数の**名前**は書いてよい（`CF_ACCESS_CLIENT_ID` など）。**値は書かない。**
- 🔒 **ドメイン・VPS の接続先・本番の具体設定は書かない。** `<frontend-domain>` /
  `<api-domain>` / `<root-domain>` のプレースホルダを用いる。TLS・リバースプロキシ・
  アクセス制限・ログローテーションは**姿勢のみ**を記述する。
  実値は gitignore 対象の `.claude/local/` に置き、「存在すれば参照する」形にする。
- 🔒 **本番で動いているコンテナの名前を文書に書かない。** 画面では公開している情報だが
  （[04](./04-access-and-privacy.md)）、文書に固定すると制限を入れたあとも残る。
  例示には `web` `db` のような仮の名前を使う。
- ⚠ **git の履歴には、過去にコミットされた実ドメインが残っている。** 現行の文書から消しても
  秘匿にはならないことを承知のうえで、現行の文書を他リポジトリと同じ規則に揃えている。
- 🔒 **一時的な情報は書かない。** 「次に何をする」のような進行状況は
  `.claude/local/TASKS.md`（gitignore 対象）に置く。

## 文書索引

| 文書 | 内容 |
|---|---|
| [00-product.md](./00-product.md) | 目的 / 利用者の範囲 / 非目標 / 画面と操作 / 機能の実装状況 |
| [01-architecture.md](./01-architecture.md) | core と web の分担 / 通信の向き / リポジトリ構成 / 設計方針 |
| [02-core.md](./02-core.md) | 収集 / 使用率の算出 / ログ形式 / キャッシュ / ダウンサンプリング / REST API / gap |
| [03-web.md](./03-web.md) | ページ構成 / チャート / 凡例兼フィルタ / 色 / キャッシュ構成 |
| [04-access-and-privacy.md](./04-access-and-privacy.md) | 公開してよい情報の線 / Cloudflare Access の計画と web 側の準備 |
| [05-deploy.md](./05-deploy.md) | 開発環境 / Vercel / VPS / ログローテーション（姿勢のみ） |
| [_grilling/decisions.md](./_grilling/decisions.md) | 仕様策定の経緯（正典ではない） |
