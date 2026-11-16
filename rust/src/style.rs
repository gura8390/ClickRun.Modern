//! 标准 BUTTON 的轻量绘制。只接管外观，不改变窗口类型、键盘处理或无障碍动作。
#![allow(unsafe_op_in_unsafe_fn)]

use std::mem::zeroed;
use windows_sys::Win32::{
    Foundation::{HWND, RECT},
    Graphics::Gdi::{
        BeginPaint, CreatePen, CreateSolidBrush, DT_CENTER, DT_END_ELLIPSIS, DT_HIDEPREFIX,
        DT_SINGLELINE, DT_VCENTER, DeleteObject, DrawFocusRect, DrawTextW, Ellipse, EndPaint,
        FillRect, GetStockObject, LineTo, MoveToEx, NULL_PEN, PAINTSTRUCT, PS_SOLID, RoundRect,
        SelectObject, SetBkMode, SetTextColor, TRANSPARENT,
    },
    UI::{
        Controls::{BST_CHECKED, BST_HOT, TBM_GETCHANNELRECT, TBM_GETTHUMBRECT},
        HiDpi::GetDpiForWindow,
        Input::KeyboardAndMouse::{GetFocus, IsWindowEnabled},
        WindowsAndMessaging::{
            BM_GETCHECK, BM_GETSTATE, BS_DEFPUSHBUTTON, BS_PUSHBUTTON, BS_TYPEMASK, BST_PUSHED,
            GWL_STYLE, GetClassNameW, GetClientRect, GetWindowLongPtrW, GetWindowTextW,
            SendMessageW, UISF_HIDEACCEL, WM_GETFONT, WM_QUERYUISTATE,
        },
    },
};

// COLORREF 按 0x00BBGGRR 存储。
const PAPER: u32 = 0x00f0f5f5;
const INK: u32 = 0x002e1a1a;
const LINE: u32 = 0x00c8d0d0;
const HOVER: u32 = 0x00e4eaea;
const GREEN: u32 = 0x002d8f2d;
const RED: u32 = 0x003834d1;
const ACCENT: u32 = 0x006045e9;
const WHITE: u32 = 0x00ffffff;
const SECONDARY: u32 = 0x00666666;

fn is_segment(id: i32) -> bool {
    matches!(id, 201 | 220 | 221 | 205 | 222)
}

unsafe fn class_is(h: HWND, expected: &[u8]) -> bool {
    let mut class = [0u16; 64];
    let length = GetClassNameW(h, class.as_mut_ptr(), class.len() as i32).max(0) as usize;
    length == expected.len()
        && class[..length]
            .iter()
            .zip(expected)
            .all(|(a, b)| *a <= 127 && (*a as u8).eq_ignore_ascii_case(b))
}

/// 在标准 BUTTON 的 WM_PAINT 中调用；true 表示已完成 BeginPaint/EndPaint。
/// 复选框及其他未支持的控件返回 false，交还原生窗口过程绘制。
pub unsafe fn paint_button(h: HWND, id: i32) -> bool {
    if h.is_null() || (302..=305).contains(&id) {
        return false;
    }
    if !class_is(h, b"BUTTON") {
        return false;
    }
    let button_type = GetWindowLongPtrW(h, GWL_STYLE) as i32 & BS_TYPEMASK;
    if !is_segment(id) && !matches!(button_type, BS_PUSHBUTTON | BS_DEFPUSHBUTTON) {
        // 也保留热键弹窗 Ctrl/Alt/Shift 等标准复选框的原生勾选外观。
        return false;
    }

    let state = SendMessageW(h, BM_GETSTATE, 0, 0) as u32;
    let selected = is_segment(id) && SendMessageW(h, BM_GETCHECK, 0, 0) as u32 == BST_CHECKED;
    let enabled = IsWindowEnabled(h) != 0;
    let focused = GetFocus() == h;
    let hot_or_pressed = state & (BST_HOT | BST_PUSHED) != 0;
    let font = SendMessageW(h, WM_GETFONT, 0, 0) as *mut std::ffi::c_void;
    let ui_state = SendMessageW(h, WM_QUERYUISTATE, 0, 0) as u32;

    // 按钮文案有固定上限；栈上 UTF-16 缓冲不产生每次重绘的堆分配。
    let mut label = [0u16; 1024];
    let length = GetWindowTextW(h, label.as_mut_ptr(), label.len() as i32).max(0) as usize;
    let stopping = label[..length]
        .windows(2)
        .any(|pair| pair == [0x505c, 0x6b62]);
    let primary = id == 210;
    let (background, border, foreground) = if !enabled {
        if selected {
            (INK, INK, LINE)
        } else {
            (PAPER, LINE, SECONDARY)
        }
    } else if primary {
        let color = if stopping { RED } else { GREEN };
        (color, color, WHITE)
    } else if selected {
        (INK, INK, WHITE)
    } else {
        (if hot_or_pressed { HOVER } else { PAPER }, LINE, INK)
    };

    let dpi = GetDpiForWindow(h).max(96);
    let px = |logical: i32| ((logical as i64 * dpi as i64 + 48) / 96).max(1) as i32;
    let stroke = px(1);
    let mut client: RECT = zeroed();
    if GetClientRect(h, &mut client) == 0 || client.right <= 0 || client.bottom <= 0 {
        return false;
    }
    // 对象仅在本次绘制存在，先确认分配成功再消费 WM_PAINT 的无效区域。
    let paper_brush = CreateSolidBrush(PAPER);
    let fill_brush = CreateSolidBrush(background);
    let pen = CreatePen(PS_SOLID, stroke, border);
    if paper_brush.is_null() || fill_brush.is_null() || pen.is_null() {
        if !paper_brush.is_null() {
            DeleteObject(paper_brush);
        }
        if !fill_brush.is_null() {
            DeleteObject(fill_brush);
        }
        if !pen.is_null() {
            DeleteObject(pen);
        }
        return false;
    }

    let mut paint: PAINTSTRUCT = zeroed();
    let dc = BeginPaint(h, &mut paint);
    if !dc.is_null() {
        FillRect(dc, &client, paper_brush);
        let old_brush = SelectObject(dc, fill_brush);
        let old_pen = SelectObject(dc, pen);
        let old_font = if font.is_null() {
            std::ptr::null_mut()
        } else {
            SelectObject(dc, font)
        };
        let old_text = SetTextColor(dc, foreground);
        let old_mode = SetBkMode(dc, TRANSPARENT as i32);
        let inset = stroke / 2;
        let far_inset = (stroke + 1) / 2;
        // 直径 4 个逻辑像素即 2px 圆角；线宽随 DPI 保持 1 个逻辑像素。
        RoundRect(
            dc,
            inset,
            inset,
            client.right - far_inset,
            client.bottom - far_inset,
            px(4),
            px(4),
        );
        let padding = px(5);
        let mut text_rect = RECT {
            left: padding,
            top: stroke,
            right: (client.right - padding).max(padding),
            bottom: (client.bottom - stroke).max(stroke),
        };
        let mut format = DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS;
        if ui_state & UISF_HIDEACCEL != 0 {
            format |= DT_HIDEPREFIX;
        }
        DrawTextW(dc, label.as_ptr(), length as i32, &mut text_rect, format);
        if focused {
            let margin = px(4);
            let focus = RECT {
                left: margin,
                top: margin,
                right: client.right - margin,
                bottom: client.bottom - margin,
            };
            if focus.right > focus.left && focus.bottom > focus.top {
                DrawFocusRect(dc, &focus);
            }
        }
        SetBkMode(dc, old_mode);
        SetTextColor(dc, old_text);
        if !old_font.is_null() {
            SelectObject(dc, old_font);
        }
        SelectObject(dc, old_pen);
        SelectObject(dc, old_brush);
    }
    EndPaint(h, &paint);
    DeleteObject(pen);
    DeleteObject(fill_brush);
    DeleteObject(paper_brush);
    true
}

