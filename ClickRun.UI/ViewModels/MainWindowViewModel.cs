using System.ComponentModel;
using System.Runtime.CompilerServices;
using System.Windows;
using System.Windows.Input;
using System.Windows.Interop;
using ClickRun.Core.Helpers;
using ClickRun.Core.Models;
using ClickRun.Core.Services;

namespace ClickRun.UI.ViewModels;

public class MainWindowViewModel : INotifyPropertyChanged, IDisposable
{
    private readonly ClickService _clickService;
    private readonly HotKeyService _hotKeyService;
    private readonly ConfigService _configService;
    private ClickConfig _config;
    private ClickStats _stats;
    private Window? _window;
    private System.Windows.Threading.DispatcherTimer? _uiTimer;
    private bool _isUnlimited = true;
    private bool _disposed;
    private Action? _onStartedHandler;
    private Action? _onStoppedHandler;
    private bool _isSettingsViewVisible;

    public event PropertyChangedEventHandler? PropertyChanged;

    // ==================== 属性 ====================

    public ClickConfig Config
    {
        get => _config;
        set { _config = value; OnPropertyChanged(); }
    }

    public ClickStats Stats
    {
        get => _stats;
        set { _stats = value; OnPropertyChanged(); }
    }

    public bool IsRunning
    {
        get => _stats.IsRunning;
        set
        {
            _stats.IsRunning = value;
            OnPropertyChanged();
            OnPropertyChanged(nameof(IsNotRunning));
            OnPropertyChanged(nameof(ButtonText));
        }
    }

    public bool IsNotRunning => !IsRunning;

    public string ButtonText => IsRunning ? "⏹  停止" : "▶  开始连点";

    public string HotKeyName => NativeMethods.GetKeyName((uint)_config.HotKey);

    public string SessionClicksText => _stats.SessionClicks.ToString("N0");

    public string TotalClicksText => _stats.TotalClicks.ToString("N0");

    public string DurationText => _stats.GetFormattedDuration();

    public string CpsText => _stats.ClicksPerSecond.ToString("F1");

    /// <summary>
    /// 是否选择无限次数
    /// </summary>
    public bool IsUnlimited
    {
        get => _isUnlimited;
        set
        {
            if (_isUnlimited != value)
            {
                _isUnlimited = value;
                OnPropertyChanged();
                OnPropertyChanged(nameof(IsLimited));
                if (value) _config.ClickLimit = 0;
            }
        }
    }

    /// <summary>
    /// 是否选择指定次数
    /// </summary>
    public bool IsLimited
    {
        get => !_isUnlimited;
        set
        {
            if (_isUnlimited == value)
            {
                _isUnlimited = !value;
                OnPropertyChanged();
                OnPropertyChanged(nameof(IsUnlimited));
            }
        }
    }

    /// <summary>
    /// 开机自启动
    /// </summary>
    public bool StartWithWindows
    {
        get => _config.StartWithWindows;
        set
        {
            if (_config.StartWithWindows != value)
            {
                _config.StartWithWindows = value;
                AutoStartService.SetAutoStart(value);
                SaveConfig();
                OnPropertyChanged();
            }
        }
    }

    /// <summary>
    /// 窗口置顶
    /// </summary>
    public bool AlwaysOnTop
    {
        get => _config.AlwaysOnTop;
        set
        {
            if (_config.AlwaysOnTop != value)
            {
                _config.AlwaysOnTop = value;
                SaveConfig();
                OnPropertyChanged();
            }
        }
    }

