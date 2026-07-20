using System.Threading;
using System.Windows;

namespace ClickRun.UI;

/// <summary>
/// Interaction logic for App.xaml
/// </summary>
public partial class App : Application
{
    private static Mutex? _mutex;

    protected override void OnStartup(StartupEventArgs e)
    {
        // 单实例检测 - 防止双保险自启动机制导致启动多个实例
        const string mutexName = "ClickRun.SingleInstance.Mutex";
        _mutex = new Mutex(true, mutexName, out bool isNewInstance);

        if (!isNewInstance)
        {
            // 已有实例在运行，直接退出
            Shutdown();
            return;
        }

        base.OnStartup(e);
        // 杂志风主题已在 App.xaml 中直接加载
    }

    protected override void OnExit(ExitEventArgs e)
    {
        // 释放 Mutex
        _mutex?.ReleaseMutex();
        _mutex?.Dispose();
        _mutex = null;

        base.OnExit(e);
    }
}
