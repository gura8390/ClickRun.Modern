//! 连点线程只在运行时存在；等待完全交给内核，无轮询、自旋及每次点击分配。
use crate::config::Config;
use std::ptr::null;
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, WAIT_FAILED, WAIT_OBJECT_0};
use windows_sys::Win32::System::Threading::{
    CREATE_WAITABLE_TIMER_HIGH_RESOLUTION, CreateEventW, CreateWaitableTimerExW, INFINITE,
    SetEvent, SetWaitableTimer, TIMER_ALL_ACCESS, WaitForMultipleObjects, WaitForSingleObject,
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, INPUT_0, INPUT_MOUSE, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, MOUSEEVENTF_MIDDLEDOWN,
    MOUSEEVENTF_MIDDLEUP, MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP, MOUSEINPUT, SendInput,
};

pub const INPUT_MARKER: usize = 0x434c4b52;
const RECENT_CAPACITY: usize = 2048;
const RECENT_WINDOW_MS: u64 = 2000;

#[derive(Clone, Debug)]
pub struct Snapshot {
    pub running: bool,
    pub session: u64,
    pub total: u64,
    pub cps: f64,
    pub elapsed: Duration,
    pub error: Option<String>,
}

struct Stats {
    running: bool,
    session: u64,
    total: u64,
    started: Option<Instant>,
    elapsed: Duration,
    error: Option<String>,
    // 精确到毫秒的最近两秒时间戳。最短组间隔 1ms，所以 2048 槽足够；
    // 双/三击组内至少 20ms。容量固定 16KiB，不随运行时长或点击数增长。
    recent: [u64; RECENT_CAPACITY],
    head: usize,
    len: usize,
}

impl Stats {
    fn new(total: u64) -> Self {
        Self {
            running: false,
            session: 0,
            total,
            started: None,
            elapsed: Duration::ZERO,
            error: None,
            recent: [0; RECENT_CAPACITY],
            head: 0,
            len: 0,
        }
    }
    fn trim(&mut self, now: u64) {
        while self.len > 0 && now.saturating_sub(self.recent[self.head]) > RECENT_WINDOW_MS {
            self.head = (self.head + 1) % RECENT_CAPACITY;
            self.len -= 1;
        }
    }
    fn record(&mut self) {
        let now = self
            .started
            .expect("运行中的计时起点")
            .elapsed()
            .as_millis() as u64;
        self.session = self.session.saturating_add(1);
        self.total = self.total.saturating_add(1);
        self.trim(now);
        // 防御性覆盖最老槽；正常 1ms 以上目标节奏不会触及容量上限。
        if self.len == RECENT_CAPACITY {
            self.head = (self.head + 1) % RECENT_CAPACITY;
            self.len -= 1;
        }
        self.recent[(self.head + self.len) % RECENT_CAPACITY] = now;
        self.len += 1;
    }
    fn finish(&mut self, error: Option<String>) {
        self.elapsed = self.started.map(|s| s.elapsed()).unwrap_or_default();
        self.running = false;
        self.head = 0;
        self.len = 0;
        self.error = error;
    }
    fn snapshot(&mut self) -> Snapshot {
        let elapsed = if self.running {
            self.started.map(|s| s.elapsed()).unwrap_or_default()
        } else {
            self.elapsed
        };
        self.trim(elapsed.as_millis() as u64);
        let window = elapsed.as_secs_f64().min(2.0);
        Snapshot {
            running: self.running,
            session: self.session,
            total: self.total,
            cps: if self.running && window >= 0.001 {
                self.len as f64 / window
            } else {
                0.0
            },
            elapsed,
            error: self.error.clone(),
        }
    }
}

fn lock_stats(stats: &Mutex<Stats>) -> MutexGuard<'_, Stats> {
    stats
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

struct OwnedHandle(HANDLE);
// 内核同步对象句柄允许跨线程等待/置位，所有权确保仅在最后一个使用者退出后关闭。
unsafe impl Send for OwnedHandle {}
unsafe impl Sync for OwnedHandle {}
impl Drop for OwnedHandle {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}

struct Worker {
    cancel: Arc<OwnedHandle>,
    join: JoinHandle<()>,
}

