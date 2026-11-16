//! 使用 CUIAutomation8 的真实系统语义模式验收。无坐标、键鼠模拟或控件消息替代。
//! 参考并复用工作区 native-uia-probe/native-uia-actions 的 COM 客户端建立方式。
#![allow(unsafe_op_in_unsafe_fn)]
use anyhow::{Context, Result, bail, ensure};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    ptr::null_mut,
    thread::sleep,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use windows::{
    Win32::{Foundation::HWND as ComHwnd, System::Com::*, UI::Accessibility::*},
    core::{BSTR, Interface},
};
use windows_sys::Win32::{
    Foundation::*,
    Graphics::Gdi::*,
    System::Threading::*,
    UI::{HiDpi::*, Input::KeyboardAndMouse::*, WindowsAndMessaging::*},
};

const PROJECT: &str = r"D:\codex\01-桌面工具\LmDmQi";
const CLASS: &str = "ClickRun.Rust.Main";

fn elapsed_ms(start: Instant) -> f64 {
    start.elapsed().as_secs_f64() * 1000.0
}
struct ComGuard;
impl Drop for ComGuard {
    fn drop(&mut self) {
        unsafe {
            CoUninitialize();
        }
    }
}
struct Client {
    client: IUIAutomation,
    _com: ComGuard,
    pid: u32,
    hwnd: isize,
    creation: u64,
}
impl Client {
    fn new(pid: u32, hwnd: isize) -> Result<Self> {
        unsafe {
            CoInitializeEx(None, COINIT_MULTITHREADED).ok()?;
        }
        let guard = ComGuard;
        let automation: IUIAutomation2 =
            unsafe { CoCreateInstance(&CUIAutomation8, None, CLSCTX_INPROC_SERVER)? };
        unsafe {
            automation.SetConnectionTimeout(2000)?;
            automation.SetTransactionTimeout(2000)?;
        }
        let creation = native_identity(pid, hwnd)?;
        Ok(Self {
            client: automation.cast()?,
            _com: guard,
            pid,
            hwnd,
            creation,
        })
    }
    fn check(&self) -> Result<()> {
        ensure!(
            native_identity(self.pid, self.hwnd)? == self.creation,
            "目标进程身份已变化"
        );
        Ok(())
    }
    fn root(&self) -> Result<IUIAutomationElement> {
        self.check()?;
        let root = unsafe { self.client.ElementFromHandle(ComHwnd(self.hwnd as _))? };
        ensure!(
            unsafe { root.CurrentProcessId()? } == self.pid as i32,
            "无障碍根属于其他进程"
        );
        Ok(root)
    }
    fn normal(&self) -> Result<()> {
        let root = self.root()?;
        let window: IUIAutomationWindowPattern =
            unsafe { root.GetCurrentPatternAs(UIA_WindowPatternId)? };
        unsafe {
            window.SetWindowVisualState(WindowVisualState_Normal)?;
        }
        wait_until(
            || {
                Ok(unsafe { IsWindowVisible(self.hwnd as _) } != 0
                    && unsafe { IsIconic(self.hwnd as _) } == 0)
            },
            Duration::from_secs(2),
        )?;
        Ok(())
    }
    fn children(
        &self,
        root: &IUIAutomationElement,
        scope: TreeScope,
    ) -> Result<Vec<IUIAutomationElement>> {
        unsafe {
            let all = root.FindAll(scope, &self.client.CreateTrueCondition()?)?;
            let count = all.Length()?;
            ensure!((0..=3000).contains(&count), "无障碍树超出测试上限");
            (0..count)
                .map(|i| all.GetElement(i).map_err(Into::into))
                .collect()
        }
    }
    fn element(&self, id: &str) -> Result<IUIAutomationElement> {
        self.in_root(&self.root()?, id)
    }
    fn in_root(&self, root: &IUIAutomationElement, id: &str) -> Result<IUIAutomationElement> {
        let mut found = vec![];
        for element in self.children(root, TreeScope_Descendants)? {
            if unsafe { element.CurrentProcessId()? } == self.pid as i32
                && unsafe { element.CurrentAutomationId()?.to_string() } == id
            {
                found.push(element);
            }
        }
        ensure!(
            found.len() == 1,
            "自动化 ID {id} 应唯一，实际 {} 个",
            found.len()
        );
        Ok(found.remove(0))
    }
    fn invoke(&self, root: Option<&IUIAutomationElement>, id: &str) -> Result<()> {
        self.check()?;
        let element = match root {
            Some(root) => self.in_root(root, id)?,
            None => self.element(id)?,
        };
        ensure!(
            unsafe { element.CurrentIsEnabled()?.as_bool() },
            "控件 {id} 不可用"
        );
        let pattern: IUIAutomationInvokePattern =
            unsafe { element.GetCurrentPatternAs(UIA_InvokePatternId)? };
        unsafe {
            element.SetFocus()?;
        }
        sleep(Duration::from_millis(20));
        unsafe {
            pattern.Invoke()?;
        }
        sleep(Duration::from_millis(80));
        Ok(())
    }
    fn set(&self, root: Option<&IUIAutomationElement>, id: &str, value: &str) -> Result<()> {
        self.check()?;
        let element = match root {
            Some(root) => self.in_root(root, id)?,
            None => self.element(id)?,
        };
        let pattern: IUIAutomationValuePattern =
            unsafe { element.GetCurrentPatternAs(UIA_ValuePatternId)? };
        ensure!(
            !unsafe { pattern.CurrentIsReadOnly()?.as_bool() },
            "控件 {id} 为只读"
        );
        unsafe {
            pattern.SetValue(&BSTR::from(value))?;
        }
        ensure!(
            unsafe { pattern.CurrentValue()?.to_string() } == value,
            "文本值未读回"
        );
        Ok(())
    }
    fn toggle(&self, id: &str) -> Result<i32> {
        self.check()?;
        let p: IUIAutomationTogglePattern =
            unsafe { self.element(id)?.GetCurrentPatternAs(UIA_TogglePatternId)? };
        unsafe {
            p.Toggle()?;
        }
        sleep(Duration::from_millis(80));
        Ok(unsafe { p.CurrentToggleState()?.0 })
    }
    fn select(&self, root: Option<&IUIAutomationElement>, id: &str, name: &str) -> Result<()> {
        self.check()?;
        let combo = match root {
            Some(root) => self.in_root(root, id)?,
            None => self.element(id)?,
        };
        if let Ok(radio) = unsafe {
            combo.GetCurrentPatternAs::<IUIAutomationSelectionItemPattern>(
                UIA_SelectionItemPatternId,
            )
        } {
            unsafe {
                radio.Select()?;
            }
            sleep(Duration::from_millis(80));
            ensure!(
                unsafe { radio.CurrentIsSelected()?.as_bool() },
                "单选按钮 {id} 未读回选中状态"
            );
            return Ok(());
        }
        let expand: IUIAutomationExpandCollapsePattern =
            unsafe { combo.GetCurrentPatternAs(UIA_ExpandCollapsePatternId)? };
        unsafe {
            expand.Expand()?;
        }
        sleep(Duration::from_millis(80));
        let mut candidates = self.children(&combo, TreeScope_Descendants)?;
        if !candidates.iter().any(|e| unsafe {
            e.CurrentName().ok().is_some_and(|n| n.to_string() == name)
                && e.GetCurrentPatternAs::<IUIAutomationSelectionItemPattern>(
                    UIA_SelectionItemPatternId,
                )
                .is_ok()
        }) {
            // Win32 下拉列表可能是同进程顶层弹窗，仍只经 UIA 模式选择。
            let desktop = unsafe { self.client.GetRootElement()? };
            for top in self.children(&desktop, TreeScope_Children)? {
                if unsafe { top.CurrentProcessId().ok() } == Some(self.pid as i32) {
                    candidates.extend(self.children(&top, TreeScope_Descendants)?);
                }
            }
        }
        let mut matches = vec![];
        for element in candidates {
            if unsafe { element.CurrentProcessId()? } == self.pid as i32
                && unsafe { element.CurrentName()?.to_string() } == name
            {
                if let Ok(pattern) = unsafe {
                    element.GetCurrentPatternAs::<IUIAutomationSelectionItemPattern>(
                        UIA_SelectionItemPatternId,
                    )
                } {
                    matches.push(pattern);
                }
            }
        }
        ensure!(!matches.is_empty(), "下拉选项“{name}”未暴露选择模式");
        unsafe {
            matches[0].Select()?;
        }
        let _ = unsafe { expand.Collapse() };
        sleep(Duration::from_millis(80));
        if let Ok(selection) = unsafe {
            combo.GetCurrentPatternAs::<IUIAutomationSelectionPattern>(UIA_SelectionPatternId)
        } {
            let chosen = unsafe { selection.GetCurrentSelection()? };
            ensure!(
                unsafe { chosen.Length()? } == 1
                    && unsafe { chosen.GetElement(0)?.CurrentName()?.to_string() } == name,
                "选择状态未正确读回"
            );
        } else {
            let value: IUIAutomationValuePattern =
                unsafe { combo.GetCurrentPatternAs(UIA_ValuePatternId)? };
            ensure!(
                unsafe { value.CurrentValue()?.to_string() } == name,
                "选择后的组合框值未正确读回"
            );
        }
        Ok(())
    }
    fn text(&self, id: &str) -> Result<String> {
        Ok(unsafe { self.element(id)?.CurrentName()?.to_string() })
    }
    fn count(&self, id: &str) -> Result<u64> {
        let digits: String = self
            .text(id)?
            .chars()
            .filter(char::is_ascii_digit)
            .collect();
        Ok(digits.parse().context("统计控件未暴露可读取的数字")?)
    }
    fn value(&self, id: &str) -> Result<String> {
        let p: IUIAutomationValuePattern =
            unsafe { self.element(id)?.GetCurrentPatternAs(UIA_ValuePatternId)? };
        Ok(unsafe { p.CurrentValue()?.to_string() })
    }
    fn range(&self, id: &str, value: f64) -> Result<Value> {
        self.check()?;
        let element = self.element(id)?;
        let p: IUIAutomationRangeValuePattern =
            unsafe { element.GetCurrentPatternAs(UIA_RangeValuePatternId)? };
        ensure!(
            !unsafe { p.CurrentIsReadOnly()?.as_bool() },
            "范围控件 {id} 为只读"
        );
        let min = unsafe { p.CurrentMinimum()? };
        let max = unsafe { p.CurrentMaximum()? };
        ensure!(value >= min && value <= max, "范围测试值越界");
        unsafe {
            p.SetValue(value)?;
        }
        let actual = unsafe { p.CurrentValue()? };
        ensure!(
            (actual - value).abs() < 0.001,
            "范围控件 {id} 未正确读回：请求{value}，实际{actual}，最小{min}，最大{max}"
        );
        Ok(json!({"自动化ID":id,"最小值":min,"最大值":max,"读回":unsafe{p.CurrentValue()?}}))
    }
    fn show_main(&self) -> Result<()> {
        if let Ok(back) = self.element("100") {
            if unsafe { back.CurrentIsOffscreen().ok().is_some_and(|v| !v.as_bool()) } {
                self.invoke(None, "100")?;
            }
        }
        self.element("203").context("连点主页面未显示")?;
        Ok(())
    }
    fn dialog(&self) -> Result<IUIAutomationElement> {
        let mut matches = vec![];
        // 原生拥有窗口在 UIA 中位于主窗口子树，不能假定是桌面直属子节点。
        for element in self.children(&self.root()?, TreeScope_Descendants)? {
            if unsafe { element.CurrentProcessId().ok() } == Some(self.pid as i32)
                && unsafe { element.CurrentClassName()?.to_string() } == "ClickRun.Rust.Dialog"
            {
                matches.push(element);
            }
        }
        ensure!(
            matches.len() == 1,
            "应有一个应用弹窗，实际 {} 个",
            matches.len()
        );
        Ok(matches.remove(0))
    }
    fn focused(&self) -> Result<Value> {
        let element = unsafe { self.client.GetFocusedElement()? };
        if unsafe { element.CurrentProcessId()? } == self.pid as i32 {
            return Ok(row(&element, self.pid));
        }
        let mut focused = vec![];
        for element in self.children(&self.root()?, TreeScope_Descendants)? {
            if unsafe { element.CurrentProcessId().ok() } == Some(self.pid as i32)
                && unsafe {
                    element
                        .CurrentHasKeyboardFocus()
                        .ok()
                        .is_some_and(|v| v.as_bool())
                }
            {
                focused.push(element);
            }
        }
        ensure!(
            focused.len() == 1,
            "全局焦点在其他进程，目标内HasKeyboardFocus未读到唯一焦点，未发送任何输入"
        );
        let mut value = row(&focused[0], self.pid);
        value["焦点依据"] = json!("目标内UIA HasKeyboardFocus，未使用其他进程焦点作断言");
        Ok(value)
    }
    fn snapshot(&self) -> Result<Value> {
        let root = self.root()?;
        let mut rows = vec![row(&root, self.pid)];
        for element in self.children(&root, TreeScope_Descendants)? {
            rows.push(row(&element, self.pid));
        }
        Ok(json!({"进程":self.pid,"窗口":self.hwnd,"节点":rows}))
    }
}

