using Microsoft.Win32;

namespace ClickRun.Core.Services;

/// <summary>
/// 开机自启动服务 - 双保险策略（注册表 + 启动文件夹快捷方式）
/// </summary>
public static class AutoStartService
{
    private const string AppName = "ClickRun";
    private const string RunKeyPath = @"Software\Microsoft\Windows\CurrentVersion\Run";

    /// <summary>
    /// 获取当前程序的完整路径
    /// </summary>
    private static string GetExecutablePath()
    {
        return Environment.ProcessPath ?? AppDomain.CurrentDomain.BaseDirectory;
    }

    /// <summary>
    /// 获取启动文件夹路径
    /// </summary>
    private static string GetStartupFolderPath()
    {
        return Environment.GetFolderPath(Environment.SpecialFolder.Startup);
    }

    /// <summary>
    /// 获取快捷方式文件路径
    /// </summary>
    private static string GetShortcutPath()
    {
        return Path.Combine(GetStartupFolderPath(), $"{AppName}.lnk");
    }

    /// <summary>
    /// 获取命令行格式的路径（用引号包围以处理空格）
    /// </summary>
    private static string GetCommandLine()
    {
        var path = GetExecutablePath();
        return path.Contains(' ') ? $"\"{path}\"" : path;
    }

    // ==================== 注册表方式 ====================

    /// <summary>
    /// 检查注册表是否已设置开机自启
    /// </summary>
    private static bool IsRegistryAutoStartEnabled()
    {
        try
        {
            using var key = Registry.CurrentUser.OpenSubKey(RunKeyPath, false);
            if (key == null) return false;

            var value = key.GetValue(AppName) as string;
            if (string.IsNullOrEmpty(value)) return false;

            var currentPath = GetExecutablePath();
            var cleanValue = value.Trim('"');

            return string.Equals(cleanValue, currentPath, StringComparison.OrdinalIgnoreCase);
        }
        catch
        {
            return false;
        }
    }

    /// <summary>
    /// 通过注册表设置开机自启
    /// </summary>
    private static bool EnableRegistryAutoStart()
    {
        try
        {
            using var key = Registry.CurrentUser.OpenSubKey(RunKeyPath, true);
            if (key == null) return false;

            var commandLine = GetCommandLine();
            key.SetValue(AppName, commandLine, RegistryValueKind.String);
            return true;
        }
        catch (Exception ex)
        {
            System.Diagnostics.Debug.WriteLine($"EnableRegistryAutoStart failed: {ex.Message}");
            return false;
        }
    }

    /// <summary>
    /// 通过注册表取消开机自启
    /// </summary>
    private static bool DisableRegistryAutoStart()
    {
        try
        {
            using var key = Registry.CurrentUser.OpenSubKey(RunKeyPath, true);
            if (key == null) return true;

            if (key.GetValue(AppName) == null) return true;

            key.DeleteValue(AppName, false);
            return true;
        }
        catch (Exception ex)
        {
            System.Diagnostics.Debug.WriteLine($"DisableRegistryAutoStart failed: {ex.Message}");
            return false;
        }
    }

    // ==================== 启动文件夹方式 ====================

    /// <summary>
    /// 检查启动文件夹快捷方式是否存在
    /// </summary>
    private static bool IsStartupFolderAutoStartEnabled()
    {
        dynamic? shell = null;
        dynamic? shortcut = null;
        try
        {
            var shortcutPath = GetShortcutPath();
            if (!File.Exists(shortcutPath)) return false;

            shell = Activator.CreateInstance(Type.GetTypeFromProgID("WScript.Shell"));
            if (shell == null) return false;

            shortcut = shell.CreateShortcut(shortcutPath);
            var targetPath = shortcut.TargetPath as string;

            if (string.IsNullOrEmpty(targetPath)) return false;

            var currentPath = GetExecutablePath();
            return string.Equals(targetPath.Trim('"'), currentPath.Trim('"'), StringComparison.OrdinalIgnoreCase);
        }
        catch
        {
            return false;
        }
        finally
        {
            if (shortcut != null) System.Runtime.InteropServices.Marshal.FinalReleaseComObject(shortcut);
            if (shell != null) System.Runtime.InteropServices.Marshal.FinalReleaseComObject(shell);
        }
    }

