//! 用户级登录启动。测试目录只模拟注册状态，不触碰真实注册表或启动文件夹。
use std::{
    fs,
    path::{Path, PathBuf},
};
use windows::{
    Win32::{
        Foundation::{ERROR_FILE_NOT_FOUND, ERROR_PATH_NOT_FOUND, RPC_E_CHANGED_MODE},
        Storage::FileSystem::{MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW},
        System::{
            Com::{
                CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
                CoTaskMemFree, CoUninitialize, IPersistFile, STGM_READ,
            },
            Registry::{
                HKEY, HKEY_CURRENT_USER, KEY_SET_VALUE, REG_OPTION_NON_VOLATILE, REG_SZ,
                RRF_RT_REG_SZ, RegCloseKey, RegCreateKeyExW, RegDeleteValueW, RegGetValueW,
                RegOpenKeyExW, RegSetValueExW,
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
const LEGACY_MODERN_EXE: &str = r"D:\codex\ClickRun\ClickRun.Modern\publish\ClickRun.Modern.exe";

fn wide(value: &std::ffi::OsStr) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    value.encode_wide().chain(Some(0)).collect()
}

fn normalized(value: &str) -> String {
    value
        .trim()
        .trim_matches('"')
        .replace('/', "\\")
        .trim_start_matches(r"\\?\")
        .to_lowercase()
}

fn same_exe(a: &str, b: &Path) -> bool {
    normalized(a) == normalized(&b.to_string_lossy())
}

fn managed_exe(path: &str, current: &Path) -> bool {
    same_exe(path, current) || normalized(path) == normalized(LEGACY_MODERN_EXE)
}

// The old version quoted paths containing spaces, but used unquoted paths otherwise.
fn executable_part(command: &str) -> Option<&str> {
    let command = command.trim();
    if let Some(rest) = command.strip_prefix('"') {
        return rest.split_once('"').map(|(exe, _)| exe);
    }
    let lower = command.to_ascii_lowercase();
    let end = lower.find(".exe")? + 4;
    if command[end..]
        .chars()
        .next()
        .is_some_and(|c| !c.is_whitespace())
    {
        return None;
    }
    Some(&command[..end])
}

struct ComGuard(bool);
impl ComGuard {
    fn new() -> Result<Self, String> {
        let result = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
        if result.is_ok() {
            Ok(Self(true))
        } else if result == RPC_E_CHANGED_MODE {
            Ok(Self(false))
        } else {
            Err(format!("无法初始化启动项接口：{result}"))
        }
    }
}
impl Drop for ComGuard {
    fn drop(&mut self) {
        if self.0 {
            unsafe { CoUninitialize() };
        }
    }
}

struct RegKey(HKEY);
impl Drop for RegKey {
    fn drop(&mut self) {
        let _ = unsafe { RegCloseKey(self.0) };
    }
}

fn registry_command() -> Result<Option<String>, String> {
    let mut bytes = 0u32;
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            RUN_KEY,
            RUN_NAME,
            RRF_RT_REG_SZ,
            None,
            None,
            Some(&mut bytes),
        )
    };
    if status == ERROR_FILE_NOT_FOUND || status == ERROR_PATH_NOT_FOUND {
        return Ok(None);
    }
    status
        .ok()
        .map_err(|e| format!("无法读取登录启动注册项：{e}"))?;
    // A Run command cannot usefully exceed this bound; reject oversized foreign data.
    if bytes > 65536 {
        return Err("登录启动注册项过长，未修改。".into());
    }
    let mut buffer = vec![0u16; (bytes as usize + 1) / 2 + 1];
    unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            RUN_KEY,
            RUN_NAME,
            RRF_RT_REG_SZ,
            None,
            Some(buffer.as_mut_ptr().cast()),
            Some(&mut bytes),
        )
    }
    .ok()
    .map_err(|e| format!("无法读取登录启动注册项：{e}"))?;
    let length = buffer.iter().position(|c| *c == 0).unwrap_or(buffer.len());
    Ok(Some(String::from_utf16_lossy(&buffer[..length])))
}