fn native_identity(pid: u32, hwnd: isize) -> Result<u64> {
    unsafe {
        let mut owner = 0;
        ensure!(
            IsWindow(hwnd as _) != 0
                && GetWindowThreadProcessId(hwnd as _, &mut owner) != 0
                && owner == pid,
            "指定窗口不属于指定进程"
        );
        let mut class = [0u16; 128];
        let count = GetClassNameW(hwnd as _, class.as_mut_ptr(), 128);
        let class = String::from_utf16_lossy(&class[..count as usize]);
        ensure!(
            class == CLASS || class.starts_with("HwndWrapper"),
            "指定窗口不是连点器主窗"
        );
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        ensure!(!process.is_null(), "无法读取测试进程身份");
        let result = (|| {
            let mut size = 32768;
            let mut image = vec![0u16; size as usize];
            ensure!(
                QueryFullProcessImageNameW(process, 0, image.as_mut_ptr(), &mut size) != 0,
                "无法读取测试程序路径"
            );
            let image = String::from_utf16_lossy(&image[..size as usize]);
            ensure!(
                image
                    .to_lowercase()
                    .starts_with(&format!("{}\\", PROJECT.to_lowercase()))
                    || image.eq_ignore_ascii_case(
                        r"D:\codex\ClickRun\ClickRun.Modern\publish\ClickRun.Modern.exe"
                    ),
                "目标不是本次连点器工程或原版 Modern"
            );
            let (mut created, mut exited, mut kernel, mut user) = (
                FILETIME::default(),
                FILETIME::default(),
                FILETIME::default(),
                FILETIME::default(),
            );
            ensure!(
                GetProcessTimes(process, &mut created, &mut exited, &mut kernel, &mut user) != 0,
                "无法读取测试进程创建时间"
            );
            Ok((created.dwHighDateTime as u64) << 32 | created.dwLowDateTime as u64)
        })();
        CloseHandle(process);
        result
    }
}

fn role(value: i32) -> &'static str {
    match value {
        50000 => "按钮",
        50002 => "复选框",
        50003 => "组合框",
        50004 => "文本输入",
        50005 => "链接",
        50007 => "列表项",
        50008 => "列表",
        50009 => "菜单",
        50010 => "菜单栏",
        50011 => "菜单项",
        50013 => "单选框",
        50015 => "滑块",
        50020 => "文本",
        50027 => "滑块手柄",
        50032 => "窗口",
        50033 => "窗格",
        50037 => "标题栏",
        _ => "其他控件",
    }
}
fn row(element: &IUIAutomationElement, pid: u32) -> Value {
    unsafe {
        let owner = element.CurrentProcessId().ok();
        if owner != Some(pid as i32) {
            return json!({"进程":owner,"跳过":"外部进程"});
        }
        let kind = element.CurrentControlType().ok().map(|v| v.0).unwrap_or(0);
        let mut patterns = vec![];
        if element
            .GetCurrentPatternAs::<IUIAutomationInvokePattern>(UIA_InvokePatternId)
            .is_ok()
        {
            patterns.push("调用");
        }
        let value = element
            .GetCurrentPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId)
            .ok()
            .map(|p| {
                patterns.push("值读写");
                p.CurrentValue().ok().map(|v| v.to_string())
            });
        let toggle = element
            .GetCurrentPatternAs::<IUIAutomationTogglePattern>(UIA_TogglePatternId)
            .ok()
            .map(|p| {
                patterns.push("切换");
                p.CurrentToggleState().ok().map(|v| v.0)
            });
        let range=element.GetCurrentPatternAs::<IUIAutomationRangeValuePattern>(UIA_RangeValuePatternId).ok().map(|p|{
            patterns.push("范围值读写");
            json!({"当前":p.CurrentValue().ok(),"最小":p.CurrentMinimum().ok(),"最大":p.CurrentMaximum().ok(),"只读":p.CurrentIsReadOnly().ok().map(|v|v.as_bool())})
        });
        if element
            .GetCurrentPatternAs::<IUIAutomationSelectionPattern>(UIA_SelectionPatternId)
            .is_ok()
        {
            patterns.push("选择容器");
        }
        let selected = element
            .GetCurrentPatternAs::<IUIAutomationSelectionItemPattern>(UIA_SelectionItemPatternId)
            .ok()
            .map(|p| {
                patterns.push("选择项目");
                p.CurrentIsSelected().ok().map(|v| v.as_bool())
            });
        if element
            .GetCurrentPatternAs::<IUIAutomationExpandCollapsePattern>(UIA_ExpandCollapsePatternId)
            .is_ok()
        {
            patterns.push("展开折叠");
        }
        if element
            .GetCurrentPatternAs::<IUIAutomationWindowPattern>(UIA_WindowPatternId)
            .is_ok()
        {
            patterns.push("窗口");
        }
        json!({"进程":pid,"角色":role(kind),"角色编号":kind,"名称":element.CurrentName().ok().map(|v|v.to_string()),"自动化ID":element.CurrentAutomationId().ok().map(|v|v.to_string()),"类":element.CurrentClassName().ok().map(|v|v.to_string()),"模式":patterns,"值":value,"范围值":range,"勾选":toggle,"选中":selected,"可用":element.CurrentIsEnabled().ok().map(|v|v.as_bool()),"屏幕外":element.CurrentIsOffscreen().ok().map(|v|v.as_bool()),"键盘可达":element.CurrentIsKeyboardFocusable().ok().map(|v|v.as_bool()),"拥有键盘焦点":element.CurrentHasKeyboardFocus().ok().map(|v|v.as_bool())})
    }
}

