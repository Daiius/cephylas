# Cephylas web

Docker container のリソース使用率を、core の API から取得してグラフで表示する Next.js のアプリ。

**仕様は [prd/03-web.md](../prd/03-web.md) にある**（ページ構成 / データ取得 / キャッシュ構成 /
チャート / 凡例兼フィルタ / 色）。開発時の起動方法は [AGENTS.md](../AGENTS.md) を参照。

初期（2024〜2025 年）の検討メモはここにあったが、現行の構成と異なるため削除した。
当時は Next.js がログファイルを直接読み、ダウンサンプリングも web 側で行っていた。
経緯は [prd/01-architecture.md](../prd/01-architecture.md) §責務の分担に残している。
