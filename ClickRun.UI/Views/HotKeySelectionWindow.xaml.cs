using System.ComponentModel;
using System.Runtime.CompilerServices;
using System.Windows;
using ClickRun.Core.Helpers;

namespace ClickRun.UI.Views;

/// <summary>
/// 热键选择窗口
/// </summary>
public partial class HotKeySelectionWindow : Window, INotifyPropertyChanged
{
    private string _currentHotKeyName;

    public event PropertyChangedEventHandler? PropertyChanged;

    /// <summary>
    /// 用户选择的热键VK码，null表示取消
    /// </summary>
    public int? SelectedHotKey { get; private set; }

    public string CurrentHotKeyName
    {
        get => _currentHotKeyName;
        set { _currentHotKeyName = value; OnPropertyChanged(); }
    }

    public HotKeySelectionWindow(int currentHotKey)
    {
        InitializeComponent();
        DataContext = this;

        _currentHotKeyName = NativeMethods.GetKeyName((uint)currentHotKey);
        SelectedHotKey = null;
    }

    private void HotKey_Click(object sender, RoutedEventArgs e)
    {
        if (sender is System.Windows.Controls.Button button && button.Tag is string tagStr && int.TryParse(tagStr, out int vk))
        {
            SelectedHotKey = vk;
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
