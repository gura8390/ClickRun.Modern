//! 与 Modern 版共用的配置格式。仅在变更时分配/写盘，空闲时无后台任务。
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Config {
    pub hot_key: i32,
    pub hot_key_modifiers: i32,
    pub button: i32,
    pub interval: i32,
    pub random_delay: i32,
    pub mode: i32,
    pub click_limit: i32,
    pub start_with_windows: bool,
    pub minimize_to_tray: bool,
    pub play_sound: bool,
    pub always_on_top: bool,
    pub total_clicks: u64,
    pub profiles: Vec<Profile>,
    /// 保留将来版本或用户附加的数据，不把保存操作变成数据删减。
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            hot_key: 117,
            hot_key_modifiers: 0,
            button: 0,
            interval: 100,
            random_delay: 0,
            mode: 0,
            click_limit: 0,
            start_with_windows: false,
            minimize_to_tray: true,
            play_sound: false,
            always_on_top: false,
            total_clicks: 0,
            profiles: Vec::new(),
            extra: BTreeMap::new(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Profile {
    pub name: String,
    pub interval: i32,
    pub random_delay: i32,
    pub button: i32,
    pub mode: i32,
    pub click_limit: i32,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

impl Default for Profile {
    fn default() -> Self {
        Self {
            name: String::new(),
            interval: 100,
            random_delay: 0,
            button: 0,
            mode: 0,
            click_limit: 0,
            extra: BTreeMap::new(),
        }
    }
}

/// 与旧版相同：F12、Windows 键与单独修饰键不能作为启停热键。
pub fn valid_hot_key(key: i32) -> bool {
    (1..255).contains(&key) && !matches!(key, 0x10 | 0x11 | 0x12 | 0x5b | 0x5c | 0x7b | 0xa0..=0xa5)
}

impl Config {
    pub fn normalize(&mut self) {
        self.interval = self.interval.clamp(1, 10_000);
        self.random_delay = self.random_delay.clamp(0, 50);
        self.click_limit = self.click_limit.max(0);
        if !valid_hot_key(self.hot_key) {
            self.hot_key = 117;
        }
        self.hot_key_modifiers &= 7;
        if !(0..=2).contains(&self.button) {
            self.button = 0;
        }
        if !(0..=2).contains(&self.mode) {
            self.mode = 0;
        }
        let mut names = BTreeSet::new();
        self.profiles.retain_mut(|profile| {
            profile.name = profile.name.trim().to_owned();
            if profile.name.is_empty() || !names.insert(profile.name.to_lowercase()) {
                return false;
            }
            profile.interval = profile.interval.clamp(1, 10_000);
            profile.random_delay = profile.random_delay.clamp(0, 50);
            profile.click_limit = profile.click_limit.max(0);
            if !(0..=2).contains(&profile.button) {
                profile.button = 0;
            }
            if !(0..=2).contains(&profile.mode) {
                profile.mode = 0;
            }
            true
        });
    }
}

pub fn default_dir() -> PathBuf {
    std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("ClickRun")
}

#[derive(Debug)]
pub struct ConfigStore {
    pub dir: PathBuf,
    gate: Mutex<()>,
}

impl ConfigStore {
    pub fn new(dir: PathBuf) -> Self {
        Self {
            dir,
            gate: Mutex::new(()),
        }
    }

    /// 读坏的主文件先留存，随后尝试上次有效版本；错误通过中文警示返回 UI。
    pub fn load(&self) -> (Config, Option<String>) {
        let _guard = self.gate.lock().unwrap_or_else(|e| e.into_inner());
        let path = self.dir.join("config.json");
        let mut warning = None;
        if path.exists() {
            match read_config(&path) {
                Ok(config) => return (config, None),
                Err(error) => {
                    let preserved = preserve_corrupt(&path);
                    warning = Some(match preserved {
                        Ok(_) => format!("配置文件无法读取，已保留原文件。{error}"),
                        Err(preserve_error) => format!(
                            "配置文件无法读取，原文件仍保留在原路径。无法另存损坏副本：{preserve_error}。{error}"
                        ),
                    });
                }
            }
        }
        let backup = self.dir.join("config.json.bak");
        if backup.exists() {
            match read_config(&backup) {
                Ok(config) => {
                    let prefix = warning.unwrap_or_else(|| "主配置文件不存在。".to_owned());
                    return (config, Some(format!("{prefix} 已恢复上一次保存的备份。")));
                }
                Err(error) => {
                    warning = Some(format!(
                        "{} 备份也无法读取：{error}",
                        warning.unwrap_or_default()
                    ));
                }
            }
        }
        (
            Config::default(),
            warning.map(|s| format!("{s} 当前使用默认设置。")),
        )
    }

    /// 先同步临时文件，再由 Windows 原子替换。有效主文件成为备份；损坏主文件
    /// 不会覆盖备份。无论哪一步失败，原主文件及有效备份均不会被先行删除。
    pub fn save(&self, config: &Config) -> Result<(), String> {
        let _guard = self.gate.lock().unwrap_or_else(|e| e.into_inner());
        self.save_inner(config)
            .map_err(|e| format!("保存配置失败：{e}"))
    }

    fn save_inner(&self, config: &Config) -> Result<(), String> {
        fs::create_dir_all(&self.dir).map_err(|e| format!("无法创建配置目录：{e}"))?;
        let mut snapshot = config.clone();
        snapshot.normalize();
        let json =
            serde_json::to_vec_pretty(&snapshot).map_err(|e| format!("无法转换配置数据：{e}"))?;
        let target = self.dir.join("config.json");
        let backup = self.dir.join("config.json.bak");
        let temp = self
            .dir
            .join(format!("config.json.{}.tmp", unique_suffix()));
        let result = (|| {
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temp)
                .map_err(|e| format!("无法创建临时配置文件：{e}"))?;
            file.write_all(&json)
                .map_err(|e| format!("无法写入临时配置文件：{e}"))?;
            file.sync_all()
                .map_err(|e| format!("无法同步临时配置文件：{e}"))?;
            drop(file);
            let existed = target.exists();
            let valid = existed && read_config(&target).is_ok();
            if existed && !valid {
                preserve_corrupt(&target)
                    .map_err(|e| format!("无法保留损坏配置，已取消覆盖：{e}"))?;
            }
            atomic_install(
                &temp,
                &target,
                if valid { Some(&backup) } else { None },
                existed,
            )
            .map_err(|e| format!("无法替换主配置文件：{e}"))
        })();
        if temp.exists() {
            let _ = fs::remove_file(&temp);
        }
        result
    }
}

const CONFIG_KEYS: &[&str] = &[
    "hotKey",
    "hotKeyModifiers",
    "button",
    "interval",
    "randomDelay",
    "mode",
    "clickLimit",
    "startWithWindows",
    "minimizeToTray",
    "playSound",
    "alwaysOnTop",
    "totalClicks",
    "profiles",
];
const PROFILE_KEYS: &[&str] = &[
    "name",
    "interval",
    "randomDelay",
    "button",
    "mode",
    "clickLimit",
];

fn canonicalize(value: &mut Value, keys: &[&str]) {
    if let Some(object) = value.as_object_mut() {
        let old = std::mem::take(object);
        for (key, value) in old {
            let canonical = keys
                .iter()
                .find(|candidate| candidate.eq_ignore_ascii_case(&key));
            object.insert(canonical.map_or(key.as_str(), |key| *key).to_owned(), value);
        }
    }
}

fn read_config(path: &Path) -> Result<Config, String> {
    let bytes = fs::read(path).map_err(|e| format!("无法读取配置文件：{e}"))?;
    // .NET 与部分编辑器输出 UTF-8 BOM；serde_json 不接受 BOM，需要显式兼容。
    let bytes = bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(&bytes);
    let mut value: Value =
        serde_json::from_slice(bytes).map_err(|e| format!("配置格式无效：{e}"))?;
    if !value.is_object() {
        return Err("配置内容必须是对象。".to_owned());
    }
    canonicalize(&mut value, CONFIG_KEYS);
    if value
        .get("totalClicks")
        .and_then(Value::as_i64)
        .is_some_and(|n| n < 0)
    {
        value["totalClicks"] = Value::from(0_u64);
    }
    if value.get("profiles").is_some_and(Value::is_null) {
        value["profiles"] = Value::Array(Vec::new());
    }
    if let Some(profiles) = value.get_mut("profiles").and_then(Value::as_array_mut) {
        profiles.retain(|profile| !profile.is_null());
        for profile in profiles {
            canonicalize(profile, PROFILE_KEYS);
            if profile.get("name").is_some_and(Value::is_null) {
                profile["name"] = Value::from("");
            }
        }
    }
    let mut config: Config =
        serde_json::from_value(value).map_err(|e| format!("配置字段无效：{e}"))?;
    config.normalize();
    Ok(config)
}

fn unique_suffix() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let ticks = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!(
        "{ticks}-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    )
}

fn preserve_corrupt(path: &Path) -> Result<PathBuf, std::io::Error> {
    let damaged = path.with_file_name(format!("config.json.corrupt-{}", unique_suffix()));
    // create_new 保证现存留存文件绝不被覆盖。
    let mut destination = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&damaged)?;
    let mut original = fs::File::open(path)?;
    std::io::copy(&mut original, &mut destination)?;
    destination.sync_all()?;
    Ok(damaged)
}

#[cfg(windows)]
fn atomic_install(
    temp: &Path,
    target: &Path,
    backup: Option<&Path>,
    existed: bool,
) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn ReplaceFileW(
            replaced: *const u16,
            replacement: *const u16,
            backup: *const u16,
            flags: u32,
            exclude: *mut core::ffi::c_void,
            reserved: *mut core::ffi::c_void,
        ) -> i32;
        fn MoveFileExW(existing: *const u16, destination: *const u16, flags: u32) -> i32;
    }
    fn wide(path: &Path) -> Vec<u16> {
        path.as_os_str().encode_wide().chain(Some(0)).collect()
    }
    let source = wide(temp);
    let destination = wide(target);
    let backup = backup.map(wide);
    // Both files reside in the same directory/volume. WRITE_THROUGH is supported
    // by MoveFileEx; ReplaceFile receives zero because its WRITE_THROUGH flag is
    // documented as unsupported.
    let ok = unsafe {
        if existed {
            ReplaceFileW(
                destination.as_ptr(),
                source.as_ptr(),
                backup.as_ref().map_or(std::ptr::null(), |v| v.as_ptr()),
                0,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        } else {
            MoveFileExW(source.as_ptr(), destination.as_ptr(), 0x8)
        }
    };
    if ok == 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(not(windows))]