fn read_config(dir: &Path) -> Result<Value> {
    Ok(serde_json::from_slice(&fs::read(dir.join("config.json"))?)?)
}
fn wait_until(mut test: impl FnMut() -> Result<bool>, timeout: Duration) -> Result<()> {
    let start = Instant::now();
    loop {
        if test()? {
            return Ok(());
        }
        ensure!(start.elapsed() < timeout, "等待状态超时");
        sleep(Duration::from_millis(30));
    }
}
fn record(
    steps: &mut Vec<Value>,
    name: &str,
    start: Instant,
    outcome: Result<Value>,
) -> Result<()> {
    match outcome {
        Ok(detail) => {
            steps.push(json!({"项目":name,"通过":true,"耗时毫秒":elapsed_ms(start),"详情":detail}));
            Ok(())
        }
        Err(error) => {
            steps.push(json!({"项目":name,"通过":false,"耗时毫秒":elapsed_ms(start),"错误":format!("{error:#}")}));
            Err(error)
        }
    }
}

fn suite(c: &Client, fixture: &Path, steps: &mut Vec<Value>, single_only: bool) -> Result<()> {
    ensure!(
        fixture.starts_with(Path::new(PROJECT).join("verification")),
        "只允许隔离验收配置"
    );
    ensure!(
        c.text("401")?.contains("隔离测试"),
        "目标未显示模拟输入状态，拒绝启动连点"
    );
    if single_only {
        ensure!(
            c.element("202").is_err(),
            "已删除的点击模式仍暴露为操作控件"
        );
        ensure!(
            c.element("204").is_err(),
            "已删除的随机变化仍暴露为操作控件"
        );
    }
    let start = Instant::now();
    let _ = record(
        steps,
        "无效参数阻止启动",
        start,
        (|| {
            c.show_main()?;
            c.set(None, "203", "0")?;
            c.invoke(None, "210")?;
            ensure!(c.text("401")?.contains("已停止"), "无效参数启动了连点");
            ensure!(c.text("400")?.contains("间隔"), "未读到中文参数错误");
            Ok(json!({"警示":c.text("400")?,"状态":c.text("401")?}))
        })(),
    );
    if single_only {
        c.set(None, "203", "60")?;
        wait_until(
            || Ok(fixture.join("config.json").is_file()),
            Duration::from_secs(2),
        )?;
        let start = Instant::now();
        let _ = record(
            steps,
            "鼠标单选与配置读回",
            start,
            (|| {
                for (id, name, value) in
                    [("220", "右键", 1), ("221", "中键", 2), ("201", "左键", 0)]
                {
                    c.select(None, id, name)?;
                    wait_until(
                        || Ok(read_config(fixture)?["button"] == json!(value)),
                        Duration::from_secs(2),
                    )?;
                }
                Ok(json!({"动作":"RadioButton SelectionItem.Select","最终按键":"左键"}))
            })(),
        );
        let start = Instant::now();
        let _ = record(
            steps,
            "间隔滑块与文本双向联动",
            start,
            (|| {
                let readback = c.range("223", 80.0)?;
                ensure!(c.value("203")? == "80", "滑块RangeValue修改未联动文本");
                wait_until(
                    || Ok(read_config(fixture)?["interval"] == json!(80)),
                    Duration::from_secs(2),
                )?;
                c.set(None, "203", "60")?;
                let p: IUIAutomationRangeValuePattern = unsafe {
                    c.element("223")?
                        .GetCurrentPatternAs(UIA_RangeValuePatternId)?
                };
                ensure!(
                    (unsafe { p.CurrentValue()? } - 60.0).abs() < 0.001,
                    "文本修改未联动滑块范围值"
                );
                Ok(
                    json!({"滑块读写":readback,"文本改回":"60","滑块读回":unsafe{p.CurrentValue()?}}),
                )
            })(),
        );
    }
    let start = Instant::now();
    let _ = record(
        steps,
        if single_only {
            "单击七次并停止"
        } else {
            "三击按单次累计七次"
        },
        start,
        (|| {
            c.set(None, "203", "60")?;
            if !single_only {
                c.set(None, "204", "0")?;
                c.select(None, "202", "三击")?;
            }
            c.select(None, "205", "无限")?;
            ensure!(
                !unsafe { c.element("206")?.CurrentIsEnabled()?.as_bool() },
                "UIA选择无限后指定次数输入未禁用"
            );
            c.select(None, if single_only { "222" } else { "205" }, "指定次数")?;
            ensure!(
                unsafe { c.element("206")?.CurrentIsEnabled()?.as_bool() },
                "UIA选择指定次数后输入未启用"
            );
            c.set(None, "206", "7")?;
            c.invoke(None, "210")?;
            wait_until(
                || Ok(c.text("401")?.contains("已停止") && c.count("402")? == 7),
                Duration::from_secs(3),
            )?;
            ensure!(c.text("404")?.contains("0.0"), "停止后速度未归零");
            if single_only {
                let config = read_config(fixture)?;
                ensure!(
                    config.get("mode").is_none_or(|v| *v == 0)
                        && config.get("randomDelay").is_none_or(|v| *v == 0),
                    "旧配置中的模式/随机参数未清零或移除"
                );
            }
            Ok(json!({"本次":c.text("402")?,"累计":c.text("403")?,"速度":c.text("404")?}))
        })(),
    );
    let profile = format!(
        "无障碍验收{}",
        SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis()
    );
    let start = Instant::now();
    let _ = record(
        steps,
        "方案保存与弹窗焦点",
        start,
        (|| {
            c.invoke(None, "208")?;
            let dialog = c.dialog()?;
            let focused = c.focused()?;
            ensure!(
                focused["自动化ID"] == "501",
                "保存方案弹窗焦点未进入名称输入"
            );
            c.set(Some(&dialog), "501", "")?;
            c.invoke(Some(&dialog), "502")?;
            ensure!(
                unsafe { c.in_root(&dialog, "504")?.CurrentName()?.to_string() }.contains("请输入"),
                "空方案名称未反馈中文错误"
            );
            c.set(Some(&dialog), "501", &profile)?;
            c.invoke(Some(&dialog), "502")?;
            ensure!(c.focused()?["自动化ID"] == "208", "保存后焦点未回到原按钮");
            wait_until(
                || {
                    Ok(read_config(fixture)?["profiles"]
                        .as_array()
                        .is_some_and(|p| p.iter().any(|v| v["name"] == profile)))
                },
                Duration::from_secs(2),
            )?;
            Ok(json!({"新方案":profile,"弹窗初始焦点":focused,"返回焦点":c.focused()?}))
        })(),
    );
    let start = Instant::now();
    let _ = record(
        steps,
        "方案读取和删除",
        start,
        (|| {
            let alternate = format!("{profile}切换");
            c.set(None, "203", "200")?;
            c.invoke(None, "208")?;
            let dialog = c.dialog()?;
            c.set(Some(&dialog), "501", &alternate)?;
            c.invoke(Some(&dialog), "502")?;
            c.select(None, "207", &profile)?;
            ensure!(c.value("203")? == "60", "方案参数未恢复");
            c.invoke(None, "209")?;
            c.select(None, "207", &alternate)?;
            c.invoke(None, "209")?;
            ensure!(
                !read_config(fixture)?["profiles"]
                    .as_array()
                    .context("缺少方案列表")?
                    .iter()
                    .any(|v| v["name"] == profile),
                "方案未删除"
            );
            ensure!(
                !read_config(fixture)?["profiles"]
                    .as_array()
                    .context("缺少方案列表")?
                    .iter()
                    .any(|v| v["name"] == alternate),
                "切换方案未删除"
            );
            Ok(
                json!({"恢复的间隔":"60","切换方案的间隔":"200","两个新方案均已删除":true,"删除后配置":read_config(fixture)?}),
            )
        })(),
    );
    let start = Instant::now();
    let _ = record(
        steps,
        "设置语义切换并保存",
        start,
        (|| {
            c.invoke(None, "101")?;
            let before = read_config(fixture)?;
            let top = c.toggle("304")?;
            let tray = c.toggle("303")?;
            wait_until(
                || {
                    let v = read_config(fixture)?;
                    Ok(v["alwaysOnTop"] == json!(top == 1)
                        && v["minimizeToTray"] == json!(tray == 1))
                },
                Duration::from_secs(2),
            )?;
            let first = read_config(fixture)?;
            // 每项均经真实 Toggle 读回，再关掉避免重启时置顶或隐藏干扰用户。
            for (id, key) in [
                ("304", "alwaysOnTop"),
                ("303", "minimizeToTray"),
                ("305", "playSound"),
                ("302", "startWithWindows"),
            ] {
                let element = c.element(id)?;
                let p: IUIAutomationTogglePattern =
                    unsafe { element.GetCurrentPatternAs(UIA_TogglePatternId)? };
                let state = unsafe { p.CurrentToggleState()?.0 };
                let changed = c.toggle(id)?;
                ensure!(changed != state, "开关 {key} 未变更状态");
                wait_until(
                    || Ok(read_config(fixture)?[key] == json!(changed == 1)),
                    Duration::from_secs(2),
                )?;
                if changed == 1 {
                    c.toggle(id)?;
                }
                wait_until(
                    || Ok(read_config(fixture)?[key] == json!(false)),
                    Duration::from_secs(2),
                )?;
            }
            Ok(
                json!({"修改前":before,"首轮保存":first,"最终保存":read_config(fixture)?,"重启读回":"由单独重启验收执行"}),
            )
        })(),
    );
    let start = Instant::now();
    let _ = record(
        steps,
        "热键冲突保留原配置",
        start,
        (|| {
            c.show_main()?;
            let old = read_config(fixture)?;
            // 测试工具预占 Ctrl+Alt+Shift+F11，仅注册系统热键，不模拟按键。
            let hotkey_id = 0x5100;
            ensure!(
                unsafe {
                    RegisterHotKey(
                        null_mut(),
                        hotkey_id,
                        MOD_CONTROL | MOD_ALT | MOD_SHIFT | MOD_NOREPEAT,
                        122,
                    )
                } != 0,
                "测试热键已被其他进程占用"
            );
            struct Unregister(i32);
            impl Drop for Unregister {
                fn drop(&mut self) {
                    unsafe {
                        UnregisterHotKey(null_mut(), self.0);
                    }
                }
            }
            let _registration = Unregister(hotkey_id);
            c.invoke(None, "301")?;
            let dialog = c.dialog()?;
            let focus = c.focused()?;
            let initial_id = if c.in_root(&dialog, "509").is_ok() {
                "509"
            } else {
                "505"
            };
            ensure!(
                focus["自动化ID"] == initial_id,
                "热键弹窗焦点未进入预期控件"
            );
            c.select(Some(&dialog), "505", "F11")?;
            for id in ["506", "507", "508"] {
                let e = c.in_root(&dialog, id)?;
                let p: IUIAutomationTogglePattern =
                    unsafe { e.GetCurrentPatternAs(UIA_TogglePatternId)? };
                if unsafe { p.CurrentToggleState()?.0 } != 1 {
                    unsafe {
                        p.Toggle()?;
                    }
                }
            }
            c.invoke(Some(&dialog), "502")?;
            let warning = unsafe { c.in_root(&dialog, "504")?.CurrentName()?.to_string() };
            ensure!(warning.contains("注册失败"), "未读到热键冲突错误");
            let after = read_config(fixture)?;
            ensure!(
                old["hotKey"] == after["hotKey"]
                    && old["hotKeyModifiers"] == after["hotKeyModifiers"],
                "冲突改变了原配置"
            );
            c.invoke(Some(&dialog), "503")?;
            ensure!(
                c.focused()?["自动化ID"] == "301",
                "取消后焦点未返回热键按钮"
            );
            Ok(
                json!({"警示":warning,"原热键":old["hotKey"],"原修饰键":old["hotKeyModifiers"],"弹窗初始焦点":focus,"返回焦点":c.focused()?}),
            )
        })(),
    );
    c.show_main()?;
    ensure!(
        steps.iter().all(|s| s["通过"] == true),
        "存在未通过的无障碍验收项目，详见步骤"
    );
    Ok(())
}

