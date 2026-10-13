
use std::io::{ Read, Write };
use std::sync::{ Arc, Condvar, Mutex };
use std::time::{ Duration, Instant };

use super::error;
use super::log_cache;

/// 待ち受けるアドレス
const LISTEN_ADDR: &str = "0.0.0.0:7878";
/// 同時に処理する接続数の上限。超えた接続は枠が空くまで待たせる
const MAX_CONNECTIONS: usize = 16;
/// 1回の読み書きの待ち時間の上限。リクエスト行を読み切るまでの上限も兼ねる
const IO_TIMEOUT: Duration = Duration::from_secs(5);
/// リクエスト行（1行目）の長さの上限。超えたら400を返す
const MAX_REQUEST_LINE_BYTES: usize = 1024;
/// accept自体が失敗したときに次を試すまでの待ち時間
/// (ファイル記述子の枯渇などで失敗が続くときに空回りしないため)
const ACCEPT_RETRY_INTERVAL: Duration = Duration::from_millis(100);
/// 500応答の本文。内部のエラー文字列は標準エラー出力にだけ出す
const INTERNAL_ERROR_BODY: &str = "internal server error";

/// HTTPリクエストを一時的に記録する構造体
/// 一時的で良いので参照を使う
#[allow(dead_code)] // http_versionは読み取るが使用しない
struct Request<'a> {
    method: &'a str,
    uri: &'a str,
    http_version: &'a str,
}