pub struct Engine {
    stats: Arc<Mutex<Stats>>,
    worker: Option<Worker>,
    dry_run: bool,
}

impl Engine {
    pub fn new(total: u64, dry_run: bool) -> Self {
        Self {
            stats: Arc::new(Mutex::new(Stats::new(total))),
            worker: None,
            dry_run,
        }
    }

    pub fn start(&mut self, config: &Config) -> Result<(), String> {
        self.start_with_sender(
            config,
            if self.dry_run {
                Sender::DryRun
            } else {
                Sender::Native(NativeSender::new(config.button))
            },
        )
    }

    fn start_with_sender(&mut self, config: &Config, sender: Sender) -> Result<(), String> {
        if lock_stats(&self.stats).running {
            return Ok(());
        }
        // 上一会话自然结束后的线程句柄在再次启动前回收，不叠加工作线程。
        self.stop();
        if !(1..=10000).contains(&config.interval)
            || !(0..=50).contains(&config.random_delay)
            || !(0..=2).contains(&config.mode)
            || !(0..=2).contains(&config.button)
            || config.click_limit < 0
        {
            return Err(
                "点击参数无效：间隔需为 1–10000 毫秒，随机范围需为 0–50%，次数不能为负数。".into(),
            );
        }
        let cancel = Arc::new(OwnedHandle(unsafe { CreateEventW(null(), 1, 0, null()) }));
        if cancel.0.is_null() {
            return Err(last_error("无法创建停止事件"));
        }
        let timer = create_timer()?;
        let settings = Settings {
            interval: config.interval as u64,
            random: config.random_delay as u64,
            burst: config.mode as u64 + 1,
            limit: config.click_limit as u64,
        };
        {
            let mut stats = lock_stats(&self.stats);
            stats.running = true;
            stats.session = 0;
            stats.started = Some(Instant::now());
            stats.elapsed = Duration::ZERO;
            stats.error = None;
            stats.head = 0;
            stats.len = 0;
        }
        let shared = Arc::clone(&self.stats);
        let stop = Arc::clone(&cancel);
        let join = thread::Builder::new()
            .name("连点器输入".into())
            .stack_size(64 * 1024)
            .spawn(move || {
                let error = run_loop(settings, sender, &stop, &timer, &shared).err();
                lock_stats(&shared).finish(error);
            });
        match join {
            Ok(join) => {
                self.worker = Some(Worker { cancel, join });
                Ok(())
            }
            Err(error) => {
                let message = format!("无法启动连点线程：{error}");
                lock_stats(&self.stats).finish(Some(message.clone()));
                Err(message)
            }
        }
    }

    pub fn stop(&mut self) {
        if let Some(worker) = self.worker.take() {
            unsafe {
                SetEvent(worker.cancel.0);
            }
            if worker.join.join().is_err() {
                lock_stats(&self.stats).finish(Some("连点线程意外结束，请重新启动连点器。".into()));
            }
        }
    }

    pub fn snapshot(&self) -> Snapshot {
        lock_stats(&self.stats).snapshot()
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        self.stop();
    }
}

#[derive(Clone, Copy)]
struct Settings {
    interval: u64,
    random: u64,
    burst: u64,
    limit: u64,
}

fn last_error(context: &str) -> String {
    format!("{context}。系统诊断：{}", std::io::Error::last_os_error())
}

fn create_timer() -> Result<OwnedHandle, String> {
    let mut handle = unsafe {
        CreateWaitableTimerExW(
            null(),
            null(),
            CREATE_WAITABLE_TIMER_HIGH_RESOLUTION,
            TIMER_ALL_ACCESS,
        )
    };
    if handle.is_null() {
        // 旧 Windows 不支持高精度标志时仍保留标准定时器路径，不提升全系统时钟频率。
        handle = unsafe { CreateWaitableTimerExW(null(), null(), 0, TIMER_ALL_ACCESS) };
    }
    if handle.is_null() {
        Err(last_error("无法创建连点计时器"))
    } else {
        Ok(OwnedHandle(handle))
    }
}