fn persistence(c: &Client, fixture: &Path, expected: &Path, single_only: bool) -> Result<Value> {
    let expected: Value = serde_json::from_slice(&fs::read(expected)?)?;
    c.show_main()?;
    ensure!(
        c.value("203")? == expected["interval"].to_string(),
        "重启后间隔不一致"
    );
    if !single_only {
        ensure!(
            c.value("204")? == expected["randomDelay"].to_string(),
            "重启后随机变化不一致"
        );
    }
    ensure!(
        c.count("403")?
            == expected["totalClicks"]
                .as_u64()
                .context("预期配置未含累计数字")?,
        "重启后累计次数不一致"
    );
    c.invoke(None, "101")?;
    let mut controls = vec![];
    for (id, key) in [
        ("304", "alwaysOnTop"),
        ("303", "minimizeToTray"),
        ("305", "playSound"),
        ("302", "startWithWindows"),
    ] {
        let e = c.element(id)?;
        let p: IUIAutomationTogglePattern = unsafe { e.GetCurrentPatternAs(UIA_TogglePatternId)? };
        let checked = unsafe { p.CurrentToggleState()?.0 } == 1;
        ensure!(json!(checked) == expected[key], "重启后设置 {key} 不一致");
        controls.push(row(&e, c.pid));
    }
    let actual = read_config(fixture)?;
    ensure!(actual == expected, "重启后配置内容改变");
    c.show_main()?;
    Ok(json!({"重启后设置读回":controls,"累计次数":expected["totalClicks"],"配置一致":true}))
}

fn tray_system(c: &Client) -> Result<Value> {
    use windows_sys::Win32::UI::Shell::{NOTIFYICONIDENTIFIER, Shell_NotifyIconGetRect};
    let identifier = NOTIFYICONIDENTIFIER {
        cbSize: std::mem::size_of::<NOTIFYICONIDENTIFIER>() as u32,
        hWnd: c.hwnd as _,
        uID: 1,
        guidItem: Default::default(),
    };
    let mut rect = RECT::default();
    ensure!(
        unsafe { Shell_NotifyIconGetRect(&identifier, &mut rect) } == 0,
        "系统未确认实际托盘图标注册"
    );
    c.invoke(None, "211")?;
    wait_until(
        || Ok(unsafe { IsWindowVisible(c.hwnd as _) } == 0),
        Duration::from_secs(2),
    )?;
    ensure!(
        unsafe { PostMessageW(c.hwnd as _, WM_APP + 1, 0, WM_LBUTTONDBLCLK as isize) } != 0,
        "托盘恢复专项消息发送失败"
    );
    wait_until(
        || Ok(unsafe { IsWindowVisible(c.hwnd as _) } != 0),
        Duration::from_secs(2),
    )?;
    Ok(
        json!({"系统注册查询":"Shell_NotifyIconGetRect","图标实际存在":true,"隐藏":"实际主窗按钮 UIA Invoke","隐藏到恢复":"源代码托盘双击回调专项消息 WM_APP+1/WM_LBUTTONDBLCLK","恢复后可见":true,"说明":"恢复回调属于系统专项验证，未声称是通知区域实际双击验收"}),
    )
}

