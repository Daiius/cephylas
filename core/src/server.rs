
use std::io::{ Read, Write };

use crate::log_cache::SharedUsageCache;

use super::error;
use super::log_cache;


/// HTTPリクエストを一時的に記録する構造体
/// 一時的で良いので参照を使う
/// (大本はbuffer)
#[allow(dead_code)] // http_versionは読み取るが使用しない
struct Request<'a> {
    method: &'a str,
    uri: &'a str,
    http_version: &'a str,
}

impl<'a> TryFrom<&'a str> for Request<'a> {
    type Error = error::Error;
    fn try_from(value: &'a str) -> Result<Self, Self::Error> {
        if let [method, uri, http_version] = 
            value.lines().next()
            .map(|l| l.split(' '))
            .ok_or("cannot find first line of request data")?
            .collect::<Vec<&str>>()[..]
        {
            Ok(Request { method, uri, http_version })
        } else {
            Err(
                error::Error::OtherError(
                    "invalid format in the first line of reqest"
                    .to_string()
                )
            )
        }
    }
}

enum StatusCode {
    Ok,
    MethodNotAllowed,
    InternalServerError,
    NotFound,
}

/// GET以外のリクエストが来た際には一律で405を返します
///
/// 一般的なルータは
/// Fn(url: &str, stream: &mut TcpStream, log_cache: &SharedUsageCache)
///   -> Result<bool, error::Error>
/// 型としていますが、エラーを返すだけなので簡略化します
fn handle_method_not_allowed(
    stream: &mut std::net::TcpStream,
) -> Result<StatusCode, error::Error> {
    let response = "HTTP/1.1 405 MethodNotAllowed\r\n\r\n";
    stream.write(response.as_bytes())?;
    stream.flush()?;
    Ok(StatusCode::MethodNotAllowed)
}

/// ルータ内でエラーが発生などした場合には500を返します
/// 
/// 一般的なルータは
/// Fn(url: &str, stream: &mut TcpStream, log_cache: &SharedUsageCache)
///   -> Result<bool, error::Error>
/// 型としていますが、エラーを返すだけなので簡略化します
fn handle_generic_error(
    stream: &mut std::net::TcpStream,
    e: &error::Error,
) -> Result<StatusCode, error::Error> {
    let response = format!(
        "HTTP/1.1 500 InternalError\r\n\r\n {}",
        e.to_string(),
    );
    stream.write(response.as_bytes())?;
    stream.flush()?;

    Ok(StatusCode::InternalServerError)
}

/// どのルートにもマッチしなかった場合には404を返します
///
/// 一般的なルータは
/// Fn(url: &str, stream: &mut TcpStream, log_cache: &SharedUsageCache)
///   -> Result<bool, error::Error>
/// 型としていますが、エラーを返すだけなので簡略化します
fn route_not_found(
    stream: &mut std::net::TcpStream,
) -> Result<StatusCode, error::Error> {
    let response = "HTTP/1.1 404 NotFound\r\n\r\n";
    stream.write(response.as_bytes())?;
    stream.flush()?;

    Ok(StatusCode::NotFound)
}