    /// <summary>
    /// 是否显示设置页面
    /// </summary>
    public bool IsSettingsViewVisible
    {
        get => _isSettingsViewVisible;
        set
        {
            if (_isSettingsViewVisible != value)
            {
                _isSettingsViewVisible = value;
                OnPropertyChanged();
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

    // ==================== 命令 ====================

    public ICommand ToggleCommand { get; }
    public ICommand ChangeHotKeyCommand { get; }
    public ICommand ShowSettingsCommand { get; }
    public ICommand HideSettingsCommand { get; }
    public ICommand OpenProjectUrlCommand { get; }
    public ICommand OpenGitHubUrlCommand { get; }

    // ==================== 构造函数 ====================

    public MainWindowViewModel()
    {
        _configService = new ConfigService();
        _config = _configService.Load();
        _stats = new ClickStats();

        // 初始化无限/指定状态
        _isUnlimited = _config.ClickLimit == 0;

        _clickService = new ClickService();
        _hotKeyService = new HotKeyService();

        // 订阅事件
        _clickService.OnClick += OnClickPerformed;
        _onStartedHandler = () =>
        {
            Application.Current?.Dispatcher.BeginInvoke(() => IsRunning = true);
        };
        _onStoppedHandler = () =>
        {
            Application.Current?.Dispatcher.BeginInvoke(() => IsRunning = false);
        };
        _clickService.OnStarted += _onStartedHandler;
        _clickService.OnStopped += _onStoppedHandler;
        _hotKeyService.OnHotKeyPressed += ToggleClicking;

        // 初始化命令
        ToggleCommand = new RelayCommand(ToggleClicking);
        ChangeHotKeyCommand = new RelayCommand(ChangeHotKey);
        ShowSettingsCommand = new RelayCommand(() => IsSettingsViewVisible = true);
        HideSettingsCommand = new RelayCommand(() => IsSettingsViewVisible = false);
        OpenProjectUrlCommand = new RelayCommand(() => OpenUrl("https://github.com/gura8390/ClickRun.Modern"));
        OpenGitHubUrlCommand = new RelayCommand(() => OpenUrl("https://github.com/gura8390/ClickRun.Modern"));

        // UI更新定时器
        _uiTimer = new System.Windows.Threading.DispatcherTimer
        {
            Interval = TimeSpan.FromMilliseconds(200)
        };
        _uiTimer.Tick += OnTimerTick;
        _uiTimer.Start();
    }

    // ==================== 公共方法 ====================

    public void Initialize(Window window)
    {
        _window = window;

        // 同步开机自启状态
        _config.StartWithWindows = AutoStartService.IsAutoStartEnabled();
        OnPropertyChanged(nameof(StartWithWindows));

        // 注册全局热键
        var hwnd = new WindowInteropHelper(window).Handle;
        bool success = _hotKeyService.Register(hwnd, _config.HotKeyModifiers, _config.HotKey);

        if (!success)
        {
            MessageBox.Show(window,
                $"热键 {HotKeyName} 注册失败，可能已被其他程序占用。\n请尝试在设置中更换热键。",
                "热键冲突",
                MessageBoxButton.OK,
                MessageBoxImage.Warning);
        }
    }

    public void ToggleClicking()
    {
        if (_disposed) return;

        if (IsRunning)
        {
            _clickService.Stop(_stats);
        }
        else
        {
            _clickService.Start(_config, _stats);
        }
    }

    public bool ProcessHotKeyMessage(int msg, IntPtr wParam, IntPtr lParam)
    {
        return _hotKeyService.ProcessMessage(msg, wParam, lParam);
    }

    public void SaveConfig()
    {
        _configService.Save(_config);
    }

    public void ChangeHotKey()
    {
        if (_window == null) return;

        var dialog = new Views.HotKeySelectionWindow(_config.HotKey)
        {
            Owner = _window
        };

        if (dialog.ShowDialog() == true && dialog.SelectedHotKey.HasValue)
        {
            _config.HotKey = dialog.SelectedHotKey.Value;

            var hwnd = new WindowInteropHelper(_window).Handle;
            bool success = _hotKeyService.Register(hwnd, _config.HotKeyModifiers, _config.HotKey);

            if (!success)
            {
                MessageBox.Show(_window,
                    $"热键 {HotKeyName} 注册失败，可能已被其他程序占用。",
                    "热键冲突",
                    MessageBoxButton.OK,
                    MessageBoxImage.Warning);
            }
            else
            {
                SaveConfig();
                OnPropertyChanged(nameof(HotKeyName));
            }
        }
    }

    public void OpenUrl(string url)
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

    // ==================== 私有方法 ====================

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
    }

    protected void OnPropertyChanged([CallerMemberName] string? name = null)
    {
        PropertyChanged?.Invoke(this, new PropertyChangedEventArgs(name));
    }

    public void Dispose()
    {
        if (_disposed) return;
        _disposed = true;

        if (_uiTimer != null)
        {
            _uiTimer.Tick -= OnTimerTick;
            _uiTimer.Stop();
            _uiTimer = null;
        }

        _clickService.OnClick -= OnClickPerformed;
        if (_onStartedHandler != null) _clickService.OnStarted -= _onStartedHandler;
        if (_onStoppedHandler != null) _clickService.OnStopped -= _onStoppedHandler;
        _hotKeyService.OnHotKeyPressed -= ToggleClicking;

        _hotKeyService.Dispose();
        _clickService.Dispose();
    }
}

/// <summary>
/// 简单的 RelayCommand 实现
/// </summary>
public class RelayCommand : ICommand
{
    private readonly Action _execute;
    private readonly Func<bool>? _canExecute;

    public RelayCommand(Action execute, Func<bool>? canExecute = null)
    {
        _execute = execute;
        _canExecute = canExecute;
    }

    public event EventHandler? CanExecuteChanged
    {
        add { CommandManager.RequerySuggested += value; }
        remove { CommandManager.RequerySuggested -= value; }
    }

    public bool CanExecute(object? parameter) => _canExecute?.Invoke() ?? true;

    public void Execute(object? parameter) => _execute();
}

public class RelayCommand<T> : ICommand
{
    private readonly Action<T?> _execute;
    private readonly Func<T?, bool>? _canExecute;

    public RelayCommand(Action<T?> execute, Func<T?, bool>? canExecute = null)
    {
        _execute = execute;
        _canExecute = canExecute;
    }

    public event EventHandler? CanExecuteChanged
    {
        add { CommandManager.RequerySuggested += value; }
        remove { CommandManager.RequerySuggested -= value; }
    }

    public bool CanExecute(object? parameter)
    {
        if (parameter is not T typedParam)
            return _canExecute?.Invoke(default) ?? true;
        return _canExecute?.Invoke(typedParam) ?? true;
    }

    public void Execute(object? parameter)
    {
        if (parameter is not T typedParam)
        {
            _execute(default);
            return;
        }
        _execute(typedParam);
    }
}