fn legacy_exit(c: &Client) -> Result<Value> {
    let root = c.root()?;
    ensure!(
        unsafe { root.CurrentFrameworkId()?.to_string() } == "WPF",
        "此动作只用于原版 WPF"
    );
    let before = c.snapshot()?;
    let mut nodes = c.children(&root, TreeScope_Descendants)?;
    let mut back = vec![];
    for element in &nodes {
        if unsafe { element.CurrentName()?.to_string() } == "←"
            && unsafe { element.CurrentControlType()?.0 } == 50000
        {
            back.push(element);
        }
    }
    if back.len() == 1 {
        let pattern: IUIAutomationInvokePattern =
            unsafe { back[0].GetCurrentPatternAs(UIA_InvokePatternId)? };
        unsafe {
            pattern.Invoke()?;
        }
        sleep(Duration::from_millis(100));
        nodes = c.children(&root, TreeScope_Descendants)?;
    }
    let stopped = nodes.iter().any(|e| unsafe {
        e.CurrentName()
            .ok()
            .is_some_and(|v| v.to_string().contains("开始连点"))
            && e.CurrentControlType().ok().is_some_and(|v| v.0 == 50000)
    });
    ensure!(stopped, "原版不处于已停止状态，不执行关闭");
    let mut gear = vec![];
    for e in &nodes {
        if unsafe { e.CurrentName()?.to_string() } == "⚙"
            && unsafe { e.CurrentControlType()?.0 } == 50000
        {
            gear.push(e);
        }
    }
    ensure!(gear.len() == 1, "原版设置按钮语义不唯一");
    let p: IUIAutomationInvokePattern =
        unsafe { gear[0].GetCurrentPatternAs(UIA_InvokePatternId)? };
    unsafe {
        p.Invoke()?;
    }
    sleep(Duration::from_millis(150));
    let mut labels = vec![];
    for e in c.children(&root, TreeScope_Descendants)? {
        if unsafe { e.CurrentName()?.to_string() } == "最小化到托盘" {
            labels.push(e);
        }
    }
    ensure!(labels.len() == 1, "无法唯一读取原版托盘设置文字");
    let walker = unsafe { c.client.RawViewWalker()? };
    let mut ancestor = labels.remove(0);
    let mut found = None;
    let mut association = json!({"方法":"RawView文字同容器的唯一TogglePattern"});
    for _ in 0..7 {
        let Ok(parent) = (unsafe { walker.GetParentElement(&ancestor) }) else {
            break;
        };
        ancestor = parent;
        let children = c.children(&ancestor, TreeScope_Descendants)?;
        let mut toggles = vec![];
        let mut names = vec![];
        for e in children {
            names.push(unsafe { e.CurrentName()?.to_string() });
            if let Ok(p) =
                unsafe { e.GetCurrentPatternAs::<IUIAutomationTogglePattern>(UIA_TogglePatternId) }
            {
                toggles.push((e, p));
            }
        }
        if toggles.len() == 1 && names.iter().any(|v| v == "最小化到托盘") {
            found = Some((toggles.remove(0), names));
            break;
        }
    }
    if found.is_none() {
        // 旧 WPF 的布局 Grid/StackPanel 没有 peer。按主代理已核准的源布局，
        // 读取两条中文行标题的 UIA 矩形区间，唯一关联同行匿名开关；动作仍是 Toggle。
        let all = c.children(&root, TreeScope_Descendants)?;
        let mut minimum = vec![];
        let mut next = vec![];
        for element in &all {
            let name = unsafe { element.CurrentName()?.to_string() };
            if name == "最小化到托盘" {
                minimum.push(unsafe { element.CurrentBoundingRectangle()? });
            }
            if name == "窗口置顶" {
                next.push(unsafe { element.CurrentBoundingRectangle()? });
            }
        }
        ensure!(
            minimum.len() == 1 && next.len() == 1,
            "旧版设置中文行标题无法唯一确定"
        );
        let low = (minimum[0].top + minimum[0].bottom) as f64 / 2.0;
        let high = (next[0].top + next[0].bottom) as f64 / 2.0;
        ensure!(high > low, "旧版设置行顺序与已核准源布局不符");
        let mut candidates = vec![];
        for element in all {
            if let Ok(pattern) = unsafe {
                element.GetCurrentPatternAs::<IUIAutomationTogglePattern>(UIA_TogglePatternId)
            } {
                let rect = unsafe { element.CurrentBoundingRectangle()? };
                let center = (rect.top + rect.bottom) as f64 / 2.0;
                if center >= low
                    && center < high
                    && unsafe { element.CurrentIsOffscreen()?.as_bool() } == false
                {
                    candidates.push((element, pattern, rect));
                }
            }
        }
        ensure!(
            candidates.len() == 1,
            "文字行矩形范围内未找到唯一TogglePattern，未按索引猜测"
        );
        let (element, pattern, rect) = candidates.remove(0);
        association = json!({"方法":"读取UIA BoundingRectangle辅助语义关联；无坐标点击","下界文字":"最小化到托盘","上界文字":"窗口置顶","中心Y区间":[low,high],"匹配开关矩形":[rect.left,rect.top,rect.right,rect.bottom],"候选数量":1,"原因":"原版匿名开关的Grid/StackPanel无UIA peer；源XAML已由主代理核准同行"});
        found = Some((
            (element, pattern),
            vec![
                "最小化到托盘".into(),
                "最小化或关闭窗口时隐藏到系统托盘".into(),
                "窗口置顶".into(),
            ],
        ));
    }
    let ((toggle_element, toggle), sibling_texts) =
        found.context("未找到与托盘文字同一容器的唯一开关，未按顺序猜测")?;
    let initial = unsafe { toggle.CurrentToggleState()?.0 };
    if initial != 0 {
        unsafe {
            toggle.Toggle()?;
        }
    }
    ensure!(
        unsafe { toggle.CurrentToggleState()?.0 } == 0,
        "托盘开关未关闭"
    );
    let toggle_readback = row(&toggle_element, c.pid);
    let process = unsafe {
        OpenProcess(
            PROCESS_SYNCHRONIZE | PROCESS_QUERY_LIMITED_INFORMATION,
            0,
            c.pid,
        )
    };
    ensure!(!process.is_null(), "无法绑定原进程退出等待");
    let outcome = (|| -> Result<()> {
        let window: IUIAutomationWindowPattern =
            unsafe { root.GetCurrentPatternAs(UIA_WindowPatternId)? };
        unsafe {
            window.Close()?;
        }
        ensure!(
            unsafe { WaitForSingleObject(process, 5000) } == WAIT_OBJECT_0,
            "原版未通过正常关闭退出，未强制终止"
        );
        Ok(())
    })();
    unsafe {
        CloseHandle(process);
    }
    outcome?;
    Ok(
        json!({"正常退出":true,"强制结束":false,"关闭方法":"真实UIA WindowPattern.Close","托盘开关识别依据":association,"关联中文文字":sibling_texts,"初始托盘状态":initial,"关闭前开关读回":toggle_readback,"动作前快照":before,"配置恢复":"由主代理按已备份原用户配置离线恢复"}),
    )
}

