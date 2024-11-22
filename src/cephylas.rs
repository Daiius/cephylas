
mod cpu_info;
mod net_info;
mod mem_info;
mod disk_info;

pub enum ApplicationError {
    CpuInfoError(cpu_info::CpuInfoError),
    NetInfoError(net_info::NetInfoError),
    MemInfoError(mem_info::MemInfoError),
    DiskInfoError(disk_info::DiskInfoError),
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

pub fn get_info() -> Result<(), ApplicationError> {
    let cpu_info_first = cpu_info::get_cpu_info()?;
    let net_info_first = net_info::get_net_info("eth0")?;
    let disk_info_first = disk_info::get_disk_info("sdb")?;
    std::thread::sleep(std::time::Duration::from_secs(1));
    let cpu_info_second = cpu_info::get_cpu_info()?;
    let net_info_second = net_info::get_net_info("eth0")?;
    let disk_info_second = disk_info::get_disk_info("sdb")?;

    let diff_cpu = cpu_info_second - cpu_info_first;
    println!(
        "cpu usage: {}%", 
        cpu_info::calc_cpu_usage(&diff_cpu)
    );
    let diff_net = net_info_second - net_info_first;
    println!(
        "net usage: RX {} bytes/s, TX {} bytes/s", 
        diff_net.rx.bytes, 
        diff_net.tx.bytes
    );
    let diff_disk = disk_info_second - disk_info_first;
    println!(
        "disk usage: R/W  {}/{} bytes/s",
        diff_disk.reads_completed * disk_info::SECTOR_SIZE,
        diff_disk.writes_completed * disk_info::SECTOR_SIZE,
    );

    let mem_info = mem_info::get_mem_info()?;
    println!(
        "mem usage: total {} kB, free {} kB, available {} kB", 
        mem_info.total, 
        mem_info.free, 
        mem_info.available
    );

    Ok(())
}

