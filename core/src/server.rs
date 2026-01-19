use tiny_http::{Server, Request, Response, Header, StatusCode};

use crate::log_cache::SharedUsageCache;

use super::error;
use super::log_cache;

type HttpResponse = Response<std::io::Cursor<Vec<u8>>>;

fn json_content_type() -> Header {
    Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap()
}

fn json_response(body: String) -> HttpResponse {
    Response::from_string(body)
        .with_status_code(StatusCode(200))
        .with_header(json_content_type())
}

fn error_response(status: u16) -> HttpResponse {
    Response::from_string("")
        .with_status_code(StatusCode(status))
}

/// リソース使用状況を記録しているコンテナの名前を
/// アルファベット順に返します
fn route_containers(
    log_cache: &log_cache::SharedUsageCache,
) -> Result<HttpResponse, error::Error> {
    let lock = log_cache.read().map_err(|e| e.to_string())?;
    let container_names = lock.cpu.container_names();
    let data = container_names
        .iter()
        .map(|s| format!("\"{}\"", s))
        .collect::<Vec<String>>()
        .join(",");
    let body = format!("[{}]", data);
    Ok(json_response(body))
}

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

/// CPU/メモリ使用状況を返すルートです
fn route_cpu_or_memory_usage(
    log_cache: &log_cache::SharedUsageCache,
    container_name: &str,
    resource_type: &str,
) -> Result<Option<HttpResponse>, error::Error> {
    let downsample_option = log_cache::DownsampleOption::default();
    let lock = log_cache.read().map_err(|e| e.to_string())?;

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

    Ok(data.map(json_response))
}

fn route_io_usage(
    log_cache: &log_cache::SharedUsageCache,
    container_name: &str,
    read_or_write: &str,
) -> Result<Option<HttpResponse>, error::Error> {
    let downsample_option = log_cache::DownsampleOption::default();
    let lock = log_cache.read().map_err(|e| e.to_string())?;

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

    Ok(data.map(json_response))
}

fn route_net_usage(
    log_cache: &SharedUsageCache,
    container_name: &str,
    recv_or_send: &str,
) -> Result<Option<HttpResponse>, error::Error> {
    let downsample_option = log_cache::DownsampleOption::default();
    let lock = log_cache.read().map_err(|e| e.to_string())?;

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

    Ok(data.map(json_response))
}

fn handle_request(
    request: Request,
    log_cache: &log_cache::SharedUsageCache,
) {
    if request.method() != &tiny_http::Method::Get {
        let _ = request.respond(error_response(405));
        return;
    }

    let url = request.url();
    let parts: Vec<&str> = url.split('/')
        .filter(|s| !s.is_empty())
        .collect();

    let response = match &parts[..] {
        ["containers"] =>
            route_containers(log_cache).map(Some),
        ["containers", container_name, resource_type] =>
            route_cpu_or_memory_usage(log_cache, container_name, resource_type),
        ["containers", container_name, "io", read_or_write] =>
            route_io_usage(log_cache, container_name, read_or_write),
        ["containers", container_name, "net", recv_or_send] =>
            route_net_usage(log_cache, container_name, recv_or_send),
        _ => Ok(None),
    };

    let final_response = match response {
        Ok(Some(resp)) => resp,
        Ok(None) => error_response(404),
        Err(_) => error_response(500),
    };

    let _ = request.respond(final_response);
}

pub fn start_server(
    log_cache: &log_cache::SharedUsageCache
) -> Result<(), error::Error> {
    let server = Server::http("0.0.0.0:7878")
        .map_err(|e| error::Error::OtherError(e.to_string()))?;

    for request in server.incoming_requests() {
        handle_request(request, log_cache);
    }

    Ok(())
}