struct GeometryContext {
    root: HWND,
    rows: Vec<Value>,
}
unsafe extern "system" fn geometry_child(h: HWND, data: LPARAM) -> i32 {
    let context = &mut *(data as *mut GeometryContext);
    if GetParent(h) != context.root {
        return 1;
    }
    let mut r = RECT::default();
    if GetWindowRect(h, &mut r) == 0 {
        return 1;
    }
    let mut top = POINT {
        x: r.left,
        y: r.top,
    };
    let mut bottom = POINT {
        x: r.right,
        y: r.bottom,
    };
    ScreenToClient(context.root, &mut top);
    ScreenToClient(context.root, &mut bottom);
    let mut class = [0u16; 128];
    let count = GetClassNameW(h, class.as_mut_ptr(), 128);
    context.rows.push(json!({"hwnd":h as isize,"id":GetDlgCtrlID(h),"类":String::from_utf16_lossy(&class[..count.max(0) as usize]),"x":top.x,"y":top.y,"宽":bottom.x-top.x,"高":bottom.y-top.y,"可见":IsWindowVisible(h)!=0}));
    1
}
fn child_geometry(c: &Client) -> Result<Value> {
    c.check()?;
    ensure!(
        unsafe { IsWindowVisible(c.hwnd as _) } != 0 && unsafe { IsIconic(c.hwnd as _) } == 0,
        "窗口不可见或最小化，不能作为可见布局验收"
    );
    let mut context = GeometryContext {
        root: c.hwnd as _,
        rows: vec![],
    };
    unsafe {
        EnumChildWindows(
            context.root,
            Some(geometry_child),
            &mut context as *mut _ as isize,
        );
    }
    context
        .rows
        .sort_by_key(|v| v["id"].as_i64().unwrap_or_default());
    let mut ids = std::collections::BTreeSet::new();
    let mut rectangles = std::collections::BTreeSet::new();
    for row in &context.rows {
        ensure!(
            ids.insert(row["id"].as_i64().context("无原生控件ID")?),
            "出现重复子控件ID"
        );
        let class = row["类"].as_str().unwrap_or_default().to_ascii_lowercase();
        if row["可见"] == true
            && ["button", "edit", "combobox", "msctls_trackbar32"].contains(&class.as_str())
        {
            let key = (
                row["x"].as_i64(),
                row["y"].as_i64(),
                row["宽"].as_i64(),
                row["高"].as_i64(),
            );
            ensure!(
                rectangles.insert(key),
                "两个可见交互控件占用完全相同的原生矩形"
            );
        }
    }
    Ok(json!(context.rows))
}
fn scroll_message(c: &Client, down: bool) -> Result<()> {
    c.check()?;
    let mut result = 0usize;
    ensure!(
        unsafe {
            SendMessageTimeoutW(
                c.hwnd as _,
                WM_VSCROLL,
                if down { SB_PAGEDOWN } else { SB_PAGEUP } as usize,
                0,
                SMTO_ABORTIFHUNG,
                1000,
                Some(&mut result).map_or(null_mut(), |v| v),
            )
        } != 0,
        "滚动专项消息未完成"
    );
    Ok(())
}
fn scroll_regression(c: &Client) -> Result<Value> {
    ensure!(
        c.text("401")?.contains("隔离测试"),
        "只允许隔离测试实例进行滚动回归"
    );
    c.show_main()?;
    let root = c.root()?;
    let transform = unsafe {
        root.GetCurrentPatternAs::<IUIAutomationTransformPattern>(UIA_TransformPatternId)
    }
    .ok()
    .filter(|p| unsafe { p.CurrentCanResize().ok().is_some_and(|v| v.as_bool()) });
    let resize = |width: i32, height: i32| -> Result<()> {
        if let Some(p) = &transform {
            unsafe {
                p.Resize(width as f64, height as f64)?;
            }
        } else {
            ensure!(
                unsafe {
                    SetWindowPos(
                        c.hwnd as _,
                        null_mut(),
                        0,
                        0,
                        width,
                        height,
                        SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE,
                    )
                } != 0,
                "系统窗口resize专项失败"
            );
        }
        Ok(())
    };
    let original = unsafe { root.CurrentBoundingRectangle()? };
    let dpi = unsafe { GetDpiForWindow(c.hwnd as _) }.max(96);
    resize((400 * dpi / 96) as i32, (420 * dpi / 96) as i32)?;
    sleep(Duration::from_millis(100));
    let scroll =
        unsafe { root.GetCurrentPatternAs::<IUIAutomationScrollPattern>(UIA_ScrollPatternId) }
            .ok()
            .filter(|p| unsafe {
                p.CurrentVerticallyScrollable()
                    .ok()
                    .is_some_and(|v| v.as_bool())
            });
    if let Some(p) = &scroll {
        unsafe {
            p.SetScrollPercent(-1.0, 0.0)?;
        }
    } else {
        for _ in 0..20 {
            scroll_message(c, false)?;
        }
    }
    let baseline = child_geometry(c)?;
    let route = if scroll.is_some() {
        "UIA ScrollPattern"
    } else {
        "WM_VSCROLL系统专项（未提供根ScrollPattern）"
    };
    let mut rounds = vec![];
    for i in 0..20 {
        if let Some(p) = &scroll {
            unsafe {
                p.Scroll(ScrollAmount_NoAmount, ScrollAmount_LargeIncrement)?;
            }
        } else {
            scroll_message(c, true)?;
        }
        let bottom = child_geometry(c)?;
        if let Some(p) = &scroll {
            unsafe {
                p.SetScrollPercent(-1.0, 0.0)?;
            }
        } else {
            for _ in 0..20 {
                scroll_message(c, false)?;
            }
        }
        let top = child_geometry(c)?;
        ensure!(
            top == baseline,
            "第{}轮滚动回顶后原生控件矩形或数量改变",
            i + 1
        );
        rounds.push(json!({"轮次":i+1,"控件数量":top.as_array().map(Vec::len),"回顶矩形一致":true,"底部位置":if i==19{bottom}else{Value::Null}}));
    }
    resize(
        original.right - original.left,
        original.bottom - original.top,
    )?;
    sleep(Duration::from_millis(80));
    c.set(None, "203", "0")?;
    c.invoke(None, "210")?;
    ensure!(!c.text("400")?.is_empty(), "未产生可读警示");
    let warning_geometry = child_geometry(c)?;
    c.set(None, "203", "60")?;
    c.select(None, "222", "指定")?;
    c.set(None, "206", "1")?;
    c.invoke(None, "210")?;
    wait_until(
        || Ok(c.text("401")?.contains("已停止") && c.count("402")? == 1),
        Duration::from_secs(3),
    )?;
    ensure!(
        c.element("400").is_err() || c.text("400")?.is_empty(),
        "有效启动后警示未清除"
    );
    for _ in 0..10 {
        c.invoke(None, "101")?;
        child_geometry(c)?;
        c.show_main()?;
        child_geometry(c)?;
    }
    let window: IUIAutomationWindowPattern =
        unsafe { root.GetCurrentPatternAs(UIA_WindowPatternId)? };
    unsafe {
        window.SetWindowVisualState(WindowVisualState_Minimized)?;
    }
    wait_until(
        || Ok(unsafe { IsIconic(c.hwnd as _) } != 0 || unsafe { IsWindowVisible(c.hwnd as _) } == 0),
        Duration::from_secs(2),
    )?;
    c.normal()?;
    child_geometry(c)?;
    c.set(None, "203", "0")?;
    c.invoke(None, "210")?;
    ensure!(!c.text("400")?.is_empty(), "最终警示未出现供视觉核验");
    Ok(
        json!({"目标DPI":dpi,"坐标单位":"物理像素（调用进程每显示器v2）","滚动路径":route,"窗口调整路径":if transform.is_some(){"UIA Transform.Resize"}else{"SetWindowPos系统窗口专项（根未提供TransformPattern）"},"循环":rounds,"最小尺寸逻辑单位":[400,420],"已恢复原窗口大小":true,"最小化与恢复":"真实UIA WindowPattern，已确认恢复后可见非最小化","警示出现时矩形":warning_geometry,"结束矩形":child_geometry(c)?,"结束警示":c.text("400")?,"设置往返次数":10,"无重复原生控件":true,"视觉结论":"未由几何或无障碍树证明；仍需主代理截图核对是否存在残影、重叠及对齐"}),
    )
}

fn edit_geometry(c: &Client) -> Result<Value> {
    c.normal()?;
    c.show_main()?;
    let dpi = unsafe { GetDpiForWindow(c.hwnd as _) }.max(96);
    let mut rows = vec![];
    for id in ["203", "206"] {
        let e = c.element(id)?;
        let h = unsafe { e.CurrentNativeWindowHandle()?.0 };
        let mut outer = RECT::default();
        let mut client = RECT::default();
        ensure!(
            unsafe { GetWindowRect(h, &mut outer) } != 0
                && unsafe { GetClientRect(h, &mut client) } != 0,
            "未读到输入框矩形"
        );
        let mut top = POINT {
            x: client.left,
            y: client.top,
        };
        let mut bottom = POINT {
            x: client.right,
            y: client.bottom,
        };
        unsafe {
            ClientToScreen(h, &mut top);
            ClientToScreen(h, &mut bottom);
        }
        let delta = ((top.y + bottom.y) - (outer.top + outer.bottom)).abs() as f64 / 2.0 * 96.0
            / dpi as f64;
        ensure!(
            delta <= 1.0,
            "输入框{id}文本区垂直中心偏移{delta:.2}逻辑像素"
        );
        let style = unsafe { GetWindowLongPtrW(h, GWL_STYLE) } as u32;
        ensure!(
            style & ES_CENTER as u32 != 0,
            "输入框{id}未设置原生水平居中样式"
        );
        let font = unsafe { SendMessageW(h, WM_GETFONT, 0, 0) };
        let mut logfont = LOGFONTW::default();
        ensure!(
            font != 0
                && unsafe {
                    GetObjectW(
                        font as _,
                        std::mem::size_of::<LOGFONTW>() as i32,
                        &mut logfont as *mut _ as _,
                    )
                } > 0,
            "无法读取原生输入字体"
        );
        let length = logfont
            .lfFaceName
            .iter()
            .position(|v| *v == 0)
            .unwrap_or(32);
        let face = String::from_utf16_lossy(&logfont.lfFaceName[..length]);
        let logical_height = logfont.lfHeight.unsigned_abs() as f64 * 96.0 / dpi as f64;
        ensure!(
            (logical_height - 14.0).abs() <= 1.0,
            "输入框{id}字号不是14逻辑像素"
        );
        ensure!(
            face.contains("方正公文楷体"),
            "输入框{id}字体不是方正公文楷体"
        );
        rows.push(json!({"ID":id,"名称":unsafe{e.CurrentName()?.to_string()},"外框":[outer.left,outer.top,outer.right,outer.bottom],"文本区":[top.x,top.y,bottom.x,bottom.y],"垂直中心偏移逻辑像素":delta,"水平居中样式":true,"字体":face,"字号逻辑像素":logical_height}));
    }
    Ok(
        json!({"目标DPI":dpi,"坐标单位":"物理像素（调用进程每显示器v2）","输入框":rows,"依据":"实际HWND WindowRect/ClientRect、ES_CENTER、WM_GETFONT/GetObject系统读回","视觉核对":"仅证明几何/样式/字体值；截图外观仍由主代理复核"}),
    )
}

