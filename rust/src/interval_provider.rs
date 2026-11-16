//! 间隔滑块的毫秒 UIA 语义；标准 TRACKBAR 继续负责绘制、键盘及拖动。
//! COM 回调按创建 STA 调度，只保存 HWND 数值，不保存 App 或窗口过程的数据指针。
#![allow(non_snake_case)]

use windows::{
    Win32::{
        Foundation::{
            E_FAIL, E_INVALIDARG, E_NOINTERFACE, HWND as ComHwnd, LPARAM, RPC_E_WRONG_THREAD,
            WPARAM,
        },
        System::Variant::VARIANT,
        UI::Accessibility::*,
    },
    core::{BOOL, Error, HRESULT, IUnknown, IUnknownImpl, Interface, Result, implement},
};
use windows_sys::Win32::{
    Foundation::HWND,
    System::Threading::GetCurrentThreadId,
    UI::{
        Input::KeyboardAndMouse::{GetFocus, IsWindowEnabled},
        WindowsAndMessaging::{
            GWL_STYLE, GetClassNameW, GetDlgCtrlID, GetParent, GetWindowLongPtrW,
            GetWindowThreadProcessId, IsWindow, SendMessageW, WS_TABSTOP,
        },
    },
};

const INTERVAL_ID: i32 = 223;
const TBM_GETPOS: u32 = 0x0400;
const TBM_SETPOS: u32 = 0x0405;

fn unavailable() -> Error {
    Error::from_hresult(HRESULT(UIA_E_ELEMENTNOTAVAILABLE as i32))
}

fn effective_enabled(mut hwnd: HWND) -> bool {
    // 禁用主窗的模态场景同样禁止写入，不能只观察子窗口自身的 WS_DISABLED。
    while !hwnd.is_null() {
        if unsafe { IsWindowEnabled(hwnd) } == 0 {
            return false;
        }
        hwnd = unsafe { GetParent(hwnd) };
    }
    true
}

#[implement(IRawElementProviderSimple, IRangeValueProvider, Agile = false)]
struct IntervalProvider {
    window: usize,
    owner_thread: u32,
}

impl IntervalProvider {
    fn new(hwnd: HWND) -> Option<Self> {
        let owner_thread = unsafe { GetWindowThreadProcessId(hwnd, std::ptr::null_mut()) };
        if owner_thread == 0 {
            return None;
        }
        let provider = Self {
            window: hwnd as usize,
            owner_thread,
        };
        provider.hwnd().ok()?;
        Some(provider)
    }

    fn hwnd(&self) -> Result<HWND> {
        // 配合 Agile=false 和 UseComThreading，拒绝未正确封送的跨 apartment 调用。
        if unsafe { GetCurrentThreadId() } != self.owner_thread {
            return Err(RPC_E_WRONG_THREAD.into());
        }
        let hwnd = self.window as HWND;
        if unsafe { IsWindow(hwnd) } == 0
            || unsafe { GetDlgCtrlID(hwnd) } != INTERVAL_ID
            || unsafe { GetWindowThreadProcessId(hwnd, std::ptr::null_mut()) } != self.owner_thread
        {
            return Err(unavailable());
        }
        let mut class = [0u16; 32];
        let length =
            unsafe { GetClassNameW(hwnd, class.as_mut_ptr(), class.len() as i32) }.max(0) as usize;
        let expected = b"msctls_trackbar32";
        if length != expected.len()
            || !class[..length]
                .iter()
                .zip(expected)
                .all(|(a, b)| *a <= 127 && (*a as u8).eq_ignore_ascii_case(b))
        {
            return Err(unavailable());
        }
        Ok(hwnd)
    }

    fn value(&self) -> Result<f64> {
        let hwnd = self.hwnd()?;
        Ok(unsafe { SendMessageW(hwnd, TBM_GETPOS, 0, 0) } as f64)
    }
}

impl IRawElementProviderSimple_Impl for IntervalProvider_Impl {
    fn ProviderOptions(&self) -> Result<ProviderOptions> {
        self.hwnd()?;
        Ok(ProviderOptions_ServerSideProvider | ProviderOptions_UseComThreading)
    }

    fn GetPatternProvider(&self, pattern: UIA_PATTERN_ID) -> Result<IUnknown> {
        self.hwnd()?;
        if pattern == UIA_RangeValuePatternId {
            self.to_interface::<IRangeValueProvider>().cast()
        } else {
            Err(E_NOINTERFACE.into())
        }
    }