/// true 表示到期，false 表示停止。没有短周期 Sleep、忙等或反复唤醒。
fn wait(duration: Duration, cancel: &OwnedHandle, timer: &OwnedHandle) -> Result<bool, String> {
    if duration.is_zero() {
        let status = unsafe { WaitForSingleObject(cancel.0, 0) };
        if status == WAIT_FAILED {
            return Err(last_error("无法读取停止事件"));
        }
        return Ok(status != WAIT_OBJECT_0);
    }
    let due = -(duration.as_nanos().div_ceil(100).min(i64::MAX as u128) as i64).max(1);
    if unsafe { SetWaitableTimer(timer.0, &due, 0, None, null(), 0) } == 0 {
        return Err(last_error("无法设置连点等待时间"));
    }
    let handles = [cancel.0, timer.0];
    match unsafe { WaitForMultipleObjects(2, handles.as_ptr(), 0, INFINITE) } {
        WAIT_OBJECT_0 => Ok(false),
        status if status == WAIT_OBJECT_0 + 1 => Ok(true),
        _ => Err(last_error("等待连点计时器失败")),
    }
}

fn random_delay(settings: Settings, rng: &mut u64) -> Duration {
    let range = settings.interval * settings.random / 100;
    // 单线程局部 xorshift；无需线程局部 RNG、堆分配或外部随机库。
    *rng ^= *rng << 13;
    *rng ^= *rng >> 7;
    *rng ^= *rng << 17;
    let delay = settings.interval as i64 + (*rng % (range * 2 + 1)) as i64 - range as i64;
    Duration::from_millis(delay.max(1) as u64)
}

fn run_loop(
    settings: Settings,
    mut sender: Sender,
    cancel: &OwnedHandle,
    timer: &OwnedHandle,
    stats: &Mutex<Stats>,
) -> Result<(), String> {
    let mut rng = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as u64
        | 1;
    let mut failures = 0;
    let mut clicks = 0u64;
    loop {
        let group_start = Instant::now();
        for i in 0..settings.burst {
            if !wait(Duration::ZERO, cancel, timer)? {
                return Ok(());
            }
            if settings.limit > 0 && clicks >= settings.limit {
                return Ok(());
            }
            if sender.send() {
                failures = 0;
                clicks = clicks.saturating_add(1);
                lock_stats(stats).record();
            } else {
                failures += 1;
                if failures >= 5 {
                    return Err("连续 5 次鼠标输入未被系统完整接收，已停止连点。请检查目标程序是否使用更高权限。".into());
                }
            }
            if settings.limit > 0 && clicks >= settings.limit {
                return Ok(());
            }
            if i + 1 < settings.burst && !wait(Duration::from_millis(20), cancel, timer)? {
                return Ok(());
            }
        }
        // 原有目标为每组开始之间的间隔，组内输入/等待耗时从目标间隔扣除。
        let delay = random_delay(settings, &mut rng).saturating_sub(group_start.elapsed());
        if !wait(delay, cancel, timer)? {
            return Ok(());
        }
    }
}

struct NativeSender {
    inputs: [INPUT; 2],
}
impl NativeSender {
    fn new(button: i32) -> Self {
        let (down, up) = match button {
            1 => (MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP),
            2 => (MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP),
            _ => (MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP),
        };
        Self {
            inputs: [down, up].map(|flags| INPUT {
                r#type: INPUT_MOUSE,
                Anonymous: INPUT_0 {
                    mi: MOUSEINPUT {
                        dx: 0,
                        dy: 0,
                        mouseData: 0,
                        dwFlags: flags,
                        time: 0,
                        dwExtraInfo: INPUT_MARKER,
                    },
                },
            }),
        }
    }
    fn send(&self) -> bool {
        let sent = unsafe { SendInput(2, self.inputs.as_ptr(), size_of::<INPUT>() as i32) };
        if sent == 1 {
            // 尽力补发释放，避免部分提交令鼠标停留在按下状态。
            unsafe {
                SendInput(1, self.inputs.as_ptr().add(1), size_of::<INPUT>() as i32);
            }
        }
        sent == 2
    }
}

