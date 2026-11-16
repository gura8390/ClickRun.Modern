//! 为紧凑排版中不相邻的原生标签提供系统无障碍名称，不添加隐藏视觉控件。
use windows::{
    Win32::{
        Foundation::{HWND, RPC_E_CHANGED_MODE},
        System::Com::{
            CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
            CoUninitialize,
        },
        UI::Accessibility::{CAccPropServices, IAccPropServices, PROPID_ACC_NAME},
    },
    core::PCWSTR,
};
pub struct Names {
    service: Option<IAccPropServices>,
    initialized: bool,
}
impl Names {
    pub fn new() -> Result<Self, String> {
        unsafe {
            let hr = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            let initialized = hr.is_ok();
            if !initialized && hr != RPC_E_CHANGED_MODE {
                return Err(format!("无法初始化无障碍名称服务：{hr}"));
            }
            match CoCreateInstance(&CAccPropServices, None, CLSCTX_INPROC_SERVER) {
                Ok(service) => Ok(Self {
                    service: Some(service),
                    initialized,
                }),
                Err(e) => {
                    if initialized {
                        CoUninitialize();
                    }
                    Err(format!("无法创建无障碍名称服务：{e}"))
                }
            }
        }
    }
    pub fn set(&self, h: windows_sys::Win32::Foundation::HWND, name: &str) -> Result<(), String> {
        let wide: Vec<_> = name.encode_utf16().chain(Some(0)).collect();
        unsafe {
            self.service
                .as_ref()
                .unwrap()
                .SetHwndPropStr(
                    HWND(h),
                    (-4i32) as u32,
                    0,
                    PROPID_ACC_NAME,
                    PCWSTR(wide.as_ptr()),
                )
                .map_err(|e| format!("无法设置无障碍名称：{e}"))
        }
    }
}
impl Drop for Names {
    fn drop(&mut self) {
        drop(self.service.take());
        if self.initialized {
            unsafe {
                CoUninitialize();
            }
        }
    }
}
