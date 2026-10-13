
use std::collections::HashMap;

use json;
use super::error;
use super::log_cache;
use super::time;

const DAILY_LOG_PATH: &str = "./log/log_daily";

const DOCKER_API_CONTAINERS: &str = "/containers/json";
const DOCKER_API_STATS: &str = "/containers/{}/stats?stream=false&one-shot=true";

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
    total: Option<u64>,
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
    read: Option<u64>,
    write: Option<u64>,
}
#[derive(Debug)]
pub struct NetStats {
    send: Option<u64>,
    recv: Option<u64>,
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
            io: IoStats { read: None, write: None },
            net: NetStats { recv: None, send: None },
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
    readkBps: Option<u32>,
    writekBps: Option<u32>,
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
    recvkBps: Option<u32>,
    sendkBps: Option<u32>,
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

fn call_docker_api<
    P: AsRef<std::path::Path>,
    S: AsRef<str>,
>(
    socket_path: P,
    url: S,
) -> Result<String, std::io::Error> {
    let mut stream = 
        std::os::unix::net::UnixStream::connect(socket_path)?;
    let request = format!(
        "GET {} HTTP/1.1\r\n\
         Host: localhost\r\n\
         Connection: close\r\n\
         \r\n",
        url.as_ref()
    );

    std::io::Write::write_all(&mut stream, request.as_bytes())?;

    let mut response = String::new();
    std::io::Read::read_to_string(&mut stream, &mut response)?;

    Ok(response)
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

fn reshape_json(
    json: &json::JsonValue,
) -> Stats {
    let total = 
        json["cpu_stats"]["cpu_usage"]["total_usage"]
        .as_u64()
        .map(|v| v / 1_000_000); // ns -> ms
    let system =
        json["cpu_stats"]["system_cpu_usage"]
        .as_u64()
        .map(|v| v / 1_000_000); // ns -> ms
    let number_cpus =
        json["cpu_stats"]["online_cpus"]
        .as_u16()
        .map(|n| n as u8);
    let used_memory =
        json["memory_stats"]["usage"].as_u64()
        .zip(
            json["memory_stats"]["stats"]["cache"]
                .as_u64()
                .or(Some(0_u64)) // cache entry might not exist
        ).map(|(a, b)| a.saturating_sub(b));
    let available_memory =
        json["memory_stats"]["limit"]
        .as_u64();
    let net_rx =
        json["networks"]["eth0"]["rx_bytes"]
        .as_u64();
    let net_tx =
        json["networks"]["eth0"]["tx_bytes"]
        .as_u64();
    let blkio_read =
        json["blkio_stats"]["io_service_bytes_recursive"]
        .members()
        .find(|ref m| m["op"] == "read")
        .and_then(|v| v["value"].as_u64());
    let blkio_write =
        json["blkio_stats"]["io_service_bytes_recursive"]
        .members()
        .find(|ref m| m["op"] == "write")
        .and_then(|v| v["value"].as_u64());
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
            read: blkio_read,
            write: blkio_write, 
        },
        net: NetStats {
            send: net_tx,
            recv: net_rx,
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

/// 積算値（バイト）の差分を、実測の経過時間で割って kB/s にします
/// bytes / ms = kB / s
fn rate_kb_per_s(
    current: Option<u64>,
    previous: Option<u64>,
    elapsed_millis: u64,
) -> Option<u32> {
    current.zip(previous).map(|(a, b)|
        u32::try_from(a.saturating_sub(b) / elapsed_millis)
            .unwrap_or(u32::MAX)
    )
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
                 (a as f32) / (b as f32) * (c as f32) * 100_f32
            )
            // 分母が0だと NaN / inf になり、jsonとして書けない
            .filter(|p| p.is_finite());

        // Memory calculations
        let memory_percentage = stats.memory.used
            .zip(stats.memory.available)
            .map(|(a,b)| (a as f32) / (b as f32) * 100_f32)
            .filter(|p| p.is_finite());

        // IO calculations
        let io_read_kb_per_s = rate_kb_per_s(
            stats.io.read, prev_stats.io.read, elapsed_millis,
        );
        let io_write_kb_per_s = rate_kb_per_s(
            stats.io.write, prev_stats.io.write, elapsed_millis,
        );

        // Net calculations
        let net_send_kb_per_s = rate_kb_per_s(
            stats.net.send, prev_stats.net.send, elapsed_millis,
        );
        let net_recv_kb_per_s = rate_kb_per_s(
            stats.net.recv, prev_stats.net.recv, elapsed_millis,
        );

        usages.insert(
            container_name.to_string(),
            Usage {
                cpu: CpuUsage {
                    percentage: cpu_percentage,
                    total: cpu_delta,
                    system: system_cpu_delta,
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

    let now_as_millis = get_now_as_millis()?;
    let tick = std::time::Duration::from_secs(10);
    let mut timing = now_as_millis + (
          tick.as_millis() //* 2  // NOTE if tick is short, maybe more wait needed.
        - now_as_millis % tick.as_millis()
    );

    let mut state = LoggerState::new();
    // 経過時間を core 自身で測るための単調時計の起点
    let started = std::time::Instant::now();
    // ここから先の失敗では止まらない。その tick を飛ばして次へ進む
    loop {
        let wait = match get_now_as_millis() {
            Ok(now) => std::time::Duration::from_millis(
                timing.saturating_sub(now) as u64
            ),
            Err(e) => {
                eprintln!("logger: failed to get the current time: {}", e);
                tick
            },
        };
        println!("waiting {} millis...", wait.as_millis());
        std::thread::sleep(wait);

        // 待ったあとの現在時刻。取れなければ、この tick では時間による削除をしない
        let now_millis = get_now_as_millis().ok()
            .and_then(|now| i64::try_from(now).ok());
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
        timing += tick.as_millis();
    }
}

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
                        readkBps: v["io"]["readkBps"].as_u32(),
                        writekBps: v["io"]["writekBps"].as_u32(),
                    },
                    net: NetUsage {
                        recvkB: v["net"]["recvkB"].as_u64(),
                        sendkB: v["net"]["sendkB"].as_u64(),
                        recvkBps: v["net"]["recvkBps"].as_u32(),
                        sendkBps: v["net"]["sendkBps"].as_u32(),
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

    /// Docker の stats API が返す応答（ヘッダ込み）を組み立てる
    /// 値はすべて積算値。cpu と system は ns、ほかはバイト
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
                r#"{{"major":8,"minor":0,"op":"write","value":{io_write}}},"#,
                r#"{{"major":8,"minor":16,"op":"read","value":999999999}}]}},"#,
                r#""cpu_stats":{{"cpu_usage":{{"total_usage":{cpu}}},"#,
                r#""system_cpu_usage":{system},"online_cpus":2}},"#,
                r#""memory_stats":{{"usage":300000,"stats":{{"cache":100000}},"limit":1000000}},"#,
                r#""networks":{{"eth0":{{"rx_bytes":{rx},"tx_bytes":{tx}}},"#,
                r#""eth1":{{"rx_bytes":777777777,"tx_bytes":777777777}}}}}}"#,
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
        // ns -> ms
        assert_eq!(stats.cpu.total, Some(5000));
        assert_eq!(stats.cpu.system, Some(70_000));
        assert_eq!(stats.cpu.ncpu, Some(2));
        // usage - stats.cache
        assert_eq!(stats.memory.used, Some(200_000));
        assert_eq!(stats.memory.available, Some(1_000_000));
        // 最初の read / write、eth0 だけ
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
        fn writes(&self, name: &str) -> Vec<(String, Option<u32>)> {
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
            ("2026-01-01T00:00:10.000Z".to_string(), Some(100)),
            ("2026-01-01T00:00:20.000Z".to_string(), Some(100)),
            ("2026-01-01T00:00:30.000Z".to_string(), Some(100)),
        ]);
        // 欠けるのは1点だけ。戻った点は 20秒で割った 300 kB/s（10秒で割った 600 ではない）
        assert_eq!(logger.writes("db"), vec![
            ("2026-01-01T00:00:10.000Z".to_string(), Some(100)),
            ("2026-01-01T00:00:30.000Z".to_string(), Some(300)),
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
                ("2026-01-01T00:00:20.000Z".to_string(), Some(200)),
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
            ("2026-01-01T01:00:10.000Z".to_string(), Some(100)),
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
            ("2026-01-01T00:00:10.000Z".to_string(), Some(100)),
            ("2026-01-01T00:00:40.000Z".to_string(), Some(100)),
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
            &("2026-01-01T00:00:55.000Z".to_string(), Some(160)),
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
            ("2026-01-01T00:00:10.000Z".to_string(), Some(100)),
            ("2026-01-01T00:00:20.000Z".to_string(), Some(200)),
        ]);

    }

    /// web だけの tick。write の積算値だけが増える
    fn web_tick(read: &str, bytes: u64) -> Result<Collected, error::Error> {
        Ok(collected(vec![("web", written(read, bytes))]))
    }

    /// ログに書かれた行の (time, web の writekBps)
    fn logged(logger: &Logger) -> Vec<(String, Option<u32>)> {
        logger.lines.iter().map(|line| {
            let json = json::parse(line).unwrap();
            (
                json["time"].as_str().unwrap().to_string(),
                json["stats"]["web"]["io"]["writekBps"].as_u32(),
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
            ("2026-01-01T00:00:10.000Z".to_string(), Some(100)),
            ("2026-01-01T00:00:30.000Z".to_string(), Some(200)),
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
            ("2026-01-01T01:00:10Z".to_string(), Some(100)),
            ("2026-01-01T00:00:40Z".to_string(), Some(100)),
            ("2026-01-01T00:00:50Z".to_string(), Some(200)),
        ]);
        // 増分が落ちていない: 速度 × 経過時間 の合計が、積算値の増分（7,500,000 バイト）に一致する
        let elapsed_seconds = [10, 45, 10];
        let total_kb: u32 = points.iter().zip(elapsed_seconds)
            .map(|((_, rate), seconds)| rate.unwrap() * seconds)
            .sum();
        assert_eq!(total_kb, 7_500);

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
            ("2026-01-01T00:00:10.000Z".to_string(), Some(100)),
            ("2026-01-01T00:00:40.000Z".to_string(), Some(100)),
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
        assert_eq!(ten.io.readkBps, Some(200));
        assert_eq!(ten.io.writekBps, Some(400));
        assert_eq!(ten.net.recvkBps, Some(600));
        assert_eq!(ten.net.sendkBps, Some(800));

        let twenty = &usages.usages["twenty"];
        assert_eq!(twenty.io.readkBps, Some(100));
        assert_eq!(twenty.io.writekBps, Some(200));
        assert_eq!(twenty.net.recvkBps, Some(300));
        assert_eq!(twenty.net.sendkBps, Some(400));

        assert_eq!(usages.usages["uneven"].io.readkBps, Some(160));

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
            assert_eq!(usage.cpu.total, Some(1000));
            assert_eq!(usage.cpu.system, Some(10_000));
            // (300000 - 100000) / 1000000 × 100
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
                r#""io":{"readkB":4161,"writekB":48689,"readkBps":416,"writekBps":4868},"#,
                r#""net":{"recvkB":40004,"sendkB":14,"recvkBps":4000,"sendkBps":1}}}}"#,
            ),
        );
    }

    #[test]
    fn written_line_is_restored_with_io_write() {
        let line = concat!(
            r#"{"time":"2026-01-01T00:00:10.023034093Z","millis":10000,"stats":{"#,
            r#""web":{"cpu":{"percentage":0.03,"total":3,"system":19860,"ncpu":2},"#,
            r#""memory":{"percentage":3.64,"used":36696064,"available":1007222784},"#,
            r#""io":{"readkB":4161,"writekB":48689,"readkBps":11,"writekBps":22},"#,
            r#""net":{"recvkB":4,"sendkB":9,"recvkBps":33,"sendkBps":44}}}}"#,
        );
        let usages = json_to_usage(&json::parse(line).unwrap()).unwrap();
        let web = &usages.usages["web"];
        assert_eq!(web.io.readkB, Some(4161));
        assert_eq!(web.io.writekB, Some(48689));
        assert_eq!(web.io.readkBps, Some(11));
        assert_eq!(web.io.writekBps, Some(22));
        assert_eq!(web.net.recvkBps, Some(33));
        assert_eq!(web.net.sendkBps, Some(44));
        // 読んだものを書き出すと元の行に戻る
        assert_eq!(usages.to_string(), line);
    }

    /// ログの1行（行末の \r\n 込み）。値は cpu と io write だけ変える
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
            vec![Some(11), Some(13), Some(15)],
        );
        assert_eq!(web[2].cpu, Some(5.0));
        assert_eq!(web[2].memory, Some(3.64));
        assert_eq!(web[2].read_kbps, Some(5));
        assert_eq!(web[2].recv_kbps, Some(6));
        assert_eq!(web[2].send_kbps, Some(7));
        assert_eq!(lock.get("db").unwrap()[0].write_kbps, Some(16));
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
        assert_eq!(web.back().unwrap().write_kbps, Some(8999));
        assert_eq!(web.front().unwrap().write_kbps, Some(360));
        // 復元した系列は、write を基準にしたダウンサンプリングでも末尾まで返る
        let sampled = lock
            .downsample("web", &DownsampleOption::default(), Series::IoWrite)
            .unwrap();
        assert_eq!(sampled.len(), 512);
        assert_eq!(sampled[511].write_kbps, Some(8999));
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
            assert_eq!(web[0].write_kbps, Some(100));
            assert_eq!(web[1].write_kbps, Some(100));
        }
    }
}
