# ClickRun Modern

> 现代化的鼠标连点器，采用杂志风设计，基于 .NET 8 + WPF 构建

![Version](https://img.shields.io/badge/version-v2.0-blue)
![Platform](https://img.shields.io/badge/platform-Windows%2010%2F11-lightgrey)
![.NET](https://img.shields.io/badge/.NET-8.0-purple)
![License](https://img.shields.io/badge/license-MIT-green)

## ✨ 特性

### 核心功能
- 🖱️ **多按键支持** — 左键、右键、中键
- ⚡ **多种点击模式** — 单击、双击、三击
- ⏱️ **精确间隔控制** — 1ms 到 10000ms，滑块调节
- 🎲 **随机延迟** — 0% 到 50%，模拟人工操作
- 🔢 **次数限制** — 无限模式或指定次数
- ⌨️ **全局热键** — F1-F12 任意键，一键启停

### 系统集成
- 📌 **系统托盘** — 最小化到托盘，后台运行
- 🚀 **开机自启** — 双保险策略（注册表 + 启动文件夹）
- 📌 **窗口置顶** — 始终显示在最上层
- 🔒 **单实例保护** — 防止重复启动

### 界面设计
- 📖 **杂志风 UI** — 方正公文楷体标题，简洁优雅
- ⚙️ **独立设置页面** — 齿轮图标进入，清晰分类
- 📊 **实时统计** — 已点击、运行时长、CPS、总点击

## 📸 界面预览

```
┌─────────────────────────────────────────────┐
│  ClickRun                           [⚙]    │
│  鼠标连点器                                  │
├─────────────────────────────────────────────┤
│  热键 [F6] [修改]     模式 [单击][双击][三击] │
├─────────────────────────────────────────────┤
│  按键   [左键] [右键] [中键]                 │
│  间隔   ═══════════●═══════  100 ms         │
│  随机   ═══════════════●═══  ± 0 %          │
│  次数   [无限] [指定] [____] 次              │
├─────────────────────────────────────────────┤
│          [▶  开始连点]                       │
├─────────────────────────────────────────────┤
│  已点击        运行时长                      │
│  1,234         00:02:15                     │
│  点击速度      总点击次数                    │
│  12.3 次/秒    5,678                        │
└─────────────────────────────────────────────┘
```

## 🚀 快速开始

### 系统要求

- Windows 10/11 (x64)
- .NET 8.0 Desktop Runtime

### 下载安装

1. 从 [Releases](https://github.com/gura8390/ClickRun.Modern/releases) 下载最新版本
2. 解压到任意目录
3. 运行 `ClickRun.Modern.exe`

### 从源码构建

```bash
# 克隆仓库
git clone https://github.com/gura8390/ClickRun.Modern.git
cd clickrun/ClickRun.Modern

# 构建项目
dotnet build

# 运行
dotnet run --project ClickRun.UI

# 发布
dotnet publish ClickRun.UI -c Release -r win-x64 --self-contained false -o publish
```

## 📖 使用说明

### 基本操作

1. **设置参数** — 选择鼠标按键、点击模式、调节间隔
2. **开始连点** — 点击"开始连点"按钮或按下热键（默认 F6）
3. **停止连点** — 再次按下热键或点击"停止"按钮

### 修改热键

1. 点击主界面热键旁的"修改"按钮
2. 在弹出窗口中选择 F1-F12 任意键
3. 热键立即生效

### 设置页面

点击右上角齿轮图标进入设置：

| 设置项 | 说明 |
|--------|------|
| 开机自启动 | Windows 启动时自动运行 |
| 最小化到托盘 | 关闭窗口时隐藏到系统托盘 |
| 窗口置顶 | 窗口始终显示在最上层 |
| 播放音效 | 开始/停止时播放提示音 |

### 系统托盘

开启"最小化到托盘"后：
- 关闭窗口 → 隐藏到托盘
- 双击托盘图标 → 切换连点状态
- 右键托盘图标 → 显示主窗口 / 退出

## ⚙️ 配置文件

配置自动保存在：`%AppData%\ClickRun\config.json`

```json
{
  "hotKey": 117,
  "hotKeyModifiers": 0,
  "button": 0,
  "interval": 100,
  "randomDelay": 0,
  "mode": 0,
  "clickLimit": 0,
  "startWithWindows": false,
  "minimizeToTray": true,
  "playSound": false,
  "alwaysOnTop": false,
  "profiles": []
}
```

## 🛠️ 技术栈

| 技术 | 用途 |
|------|------|
| .NET 8.0 | 运行时框架 |
| WPF | UI 框架 |
| MVVM | 架构模式 |
| P/Invoke | Win32 API 调用 |
| COM Interop | 创建快捷方式 |
| System.Text.Json | 配置序列化 |

## 📁 项目结构

```
ClickRun.Modern/
├── ClickRun.sln                    # 解决方案
├── ClickRun.Core/                  # 核心逻辑层
│   ├── Models/                     # 数据模型
│   ├── Services/                   # 业务服务
│   └── Helpers/                    # 工具类
├── ClickRun.UI/                    # UI 层
│   ├── Views/                      # 窗口
│   ├── ViewModels/                 # 视图模型
│   ├── Themes/                     # 主题样式
│   ├── Fonts/                      # 字体资源
│   └── Resources/                  # 图标资源
└── publish/                        # 发布输出
```

## 📄 许可证

MIT License

## 🙏 致谢

- [Hardcodet.NotifyIcon.Wpf](https://github.com/hardcodet/wpf-notifyicon) — 系统托盘支持
- [CommunityToolkit.Mvvm](https://github.com/CommunityToolkit/dotnet) — MVVM 工具包

---

**ClickRun Modern** — 简洁、高效、优雅的鼠标连点器
