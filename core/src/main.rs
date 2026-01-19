
mod error;
mod log;
mod server;
mod log_cache;

use log_cache::create_shared_cache;

#[allow(dead_code)]
const MAX_LOG_LINES: usize = 8640;

#[tokio::main]
async fn main() -> Result<(), error::Error> {

    let log_cache = create_shared_cache();

    log::read_log(&log_cache)?;

    let server_cache = std::sync::Arc::clone(&log_cache);
    let server_task = tokio::spawn(async move {
        server::start_server(server_cache).await
    });

    let logger_cache = std::sync::Arc::clone(&log_cache);
    let logger_task = tokio::spawn(async move {
        log::log_json(logger_cache).await
    });

    let (server_result, logger_result) = tokio::join!(server_task, logger_task);
    server_result.expect("server task panicked")?;
    logger_result.expect("logger task panicked")?;

    Ok(())
}

