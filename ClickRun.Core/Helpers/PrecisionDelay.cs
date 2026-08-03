using System.Diagnostics;

namespace ClickRun.Core.Helpers;

/// <summary>
/// 系统计时器粒度管理 - 引用计数式配对调用 timeBeginPeriod/timeEndPeriod
/// </summary>
internal static class TimerResolution
{
    private static readonly object _lock = new();
    private static int _refCount;

    /// <summary>
    /// 申请 1ms 计时器粒度
    /// </summary>
    public static void Acquire()
    {
        lock (_lock)
        {
            if (_refCount++ == 0)
            {
                NativeMethods.TimeBeginPeriod(1);
            }
        }
    }

    /// <summary>
    /// 释放计时器粒度（必须与 Acquire 配对）
    /// </summary>
    public static void Release()
    {
        lock (_lock)
        {
            if (_refCount > 0 && --_refCount == 0)
            {
                NativeMethods.TimeEndPeriod(1);
            }
        }
    }
}

/// <summary>
/// 高精度延迟 - 粗睡 + 自旋混合，支持取消即时唤醒
/// </summary>
public static class PrecisionDelay
{
    /// <summary>
    /// 自旋补齐的剩余时间阈值（毫秒）
    /// </summary>
    private const int SpinThresholdMs = 3;

    /// <summary>
    /// 等待指定毫秒数。取消时立即返回。
    /// 大间隔走可取消的 WaitOne，尾部 &lt;3ms 用自旋补齐精度。
    /// </summary>
    public static void Wait(int milliseconds, CancellationToken token)
    {
        if (milliseconds <= 0 || token.IsCancellationRequested)
            return;

        var sw = Stopwatch.StartNew();

        // 粗睡阶段：留出余量给自旋
        int coarse = milliseconds > SpinThresholdMs * 2
            ? milliseconds - SpinThresholdMs
            : milliseconds / 2;

        if (coarse > 0)
        {
            token.WaitHandle.WaitOne(coarse);
        }

        // 自旋阶段：补齐剩余时间，期间持续响应取消
        var spin = new SpinWait();
        while (sw.ElapsedMilliseconds < milliseconds)
        {
            if (token.IsCancellationRequested)
                return;
            spin.SpinOnce();
        }
    }
}
