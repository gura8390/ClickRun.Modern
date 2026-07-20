using ClickRun.Core.Helpers;
using ClickRun.Core.Models;

namespace ClickRun.Core.Services;

/// <summary>
/// 鼠标点击服务
/// </summary>
public class ClickService : IDisposable
{
    private Thread? _clickThread;
    private CancellationTokenSource? _cts;
    private readonly object _lock = new();
    private bool _disposed;

    public event Action? OnClick;
    public event Action? OnStarted;
    public event Action? OnStopped;

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
            _cts = new CancellationTokenSource();
            var token = _cts.Token;

            stats.StartSession();

            _clickThread = new Thread(() => ClickLoop(config, stats, token))
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
    /// 停止连点
    /// </summary>
    public void Stop(ClickStats stats)
    {
        CancellationTokenSource? ctsToDispose;

        lock (_lock)
        {
            if (_disposed || !IsRunning) return;

            IsRunning = false;
            ctsToDispose = _cts;
            _cts = null;
        }

        // 在锁外取消；ClickLoop 的 finally 块会触发 OnStopped
        ctsToDispose?.Cancel();
        ctsToDispose?.Dispose();
    }

    /// <summary>
    /// 点击循环
    /// </summary>
    private void ClickLoop(ClickConfig config, ClickStats stats, CancellationToken token)
    {
        try
        {
            while (!token.IsCancellationRequested)
            {
                // 检查点击次数限制
                if (config.ClickLimit > 0 && stats.SessionClicks >= config.ClickLimit)
                {
                    break;
                }

                // 执行点击
                MouseSimulator.Click(config.Button, config.Mode);
                stats.RecordClick();
                OnClick?.Invoke();

                // 计算延迟（确保最小1ms）
                int delay = Math.Max(1, config.Interval);

                if (config.RandomDelay > 0)
                {
                    int randomRange = delay * config.RandomDelay / 100;
                    delay += Random.Shared.Next(-randomRange, randomRange + 1);
                    delay = Math.Max(1, delay);
                }

                // 等待
                Thread.Sleep(delay);
            }
        }
        catch (OperationCanceledException)
        {
            // 正常取消
        }
        catch (Exception ex)
        {
            System.Diagnostics.Debug.WriteLine($"ClickLoop error: {ex.Message}");
        }
        finally
        {
            // 在 finally 中更新状态并触发事件
            lock (_lock)
            {
                IsRunning = false;
            }

            stats.StopSession();
            OnStopped?.Invoke();
        }
    }

    public void Dispose()
    {
        CancellationTokenSource? ctsToDispose;
        Thread? threadToJoin;

        lock (_lock)
        {
            if (_disposed) return;
            _disposed = true;

            IsRunning = false;
            ctsToDispose = _cts;
            _cts = null;
            threadToJoin = _clickThread;
        }

        // 在锁外取消和等待线程
        ctsToDispose?.Cancel();
        ctsToDispose?.Dispose();

        if (threadToJoin != null && threadToJoin.IsAlive)
        {
            threadToJoin.Join(TimeSpan.FromSeconds(5));
        }
    }
}
