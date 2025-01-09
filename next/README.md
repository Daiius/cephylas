# Cephylas Next.js Viewer
This is a Next.js application visualize cephylas log file.

## Memo
Cephylas core, written in Rust, outputs JSONL log file.

## 開発メモ
- ログ記録
  - 小数点以下不要な桁数がデータに含まれる
    - 自作のdump関数を準備する
  - ログファイルが大きくなる
    - logrotateをどう使うか？
      - ホストで呼び出す
      - ~~コンテナで呼び出す~~これは筋が悪そう
- ログ読み取り
  - 最新の一定行を読み取るにはどうするか？
    大きなJSONLファイル中から必要な行だけ抜き出すのがまず難しい
    （末尾から一定行数だけ呼び出す簡便な手法がない）。
    - streamで一度行数を確認し再度必要な行までスキップ?
    - ファイル末尾から読み出しサイズ不足ならリトライ?
    - Rust側機能を更に発展させJSONサーバ機能を持たせる?
- グラフ表示
  - Next.jsグラフ表示のベストプラクティス検討
    - CSR
      - Chart.js等インタラクティブ性の高いコンポーネントあり
      - SC -> CC へのデータ受け渡しがHTML中のJSON埋め込みによって
        行われるのでデータを相当絞らないといけない
        - 2025/01/10 実験
          - JSON.stringifyにnumberの小数点以下の桁数を絞るロジックを追加
            → HTMLは74.8kBに
          - 外してみる...downsampleCountは512→107kB
          - 25%の通信量低減は、型チェックが効かなくなる
            デメリットには及ばないだろうという感覚が有る
            グラフ形状もほぼ問題なさそう
          - downsampleCountが大きくなるとこの効果は大きくなっていきそう
    - SSR
      - ライブラリがある程度絞られる
      - Recharts, nivo, React Plotly.js, visx
      - 今回は保留、Chart.jsを活用する前提でやってみよう
      - でもRecharts+最低限の機能も良さげに見える
        - Legend関連のクリック機能など付けたらどうせCCになっちゃう？


複雑なログ形式をグラフ表示用に再成型するのがまた少し手間

Next.jsはSCからCCへデータを渡す際にJSON stringを使うので、
小数点以下細かすぎる文字列データがクライアントに渡され
通信データ量が増加する

それを避けるため自前でstringifyすると型チェックが通らなくなる

