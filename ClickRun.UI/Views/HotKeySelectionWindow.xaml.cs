using System.ComponentModel;
using System.Runtime.CompilerServices;
using System.Windows;
using System.Windows.Input;
using ClickRun.Core.Helpers;

namespace ClickRun.UI.Views;

/// <summary>
/// 热键捕获窗口 - 直接按下组合键完成捕获
/// </summary>
public partial class HotKeySelectionWindow : Window, INotifyPropertyChanged
{
    private string _currentHotKeyName;
    private string _previewText = "按下热键…";
    private bool _isConfirmEnabled;

    public event PropertyChangedEventHandler? PropertyChanged;

    /// <summary>
    /// 用户选择的热键VK码，null表示取消
    /// </summary>
    public int? SelectedHotKey { get; private set; }

    /// <summary>
    /// 用户选择的修饰键（MOD_ALT/MOD_CONTROL/MOD_SHIFT 组合）
    /// </summary>
    public int SelectedModifiers { get; private set; }

    public string CurrentHotKeyName
    {
        get => _currentHotKeyName;
        set { _currentHotKeyName = value; OnPropertyChanged(); }
    }

    /// <summary>
    /// 捕获预览文本（实时更新）
    /// </summary>
    public string PreviewText
    {
        get => _previewText;
        set { _previewText = value; OnPropertyChanged(); }
    }

    /// <summary>
    /// 是否已捕获到有效按键（决定"确定"按钮可用性）
    /// </summary>
    public bool IsConfirmEnabled
    {
        get => _isConfirmEnabled;
        set { _isConfirmEnabled = value; OnPropertyChanged(); }
    }

    public HotKeySelectionWindow(int currentHotKey, int currentModifiers)
    {
        InitializeComponent();
        DataContext = this;

        _currentHotKeyName = NativeMethods.FormatHotKey(currentModifiers, currentHotKey);
        SelectedHotKey = null;
        SelectedModifiers = 0;
    }

    private void Window_PreviewKeyDown(object sender, KeyEventArgs e)
    {
        var key = e.Key == Key.System ? e.SystemKey : e.Key;

        // Esc 取消
        if (key == Key.Escape)
        {
            DialogResult = false;
            Close();
            e.Handled = true;
            return;
        }

        // Win 键不参与捕获
        if (key is Key.LWin or Key.RWin)
        {
            e.Handled = true;
            return;
        }

        // 读取当前修饰键状态
        var mods = Keyboard.Modifiers;
        int modFlags = 0;
        if (mods.HasFlag(ModifierKeys.Control)) modFlags |= NativeMethods.MOD_CONTROL;
        if (mods.HasFlag(ModifierKeys.Alt)) modFlags |= NativeMethods.MOD_ALT;
        if (mods.HasFlag(ModifierKeys.Shift)) modFlags |= NativeMethods.MOD_SHIFT;

        // 单独按修饰键：只更新预览，不完成捕获
        if (IsModifierKey(key))
        {
            SelectedHotKey = null;
            PreviewText = BuildPreview(modFlags, null);
            IsConfirmEnabled = false;
            e.Handled = true;
            return;
        }

        int vk = KeyInterop.VirtualKeyFromKey(key);
        SelectedModifiers = modFlags;
        SelectedHotKey = vk;
        PreviewText = BuildPreview(modFlags, vk);
        IsConfirmEnabled = true;
        e.Handled = true;
    }

    private static bool IsModifierKey(Key key)
    {
        return key is Key.LeftCtrl or Key.RightCtrl
            or Key.LeftAlt or Key.RightAlt
            or Key.LeftShift or Key.RightShift;
    }

    private static string BuildPreview(int modifiers, int? vk)
    {
        var parts = new List<string>(4);
        if ((modifiers & NativeMethods.MOD_CONTROL) != 0) parts.Add("Ctrl");
        if ((modifiers & NativeMethods.MOD_ALT) != 0) parts.Add("Alt");
        if ((modifiers & NativeMethods.MOD_SHIFT) != 0) parts.Add("Shift");
        parts.Add(vk.HasValue ? NativeMethods.GetKeyName((uint)vk.Value) : "…");
        return string.Join(" + ", parts);
    }

    private void Confirm_Click(object sender, RoutedEventArgs e)
    {
        if (SelectedHotKey.HasValue)
        {
            DialogResult = true;
            Close();
        }
    }

    private void Cancel_Click(object sender, RoutedEventArgs e)
    {
        DialogResult = false;
        Close();
    }

    protected void OnPropertyChanged([CallerMemberName] string? name = null)
    {
        PropertyChanged?.Invoke(this, new PropertyChangedEventArgs(name));
    }
}
