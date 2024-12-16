
mod cpu_info;
mod net_info;
mod mem_info;
mod disk_info;
mod watch;
mod log;
pub mod time;

use json;

pub enum ApplicationError {
    CpuInfoError(cpu_info::CpuInfoError),
    NetInfoError(net_info::NetInfoError),
    MemInfoError(mem_info::MemInfoError),
    DiskInfoError(disk_info::DiskInfoError),
    TimeError(time::TimeError),
}
impl std::fmt::Display for ApplicationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ApplicationError::CpuInfoError(e) => 
                write!(f, "CpuInfoError, {}", e),
            ApplicationError::NetInfoError(e) => 
                write!(f, "NetInfoError, {}", e),
            ApplicationError::MemInfoError(e) => 
                write!(f, "MemInfoError, {}", e),
            ApplicationError::DiskInfoError(e) =>
                write!(f, "DiskInfoError, {}", e),
            ApplicationError::TimeError(e) =>
                write!(f, "TimeError, {}", e),
        }
    }
}
impl std::fmt::Debug for ApplicationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_string())
    }
}
impl From<cpu_info::CpuInfoError> for ApplicationError {
    fn from(value: cpu_info::CpuInfoError) -> Self {
        ApplicationError::CpuInfoError(value)
    }
}
impl From<net_info::NetInfoError> for ApplicationError {
    fn from(value: net_info::NetInfoError) -> Self {
        ApplicationError::NetInfoError(value)
    }
}
impl From<mem_info::MemInfoError> for ApplicationError {
    fn from(value: mem_info::MemInfoError) -> Self {
        ApplicationError::MemInfoError(value)
    }
}
impl From<disk_info::DiskInfoError> for ApplicationError {
   fn from(value: disk_info::DiskInfoError) -> Self {
       ApplicationError::DiskInfoError(value)
   }
}
impl From<time::TimeError> for ApplicationError {
    fn from(value: time::TimeError) -> Self {
        ApplicationError::TimeError(value)
    }
}

pub struct ResourceInfo {
    pub timestamp: std::time::SystemTime,
    pub cpu_info: Result<cpu_info::CpuInfo, cpu_info::CpuInfoError>,
    pub net_info: Result<net_info::NetInfo, net_info::NetInfoError>,
    pub disk_info: Result<disk_info::DiskInfo, disk_info::DiskInfoError>,
    pub mem_info: Result<mem_info::MemInfo, mem_info::MemInfoError>,
}

fn diff_results<T, E>(
    a: Result<T, E>,
    b: Result<T, E>
) -> Result<T, E>
    where T: std::ops::Sub<Output = T>
{
    match (a, b) {
        (Ok(a), Ok(b)) => Ok(a - b),
        (Err(c), Ok(_)) => Err(c),
        (Ok(_), Err(b)) => Err(b),
        (Err(_), Err(b)) => Err(b),
    }
}

pub fn get_info(
    net_name: &str,
    disk_name: &str,
) -> ResourceInfo {
    let cpu_info_result_first = cpu_info::get_cpu_info();
    let net_info_result_first = net_info::get_net_info(net_name);
    let disk_info_result_first = disk_info::get_disk_info(disk_name);
    std::thread::sleep(std::time::Duration::from_secs(1));
    let cpu_info_result_second = cpu_info::get_cpu_info();
    let net_info_result_second = net_info::get_net_info(net_name);
    let disk_info_result_second = disk_info::get_disk_info(disk_name);


    ResourceInfo {
        timestamp: std::time::SystemTime::now(),
        cpu_info: 
            diff_results(
                cpu_info_result_second, 
                cpu_info_result_first
            ),
        net_info:
            diff_results(
                net_info_result_second,
                net_info_result_first
            ),
        disk_info:
            diff_results(
                disk_info_result_second,
                disk_info_result_first
            ),
        mem_info:
            mem_info::get_mem_info(),
    }
}

pub use watch::DurationSettings;
pub use log::OutputSettings;

pub struct Settings {
    pub durations: watch::DurationSettings,
    pub targets: TargetSettings,
    pub outputs: log::OutputSettings,
}
pub struct TargetSettings {
    pub disk_name: String,
    pub net_name: String,
}


#[allow(unreachable_code)]
pub fn start_watch(
    net_name: &str,
    disk_name: &str,
) -> Result<(), ApplicationError> {
    loop {
        print!("{}[2J", 27 as char);
        let info = get_info(net_name, disk_name);

        if let Ok(diff_cpu) = info.cpu_info {
            println!(
                "cpu usage: {}%",
                cpu_info::calc_cpu_usage(&diff_cpu)
            );
        }

        if let Ok(diff_disk) = info.disk_info {
            println!(
                "disk usage: R/W  {}/{} kB/s",
                diff_disk.reads_completed / 2,
                diff_disk.writes_completed / 2,
            );
        }

        if let Ok(mem_info) = info.mem_info {
            println!(
                "mem usage: {}/{} MB, swap: {}/{} MB", 
                (mem_info.total - mem_info.free) / 1024, 
                mem_info.total / 1024, 
                (mem_info.swap_total - mem_info.swap_free) / 1024, 
                mem_info.swap_total / 1024
            );
        }

        if let Ok(diff_net) = info.net_info {
            println!(
                "net usage: send/recv {:.3}/{:.3} Mbps",
                (diff_net.rx.bytes * 8) as f32 / (1024*1024) as f32,
                (diff_net.tx.bytes * 8) as f32 / (1024*1024) as f32,
            );
        }

    }

    Ok(())
}

