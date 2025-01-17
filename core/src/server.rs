
use super::error;
use super::log;
use super::log_cache;

fn handle_connection(
    mut stream: std::net::TcpStream,
    log_cache: 
        &std::sync::Arc<
            std::sync::RwLock<
                log_cache::LogCache
            >
        >
) -> Result<(), error::Error> {
    let mut buffer = [0; 1024];
    std::io::Read::read(&mut stream, &mut buffer)?;

    println!("Request: {}", String::from_utf8_lossy(&buffer[..]));

    let response = format!(
        "HTTP/1.1 200 OK\r\n\r\n {}", 
        log::custom_dump(&log::reshape_log_cache(log_cache)?)
    );
    std::io::Write::write(&mut stream, response.as_bytes())?;
    std::io::Write::flush(&mut stream)?;

    Ok(())
}

pub fn start_server(
    log_cache:
        &std::sync::Arc<
            std::sync::RwLock<
                log_cache::LogCache
            >
        >
) -> Result<(), error::Error> {
    let listener = std::net::TcpListener::bind("0.0.0.0:7878")?;

    for stream in listener.incoming() {
        let stream = stream?;
        handle_connection(stream, log_cache)?;
    }

    Ok(())
}

