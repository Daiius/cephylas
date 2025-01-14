
mod error;
mod log;
mod server;


#[allow(unreachable_code)]
fn main() -> Result<(), error::Error> {

    let http_handle = std::thread::spawn(server::start_server);

    let logger_handle = std::thread::spawn(log::log_json);

    logger_handle.join().expect("failed to join logger_handle");
    http_handle.join().expect("failed to join server_handle");

    Ok(())
}

