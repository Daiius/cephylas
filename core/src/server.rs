
use std::io::{ Read, Write };
use std::sync::{ Arc, Condvar, Mutex };
use std::time::{ Duration, Instant };

use super::error;
use super::log_cache;

/// 待ち受けるアドレス
const LISTEN_ADDR: &str = "0.0.0.0:7878";
/// 同時に処理する接続数の上限。超えた接続は枠が空くまで待たせる
const MAX_CONNECTIONS: usize = 16;
/// 1回の読み書きの待ち時間の上限
/// リクエスト行とヘッダ（空行まで）を読み切るまでの、合計の上限も兼ねる
const IO_TIMEOUT: Duration = Duration::from_secs(5);
/// リクエスト行（1行目）の長さの上限。超えたら400を返す
const MAX_REQUEST_LINE_BYTES: usize = 1024;
/// リクエスト行とヘッダを合わせた長さ（空行まで）の上限。超えたら431を返す
const MAX_REQUEST_HEAD_BYTES: usize = 16 * 1024;
/// 応答を書き終えたあとに読み捨てる入力の上限。達したら、読み残しがあっても閉じる
const MAX_LINGER_BYTES: usize = 64 * 1024;
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
    RequestHeaderFieldsTooLarge,
    InternalServerError,
}

impl StatusCode {
    fn status_line(self) -> &'static str {
        match self {
            StatusCode::Ok => "200 OK",
            StatusCode::BadRequest => "400 Bad Request",
            StatusCode::NotFound => "404 Not Found",
            StatusCode::MethodNotAllowed => "405 Method Not Allowed",
            StatusCode::RequestHeaderFieldsTooLarge =>
                "431 Request Header Fields Too Large",
            StatusCode::InternalServerError => "500 Internal Server Error",
        }
    }
}

/// /usages/{kind} の本文として、同時に保持してよいバイト数の合計
///
/// 全コンテナ分の本文は1本で数百kB〜数MBになり、送り終えるまで接続ごとに持ち続ける
/// 同時接続の上限（16本）まで並ぶとメモリの上限を超えうるので、合計で抑える
/// 超える分は断らず、先の応答が送り終わるまで待たせる
const MAX_BULK_BODY_BYTES: usize = 8 * 1024 * 1024;

/// プロセス全体で1つの、/usages/{kind} の本文の枠
static BULK_BODY_BUDGET: BodyBudget = BodyBudget::new(MAX_BULK_BODY_BYTES);

/// 同時に保持する本文のバイト数を数えるセマフォ
struct BodyBudget {
    used: Mutex<usize>,
    released: Condvar,
    max: usize,
}

impl BodyBudget {
    const fn new(max: usize) -> Self {
        BodyBudget {
            used: Mutex::new(0),
            released: Condvar::new(),
            max,
        }
    }

    /// bytes 分の枠を取ります。合計が上限を超えるなら、空くまで待ちます
    ///
    /// ほかに誰も枠を持っていなければ、上限より大きくても通します
    /// (1本で上限を超える本文を、永久に返せなくならないようにするため)
    /// 待つことがあるので、キャッシュのlockを持ったまま呼ばないこと
    fn reserve(&'static self, bytes: usize) -> BodyReservation {
        // lockを持ったままpanicする箇所は無いが、poisonされていても止まらない
        let mut used = self.used.lock().unwrap_or_else(|e| e.into_inner());
        while *used > 0 && used.saturating_add(bytes) > self.max {
            used = self.released.wait(used)
                .unwrap_or_else(|e| e.into_inner());
        }
        *used = used.saturating_add(bytes);

        BodyReservation { budget: self, bytes }
    }
}

/// 本文の枠。Dropで必ず返すので、ハンドラがpanicしても枠は戻る
struct BodyReservation {
    budget: &'static BodyBudget,
    bytes: usize,
}

// 取った枠の大きさは変えられない（増やす操作を持たせない）
// 枠を使う側は、取った大きさを超えない本文しか作らないこと

impl Drop for BodyReservation {
    fn drop(&mut self) {
        let mut used = self.budget.used.lock()
            .unwrap_or_else(|e| e.into_inner());
        *used = used.saturating_sub(self.bytes);
        // 待っている側の大きさはまちまちなので、全員を起こして判定し直させる
        self.budget.released.notify_all();
    }
}

/// 書き出す前の応答。経路の判定はこれを返すだけにして、
/// ストリームへの書き込みは write_response の1箇所に集める
struct Response {
    status: StatusCode,
    content_type: &'static str,
    body: String,
    /// 本文の枠。応答を送り終えて破棄するまで持ち続ける
    _reservation: Option<BodyReservation>,
}

