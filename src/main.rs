use std::thread;

mod cephylas;

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

fn main() -> Result<(), cephylas::ApplicationError> {
    println!("Hello, world!");
    //run_threads(4);
 
    let net_name = std::env::var("NET_NAME")
        .unwrap_or_else(|_| {
            println!("environment variable NET_NAME not found.");
            "eth0".to_string()
        });
    let disk_name = std::env::var("DISK_NAME")
        .unwrap_or_else(|_| {
            println!("environment variable DISK_NAME not found.");
            "sdb".to_string()
        });
    let host_proc = std::env::var("HOST_PROC")
        .unwrap_or_else(|_| {
            println!("environment variable HOST_PROC not found.");
            "/proc".to_string()
        });

    loop {
        print!("{}[2J", 27 as char);
        cephylas::get_info(
            &net_name,
            &disk_name,
            &host_proc,
        )?;
    }

    Ok(())
}