/// 保留原生 TRACKBAR 的值、拖动、键盘和 RangeValue，只绘制其真实轨道与滑块位置。
pub unsafe fn paint_slider(h: HWND) -> bool {
    if h.is_null() || !class_is(h, b"msctls_trackbar32") {
        return false;
    }
    let mut channel: RECT = zeroed();
    let mut thumb: RECT = zeroed();
    let mut client: RECT = zeroed();
    SendMessageW(h, TBM_GETCHANNELRECT, 0, &mut channel as *mut RECT as isize);
    SendMessageW(h, TBM_GETTHUMBRECT, 0, &mut thumb as *mut RECT as isize);
    if GetClientRect(h, &mut client) == 0
        || client.right <= 0
        || client.bottom <= 0
        || channel.right <= channel.left
        || thumb.right <= thumb.left
        || thumb.bottom <= thumb.top
    {
        return false;
    }
    let dpi = GetDpiForWindow(h).max(96);
    let px = |logical: i32| ((logical as i64 * dpi as i64 + 48) / 96).max(1) as i32;
    let enabled = IsWindowEnabled(h) != 0;
    let paper_brush = CreateSolidBrush(PAPER);
    let dot_brush = CreateSolidBrush(if enabled { ACCENT } else { LINE });
    let pen = CreatePen(PS_SOLID, px(1), LINE);
    if paper_brush.is_null() || dot_brush.is_null() || pen.is_null() {
        if !paper_brush.is_null() {
            DeleteObject(paper_brush);
        }
        if !dot_brush.is_null() {
            DeleteObject(dot_brush);
        }
        if !pen.is_null() {
            DeleteObject(pen);
        }
        return false;
    }
    let mut paint: PAINTSTRUCT = zeroed();
    let dc = BeginPaint(h, &mut paint);
    if !dc.is_null() {
        FillRect(dc, &client, paper_brush);
        let old_pen = SelectObject(dc, pen);
        let old_brush = SelectObject(dc, dot_brush);
        let y = (channel.top + channel.bottom) / 2;
        MoveToEx(dc, channel.left, y, std::ptr::null_mut());
        LineTo(dc, channel.right, y);
        // 圆点中心直接跟随原生滑块，命中范围仍由原 TRACKBAR 管理。
        let x = (thumb.left + thumb.right) / 2;
        let center_y = (thumb.top + thumb.bottom) / 2;
        let diameter = px(8)
            .min(thumb.right - thumb.left)
            .min(thumb.bottom - thumb.top)
            .max(1);
        let radius = diameter / 2;
        SelectObject(dc, GetStockObject(NULL_PEN));
        Ellipse(
            dc,
            x - radius,
            center_y - radius,
            x - radius + diameter,
            center_y - radius + diameter,
        );
        if GetFocus() == h {
            let margin = px(3);
            let focus = RECT {
                left: (x - radius - margin).max(0),
                top: (center_y - radius - margin).max(0),
                right: (x - radius + diameter + margin).min(client.right),
                bottom: (center_y - radius + diameter + margin).min(client.bottom),
            };
            if focus.right > focus.left && focus.bottom > focus.top {
                DrawFocusRect(dc, &focus);
            }
        }
        SelectObject(dc, old_brush);
        SelectObject(dc, old_pen);
    }
    EndPaint(h, &paint);
    DeleteObject(pen);
    DeleteObject(dot_brush);
    DeleteObject(paper_brush);
    true
}