    fn GetPropertyValue(&self, property: UIA_PROPERTY_ID) -> Result<VARIANT> {
        let hwnd = self.hwnd()?;
        Ok(if property == UIA_NamePropertyId {
            VARIANT::from("点击间隔（毫秒）")
        } else if property == UIA_AutomationIdPropertyId {
            VARIANT::from("223")
        } else if property == UIA_ControlTypePropertyId {
            VARIANT::from(UIA_SliderControlTypeId.0)
        } else if property == UIA_LocalizedControlTypePropertyId {
            VARIANT::from("滑块")
        } else if property == UIA_ClassNamePropertyId {
            VARIANT::from("msctls_trackbar32")
        } else if property == UIA_FrameworkIdPropertyId {
            VARIANT::from("Win32")
        } else if property == UIA_HelpTextPropertyId {
            VARIANT::from(
                "点击间隔，单位毫秒。范围 1–10000 毫秒；方向键每次 1 毫秒，翻页键每次 100 毫秒。",
            )
        } else if property == UIA_IsEnabledPropertyId {
            VARIANT::from(effective_enabled(hwnd))
        } else if property == UIA_HasKeyboardFocusPropertyId {
            VARIANT::from(unsafe { GetFocus() } == hwnd)
        } else if property == UIA_IsKeyboardFocusablePropertyId {
            VARIANT::from(
                effective_enabled(hwnd)
                    && unsafe { GetWindowLongPtrW(hwnd, GWL_STYLE) } as u32 & WS_TABSTOP != 0,
            )
        } else if property == UIA_IsControlElementPropertyId
            || property == UIA_IsContentElementPropertyId
            || property == UIA_IsRangeValuePatternAvailablePropertyId
        {
            VARIANT::from(true)
        } else if property == UIA_RangeValueValuePropertyId {
            VARIANT::from(self.value()?)
        } else if property == UIA_RangeValueMinimumPropertyId
            || property == UIA_RangeValueSmallChangePropertyId
        {
            VARIANT::from(1.0)
        } else if property == UIA_RangeValueMaximumPropertyId {
            VARIANT::from(10000.0)
        } else if property == UIA_RangeValueLargeChangePropertyId {
            VARIANT::from(100.0)
        } else if property == UIA_RangeValueIsReadOnlyPropertyId {
            VARIANT::from(false)
        } else {
            VARIANT::default()
        })
    }

    fn HostRawElementProvider(&self) -> Result<IRawElementProviderSimple> {
        unsafe { UiaHostProviderFromHwnd(ComHwnd(self.hwnd()?)) }
    }
}

impl IRangeValueProvider_Impl for IntervalProvider_Impl {
    fn SetValue(&self, value: f64) -> Result<()> {
        let hwnd = self.hwnd()?;
        if !effective_enabled(hwnd) {
            return Err(Error::from_hresult(HRESULT(UIA_E_ELEMENTNOTENABLED as i32)));
        }
        if !value.is_finite() || !(1.0..=10000.0).contains(&value) || value.fract() != 0.0 {
            return Err(E_INVALIDARG.into());
        }
        let old_value = self.value()?;
        // 主窗口已有的 TBM_SETPOS 处理同步配置与数值框；这里不访问 App 内存。
        unsafe {
            SendMessageW(hwnd, TBM_SETPOS, 1, value as isize);
        }
        let actual = self.value()?;
        if actual != value {
            return Err(E_FAIL.into());
        }
        if old_value != actual {
            let provider = self.to_interface::<IRawElementProviderSimple>();
            let _ = unsafe {
                UiaRaiseAutomationPropertyChangedEvent(
                    &provider,
                    UIA_RangeValueValuePropertyId,
                    &VARIANT::from(old_value),
                    &VARIANT::from(actual),
                )
            };
        }
        Ok(())
    }

    fn Value(&self) -> Result<f64> {
        self.value()
    }
    fn IsReadOnly(&self) -> Result<BOOL> {
        self.hwnd()?;
        Ok(BOOL::from(false))
    }
    fn Maximum(&self) -> Result<f64> {
        self.hwnd()?;
        Ok(10000.0)
    }
    fn Minimum(&self) -> Result<f64> {
        self.hwnd()?;
        Ok(1.0)
    }
    fn LargeChange(&self) -> Result<f64> {
        self.hwnd()?;
        Ok(100.0)
    }
    fn SmallChange(&self) -> Result<f64> {
        self.hwnd()?;
        Ok(1.0)
    }
}

/// 仅用于 ID223 的 WM_GETOBJECT / UiaRootObjectId；UIA 持有返回 COM 对象的引用。
pub unsafe fn get_object(hwnd: HWND, wparam: usize, lparam: isize) -> isize {
    if lparam != UiaRootObjectId as isize {
        return 0;
    }
    let Some(provider) = IntervalProvider::new(hwnd) else {
        return 0;
    };
    let provider: IRawElementProviderSimple = provider.into();
    unsafe {
        UiaReturnRawElementProvider(ComHwnd(hwnd), WPARAM(wparam), LPARAM(lparam), &provider).0
    }
}
