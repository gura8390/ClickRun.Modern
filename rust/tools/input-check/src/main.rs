//! 原生 SendInput 集成验收。钩子先于输入存在，专门吞掉本程序标记事件。
//! 不创建窗口，不操作前台，不设置指针坐标，真实用户事件全部传递给下一钩子。
#![allow(dead_code)]
#[path = "../../../src/config.rs"]
mod config;
#[path = "../../../src/engine.rs"]
mod engine;

use serde_json::{Value, json};
use std::{mem::{size_of, zeroed}, path::PathBuf, ptr::{null, null_mut},
    sync::atomic::{AtomicU32, AtomicU64, AtomicUsize, Ordering}, thread, time::{Duration, Instant, SystemTime, UNIX_EPOCH}};
use windows_sys::Win32::{Foundation::*, System::LibraryLoader::GetModuleHandleW,
    UI::{Input::KeyboardAndMouse::*, WindowsAndMessaging::*}};

const MAX_EVENTS: usize = 512;
static EVENT_COUNT: AtomicUsize = AtomicUsize::new(0);
static EVENT_MESSAGES: [AtomicU32; MAX_EVENTS] = [const { AtomicU32::new(0) }; MAX_EVENTS];
static EVENT_FLAGS: [AtomicU32; MAX_EVENTS] = [const { AtomicU32::new(0) }; MAX_EVENTS];
static EVENT_MOUSE_DATA: [AtomicU32; MAX_EVENTS] = [const { AtomicU32::new(0) }; MAX_EVENTS];
static UNMARKED_PASSED: AtomicU64 = AtomicU64::new(0);

fn desktop_diagnostics() -> (Value, Option<String>) {
    type Object = *mut core::ffi::c_void;
    #[link(name = "user32")]
    unsafe extern "system" {
        fn GetThreadDesktop(thread: u32) -> Object;
        fn GetProcessWindowStation() -> Object;
        fn OpenInputDesktop(flags: u32, inherit: i32, access: u32) -> Object;
        fn CloseDesktop(desktop: Object) -> i32;
        fn GetUserObjectInformationW(object: Object, index: i32, buffer: *mut core::ffi::c_void, bytes: u32, needed: *mut u32) -> i32;
    }
    fn object_name(object: Object) -> Result<String, String> {
        if object.is_null() { return Err(std::io::Error::last_os_error().to_string()); }
        let mut buffer = [0u16; 1024]; let mut needed = 0;
        if unsafe { GetUserObjectInformationW(object, 2, buffer.as_mut_ptr().cast(), (buffer.len()*2) as u32, &mut needed) } == 0 {
            return Err(std::io::Error::last_os_error().to_string());
        }
        let length = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
        Ok(String::from_utf16_lossy(&buffer[..length]))
    }
    let thread_id = unsafe { windows_sys::Win32::System::Threading::GetCurrentThreadId() };
    let thread_name = object_name(unsafe { GetThreadDesktop(thread_id) });
    let station_name = object_name(unsafe { GetProcessWindowStation() });
    let input = unsafe { OpenInputDesktop(0, 0, 1) };
    let input_name = object_name(input);
    if !input.is_null() { unsafe { CloseDesktop(input); } }
    let matching = thread_name.as_ref().ok().zip(input_name.as_ref().ok()).is_some_and(|(a,b)| a == b);
    let foreground = unsafe { GetForegroundWindow() };
    let diagnostic = json!({"ThreadDesktop": thread_name, "ProcessWindowStation": station_name,
        "InputDesktop": input_name, "InputAndThreadDesktopNamesMatch": matching,
        "ForegroundWindowPresent": !foreground.is_null()});
    let failure = if !matching || foreground.is_null() { Some("当前执行桌面未确认为可交互输入桌面，未安装钩子/未发送输入。".to_owned()) } else { None };
    (diagnostic, failure)
}