/// リソース使用状況を記録しているコンテナの名前を
/// アルファベット順に返します
///
/// ルータは以下の型に統一して配列に格納し、
/// ループを回して順番にマッチするか否か確認しています
/// Fn(url: &str, stream: &mut TcpStream, log_cache: &SharedUsageCache)
///   -> Result<bool, error::Error>
/// 型としており、bool型はマッチしたか否かを返します
/// 処理に失敗すればError型を返します
fn route_containers(
    stream: &mut std::net::TcpStream,
    log_cache: &log_cache::SharedUsageCache,
) -> Result<StatusCode, error::Error> {
    let lock = log_cache.read().map_err(|e| e.to_string())?;
    let container_names = lock.cpu.container_names();
    let data = container_names
        .iter()
        .map(|s| format!("\"{}\"", s))
        .collect::<Vec<String>>()
        .join(",");
    let body = format!("[{}]", data);
    let body_bytes = body.as_bytes();
    let response = format!(
        "HTTP/1.1 200 OK\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n",
        body_bytes.len(),
    );
    stream.write(response.as_bytes())?;
    stream.write(body_bytes)?;
    stream.flush()?;

    Ok(StatusCode::Ok)
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
            //println!("h,m,s = {},{},{}", hours, minutes, seconds);
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
///
/// ルータは以下の型に統一して配列に格納し、
/// ループを回して順番にマッチするか否か確認しています
/// Fn(url: &str, stream: &mut TcpStream, log_cache: &SharedUsageCache)
///   -> Result<bool, error::Error>
/// 型としており、bool型はマッチしたか否かを返します
/// 処理に失敗すればError型を返します
fn route_cpu_or_memory_usage(
    stream: &mut std::net::TcpStream,
    log_cache: &log_cache::SharedUsageCache,
    container_name: &str,
    resource_type: &str,
) -> Result<StatusCode, error::Error> {
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

    if let Some(data) = data {
        let body_bytes = data.as_bytes();
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body_bytes.len(),
        );
        stream.write(response.as_bytes())?;
        stream.write(body_bytes)?;
        stream.flush()?;
        return Ok(StatusCode::Ok);
    }

    Ok(StatusCode::NotFound)
}

fn route_io_usage(
    stream: &mut std::net::TcpStream,
    log_cache: &log_cache::SharedUsageCache,
    container_name: &str,
    read_or_write: &str,
) -> Result<StatusCode, error::Error> {
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

    if let Some(data) = data {
        let body_bytes = data.as_bytes();
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body_bytes.len()
        );
        stream.write(response.as_bytes())?;
        stream.write(body_bytes)?;
        stream.flush()?;
        return Ok(StatusCode::Ok);
    }

    Ok(StatusCode::NotFound)
}

fn route_net_usage(
    stream: &mut std::net::TcpStream,
    log_cache: &SharedUsageCache,
    container_name: &str,
    recv_or_send: &str,
) -> Result<StatusCode, error::Error> {
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

    if let Some(data) = data {
        let body_bytes = data.as_bytes();
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n",
            body_bytes.len(),
        );
        stream.write(response.as_bytes())?;
        stream.write(body_bytes)?;
        stream.flush()?;

        return Ok(StatusCode::Ok);
    }

    Ok(StatusCode::NotFound)
}

fn handle_connection(
    stream: &mut std::net::TcpStream,
    log_cache: &log_cache::SharedUsageCache,
) -> Result<(), error::Error> {
    let mut buffer = [0; 1024];
    stream.read(&mut buffer)?;

    let request_data = String::from_utf8_lossy(&buffer[..]);
    //println!("Request: {}", request_data);

    let request = Request::try_from(request_data.as_ref())?;
    if request.method != "GET" {
        handle_method_not_allowed(stream)?;
        return Ok(());
    }

    // match式を使った単純なものに書き直せそう
    let parts: Vec<&str> = request.uri.split('/')
        .filter(|s| !s.is_empty())
        .collect();
    let result = match &parts[..] {
        ["containers"] => 
            route_containers(stream, log_cache),
        ["containers", container_name, resource_type] =>
            route_cpu_or_memory_usage(stream, log_cache, container_name, resource_type),
        ["containers", container_name, "io", read_or_write] =>
            route_io_usage(stream, log_cache, container_name, read_or_write),
        ["containers", container_name, "net", recv_or_send] =>
            route_net_usage(stream, log_cache, container_name, recv_or_send),
        _ => route_not_found(stream)
    };

    match result {
        Ok(StatusCode::NotFound) => route_not_found(stream)?,
        Err(e) => handle_generic_error(stream, &e)?,
        _ => { /* do nothing... */ StatusCode::Ok }
    };

    Ok(())
}

pub fn start_server(
    log_cache: &log_cache::SharedUsageCache
) -> Result<(), error::Error> {
    let listener = std::net::TcpListener::bind("0.0.0.0:7878")?;

    for stream in listener.incoming() {
        let mut stream = stream?;
        handle_connection(&mut stream, log_cache)?;
    }

    Ok(())
}

