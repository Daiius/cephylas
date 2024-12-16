use std::thread;

mod cephylas;


fn main() -> Result<(), cephylas::ApplicationError> {
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

    let settings = cephylas::Settings {
        targets: cephylas::TargetSettings { disk_name, net_name, },
        durations: cephylas::DurationSettings::default(),
    };
    cephylas::start_watch(&net_name, &disk_name)?;


    Ok(())
}