unsafe extern "system" fn mouse_hook(code: i32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if code >= 0 && lp != 0 {
        let event = unsafe { &*(lp as *const MSLLHOOKSTRUCT) };
        if event.dwExtraInfo == engine::INPUT_MARKER {
            let index = EVENT_COUNT.fetch_add(1, Ordering::SeqCst);
            if index < MAX_EVENTS {
                EVENT_MESSAGES[index].store(wp as u32, Ordering::SeqCst);
                EVENT_FLAGS[index].store(event.flags, Ordering::SeqCst);
                EVENT_MOUSE_DATA[index].store(event.mouseData, Ordering::SeqCst);
            }
            // 所有带连点器标记的事件均在此吞掉，包括零滚动探针、按下及释放。
            return 1;
        }
        UNMARKED_PASSED.fetch_add(1, Ordering::Relaxed);
    }
    unsafe { CallNextHookEx(null_mut(), code, wp, lp) }
}

struct Hook(HHOOK);
impl Hook {
    fn install() -> Result<Self, String> {
        // 确保主线程消息队列先建立；低级钩子回调在此线程泵中执行。
        let mut message: MSG = unsafe { zeroed() };
        unsafe { PeekMessageW(&mut message, null_mut(), 0, 0, PM_NOREMOVE); }
        let hook = unsafe { SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_hook), GetModuleHandleW(null()), 0) };
        if hook.is_null() {
            Err(format!("低级鼠标钩子安装失败，未发送任何输入：{}", std::io::Error::last_os_error()))
        } else { Ok(Self(hook)) }
    }
    fn uninstall(&mut self) -> Result<(), String> {
        if self.0.is_null() { return Ok(()); }
        if unsafe { UnhookWindowsHookEx(self.0) } == 0 {
            return Err(format!("钩子卸载失败：{}", std::io::Error::last_os_error()));
        }
        self.0 = null_mut();
        Ok(())
    }
}
impl Drop for Hook {
    fn drop(&mut self) { let _ = self.uninstall(); }
}

fn pump() {
    let mut message: MSG = unsafe { zeroed() };
    unsafe {
        while PeekMessageW(&mut message, null_mut(), 0, 0, PM_REMOVE) != 0 {
            TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
}

fn pump_until(timeout: Duration, mut complete: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + timeout;
    loop {
        pump();
        if complete() { return true; }
        if Instant::now() >= deadline { return false; }
        unsafe { MsgWaitForMultipleObjectsEx(0, null(), 10, QS_ALLINPUT, MWMO_INPUTAVAILABLE); }
    }
}

fn pump_for(duration: Duration) { let _ = pump_until(duration, || false); }

fn stop_engine_pumping(mut engine: engine::Engine) -> engine::Engine {
    // Stop会Join输入线程，不能阻塞钩子所属线程，否则尚在发送中的输入可能等待钩子。
    let stopper = thread::spawn(move || { engine.stop(); engine });
    if !pump_until(Duration::from_secs(10), || stopper.is_finished()) {
        // 不卸钩再留下仍可能发送输入的线程；异常时直接结束整个验收进程。
        eprintln!("停止输入线程超时，直接结束验收进程以同时终止钩子与所有输入线程。");
        std::process::exit(3);
    }
    stopper.join().expect("停止输入线程异常")
}

fn event_sequence(start: usize, end: usize) -> Result<Vec<(u32, u32, u32)>, String> {
    if end > MAX_EVENTS { return Err("标记事件超出固定记录容量，验收中止。".into()); }
    Ok((start..end).map(|i| (EVENT_MESSAGES[i].load(Ordering::SeqCst), EVENT_FLAGS[i].load(Ordering::SeqCst), EVENT_MOUSE_DATA[i].load(Ordering::SeqCst))).collect())
}

fn safe_probe() -> Result<Value, String> {
    let before = EVENT_COUNT.load(Ordering::SeqCst);
    // 唯一一次零滚动探针：mouseData=0，不滚动、不移动指针、不点击。
    let sender = thread::spawn(|| {
        let input = INPUT { r#type: INPUT_MOUSE, Anonymous: INPUT_0 { mi: MOUSEINPUT {
            dx: 0, dy: 0, mouseData: 0, dwFlags: MOUSEEVENTF_WHEEL,
            time: 0, dwExtraInfo: engine::INPUT_MARKER,
        } } };
        unsafe { SendInput(1, &input, size_of::<INPUT>() as i32) }
    });
    if !pump_until(Duration::from_secs(3), || sender.is_finished()) {
        return Err("零滚动探针未完成，未启动点击引擎。".into());
    }
    let sent = sender.join().map_err(|_| "探针线程异常，未启动点击引擎。")?;
    // SendInput返回只表示进入系统队列，低级钩子通知可能仍待投递。
    let _ = pump_until(Duration::from_secs(3), || EVENT_COUNT.load(Ordering::SeqCst) > before);
    let after = EVENT_COUNT.load(Ordering::SeqCst);
    let events = event_sequence(before, after)?;
    if sent != 1 || events.len() != 1 || events[0].0 != WM_MOUSEWHEEL || events[0].1 & LLMHF_INJECTED == 0 || events[0].2 != 0 {
        return Err(format!("零滚动探针未确认钩子实际拦截：SendInput={sent}，事件={events:?}。未启动点击引擎。"));
    }
    Ok(json!({"Kind": "MOUSEEVENTF_WHEEL", "SendInputAccepted": sent, "InterceptedWheelEvents": events.len(),
        "InjectedFlagPresent": true, "MouseData": 0, "WheelDelta": 0, "RelativeDx": 0, "RelativeDy": 0,
        "SwallowedBeforeReachingForeground": true, "Passed": true}))
}

