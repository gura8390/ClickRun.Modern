using System.ComponentModel;
using System.Runtime.CompilerServices;
using System.Windows;

namespace ClickRun.UI.Views;

/// <summary>
/// 保存方案对话框 - 输入方案名称，校验空值与重名
/// </summary>
public partial class SaveProfileDialog : Window, INotifyPropertyChanged
{
    private readonly HashSet<string> _existingNames;
    private string _profileName = string.Empty;
    private string _errorMessage = string.Empty;

    public event PropertyChangedEventHandler? PropertyChanged;

    /// <summary>
    /// 输入的方案名称
    /// </summary>
    public string ProfileName
    {
        get => _profileName;
        set
        {
            _profileName = value;
            OnPropertyChanged();
            Validate();
        }
    }

    /// <summary>
    /// 校验错误消息（空时隐藏）
    /// </summary>
    public string ErrorMessage
    {
        get => _errorMessage;
        private set
        {
            _errorMessage = value;
            OnPropertyChanged();
            OnPropertyChanged(nameof(ErrorVisibility));
        }
    }

    public Visibility ErrorVisibility =>
        string.IsNullOrEmpty(_errorMessage) ? Visibility.Collapsed : Visibility.Visible;

    /// <summary>
    /// 名称是否有效（决定"确定"按钮可用性）
    /// </summary>
    public bool IsConfirmEnabled =>
        !string.IsNullOrWhiteSpace(_profileName) && string.IsNullOrEmpty(_errorMessage);

    public SaveProfileDialog(IEnumerable<string> existingNames)
    {
        InitializeComponent();
        DataContext = this;

        _existingNames = new HashSet<string>(existingNames, StringComparer.OrdinalIgnoreCase);
        Loaded += (_, _) => NameTextBox.Focus();
    }

    private void Validate()
    {
        var name = _profileName.Trim();

        if (name.Length > 0 && _existingNames.Contains(name))
        {
            ErrorMessage = "已存在同名方案";
        }
        else
        {
            ErrorMessage = string.Empty;
        }

        OnPropertyChanged(nameof(IsConfirmEnabled));
    }

    private void Confirm_Click(object sender, RoutedEventArgs e)
    {
        if (IsConfirmEnabled)
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
