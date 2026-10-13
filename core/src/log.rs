
use std::collections::HashMap;

use json;
use super::error;
use super::log_cache;
use super::time;

const DAILY_LOG_PATH: &str = "./log/log_daily";

const DOCKER_API_CONTAINERS: &str = "/containers/json";
const DOCKER_API_STATS: &str = "/containers/{}/stats?stream=false&one-shot=true";

/// Docker ソケットへの1回の問い合わせに掛けてよい時間の上限
/// 1回の読み書きの待ち時間の上限と、接続してから応答を読み切るまでの合計の上限を兼ねる
/// (dockerd が応答を返さないときに、logger が止まり続けないようにするため)
const DOCKER_IO_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

/// 収集の間隔
const TICK: std::time::Duration = std::time::Duration::from_secs(10);

/// 時刻が進まない統計が、同じコンテナでこの回数だけ続いたら、
/// 前回の統計を今回の値に置き換える
const MAX_STALLED_TICKS: u8 = 3;

/// Docker が稀に返す「時刻が入っていない」ことを表す値
const ZERO_TIME: &str = "0001-01-01T00:00:00Z";

//
// resource usage data structures
//
#[derive(Debug)]
pub struct CpuStats {
    /// 積算値（ns）。丸めずに持つ
    total: Option<u64>,
    /// 積算値（ns）。丸めずに持つ
    system: Option<u64>,
    ncpu: Option<u8>, // more than 256 cores??
}
#[derive(Debug)]
pub struct MemoryStats {
    used: Option<u64>,
    available: Option<u64>,
}
#[derive(Debug)]
pub struct IoStats {
    /// 全デバイスの合計
    read: Option<u64>,
    write: Option<u64>,
    /// デバイス（major:minor）ごとの積算値。差分はデバイスごとに取る
    read_by_device: Vec<(String, u64)>,
    write_by_device: Vec<(String, u64)>,
}
#[derive(Debug)]
pub struct NetStats {
    /// 全インターフェースの合計
    send: Option<u64>,
    recv: Option<u64>,
    /// インターフェースごとの積算値。差分はインターフェースごとに取る
    send_by_interface: Vec<(String, u64)>,
    recv_by_interface: Vec<(String, u64)>,
}
#[derive(Debug)]
pub struct Stats {
    time: Option<String>,
    cpu: CpuStats,
    memory: MemoryStats,
    io: IoStats,
    net: NetStats,
}
impl Default for Stats {
    fn default() -> Self {
        Stats {
            time: None,
            cpu: CpuStats { total: None, system: None, ncpu: None, },
            memory: MemoryStats { used: None, available: None, },
            io: IoStats {
                read: None, write: None,
                read_by_device: Vec::new(), write_by_device: Vec::new(),
            },
            net: NetStats {
                recv: None, send: None,
                recv_by_interface: Vec::new(), send_by_interface: Vec::new(),
            },
        }
    }
}
#[derive(Debug)]
pub struct CpuUsage {
    percentage: Option<f32>,
    total: Option<u64>,
    system: Option<u64>,
    ncpu: Option<u8>,
}
/// 値を json の数値にします。小数は2桁に丸め、整数はそのまま出します
/// (整数型では精度の指定が無視される)。値が無ければ null です
pub fn option_to_string<T>(value: Option<T>) -> String 
where
    T: ToString + std::fmt::Display
{
    match value {
        Some(n) => {
            format!("{:.2}", n)
        },
        None => "null".to_string(),
    }
}
impl std::fmt::Display for CpuUsage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{{\"percentage\":{0},\"total\":{1},\"system\":{2},\"ncpu\":{3}}}",
            option_to_string(self.percentage),
            option_to_string(self.total),
            option_to_string(self.system),
            option_to_string(self.ncpu),
        )
    }
}
impl Default for CpuUsage {
    fn default() -> Self {
        CpuUsage { percentage: None, total: None, system: None, ncpu: None }
    }
}
#[derive(Debug)]
pub struct MemoryUsage {
    percentage: Option<f32>,
    used: Option<u64>,
    available: Option<u64>,
}
impl std::fmt::Display for MemoryUsage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{{\"percentage\":{0},\"used\":{1},\"available\":{2}}}",
            option_to_string(self.percentage),
            option_to_string(self.used),
            option_to_string(self.available),
        )
    }
}
impl Default for MemoryUsage {
    fn default() -> Self {
        MemoryUsage { percentage: None, used: None, available: None }
    }
}
#[allow(non_snake_case)]
#[derive(Debug)]
pub struct IoUsage {
    readkB: Option<u64>,
    writekB: Option<u64>,
    readkBps: Option<f32>,
    writekBps: Option<f32>,
}
impl std::fmt::Display for IoUsage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{{\"readkB\":{0},\"writekB\":{1},\"readkBps\":{2},\"writekBps\":{3}}}",
            option_to_string(self.readkB),
            option_to_string(self.writekB),
            option_to_string(self.readkBps),
            option_to_string(self.writekBps),
        )
    }
}
impl Default for IoUsage {
    fn default() -> Self {
        IoUsage { readkB: None, writekB: None, readkBps: None, writekBps: None }
    }
}
#[allow(non_snake_case)]
#[derive(Debug)]
pub struct NetUsage {
    recvkB: Option<u64>,
    sendkB: Option<u64>,
    recvkBps: Option<f32>,
    sendkBps: Option<f32>,
}
impl std::fmt::Display for NetUsage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{{\"recvkB\":{0},\"sendkB\":{1},\"recvkBps\":{2},\"sendkBps\":{3}}}",
            option_to_string(self.recvkB),
            option_to_string(self.sendkB),
            option_to_string(self.recvkBps),
            option_to_string(self.sendkBps),
        )
    }
}
impl Default for NetUsage {
    fn default() -> Self {
        NetUsage { recvkB: None, sendkB: None, recvkBps: None, sendkBps: None }
    }
}
#[derive(Debug)]
pub struct Usage {
    cpu: CpuUsage,
    memory: MemoryUsage,
    io: IoUsage,
    net: NetUsage,
}
impl std::fmt::Display for Usage {
    fn fmt(
        &self, 
        f: &mut std::fmt::Formatter<'_>
    ) -> std::fmt::Result {
        write!(
            f,
            "{{\"cpu\":{cpu},\"memory\":{memory},\"io\":{io},\"net\":{net}}}",
            cpu = self.cpu,
            memory = self.memory,
            io = self.io,
            net = self.net,
        )
    }
}
#[allow(non_snake_case)]
#[derive(Debug)]
pub struct Usages {
    time: String,
    millis: u16,
    usages: HashMap<String, Usage>,
}
impl std::fmt::Display for Usages {
    fn fmt(
        &self, 
        f: &mut std::fmt::Formatter<'_>
    ) -> std::fmt::Result {
        let containers_part = self.usages
            .iter()
            .map(|(k, v)| format!("\"{k}\":{v}", k = k, v = v))
            .collect::<Vec<String>>()
            .join(",")
            ;
            
        write!(
            f,
            "{{\"time\":\"{time}\",\"millis\":{millis},\"stats\":{{{containers_part}}}}}",
            time = self.time, 
            millis = self.millis,
        )
    }
}

/// Docker ソケットに GET を1回送り、応答（ヘッダ込み）を文字列で返します
///
/// 読み書きには DOCKER_IO_TIMEOUT のタイムアウトを掛け、
/// 接続してから応答を読み切るまでの合計も同じ時間で打ち切ります
fn call_docker_api<
    P: AsRef<std::path::Path>,
    S: AsRef<str>,
>(
    socket_path: P,
    url: S,
) -> Result<String, std::io::Error> {
    let mut stream = 
        std::os::unix::net::UnixStream::connect(socket_path)?;
    let started = std::time::Instant::now();
    stream.set_read_timeout(Some(DOCKER_IO_TIMEOUT))?;
    stream.set_write_timeout(Some(DOCKER_IO_TIMEOUT))?;
    let request = format!(
        "GET {} HTTP/1.1\r\n\
         Host: localhost\r\n\
         Connection: close\r\n\
         \r\n",
        url.as_ref()
    );

    std::io::Write::write_all(&mut stream, request.as_bytes())?;

    // 応答を読むのに使えるのは、接続と送信に掛かった分を引いた残りの時間
    read_response_within(
        &mut stream,
        DOCKER_IO_TIMEOUT.saturating_sub(started.elapsed()),
        |stream, remaining| stream.set_read_timeout(Some(remaining)),
    )
}

/// 相手が閉じるまで応答を読みます。読み切るまでの合計が timeout を超えたら打ち切ります
///
/// readごとのタイムアウトだけでは、少しずつ返し続けられると合計が延びるので、
/// readのたびに limit_next_read で残り時間を渡し、その時間だけ待たせます
/// 期限を過ぎてから届いた分は（EOFであっても）受け付けません
/// readの失敗（ソケットのタイムアウトを含む）は、そのままエラーとして返します
fn read_response_within<R, L>(
    stream: &mut R,
    timeout: std::time::Duration,
    mut limit_next_read: L,
) -> Result<String, std::io::Error>
where
    R: std::io::Read,
    L: FnMut(&mut R, std::time::Duration) -> std::io::Result<()>,
{
    let timed_out = || std::io::Error::new(
        std::io::ErrorKind::TimedOut,
        "timed out while reading the response from docker",
    );
    let started = std::time::Instant::now();
    let mut response: Vec<u8> = Vec::new();
    let mut chunk = [0; 8192];

    loop {
        let remaining = timeout.saturating_sub(started.elapsed());
        if remaining.is_zero() {
            return Err(timed_out());
        }
        limit_next_read(stream, remaining)?;
        let n = match stream.read(&mut chunk) {
            Ok(n) => n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        };
        if started.elapsed() > timeout {
            return Err(timed_out());
        }
        if n == 0 {
            break;
        }
        response.extend_from_slice(&chunk[..n]);
    }

    String::from_utf8(response).map_err(|e| std::io::Error::new(
        std::io::ErrorKind::InvalidData, e.utf8_error(),
    ))
}

fn get_container_names<T: AsRef<std::path::Path>>(
    socket_path: T,
) -> Result<Vec<String>, error::Error> {
    let response = call_docker_api(
        socket_path, DOCKER_API_CONTAINERS,
    )?;
    let mut lines = response.lines();
    let body = lines.find(|l| l.starts_with('['))
        .ok_or("cannot find json body")?;
    let json_body = json::parse(body)?;
    
    let members = json_body.members();
    let mut failed_to_get_name = false;
    let container_names: Vec<String> = members.map(
        |ref m| m["Names"][0].as_str()
            .unwrap_or_else(|| {
                failed_to_get_name = true;
                ""
            })
            .replace("/", "")
            .to_string()
    ).collect();

    if failed_to_get_name {
        return Err(error::Error::OtherError(
            "failed to get container name.".to_string()
        ));
    }

    Ok(container_names)
}

fn get_container_stats<
    T: AsRef<std::path::Path>,
    U: AsRef<str>,
>(
    socket_path: T,
    container_name: U,
) -> Result<Stats, error::Error> {
    let response = call_docker_api(
        socket_path,
        DOCKER_API_STATS.replace("{}", container_name.as_ref()),
    )?;

    parse_stats_response(&response)
}

/// Docker の stats API の応答（ヘッダ込みの文字列）から統計を取り出します
fn parse_stats_response(response: &str) -> Result<Stats, error::Error> {
    let stats_data = response.lines()
        .find(|l| l.starts_with('{'))
        .ok_or("cannot find response json body")?;
    let stats = json::parse(stats_data)?;

    Ok(reshape_json(&stats))
}

/// メモリの使用量（バイト）を、docker stats と同じ判定で求めます
///
/// usage からページキャッシュのうち回収しやすい分（inactive file）を引きます
/// - cgroup v1: stats.total_inactive_file
/// - cgroup v2: stats.inactive_file
/// 引く値が無い、または usage 以上なら、その値は使いません
fn used_memory(memory_stats: &json::JsonValue) -> Option<u64> {
    let usage = memory_stats["usage"].as_u64()?;
    let inactive_file = ["total_inactive_file", "inactive_file"].iter()
        .filter_map(|key| memory_stats["stats"][*key].as_u64())
        .find(|v| *v < usage);

    Some(usage - inactive_file.unwrap_or(0))
}

/// 内訳（名前ごとの積算値）の合計を返します。内訳が無ければ None です
fn total_of(parts: &[(String, u64)]) -> Option<u64> {
    if parts.is_empty() {
        return None;
    }
    Some(parts.iter().map(|(_, v)| *v).fold(0_u64, u64::saturating_add))
}

/// インターフェースごとの積算値（バイト）を取り出します
/// networks が無ければ（ホストネットワークなど）空です
fn network_counters(networks: &json::JsonValue, key: &str) -> Vec<(String, u64)> {
    networks.entries()
        .filter_map(|(name, interface)|
            interface[key].as_u64().map(|v| (name.to_string(), v))
        )
        .collect()
}

/// デバイス（major:minor）ごとの積算値（バイト）のうち、op が一致するものを取り出します
/// op は大文字小文字を区別しません（cgroup v1 は Read / Write、v2 は read / write）
/// 一致する要素が1つも無ければ（配列が無い・null・空の場合を含む）空です
fn blkio_counters(entries: &json::JsonValue, op: &str) -> Vec<(String, u64)> {
    let mut counters: Vec<(String, u64)> = Vec::new();
    for entry in entries.members() {
        if !entry["op"].as_str().is_some_and(|v| v.eq_ignore_ascii_case(op)) {
            continue;
        }
        let Some(value) = entry["value"].as_u64() else { continue; };
        let device = format!("{}:{}", entry["major"], entry["minor"]);
        // 同じデバイスの要素が複数あれば足す
        match counters.iter_mut().find(|(name, _)| *name == device) {
            Some((_, sum)) => *sum = sum.saturating_add(value),
            None => counters.push((device, value)),
        }
    }

    counters
}

/// 内訳ごとの差分を合算して、前回からの増分（バイト）を求めます
///
/// 合計同士の差ではなく、名前（インターフェース・デバイス）ごとの差を足します
/// 合計の差だと、大きな積算値を持つものが外れた tick で合計が減り、
/// 残ったものの増分まで 0 になってしまうためです
/// - 両方にあるもの: 差分を足す（積算値が戻っていたら、そのものの差分だけを 0 とする）
/// - 今回だけにあるもの（追加された）: 0 から数え始めるので、今回の値を足す
/// - 前回だけにあるもの（外された）: 数えない
///
/// 今回の内訳が無ければ None です（値そのものが無い）
/// 前回の内訳が無いだけなら、今回のものはすべて「追加された」ものとして数えます
/// (まだ一度も読み書きしていないコンテナは、IO の要素を1つも返さない。
///  前回の統計そのものが無いコンテナは、呼び出し側が先に飛ばしている)
fn increase_of(
    current: &[(String, u64)],
    previous: &[(String, u64)],
) -> Option<u64> {
    if current.is_empty() {
        return None;
    }
    Some(
        current.iter()
            .map(|(name, value)| {
                let before = previous.iter()
                    .find(|(prev_name, _)| prev_name == name)
                    .map_or(0, |(_, prev_value)| *prev_value);
                value.saturating_sub(before)
            })
            .fold(0_u64, u64::saturating_add)
    )
}

fn reshape_json(
    json: &json::JsonValue,
) -> Stats {
    // ns のまま持つ（ms に丸めると、1 tick の差分が 1ms 未満のときに 0 になる）
    let total =
        json["cpu_stats"]["cpu_usage"]["total_usage"]
        .as_u64();
    let system =
        json["cpu_stats"]["system_cpu_usage"]
        .as_u64();
    let number_cpus =
        json["cpu_stats"]["online_cpus"]
        .as_u16()
        .map(|n| n as u8);
    let used_memory = used_memory(&json["memory_stats"]);
    let available_memory =
        json["memory_stats"]["limit"]
        .as_u64();
    let net_rx = network_counters(&json["networks"], "rx_bytes");
    let net_tx = network_counters(&json["networks"], "tx_bytes");
    let blkio = &json["blkio_stats"]["io_service_bytes_recursive"];
    let blkio_read = blkio_counters(blkio, "read");
    let blkio_write = blkio_counters(blkio, "write");
    let time = json["read"].as_str();

    Stats {
        time: time.map(|s| s.to_string()), 
        cpu: CpuStats { 
            total, system, 
            ncpu: number_cpus
        },
        memory: MemoryStats {
            used: used_memory, 
            available: available_memory
        },
        io: IoStats {
            read: total_of(&blkio_read),
            write: total_of(&blkio_write),
            read_by_device: blkio_read,
            write_by_device: blkio_write,
        },
        net: NetStats {
            send: total_of(&net_tx),
            recv: total_of(&net_rx),
            send_by_interface: net_tx,
            recv_by_interface: net_rx,
        }
    }
}

