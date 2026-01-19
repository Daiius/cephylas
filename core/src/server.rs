use axum::{
    Router,
    routing::get,
    extract::{Path, State},
    response::Json,
    http::StatusCode,
};

use crate::log_cache::SharedUsageCache;
use super::error;
use super::log_cache;

/// 注意: かなり微妙な実装です
/// ログは1日でローテーションされるので(運用の仕方次第......)
/// 時刻のみ解釈して一日の始まりから何秒経ったかに変換します
fn limited_convert_time_string_to_f32(
    time_str: &str
) -> Result<f32, error::Error> {
    // ISO time string は YYYY/MM/ddTHH:mm:ss.nnnnn という形式
    if let [_year_month_date, time] =
        time_str.split('T').collect::<Vec<&str>>()[..]
    {
        // 今はyear, month, dateは無視...
        if let [hours, minutes, seconds] =
            time.split(':').collect::<Vec<&str>>()[..]
        {
            let seconds = str::parse::<f32>(&seconds.replace("Z",""))
                .map_err(|e| e.to_string())?;
            let minutes = str::parse::<f32>(minutes)
                .map_err(|e| e.to_string())?;
            let hours = str::parse::<f32>(hours)
                .map_err(|e| e.to_string())?;

            return Ok(
                ((hours * 60.0) + minutes) * 60.0 + seconds
            )
        }
    }
    Err(error::Error::OtherError("invalid time format".to_string()))
}

/// Vec<&T> 型の使用率データをjson文字列に変換します
pub fn data_to_json<T: ToString>(data: Vec<&T>) -> String {
    format!(
        "[{}]",
        data.iter()
            .map(|d| d.to_string())
            .collect::<Vec<String>>()
            .join(",")
    )
}

/// リソース使用状況を記録しているコンテナの名前を
/// アルファベット順に返します
async fn get_containers(
    State(cache): State<SharedUsageCache>,
) -> Json<Vec<String>> {
    let lock = cache.read().unwrap();
    let names: Vec<String> = lock.cpu.container_names()
        .iter()
        .map(|s| (*s).clone())
        .collect();
    Json(names)
}

/// CPU/メモリ使用状況を返すハンドラ
async fn get_resource(
    State(cache): State<SharedUsageCache>,
    Path((container_name, resource_type)): Path<(String, String)>,
) -> Result<String, StatusCode> {
    let downsample_option = log_cache::DownsampleOption::default();
    let lock = cache.read().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let data = match resource_type.as_str() {
        "cpu" => lock.cpu
            .downsample(
                &container_name,
                &downsample_option,
                |c| (
                    limited_convert_time_string_to_f32(&c.time)
                        .unwrap_or_default(),
                    c.percentage.unwrap_or_default()
                ),
            )
            .map(|v| data_to_json(v)),
        "memory" => lock.memory
            .downsample(
                &container_name,
                &downsample_option,
                |m| (
                    limited_convert_time_string_to_f32(&m.time)
                        .unwrap_or_default(),
                    m.percentage.unwrap_or_default(),
                ),
            )
            .map(|v| data_to_json(v)),
        _ => None,
    };

    data.ok_or(StatusCode::NOT_FOUND)
}

/// IO使用状況を返すハンドラ
async fn get_io(
    State(cache): State<SharedUsageCache>,
    Path((container_name, read_or_write)): Path<(String, String)>,
) -> Result<String, StatusCode> {
    let downsample_option = log_cache::DownsampleOption::default();
    let lock = cache.read().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let data = match read_or_write.as_str() {
        "read" => lock.io
            .downsample(
                &container_name,
                &downsample_option,
                |r| (
                    limited_convert_time_string_to_f32(&r.time)
                        .unwrap_or_default(),
                    r.readkBps.unwrap_or_default() as f32
                ),
            )
            .map(|v| data_to_json(v)),
        "write" => lock.io
            .downsample(
                &container_name,
                &downsample_option,
                |w| (
                    limited_convert_time_string_to_f32(&w.time)
                        .unwrap_or_default(),
                    w.writekBps.unwrap_or_default() as f32,
                ),
            )
            .map(|v| data_to_json(v)),
        _ => None,
    };

    data.ok_or(StatusCode::NOT_FOUND)
}

/// ネットワーク使用状況を返すハンドラ
async fn get_net(
    State(cache): State<SharedUsageCache>,
    Path((container_name, recv_or_send)): Path<(String, String)>,
) -> Result<String, StatusCode> {
    let downsample_option = log_cache::DownsampleOption::default();
    let lock = cache.read().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let data = match recv_or_send.as_str() {
        "recv" => lock.net
            .downsample(
                &container_name,
                &downsample_option,
                |r| (
                    limited_convert_time_string_to_f32(&r.time)
                        .unwrap_or_default(),
                    r.recvkBps.unwrap_or_default() as f32
                ),
            )
            .map(|v| data_to_json(v)),
        "send" => lock.net
            .downsample(
                &container_name,
                &downsample_option,
                |s| (
                    limited_convert_time_string_to_f32(&s.time)
                        .unwrap_or_default(),
                    s.sendkBps.unwrap_or_default() as f32,
                ),
            )
            .map(|v| data_to_json(v)),
        _ => None,
    };

    data.ok_or(StatusCode::NOT_FOUND)
}

pub async fn start_server(
    log_cache: SharedUsageCache
) -> Result<(), error::Error> {
    let app = Router::new()
        .route("/containers", get(get_containers))
        .route("/containers/{name}/{resource}", get(get_resource))
        .route("/containers/{name}/io/{type}", get(get_io))
        .route("/containers/{name}/net/{type}", get(get_net))
        .with_state(log_cache);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:7878").await?;
    println!("Server listening on 0.0.0.0:7878");
    axum::serve(listener, app).await?;

    Ok(())
}
