#![allow(unsafe_op_in_unsafe_fn)]
use crate::{
    config::{Config, ConfigStore, Profile},
    engine::Engine,
    system,
};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    ffi::c_void,
    mem::{size_of, zeroed},
    path::PathBuf,
    ptr::{null, null_mut},
    time::Instant,
};
use windows_sys::Win32::System::Diagnostics::Debug::MessageBeep;
use windows_sys::Win32::{
    Foundation::*,
    Graphics::Gdi::*,
    System::{LibraryLoader::*, Threading::*},
    UI::{
        Accessibility::*, Controls::*, HiDpi::*, Input::KeyboardAndMouse::*, Shell::*,
        WindowsAndMessaging::*,
    },
};

const NAME: &str = "连点器 Modern";
const TRAY: u32 = WM_APP + 1;
const ACTIVATE: u32 = WM_APP + 2;
const ENSURE_FOCUS: u32 = WM_APP + 3;
const TICK: usize = 1;
const SAVE: usize = 2;
const NIN_KEYSELECT: u32 = 0x401;
const HOTKEY: i32 = 1;
const ID_MAIN: i32 = 100;
const ID_SETTINGS: i32 = 101;
const ID_ABOUT: i32 = 102;
const ID_BUTTON: i32 = 201;
const ID_INTERVAL: i32 = 203;
const ID_RIGHT: i32 = 220;
const ID_MIDDLE: i32 = 221;
const ID_LIMITED: i32 = 222;
const ID_SLIDER: i32 = 223;
const ID_HOTKEY_VALUE: i32 = 307;
const ID_LIMIT_KIND: i32 = 205;
const ID_LIMIT: i32 = 206;
const ID_PROFILE: i32 = 207;
const ID_SAVE_PROFILE: i32 = 208;
const ID_DELETE_PROFILE: i32 = 209;
const ID_START: i32 = 210;
const ID_HIDE: i32 = 211;
const ID_EXIT: i32 = 212;
const ID_DISMISS: i32 = 213;
const ID_HOTKEY: i32 = 301;
const ID_AUTOSTART: i32 = 302;
const ID_TRAY: i32 = 303;
const ID_TOP: i32 = 304;
const ID_SOUND: i32 = 305;
const ID_OPEN_CONFIG: i32 = 306;
const ID_WARNING: i32 = 400;
const ID_STATUS: i32 = 401;
const ID_SESSION: i32 = 402;
const ID_TOTAL: i32 = 403;
const ID_CPS: i32 = 404;
const ID_TIME: i32 = 405;
const ID_DIALOG_EDIT: i32 = 501;
const ID_DIALOG_OK: i32 = 502;
const ID_DIALOG_CANCEL: i32 = 503;
const ID_DIALOG_MSG: i32 = 504;
const ID_KEY: i32 = 505;
const ID_CTRL: i32 = 506;
const ID_ALT: i32 = 507;
const ID_SHIFT: i32 = 508;
const ID_CAPTURE: i32 = 509;
const BG: u32 = 0x00f0f5f5;
const INK: u32 = 0x002e1a1a;
const TBM_GETPOS: u32 = WM_USER;
const SS_CENTER: u32 = 1;
const SS_CENTERIMAGE: u32 = 0x200;

fn w(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}
unsafe fn text(h: HWND, s: &str) {
    SetWindowTextW(h, w(s).as_ptr());
}
unsafe fn read(h: HWND) -> String {
    let n = GetWindowTextLengthW(h);
    let mut b = vec![0u16; n as usize + 1];
    GetWindowTextW(h, b.as_mut_ptr(), n + 1);
    String::from_utf16_lossy(&b[..n as usize])
}
unsafe fn checked(h: HWND) -> bool {
    SendMessageW(h, BM_GETCHECK, 0, 0) == BST_CHECKED as isize
}
unsafe fn check(h: HWND, b: bool) {
    SetPropW(h, w("ClickRun.InternalSelection").as_ptr(), 1usize as _);
    SendMessageW(
        h,
        BM_SETCHECK,
        if b {
            BST_CHECKED as usize
        } else {
            BST_UNCHECKED as usize
        },
        0,
    );
    RemovePropW(h, w("ClickRun.InternalSelection").as_ptr());
}
unsafe fn slider_set(h: HWND, value: i32) {
    SetPropW(h, w("ClickRun.InternalSelection").as_ptr(), 1usize as _);
    SendMessageW(h, TBM_SETPOS, 1, value as isize);
    RemovePropW(h, w("ClickRun.InternalSelection").as_ptr());
}
unsafe fn sel(h: HWND) -> i32 {
    SendMessageW(h, CB_GETCURSEL, 0, 0) as i32
}
unsafe fn choose(h: HWND, n: i32) {
    SetPropW(h, w("ClickRun.InternalSelection").as_ptr(), 1usize as _);
    SendMessageW(h, CB_SETCURSEL, n as usize, 0);
    RemovePropW(h, w("ClickRun.InternalSelection").as_ptr());
}
unsafe fn add(h: HWND, s: &str) {
    SendMessageW(h, CB_ADDSTRING, 0, w(s).as_ptr() as isize);
}

