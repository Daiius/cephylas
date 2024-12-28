use json;

mod error;
mod time;

const DOCKER_API_CONTAINERS: &str = "/containers/json";
const DOCKER_API_STATS: &str = "/containers/{}/stats?stream=false&one-shot=true";

fn call_docker_api<
    T: AsRef<std::path::Path>,
    U: AsRef<str>,
>(
    socket_path: T,
    url: U,
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
        ).map(|(a, b)| a - b);
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

    let mut log_json = json::JsonValue::new_object();
    log_json["time"] = 
        time::format_time(&std::time::SystemTime::now())?
        .into();
    for container_name in container_names {
        // in one-shot mode, pre-stats are not available.
        // we have to take diff by ourselves
        let stats_json = get_container_stats(
            &socket_path, &container_name
        )?;

        log_json["stats"][&container_name] = stats_json;
    }

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
    // CPU calculations
    let cpu_delta =
       stats["cpu"]["total"].as_u64() 
       .zip(prev_stats["cpu"]["total"].as_u64())
       .map(|(a, b)| a - b);
    let system_cpu_delta = 
       stats["cpu"]["system"].as_u64()
       .zip(prev_stats["cpu"]["system"].as_u64())
       .map(|(a, b)| a - b);
    let ncpu = stats["ncpu"].as_u64();
    let cpu_percentage =
        cpu_delta.zip(system_cpu_delta).zip(ncpu)
        .map(|((a, b), c)| (a as f32) / (b as f32) * (c as f32) * 100_f32);

    // Memory calculations
    let memory_percentage =
        stats["memory"]["used"].as_u64()
        .zip(stats["memory"]["available"].as_u64())
        .map(|(a,b)| (a as f32) / (b as f32) * 100_f32);

    // IO calculations
    let io_read_kb_per_s =
        stats["io"]["read"].as_u64()
        .zip(prev_stats["io"]["read"].as_u64())
        .map(|(a,b)| (a - b) / 1000 / (millis / 1000));
    let io_write_kb_per_s =
        stats["io"]["write"].as_u64()
        .zip(prev_stats["io"]["write"].as_u64())
        .map(|(a,b)| (a - b) / 1000 / (millis / 1000));

    // Net calculations
    let net_send_kb_per_s =
        stats["net"]["send"].as_u64()
        .zip(prev_stats["net"]["send"].as_u64())
        .map(|(a,b)| (a - b) / millis);
    let net_recv_kb_per_s =
        stats["net"]["recv"].as_u64()
        .zip(prev_stats["net"]["recv"].as_u64())
        .map(|(a,b)| (a - b) / millis);

    Ok(
        json::object!{
            cpu: {
                percentage: cpu_percentage,
                total: cpu_delta,
                system: system_cpu_delta,
            },
            memory: {
                percentage: memory_percentage,
                used: stats["memory"]["used"].as_u64(),
                available: stats["memory"]["available"].as_u64(),
            },
            io: {
                readkBs: io_read_kb_per_s,
                writekBs: io_write_kb_per_s,
                readkB: stats["io"]["read"].as_u64().map(|x| x / 1000),
                writekB: stats["io"]["write"].as_u64().map(|x| x / 1000),
            },
            net: {
                sendkBps: net_send_kb_per_s,
                recvkBps: net_recv_kb_per_s,
                sendkB: stats["net"]["send"].as_u64().map(|x| x / 1000),
                recvkB: stats["net"]["recv"].as_u64().map(|x| x / 1000),
            },
        }
    )
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

#[allow(unreachable_code)]
fn main() -> Result<(), error::Error> {
    let socket_path = "/var/run/docker.sock";
    let daily_log_path = "./log/log_daily";

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
          tick.as_millis() * 2 
        - now_as_millis % tick.as_millis()
    );
    let mut prev_stats = json::object!{};
    loop {
        // TODO possibly overflow and panic.
        let millis_to_wait = (timing - get_now_as_millis()?) as u64;
        std::thread::sleep(
            std::time::Duration::from_millis(millis_to_wait)
        );

        let stats = get_containers_stats(&socket_path)?;
        if let Ok(usage) = calc_usage(&millis_to_wait, &stats, &prev_stats) {
            println!("{}", &usage);
            log_daily(daily_log_path, &usage.dump())?;
        }

        timing += tick.as_millis();
        prev_stats = stats;
    }

    Ok(())
}

