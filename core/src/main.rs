
mod error;
mod log;
mod server;
mod log_cache;
mod shutdown;
mod time;

use std::time::{ Duration, Instant };

use log_cache::create_shared_cache;

/// 停止の合図に気づいてから、プロセスを終えるまでの上限
/// docker stop の猶予（既定 10 秒）を過ぎると SIGKILL されるので、それより短くする
/// (server が処理中の接続を待つ上限 5 秒に、余裕を足した値)
/// 合図に気づくまでの POLL_INTERVAL と、ポーリングの刻みを足しても 8.2 秒ほどで、9 秒に収まる
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
    let mut logger_handle = Some(std::thread::spawn(move || {
        report_failure("logger", log::log_json(&logger_cache, &logger_shutdown));
    }));

    // 合図の印は、ハンドラの中では Condvar を起こせないので、ここで見て知らせる
    // join の順は以前（logger を join してから server を join）と同じにする:
    // logger が panic したら、ここで panic してプロセスを終える
    while !shutdown::signaled() {
        if logger_handle.as_ref().is_some_and(|h| h.is_finished()) {
            if let Some(handle) = logger_handle.take() {
                handle.join().expect("failed to join logger_handle");
            }
        }
        if logger_handle.is_none() && server_handle.is_finished() {
            server_handle.join().expect("failed to join server_handle");
            return Ok(());
        }
        std::thread::sleep(POLL_INTERVAL);
    }

    let deadline = Instant::now() + SHUTDOWN_TIMEOUT;
    stop.trigger();
    // 出力は別のスレッドで行う（標準エラー出力が詰まっても、main の待ちと終了を止めない）
    // スレッドを作れなくても（スレッド数・メモリの枯渇）、通知を省いて待ちへ進む（`thread::spawn` は失敗で panic する。OCL-76D9099D）
    let _ = std::thread::Builder::new()
        .spawn(|| eprintln!("received a stop signal, shutting down"));
    // logger は tick の途中ならそれを終えて、server は処理中の接続を待って（上限あり）戻る
    let all_finished = || {
        logger_handle.as_ref().is_none_or(|h| h.is_finished()) && server_handle.is_finished()
    };
    while !all_finished() && Instant::now() < deadline {
        std::thread::sleep(POLL_INTERVAL);
    }
    // 上限に達しても、ここでは何も出力しない（出力が詰まると終了が止まる）
    // そのとき logger が追記の途中なら行が途中で切れうるが、次の追記は新しい行から始まる（log_daily）
    Ok(())
}

