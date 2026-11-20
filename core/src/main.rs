
mod error;
mod log;
mod server;
mod log_cache;
mod shutdown;
mod time;

use std::time::{ Duration, Instant };

use log_cache::create_shared_cache;

/// 停止の合図を受けてから、プロセスを終えるまでの上限
/// docker stop の猶予（既定 10 秒）を過ぎると SIGKILL されるので、それより短くする
/// (server が処理中の接続を待つ上限 5 秒に、余裕を足した値)
const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(8);
/// main のスレッドが、合図とスレッドの終了を確かめる間隔
const POLL_INTERVAL: Duration = Duration::from_millis(100);

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

    // ログの復元より前に登録する（復元の最中の合図も取りこぼさない）
    shutdown::install_signal_handlers()?;
    let stop = shutdown::Shutdown::new();

    let log_cache = create_shared_cache();

    log::read_log(&log_cache)?;

    let server_cache = std::sync::Arc::clone(&log_cache);
    let server_shutdown = std::sync::Arc::clone(&stop);
    let server_handle = std::thread::spawn(move || {
        report_failure("server", server::start_server(&server_cache, &server_shutdown));
    });

    let logger_cache = std::sync::Arc::clone(&log_cache);
    let logger_shutdown = std::sync::Arc::clone(&stop);
    let logger_handle = std::thread::spawn(move || {
        report_failure("logger", log::log_json(&logger_cache, &logger_shutdown));
    });

    // 合図の印は、ハンドラの中では Condvar を起こせないので、ここで見て知らせる
    while !shutdown::signaled() {
        if logger_handle.is_finished() && server_handle.is_finished() {
            logger_handle.join().expect("failed to join logger_handle");
            server_handle.join().expect("failed to join server_handle");
            return Ok(());
        }
        std::thread::sleep(POLL_INTERVAL);
    }

    eprintln!("received a stop signal, shutting down");
    let deadline = Instant::now() + SHUTDOWN_TIMEOUT;
    stop.trigger();
    // logger は tick の途中ならそれを終えて、server は処理中の接続を待って（上限あり）戻る
    while !(logger_handle.is_finished() && server_handle.is_finished())
        && Instant::now() < deadline
    {
        std::thread::sleep(POLL_INTERVAL);
    }
    if !(logger_handle.is_finished() && server_handle.is_finished()) {
        eprintln!("shutdown timed out, exiting anyway");
    }

    // 追記の途中で終えない。ロックを持ったまま戻り、プロセスを終える
    // (logger がまだ動いていても、次の追記はこのロックで止まる)
    let _writing = log::LOG_WRITE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    Ok(())
}