/// 1 tick 分の収集結果
struct Collected {
    /// その tick のコンテナ一覧に載っていた名前（統計を採れなかったものも含む）
    listed: Vec<String>,
    /// 統計を採れたコンテナ
    stats: HashMap<String, Stats>,
}

fn get_containers_stats<T: AsRef<std::path::Path>>(
    socket_path: T,
) -> Result<Collected, error::Error> {
    // 一覧を取れなければ、その tick は失敗として扱う
    let container_names = get_container_names(&socket_path)?;

    // in one-shot mode, pre-stats are not available.
    // we have to take diff by ourselves
    Ok(collect_stats(
        container_names,
        |container_name| get_container_stats(&socket_path, container_name),
    ))
}

/// コンテナごとに統計を取り、採れたものだけを集めます
///
/// 次のコンテナは、そのコンテナだけを飛ばします（他のコンテナの収集は続ける）
/// - 統計の取得に失敗した
/// - 時刻が ZERO_TIME である、または時刻を解釈できない
fn collect_stats<F: FnMut(&str) -> Result<Stats, error::Error>>(
    container_names: Vec<String>,
    mut fetch: F,
) -> Collected {
    let mut stats_map: HashMap<String, Stats>
         = HashMap::new();
    for container_name in &container_names {
        match fetch(container_name) {
            Ok(stats) => {
                // it's terrible, docker api sometimes returns unix epoc ZERO.
                let usable = stats.time.as_deref()
                    .and_then(valid_time_millis)
                    .is_some();
                if usable {
                    stats_map.insert(container_name.clone(), stats);
                }
            },
            Err(e) => {
                eprintln!(
                    "failed to get stats of {}: {}", container_name, e
                );
            },
        }
    }

    Collected { listed: container_names, stats: stats_map }
}

/// 時刻が前回から進んでいない（同じ、または戻った）統計なら true を返します
/// 前回の統計が無いコンテナは false です
fn is_stalled(prev_stats: &HashMap<String, Stats>, name: &str, stats: &Stats) -> bool {
    let time_millis = |stats: &Stats| {
        stats.time.as_deref().and_then(valid_time_millis)
    };
    prev_stats.get(name)
        .and_then(time_millis)
        .zip(time_millis(stats))
        .is_some_and(|(prev, current)| current <= prev)
}

/// この tick で前回の統計を置き換えるコンテナについて、core が測った経過時間を返します
///
/// 対象は、時刻が進まない統計が MAX_STALLED_TICKS 回目になるコンテナです
/// Docker の時刻の差は使えない（0 以下）ので、前回の統計を採った時点からの
/// 単調時計での経過時間を代わりに使い、置き換える tick でも1点を記録します
/// (記録せずに置き換えると、そこまでの積算値の増分が記録から落ちる)
fn stalled_elapsed_millis(
    state: &LoggerState,
    stats: &HashMap<String, Stats>,
    monotonic_millis: u64,
) -> HashMap<String, u64> {
    stats.iter()
        .filter(|(name, stats)| is_stalled(&state.prev_stats, name, stats))
        .filter(|(name, _)|
            state.stalled_ticks.get(*name).copied().unwrap_or(0) + 1
                >= MAX_STALLED_TICKS
        )
        .filter_map(|(name, _)| {
            let taken_at = state.taken_at.get(name)?;
            let elapsed = monotonic_millis.checked_sub(*taken_at)?;
            (elapsed > 0).then(|| (name.clone(), elapsed))
        })
        .collect()
}

/// 一覧に載っていないコンテナの状態を消します
/// (止まったコンテナの値を持ち続けない)
fn forget_unlisted(state: &mut LoggerState, listed: &[String]) {
    state.prev_stats.retain(|name, _| listed.contains(name));
    state.stalled_ticks.retain(|name, _| listed.contains(name));
    state.taken_at.retain(|name, _| listed.contains(name));
}

/// 次の tick に向けて、前回の統計を更新します
///
/// - 時刻が前回より進んだ統計を採れたコンテナは、今回の値に置き換える
/// - 一覧に載っているが採れなかったコンテナは、前回の値をそのまま残す
///   (次に採れた tick で、実際の経過時間を使って1点を記録できる)
/// - 採れたが時刻が前回から進んでいない（同じ、または戻った）コンテナも、
///   前回の値をそのまま残す
///   (その tick は記録されないので、置き換えると積算値の増分が記録から落ちる)
/// - ただし、時刻が進まない統計が MAX_STALLED_TICKS 回続いたら、今回の値に置き換える
///   (時計が大きく巻き戻された場合に、追いつくまで記録が止まり続けないように)
///   置き換えるのは、その tick を stalled_elapsed に載せて記録できた場合だけ
/// - 一覧に載っていないコンテナは消す
fn carry_over_stats(
    state: &mut LoggerState,
    collected: Collected,
    monotonic_millis: u64,
    stalled_elapsed: &HashMap<String, u64>,
) {
    forget_unlisted(state, &collected.listed);

    for (name, stats) in collected.stats {
        if is_stalled(&state.prev_stats, &name, &stats)
            && !stalled_elapsed.contains_key(&name)
        {
            let count = state.stalled_ticks.entry(name).or_insert(0);
            *count = (*count + 1).min(MAX_STALLED_TICKS - 1);
            continue;
        }
        // 記録できたので、数え直す
        state.stalled_ticks.remove(&name);
        state.taken_at.insert(name.clone(), monotonic_millis);
        state.prev_stats.insert(name, stats);
    }
}

/// logger が tick をまたいで持つ状態
struct LoggerState {
    /// 差分の基準にする、コンテナごとの前回の統計
    prev_stats: HashMap<String, Stats>,
    /// 時刻が進まない統計が続いている回数（続いているコンテナだけが入る）
    /// 統計を採れなかった tick では変えない
    stalled_ticks: HashMap<String, u8>,
    /// 前回の統計を採った時点（単調時計のミリ秒）
    taken_at: HashMap<String, u64>,
}

impl LoggerState {
    fn new() -> Self {
        LoggerState {
            prev_stats: HashMap::new(),
            stalled_ticks: HashMap::new(),
            taken_at: HashMap::new(),
        }
    }
}

/// 1 tick 分の処理: 算出 → ログへ追記 → キャッシュへ追加 → 古い点の削除
///
/// 記録したら true、記録するものが無ければ false を返します
/// 失敗した tick は何も記録せずにエラーを返します。呼び出し側は次の tick へ進みます
/// - 収集（コンテナ一覧の取得）に失敗した場合、前回の統計は変えません
/// - ログへの追記に失敗した場合、キャッシュにも入れず、前回の統計も進めません
///   （ログとキャッシュの内容を揃え、増分を次に記録できた点に含めるため）
///
/// now_millis（現在時刻）が分かっていれば、記録の有無や成否に関わらず、
/// 現在時刻から見て古くなった点をキャッシュから捨てます
/// (コンテナが1つも動いていない間も、止まったコンテナが24時間で消えるように)
///
/// monotonic_millis は、統計を採った時点の単調時計の値です
/// 時刻が進まない統計が続いたコンテナの経過時間を測るのに使います
fn run_tick<W: FnOnce(&str) -> Result<(), error::Error>>(
    millis: u16,
    now_millis: Option<i64>,
    monotonic_millis: u64,
    collected: Result<Collected, error::Error>,
    state: &mut LoggerState,
    write_log: W,
    log_cache: &log_cache::SharedUsageCache,
) -> Result<bool, error::Error> {
    let result = record_tick(
        millis, monotonic_millis, collected, state, write_log, log_cache,
    );
    if let Some(now_millis) = now_millis {
        log_cache.write()
            .expect("failed to get write lock for log_cache")
            .prune_at(now_millis);
    }

    result
}

fn record_tick<W: FnOnce(&str) -> Result<(), error::Error>>(
    millis: u16,
    monotonic_millis: u64,
    collected: Result<Collected, error::Error>,
    state: &mut LoggerState,
    write_log: W,
    log_cache: &log_cache::SharedUsageCache,
) -> Result<bool, error::Error> {
    let collected = collected?;

    // 起動後の最初の tick は前回の統計が無いので、何も記録されない
    let stalled_elapsed = stalled_elapsed_millis(
        state, &collected.stats, monotonic_millis,
    );
    let usages = calc_usages_with(
        millis, &collected.stats, &state.prev_stats, &stalled_elapsed,
    );
    if let Some(usages) = &usages {
        if let Err(e) = write_log(&usages.to_string()) {
            // 前回の統計を進めない。増分は、次に記録できた点に含まれる
            forget_unlisted(state, &collected.listed);
            return Err(e);
        }
    }
    carry_over_stats(state, collected, monotonic_millis, &stalled_elapsed);
    let Some(usages) = usages else { return Ok(false); };

    let mut lock = log_cache.write()
        .expect("failed to get write lock for log_cache");
    store_usages(&usages, &mut lock);

    Ok(true)
}

/// now_millis より後の、最初の tick の境界（エポックからのミリ秒）
/// now_millis がちょうど境界なら、その次の境界です
fn next_boundary_after(now_millis: u128, tick_millis: u128) -> u128 {
    let tick_millis = tick_millis.max(1);
    now_millis - now_millis % tick_millis + tick_millis
}

/// 次の tick までの待ち時間と、その tick の時刻を決めます
///
/// timing_millis は予定していた次の tick の時刻です（前回の tick の時刻 + tick）
/// 現在時刻より後で、tick 以内に来るなら、そのまま使います
/// そうでなければ、現在時刻より後の最初の境界に合わせ直します
/// - 予定の時刻が現在時刻以下: 処理が tick より長引いた、または時計が進んだ
///   遅れた分の tick は取り戻さない（待ちなしで連続して実行しない）
/// - 予定の時刻が tick より先: 時計が戻った
///   予定の時刻まで待たない（その間、収集も古い点の削除も止まってしまう）
///
/// 返す待ち時間は 0 より長く、tick 以下です
fn next_tick(
    now_millis: u128,
    timing_millis: u128,
    tick_millis: u128,
) -> (std::time::Duration, u128) {
    let tick_millis = tick_millis.max(1);
    let on_schedule = timing_millis > now_millis
        && timing_millis - now_millis <= tick_millis;
    let timing_millis = if on_schedule {
        timing_millis
    } else {
        next_boundary_after(now_millis, tick_millis)
    };
    // 差は tick 以下なので u64 に収まる（tick が極端に大きい場合だけ丸める）
    let wait_millis = u64::try_from(timing_millis - now_millis).unwrap_or(u64::MAX);

    (std::time::Duration::from_millis(wait_millis), timing_millis)
}

fn get_now_as_millis() -> Result<u128, std::time::SystemTimeError> {
    let duration = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?;
    Ok(duration.as_millis())
}

/// 記録に使える時刻なら、エポックからのミリ秒にして返します
/// 解釈できない時刻と ZERO_TIME（エポックより前）は None にします
fn valid_time_millis(time_str: &str) -> Option<i64> {
    if time_str == ZERO_TIME {
        return None;
    }
    time::iso8601_to_millis(time_str).ok().filter(|millis| *millis > 0)
}

/// 増分（バイト）を、実測の経過時間で割って kB/s にします
/// bytes / ms = kB / s（1 kB = 1000 バイト）
/// 切り捨てずに小数で返します（整数にすると 1 kB/s 未満が 0 になる）
fn rate_kb_per_s(
    current: &[(String, u64)],
    previous: &[(String, u64)],
    elapsed_millis: u64,
) -> Option<f32> {
    increase_of(current, previous)
        .map(|bytes| (bytes as f64 / elapsed_millis as f64) as f32)
        .filter(|rate| rate.is_finite())
}

/// 前回の tick との差分から、1 tick 分の使用率を求めます
///
/// 次のコンテナは、その tick では記録しません（次の tick から記録される）
/// - 前回の統計が無い
/// - 今回または前回の時刻を解釈できない
/// - Docker が返した時刻が前回から進んでいない
///
/// 記録できるコンテナが1つも無ければ None を返します
#[cfg(test)]
fn calc_usages(
    millis: u16,
    stats: &HashMap<String, Stats>,
    prev_stats: &HashMap<String, Stats>,
) -> Option<Usages> {
    calc_usages_with(millis, stats, prev_stats, &HashMap::new())
}

/// calc_usages と同じですが、Docker が返した時刻が前回から進んでいないコンテナでも、
/// stalled_elapsed に経過時間（core が測ったミリ秒）があれば、それを使って記録します
fn calc_usages_with(
    millis: u16,
    stats: &HashMap<String, Stats>,
    prev_stats: &HashMap<String, Stats>,
    stalled_elapsed: &HashMap<String, u64>,
) -> Option<Usages> {

    let mut usages: HashMap<String, Usage> = HashMap::new();
    // tick の時刻には、記録するコンテナのうち最も早く読めたものを使う
    // (時刻, コンテナ名, 元の文字列)。同時刻なら名前の順
    let mut tick_time: Option<(i64, &str, &str)> = None;
    // 時刻が進んでいないコンテナの時刻は、ほかに候補が無いときだけ使う
    // (古い時刻を tick の時刻にすると、全コンテナの点がキャッシュに入らなくなる)
    let mut stalled_tick_time: Option<(i64, &str, &str)> = None;

    for (container_name, stats) in stats {
        let Some(prev_stats) = prev_stats.get(container_name) else {
            continue;
        };
        let Some(time_str) = stats.time.as_deref() else { continue; };
        let Some(time_millis) = valid_time_millis(time_str) else { continue; };
        let Some(prev_time_millis) = prev_stats.time.as_deref()
            .and_then(valid_time_millis) else { continue; };
        // 経過時間は設定上の間隔ではなく、Docker が返した時刻の差を使う
        let docker_elapsed_millis = time_millis - prev_time_millis;
        let (elapsed_millis, tick_time) = if docker_elapsed_millis > 0 {
            (docker_elapsed_millis as u64, &mut tick_time)
        } else if let Some(elapsed) = stalled_elapsed.get(container_name) {
            (*elapsed, &mut stalled_tick_time)
        } else {
            continue;
        };
        if elapsed_millis == 0 {
            continue;
        }

        let candidate = (time_millis, container_name.as_str(), time_str);
        if tick_time.is_none_or(|current|
            (candidate.0, candidate.1) < (current.0, current.1)
        ) {
            *tick_time = Some(candidate);
        }

        // CPU calculations
        // 差分とその比は ns のまま計算する
        let cpu_delta = stats.cpu.total
           .zip(prev_stats.cpu.total)
           .map(|(a, b)| a.saturating_sub(b));
        let system_cpu_delta = stats.cpu.system
           .zip(prev_stats.cpu.system)
           .map(|(a, b)| a.saturating_sub(b));
        let cpu_percentage = cpu_delta
            .zip(system_cpu_delta)
            .zip(stats.cpu.ncpu)
            .map(|((a, b), c)|
                 ((a as f64) / (b as f64) * (c as f64) * 100_f64) as f32
            )
            // 分母が0だと NaN / inf になり、jsonとして書けない
            .filter(|p| p.is_finite());

        // Memory calculations
        let memory_percentage = stats.memory.used
            .zip(stats.memory.available)
            .map(|(a,b)| ((a as f64) / (b as f64) * 100_f64) as f32)
            .filter(|p| p.is_finite());

        // IO calculations
        let io_read_kb_per_s = rate_kb_per_s(
            &stats.io.read_by_device, &prev_stats.io.read_by_device, elapsed_millis,
        );
        let io_write_kb_per_s = rate_kb_per_s(
            &stats.io.write_by_device, &prev_stats.io.write_by_device, elapsed_millis,
        );

        // Net calculations
        let net_send_kb_per_s = rate_kb_per_s(
            &stats.net.send_by_interface, &prev_stats.net.send_by_interface, elapsed_millis,
        );
        let net_recv_kb_per_s = rate_kb_per_s(
            &stats.net.recv_by_interface, &prev_stats.net.recv_by_interface, elapsed_millis,
        );

        usages.insert(
            container_name.to_string(),
            Usage {
                cpu: CpuUsage {
                    percentage: cpu_percentage,
                    // ログには、差分を ms に丸めた整数で出す
                    total: cpu_delta.map(|ns| ns / 1_000_000),
                    system: system_cpu_delta.map(|ns| ns / 1_000_000),
                    ncpu: stats.cpu.ncpu,
                },
                memory: MemoryUsage {
                    percentage: memory_percentage,
                    used: stats.memory.used,
                    available: stats.memory.available,
                },
                io: IoUsage {
                    readkBps: io_read_kb_per_s,
                    writekBps: io_write_kb_per_s,
                    readkB: stats.io.read.map(|x| x / 1000),
                    writekB: stats.io.write.map(|x| x / 1000),
                },
                net: NetUsage {
                    sendkBps: net_send_kb_per_s,
                    recvkBps: net_recv_kb_per_s,
                    sendkB: stats.net.send.map(|x| x / 1000),
                    recvkB: stats.net.recv.map(|x| x / 1000),
                },
            }
        );
    }

    let (_, _, time) = tick_time.or(stalled_tick_time)?;

    Some(Usages { time: time.to_string(), millis, usages })
}