fn edit_semantics(c: &Client) -> Result<Value> {
    ensure!(
        c.text("401")?.contains("隔离测试"),
        "只允许隔离测试实例修改数字"
    );
    c.show_main()?;
    c.select(None, "222", "指定")?;
    let mut rows = vec![];
    for (id, value) in [("203", "123"), ("206", "12")] {
        c.set(None, id, value)?;
        let element = c.element(id)?;
        let p: IUIAutomationTextPattern = unsafe { element.GetCurrentPatternAs(UIA_TextPatternId) }
            .context("原生编辑框未提供UIA TextPattern，未用坐标或模拟输入替代")?;
        let range = unsafe { p.DocumentRange()? };
        unsafe {
            range.Select()?;
        }
        let selected = unsafe { p.GetSelection()? };
        ensure!(unsafe { selected.Length()? } > 0, "数字文本选择未读回范围");
        let selected_text = unsafe { selected.GetElement(0)?.GetText(-1)?.to_string() };
        ensure!(selected_text.trim() == value, "数字文本选中内容读回不一致");
        rows.push(json!({"ID":id,"ValuePattern读写":value,"TextPattern选择读回":selected_text}));
    }
    c.set(None, "203", "0")?;
    c.invoke(None, "210")?;
    ensure!(
        c.text("401")?.contains("已停止") && c.text("400")?.contains("间隔"),
        "数字错误未阻止启动并反馈"
    );
    c.set(None, "203", "60")?;
    c.set(None, "206", "7")?;
    Ok(
        json!({"控件":rows,"错误反馈":c.text("400")?,"键盘物理输入":"本次未使用输入模拟；实际键盘导航另行验收"}),
    )
}

fn prepare_main(c:&Client)->Result<Value>{
    ensure!(c.text("401")?.contains("隔离测试"),"只允许隔离模拟输入实例准备截图状态");
    c.show_main()?;c.set(None,"203","100")?;c.select(None,"222","指定")?;c.set(None,"206","7")?;
    c.invoke(None,"210")?;
    wait_until(||Ok(c.text("401")?.contains("已停止")&&c.count("402")?==7),Duration::from_secs(3))?;
    ensure!(c.element("400").is_err()||c.text("400")?.is_empty(),"准备截图后警示未清除");
    ensure!(c.value("203")?=="100"&&c.value("206")?=="7","准备截图的数字未读回");
    Ok(json!({"已正常可见":true,"间隔":"100","指定次数":"7","警示已清除":true,"本次模拟点击":7,"快照":c.snapshot()?}))
}

fn warning_main(c:&Client)->Result<Value>{
    ensure!(c.text("401")?.contains("隔离测试"),"只允许隔离实例准备错误示例");
    c.show_main()?;c.set(None,"203","0")?;c.invoke(None,"210")?;
    let mut dismissed=false;
    if c.element("213").is_ok(){
        c.invoke(None,"213")?;ensure!(c.element("400").is_err()||c.text("400")?.is_empty(),"警示关闭按钮未清除警示");
        dismissed=true;c.invoke(None,"210")?;
    }
    if let Ok(p)=unsafe{c.root()?.GetCurrentPatternAs::<IUIAutomationScrollPattern>(UIA_ScrollPatternId)}{
        if unsafe{p.CurrentVerticallyScrollable()?.as_bool()}{unsafe{p.SetScrollPercent(-1.0,0.0)?;}}
    }
    ensure!(c.text("400")?.contains("间隔")&&c.text("401")?.contains("已停止"),"错误示例未显示或意外启动");
    Ok(json!({"警示":c.text("400")?,"间隔":"0","已停止":true,"警示关闭按钮语义验证":dismissed,"主窗口已回顶部":true,"快照":c.snapshot()?}))
}

fn real_menu_items(c:&Client)->Result<Vec<IUIAutomationElement>>{
    let mut all=c.children(&c.root()?,TreeScope_Descendants)?;
    let desktop=unsafe{c.client.GetRootElement()?};
    for top in c.children(&desktop,TreeScope_Children)?{
        if unsafe{top.CurrentProcessId().ok()}==Some(c.pid as i32){
            all.push(top.clone());all.extend(c.children(&top,TreeScope_Descendants)?);
        }
    }
    let mut menu_items=vec![];
    for element in all{
        if unsafe{element.CurrentProcessId().ok()}!=Some(c.pid as i32)
            || unsafe{element.CurrentControlType().ok().map(|v|v.0)}!=Some(50011){continue;}
        let mut duplicate=false;
        for existing in &menu_items{if unsafe{c.client.CompareElements(&element,existing)?.as_bool()}{duplicate=true;break;}}
        if !duplicate{menu_items.push(element);}
    }
    Ok(menu_items)
}
fn open_real_menu(c:&Client)->Result<Vec<IUIAutomationElement>>{
    c.check()?;
    ensure!(unsafe{PostMessageW(c.hwnd as _,WM_APP+1,0,WM_RBUTTONUP as isize)}!=0,"无法触发源代码托盘菜单回调");
    let start=Instant::now();
    loop{
        let items=real_menu_items(c)?;
        if items.iter().any(|e|unsafe{e.CurrentName().ok().is_some_and(|v|v.to_string()=="打开设置")}){return Ok(items);}
        ensure!(start.elapsed()<Duration::from_secs(3),"实际Native菜单未暴露可识别中文MenuItem");
        sleep(Duration::from_millis(30));
    }
}
fn invoke_menu(c:&Client,items:&[IUIAutomationElement],name:&str)->Result<()> {
    c.check()?;
    let mut matching=vec![];
    for element in items{if unsafe{element.CurrentName()?.to_string()}==name{matching.push(element);}}
    ensure!(matching.len()==1,"实际菜单项“{name}”不唯一或缺失");
    let pattern:IUIAutomationInvokePattern=unsafe{matching[0].GetCurrentPatternAs(UIA_InvokePatternId)}.context("实际Native菜单未提供Invoke，未使用坐标或WM_COMMAND替代")?;
    unsafe{pattern.Invoke()?;}
    sleep(Duration::from_millis(80));Ok(())
}
fn tray_menu_semantics(c:&Client)->Result<Value>{
    ensure!(c.text("401")?.contains("隔离测试"),"只允许隔离模拟实例验证菜单");
    c.show_main()?;c.set(None,"203","1000")?;c.select(None,"222","指定")?;c.set(None,"206","100")?;
    let items=open_real_menu(c)?;
    let listed:Vec<_>=items.iter().map(|e|row(e,c.pid)).collect();
    for expected in ["打开主窗口","打开设置","开始连点","退出"]{
        ensure!(listed.iter().any(|v|v["名称"]==expected),"实际菜单缺少中文入口 {expected}");
    }
    invoke_menu(c,&items,"打开设置")?;
    wait_until(||Ok(c.element("302").is_ok()),Duration::from_secs(2))?;
    let settings=row(&c.element("302")?,c.pid);
    c.show_main()?;
    let items=open_real_menu(c)?;invoke_menu(c,&items,"开始连点")?;
    wait_until(||Ok(c.text("401")?.contains("模拟连点中")),Duration::from_secs(2))?;
    let items=open_real_menu(c)?;invoke_menu(c,&items,"停止连点")?;
    wait_until(||Ok(c.text("401")?.contains("已停止")),Duration::from_secs(2))?;
    c.set(None,"203","100")?;c.set(None,"206","7")?;
    let items=open_real_menu(c)?;invoke_menu(c,&items,"打开主窗口")?;
    ensure!(unsafe{IsWindowVisible(c.hwnd as _)}!=0&&unsafe{IsIconic(c.hwnd as _)}==0,"菜单主窗入口未恢复正常可见窗口");
    let process=unsafe{OpenProcess(PROCESS_SYNCHRONIZE|PROCESS_QUERY_LIMITED_INFORMATION,0,c.pid)};
    ensure!(!process.is_null(),"无法绑定菜单退出目标进程");
    let items=open_real_menu(c)?;
    let outcome=invoke_menu(c,&items,"退出");
    let exited=unsafe{WaitForSingleObject(process,5000)}==WAIT_OBJECT_0;
    unsafe{CloseHandle(process);}
    ensure!(exited,"实际菜单Invoke未导致正常退出；未用WM_COMMAND或强杀替代：{:?}",outcome.err());
    let identifier=windows_sys::Win32::UI::Shell::NOTIFYICONIDENTIFIER{cbSize:std::mem::size_of::<windows_sys::Win32::UI::Shell::NOTIFYICONIDENTIFIER>() as u32,hWnd:c.hwnd as _,uID:1,guidItem:Default::default()};
    let mut rect=RECT::default();let tray_status=unsafe{windows_sys::Win32::UI::Shell::Shell_NotifyIconGetRect(&identifier,&mut rect)};
    ensure!(tray_status!=0,"菜单退出后系统仍找到托盘图标");
    Ok(json!({"打开菜单路径":"系统专项PostMessage(TRAY/WM_RBUTTONUP)进入实际TrackPopupMenu","菜单动作路径":"实际MenuItem UIA Invoke；没有WM_COMMAND或输入模拟","菜单节点":listed,"打开设置读回":settings,"主窗入口":true,"启停入口":true,"菜单退出进程结束":true,"退出后托盘不再找到":true,"退出后托盘查询":tray_status}))
}
fn installed_smoke(c:&Client)->Result<Value>{
    let root=c.root()?;
    let title=unsafe{root.CurrentName()?.to_string()};ensure!(title=="连点器 Modern","正式窗口中文标题不一致");
    c.show_main()?;
    let badge=c.text("307")?;ensure!(badge.contains("F5"),"正式热键未保留F5");
    ensure!(c.value("203")?=="1","正式间隔未保留1毫秒");
    ensure!(c.count("403")?==321748,"正式累计次数未保留321748");
    ensure!(c.count("402")?==0,"正式启动产生了会话点击");
    let start=c.text("210")?;ensure!(start.contains("开始连点"),"正式入口不是已停止状态");
    ensure!(!c.text("401")?.contains("隔离测试"),"正式入口仍使用测试模式");
    ensure!(c.element("202").is_err()&&c.element("204").is_err(),"删除的参数重新出现");
    let main=c.snapshot()?;
    c.invoke(None,"101")?;let mut settings=vec![];
    for id in ["302","303","304","305"]{let e=c.element(id)?;let p:IUIAutomationTogglePattern=unsafe{e.GetCurrentPatternAs(UIA_TogglePatternId)?};
        settings.push(json!({"ID":id,"名称":unsafe{e.CurrentName()?.to_string()},"当前状态":unsafe{p.CurrentToggleState()?.0},"动作":"只读；没有Toggle"}));}
    c.show_main()?;
    ensure!(c.count("402")?==0,"正式只读核验过程中产生了点击");
    let big_icon=unsafe{GetClassLongPtrW(c.hwnd as _,GCLP_HICON)};
    let small_icon=unsafe{GetClassLongPtrW(c.hwnd as _,GCLP_HICONSM)};
    ensure!(big_icon!=0&&small_icon!=0,"正式窗口未读取到有效类图标");
    let identifier=windows_sys::Win32::UI::Shell::NOTIFYICONIDENTIFIER{cbSize:std::mem::size_of::<windows_sys::Win32::UI::Shell::NOTIFYICONIDENTIFIER>() as u32,hWnd:c.hwnd as _,uID:1,guidItem:Default::default()};
    let mut rect=RECT::default();ensure!(unsafe{windows_sys::Win32::UI::Shell::Shell_NotifyIconGetRect(&identifier,&mut rect)}==0,"正式托盘图标未在系统中找到");
    Ok(json!({"正式版本只读核验":true,"没有StartToggle或鼠标注入":true,"窗口标题":title,"热键标识":badge,"间隔毫秒":1,"累计次数":321748,"本次次数":0,"开始按钮":start,"设置真实读回":settings,"主界面快照":main,"最终主界面正常可见":true,"类大图标句柄":big_icon,"类小图标句柄":small_icon,"托盘实际注册查询":"Shell_NotifyIconGetRect成功"}))
}

