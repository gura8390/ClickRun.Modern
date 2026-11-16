//! 数字框沿用原生单行 EDIT，只调整非客户区，让文字区垂直居中。
//! 不拦截文本、选择、键盘或无障碍消息，也不自行绘制文字。
use std::mem::zeroed;
use std::ptr::null_mut;
use windows_sys::Win32::{
    Foundation::{HWND, POINT, RECT},
    Graphics::Gdi::{
        ClientToScreen, CreateSolidBrush, DEFAULT_GUI_FONT, DeleteObject, ExcludeClipRect,
        FillRect, GetDC, GetStockObject, GetTextMetricsW, GetWindowDC, ReleaseDC, RestoreDC,
        SaveDC, SelectObject, TEXTMETRICW,
    },
    UI::{
        HiDpi::GetDpiForWindow,
        Shell::DefSubclassProc,
        WindowsAndMessaging::{
            GetClientRect, GetWindowRect, NCCALCSIZE_PARAMS, SWP_FRAMECHANGED, SWP_NOACTIVATE,
            SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, SendMessageW, SetWindowPos, WM_GETFONT,
            WM_NCCALCSIZE, WM_NCPAINT, WM_SETFONT,
        },
    },
};

const PAPER: u32 = 0x00f0f5f5; // RGB #f5f5f0，COLORREF 按 BGR 编码。
const BORDER: u32 = 0x00c8d0d0; // RGB #d0d0c8。

/// child_proc 在最前调用；Some 表示已经处理，None 继续原有子类流程。
/// h 必须是 id 对应的有效原生 EDIT 窗口，消息参数须符合 Win32 约定。
pub unsafe fn handle(h: HWND, msg: u32, wp: usize, lp: isize, id: usize) -> Option<isize> {
    if !matches!(id, 203 | 206) {
        return None;
    }
    match msg {
        WM_NCCALCSIZE => {
            // rgrc[0]/RECT 在默认处理前是拟议的外框，处理后才是客户区。
            // 有些 EDIT 将 WS_BORDER 画在客户区，不能依赖默认处理预留左右边框。
            let outer = if lp == 0 {
                None
            } else if wp != 0 {
                Some(unsafe { (*(lp as *const NCCALCSIZE_PARAMS)).rgrc[0] })
            } else {
                Some(unsafe { *(lp as *const RECT) })
            };
            let result = unsafe { DefSubclassProc(h, msg, wp, lp) };
            if lp != 0 {
                let client = if wp != 0 {
                    unsafe { &mut (*(lp as *mut NCCALCSIZE_PARAMS)).rgrc[0] }
                } else {
                    unsafe { &mut *(lp as *mut RECT) }
                };
                if let Some(outer) = outer {
                    let width = outer.right.saturating_sub(outer.left);
                    if width > 0 {
                        let dpi = unsafe { GetDpiForWindow(h) }.max(96);
                        let line = ((dpi + 48) / 96) as i32;
                        let padding = ((4 * dpi + 48) / 96) as i32;
                        let requested = line.saturating_add(padding);
                        // 默认框架若更宽则保留；统一采用左右较大的边距以保持对称。
                        let margin = requested
                            .max(client.left.saturating_sub(outer.left))
                            .max(outer.right.saturating_sub(client.right))
                            .min(width.saturating_sub(1) / 2);
                        // 极窄/资源不足场景至少留下一个客户区像素，不越出拟议外框。
                        client.left = outer.left.saturating_add(margin);
                        client.right = outer.right.saturating_sub(margin);
                    } else {
                        client.left = outer.left;
                        client.right = outer.left;
                    }
                }
                if let Some(font_height) = unsafe { font_height(h) } {
                    let available = client.bottom.saturating_sub(client.top);
                    let desired = font_height.saturating_add(2).min(available);
                    if desired > 0 && available > desired {
                        // 奇数余量放在底部；上下边距差最多 1 个物理像素。
                        client.top += (available - desired) / 2;
                        client.bottom = client.top + desired;
                    }
                }
            }
            Some(result)
        }
        WM_NCPAINT => {
            unsafe { paint_nonclient(h) };
            // 不再执行默认凹陷边框绘制；客户区的原生 EDIT 文本绘制不受影响。
            Some(0)
        }
        WM_SETFONT => {
            let result = unsafe { DefSubclassProc(h, msg, wp, lp) };
            unsafe {
                SetWindowPos(
                    h,
                    null_mut(),
                    0,
                    0,
                    0,
                    0,
                    SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_NOZORDER,
                );
            }
            Some(result)
        }
        _ => None,
    }
}

