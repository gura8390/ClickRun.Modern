# ClickRun Modern · Windows 鼠标连点器

使用 C#、.NET 8 和 WPF 编写的鼠标连点工具，支持全局热键、参数方案与系统托盘。A Windows auto-clicker built with .NET 8 and WPF.

当前源码项目版本为 **2.1.0**，以 `ClickRun.UI/ClickRun.UI.csproj` 为准。以下分别提供现有下载入口和源码构建方法。

## 下载与启动

前往 [Windows 下载页](https://github.com/gura8390/ClickRun.Modern/releases/latest) 获取现有 `ClickRun.Modern.exe`。目前保留的 Release tag 是 `Publish`；该历史二进制的内部版本尚未核验。

源码目标为 Windows 和 .NET 8。运行非自包含构建时需要 **.NET 8 Desktop Runtime**。如历史单文件下载缺少运行依赖，可按下文从源码构建完整发布目录。

## 主要功能

- 左键、右键、中键，以及单击、双击、三击模式。
- 点击间隔、随机延迟和指定次数。
- 全局热键启停，支持组合键。
- 保存多套参数方案，自动保存设置。
- 系统托盘、窗口置顶、单实例及开机自启设置。
- 显示点击次数、运行时间与点击速度。

## 使用步骤

1. 选择按键、点击模式、间隔和次数，按需设置随机延迟。
2. 确认界面显示的启停热键，点击开始或使用热键启动。
3. 再次点击停止或使用同一热键结束；离开目标程序前先停止连点。
4. 需要复用参数时保存为方案；托盘和开机启动选项在设置中调整。

## 从源码构建

安装 Windows 上的 .NET 8 SDK，然后执行：

```powershell
git clone https://github.com/gura8390/ClickRun.Modern.git
cd ClickRun.Modern
dotnet build ClickRun.sln -c Release
dotnet run --project ClickRun.UI -c Release
```

发布一个不依赖预装 .NET Runtime 的 win-x64 目录：

```powershell
dotnet publish ClickRun.UI -c Release -r win-x64 --self-contained true -o artifacts/win-x64
```

分发时保留整个输出目录，不要只复制其中的 EXE。

## 代码结构

- `ClickRun.Core/`：模型、点击服务与系统辅助逻辑。
- `ClickRun.UI/`：WPF 窗口、ViewModel、主题和资源。
- `ClickRun.sln`：解决方案。
- `publish/`：保留的历史发布输出。
- [TECHNICAL.md](TECHNICAL.md)：现有实现的技术参考。

`bin/obj` 是本地自动生成的编译缓存，不提交版本库。

## 许可与致谢

[MIT License](LICENSE)。感谢 [CommunityToolkit.Mvvm](https://github.com/CommunityToolkit/dotnet) 和 [Hardcodet.NotifyIcon.Wpf](https://github.com/hardcodet/wpf-notifyicon) 等依赖项目。
