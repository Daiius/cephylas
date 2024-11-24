
mod cpu_info;
mod net_info;
mod mem_info;
mod disk_info;
mod watch;

pub enum ApplicationError {
    CpuInfoError(cpu_info::CpuInfoError),
    NetInfoError(net_info::NetInfoError),
    MemInfoError(mem_info::MemInfoError),
    DiskInfoError(disk_info::DiskInfoError),
    SyncError,
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
            ApplicationError::SyncError =>
                write!(f, "SyncTimeError"),
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

pub struct ResourceInfo {
    pub cpu_info: Result<cpu_info::CpuInfo, cpu_info::CpuInfoError>,
    pub net_info: Result<net_info::NetInfo, net_info::NetInfoError>,
    pub disk_info: Result<disk_info::DiskInfo, disk_info::DiskInfoError>,
    pub mem_info: Result<mem_info::MemInfo, mem_info::MemInfoError>,
}

fn diff_results<T, E>(a: Result<T, E>, b: Result<T, E>) -> Result<T, E>
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

    //println!(
    //    "disk usage: R/W  {}/{} bytes/s",
    //    diff_disk.reads_completed * disk_info::SECTOR_SIZE,
    //    diff_disk.writes_completed * disk_info::SECTOR_SIZE,
    //);

    //println!(
    //    "mem usage: {}/{} kB, swap: {}/{} kB", 
    //    mem_info.total - mem_info.free, mem_info.total, 
    //    mem_info.swap_total - mem_info.swap_free, mem_info.swap_total
    //);

    ResourceInfo {
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

pub struct Settings {
    pub durations: watch::DurationSettings,
    pub targets: TargetSettings,
}
pub struct TargetSettings {
    pub disk_name: String,
    pub net_dev_name: String,
}


pub fn start_watch(
    net_name: &str,
    disk_name: &str,
) -> Result<(), ApplicationError> {
    loop {
        print!("{}[2J", 27 as char);
        let resource_info = get_info(
            net_name,
            disk_name,
        );
    }
}