fn atomic_install(
    temp: &Path,
    target: &Path,
    backup: Option<&Path>,
    _existed: bool,
) -> std::io::Result<()> {
    if let Some(backup) = backup {
        fs::copy(target, backup)?;
    }
    fs::rename(temp, target)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestDirectory(PathBuf);
    impl TestDirectory {
        fn new() -> Self {
            let path =
                std::env::temp_dir().join(format!("clickrun-config-test-{}", unique_suffix()));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
        fn store(&self) -> ConfigStore {
            ConfigStore::new(self.0.clone())
        }
    }
    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn defaults_match_modern_and_missing_file_has_no_warning() {
        let directory = TestDirectory::new();
        let (config, warning) = directory.store().load();
        assert_eq!(config, Config::default());
        assert_eq!(warning, None);
    }

    #[test]
    fn case_insensitive_legacy_bom_negative_total_and_null_profiles() {
        let directory = TestDirectory::new();
        fs::write(
            directory.0.join("config.json"),
            b"\xef\xbb\xbf{\"HOTKEY\":65,\"INTERVAL\":17,\"TOTALCLICKS\":-3,\"PROFILES\":null}",
        )
        .unwrap();
        let (config, warning) = directory.store().load();
        assert!(warning.is_none());
        assert_eq!(config.hot_key, 65);
        assert_eq!(config.interval, 17);
        assert_eq!(config.total_clicks, 0);
        assert!(config.profiles.is_empty());
    }

    #[test]
    fn normalization_and_profiles_match_legacy_rules() {
        let directory = TestDirectory::new();
        fs::write(directory.0.join("config.json"), r#"{"hotKey":123,"hotKeyModifiers":15,"interval":-1,"randomDelay":77,"button":3,"mode":-2,"clickLimit":-1,"profiles":[null,{"NAME":" A ","Interval":20000,"randomDelay":-1},{"name":"a"},{"name":null}]}"#).unwrap();
        let (config, warning) = directory.store().load();
        assert!(warning.is_none());
        assert_eq!((config.hot_key, config.hot_key_modifiers), (117, 7));
        assert_eq!(
            (
                config.interval,
                config.random_delay,
                config.button,
                config.mode,
                config.click_limit
            ),
            (1, 50, 0, 0, 0)
        );
        assert_eq!(config.profiles.len(), 1);
        assert_eq!(config.profiles[0].name, "A");
        assert_eq!(config.profiles[0].interval, 10_000);
        assert_eq!(config.profiles[0].random_delay, 0);
    }

    #[test]
    fn atomic_save_round_trip_unknown_fields_and_previous_version_backup() {
        let directory = TestDirectory::new();
        let store = directory.store();
        let mut config = Config {
            total_clicks: 123,
            ..Config::default()
        };
        config
            .extra
            .insert("futureSetting".to_owned(), serde_json::json!({"keep":true}));
        config.profiles.push(Profile {
            name: "测试方案".to_owned(),
            mode: 2,
            click_limit: 7,
            ..Profile::default()
        });
        config.profiles[0]
            .extra
            .insert("futureProfile".to_owned(), Value::from(3));
        store.save(&config).unwrap();
        assert_eq!(store.load().0, config);
        config.interval = 42;
        store.save(&config).unwrap();
        assert_eq!(store.load().0, config);
        let old = read_config(&directory.0.join("config.json.bak")).unwrap();
        assert_eq!(old.interval, 100);
        assert_eq!(old.total_clicks, 123);
        assert!(old.extra.contains_key("futureSetting"));
        assert!(old.profiles[0].extra.contains_key("futureProfile"));
        assert!(!fs::read_dir(&directory.0).unwrap().any(|entry| {
            entry
                .unwrap()
                .path()
                .extension()
                .is_some_and(|v| v == "tmp")
        }));
    }

    #[test]
    fn corrupt_primary_preserved_and_backup_never_overwritten_with_damage() {
        let directory = TestDirectory::new();
        let store = directory.store();
        let config = Config {
            total_clicks: 456,
            ..Config::default()
        };
        store.save(&config).unwrap();
        store
            .save(&Config {
                total_clicks: 789,
                ..Config::default()
            })
            .unwrap();
        fs::write(directory.0.join("config.json"), "{损坏}").unwrap();
        let (recovered, warning) = store.load();
        assert_eq!(recovered.total_clicks, 456);
        assert!(warning.unwrap().contains("已恢复"));
        store.save(&recovered).unwrap();
        assert_eq!(
            read_config(&directory.0.join("config.json.bak"))
                .unwrap()
                .total_clicks,
            456
        );
        let saved_damage: Vec<_> = fs::read_dir(&directory.0)
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with("config.json.corrupt-")
            })
            .collect();
        assert!(!saved_damage.is_empty());
        assert_eq!(
            fs::read_to_string(saved_damage[0].path()).unwrap(),
            "{损坏}"
        );
    }

    #[test]
    fn failed_save_has_chinese_error_and_preserves_original() {
        let directory = TestDirectory::new();
        let blocked = directory.0.join("blocked");
        fs::write(&blocked, "原始内容").unwrap();
        let result = ConfigStore::new(blocked.clone()).save(&Config::default());
        assert!(result.unwrap_err().contains("保存配置失败"));
        assert_eq!(fs::read_to_string(blocked).unwrap(), "原始内容");
    }

    #[test]
    fn invalid_primary_and_backup_use_defaults_with_warning() {
        let directory = TestDirectory::new();
        fs::write(directory.0.join("config.json"), "null").unwrap();
        fs::write(directory.0.join("config.json.bak"), "[]").unwrap();
        let (config, warning) = directory.store().load();
        assert_eq!(config, Config::default());
        assert!(warning.unwrap().contains("默认设置"));
    }
}
