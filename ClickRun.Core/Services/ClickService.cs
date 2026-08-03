using System.Diagnostics;
using ClickRun.Core.Helpers;
using ClickRun.Core.Models;

namespace ClickRun.Core.Services;

/// <summary>
/// 鼠标点击服务
/// </summary>
public class ClickService : IDisposable
{
    /// <summary>
    /// 连续送达失败多少次后判定为被系统拦截（UIPI）
    /// </summary>
    private const int BlockedThreshold = 5;

    private Thread? _clickThread;
    private CancellationTokenSource? _cts;
    private readonly object _lock = new();
    private bool _disposed;
    private long _generation;
    private int _consecutiveFailures;

    public event Action? OnClick;
    public event Action? OnStarted;
    public event Action? OnStopped;

    /// <summary>
    /// 点击连续被系统拦截（如目标窗口以管理员权限运行）时触发
    /// </summary>
    public event Action? OnClickBlocked;

    public bool IsRunning { get; private set; }

    /// <summary>
    /// 开始连点
    /// </summary>
    public void Start(ClickConfig config, ClickStats stats)
    {
        lock (_lock)
        {
            if (_disposed) throw new ObjectDisposedException(nameof(ClickService));
            if (IsRunning) return;

            IsRunning = true;
            long generation = ++_generation;
            _consecutiveFailures = 0;
            _cts = new CancellationTokenSource();
            var token = _cts.Token;

            stats.StartSession();
            TimerResolution.Acquire();

            _clickThread = new Thread(() => ClickLoop(config, stats, token, generation))
            {
                IsBackground = true,
                Priority = ThreadPriority.AboveNormal
            };
            _clickThread.Start();
        }

        // 事件在锁外触发，避免死锁
        OnStarted?.Invoke();
    }

    /// <summary>
    /// 停止连点。取消即时生效，并等待点击线程退出后才返回，
    /// 保证返回后状态已收敛（可安全地再次 Start）。
    /// </summary>
    public void Stop()
    {
        Thread? threadToJoin;
        CancellationTokenSource? ctsToDispose;

        lock (_lock)
        {
            if (_disposed || !IsRunning) return;

            IsRunning = false;
            ctsToDispose = _cts;
            _cts = null;
            threadToJoin = _clickThread;
            _clickThread = null;
        }

        // 锁外取消：WaitOne 立即唤醒点击线程
        ctsToDispose?.Cancel();

        // 等待线程退出，其 finally 块会先收敛状态并触发 OnStopped
        if (threadToJoin != null && threadToJoin.IsAlive)
        {
            threadToJoin.Join(TimeSpan.FromSeconds(3));
        }

        ctsToDispose?.Dispose();
    }

    /// <summary>
    /// 点击循环
    /// </summary>
    private void ClickLoop(ClickConfig config, ClickStats stats, CancellationToken token, long generation)
    {
        try
        {
            var clickWatch = new Stopwatch();

            while (!token.IsCancellationRequested)
            {
                // 检查点击次数限制
                if (config.ClickLimit > 0 && stats.SessionClicks >= config.ClickLimit)
                {
                    break;
                }

                // 执行点击并计时，执行耗时将从间隔中扣除（P0-3）
                clickWatch.Restart();
                bool delivered = MouseSimulator.Click(config.Button, config.Mode);
                clickWatch.Stop();

                stats.RecordClick();
                TrackDelivery(delivered);
                OnClick?.Invoke();

                // 计算延迟（确保最小1ms）
                int delay = Math.Max(1, config.Interval);

                if (config.RandomDelay > 0)
                {
                    int randomRange = delay * config.RandomDelay / 100;
                    delay += Random.Shared.Next(-randomRange, randomRange + 1);
                    delay = Math.Max(1, delay);
                }

                // 扣除点击执行耗时后高精度等待（P0-2）
                delay -= (int)clickWatch.ElapsedMilliseconds;
                PrecisionDelay.Wait(delay, token);
            }
        }
        catch (Exception ex)
        {
            Debug.WriteLine($"ClickLoop error: {ex.Message}");
        }
        finally
        {
            // 只有当前代际的线程才能收敛状态，避免旧线程覆盖新会话（P0-1）
            bool isCurrentGeneration;
            lock (_lock)
            {
                isCurrentGeneration = generation == _generation;
                if (isCurrentGeneration)
                {
                    IsRunning = false;
                    _clickThread = null;
                    // Stop 路径已取走 _cts；自然退出（达到次数上限）时在此释放
                    _cts?.Dispose();
                    _cts = null;
                }
            }

            if (isCurrentGeneration)
            {
                TimerResolution.Release();
                stats.StopSession();
                OnStopped?.Invoke();
            }
        }
    }

    /// <summary>
    /// 追踪点击送达情况，连续失败达到阈值时上报拦截事件
    /// </summary>
    private void TrackDelivery(bool delivered)
    {
        if (delivered)
        {
            Interlocked.Exchange(ref _consecutiveFailures, 0);
            return;
        }

        if (Interlocked.Increment(ref _consecutiveFailures) == BlockedThreshold)
        {
            OnClickBlocked?.Invoke();
        }
    }

    public void Dispose()
    {
        Thread? threadToJoin;
        CancellationTokenSource? ctsToDispose;

        lock (_lock)
        {
            if (_disposed) return;
            _disposed = true;

            IsRunning = false;
            ctsToDispose = _cts;
            _cts = null;
            threadToJoin = _clickThread;
            _clickThread = null;
        }

        // 在锁外取消和等待线程
        ctsToDispose?.Cancel();

        if (threadToJoin != null && threadToJoin.IsAlive)
        {
            threadToJoin.Join(TimeSpan.FromSeconds(5));
        }

        ctsToDispose?.Dispose();
    }
}