fn main() -> std::process::ExitCode {
    unsafe {
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }
    if unsafe {
        AreDpiAwarenessContextsEqual(
            GetThreadDpiAwarenessContext(),
            DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
        )
    } == 0
    {
        eprintln!("未能启用每显示器v2 DPI模式，停止几何混算及UI动作");
        return 2.into();
    }
    let mut args = BTreeMap::new();
    let mut iter = std::env::args().skip(1);
    while let Some(key) = iter.next() {
        let Some(value) = iter.next() else {
            eprintln!("参数缺少值：{key}");
            return 2.into();
        };
        args.insert(key, value);
    }
    let mut report = json!({"后端":"CUIAutomation8 / 真实UIA语义模式","无坐标或键鼠模拟":true,"系统专项验证与UIA分列":true,"通过":false,"步骤":[]});
    let mut steps = vec![];
    let result = (|| -> Result<Value> {
        let pid = args.get("--pid").context("需要 --pid")?.parse()?;
        let hwnd = args.get("--hwnd").context("需要 --hwnd")?.parse()?;
        let c = Client::new(pid, hwnd)?;
        let action = args
            .get("--action")
            .map(String::as_str)
            .unwrap_or("observe");
        if action != "observe" {
            c.normal()?;
        }
        report["进程"] = json!(pid);
        report["窗口"] = json!(hwnd);
        report["进程创建时间"] = json!(c.creation);
        report["动作"] = json!(action);
        match action {
            "observe" => c.snapshot(),
            "invoke" => {
                let id = args.get("--id").context("需要 --id")?;
                c.invoke(None, id)?;
                if id == "212" {
                    wait_until(
                        || Ok(unsafe { IsWindow(c.hwnd as _) } == 0),
                        Duration::from_secs(3),
                    )?;
                    let identifier=windows_sys::Win32::UI::Shell::NOTIFYICONIDENTIFIER{cbSize:std::mem::size_of::<windows_sys::Win32::UI::Shell::NOTIFYICONIDENTIFIER>() as u32,hWnd:c.hwnd as _,uID:1,guidItem:Default::default()};
                    let mut rect=RECT::default();let tray_status=unsafe{windows_sys::Win32::UI::Shell::Shell_NotifyIconGetRect(&identifier,&mut rect)};
                    ensure!(tray_status!=0,"主窗关闭后系统仍找到对应托盘图标");
                    Ok(json!({"正常退出":true,"托盘退出后系统不再找到":"Shell_NotifyIconGetRect返回非成功","托盘查询结果":tray_status}))
                } else {
                    c.snapshot()
                }
            }
            "set" => {
                c.set(
                    None,
                    args.get("--id").context("需要 --id")?,
                    args.get("--value").context("需要 --value")?,
                )?;
                c.snapshot()
            }
            "select" => {
                c.select(
                    None,
                    args.get("--id").context("需要 --id")?,
                    args.get("--value").context("需要 --value")?,
                )?;
                c.snapshot()
            }
            "toggle" => {
                let state = c.toggle(args.get("--id").context("需要 --id")?)?;
                Ok(json!({"开关状态":state,"快照":c.snapshot()?}))
            }
            "range" => c.range(
                args.get("--id").context("需要 --id")?,
                args.get("--value").context("需要 --value")?.parse()?,
            ),
            "suite" => {
                suite(
                    &c,
                    Path::new(args.get("--fixture-dir").context("需要 --fixture-dir")?),
                    &mut steps,
                    args.get("--single-only").map(String::as_str) == Some("yes"),
                )?;
                c.snapshot()
            }
            "persist-check" => persistence(
                &c,
                Path::new(args.get("--fixture-dir").context("需要 --fixture-dir")?),
                Path::new(args.get("--expected").context("需要 --expected")?),
                args.get("--single-only").map(String::as_str) == Some("yes"),
            ),
            "tray-system" => tray_system(&c),
            "scroll-regression" => scroll_regression(&c),
            "edit-geometry" => edit_geometry(&c),
            "edit-semantics" => edit_semantics(&c),
            "prepare-main"=>prepare_main(&c),
            "warning-main"=>warning_main(&c),
            "tray-menu"=>tray_menu_semantics(&c),
            "installed-smoke"=>installed_smoke(&c),
            "legacy-exit" => {
                ensure!(
                    args.get("--execute").map(String::as_str) == Some("yes"),
                    "legacy-exit 需要 --execute yes"
                );
                legacy_exit(&c)
            }
            _ => bail!("不支持的动作：{action}"),
        }
    })();
    report["步骤"] = json!(steps);
    match result {
        Ok(value) => {
            report["通过"] = json!(true);
            report["结果"] = value;
        }
        Err(error) => {
            report["错误"] = json!(format!("{error:#}"));
        }
    }
    let success = report["通过"] == true;
    if let Some(output) = args.get("--output") {
        let path = PathBuf::from(output);
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Err(e) = fs::write(&path, serde_json::to_vec_pretty(&report).unwrap()) {
            eprintln!("报告保存失败：{e}");
            return 2.into();
        }
    }
    println!(
        "{}",
        json!({"通过":success,"报告":args.get("--output"),"错误":report.get("错误"),"步骤数":report["步骤"].as_array().map(Vec::len)})
    );
    if success { 0.into() } else { 2.into() }
}
