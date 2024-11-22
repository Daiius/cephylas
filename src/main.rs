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

    loop {
        print!("{}[2J", 27 as char);
        cephylas::get_info()?;
    }

    Ok(())
}

