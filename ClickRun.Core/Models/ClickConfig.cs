using CommunityToolkit.Mvvm.ComponentModel;

namespace ClickRun.Core.Models;

/// <summary>
/// 鼠标按键类型
/// </summary>
public enum MouseButton
{
    Left,
    Right,
    Middle
}

/// <summary>
/// 连点模式
/// </summary>
public enum ClickMode
{
    Single,     // 单击
    Double,     // 双击
    Triple      // 三击
}

/// <summary>
/// 应用配置模型 - ObservableObject 支持变更通知（用于防抖保存）
/// </summary>
public partial class ClickConfig : ObservableObject
{
    /// <summary>
    /// 热键键码（默认F6 = 117）
    /// </summary>
    [ObservableProperty]
    private int _hotKey = 117;

    /// <summary>
    /// 热键修饰键（0=无, 1=Alt, 2=Ctrl, 4=Shift, 8=Win）
    /// </summary>
    [ObservableProperty]
    private int _hotKeyModifiers = 0;

    /// <summary>
    /// 鼠标按键
    /// </summary>
    [ObservableProperty]
    private MouseButton _button = MouseButton.Left;

    /// <summary>
    /// 点击间隔（毫秒）
    /// </summary>
    [ObservableProperty]
    private int _interval = 100;

    /// <summary>
    /// 随机延迟百分比（0-50）
    /// </summary>
    [ObservableProperty]
    private int _randomDelay = 0;

    /// <summary>
    /// 点击模式
    /// </summary>
    [ObservableProperty]
    private ClickMode _mode = ClickMode.Single;

    /// <summary>
    /// 点击次数限制（0=无限）
    /// </summary>
    [ObservableProperty]
    private int _clickLimit = 0;

    /// <summary>
    /// 是否开机自启
    /// </summary>
    [ObservableProperty]
    private bool _startWithWindows = false;

    /// <summary>
    /// 是否最小化到托盘
    /// </summary>
    [ObservableProperty]
    private bool _minimizeToTray = true;

    /// <summary>
    /// 是否播放音效
    /// </summary>
    [ObservableProperty]
    private bool _playSound = false;

    /// <summary>
    /// 是否窗口置顶
    /// </summary>
    [ObservableProperty]
    private bool _alwaysOnTop = false;

    /// <summary>
    /// 保存的配置方案列表
    /// </summary>
    [ObservableProperty]
    private List<ConfigProfile> _profiles = new();

    // ==================== 边界钳制 ====================

    partial void OnIntervalChanged(int value)
    {
        int clamped = Math.Clamp(value, 1, 10000);
        if (clamped != value) Interval = clamped;
    }

    partial void OnRandomDelayChanged(int value)
    {
        int clamped = Math.Clamp(value, 0, 50);
        if (clamped != value) RandomDelay = clamped;
    }

    partial void OnClickLimitChanged(int value)
    {
        if (value < 0) ClickLimit = 0;
    }

    partial void OnHotKeyChanged(int value)
    {
        if (value < 0) HotKey = 0;
    }

    partial void OnButtonChanged(MouseButton value)
    {
        if (!Enum.IsDefined(value)) Button = MouseButton.Left;
    }

    partial void OnModeChanged(ClickMode value)
    {
        if (!Enum.IsDefined(value)) Mode = ClickMode.Single;
    }
}

/// <summary>
/// 配置方案
/// </summary>
public class ConfigProfile
{
    public string Name { get; set; } = string.Empty;
    public int Interval { get; set; } = 100;
    public int RandomDelay { get; set; } = 0;
    public MouseButton Button { get; set; } = MouseButton.Left;
    public ClickMode Mode { get; set; } = ClickMode.Single;
    public int ClickLimit { get; set; } = 0;
}
