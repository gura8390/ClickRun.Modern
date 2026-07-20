using System.Runtime.InteropServices;
using ClickRun.Core.Helpers;

namespace ClickRun.Core.Services;

/// <summary>
/// 全局热键服务
/// </summary>
public class HotKeyService : IDisposable
{
    private IntPtr _windowHandle;
    private int _hotKeyId = 1;
    private volatile bool _isRegistered;
    private readonly object _lock = new();

    public event Action? OnHotKeyPressed;

    /// <summary>
    /// 注册全局热键
    /// </summary>
    public bool Register(IntPtr windowHandle, int modifiers, int vk)
    {
        lock (_lock)
        {
            UnregisterInternal();

            _windowHandle = windowHandle;
            _isRegistered = NativeMethods.RegisterHotKey(windowHandle, _hotKeyId, (uint)modifiers, (uint)vk);
            return _isRegistered;
        }
    }

    /// <summary>
    /// 注销全局热键
    /// </summary>
    public void Unregister()
    {
        lock (_lock)
        {
            UnregisterInternal();
        }
    }

    private void UnregisterInternal()
    {
        if (_isRegistered && _windowHandle != IntPtr.Zero)
        {
            NativeMethods.UnregisterHotKey(_windowHandle, _hotKeyId);
            _isRegistered = false;
        }
    }

    /// <summary>
    /// 处理窗口消息 - 验证热键ID
    /// </summary>
    public bool ProcessMessage(int msg, IntPtr wParam, IntPtr lParam)
    {
        if (msg == NativeMethods.WM_HOTKEY && wParam.ToInt32() == _hotKeyId)
        {
            OnHotKeyPressed?.Invoke();
            return true;
        }
        return false;
    }

    public void Dispose()
    {
        Unregister();
    }
}
