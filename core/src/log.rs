use json;
use super::error;
use super::log_cache;

const DAILY_LOG_PATH: &str = "./log/log_daily";

const DOCKER_API_CONTAINERS: &str = "/containers/json";
const DOCKER_API_STATS: &str = "/containers/{}/stats?stream=false&one-shot=true";


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
) -> Result<json::JsonValue, error::Error> {
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
) -> json::JsonValue {
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
        .as_u32();
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


    json::object!{
        time: time, 
        cpu: { 
            total: total, 
            system: system, 
            ncpu: number_cpus
        },
        memory: {
            used: used_memory, 
            available: available_memory
        },
        io: {
            read: blkio_read,
            write: blkio_write, 
        },
        net: {
            send: net_tx,
            recv: net_rx,
        }
    }
}

fn get_containers_stats<T: AsRef<std::path::Path>>(
    socket_path: T,
) -> Result<json::JsonValue, error::Error> {
    let container_names = get_container_names(&socket_path)?;

    let mut log_json_tmp = json::JsonValue::new_object();
    for container_name in container_names {
        // in one-shot mode, pre-stats are not available.
        // we have to take diff by ourselves
        let stats_json = get_container_stats(
            &socket_path, &container_name
        )?;

        log_json_tmp[&container_name] = stats_json;
    }
    let log_json = json::object!{
        time: log_json_tmp.entries()
            .take(1)
            .map(|(_, v)| v["time"].as_str())
            .next(),
        stats: log_json_tmp,
    };

    Ok(log_json)
}

fn get_now_as_millis() -> Result<u128, std::time::SystemTimeError> {
    let duration = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?;
    Ok(duration.as_millis())
}

fn calc_usage(
    millis: &u64,
    stats: &json::JsonValue,
    prev_stats: &json::JsonValue,
) -> Result<json::JsonValue, error::Error> {
    let container_names = stats["stats"].entries()
        .map(|(key, _)| key);

    let mut usages = json::JsonValue::new_object();
    usages["time"] = stats["time"].as_str().unwrap_or("null").into();
    usages["millis"] = (*millis).into();
    for container_name in container_names {
        let stats = &stats["stats"][container_name];
        //println!("calc stats: {}", stats);
        let prev_stats = &prev_stats["stats"][container_name];
        //println!("calc prev_stats: {}", prev_stats);

        // CPU calculations
        let cpu_delta =
           stats["cpu"]["total"].as_u64() 
           .zip(prev_stats["cpu"]["total"].as_u64())
           .map(|(a, b)| a.saturating_sub(b));
        let system_cpu_delta = 
           stats["cpu"]["system"].as_u64()
           .zip(prev_stats["cpu"]["system"].as_u64())
           .map(|(a, b)| a.saturating_sub(b));
        let ncpu = 
            stats["cpu"]["ncpu"].as_u64();
        let cpu_percentage =
            cpu_delta.zip(system_cpu_delta).zip(ncpu)
            .map(|((a, b), c)| (a as f32) / (b as f32) * (c as f32) * 100_f32);
        //println!("cpu_percentage: {:?}", cpu_percentage);

        // Memory calculations
        let memory_percentage =
            stats["memory"]["used"].as_u64()
            .zip(stats["memory"]["available"].as_u64())
            .map(|(a,b)| (a as f32) / (b as f32) * 100_f32);

        // IO calculations
        let io_read_kb_per_s =
            stats["io"]["read"].as_u64()
            .zip(prev_stats["io"]["read"].as_u64())
            .map(|(a,b)| a.saturating_sub(b) / 1000 / (millis / 1000));
        let io_write_kb_per_s =
            stats["io"]["write"].as_u64()
            .zip(prev_stats["io"]["write"].as_u64())
            .map(|(a,b)| a.saturating_sub(b) / 1000 / (millis / 1000));

        // Net calculations
        let net_send_kb_per_s =
            stats["net"]["send"].as_u64()
            .zip(prev_stats["net"]["send"].as_u64())
            .map(|(a,b)| a.saturating_sub(b) / millis);
        let net_recv_kb_per_s =
            stats["net"]["recv"].as_u64()
            .zip(prev_stats["net"]["recv"].as_u64())
            .map(|(a,b)| a.saturating_sub(b) / millis);

        usages["stats"][container_name] = json::object!{
            cpu: {
                percentage: cpu_percentage,
                total: cpu_delta,
                system: system_cpu_delta,
                ncpu: ncpu,
            },
            memory: {
                percentage: memory_percentage,
                used: stats["memory"]["used"].as_u64(),
                available: stats["memory"]["available"].as_u64(),
            },
            io: {
                readkBps: io_read_kb_per_s,
                writekBps: io_write_kb_per_s,
                readkB: stats["io"]["read"].as_u64().map(|x| x / 1000),
                writekB: stats["io"]["write"].as_u64().map(|x| x / 1000),
            },
            net: {
                sendkBps: net_send_kb_per_s,
                recvkBps: net_recv_kb_per_s,
                sendkB: stats["net"]["send"].as_u64().map(|x| x / 1000),
                recvkB: stats["net"]["recv"].as_u64().map(|x| x / 1000),
            },
        };
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

    let mut prev_stats = json::object!{};
    loop {
        let millis_to_wait = timing.saturating_sub(get_now_as_millis()?) as u64;
        println!("waiting {} millis...", millis_to_wait);

        std::thread::sleep(
            std::time::Duration::from_millis(millis_to_wait)
        );

        let stats = get_containers_stats(&socket_path)?;
        //println!("stats: {}", stats.dump());
        //println!("prev_stats: {}", prev_stats.dump());


        let first_stat = stats["stats"].entries()
            .take(1).map(|(_, v)| v).next();
        let first_prev_stat = prev_stats["stats"].entries()
            .take(1).map(|(_, v)| v).next();
            
        let log_condition = first_stat
            .zip(first_prev_stat)
            .map(|(a, b)| !a["cpu"].is_null() && !b["cpu"].is_null() )
            .unwrap_or(false);

        println!("log condition: {}", log_condition);

        if log_condition {
            let usage_result = calc_usage(
                &(tick.as_millis() as u64), 
                &stats, &prev_stats
            );
            if let Ok(usage) = usage_result {
                let log_content = custom_dump(&usage);
                println!("{}", log_content);
                log_daily(DAILY_LOG_PATH, log_content)?;
                let mut lock = log_cache.write()
                    .expect("failed to get write lock for log_cache");
                lock.add_and_rotate(usage);
            }
        }

        timing += tick.as_millis();
        prev_stats = stats;
    }
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
        let log_line = json::parse(&line)?;

        let mut lock = log_cache.write()
            .expect("cannot lock log_cache"); 
        lock.add_and_rotate(log_line);
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
    for log_line in lock.data() {
        let container_names = log_line["stats"]
            .entries()
            .map(|(k, _v)| k);
        for container_name in container_names {
            if json[container_name].is_null() {
                json[container_name] = json::array!{};
            }
            let mut stat_json = json::object!{
                time: log_line["time"].as_str(),
            };
            log_line["stats"][container_name].entries()
                .for_each(|(k, v)| stat_json.insert(
                    k, json::parse(&custom_dump(v)).expect("")
                ).expect(""));
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
