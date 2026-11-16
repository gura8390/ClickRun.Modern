//! Explicitly gated integration tool. Ordinary application cargo test never runs this.
#[path = "../../../src/system.rs"]
mod system;
use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
};
use windows::{
    Win32::{
        Foundation::{ERROR_FILE_NOT_FOUND, ERROR_PATH_NOT_FOUND, RPC_E_CHANGED_MODE},
        System::{
            Com::{
                CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
                CoTaskMemFree, CoUninitialize, IPersistFile, STGM_READ,
            },
            Registry::{
                HKEY, HKEY_CURRENT_USER, KEY_QUERY_VALUE, KEY_SET_VALUE, REG_OPTION_NON_VOLATILE,
                REG_SZ, REG_VALUE_TYPE, RegCloseKey, RegCreateKeyExW, RegDeleteValueW,
                RegOpenKeyExW, RegQueryValueExW, RegSetValueExW,
            },
        },
        UI::Shell::{
            FOLDERID_Startup, IShellLinkW, KF_FLAG_DEFAULT, SHGetKnownFolderPath, SLGP_RAWPATH,
            ShellLink,
        },
    },
    core::{Interface, PCWSTR, w},
};
const RUN_KEY: PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
const RUN_NAME: PCWSTR = w!("ClickRun");
type CheckResult<T> = Result<T, String>;
fn wide(path: &Path) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    path.as_os_str().encode_wide().chain(Some(0)).collect()
}
fn from_wide(value: &[u16]) -> String {
    String::from_utf16_lossy(&value[..value.iter().position(|c| *c == 0).unwrap_or(value.len())])
}
struct Key(HKEY);
impl Drop for Key {
    fn drop(&mut self) {
        let _ = unsafe { RegCloseKey(self.0) };
    }
}
#[derive(Clone, PartialEq, Eq)]
struct RawValue {
    kind: u32,
    bytes: Vec<u8>,
}
fn registry_read() -> CheckResult<Option<RawValue>> {
    let mut key = HKEY::default();
    let status =
        unsafe { RegOpenKeyExW(HKEY_CURRENT_USER, RUN_KEY, None, KEY_QUERY_VALUE, &mut key) };
    if status == ERROR_FILE_NOT_FOUND || status == ERROR_PATH_NOT_FOUND {
        return Ok(None);
    }
    status.ok().map_err(|e| format!("读取Run键失败：{e}"))?;
    let key = Key(key);
    let mut kind = REG_VALUE_TYPE::default();
    let mut length = 0;
    let status = unsafe {
        RegQueryValueExW(
            key.0,
            RUN_NAME,
            None,
            Some(&mut kind),
            None,
            Some(&mut length),
        )
    };
    if status == ERROR_FILE_NOT_FOUND {
        return Ok(None);
    }
    status
        .ok()
        .map_err(|e| format!("读取ClickRun值元数据失败：{e}"))?;
    if length > 1_048_576 {
        return Err("ClickRun注册值异常过大，未测试。".into());
    }
    let mut bytes = vec![0u8; length as usize];
    unsafe {
        RegQueryValueExW(
            key.0,
            RUN_NAME,
            None,
            Some(&mut kind),
            Some(bytes.as_mut_ptr()),
            Some(&mut length),
        )
    }
    .ok()
    .map_err(|e| format!("读取ClickRun值原始字节失败：{e}"))?;
    bytes.truncate(length as usize);
    Ok(Some(RawValue {
        kind: kind.0,
        bytes,
    }))
}
fn registry_restore(original: &Option<RawValue>) -> CheckResult<()> {
    let mut key = HKEY::default();
    if original.is_some() {
        unsafe {
            RegCreateKeyExW(
                HKEY_CURRENT_USER,
                RUN_KEY,
                None,
                PCWSTR::null(),
                REG_OPTION_NON_VOLATILE,
                KEY_SET_VALUE,
                None,
                &mut key,
                None,
            )
        }
        .ok()
        .map_err(|e| format!("恢复时打开Run键失败：{e}"))?;
    } else {
        let status =
            unsafe { RegOpenKeyExW(HKEY_CURRENT_USER, RUN_KEY, None, KEY_SET_VALUE, &mut key) };
        if status == ERROR_FILE_NOT_FOUND || status == ERROR_PATH_NOT_FOUND {
            return Ok(());
        }
        status
            .ok()
            .map_err(|e| format!("恢复时打开Run键失败：{e}"))?;
    }
    let key = Key(key);
    match original {
        Some(value) => unsafe {
            RegSetValueExW(
                key.0,
                RUN_NAME,
                None,
                REG_VALUE_TYPE(value.kind),
                Some(&value.bytes),
            )
        }
        .ok()
        .map_err(|e| format!("恢复ClickRun原类型和字节失败：{e}")),
        None => {
            let status = unsafe { RegDeleteValueW(key.0, RUN_NAME) };
            if status == ERROR_FILE_NOT_FOUND {
                Ok(())
            } else {
                status
                    .ok()
                    .map_err(|e| format!("恢复ClickRun原不存在状态失败：{e}"))
            }
        }
    }
}
fn startup_path() -> CheckResult<PathBuf> {
    let allocated = unsafe { SHGetKnownFolderPath(&FOLDERID_Startup, KF_FLAG_DEFAULT, None) }
        .map_err(|e| format!("读取启动文件夹失败：{e}"))?;
    let path = unsafe { allocated.to_string() };
    unsafe { CoTaskMemFree(Some(allocated.0.cast())) };
    Ok(PathBuf::from(path.map_err(|e| e.to_string())?).join("ClickRun.lnk"))
}
fn optional_bytes(path: &Path) -> CheckResult<Option<Vec<u8>>> {
    match fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("读取{}失败：{e}", path.display())),
    }
}
struct Snapshot {
    registry: Option<RawValue>,
    link: Option<Vec<u8>>,
    startup: PathBuf,
}
impl Snapshot {
    fn capture() -> CheckResult<Self> {
        let startup = startup_path()?;
        Ok(Self {
            registry: registry_read()?,
            link: optional_bytes(&startup)?,
            startup,
        })
    }
    fn save_backup(&self, dir: &Path) -> CheckResult<()> {
        // Never overwrite a prior test's restoration evidence.
        fs::create_dir(dir).map_err(|e| format!("创建独立备份目录失败：{e}"))?;
        if let Some(registry) = &self.registry {
            write_sync(&dir.join("registry-value.bin"), &registry.bytes)?;
        }
        if let Some(link) = &self.link {
            write_sync(&dir.join("ClickRun.lnk"), link)?;
        }
        let metadata = serde_json::json!({
            "registryKey": "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Run",
            "registryValue": "ClickRun", "registryExisted": self.registry.is_some(),
            "registryType": self.registry.as_ref().map(|v| v.kind),
            "registryByteLength": self.registry.as_ref().map(|v| v.bytes.len()),
            "startupPath": self.startup, "shortcutExisted": self.link.is_some(),
            "shortcutByteLength": self.link.as_ref().map(Vec::len),
            "testScope": "真实API注册验证；不启动应用、不操作UI、不实际登录"
        });
        write_sync(
            &dir.join("before.json"),
            &serde_json::to_vec_pretty(&metadata).unwrap(),
        )
    }
    fn restore(&self) -> CheckResult<()> {
        // Attempt both restores independently, even if one fails.
        let registry = registry_restore(&self.registry);
        let link = match &self.link {
            Some(bytes) => write_sync(&self.startup, bytes),
            None => match fs::remove_file(&self.startup) {
                Ok(()) => Ok(()),
                Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
                Err(e) => Err(format!("恢复快捷方式原不存在状态失败：{e}")),
            },
        };
        match (registry, link) {
            (Ok(()), Ok(())) => Ok(()),
            (Err(a), Err(b)) => Err(format!("{a}；{b}")),
            (Err(e), _) | (_, Err(e)) => Err(e),
        }
    }
    fn verify(&self) -> CheckResult<(bool, bool)> {
        Ok((
            registry_read()? == self.registry,
            optional_bytes(&self.startup)? == self.link,
        ))
    }
}
fn write_sync(path: &Path, data: &[u8]) -> CheckResult<()> {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(path)
        .map_err(|e| format!("写入{}失败：{e}", path.display()))?;
    file.write_all(data)
        .and_then(|_| file.sync_all())
        .map_err(|e| format!("持久化{}失败：{e}", path.display()))
}
fn same_path(a: &str, b: &Path) -> bool {
    a.replace('/', "\\").to_lowercase() == b.to_string_lossy().replace('/', "\\").to_lowercase()
}
fn validate_enabled(startup: &Path, current: &Path) -> CheckResult<serde_json::Value> {
    if !system::autostart_state()? {
        return Err("启用后系统状态未读回开启。".into());
    }
    let raw = registry_read()?.ok_or("启用后缺少Run ClickRun值。")?;
    if raw.kind != REG_SZ.0 {
        return Err("Run值不是REG_SZ。".into());
    }
    let command = format!("\"{}\" --tray", current.display());
    let expected: Vec<u8> = command
        .encode_utf16()
        .chain(Some(0))
        .flat_map(u16::to_le_bytes)
        .collect();
    if raw.bytes != expected {
        return Err("Run值路径、引号、参数或终止符不匹配。".into());
    }
    let link: IShellLinkW = unsafe { CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER) }
        .map_err(|e| e.to_string())?;
    let persist: IPersistFile = link.cast().map_err(|e| e.to_string())?;
    let source = wide(startup);
    unsafe { persist.Load(PCWSTR(source.as_ptr()), STGM_READ) }.map_err(|e| e.to_string())?;
    let mut target = vec![0u16; 32768];
    let mut arguments = vec![0u16; 32768];
    let mut working = vec![0u16; 32768];
    (|| unsafe {
        link.GetPath(&mut target, std::ptr::null_mut(), SLGP_RAWPATH.0 as u32)?;
        link.GetArguments(&mut arguments)?;
        link.GetWorkingDirectory(&mut working)?;
        Ok::<(), windows::core::Error>(())
    })()
    .map_err(|e| e.to_string())?;
    let (target, arguments, working) = (
        from_wide(&target),
        from_wide(&arguments),
        from_wide(&working),
    );
    if !same_path(&target, current)
        || arguments != "--tray"
        || !same_path(&working, current.parent().unwrap())
    {
        return Err("ShellLink目标、--tray参数或工作目录不匹配。".into());
    }
    Ok(
        serde_json::json!({"runType": raw.kind, "runCommand": command, "shellLinkTarget": target,
        "shellLinkArguments": arguments, "shellLinkWorkingDirectory": working, "state": true}),
    )
}
fn validate_disabled(startup: &Path) -> CheckResult<serde_json::Value> {
    if system::autostart_state()?
        || registry_read()?.is_some()
        || optional_bytes(startup)?.is_some()
    {
        return Err("停用后仍存在本次测试启动入口或状态未关闭。".into());
    }
    Ok(serde_json::json!({"state": false, "runValueAbsent": true, "shortcutAbsent": true}))
}
fn checks(
    steps: &mut Vec<serde_json::Value>,
    snapshot: &Snapshot,
    current: &Path,
) -> CheckResult<()> {
    for (phase, enable) in [
        ("enable", true),
        ("enable-again", true),
        ("disable", false),
        ("disable-again", false),
    ] {
        system::set_autostart(enable)?;
        let state = if enable {
            validate_enabled(&snapshot.startup, current)?
        } else {
            validate_disabled(&snapshot.startup)?
        };
        steps.push(serde_json::json!({"phase": phase, "passed": true, "readback": state}));
    }
    Ok(())
}
fn run() -> CheckResult<()> {
    let args: Vec<_> = std::env::args_os().collect();
    if args.len() != 3 || args[1] != "--confirm-real-system" {
        return Err("仅显式运行：clickrun-system-check.exe --confirm-real-system <尚不存在的备份目录>。普通测试不会修改系统。".into());
    }
    if std::env::var_os("CLICKRUN_TEST_SYSTEM_DIR").is_some() {
        return Err(
            "检测到模拟系统目录环境变量，真实验证未执行。请从未设置该变量的环境运行。".into(),
        );
    }
    let dir = PathBuf::from(&args[2]);
    let current = std::env::current_exe().map_err(|e| e.to_string())?;
    let snapshot = Snapshot::capture()?;
    snapshot.save_backup(&dir)?;
    let mut steps = Vec::new();
    // Every ordinary Result failure and panic reaches restoration before returning.
    let execution = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        checks(&mut steps, &snapshot, &current)
    }));
    let test_error = match execution {
        Ok(result) => result.err(),
        Err(_) => Some("测试发生panic，已进入恢复。".into()),
    };
    let restore_error = snapshot.restore().err();
    let verification = snapshot.verify();
    let (registry_match, link_match, verify_error) = match verification {
        Ok((registry, link)) => (registry, link, None),
        Err(error) => (false, false, Some(error)),
    };
    let passed = test_error.is_none()
        && restore_error.is_none()
        && verify_error.is_none()
        && registry_match
        && link_match;
    let report = serde_json::json!({
        "scope": "真实用户级注册验证；未实际登录，未启动/退出应用，未操作UI",
        "harnessExecutable": current, "backupDirectory": dir,
        "beforeRegistryExisted": snapshot.registry.is_some(),
        "beforeRegistryType": snapshot.registry.as_ref().map(|v| v.kind),
        "beforeShortcutExisted": snapshot.link.is_some(),
        "steps": steps, "testError": test_error, "restoreError": restore_error, "verifyError": verify_error,
        "registryExistenceTypeAndBytesRestoredExactly": registry_match,
        "shortcutExistenceAndBytesRestoredExactly": link_match, "passed": passed
    });
    let report_path = dir
        .parent()
        .ok_or("备份目录必须有父目录。")?
        .join("system-real-registration.json");
    write_sync(
        &dir.join("registration-result.json"),
        &serde_json::to_vec_pretty(&report).unwrap(),
    )?;
    write_sync(&report_path, &serde_json::to_vec_pretty(&report).unwrap())?;
    println!("{}", serde_json::to_string_pretty(&report).unwrap());
    if passed {
        Ok(())
    } else {
        Err(format!(
            "真实系统注册验证未通过，详见{}",
            report_path.display()
        ))
    }
}
fn main() {
    let initialized = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
    if initialized.is_err() && initialized != RPC_E_CHANGED_MODE {
        eprintln!("COM初始化失败：{initialized}");
        std::process::exit(2);
    }
    let result = run();
    if initialized.is_ok() {
        unsafe { CoUninitialize() };
    }
    if let Err(error) = result {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
