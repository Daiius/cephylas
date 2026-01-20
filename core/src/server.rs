
use std::net::SocketAddr;
use std::sync::Arc;

use bytes::Bytes;
use http_body_util::Full;
use hyper::body::Incoming;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request, Response, StatusCode, Method};
use hyper_util::rt::TokioIo;
use tokio::net::TcpListener;

use crate::log_cache::{self, SharedUsageCache};
use super::error;

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

fn json_response(body: String) -> Response<Full<Bytes>> {
    Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "application/json")
        .header("Connection", "close")
        .body(Full::new(Bytes::from(body)))
        .unwrap()
}

fn not_found_response() -> Response<Full<Bytes>> {
    Response::builder()
        .status(StatusCode::NOT_FOUND)
        .body(Full::new(Bytes::new()))
        .unwrap()
}

fn method_not_allowed_response() -> Response<Full<Bytes>> {
    Response::builder()
        .status(StatusCode::METHOD_NOT_ALLOWED)
        .body(Full::new(Bytes::new()))
        .unwrap()
}

fn internal_error_response(msg: &str) -> Response<Full<Bytes>> {
    Response::builder()
        .status(StatusCode::INTERNAL_SERVER_ERROR)
        .body(Full::new(Bytes::from(msg.to_string())))
        .unwrap()
}

/// リソース使用状況を記録しているコンテナの名前を
/// アルファベット順に返します
async fn route_containers(
    log_cache: &SharedUsageCache,
) -> Response<Full<Bytes>> {
    let lock = log_cache.read().await;
    let container_names = lock.cpu.container_names();
    let data = container_names
        .iter()
        .map(|s| format!("\"{}\"", s))
        .collect::<Vec<String>>()
        .join(",");
    let body = format!("[{}]", data);
    json_response(body)
}

/// CPU/メモリ使用状況を返すルートです
async fn route_cpu_or_memory_usage(
    log_cache: &SharedUsageCache,
    container_name: &str,
    resource_type: &str,
) -> Response<Full<Bytes>> {
    let downsample_option = log_cache::DownsampleOption::default();
    let lock = log_cache.read().await;

    let data = match resource_type {
        "cpu" => lock.cpu
            .downsample(
                container_name,
                &downsample_option,
                |c| (
                    limited_convert_time_string_to_f32(&c.time)
                        .unwrap(),
                    c.percentage.unwrap_or_default()
                ),
            )
            .map(|v| data_to_json(v)),
        "memory" => lock.memory
            .downsample(
                container_name,
                &downsample_option,
                |m| (
                    limited_convert_time_string_to_f32(&m.time)
                        .unwrap(),
                    m.percentage.unwrap_or_default(),
                ),
            )
            .map(|v| data_to_json(v)),
        _ => None,
    };

    match data {
        Some(body) => json_response(body),
        None => not_found_response(),
    }
}

async fn route_io_usage(
    log_cache: &SharedUsageCache,
    container_name: &str,
    read_or_write: &str,
) -> Response<Full<Bytes>> {
    let downsample_option = log_cache::DownsampleOption::default();
    let lock = log_cache.read().await;

    let data = match read_or_write {
        "read" => lock.io
            .downsample(
                container_name,
                &downsample_option,
                |r| (
                    limited_convert_time_string_to_f32(&r.time)
                        .unwrap(),
                    r.readkBps.unwrap_or_default() as f32
                ),
            )
            .map(|v| data_to_json(v)),
        "write" => lock.io
            .downsample(
                container_name,
                &downsample_option,
                |w| (
                    limited_convert_time_string_to_f32(&w.time)
                        .unwrap(),
                    w.writekBps.unwrap_or_default() as f32,
                ),
            )
            .map(|v| data_to_json(v)),
        _ => None,
    };

    match data {
        Some(body) => json_response(body),
        None => not_found_response(),
    }
}

async fn route_net_usage(
    log_cache: &SharedUsageCache,
    container_name: &str,
    recv_or_send: &str,
) -> Response<Full<Bytes>> {
    let downsample_option = log_cache::DownsampleOption::default();
    let lock = log_cache.read().await;

    let data = match recv_or_send {
        "recv" => lock.net
            .downsample(
                container_name,
                &downsample_option,
                |r| (
                    limited_convert_time_string_to_f32(&r.time)
                        .unwrap(),
                    r.recvkBps.unwrap_or_default() as f32
                ),
            )
            .map(|v| data_to_json(v)),
        "send" => lock.net
            .downsample(
                container_name,
                &downsample_option,
                |s| (
                    limited_convert_time_string_to_f32(&s.time)
                        .unwrap(),
                    s.sendkBps.unwrap_or_default() as f32,
                ),
            )
            .map(|v| data_to_json(v)),
        _ => None,
    };

    match data {
        Some(body) => json_response(body),
        None => not_found_response(),
    }
}

async fn handle_request(
    req: Request<Incoming>,
    cache: SharedUsageCache,
) -> Result<Response<Full<Bytes>>, std::convert::Infallible> {
    if req.method() != Method::GET {
        return Ok(method_not_allowed_response());
    }

    let path = req.uri().path();
    let parts: Vec<&str> = path.split('/')
        .filter(|s| !s.is_empty())
        .collect();

    let response = match &parts[..] {
        ["containers"] =>
            route_containers(&cache).await,
        ["containers", container_name, resource_type] =>
            route_cpu_or_memory_usage(&cache, container_name, resource_type).await,
        ["containers", container_name, "io", read_or_write] =>
            route_io_usage(&cache, container_name, read_or_write).await,
        ["containers", container_name, "net", recv_or_send] =>
            route_net_usage(&cache, container_name, recv_or_send).await,
        _ => not_found_response()
    };

    Ok(response)
}

pub async fn start_server(
    cache: SharedUsageCache
) -> Result<(), error::Error> {
    let addr = SocketAddr::from(([0, 0, 0, 0], 7878));
    let listener = TcpListener::bind(addr).await?;

    println!("Server listening on http://{}", addr);

    loop {
        let (stream, _) = listener.accept().await?;
        let io = TokioIo::new(stream);
        let cache_clone = Arc::clone(&cache);

        tokio::spawn(async move {
            let service = service_fn(move |req| {
                let cache = Arc::clone(&cache_clone);
                async move { handle_request(req, cache).await }
            });
            if let Err(e) = http1::Builder::new()
                .serve_connection(io, service)
                .await
            {
                eprintln!("Error serving connection: {:?}", e);
            }
        });
    }
}
