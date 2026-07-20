using System.Text.Json;
using ClickRun.Core.Models;

namespace ClickRun.Core.Services;

/// <summary>
/// 配置持久化服务 - 原子写入防止数据损坏
/// </summary>
public class ConfigService
{
    private readonly string _configPath;
    private readonly string _configDir;
    private readonly JsonSerializerOptions _jsonOptions;

    public ConfigService()
    {
        _configDir = Path.Combine(
            Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData),
            "ClickRun"
        );
        _configPath = Path.Combine(_configDir, "config.json");

        _jsonOptions = new JsonSerializerOptions
        {
            WriteIndented = true,
            PropertyNamingPolicy = JsonNamingPolicy.CamelCase
        };
    }

    /// <summary>
    /// 加载配置
    /// </summary>
    public ClickConfig Load()
    {
        try
        {
            if (File.Exists(_configPath))
            {
                var json = File.ReadAllText(_configPath);
                var config = JsonSerializer.Deserialize<ClickConfig>(json, _jsonOptions);
                if (config != null)
                {
                    ValidateConfig(config);
                    return config;
                }
            }
        }
        catch (Exception ex)
        {
            System.Diagnostics.Debug.WriteLine($"Config load failed: {ex.Message}");
        }

        return new ClickConfig();
    }

    /// <summary>
    /// 保存配置（原子写入：先写临时文件，再替换）
    /// </summary>
    public bool Save(ClickConfig config)
    {
        try
        {
            if (!Directory.Exists(_configDir))
            {
                Directory.CreateDirectory(_configDir);
            }

            ValidateConfig(config);

            var json = JsonSerializer.Serialize(config, _jsonOptions);
            var tempPath = _configPath + ".tmp";

            // 先写入临时文件
            File.WriteAllText(tempPath, json);

            // 原子替换（使用 File.Replace 保证 NTFS 原子性）
            if (File.Exists(_configPath))
            {
                var backupPath = _configPath + ".bak";
                File.Replace(tempPath, _configPath, backupPath);
                // 删除备份文件
                try { File.Delete(backupPath); } catch { }
            }
            else
            {
                File.Move(tempPath, _configPath);
            }

            return true;
        }
        catch (Exception ex)
        {
            System.Diagnostics.Debug.WriteLine($"Config save failed: {ex.Message}");
            return false;
        }
    }

    /// <summary>
    /// 导出配置到指定路径
    /// </summary>
    public bool Export(ClickConfig config, string filePath)
    {
        if (string.IsNullOrWhiteSpace(filePath))
            throw new ArgumentException("文件路径不能为空", nameof(filePath));

        try
        {
            var json = JsonSerializer.Serialize(config, _jsonOptions);
            File.WriteAllText(filePath, json);
            return true;
        }
        catch (Exception ex)
        {
            System.Diagnostics.Debug.WriteLine($"Config export failed: {ex.Message}");
            return false;
        }
    }

    /// <summary>
    /// 从指定路径导入配置
    /// </summary>
    public ClickConfig? Import(string filePath)
    {
        if (string.IsNullOrWhiteSpace(filePath))
            throw new ArgumentException("文件路径不能为空", nameof(filePath));

        try
        {
            if (File.Exists(filePath))
            {
                var json = File.ReadAllText(filePath);
                var config = JsonSerializer.Deserialize<ClickConfig>(json, _jsonOptions);
                if (config != null)
                {
                    ValidateConfig(config);
                    return config;
                }
            }
        }
        catch (Exception ex)
        {
            System.Diagnostics.Debug.WriteLine($"Config import failed: {ex.Message}");
        }

        return null;
    }

    /// <summary>
    /// 验证并修正配置值
    /// </summary>
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

    /// <summary>
    /// 获取配置目录路径
    /// </summary>
    public string GetConfigDirectory() => _configDir;
}
