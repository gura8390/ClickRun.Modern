using System.ComponentModel;
using System.Windows;
using System.Windows.Interop;
using ClickRun.Core.Helpers;
using ClickRun.UI.ViewModels;

namespace ClickRun.UI.Views;

/// <summary>
/// Interaction logic for MainWindow.xaml
/// </summary>
public partial class MainWindow : Window
{
    private readonly MainWindowViewModel _viewModel;
    private HwndSource? _hwndSource;
    private bool _isExiting;
    private bool _cleanupDone;

    public MainWindow()
    {
        InitializeComponent();

        // 设置托盘图标
        try
        {
            var iconUri = new Uri("pack://application:,,,/Resources/clickrun.ico");
            TrayIcon.IconSource = new System.Windows.Media.Imaging.BitmapImage(iconUri);
        }
        catch { }

        _viewModel = new MainWindowViewModel();
        DataContext = _viewModel;

        Loaded += MainWindow_Loaded;
        Closing += MainWindow_Closing;
        SourceInitialized += MainWindow_SourceInitialized;
    }

    private void MainWindow_SourceInitialized(object? sender, EventArgs e)
    {
        var hwnd = new WindowInteropHelper(this).Handle;
        _hwndSource = HwndSource.FromHwnd(hwnd);
        _hwndSource?.AddHook(WndProc);
    }

    private void MainWindow_Loaded(object sender, RoutedEventArgs e)
    {
        _viewModel.Initialize(this);
    }

    private void MainWindow_Closing(object? sender, CancelEventArgs e)
    {
        if (_isExiting)
        {
            // 已确认退出，执行清理
            CleanupAndExit();
            return;
        }

        // 如果设置了最小化到托盘，则不关闭，只隐藏
        if (_viewModel.Config.MinimizeToTray)
        {
            e.Cancel = true;
            TrayIcon.Visibility = Visibility.Visible;
            Hide();
            return;
        }

        // 非托盘模式，确认退出
        e.Cancel = true;
        _isExiting = true;
        Dispatcher.BeginInvoke(() => Close());
    }

    /// <summary>
    /// 清理资源
    /// </summary>
    private void CleanupAndExit()
    {
        if (_cleanupDone) return;
        _cleanupDone = true;

        // 先移除消息钩子，防止后续 WndProc 调用
        if (_hwndSource != null)
        {
            _hwndSource.RemoveHook(WndProc);
            // 不要 Dispose HwndSource，让 WPF 的 Shutdown 流程自然处理
            _hwndSource = null;
        }

        // 取消事件订阅
        Loaded -= MainWindow_Loaded;
        Closing -= MainWindow_Closing;
        SourceInitialized -= MainWindow_SourceInitialized;

        // 保存配置并释放 ViewModel
        _viewModel.SaveConfig();
        _viewModel.Dispose();

        // 清理托盘图标
        try { TrayIcon?.Dispose(); } catch { }
    }

    /// <summary>
    /// 处理Windows消息（热键）- 通过 HotKeyService 验证热键ID
    /// </summary>
    private IntPtr WndProc(IntPtr hwnd, int msg, IntPtr wParam, IntPtr lParam, ref bool handled)
    {
        try
        {
            if (_viewModel.ProcessHotKeyMessage(msg, wParam, lParam))
            {
                handled = true;
            }
        }
        catch (Exception ex)
        {
            System.Diagnostics.Debug.WriteLine($"WndProc hotkey error: {ex.Message}");
        }
        return IntPtr.Zero;
    }

    /// <summary>
    /// 托盘菜单 - 显示主窗口
    /// </summary>
    private void ShowMenuItem_Click(object sender, RoutedEventArgs e)
    {
        TrayIcon.Visibility = Visibility.Collapsed; // 隐藏托盘图标
        Show();
        WindowState = WindowState.Normal;
        Activate();
    }

    /// <summary>
    /// 托盘菜单 - 退出应用
    /// </summary>
    private void ExitMenuItem_Click(object sender, RoutedEventArgs e)
    {
        _isExiting = true;
        Close();
    }

    /// <summary>
    /// 从托盘恢复窗口（双击托盘图标时由命令调用）
    /// </summary>
    public void ShowFromTray()
    {
        TrayIcon.Visibility = Visibility.Collapsed; // 隐藏托盘图标
        Show();
        WindowState = WindowState.Normal;
        Activate();
    }
}
