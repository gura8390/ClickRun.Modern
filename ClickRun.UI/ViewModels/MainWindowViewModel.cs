using System.Collections.ObjectModel;
using System.Reflection;
using System.Windows;
using System.Windows.Interop;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using ClickRun.Core.Helpers;
using ClickRun.Core.Models;
using ClickRun.Core.Services;

namespace ClickRun.UI.ViewModels;

public partial class MainWindowViewModel : ObservableObject, IDisposable
{
    private readonly ClickService _clickService;
    private readonly HotKeyService _hotKeyService;
    private readonly ConfigService _configService;
    private readonly ClickStats _stats;
    private Window? _window;
    private System.Windows.Threading.DispatcherTimer? _uiTimer;
    private System.Windows.Threading.DispatcherTimer? _saveTimer;
    private bool _disposed;
    private Action? _onStartedHandler;
    private Action? _onStoppedHandler;
    private Action? _onBlockedHandler;
    private WarningKind _warningKind = WarningKind.None;

    private enum WarningKind { None, HotKey, Blocked }

    // ==================== 属性 ====================

    [ObservableProperty]
    private ClickConfig _config;

    public ClickStats Stats => _stats;

    /// <summary>
    /// 运行状态 - 以 ClickService 为唯一事实源
    /// </summary>
    public bool IsRunning => _clickService.IsRunning;

    public bool IsNotRunning => !IsRunning;

    public string ButtonText => IsRunning ? "⏹  停止" : "▶  开始连点";

    public string ToggleMenuText => IsRunning ? "⏹ 停止连点" : "▶ 开始连点";

    public string TrayToolTip => IsRunning
        ? $"ClickRun - 运行中 · {_stats.SessionClicks:N0} 次"
        : "ClickRun - 已停止";

    public string HotKeyName => NativeMethods.FormatHotKey(Config.HotKeyModifiers, Config.HotKey);

    public string SessionClicksText => _stats.SessionClicks.ToString("N0");

    public string TotalClicksText => _stats.TotalClicks.ToString("N0");

    public string DurationText => _stats.GetFormattedDuration();

    public string CpsText => _stats.ClicksPerSecond.ToString("F1");

    public string VersionText { get; } = "v" + (Assembly.GetExecutingAssembly()
        .GetCustomAttribute<AssemblyInformationalVersionAttribute>()?.InformationalVersion ?? "2.1");

