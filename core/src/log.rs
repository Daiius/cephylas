use json;
use super::error;
use super::log_cache;

const DAILY_LOG_PATH: &str = "./log/log_daily";

const DOCKER_API_CONTAINERS: &str = "/containers/json";
const DOCKER_API_STATS: &str = "/containers/{}/stats?stream=false&one-shot=true";

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

pub struct CpuUsage {
    percentage: Option<f32>,
    total: Option<u64>,
    system: Option<u64>,
    ncpu: Option<u8>,
}
fn option_to_string<T>(value: Option<T>) -> String 
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
#[allow(non_snake_case)]
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
#[allow(non_snake_case)]
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
#[allow(non_snake_case)]
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
pub struct Usages {
    time: String,
    millis: u16,
    usages: std::collections::HashMap<String, Usage>,
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
            "{{\"time\":\"{time}\",\"millis\":{millis},{containers_part}}}",
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
    let stats_data = response.lines()
        .find(|l| l.starts_with('{'))
        .ok_or("cannot find response json body")?;
    let stats = json::parse(stats_data)?;
    let stats_json = reshape_json(&stats);

    Ok(stats_json)
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

fn get_containers_stats<T: AsRef<std::path::Path>>(
    socket_path: T,
) -> Result<
    std::collections::HashMap<String, Stats>, 
    error::Error
> {
    let container_names = get_container_names(&socket_path)?;

    let mut stats_map: std::collections::HashMap<String, Stats>
         = std::collections::HashMap::new();
    for container_name in container_names {
        // in one-shot mode, pre-stats are not available.
        // we have to take diff by ourselves
        let stats = get_container_stats(
            &socket_path, &container_name
        )?;

        stats_map.insert(container_name, stats);
    }

    Ok(stats_map)
}

fn get_now_as_millis() -> Result<u128, std::time::SystemTimeError> {
    let duration = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?;
    Ok(duration.as_millis())
}

fn calc_usages(
    millis: &u16,
    stats: &std::collections::HashMap<String, Stats>,
    prev_stats: &std::collections::HashMap<String, Stats>,
) -> Result<Usages, error::Error> {

    let container_names = stats.keys();

    let time = stats.values().next()
        .and_then(|s| s.time.clone())
        .expect("time entry should exist");
    let millis = millis.clone();
    let mut usages = Usages {
        time, millis, 
        usages: std::collections::HashMap::new(),
    };
    for container_name in container_names {
        let stats = &stats[container_name];
        //println!("calc stats: {}", stats);
        let prev_stats = &prev_stats[container_name];
        //println!("calc prev_stats: {}", prev_stats);

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
            );
        //println!("cpu_percentage: {:?}", cpu_percentage);

        // Memory calculations
        let memory_percentage = stats.memory.used
            .zip(stats.memory.available)
            .map(|(a,b)| (a as f32) / (b as f32) * 100_f32);

        // IO calculations
        let io_read_kb_per_s = stats.io.read
            .zip(prev_stats.io.read)
            .map(|(a,b)| a.saturating_sub(b) 
                 / 1000 / (millis as u64 / 1000)
            );
        let io_write_kb_per_s = stats.io.write
            .zip(prev_stats.io.write)
            .map(|(a,b)| a.saturating_sub(b) 
                 / 1000 / (millis as u64 / 1000)
            );

        // Net calculations
        let net_send_kb_per_s = stats.net.send
            .zip(prev_stats.net.send)
            .map(|(a,b)| a.saturating_sub(b) / millis as u64);
        let net_recv_kb_per_s = stats.net.recv
            .zip(prev_stats.net.recv)
            .map(|(a,b)| a.saturating_sub(b) / millis as u64);

        usages.usages.insert(
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
                    readkBps: io_read_kb_per_s.map(|n| n as u32),
                    writekBps: io_write_kb_per_s.map(|n| n as u32),
                    readkB: stats.io.read.map(|x| x / 1000),
                    writekB: stats.io.write.map(|x| x / 1000),
                },
                net: NetUsage {
                    sendkBps: net_send_kb_per_s.map(|n| n as u32),
                    recvkBps: net_recv_kb_per_s.map(|n| n as u32),
                    sendkB: stats.net.send.map(|x| x / 1000),
                    recvkB: stats.net.recv.map(|x| x / 1000),
                },
            }
        );
    }

    Ok(usages)
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