impl Response {
    fn json(body: String) -> Self {
        Response {
            status: StatusCode::Ok,
            content_type: "application/json",
            body,
            _reservation: None,
        }
    }
    /// 本文の枠を持ったままのjson応答
    fn reserved_json(body: String, reservation: BodyReservation) -> Self {
        Response {
            _reservation: Some(reservation),
            ..Response::json(body)
        }
    }
    /// 本文を持たないエラー応答
    fn empty(status: StatusCode) -> Self {
        Response {
            status,
            content_type: "text/plain; charset=utf-8",
            body: String::new(),
            _reservation: None,
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
    let head = format!(
        "HTTP/1.1 {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        response.status.status_line(),
        response.content_type,
        body_bytes.len(),
    );
    // ヘッダと本文を1つのバッファにまとめない
    // (本文が数MBになる応答で、送る間だけ本文の複製を持つことになるため)
    stream.write_all(head.as_bytes())?;
    stream.write_all(body_bytes)?;
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
///
/// 名前は /usages/{kind} のキーと同じ規則でエスケープします
/// (ログから復元した名前に引用符などがあっても、本文が壊れないように)
fn container_names_json(cache: &log_cache::UsageCache) -> String {
    let mut out = String::from("[");
    for (i, name) in cache.container_names().iter().enumerate() {
        if i > 0 { out.push(','); }
        // String への書き込みは失敗しない
        let _ = write_json_string(&mut out, name);
    }
    out.push(']');

    out
}

/// 枠を取り直して本文を組み立てる回数の上限
/// (枠を待つ間にキャッシュが伸びると取り直す。それが続いたら500にして終わる)
const MAX_BULK_BUILD_ATTEMPTS: usize = 8;

/// 書き込まれたバイト数だけを数える書き出し先
struct ByteCounter(usize);

impl std::fmt::Write for ByteCounter {
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        self.0 += s.len();
        Ok(())
    }
}

/// 決めた長さを超える書き込みを受け付けない String
///
/// 最初に limit バイトを確保し、それを超える書き込みは失敗させます
/// 確保した領域が伸びることはありません
struct CappedString {
    out: String,
    limit: usize,
}

impl CappedString {
    fn new(limit: usize) -> Self {
        CappedString { out: String::with_capacity(limit), limit }
    }
}

impl std::fmt::Write for CappedString {
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        if s.len() > self.limit - self.out.len() {
            return Err(std::fmt::Error);
        }
        self.out.push_str(s);
        Ok(())
    }
}

/// jsonの文字列として書き出します（引用符つき）
/// Dockerのコンテナ名は英数字と _ . - だけだが、ログから復元した名前も通るので
/// 本文全体が壊れないようエスケープする
fn write_json_string<W: std::fmt::Write>(
    out: &mut W,
    value: &str,
) -> std::fmt::Result {
    out.write_char('"')?;
    for c in value.chars() {
        match c {
            '"' => out.write_str(r#"\""#)?,
            '\\' => out.write_str(r"\\")?,
            c if (c as u32) < 0x20 => write!(out, "\\u{:04x}", c as u32)?,
            c => out.write_char(c)?,
        }
    }
    out.write_char('"')
}

/// 全コンテナ分の系列を、種別ごとに1つのjsonオブジェクトとして書き出します
///
/// 形は コンテナ名 → 系列名 → { "t": [ミリ秒, ...], "v": [値, ...] } です
/// コンテナ名はアルファベット順に並べます
/// 間引きは系列ごとに独立して行うので、同じコンテナでも系列によって t が変わります
/// 間引いたあと、データが途切れていた箇所に値が null の点（目印）を1つ挟みます
/// (log_cache::UsageCache::downsample_with_gaps。コンテナ別のAPIには入れません)
/// 書き出し先を差し替えられるので、長さだけを数えることも、本文を作ることもできます
/// (同じキャッシュに対しては、どちらも同じバイト列になる)
fn write_usages_json<W: std::fmt::Write>(
    out: &mut W,
    cache: &log_cache::UsageCache,
    kind: log_cache::UsageKind,
) -> std::fmt::Result {
    let option = log_cache::DownsampleOption::default();

    out.write_char('{')?;
    for (i, name) in cache.container_names().iter().enumerate() {
        if i > 0 { out.write_char(',')?; }
        write_json_string(out, name)?;
        out.write_str(":{")?;
        for (j, (key, series)) in kind.series().iter().enumerate() {
            // 名前は直前に一覧から取ったものなので、必ず見つかる
            // 途切れの目印もここで決まる。長さを数える回と書く回で同じ並びになる
            let samples = cache.downsample_with_gaps(name, &option, *series)
                .unwrap_or_default();
            if j > 0 { out.write_char(',')?; }
            write!(out, "\"{}\":{{\"t\":[", key)?;
            for (k, sample) in samples.iter().enumerate() {
                if k > 0 { out.write_char(',')?; }
                write!(out, "{}", sample.time_ms())?;
            }
            out.write_str("],\"v\":[")?;
            for (k, sample) in samples.iter().enumerate() {
                if k > 0 { out.write_char(',')?; }
                match sample {
                    log_cache::Sample::Point(point) => match series.raw_value(point) {
                        // NaN や無限大はjsonに書けない。1つで本文全体が壊れるので欠損にする
                        Some(v) if v.is_finite() => write!(out, "{:.2}", v)?,
                        _ => out.write_str("null")?,
                    },
                    // 途切れの目印。値の無い点として出し、線をここで切る
                    log_cache::Sample::Gap(_) => out.write_str("null")?,
                }
            }
            out.write_str("]}")?;
        }
        out.write_char('}')?;
    }
    out.write_char('}')
}

/// /usages/{kind} の本文の正確な長さ（バイト数）。本文は作りません
fn usages_json_len(
    cache: &log_cache::UsageCache,
    kind: log_cache::UsageKind,
) -> usize {
    let mut counter = ByteCounter(0);
    // ByteCounter への書き込みは失敗しない
    let _ = write_usages_json(&mut counter, cache, kind);

    counter.0
}

/// /usages/{kind} の本文を、limit バイトちょうどの領域に組み立てます
/// limit に収まらなければ None を返します（領域は伸ばさずに破棄する）
fn usages_json_within(
    cache: &log_cache::UsageCache,
    kind: log_cache::UsageKind,
    limit: usize,
) -> Option<String> {
    let mut capped = CappedString::new(limit);
    write_usages_json(&mut capped, cache, kind).ok()?;

    Some(capped.out)
}

/// /usages/{kind} の応答を、本文の枠を取ってから組み立てます
///
/// needed は呼び出し側がlockを持っている間に数えた、本文の正確な長さです
/// その長さの枠を取り、その長さを超えない領域にだけ本文を書きます
/// 枠を待つ間はキャッシュのlockを持ちません（loggerのwrite lockを待たせないため）
/// 待つ間にキャッシュが変わって本文が長くなっていたら、領域は伸ばさず、
/// 枠を返してから、数え直した長さで取り直します
/// 枠は応答に持たせ、送り終えて破棄されるまで返しません
fn usages_response(
    log_cache: &log_cache::SharedUsageCache,
    kind: log_cache::UsageKind,
    mut needed: usize,
    budget: &'static BodyBudget,
) -> Result<Response, error::Error> {
    for _ in 0..MAX_BULK_BUILD_ATTEMPTS {
        let reservation = budget.reserve(needed);
        let cache = log_cache.read().map_err(|e| e.to_string())?;
        if let Some(body) = usages_json_within(&cache, kind, needed) {
            return Ok(Response::reserved_json(body, reservation));
        }
        needed = usages_json_len(&cache, kind);
        // ここで cache（lock）と reservation（枠）を手放してから、取り直す
    }

    Err("the cache kept growing while waiting for the body budget".into())
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
        ["usages", kind] => match log_cache::UsageKind::from_path(kind) {
            Some(kind) => {
                let needed = usages_json_len(&cache, kind);
                // 本文の枠を待つことがあるので、lockを手放してから取りにいく
                drop(cache);
                return usages_response(log_cache, kind, needed, &BULK_BODY_BUDGET);
            }
            None => None,
        },
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
/// リクエスト行とヘッダの読み取りを合計時間で打ち切るために使います
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

/// リクエストの先頭（リクエスト行とヘッダ）の読み取り結果
enum RequestHead {
    /// 1バイトも受け取らずに終わった
    /// (相手が閉じた、無言のままタイムアウトした、など)
    Closed,
    /// リクエスト行が MAX_REQUEST_LINE_BYTES を超えた
    LineTooLong,
    /// 空行が来ないまま MAX_REQUEST_HEAD_BYTES を超えた
    TooLarge,
    /// リクエスト行。ヘッダは空行（または相手が送信を終えたところ）まで読み捨ててある
    Line(String),
}

/// ヘッダの終わり（空行）を探す状態。届いたバイトを先頭から順に渡します
///
/// 空行は \r\n\r\n のほか、\n\n も受け付けます
/// リクエスト行の直後の空行（ヘッダが1つも無い場合）も終わりとして扱います
#[derive(Clone, Copy, Debug, PartialEq)]
enum BlankLineScan {
    /// 行の途中
    InLine,
    /// 改行の直後
    AfterNewline,
    /// 改行の直後に \r が来たところ
    AfterNewlineCr,
    /// 空行まで読んだ
    Found,
}

impl BlankLineScan {
    fn feed(self, byte: u8) -> Self {
        match (self, byte) {
            (BlankLineScan::Found, _) => BlankLineScan::Found,
            (BlankLineScan::AfterNewline | BlankLineScan::AfterNewlineCr, b'\n') =>
                BlankLineScan::Found,
            (BlankLineScan::AfterNewline, b'\r') => BlankLineScan::AfterNewlineCr,
            (_, b'\n') => BlankLineScan::AfterNewline,
            _ => BlankLineScan::InLine,
        }
    }
}

/// リクエスト行を読み、続くヘッダを空行まで読み捨てます。ヘッダは解釈しません
///
/// ヘッダを読み残したまま応答して閉じると、OSが接続をリセットして、
/// 送りきれていない応答を捨てることがあります。それを避けるために読み切ります
/// 本文（ボディ）は解釈せず、届くのを待つこともしません。空行より先のバイトは、
/// 同じreadで届いた分を除いて受信バッファに残します
/// (応答を書いたあとで discard_until_closed が読み捨てます)
///
/// - リクエスト行が長すぎても、形式が不正でも、空行までは読み捨てます
/// - 空行までの合計が MAX_REQUEST_HEAD_BYTES を超えたら、そこで読むのをやめます
/// - 相手が送信を終えた（EOF）場合は、そこまでを受け取ったものとして扱います
///
/// timeout は、started（接続の処理を始めた時点）から、リクエスト行とヘッダを
/// 読み切るまでの合計の上限です
/// 少しずつ送り続けて接続を占有されないよう、readのたびに残り時間だけ待ち、
/// 期限を過ぎてから届いた分は（空行やEOFであっても）受け付けません
fn read_request_head<R: Read + ReadDeadline>(
    stream: &mut R,
    started: Instant,
    timeout: Duration,
) -> Result<RequestHead, error::Error> {
    // これまでに受け取ったバイト数（リクエスト行とヘッダの合計）
    let mut total: usize = 0;
    // リクエスト行（最初の改行の手前まで）。上限を1バイト超えたところで伸ばすのをやめる
    let mut line: Vec<u8> = Vec::new();
    let mut line_ended = false;
    let mut scan = BlankLineScan::InLine;
    let mut chunk = [0; 1024];

    // 期限切れ。1バイトも受け取っていなければエラーとして扱わない
    let timed_out = |total: usize| {
        if total == 0 {
            Ok(RequestHead::Closed)
        } else {
            Err("timed out while reading the request line and headers".into())
        }
    };

    'read: loop {
        let remaining = timeout.saturating_sub(started.elapsed());
        if remaining.is_zero() {
            return timed_out(total);
        }
        if let Err(e) = stream.limit_next_read(remaining) {
            return Err(e.into());
        }
        let n = match stream.read(&mut chunk) {
            Ok(n) => n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            // プリコネクトや死活監視でログが増えないよう、
            // 1バイトも受け取っていない接続の失敗はエラーとして扱わない
            Err(_) if total == 0 => return Ok(RequestHead::Closed),
            Err(e) => return Err(e.into()),
        };
        if started.elapsed() > timeout {
            return timed_out(total);
        }
        if n == 0 {
            if total == 0 {
                return Ok(RequestHead::Closed);
            }
            // 相手はこれ以上送らない。読み残しも無い
            break;
        }

        for byte in &chunk[..n] {
            total += 1;
            if total > MAX_REQUEST_HEAD_BYTES {
                // これ以上は待たない。読み残しがあれば、応答は届かないことがある
                return Ok(if line.len() > MAX_REQUEST_LINE_BYTES {
                    RequestHead::LineTooLong
                } else {
                    RequestHead::TooLarge
                });
            }
            if !line_ended {
                if *byte == b'\n' {
                    line_ended = true;
                } else if line.len() <= MAX_REQUEST_LINE_BYTES {
                    line.push(*byte);
                }
            }
            scan = scan.feed(*byte);
            if scan == BlankLineScan::Found {
                break 'read;
            }
        }
    }

    if line.len() > MAX_REQUEST_LINE_BYTES {
        return Ok(RequestHead::LineTooLong);
    }
    Ok(RequestHead::Line(String::from_utf8_lossy(&line).into_owned()))
}

/// 応答を書き終えたあと、相手が閉じるまで、残りの入力を読み捨てます
///
/// 読み残しのあるソケットを閉じると、OSが接続をリセットして、相手がまだ
/// 受け取っていない応答を捨てることがあります。それを避けるために、
/// 書き込み側を閉じたあと、相手が閉じる（EOF）のを待ってから閉じます
/// 読んだ中身は解釈しません（リクエストの本文や、続けて送られた別のリクエストなど）
///
/// 待つのは、started から数えた timeout の残りまでです（接続の期限を延ばさない）
/// 読み捨てるのは MAX_LINGER_BYTES までです。どちらかに達したら、そこでやめます
/// 失敗しても何もしません（このあと閉じるだけなので、エラーとしても扱わない）
/// 読み捨てたバイト数を返します
fn discard_until_closed<R: Read + ReadDeadline>(
    stream: &mut R,
    started: Instant,
    timeout: Duration,
) -> usize {
    let mut discarded: usize = 0;
    let mut chunk = [0; 4096];

    while discarded < MAX_LINGER_BYTES {
        let remaining = timeout.saturating_sub(started.elapsed());
        if remaining.is_zero() || stream.limit_next_read(remaining).is_err() {
            break;
        }
        // 上限ちょうどでやめられるよう、残りの分だけ読む
        let room = chunk.len().min(MAX_LINGER_BYTES - discarded);
        match stream.read(&mut chunk[..room]) {
            Ok(0) => break,
            Ok(n) => discarded += n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => break,
        }
    }

    discarded
}

/// 1本の接続を処理します。1接続につき1リクエストです
///
/// 応答は、ヘッダを空行まで読み捨ててから書きます（read_request_head）
/// 期限内に空行まで届かなければ、応答せずにエラーを返します
/// 応答を書き終えたら true、何も送らなかった相手に何も返さなかったら false を返します
/// 応答（と、その本文の枠）は、戻る前に破棄します
///
/// started は接続の処理を始めた時点で、timeout はそこからの期限です
///
/// TcpStream固有の操作（書き込みのタイムアウトの設定、書き込み側を閉じる操作など）は
/// accept側で行い、ここは Read + Write と ReadDeadline だけに依存させています
/// （メモリ上のストリームで試験するため）
fn handle_connection<S: Read + Write + ReadDeadline>(
    stream: &mut S,
    log_cache: &log_cache::SharedUsageCache,
    started: Instant,
    timeout: Duration,
) -> Result<bool, error::Error> {
    let response = match read_request_head(stream, started, timeout)? {
        // 何も送らなかった相手には何も返さない
        RequestHead::Closed => return Ok(false),
        RequestHead::LineTooLong => Response::empty(StatusCode::BadRequest),
        RequestHead::TooLarge =>
            Response::empty(StatusCode::RequestHeaderFieldsTooLarge),
        RequestHead::Line(line) => respond(&line, log_cache),
    };
    write_response(stream, &response)?;

    Ok(true)
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
            let started = Instant::now();
            match handle_connection(&mut stream, &log_cache, started, timeout) {
                // 応答を書き終えたら、書き込み側を先に閉じる（送り終わりを相手に伝える）
                // そのあと、相手が閉じるまで残りの入力を読み捨ててから閉じる
                // (応答と本文の枠は、ここに戻った時点で破棄されている)
                // 相手がすでに切っていれば失敗するが、できることは無いので無視する
                Ok(true) => {
                    if stream.shutdown(std::net::Shutdown::Write).is_ok() {
                        discard_until_closed(&mut stream, started, timeout);
                    }
                }
                Ok(false) => {}
                Err(e) => eprintln!("server: connection error: {}", e),
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
                    let v = (i % 7) as f32;
                    assert!(lock.insert(name, UsagePoint {
                        time_ms: time_of(i),
                        cpu: Some(v),
                        memory: Some(v + 0.5),
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
        handle_connection(&mut stream, cache, Instant::now(), IO_TIMEOUT)
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
        // 値は小数2桁（1 kB/s 未満の速度も 0 にならない）、欠損は null
        let cache = create_shared_cache();
        {
            let mut lock = cache.write().unwrap();
            lock.insert("web", UsagePoint {
                time_ms: BASE_MS + 10_023,
                cpu: Some(0.125),
                memory: Some(3.64),
                read_kbps: Some(0.0),
                write_kbps: Some(1234.0),
                recv_kbps: None,
                send_kbps: Some(0.35),
            });
            lock.insert("web", UsagePoint {
                time_ms: BASE_MS + 20_000,
                cpu: None,
                memory: Some(100.0),
                read_kbps: None,
                write_kbps: None,
                recv_kbps: Some(12.345),
                send_kbps: None,
            });
        }
        let body_of = |path: &str| body_of(&cache, path);
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
            r#"[{"time":"2026-01-01T00:00:10.023Z","readkBps":0.00,"writekBps":1234.00},"#,
            r#"{"time":"2026-01-01T00:00:20.000Z","readkBps":null,"writekBps":null}]"#,
        );
        assert_eq!(body_of("/containers/web/io/read"), io);
        assert_eq!(body_of("/containers/web/io/write"), io);
        let net = concat!(
            r#"[{"time":"2026-01-01T00:00:10.023Z","recvkBps":null,"sendkBps":0.35},"#,
            r#"{"time":"2026-01-01T00:00:20.000Z","recvkBps":12.35,"sendkBps":null}]"#,
        );
        assert_eq!(body_of("/containers/web/net/recv"), net);
        assert_eq!(body_of("/containers/web/net/send"), net);
        assert_eq!(body_of("/containers"), r#"["web"]"#);
    }

    // ---- /usages/{kind}（全コンテナ分をまとめて返す） ----

    const USAGE_KINDS: [(&str, &[&str]); 4] = [
        ("cpu", &["percentage"]),
        ("memory", &["percentage"]),
        ("io", &["read", "write"]),
        ("net", &["recv", "send"]),
    ];

    fn body_of(cache: &SharedUsageCache, path: &str) -> String {
        let response = get(cache, path);
        assert_json_ok(&response);
        split(&response).2.to_string()
    }

    fn times_of(series: &json::JsonValue) -> Vec<i64> {
        series["t"].members().map(|t| t.as_i64().unwrap()).collect()
    }

    #[test]
    fn usages_bodies_keep_their_exact_shape() {
        // 時刻はエポックからのミリ秒の整数、値は小数2桁、欠損は null
        // キャッシュには web を先に入れるが、出力は名前の順（db が先）
        let cache = create_shared_cache();
        {
            let mut lock = cache.write().unwrap();
            lock.insert("web", UsagePoint {
                time_ms: BASE_MS + 10_023,
                cpu: Some(0.125),
                memory: Some(3.64),
                read_kbps: Some(0.0),
                write_kbps: Some(1234.0),
                recv_kbps: None,
                send_kbps: Some(0.35),
            });
            lock.insert("web", UsagePoint {
                time_ms: BASE_MS + 20_000,
                cpu: None,
                memory: Some(100.0),
                read_kbps: None,
                write_kbps: None,
                recv_kbps: Some(12.345),
                send_kbps: None,
            });
            lock.insert("db", UsagePoint {
                time_ms: BASE_MS + 10_031,
                cpu: Some(0.03),
                memory: None,
                read_kbps: Some(1.0),
                write_kbps: Some(0.004),
                recv_kbps: Some(2.5),
                send_kbps: None,
            });
        }
        assert_eq!(
            body_of(&cache, "/usages/cpu"),
            concat!(
                r#"{"db":{"percentage":{"t":[1767225610031],"v":[0.03]}},"#,
                r#""web":{"percentage":{"t":[1767225610023,1767225620000],"v":[0.12,null]}}}"#,
            ),
        );
        assert_eq!(
            body_of(&cache, "/usages/memory"),
            concat!(
                r#"{"db":{"percentage":{"t":[1767225610031],"v":[null]}},"#,
                r#""web":{"percentage":{"t":[1767225610023,1767225620000],"v":[3.64,100.00]}}}"#,
            ),
        );
        assert_eq!(
            body_of(&cache, "/usages/io"),
            concat!(
                r#"{"db":{"read":{"t":[1767225610031],"v":[1.00]},"#,
                r#""write":{"t":[1767225610031],"v":[0.00]}},"#,
                r#""web":{"read":{"t":[1767225610023,1767225620000],"v":[0.00,null]},"#,
                r#""write":{"t":[1767225610023,1767225620000],"v":[1234.00,null]}}}"#,
            ),
        );
        assert_eq!(
            body_of(&cache, "/usages/net"),
            concat!(
                r#"{"db":{"recv":{"t":[1767225610031],"v":[2.50]},"#,
                r#""send":{"t":[1767225610031],"v":[null]}},"#,
                r#""web":{"recv":{"t":[1767225610023,1767225620000],"v":[null,12.35]},"#,
                r#""send":{"t":[1767225610023,1767225620000],"v":[0.35,null]}}}"#,
            ),
        );
    }

    #[test]
    fn usages_series_have_matching_lengths_and_are_downsampled() {
        // 3点（間引かない）と 2000点（512点に間引く）
        for (npoints, expected) in [(3, 3), (2000, 512)] {
            let cache = cache_with(npoints, time_ms_at);
            for (kind, series_names) in USAGE_KINDS {
                let parsed = assert_json_ok(&get(&cache, &format!("/usages/{}", kind)));
                // コンテナは web と db だけ
                assert_eq!(parsed.len(), 2);
                for name in ["web", "db"] {
                    // 系列はその種別のものだけ
                    assert_eq!(parsed[name].len(), series_names.len());
                    for series_name in series_names {
                        let series = &parsed[name][*series_name];
                        // 系列が持つのは t と v だけ
                        assert_eq!(series.len(), 2);
                        assert_eq!(series["t"].len(), expected, "{} {}", kind, series_name);
                        assert_eq!(series["v"].len(), expected, "{} {}", kind, series_name);
                        let times = times_of(series);
                        assert_eq!(times[0], time_ms_at(0));
                        assert_eq!(times[expected - 1], time_ms_at(npoints - 1));
                        assert!(times.windows(2).all(|w| w[0] < w[1]));
                    }
                }
            }
        }
    }

    #[test]
    fn usages_values_match_the_per_container_api() {
        // 同じキャッシュなら、選ばれる点も値もコンテナ別のAPIと同じ
        let cache = cache_with(2000, time_ms_at);
        let bulk = assert_json_ok(&get(&cache, "/usages/io"));
        let single = assert_json_ok(&get(&cache, "/containers/web/io/read"));
        let read = &bulk["web"]["read"];
        assert_eq!(read["t"].len(), single.len());
        for (i, point) in single.members().enumerate() {
            let time = crate::time::millis_to_iso8601(read["t"][i].as_i64().unwrap());
            assert_eq!(point["time"], time.as_str());
            assert_eq!(point["readkBps"], read["v"][i]);
        }
        // write は全点が欠損
        assert!(bulk["web"]["write"]["v"].members().all(|v| v.is_null()));
    }

    #[test]
    fn usages_read_and_write_are_downsampled_independently() {
        // read と write で山の位置が違う系列
        const READ_SPIKE: usize = 700;
        const WRITE_SPIKE: usize = 1300;
        let cache = create_shared_cache();
        {
            let mut lock = cache.write().unwrap();
            for i in 0..2000 {
                let spike = |at: usize| Some(if i == at { 500.0 } else { 1.0 });
                assert!(lock.insert("web", UsagePoint {
                    time_ms: time_ms_at(i),
                    cpu: Some(1.0),
                    memory: Some(1.0),
                    read_kbps: spike(READ_SPIKE),
                    write_kbps: spike(WRITE_SPIKE),
                    recv_kbps: spike(WRITE_SPIKE),
                    send_kbps: spike(READ_SPIKE),
                }));
            }
        }
        for (kind, first, second) in [("io", "read", "write"), ("net", "send", "recv")] {
            let parsed = assert_json_ok(&get(&cache, &format!("/usages/{}", kind)));
            let first_times = times_of(&parsed["web"][first]);
            let second_times = times_of(&parsed["web"][second]);
            assert_eq!(first_times.len(), 512);
            assert_eq!(second_times.len(), 512);
            // それぞれ自分の山を残し、残した時刻の並びは一致しない
            assert!(first_times.contains(&time_ms_at(READ_SPIKE)));
            assert!(second_times.contains(&time_ms_at(WRITE_SPIKE)));
            assert_ne!(first_times, second_times);
            // 山の値は、その時刻と同じ位置に入っている
            let at = first_times.iter().position(|t| *t == time_ms_at(READ_SPIKE)).unwrap();
            assert_eq!(parsed["web"][first]["v"][at], 500.0);
        }
    }

    #[test]
    fn usages_container_names_are_sorted() {
        // /containers と同じ並び（Stringの順序。大文字は小文字より前）
        let cache = create_shared_cache();
        {
            let mut lock = cache.write().unwrap();
            for name in ["web", "db", "api2", "Api", "api", "web-2"] {
                assert!(lock.insert(name, UsagePoint {
                    time_ms: BASE_MS,
                    cpu: Some(1.0),
                    memory: None,
                    read_kbps: None,
                    write_kbps: None,
                    recv_kbps: None,
                    send_kbps: None,
                }));
            }
        }
        let expected = ["Api", "api", "api2", "db", "web", "web-2"];
        let names = json::parse(&body_of(&cache, "/containers")).unwrap();
        let names: Vec<&str> = names.members().map(|n| n.as_str().unwrap()).collect();
        assert_eq!(names, expected);

        for (kind, _) in USAGE_KINDS {
            let body = body_of(&cache, &format!("/usages/{}", kind));
            let positions: Vec<usize> = expected.iter()
                .map(|name| body.find(&format!("\"{}\":{{", name)).unwrap())
                .collect();
            assert!(positions.windows(2).all(|w| w[0] < w[1]), "{}", body);
        }
    }

    #[test]
    fn usages_of_empty_cache_is_an_empty_object() {
        let cache = create_shared_cache();
        for (kind, _) in USAGE_KINDS {
            assert_eq!(body_of(&cache, &format!("/usages/{}", kind)), "{}");
        }
    }

    #[test]
    fn usages_body_is_built_in_exactly_its_counted_length() {
        // 数えた長さは本文の長さと一致し、領域はその長さで1回だけ確保される
        let cache = full_cache(8);
        let lock = cache.read().unwrap();
        for kind in ["cpu", "memory", "io", "net"] {
            let kind = crate::log_cache::UsageKind::from_path(kind).unwrap();
            let len = usages_json_len(&lock, kind);
            let body = usages_json_within(&lock, kind, len).unwrap();
            assert_eq!(body.len(), len);
            assert_eq!(body.capacity(), len);
            json::parse(&body).unwrap();
            // 1バイトでも足りなければ作らない。余っていても伸ばさない
            assert!(usages_json_within(&lock, kind, len - 1).is_none());
            assert!(usages_json_within(&lock, kind, 0).is_none());
            let roomy = usages_json_within(&lock, kind, len + 100).unwrap();
            assert_eq!(roomy.len(), len);
            assert_eq!(roomy.capacity(), len + 100);
        }
    }

    #[test]
    fn capped_string_never_grows() {
        use std::fmt::Write as _;

        let mut capped = CappedString::new(5);
        assert!(capped.write_str("abc").is_ok());
        // 収まらない書き込みは、一部も書かずに失敗する
        assert!(capped.write_str("def").is_err());
        assert!(write!(capped, "{:.2}", f32::MAX).is_err());
        assert!(capped.write_char('あ').is_err());
        assert!(capped.write_str("de").is_ok());
        assert!(capped.write_str("f").is_err());
        assert!(capped.write_str("").is_ok());
        assert_eq!(capped.out, "abcde");
        assert_eq!(capped.out.capacity(), 5);
    }

    /// 本文が長くなる値と名前を持つキャッシュ
    /// 値は f32 の最大・最小（小数2桁で42〜43文字）、名前はエスケープで6倍になる
    fn widest_cache(ncontainers: usize, npoints: usize) -> SharedUsageCache {
        let cache = create_shared_cache();
        {
            let mut lock = cache.write().unwrap();
            for c in 0..ncontainers {
                let name = format!("{}{}", "\u{1}\"\\".repeat(100), c);
                for i in 0..npoints {
                    let v = if i % 2 == 0 { f32::MAX } else { f32::MIN };
                    assert!(lock.insert(&name, UsagePoint {
                        time_ms: time_ms_at(i),
                        cpu: Some(v),
                        memory: Some(v),
                        read_kbps: Some(v),
                        write_kbps: Some(v),
                        recv_kbps: Some(v),
                        send_kbps: Some(v),
                    }));
                }
            }
        }
        cache
    }

    #[test]
    fn widest_values_and_names_are_counted_exactly() {
        assert_eq!(format!("{:.2}", f32::MAX).len(), 42);
        assert_eq!(format!("{:.2}", f32::MIN).len(), 43);

        let cache = widest_cache(2, 600);
        let lock = cache.read().unwrap();
        let kind = crate::log_cache::UsageKind::Io;
        let len = usages_json_len(&lock, kind);
        // 1点あたり 13桁 + 42〜43文字 + 区切り2。通常の値（約20バイト）の3倍近い
        assert!(len > 2 * 2 * 512 * 57, "{}", len);
        let body = usages_json_within(&lock, kind, len).unwrap();
        assert_eq!(body.len(), len);
        assert_eq!(body.capacity(), len);
        // 名前は制御文字・引用符・バックスラッシュをエスケープしたうえで、元に戻せる
        let parsed = json::parse(&body).unwrap();
        assert_eq!(parsed.len(), 2);
        for (name, _) in parsed.entries() {
            assert!(name.starts_with("\u{1}\"\\\u{1}\"\\"), "{:?}", name);
        }
    }

    // ---- /usages/{kind} の途切れの目印 ----

    /// i 番目の点の時刻（10秒間隔）。添字 1000 と 1001 の間だけ 1 時間空く
    fn time_ms_with_gap(i: usize) -> i64 {
        BASE_MS + i as i64 * 10_000 + if i > 1000 { 3_600_000 } else { 0 }
    }

    /// 途切れの目印の時刻（途切れの直前の点の 1 ミリ秒後）
    fn marker_ms() -> i64 { time_ms_with_gap(1000) + 1 }

    #[test]
    fn usages_mark_a_gap_with_one_null_point() {
        // web は 10秒 → 20秒 のあと 70 秒空いて 90秒。db は 60 秒ちょうどで、途切れではない
        let cache = create_shared_cache();
        {
            let mut lock = cache.write().unwrap();
            for (name, offsets) in [
                ("web", [10_000, 20_000, 90_000]),
                ("db", [10_000, 70_000, 80_000]),
            ] {
                for offset in offsets {
                    assert!(lock.insert(name, UsagePoint {
                        time_ms: BASE_MS + offset,
                        cpu: Some(1.5),
                        memory: None,
                        read_kbps: Some(2.0),
                        write_kbps: None,
                        recv_kbps: None,
                        send_kbps: None,
                    }));
                }
            }
        }
        assert_eq!(
            body_of(&cache, "/usages/cpu"),
            concat!(
                r#"{"db":{"percentage":{"t":[1767225610000,1767225670000,1767225680000],"#,
                r#""v":[1.50,1.50,1.50]}},"#,
                r#""web":{"percentage":{"t":[1767225610000,1767225620000,1767225620001,"#,
                r#"1767225690000],"v":[1.50,1.50,null,1.50]}}}"#,
            ),
        );
        // 値が取れなかった点の null はそのまま出て、目印も同じ null で並ぶ
        assert_eq!(
            body_of(&cache, "/usages/io"),
            concat!(
                r#"{"db":{"read":{"t":[1767225610000,1767225670000,1767225680000],"#,
                r#""v":[2.00,2.00,2.00]},"#,
                r#""write":{"t":[1767225610000,1767225670000,1767225680000],"#,
                r#""v":[null,null,null]}},"#,
                r#""web":{"read":{"t":[1767225610000,1767225620000,1767225620001,"#,
                r#"1767225690000],"v":[2.00,2.00,null,2.00]},"#,
                r#""write":{"t":[1767225610000,1767225620000,1767225620001,"#,
                r#"1767225690000],"v":[null,null,null,null]}}}"#,
            ),
        );

        // コンテナ別のAPIには目印を入れない（形も中身も従来のまま）
        assert_eq!(
            body_of(&cache, "/containers/web/cpu"),
            concat!(
                r#"[{"time":"2026-01-01T00:00:10.000Z","percentage":1.50},"#,
                r#"{"time":"2026-01-01T00:00:20.000Z","percentage":1.50},"#,
                r#"{"time":"2026-01-01T00:01:30.000Z","percentage":1.50}]"#,
            ),
        );
        assert_eq!(
            body_of(&cache, "/containers/web/io/write"),
            concat!(
                r#"[{"time":"2026-01-01T00:00:10.000Z","readkBps":2.00,"writekBps":null},"#,
                r#"{"time":"2026-01-01T00:00:20.000Z","readkBps":2.00,"writekBps":null},"#,
                r#"{"time":"2026-01-01T00:01:30.000Z","readkBps":2.00,"writekBps":null}]"#,
            ),
        );
    }

    #[test]
    fn usages_mark_a_gap_in_every_series_with_or_without_downsampling() {
        // 1500点（512点に間引く）と、5点（間引かない）。どちらも途切れを 1 つ含む
        fn short_with_gap(i: usize) -> i64 { time_ms_with_gap(i + 999) }
        for (npoints, time_of, expected, marker) in [
            (1500, time_ms_with_gap as fn(usize) -> i64, 512, marker_ms()),
            (5, short_with_gap as fn(usize) -> i64, 5, marker_ms()),
        ] {
            let cache = cache_with(npoints, time_of);
            for (kind, series_names) in USAGE_KINDS {
                let parsed = assert_json_ok(&get(&cache, &format!("/usages/{}", kind)));
                for name in ["web", "db"] {
                    for series_name in series_names {
                        let series = &parsed[name][*series_name];
                        let label = format!("{} {} {}", npoints, kind, series_name);
                        // 間引いた点 + 目印 1 つ。t と v は同じ長さ
                        assert_eq!(series["t"].len(), expected + 1, "{}", label);
                        assert_eq!(series["v"].len(), expected + 1, "{}", label);
                        let times = times_of(series);
                        assert!(times.windows(2).all(|w| w[0] < w[1]), "{}", label);
                        assert_eq!(times[0], time_of(0));
                        assert_eq!(times[expected], time_of(npoints - 1));
                        // 目印は 1 つだけで、その位置の値は null
                        let at: Vec<usize> = times.iter().enumerate()
                            .filter(|(_, t)| **t % 10_000 != 0)
                            .map(|(i, _)| i)
                            .collect();
                        assert_eq!(at.len(), 1, "{}", label);
                        assert_eq!(times[at[0]], marker, "{}", label);
                        assert!(series["v"][at[0]].is_null(), "{}", label);
                        // 目印の前後は、途切れをまたぐ元の点
                        assert!(times[at[0] - 1] <= time_ms_with_gap(1000));
                        assert!(times[at[0] + 1] >= time_ms_with_gap(1001));
                    }
                }
            }
        }
    }

    #[test]
    fn usages_mark_the_gap_in_both_read_and_write() {
        // read と write で山の位置が違い、間引きの結果も違う系列
        const READ_SPIKE: usize = 700;
        const WRITE_SPIKE: usize = 1300;
        let cache = create_shared_cache();
        {
            let mut lock = cache.write().unwrap();
            for i in 0..2000 {
                let spike = |at: usize| Some(if i == at { 500.0 } else { 1.0 });
                assert!(lock.insert("web", UsagePoint {
                    time_ms: time_ms_with_gap(i),
                    cpu: Some(1.0),
                    memory: Some(1.0),
                    read_kbps: spike(READ_SPIKE),
                    write_kbps: spike(WRITE_SPIKE),
                    recv_kbps: spike(WRITE_SPIKE),
                    send_kbps: spike(READ_SPIKE),
                }));
            }
        }
        for (kind, first, second, route) in [
            ("io", "read", "write", "io/read"),
            ("net", "send", "recv", "net/send"),
        ] {
            let parsed = assert_json_ok(&get(&cache, &format!("/usages/{}", kind)));
            let first_times = times_of(&parsed["web"][first]);
            let second_times = times_of(&parsed["web"][second]);
            assert_ne!(first_times, second_times);
            assert!(first_times.contains(&time_ms_with_gap(READ_SPIKE)));
            assert!(second_times.contains(&time_ms_with_gap(WRITE_SPIKE)));
            for (series_name, times) in [(first, &first_times), (second, &second_times)] {
                assert_eq!(times.len(), 513);
                assert_eq!(parsed["web"][series_name]["v"].len(), 513);
                assert!(times.windows(2).all(|w| w[0] < w[1]));
                let at = times.iter().position(|t| *t == marker_ms()).unwrap();
                assert!(parsed["web"][series_name]["v"][at].is_null());
                assert_eq!(times.iter().filter(|t| **t % 10_000 != 0).count(), 1);
            }

            // コンテナ別のAPIは 512 点のままで、目印を除いた /usages と同じ点を返す
            let single = assert_json_ok(&get(&cache, &format!("/containers/web/{}", route)));
            assert_eq!(single.len(), 512);
            let without_marker: Vec<String> = first_times.iter()
                .filter(|t| **t != marker_ms())
                .map(|t| crate::time::millis_to_iso8601(*t))
                .collect();
            let single_times: Vec<&str> = single.members()
                .map(|p| p["time"].as_str().unwrap())
                .collect();
            assert_eq!(single_times, without_marker);
        }
    }

    #[test]
    fn usages_body_with_markers_is_built_in_exactly_its_counted_length() {
        // 24時間分の点数で、100点ごとに途切れがある系列（目印は 1 系列に 86 個）
        fn gap_every_100(i: usize) -> i64 {
            BASE_MS + i as i64 * 1000 + (i / 100) as i64 * 61_000
        }
        let cache = cache_with(crate::log_cache::MAX_LOG_LENGTH, gap_every_100);
        let budget = leaked_budget(MAX_BULK_BODY_BYTES);
        for kind in ["cpu", "memory", "io", "net"] {
            let kind = crate::log_cache::UsageKind::from_path(kind).unwrap();
            let len = {
                let lock = cache.read().unwrap();
                let len = usages_json_len(&lock, kind);
                let body = usages_json_within(&lock, kind, len).unwrap();
                assert_eq!(body.len(), len);
                assert_eq!(body.capacity(), len);
                // 1バイトでも足りなければ作らない
                assert!(usages_json_within(&lock, kind, len - 1).is_none());

                let parsed = json::parse(&body).unwrap();
                for (_, container) in parsed.entries() {
                    for (_, series) in container.entries() {
                        assert_eq!(series["t"].len(), 512 + 86);
                        assert_eq!(series["v"].len(), 512 + 86);
                    }
                }
                len
            };

            // 取った枠・確保した領域・本文の長さが一致する
            let response = usages_response(&cache, kind, len, budget).unwrap();
            assert_eq!(response.body.len(), len);
            assert_eq!(response.body.capacity(), len);
            assert_eq!(used_bytes(budget), len);
            drop(response);
            assert_eq!(used_bytes(budget), 0);
        }
    }

    // ---- /usages/{kind} の本文の枠 ----

    /// 試験ごとに独立した枠（プロセス全体の枠は他の試験と共有されるので使わない）
    fn leaked_budget(max: usize) -> &'static BodyBudget {
        Box::leak(Box::new(BodyBudget::new(max)))
    }

    fn used_bytes(budget: &BodyBudget) -> usize {
        *budget.used.lock().unwrap()
    }

    /// 24時間分のコンテナを ncontainers 個入れたキャッシュ
    fn full_cache(ncontainers: usize) -> SharedUsageCache {
        let cache = create_shared_cache();
        {
            let mut lock = cache.write().unwrap();
            for c in 0..ncontainers {
                let name = format!("container-{:02}", c);
                for i in 0..crate::log_cache::MAX_LOG_LENGTH {
                    let v = (i % 13) as f32 * 123.45;
                    assert!(lock.insert(&name, UsagePoint {
                        time_ms: time_ms_at(i),
                        cpu: Some(v),
                        memory: Some(v),
                        read_kbps: Some(v),
                        write_kbps: Some(v),
                        recv_kbps: Some(v),
                        send_kbps: Some(v),
                    }));
                }
            }
        }
        cache
    }

    fn io_len(cache: &SharedUsageCache) -> usize {
        usages_json_len(&cache.read().unwrap(), crate::log_cache::UsageKind::Io)
    }

    #[test]
    fn body_budget_is_returned_on_drop_and_on_panic() {
        let budget = leaked_budget(100);
        let first = budget.reserve(40);
        let second = budget.reserve(60);
        assert_eq!(used_bytes(budget), 100);

        let _ = std::thread::spawn(move || {
            let _held = second;
            panic!("panicking while holding a reservation (expected in this test)");
        }).join();
        assert_eq!(used_bytes(budget), 40);

        drop(first);
        assert_eq!(used_bytes(budget), 0);
    }

    #[test]
    fn body_budget_admits_one_oversized_body_alone() {
        let budget = leaked_budget(100);
        // 誰も持っていなければ、上限より大きくても待たずに通る
        let oversized = budget.reserve(1000);
        assert_eq!(used_bytes(budget), 1000);

        // その間は、小さな本文も待たされる
        let (sender, receiver) = std::sync::mpsc::channel();
        let waiter = std::thread::spawn(move || {
            let _held = budget.reserve(1);
            sender.send(()).unwrap();
        });
        assert!(receiver.recv_timeout(Duration::from_millis(100)).is_err());
        drop(oversized);
        receiver.recv_timeout(Duration::from_secs(10)).unwrap();
        waiter.join().unwrap();
        assert_eq!(used_bytes(budget), 0);
    }

    #[test]
    fn body_budget_never_exceeds_max_and_wakes_waiters() {
        use std::sync::atomic::{ AtomicUsize, Ordering };

        // 大きさの違う要求が混ざっても、合計は上限を超えず、全員が通る
        const MAX: usize = 100;
        let budget = leaked_budget(MAX);
        let peak = Arc::new(AtomicUsize::new(0));

        let workers: Vec<_> = (0..40).map(|i| {
            let peak = Arc::clone(&peak);
            std::thread::spawn(move || {
                let _held = budget.reserve(10 + (i % 4) * 20);
                peak.fetch_max(used_bytes(budget), Ordering::SeqCst);
                std::thread::sleep(Duration::from_millis(5));
            })
        }).collect();
        // 起こし損ねがあればここで止まる
        for worker in workers { worker.join().unwrap(); }

        let peak = peak.load(Ordering::SeqCst);
        assert!(peak <= MAX, "{}", peak);
        assert!(peak > 70, "{}", peak);
        assert_eq!(used_bytes(budget), 0);
    }

    #[test]
    fn usages_response_holds_exactly_its_body_size_until_dropped() {
        let cache = cache_with(2000, time_ms_at);
        let budget = leaked_budget(MAX_BULK_BODY_BYTES);
        let kind = crate::log_cache::UsageKind::Io;
        let len = io_len(&cache);

        let response = usages_response(&cache, kind, len, budget).unwrap();
        assert_eq!(response.status, StatusCode::Ok);
        // 取った枠・確保した領域・本文の長さが一致する
        assert_eq!(response.body.len(), len);
        assert_eq!(response.body.capacity(), len);
        assert_eq!(used_bytes(budget), len);

        // 応答を書き出しても、破棄するまでは枠を持ち続ける
        let mut output = Vec::new();
        write_response(&mut output, &response).unwrap();
        assert_eq!(used_bytes(budget), len);
        drop(response);
        assert_eq!(used_bytes(budget), 0);

        // 数えたあとでキャッシュが伸びていた場合（渡された長さでは足りない）は、
        // 小さい枠のまま書かずに、数え直した長さで枠を取り直す
        for stale in [0, 1, len - 1] {
            let response = usages_response(&cache, kind, stale, budget).unwrap();
            assert_eq!(response.body.capacity(), len);
            assert_eq!(used_bytes(budget), len);
            drop(response);
            assert_eq!(used_bytes(budget), 0);
        }
        // 縮んでいた場合は、取った枠の中に収める（枠は取った大きさのまま）
        let response = usages_response(&cache, kind, len + 10, budget).unwrap();
        assert_eq!(response.body.len(), len);
        assert_eq!(response.body.capacity(), len + 10);
        assert_eq!(used_bytes(budget), len + 10);
        drop(response);

        // 失敗したときは枠を残さない
        poison(&cache);
        assert!(usages_response(&cache, kind, 1000, budget).is_err());
        assert_eq!(used_bytes(budget), 0);
    }

    /// 同じ本文を nthreads 本並列に要求し、
    /// (枠の使用量の最大, 保持した領域の合計の最大) を返す
    /// stale_needed を渡すと、半数はその長さ（数えたあとで伸びた想定）で要求する
    fn request_bulk_bodies_in_parallel(
        cache: &SharedUsageCache,
        budget: &'static BodyBudget,
        nthreads: usize,
        stale_needed: Option<usize>,
    ) -> (usize, usize) {
        use std::sync::atomic::{ AtomicUsize, Ordering };

        let len = io_len(cache);
        let peak_used = Arc::new(AtomicUsize::new(0));
        let held_bytes = Arc::new(AtomicUsize::new(0));
        let peak_held = Arc::new(AtomicUsize::new(0));

        let workers: Vec<_> = (0..nthreads).map(|i| {
            let cache = Arc::clone(cache);
            let peak_used = Arc::clone(&peak_used);
            let held_bytes = Arc::clone(&held_bytes);
            let peak_held = Arc::clone(&peak_held);
            let needed = match stale_needed {
                Some(stale) if i % 2 == 0 => stale,
                _ => len,
            };
            std::thread::spawn(move || {
                let kind = crate::log_cache::UsageKind::Io;
                let response = usages_response(&cache, kind, needed, budget).unwrap();
                // 実際に確保されている領域の合計を、枠とは別に数える
                let capacity = response.body.capacity();
                assert_eq!(capacity, len);
                let now = held_bytes.fetch_add(capacity, Ordering::SeqCst) + capacity;
                peak_held.fetch_max(now, Ordering::SeqCst);
                peak_used.fetch_max(used_bytes(budget), Ordering::SeqCst);
                // 送っている間にあたる時間
                std::thread::sleep(Duration::from_millis(5));
                held_bytes.fetch_sub(capacity, Ordering::SeqCst);
                drop(response);
            })
        }).collect();
        for worker in workers { worker.join().unwrap(); }
        assert_eq!(used_bytes(budget), 0);

        (peak_used.load(Ordering::SeqCst), peak_held.load(Ordering::SeqCst))
    }

    #[test]
    fn concurrent_bulk_bodies_stay_within_the_budget() {
        // 24時間分 × 40コンテナ。io の本文は1本で 1MB 近くになる
        let cache = full_cache(40);
        let len = io_len(&cache);
        assert!(len > 800_000, "{}", len);
        // 同時接続の上限まで並ぶと、枠が無ければ上限を超える
        assert!(len * MAX_CONNECTIONS > MAX_BULK_BODY_BYTES);

        let budget = leaked_budget(MAX_BULK_BODY_BYTES);
        let (peak_used, peak_held) = request_bulk_bodies_in_parallel(
            &cache, budget, MAX_CONNECTIONS * 2, None,
        );
        assert!(peak_used <= MAX_BULK_BODY_BYTES, "{}", peak_used);
        assert!(peak_held <= MAX_BULK_BODY_BYTES, "{}", peak_held);
        // 1本ずつに絞っているわけではない（収まる範囲では並行に処理する）
        assert!(peak_held >= len * 2, "{}", peak_held);
    }

    #[test]
    fn widest_bodies_stay_within_the_budget_even_when_the_count_is_stale() {
        // 桁数の多い値と、エスケープで伸びる長い名前。通常の値の3倍近い本文になる
        let cache = widest_cache(8, 2000);
        let len = io_len(&cache);
        assert!(len > 8 * 2 * 512 * 57, "{}", len);

        // 3本と半分が入る枠。半数は、実際の約 1/3 の長さ（通常の値なら足りた長さ）で要求する
        let max = len * 3 + len / 2;
        let budget = leaked_budget(max);
        let (peak_used, peak_held) = request_bulk_bodies_in_parallel(
            &cache, budget, MAX_CONNECTIONS * 2, Some(len / 3),
        );
        assert!(peak_used <= max, "{} > {}", peak_used, max);
        assert!(peak_held <= len * 3, "{} > {}", peak_held, len * 3);
        assert!(peak_held >= len * 2, "{}", peak_held);
    }

    #[test]
    fn oversized_body_is_served_only_while_nothing_else_is_held() {
        // 1本で上限を超える本文は、ほかに保持中の本文が無いときに1本ずつ通る
        let cache = widest_cache(2, 600);
        let len = io_len(&cache);
        let max = len / 2;
        let budget = leaked_budget(max);
        let (peak_used, peak_held) = request_bulk_bodies_in_parallel(
            &cache, budget, 8, Some(len / 3),
        );
        assert_eq!(peak_held, len);
        assert!(peak_used <= len, "{} > {}", peak_used, len);
    }

    #[test]
    fn waiting_for_the_budget_does_not_hold_the_cache_lock() {
        // 枠が空くのを待っている間も、loggerはwrite lockを取れる
        let cache = small_cache();
        let budget = leaked_budget(100);
        let blocker = budget.reserve(100);

        let waiting_cache = Arc::clone(&cache);
        let waiter = std::thread::spawn(move || {
            let kind = crate::log_cache::UsageKind::Cpu;
            usages_response(&waiting_cache, kind, 50, budget).unwrap().body
        });
        // 待ちに入るまで待つ（待っている間、使用量は変わらない）
        std::thread::sleep(Duration::from_millis(100));
        assert_eq!(used_bytes(budget), 100);
        assert!(!waiter.is_finished());

        // 待たされている間に追加した点も、応答に入る
        assert!(cache.try_write().is_ok());
        assert!(cache.write().unwrap().insert("web", UsagePoint {
            time_ms: time_ms_at(100),
            cpu: Some(42.0),
            memory: None,
            read_kbps: None,
            write_kbps: None,
            recv_kbps: None,
            send_kbps: None,
        }));
        drop(blocker);
        let body = waiter.join().unwrap();
        assert!(body.contains("42.00"), "{}", body);
        assert_eq!(used_bytes(budget), 0);
    }

    #[test]
    fn response_is_written_without_changing_its_bytes() {
        // ヘッダと本文を分けて書いても、届くバイト列は同じ
        let response = Response::json(r#"{"a":1}"#.to_string());
        let mut output = Vec::new();
        write_response(&mut output, &response).unwrap();
        assert_eq!(
            String::from_utf8(output).unwrap(),
            concat!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n",
                "Content-Length: 7\r\nConnection: close\r\n\r\n",
                r#"{"a":1}"#,
            ),
        );
    }

    #[test]
    fn usages_escape_names_and_drop_non_finite_values() {
        let mut escaped = String::new();
        write_json_string(&mut escaped, "a\"b\\c\nd").unwrap();
        assert_eq!(escaped, r#""a\"b\\c\u000ad""#);

        let cache = create_shared_cache();
        {
            let mut lock = cache.write().unwrap();
            lock.insert("we\"b", UsagePoint {
                time_ms: BASE_MS,
                cpu: Some(f32::NAN),
                memory: Some(f32::INFINITY),
                read_kbps: None,
                write_kbps: None,
                recv_kbps: None,
                send_kbps: None,
            });
        }
        for kind in ["cpu", "memory"] {
            let parsed = assert_json_ok(&get(&cache, &format!("/usages/{}", kind)));
            assert!(parsed["we\"b"]["percentage"]["v"][0].is_null());
        }
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
            "/usages",
            "/usages/",
            "/usages/disk",
            "/usages/CPU",
            "/usages/io/read",
            "/usages/cpu/extra",
            "/usages/cpu?nsample=10",
            "/usages/web/cpu",
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
        handle_connection(&mut in_time, &cache, Instant::now(), timeout).unwrap();
        assert_json_ok(&String::from_utf8(in_time.output).unwrap());

        // 改行が届くのが期限の後なら、処理せずに打ち切る
        // 2回目のreadで待つのは残り時間までで、合計が期限を大きく超えない
        let mut late = SlowStream::new(
            b"GET /cont", b"ainers HTTP/1.1\r\n\r\n", Duration::from_millis(1000),
        );
        let started = Instant::now();
        assert!(handle_connection(&mut late, &cache, Instant::now(), timeout).is_err());
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
        assert!(handle_connection(&mut unlimited, &cache, Instant::now(), timeout).is_err());
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
        assert!(handle_connection(&mut silent, &cache, Instant::now(), IO_TIMEOUT).is_ok());
        assert!(silent.output.is_empty());

        // 途中まで受け取ってから失敗した接続は、エラーとして報告する
        let mut partial = StallingStream {
            prefix: std::io::Cursor::new(b"GET /cont".to_vec()),
            output: Vec::new(),
        };
        assert!(handle_connection(&mut partial, &cache, Instant::now(), IO_TIMEOUT).is_err());
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

        for path in ["/containers", "/containers/web/cpu", "/usages/cpu", "/unknown"] {
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
        // コンテナ別のAPIだけで1画面分を取ると 6N 本になる（Nはコンテナ数）
        // 同時に発射されて上限を超えた分も断られず、全部200になること
        const NCONTAINERS: usize = 10;
        let cache = create_shared_cache();
        {
            let mut lock = cache.write().unwrap();
            for c in 0..NCONTAINERS {
                for i in 0..crate::log_cache::MAX_LOG_LENGTH {
                    let name = format!("c{}", c);
                    let v = (i % 13) as f32;
                    assert!(lock.insert(&name, UsagePoint {
                        time_ms: time_ms_at(i),
                        cpu: Some(v),
                        memory: Some(v),
                        read_kbps: Some(v),
                        write_kbps: Some(1.0),
                        recv_kbps: Some(v),
                        send_kbps: Some(1.0),
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

    #[test]
    fn container_names_are_escaped_like_the_usages_keys() {
        // 通常の名前では、エスケープする前と1バイトも変わらない
        let plain = create_shared_cache();
        let names = ["web", "db", "api_1", "my-app.v2", "Api", "0"];
        {
            let mut lock = plain.write().unwrap();
            for name in names {
                assert!(lock.insert(name, point_at(BASE_MS)));
            }
        }
        let mut sorted = names.to_vec();
        sorted.sort();
        let unescaped = format!(
            "[{}]",
            sorted.iter().map(|s| format!("\"{}\"", s)).collect::<Vec<String>>().join(","),
        );
        assert_eq!(body_of(&plain, "/containers"), unescaped);
        assert_eq!(body_of(&plain, "/containers"), r#"["0","Api","api_1","db","my-app.v2","web"]"#);
        assert_eq!(body_of(&create_shared_cache(), "/containers"), "[]");

        // 引用符・バックスラッシュ・制御文字があっても、本文は正しいjsonで、元の名前に戻る
        let odd_names = ["we\"b", "d\\b", "a\nb\tc\u{1}", "\"", "\\", "日本語"];
        let odd = create_shared_cache();
        {
            let mut lock = odd.write().unwrap();
            for name in odd_names {
                assert!(lock.insert(name, point_at(BASE_MS)));
            }
        }
        let body = body_of(&odd, "/containers");
        assert!(!body.contains('\n') && !body.contains('\u{1}'), "{:?}", body);
        let parsed = json::parse(&body).unwrap();
        let mut restored: Vec<&str> = parsed.members().map(|n| n.as_str().unwrap()).collect();
        let mut expected = odd_names.to_vec();
        restored.sort();
        expected.sort();
        assert_eq!(restored, expected);
        // /usages/{kind} のキーと同じ名前の集合になる
        let usages = assert_json_ok(&get(&odd, "/usages/cpu"));
        let mut keys: Vec<&str> = usages.entries().map(|(k, _)| k).collect();
        keys.sort();
        assert_eq!(keys, expected);
    }

    /// cpu だけ値を持つ1点
    fn point_at(time_ms: i64) -> UsagePoint {
        UsagePoint {
            time_ms,
            cpu: Some(1.0),
            memory: None,
            read_kbps: None,
            write_kbps: None,
            recv_kbps: None,
            send_kbps: None,
        }
    }

    // ---- ヘッダの読み捨て（メモリ上のストリーム） ----

    /// 1回のreadで1バイトずつ返すストリーム。読まれたバイト数を数える
    struct OneByteStream {
        input: Vec<u8>,
        consumed: usize,
        output: Vec<u8>,
    }
    impl OneByteStream {
        fn new(input: &[u8]) -> Self {
            OneByteStream { input: input.to_vec(), consumed: 0, output: Vec::new() }
        }
    }
    impl Read for OneByteStream {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            match self.input.get(self.consumed) {
                Some(byte) => {
                    buf[0] = *byte;
                    self.consumed += 1;
                    Ok(1)
                }
                None => Ok(0),
            }
        }
    }
    impl Write for OneByteStream {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.output.write(buf)
        }
        fn flush(&mut self) -> std::io::Result<()> { Ok(()) }
    }
    impl ReadDeadline for OneByteStream {
        fn limit_next_read(&mut self, _: Duration) -> std::io::Result<()> { Ok(()) }
    }

    /// 合計が total バイトちょうどになる、/containers へのリクエスト（空行まで）
    fn request_of_exactly(total: usize) -> Vec<u8> {
        let fixed = "GET /containers HTTP/1.1\r\nX-Pad: \r\n\r\n".len();
        let request = format!(
            "GET /containers HTTP/1.1\r\nX-Pad: {}\r\n\r\n",
            "p".repeat(total - fixed),
        );
        assert_eq!(request.len(), total);
        request.into_bytes()
    }

    #[test]
    fn blank_line_is_found_in_every_form() {
        fn found_at(input: &[u8]) -> Option<usize> {
            let mut scan = BlankLineScan::InLine;
            for (i, byte) in input.iter().enumerate() {
                scan = scan.feed(*byte);
                if scan == BlankLineScan::Found { return Some(i + 1); }
            }
            None
        }
        assert_eq!(found_at(b"GET / HTTP/1.1\r\n\r\n"), Some(18));
        assert_eq!(found_at(b"GET / HTTP/1.1\n\n"), Some(16));
        assert_eq!(found_at(b"GET / HTTP/1.1\r\nA: b\r\n\r\nrest"), Some(24));
        assert_eq!(found_at(b"GET / HTTP/1.1\nA: b\n\nrest"), Some(21));
        // 改行の種類が混ざっていてもよい
        assert_eq!(found_at(b"GET / HTTP/1.1\r\nA: b\n\r\n"), Some(23));
        assert_eq!(found_at(b"GET / HTTP/1.1\nA: b\r\n\n"), Some(22));
        // 先頭の改行だけでは空行にならない。行の途中の \r は改行ではない
        assert_eq!(found_at(b"\n"), None);
        assert_eq!(found_at(b"\r\n"), None);
        assert_eq!(found_at(b"\n\n"), Some(2));
        assert_eq!(found_at(b"A: b\r\r\nC: d\r\n"), None);
        assert_eq!(found_at(b"A: b\n\r\rC: d\n"), None);
        assert_eq!(found_at(b"GET / HTTP/1.1\r\nA: b\r\n"), None);
    }

    #[test]
    fn headers_are_discarded_before_responding() {
        let cache = small_cache();
        let long_header = format!("Cookie: {}\r\n", "c".repeat(3000));
        // ヘッダの長さや数、改行の種類に関わらず、応答は変わらない
        for headers in [
            "".to_string(),
            "Host: x\r\n".to_string(),
            long_header.clone(),
            format!("Host: x\r\n{}Accept: */*\r\n", long_header.repeat(4)),
        ] {
            for (line, status) in [
                ("GET /containers HTTP/1.1", "HTTP/1.1 200 OK"),
                ("GET /unknown HTTP/1.1", "HTTP/1.1 404 Not Found"),
                ("POST /containers HTTP/1.1", "HTTP/1.1 405 Method Not Allowed"),
                ("garbage", "HTTP/1.1 400 Bad Request"),
            ] {
                let crlf = format!("{}\r\n{}\r\n", line, headers);
                let lf = crlf.replace("\r\n", "\n");
                for request in [crlf, lf] {
                    assert_status(&exchange(&cache, request.as_bytes()), status);

                    // 1バイトずつ届いても同じで、空行まで読んでから応答する
                    let mut stream = OneByteStream::new(request.as_bytes());
                    handle_connection(&mut stream, &cache, Instant::now(), IO_TIMEOUT).unwrap();
                    assert_eq!(stream.consumed, request.len());
                    assert_status(&String::from_utf8(stream.output).unwrap(), status);
                }
            }
        }
    }

    #[test]
    fn nothing_is_read_past_the_blank_line() {
        // 応答を書くまでは、本文（ボディ）を待たない。空行の次のバイトには触らない
        let cache = small_cache();
        for (head, status) in [
            (
                "POST /containers HTTP/1.1\r\nContent-Length: 9\r\n\r\n",
                "HTTP/1.1 405 Method Not Allowed",
            ),
            ("GET /containers HTTP/1.1\n\n", "HTTP/1.1 200 OK"),
        ] {
            let mut stream = OneByteStream::new(format!("{}body body", head).as_bytes());
            handle_connection(&mut stream, &cache, Instant::now(), IO_TIMEOUT).unwrap();
            assert_eq!(stream.consumed, head.len());
            assert_status(&String::from_utf8(stream.output).unwrap(), status);
        }
    }

    #[test]
    fn too_large_request_head_is_431() {
        let cache = small_cache();
        const TOO_LARGE: &str = "HTTP/1.1 431 Request Header Fields Too Large";

        // 空行まで含めて上限ちょうどなら通る。1バイトでも超えれば431
        assert_json_ok(&exchange(&cache, &request_of_exactly(MAX_REQUEST_HEAD_BYTES)));
        let over = exchange(&cache, &request_of_exactly(MAX_REQUEST_HEAD_BYTES + 1));
        assert_status(&over, TOO_LARGE);
        // 本文は無い（内部の情報を載せない）
        assert_eq!(split(&over).2, "");

        // 上限を超えたところで読むのをやめる（残りを読み続けない）
        let huge = request_of_exactly(MAX_REQUEST_HEAD_BYTES * 8);
        let mut stream = OneByteStream::new(&huge);
        handle_connection(&mut stream, &cache, Instant::now(), IO_TIMEOUT).unwrap();
        assert_eq!(stream.consumed, MAX_REQUEST_HEAD_BYTES + 1);
        assert_status(&String::from_utf8(stream.output).unwrap(), TOO_LARGE);

        // 空行が来ないまま続くヘッダも、多数の短いヘッダも同じ
        let endless = format!("GET /containers HTTP/1.1\r\n{}", "A: b\r\n".repeat(4000));
        assert_status(&exchange(&cache, endless.as_bytes()), TOO_LARGE);
        // リクエスト行の内容に関わらず431（経路の判定より先に決まる）
        let post = format!("POST /unknown HTTP/1.1\r\n{}", "A: b\r\n".repeat(4000));
        assert_status(&exchange(&cache, post.as_bytes()), TOO_LARGE);

        // リクエスト行が長すぎる場合は、全体が上限を超えても400のまま
        let long_line = "a".repeat(MAX_REQUEST_HEAD_BYTES * 2);
        assert_status(&exchange(&cache, long_line.as_bytes()), "HTTP/1.1 400 Bad Request");
        let mut stream = OneByteStream::new(long_line.as_bytes());
        handle_connection(&mut stream, &cache, Instant::now(), IO_TIMEOUT).unwrap();
        assert_eq!(stream.consumed, MAX_REQUEST_HEAD_BYTES + 1);
    }

    #[test]
    fn too_long_request_line_is_400_after_discarding_headers() {
        // リクエスト行が長すぎても、空行までは読み捨ててから400を返す
        let cache = small_cache();
        let request = format!(
            "GET /{} HTTP/1.1\r\nHost: x\r\nCookie: {}\r\n\r\n",
            "a".repeat(MAX_REQUEST_LINE_BYTES * 2),
            "c".repeat(3000),
        );
        let mut stream = OneByteStream::new(request.as_bytes());
        handle_connection(&mut stream, &cache, Instant::now(), IO_TIMEOUT).unwrap();
        assert_eq!(stream.consumed, request.len());
        assert_status(
            &String::from_utf8(stream.output).unwrap(),
            "HTTP/1.1 400 Bad Request",
        );
    }

    #[test]
    fn stalled_headers_get_no_response() {
        let cache = small_cache();
        // リクエスト行は届いたが、空行が来ないまま止まった接続には応答しない
        for prefix in [
            &b"GET /containers HTTP/1.1\r\n"[..],
            b"GET /containers HTTP/1.1\r\nHost: x\r\n",
            b"GET /containers HTTP/1.1\r\nHost: x\r\n\r",
            b"garbage\r\nHost: x\r\n",
        ] {
            let mut stalled = StallingStream {
                prefix: std::io::Cursor::new(prefix.to_vec()),
                output: Vec::new(),
            };
            assert!(handle_connection(&mut stalled, &cache, Instant::now(), IO_TIMEOUT).is_err());
            assert!(stalled.output.is_empty());
        }
    }

    #[test]
    fn header_deadline_is_shared_with_the_request_line() {
        let cache = small_cache();
        let timeout = Duration::from_millis(100);

        // リクエスト行のあと、期限内に空行が届けば処理する
        let mut in_time = SlowStream::new(
            b"GET /containers HTTP/1.1\r\nHost: x\r\n", b"\r\n", Duration::from_millis(10),
        );
        handle_connection(&mut in_time, &cache, Instant::now(), timeout).unwrap();
        assert_json_ok(&String::from_utf8(in_time.output).unwrap());

        // 空行が届くのが期限の後なら、応答せずに打ち切る
        // ヘッダを待つのは残り時間までで、リクエスト行と合わせた合計が期限を大きく超えない
        let mut late = SlowStream::new(
            b"GET /containers HTTP/1.1\r\nHost: x\r\n", b"\r\n", Duration::from_millis(1000),
        );
        let started = Instant::now();
        assert!(handle_connection(&mut late, &cache, Instant::now(), timeout).is_err());
        assert!(late.output.is_empty());
        assert!(started.elapsed() < Duration::from_millis(700), "{:?}", started.elapsed());
        assert_eq!(late.limits_seen.len(), 2);
        assert!(late.limits_seen.iter().all(|limit| *limit <= timeout));
        assert!(late.limits_seen[1] <= late.limits_seen[0]);
    }

    // ---- 応答後の読み捨て（メモリ上のストリーム） ----

    #[test]
    fn handle_connection_tells_whether_it_responded() {
        let cache = small_cache();
        for (input, responded) in [
            (&b""[..], false),
            (b"GET /containers HTTP/1.1\r\n\r\n", true),
            (b"GET /unknown HTTP/1.1\r\n\r\n", true),
            (b"garbage", true),
        ] {
            let mut stream = OneByteStream::new(input);
            let result = handle_connection(&mut stream, &cache, Instant::now(), IO_TIMEOUT);
            assert_eq!(result.unwrap(), responded);
            assert_eq!(stream.output.is_empty(), !responded);
        }
    }

    #[test]
    fn rest_of_the_input_is_discarded_until_the_peer_closes() {
        let cache = small_cache();
        // 応答のあと、空行より先に残っていた分を、相手が閉じるまで読み捨てる
        let head = "POST /containers HTTP/1.1\r\nContent-Length: 10000\r\n\r\n";
        let request = format!("{}{}", head, "b".repeat(10_000));
        let mut stream = OneByteStream::new(request.as_bytes());
        let started = Instant::now();
        assert!(handle_connection(&mut stream, &cache, started, IO_TIMEOUT).unwrap());
        assert_eq!(stream.consumed, head.len());
        assert_eq!(discard_until_closed(&mut stream, started, IO_TIMEOUT), 10_000);
        assert_eq!(stream.consumed, request.len());
        // 読み捨てた分は解釈しない（応答は1つだけ）
        assert_status(
            &String::from_utf8(stream.output).unwrap(),
            "HTTP/1.1 405 Method Not Allowed",
        );

        // 残りが無ければ、何も読まずに終わる
        let mut empty = OneByteStream::new(b"");
        assert_eq!(discard_until_closed(&mut empty, Instant::now(), IO_TIMEOUT), 0);
    }

    #[test]
    fn discarding_stops_at_the_byte_limit() {
        // 上限まで読んだら、相手が送り続けていてもやめる
        for (len, expected) in [
            (MAX_LINGER_BYTES - 1, MAX_LINGER_BYTES - 1),
            (MAX_LINGER_BYTES, MAX_LINGER_BYTES),
            (MAX_LINGER_BYTES + 1, MAX_LINGER_BYTES),
            (MAX_LINGER_BYTES * 3, MAX_LINGER_BYTES),
        ] {
            let mut stream = MemoryStream {
                input: std::io::Cursor::new(vec![b'x'; len]),
                output: Vec::new(),
            };
            assert_eq!(
                discard_until_closed(&mut stream, Instant::now(), IO_TIMEOUT),
                expected,
            );
            assert_eq!(stream.input.position() as usize, expected);
            assert!(stream.output.is_empty());
        }
    }

    #[test]
    fn discarding_stops_at_the_connection_deadline() {
        // 期限が残っていなければ、1回も読まない
        let mut unread = OneByteStream::new(b"rest");
        assert_eq!(discard_until_closed(&mut unread, Instant::now(), Duration::ZERO), 0);
        assert_eq!(unread.consumed, 0);

        // 待つのは接続の期限の残りまで。新しい期限を足さない
        let timeout = Duration::from_millis(100);
        let mut slow = SlowStream::new(b"abc", b"def", Duration::from_millis(1000));
        let started = Instant::now();
        std::thread::sleep(Duration::from_millis(40));
        assert_eq!(discard_until_closed(&mut slow, started, timeout), 3);
        assert!(started.elapsed() < Duration::from_millis(700), "{:?}", started.elapsed());
        assert!(slow.limits_seen.iter().all(|limit| *limit <= Duration::from_millis(60)));

        // 読み取りの失敗（相手のリセット、タイムアウト）では、そこでやめる
        let mut stalled = StallingStream {
            prefix: std::io::Cursor::new(b"rest".to_vec()),
            output: Vec::new(),
        };
        assert_eq!(discard_until_closed(&mut stalled, Instant::now(), IO_TIMEOUT), 4);
    }

    // ---- ヘッダの読み捨て（実際のTCP） ----

    /// 相手が閉じるか失敗するまで読み、(届いたバイト列, 失敗の内容) を返す
    /// (途中で接続が切られた場合も、そこまでに届いた分を捨てない)
    fn read_until_closed(
        stream: &mut std::net::TcpStream,
    ) -> (Vec<u8>, Option<std::io::Error>) {
        let mut received = Vec::new();
        let mut chunk = [0; 64 * 1024];
        loop {
            match stream.read(&mut chunk) {
                Ok(0) => return (received, None),
                Ok(n) => received.extend_from_slice(&chunk[..n]),
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(e) => return (received, Some(e)),
            }
        }
    }

    #[test]
    fn large_body_arrives_in_full_after_long_headers() {
        // 読み残しのあるソケットを閉じると、未送信の本文が捨てられて応答が途中で切れる
        // ヘッダを読み切ってから応答すれば、数MBの本文も最後まで届く
        let cache = widest_cache(64, 600);
        let len = io_len(&cache);
        assert!(len > 3_000_000, "{}", len);
        let addr = spawn_server_with(cache, IO_TIMEOUT);

        let request = format!(
            "GET /usages/io HTTP/1.1\r\nHost: x\r\nCookie: {}\r\n\r\n",
            "c".repeat(3000),
        );
        let mut stream = connect(addr);
        stream.write_all(request.as_bytes()).unwrap();
        let (received, error) = read_until_closed(&mut stream);
        assert!(
            error.is_none(),
            "the response was cut off after {} bytes: {:?}", received.len(), error,
        );

        let response = String::from_utf8(received).unwrap();
        let (status, headers, body) = split(&response);
        assert_eq!(status, "HTTP/1.1 200 OK");
        assert_common_headers(headers, body);
        assert_eq!(body.len(), len);
        assert_eq!(json::parse(body).unwrap().len(), 64);
    }

    #[test]
    fn error_responses_arrive_after_long_headers_over_tcp() {
        let addr = spawn_server(IO_TIMEOUT);
        let cookie = format!("Cookie: {}\r\n", "c".repeat(3000));
        for (line, status) in [
            ("GET /unknown HTTP/1.1", "HTTP/1.1 404 Not Found"),
            ("DELETE /containers HTTP/1.1", "HTTP/1.1 405 Method Not Allowed"),
            ("garbage", "HTTP/1.1 400 Bad Request"),
        ] {
            let request = format!("{}\r\nHost: x\r\n{}\r\n", line, cookie);
            assert_status(&tcp_exchange(addr, request.as_bytes()), status);
        }
        // 長すぎるリクエスト行でも、ヘッダを読み切ってから400を返す
        let long_line = format!(
            "GET /{} HTTP/1.1\r\n{}\r\n", "a".repeat(MAX_REQUEST_LINE_BYTES * 2), cookie,
        );
        assert_status(&tcp_exchange(addr, long_line.as_bytes()), "HTTP/1.1 400 Bad Request");
    }

    #[test]
    fn too_large_request_head_is_431_over_tcp() {
        let addr = spawn_server(IO_TIMEOUT);

        // 上限を1バイトだけ超える（サーバは届いた分をすべて読んでから431を返す）
        let response = tcp_exchange(addr, &request_of_exactly(MAX_REQUEST_HEAD_BYTES + 1));
        assert_status(&response, "HTTP/1.1 431 Request Header Fields Too Large");
        assert_eq!(split(&response).2, "");
        // 上限ちょうどは通る
        assert_json_ok(&tcp_exchange(addr, &request_of_exactly(MAX_REQUEST_HEAD_BYTES)));
    }

    #[test]
    fn headers_split_across_writes_are_served_over_tcp() {
        let addr = spawn_server(IO_TIMEOUT);

        let mut stream = connect(addr);
        for part in [
            &b"GET /cont"[..], b"ainers HTTP/1.1\r", b"\nHost: x\r\nCookie: abc",
            b"def\r\n\r", b"\n",
        ] {
            stream.write_all(part).unwrap();
            stream.flush().unwrap();
            std::thread::sleep(Duration::from_millis(20));
        }
        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();
        assert_json_ok(&response);
    }

    #[test]
    fn connection_stalled_in_headers_is_closed_after_timeout() {
        let addr = spawn_server(TEST_TIMEOUT);

        // リクエスト行は送るが、ヘッダの途中で止める
        let mut stream = connect(addr);
        let started = Instant::now();
        stream.write_all(b"GET /containers HTTP/1.1\r\nHost: x\r\nCookie: abc").unwrap();
        let mut response = String::new();
        // 期限まで待たされたうえで、応答なしで閉じられる
        stream.read_to_string(&mut response).unwrap();
        assert_eq!(response, "");
        assert!(started.elapsed() >= TEST_TIMEOUT / 2, "{:?}", started.elapsed());
        assert!(started.elapsed() < IO_TIMEOUT, "{:?}", started.elapsed());

        assert_json_ok(&tcp_get_containers(addr));
    }

    #[test]
    fn late_blank_line_is_not_served_over_tcp() {
        let addr = spawn_server(TEST_TIMEOUT);

        // 期限の少し前にリクエスト行だけ送り、期限を過ぎてから空行を送る
        // (期限がリクエスト行だけのものなら、ここで応答が返ってしまう)
        let mut stream = connect(addr);
        std::thread::sleep(TEST_TIMEOUT * 2 / 3);
        stream.write_all(b"GET /containers HTTP/1.1\r\nHost: x\r\n").unwrap();
        std::thread::sleep(TEST_TIMEOUT * 2 / 3);
        // サーバ側はすでに閉じているので、書き込みも読み取りも失敗しうる
        let _ = stream.write_all(b"\r\n");
        let (received, _) = read_until_closed(&mut stream);
        assert_eq!(received, b"");

        assert_json_ok(&tcp_get_containers(addr));
    }

    // ---- 応答後の読み捨て（実際のTCP） ----

    /// input を一気に送り、相手が閉じるまで読む。途中で切られたら失敗させる
    fn tcp_exchange_without_reset(addr: std::net::SocketAddr, input: &[u8]) -> String {
        let mut stream = connect(addr);
        stream.write_all(input).unwrap();
        let (received, error) = read_until_closed(&mut stream);
        assert!(
            error.is_none(),
            "the connection was reset after {} bytes: {:?}", received.len(), error,
        );
        String::from_utf8(received).unwrap()
    }

    #[test]
    fn error_responses_arrive_even_when_input_is_left_unread() {
        let addr = spawn_server(IO_TIMEOUT);

        // 上限を大きく超えるヘッダ（40KB を一気に送る）への431
        let huge = request_of_exactly(40 * 1024);
        assert_status(
            &tcp_exchange_without_reset(addr, &huge),
            "HTTP/1.1 431 Request Header Fields Too Large",
        );

        // 長すぎるリクエスト行への400。行のあとにヘッダと本文が続く場合と、
        // 改行が無いまま上限を超えて続く場合
        let long_line = format!(
            "POST /{} HTTP/1.1\r\nHost: x\r\n\r\n{}",
            "a".repeat(MAX_REQUEST_LINE_BYTES * 2),
            "b".repeat(10_000),
        );
        assert_status(
            &tcp_exchange_without_reset(addr, long_line.as_bytes()),
            "HTTP/1.1 400 Bad Request",
        );
        let endless_line = "a".repeat(40 * 1024);
        assert_status(
            &tcp_exchange_without_reset(addr, endless_line.as_bytes()),
            "HTTP/1.1 400 Bad Request",
        );

        // 本文付きのPOSTへの405
        let post = format!(
            "POST /containers HTTP/1.1\r\nHost: x\r\nContent-Length: 10000\r\n\r\n{}",
            "b".repeat(10_000),
        );
        assert_status(
            &tcp_exchange_without_reset(addr, post.as_bytes()),
            "HTTP/1.1 405 Method Not Allowed",
        );
    }

    #[test]
    fn large_body_arrives_in_full_when_more_input_follows_the_request() {
        // 空行のあとに続けて送られた2本目のリクエストは、処理せずに読み捨てる
        // 読み残しにしないので、1本目の本文は最後まで届く
        let cache = widest_cache(64, 600);
        let len = io_len(&cache);
        assert!(len > 3_000_000, "{}", len);
        let addr = spawn_server_with(cache, IO_TIMEOUT);

        let request = format!(
            "GET /usages/io HTTP/1.1\r\nHost: x\r\n\r\n{}",
            "GET /containers HTTP/1.1\r\nHost: x\r\n\r\n".repeat(200),
        );
        let response = tcp_exchange_without_reset(addr, request.as_bytes());
        let (status, headers, body) = split(&response);
        assert_eq!(status, "HTTP/1.1 200 OK");
        assert_common_headers(headers, body);
        assert_eq!(body.len(), len);
    }

    /// 本文付きのPOSTを送ったあと、interval ごとに chunk バイトずつ送り続ける
    /// 相手に閉じられて書き込みが失敗するまでの時間を返す（give_up まで待つ）
    fn keep_sending_until_closed(
        addr: std::net::SocketAddr,
        chunk: usize,
        interval: Duration,
        give_up: Duration,
    ) -> Option<Duration> {
        let mut stream = connect(addr);
        stream.set_write_timeout(Some(give_up)).unwrap();
        let started = Instant::now();
        stream.write_all(b"POST /containers HTTP/1.1\r\nHost: x\r\n\r\n").ok()?;
        let data = vec![b'b'; chunk];
        while started.elapsed() < give_up {
            if stream.write_all(&data).is_err() {
                return Some(started.elapsed());
            }
            std::thread::sleep(interval);
        }
        None
    }

    #[test]
    fn endless_senders_are_closed_within_the_deadline_and_free_their_slots() {
        let addr = spawn_server(TEST_TIMEOUT);
        let give_up = Duration::from_secs(4);
        assert!(give_up < IO_TIMEOUT);

        // 枠を埋める数の相手が、応答のあとも送り続ける
        // 半分は速く送り（バイト数の上限に達する）、半分は少しずつ送る（期限に達する）
        let senders: Vec<_> = (0..MAX_CONNECTIONS).map(|i| {
            std::thread::spawn(move || if i % 2 == 0 {
                keep_sending_until_closed(addr, 16 * 1024, Duration::ZERO, give_up)
            } else {
                keep_sending_until_closed(addr, 16, Duration::from_millis(20), give_up)
            })
        }).collect();
        for sender in senders {
            let closed_after = sender.join().unwrap()
                .expect("the server should close the connection");
            assert!(closed_after < give_up, "{:?}", closed_after);
        }

        // 枠は返っていて、次のリクエストは待たされずに処理される
        let started = Instant::now();
        assert_json_ok(&tcp_get_containers(addr));
        assert!(started.elapsed() < Duration::from_secs(2), "{:?}", started.elapsed());
    }
}
