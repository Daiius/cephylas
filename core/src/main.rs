
mod error;
mod log;
mod server;
mod log_cache;
mod time;

use log_cache::create_shared_cache;

/// スレッドの本体が失敗で終わっていたら、その内容を標準エラー出力に出します
///
/// 出すだけで、プロセスは終了させません（もう一方のスレッドは動き続ける）
/// スレッドの中で、戻る直前に呼びます（joinを待たずに、終わった時点で出すため）
fn report_failure(name: &str, result: Result<(), error::Error>) {
    if let Err(e) = result {
        eprintln!("{}: stopped with an error: {}", name, e);
    }
}

fn main() -> Result<(), error::Error> {

    let log_cache = create_shared_cache();

    log::read_log(&log_cache)?;

    let server_cache = std::sync::Arc::clone(&log_cache);
    let server_handle = std::thread::spawn(move || {
        report_failure("server", server::start_server(&server_cache));
    });

    let logger_cache = std::sync::Arc::clone(&log_cache);
    let logger_handle = std::thread::spawn(move || {
        report_failure("logger", log::log_json(&logger_cache));
    });

    logger_handle.join().expect("failed to join logger_handle");
    server_handle.join().expect("failed to join server_handle");

    Ok(())
}

