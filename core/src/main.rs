
mod error;
mod log;
mod server;
mod log_cache;

use std::sync::Arc;
use log_cache::create_shared_cache;

#[tokio::main]
async fn main() -> Result<(), error::Error> {

    let log_cache = create_shared_cache();

    log::read_log(&log_cache).await?;

    let server_cache = Arc::clone(&log_cache);
    let server_task = tokio::spawn(async move {
        server::start_server(server_cache).await
    });

    let logger_cache = Arc::clone(&log_cache);
    let logger_task = tokio::spawn(async move {
        log::log_loop(logger_cache).await
    });

    tokio::select! {
        r = server_task => eprintln!("Server ended: {:?}", r),
        r = logger_task => eprintln!("Logger ended: {:?}", r),
    }
    Ok(())
}
