use std::thread;

use mem_info::get_mem_info;

mod cpu_info;
mod net_info;
mod mem_info;

enum ApplicationError {
    CpuInfoError(cpu_info::CpuInfoError),
    NetInfoError(net_info::NetInfoError),
    MemInfoError(mem_info::MemInfoError),
}
impl std::fmt::Display for ApplicationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ApplicationError::CpuInfoError(e) => write!(f, "CpuInfoError, {}", e),
            ApplicationError::NetInfoError(e) => write!(f, "NetInfoError, {}", e),
            ApplicationError::MemInfoError(e) => write!(f, "MemInfoError, {}", e),
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

fn run_threads(n: u8) {
    let mut thread_handles: Vec<std::thread::JoinHandle<()>> = vec!();
    for _i in 0..n {
        thread_handles.push(
            thread::spawn(|| {
                let target_time = 
                    std::time::SystemTime::now() 
                    + std::time::Duration::from_secs(5);

                let mut count: u64 = 0;
                println!("Loop started!");
                while std::time::SystemTime::now() < target_time {
                    count += 1;
                }
                println!("Loop count: {}", count);
            })
        );
    }
}

fn main() -> Result<(), ApplicationError> {
    println!("Hello, world!");
    run_threads(4);


    let cpu_info_first = cpu_info::get_cpu_info()?;
    let net_info_first = net_info::get_net_info("eth0")?;
    std::thread::sleep(std::time::Duration::from_secs(1));
    let cpu_info_second = cpu_info::get_cpu_info()?;
    let net_info_second = net_info::get_net_info("eth0")?;

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

    let mem_info = get_mem_info()?;
    println!(
        "mem usage: total {} kB, free {} kB, available {} kB", 
        mem_info.total, 
        mem_info.free, 
        mem_info.available
    );

    Ok(())
}