fn log_daily<T: AsRef<std::path::Path>, S: AsRef<str>>(
    file_path: T,
    content: S,
) -> Result<(), error::Error> {
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .write(true)
        .open(file_path)?;
    std::io::Write::write_all(
        &mut file, 
        ("".to_string() + content.as_ref() + "\r\n").as_bytes()
    )?;

    Ok(())
}

pub fn custom_dump(json: &json::JsonValue) -> String {
    match json {
        json::JsonValue::Object(o) => {
            let entries: Vec<String> = o.iter()
                .map(|(k,v)| format!("\"{}\":{}", k, custom_dump(v)))
                .collect();
            format!("{{{}}}", entries.join(","))
        },
        json::JsonValue::Array(a) => {
            let elements: Vec<String> = a.iter()
                .map(custom_dump)
                .collect();
            format!("[{}]", elements.join(","))
        }
        json::JsonValue::Number(n) => {
            let (_, _, exponent) = n.as_parts();
            if exponent != 0 {
                format!("{:.2}", json.as_f64().unwrap())
            } else {
                n.to_string()
            }
        }
        _ => json.dump(),
    }
}

/// 1 tick 分の使用率をキャッシュへ入れ、古くなった点を捨てます
/// tick の時刻を記録に使えなければ、何も入れずに false を返します
fn store_usages(
    usages: &Usages,
    log_cache: &mut log_cache::UsageCache,
) -> bool {
    let Some(time_ms) = valid_time_millis(&usages.time) else {
        return false;
    };
    for (container_name, usage) in &usages.usages {
        log_cache.insert(
            container_name,
            log_cache::UsagePoint {
                time_ms,
                cpu: usage.cpu.percentage,
                memory: usage.memory.percentage,
                read_kbps: usage.io.readkBps,
                write_kbps: usage.io.writekBps,
                recv_kbps: usage.net.recvkBps,
                send_kbps: usage.net.sendkBps,
            },
        );
    }
    // データが来なかったコンテナの古い点も、ここで捨てる
    log_cache.prune();

    true
}

pub fn log_json(
    log_cache: &log_cache::SharedUsageCache
) -> Result<(), error::Error> {
    let socket_path = "/var/run/docker.sock";

    // create log dir if not exists
    if std::fs::exists("./log")? {
        println!("./log directory exists.");
    } else {
        println!("creating ./log directory...");
        std::fs::create_dir("./log")?;
    }

    let tick = TICK;
    // 次の tick の時刻。壁時計の 10 秒境界に揃える
    let mut timing = next_boundary_after(get_now_as_millis()?, tick.as_millis());

    let mut state = LoggerState::new();
    // 経過時間を core 自身で測るための単調時計の起点
    let started = std::time::Instant::now();
    // ここから先の失敗では止まらない。その tick を飛ばして次へ進む
    loop {
        // 時計が飛んでいたら、tick の時刻を現在時刻に合わせ直す（next_tick）
        let wait = match get_now_as_millis() {
            Ok(now) => {
                let (wait, next) = next_tick(now, timing, tick.as_millis());
                timing = next;
                wait
            },
            Err(e) => {
                eprintln!("logger: failed to get the current time: {}", e);
                tick
            },
        };
        println!("waiting {} millis...", wait.as_millis());
        std::thread::sleep(wait);

        // 待ったあとの現在時刻。取れなければ、この tick では時間による削除をしない
        let now_millis = match get_now_as_millis() {
            Ok(now) => i64::try_from(now).ok(),
            Err(e) => {
                eprintln!(
                    "logger: failed to get the current time; \
                     skipping time-based pruning for this tick: {}", e,
                );
                None
            },
        };
        let collected = get_containers_stats(socket_path);
        let result = run_tick(
            tick.as_millis() as u16,
            now_millis,
            started.elapsed().as_millis() as u64,
            collected,
            &mut state,
            |line| log_daily(DAILY_LOG_PATH, line),
            log_cache,
        );
        if let Err(e) = result {
            eprintln!("logger: tick skipped: {}", e);
        }

        // 失敗した tick でも、次の tick の時刻は進める
        // ここまでの処理が tick より長引いていたら、次の周で合わせ直す
        timing += tick.as_millis();
    }
}

/// ログの1行を読みます
/// 速度（kBps）は、整数で書かれた古い行も、小数で書かれた行も読めます
fn json_to_usage(
    json: &json::JsonValue
) -> Result<Usages, error::Error> {

    Ok(Usages {
        time:
            json["time"].as_str()
                .ok_or("time entry not found".to_string())?
                .to_string(),
        millis:
            json["millis"].as_u16()
                .ok_or("millis entry not found")?,
        usages:
            json["stats"].entries()
                .map(|(k, v)| (k.to_string(), Usage {
                    cpu: CpuUsage {
                        percentage: v["cpu"]["percentage"].as_f32(),
                        total: v["cpu"]["total"].as_u64(),
                        system: v["cpu"]["system"].as_u64(),
                        ncpu: v["cpu"]["ncpu"].as_u8(),
                    },
                    memory: MemoryUsage {
                        percentage: v["memory"]["percentage"].as_f32(),
                        used: v["memory"]["used"].as_u64(),
                        available: v["memory"]["available"].as_u64(),
                    },
                    io: IoUsage {
                        readkB: v["io"]["readkB"].as_u64(),
                        writekB: v["io"]["writekB"].as_u64(),
                        readkBps: v["io"]["readkBps"].as_f32(),
                        writekBps: v["io"]["writekBps"].as_f32(),
                    },
                    net: NetUsage {
                        recvkB: v["net"]["recvkB"].as_u64(),
                        sendkB: v["net"]["sendkB"].as_u64(),
                        recvkBps: v["net"]["recvkBps"].as_f32(),
                        sendkBps: v["net"]["sendkBps"].as_f32(),
                    }
                }))
                .collect(),
    })
}

/// 復元の結果
#[derive(Debug, PartialEq)]
struct RestoreSummary {
    /// 読んだ行数（読み飛ばした先頭の行は含まない）
    lines: u64,
    /// キャッシュに入れた行数
    restored: u64,
}

/// JSONL のログを読んで、キャッシュを復元します
///
/// 先頭の lines_to_skip 行は解釈せずに読み飛ばします
/// 次の行は、その行だけを飛ばして続きを読みます
/// - json として解釈できない行、必要な項目が無い行
/// - time を解釈できない行、time が ZERO_TIME の行
fn restore_from_reader<R: std::io::BufRead>(
    mut reader: R,
    lines_to_skip: u64,
    log_cache: &mut log_cache::UsageCache,
) -> Result<RestoreSummary, error::Error> {
    let mut summary = RestoreSummary { lines: 0, restored: 0 };
    let mut iline: u64 = 0;
    let mut buffer: Vec<u8> = Vec::new();
    loop {
        buffer.clear();
        if reader.read_until(b'\n', &mut buffer)? == 0 {
            break;
        }
        iline += 1;
        if iline <= lines_to_skip { continue; }
        summary.lines += 1;

        let parsed = std::str::from_utf8(&buffer)
            .map_err(|e| error::Error::OtherError(e.to_string()))
            .and_then(|line| Ok(json::parse(line.trim())?))
            .and_then(|json| json_to_usage(&json));
        match parsed {
            Ok(usages) => {
                // time が ZERO_TIME の行（docker api sometimes returns
                // unix epoc ZERO）や解釈できない行は、ここで false になる
                if store_usages(&usages, log_cache) {
                    summary.restored += 1;
                }
            },
            Err(e) => {
                eprintln!("error in log line {}: {}", iline, e);
            },
        }
    }

    Ok(summary)
}

pub fn read_log(
    log_cache: &log_cache::SharedUsageCache
) -> Result<(), error::Error> {
    read_log_from(DAILY_LOG_PATH, log_cache)
}

