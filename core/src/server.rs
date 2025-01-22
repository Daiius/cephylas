
use std::io::{ Read, Write };

use super::error;
use super::log_cache;

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

fn handle_method_not_allowed(
    stream: &mut std::net::TcpStream,
) -> Result<(), error::Error> {
    let response = "HTTP/1.1 405 MethodNotAllowed\r\n\r\n";
    stream.write(response.as_bytes())?;
    stream.flush()?;
    Ok(())
}
fn handle_generic_error(
    stream: &mut std::net::TcpStream,
    e: &error::Error,
) -> Result<(), error::Error> {
    let response = format!(
        "HTTP/1.1 500 InternalError\r\n\r\n {}",
        e.to_string(),
    );
    stream.write(response.as_bytes())?;
    stream.flush()?;
    Ok(())
}

fn route_not_found(
    _url: &str,
    stream: &mut std::net::TcpStream,
    _log_cache: &log_cache::SharedUsageCache,
) -> Result<bool, error::Error> {
    let response = "HTTP/1.1 404 NotFound\r\n\r\n";
    stream.write(response.as_bytes())?;
    stream.flush()?;
    
    Ok(true)
}

fn route_containers(
    url: &str,
    stream: &mut std::net::TcpStream,
    log_cache: &log_cache::SharedUsageCache,
) -> Result<bool, error::Error> {

    let pattern = "/containers";
    if url != pattern { return Ok(false) }

    let lock = log_cache.read().map_err(|e| e.to_string())?;
    let container_names = lock.cpu.container_names();
    let response = format!(
        "HTTP/1.1 200 OK\r\n\r\n [{}]",
        container_names
            .iter()
            .map(|s| format!("\"{}\"", s))
            .collect::<Vec<String>>()
            .join(",")
    );
    stream.write(response.as_bytes())?;
    stream.flush()?;

    Ok(true)
}

/// かなり微妙な実装
/// ログは1日でローテーションされるので
/// 日時まで変換する
/// 一日の始まりから何秒経ったか変換する
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

pub fn data_to_json<T: ToString>(data: Vec<&T>) -> String {
    format!(
        "[{}]",
        data.iter()
            .map(|d| d.to_string())
            .collect::<Vec<String>>()
            .join(",")
    )
}

fn route_usage(
    url: &str,
    stream: &mut std::net::TcpStream,
    log_cache: &log_cache::SharedUsageCache,
) -> Result<bool, error::Error> {
    let parts: Vec<&str> = url.split('/')
        .filter(|s| !s.is_empty())
        .collect();
    if let ["containers", container_name, resource] = &parts[..] {
        let downsample_option = log_cache::DownsampleOption::default();
        let lock = log_cache.read().map_err(|e| e.to_string())?;

        let data = match *resource {
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
            //"io" => lock.io
            //    .get(container_name)
            //    .map(|v| v.to_json()),
            //"net" => lock.net
            //    .get(container_name)
            //    .map(|v| v.to_json()),
            _ => None,
        };

        // コンテナ名が存在すればデータを返し、
        // 無ければルート自体にマッチしなかったことにする
        // リソース名がマッチしなかった場合もそうする
        // (404が後で返されるはず)
        if let Some(data) = data {
            let response = format!(
                "HTTP/1.1 200 OK\r\n\r\n {}",
                data
            );
            stream.write(response.as_bytes())?;
            stream.flush()?;
            return Ok(true);
        } else {
            return Ok(false)
        }
    } else {
        // url pattern is not matched
        return Ok(false);
    }
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

    let routes = [
        route_containers,
        route_usage,
        route_not_found,
    ];

    for route in routes {
        match route(request.uri, stream, log_cache) {
            Ok(true) => { /* uri match, handled */ break; },
            Ok(false) => { /* do nothing */ },
            Err(e) => { handle_generic_error(stream, &e)?; }
        }
    }

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