pub fn log_json(
    log_cache: 
        &std::sync::Arc<
            std::sync::RwLock<
                log_cache::LogCache
            >
        >
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

    let mut prev_stats: std::collections::HashMap<String, Stats>
        = std::collections::HashMap::new();
    loop {
        let millis_to_wait = timing.saturating_sub(get_now_as_millis()?) as u64;
        println!("waiting {} millis...", millis_to_wait);

        std::thread::sleep(
            std::time::Duration::from_millis(millis_to_wait)
        );

        let stats = get_containers_stats(&socket_path)?;
        //println!("stats: {}", stats.dump());
        //println!("prev_stats: {}", prev_stats.dump());


        let first_stat = stats.values().next();
        let first_prev_stat = prev_stats.values().next();
            
        let log_condition = first_stat
            .zip(first_prev_stat)
            .map(|(a, b)| !a.cpu.total.is_none() 
                 && !b.cpu.total.is_none()
            )
            .unwrap_or(false);

        println!("log condition: {}", log_condition);

        if log_condition {
            let usage_result = calc_usages(
                &(tick.as_millis() as u16), 
                &stats, &prev_stats
            );
            if let Ok(usage) = usage_result {
                println!("{}", usage);
                log_daily(DAILY_LOG_PATH, usage.to_string())?;
                let mut lock = log_cache.write()
                    .expect("failed to get write lock for log_cache");
                lock.add_and_rotate(usage);
            }
        }

        timing += tick.as_millis();
        prev_stats = stats;
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
                        writekB: v["io"]["sendkB"].as_u64(),
                        readkBps: v["io"]["readkBps"].as_u32(),
                        writekBps: v["io"]["sendkBps"].as_u32(),
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

pub fn read_log(
    log_cache:
        &std::sync::Arc<
            std::sync::RwLock<
                log_cache::LogCache
            >
        >
) -> Result<(), error::Error> {

    let nlines = check_nlines()?;
    let nlines_to_skip = nlines.saturating_sub(log_cache::MAX_LOG_LENGTH as u64);

    let file = std::fs::OpenOptions::new()
        .read(true)
        .open(DAILY_LOG_PATH)?;
    let reader = std::io::BufReader::new(file);
    let mut iline = 0;
    for line in std::io::BufRead::lines(reader) {
        iline += 1;
        if iline < nlines_to_skip { continue; }

        let line = line?;
        match json::parse(&line) {
            Ok(json) => {
                match json_to_usage(&json) {
                    Ok(usages) => {
                        let mut lock = log_cache.write()
                            .expect("cannot lock log_cache"); 
                        lock.add_and_rotate(usages);
                    },
                    Err(e) => {
                        eprintln!("error in log: {}", e);
                    },
                }
            },
            Err(e) => {
                eprintln!("error in log json format: {}", e);
            },
        }
    }

    Ok(())
}

pub fn reshape_log_cache(
    log_cache:
        &std::sync::Arc<
            std::sync::RwLock<
                log_cache::LogCache
            >
        >
) -> Result<json::JsonValue, error::Error> {
    let mut json = json::object!{};
    let lock = log_cache.read().expect("failed to read lock log_cache");
    for usages in lock.data() {
        let container_names = usages.usages.keys();
        for container_name in container_names {
            if json[container_name].is_null() {
                // 初期長さをセット
                json[container_name] = 
                    json::Array::with_capacity(log_cache::MAX_LOG_LENGTH)
                    .into();
            }
            let u = &usages.usages[container_name];
            let stat_json = json::object!{
                time: usages.time.clone(),
                cpu: {
                    percentage: u.cpu.percentage,
                    total: u.cpu.total,
                    system: u.cpu.system,
                    ncpu: u.cpu.ncpu,
                },
                memory: {
                    percentage: u.memory.percentage,
                    used: u.memory.used,
                    available: u.memory.available,
                },
                io: {
                    readkB: u.io.readkB,
                    sendkB: u.io.writekB,
                    readkBps: u.io.readkBps,
                    sendkBps: u.io.writekBps,
                },
                net: {
                    recvkB: u.net.recvkB,
                    sendkB: u.net.sendkB,
                    recvkBps: u.net.recvkBps,
                    sendkBps: u.net.sendkBps,
                }
            };

            json[container_name].push(stat_json)?;
        }
    }

    Ok(json)
}

fn check_nlines() -> Result<u64, error::Error> {
    let file = std::fs::OpenOptions::new()
        .read(true)
        .open(DAILY_LOG_PATH)?;
    let reader = std::io::BufReader::new(file);
    let mut nlines: u64 = 0;
    for _ in std::io::BufRead::lines(reader) {
        nlines += 1;
    }
    Ok(nlines)
}