unsafe fn font_height(h: HWND) -> Option<i32> {
    let dc = unsafe { GetDC(h) };
    if dc.is_null() {
        return None;
    }
    let requested = unsafe { SendMessageW(h, WM_GETFONT, 0, 0) };
    let font = if requested == 0 {
        unsafe { GetStockObject(DEFAULT_GUI_FONT) }
    } else {
        requested as _
    };
    let previous = if font.is_null() {
        null_mut()
    } else {
        unsafe { SelectObject(dc, font) }
    };
    let mut metrics: TEXTMETRICW = unsafe { zeroed() };
    let success = unsafe { GetTextMetricsW(dc, &mut metrics) } != 0;
    if !previous.is_null() && previous as isize != -1 {
        unsafe { SelectObject(dc, previous) };
    }
    unsafe { ReleaseDC(h, dc) };
    if success && metrics.tmHeight > 0 {
        Some(metrics.tmHeight)
    } else {
        None
    }
}

unsafe fn paint_nonclient(h: HWND) {
    let mut window: RECT = unsafe { zeroed() };
    let mut client: RECT = unsafe { zeroed() };
    let mut origin = POINT { x: 0, y: 0 };
    if unsafe { GetWindowRect(h, &mut window) } == 0
        || unsafe { GetClientRect(h, &mut client) } == 0
        || unsafe { ClientToScreen(h, &mut origin) } == 0
    {
        return;
    }
    let width = window.right - window.left;
    let height = window.bottom - window.top;
    if width <= 0 || height <= 0 {
        return;
    }
    let dc = unsafe { GetWindowDC(h) };
    if dc.is_null() {
        return;
    }
    let saved = unsafe { SaveDC(dc) };
    // Window DC 的原点在窗口左上角；client 原点由系统转换，禁止猜边距。
    let left = origin.x - window.left;
    let top = origin.y - window.top;
    let clipped = unsafe {
        ExcludeClipRect(
            dc,
            left,
            top,
            left + client.right - client.left,
            top + client.bottom - client.top,
        )
    };
    if clipped == 0 {
        // 无法排除客户区时不绘制，避免在系统资源不足时擦掉文字或选择。
        if saved != 0 {
            unsafe { RestoreDC(dc, saved) };
        }
        unsafe { ReleaseDC(h, dc) };
        return;
    }
    let border = unsafe { CreateSolidBrush(BORDER) };
    let paper = unsafe { CreateSolidBrush(PAPER) };
    let outer = RECT {
        left: 0,
        top: 0,
        right: width,
        bottom: height,
    };
    // 一逻辑像素映射到当前 DPI 的整数物理像素，至少保留一像素线。
    let line = ((unsafe { GetDpiForWindow(h) }.max(96) + 48) / 96) as i32;
    let inset = line.min(width / 2).min(height / 2).max(0);
    let inner = RECT {
        left: inset,
        top: inset,
        right: width - inset,
        bottom: height - inset,
    };
    if !border.is_null() {
        unsafe { FillRect(dc, &outer, border) };
    }
    if !paper.is_null() && inner.right > inner.left && inner.bottom > inner.top {
        unsafe { FillRect(dc, &inner, paper) };
    }
    if !paper.is_null() {
        unsafe { DeleteObject(paper as _) };
    }
    if !border.is_null() {
        unsafe { DeleteObject(border as _) };
    }
    if saved != 0 {
        unsafe { RestoreDC(dc, saved) };
    }
    unsafe { ReleaseDC(h, dc) };
}
