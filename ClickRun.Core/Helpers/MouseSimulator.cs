using System.Runtime.InteropServices;
using ClickRun.Core.Models;
using static ClickRun.Core.Helpers.NativeMethods;

namespace ClickRun.Core.Helpers;

/// <summary>
/// 鼠标模拟器
/// </summary>
public static class MouseSimulator
{
    /// <summary>
    /// 模拟鼠标点击
    /// </summary>
    /// <param name="button">鼠标按键</param>
    /// <param name="mode">点击模式</param>
    public static void Click(MouseButton button, ClickMode mode = ClickMode.Single)
    {
        int clickCount = mode switch
        {
            ClickMode.Single => 1,
            ClickMode.Double => 2,
            ClickMode.Triple => 3,
            _ => 1
        };

        for (int i = 0; i < clickCount; i++)
        {
            SimulateClick(button);
            // 双击/三击之间需要短暂延迟，否则OS无法识别为双击
            if (i < clickCount - 1)
            {
                Thread.Sleep(20);
            }
        }
    }

    /// <summary>
    /// 模拟单次鼠标点击
    /// </summary>
    private static void SimulateClick(MouseButton button)
    {
        uint downFlag, upFlag;

        switch (button)
        {
            case MouseButton.Left:
                downFlag = MOUSEEVENTF_LEFTDOWN;
                upFlag = MOUSEEVENTF_LEFTUP;
                break;
            case MouseButton.Right:
                downFlag = MOUSEEVENTF_RIGHTDOWN;
                upFlag = MOUSEEVENTF_RIGHTUP;
                break;
            case MouseButton.Middle:
                downFlag = MOUSEEVENTF_MIDDLEDOWN;
                upFlag = MOUSEEVENTF_MIDDLEUP;
                break;
            default:
                downFlag = MOUSEEVENTF_LEFTDOWN;
                upFlag = MOUSEEVENTF_LEFTUP;
                break;
        }

        var inputs = new INPUT[2];

        // 按下
        inputs[0].type = INPUT_MOUSE;
        inputs[0].mi.dx = 0;
        inputs[0].mi.dy = 0;
        inputs[0].mi.mouseData = 0;
        inputs[0].mi.dwFlags = downFlag;
        inputs[0].mi.time = 0;
        inputs[0].mi.dwExtraInfo = IntPtr.Zero;

        // 释放
        inputs[1].type = INPUT_MOUSE;
        inputs[1].mi.dx = 0;
        inputs[1].mi.dy = 0;
        inputs[1].mi.mouseData = 0;
        inputs[1].mi.dwFlags = upFlag;
        inputs[1].mi.time = 0;
        inputs[1].mi.dwExtraInfo = IntPtr.Zero;

        uint sent = SendInput(2, inputs, Marshal.SizeOf(typeof(INPUT)));
        if (sent < 2)
        {
            System.Diagnostics.Debug.WriteLine($"SendInput: sent {sent}/2, error: {Marshal.GetLastWin32Error()}");
        }
    }
}