fn set_registry(command: Option<&str>) -> Result<(), String> {
    let mut handle = HKEY::default();
    if command.is_some() {
        unsafe {
            RegCreateKeyExW(
                HKEY_CURRENT_USER,
                RUN_KEY,
                None,
                PCWSTR::null(),
                REG_OPTION_NON_VOLATILE,
                KEY_SET_VALUE,
                None,
                &mut handle,
                None,
            )
        }
        .ok()
        .map_err(|e| format!("无法打开登录启动注册项：{e}"))?;
    } else {
        let status =
            unsafe { RegOpenKeyExW(HKEY_CURRENT_USER, RUN_KEY, None, KEY_SET_VALUE, &mut handle) };
        if status == ERROR_FILE_NOT_FOUND || status == ERROR_PATH_NOT_FOUND {
            return Ok(());
        }
        status
            .ok()
            .map_err(|e| format!("无法打开登录启动注册项：{e}"))?;
    }
    let key = RegKey(handle);
    if let Some(command) = command {
        let data: Vec<u8> = command
            .encode_utf16()
            .chain(Some(0))
            .flat_map(u16::to_le_bytes)
            .collect();
        unsafe { RegSetValueExW(key.0, RUN_NAME, None, REG_SZ, Some(&data)) }
            .ok()
            .map_err(|e| format!("无法保存登录启动注册项：{e}"))
    } else {
        let status = unsafe { RegDeleteValueW(key.0, RUN_NAME) };
        if status == ERROR_FILE_NOT_FOUND {
            return Ok(());
        }
        status
            .ok()
            .map_err(|e| format!("无法移除登录启动注册项：{e}"))
    }
}

fn shortcut_path() -> Result<PathBuf, String> {
    let result = unsafe { SHGetKnownFolderPath(&FOLDERID_Startup, KF_FLAG_DEFAULT, None) }
        .map_err(|e| format!("无法取得用户启动文件夹：{e}"))?;
    let path = unsafe { result.to_string() };
    unsafe { CoTaskMemFree(Some(result.0.cast())) };
    Ok(PathBuf::from(path.map_err(|e| format!("启动文件夹路径无效：{e}"))?).join("ClickRun.lnk"))
}

struct ShortcutInfo {
    target: String,
    arguments: String,
    working_directory: String,
}

fn shortcut_matches(shortcut: &ShortcutInfo, current: &Path) -> bool {
    same_exe(&shortcut.target, current)
        && shortcut.arguments.trim() == "--tray"
        && same_exe(
            &shortcut.working_directory,
            current.parent().unwrap_or(Path::new("")),
        )
}

fn read_shortcut(path: &Path) -> Result<Option<ShortcutInfo>, String> {
    if !path
        .try_exists()
        .map_err(|e| format!("无法读取启动快捷方式：{e}"))?
    {
        return Ok(None);
    }
    let _com = ComGuard::new()?;
    let link: IShellLinkW = unsafe { CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER) }
        .map_err(|e| format!("无法读取启动快捷方式：{e}"))?;
    let persist: IPersistFile = link
        .cast()
        .map_err(|e| format!("无法读取启动快捷方式：{e}"))?;
    let path = wide(path.as_os_str());
    unsafe { persist.Load(PCWSTR(path.as_ptr()), STGM_READ) }
        .map_err(|e| format!("启动快捷方式损坏，未修改：{e}"))?;
    let decode = |value: &[u16]| {
        let len = value.iter().position(|c| *c == 0).unwrap_or(value.len());
        String::from_utf16_lossy(&value[..len])
    };
    // 保留完整长路径/参数容量，但三个字段依次读取，共用一个 64KiB 缓冲。
    // 与原来并存的三个 Vec 相比，临时字符缓冲峰值减少 128KiB。
    let mut buffer = vec![0u16; 32768];
    unsafe { link.GetPath(&mut buffer, std::ptr::null_mut(), SLGP_RAWPATH.0 as u32) }
        .map_err(|e| format!("无法读取启动快捷方式目标：{e}"))?;
    let target = decode(&buffer);
    buffer.fill(0);
    unsafe { link.GetArguments(&mut buffer) }
        .map_err(|e| format!("无法读取启动快捷方式参数：{e}"))?;
    let arguments = decode(&buffer);
    buffer.fill(0);
    unsafe { link.GetWorkingDirectory(&mut buffer) }
        .map_err(|e| format!("无法读取启动快捷方式工作目录：{e}"))?;
    let working_directory = decode(&buffer);
    Ok(Some(ShortcutInfo {
        target,
        arguments,
        working_directory,
    }))
}

