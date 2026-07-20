using System.Text.Json.Serialization;

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
/// 应用配置模型
/// </summary>
public class ClickConfig
{
    /// <summary>
    /// 热键键码（默认F6 = 117）
    /// </summary>
    public int HotKey { get; set; } = 117;

    /// <summary>
    /// 热键修饰键（0=无, 1=Alt, 2=Ctrl, 4=Shift, 8=Win）
    /// </summary>
    public int HotKeyModifiers { get; set; } = 0;

    /// <summary>
    /// 鼠标按键
    /// </summary>
    public MouseButton Button { get; set; } = MouseButton.Left;

    /// <summary>
    /// 点击间隔（毫秒）
    /// </summary>
    public int Interval { get; set; } = 100;

    /// <summary>
    /// 随机延迟百分比（0-50）
    /// </summary>
    public int RandomDelay { get; set; } = 0;

    /// <summary>
    /// 点击模式
    /// </summary>
    public ClickMode Mode { get; set; } = ClickMode.Single;

    /// <summary>
    /// 点击次数限制（0=无限）
    /// </summary>
    public int ClickLimit { get; set; } = 0;

    /// <summary>
    /// 是否开机自启
    /// </summary>
    public bool StartWithWindows { get; set; } = false;

    /// <summary>
    /// 是否最小化到托盘
    /// </summary>
    public bool MinimizeToTray { get; set; } = true;

    /// <summary>
    /// 是否播放音效
    /// </summary>
    public bool PlaySound { get; set; } = false;

    /// <summary>
    /// 是否窗口置顶
    /// </summary>
    public bool AlwaysOnTop { get; set; } = false;

    /// <summary>
    /// 保存的配置方案列表
    /// </summary>
    public List<ConfigProfile> Profiles { get; set; } = new();
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
