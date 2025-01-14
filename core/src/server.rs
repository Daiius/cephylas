
use super::error;
use super::log;

fn handle_connection(
    mut stream: std::net::TcpStream
) -> Result<(), error::Error> {
    let mut buffer = [0; 1024];
    std::io::Read::read(&mut stream, &mut buffer)?;

    println!("Request: {}", String::from_utf8_lossy(&buffer[..]));

    let log_data = log::read_log()?;
    let response = format!(
        "HTTP/1.1 200 OK\r\n\r\n {}", 
        log::custom_dump(&log_data)
    );
    std::io::Write::write(&mut stream, response.as_bytes())?;
    std::io::Write::flush(&mut stream)?;

    Ok(())
}

pub fn start_server() -> Result<(), error::Error> {
    let listener = std::net::TcpListener::bind("127.0.0.1:7878")?;

    for stream in listener.incoming() {
        let stream = stream?;
        handle_connection(stream)?;
    }

    Ok(())
}