struct Control {
    h: HWND,
    page: i32,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    right: bool,
    stretch: bool,
}
struct App {
    hwnd: HWND,
    controls: BTreeMap<i32, Control>,
    config: Config,
    store: ConfigStore,
    engine: Engine,
    page: i32,
    dpi: u32,
    scroll: Cell<i32>,
    font: HFONT,
    heading: HFONT,
    caption: HFONT,
    data_font: HFONT,
    brush: HBRUSH,
    ink_brush: HBRUSH,
    icon: HICON,
    tray: bool,
    taskbar_created: u32,
    loading: bool,
    registered: bool,
    hotkey_id: i32,
    test: bool,
    dialog: HWND,
    dialog_kind: i32,
    keys: Vec<i32>,
    pending_key: i32,
    pending_mods: i32,
    started: bool,
    last_checkpoint: Instant,
    font_path: PathBuf,
    font_registered: bool,
    names: Option<crate::a11y::Names>,
    defer_ui: bool,
    ui_ready: bool,
    ui_creating: bool,
    pending_warning: RefCell<String>,
    layout_busy: Cell<bool>,
}
impl App {
    unsafe fn h(&self, id: i32) -> HWND {
        self.controls.get(&id).map(|x| x.h).unwrap_or(null_mut())
    }
    unsafe fn warn(&self, s: &str) {
        if !s.is_empty() {
            crate::log::write(s);
        }
        self.pending_warning.replace(s.to_owned());
        if self.ui_ready {
            text(self.h(ID_WARNING), s);
            NotifyWinEvent(EVENT_OBJECT_NAMECHANGE, self.h(ID_WARNING), OBJID_CLIENT, 0);
            self.layout();
        }
    }
    fn px(&self, n: i32) -> i32 {
        n * self.dpi as i32 / 96
    }
    unsafe fn control(
        &mut self,
        id: i32,
        class: &str,
        label: &str,
        style: u32,
        page: i32,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        right: bool,
        stretch: bool,
    ) -> HWND {
        let h = CreateWindowExW(
            0,
            w(class).as_ptr(),
            w(label).as_ptr(),
            WS_CHILD
                | WS_VISIBLE
                | style
                | if class == "EDIT" && ![ID_INTERVAL, ID_LIMIT].contains(&id) {
                    WS_BORDER
                } else {
                    0
                }
                | if [ID_INTERVAL, ID_LIMIT].contains(&id) {
                    ES_CENTER as u32
                } else {
                    0
                },
            0,
            0,
            0,
            0,
            self.hwnd,
            id as usize as _,
            GetModuleHandleW(null()),
            null(),
        );
        SendMessageW(h, WM_SETFONT, self.font_for(id) as usize, 1);
        SetWindowSubclass(h, Some(child_proc), id as usize, 0);
        self.controls.insert(
            id,
            Control {
                h,
                page,
                x,
                y,
                width,
                height,
                right,
                stretch,
            },
        );
        h
    }
    unsafe fn label(
        &mut self,
        id: i32,
        label: &str,
        page: i32,
        x: i32,
        y: i32,
        width: i32,
        right: bool,
    ) {
        self.control(id, "STATIC", label, 0, page, x, y, width, 24, right, false);
    }
    unsafe fn button(
        &mut self,
        id: i32,
        label: &str,
        page: i32,
        x: i32,
        y: i32,
        width: i32,
    ) -> HWND {
        self.control(
            id,
            "BUTTON",
            label,
            WS_TABSTOP | BS_PUSHBUTTON as u32,
            page,
            x,
            y,
            width,
            28,
            false,
            false,
        )
    }
    unsafe fn core_init(&mut self) {
        // Configuration and system integration are available even before the
        // first window is shown; old profile fields cannot restore removed modes.
        self.config.mode = 0;
        self.config.random_delay = 0;
        for profile in &mut self.config.profiles {
            profile.mode = 0;
            profile.random_delay = 0;
        }
        self.register();
        self.tray_add();
    }
    unsafe fn ensure_ui(&mut self) {
        if self.ui_ready || self.ui_creating {
            return;
        }
        // Creating children synchronously dispatches window messages. Mark this
        // transaction before any Win32 call so show/layout cannot create twice.
        self.ui_creating = true;
        self.loading = true;
        let mut ic: INITCOMMONCONTROLSEX = zeroed();
        ic.dwSize = size_of::<INITCOMMONCONTROLSEX>() as u32;
        ic.dwICC = ICC_STANDARD_CLASSES | ICC_BAR_CLASSES;
        InitCommonControlsEx(&ic);
        self.font_registered = AddFontResourceExW(
            w(&self.font_path.to_string_lossy()).as_ptr(),
            FR_PRIVATE,
            null(),
        ) > 0;
        self.names = match crate::a11y::Names::new() {
            Ok(names) => Some(names),
            Err(error) => {
                crate::log::write(&error);
                None
            }
        };
        self.dpi = GetDpiForWindow(self.hwnd).max(96);
        self.make_fonts();
        self.control(1, "STATIC", "ClickRun", 0, 0, 20, 18, 260, 38, false, false);
        self.control(
            2,
            "STATIC",
            "鼠标连点器",
            0,
            1,
            20,
            56,
            180,
            20,
            false,
            false,
        );
        self.button(ID_MAIN, "返回", 0, 20, 23, 44);
        let settings = self.button(ID_SETTINGS, "设置", 1, 20, 26, 52);
        self.controls.get_mut(&ID_SETTINGS).unwrap().right = true;
        let _ = settings;
        self.control(ID_WARNING, "STATIC", "", 0, 0, 20, 82, 400, 38, false, true);
        self.button(ID_DISMISS, "×", 0, 20, 82, 24);
        self.controls.get_mut(&ID_DISMISS).unwrap().right = true;
        self.label(1207, "方案", 1, 20, 92, 44, false);
        let profile = self.control(
            ID_PROFILE,
            "COMBOBOX",
            "",
            WS_TABSTOP | WS_VSCROLL | CBS_DROPDOWNLIST as u32,
            1,
            68,
            86,
            190,
            180,
            false,
            false,
        );
        let _ = profile;
        self.button(ID_SAVE_PROFILE, "存", 1, 68, 86, 44);
        self.controls.get_mut(&ID_SAVE_PROFILE).unwrap().right = true;
        self.button(ID_DELETE_PROFILE, "删", 1, 20, 86, 44);
        self.controls.get_mut(&ID_DELETE_PROFILE).unwrap().right = true;
        self.label(1301, "热键", 1, 20, 132, 44, false);
        self.control(
            ID_HOTKEY_VALUE,
            "STATIC",
            "F6",
            SS_CENTER as u32 | SS_CENTERIMAGE as u32,
            1,
            68,
            126,
            54,
            28,
            false,
            false,
        );
        self.button(ID_HOTKEY, "修改", 1, 130, 126, 48);
        self.label(1201, "按键", 1, 20, 192, 44, false);
        for (id, label, x, group) in [
            (ID_BUTTON, "左键", 68, true),
            (ID_RIGHT, "右键", 140, false),
            (ID_MIDDLE, "中键", 212, false),
        ] {
            self.control(
                id,
                "BUTTON",
                label,
                WS_TABSTOP
                    | BS_AUTORADIOBUTTON as u32
                    | BS_PUSHLIKE as u32
                    | if group { WS_GROUP } else { 0 },
                1,
                x,
                184,
                64,
                30,
                false,
                false,
            );
        }
        self.label(1203, "间隔", 1, 20, 232, 44, false);
        let slider = self.control(
            ID_SLIDER,
            "msctls_trackbar32",
            "",
            WS_TABSTOP | TBS_NOTICKS as u32,
            1,
            68,
            224,
            180,
            30,
            false,
            false,
        );
        SendMessageW(slider, TBM_SETRANGEMIN, 0, 1);
        SendMessageW(slider, TBM_SETRANGEMAX, 0, 10000);
        SendMessageW(slider, TBM_SETLINESIZE, 0, 1);
        SendMessageW(slider, TBM_SETPAGESIZE, 0, 100);
        let interval = self.control(
            ID_INTERVAL,
            "EDIT",
            "",
            WS_TABSTOP | ES_AUTOHSCROLL as u32,
            1,
            46,
            224,
            64,
            28,
            true,
            false,
        );
        SendMessageW(interval, EM_SETLIMITTEXT, 5, 0);
        self.label(1503, "毫秒", 1, 20, 232, 28, true);
        self.label(1205, "次数", 1, 20, 276, 44, false);
        self.control(
            ID_LIMIT_KIND,
            "BUTTON",
            "无限",
            WS_TABSTOP | WS_GROUP | BS_AUTORADIOBUTTON as u32 | BS_PUSHLIKE as u32,
            1,
            68,
            268,
            64,
            30,
            false,
            false,
        );
        self.control(
            ID_LIMITED,
            "BUTTON",
            "指定",
            WS_TABSTOP | BS_AUTORADIOBUTTON as u32 | BS_PUSHLIKE as u32,
            1,
            140,
            268,
            64,
            30,
            false,
            false,
        );
        let limit = self.control(
            ID_LIMIT,
            "EDIT",
            "",
            WS_TABSTOP | ES_AUTOHSCROLL as u32,
            1,
            216,
            268,
            70,
            28,
            false,
            false,
        );
        SendMessageW(limit, EM_SETLIMITTEXT, 10, 0);
        self.label(1506, "次", 1, 294, 276, 30, false);
        for (id, top, height) in [
            (1201, 184, 30),
            (1203, 224, 28),
            (1205, 268, 30),
            (1503, 224, 28),
            (1506, 268, 28),
        ] {
            let c = self.controls.get_mut(&id).unwrap();
            c.y = top;
            c.height = height;
            SetWindowLongPtrW(
                c.h,
                GWL_STYLE,
                GetWindowLongPtrW(c.h, GWL_STYLE) | SS_CENTERIMAGE as isize,
            );
        }
        self.control(
            ID_START,
            "BUTTON",
            "开始连点",
            WS_TABSTOP | BS_PUSHBUTTON as u32,
            1,
            20,
            326,
            400,
            42,
            false,
            true,
        );
        self.control(
            ID_STATUS,
            "STATIC",
            "已停止",
            0,
            1,
            20,
            378,
            400,
            20,
            false,
            true,
        );
        self.label(1702, "已点击", 1, 20, 420, 180, false);
        self.control(
            ID_SESSION, "STATIC", "0", 0, 1, 20, 444, 180, 34, false, false,
        );
        self.label(1705, "运行时长", 1, 20, 420, 180, true);
        self.control(
            ID_TIME, "STATIC", "00:00:00", 0, 1, 20, 444, 180, 34, true, false,
        );
        self.label(1704, "点击速度", 1, 20, 492, 180, false);
        self.control(
            ID_CPS,
            "STATIC",
            "0.0 次/秒",
            0,
            1,
            20,
            516,
            180,
            24,
            false,
            false,
        );
        self.label(1703, "总点击次数", 1, 20, 492, 180, true);
        self.control(ID_TOTAL, "STATIC", "0", 0, 1, 20, 516, 180, 24, true, false);
        self.button(ID_HIDE, "隐藏到托盘", 0, 20, 558, 108);
        self.button(ID_EXIT, "退出", 0, 20, 558, 52);
        self.controls.get_mut(&ID_EXIT).unwrap().right = true;
        self.label(1800, "通用", 2, 20, 92, 360, false);
        for (id, label, description, y) in [
            (ID_AUTOSTART, "登录时启动", "登录后隐藏到托盘", 120),
            (ID_TRAY, "最小化到托盘", "最小化或关闭窗口时隐藏到托盘", 192),
            (ID_TOP, "窗口置顶", "窗口保持在其他普通窗口上方", 264),
            (ID_SOUND, "播放音效", "开始或停止时播放提示音", 336),
        ] {
            self.control(
                id,
                "BUTTON",
                label,
                WS_TABSTOP | BS_AUTOCHECKBOX as u32 | BS_RIGHTBUTTON as u32,
                2,
                20,
                y,
                400,
                28,
                false,
                true,
            );
            self.control(
                id + 1000,
                "STATIC",
                description,
                0,
                2,
                20,
                y + 30,
                400,
                22,
                false,
                true,
            );
        }
        self.button(ID_ABOUT, "关于", 2, 20, 440, 68);
        self.button(ID_OPEN_CONFIG, "打开配置文件夹", 2, 104, 440, 144);
        self.control(9,"STATIC","版本 3.0.1 · Rust\r\n\r\n固定间隔，单次连点。\r\nTab 移动焦点；空格操作按钮或开关。\r\n1 毫秒是目标间隔，实际速度取决于系统与目标应用。",0,3,20,106,400,190,false,true);
        self.button(601, "项目主页", 3, 20, 320, 144);
        self.button(602, "GitHub", 3, 20, 366, 144);
        if let Some(names) = &self.names {
            for (id, name) in [
                (ID_INTERVAL, "点击间隔（毫秒）"),
                (ID_LIMIT, "指定点击次数"),
                (ID_SLIDER, "点击间隔"),
                (ID_HOTKEY, "修改启停热键"),
                (ID_SAVE_PROFILE, "保存方案"),
                (ID_DELETE_PROFILE, "删除方案"),
                (ID_DISMISS, "关闭提示"),
            ] {
                if let Err(e) = names.set(self.h(id), name) {
                    crate::log::write(&e);
                }
            }
        }
        // 登录启动属于设置页。显示该页时再读取系统真实状态，避免主窗启动
        // 就为隐藏控件创建 ShellLink COM 对象并加载整组 Shell 存储模块。
        self.ui_ready = true;
        let warning = self.pending_warning.borrow().clone();
        text(self.h(ID_WARNING), &warning);
        if !warning.is_empty() {
            NotifyWinEvent(EVENT_OBJECT_NAMECHANGE, self.h(ID_WARNING), OBJID_CLIENT, 0);
        }
        self.fill();
        self.show_page(1);
        self.ui_creating = false;
    }
    unsafe fn font_for(&self, id: i32) -> HFONT {
        if id == 1 {
            self.heading
        } else if [ID_SESSION, ID_TIME].contains(&id) {
            self.data_font
        } else if id >= 1000
            || [
                2,
                ID_WARNING,
                ID_STATUS,
                ID_MAIN,
                ID_SETTINGS,
                ID_ABOUT,
                ID_SAVE_PROFILE,
                ID_DELETE_PROFILE,
                ID_HOTKEY,
                ID_HIDE,
                ID_EXIT,
            ]
            .contains(&id)
        {
            self.caption
        } else {
            self.font
        }
    }
    unsafe fn make_fonts(&mut self) {
        let old = [self.font, self.heading, self.caption, self.data_font];
        let dpi = self.dpi;
        let make = |size: i32| {
            CreateFontW(
                -size * dpi as i32 / 96,
                0,
                0,
                0,
                400,
                0,
                0,
                0,
                DEFAULT_CHARSET as u32,
                0,
                0,
                CLEARTYPE_QUALITY as u32,
                0,
                w("方正公文楷体").as_ptr(),
            )
        };
        self.font = make(14);
        self.heading = make(28);
        self.caption = make(12);
        self.data_font = make(24);
        for (&id, c) in &self.controls {
            SendMessageW(c.h, WM_SETFONT, self.font_for(id) as usize, 1);
        }
        for font in old {
            if !font.is_null() {
                DeleteObject(font as _);
            }
        }
    }
    unsafe fn warning_offset(&self) -> i32 {
        if self.pending_warning.borrow().is_empty() {
            0
        } else {
            44
        }
    }
    unsafe fn layout(&self) {
        if !self.ui_ready {
            return;
        }
        if self.layout_busy.replace(true) {
            return;
        }
        let mut r: RECT = zeroed();
        GetClientRect(self.hwnd, &mut r);
        let offset = self.warning_offset();
        ShowWindow(
            self.h(ID_WARNING),
            if offset > 0 { SW_SHOW } else { SW_HIDE },
        );
        ShowWindow(
            self.h(ID_DISMISS),
            if offset > 0 { SW_SHOW } else { SW_HIDE },
        );
        let mut si: SCROLLINFO = zeroed();
        si.cbSize = size_of::<SCROLLINFO>() as u32;
        si.fMask = SIF_RANGE | SIF_PAGE | SIF_POS;
        si.nMax = 590 + offset;
        si.nPage = (r.bottom * 96 / self.dpi as i32).max(1) as u32;
        self.scroll.set(
            self.scroll
                .get()
                .clamp(0, (591 + offset - r.bottom * 96 / self.dpi as i32).max(0)),
        );
        si.nPos = self.scroll.get();
        SetScrollInfo(self.hwnd, SB_VERT, &si, 1);
        GetClientRect(self.hwnd, &mut r);
        let width = (r.right * 96 / self.dpi as i32).max(380);
        let mut batch = BeginDeferWindowPos(self.controls.len() as i32);
        for (&id, c) in &self.controls {
            let stat = [
                ID_SESSION, ID_TIME, ID_CPS, ID_TOTAL, 1702, 1705, 1704, 1703,
            ]
            .contains(&id);
            let cw = if stat {
                (width - 52) / 2
            } else if id == ID_SLIDER {
                width - 68 - 122
            } else if c.stretch {
                width - c.x - 20
            } else if id == ID_PROFILE {
                (width - 68 - 132).min(190)
            } else if id == ID_HOTKEY_VALUE {
                (hotkey_name(self.config.hot_key, self.config.hot_key_modifiers)
                    .chars()
                    .count() as i32
                    * 8
                    + 20)
                    .clamp(54, 210)
            } else {
                c.width
            };
            let x = if id == ID_HOTKEY {
                68 + (hotkey_name(self.config.hot_key, self.config.hot_key_modifiers)
                    .chars()
                    .count() as i32
                    * 8
                    + 20)
                    .clamp(54, 210)
                    + 8
            } else if id == 1 {
                if self.page == 1 { 20 } else { 76 }
            } else if c.right {
                width - c.x - cw
            } else {
                c.x
            };
            let y = c.y + if c.y >= 86 { offset } else { 0 };
            let mut y_px = self.px(y - self.scroll.get());
            if id == ID_SLIDER {
                let mut thumb: RECT = zeroed();
                SendMessageW(c.h, TBM_GETTHUMBRECT, 0, &mut thumb as *mut RECT as isize);
                if thumb.bottom > thumb.top {
                    y_px =
                        self.px(238 + offset - self.scroll.get()) - (thumb.top + thumb.bottom) / 2;
                }
            }
            if !batch.is_null() {
                batch = DeferWindowPos(
                    batch,
                    c.h,
                    null_mut(),
                    self.px(x),
                    y_px,
                    self.px(cw),
                    self.px(c.height),
                    SWP_NOZORDER | SWP_NOACTIVATE | SWP_NOREDRAW | SWP_NOCOPYBITS,
                );
            } else {
                SetWindowPos(
                    c.h,
                    null_mut(),
                    self.px(x),
                    y_px,
                    self.px(cw),
                    self.px(c.height),
                    SWP_NOZORDER | SWP_NOACTIVATE | SWP_NOREDRAW | SWP_NOCOPYBITS,
                );
            }
        }
        if !batch.is_null() {
            EndDeferWindowPos(batch);
        }
        // One complete repaint after the geometry transaction; copying child
        // window pixels during each move leaves stale controls while scrolling.
        RedrawWindow(
            self.hwnd,
            null(),
            null_mut(),
            RDW_INVALIDATE | RDW_ERASE | RDW_ALLCHILDREN | RDW_UPDATENOW,
        );
        self.layout_busy.set(false);
    }
    unsafe fn scroll_to(&mut self, n: i32) {
        let mut r = zeroed();
        GetClientRect(self.hwnd, &mut r);
        self.scroll.set(n.clamp(
            0,
            (591 + self.warning_offset() - r.bottom * 96 / self.dpi as i32).max(0),
        ));
        self.layout();
    }
    unsafe fn ensure_focus(&mut self, id: i32) {
        if let Some(c) = self.controls.get(&id) {
            let top = c.y + if c.y >= 86 { self.warning_offset() } else { 0 };
            let bottom = top + c.height.min(40);
            let mut r = zeroed();
            GetClientRect(self.hwnd, &mut r);
            let height = r.bottom * 96 / self.dpi as i32;
            if top < self.scroll.get() {
                self.scroll_to(top - 8);
            } else if bottom > self.scroll.get() + height {
                self.scroll_to(bottom - height + 8);
            }
        }
    }
    unsafe fn show_page(&mut self, page: i32) {
        self.page = page;
        text(
            self.h(1),
            if page == 1 {
                "ClickRun"
            } else if page == 2 {
                "设置"
            } else {
                "关于"
            },
        );
        if page == 2 {
            match system::autostart_state() {
                Ok(value) => {
                    self.config.start_with_windows = value;
                    check(self.h(ID_AUTOSTART), value);
                    EnableWindow(self.h(ID_AUTOSTART), 1);
                }
                Err(e) => {
                    self.warn(&e);
                    EnableWindow(self.h(ID_AUTOSTART), 0);
                }
            }
        }
        self.scroll.set(0);
        for c in self.controls.values() {
            ShowWindow(
                c.h,
                if c.page == 0 || c.page == page {
                    SW_SHOW
                } else {
                    SW_HIDE
                },
            );
        }
        self.layout();
        ShowWindow(self.h(ID_MAIN), if page == 1 { SW_HIDE } else { SW_SHOW });
        SetFocus(self.h(if page == 1 {
            if self.engine.snapshot().running { ID_START } else { ID_BUTTON }
        } else if page == 2 {
            ID_AUTOSTART
        } else {
            601
        }));
    }
    unsafe fn fill(&mut self) {
        self.loading = true;
        self.config.mode = 0;
        self.config.random_delay = 0;
        for (id, value) in [(ID_BUTTON, 0), (ID_RIGHT, 1), (ID_MIDDLE, 2)] {
            check(self.h(id), self.config.button == value);
        }
        text(self.h(ID_INTERVAL), &self.config.interval.to_string());
        slider_set(self.h(ID_SLIDER), self.config.interval);
        check(self.h(ID_LIMIT_KIND), self.config.click_limit == 0);
        check(self.h(ID_LIMITED), self.config.click_limit > 0);
        text(
            self.h(ID_LIMIT),
            &self.config.click_limit.max(1).to_string(),
        );
        EnableWindow(
            self.h(ID_LIMIT),
            if self.config.click_limit > 0 { 1 } else { 0 },
        );
        for (id, b) in [
            (ID_AUTOSTART, self.config.start_with_windows),
            (ID_TRAY, self.config.minimize_to_tray),
            (ID_TOP, self.config.always_on_top),
            (ID_SOUND, self.config.play_sound),
        ] {
            check(self.h(id), b);
        }
        text(
            self.h(ID_HOTKEY_VALUE),
            &hotkey_name(self.config.hot_key, self.config.hot_key_modifiers),
        );
        SendMessageW(self.h(ID_PROFILE), CB_RESETCONTENT, 0, 0);
        for p in &self.config.profiles {
            add(self.h(ID_PROFILE), &p.name);
        }
        SetWindowPos(
            self.hwnd,
            if self.config.always_on_top {
                HWND_TOPMOST
            } else {
                HWND_NOTOPMOST
            },
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
        );
        self.loading = false;
        self.refresh();
        self.layout();
    }
    unsafe fn read_config(&mut self) -> Result<(), String> {
        let interval = read(self.h(ID_INTERVAL))
            .trim()
            .parse::<i32>()
            .map_err(|_| "间隔须为 1–10000 的整数。")?;
        if !(1..=10000).contains(&interval) {
            return Err("间隔须在 1–10000 毫秒之间。".into());
        }
        let limit = if checked(self.h(ID_LIMITED)) {
            let n = read(self.h(ID_LIMIT))
                .trim()
                .parse::<i32>()
                .map_err(|_| "指定次数须为正整数，最多 2147483647。")?;
            if n < 1 {
                return Err("指定次数须大于 0。".into());
            }
            n
        } else {
            0
        };
        self.config.button = if checked(self.h(ID_RIGHT)) {
            1
        } else if checked(self.h(ID_MIDDLE)) {
            2
        } else {
            0
        };
        self.config.mode = 0;
        self.config.random_delay = 0;
        self.config.interval = interval;
        self.config.click_limit = limit;
        slider_set(self.h(ID_SLIDER), interval);
        Ok(())
    }
    unsafe fn dirty(&self) {
        SetTimer(self.hwnd, SAVE, 500, None);
    }
    unsafe fn save(&mut self) {
        KillTimer(self.hwnd, SAVE);
        self.config.total_clicks = self.engine.snapshot().total;
        if let Err(e) = self.store.save(&self.config) {
            self.warn(&e);
        }
    }
    unsafe fn register(&mut self) {
        if self.registered {
            UnregisterHotKey(self.hwnd, self.hotkey_id);
            self.registered = false;
        }
        if RegisterHotKey(
            self.hwnd,
            self.hotkey_id,
            self.config.hot_key_modifiers as u32 | MOD_NOREPEAT,
            self.config.hot_key as u32,
        ) != 0
        {
            self.registered = true;
        } else {
            self.warn("热键已被占用；可在设置中修改，也可用按钮开始和停止。");
        }
    }
    unsafe fn refresh(&mut self) {
        let s = self.engine.snapshot();
        // Hidden startup still completes sessions, persists counts, and updates
        // the tray; only the window controls depend on first-show initialization.
        if self.ui_ready {
            let sec = s.elapsed.as_secs();
            text(self.h(ID_SESSION), &s.session.to_string());
            text(self.h(ID_TOTAL), &s.total.to_string());
            text(self.h(ID_CPS), &format!("{:.1} 次/秒", s.cps));
            text(
                self.h(ID_TIME),
                &format!("{:02}:{:02}:{:02}", sec / 3600, sec / 60 % 60, sec % 60),
            );
            text(
                self.h(ID_STATUS),
                if self.test {
                    if s.running {
                        "隔离测试：模拟连点中"
                    } else {
                        "隔离测试：已停止"
                    }
                } else if s.running {
                    "正在连点 · 再按热键可停止"
                } else {
                    "已停止"
                },
            );
            text(
                self.h(ID_START),
                if s.running {
                    "停止连点"
                } else {
                    "开始连点"
                },
            );
            for id in [
                ID_BUTTON,
                ID_RIGHT,
                ID_MIDDLE,
                ID_LIMITED,
                ID_SLIDER,
                ID_INTERVAL,
                ID_LIMIT_KIND,
                ID_PROFILE,
                ID_SAVE_PROFILE,
                ID_DELETE_PROFILE,
                ID_HOTKEY,
            ] {
                EnableWindow(self.h(id), if s.running { 0 } else { 1 });
            }
            EnableWindow(
                self.h(ID_LIMIT),
                if !s.running && checked(self.h(ID_LIMITED)) {
                    1
                } else {
                    0
                },
            );
        }
        if !s.running && self.started {
            self.started = false;
            self.engine.stop();
            KillTimer(self.hwnd, TICK);
            self.save();
            if self.config.play_sound {
                MessageBeep(MB_OK);
            }
            if let Some(e) = s.error {
                self.warn(&e);
            }
        }
        if self.tray {
            let mut n = self.tray_data();
            copy_tip(
                &mut n,
                &format!(
                    "{} · {} · 本次 {} 次",
                    NAME,
                    if s.running { "连点中" } else { "已停止" },
                    s.session
                ),
            );
            Shell_NotifyIconW(NIM_MODIFY, &n);
        }
    }
    unsafe fn toggle(&mut self) {
        if !self.dialog.is_null() {
            return;
        }
        if self.engine.snapshot().running {
            self.engine.stop();
            self.refresh();
            return;
        }
        if self.ui_ready {
            if let Err(e) = self.read_config() {
                self.warn(&e);
                return;
            }
        }
        self.warn("");
        match self.engine.start(&self.config) {
            Ok(()) => {
                self.started = true;
                self.last_checkpoint = Instant::now();
                SetTimer(
                    self.hwnd,
                    TICK,
                    if IsWindowVisible(self.hwnd) != 0 {
                        250
                    } else {
                        1000
                    },
                    None,
                );
                if self.config.play_sound {
                    MessageBeep(MB_OK);
                }
                self.refresh();
            }
            Err(e) => self.warn(&e),
        }
    }
    unsafe fn tray_data(&self) -> NOTIFYICONDATAW {
        let mut n: NOTIFYICONDATAW = zeroed();
        n.cbSize = size_of::<NOTIFYICONDATAW>() as u32;
        n.hWnd = self.hwnd;
        n.uID = 1;
        n.uFlags = NIF_ICON | NIF_MESSAGE | NIF_TIP;
        n.uCallbackMessage = TRAY;
        n.hIcon = self.icon;
        copy_tip(&mut n, NAME);
        n
    }
    unsafe fn tray_add(&mut self) {
        let n = self.tray_data();
        self.tray = Shell_NotifyIconW(NIM_ADD, &n) != 0;
        if !self.tray {
            self.warn("托盘图标注册失败，窗口将保留。可用“退出”关闭程序。");
            // Registration can fail during WM_CREATE or Explorer recovery.
            // Show after this synchronous initialization transaction completes.
            PostMessageW(self.hwnd, ACTIVATE, 0, 0);
        }
    }
    unsafe fn hide(&mut self) {
        if !self.tray {
            self.tray_add();
        }
        if self.tray {
            self.save();
            ShowWindow(self.hwnd, SW_HIDE);
            if self.started {
                SetTimer(self.hwnd, TICK, 1000, None);
            }
        }
    }
    unsafe fn show(&mut self) {
        self.ensure_ui();
        ShowWindow(self.hwnd, SW_RESTORE);
        SetForegroundWindow(self.hwnd);
        if self.started {
            SetTimer(self.hwnd, TICK, 250, None);
        }
    }
    unsafe fn menu(&mut self) {
        let menu = CreatePopupMenu();
        for (id, label) in [
            (700, "打开主窗口"),
            (701, "打开设置"),
            (
                702,
                if self.engine.snapshot().running {
                    "停止连点"
                } else {
                    "开始连点"
                },
            ),
            (703, "退出"),
        ] {
            AppendMenuW(menu, MF_STRING, id, w(label).as_ptr());
        }
        let mut p: POINT = zeroed();
        GetCursorPos(&mut p);
        SetForegroundWindow(self.hwnd);
        let id = TrackPopupMenu(
            menu,
            TPM_RETURNCMD | TPM_RIGHTBUTTON,
            p.x,
            p.y,
            0,
            self.hwnd,
            null(),
        );
        DestroyMenu(menu);
        PostMessageW(self.hwnd, WM_NULL, 0, 0);
        match id {
            700 => self.show(),
            701 => {
                self.show();
                self.show_page(2);
            }
            702 => self.toggle(),
            703 => {
                DestroyWindow(self.hwnd);
            }
            _ => {}
        }
    }
    unsafe fn open_dialog(&mut self, kind: i32) {
        if !self.dialog.is_null() {
            return;
        }
        if self.engine.snapshot().running {
            return;
        }
        if kind == 1 {
            if let Err(e) = self.read_config() {
                self.warn(&e);
                return;
            }
        }
        self.dialog_kind = kind;
        self.pending_key = self.config.hot_key;
        self.pending_mods = self.config.hot_key_modifiers;
        let p = self as *mut App;
        let h = CreateWindowExW(
            WS_EX_DLGMODALFRAME,
            w("ClickRun.Rust.Dialog").as_ptr(),
            w(if kind == 1 {
                "连点器 Modern · 保存方案"
            } else {
                "连点器 Modern · 修改热键"
            })
            .as_ptr(),
            WS_CAPTION | WS_SYSMENU,
            self.px(100),
            self.px(100),
            self.px(540),
            self.px(if kind == 1 { 260 } else { 340 }),
            self.hwnd,
            null_mut(),
            GetModuleHandleW(null()),
            p as _,
        );
        if h.is_null() {
            self.register();
            self.warn("无法打开对话框。");
            return;
        }
        self.dialog = h;
        EnableWindow(self.hwnd, 0);
        ShowWindow(h, SW_SHOW);
        SetFocus(GetDlgItem(
            h,
            if kind == 1 {
                ID_DIALOG_EDIT
            } else {
                ID_CAPTURE
            },
        ));
    }
    unsafe fn dialog_control(
        &self,
        h: HWND,
        id: i32,
        class: &str,
        label: &str,
        style: u32,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
    ) -> HWND {
        let c = CreateWindowExW(
            if class == "EDIT" { WS_EX_CLIENTEDGE } else { 0 },
            w(class).as_ptr(),
            w(label).as_ptr(),
            WS_CHILD | WS_VISIBLE | style | if class == "EDIT" { WS_BORDER } else { 0 },
            self.px(x),
            self.px(y),
            self.px(width),
            self.px(height),
            h,
            id as usize as _,
            GetModuleHandleW(null()),
            null(),
        );
        SendMessageW(c, WM_SETFONT, self.font as usize, 1);
        SetWindowSubclass(c, Some(child_proc), id as usize, 0);
        c
    }
    unsafe fn dialog_create(&mut self, h: HWND) {
        if self.dialog_kind == 1 {
            self.dialog_control(h, 510, "STATIC", "方案名称(&N)", 0, 20, 20, 480, 24);
            let e = self.dialog_control(
                h,
                ID_DIALOG_EDIT,
                "EDIT",
                "",
                WS_TABSTOP | ES_AUTOHSCROLL as u32,
                20,
                50,
                480,
                32,
            );
            SendMessageW(e, EM_SETLIMITTEXT, 100, 0);
        } else {
            self.dialog_control(
                h,
                510,
                "STATIC",
                "按下新组合，或选择按键和修饰键。Esc 取消。",
                0,
                20,
                20,
                480,
                24,
            );
            self.dialog_control(h, 511, "STATIC", "按键(&K)", 0, 20, 55, 100, 24);
            let c = self.dialog_control(
                h,
                ID_KEY,
                "COMBOBOX",
                "",
                WS_TABSTOP | WS_VSCROLL | CBS_DROPDOWNLIST as u32,
                20,
                84,
                210,
                220,
            );
            self.keys = (1..=254).filter(|k| valid_key(*k)).collect();
            let mut chosen = 0;
            for (i, &k) in self.keys.iter().enumerate() {
                add(c, &key_name(k));
                if k == self.pending_key {
                    chosen = i;
                }
            }
            choose(c, chosen as i32);
            for (id, label, x, bit) in [
                (ID_CTRL, "Ctrl", 250, 2),
                (ID_ALT, "Alt", 335, 1),
                (ID_SHIFT, "Shift", 410, 4),
            ] {
                let c = self.dialog_control(
                    h,
                    id,
                    "BUTTON",
                    label,
                    WS_TABSTOP | BS_AUTOCHECKBOX as u32,
                    x,
                    84,
                    80,
                    32,
                );
                check(c, self.pending_mods & bit != 0);
            }
            self.dialog_control(
                h,
                512,
                "STATIC",
                "直接输入组合（Tab 离开，Esc 取消）",
                0,
                20,
                128,
                480,
                24,
            );
            let capture = self.dialog_control(
                h,
                ID_CAPTURE,
                "EDIT",
                &hotkey_name(self.pending_key, self.pending_mods),
                WS_TABSTOP | ES_READONLY as u32,
                20,
                154,
                480,
                32,
            );
            SetWindowSubclass(capture, Some(capture_proc), 1, self as *mut App as usize);
        }
        let y = if self.dialog_kind == 1 { 96 } else { 194 };
        self.dialog_control(h, ID_DIALOG_MSG, "STATIC", "", 0, 20, y, 480, 60);
        self.dialog_control(
            h,
            ID_DIALOG_OK,
            "BUTTON",
            "保存(&S)",
            WS_TABSTOP | BS_DEFPUSHBUTTON as u32,
            260,
            y + 68,
            110,
            34,
        );
        self.dialog_control(
            h,
            ID_DIALOG_CANCEL,
            "BUTTON",
            "取消",
            WS_TABSTOP | BS_PUSHBUTTON as u32,
            390,
            y + 68,
            110,
            34,
        );
    }
    unsafe fn dialog_save(&mut self, h: HWND) {
        if self.dialog_kind == 1 {
            let name = read(GetDlgItem(h, ID_DIALOG_EDIT)).trim().to_string();
            if name.is_empty() {
                text(GetDlgItem(h, ID_DIALOG_MSG), "请输入方案名称。");
                return;
            }
            if self
                .config
                .profiles
                .iter()
                .any(|p| p.name.to_lowercase() == name.to_lowercase())
            {
                text(
                    GetDlgItem(h, ID_DIALOG_MSG),
                    "方案名称已存在，请使用其他名称。",
                );
                return;
            }
            self.config.profiles.push(Profile {
                name,
                interval: self.config.interval,
                random_delay: self.config.random_delay,
                button: self.config.button,
                mode: self.config.mode,
                click_limit: self.config.click_limit,
                ..Default::default()
            });
            self.fill();
            choose(self.h(ID_PROFILE), self.config.profiles.len() as i32 - 1);
            self.save();
        } else {
            let n = sel(GetDlgItem(h, ID_KEY));
            if n < 0 {
                return;
            }
            let key = self.keys[n as usize];
            let mods = if checked(GetDlgItem(h, ID_CTRL)) {
                2
            } else {
                0
            } | if checked(GetDlgItem(h, ID_ALT)) { 1 } else { 0 }
                | if checked(GetDlgItem(h, ID_SHIFT)) {
                    4
                } else {
                    0
                };
            if !self.registered
                || key != self.config.hot_key
                || mods != self.config.hot_key_modifiers as u32
            {
                let new_id = if self.hotkey_id == HOTKEY {
                    HOTKEY + 1
                } else {
                    HOTKEY
                };
                if RegisterHotKey(self.hwnd, new_id, mods | MOD_NOREPEAT, key as u32) == 0 {
                    text(
                        GetDlgItem(h, ID_DIALOG_MSG),
                        "热键注册失败，原组合保留。请选择其他组合。",
                    );
                    return;
                }
                if self.registered {
                    UnregisterHotKey(self.hwnd, self.hotkey_id);
                }
                self.hotkey_id = new_id;
                self.registered = true;
            }
            self.config.hot_key = key;
            self.config.hot_key_modifiers = mods as i32;
            self.fill();
            self.save();
        }
        DestroyWindow(h);
    }
    unsafe fn command(&mut self, id: i32, code: u32) {
        if self.loading {
            return;
        }
        match id {
            ID_MAIN => self.show_page(1),
            ID_SETTINGS => self.show_page(2),
            ID_ABOUT => self.show_page(3),
            ID_START => self.toggle(),
            ID_DISMISS => self.warn(""),
            ID_HIDE => self.hide(),
            ID_EXIT => {
                DestroyWindow(self.hwnd);
            }
            ID_HOTKEY => self.open_dialog(2),
            ID_SAVE_PROFILE => self.open_dialog(1),
            ID_DELETE_PROFILE => {
                let i = sel(self.h(ID_PROFILE));
                if i >= 0 && (i as usize) < self.config.profiles.len() {
                    self.config.profiles.remove(i as usize);
                    self.fill();
                    self.save();
                }
            }
            ID_PROFILE if code == CBN_SELCHANGE => {
                let i = sel(self.h(ID_PROFILE));
                if i >= 0 {
                    if let Some(p) = self.config.profiles.get(i as usize) {
                        self.config.interval = p.interval;
                        self.config.random_delay = 0;
                        self.config.button = p.button;
                        self.config.mode = 0;
                        self.config.click_limit = p.click_limit;
                        self.fill();
                        choose(self.h(ID_PROFILE), i);
                        self.dirty();
                    }
                }
            }
            ID_BUTTON | ID_RIGHT | ID_MIDDLE | ID_LIMIT_KIND | ID_LIMITED if code == BN_CLICKED => {
                if [ID_BUTTON, ID_RIGHT, ID_MIDDLE].contains(&id) {
                    for other in [ID_BUTTON, ID_RIGHT, ID_MIDDLE] {
                        check(self.h(other), other == id);
                    }
                } else {
                    check(self.h(ID_LIMIT_KIND), id == ID_LIMIT_KIND);
                    check(self.h(ID_LIMITED), id == ID_LIMITED);
                }
                EnableWindow(
                    self.h(ID_LIMIT),
                    if checked(self.h(ID_LIMITED)) { 1 } else { 0 },
                );
                if self.read_config().is_ok() {
                    self.dirty();
                }
            }
            ID_INTERVAL | ID_LIMIT if code == EN_CHANGE => {
                if self.read_config().is_ok() {
                    self.dirty();
                }
            }
            ID_TRAY | ID_TOP | ID_SOUND => {
                self.config.minimize_to_tray = checked(self.h(ID_TRAY));
                self.config.always_on_top = checked(self.h(ID_TOP));
                self.config.play_sound = checked(self.h(ID_SOUND));
                SetWindowPos(
                    self.hwnd,
                    if self.config.always_on_top {
                        HWND_TOPMOST
                    } else {
                        HWND_NOTOPMOST
                    },
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                );
                self.dirty();
            }
            ID_AUTOSTART => {
                let enable = checked(self.h(ID_AUTOSTART));
                match system::set_autostart(enable) {
                    Ok(()) => self.warn("登录启动设置已保存。"),
                    Err(e) => self.warn(&e),
                }
                match system::autostart_state() {
                    Ok(value) => {
                        self.config.start_with_windows = value;
                        check(self.h(ID_AUTOSTART), value);
                        self.dirty();
                    }
                    Err(e) => {
                        self.warn(&e);
                        EnableWindow(self.h(ID_AUTOSTART), 0);
                    }
                }
            }
            ID_OPEN_CONFIG => {
                if let Err(e) = std::fs::create_dir_all(&self.store.dir) {
                    self.warn(&format!("无法打开配置文件夹：{e}"));
                } else {
                    ShellExecuteW(
                        self.hwnd,
                        w("open").as_ptr(),
                        w(&self.store.dir.to_string_lossy()).as_ptr(),
                        null(),
                        null(),
                        SW_SHOWNORMAL,
                    );
                }
            }
            601 | 602 => {
                ShellExecuteW(
                    self.hwnd,
                    w("open").as_ptr(),
                    w("https://github.com/gura8390/ClickRun.Modern").as_ptr(),
                    null(),
                    null(),
                    SW_SHOWNORMAL,
                );
            }
            _ => {}
        }
    }
}
fn copy_tip(n: &mut NOTIFYICONDATAW, s: &str) {
    for (a, b) in n.szTip.iter_mut().zip(s.encode_utf16().take(127)) {
        *a = b;
    }
}
fn valid_key(k: i32) -> bool {
    crate::config::valid_hot_key(k)
}
fn key_name(k: i32) -> String {
    if (112..=122).contains(&k) {
        return format!("F{}", k - 111);
    }
    if (48..=57).contains(&k) || (65..=90).contains(&k) {
        return char::from_u32(k as u32).unwrap().to_string();
    }
    unsafe {
        let scan = MapVirtualKeyW(k as u32, MAPVK_VK_TO_VSC);
        let mut b = [0u16; 80];
        let n = GetKeyNameTextW((scan << 16) as i32, b.as_mut_ptr(), 80);
        if n > 0 {
            String::from_utf16_lossy(&b[..n as usize])
        } else {
            format!("按键 {k}")
        }
    }
}
fn hotkey_name(k: i32, m: i32) -> String {
    format!(
        "{}{}{}{}",
        if m & 2 != 0 { "Ctrl+" } else { "" },
        if m & 1 != 0 { "Alt+" } else { "" },
        if m & 4 != 0 { "Shift+" } else { "" },
        key_name(k)
    )
}
unsafe extern "system" fn child_proc(
    h: HWND,
    msg: u32,
    wp: WPARAM,
    lp: LPARAM,
    _id: usize,
    _data: usize,
) -> LRESULT {
    if let Some(result) = crate::numeric_edit::handle(h, msg, wp, lp, _id) {
        return result;
    }
    if _id == ID_SLIDER as usize && msg == WM_GETOBJECT && lp as i32 == -25 {
        return crate::interval_provider::get_object(h, wp, lp);
    }
    if msg == WM_PAINT
        && (crate::style::paint_button(h, _id as i32) || crate::style::paint_slider(h))
    {
        return 0;
    }
    if msg == WM_ENABLE
        || msg == WM_SETFOCUS
        || msg == WM_KILLFOCUS
        || msg == WM_MOUSEMOVE
        || msg == WM_MOUSELEAVE
        || msg == BM_SETCHECK
    {
        InvalidateRect(h, null(), 0);
    }
    if msg == TBM_SETPOS && _id == ID_SLIDER as usize {
        let internal = !GetPropW(h, w("ClickRun.InternalSelection").as_ptr()).is_null();
        let result = DefSubclassProc(h, msg, wp, lp);
        if !internal {
            SendMessageW(
                GetParent(h),
                WM_HSCROLL,
                TB_THUMBPOSITION as usize,
                h as isize,
            );
        }
        return result;
    }
    if msg == BM_SETCHECK
        && [ID_BUTTON, ID_RIGHT, ID_MIDDLE, ID_LIMIT_KIND, ID_LIMITED].contains(&(_id as i32))
    {
        let internal = !GetPropW(h, w("ClickRun.InternalSelection").as_ptr()).is_null();
        let result = DefSubclassProc(h, msg, wp, lp);
        if !internal && wp == BST_CHECKED as usize {
            SendMessageW(GetParent(h), WM_COMMAND, _id, h as isize);
        }
        return result;
    }
    // The standard UIA ComboBox proxy changes selection with CB_SETCURSEL,
    // which does not send CBN_SELCHANGE. Bridge that semantic action to the
    // same application command as a physical selection. Our own setters are
    // explicitly marked, so fill/apply cannot create a notification loop.
    if msg == CB_SETCURSEL || msg == CB_SELECTSTRING {
        let internal = !GetPropW(h, w("ClickRun.InternalSelection").as_ptr()).is_null();
        if !internal {
            SetPropW(h, w("ClickRun.InternalSelection").as_ptr(), 1usize as _);
        }
        let result = DefSubclassProc(h, msg, wp, lp);
        if !internal {
            RemovePropW(h, w("ClickRun.InternalSelection").as_ptr());
        }
        if !internal && result != CB_ERR as isize {
            SendMessageW(
                GetParent(h),
                WM_COMMAND,
                _id | ((CBN_SELCHANGE as usize) << 16),
                h as isize,
            );
        }
        return result;
    }
    if msg == CB_SHOWDROPDOWN {
        let result = DefSubclassProc(h, msg, wp, lp);
        let mut info: COMBOBOXINFO = zeroed();
        info.cbSize = size_of::<COMBOBOXINFO>() as u32;
        if GetComboBoxInfo(h, &mut info) != 0 && !info.hwndList.is_null() {
            SetWindowSubclass(info.hwndList, Some(combo_list_proc), _id, h as usize);
        }
        return result;
    }
    if (0x201..=0x209).contains(&msg) && GetMessageExtraInfo() as usize == 0x434c4b52 {
        return 0;
    }
    if msg == WM_SETFOCUS {
        PostMessageW(GetParent(h), ENSURE_FOCUS, _id, 0);
    }
    if msg == WM_NCDESTROY {
        RemoveWindowSubclass(h, Some(child_proc), _id);
    }
    DefSubclassProc(h, msg, wp, lp)
}
unsafe extern "system" fn combo_list_proc(
    h: HWND,
    msg: u32,
    wp: WPARAM,
    lp: LPARAM,
    id: usize,
    data: usize,
) -> LRESULT {
    let combo = data as HWND;
    if msg == LB_SETCURSEL || msg == LB_SELECTSTRING {
        let result = DefSubclassProc(h, msg, wp, lp);
        if result != LB_ERR as isize
            && GetPropW(combo, w("ClickRun.InternalSelection").as_ptr()).is_null()
        {
            let selected = SendMessageW(h, LB_GETCURSEL, 0, 0);
            if selected != LB_ERR as isize {
                SendMessageW(combo, CB_SETCURSEL, selected as usize, 0);
            }
        }
        return result;
    }
    if msg == WM_NCDESTROY {
        RemoveWindowSubclass(h, Some(combo_list_proc), id);
    }
    DefSubclassProc(h, msg, wp, lp)
}
unsafe extern "system" fn capture_proc(
    h: HWND,
    msg: u32,
    wp: WPARAM,
    lp: LPARAM,
    id: usize,
    data: usize,
) -> LRESULT {
    if msg == WM_GETDLGCODE {
        if lp != 0 {
            let key = (*(lp as *const MSG)).wParam;
            if key != VK_TAB as usize && key != VK_RETURN as usize && key != VK_ESCAPE as usize {
                return (DLGC_WANTALLKEYS | DLGC_WANTCHARS) as isize;
            }
        }
        return 0;
    }
    if (msg == WM_KEYDOWN || msg == WM_SYSKEYDOWN)
        && wp != VK_TAB as usize
        && wp != VK_RETURN as usize
        && wp != VK_ESCAPE as usize
    {
        let a = &mut *(data as *mut App);
        let key = wp as i32;
        if !valid_key(key) {
            text(
                GetDlgItem(GetParent(h), ID_DIALOG_MSG),
                "此按键不可用：请避开 F12、Windows 键和单独的修饰键。",
            );
            return 0;
        }
        if GetKeyState(VK_LWIN as i32) < 0 || GetKeyState(VK_RWIN as i32) < 0 {
            return 0;
        }
        a.pending_key = key;
        a.pending_mods = if GetKeyState(VK_CONTROL as i32) < 0 {
            2
        } else {
            0
        } | if GetKeyState(VK_MENU as i32) < 0 {
            1
        } else {
            0
        } | if GetKeyState(VK_SHIFT as i32) < 0 {
            4
        } else {
            0
        };
        if let Some(i) = a.keys.iter().position(|&k| k == key) {
            choose(GetDlgItem(GetParent(h), ID_KEY), i as i32);
        }
        for (id, bit) in [(ID_CTRL, 2), (ID_ALT, 1), (ID_SHIFT, 4)] {
            check(GetDlgItem(GetParent(h), id), a.pending_mods & bit != 0);
        }
        text(h, &hotkey_name(key, a.pending_mods));
        return 0;
    }
    if msg == WM_NCDESTROY {
        RemoveWindowSubclass(h, Some(capture_proc), id);
    }
    DefSubclassProc(h, msg, wp, lp)
}
unsafe extern "system" fn dialog_proc(h: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if msg == WM_NCCREATE {
        let cs = &*(lp as *const CREATESTRUCTW);
        SetWindowLongPtrW(h, GWLP_USERDATA, cs.lpCreateParams as isize);
    }
    let p = GetWindowLongPtrW(h, GWLP_USERDATA) as *mut App;
    if p.is_null() {
        return DefWindowProcW(h, msg, wp, lp);
    }
    let a = &mut *p;
    match msg {
        WM_CREATE => {
            a.dialog_create(h);
            0
        }
        WM_COMMAND => {
            match (wp & 0xffff) as i32 {
                ID_DIALOG_OK | 1 => a.dialog_save(h),
                ID_DIALOG_CANCEL | 2 => {
                    DestroyWindow(h);
                }
                ID_KEY | ID_CTRL | ID_ALT | ID_SHIFT => {
                    let n = sel(GetDlgItem(h, ID_KEY));
                    if n >= 0 {
                        let key = a.keys[n as usize];
                        let mods = if checked(GetDlgItem(h, ID_CTRL)) {
                            2
                        } else {
                            0
                        } | if checked(GetDlgItem(h, ID_ALT)) { 1 } else { 0 }
                            | if checked(GetDlgItem(h, ID_SHIFT)) {
                                4
                            } else {
                                0
                            };
                        text(GetDlgItem(h, ID_CAPTURE), &hotkey_name(key, mods));
                    }
                }
                _ => {}
            }
            0
        }
        WM_CLOSE => {
            DestroyWindow(h);
            0
        }
        WM_DESTROY => {
            a.dialog = null_mut();
            EnableWindow(a.hwnd, 1);
            if !a.registered {
                a.register();
            }
            SetForegroundWindow(a.hwnd);
            SetFocus(a.h(if a.dialog_kind == 1 {
                ID_SAVE_PROFILE
            } else {
                ID_HOTKEY
            }));
            0
        }
        WM_CTLCOLORSTATIC => {
            SetTextColor(wp as HDC, INK);
            SetBkColor(wp as HDC, BG);
            a.brush as isize
        }
        _ => DefWindowProcW(h, msg, wp, lp),
    }
}
unsafe extern "system" fn window_proc(h: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if msg == WM_NCCREATE {
        let cs = &*(lp as *const CREATESTRUCTW);
        SetWindowLongPtrW(h, GWLP_USERDATA, cs.lpCreateParams as isize);
        (*(cs.lpCreateParams as *mut App)).hwnd = h;
    }
    let p = GetWindowLongPtrW(h, GWLP_USERDATA) as *mut App;
    if p.is_null() {
        return DefWindowProcW(h, msg, wp, lp);
    }
    let a = &mut *p;
    if msg == a.taskbar_created && a.taskbar_created != 0 {
        a.tray_add();
        return 0;
    }
    match msg {
        WM_CREATE => {
            a.core_init();
            if !a.defer_ui {
                a.ensure_ui();
            }
            0
        }
        WM_SHOWWINDOW => {
            if wp != 0 {
                // Also covers a first restore through the system UIA WindowPattern.
                a.ensure_ui();
            }
            0
        }
        WM_SIZE => {
            if wp == SIZE_MINIMIZED as usize {
                if a.config.minimize_to_tray {
                    a.hide();
                }
            } else {
                let mut r = zeroed();
                GetClientRect(h, &mut r);
                a.scroll.set(a.scroll.get().clamp(
                    0,
                    (591 + a.warning_offset() - r.bottom * 96 / a.dpi as i32).max(0),
                ));
                a.layout();
            }
            0
        }
        WM_GETMINMAXINFO => {
            let m = &mut *(lp as *mut MINMAXINFO);
            m.ptMinTrackSize.x = a.px(400);
            m.ptMinTrackSize.y = a.px(420);
            0
        }
        WM_DPICHANGED => {
            a.dpi = (wp & 0xffff) as u32;
            if a.ui_ready {
                a.make_fonts();
            }
            let r = &*(lp as *const RECT);
            SetWindowPos(
                h,
                null_mut(),
                r.left,
                r.top,
                r.right - r.left,
                r.bottom - r.top,
                SWP_NOACTIVATE | SWP_NOZORDER,
            );
            a.layout();
            0
        }
        WM_COMMAND => {
            a.command((wp & 0xffff) as i32, ((wp >> 16) & 0xffff) as u32);
            0
        }
        WM_HSCROLL => {
            if lp == a.h(ID_SLIDER) as isize && !a.loading {
                let pos = SendMessageW(a.h(ID_SLIDER), TBM_GETPOS, 0, 0) as i32;
                text(a.h(ID_INTERVAL), &pos.to_string());
            }
            0
        }
        WM_HOTKEY => {
            if wp == a.hotkey_id as usize
                && a.registered
                && (lp as u32 & 0xffff) == a.config.hot_key_modifiers as u32
                && ((lp as u32 >> 16) & 0xffff) == a.config.hot_key as u32
            {
                a.toggle();
            }
            0
        }
        ENSURE_FOCUS => {
            a.ensure_focus(wp as i32);
            0
        }
        WM_VSCROLL => {
            let code = (wp & 0xffff) as i32;
            let mut si: SCROLLINFO = zeroed();
            si.cbSize = size_of::<SCROLLINFO>() as u32;
            si.fMask = SIF_TRACKPOS;
            GetScrollInfo(h, SB_VERT, &mut si);
            let n = match code {
                SB_LINEUP => a.scroll.get() - 32,
                SB_LINEDOWN => a.scroll.get() + 32,
                SB_PAGEUP => a.scroll.get() - 240,
                SB_PAGEDOWN => a.scroll.get() + 240,
                SB_THUMBPOSITION | SB_THUMBTRACK => si.nTrackPos,
                _ => a.scroll.get(),
            };
            a.scroll_to(n);
            0
        }
        WM_MOUSEWHEEL => {
            a.scroll_to(a.scroll.get() - ((wp >> 16) as u16 as i16 as i32) * 48 / 120);
            0
        }
        WM_TIMER => {
            if wp == SAVE {
                a.save();
            } else if wp == TICK {
                a.refresh();
                if a.started && a.last_checkpoint.elapsed().as_secs() >= 5 {
                    a.save();
                    a.last_checkpoint = Instant::now();
                }
            }
            0
        }
        ACTIVATE => {
            a.show();
            0
        }
        TRAY => {
            match lp as u32 {
                WM_LBUTTONDBLCLK | NIN_KEYSELECT => a.show(),
                WM_RBUTTONUP | WM_CONTEXTMENU => a.menu(),
                _ => {}
            }
            0
        }
        WM_CLOSE => {
            if !a.dialog.is_null() {
                SendMessageW(a.dialog, WM_CLOSE, 0, 0);
            }
            if a.config.minimize_to_tray {
                a.hide();
            } else {
                DestroyWindow(h);
            }
            0
        }
        WM_QUERYENDSESSION => {
            a.engine.stop();
            a.save();
            1
        }
        WM_DESTROY => {
            if !a.dialog.is_null() {
                DestroyWindow(a.dialog);
            }
            a.engine.stop();
            a.save();
            UnregisterHotKey(h, a.hotkey_id);
            if a.tray {
                let n = a.tray_data();
                Shell_NotifyIconW(NIM_DELETE, &n);
            }
            PostQuitMessage(0);
            0
        }
        WM_CTLCOLORSTATIC | WM_CTLCOLOREDIT | WM_CTLCOLORBTN => {
            let id = GetDlgCtrlID(lp as HWND);
            if id == ID_HOTKEY_VALUE {
                SetTextColor(wp as HDC, 0xffffff);
                SetBkColor(wp as HDC, INK);
                return a.ink_brush as isize;
            }
            SetTextColor(
                wp as HDC,
                if [ID_SESSION, ID_TIME, ID_CPS, ID_TOTAL].contains(&id) {
                    0x0060340f
                } else if id == ID_WARNING {
                    0x003834d1
                } else if id >= 1000 || [2, ID_STATUS].contains(&id) {
                    0x00666666
                } else {
                    INK
                },
            );
            SetBkColor(wp as HDC, BG);
            a.brush as isize
        }
        WM_PAINT => {
            let mut ps: PAINTSTRUCT = zeroed();
            let dc = BeginPaint(h, &mut ps);
            let mut r = zeroed();
            GetClientRect(h, &mut r);
            FillRect(dc, &r, a.brush);
            let pen = CreatePen(PS_SOLID, a.px(1).max(1), 0x00c8d0d0);
            let old = SelectObject(dc, pen as _);
            let offset = a.warning_offset();
            let lines: &[i32] = if a.page == 1 {
                &[172, 310, 406]
            } else {
                &[84, 406]
            };
            for &y in lines {
                let y = a.px(y + if y >= 86 { offset } else { 0 } - a.scroll.get());
                MoveToEx(dc, a.px(20), y, null_mut());
                LineTo(dc, r.right - a.px(20), y);
            }
            SelectObject(dc, old);
            DeleteObject(pen as _);
            EndPaint(h, &ps);
            0
        }
        WM_ERASEBKGND => {
            let mut r = zeroed();
            GetClientRect(h, &mut r);
            FillRect(wp as HDC, &r, a.brush);
            1
        }
        _ => DefWindowProcW(h, msg, wp, lp),
    }
}
pub fn run() {
    unsafe {
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let args: Vec<_> = std::env::args().collect();
        let config_dir = args
            .windows(2)
            .find(|x| x[0] == "--config-dir")
            .map(|x| PathBuf::from(&x[1]));
        let test = args.iter().any(|x| x == "--test-mode");
        let isolated = config_dir.is_some();
        if test && !isolated {
            MessageBoxW(
                null_mut(),
                w("隔离测试必须指定 --config-dir，避免读写正式配置。").as_ptr(),
                w(NAME).as_ptr(),
                MB_OK | MB_ICONERROR,
            );
            return;
        }
        if isolated {
            if std::env::var_os("CLICKRUN_TEST_SYSTEM_DIR").is_none() {
                std::env::set_var(
                    "CLICKRUN_TEST_SYSTEM_DIR",
                    config_dir.as_ref().unwrap().join("system"),
                );
            }
            if std::env::var_os("CLICKRUN_TEST_LOG_DIR").is_none() {
                std::env::set_var(
                    "CLICKRUN_TEST_LOG_DIR",
                    config_dir.as_ref().unwrap().join("logs"),
                );
            }
        }
        let prefix = if isolated {
            format!(
                "ClickRun.Rust.Test.{}",
                config_dir
                    .as_ref()
                    .unwrap()
                    .to_string_lossy()
                    .chars()
                    .map(|x| if x.is_alphanumeric() { x } else { '_' })
                    .collect::<String>()
            )
        } else {
            "ClickRun.SingleInstance".into()
        };
        let event = CreateEventW(null(), 0, 0, w(&format!("{prefix}.Activate")).as_ptr());
        let mutex = CreateMutexW(null(), 0, w(&format!("{prefix}.Mutex")).as_ptr());
        if mutex.is_null() || event.is_null() {
            MessageBoxW(
                null_mut(),
                w("无法创建单实例对象，程序未启动。").as_ptr(),
                w(NAME).as_ptr(),
                MB_OK | MB_ICONERROR,
            );
            if !mutex.is_null() {
                CloseHandle(mutex);
            }
            if !event.is_null() {
                CloseHandle(event);
            }
            return;
        }
        let acquired = WaitForSingleObject(mutex, 0);
        if acquired != WAIT_OBJECT_0 && acquired != WAIT_ABANDONED {
            SetEvent(event);
            CloseHandle(event);
            CloseHandle(mutex);
            return;
        }
        let store = ConfigStore::new(config_dir.unwrap_or_else(crate::config::default_dir));
        let (config, warning) = store.load();
        let font_path = std::env::current_exe()
            .unwrap_or_default()
            .parent()
            .unwrap_or(std::path::Path::new("."))
            .join("assets/FZGWKaiS.ttf");
        let mut a = Box::new(App {
            hwnd: null_mut(),
            controls: BTreeMap::new(),
            engine: Engine::new(config.total_clicks, test),
            config,
            store,
            page: 1,
            dpi: 96,
            scroll: Cell::new(0),
            font: null_mut(),
            heading: null_mut(),
            caption: null_mut(),
            data_font: null_mut(),
            brush: CreateSolidBrush(BG),
            ink_brush: CreateSolidBrush(INK),
            icon: LoadImageW(
                GetModuleHandleW(null()),
                32512 as *const u16,
                IMAGE_ICON,
                0,
                0,
                LR_DEFAULTSIZE,
            ) as HICON,
            tray: false,
            taskbar_created: RegisterWindowMessageW(w("TaskbarCreated").as_ptr()),
            loading: false,
            registered: false,
            hotkey_id: HOTKEY,
            test,
            dialog: null_mut(),
            dialog_kind: 0,
            keys: Vec::new(),
            pending_key: 117,
            pending_mods: 0,
            started: false,
            last_checkpoint: Instant::now(),
            font_path,
            font_registered: false,
            names: None,
            defer_ui: args.iter().any(|argument| argument == "--tray"),
            ui_ready: false,
            ui_creating: false,
            pending_warning: RefCell::new(String::new()),
            layout_busy: Cell::new(false),
        });
        // winresource's first icon is ID 1.
        let icon = LoadImageW(
            GetModuleHandleW(null()),
            1 as *const u16,
            IMAGE_ICON,
            0,
            0,
            LR_DEFAULTSIZE,
        ) as HICON;
        if !icon.is_null() {
            a.icon = icon;
        }
        for (class, proc) in [
            (
                "ClickRun.Rust.Main",
                Some(window_proc as unsafe extern "system" fn(_, _, _, _) -> _),
            ),
            (
                "ClickRun.Rust.Dialog",
                Some(dialog_proc as unsafe extern "system" fn(_, _, _, _) -> _),
            ),
        ] {
            let class_name = w(class);
            let mut wc: WNDCLASSEXW = zeroed();
            wc.cbSize = size_of::<WNDCLASSEXW>() as u32;
            wc.style = CS_HREDRAW | CS_VREDRAW;
            wc.lpfnWndProc = proc;
            wc.hInstance = GetModuleHandleW(null());
            wc.hCursor = LoadCursorW(null_mut(), IDC_ARROW);
            wc.hbrBackground = a.brush;
            wc.hIcon = a.icon;
            wc.hIconSm = a.icon;
            wc.lpszClassName = class_name.as_ptr();
            if RegisterClassExW(&wc) == 0 {
                crate::log::write("窗口类注册失败。");
            }
        }
        a.dpi = GetDpiForSystem().max(96);
        let mut work: RECT = zeroed();
        SystemParametersInfoW(SPI_GETWORKAREA, 0, &mut work as *mut RECT as _, 0);
        let width = a.px(460).min(work.right - work.left);
        let height = a.px(640).min(work.bottom - work.top);
        let h = CreateWindowExW(
            WS_EX_CONTROLPARENT,
            w("ClickRun.Rust.Main").as_ptr(),
            w(NAME).as_ptr(),
            WS_OVERLAPPEDWINDOW | WS_VSCROLL | WS_CLIPCHILDREN,
            work.left + (work.right - work.left - width) / 2,
            work.top + (work.bottom - work.top - height) / 2,
            width,
            height,
            null_mut(),
            null_mut(),
            GetModuleHandleW(null()),
            &mut *a as *mut App as *const c_void,
        );
        if h.is_null() {
            MessageBoxW(
                null_mut(),
                w("无法创建主窗口。").as_ptr(),
                w(NAME).as_ptr(),
                MB_OK | MB_ICONERROR,
            );
        } else {
            if let Some(warning) = warning {
                a.warn(&warning);
            }
            if !args.iter().any(|x| x == "--tray") || !a.tray {
                ShowWindow(h, SW_SHOWNORMAL);
            }
            // Wait for both the cross-process activation event and UI messages: no idle polling thread.
            let mut msg: MSG = zeroed();
            'outer: loop {
                let r = MsgWaitForMultipleObjectsEx(
                    1,
                    &event,
                    INFINITE,
                    QS_ALLINPUT,
                    MWMO_INPUTAVAILABLE,
                );
                if r == WAIT_OBJECT_0 {
                    PostMessageW(h, ACTIVATE, 0, 0);
                }
                while PeekMessageW(&mut msg, null_mut(), 0, 0, PM_REMOVE) != 0 {
                    if msg.message == WM_QUIT {
                        break 'outer;
                    }
                    let target = if !a.dialog.is_null() { a.dialog } else { h };
                    if IsDialogMessageW(target, &msg) == 0 {
                        TranslateMessage(&msg);
                        DispatchMessageW(&msg);
                    }
                }
                if r == WAIT_FAILED {
                    break;
                }
            }
        }
        DeleteObject(a.font as _);
        DeleteObject(a.heading as _);
        DeleteObject(a.caption as _);
        DeleteObject(a.data_font as _);
        DeleteObject(a.ink_brush as _);
        DeleteObject(a.brush as _);
        if !a.icon.is_null() {
            DestroyIcon(a.icon);
        }
        if a.font_registered {
            RemoveFontResourceExW(
                w(&a.font_path.to_string_lossy()).as_ptr(),
                FR_PRIVATE,
                null(),
            );
        }
        ReleaseMutex(mutex);
        CloseHandle(mutex);
        CloseHandle(event);
    }
}
