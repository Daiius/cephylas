# Cephylas core

Docker container のリソース使用率を記録し、REST API で返す Rust 製の単一バイナリ。

Cephylas comes from "cephonodes hylas", a kind of moth with clear wings.

**仕様は [prd/02-core.md](../prd/02-core.md) にある**（収集 / 使用率の算出 / ログ形式 /
キャッシュ / ダウンサンプリング / REST API / 既知の問題）。

## 実行

```bash
cargo run            # ./log/log_daily を読み書きし、/var/run/docker.sock を読む
```

`0.0.0.0:7878` で待ち受ける。開発時の起動方法は [AGENTS.md](../AGENTS.md) を参照。

## データ構造

```mermaid
classDiagram
    class Usages {
        time: String
        millis: u16
        usages: HashMap(String, Usage)
    }
    class Usage {
        cpu: CpuUsage
        memory: MemoryUsage
        io: IoUsage
        net: NetUsage
    }
    class UsageCache {
        map: HashMap(String, VecDeque(UsagePoint))
    }
    class UsagePoint {
        time_ms: i64
        cpu: Option(f32)
        memory: Option(f32)
        read_kbps: Option(u32)
        write_kbps: Option(u32)
        recv_kbps: Option(u32)
        send_kbps: Option(u32)
    }
    Usages --> Usage : コンテナ名ごと
    UsageCache --> UsagePoint : コンテナ名ごとに時刻順
    note for Usages "ログ 1 行 = 1 tick"
    note for UsageCache "最新の点から 24 時間分、1 コンテナ最大 8640 点"
```

## 処理の流れ

```mermaid
sequenceDiagram
    participant docker as Docker Engine<br/>(unix socket)
    participant logger as logger thread
    participant log as log/log_daily
    participant cache as UsageCache<br/>(Arc RwLock)
    participant server as server thread
    participant web as Next.js

    note over logger,cache : 起動時: ログ末尾 8640 行からキャッシュを復元

    loop 10 秒ごと
        logger ->> docker : GET /containers/json
        loop コンテナごと
            logger ->> docker : GET /containers/{name}/stats
        end
        note over logger : 前回との差分から使用率を算出
        logger ->> log : JSONL を 1 行追記
        logger ->> cache : コンテナごとに 1 点追加し、24 時間より古い点を捨てる
    end

    web ->> server : GET /containers/{name}/cpu など
    server ->> cache : 読み取り + ダウンサンプリング
    server -->> web : JSON
```
