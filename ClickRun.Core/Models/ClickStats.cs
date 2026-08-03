namespace ClickRun.Core.Models;

/// <summary>
/// 点击统计模型 - 使用 Interlocked 保证线程安全
/// CPS 采用滑动窗口（近 2 秒）计算实时速度
/// </summary>
public class ClickStats
{
    /// <summary>
    /// CPS 滑动窗口长度（毫秒）
    /// </summary>
    private const long CpsWindowMs = 2000;

    private long _totalClicks;
    private long _sessionClicks;
    private DateTime? _sessionStartTime;

    private readonly object _windowLock = new();
    private readonly Queue<long> _recentClicks = new();

    /// <summary>
    /// 总点击次数（线程安全）
    /// </summary>
    public long TotalClicks => Interlocked.Read(ref _totalClicks);

    /// <summary>
    /// 当前会话点击次数（线程安全）
    /// </summary>
    public long SessionClicks => Interlocked.Read(ref _sessionClicks);

    /// <summary>
    /// 会话开始时间
    /// </summary>
    public DateTime? SessionStartTime => _sessionStartTime;

    /// <summary>
    /// 实时点击速度（次/秒）- 近 2 秒滑动窗口
    /// </summary>
    public double ClicksPerSecond
    {
        get
        {
            lock (_windowLock)
            {
                long now = Environment.TickCount64;
                TrimWindow(now);

                if (_recentClicks.Count < 2)
                    return 0;

                long span = now - _recentClicks.Peek();
                return span > 0 ? (_recentClicks.Count - 1) * 1000.0 / span : 0;
            }
        }
    }

    /// <summary>
    /// 是否正在运行（volatile 保证跨线程可见性）
    /// </summary>
    private volatile bool _isRunning;
    public bool IsRunning
    {
        get => _isRunning;
        internal set => _isRunning = value;
    }

    /// <summary>
    /// 获取运行时长
    /// </summary>
    public TimeSpan GetSessionDuration()
    {
        var startTime = _sessionStartTime;
        if (startTime == null)
            return TimeSpan.Zero;
        return DateTime.Now - startTime.Value;
    }

    /// <summary>
    /// 获取格式化的运行时长
    /// </summary>
    public string GetFormattedDuration()
    {
        var duration = GetSessionDuration();
        return $"{(int)duration.TotalHours:D2}:{duration.Minutes:D2}:{duration.Seconds:D2}";
    }

    /// <summary>
    /// 重置会话统计
    /// </summary>
    public void ResetSession()
    {
        Interlocked.Exchange(ref _sessionClicks, 0);
        _sessionStartTime = null;
        lock (_windowLock)
        {
            _recentClicks.Clear();
        }
    }

    /// <summary>
    /// 开始新会话
    /// </summary>
    public void StartSession()
    {
        Interlocked.Exchange(ref _sessionClicks, 0);
        _sessionStartTime = DateTime.Now;
        lock (_windowLock)
        {
            _recentClicks.Clear();
        }
        _isRunning = true;
    }

    /// <summary>
    /// 结束会话
    /// </summary>
    public void StopSession()
    {
        _isRunning = false;
    }

    /// <summary>
    /// 记录一次点击（线程安全）
    /// </summary>
    public void RecordClick()
    {
        Interlocked.Increment(ref _sessionClicks);
        Interlocked.Increment(ref _totalClicks);

        lock (_windowLock)
        {
            long now = Environment.TickCount64;
            _recentClicks.Enqueue(now);
            TrimWindow(now);
        }
    }

    /// <summary>
    /// 剔除窗口之外的点击时间戳（调用方需持有 _windowLock）
    /// </summary>
    private void TrimWindow(long now)
    {
        while (_recentClicks.Count > 0 && now - _recentClicks.Peek() > CpsWindowMs)
        {
            _recentClicks.Dequeue();
        }
    }
}
