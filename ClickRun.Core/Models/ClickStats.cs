namespace ClickRun.Core.Models;

/// <summary>
/// 点击统计模型 - 使用 Interlocked 保证线程安全
/// </summary>
public class ClickStats
{
    private long _totalClicks;
    private long _sessionClicks;
    private double _clicksPerSecond;
    private DateTime? _sessionStartTime;

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
    /// 当前点击速度（次/秒）- volatile 保证可见性
    /// </summary>
    public double ClicksPerSecond
    {
        get => Volatile.Read(ref _clicksPerSecond);
        private set => Volatile.Write(ref _clicksPerSecond, value);
    }

    /// <summary>
    /// 是否正在运行（volatile 保证跨线程可见性）
    /// </summary>
    private volatile bool _isRunning;
    public bool IsRunning
    {
        get => _isRunning;
        set => _isRunning = value;
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
        ClicksPerSecond = 0;
    }

    /// <summary>
    /// 开始新会话
    /// </summary>
    public void StartSession()
    {
        Interlocked.Exchange(ref _sessionClicks, 0);
        _sessionStartTime = DateTime.Now;
        IsRunning = true;
    }

    /// <summary>
    /// 结束会话
    /// </summary>
    public void StopSession()
    {
        IsRunning = false;
    }

    /// <summary>
    /// 记录一次点击（线程安全）
    /// </summary>
    public void RecordClick()
    {
        Interlocked.Increment(ref _sessionClicks);
        Interlocked.Increment(ref _totalClicks);

        // 计算CPS
        var startTime = _sessionStartTime;
        if (startTime.HasValue)
        {
            var elapsed = (DateTime.Now - startTime.Value).TotalSeconds;
            if (elapsed > 0)
            {
                var clicks = Interlocked.Read(ref _sessionClicks);
                ClicksPerSecond = clicks / elapsed;
            }
        }
    }
}