    /// <summary>
    /// 警示条消息（null 时隐藏）
    /// </summary>
    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(WarningVisibility))]
    private string? _warningMessage;

    public Visibility WarningVisibility =>
        string.IsNullOrEmpty(WarningMessage) ? Visibility.Collapsed : Visibility.Visible;

    /// <summary>
    /// 配置方案列表（ComboBox 数据源）
    /// </summary>
    public ObservableCollection<ConfigProfile> Profiles { get; }

    /// <summary>
    /// 当前选中的配置方案，选中即应用
    /// </summary>
    [ObservableProperty]
    private ConfigProfile? _selectedProfile;

    /// <summary>
    /// 是否选择无限次数
    /// </summary>
    private bool _isUnlimited = true;
    public bool IsUnlimited
    {
        get => _isUnlimited;
        set
        {
            if (SetProperty(ref _isUnlimited, value))
            {
                OnPropertyChanged(nameof(IsLimited));
                if (value) Config.ClickLimit = 0;
            }
        }
    }

    /// <summary>
    /// 是否选择指定次数
    /// </summary>
    public bool IsLimited
    {
        get => !_isUnlimited;
        set => IsUnlimited = !value;
    }

    /// <summary>
    /// 开机自启动
    /// </summary>
    public bool StartWithWindows
    {
        get => Config.StartWithWindows;
        set
        {
            if (Config.StartWithWindows != value)
            {
                Config.StartWithWindows = value;
                AutoStartService.SetAutoStart(value);
                OnPropertyChanged();
            }
        }
    }

    /// <summary>
    /// 窗口置顶
    /// </summary>
    public bool AlwaysOnTop
    {
        get => Config.AlwaysOnTop;
        set
        {
            if (Config.AlwaysOnTop != value)
            {
                Config.AlwaysOnTop = value;
                OnPropertyChanged();
            }
        }
    }

    /// <summary>
    /// 是否显示设置页面
    /// </summary>
    private bool _isSettingsViewVisible;
    public bool IsSettingsViewVisible
    {
        get => _isSettingsViewVisible;
        set
        {
            if (SetProperty(ref _isSettingsViewVisible, value))
            {
                OnPropertyChanged(nameof(IsMainViewVisible));
                OnPropertyChanged(nameof(IsSettingsViewVisibleAsVisibility));
            }
        }
    }

    /// <summary>
    /// 是否显示主界面
    /// </summary>
    public Visibility IsMainViewVisible => _isSettingsViewVisible ? Visibility.Collapsed : Visibility.Visible;

    /// <summary>
    /// 设置页面Visibility
    /// </summary>
    public Visibility IsSettingsViewVisibleAsVisibility => _isSettingsViewVisible ? Visibility.Visible : Visibility.Collapsed;

    // ==================== 构造函数 ====================

    public MainWindowViewModel()
    {
        _configService = new ConfigService();
        _config = _configService.Load();
        _stats = new ClickStats();

        // 初始化无限/指定状态
        _isUnlimited = Config.ClickLimit == 0;

        // 初始化方案列表
        Profiles = new ObservableCollection<ConfigProfile>(Config.Profiles);

        _clickService = new ClickService();
        _hotKeyService = new HotKeyService();

        // 订阅事件
        _clickService.OnClick += OnClickPerformed;
        _onStartedHandler = () =>
        {
            Application.Current?.Dispatcher.BeginInvoke(() =>
            {
                NotifyRunningStateChanged();
                if (Config.PlaySound) NativeMethods.MessageBeep(NativeMethods.MB_ICONASTERISK);
            });
        };
        _onStoppedHandler = () =>
        {
            Application.Current?.Dispatcher.BeginInvoke(() =>
            {
                NotifyRunningStateChanged();
                if (Config.PlaySound) NativeMethods.MessageBeep(NativeMethods.MB_OK);
            });
        };
        _onBlockedHandler = () =>
        {
            Application.Current?.Dispatcher.BeginInvoke(() =>
                ShowWarning(WarningKind.Blocked, "点击可能被系统拦截：目标窗口权限过高，可尝试以管理员身份运行 ClickRun。"));
        };
        _clickService.OnStarted += _onStartedHandler;
        _clickService.OnStopped += _onStoppedHandler;
        _clickService.OnClickBlocked += _onBlockedHandler;
        _hotKeyService.OnHotKeyPressed += Toggle;

        // 配置变更 → 防抖保存（P1-4）
        Config.PropertyChanged += OnConfigPropertyChanged;
        _saveTimer = new System.Windows.Threading.DispatcherTimer
        {
            Interval = TimeSpan.FromMilliseconds(500)
        };
        _saveTimer.Tick += OnSaveTimerTick;

        // UI更新定时器
        _uiTimer = new System.Windows.Threading.DispatcherTimer
        {
            Interval = TimeSpan.FromMilliseconds(200)
        };
        _uiTimer.Tick += OnTimerTick;
        _uiTimer.Start();
    }

    // ==================== 命令 ====================

    [RelayCommand]
    private void Toggle()
    {
        if (_disposed) return;

        if (_clickService.IsRunning)
        {
            _clickService.Stop();
        }
        else
        {
            if (_warningKind == WarningKind.Blocked) ClearWarning();
            _clickService.Start(Config, _stats);
        }
    }

    [RelayCommand]
    private void ChangeHotKey()
    {
        if (_window == null) return;

        var dialog = new Views.HotKeySelectionWindow(Config.HotKey, Config.HotKeyModifiers)
        {
            Owner = _window
        };

        if (dialog.ShowDialog() == true && dialog.SelectedHotKey.HasValue)
        {
            Config.HotKey = dialog.SelectedHotKey.Value;
            Config.HotKeyModifiers = dialog.SelectedModifiers;
            RegisterHotKey();
            OnPropertyChanged(nameof(HotKeyName));
        }
    }

    [RelayCommand]
    private void ShowSettings() => IsSettingsViewVisible = true;

    [RelayCommand]
    private void HideSettings() => IsSettingsViewVisible = false;

    [RelayCommand]
    private void OpenProjectUrl() => OpenUrl("https://github.com/gura8390/ClickRun.Modern");

    [RelayCommand]
    private void OpenGitHubUrl() => OpenUrl("https://github.com/gura8390/ClickRun.Modern");

    [RelayCommand]
    private void SaveProfile()
    {
        if (_window == null) return;

        var dialog = new Views.SaveProfileDialog(Config.Profiles.Select(p => p.Name))
        {
            Owner = _window
        };

        if (dialog.ShowDialog() != true) return;

        var profile = new ConfigProfile
        {
            Name = dialog.ProfileName.Trim(),
            Interval = Config.Interval,
            RandomDelay = Config.RandomDelay,
            Button = Config.Button,
            Mode = Config.Mode,
            ClickLimit = Config.ClickLimit
        };

        Config.Profiles.Add(profile);
        Profiles.Add(profile);
        SelectedProfile = profile;
        ScheduleSave();
    }

    [RelayCommand(CanExecute = nameof(HasSelectedProfile))]
    private void DeleteProfile()
    {
        if (SelectedProfile == null) return;

        Config.Profiles.Remove(SelectedProfile);
        Profiles.Remove(SelectedProfile);
        SelectedProfile = null;
        ScheduleSave();
    }

    private bool HasSelectedProfile => SelectedProfile != null;

    [RelayCommand]
    private void DismissWarning() => ClearWarning();

    // ==================== 公共方法 ====================

    public void Initialize(Window window)
    {
        _window = window;

        // 同步开机自启状态
        Config.StartWithWindows = AutoStartService.IsAutoStartEnabled();
        OnPropertyChanged(nameof(StartWithWindows));

        // 注册全局热键（失败走非模态警示条，不打断启动）
        RegisterHotKey();
    }

    public bool ProcessHotKeyMessage(int msg, IntPtr wParam, IntPtr lParam)
    {
        return _hotKeyService.ProcessMessage(msg, wParam, lParam);
    }

    /// <summary>
    /// 立即保存配置（退出前刷新防抖缓冲）
    /// </summary>
    public void SaveConfig()
    {
        _saveTimer?.Stop();
        _configService.Save(Config);
    }

    // ==================== 私有方法 ====================

    partial void OnSelectedProfileChanged(ConfigProfile? value)
    {
        DeleteProfileCommand.NotifyCanExecuteChanged();
        if (value != null)
        {
            ApplyProfile(value);
        }
    }

    private void ApplyProfile(ConfigProfile profile)
    {
        Config.Interval = profile.Interval;
        Config.RandomDelay = profile.RandomDelay;
        Config.Button = profile.Button;
        Config.Mode = profile.Mode;
        IsUnlimited = profile.ClickLimit == 0;
        Config.ClickLimit = profile.ClickLimit;
    }

    private bool RegisterHotKey()
    {
        if (_window == null) return false;

        var hwnd = new WindowInteropHelper(_window).Handle;
        bool success = _hotKeyService.Register(hwnd, Config.HotKeyModifiers, Config.HotKey);

        if (!success)
        {
            ShowWarning(WarningKind.HotKey, $"热键 {HotKeyName} 注册失败，可能已被其他程序占用，请更换。");
        }
        else if (_warningKind == WarningKind.HotKey)
        {
            ClearWarning();
        }
        return success;
    }

    private void ShowWarning(WarningKind kind, string message)
    {
        _warningKind = kind;
        WarningMessage = message;
    }

    private void ClearWarning()
    {
        _warningKind = WarningKind.None;
        WarningMessage = null;
    }

    private void OnConfigPropertyChanged(object? sender, System.ComponentModel.PropertyChangedEventArgs e)
    {
        ScheduleSave();
    }

    private void ScheduleSave()
    {
        if (_disposed || _saveTimer == null) return;
        _saveTimer.Stop();
        _saveTimer.Start();
    }

    private void OnSaveTimerTick(object? sender, EventArgs e)
    {
        _saveTimer?.Stop();
        _configService.Save(Config);
    }

    private void OnTimerTick(object? sender, EventArgs e)
    {
        UpdateStats();
    }

    private void OnClickPerformed()
    {
        Application.Current?.Dispatcher.BeginInvoke(() => UpdateStats());
    }

    private void UpdateStats()
    {
        OnPropertyChanged(nameof(SessionClicksText));
        OnPropertyChanged(nameof(TotalClicksText));
        OnPropertyChanged(nameof(DurationText));
        OnPropertyChanged(nameof(CpsText));
        OnPropertyChanged(nameof(TrayToolTip));
    }

    private void NotifyRunningStateChanged()
    {
        OnPropertyChanged(nameof(IsRunning));
        OnPropertyChanged(nameof(IsNotRunning));
        OnPropertyChanged(nameof(ButtonText));
        OnPropertyChanged(nameof(ToggleMenuText));
        OnPropertyChanged(nameof(TrayToolTip));
    }

    private void OpenUrl(string url)
    {
        try
        {
            System.Diagnostics.Process.Start(new System.Diagnostics.ProcessStartInfo
            {
                FileName = url,
                UseShellExecute = true
            });
        }
        catch { }
    }

    public void Dispose()
    {
        if (_disposed) return;
        _disposed = true;

        if (_saveTimer != null)
        {
            _saveTimer.Tick -= OnSaveTimerTick;
            _saveTimer.Stop();
            _saveTimer = null;
        }

        if (_uiTimer != null)
        {
            _uiTimer.Tick -= OnTimerTick;
            _uiTimer.Stop();
            _uiTimer = null;
        }

        Config.PropertyChanged -= OnConfigPropertyChanged;
        _clickService.OnClick -= OnClickPerformed;
        if (_onStartedHandler != null) _clickService.OnStarted -= _onStartedHandler;
        if (_onStoppedHandler != null) _clickService.OnStopped -= _onStoppedHandler;
        if (_onBlockedHandler != null) _clickService.OnClickBlocked -= _onBlockedHandler;
        _hotKeyService.OnHotKeyPressed -= Toggle;

        _hotKeyService.Dispose();
        _clickService.Dispose();
    }
}
