using System.Globalization;
using System.Windows.Data;

namespace ClickRun.UI;

/// <summary>
/// 通用枚举转布尔转换器 - ConverterParameter 传目标枚举值
/// 用法：Converter="{x:Static local:EnumToBoolConverter.Instance}"
///       ConverterParameter="{x:Static models:MouseButton.Left}"
/// </summary>
public class EnumToBoolConverter : IValueConverter
{
    public static readonly EnumToBoolConverter Instance = new();

    public object Convert(object value, Type targetType, object parameter, CultureInfo culture)
    {
        if (value is Enum actual && parameter is Enum target)
        {
            return Equals(actual, target);
        }
        return false;
    }

    public object ConvertBack(object value, Type targetType, object parameter, CultureInfo culture)
    {
        // 只有当 IsChecked=true 时才写回目标值，取消选中时忽略
        if (value is bool isChecked && isChecked && parameter != null)
        {
            return parameter;
        }
        return Binding.DoNothing;
    }
}