/// ログファイルを開きます。ファイルが無ければ None を返します
fn open_log<P: AsRef<std::path::Path>>(
    path: P,
) -> Result<Option<std::fs::File>, error::Error> {
    match std::fs::OpenOptions::new().read(true).open(path) {
        Ok(file) => Ok(Some(file)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// ログファイルからキャッシュを復元します
/// ファイルが無い場合は、復元する点が無いものとして成功で返します
fn read_log_from<P: AsRef<std::path::Path>>(
    path: P,
    log_cache: &log_cache::SharedUsageCache,
) -> Result<(), error::Error> {

    // 1コンテナに入るのは1行につき1点までなので、末尾の MAX_LOG_LENGTH 行で足りる
    let Some(nlines) = check_nlines(&path)? else {
        println!("log file does not exist. nothing to restore.");
        return Ok(());
    };
    let nlines_to_skip = nlines.saturating_sub(log_cache::MAX_LOG_LENGTH as u64);

    // 数えたあとに消された場合も、無いものとして扱う
    let Some(file) = open_log(&path)? else {
        println!("log file does not exist. nothing to restore.");
        return Ok(());
    };
    let reader = std::io::BufReader::new(file);

    let summary = {
        let mut lock = log_cache.write()
            .expect("cannot lock log_cache");
        restore_from_reader(reader, nlines_to_skip, &mut lock)?
    };

    println!(
        "{}/{} lines are successfully processed.",
        summary.restored,
        summary.lines,
    );

    Ok(())
}


/// ログファイルの行数を返します。ファイルが無ければ None を返します
fn check_nlines<P: AsRef<std::path::Path>>(
    path: P,
) -> Result<Option<u64>, error::Error> {
    let Some(file) = open_log(path)? else { return Ok(None); };
    let mut reader = std::io::BufReader::new(file);
    let mut nlines: u64 = 0;
    let mut buffer: Vec<u8> = Vec::new();
    loop {
        buffer.clear();
        if std::io::BufRead::read_until(&mut reader, b'\n', &mut buffer)? == 0 {
            break;
        }
        nlines += 1;
    }
    Ok(Some(nlines))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::log_cache::{ create_shared_cache, DownsampleOption, Series };

    const TICK_MILLIS: u16 = 10_000;

    // ---- tick の時刻 ----

    /// 2026-01-01T00:00:00Z（10 秒境界）
    const BOUNDARY_MILLIS: u128 = 1_767_225_600_000;

    fn wait_and_timing(now: u128, timing: u128) -> (u128, u128) {
        let (wait, timing) = next_tick(now, timing, 10_000);
        (wait.as_millis(), timing)
    }

    #[test]
    fn next_tick_waits_until_the_scheduled_boundary() {
        let t = BOUNDARY_MILLIS;
        // 通常: 前回の処理が終わってから、予定の境界まで待つ。予定は変えない
        assert_eq!(wait_and_timing(t - 9_700, t), (9_700, t));
        assert_eq!(wait_and_timing(t - 1, t), (1, t));
        // 前回の処理が一瞬で終わった（まるまる 1 tick 待つ）
        assert_eq!(wait_and_timing(t - 10_000, t), (10_000, t));
        // 起動直後と同じ計算。境界の途中から、次の境界まで
        assert_eq!(next_boundary_after(t - 9_700, 10_000), t);
        assert_eq!(next_boundary_after(t - 1, 10_000), t);
    }

    #[test]
    fn next_tick_on_the_exact_boundary_moves_to_the_next_one() {
        let t = BOUNDARY_MILLIS;
        // 予定の時刻ちょうど（処理にちょうど 1 tick 掛かった）: 待ちなしにせず、次の境界へ
        assert_eq!(wait_and_timing(t, t), (10_000, t + 10_000));
        assert_eq!(next_boundary_after(t, 10_000), t + 10_000);
        // 1 ミリ秒でも過ぎていれば、その次の境界までの残りを待つ
        assert_eq!(wait_and_timing(t + 1, t), (9_999, t + 10_000));
    }

    #[test]
    fn next_tick_does_not_catch_up_after_the_clock_jumps_forward() {
        let t = BOUNDARY_MILLIS;
        // 処理が tick より長引いた（Docker の応答が滞った）: 飛んだ tick は取り戻さない
        assert_eq!(wait_and_timing(t + 12_345, t), (7_655, t + 20_000));
        // 時計が 1 時間進んだ（サスペンドからの復帰、時刻の補正）
        // 360 回分を連続で実行せず、現在時刻の次の境界まで待つ
        assert_eq!(wait_and_timing(t + 3_600_000 + 4_000, t), (6_000, t + 3_610_000));

        // そのあとは通常どおり、1 tick ずつ進む
        let (mut now, mut timing) = (t + 3_600_000 + 4_000, t);
        let mut waits = Vec::new();
        for _ in 0..3 {
            let (wait, next) = next_tick(now, timing, 10_000);
            waits.push(wait.as_millis());
            // 待ったあと、処理に 200 ミリ秒掛かる
            now += wait.as_millis() + 200;
            timing = next + 10_000;
        }
        assert_eq!(waits, [6_000, 9_800, 9_800]);
        assert_eq!(timing, t + 3_640_000);
    }

    #[test]
    fn next_tick_does_not_stall_after_the_clock_jumps_back() {
        let t = BOUNDARY_MILLIS;
        // 時計が 1 時間戻った: 予定の時刻（1 時間先）まで待たず、現在時刻の次の境界へ
        assert_eq!(wait_and_timing(t - 3_600_000 + 4_000, t), (6_000, t - 3_590_000));
        // 1 tick を 1 ミリ秒でも超える待ちは、合わせ直す
        assert_eq!(wait_and_timing(t - 10_001, t), (1, t - 10_000));
        // 戻った先がちょうど境界でも、待つのは 1 tick まで
        assert_eq!(wait_and_timing(t - 3_600_000, t), (10_000, t - 3_590_000));
    }

    #[test]
    fn next_tick_always_waits_between_zero_and_one_tick() {
        let t = BOUNDARY_MILLIS;
        for now in [0, 1, 9_999, 10_000, t - 86_400_000, t - 10_001, t - 5, t, t + 5, t + 86_400_000] {
            let (wait, timing) = wait_and_timing(now, t);
            assert!(wait > 0 && wait <= 10_000, "{} {}", now, wait);
            assert_eq!(timing, now + wait);
            assert_eq!(timing % 10_000, 0);
        }
        // tick が 0 でも止まらない（0 で割らない）
        assert_eq!(next_tick(5, 3, 0), (std::time::Duration::from_millis(1), 6));
    }

    // ---- Docker ソケットからの応答の読み取り ----

    /// 用意したバイト列を chunk_size ずつ返すストリーム
    /// readのたびに delay だけ待ち、読み切ったあとは after の振る舞いをする
    struct ScriptedStream {
        data: Vec<u8>,
        position: usize,
        chunk_size: usize,
        delay: std::time::Duration,
        after: After,
        /// limit_next_read で渡された上限
        limits_seen: Vec<std::time::Duration>,
    }
    enum After {
        /// 相手が閉じた
        Eof,
        /// ソケットのタイムアウト（read_timeout を過ぎた）
        TimedOut,
        /// 同じバイトを返し続ける（閉じない）
        Forever,
    }
    impl ScriptedStream {
        fn new(data: &[u8], chunk_size: usize, after: After) -> Self {
            ScriptedStream {
                data: data.to_vec(),
                position: 0,
                chunk_size,
                delay: std::time::Duration::ZERO,
                after,
                limits_seen: Vec::new(),
            }
        }
    }
    impl std::io::Read for ScriptedStream {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            std::thread::sleep(self.delay);
            let rest = &self.data[self.position..];
            if rest.is_empty() {
                return match self.after {
                    After::Eof => Ok(0),
                    After::TimedOut => Err(std::io::ErrorKind::WouldBlock.into()),
                    After::Forever => { buf[0] = b'x'; Ok(1) },
                };
            }
            let n = rest.len().min(self.chunk_size).min(buf.len());
            buf[..n].copy_from_slice(&rest[..n]);
            self.position += n;
            Ok(n)
        }
    }

    fn read_scripted(
        stream: &mut ScriptedStream,
        timeout: std::time::Duration,
    ) -> Result<String, std::io::Error> {
        read_response_within(stream, timeout, |stream, remaining| {
            stream.limits_seen.push(remaining);
            Ok(())
        })
    }

    #[test]
    fn docker_response_is_read_until_the_peer_closes() {
        let timeout = std::time::Duration::from_secs(5);
        let response = stats_response("2026-01-01T00:00:10Z", 1, 2, (3, 4), (5, 6));
        // 1回で届いても、少しずつ届いても、同じ文字列になる
        for chunk_size in [1, 7, 100_000] {
            let mut stream = ScriptedStream::new(response.as_bytes(), chunk_size, After::Eof);
            assert_eq!(read_scripted(&mut stream, timeout).unwrap(), response);
            // readのたびに残り時間を渡している（増えることはない）
            assert!(stream.limits_seen.len() >= 2);
            assert!(stream.limits_seen.iter().all(|limit| *limit <= timeout));
            assert!(stream.limits_seen.windows(2).all(|w| w[1] <= w[0]));
        }
        // 読み取った応答は、そのまま統計として解釈できる
        let mut stream = ScriptedStream::new(response.as_bytes(), 7, After::Eof);
        let text = read_scripted(&mut stream, timeout).unwrap();
        assert!(parse_stats_response(&text).is_ok());

        // 空の応答（すぐ閉じられた）は空文字列。utf-8 でなければエラー
        let mut empty = ScriptedStream::new(b"", 1, After::Eof);
        assert_eq!(read_scripted(&mut empty, timeout).unwrap(), "");
        let mut invalid = ScriptedStream::new(b"\xff\xfe", 1, After::Eof);
        assert_eq!(
            read_scripted(&mut invalid, timeout).unwrap_err().kind(),
            std::io::ErrorKind::InvalidData,
        );
    }

    #[test]
    fn docker_response_that_stops_is_an_error() {
        // 途中まで返して黙った（ソケットのタイムアウト）: 届いた分を使わずにエラーにする
        let timeout = std::time::Duration::from_secs(5);
        for data in [&b""[..], b"HTTP/1.1 200 OK\r\n\r\n{\"read\":"] {
            let mut stream = ScriptedStream::new(data, 100, After::TimedOut);
            assert_eq!(
                read_scripted(&mut stream, timeout).unwrap_err().kind(),
                std::io::ErrorKind::WouldBlock,
            );
        }
        // 残り時間を設定できなかった場合も、読まずにエラーにする
        let mut stream = ScriptedStream::new(b"data", 100, After::Eof);
        let result = read_response_within(
            &mut stream,
            timeout,
            |_, _| Err(std::io::ErrorKind::InvalidInput.into()),
        );
        assert_eq!(result.unwrap_err().kind(), std::io::ErrorKind::InvalidInput);
        assert_eq!(stream.position, 0);
    }

    #[test]
    fn docker_response_is_cut_off_at_the_total_deadline() {
        let timeout = std::time::Duration::from_millis(100);

        // 少しずつ返し続けて閉じない相手: readは毎回成功するが、合計の期限で打ち切る
        let mut trickle = ScriptedStream::new(b"", 1, After::Forever);
        trickle.delay = std::time::Duration::from_millis(5);
        let started = std::time::Instant::now();
        let error = read_scripted(&mut trickle, timeout).unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::TimedOut);
        assert!(started.elapsed() >= timeout, "{:?}", started.elapsed());
        assert!(
            started.elapsed() < std::time::Duration::from_millis(2000),
            "{:?}", started.elapsed(),
        );
        assert!(trickle.limits_seen.windows(2).all(|w| w[1] <= w[0]));

        // 期限を過ぎてから届いた終わり（EOF）は受け付けない
        let mut late = ScriptedStream::new(b"[]", 100, After::Eof);
        late.delay = std::time::Duration::from_millis(80);
        assert_eq!(
            read_scripted(&mut late, timeout).unwrap_err().kind(),
            std::io::ErrorKind::TimedOut,
        );
        // 期限内に終われば受け付ける
        let mut in_time = ScriptedStream::new(b"[]", 100, After::Eof);
        in_time.delay = std::time::Duration::from_millis(5);
        assert_eq!(read_scripted(&mut in_time, timeout).unwrap(), "[]");

        // 残り時間が無ければ、1回も読まない
        let mut unread = ScriptedStream::new(b"[]", 100, After::Eof);
        assert_eq!(
            read_scripted(&mut unread, std::time::Duration::ZERO).unwrap_err().kind(),
            std::io::ErrorKind::TimedOut,
        );
        assert_eq!(unread.position, 0);
    }

    /// Docker の stats API が返す応答（ヘッダ込み）を組み立てる
    /// 値はすべて積算値。cpu と system は ns、ほかはバイト
    /// デバイスとインターフェースは1つずつ、メモリは cgroup v2 の形
    fn stats_response(
        read: &str,
        cpu_total_ns: u64,
        system_ns: u64,
        io: (u64, u64),
        net: (u64, u64),
    ) -> String {
        format!(
            concat!(
                "HTTP/1.1 200 OK\r\n",
                "Content-Type: application/json\r\n",
                "\r\n",
                r#"{{"read":"{read}","preread":"0001-01-01T00:00:00Z","#,
                r#""blkio_stats":{{"io_service_bytes_recursive":["#,
                r#"{{"major":8,"minor":0,"op":"read","value":{io_read}}},"#,
                r#"{{"major":8,"minor":0,"op":"write","value":{io_write}}}]}},"#,
                r#""cpu_stats":{{"cpu_usage":{{"total_usage":{cpu}}},"#,
                r#""system_cpu_usage":{system},"online_cpus":2}},"#,
                r#""memory_stats":{{"usage":300000,"stats":{{"anon":50000,"file":150000,"inactive_file":100000}},"limit":1000000}},"#,
                r#""networks":{{"eth0":{{"rx_bytes":{rx},"tx_bytes":{tx}}}}}}}"#,
                "\n",
            ),
            read = read,
            cpu = cpu_total_ns,
            system = system_ns,
            io_read = io.0,
            io_write = io.1,
            rx = net.0,
            tx = net.1,
        )
    }

    fn stats_at(
        read: &str,
        cpu_total_ns: u64,
        system_ns: u64,
        io: (u64, u64),
        net: (u64, u64),
    ) -> Stats {
        parse_stats_response(
            &stats_response(read, cpu_total_ns, system_ns, io, net)
        ).unwrap()
    }

    /// 時刻だけが違う、値が増えていない統計
    fn idle_stats(read: &str) -> Stats {
        stats_at(read, 0, 0, (0, 0), (0, 0))
    }

    fn stats_map(entries: Vec<(&str, Stats)>) -> HashMap<String, Stats> {
        entries.into_iter().map(|(k, v)| (k.to_string(), v)).collect()
    }

    #[test]
    fn stats_response_is_reshaped() {
        let stats = stats_at(
            "2026-01-01T00:00:10.023034093Z",
            5_000_000_000, 70_000_000_000,
            (1000, 2000), (3000, 4000),
        );
        assert_eq!(stats.time.as_deref(), Some("2026-01-01T00:00:10.023034093Z"));
        // ns のまま持つ（ms に丸めない）
        assert_eq!(stats.cpu.total, Some(5_000_000_000));
        assert_eq!(stats.cpu.system, Some(70_000_000_000));
        assert_eq!(stats.cpu.ncpu, Some(2));
        // usage - stats.inactive_file
        assert_eq!(stats.memory.used, Some(200_000));
        assert_eq!(stats.memory.available, Some(1_000_000));
        assert_eq!(stats.io.read, Some(1000));
        assert_eq!(stats.io.write, Some(2000));
        assert_eq!(stats.net.recv, Some(3000));
        assert_eq!(stats.net.send, Some(4000));

        assert!(parse_stats_response("HTTP/1.1 500 Internal Server Error\r\n\r\n").is_err());
        assert!(parse_stats_response("HTTP/1.1 200 OK\r\n\r\n{broken\n").is_err());
        // 本文が統計の形でなくても panic せず、値が無いものとして扱う
        let gone = parse_stats_response(
            "HTTP/1.1 404 Not Found\r\n\r\n{\"message\":\"No such container\"}\n"
        ).unwrap();
        assert!(gone.time.is_none() && gone.cpu.total.is_none());
    }

    #[test]
    fn collection_skips_only_the_affected_container() {
        let names: Vec<String> = ["a", "b", "c", "d", "e", "f"]
            .iter().map(|s| s.to_string()).collect();
        let mut asked: Vec<String> = Vec::new();
        let collected = collect_stats(names, |name| {
            asked.push(name.to_string());
            match name {
                // ZERO_TIME を返すコンテナ
                "b" => Ok(idle_stats(ZERO_TIME)),
                // 統計の取得に失敗するコンテナ
                "d" => Err("connection refused (expected in this test)".into()),
                // 時刻を解釈できないコンテナ
                "f" => Ok(idle_stats("not a time")),
                _ => Ok(idle_stats("2026-01-01T00:00:10Z")),
            }
        });
        // 途中で打ち切らず、全コンテナに問い合わせる
        assert_eq!(asked, vec!["a", "b", "c", "d", "e", "f"]);
        let mut usable: Vec<&String> = collected.stats.keys().collect();
        usable.sort();
        assert_eq!(usable, vec!["a", "c", "e"]);
        // 一覧には、採れなかったコンテナも残る
        assert_eq!(collected.listed, vec!["a", "b", "c", "d", "e", "f"]);
    }

    /// 1 tick 分の収集結果を作る。None は「一覧には載っているが採れなかった」
    fn collected(entries: Vec<(&str, Option<Stats>)>) -> Collected {
        let names: Vec<String> =
            entries.iter().map(|(name, _)| name.to_string()).collect();
        let mut entries: HashMap<String, Option<Stats>> = entries.into_iter()
            .map(|(name, stats)| (name.to_string(), stats))
            .collect();
        collect_stats(names, |name| {
            entries.remove(name).flatten()
                .ok_or_else(|| "no stats (expected in this test)".into())
        })
    }

    /// write だけが増えていく統計
    fn written(read: &str, bytes: u64) -> Option<Stats> {
        Some(stats_at(read, 0, 0, (0, bytes), (0, 0)))
    }

    /// run_tick を、ログの追記先をメモリに差し替えて呼ぶ
    struct Logger {
        /// 単調時計。tick のたびに10秒進める
        mono_millis: u64,
        state: LoggerState,
        cache: log_cache::SharedUsageCache,
        lines: Vec<String>,
    }
    impl Logger {
        fn new() -> Self {
            Logger {
                mono_millis: 0,
                state: LoggerState::new(),
                cache: create_shared_cache(),
                lines: Vec::new(),
            }
        }
        fn tick(
            &mut self,
            collected: Result<Collected, error::Error>,
        ) -> Result<bool, error::Error> {
            self.tick_at(None, collected)
        }
        fn tick_at(
            &mut self,
            now_millis: Option<i64>,
            collected: Result<Collected, error::Error>,
        ) -> Result<bool, error::Error> {
            self.mono_millis += 10_000;
            let lines = &mut self.lines;
            run_tick(
                TICK_MILLIS, now_millis, self.mono_millis, collected, &mut self.state,
                |line| { lines.push(line.to_string()); Ok(()) },
                &self.cache,
            )
        }
        fn writes(&self, name: &str) -> Vec<(String, Option<f32>)> {
            self.cache.read().unwrap().get(name).unwrap().iter()
                .map(|p| (time::millis_to_iso8601(p.time_ms), p.write_kbps))
                .collect()
        }
    }

    #[test]
    fn skipped_container_loses_one_point_and_resumes_with_real_elapsed_time() {
        let mut logger = Logger::new();
        // 起動後の最初の tick は記録しない
        assert!(!logger.tick(Ok(collected(vec![
            ("web", written("2026-01-01T00:00:00Z", 0)),
            ("db", written("2026-01-01T00:00:00Z", 0)),
        ]))).unwrap());
        assert!(logger.tick(Ok(collected(vec![
            ("web", written("2026-01-01T00:00:10Z", 1_000_000)),
            ("db", written("2026-01-01T00:00:10Z", 1_000_000)),
        ]))).unwrap());
        // db は一覧に載っているが、統計を採れなかった
        assert!(logger.tick(Ok(collected(vec![
            ("web", written("2026-01-01T00:00:20Z", 2_000_000)),
            ("db", None),
        ]))).unwrap());
        // 前回の値（10秒の時点）が残っている
        assert_eq!(
            logger.state.prev_stats["db"].time.as_deref(),
            Some("2026-01-01T00:00:10Z"),
        );
        // db が戻る。10秒の時点から 6,000,000 バイト増えた
        assert!(logger.tick(Ok(collected(vec![
            ("web", written("2026-01-01T00:00:30Z", 3_000_000)),
            ("db", written("2026-01-01T00:00:30Z", 7_000_000)),
        ]))).unwrap());

        assert_eq!(logger.writes("web"), vec![
            ("2026-01-01T00:00:10.000Z".to_string(), Some(100.0)),
            ("2026-01-01T00:00:20.000Z".to_string(), Some(100.0)),
            ("2026-01-01T00:00:30.000Z".to_string(), Some(100.0)),
        ]);
        // 欠けるのは1点だけ。戻った点は 20秒で割った 300 kB/s（10秒で割った 600 ではない）
        assert_eq!(logger.writes("db"), vec![
            ("2026-01-01T00:00:10.000Z".to_string(), Some(100.0)),
            ("2026-01-01T00:00:30.000Z".to_string(), Some(300.0)),
        ]);
        assert_eq!(logger.lines.len(), 3);
    }

    #[test]
    fn zero_time_and_unparsable_time_also_keep_the_previous_stats() {
        for bad_time in [ZERO_TIME, "not a time"] {
            let mut logger = Logger::new();
            logger.tick(Ok(collected(vec![
                ("web", written("2026-01-01T00:00:00Z", 0)),
                ("db", written("2026-01-01T00:00:00Z", 0)),
            ]))).unwrap();
            logger.tick(Ok(collected(vec![
                ("web", written("2026-01-01T00:00:10Z", 0)),
                ("db", written(bad_time, 999_999_999)),
            ]))).unwrap();
            logger.tick(Ok(collected(vec![
                ("web", written("2026-01-01T00:00:20Z", 0)),
                ("db", written("2026-01-01T00:00:20Z", 4_000_000)),
            ]))).unwrap();
            assert_eq!(logger.writes("db"), vec![
                ("2026-01-01T00:00:20.000Z".to_string(), Some(200.0)),
            ], "{}", bad_time);
        }
    }

    #[test]
    fn stats_of_unlisted_container_are_forgotten() {
        let mut logger = Logger::new();
        logger.tick(Ok(collected(vec![
            ("web", written("2026-01-01T00:00:00Z", 0)),
            ("db", written("2026-01-01T00:00:00Z", 0)),
        ]))).unwrap();
        assert!(logger.state.prev_stats.contains_key("db"));
        // db が一覧から消えた（止まった）
        logger.tick(Ok(collected(vec![
            ("web", written("2026-01-01T00:00:10Z", 0)),
        ]))).unwrap();
        assert!(!logger.state.prev_stats.contains_key("db"));
        assert_eq!(logger.state.prev_stats.len(), 1);

        // 同じ名前で戻ってきても、止まる前の値との差分は取らない
        // (最初の tick は記録せず、次の tick から記録する)
        logger.tick(Ok(collected(vec![
            ("web", written("2026-01-01T01:00:00Z", 0)),
            ("db", written("2026-01-01T01:00:00Z", 50_000_000)),
        ]))).unwrap();
        assert!(logger.cache.read().unwrap().get("db").is_none());
        logger.tick(Ok(collected(vec![
            ("web", written("2026-01-01T01:00:10Z", 0)),
            ("db", written("2026-01-01T01:00:10Z", 51_000_000)),
        ]))).unwrap();
        assert_eq!(logger.writes("db"), vec![
            ("2026-01-01T01:00:10.000Z".to_string(), Some(100.0)),
        ]);

        // コンテナが1つも無い tick では、全部消える
        logger.tick(Ok(collected(vec![]))).unwrap();
        assert!(logger.state.prev_stats.is_empty());
    }

    #[test]
    fn failed_ticks_are_skipped_and_the_logger_goes_on() {
        let mut logger = Logger::new();
        let tick = |second: u32, bytes: u64| Ok(collected(vec![
            ("web", written(&format!("2026-01-01T00:00:{:02}Z", second), bytes)),
        ]));
        assert!(!logger.tick(tick(0, 0)).unwrap());
        assert!(logger.tick(tick(10, 1_000_000)).unwrap());
        // コンテナ一覧の取得に失敗した tick。エラーが返るだけで、何も壊れない
        for _ in 0..2 {
            let failed = logger.tick(Err("cannot list containers (expected in this test)".into()));
            assert!(failed.is_err());
        }
        assert_eq!(logger.lines.len(), 1);
        // 前回の統計は残っているので、次に採れた tick で30秒分を30秒で割って記録する
        assert!(logger.tick(tick(40, 4_000_000)).unwrap());
        assert_eq!(logger.writes("web"), vec![
            ("2026-01-01T00:00:10.000Z".to_string(), Some(100.0)),
            ("2026-01-01T00:00:40.000Z".to_string(), Some(100.0)),
        ]);

        // ログへの追記に失敗した tick。キャッシュにも入れず、前回の統計も進めない
        let failed = run_tick(
            TICK_MILLIS, None, 1_000_000, tick(50, 5_000_000), &mut logger.state,
            |_| Err("disk full (expected in this test)".into()),
            &logger.cache,
        );
        assert!(failed.is_err());
        assert_eq!(logger.writes("web").len(), 2);
        assert_eq!(
            logger.state.prev_stats["web"].time.as_deref(),
            Some("2026-01-01T00:00:40Z"),
        );
        // 次に書けた tick の点は、最後に記録できた値からの差分になる
        // (2,400,000 バイト / 15秒)。失敗した tick の増分も落ちない
        assert!(logger.tick(tick(55, 6_400_000)).unwrap());
        assert_eq!(
            logger.writes("web").last().unwrap(),
            &("2026-01-01T00:00:55.000Z".to_string(), Some(160.0)),
        );
        assert_eq!(logger.lines.len(), 3);
    }

    #[test]
    fn stats_with_the_same_time_do_not_replace_the_previous_stats() {
        let mut logger = Logger::new();
        let tick = |read: &str, bytes: u64| Ok(collected(vec![
            ("web", written(read, bytes)),
        ]));
        logger.tick(tick("2026-01-01T00:00:00Z", 0)).unwrap();
        assert!(logger.tick(tick("2026-01-01T00:00:10Z", 1_000_000)).unwrap());
        // 時刻は同じ（ミリ秒未満だけ違う）だが、積算値は進んでいる。この tick は記録されない
        assert!(!logger.tick(tick("2026-01-01T00:00:10.000999Z", 2_000_000)).unwrap());
        assert_eq!(logger.state.prev_stats["web"].io.write, Some(1_000_000));
        // 次の点は、最後に記録できた値からの差分（2,000,000 バイト / 10秒）になる
        // 前回値を置き換えていると 1,000,000 バイト分が落ちて 100 kB/s になる
        assert!(logger.tick(tick("2026-01-01T00:00:20Z", 3_000_000)).unwrap());
        assert_eq!(logger.writes("web"), vec![
            ("2026-01-01T00:00:10.000Z".to_string(), Some(100.0)),
            ("2026-01-01T00:00:20.000Z".to_string(), Some(200.0)),
        ]);

    }

    /// web だけの tick。write の積算値だけが増える
    fn web_tick(read: &str, bytes: u64) -> Result<Collected, error::Error> {
        Ok(collected(vec![("web", written(read, bytes))]))
    }

    /// ログに書かれた行の (time, web の writekBps)
    fn logged(logger: &Logger) -> Vec<(String, Option<f32>)> {
        logger.lines.iter().map(|line| {
            let json = json::parse(line).unwrap();
            (
                json["time"].as_str().unwrap().to_string(),
                json["stats"]["web"]["io"]["writekBps"].as_f32(),
            )
        }).collect()
    }

    #[test]
    fn increments_during_a_backward_time_are_not_lost() {
        let mut logger = Logger::new();
        logger.tick(web_tick("2026-01-01T00:00:00Z", 0)).unwrap();
        assert!(logger.tick(web_tick("2026-01-01T00:00:10Z", 1_000_000)).unwrap());
        // 時刻が戻った統計。記録せず、前回値（10秒・1,000,000）を残す
        assert!(!logger.tick(web_tick("2026-01-01T00:00:05Z", 2_500_000)).unwrap());
        assert_eq!(
            logger.state.prev_stats["web"].time.as_deref(),
            Some("2026-01-01T00:00:10Z"),
        );
        assert_eq!(logger.state.stalled_ticks["web"], 1);
        // 時刻が進んだ統計。最後に記録できた値からの差分
        // (4,000,000 バイト / 20秒) になり、戻っていた間の増分も含む
        assert!(logger.tick(web_tick("2026-01-01T00:00:30Z", 5_000_000)).unwrap());
        assert_eq!(logger.writes("web"), vec![
            ("2026-01-01T00:00:10.000Z".to_string(), Some(100.0)),
            ("2026-01-01T00:00:30.000Z".to_string(), Some(200.0)),
        ]);
        // 記録できたので、回数は戻る
        assert!(!logger.state.stalled_ticks.contains_key("web"));
    }

    #[test]
    fn third_stalled_tick_is_recorded_with_the_elapsed_time_measured_by_core() {
        assert_eq!(MAX_STALLED_TICKS, 3);
        let mut logger = Logger::new();
        logger.tick(web_tick("2026-01-01T01:00:00Z", 0)).unwrap();
        // 単調時計で 20秒の時点。ここまでは通常どおり
        assert!(logger.tick(web_tick("2026-01-01T01:00:10Z", 1_000_000)).unwrap());
        assert_eq!(logger.mono_millis, 20_000);

        // 時計が1時間戻された。1回目・2回目は記録せず、前回値を残す
        assert!(!logger.tick(web_tick("2026-01-01T00:00:20Z", 2_000_000)).unwrap());
        assert!(!logger.tick(web_tick("2026-01-01T00:00:30Z", 3_000_000)).unwrap());
        assert_eq!(
            logger.state.prev_stats["web"].time.as_deref(),
            Some("2026-01-01T01:00:10Z"),
        );
        assert_eq!(logger.state.stalled_ticks["web"], 2);

        // 3回目（単調時計で 65秒の時点。tick が遅れた想定）で1点を記録する
        // 差分は最後に記録できた統計から: (5,500,000 - 1,000,000) バイト
        // 経過時間は core が測った 65 - 20 = 45秒 → 100 kB/s
        logger.mono_millis = 55_000;
        assert!(logger.tick(web_tick("2026-01-01T00:00:40Z", 5_500_000)).unwrap());
        // 前回値は今回の値に置き換わり、回数は戻る
        assert_eq!(
            logger.state.prev_stats["web"].time.as_deref(),
            Some("2026-01-01T00:00:40Z"),
        );
        assert!(!logger.state.stalled_ticks.contains_key("web"));

        // その次の tick からは、Docker の時刻の差（10秒）で通常どおり記録する
        // (単調時計では 30秒空いているが、使わない)
        logger.mono_millis = 85_000;
        assert!(logger.tick(web_tick("2026-01-01T00:00:50Z", 7_500_000)).unwrap());

        let points = logged(&logger);
        assert_eq!(points, vec![
            ("2026-01-01T01:00:10Z".to_string(), Some(100.0)),
            ("2026-01-01T00:00:40Z".to_string(), Some(100.0)),
            ("2026-01-01T00:00:50Z".to_string(), Some(200.0)),
        ]);
        // 増分が落ちていない: 速度 × 経過時間 の合計が、積算値の増分（7,500,000 バイト）に一致する
        let elapsed_seconds = [10, 45, 10];
        let total_kb: f32 = points.iter().zip(elapsed_seconds)
            .map(|((_, rate), seconds)| rate.unwrap() * seconds as f32)
            .sum();
        assert_eq!(total_kb, 7_500.0);

        // 同じ統計が返り続ける場合も、3回目で記録して数え直す
        let mut logger = Logger::new();
        logger.tick(web_tick("2026-01-01T00:00:00Z", 0)).unwrap();
        for (recorded, count) in [(false, Some(1)), (false, Some(2)), (true, None)] {
            assert_eq!(logger.tick(web_tick("2026-01-01T00:00:00Z", 0)).unwrap(), recorded);
            assert_eq!(logger.state.stalled_ticks.get("web").copied(), count);
        }
    }

    #[test]
    fn stalled_container_does_not_drag_the_tick_time_back() {
        let mut logger = Logger::new();
        let tick = |web_read: &str, db_bytes: u64| Ok(collected(vec![
            ("web", written(web_read, 0)),
            // db は同じ時刻の統計を返し続けるが、積算値は進んでいる
            ("db", written("2026-01-01T00:00:10Z", db_bytes)),
        ]));
        logger.tick(Ok(collected(vec![
            ("web", written("2026-01-01T00:00:00Z", 0)),
            ("db", written("2026-01-01T00:00:00Z", 0)),
        ]))).unwrap();
        assert!(logger.tick(tick("2026-01-01T00:00:10Z", 1_000_000)).unwrap());
        assert!(logger.tick(tick("2026-01-01T00:00:20Z", 2_000_000)).unwrap());
        assert!(logger.tick(tick("2026-01-01T00:00:30Z", 3_000_000)).unwrap());
        assert!(logger.tick(tick("2026-01-01T00:00:40Z", 4_000_000)).unwrap());

        // tick の時刻は、Docker の時刻が進んだコンテナ（web）のものを使う
        // db の古い時刻を使うと、web の点までキャッシュに入らなくなる
        let last = json::parse(logger.lines.last().unwrap()).unwrap();
        assert_eq!(last["time"], "2026-01-01T00:00:40Z");
        assert_eq!(logger.writes("web").len(), 4);
        // db は3回目で、core が測った 30秒で割った1点が入る
        assert_eq!(logger.writes("db"), vec![
            ("2026-01-01T00:00:10.000Z".to_string(), Some(100.0)),
            ("2026-01-01T00:00:40.000Z".to_string(), Some(100.0)),
        ]);
    }

    #[test]
    fn stalled_count_survives_failed_ticks_and_resets_on_a_record() {
        let mut logger = Logger::new();
        let failed = || Ok(collected(vec![("web", None)]));
        logger.tick(web_tick("2026-01-01T01:00:00Z", 0)).unwrap();
        assert!(logger.tick(web_tick("2026-01-01T01:00:10Z", 1_000_000)).unwrap());

        // 進まない → 採れない → 進まない → 採れない（ZERO_TIME）→ 進まない
        assert!(!logger.tick(web_tick("2026-01-01T00:00:20Z", 2_000_000)).unwrap());
        assert_eq!(logger.state.stalled_ticks["web"], 1);
        assert!(!logger.tick(failed()).unwrap());
        // 採れなかった tick は数えず、0 にも戻さない
        assert_eq!(logger.state.stalled_ticks["web"], 1);
        assert!(!logger.tick(web_tick("2026-01-01T00:00:40Z", 3_000_000)).unwrap());
        assert_eq!(logger.state.stalled_ticks["web"], 2);
        assert!(!logger.tick(web_tick(ZERO_TIME, 9_000_000)).unwrap());
        assert_eq!(logger.state.stalled_ticks["web"], 2);
        // 一覧の取得に失敗した tick でも変わらない
        assert!(logger.tick(Err("cannot list containers (expected in this test)".into())).is_err());
        assert_eq!(logger.state.stalled_ticks["web"], 2);
        // 採れなかった tick を挟んでも、進まない統計の3回目で記録して置き換わる
        assert!(logger.tick(web_tick("2026-01-01T00:01:00Z", 4_000_000)).unwrap());
        assert_eq!(
            logger.state.prev_stats["web"].time.as_deref(),
            Some("2026-01-01T00:01:00Z"),
        );

        // 記録できたら 0 に戻る: 進まない2回のあとに記録し、また2回進まなくても置き換わらない
        let mut logger = Logger::new();
        logger.tick(web_tick("2026-01-01T00:00:00Z", 0)).unwrap();
        assert!(logger.tick(web_tick("2026-01-01T00:01:00Z", 1_000_000)).unwrap());
        assert!(!logger.tick(web_tick("2026-01-01T00:00:10Z", 1_000_000)).unwrap());
        assert!(!logger.tick(web_tick("2026-01-01T00:00:20Z", 1_000_000)).unwrap());
        assert!(logger.tick(web_tick("2026-01-01T00:01:10Z", 2_000_000)).unwrap());
        assert!(!logger.state.stalled_ticks.contains_key("web"));
        assert!(!logger.tick(web_tick("2026-01-01T00:00:30Z", 2_000_000)).unwrap());
        assert!(!logger.tick(web_tick("2026-01-01T00:00:40Z", 2_000_000)).unwrap());
        assert_eq!(logger.state.stalled_ticks["web"], 2);
        assert_eq!(
            logger.state.prev_stats["web"].time.as_deref(),
            Some("2026-01-01T00:01:10Z"),
        );

        // 一覧から消えたコンテナは、回数も消える
        assert!(!logger.tick(Ok(collected(vec![]))).unwrap());
        assert!(logger.state.stalled_ticks.is_empty());
        assert!(logger.state.prev_stats.is_empty());
    }

    #[test]
    fn stopped_containers_expire_even_when_nothing_is_recorded() {
        const BASE_MS: i64 = 1_767_225_600_000; // 2026-01-01T00:00:00Z
        const DAY_MS: i64 = 24 * 60 * 60 * 1000;
        let mut logger = Logger::new();
        logger.tick_at(Some(BASE_MS), Ok(collected(vec![
            ("web", written("2026-01-01T00:00:00Z", 0)),
        ]))).unwrap();
        logger.tick_at(Some(BASE_MS + 10_000), Ok(collected(vec![
            ("web", written("2026-01-01T00:00:10Z", 1_000_000)),
        ]))).unwrap();
        let names = |logger: &Logger| {
            logger.cache.read().unwrap().container_names()
                .into_iter().cloned().collect::<Vec<String>>()
        };
        assert_eq!(names(&logger), vec!["web"]);

        // 最後のコンテナが止まり、記録するものが無い tick が続く
        let last = BASE_MS + 10_000;
        assert!(!logger.tick_at(Some(last + DAY_MS), Ok(collected(vec![]))).unwrap());
        assert_eq!(names(&logger), vec!["web"]);
        // 24時間を過ぎると、新しい点が無くても一覧から消える
        assert!(!logger.tick_at(Some(last + DAY_MS + 10_000), Ok(collected(vec![]))).unwrap());
        assert!(names(&logger).is_empty());

        // 収集に失敗し続けている間も同じ
        let mut logger = Logger::new();
        logger.tick_at(Some(BASE_MS), Ok(collected(vec![
            ("web", written("2026-01-01T00:00:00Z", 0)),
        ]))).unwrap();
        logger.tick_at(Some(BASE_MS + 10_000), Ok(collected(vec![
            ("web", written("2026-01-01T00:00:10Z", 1_000_000)),
        ]))).unwrap();
        let failed = logger.tick_at(
            Some(last + DAY_MS + 10_000),
            Err("cannot list containers (expected in this test)".into()),
        );
        assert!(failed.is_err());
        assert!(names(&logger).is_empty());

        // 現在時刻が分からない tick では、時間による削除をしない
        let mut logger = Logger::new();
        logger.tick(Ok(collected(vec![
            ("web", written("2026-01-01T00:00:00Z", 0)),
        ]))).unwrap();
        logger.tick(Ok(collected(vec![
            ("web", written("2026-01-01T00:00:10Z", 1_000_000)),
        ]))).unwrap();
        logger.tick(Ok(collected(vec![]))).unwrap();
        assert_eq!(names(&logger), vec!["web"]);
    }

    #[test]
    fn missing_log_file_is_not_an_error() {
        let path = std::env::temp_dir().join(format!(
            "cephylas-test-missing-log-{}-{:?}",
            std::process::id(), std::thread::current().id(),
        ));
        assert!(!path.exists());
        let cache = create_shared_cache();
        read_log_from(&path, &cache).unwrap();
        assert!(cache.read().unwrap().container_names().is_empty());
        assert_eq!(check_nlines(&path).unwrap(), None);
        // 読むだけなので、ファイルを作らない
        assert!(!path.exists());

        // 無いこと以外の失敗は、従来どおりエラーにする（ディレクトリは読めない）
        assert!(read_log_from(std::env::temp_dir(), &cache).is_err());
    }

    #[test]
    fn container_without_previous_stats_is_skipped() {
        let prev = stats_map(vec![
            ("web", idle_stats("2026-01-01T00:00:00Z")),
        ]);
        let now = stats_map(vec![
            ("web", idle_stats("2026-01-01T00:00:10Z")),
            ("db", idle_stats("2026-01-01T00:00:10.5Z")),
        ]);
        // 前回に無かった db があっても panic せず、db だけを飛ばす
        let usages = calc_usages(TICK_MILLIS, &now, &prev).unwrap();
        assert_eq!(usages.usages.keys().collect::<Vec<_>>(), vec!["web"]);

        // 次の tick からは db も記録される
        let next = stats_map(vec![
            ("web", idle_stats("2026-01-01T00:00:20Z")),
            ("db", idle_stats("2026-01-01T00:00:20.5Z")),
        ]);
        let usages = calc_usages(TICK_MILLIS, &next, &now).unwrap();
        assert_eq!(usages.usages.len(), 2);

        // 起動後の最初の tick（前回が空）と、コンテナが1つも無い tick は記録しない
        assert!(calc_usages(TICK_MILLIS, &now, &HashMap::new()).is_none());
        assert!(calc_usages(TICK_MILLIS, &HashMap::new(), &now).is_none());
    }

    #[test]
    fn rates_use_the_measured_elapsed_time() {
        // 2,000,000 バイト増えた。10秒なら 200 kB/s、20秒なら 100 kB/s
        let before = || stats_at("2026-01-01T00:00:00Z", 0, 0, (1000, 0), (0, 500));
        let after = |read: &str| stats_at(
            read, 0, 0,
            (2_001_000, 4_000_000), (6_000_000, 8_000_500),
        );
        let prev = stats_map(vec![
            ("ten", before()), ("twenty", before()), ("uneven", before()),
        ]);
        let now = stats_map(vec![
            ("ten", after("2026-01-01T00:00:10Z")),
            ("twenty", after("2026-01-01T00:00:20Z")),
            // 12.5秒。2,000,000 / 12,500 = 160
            ("uneven", after("2026-01-01T00:00:12.500Z")),
        ]);
        let usages = calc_usages(TICK_MILLIS, &now, &prev).unwrap();

        let ten = &usages.usages["ten"];
        assert_eq!(ten.io.readkBps, Some(200.0));
        assert_eq!(ten.io.writekBps, Some(400.0));
        assert_eq!(ten.net.recvkBps, Some(600.0));
        assert_eq!(ten.net.sendkBps, Some(800.0));

        let twenty = &usages.usages["twenty"];
        assert_eq!(twenty.io.readkBps, Some(100.0));
        assert_eq!(twenty.io.writekBps, Some(200.0));
        assert_eq!(twenty.net.recvkBps, Some(300.0));
        assert_eq!(twenty.net.sendkBps, Some(400.0));

        assert_eq!(usages.usages["uneven"].io.readkBps, Some(160.0));

        // 積算値（kB）は経過時間に関係しない
        assert_eq!(twenty.io.readkB, Some(2001));
        assert_eq!(twenty.net.sendkB, Some(8000));
        // millis は tick の間隔のまま（実測値を書かない）
        assert_eq!(usages.millis, 10_000);
    }

    #[test]
    fn cpu_and_memory_do_not_depend_on_elapsed_time() {
        // CPU は差分同士の比: 1000ms / 10000ms × 2 cores × 100 = 20%
        let before = || stats_at(
            "2026-01-01T00:00:00Z", 1_000_000_000, 50_000_000_000, (0, 0), (0, 0),
        );
        let after = |read: &str| stats_at(
            read, 2_000_000_000, 60_000_000_000, (0, 0), (0, 0),
        );
        let prev = stats_map(vec![("ten", before()), ("twenty", before())]);
        let now = stats_map(vec![
            ("ten", after("2026-01-01T00:00:10Z")),
            ("twenty", after("2026-01-01T00:00:20Z")),
        ]);
        let usages = calc_usages(TICK_MILLIS, &now, &prev).unwrap();
        for name in ["ten", "twenty"] {
            let usage = &usages.usages[name];
            assert_eq!(usage.cpu.percentage, Some(20.0));
            // ログに出す差分は ms
            assert_eq!(usage.cpu.total, Some(1000));
            assert_eq!(usage.cpu.system, Some(10_000));
            // (usage 300000 - inactive_file 100000) / limit 1000000 × 100
            assert_eq!(usage.memory.percentage, Some(20.0));
        }
    }

    #[test]
    fn zero_denominator_becomes_null_not_nan() {
        // system の差分が0（時刻は進んでいる）
        let prev = stats_map(vec![
            ("web", stats_at("2026-01-01T00:00:00Z", 1_000_000_000, 50_000_000_000, (0, 0), (0, 0))),
        ]);
        let now = stats_map(vec![
            ("web", stats_at("2026-01-01T00:00:10Z", 2_000_000_000, 50_000_000_000, (0, 0), (0, 0))),
        ]);
        let usages = calc_usages(TICK_MILLIS, &now, &prev).unwrap();
        assert_eq!(usages.usages["web"].cpu.percentage, None);
        // ログの1行が json として読める
        let line = usages.to_string();
        assert!(json::parse(&line).is_ok(), "{}", line);
        assert!(line.contains(r#""cpu":{"percentage":null,"#), "{}", line);
    }

    #[test]
    fn container_whose_time_did_not_advance_is_skipped() {
        let prev = stats_map(vec![
            ("web", idle_stats("2026-01-01T00:00:10Z")),
            ("same", idle_stats("2026-01-01T00:00:10.123456789Z")),
            ("back", idle_stats("2026-01-01T00:00:10Z")),
            ("broken", idle_stats("2026-01-01T00:00:10Z")),
            ("broken-before", idle_stats("not a time")),
            ("zero", idle_stats("2026-01-01T00:00:10Z")),
            ("zero-before", idle_stats(ZERO_TIME)),
        ]);
        let now = stats_map(vec![
            ("web", idle_stats("2026-01-01T00:00:20Z")),
            // 同じ時刻（ミリ秒未満だけが違う場合も含む）
            ("same", idle_stats("2026-01-01T00:00:10.123999999Z")),
            ("back", idle_stats("2026-01-01T00:00:09Z")),
            ("broken", idle_stats("not a time")),
            ("broken-before", idle_stats("2026-01-01T00:00:20Z")),
            ("zero", idle_stats(ZERO_TIME)),
            ("zero-before", idle_stats("2026-01-01T00:00:20Z")),
        ]);
        let usages = calc_usages(TICK_MILLIS, &now, &prev).unwrap();
        assert_eq!(usages.usages.keys().collect::<Vec<_>>(), vec!["web"]);
        assert_eq!(usages.time, "2026-01-01T00:00:20Z");

        // 時刻の無い統計（存在しないコンテナへの応答など）でも panic しない
        let no_time = stats_map(vec![("web", Stats::default())]);
        assert!(calc_usages(TICK_MILLIS, &no_time, &prev).is_none());
        assert!(calc_usages(TICK_MILLIS, &now, &no_time).is_none());
    }

    #[test]
    fn tick_time_comes_from_a_recorded_container() {
        let prev = stats_map(vec![
            ("a-stuck", idle_stats("2026-01-01T00:00:19Z")),
            ("b", idle_stats("2026-01-01T00:00:10.200Z")),
            ("c", idle_stats("2026-01-01T00:00:10.100Z")),
            ("d", idle_stats("2026-01-01T00:00:10.300Z")),
        ]);
        let now = stats_map(vec![
            // 飛ばされるコンテナ。時刻は最も早いが、tick の時刻には使わない
            ("a-stuck", idle_stats("2026-01-01T00:00:19Z")),
            ("a-new", idle_stats("2026-01-01T00:00:19.5Z")),
            ("b", idle_stats("2026-01-01T00:00:20.200000001Z")),
            ("c", idle_stats("2026-01-01T00:00:20.100000002Z")),
            ("d", idle_stats("2026-01-01T00:00:20.300Z")),
        ]);
        // HashMap の順序に左右されず、記録するコンテナのうち最も早い時刻になる
        for _ in 0..20 {
            let usages = calc_usages(TICK_MILLIS, &now, &prev).unwrap();
            // Docker が返した文字列をそのまま使う
            assert_eq!(usages.time, "2026-01-01T00:00:20.100000002Z");
            assert_eq!(usages.usages.len(), 3);
        }

        // 記録できるコンテナが無ければ、tick ごと記録しない（panic しない）
        let stuck = stats_map(vec![
            ("a-stuck", idle_stats("2026-01-01T00:00:19Z")),
        ]);
        assert!(calc_usages(TICK_MILLIS, &stuck, &prev).is_none());
    }

    #[test]
    fn log_line_format_is_unchanged() {
        // キー名・構造・並びは変えない。速度（kBps）だけが小数2桁になる
        let prev = stats_map(vec![
            ("web", stats_at(
                "2026-01-01T00:00:00.023034093Z",
                1_000_000_000, 50_000_000_000, (1000, 2000), (3000, 4000),
            )),
        ]);
        let now = stats_map(vec![
            ("web", stats_at(
                "2026-01-01T00:00:10.023034093Z",
                1_003_000_000, 69_860_000_000,
                (4_161_000, 48_689_000), (40_004_000, 14_000),
            )),
        ]);
        let usages = calc_usages(TICK_MILLIS, &now, &prev).unwrap();
        assert_eq!(
            usages.to_string(),
            concat!(
                r#"{"time":"2026-01-01T00:00:10.023034093Z","millis":10000,"stats":{"#,
                r#""web":{"cpu":{"percentage":0.03,"total":3,"system":19860,"ncpu":2},"#,
                r#""memory":{"percentage":20.00,"used":200000,"available":1000000},"#,
                r#""io":{"readkB":4161,"writekB":48689,"readkBps":416.00,"writekBps":4868.70},"#,
                r#""net":{"recvkB":40004,"sendkB":14,"recvkBps":4000.10,"sendkBps":1.00}}}}"#,
            ),
        );
    }

    #[test]
    fn written_line_is_restored_with_io_write() {
        let line = concat!(
            r#"{"time":"2026-01-01T00:00:10.023034093Z","millis":10000,"stats":{"#,
            r#""web":{"cpu":{"percentage":0.03,"total":3,"system":19860,"ncpu":2},"#,
            r#""memory":{"percentage":3.64,"used":36696064,"available":1007222784},"#,
            r#""io":{"readkB":4161,"writekB":48689,"readkBps":11.50,"writekBps":0.35},"#,
            r#""net":{"recvkB":4,"sendkB":9,"recvkBps":33.00,"sendkBps":0.00}}}}"#,
        );
        let usages = json_to_usage(&json::parse(line).unwrap()).unwrap();
        let web = &usages.usages["web"];
        assert_eq!(web.io.readkB, Some(4161));
        assert_eq!(web.io.writekB, Some(48689));
        assert_eq!(web.io.readkBps, Some(11.5));
        assert_eq!(web.io.writekBps, Some(0.35));
        assert_eq!(web.net.recvkBps, Some(33.0));
        assert_eq!(web.net.sendkBps, Some(0.0));
        // 読んだものを書き出すと元の行に戻る
        assert_eq!(usages.to_string(), line);
    }

    #[test]
    fn line_with_integer_rates_is_still_restored() {
        // 速度を整数で書いていた頃の行
        let old_line = concat!(
            r#"{"time":"2026-01-01T00:00:10.023034093Z","millis":10000,"stats":{"#,
            r#""web":{"cpu":{"percentage":0.03,"total":3,"system":19860,"ncpu":2},"#,
            r#""memory":{"percentage":3.64,"used":36696064,"available":1007222784},"#,
            r#""io":{"readkB":4161,"writekB":48689,"readkBps":11,"writekBps":0},"#,
            r#""net":{"recvkB":4,"sendkB":9,"recvkBps":null,"sendkBps":4294967295}}}}"#,
        );
        let usages = json_to_usage(&json::parse(old_line).unwrap()).unwrap();
        let web = &usages.usages["web"];
        assert_eq!(web.io.readkBps, Some(11.0));
        assert_eq!(web.io.writekBps, Some(0.0));
        assert_eq!(web.net.recvkBps, None);
        // 上限に張り付いた値（u32::MAX）も読める
        assert_eq!(web.net.sendkBps, Some(u32::MAX as f32));
        // 積算値は整数のまま
        assert_eq!(web.io.readkB, Some(4161));
        assert_eq!(web.net.sendkB, Some(9));

        // 整数の行と小数の行が混じったログを、どちらも復元できる
        let new_line = old_line
            .replace("2026-01-01T00:00:10.023034093Z", "2026-01-01T00:00:20.023034093Z")
            .replace(r#""readkBps":11,"writekBps":0}"#, r#""readkBps":11.25,"writekBps":0.35}"#)
            .replace(r#""recvkBps":null,"sendkBps":4294967295}"#, r#""recvkBps":0.07,"sendkBps":1234.50}"#);
        assert!(new_line.contains(r#""writekBps":0.35}"#) && new_line.contains(r#""recvkBps":0.07,"#));
        let log = format!("{}\r\n{}\r\n", old_line, new_line);
        let (summary, cache) = restore(log.as_bytes(), 0);
        assert_eq!(summary, RestoreSummary { lines: 2, restored: 2 });
        let lock = cache.read().unwrap();
        let web: Vec<_> = lock.get("web").unwrap().iter().collect();
        assert_eq!(
            (web[0].read_kbps, web[0].write_kbps, web[0].recv_kbps),
            (Some(11.0), Some(0.0), None),
        );
        assert_eq!(
            (web[1].read_kbps, web[1].write_kbps, web[1].recv_kbps, web[1].send_kbps),
            (Some(11.25), Some(0.35), Some(0.07), Some(1234.5)),
        );
    }

    /// ログの1行（行末の \r\n 込み）。値は cpu と io write だけ変える
    /// 速度は整数で書く（小数にする前の形式。復元できることを保つ）
    fn log_line(time: &str, containers: &[(&str, f32, u32)]) -> String {
        let stats = containers.iter()
            .map(|(name, cpu, write)| format!(
                concat!(
                    r#""{name}":{{"cpu":{{"percentage":{cpu:.2},"total":3,"system":19860,"ncpu":2}},"#,
                    r#""memory":{{"percentage":3.64,"used":36696064,"available":1007222784}},"#,
                    r#""io":{{"readkB":4161,"writekB":48689,"readkBps":5,"writekBps":{write}}},"#,
                    r#""net":{{"recvkB":4,"sendkB":0,"recvkBps":6,"sendkBps":7}}}}"#,
                ),
                name = name, cpu = cpu, write = write,
            ))
            .collect::<Vec<String>>()
            .join(",");
        format!(
            "{{\"time\":\"{}\",\"millis\":10000,\"stats\":{{{}}}}}\r\n",
            time, stats,
        )
    }

    fn restore(
        log: &[u8],
        lines_to_skip: u64,
    ) -> (RestoreSummary, log_cache::SharedUsageCache) {
        let cache = create_shared_cache();
        let summary = restore_from_reader(
            std::io::Cursor::new(log.to_vec()),
            lines_to_skip,
            &mut cache.write().unwrap(),
        ).unwrap();
        (summary, cache)
    }

    fn restored_times(
        cache: &log_cache::SharedUsageCache,
        name: &str,
    ) -> Vec<String> {
        cache.read().unwrap().get(name).unwrap().iter()
            .map(|p| time::millis_to_iso8601(p.time_ms))
            .collect()
    }

    #[test]
    fn restore_skips_bad_lines_and_keeps_reading() {
        let mut log: Vec<u8> = Vec::new();
        log.extend(log_line("2026-01-01T00:00:10.023034093Z", &[("web", 1.0, 11)]).bytes());
        // Docker が返した ZERO_TIME がそのまま書かれた行
        log.extend(log_line(ZERO_TIME, &[("web", 2.0, 12)]).bytes());
        log.extend(log_line("2026-01-01T00:00:20.5Z", &[("web", 3.0, 13)]).bytes());
        // json として壊れた行、途中で切れた行、空行、jsonだが形が違う行
        log.extend(b"this is not json\r\n");
        log.extend(b"{\"time\":\"2026-01-01T00:00:25Z\",\"millis\":10000,\"stats\":{\"web\":{\"cpu\r\n");
        log.extend(b"\r\n");
        log.extend(b"[1,2,3]\r\n");
        log.extend(b"{\"millis\":10000,\"stats\":{}}\r\n");
        // 文字コードとして壊れた行
        log.extend(b"\xff\xfe\x00garbage\r\n");
        // time を解釈できない行
        log.extend(log_line("2026-01-01Txx:yy:zzZ", &[("web", 4.0, 14)]).bytes());
        log.extend(log_line("2026-01-01T00:00:30Z", &[("web", 5.0, 15), ("db", 6.0, 16)]).bytes());
        // 最後の行は書きかけ（改行が無く、途中で切れている）
        log.extend(b"{\"time\":\"2026-01-01T00:00:40Z\",\"mil");

        let (summary, cache) = restore(&log, 0);
        assert_eq!(summary, RestoreSummary { lines: 12, restored: 3 });
        assert_eq!(
            restored_times(&cache, "web"),
            vec![
                "2026-01-01T00:00:10.023Z",
                "2026-01-01T00:00:20.500Z",
                "2026-01-01T00:00:30.000Z",
            ],
        );
        assert_eq!(restored_times(&cache, "db"), vec!["2026-01-01T00:00:30.000Z"]);

        // IO の write と、ほかの値も復元されている
        let lock = cache.read().unwrap();
        let web: Vec<_> = lock.get("web").unwrap().iter().collect();
        assert_eq!(
            web.iter().map(|p| p.write_kbps).collect::<Vec<_>>(),
            vec![Some(11.0), Some(13.0), Some(15.0)],
        );
        assert_eq!(web[2].cpu, Some(5.0));
        assert_eq!(web[2].memory, Some(3.64));
        assert_eq!(web[2].read_kbps, Some(5.0));
        assert_eq!(web[2].recv_kbps, Some(6.0));
        assert_eq!(web[2].send_kbps, Some(7.0));
        assert_eq!(lock.get("db").unwrap()[0].write_kbps, Some(16.0));
    }

    #[test]
    fn restore_drops_lines_older_than_24_hours() {
        let mut log = String::new();
        // 3日前に止まった old と、25時間前・24時間前・現在の web
        log += &log_line("2025-12-29T00:00:00Z", &[("web", 1.0, 1), ("old", 1.0, 1)]);
        log += &log_line("2025-12-31T23:00:00Z", &[("web", 2.0, 2)]);
        log += &log_line("2026-01-01T00:00:00Z", &[("web", 3.0, 3)]);
        log += &log_line("2026-01-01T12:00:00Z", &[("web", 4.0, 4), ("db", 4.0, 4)]);
        log += &log_line("2026-01-02T00:00:00Z", &[("web", 5.0, 5)]);

        let (summary, cache) = restore(log.as_bytes(), 0);
        assert_eq!(summary, RestoreSummary { lines: 5, restored: 5 });
        assert_eq!(
            restored_times(&cache, "web"),
            vec![
                "2026-01-01T00:00:00.000Z",
                "2026-01-01T12:00:00.000Z",
                "2026-01-02T00:00:00.000Z",
            ],
        );
        // 最新の点が24時間以内にあるコンテナは残り、無いコンテナは一覧から消える
        let lock = cache.read().unwrap();
        assert_eq!(lock.container_names(), vec!["db", "web"]);
        assert!(lock.get("old").is_none());
    }

    #[test]
    fn restore_skips_leading_lines_without_parsing() {
        let mut log = String::from("broken line that must not be parsed\r\n");
        for i in 0..5 {
            log += &log_line(
                &format!("2026-01-01T00:00:{:02}Z", i * 10),
                &[("web", i as f32, i)],
            );
        }
        let (summary, cache) = restore(log.as_bytes(), 3);
        assert_eq!(summary, RestoreSummary { lines: 3, restored: 3 });
        assert_eq!(
            restored_times(&cache, "web"),
            vec![
                "2026-01-01T00:00:20.000Z",
                "2026-01-01T00:00:30.000Z",
                "2026-01-01T00:00:40.000Z",
            ],
        );
        // 行数より多く飛ばしても、空のまま正常に終わる
        let (summary, cache) = restore(log.as_bytes(), 100);
        assert_eq!(summary, RestoreSummary { lines: 0, restored: 0 });
        assert!(cache.read().unwrap().container_names().is_empty());
        // 空のログ
        let (summary, _) = restore(b"", 0);
        assert_eq!(summary, RestoreSummary { lines: 0, restored: 0 });
    }

    #[test]
    fn restore_of_more_than_a_day_keeps_the_latest_window() {
        // 10秒間隔で 9000 行（25時間分）
        let mut log = String::new();
        for i in 0..9000_u32 {
            let seconds = i * 10;
            log += &log_line(
                &format!(
                    "2026-01-{:02}T{:02}:{:02}:{:02}.000000001Z",
                    1 + seconds / 86_400,
                    (seconds / 3600) % 24, (seconds / 60) % 60, seconds % 60,
                ),
                &[("web", (i % 50) as f32, i)],
            );
        }
        let (summary, cache) = restore(log.as_bytes(), 0);
        assert_eq!(summary, RestoreSummary { lines: 9000, restored: 9000 });
        let lock = cache.read().unwrap();
        let web = lock.get("web").unwrap();
        assert_eq!(web.len(), log_cache::MAX_LOG_LENGTH);
        assert_eq!(web.back().unwrap().write_kbps, Some(8999.0));
        assert_eq!(web.front().unwrap().write_kbps, Some(360.0));
        // 復元した系列は、write を基準にしたダウンサンプリングでも末尾まで返る
        let sampled = lock
            .downsample("web", &DownsampleOption::default(), Series::IoWrite)
            .unwrap();
        assert_eq!(sampled.len(), 512);
        assert_eq!(sampled[511].write_kbps, Some(8999.0));
    }

    #[test]
    fn collected_ticks_reach_the_cache() {
        // 算出 → ログの1行 → 復元、と通しても同じ値になる
        let tick = |second: u32, bytes: u64| stats_map(vec![
            ("web", stats_at(
                &format!("2026-01-01T00:00:{:02}.5Z", second),
                0, 0, (0, bytes), (0, 0),
            )),
        ]);
        let first = tick(0, 0);
        let second = tick(10, 1_000_000);
        let third = tick(30, 3_000_000); // 1回飛んで20秒後

        let cache = create_shared_cache();
        let mut log = String::new();
        for (now, prev) in [(&second, &first), (&third, &second)] {
            let usages = calc_usages(TICK_MILLIS, now, prev).unwrap();
            assert!(store_usages(&usages, &mut cache.write().unwrap()));
            log += &(usages.to_string() + "\r\n");
        }
        let (summary, restored) = restore(log.as_bytes(), 0);
        assert_eq!(summary, RestoreSummary { lines: 2, restored: 2 });

        for cache in [&cache, &restored] {
            let lock = cache.read().unwrap();
            let web: Vec<_> = lock.get("web").unwrap().iter().collect();
            assert_eq!(web.len(), 2);
            assert_eq!(
                time::millis_to_iso8601(web[1].time_ms),
                "2026-01-01T00:00:30.500Z",
            );
            // どちらの tick も 100 kB/s（飛んだ tick で2倍にならない）
            assert_eq!(web[0].write_kbps, Some(100.0));
            assert_eq!(web[1].write_kbps, Some(100.0));
        }
    }

    /// 統計の本文（json）から、時刻以外の一部だけを差し替えた統計を作る
    /// 各引数は json の値をそのまま書く（そのキーごと省くなら None）
    fn stats_from_parts(
        read: &str,
        memory_stats: Option<&str>,
        networks: Option<&str>,
        blkio: Option<&str>,
    ) -> Stats {
        let mut parts = vec![format!(r#""read":"{}""#, read)];
        if let Some(v) = memory_stats {
            parts.push(format!(r#""memory_stats":{}"#, v));
        }
        if let Some(v) = networks {
            parts.push(format!(r#""networks":{}"#, v));
        }
        if let Some(v) = blkio {
            parts.push(format!(
                r#""blkio_stats":{{"io_service_bytes_recursive":{}}}"#, v,
            ));
        }
        parse_stats_response(&format!(
            "HTTP/1.1 200 OK\r\n\r\n{{{}}}\n", parts.join(","),
        )).unwrap()
    }

    const T0: &str = "2026-01-01T00:00:00Z";
    const T10: &str = "2026-01-01T00:00:10Z";

    /// 前回と今回の統計から、web の使用率を求める
    fn usage_between(prev: Stats, now: Stats) -> Usage {
        let prev = stats_map(vec![("web", prev)]);
        let now = stats_map(vec![("web", now)]);
        calc_usages(TICK_MILLIS, &now, &prev).unwrap()
            .usages.remove("web").unwrap()
    }

    fn memory_of(memory_stats: Option<&str>) -> MemoryUsage {
        usage_between(
            stats_from_parts(T0, memory_stats, None, None),
            stats_from_parts(T10, memory_stats, None, None),
        ).memory
    }

    #[test]
    fn memory_follows_the_docker_cli_rule() {
        // cgroup v1: total_inactive_file を引く（cache と inactive_file は見ない）
        let v1 = memory_of(Some(concat!(
            r#"{"usage":500000,"limit":2000000,"stats":{"cache":300000,"#,
            r#""inactive_file":7,"total_cache":300000,"total_inactive_file":100000}}"#,
        )));
        assert_eq!(v1.used, Some(400_000));
        assert_eq!(v1.available, Some(2_000_000));
        assert_eq!(v1.percentage, Some(20.0));

        // cgroup v2: cache が無く、inactive_file を引く
        let v2 = memory_of(Some(concat!(
            r#"{"usage":500000,"limit":2000000,"stats":{"anon":150000,"#,
            r#""file":350000,"active_file":50000,"inactive_file":300000}}"#,
        )));
        assert_eq!(v2.used, Some(200_000));
        assert_eq!(v2.percentage, Some(10.0));

        // どちらも無ければ usage のまま。cache があっても引かない
        for stats in [
            r#"{"usage":500000,"limit":2000000,"stats":{"cache":300000,"anon":150000}}"#,
            r#"{"usage":500000,"limit":2000000,"stats":{}}"#,
            r#"{"usage":500000,"limit":2000000}"#,
        ] {
            let plain = memory_of(Some(stats));
            assert_eq!(plain.used, Some(500_000), "{}", stats);
            assert_eq!(plain.percentage, Some(25.0), "{}", stats);
        }
    }

    #[test]
    fn memory_ignores_a_value_not_smaller_than_usage() {
        // 引く値が usage 以上なら使わない（同じ値も含む）
        for stats in [
            r#"{"usage":500000,"limit":2000000,"stats":{"total_inactive_file":500000}}"#,
            r#"{"usage":500000,"limit":2000000,"stats":{"total_inactive_file":900000}}"#,
            r#"{"usage":500000,"limit":2000000,"stats":{"inactive_file":500000}}"#,
            r#"{"usage":500000,"limit":2000000,"stats":{"inactive_file":900000}}"#,
        ] {
            let memory = memory_of(Some(stats));
            assert_eq!(memory.used, Some(500_000), "{}", stats);
            assert_eq!(memory.percentage, Some(25.0), "{}", stats);
        }
        // total_inactive_file が使えなければ、inactive_file を見る
        let fallback = memory_of(Some(concat!(
            r#"{"usage":500000,"limit":2000000,"#,
            r#""stats":{"total_inactive_file":900000,"inactive_file":100000}}"#,
        )));
        assert_eq!(fallback.used, Some(400_000));
        // 引く値がちょうど usage より 1 小さい
        let edge = memory_of(Some(
            r#"{"usage":500000,"limit":2000000,"stats":{"inactive_file":499999}}"#,
        ));
        assert_eq!(edge.used, Some(1));
    }

    #[test]
    fn memory_without_a_limit_is_null() {
        // limit が 0
        let zero = memory_of(Some(
            r#"{"usage":500000,"limit":0,"stats":{"inactive_file":100000}}"#,
        ));
        assert_eq!(zero.percentage, None);
        assert_eq!(zero.used, Some(400_000));
        // usage も 0（0 / 0）
        let both_zero = memory_of(Some(r#"{"usage":0,"limit":0}"#));
        assert_eq!(both_zero.percentage, None);
        // limit が無い
        let no_limit = memory_of(Some(
            r#"{"usage":500000,"stats":{"inactive_file":100000}}"#,
        ));
        assert_eq!(no_limit.percentage, None);
        assert_eq!(no_limit.available, None);
        // memory_stats が空、または無い
        for stats in [Some("{}"), None] {
            let none = memory_of(stats);
            assert_eq!(none.percentage, None);
            assert_eq!(none.used, None);
        }
        // ログの1行が json として読め、null で出る
        let usage = usage_between(
            stats_from_parts(T0, Some(r#"{"usage":5,"limit":0}"#), None, None),
            stats_from_parts(T10, Some(r#"{"usage":5,"limit":0}"#), None, None),
        );
        let line = usage.to_string();
        assert!(json::parse(&line).is_ok(), "{}", line);
        assert!(line.contains(
            r#""memory":{"percentage":null,"used":5,"available":0}"#
        ), "{}", line);
    }

    #[test]
    fn network_sums_every_interface() {
        // インターフェース 1 つ
        let single = stats_from_parts(
            T0, None, Some(r#"{"eth0":{"rx_bytes":3000,"tx_bytes":4000}}"#), None,
        );
        assert_eq!(single.net.recv, Some(3000));
        assert_eq!(single.net.send, Some(4000));

        // 複数は合算する。eth0 という名前が無くてもよい
        let multi = stats_from_parts(T10, None, Some(concat!(
            r#"{"eth1":{"rx_bytes":10000,"rx_packets":9,"tx_bytes":20000},"#,
            r#""eth2":{"rx_bytes":2000,"tx_bytes":300},"#,
            r#""enp3s0":{"rx_bytes":1,"tx_bytes":2}}"#,
        )), None);
        assert_eq!(multi.net.recv, Some(12_001));
        assert_eq!(multi.net.send, Some(20_302));

        // 速度は、インターフェースごとの差分の合算
        // recv: (9000 + 0 + 1) / 10000 ms、send: (16000 + 300 + 2) / 10000 ms
        let before = stats_from_parts(T0, None, Some(concat!(
            r#"{"eth1":{"rx_bytes":1000,"tx_bytes":4000},"#,
            r#""eth2":{"rx_bytes":2000,"tx_bytes":0}}"#,
        )), None);
        let usage = usage_between(before, multi);
        assert_eq!(usage.net.recvkB, Some(12));
        assert_eq!(usage.net.sendkB, Some(20));
        let line = usage.to_string();
        assert!(line.contains(
            r#""net":{"recvkB":12,"sendkB":20,"recvkBps":0.90,"sendkBps":1.63}"#
        ), "{}", line);
    }

    #[test]
    fn missing_networks_are_null() {
        // ホストネットワークなど。networks が無い、または null
        for networks in [None, Some("null")] {
            let prev = stats_from_parts(T0, None, networks, None);
            assert_eq!(prev.net.recv, None);
            assert_eq!(prev.net.send, None);
            let usage = usage_between(prev, stats_from_parts(T10, None, networks, None));
            assert_eq!(usage.net.recvkBps, None);
            assert_eq!(usage.net.sendkBps, None);
            let line = usage.to_string();
            assert!(line.contains(
                r#""net":{"recvkB":null,"sendkB":null,"recvkBps":null,"sendkBps":null}"#
            ), "{}", line);
        }
    }

    #[test]
    fn io_sums_every_device() {
        // デバイス 1 つ
        let single = stats_from_parts(T0, None, None, Some(concat!(
            r#"[{"major":8,"minor":0,"op":"read","value":1000},"#,
            r#"{"major":8,"minor":0,"op":"write","value":2000}]"#,
        )));
        assert_eq!(single.io.read, Some(1000));
        assert_eq!(single.io.write, Some(2000));

        // 複数は合算する
        let multi = stats_from_parts(T10, None, None, Some(concat!(
            r#"[{"major":8,"minor":0,"op":"read","value":5000},"#,
            r#"{"major":8,"minor":0,"op":"write","value":6000},"#,
            r#"{"major":8,"minor":16,"op":"read","value":700},"#,
            r#"{"major":8,"minor":16,"op":"write","value":80},"#,
            r#"{"major":253,"minor":0,"op":"write","value":9}]"#,
        )));
        assert_eq!(multi.io.read, Some(5700));
        assert_eq!(multi.io.write, Some(6089));

        // 速度も合算した値の差分: (5700 - 1000) / 10000 ms、(6089 - 2000) / 10000 ms
        let usage = usage_between(single, multi);
        let line = usage.to_string();
        assert!(line.contains(
            r#""io":{"readkB":5,"writekB":6,"readkBps":0.47,"writekBps":0.41}"#
        ), "{}", line);
    }

    #[test]
    fn io_op_is_case_insensitive_and_other_ops_are_ignored() {
        // cgroup v1 の形: op が大文字始まりで、read / write 以外も混じる
        let v1 = stats_from_parts(T0, None, None, Some(concat!(
            r#"[{"major":8,"minor":0,"op":"Read","value":1000},"#,
            r#"{"major":8,"minor":0,"op":"Write","value":2000},"#,
            r#"{"major":8,"minor":0,"op":"Sync","value":40000},"#,
            r#"{"major":8,"minor":0,"op":"Async","value":50000},"#,
            r#"{"major":8,"minor":0,"op":"Discard","value":60000},"#,
            r#"{"major":8,"minor":0,"op":"Total","value":70000},"#,
            r#"{"major":8,"minor":16,"op":"Read","value":30},"#,
            r#"{"major":8,"minor":16,"op":"Write","value":4},"#,
            r#"{"major":8,"minor":16,"op":"Total","value":34}]"#,
        )));
        assert_eq!(v1.io.read, Some(1030));
        assert_eq!(v1.io.write, Some(2004));

        // 小文字に read / write 以外が混じる形、大文字だけの形
        let mixed = stats_from_parts(T0, None, None, Some(concat!(
            r#"[{"op":"sync","value":40000},{"op":"read","value":10},"#,
            r#"{"op":"total","value":70000},{"op":"WRITE","value":20},"#,
            r#"{"op":"READ","value":1},{"op":"readahead","value":999},"#,
            r#"{"value":888},{"op":null,"value":777}]"#,
        )));
        assert_eq!(mixed.io.read, Some(11));
        assert_eq!(mixed.io.write, Some(20));
    }

    #[test]
    fn missing_io_entries_are_null() {
        // 配列が空、null、キーごと無い
        for blkio in [Some("[]"), Some("null"), None] {
            let prev = stats_from_parts(T0, None, None, blkio);
            assert_eq!(prev.io.read, None, "{:?}", blkio);
            assert_eq!(prev.io.write, None, "{:?}", blkio);
            let usage = usage_between(prev, stats_from_parts(T10, None, None, blkio));
            let line = usage.to_string();
            assert!(line.contains(
                r#""io":{"readkB":null,"writekB":null,"readkBps":null,"writekBps":null}"#
            ), "{:?} {}", blkio, line);
        }
        // read / write の要素が1つも無ければ、同じく null
        let others = stats_from_parts(
            T0, None, None, Some(r#"[{"op":"sync","value":1},{"op":"total","value":2}]"#),
        );
        assert_eq!(others.io.read, None);
        assert_eq!(others.io.write, None);
    }

    #[test]
    fn rates_below_one_kb_per_second_are_kept() {
        // 10秒で 3,500 バイト → 0.35 kB/s（整数に切り捨てていた頃は 0）
        let before = || stats_at(T0, 0, 0, (1000, 2000), (3000, 4000));
        let after = || stats_at(T10, 0, 0, (4500, 2000), (3050, 7500));
        let usage = usage_between(before(), after());
        let close = |value: Option<f32>, expected: f32| {
            value.is_some_and(|v| (v - expected).abs() < 1e-6)
        };
        assert!(close(usage.io.readkBps, 0.35), "{:?}", usage.io.readkBps);
        // 増えていなければ 0
        assert_eq!(usage.io.writekBps, Some(0.0));
        // 小数2桁に満たない値も、持っている間は 0 にしない（書き出すときに丸める）
        assert!(close(usage.net.recvkBps, 0.005), "{:?}", usage.net.recvkBps);
        assert!(close(usage.net.sendkBps, 0.35), "{:?}", usage.net.sendkBps);

        // ログの1行: 速度は小数2桁、積算値（kB）は整数のまま
        let line = usage.to_string();
        assert!(line.contains(
            r#""io":{"readkB":4,"writekB":2,"readkBps":0.35,"writekBps":0.00}"#
        ), "{}", line);
        assert!(line.contains(r#""net":{"recvkB":3,"sendkB":7,"#), "{}", line);
        assert!(line.contains(r#""sendkBps":0.35}"#), "{}", line);

        // 算出 → ログの1行 → 復元 → API の要素、と通しても 0.35 のまま
        let prev = stats_map(vec![("web", before())]);
        let now = stats_map(vec![("web", after())]);
        let usages = calc_usages(TICK_MILLIS, &now, &prev).unwrap();
        let (summary, cache) = restore((usages.to_string() + "\r\n").as_bytes(), 0);
        assert_eq!(summary, RestoreSummary { lines: 1, restored: 1 });
        let lock = cache.read().unwrap();
        let point = &lock.get("web").unwrap()[0];
        assert_eq!(point.read_kbps, Some(0.35));
        assert_eq!(
            Series::IoRead.point_to_json(point),
            r#"{"time":"2026-01-01T00:00:10.000Z","readkBps":0.35,"writekBps":0.00}"#,
        );
        let net = Series::NetSend.point_to_json(point);
        assert!(net.ends_with(r#""sendkBps":0.35}"#), "{}", net);
    }

    #[test]
    fn cpu_delta_below_one_millisecond_is_not_zero() {
        // total の差分は 900,000 ns（0.9 ms）。system の差分は 10秒 × 2 cores
        // 0.0009 s / 20 s × 2 cores × 100 = 0.009 %
        let usage = usage_between(
            stats_at(T0, 5_000_400_000, 70_000_000_000, (0, 0), (0, 0)),
            stats_at(T10, 5_001_300_000, 90_000_000_000, (0, 0), (0, 0)),
        );
        let percentage = usage.cpu.percentage.unwrap();
        assert!(percentage > 0.0);
        assert!((percentage - 0.009).abs() < 1e-6, "{}", percentage);
        // ログの total / system は ms の整数のまま（差分を ms に丸めた値）
        assert_eq!(usage.cpu.total, Some(0));
        assert_eq!(usage.cpu.system, Some(20_000));
        let line = usage.to_string();
        assert!(line.contains(
            r#""cpu":{"percentage":0.01,"total":0,"system":20000,"ncpu":2}"#
        ), "{}", line);

        // ms の境界をまたがない 0.4 ms の差分が続いても、毎回 0 にならない
        // (ms に丸めてから差を取っていた頃は、0 と 1 ms 分の値が混じっていた)
        let step = |i: u64| stats_at(
            &format!("2026-01-01T00:00:{:02}Z", i * 10),
            5_000_000_000 + i * 400_000, 70_000_000_000 + i * 20_000_000_000,
            (0, 0), (0, 0),
        );
        for i in 1..5 {
            let usage = usage_between(step(i - 1), step(i));
            let percentage = usage.cpu.percentage.unwrap();
            assert!((percentage - 0.004).abs() < 1e-6, "{}: {}", i, percentage);
            assert_eq!(usage.cpu.total, Some(0));
        }

        // ms 未満の端数を含む差分でも、比は ns で求める: 1.5 ms / 10,000.5 ms × 2 × 100
        let usage = usage_between(
            stats_at(T0, 0, 0, (0, 0), (0, 0)),
            stats_at(T10, 1_500_000, 10_000_500_000, (0, 0), (0, 0)),
        );
        let expected = (1_500_000_f64 / 10_000_500_000_f64 * 200.0) as f32;
        assert_eq!(usage.cpu.percentage, Some(expected));
        assert_eq!(usage.cpu.total, Some(1));
        assert_eq!(usage.cpu.system, Some(10_000));
    }

    fn net_stats(read: &str, networks: &str) -> Stats {
        stats_from_parts(read, None, Some(networks), None)
    }

    fn io_stats(read: &str, blkio: &str) -> Stats {
        stats_from_parts(read, None, None, Some(blkio))
    }

    #[test]
    fn removed_interface_does_not_hide_the_traffic_of_the_others() {
        // eth1 は大きな積算値を持つ。次の tick までに外され、eth0 だけが残る
        let usage = usage_between(
            net_stats(T0, concat!(
                r#"{"eth0":{"rx_bytes":0,"tx_bytes":500},"#,
                r#""eth1":{"rx_bytes":1000000,"tx_bytes":2000000}}"#,
            )),
            net_stats(T10, r#"{"eth0":{"rx_bytes":1000,"tx_bytes":4000}}"#),
        );
        // eth0 の増分（1,000 / 3,500 バイト）が、合計の減少に埋もれずに残る
        // 合計同士の差（1,000 - 1,000,000）だと 0 になっていた
        assert_eq!(usage.net.recvkBps, Some(0.1));
        assert_eq!(usage.net.sendkBps, Some(0.35));
        // 積算値は、その時点で残っているインターフェースの合計
        assert_eq!(usage.net.recvkB, Some(1));
        assert_eq!(usage.net.sendkB, Some(4));
        let line = usage.to_string();
        assert!(line.contains(
            r#""net":{"recvkB":1,"sendkB":4,"recvkBps":0.10,"sendkBps":0.35}"#
        ), "{}", line);
    }

    #[test]
    fn added_interface_counts_from_zero() {
        // eth1 が追加された。新しいインターフェースの積算値は 0 から始まるので、
        // 今回の値がそのまま増分になる
        let usage = usage_between(
            net_stats(T0, r#"{"eth0":{"rx_bytes":50000,"tx_bytes":60000}}"#),
            net_stats(T10, concat!(
                r#"{"eth0":{"rx_bytes":51000,"tx_bytes":60000},"#,
                r#""eth1":{"rx_bytes":2000,"tx_bytes":500}}"#,
            )),
        );
        // (1,000 + 2,000) / 10,000 ms、(0 + 500) / 10,000 ms
        assert_eq!(usage.net.recvkBps, Some(0.3));
        assert_eq!(usage.net.sendkBps, Some(0.05));

        // 入れ替わった場合: 外された分は数えず、追加された分は 0 から数える
        let usage = usage_between(
            net_stats(T0, r#"{"eth0":{"rx_bytes":900000,"tx_bytes":900000}}"#),
            net_stats(T10, r#"{"eth1":{"rx_bytes":2000,"tx_bytes":0}}"#),
        );
        assert_eq!(usage.net.recvkBps, Some(0.2));
        assert_eq!(usage.net.sendkBps, Some(0.0));
    }

    #[test]
    fn counter_reset_of_one_interface_does_not_cancel_the_others() {
        // eth1 の積算値だけが戻った（つなぎ直しなど）。eth1 の差分だけを 0 とし、
        // eth0 の増分は残す
        let usage = usage_between(
            net_stats(T0, concat!(
                r#"{"eth0":{"rx_bytes":1000,"tx_bytes":1000},"#,
                r#""eth1":{"rx_bytes":800000,"tx_bytes":800000}}"#,
            )),
            net_stats(T10, concat!(
                r#"{"eth0":{"rx_bytes":6000,"tx_bytes":1000},"#,
                r#""eth1":{"rx_bytes":300,"tx_bytes":0}}"#,
            )),
        );
        assert_eq!(usage.net.recvkBps, Some(0.5));
        assert_eq!(usage.net.sendkBps, Some(0.0));
    }

    #[test]
    fn first_interfaces_count_from_zero_and_no_interface_is_null() {
        let some = r#"{"eth0":{"rx_bytes":1000,"tx_bytes":3500}}"#;
        // 前回は networks が無かった（null・空も同じ）。今回の値を 0 からの増分として数える
        for before in [
            stats_from_parts(T0, None, None, None),
            net_stats(T0, "null"),
            net_stats(T0, "{}"),
        ] {
            let usage = usage_between(before, net_stats(T10, some));
            assert_eq!(usage.net.recvkBps, Some(0.1));
            assert_eq!(usage.net.sendkBps, Some(0.35));
        }
        // 今回は networks が無い。値そのものが無いので null
        for after in [
            stats_from_parts(T10, None, None, None),
            net_stats(T10, "{}"),
        ] {
            let usage = usage_between(net_stats(T0, some), after);
            assert_eq!(usage.net.recvkBps, None);
            assert_eq!(usage.net.sendkBps, None);
            assert_eq!(usage.net.recvkB, None);
        }
    }

    #[test]
    fn first_io_of_an_op_counts_from_zero() {
        let read_only = r#"[{"major":8,"minor":0,"op":"read","value":1000}]"#;
        let both = concat!(
            r#"[{"major":8,"minor":0,"op":"read","value":2000},"#,
            r#"{"major":8,"minor":0,"op":"write","value":3500}]"#,
        );
        // 前回は read の要素だけ。今回初めて write の要素が現れた
        let usage = usage_between(io_stats(T0, read_only), io_stats(T10, both));
        assert_eq!(usage.io.readkBps, Some(0.1));
        assert_eq!(usage.io.writekBps, Some(0.35));
        let line = usage.to_string();
        assert!(line.contains(
            r#""io":{"readkB":2,"writekB":3,"readkBps":0.10,"writekBps":0.35}"#
        ), "{}", line);

        // 前回は IO の要素が1つも無かった（空・null・キーごと無い・ほかの op だけ）
        // まだ読み書きしていなかったコンテナの、最初の読み書きにあたる
        for before in [
            io_stats(T0, "[]"),
            io_stats(T0, "null"),
            stats_from_parts(T0, None, None, None),
            io_stats(T0, r#"[{"major":8,"minor":0,"op":"total","value":99999}]"#),
        ] {
            let usage = usage_between(before, io_stats(T10, both));
            assert_eq!(usage.io.readkBps, Some(0.2));
            assert_eq!(usage.io.writekBps, Some(0.35));
        }

        // 今回の要素が無い op は、従来どおり null（増分 0 ではない）
        let usage = usage_between(io_stats(T0, both), io_stats(T10, read_only));
        assert_eq!(usage.io.readkBps, Some(0.0));
        assert_eq!(usage.io.writekBps, None);
        assert_eq!(usage.io.writekB, None);
    }

    #[test]
    fn first_io_reaches_the_log_and_the_cache() {
        // 起動直後で IO の無いコンテナが、3 tick 目に初めて書き込む
        let io = |read: &str, blkio: &str| Some(io_stats(read, blkio));
        let wrote = r#"[{"major":8,"minor":0,"op":"write","value":3500}]"#;
        let mut logger = Logger::new();
        assert!(!logger.tick(Ok(collected(vec![("web", io(T0, "null"))]))).unwrap());
        assert!(logger.tick(Ok(collected(vec![("web", io(T10, "null"))]))).unwrap());
        assert!(logger.tick(Ok(collected(vec![
            ("web", io("2026-01-01T00:00:20Z", wrote)),
        ]))).unwrap());
        assert!(logger.tick(Ok(collected(vec![
            ("web", io("2026-01-01T00:00:30Z", wrote)),
        ]))).unwrap());
        // IO が無い間は null、最初の書き込みは 0 からの増分、そのあとは 0
        assert_eq!(logger.writes("web"), vec![
            ("2026-01-01T00:00:10.000Z".to_string(), None),
            ("2026-01-01T00:00:20.000Z".to_string(), Some(0.35)),
            ("2026-01-01T00:00:30.000Z".to_string(), Some(0.0)),
        ]);
        assert!(logger.lines[1].contains(r#""writekBps":0.35"#), "{}", logger.lines[1]);
    }

    #[test]
    fn removed_device_does_not_hide_the_io_of_the_others() {
        // 8:16 は大きな積算値を持つ。次の tick までに要素が消え、8:0 だけが残る
        let usage = usage_between(
            io_stats(T0, concat!(
                r#"[{"major":8,"minor":0,"op":"read","value":1000},"#,
                r#"{"major":8,"minor":0,"op":"write","value":2000},"#,
                r#"{"major":8,"minor":16,"op":"read","value":5000000},"#,
                r#"{"major":8,"minor":16,"op":"write","value":6000000}]"#,
            )),
            io_stats(T10, concat!(
                r#"[{"major":8,"minor":0,"op":"read","value":4500},"#,
                r#"{"major":8,"minor":0,"op":"write","value":3000}]"#,
            )),
        );
        assert_eq!(usage.io.readkBps, Some(0.35));
        assert_eq!(usage.io.writekBps, Some(0.1));
        assert_eq!(usage.io.readkB, Some(4));
    }

    #[test]
    fn io_increase_is_taken_per_device() {
        // 追加されたデバイス（253:0）は 0 から数える
        // major が同じで minor が違うデバイスは、別のものとして扱う
        let usage = usage_between(
            io_stats(T0, concat!(
                r#"[{"major":8,"minor":0,"op":"Read","value":1000},"#,
                r#"{"major":8,"minor":16,"op":"Read","value":700000},"#,
                r#"{"major":8,"minor":0,"op":"Write","value":0}]"#,
            )),
            io_stats(T10, concat!(
                r#"[{"major":8,"minor":0,"op":"read","value":2000},"#,
                // 積算値が戻ったデバイス。このデバイスの差分だけを 0 とする
                r#"{"major":8,"minor":16,"op":"read","value":100},"#,
                r#"{"major":253,"minor":0,"op":"read","value":500},"#,
                r#"{"major":8,"minor":0,"op":"write","value":0},"#,
                r#"{"major":253,"minor":0,"op":"write","value":3500}]"#,
            )),
        );
        // read: 1,000 + 0 + 500、write: 0 + 3,500
        assert_eq!(usage.io.readkBps, Some(0.15));
        assert_eq!(usage.io.writekBps, Some(0.35));

        // 同じデバイスの要素が複数あれば、足してから差を取る
        let stats = io_stats(T0, concat!(
            r#"[{"major":8,"minor":0,"op":"read","value":10},"#,
            r#"{"major":8,"minor":0,"op":"Read","value":5}]"#,
        ));
        assert_eq!(stats.io.read, Some(15));
        assert_eq!(stats.io.read_by_device, vec![("8:0".to_string(), 15)]);
    }

    #[test]
    fn interface_removed_while_stats_were_carried_over() {
        // 採れなかった tick をまたいでも、内訳ごとの前回値で差分を取る
        let net = |read: &str, networks: &str| Some(net_stats(read, networks));
        let mut logger = Logger::new();
        logger.tick(Ok(collected(vec![("web", net(T0, concat!(
            r#"{"eth0":{"rx_bytes":0,"tx_bytes":0},"#,
            r#""eth1":{"rx_bytes":9000000,"tx_bytes":9000000}}"#,
        )))]))).unwrap();
        assert!(!logger.tick(Ok(collected(vec![("web", None)]))).unwrap());
        // 20秒後。eth1 は外されている。eth0 の 7,000 バイトを 20秒で割る
        assert!(logger.tick(Ok(collected(vec![("web", net(
            "2026-01-01T00:00:20Z", r#"{"eth0":{"rx_bytes":7000,"tx_bytes":0}}"#,
        ))]))).unwrap());
        let lock = logger.cache.read().unwrap();
        let point = lock.get("web").unwrap()[0];
        assert_eq!(point.recv_kbps, Some(0.35));
        assert_eq!(point.send_kbps, Some(0.0));
    }
}
