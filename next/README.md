# Cephylas Next.js Viewer
This is a Next.js application visualize cephylas log file.

## Memo
Cephylas core, written in Rust, outputs JSONL log file.

## 開発メモ
大きなJSONLファイル中から必要な行だけ抜き出すのがまず難しい
（末尾から一定行数だけ呼び出す簡便な手法がない）

複雑なログ形式をグラフ表示用に再成型するのがまた少し手間

Next.jsはSCからCCへデータを渡す際にJSON stringを使うので、
小数点以下細かすぎる文字列データがクライアントに渡され
通信データ量が増加する

それを避けるため自前でstringifyすると型チェックが通らなくなる

