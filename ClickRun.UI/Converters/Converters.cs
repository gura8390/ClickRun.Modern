using System.Globalization;
using System.Windows;
using System.Windows.Data;
using ClickRun.Core.Models;

namespace ClickRun.UI;

/// <summary>
/// 鼠标按键枚举转换器 - 每个实例持有目标值
/// </summary>
public class EnumConverter : IValueConverter
{
    public static readonly EnumConverter Left = new(MouseButton.Left);
    public static readonly EnumConverter Right = new(MouseButton.Right);
    public static readonly EnumConverter Middle = new(MouseButton.Middle);

    private readonly MouseButton _targetValue;

    public EnumConverter(MouseButton targetValue)
    {
        _targetValue = targetValue;
    }

    public object Convert(object value, Type targetType, object parameter, CultureInfo culture)
    {
        if (value is MouseButton button)
        {
            return button == _targetValue;
        }
        return false;
    }

    public object ConvertBack(object value, Type targetType, object parameter, CultureInfo culture)
    {
        // 只有当 IsChecked=true 时才写回目标值，取消选中时忽略
        if (value is bool isChecked && isChecked)
        {
            return _targetValue;
        }
        return Binding.DoNothing;
    }
}

/// <summary>
/// 点击模式枚举转换器 - 每个实例持有目标值
/// </summary>
public class ClickModeConverter : IValueConverter
{
    public static readonly ClickModeConverter Single = new(ClickMode.Single);
    public static readonly ClickModeConverter Double = new(ClickMode.Double);
    public static readonly ClickModeConverter Triple = new(ClickMode.Triple);

    private readonly ClickMode _targetValue;

    public ClickModeConverter(ClickMode targetValue)
    {
        _targetValue = targetValue;
    }

    public object Convert(object value, Type targetType, object parameter, CultureInfo culture)
    {
        if (value is ClickMode mode)
        {
            return mode == _targetValue;
        }
        return false;
    }

    public object ConvertBack(object value, Type targetType, object parameter, CultureInfo culture)
    {
        // 只有当 IsChecked=true 时才写回目标值，取消选中时忽略
        if (value is bool isChecked && isChecked)
        {
            return _targetValue;
        }
        return Binding.DoNothing;
    }
}

/// <summary>
/// 布尔反转转换器
/// </summary>
public class InverseBoolConverter : IValueConverter
{
    public static readonly InverseBoolConverter Instance = new();

    public object Convert(object value, Type targetType, object parameter, CultureInfo culture)
    {
        if (value is bool b)
            return !b;
        return DependencyProperty.UnsetValue;
    }

    public object ConvertBack(object value, Type targetType, object parameter, CultureInfo culture)
    {
        if (value is bool b)
            return !b;
        return DependencyProperty.UnsetValue;
    }
}
