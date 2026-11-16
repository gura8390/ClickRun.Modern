//! 按事件写入诊断，空闲时不运行线程或定时任务。诊断失败不影响停止/退出。
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::Mutex,
};

const MAX_LOG_BYTES: u64 = 1_048_576;
static GATE: Mutex<()> = Mutex::new(());

pub fn write(message: &str) {
    if message.is_empty() {
        return;
    }
    let _guard = GATE.lock().unwrap_or_else(|error| error.into_inner());
    let dir = std::env::var_os("CLICKRUN_TEST_LOG_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            std::env::var_os("LOCALAPPDATA")
                .map(PathBuf::from)
                .unwrap_or_else(std::env::temp_dir)
                .join("ClickRun")
                .join("logs")
        });
    let _ = write_to(&dir, message);
}

fn write_to(dir: &Path, message: &str) -> std::io::Result<()> {
    fs::create_dir_all(dir)?;
    let path = dir.join("clickrun.log");
    if fs::metadata(&path).is_ok_and(|metadata| metadata.len() > MAX_LOG_BYTES) {
        let old = dir.join("clickrun.log.1");
        if old.try_exists()? {
            fs::remove_file(&old)?;
        }
        fs::rename(&path, &old)?;
    }
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    writeln!(file, "{} {}", timestamp(), message)
}

#[cfg(windows)]
fn timestamp() -> String {
    use windows_sys::Win32::Foundation::SYSTEMTIME;
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetLocalTime(time: *mut SYSTEMTIME);
    }
    let mut time = SYSTEMTIME::default();
    unsafe {
        GetLocalTime(&mut time);
    }
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}.{:03} 本地时间",
        time.wYear,
        time.wMonth,
        time.wDay,
        time.wHour,
        time.wMinute,
        time.wSecond,
        time.wMilliseconds
    )
}

#[cfg(not(windows))]
fn timestamp() -> String {
    let time = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    format!("Unix {}.{:03}", time.as_secs(), time.subsec_millis())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn log_preserves_unicode_and_rotates_previous_megabyte() {
        let suffix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir =
            std::env::temp_dir().join(format!("clickrun-log-test-{}-{suffix}", std::process::id()));
        fs::create_dir(&dir).unwrap();
        let path = dir.join("clickrun.log");
        write_to(&dir, "配置保存失败：测试中文").unwrap();
        assert!(
            fs::read_to_string(&path)
                .unwrap()
                .contains("配置保存失败：测试中文")
        );
        fs::write(&path, vec![b'x'; MAX_LOG_BYTES as usize + 1]).unwrap();
        fs::write(dir.join("clickrun.log.1"), "上次留存").unwrap();
        write_to(&dir, "第二条诊断").unwrap();
        assert_eq!(
            fs::metadata(dir.join("clickrun.log.1")).unwrap().len(),
            MAX_LOG_BYTES + 1
        );
        let current = fs::read_to_string(path).unwrap();
        assert!(current.contains("第二条诊断"));
        assert!(current.len() < 512);
        fs::remove_dir_all(dir).unwrap();
    }
}
