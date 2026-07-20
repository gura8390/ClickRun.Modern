# ClickRun Modern 技术详解

> 本文档详细说明 ClickRun Modern 的架构设计、核心技术实现和关键代码解析。

---

## 目录

1. [架构设计](#1-架构设计)
2. [核心技术实现](#2-核心技术实现)
3. [线程安全设计](#3-线程安全设计)
4. [配置管理](#4-配置管理)
5. [UI 设计](#5-ui-设计)
6. [关键技术点](#6-关键技术点)

---

## 1. 架构设计

### 1.1 分层架构

```
┌─────────────────────────────────────────────┐
│                   UI 层                      │
│  (WPF + MVVM + Data Binding)                │
├─────────────────────────────────────────────┤
│                 ViewModel 层                 │
│  (MainWindowViewModel + Commands)            │
├─────────────────────────────────────────────┤
│                 服务层 (Services)             │
│  ClickService | HotKeyService | ConfigService│
├─────────────────────────────────────────────┤
│                 模型层 (Models)               │
│  ClickConfig | ClickStats                    │
├─────────────────────────────────────────────┤
│               原生互操作层                    │
│  P/Invoke (user32.dll) | COM (WScript.Shell) │
└─────────────────────────────────────────────┘
```

### 1.2 项目依赖关系

```
ClickRun.UI (WPF 应用)
    ├── 引用 ClickRun.Core (类库)
    ├── 引用 Hardcodet.NotifyIcon.Wpf (托盘图标)
    └── 引用 CommunityToolkit.Mvvm (MVVM 工具)

ClickRun.Core (类库)
    ├── 引用 System.Text.Json (JSON 序列化)
    └── 引用 Microsoft.Win32 (注册表操作)
```

---

## 2. 核心技术实现

### 2.1 鼠标模拟 (SendInput)

**文件**: `ClickRun.Core/Helpers/MouseSimulator.cs`

使用 Win32 API `SendInput` 模拟鼠标点击：

```csharp
[DllImport("user32.dll", SetLastError = true)]
public static extern uint SendInput(uint nInputs, INPUT[] pInputs, int cbSize);

[StructLayout(LayoutKind.Sequential)]
public struct INPUT
{
    public uint type;
    public MOUSEINPUT mi;
}

[StructLayout(LayoutKind.Sequential)]
public struct MOUSEINPUT
{
    public int dx;
    public int dy;
    public uint mouseData;
    public uint dwFlags;
    public uint time;
    public IntPtr dwExtraInfo;
}
```

**点击实现**:
```csharp
public static void Click(MouseButton button, ClickMode mode = ClickMode.Single)
{
    int clickCount = mode switch
    {
        ClickMode.Single => 1,
        ClickMode.Double => 2,
        ClickMode.Triple => 3,
        _ => 1
    };

    for (int i = 0; i < clickCount; i++)
    {
        SimulateClick(button);
        if (i < clickCount - 1)
            Thread.Sleep(20); // 双击/三击间隔
    }
}
```

### 2.2 全局热键 (RegisterHotKey)

**文件**: `ClickRun.Core/Services/HotKeyService.cs`

使用 Win32 API 注册全局热键：

```csharp
[DllImport("user32.dll", SetLastError = true)]
public static extern bool RegisterHotKey(IntPtr hWnd, int id, uint fsModifiers, uint vk);

[DllImport("user32.dll", SetLastError = true)]
public static extern bool UnregisterHotKey(IntPtr hWnd, int id);
```

**窗口消息处理**:
```csharp
// MainWindow.xaml.cs
private IntPtr WndProc(IntPtr hwnd, int msg, IntPtr wParam, IntPtr lParam, ref bool handled)
{
    if (msg == NativeMethods.WM_HOTKEY)
    {
        if (_viewModel.ProcessHotKeyMessage(msg, wParam, lParam))
            handled = true;
    }
    return IntPtr.Zero;
}
```

### 2.3 开机自启 (双保险策略)

**文件**: `ClickRun.Core/Services/AutoStartService.cs`

同时使用两种方式确保自启动可靠：

**方式一：注册表**
```csharp
private static bool EnableRegistryAutoStart()
{
    using var key = Registry.CurrentUser.OpenSubKey(
        @"Software\Microsoft\Windows\CurrentVersion\Run", true);
    
    var commandLine = GetCommandLine(); // 路径含空格时加引号
    key.SetValue("ClickRun", commandLine, RegistryValueKind.String);
    return true;
}
```

**方式二：启动文件夹快捷方式**
```csharp
private static bool EnableStartupFolderAutoStart()
{
    dynamic shell = Activator.CreateInstance(
        Type.GetTypeFromProgID("WScript.Shell"));
    
    var shortcut = shell.CreateShortcut(GetShortcutPath());
    shortcut.TargetPath = GetExecutablePath();
    shortcut.WorkingDirectory = Path.GetDirectoryName(GetExecutablePath());
    shortcut.Description = "ClickRun 鼠标连点器";
    shortcut.Save();
    
    Marshal.FinalReleaseComObject(shortcut);
    Marshal.FinalReleaseComObject(shell);
    return true;
}
```

### 2.4 单实例保护 (Mutex)

**文件**: `ClickRun.UI/App.xaml.cs`

```csharp
private static Mutex? _mutex;

protected override void OnStartup(StartupEventArgs e)
{
    const string mutexName = "ClickRun.SingleInstance.Mutex";
    _mutex = new Mutex(true, mutexName, out bool isNewInstance);

    if (!isNewInstance)
    {
        Shutdown(); // 已有实例在运行
        return;
    }

    base.OnStartup(e);
}
```

---

## 3. 线程安全设计

### 3.1 点击统计 (Interlocked)

**文件**: `ClickRun.Core/Models/ClickStats.cs`

使用 `Interlocked` 保证计数器原子性：

```csharp
private long _sessionClicks;
private long _totalClicks;
private volatile double _clicksPerSecond;

public void RecordClick()
{
    Interlocked.Increment(ref _sessionClicks);
    Interlocked.Increment(ref _totalClicks);

    // 计算 CPS
    var elapsed = (DateTime.Now - _sessionStartTime.Value).TotalSeconds;
    if (elapsed > 0)
    {
        var clicks = Interlocked.Read(ref _sessionClicks);
        ClicksPerSecond = clicks / elapsed;
    }
}
```

### 3.2 点击服务 (lock + CancellationToken)

**文件**: `ClickRun.Core/Services/ClickService.cs`

```csharp
private readonly object _lock = new();
private CancellationTokenSource? _cts;

public void Start(ClickConfig config, ClickStats stats)
{
    lock (_lock)
    {
        if (IsRunning) return;
        IsRunning = true;
        _cts = new CancellationTokenSource();
    }

    // 在锁外触发事件，避免死锁
    OnStarted?.Invoke();
}

private void ClickLoop(ClickConfig config, ClickStats stats, CancellationToken token)
{
    try
    {
        while (!token.IsCancellationRequested)
        {
            MouseSimulator.Click(config.Button, config.Mode);
            stats.RecordClick();
            OnClick?.Invoke();

            // 使用 WaitHandle 支持取消
            token.WaitHandle.WaitOne(delay);
        }
    }
    finally
    {
        lock (_lock) { IsRunning = false; }
        OnStopped?.Invoke();
    }
}
```

---

## 4. 配置管理

### 4.1 配置模型

**文件**: `ClickRun.Core/Models/ClickConfig.cs`

```csharp
public class ClickConfig
{
    public int HotKey { get; set; } = 117;           // F6
    public int HotKeyModifiers { get; set; } = 0;    // 无修饰键
    public MouseButton Button { get; set; } = MouseButton.Left;
    public int Interval { get; set; } = 100;          // 100ms
    public int RandomDelay { get; set; } = 0;         // 0%
    public ClickMode Mode { get; set; } = ClickMode.Single;
    public int ClickLimit { get; set; } = 0;          // 无限
    public bool StartWithWindows { get; set; } = false;
    public bool MinimizeToTray { get; set; } = true;
    public bool PlaySound { get; set; } = false;
    public bool AlwaysOnTop { get; set; } = false;
}
```

### 4.2 原子写入

**文件**: `ClickRun.Core/Services/ConfigService.cs`

```csharp
public bool Save(ClickConfig config)
{
    var json = JsonSerializer.Serialize(config, _jsonOptions);
    var tempPath = _configPath + ".tmp";

    // 先写入临时文件
    File.WriteAllText(tempPath, json);

    // 原子替换
    if (File.Exists(_configPath))
    {
        var backupPath = _configPath + ".bak";
        File.Replace(tempPath, _configPath, backupPath);
        try { File.Delete(backupPath); } catch { }
    }
    else
    {
        File.Move(tempPath, _configPath);
    }
    return true;
}
```

### 4.3 配置验证

```csharp
private static void ValidateConfig(ClickConfig config)
{
    config.Interval = Math.Max(1, Math.Min(10000, config.Interval));
    config.RandomDelay = Math.Max(0, Math.Min(50, config.RandomDelay));
    config.ClickLimit = Math.Max(0, config.ClickLimit);
    config.HotKey = Math.Max(0, config.HotKey);

    if (!Enum.IsDefined(config.Button))
        config.Button = MouseButton.Left;
    if (!Enum.IsDefined(config.Mode))
        config.Mode = ClickMode.Single;
}
```

---

## 5. UI 设计

### 5.1 杂志风主题

**文件**: `ClickRun.UI/Themes/MagazineTheme.xaml`

**设计原则**:
- 克制优于炫技 — 无阴影、无渐变
- 结构优于装饰 — 字号对比 + 网格留白
- 内容层级由字体定义 — 衬线标题 + 非衬线正文 + 等宽数据

**字体方案**:
```xml
<FontFamily x:Key="TitleFont">pack://application:,,,/Fonts/#方正公文楷体</FontFamily>
<FontFamily x:Key="BodyFont">Microsoft YaHei UI, Segoe UI</FontFamily>
<FontFamily x:Key="MonoFont">Cascadia Code, Consolas</FontFamily>
```

**配色方案**:
```xml
<Color x:Key="InkColor">#1a1a2e</Color>      <!-- 深墨 -->
<Color x:Key="PaperColor">#f5f5f0</Color>     <!-- 宣纸白 -->
<Color x:Key="AccentColor">#e94560</Color>    <!-- 朱砂红 -->
<Color x:Key="DataColor">#0f3460</Color>      <!-- 靛蓝 -->
```

### 5.2 双界面切换

**文件**: `ClickRun.UI/Views/MainWindow.xaml`

主界面和设置界面通过 Visibility 绑定切换：

```xml
<!-- 主界面 -->
<Grid x:Name="MainPanel" Visibility="{Binding IsMainViewVisible}">
    <!-- 核心功能：热键、点击设置、开始按钮、统计 -->
</Grid>

<!-- 设置界面 -->
<Grid x:Name="SettingsPanel" Visibility="{Binding IsSettingsViewVisibleAsVisibility}">
    <!-- 应用设置：开机自启、托盘、置顶、音效、关于 -->
</Grid>
```

**ViewModel 切换逻辑**:
```csharp
public bool IsSettingsViewVisible
{
    get => _isSettingsViewVisible;
    set
    {
        _isSettingsViewVisible = value;
        OnPropertyChanged();
        OnPropertyChanged(nameof(IsMainViewVisible));
        OnPropertyChanged(nameof(IsSettingsViewVisibleAsVisibility));
    }
}

public Visibility IsMainViewVisible => 
    _isSettingsViewVisible ? Visibility.Collapsed : Visibility.Visible;

public Visibility IsSettingsViewVisibleAsVisibility => 
    _isSettingsViewVisible ? Visibility.Visible : Visibility.Collapsed;
```

### 5.3 自定义控件样式

**Toggle 开关**:
```xml
<Style x:Key="ToggleStyle" TargetType="CheckBox">
    <Setter Property="Template">
        <ControlTemplate TargetType="CheckBox">
            <Border x:Name="toggleBorder" Width="36" Height="20" CornerRadius="10">
                <Ellipse x:Name="toggleDot" Width="16" Height="16" />
            </Border>
            <ControlTemplate.Triggers>
                <Trigger Property="IsChecked" Value="True">
                    <Setter TargetName="toggleBorder" Property="Background" Value="{DynamicResource AccentBrush}" />
                    <Setter TargetName="toggleDot" Property="Margin" Value="18,0,0,0" />
                </Trigger>
            </ControlTemplate.Triggers>
        </ControlTemplate>
    </Setter>
</Style>
```

---

## 6. 关键技术点

### 6.1 P/Invoke 结构体对齐

```csharp
[StructLayout(LayoutKind.Sequential)]
public struct INPUT
{
    public uint type;
    public MOUSEINPUT mi;
}

[StructLayout(LayoutKind.Sequential)]
public struct MOUSEINPUT
{
    public int dx;
    public int dy;
    public uint mouseData;
    public uint dwFlags;
    public uint time;
    public IntPtr dwExtraInfo;  // 64位系统上为8字节
}
```

### 6.2 COM 对象生命周期管理

```csharp
dynamic? shell = null;
dynamic? shortcut = null;
try
{
    shell = Activator.CreateInstance(Type.GetTypeFromProgID("WScript.Shell"));
    shortcut = shell.CreateShortcut(shortcutPath);
    // ... 使用 shortcut
}
finally
{
    if (shortcut != null) Marshal.FinalReleaseComObject(shortcut);
    if (shell != null) Marshal.FinalReleaseComObject(shell);
}
```

### 6.3 WndProc 消息处理

```csharp
// 注册 Hook
_hwndSource = HwndSource.FromHwnd(hwnd);
_hwndSource?.AddHook(WndProc);

// 处理消息
private IntPtr WndProc(IntPtr hwnd, int msg, IntPtr wParam, IntPtr lParam, ref bool handled)
{
    try
    {
        if (_viewModel.ProcessHotKeyMessage(msg, wParam, lParam))
            handled = true;
    }
    catch (Exception ex)
    {
        Debug.WriteLine($"WndProc error: {ex.Message}");
    }
    return IntPtr.Zero;
}
```

### 6.4 事件订阅与取消

```csharp
// 订阅
_clickService.OnClick += OnClickPerformed;
_onStartedHandler = () => Dispatcher.BeginInvoke(() => IsRunning = true);
_clickService.OnStarted += _onStartedHandler;

// 取消订阅（在 Dispose 中）
_clickService.OnClick -= OnClickPerformed;
if (_onStartedHandler != null) _clickService.OnStarted -= _onStartedHandler;
```

---

## 附录：配置项参考

| 配置项 | 类型 | 默认值 | 范围 | 说明 |
|--------|------|--------|------|------|
| HotKey | int | 117 (F6) | 0-255 | 虚拟键码 |
| HotKeyModifiers | int | 0 | 0/1/2/4/8 | 修饰键位掩码 |
| Button | enum | Left | Left/Right/Middle | 鼠标按键 |
| Interval | int | 100 | 1-10000 | 点击间隔(ms) |
| RandomDelay | int | 0 | 0-50 | 随机延迟(%) |
| Mode | enum | Single | Single/Double/Triple | 点击模式 |
| ClickLimit | int | 0 | ≥0 | 点击次数(0=无限) |
| StartWithWindows | bool | false | - | 开机自启 |
| MinimizeToTray | bool | true | - | 最小化到托盘 |
| PlaySound | bool | false | - | 播放音效 |
| AlwaysOnTop | bool | false | - | 窗口置顶 |

---

**ClickRun Modern** — 技术详解文档
