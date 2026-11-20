//! 停止の合図（SIGTERM / SIGINT）を受けて、きれいに終了するための部品
//!
//! コンテナの中ではこのバイナリが PID 1 になる。Linux の PID 1 は、ハンドラを
//! 登録していない合図を無視するので、ハンドラが無いと `docker stop` のたびに
//! 猶予を待って SIGKILL される
//!
//! 合図のハンドラでできることは限られる（async-signal-safe な操作だけ）ので、
//! ハンドラは印（SIGNALED）を立てるだけにする。印は main のスレッドが短い間隔で見て、
//! 立っていたら Shutdown::trigger で各スレッドに知らせる

use std::sync::atomic::{ AtomicBool, Ordering };
use std::sync::{ Arc, Condvar, Mutex };
use std::time::Duration;

/// 合図のハンドラが立てる印
static SIGNALED: AtomicBool = AtomicBool::new(false);

// Linux の番号（どのアーキテクチャでも同じ）
const SIGINT: std::os::raw::c_int = 2;
const SIGTERM: std::os::raw::c_int = 15;
/// signal() が失敗したときに返す値（(sighandler_t)-1）
const SIG_ERR: usize = usize::MAX;

extern "C" {
    // libc の signal(2)。std がすでに libc をリンクしているので、依存を足さずに呼べる
    // glibc / musl とも BSD の意味（ハンドラは一度で外れない・SA_RESTART）になる
    fn signal(signum: std::os::raw::c_int, handler: extern "C" fn(std::os::raw::c_int)) -> usize;
}

extern "C" fn on_signal(_: std::os::raw::c_int) {
    // アトミックな書き込みだけ（async-signal-safe）
    SIGNALED.store(true, Ordering::SeqCst);
}

/// SIGTERM と SIGINT のハンドラを登録します
pub fn install_signal_handlers() -> std::io::Result<()> {
    for signum in [SIGTERM, SIGINT] {
        // SAFETY: on_signal はアトミックな書き込みしかしないので、
        // どのスレッドのどの時点で呼ばれても安全
        if unsafe { signal(signum, on_signal) } == SIG_ERR {
            return Err(std::io::Error::last_os_error());
        }
    }
    Ok(())
}

/// 合図を受けたかどうか
pub fn signaled() -> bool {
    SIGNALED.load(Ordering::SeqCst)
}

/// 止まれの印。立ったら、待っているスレッドをすぐに起こします
/// （合図とは切り離してあるので、テストでも使える）
pub struct Shutdown {
    triggered: Mutex<bool>,
    changed: Condvar,
}

impl Shutdown {
    pub fn new() -> Arc<Self> {
        Arc::new(Shutdown { triggered: Mutex::new(false), changed: Condvar::new() })
    }

    /// 印を立て、待っているスレッドを全部起こします
    pub fn trigger(&self) {
        let mut triggered = self.triggered.lock().unwrap_or_else(|e| e.into_inner());
        *triggered = true;
        self.changed.notify_all();
    }

    pub fn is_triggered(&self) -> bool {
        *self.triggered.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// 最長 timeout だけ待ちます。印が立っていれば（待つ間に立っても）すぐに true を返します
    /// sleep の代わりに使う
    pub fn wait_timeout(&self, timeout: Duration) -> bool {
        let triggered = self.triggered.lock().unwrap_or_else(|e| e.into_inner());
        let (triggered, _) = self.changed
            .wait_timeout_while(triggered, timeout, |t| !*t)
            .unwrap_or_else(|e| e.into_inner());
        *triggered
    }

    /// 印が立つまで待ちます
    pub fn wait(&self) {
        let triggered = self.triggered.lock().unwrap_or_else(|e| e.into_inner());
        drop(self.changed
            .wait_while(triggered, |t| !*t)
            .unwrap_or_else(|e| e.into_inner()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    #[test]
    fn wait_timeout_returns_early_when_triggered() {
        let shutdown = Shutdown::new();
        assert!(!shutdown.wait_timeout(Duration::from_millis(10)));

        let waiter = Arc::clone(&shutdown);
        let started = Instant::now();
        let handle = std::thread::spawn(move || waiter.wait_timeout(Duration::from_secs(30)));
        std::thread::sleep(Duration::from_millis(50));
        shutdown.trigger();

        assert!(handle.join().unwrap());
        assert!(started.elapsed() < Duration::from_secs(5));
        // 立ったあとは待たない
        assert!(shutdown.wait_timeout(Duration::from_secs(30)));
    }
}
