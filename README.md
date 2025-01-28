# Cephylas Logger and Visualizer
Cephylas is a simple resource usage logger & visualizer for docker containers.

CephylasはDocker containerのリソース使用率を記録・可視化する
Rust + Next.js によるWebアプリケーションです。

## Screenshots

## Name and Logo
inspired by super cute kawaii insect _cephonodes hylas_, which has pretty transparent wings!

名前は _cephonodes hylas_ という学名の虫から取っています。透明な羽のかわいいやつです。

![](https://faveo-systema.net/cephylas/cephonodes-hylas.svg)

## Design
### Backend
- Goals: minimal and efficient<br/>
  - Language: Rust<br/>
  - Dependency<s>ies</s>: [json](https://docs.rs/json/latest/json/)
  - Functions:
    - Logs Docker API result (/containers/{id}/stats) to a file
    - Caches resource usage in memory
    - Serves JSON as a REST API server (e.g. /containers/{id}/cpu)
### Frontend
- Goals: technical exploration for data visualizations in Next.js
  - Language, Framework: TypeScript, Next.js
  - Dependencies: Next.js, Chart.js (for visualization), etc. 
  - Functions:
    - serves resource usage graph as dynamically rendered HTML page