impl<'a> TryFrom<&'a str> for Request<'a> {
    type Error = error::Error;
    fn try_from(value: &'a str) -> Result<Self, Self::Error> {
        let line = value.lines().next()
            .ok_or("cannot find first line of request data")?
            .trim_end_matches('\r');
        match line.split(' ').collect::<Vec<&str>>()[..] {
            [method, uri, http_version]
                if !method.is_empty()
                    && uri.starts_with('/')
                    && http_version.starts_with("HTTP/") =>
                Ok(Request { method, uri, http_version }),
            _ => Err(error::Error::OtherError(
                "invalid format in the first line of request".to_string()
            )),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum StatusCode {
    Ok,
    BadRequest,
    NotFound,
    MethodNotAllowed,
    InternalServerError,
}

impl StatusCode {
    fn status_line(self) -> &'static str {
        match self {
            StatusCode::Ok => "200 OK",
            StatusCode::BadRequest => "400 Bad Request",
            StatusCode::NotFound => "404 Not Found",
            StatusCode::MethodNotAllowed => "405 Method Not Allowed",
            StatusCode::InternalServerError => "500 Internal Server Error",
        }
    }
}

/// 書き出す前の応答。経路の判定はこれを返すだけにして、
/// ストリームへの書き込みは write_response の1箇所に集める
struct Response {
    status: StatusCode,
    content_type: &'static str,
    body: String,
}

impl Response {
    fn json(body: String) -> Self {
        Response {
            status: StatusCode::Ok,
            content_type: "application/json",
            body,
        }
    }
    /// 本文を持たないエラー応答
    fn empty(status: StatusCode) -> Self {
        Response {
            status,
            content_type: "text/plain; charset=utf-8",
            body: String::new(),
        }
    }
    /// 500応答。本文は固定の文言で、内部の情報を載せない
    fn internal_error() -> Self {
        Response {
            body: INTERNAL_ERROR_BODY.to_string(),
            ..Response::empty(StatusCode::InternalServerError)
        }
    }
}

fn write_response<W: Write>(
    stream: &mut W,
    response: &Response,
) -> Result<(), error::Error> {
    let body_bytes = response.body.as_bytes();
    let mut bytes = format!(
        "HTTP/1.1 {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        response.status.status_line(),
        response.content_type,
        body_bytes.len(),
    ).into_bytes();
    bytes.extend_from_slice(body_bytes);
    stream.write_all(&bytes)?;
    stream.flush()?;

    Ok(())
}

/// 1コンテナ・1系列をダウンサンプリングしてjson文字列にします
/// 知らないコンテナ名ならNoneを返します
fn series_json(
    cache: &log_cache::UsageCache,
    container_name: &str,
    series: log_cache::Series,
) -> Option<String> {
    let points = cache.downsample(
        container_name,
        &log_cache::DownsampleOption::default(),
        series,
    )?;

    Some(format!(
        "[{}]",
        points.iter()
            .map(|point| series.point_to_json(point))
            .collect::<Vec<String>>()
            .join(",")
    ))
}

/// リソース使用状況を記録しているコンテナの名前を
/// アルファベット順に並べたjson文字列を返します
fn container_names_json(cache: &log_cache::UsageCache) -> String {
    let data = cache.container_names()
        .iter()
        .map(|s| format!("\"{}\"", s))
        .collect::<Vec<String>>()
        .join(",");
    format!("[{}]", data)
}

/// パスから応答を決めます。ストリームには触りません
/// 経路・コンテナ名・種別のどれかに一致しなければ404、
/// 処理に失敗すればError型を返します（呼び出し側で500にします）
fn route(
    uri: &str,
    log_cache: &log_cache::SharedUsageCache,
) -> Result<Response, error::Error> {
    let parts: Vec<&str> = uri.split('/')
        .filter(|s| !s.is_empty())
        .collect();
    // loggerがpanicしてlockがpoisonされていても、ここではpanicさせない
    let cache = log_cache.read().map_err(|e| e.to_string())?;

    let body = match &parts[..] {
        ["containers"] =>
            Some(container_names_json(&cache)),
        ["containers", name, "cpu"] =>
            series_json(&cache, name, log_cache::Series::Cpu),
        ["containers", name, "memory"] =>
            series_json(&cache, name, log_cache::Series::Memory),
        ["containers", name, "io", "read"] =>
            series_json(&cache, name, log_cache::Series::IoRead),
        ["containers", name, "io", "write"] =>
            series_json(&cache, name, log_cache::Series::IoWrite),
        ["containers", name, "net", "recv"] =>
            series_json(&cache, name, log_cache::Series::NetRecv),
        ["containers", name, "net", "send"] =>
            series_json(&cache, name, log_cache::Series::NetSend),
        _ => None,
    };

    Ok(match body {
        Some(body) => Response::json(body),
        None => Response::empty(StatusCode::NotFound),
    })
}

/// リクエスト行から応答を決めます
fn respond(
    request_line: &str,
    log_cache: &log_cache::SharedUsageCache,
) -> Response {
    let request = match Request::try_from(request_line) {
        Ok(request) => request,
        Err(_) => return Response::empty(StatusCode::BadRequest),
    };
    if request.method != "GET" {
        return Response::empty(StatusCode::MethodNotAllowed);
    }
    match route(request.uri, log_cache) {
        Ok(response) => response,
        Err(e) => {
            eprintln!("server: failed to handle request: {}", e);
            Response::internal_error()
        }
    }
}

/// 次の1回のreadで待つ時間の上限を変えられるストリーム
///
/// リクエスト行の読み取りを合計時間で打ち切るために使います
/// (readごとの固定のタイムアウトだけでは、期限の直前に届いたデータで
///  待ち時間が延び、合計が上限を超えてしまう)
trait ReadDeadline {
    fn limit_next_read(&mut self, timeout: Duration) -> std::io::Result<()>;
}

impl ReadDeadline for std::net::TcpStream {
    fn limit_next_read(&mut self, timeout: Duration) -> std::io::Result<()> {
        self.set_read_timeout(Some(timeout))
    }
}

/// リクエスト行の読み取り結果
enum RequestLine {
    /// 1バイトも受け取らずに終わった
    /// (相手が閉じた、無言のままタイムアウトした、など)
    Closed,
    /// MAX_REQUEST_LINE_BYTES を超えた
    TooLong,
    Line(String),
}

/// 最初の改行までを読みます。ヘッダは解釈しません
///
/// timeout はリクエスト行を読み切るまでの合計の上限です
/// 少しずつ送り続けて接続を占有されないよう、readのたびに残り時間だけ待ち、
/// 期限を過ぎてから届いた分は（改行やEOFであっても）受け付けません
fn read_request_line<R: Read + ReadDeadline>(
    stream: &mut R,
    timeout: Duration,
) -> Result<RequestLine, error::Error> {
    let started = Instant::now();
    let mut received: Vec<u8> = Vec::new();
    let mut chunk = [0; 1024];

    // 期限切れ。1バイトも受け取っていなければエラーとして扱わない
    let timed_out = |received: &Vec<u8>| {
        if received.is_empty() {
            Ok(RequestLine::Closed)
        } else {
            Err("timed out while reading the request line".into())
        }
    };

    loop {
        let remaining = timeout.saturating_sub(started.elapsed());
        if remaining.is_zero() {
            return timed_out(&received);
        }
        if let Err(e) = stream.limit_next_read(remaining) {
            return Err(e.into());
        }
        let n = match stream.read(&mut chunk) {
            Ok(n) => n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            // プリコネクトや死活監視でログが増えないよう、
            // 1バイトも受け取っていない接続の失敗はエラーとして扱わない
            Err(_) if received.is_empty() => return Ok(RequestLine::Closed),
            Err(e) => return Err(e.into()),
        };
        if started.elapsed() > timeout {
            return timed_out(&received);
        }
        if n == 0 {
            if received.is_empty() {
                return Ok(RequestLine::Closed);
            }
            break;
        }
        received.extend_from_slice(&chunk[..n]);

        if let Some(end) = received.iter().position(|b| *b == b'\n') {
            received.truncate(end);
            break;
        }
        if received.len() > MAX_REQUEST_LINE_BYTES {
            return Ok(RequestLine::TooLong);
        }
    }

    if received.len() > MAX_REQUEST_LINE_BYTES {
        return Ok(RequestLine::TooLong);
    }
    Ok(RequestLine::Line(String::from_utf8_lossy(&received).into_owned()))
}

/// 1本の接続を処理します。1接続につき1リクエストです
///
/// TcpStream固有の操作（書き込みのタイムアウトの設定など）はaccept側で行い、
/// ここは Read + Write と ReadDeadline だけに依存させています
/// （メモリ上のストリームで試験するため）
fn handle_connection<S: Read + Write + ReadDeadline>(
    stream: &mut S,
    log_cache: &log_cache::SharedUsageCache,
    timeout: Duration,
) -> Result<(), error::Error> {
    let response = match read_request_line(stream, timeout)? {
        // 何も送らなかった相手には何も返さない
        RequestLine::Closed => return Ok(()),
        RequestLine::TooLong => Response::empty(StatusCode::BadRequest),
        RequestLine::Line(line) => respond(&line, log_cache),
    };

    write_response(stream, &response)
}

/// 同時に処理する接続数を数えるセマフォ
struct ConnectionLimiter {
    active: Mutex<usize>,
    released: Condvar,
    max: usize,
}

impl ConnectionLimiter {
    fn new(max: usize) -> Arc<Self> {
        Arc::new(ConnectionLimiter {
            active: Mutex::new(0),
            released: Condvar::new(),
            max,
        })
    }

    /// 枠を1つ取ります。上限に達していれば、空くまで待ちます
    fn acquire(self: &Arc<Self>) -> ConnectionSlot {
        // lockを持ったままpanicする箇所は無いが、poisonされていても止まらない
        let mut active = self.active.lock().unwrap_or_else(|e| e.into_inner());
        while *active >= self.max {
            active = self.released.wait(active)
                .unwrap_or_else(|e| e.into_inner());
        }
        *active += 1;

        ConnectionSlot { limiter: Arc::clone(self) }
    }
}

/// 同時接続数の1枠。Dropで必ず返すので、ハンドラがpanicしても枠は戻る
struct ConnectionSlot {
    limiter: Arc<ConnectionLimiter>,
}

impl Drop for ConnectionSlot {
    fn drop(&mut self) {
        let mut active = self.limiter.active.lock()
            .unwrap_or_else(|e| e.into_inner());
        *active -= 1;
        self.limiter.released.notify_one();
    }
}

/// acceptした1本の接続を専用のスレッドに渡します
/// ここで起きたエラーは呼び出し元（acceptのループ）に伝えません
fn dispatch(
    mut stream: std::net::TcpStream,
    slot: ConnectionSlot,
    log_cache: &log_cache::SharedUsageCache,
    timeout: Duration,
) {
    if let Err(e) = stream.set_read_timeout(Some(timeout))
        .and_then(|_| stream.set_write_timeout(Some(timeout)))
    {
        eprintln!("server: failed to set timeouts: {}", e);
        return;
    }

    let log_cache = Arc::clone(log_cache);
    // thread::spawn はスレッドを作れないとpanicするので Builder を使う
    // 失敗した場合はクロージャごと破棄され、接続は閉じられ枠も戻る
    let spawned = std::thread::Builder::new()
        .name("connection".to_string())
        .spawn(move || {
            let _slot = slot;
            if let Err(e) = handle_connection(&mut stream, &log_cache, timeout) {
                eprintln!("server: connection error: {}", e);
            }
        });
    if let Err(e) = spawned {
        eprintln!("server: failed to spawn a connection thread: {}", e);
    }
}

/// 接続を受け付け続けます
/// 1本の接続の失敗でも、accept自体の失敗でもループを抜けません
///
/// 枠を取ってからacceptするので、上限に達している間に来た接続は
/// 断られずにOSのbacklogで待ちます
fn serve(
    listener: std::net::TcpListener,
    log_cache: &log_cache::SharedUsageCache,
    timeout: Duration,
) -> Result<(), error::Error> {
    let limiter = ConnectionLimiter::new(MAX_CONNECTIONS);

    loop {
        let slot = limiter.acquire();
        match listener.accept() {
            Ok((stream, _)) => dispatch(stream, slot, log_cache, timeout),
            Err(e) => {
                eprintln!("server: failed to accept a connection: {}", e);
                drop(slot);
                std::thread::sleep(ACCEPT_RETRY_INTERVAL);
            }
        }
    }
}

pub fn start_server(
    log_cache: &log_cache::SharedUsageCache
) -> Result<(), error::Error> {
    let listener = std::net::TcpListener::bind(LISTEN_ADDR)?;

    serve(listener, log_cache, IO_TIMEOUT)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::log_cache::{
        create_shared_cache, SharedUsageCache, UsagePoint,
    };

    /// 入力を読み切るとEOFになり、書き込みはメモリに溜めるストリーム
    struct MemoryStream {
        input: std::io::Cursor<Vec<u8>>,
        output: Vec<u8>,
    }
    impl Read for MemoryStream {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            self.input.read(buf)
        }
    }
    impl Write for MemoryStream {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.output.write(buf)
        }
        fn flush(&mut self) -> std::io::Result<()> { Ok(()) }
    }

    /// 2026-01-01T00:00:00Z
    const BASE_MS: i64 = 1_767_225_600_000;

    /// i 番目の点の時刻（1秒間隔）
    fn time_ms_at(i: usize) -> i64 {
        BASE_MS + i as i64 * 1000
    }

    /// time_ms_at(i) を API が返す文字列（ミリ秒精度）
    /// 変換の関数を通さずに組み立てる。i は1日分（86400）未満であること
    fn time_at(i: usize) -> String {
        assert!(i < 86_400);
        format!(
            "2026-01-01T{:02}:{:02}:{:02}.000Z",
            i / 3600, (i / 60) % 60, i % 60,
        )
    }

    /// "web" と "db" の2コンテナに npoints 点ずつ入れたキャッシュ
    fn cache_with(npoints: usize, time_of: fn(usize) -> i64) -> SharedUsageCache {
        let cache = create_shared_cache();
        {
            let mut lock = cache.write().unwrap();
            for name in ["web", "db"] {
                for i in 0..npoints {
                    let v = (i % 7) as u32;
                    assert!(lock.insert(name, UsagePoint {
                        time_ms: time_of(i),
                        cpu: Some(v as f32),
                        memory: Some(v as f32 + 0.5),
                        read_kbps: Some(v),
                        write_kbps: None,
                        recv_kbps: None,
                        send_kbps: Some(v),
                    }));
                }
            }
        }
        cache
    }

    fn small_cache() -> SharedUsageCache { cache_with(3, time_ms_at) }

    /// 生のバイト列を送り、書き出された応答をそのまま返す
    fn exchange(cache: &SharedUsageCache, input: &[u8]) -> String {
        let mut stream = MemoryStream {
            input: std::io::Cursor::new(input.to_vec()),
            output: Vec::new(),
        };
        handle_connection(&mut stream, cache, IO_TIMEOUT)
            .expect("handle_connection should not fail on a memory stream");
        String::from_utf8(stream.output).unwrap()
    }

    fn get(cache: &SharedUsageCache, path: &str) -> String {
        exchange(cache, format!("GET {} HTTP/1.1\r\nHost: x\r\n\r\n", path).as_bytes())
    }

    /// 応答を (ステータス行, ヘッダ, 本文) に分ける
    /// ステータス行が1回しか書かれていないことも確かめる
    fn split(response: &str) -> (&str, &str, &str) {
        assert_eq!(response.matches("HTTP/1.1 ").count(), 1, "{}", response);
        let (head, body) = response.split_once("\r\n\r\n").unwrap();
        let (status, headers) = head.split_once("\r\n").unwrap();
        (status, headers, body)
    }

    fn assert_common_headers(headers: &str, body: &str) {
        assert!(headers.contains("Connection: close"), "{}", headers);
        assert!(headers.contains("Content-Type: "), "{}", headers);
        assert!(
            headers.contains(&format!("Content-Length: {}", body.len())),
            "{}", headers,
        );
    }

    fn assert_json_ok(response: &str) -> json::JsonValue {
        let (status, headers, body) = split(response);
        assert_eq!(status, "HTTP/1.1 200 OK");
        assert!(headers.contains("Content-Type: application/json"), "{}", headers);
        assert_common_headers(headers, body);
        json::parse(body).unwrap()
    }

    fn assert_status(response: &str, status_line: &str) {
        let (status, headers, body) = split(response);
        assert_eq!(status, status_line);
        assert_common_headers(headers, body);
    }

    #[test]
    fn containers_are_sorted() {
        let response = get(&small_cache(), "/containers");
        let (_, _, body) = split(&response);
        assert_json_ok(&response);
        assert_eq!(body, r#"["db","web"]"#);
    }

    #[test]
    fn cpu_and_memory_return_series() {
        let cache = small_cache();
        for (kind, expected) in [("cpu", 1.0), ("memory", 1.5)] {
            let parsed = assert_json_ok(&get(&cache, &format!("/containers/web/{}", kind)));
            assert_eq!(parsed.len(), 3);
            assert_eq!(parsed[1]["time"], time_at(1).as_str());
            assert_eq!(parsed[1]["percentage"], expected);
            // 要素が持つのは time と percentage だけ
            assert_eq!(parsed[1].len(), 2);
        }
    }

    #[test]
    fn response_bodies_keep_their_exact_shape() {
        // 時刻はミリ秒精度の ISO 8601 (UTC、Z 終端)
        // 小数は2桁、整数はそのまま、欠損は null
        let cache = create_shared_cache();
        {
            let mut lock = cache.write().unwrap();
            lock.insert("web", UsagePoint {
                time_ms: BASE_MS + 10_023,
                cpu: Some(0.125),
                memory: Some(3.64),
                read_kbps: Some(0),
                write_kbps: Some(1234),
                recv_kbps: None,
                send_kbps: Some(5),
            });
            lock.insert("web", UsagePoint {
                time_ms: BASE_MS + 20_000,
                cpu: None,
                memory: Some(100.0),
                read_kbps: None,
                write_kbps: None,
                recv_kbps: Some(7),
                send_kbps: None,
            });
        }
        let body_of = |path: &str| {
            let response = get(&cache, path);
            assert_json_ok(&response);
            split(&response).2.to_string()
        };
        assert_eq!(
            body_of("/containers/web/cpu"),
            concat!(
                r#"[{"time":"2026-01-01T00:00:10.023Z","percentage":0.12},"#,
                r#"{"time":"2026-01-01T00:00:20.000Z","percentage":null}]"#,
            ),
        );
        assert_eq!(
            body_of("/containers/web/memory"),
            concat!(
                r#"[{"time":"2026-01-01T00:00:10.023Z","percentage":3.64},"#,
                r#"{"time":"2026-01-01T00:00:20.000Z","percentage":100.00}]"#,
            ),
        );
        let io = concat!(
            r#"[{"time":"2026-01-01T00:00:10.023Z","readkBps":0,"writekBps":1234},"#,
            r#"{"time":"2026-01-01T00:00:20.000Z","readkBps":null,"writekBps":null}]"#,
        );
        assert_eq!(body_of("/containers/web/io/read"), io);
        assert_eq!(body_of("/containers/web/io/write"), io);
        let net = concat!(
            r#"[{"time":"2026-01-01T00:00:10.023Z","recvkBps":null,"sendkBps":5},"#,
            r#"{"time":"2026-01-01T00:00:20.000Z","recvkBps":7,"sendkBps":null}]"#,
        );
        assert_eq!(body_of("/containers/web/net/recv"), net);
        assert_eq!(body_of("/containers/web/net/send"), net);
        assert_eq!(body_of("/containers"), r#"["web"]"#);
    }

    #[test]
    fn io_read_and_write_return_series() {
        let cache = small_cache();
        for kind in ["read", "write"] {
            let parsed = assert_json_ok(&get(&cache, &format!("/containers/web/io/{}", kind)));
            assert_eq!(parsed.len(), 3);
            assert_eq!(parsed[2]["time"], time_at(2).as_str());
            assert_eq!(parsed[2]["readkBps"], 2.0);
            assert!(parsed[2]["writekBps"].is_null());
            assert_eq!(parsed[2].len(), 3);
        }
    }

    #[test]
    fn net_recv_and_send_return_series() {
        let cache = small_cache();
        for kind in ["recv", "send"] {
            let parsed = assert_json_ok(&get(&cache, &format!("/containers/db/net/{}", kind)));
            assert_eq!(parsed.len(), 3);
            assert_eq!(parsed[2]["time"], time_at(2).as_str());
            assert!(parsed[2]["recvkBps"].is_null());
            assert_eq!(parsed[2]["sendkBps"], 2.0);
            assert_eq!(parsed[2].len(), 3);
        }
    }

    #[test]
    fn long_series_is_downsampled() {
        let cache = cache_with(2000, time_ms_at);
        let parsed = assert_json_ok(&get(&cache, "/containers/web/cpu"));
        assert_eq!(parsed.len(), 512);
        assert_eq!(parsed[0]["time"], time_at(0).as_str());
        assert_eq!(parsed[parsed.len() - 1]["time"], time_at(1999).as_str());
    }

    #[test]
    fn unknown_targets_are_404() {
        let cache = small_cache();
        for path in [
            "/",
            "/unknown",
            "/containers/nobody/cpu",
            "/containers/nobody/io/read",
            "/containers/nobody/net/send",
            "/containers/web",
            "/containers/web/disk",
            "/containers/web/io/unknown",
            "/containers/web/net/unknown",
            "/containers/web/cpu/extra",
            "/containers?limit=1",
            "/containers/web/cpu?nsample=10",
        ] {
            assert_status(&get(&cache, path), "HTTP/1.1 404 Not Found");
        }
    }

    #[test]
    fn non_get_is_405() {
        let cache = small_cache();
        for method in ["POST", "HEAD", "DELETE", "get"] {
            let response = exchange(
                &cache,
                format!("{} /containers HTTP/1.1\r\n\r\n", method).as_bytes(),
            );
            assert_status(&response, "HTTP/1.1 405 Method Not Allowed");
        }
    }

    #[test]
    fn empty_input_gets_no_response() {
        assert_eq!(exchange(&small_cache(), b""), "");
    }

    impl ReadDeadline for MemoryStream {
        fn limit_next_read(&mut self, _: Duration) -> std::io::Result<()> { Ok(()) }
    }

    /// 先に prefix を返し、その後のreadはタイムアウトとして失敗するストリーム
    struct StallingStream {
        prefix: std::io::Cursor<Vec<u8>>,
        output: Vec<u8>,
    }
    impl Read for StallingStream {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            match self.prefix.read(buf)? {
                0 => Err(std::io::ErrorKind::WouldBlock.into()),
                n => Ok(n),
            }
        }
    }
    impl Write for StallingStream {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.output.write(buf)
        }
        fn flush(&mut self) -> std::io::Result<()> { Ok(()) }
    }

    impl ReadDeadline for StallingStream {
        fn limit_next_read(&mut self, _: Duration) -> std::io::Result<()> { Ok(()) }
    }

    /// 1回目のreadで first を返し、2回目のreadは delay だけ待ってから rest を返す
    /// 待つ時間は limit_next_read で渡された上限で打ち切り、タイムアウトとして失敗する
    struct SlowStream {
        chunks: Vec<Vec<u8>>,
        delay: Duration,
        limit: Option<Duration>,
        limits_seen: Vec<Duration>,
        output: Vec<u8>,
    }
    impl SlowStream {
        fn new(first: &[u8], rest: &[u8], delay: Duration) -> Self {
            SlowStream {
                chunks: vec![rest.to_vec(), first.to_vec()],
                delay,
                limit: None,
                limits_seen: Vec::new(),
                output: Vec::new(),
            }
        }
    }
    impl Read for SlowStream {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            if self.chunks.len() == 1 {
                match self.limit {
                    Some(limit) if limit < self.delay => {
                        std::thread::sleep(limit);
                        return Err(std::io::ErrorKind::WouldBlock.into());
                    }
                    _ => std::thread::sleep(self.delay),
                }
            }
            match self.chunks.pop() {
                Some(chunk) => {
                    buf[..chunk.len()].copy_from_slice(&chunk);
                    Ok(chunk.len())
                }
                None => Ok(0),
            }
        }
    }
    impl Write for SlowStream {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.output.write(buf)
        }
        fn flush(&mut self) -> std::io::Result<()> { Ok(()) }
    }
    impl ReadDeadline for SlowStream {
        fn limit_next_read(&mut self, timeout: Duration) -> std::io::Result<()> {
            self.limit = Some(timeout);
            self.limits_seen.push(timeout);
            Ok(())
        }
    }

    #[test]
    fn request_line_deadline_is_a_total() {
        let cache = small_cache();
        let timeout = Duration::from_millis(100);

        // 期限内に残りが届けば処理する
        let mut in_time = SlowStream::new(
            b"GET /cont", b"ainers HTTP/1.1\r\n\r\n", Duration::from_millis(10),
        );
        handle_connection(&mut in_time, &cache, timeout).unwrap();
        assert_json_ok(&String::from_utf8(in_time.output).unwrap());

        // 改行が届くのが期限の後なら、処理せずに打ち切る
        // 2回目のreadで待つのは残り時間までで、合計が期限を大きく超えない
        let mut late = SlowStream::new(
            b"GET /cont", b"ainers HTTP/1.1\r\n\r\n", Duration::from_millis(1000),
        );
        let started = Instant::now();
        assert!(handle_connection(&mut late, &cache, timeout).is_err());
        assert!(late.output.is_empty());
        assert!(started.elapsed() < Duration::from_millis(700), "{:?}", started.elapsed());
        assert!(late.limits_seen.iter().all(|limit| *limit <= timeout));
        assert!(late.limits_seen.windows(2).all(|w| w[1] <= w[0]));

        // 上限を渡せないストリームでも、期限を過ぎて届いた改行は受け付けない
        struct Unlimited(SlowStream);
        impl Read for Unlimited {
            fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
                self.0.read(buf)
            }
        }
        impl Write for Unlimited {
            fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
                self.0.write(buf)
            }
            fn flush(&mut self) -> std::io::Result<()> { Ok(()) }
        }
        impl ReadDeadline for Unlimited {
            fn limit_next_read(&mut self, _: Duration) -> std::io::Result<()> { Ok(()) }
        }
        let mut unlimited = Unlimited(SlowStream::new(
            b"GET /cont", b"ainers HTTP/1.1\r\n\r\n", Duration::from_millis(150),
        ));
        assert!(handle_connection(&mut unlimited, &cache, timeout).is_err());
        assert!(unlimited.0.output.is_empty());
    }

    #[test]
    fn silent_connection_is_not_reported_as_an_error() {
        let cache = small_cache();
        // 1バイトも受け取らずに失敗した接続は、エラーにせず何も返さない
        let mut silent = StallingStream {
            prefix: std::io::Cursor::new(Vec::new()),
            output: Vec::new(),
        };
        assert!(handle_connection(&mut silent, &cache, IO_TIMEOUT).is_ok());
        assert!(silent.output.is_empty());

        // 途中まで受け取ってから失敗した接続は、エラーとして報告する
        let mut partial = StallingStream {
            prefix: std::io::Cursor::new(b"GET /cont".to_vec()),
            output: Vec::new(),
        };
        assert!(handle_connection(&mut partial, &cache, IO_TIMEOUT).is_err());
        assert!(partial.output.is_empty());
    }

    #[test]
    fn malformed_request_line_is_400() {
        let cache = small_cache();
        let inputs: [&[u8]; 7] = [
            b"garbage\r\n\r\n",
            b"garbage",
            b"\r\n\r\n",
            b"GET /containers\r\n\r\n",
            b"GET /containers HTTP/1.1 extra\r\n\r\n",
            b"GET containers HTTP/1.1\r\n\r\n",
            b"\xff\xfe\x00\x01\r\n",
        ];
        for input in inputs {
            assert_status(&exchange(&cache, input), "HTTP/1.1 400 Bad Request");
        }
    }

    #[test]
    fn too_long_request_line_is_400() {
        let cache = small_cache();
        let long_path = format!("/{}", "a".repeat(MAX_REQUEST_LINE_BYTES * 4));
        assert_status(&get(&cache, &long_path), "HTTP/1.1 400 Bad Request");
        // 改行が来ないまま上限を超えた場合
        let no_newline = "a".repeat(MAX_REQUEST_LINE_BYTES * 4);
        assert_status(
            &exchange(&cache, no_newline.as_bytes()),
            "HTTP/1.1 400 Bad Request",
        );
    }

    #[test]
    fn request_line_within_limit_is_routed() {
        let cache = small_cache();
        // 上限ちょうどのリクエスト行は400ではなく通常の経路判定に進む
        let fixed = "GET / HTTP/1.1\r".len();
        let path = format!("/{}", "a".repeat(MAX_REQUEST_LINE_BYTES - fixed));
        assert_status(&get(&cache, &path), "HTTP/1.1 404 Not Found");
        // 1バイトでも超えれば400
        let path = format!("/{}", "a".repeat(MAX_REQUEST_LINE_BYTES - fixed + 1));
        assert_status(&get(&cache, &path), "HTTP/1.1 400 Bad Request");
    }

    #[test]
    fn request_line_without_headers_is_accepted() {
        let response = exchange(&small_cache(), b"GET /containers HTTP/1.0\n");
        assert_json_ok(&response);
    }

    #[test]
    fn series_across_days_is_served_on_every_route() {
        // 時刻は数値で持つので、解釈できない時刻はキャッシュに入りえない
        // 代わりに、日付と年をまたぐ系列（ダウンサンプリングが走る点数）を
        // 全経路で返せることを確かめる
        fn around_new_year(i: usize) -> i64 {
            // 2025-12-31T23:50:00Z から10秒間隔
            BASE_MS - 600_000 + i as i64 * 10_000
        }
        let cache = cache_with(2000, around_new_year);
        for path in [
            "/containers/web/cpu",
            "/containers/web/memory",
            "/containers/web/io/read",
            "/containers/web/io/write",
            "/containers/web/net/recv",
            "/containers/web/net/send",
        ] {
            let parsed = assert_json_ok(&get(&cache, path));
            assert_eq!(parsed.len(), 512);
            assert_eq!(parsed[0]["time"], "2025-12-31T23:50:00.000Z");
            // 1999 × 10秒 = 5時間33分10秒後
            assert_eq!(parsed[511]["time"], "2026-01-01T05:23:10.000Z");
            // 文字列として昇順（= 時刻として昇順）
            let times: Vec<&str> = parsed.members()
                .map(|p| p["time"].as_str().unwrap())
                .collect();
            assert!(times.windows(2).all(|w| w[0] < w[1]));
        }
    }

    /// write lockを持ったままpanicさせて、lockをpoisonする
    fn poison(cache: &SharedUsageCache) {
        let cache = Arc::clone(cache);
        let _ = std::thread::spawn(move || {
            let _lock = cache.write().unwrap();
            panic!("poisoning the cache on purpose (expected in this test)");
        }).join();
    }

    #[test]
    fn internal_error_body_is_fixed() {
        let cache = small_cache();
        poison(&cache);
        assert!(cache.read().is_err());

        for path in ["/containers", "/containers/web/cpu", "/unknown"] {
            let response = get(&cache, path);
            let (status, headers, body) = split(&response);
            assert_eq!(status, "HTTP/1.1 500 Internal Server Error");
            assert_common_headers(headers, body);
            assert_eq!(body, INTERNAL_ERROR_BODY);
            assert!(!response.to_lowercase().contains("poison"), "{}", response);
            assert!(!response.contains("OtherError"), "{}", response);
        }
    }

    fn active_count(limiter: &Arc<ConnectionLimiter>) -> usize {
        *limiter.active.lock().unwrap()
    }

    #[test]
    fn slot_is_returned_on_drop_and_on_panic() {
        let limiter = ConnectionLimiter::new(3);
        let mut slots: Vec<ConnectionSlot> =
            (0..3).map(|_| limiter.acquire()).collect();
        assert_eq!(active_count(&limiter), 3);

        let slot = slots.pop().unwrap();
        let _ = std::thread::spawn(move || {
            let _slot = slot;
            panic!("panicking while holding a slot (expected in this test)");
        }).join();
        assert_eq!(active_count(&limiter), 2);

        drop(slots);
        assert_eq!(active_count(&limiter), 0);
    }

    #[test]
    fn limiter_never_exceeds_max_and_wakes_waiters() {
        use std::sync::atomic::{ AtomicUsize, Ordering };

        const MAX: usize = 4;
        let limiter = ConnectionLimiter::new(MAX);
        let running = Arc::new(AtomicUsize::new(0));
        let peak = Arc::new(AtomicUsize::new(0));

        let workers: Vec<_> = (0..40).map(|_| {
            let limiter = Arc::clone(&limiter);
            let running = Arc::clone(&running);
            let peak = Arc::clone(&peak);
            std::thread::spawn(move || {
                let _slot = limiter.acquire();
                let now = running.fetch_add(1, Ordering::SeqCst) + 1;
                peak.fetch_max(now, Ordering::SeqCst);
                std::thread::sleep(Duration::from_millis(5));
                running.fetch_sub(1, Ordering::SeqCst);
            })
        }).collect();
        // 待たされた側も全員起こされて終わる（起こし損ねがあればここで止まる）
        for worker in workers { worker.join().unwrap(); }

        assert_eq!(peak.load(Ordering::SeqCst), MAX);
        assert_eq!(active_count(&limiter), 0);
    }

    // ---- ここから実際のTCPを使う試験 ----
    // 待ち受けは必ず 127.0.0.1:0（空きポートの自動割り当て）にする

    const TEST_TIMEOUT: Duration = Duration::from_millis(300);

    /// serve を別スレッドで動かし、接続先を返す
    /// スレッドは止める手段が無いので、試験プロセスの終了まで残る
    fn spawn_server(timeout: Duration) -> std::net::SocketAddr {
        spawn_server_with(small_cache(), timeout)
    }

    fn spawn_server_with(
        cache: SharedUsageCache,
        timeout: Duration,
    ) -> std::net::SocketAddr {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        std::thread::spawn(move || serve(listener, &cache, timeout));
        addr
    }

    fn connect(addr: std::net::SocketAddr) -> std::net::TcpStream {
        let stream = std::net::TcpStream::connect(addr).unwrap();
        stream.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
        stream
    }

    fn try_tcp_exchange(
        addr: std::net::SocketAddr,
        input: &[u8],
    ) -> std::io::Result<String> {
        let mut stream = connect(addr);
        stream.write_all(input)?;
        let mut response = String::new();
        stream.read_to_string(&mut response)?;
        Ok(response)
    }

    fn tcp_exchange(addr: std::net::SocketAddr, input: &[u8]) -> String {
        try_tcp_exchange(addr, input).unwrap()
    }

    fn tcp_get_containers(addr: std::net::SocketAddr) -> String {
        tcp_exchange(addr, b"GET /containers HTTP/1.1\r\nHost: x\r\n\r\n")
    }

    #[test]
    fn server_survives_bad_connections() {
        let addr = spawn_server(IO_TIMEOUT);

        // 接続してすぐ閉じる
        drop(connect(addr));
        assert_json_ok(&tcp_get_containers(addr));

        // 不正なリクエスト行
        assert_status(
            &tcp_exchange(addr, b"garbage\r\n\r\n"),
            "HTTP/1.1 400 Bad Request",
        );
        assert_json_ok(&tcp_get_containers(addr));
    }

    #[test]
    fn idle_connection_does_not_block_others() {
        let addr = spawn_server(IO_TIMEOUT);

        let _idle = connect(addr);
        let started = Instant::now();
        assert_json_ok(&tcp_get_containers(addr));
        assert!(started.elapsed() < IO_TIMEOUT, "{:?}", started.elapsed());
    }

    #[test]
    fn idle_connection_is_closed_after_timeout() {
        let addr = spawn_server(TEST_TIMEOUT);

        let mut idle = connect(addr);
        let mut response = String::new();
        // サーバ側がタイムアウトで閉じるので、何も返らずEOFになる
        idle.read_to_string(&mut response).unwrap();
        assert_eq!(response, "");
        assert_json_ok(&tcp_get_containers(addr));
    }

    #[test]
    fn burst_over_the_limit_is_all_served() {
        // webは1回の表示で 6N 本を同時に発射する（Nはコンテナ数）
        // 上限を超えた分も断られず、全部200になること
        const NCONTAINERS: usize = 10;
        let cache = create_shared_cache();
        {
            let mut lock = cache.write().unwrap();
            for c in 0..NCONTAINERS {
                for i in 0..crate::log_cache::MAX_LOG_LENGTH {
                    let name = format!("c{}", c);
                    let v = (i % 13) as u32;
                    assert!(lock.insert(&name, UsagePoint {
                        time_ms: time_ms_at(i),
                        cpu: Some(v as f32),
                        memory: Some(v as f32),
                        read_kbps: Some(v),
                        write_kbps: Some(1),
                        recv_kbps: Some(v),
                        send_kbps: Some(1),
                    }));
                }
            }
        }
        let addr = spawn_server_with(cache, IO_TIMEOUT);

        let mut clients = Vec::new();
        for c in 0..NCONTAINERS {
            for kind in ["cpu", "memory", "io/read", "io/write", "net/recv", "net/send"] {
                let request = format!("GET /containers/c{}/{} HTTP/1.1\r\n\r\n", c, kind);
                clients.push(std::thread::spawn(move || {
                    try_tcp_exchange(addr, request.as_bytes())
                }));
            }
        }
        assert!(clients.len() > MAX_CONNECTIONS * 3);

        for client in clients {
            let response = client.join().unwrap()
                .expect("no connection should be refused or reset");
            let parsed = assert_json_ok(&response);
            assert_eq!(parsed.len(), 512);
        }
    }

    #[test]
    fn connection_over_the_limit_waits_for_a_slot() {
        let addr = spawn_server(TEST_TIMEOUT);

        // 上限まで、何も送らない接続で埋める
        // acceptは接続した順なので、次の1本は必ず枠待ちになる
        let _idles: Vec<std::net::TcpStream> =
            (0..MAX_CONNECTIONS).map(|_| connect(addr)).collect();
        let started = Instant::now();

        // 断られず、無言の接続がタイムアウトして枠が空いてから応答が来る
        // (16本を超えて同時に処理していれば、待たずに返ってきてしまう)
        assert_json_ok(&tcp_get_containers(addr));
        assert!(started.elapsed() >= TEST_TIMEOUT / 2, "{:?}", started.elapsed());
    }

    #[test]
    fn late_request_line_is_not_served_over_tcp() {
        let addr = spawn_server(TEST_TIMEOUT);

        // 期限の少し前に一部だけ送り、期限を過ぎてから残りを送る
        // (readごとのタイムアウトだけだと、ここで待ち時間が延びて処理されてしまう)
        let mut stream = connect(addr);
        std::thread::sleep(TEST_TIMEOUT * 2 / 3);
        stream.write_all(b"GET /cont").unwrap();
        std::thread::sleep(TEST_TIMEOUT * 2 / 3);
        // サーバ側はすでに閉じているので、書き込みも読み取りも失敗しうる
        let _ = stream.write_all(b"ainers HTTP/1.1\r\n\r\n");
        let mut response = String::new();
        let _ = stream.read_to_string(&mut response);
        assert_eq!(response, "");

        assert_json_ok(&tcp_get_containers(addr));
    }
}
