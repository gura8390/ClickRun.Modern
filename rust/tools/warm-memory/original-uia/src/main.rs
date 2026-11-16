//! Isolated UIA warm-up client. No real input, hotkey, Start, or legacy Lock action.
#![allow(unsafe_op_in_unsafe_fn)]
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    thread::sleep,
    time::{Duration, Instant},
};
use windows::{
    Win32::{Foundation::HWND as ComHwnd, System::Com::*, UI::Accessibility::*},
    core::{BSTR, Interface},
};
use windows_sys::Win32::{Foundation::*, System::Threading::*, UI::WindowsAndMessaging::*};

const PROJECT: &str = r"D:\codex\01-桌面工具\LmDmQi";
struct ComGuard;
impl Drop for ComGuard {
    fn drop(&mut self) {
        unsafe { CoUninitialize() }
    }
}
struct Handle(HANDLE);
impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}
fn unsigned(v: &Value) -> Result<u64> {
    v.as_u64()
        .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
        .context("缺少无符号整数")
}
fn title(h: HWND, class: bool) -> String {
    let mut text = [0u16; 1024];
    let n = unsafe {
        if class {
            GetClassNameW(h, text.as_mut_ptr(), 1024)
        } else {
            GetWindowTextW(h, text.as_mut_ptr(), 1024)
        }
    };
    String::from_utf16_lossy(&text[..n.max(0) as usize])
}
struct Windows {
    pid: u32,
    handles: Vec<isize>,
}
unsafe extern "system" fn enumerate(h: HWND, data: LPARAM) -> i32 {
    let context = &mut *(data as *mut Windows);
    let mut pid = 0;
    GetWindowThreadProcessId(h, &mut pid);
    if pid == context.pid {
        context.handles.push(h as isize);
    }
    1
}
struct Client {
    client: IUIAutomation,
    _com: ComGuard,
    pid: u32,
    hwnd: isize,
    expected: String,
    creation: u64,
    legacy: bool,
    fixture: Option<String>,
}
impl Client {
    fn new(context: &Value) -> Result<Self> {
        let pid = unsigned(&context["ProcessId"])? as u32;
        let hwnd = context["WindowHandle"]
            .as_i64()
            .or_else(|| {
                context["WindowHandle"]
                    .as_str()
                    .and_then(|s| s.parse().ok())
            })
            .context("缺少WindowHandle")? as isize;
        let expected = context["Executable"]
            .as_str()
            .context("缺少Executable")?
            .to_string();
        let lower = expected.to_lowercase();
        ensure!(
            lower.starts_with(&format!("{}\\verification\\", PROJECT.to_lowercase()))
                || lower == r"d:\codex\clickrun\legacy\clickrun.exe"
                || lower == r"d:\codex\clickrun\clickrun.modern\publish\clickrun.modern.exe",
            "目标路径不在测试白名单"
        );
        let legacy = context["Flavor"].as_str().context("缺少Flavor")? == "LegacyC";
        if !legacy {
            ensure!(context["Flavor"] == "Rust", "未知Flavor");
            let fixture = context["Fixture"]
                .as_str()
                .context("Rust测试需要Fixture")?
                .to_lowercase();
            let appdata = context["AppData"]
                .as_str()
                .context("Rust测试需要隔离AppData")?
                .to_lowercase();
            ensure!(
                fixture.starts_with(&format!("{}\\verification\\", PROJECT.to_lowercase()))
                    && appdata.starts_with(&format!("{}\\verification\\", PROJECT.to_lowercase())),
                "Rust暖机只允许工程verification下隔离配置和APPDATA"
            );
        }
        let creation = unsigned(&context["CreationFiletime"])?;
        unsafe {
            CoInitializeEx(None, COINIT_MULTITHREADED).ok()?;
        }
        let guard = ComGuard;
        let client2: IUIAutomation2 =
            unsafe { CoCreateInstance(&CUIAutomation8, None, CLSCTX_INPROC_SERVER)? };
        unsafe {
            client2.SetConnectionTimeout(2000)?;
            client2.SetTransactionTimeout(2000)?;
        }
        let result = Self {
            client: client2.cast()?,
            _com: guard,
            pid,
            hwnd,
            expected,
            creation,
            legacy,
            fixture: context["Fixture"].as_str().map(str::to_owned),
        };
        result.check()?;
        Ok(result)
    }
    fn check(&self) -> Result<Handle> {
        let h = unsafe {
            OpenProcess(
                PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE,
                0,
                self.pid,
            )
        };
        ensure!(!h.is_null(), "无法读取测试进程");
        let h = Handle(h);
        let mut text = vec![0u16; 32768];
        let mut n = text.len() as u32;
        ensure!(
            unsafe { QueryFullProcessImageNameW(h.0, 0, text.as_mut_ptr(), &mut n) } != 0,
            "无法读取进程路径"
        );
        ensure!(
            String::from_utf16_lossy(&text[..n as usize]).eq_ignore_ascii_case(&self.expected),
            "PID路径变化"
        );
        let (mut made, mut exited, mut kernel, mut user) = (
            FILETIME::default(),
            FILETIME::default(),
            FILETIME::default(),
            FILETIME::default(),
        );
        ensure!(
            unsafe { GetProcessTimes(h.0, &mut made, &mut exited, &mut kernel, &mut user) } != 0,
            "无法读取创建时间"
        );
        ensure!(
            (((made.dwHighDateTime as u64) << 32) | made.dwLowDateTime as u64) == self.creation,
            "PID创建时间变化"
        );
        let mut owner = 0;
        ensure!(
            unsafe { IsWindow(self.hwnd as _) } != 0
                && unsafe { GetWindowThreadProcessId(self.hwnd as _, &mut owner) } != 0
                && owner == self.pid,
            "HWND归属变化"
        );
        ensure!(
            title(self.hwnd as _, true)
                == if self.legacy {
                    "WND"
                } else {
                    "ClickRun.Rust.Main"
                },
            "主窗类不匹配"
        );
        Ok(h)
    }
    fn root(&self) -> Result<IUIAutomationElement> {
        self.check()?;
        unsafe {
            self.client
                .ElementFromHandle(ComHwnd(self.hwnd as _))
                .map_err(Into::into)
        }
    }
    fn descendants(&self, root: &IUIAutomationElement) -> Result<Vec<IUIAutomationElement>> {
        let all =
            unsafe { root.FindAll(TreeScope_Descendants, &self.client.CreateTrueCondition()?)? };
        let n = unsafe { all.Length()? };
        ensure!((0..=3000).contains(&n), "树节点超限");
        let mut rows = vec![];
        for i in 0..n {
            let e = unsafe { all.GetElement(i)? };
            if unsafe { e.CurrentProcessId()? } == self.pid as i32 {
                rows.push(e);
            }
        }
        Ok(rows)
    }
    fn find(
        &self,
        root: &IUIAutomationElement,
        id: &str,
        name: Option<&str>,
    ) -> Result<IUIAutomationElement> {
        let mut found = vec![];
        for e in self.descendants(root)? {
            if unsafe { e.CurrentAutomationId()?.to_string() } == id
                && (name.is_none()
                    || Some(unsafe { e.CurrentName()?.to_string() }).as_deref() == name)
            {
                found.push(e);
            }
        }
        ensure!(
            found.len() == 1,
            "控件{id}/{name:?}应唯一，实际{}个",
            found.len()
        );
        Ok(found.remove(0))
    }
    fn e(&self, id: &str) -> Result<IUIAutomationElement> {
        self.find(&self.root()?, id, None)
    }
    fn normal(&self) -> Result<()> {
        let p: IUIAutomationWindowPattern =
            unsafe { self.root()?.GetCurrentPatternAs(UIA_WindowPatternId)? };
        unsafe {
            p.SetWindowVisualState(WindowVisualState_Normal)?;
        }
        wait(|| {
            Ok(unsafe { IsWindowVisible(self.hwnd as _) } != 0
                && unsafe { IsIconic(self.hwnd as _) } == 0)
        })
    }
    fn stopped(&self) -> Result<Value> {
        if self.legacy {
            let lock = self.e("3305")?;
            ensure!(
                unsafe { lock.CurrentName()?.to_string() } == "锁定当前配置",
                "原版已被锁定，拒绝继续"
            );
            ensure!(
                unsafe { self.e("3303")?.CurrentIsEnabled()?.as_bool() },
                "原版输入框未启用"
            );
            Ok(json!({"原版未锁定":true,"未调用锁定及热键":true}))
        } else {
            ensure!(
                unsafe { self.e("401")?.CurrentName()?.to_string() }.contains("已停止"),
                "Rust未停止"
            );
            ensure!(
                unsafe { self.e("210")?.CurrentName()?.to_string() } == "开始连点",
                "Rust按钮状态不是停止"
            );
            Ok(json!({"已停止":true,"未调用开始":true}))
        }
    }
    fn snapshot(&self) -> Result<Value> {
        let root = self.root()?;
        let mut nodes = vec![row(&root)];
        for e in self.descendants(&root)? {
            nodes.push(row(&e));
        }
        Ok(
            json!({"可见":unsafe{IsWindowVisible(self.hwnd as _)}!=0,"最小化":unsafe{IsIconic(self.hwnd as _)}!=0,"节点":nodes}),
        )
    }
    fn value(&self, e: &IUIAutomationElement) -> Result<String> {
        unsafe {
            e.GetCurrentPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId)?
                .CurrentValue()
                .map(|v| v.to_string())
                .map_err(Into::into)
        }
    }
    fn set(&self, e: &IUIAutomationElement, value: &str, steps: &mut Vec<Value>) -> Result<()> {
        self.check()?;
        let p: IUIAutomationValuePattern = unsafe { e.GetCurrentPatternAs(UIA_ValuePatternId)? };
        ensure!(!unsafe { p.CurrentIsReadOnly()?.as_bool() }, "输入框为只读");
        unsafe {
            p.SetValue(&BSTR::from(value))?;
        }
        pause();
        ensure!(self.value(e)? == value, "Value.SetValue未读回");
        steps.push(json!({"模式":"Value.SetValue","请求":value,"实际":row(e)}));
        Ok(())
    }
    fn select(&self, e: &IUIAutomationElement, steps: &mut Vec<Value>) -> Result<()> {
        self.check()?;
        let p: IUIAutomationSelectionItemPattern =
            unsafe { e.GetCurrentPatternAs(UIA_SelectionItemPatternId)? };
        unsafe {
            p.Select()?;
        }
        pause();
        ensure!(
            unsafe { p.CurrentIsSelected()?.as_bool() },
            "SelectionItem.Select未选中"
        );
        steps.push(json!({"模式":"SelectionItem.Select","实际":row(e)}));
        Ok(())
    }
    fn combo(&self, e: &IUIAutomationElement, steps: &mut Vec<Value>) -> Result<()> {
        self.check()?;
        let p: IUIAutomationExpandCollapsePattern =
            unsafe { e.GetCurrentPatternAs(UIA_ExpandCollapsePatternId)? };
        unsafe {
            p.Expand()?;
        }
        pause();
        let expanded = unsafe { p.CurrentExpandCollapseState()?.0 };
        steps.push(json!({"模式":"ExpandCollapse.Expand","实际状态":expanded,"实际":row(e)}));
        unsafe {
            p.Collapse()?;
        }
        pause();
        ensure!(
            unsafe { p.CurrentExpandCollapseState()? } == ExpandCollapseState_Collapsed,
            "下拉框未收起"
        );
        steps.push(json!({"模式":"ExpandCollapse.Collapse","实际":row(e)}));
        Ok(())
    }
    fn invoke(&self, e: &IUIAutomationElement, steps: &mut Vec<Value>) -> Result<()> {
        self.check()?;
        ensure!(!self.legacy, "原版禁止所有Invoke动作");
        let id = unsafe { e.CurrentAutomationId()?.to_string() };
        ensure!(
            [
                "100", "101", "102", "208", "209", "301", "502", "503", "212"
            ]
            .contains(&id.as_str()),
            "Invoke白名单未包含{id}"
        );
        let before = row(e);
        let p: IUIAutomationInvokePattern = unsafe { e.GetCurrentPatternAs(UIA_InvokePatternId)? };
        unsafe {
            p.Invoke()?;
        }
        pause();
        steps.push(json!({"模式":"Invoke.Invoke","动作前":before}));
        Ok(())
    }
    fn dialog(&self) -> Result<IUIAutomationElement> {
        let mut windows = Windows {
            pid: self.pid,
            handles: vec![],
        };
        unsafe {
            EnumWindows(Some(enumerate), &mut windows as *mut _ as isize);
        }
        let all: Vec<_> = windows
            .handles
            .into_iter()
            .filter(|h| title(*h as _, true) == "ClickRun.Rust.Dialog")
            .collect();
        ensure!(all.len() == 1, "应有唯一弹窗，实际{}", all.len());
        unsafe {
            self.client
                .ElementFromHandle(ComHwnd(all[0] as _))
                .map_err(Into::into)
        }
    }
    fn common(&self, steps: &mut Vec<Value>) -> Result<Value> {
        self.normal()?;
        let stopped = self.stopped()?;
        steps.push(json!({"模式":"Window.SetWindowVisualState(Normal)","实际":stopped,"快照":self.snapshot()?}));
        let root = self.root()?;
        let (left, right, edit, combo) = if self.legacy {
            (
                self.find(&root, "3301", Some("鼠标左键"))?,
                self.find(&root, "3301", Some("鼠标右键"))?,
                self.e("3303")?,
                self.e("3302")?,
            )
        } else {
            (
                self.e("201")?,
                self.e("220")?,
                self.e("203")?,
                self.e("207")?,
            )
        };
        let initial = self.value(&edit)?;
        let parsed: i32 = initial.parse().context("启动间隔不是整数")?;
        ensure!((1..=10000).contains(&parsed), "共同间隔范围必须1–10000");
        let selected = unsafe {
            left.GetCurrentPatternAs::<IUIAutomationSelectionItemPattern>(
                UIA_SelectionItemPatternId,
            )?
            .CurrentIsSelected()?
            .as_bool()
        };
        let initial_button = if selected { "左键" } else { "右键" };
        self.select(&right, steps)?;
        self.select(&left, steps)?;
        if !selected {
            self.select(&right, steps)?;
        }
        self.set(&edit, if parsed == 250 { "100" } else { "250" }, steps)?;
        self.set(&edit, &initial, steps)?;
        self.combo(&combo, steps)?;
        unsafe {
            edit.SetFocus()?;
        }
        pause();
        let focused = unsafe { edit.CurrentHasKeyboardFocus()?.as_bool() };
        ensure!(focused, "SetFocus后间隔框未拥有焦点");
        let text = match unsafe {
            edit.GetCurrentPatternAs::<IUIAutomationTextPattern>(UIA_TextPatternId)
        } {
            Ok(pattern) => {
                let text = unsafe { pattern.DocumentRange()?.GetText(-1)?.to_string() };
                ensure!(text.trim_end_matches(['\r', '\n']) == initial, "TextPattern文本未匹配初始间隔");
                json!({"模式":"Text.DocumentRange.GetText","文本":text})
            }
            Err(_) => json!({"TextPattern可用":false,"已读取模式":"Value.CurrentValue"}),
        };
        steps.push(json!({"模式":"Element.SetFocus/Name/Value读取","文本读取":text,"HasKeyboardFocus":focused,"实际":row(&edit)}));
        let finish = self.stopped()?;
        ensure!(
            unsafe { IsWindowVisible(self.hwnd as _) } != 0
                && unsafe { IsIconic(self.hwnd as _) } == 0,
            "结束主窗不可见或被最小化"
        );
        Ok(
            json!({"启动间隔":initial,"最终间隔":self.value(&edit)?,"启动及最终按键":initial_button,"状态":finish,"最终快照":self.snapshot()?}),
        )
    }
    fn select_profile_name(&self, name: &str, steps: &mut Vec<Value>) -> Result<()> {
        self.check()?;
        let combo = self.e("207")?;
        let expand: IUIAutomationExpandCollapsePattern =
            unsafe { combo.GetCurrentPatternAs(UIA_ExpandCollapsePatternId)? };
        unsafe { expand.Expand()?; }
        pause();
        let mut candidates = self.descendants(&combo)?;
        // Native Win32 ComboLBox may be a same-process top-level popup rather
        // than a descendant of the ComboBox. Query that popup through UIA too.
        let desktop = unsafe { self.client.GetRootElement()? };
        let tops = unsafe { desktop.FindAll(TreeScope_Children, &self.client.CreateTrueCondition()?)? };
        for index in 0..unsafe { tops.Length()? } {
            let top = unsafe { tops.GetElement(index)? };
            if unsafe { top.CurrentProcessId().ok() } == Some(self.pid as i32) {
                candidates.extend(self.descendants(&top)?);
            }
        }
        let mut patterns = vec![];
        for element in candidates {
            if unsafe { element.CurrentName().ok().map(|value| value.to_string()) }.as_deref() == Some(name) {
                if let Ok(pattern) = unsafe {
                    element.GetCurrentPatternAs::<IUIAutomationSelectionItemPattern>(UIA_SelectionItemPatternId)
                } {
                    patterns.push(pattern);
                }
            }
        }
        ensure!(!patterns.is_empty(), "新方案未暴露SelectionItem模式");
        unsafe { patterns[0].Select()?; }
        let _ = unsafe { expand.Collapse() };
        pause();
        let actual = self.value(&combo)?;
        ensure!(actual == name, "Combo Value.CurrentValue未确认本次方案，拒绝删除其他方案");
        steps.push(json!({"模式":"ExpandCollapse.Expand → 唯一Name的SelectionItem.Select → Collapse → Value.CurrentValue","请求方案":name,"实际方案":actual,"同名语义节点数":patterns.len(),"实际":row(&combo)}));
        Ok(())
    }
    fn fixture_profile_count(&self) -> Result<usize> {
        let fixture = self.fixture.as_deref().context("完整暖机需要隔离Fixture")?;
        let config: Value = serde_json::from_slice(&fs::read(std::path::Path::new(fixture).join("config.json"))?)?;
        Ok(config["profiles"].as_array().context("隔离配置缺少profiles数组")?.len())
    }
    fn full(&self, steps: &mut Vec<Value>) -> Result<Value> {
        ensure!(!self.legacy, "完整功能暖机仅适用于Rust隔离配置");
        ensure!(self.fixture_profile_count()? == 0, "完整暖机要求新隔离配置profiles初始为空");
        steps.push(json!({"检查":"只读隔离Fixture配置","初始profiles数量":0,"固定方案名称":"内存暖机-1234567890"}));
        let before = self.common(steps)?;
        let interval = self.value(&self.e("203")?)?;
        let slider = self.e("223")?;
        let range: IUIAutomationRangeValuePattern =
            unsafe { slider.GetCurrentPatternAs(UIA_RangeValuePatternId)? };
        for v in [321.0, interval.parse::<f64>()?] {
            unsafe {
                range.SetValue(v)?;
            }
            pause();
            ensure!(unsafe { range.CurrentValue()? } == v, "滑块毫秒值未读回");
            steps.push(json!({"模式":"RangeValue.SetValue","实际":unsafe{range.CurrentValue()?}}));
        }
        let limited = self.e("222")?;
        let was_limited = unsafe {
            limited
                .GetCurrentPatternAs::<IUIAutomationSelectionItemPattern>(
                    UIA_SelectionItemPatternId,
                )?
                .CurrentIsSelected()?
                .as_bool()
        };
        let limit = self.value(&self.e("206")?)?;
        self.select(&limited, steps)?;
        self.set(&self.e("206")?, "7", steps)?;
        self.set(&self.e("206")?, &limit, steps)?;
        if !was_limited {
            self.select(&self.e("205")?, steps)?;
        }
        let unique = "内存暖机-1234567890";
        self.invoke(&self.e("208")?, steps)?;
        let dialog = self.dialog()?;
        self.set(&self.find(&dialog, "501", None)?, unique, steps)?;
        self.invoke(&self.find(&dialog, "502", None)?, steps)?;
        wait(|| Ok(self.dialog().is_err()))?;
        self.select_profile_name(unique, steps)?;
        self.invoke(&self.e("209")?, steps)?;
        wait(|| Ok(self.fixture_profile_count()? == 0))?;
        steps.push(json!({"动作":"已创建并删除本次唯一临时方案","名称":unique}));
        self.invoke(&self.e("301")?, steps)?;
        let dialog = self.dialog()?;
        steps.push(json!({"动作":"热键弹窗只读","节点":self.descendants(&dialog)?.iter().map(row).collect::<Vec<_>>()}));
        self.invoke(&self.find(&dialog, "503", None)?, steps)?;
        wait(|| Ok(self.dialog().is_err()))?;
        self.invoke(&self.e("101")?, steps)?;
        let mut settings = vec![];
        for id in ["302", "303", "304", "305"] {
            let e = self.e(id)?;
            let p: IUIAutomationTogglePattern =
                unsafe { e.GetCurrentPatternAs(UIA_TogglePatternId)? };
            settings.push(json!({"控件":row(&e),"只读勾选状态":unsafe{p.CurrentToggleState()?.0}}));
        }
        steps.push(json!({"动作":"设置只读，不操作启动项或其他开关","设置":settings,"快照":self.snapshot()?}));
        self.invoke(&self.e("102")?, steps)?;
        steps.push(json!({"动作":"关于页只读","快照":self.snapshot()?}));
        self.invoke(&self.e("100")?, steps)?;
        let after = self.common(steps)?;
        Ok(
            json!({"完整暖机前共同状态":before,"完整暖机后共同状态":after,"未调用开始或任何热键":true,"未切换系统设置":true}),
        )
    }
    fn exit(&self, steps: &mut Vec<Value>) -> Result<Value> {
        self.normal()?;
        self.stopped()?;
        let process = self.check()?;
        if self.legacy {
            let p: IUIAutomationWindowPattern =
                unsafe { self.root()?.GetCurrentPatternAs(UIA_WindowPatternId)? };
            unsafe {
                p.Close()?;
            }
            steps.push(json!({"模式":"Window.Close","原版未锁定":true}));
        } else {
            self.invoke(&self.e("212")?, steps)?;
        }
        ensure!(
            unsafe { WaitForSingleObject(process.0, 5000) } == WAIT_OBJECT_0,
            "正常退出超时，未强制终止"
        );
        Ok(json!({"正常退出":true,"强制结束":false}))
    }
}
fn row(e: &IUIAutomationElement) -> Value {
    unsafe {
        let value = e
            .GetCurrentPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId)
            .ok()
            .and_then(|p| p.CurrentValue().ok())
            .map(|v| v.to_string());
        let selected = e
            .GetCurrentPatternAs::<IUIAutomationSelectionItemPattern>(UIA_SelectionItemPatternId)
            .ok()
            .and_then(|p| p.CurrentIsSelected().ok())
            .map(|v| v.as_bool());
        json!({"Name":e.CurrentName().ok().map(|v|v.to_string()),"AutomationId":e.CurrentAutomationId().ok().map(|v|v.to_string()),"ControlType":e.CurrentControlType().ok().map(|v|v.0),"Value":value,"IsSelected":selected,"IsEnabled":e.CurrentIsEnabled().ok().map(|v|v.as_bool()),"IsOffscreen":e.CurrentIsOffscreen().ok().map(|v|v.as_bool())})
    }
}
fn pause() {
    sleep(Duration::from_millis(80));
}
fn wait(mut f: impl FnMut() -> Result<bool>) -> Result<()> {
    let start = Instant::now();
    while !f()? {
        ensure!(start.elapsed() < Duration::from_secs(3), "等待状态超时");
        pause();
    }
    Ok(())
}
fn main() {
    let mut args = BTreeMap::new();
    let mut input = std::env::args().skip(1);
    while let Some(key) = input.next() {
        if let Some(value) = input.next() {
            args.insert(key, value);
        } else {
            eprintln!("参数缺少值");
            std::process::exit(2);
        }
    }
    let mut steps = vec![];
    let outcome = (|| -> Result<Value> {
        let context: Value =
            serde_json::from_slice(&fs::read(args.get("--context").context("需要--context")?)?)?;
        let action = args.get("--action").map(String::as_str).unwrap_or("probe");
        let c = Client::new(&context)?;
        let result = match action {
            "probe" => json!({"停止状态":c.stopped()?,"快照":c.snapshot()?}),
            "common" => c.common(&mut steps)?,
            "full" => c.full(&mut steps)?,
            "exit" => c.exit(&mut steps)?,
            _ => anyhow::bail!("未知动作"),
        };
        Ok(
            json!({"Passed":true,"ProcessId":c.pid,"CreationFiletime":c.creation,"Action":action,"Backend":"CUIAutomation8","Steps":steps,"Result":result}),
        )
    })();
    match outcome {
        Ok(v) => println!("{}", serde_json::to_string_pretty(&v).unwrap()),
        Err(e) => {
            println!(
                "{}",
                serde_json::to_string_pretty(
                    &json!({"Passed":false,"Error":format!("{e:#}"),"Steps":steps})
                )
                .unwrap()
            );
            std::process::exit(1)
        }
    }
}
