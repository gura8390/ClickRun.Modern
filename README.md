# ClickRun Modern · 连点器

当前版本 **3.0.1**，采用 Rust 与 Windows 原生控件，以低内存占用为首要目标。

## 下载

[下载 Windows x64 便携包](https://github.com/gura8390/ClickRun.Modern/releases/download/v3.0.1/ClickRun-Modern-v3.0.1-Windows-x64.zip) · [版本说明](https://github.com/gura8390/ClickRun.Modern/releases/tag/v3.0.1)

解压完整压缩包后运行 `连点器 Modern.exe`。包内含运行库和字体，无需安装 .NET Desktop Runtime。

## 3.0.1 更新

- 托盘启动时延迟创建界面、字体及无障碍名称服务，首次显示后完整可用。
- 保留隐藏状态下的热键、启停、限次结束、统计保存和错误反馈。
- 登录启动状态在设置页读取；快捷方式共用缓冲，字符缓冲峰值减少 128 KiB。
- 无障碍库透明延迟加载，中文语义、状态和操作保留。
- 窄窗布局、字体配色、滚动修复与数字居中沿用 3.0.0。

与 Rust 3.0.0 用相同配置重复采样：首次显示前托盘工作集 **17.52 → 11.25 MiB（减少 35.8%）**，该阶段私有提交峰值 **2.53 → 1.87 MiB（减少 26.3%）**。使用设置后的暖态工作集约 23.7 MiB，基本持平；稳态私有提交减少约 122 KiB。每个稳态区间持续 20 秒，记录系统维护的峰值，不修剪工作集。

21 项测试、六轮共 48 个 UIA 交互步骤、首次隐藏启动、设置持久化、滚动、数字居中与托盘回归通过。完整结果见下方测试记录。

## 源码与说明

- [Rust 工程与使用说明](rust/README.md)
- [3.0.1 内存优化与测试记录](rust/docs/verification-3.0.1.md)
- [第三方资源说明](rust/THIRD_PARTY_NOTICES.md)
- [旧 WPF 说明](docs/legacy-wpf.md)

Rust 工程位于 `rust/`。仓库根的 `ClickRun.Core`、`ClickRun.UI` 和 `publish` 保留旧 WPF 版本，当前便携版请从上方新版下载入口获取。
