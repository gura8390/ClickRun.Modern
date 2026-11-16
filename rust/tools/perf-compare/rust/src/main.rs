#![allow(dead_code)]
#[path = "../../../../src/config.rs"]
mod config;
#[path = "../../../../src/engine.rs"]
mod engine;
use std::{io::{self, Write}, thread, time::{Duration, Instant}};
use windows_sys::Win32::{Foundation::FILETIME, System::Threading::{GetCurrentProcess, GetProcessTimes}};

fn cpu_seconds() -> f64 {
    let mut created = FILETIME::default();
    let mut exited = FILETIME::default();
    let mut kernel = FILETIME::default();
    let mut user = FILETIME::default();
    let success = unsafe { GetProcessTimes(GetCurrentProcess(), &mut created, &mut exited, &mut kernel, &mut user) };
    assert_ne!(success, 0, "CPU time query failed");
    let ticks = |t: FILETIME| (u64::from(t.dwHighDateTime) << 32) | u64::from(t.dwLowDateTime);
    (ticks(kernel) + ticks(user)) as f64 / 10_000_000.0
}
fn main() {
    let args: Vec<_> = std::env::args().collect();
    let interval = args[1].parse::<i32>().unwrap();
    let seconds = args[2].parse::<u64>().unwrap();
    let settings = config::Config { interval, mode: 0, random_delay: 0, click_limit: 0, ..Default::default() };
    let mut warm = engine::Engine::new(0, true);
    warm.start(&config::Config { interval: 1, ..Default::default() }).unwrap();
    thread::sleep(Duration::from_millis(500));
    warm.stop();
    drop(warm);
    let mut engine = engine::Engine::new(0, true);
    println!("READY");
    io::stdout().flush().unwrap();
    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap();
    assert_eq!(input.trim(), "START");
    let before = cpu_seconds();
    let clock = Instant::now();
    engine.start(&settings).unwrap();
    thread::sleep(Duration::from_secs(seconds));
    engine.stop();
    let elapsed = clock.elapsed().as_secs_f64();
    let after = cpu_seconds();
    let snapshot = engine.snapshot();
    assert!(snapshot.error.is_none(), "{:?}", snapshot.error);
    println!("{}", serde_json::json!({
        "implementation": "Rust 3.0.0 source core",
        "workload": "dry sender; single click; zero random; unlimited; no UI/config/autostart",
        "intervalMs": interval,
        "requestedSeconds": seconds,
        "elapsedSeconds": elapsed,
        "clicks": snapshot.session,
        "clicksPerSecond": snapshot.session as f64 / elapsed,
        "cpuSeconds": after - before,
        "cpuPercentOneCore": 100.0 * (after - before) / elapsed,
        "cpuMillisecondsPerThousandClicks": if snapshot.session == 0 { 0.0 } else { (after - before) * 1_000_000.0 / snapshot.session as f64 },
        "processId": std::process::id()
    }));
}