fn run_case(button: i32, mode: i32) -> Result<Value, String> {
    let (button_name, down, up) = match button {
        0 => ("左键", WM_LBUTTONDOWN, WM_LBUTTONUP),
        1 => ("右键", WM_RBUTTONDOWN, WM_RBUTTONUP),
        _ => ("中键", WM_MBUTTONDOWN, WM_MBUTTONUP),
    };
    let mode_name = ["单击", "双击", "三击"][mode as usize];
    let start = EVENT_COUNT.load(Ordering::SeqCst);
    let mut engine = engine::Engine::new(0, false);
    engine.start(&config::Config { button, mode, click_limit: 7, interval: 60, ..Default::default() })?;
    if !pump_until(Duration::from_secs(5), || !engine.snapshot().running) {
        let _stopped = stop_engine_pumping(engine);
        return Err(format!("{button_name}/{mode_name} 超时，已停止引擎。"));
    }
    let engine = stop_engine_pumping(engine);
    let snapshot = engine.snapshot();
    let end = EVENT_COUNT.load(Ordering::SeqCst);
    let events = event_sequence(start, end)?;
    let expected: Vec<_> = (0..7).flat_map(|_| [down, up]).collect();
    let actual: Vec<_> = events.iter().map(|event| event.0).collect();
    if snapshot.error.is_some() || snapshot.session != 7 || snapshot.total != 7 || snapshot.running
        || actual != expected || events.iter().any(|event| event.1 & LLMHF_INJECTED == 0) {
        return Err(format!("{button_name}/{mode_name} 断言失败：session={} total={} running={} error={:?} events={events:?}",
            snapshot.session, snapshot.total, snapshot.running, snapshot.error));
    }
    // 完成并 Join 线程后继续泵200ms，验证不存在排队输入或停止后的续点。
    pump_for(Duration::from_millis(200));
    let no_late_events = EVENT_COUNT.load(Ordering::SeqCst) == end && engine.snapshot().session == 7;
    if !no_late_events { return Err(format!("{button_name}/{mode_name} 停止后发现额外标记输入。")); }
    Ok(json!({"Button": button_name, "ButtonValue": button, "Mode": mode_name, "ModeValue": mode,
        "Limit": 7, "IntervalMilliseconds": 60, "NativeSendInput": true, "DryRun": false,
        "SuccessfulEngineClicks": snapshot.session, "EngineTotal": snapshot.total,
        "InterceptedDownEvents": actual.iter().filter(|&&message| message == down).count(),
        "InterceptedUpEvents": actual.iter().filter(|&&message| message == up).count(),
        "OrderedDownUpPairs": actual == expected, "AllInjectedFlagPresent": true,
        "SwallowedBeforeReachingForeground": true, "StoppedNoMoreEventsForMilliseconds": 200,
        "ElapsedSeconds": snapshot.elapsed.as_secs_f64(), "Passed": true,
        "RawMessages": events.iter().map(|(message, flags, mouse_data)| json!({"Message": message, "Flags": flags, "MouseData": mouse_data})).collect::<Vec<_>>() }))
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let argument = |key: &str| args.windows(2).find(|pair| pair[0] == key).map(|pair| pair[1].clone());
    let Some(out) = argument("--out").map(PathBuf::from) else { eprintln!("需要 --out 指定新结果文件。"); std::process::exit(2); };
    if out.exists() { eprintln!("结果文件已存在，未覆盖。"); std::process::exit(2); }
    let mut hashes = serde_json::Map::new();
    for (key, label) in [("--engine-sha256", "EngineSource"), ("--config-sha256", "ConfigSource"),
        ("--harness-sha256", "HarnessSource"), ("--exe-sha256", "HarnessExecutable")] {
        let Some(hash) = argument(key).filter(|hash| hash.len() == 64 && hash.chars().all(|c| c.is_ascii_hexdigit())) else {
            eprintln!("缺少有效SHA256：{key}，未安装钩子/未发送输入。"); std::process::exit(2);
        };
        hashes.insert(label.into(), Value::String(hash.to_ascii_lowercase()));
    }
    let started = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis();
    let mut report = json!({"SchemaVersion": 1, "StartedUnixMilliseconds": started, "ProcessId": std::process::id(),
        "HashesSha256": hashes, "Completed": false, "HookInstalled": false, "HookUninstalled": false,
        "Probe": null, "Cases": [], "Failure": null,
        "Safety": {"OnlyMarkerSwallowed": format!("0x{:08x}", engine::INPUT_MARKER),
            "UnmarkedInputAlwaysPassedToCallNextHookEx": true, "UsesForegroundActivation": false,
            "UsesCoordinatesOrSetCursorPos": false, "ProbeRelativeDxDy": [0,0], "ProbeWheelDelta": 0,
            "ProbeKind": "MOUSEEVENTF_WHEEL",
            "ClicksOnlyAfterObservedInjectedProbe": true, "AllMarkedMouseEventsSwallowed": true},
        "AcceptanceScope": "验证当前Rust输入引擎通过真实Windows SendInput进入鼠标输入链，随后由专用钩子吞掉；不代表任何游戏或前台目标接收输入。"});
    let (desktop, desktop_failure) = desktop_diagnostics();
    report["DesktopDiagnostics"] = desktop;
    if let Some(error) = desktop_failure {
        report["Failure"] = json!(error); write_report(&out, &report); std::process::exit(1);
    }
    let mut hook = match Hook::install() {
        Ok(hook) => { report["HookInstalled"] = json!(true); hook },
        Err(error) => { report["Failure"] = json!(error); write_report(&out, &report); std::process::exit(1); }
    };
    let outcome = (|| -> Result<(), String> {
        report["Probe"] = safe_probe()?;
        for button in 0..=2 {
            for mode in 0..=2 {
                report["Cases"].as_array_mut().unwrap().push(run_case(button, mode)?);
            }
        }
        Ok(())
    })();
    let cleanup = hook.uninstall();
    report["HookUninstalled"] = json!(cleanup.is_ok());
    report["TotalMarkedEventsSwallowed"] = json!(EVENT_COUNT.load(Ordering::SeqCst));
    report["UnmarkedEventsPassed"] = json!(UNMARKED_PASSED.load(Ordering::Relaxed));
    let success = outcome.is_ok() && cleanup.is_ok();
    report["Completed"] = json!(success);
    if let Err(error) = outcome.and(cleanup) { report["Failure"] = json!(error); }
    report["EndedUnixMilliseconds"] = json!(SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis());
    write_report(&out, &report);
    if success { println!("原生输入验收：9项全部通过；127个标记事件已吞掉；钩子已卸载。结果：{}", out.display()); }
    else { eprintln!("原生输入验收失败：{}", report["Failure"]); std::process::exit(1); }
}

fn write_report(path: &PathBuf, report: &Value) {
    if let Some(parent) = path.parent() { std::fs::create_dir_all(parent).expect("无法创建结果目录"); }
    let json = serde_json::to_vec_pretty(report).expect("无法序列化结果");
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new().write(true).create_new(true).open(path).expect("无法写入新结果文件");
    file.write_all(&json).expect("无法写入完整结果");
    file.sync_all().expect("无法同步结果文件");
}