enum Sender {
    Native(NativeSender),
    DryRun,
    #[cfg(test)]
    Failure,
}
impl Sender {
    fn send(&mut self) -> bool {
        match self {
            Self::Native(native) => native.send(),
            Self::DryRun => true,
            #[cfg(test)]
            Self::Failure => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn finish(engine: &mut Engine) -> Snapshot {
        let deadline = Instant::now() + Duration::from_secs(3);
        while engine.snapshot().running {
            assert!(Instant::now() < deadline, "连点测试超时");
            thread::sleep(Duration::from_millis(2));
        }
        engine.stop();
        engine.snapshot()
    }

    #[test]
    fn triple_seven_counts_individual_clicks_and_keeps_total() {
        let mut engine = Engine::new(90, true);
        let config = Config {
            mode: 2,
            interval: 60,
            click_limit: 7,
            ..Config::default()
        };
        engine.start(&config).unwrap();
        let snapshot = finish(&mut engine);
        assert_eq!((snapshot.session, snapshot.total), (7, 97));
        assert!(snapshot.elapsed >= Duration::from_millis(115));
        assert_eq!(snapshot.cps, 0.0);
        let elapsed = snapshot.elapsed;
        thread::sleep(Duration::from_millis(20));
        assert_eq!(engine.snapshot().elapsed, elapsed);
        engine
            .start(&Config {
                click_limit: 1,
                ..config
            })
            .unwrap();
        let snapshot = finish(&mut engine);
        assert_eq!((snapshot.session, snapshot.total), (1, 98));
    }

    #[test]
    fn stop_interrupts_ten_second_wait_and_does_not_add_clicks() {
        let mut engine = Engine::new(0, true);
        engine
            .start(&Config {
                interval: 10000,
                ..Config::default()
            })
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(1);
        while engine.snapshot().session == 0 {
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(1));
        }
        let stopping = Instant::now();
        engine.stop();
        assert!(stopping.elapsed() < Duration::from_millis(250));
        assert!(!engine.snapshot().running);
        assert_eq!(engine.snapshot().session, 1);
        assert!(engine.worker.is_none());
    }

    #[test]
    fn five_failures_stop_without_incrementing_total() {
        let mut engine = Engine::new(12, true);
        engine
            .start_with_sender(
                &Config {
                    interval: 1,
                    ..Config::default()
                },
                Sender::Failure,
            )
            .unwrap();
        let snapshot = finish(&mut engine);
        assert_eq!((snapshot.session, snapshot.total), (0, 12));
        assert!(snapshot.error.as_deref().unwrap().contains("连续 5 次"));
    }

    #[test]
    fn random_target_stays_within_inclusive_range() {
        let settings = Settings {
            interval: 100,
            random: 50,
            burst: 1,
            limit: 0,
        };
        let mut rng = 1;
        let mut seen = [false; 101];
        for _ in 0..10000 {
            let millis = random_delay(settings, &mut rng).as_millis();
            assert!((50..=150).contains(&millis));
            seen[(millis - 50) as usize] = true;
        }
        assert!(seen.iter().all(|v| *v));
    }

    #[test]
    fn fixed_recent_window_removes_old_entries_and_resets_session() {
        let mut stats = Stats::new(0);
        stats.started = Some(Instant::now());
        stats.running = true;
        stats.record();
        assert_eq!(stats.len, 1);
        stats.trim(2001);
        assert_eq!(stats.len, 0);
        stats.finish(None);
        assert_eq!(stats.snapshot().cps, 0.0);
    }

    #[test]
    fn invalid_config_cannot_start_worker() {
        let mut engine = Engine::new(0, true);
        assert!(
            engine
                .start(&Config {
                    interval: 0,
                    ..Config::default()
                })
                .is_err()
        );
        assert!(!engine.snapshot().running);
        assert!(engine.worker.is_none());
    }

    #[test]
    fn native_packets_contain_button_pair_and_own_input_marker() {
        for (button, down, up) in [
            (0, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP),
            (1, MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP),
            (2, MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP),
        ] {
            let sender = NativeSender::new(button);
            unsafe {
                assert_eq!(sender.inputs[0].Anonymous.mi.dwFlags, down);
                assert_eq!(sender.inputs[1].Anonymous.mi.dwFlags, up);
                assert_eq!(sender.inputs[0].Anonymous.mi.dwExtraInfo, INPUT_MARKER);
                assert_eq!(sender.inputs[1].Anonymous.mi.dwExtraInfo, INPUT_MARKER);
            }
        }
    }
}