    /// <summary>
    /// 在启动文件夹创建快捷方式
    /// </summary>
    private static bool EnableStartupFolderAutoStart()
    {
        dynamic? shell = null;
        dynamic? shortcut = null;
        try
        {
            var shortcutPath = GetShortcutPath();
            var exePath = GetExecutablePath();
            var workingDir = Path.GetDirectoryName(exePath) ?? "";

            shell = Activator.CreateInstance(Type.GetTypeFromProgID("WScript.Shell"));
            if (shell == null) return false;

            shortcut = shell.CreateShortcut(shortcutPath);
            shortcut.TargetPath = exePath;
            shortcut.WorkingDirectory = workingDir;
            shortcut.Description = "ClickRun 鼠标连点器";
            shortcut.Save();

            System.Diagnostics.Debug.WriteLine($"Startup folder shortcut created: {shortcutPath}");
            return true;
        }
        catch (Exception ex)
        {
            System.Diagnostics.Debug.WriteLine($"EnableStartupFolderAutoStart failed: {ex.Message}");
            return false;
        }
        finally
        {
            if (shortcut != null) System.Runtime.InteropServices.Marshal.FinalReleaseComObject(shortcut);
            if (shell != null) System.Runtime.InteropServices.Marshal.FinalReleaseComObject(shell);
        }
    }

    /// <summary>
    /// 删除启动文件夹快捷方式
    /// </summary>
    private static bool DisableStartupFolderAutoStart()
    {
        try
        {
            var shortcutPath = GetShortcutPath();
            if (File.Exists(shortcutPath))
            {
                File.Delete(shortcutPath);
                System.Diagnostics.Debug.WriteLine("Startup folder shortcut deleted");
            }
            return true;
        }
        catch (Exception ex)
        {
            System.Diagnostics.Debug.WriteLine($"DisableStartupFolderAutoStart failed: {ex.Message}");
            return false;
        }
    }

    // ==================== 公共接口 ====================

    /// <summary>
    /// 检查是否已设置开机自启（双保险：任一方式启用即返回true）
    /// </summary>
    public static bool IsAutoStartEnabled()
    {
        return IsRegistryAutoStartEnabled() || IsStartupFolderAutoStartEnabled();
    }

    /// <summary>
    /// 设置开机自启（双保险：同时启用两种方式）
    /// </summary>
    /// <returns>是否成功（至少一种方式成功）</returns>
    public static bool EnableAutoStart()
    {
        bool registryResult = EnableRegistryAutoStart();
        bool startupFolderResult = EnableStartupFolderAutoStart();

        System.Diagnostics.Debug.WriteLine($"EnableAutoStart - Registry: {registryResult}, StartupFolder: {startupFolderResult}");

        // 至少一种方式成功即可
        return registryResult || startupFolderResult;
    }

    /// <summary>
    /// 取消开机自启（双保险：同时取消两种方式）
    /// </summary>
    /// <returns>是否成功</returns>
    public static bool DisableAutoStart()
    {
        bool registryResult = DisableRegistryAutoStart();
        bool startupFolderResult = DisableStartupFolderAutoStart();

        System.Diagnostics.Debug.WriteLine($"DisableAutoStart - Registry: {registryResult}, StartupFolder: {startupFolderResult}");

        // 两种方式都成功才算成功
        return registryResult && startupFolderResult;
    }

    /// <summary>
    /// 切换开机自启状态
    /// </summary>
    /// <param name="enable">是否启用</param>
    /// <returns>是否成功</returns>
    public static bool SetAutoStart(bool enable)
    {
        return enable ? EnableAutoStart() : DisableAutoStart();
    }
}