fn write_shortcut(path: &Path, current: &Path) -> Result<(), String> {
    let _com = ComGuard::new()?;
    let link: IShellLinkW = unsafe { CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER) }
        .map_err(|e| format!("无法建立启动快捷方式：{e}"))?;
    let exe = wide(current.as_os_str());
    let working = wide(current.parent().unwrap_or(Path::new("")).as_os_str());
    (|| unsafe {
        link.SetPath(PCWSTR(exe.as_ptr()))?;
        link.SetArguments(w!("--tray"))?;
        link.SetWorkingDirectory(PCWSTR(working.as_ptr()))?;
        link.SetDescription(w!("连点器 Modern"))?;
        link.SetIconLocation(PCWSTR(exe.as_ptr()), 0)?;
        Ok::<(), windows::core::Error>(())
    })()
    .map_err(|e| format!("无法设置启动快捷方式：{e}"))?;
    let persist: IPersistFile = link
        .cast()
        .map_err(|e| format!("无法保存启动快捷方式：{e}"))?;
    let temp = path.with_file_name(format!(".ClickRun-{}.tmp.lnk", std::process::id()));
    let temp_wide = wide(temp.as_os_str());
    let target_wide = wide(path.as_os_str());
    let save = unsafe { persist.Save(PCWSTR(temp_wide.as_ptr()), true) }.and_then(|_| unsafe {
        MoveFileExW(
            PCWSTR(temp_wide.as_ptr()),
            PCWSTR(target_wide.as_ptr()),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    });
    if save.is_err() {
        let _ = fs::remove_file(temp);
    }
    save.map_err(|e| format!("无法保存启动快捷方式：{e}"))
}

fn test_dir() -> Option<PathBuf> {
    std::env::var_os("CLICKRUN_TEST_SYSTEM_DIR").map(PathBuf::from)
}

#[cfg(test)]
fn simulated_enabled(dir: &Path, current: &Path) -> bool {
    simulated_state(dir, current).unwrap_or(false)
}

fn simulated_state(dir: &Path, current: &Path) -> Result<bool, String> {
    let states = ["registry-run.txt", "startup-link.txt"].map(|file| {
        match fs::read_to_string(dir.join(file)) {
            Ok(command) => Ok(executable_part(&command).is_some_and(|exe| same_exe(exe, current))),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(format!("无法读取隔离启动项：{e}")),
        }
    });
    combine_states(states)
}

fn combine_states(states: [Result<bool, String>; 2]) -> Result<bool, String> {
    match states {
        [Ok(true), _] | [_, Ok(true)] => Ok(true),
        [Ok(false), Ok(false)] => Ok(false),
        [Err(a), Err(b)] => Err(format!("{a}；{b}")),
        [Err(e), _] | [_, Err(e)] => Err(e),
    }
}

/// 任一正确入口即可确定启用。注册项已经命中时无需激活 ShellLink COM 或读盘。
/// false/读取错误仍查询另一入口，并保留原有未知状态的错误反馈。
fn state_with_shortcut(
    registry: Result<bool, String>,
    shortcut: impl FnOnce() -> Result<bool, String>,
) -> Result<bool, String> {
    match registry {
        Ok(true) => Ok(true),
        registry => combine_states([registry, shortcut()]),
    }
}

