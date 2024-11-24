
use json;

pub enum LogError {
    IOError(std::io::Error),
    JsonError(json::JsonError),
}

pub fn format_log(
    info: &super::ResourceInfo,
) -> String {
    let mut data = json::JsonValue::new_object();
    let mut errors = json::JsonValue::new_array();
    let json_error_message =
        "unexpected error at adding a string to json array";
    data["time"] = format!("{:?}", info.timestamp).into();
    match info.cpu_info {
        Ok(ref cpu) =>
            data["cpu"] = json_cpu(cpu),
        Err(ref e) => errors
            .push(e.to_string())
            .expect(&json_error_message),
    };
    match info.mem_info {
        Ok(ref mem_info) => data["mem"] = json_mem(&mem_info),
        Err(ref e) => errors
            .push(e.to_string())
            .expect(&json_error_message),
    };
    match info.disk_info {
        Ok(ref disk_info) => data["disk"] = json_disk(&disk_info),
        Err(ref e) => errors
            .push(e.to_string())
            .expect(&json_error_message),
    }
    match info.net_info {
        Ok(ref net_info) => data["net"] = json_net(&net_info),
        Err(ref e) => errors
            .push(e.to_string())
            .expect(&json_error_message),
    }

    if errors.len() > 0 {
        data["errors"] = errors;
    }

    json::stringify(data)
}

fn json_cpu(
    cpu: &super::cpu_info::CpuInfo
) -> json::JsonValue {
    let mut data = json::JsonValue::new_object();
    data["usage"] = super::cpu_info::calc_cpu_usage(&cpu).into();

    data
}

fn json_mem(
    mem: &super::mem_info::MemInfo
) -> json::JsonValue {
    let mut data = json::JsonValue::new_object();
    data["usage"] = (
        (mem.total - mem.free) as f32 
        / mem.total as f32
    ).into();
    data["total"] = mem.total.into();
    data["used"] = (mem.total - mem.free).into();
    data["free"] = mem.free.into();
    data["swapUsage"] = (
        (mem.swap_total - mem.swap_free) as f32
        / mem.swap_total as f32
    ).into();
    data["swapTotal"] = mem.swap_total.into();
    data["swapFree"] = mem.swap_free.into();

    data
}

fn json_disk(
    disk: &super::disk_info::DiskInfo
) -> json::JsonValue {
    let mut data = json::JsonValue::new_object();
    data["read"] = (disk.reads_completed / 2).into(); // kB/s
    data["write"] = (disk.writes_completed / 2).into();

    data
}

fn json_net(
    net: &super::net_info::NetInfo
) -> json::JsonValue {
    let mut data = json::JsonValue::new_object();
    data["send"] = (
        (net.tx.bytes * 8) as f32 / (1024 * 1024) as f32
    ).into(); // Mbps
    data["recv"] = (
        (net.rx.bytes * 8) as f32 / (1024 * 1024) as f32
    ).into();

    data
}

pub fn log_daily(
    path: &str,
    info: &super::ResourceInfo
) -> Result<(), LogError> {
    let mut file = std::fs::OpenOptions::new()
        .write(true).create(true).append(true)
        .open(path)
        .map_err(LogError::IOError)?;
    std::io::Write::write(
        &mut file, 
        format_log(&info).as_bytes()
    ).map_err(LogError::IOError)?;

    Ok(())
}

