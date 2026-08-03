using System.Runtime.InteropServices;

namespace ClickRun.Core.Helpers;

/// <summary>
/// Win32 API 声明
/// </summary>
public static class NativeMethods
{
    // ==================== 鼠标输入相关 ====================

    public const int INPUT_MOUSE = 0;
    public const int MOUSEEVENTF_LEFTDOWN = 0x0002;
    public const int MOUSEEVENTF_LEFTUP = 0x0004;
    public const int MOUSEEVENTF_RIGHTDOWN = 0x0008;
    public const int MOUSEEVENTF_RIGHTUP = 0x0010;
    public const int MOUSEEVENTF_MIDDLEDOWN = 0x0020;
    public const int MOUSEEVENTF_MIDDLEUP = 0x0040;

    [StructLayout(LayoutKind.Sequential)]
    public struct INPUT
    {
        public uint type;
        public MOUSEINPUT mi;
    }

    [StructLayout(LayoutKind.Sequential)]
    public struct MOUSEINPUT
    {
        public int dx;
        public int dy;
        public uint mouseData;
        public uint dwFlags;
        public uint time;
        public IntPtr dwExtraInfo;
    }

    [DllImport("user32.dll", SetLastError = true)]
    public static extern uint SendInput(uint nInputs, INPUT[] pInputs, int cbSize);

    // ==================== 热键相关 ====================

    public const int WM_HOTKEY = 0x0312;
    public const int MOD_NONE = 0x0000;
    public const int MOD_ALT = 0x0001;
    public const int MOD_CONTROL = 0x0002;
    public const int MOD_SHIFT = 0x0004;
    public const int MOD_WIN = 0x0008;

    // 虚拟键码
    public const int VK_F1 = 0x70;
    public const int VK_F2 = 0x71;
    public const int VK_F3 = 0x72;
    public const int VK_F4 = 0x73;
    public const int VK_F5 = 0x74;
    public const int VK_F6 = 0x75;
    public const int VK_F7 = 0x76;
    public const int VK_F8 = 0x77;
    public const int VK_F9 = 0x78;
    public const int VK_F10 = 0x79;
    public const int VK_F11 = 0x7A;
    public const int VK_F12 = 0x7B;

    [DllImport("user32.dll", SetLastError = true)]
    public static extern bool RegisterHotKey(IntPtr hWnd, int id, uint fsModifiers, uint vk);

    [DllImport("user32.dll", SetLastError = true)]
    public static extern bool UnregisterHotKey(IntPtr hWnd, int id);

    // ==================== 高精度计时器（winmm）====================

    /// <summary>
    /// 申请系统计时器精度（连点期间提升为 1ms）
    /// </summary>
    [DllImport("winmm.dll", EntryPoint = "timeBeginPeriod")]
    public static extern uint TimeBeginPeriod(uint uMilliseconds);

    /// <summary>
    /// 释放系统计时器精度（必须与 TimeBeginPeriod 配对调用）
    /// </summary>
    [DllImport("winmm.dll", EntryPoint = "timeEndPeriod")]
    public static extern uint TimeEndPeriod(uint uMilliseconds);

    // ==================== 音效相关 ====================

    [DllImport("user32.dll")]
    public static extern bool MessageBeep(uint uType);

    public const uint MB_OK = 0x00000000;
    public const uint MB_ICONASTERISK = 0x00000040;

    /// <summary>
    /// 获取按键名称
    /// </summary>
    public static string GetKeyName(uint vk)
    {
        return vk switch
        {
            0x70 => "F1",
            0x71 => "F2",
            0x72 => "F3",
            0x73 => "F4",
            0x74 => "F5",
            0x75 => "F6",
            0x76 => "F7",
            0x77 => "F8",
            0x78 => "F9",
            0x79 => "F10",
            0x7A => "F11",
            0x7B => "F12",
            0x08 => "Backspace",
            0x09 => "Tab",
            0x0D => "Enter",
            0x10 => "Shift",
            0x11 => "Ctrl",
            0x12 => "Alt",
            0x13 => "Pause",
            0x14 => "CapsLock",
            0x1B => "Esc",
            0x20 => "Space",
            0x21 => "PageUp",
            0x22 => "PageDown",
            0x23 => "End",
            0x24 => "Home",
            0x25 => "←",
            0x26 => "↑",
            0x27 => "→",
            0x28 => "↓",
            0x2C => "PrintScreen",
            0x2D => "Insert",
            0x2E => "Delete",
            0x5B => "Win",
            0x5C => "Win",
            0x60 => "Num0",
            0x61 => "Num1",
            0x62 => "Num2",
            0x63 => "Num3",
            0x64 => "Num4",
            0x65 => "Num5",
            0x66 => "Num6",
            0x67 => "Num7",
            0x68 => "Num8",
            0x69 => "Num9",
            0x6A => "Num*",
            0x6B => "Num+",
            0x6C => "NumEnter",
            0x6D => "Num-",
            0x6E => "Num.",
            0x6F => "Num/",
            0x90 => "NumLock",
            0x91 => "ScrollLock",
            0xA0 => "LShift",
            0xA1 => "RShift",
            0xA2 => "LCtrl",
            0xA3 => "RCtrl",
            0xA4 => "LAlt",
            0xA5 => "RAlt",
            _ when vk >= 0x30 && vk <= 0x39 => ((char)vk).ToString(),
            _ when vk >= 0x41 && vk <= 0x5A => ((char)vk).ToString(),
            _ => $"VK:0x{vk:X2}"
        };
    }

    /// <summary>
    /// 格式化组合热键名称，如 "Ctrl+F6"
    /// </summary>
    public static string FormatHotKey(int modifiers, int vk)
    {
        var parts = new System.Collections.Generic.List<string>(4);
        if ((modifiers & MOD_CONTROL) != 0) parts.Add("Ctrl");
        if ((modifiers & MOD_ALT) != 0) parts.Add("Alt");
        if ((modifiers & MOD_SHIFT) != 0) parts.Add("Shift");
        if ((modifiers & MOD_WIN) != 0) parts.Add("Win");
        parts.Add(GetKeyName((uint)vk));
        return string.Join("+", parts);
    }
}