fn set_simulated(dir: &Path, current: &Path, enable: bool) -> Result<(), String> {
    fs::create_dir_all(dir).map_err(|e| format!("无法创建隔离系统目录：{e}"))?;
    let command = format!("\"{}\" --tray", current.display());
    let paths = [dir.join("registry-run.txt"), dir.join("startup-link.txt")];
    for path in &paths {
        match fs::read_to_string(path) {
            Ok(value) => {
                if enable && !executable_part(&value).is_some_and(|p| managed_exe(p, current)) {
                    return Err("同名启动入口由其他程序使用，未覆盖。".into());
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(format!("无法读取隔离启动项：{e}")),
        }
    }
    for path in &paths {
        if enable {
            if fs::read_to_string(path).ok().as_deref() != Some(command.as_str()) {
                fs::write(path, &command).map_err(|e| format!("无法保存隔离启动项：{e}"))?;
            }
        } else if fs::read_to_string(path)
            .ok()
            .is_some_and(|s| executable_part(&s).is_some_and(|p| managed_exe(p, current)))
        {
            fs::remove_file(path).map_err(|e| format!("无法移除隔离启动项：{e}"))?;
        }
    }
    Ok(())
}

/// 当无法确定系统状态时返回中文错误，调用方不能把读取失败显示成已关闭。
pub fn autostart_state() -> Result<bool, String> {
    let current = std::env::current_exe().map_err(|e| format!("无法取得程序位置：{e}"))?;
    if let Some(dir) = test_dir() {
        return simulated_state(&dir, &current);
    }
    let registry = registry_command().map(|command| {
        command.is_some_and(|command| {
            executable_part(&command).is_some_and(|exe| same_exe(exe, &current))
        })
    });
    state_with_shortcut(registry, || {
        shortcut_path()
            .and_then(|path| read_shortcut(&path))
            .map(|shortcut| shortcut.is_some_and(|shortcut| same_exe(&shortcut.target, &current)))
    })
}

/// 用户主动切换时，幂等地同时维护原版的两种登录启动入口。
pub fn set_autostart(enable: bool) -> Result<(), String> {
    let current = std::env::current_exe().map_err(|e| format!("无法取得程序位置：{e}"))?;
    let command = format!("\"{}\" --tray", current.display());
    if let Some(dir) = test_dir() {
        return set_simulated(&dir, &current, enable);
    }
    let registered = registry_command()?;
    let shortcut = shortcut_path()?;
    let linked = read_shortcut(&shortcut)?;
    let own_registry = registered
        .as_deref()
        .is_some_and(|s| executable_part(s).is_some_and(|p| managed_exe(p, &current)));
    let own_shortcut = linked
        .as_ref()
        .is_some_and(|shortcut| managed_exe(&shortcut.target, &current));
    if enable && ((registered.is_some() && !own_registry) || (linked.is_some() && !own_shortcut)) {
        return Err("同名登录启动入口由其他程序使用，未覆盖。".into());
    }
    // Attempt both channels even if one fails, then expose partial failure to the UI.
    let registry_result = if enable {
        if registered.as_deref() == Some(command.as_str()) {
            Ok(())
        } else {
            set_registry(Some(&command))
        }
    } else if own_registry {
        set_registry(None)
    } else {
        Ok(())
    };
    let shortcut_result = if enable {
        // Do not replace a correct link: Shell readers may temporarily deny replacement.
        // The second enable is a read-only no-op rather than another filesystem mutation.
        if linked
            .as_ref()
            .is_some_and(|shortcut| shortcut_matches(shortcut, &current))
        {
            Ok(())
        } else {
            write_shortcut(&shortcut, &current)
        }
    } else if own_shortcut {
        fs::remove_file(&shortcut).map_err(|e| format!("无法移除启动快捷方式：{e}"))
    } else {
        Ok(())
    };
    match (registry_result, shortcut_result) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(a), Err(b)) => Err(format!("{a}；{b}")),
        (Err(e), _) | (_, Err(e)) => Err(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn known_enabled_registry_does_not_activate_or_read_shortcut() {
        assert_eq!(
            state_with_shortcut(Ok(true), || panic!("已确定启用时不应访问快捷方式")),
            Ok(true)
        );
        let calls = std::cell::Cell::new(0);
        assert_eq!(
            state_with_shortcut(Ok(false), || {
                calls.set(calls.get() + 1);
                Ok(true)
            }),
            Ok(true)
        );
        assert_eq!(calls.get(), 1);
        assert_eq!(
            state_with_shortcut(Err("注册项无法读取".into()), || {
                calls.set(calls.get() + 1);
                Ok(true)
            }),
            Ok(true)
        );
        assert_eq!(calls.get(), 2);
        assert!(state_with_shortcut(Ok(false), || Err("快捷方式无法读取".into())).is_err());
        assert!(state_with_shortcut(Err("注册项无法读取".into()), || Ok(false)).is_err());
    }
    #[test]
    fn reused_shortcut_buffer_preserves_real_target_short_arguments_and_working_directory() {
        let dir = std::env::temp_dir().join(format!(
            "ClickRun-link-buffer-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&dir).unwrap();
        let path = dir.join("启动链接测试.lnk");
        let current = std::env::current_exe().unwrap();
        write_shortcut(&path, &current).unwrap();
        let read = read_shortcut(&path).unwrap().unwrap();
        assert!(same_exe(&read.target, &current));
        assert_eq!(read.arguments, "--tray");
        assert!(same_exe(&read.working_directory, current.parent().unwrap()));
        assert!(shortcut_matches(&read, &current));
        fs::remove_file(path).unwrap();
        fs::remove_dir(dir).unwrap();
    }
    #[test]
    fn existing_shortcut_is_reused_only_when_target_arguments_and_working_directory_match() {
        let current = Path::new(r"D:\测试 空间\连点器 Modern.exe");
        let mut shortcut = ShortcutInfo {
            target: r"d:\测试 空间\连点器 modern.exe".into(),
            arguments: "--tray".into(),
            working_directory: r"D:\测试 空间".into(),
        };
        assert!(shortcut_matches(&shortcut, current));
        shortcut.arguments = "".into();
        assert!(!shortcut_matches(&shortcut, current));
        shortcut.arguments = "--tray".into();
        shortcut.working_directory = r"D:\其他目录".into();
        assert!(!shortcut_matches(&shortcut, current));
        shortcut.working_directory = r"D:\测试 空间".into();
        shortcut.target = LEGACY_MODERN_EXE.into();
        assert!(!shortcut_matches(&shortcut, current));
    }
    #[test]
    fn parses_legacy_and_tray_commands_without_prefix_confusion() {
        assert_eq!(
            executable_part(r#""D:\含 空格\连点器.exe" --tray"#),
            Some(r"D:\含 空格\连点器.exe")
        );
        assert_eq!(
            executable_part(r"D:\ClickRun\ClickRun.Modern.exe"),
            Some(r"D:\ClickRun\ClickRun.Modern.exe")
        );
        assert!(executable_part(r"D:\x.exe-other.exe").is_none());
        assert!(managed_exe(LEGACY_MODERN_EXE, Path::new(r"D:\new.exe")));
        assert!(!managed_exe(r"F:\unrelated.exe", Path::new(r"D:\new.exe")));
    }

    #[test]
    fn isolated_registration_is_idempotent_and_preserves_foreign_entries() {
        let dir = std::env::temp_dir().join(format!(
            "ClickRun-system-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let current = Path::new(r"D:\测试 空间\连点器 Modern.exe");
        assert!(!simulated_enabled(&dir, current));
        set_simulated(&dir, current, true).unwrap();
        set_simulated(&dir, current, true).unwrap();
        assert!(simulated_enabled(&dir, current));
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 2);
        assert!(
            fs::read_to_string(dir.join("registry-run.txt"))
                .unwrap()
                .ends_with(" --tray")
        );
        set_simulated(&dir, current, false).unwrap();
        set_simulated(&dir, current, false).unwrap();
        assert!(!simulated_enabled(&dir, current));
        let foreign = r#""F:\其他软件.exe" --background"#;
        fs::write(dir.join("registry-run.txt"), foreign).unwrap();
        assert!(set_simulated(&dir, current, true).is_err());
        set_simulated(&dir, current, false).unwrap();
        assert_eq!(
            fs::read_to_string(dir.join("registry-run.txt")).unwrap(),
            foreign
        );
        fs::write(dir.join("registry-run.txt"), LEGACY_MODERN_EXE).unwrap();
        set_simulated(&dir, current, true).unwrap();
        assert!(simulated_enabled(&dir, current));
        set_simulated(&dir, current, false).unwrap();
        fs::remove_dir(dir).unwrap();
    }

    #[test]
    fn state_read_errors_are_not_reported_as_disabled() {
        assert!(combine_states([Ok(false), Err("读取失败".into())]).is_err());
        assert!(combine_states([Err("读取失败".into()), Ok(false)]).is_err());
        assert_eq!(
            combine_states([Ok(true), Err("另一入口读取失败".into())]),
            Ok(true)
        );
        assert_eq!(combine_states([Ok(false), Ok(false)]), Ok(false));
    }
}
