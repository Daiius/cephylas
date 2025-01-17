
mod error;
mod log;
mod server;
mod log_cache;

const MAX_LOG_LINES: usize = 8640;

fn main() -> Result<(), error::Error> {

    let log_cache = 
        std::sync::Arc::new(
        std::sync::RwLock::new(
            log_cache::LogCache::new()
        ));

    log::read_log(&log_cache)?;

    let server_cache = std::sync::Arc::clone(&log_cache);
    let server_handle = std::thread::spawn(move || server::start_server(&server_cache));

    let logger_cache = std::sync::Arc::clone(&log_cache);
    let logger_handle = std::thread::spawn(move || log::log_json(&logger_cache));

    logger_handle.join().expect("failed to join logger_handle");
    server_handle.join().expect("failed to join server_handle");

    Ok(())
}

