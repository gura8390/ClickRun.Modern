# ClickRun Modern · 连点器

当前版本 **3.0.0**，采用 Rust 与 Windows 原生控件，以低内存占用为首要目标。

## 下载

[下载 Windows x64 便携包](https://github.com/gura8390/ClickRun.Modern/releases/download/v3.0.0/ClickRun-Modern-v3.0.0-Windows-x64.zip) · [版本说明](https://github.com/gura8390/ClickRun.Modern/releases/tag/v3.0.0)

解压完整压缩包后运行 `连点器 Modern.exe`。包内含运行库和字体，无需安装 .NET Desktop Runtime。

## 3.0.0 更新

- 使用 Rust 重构，空闲时阻塞等待消息，连点时使用可取消的系统计时器。
- 恢复旧 ClickRun 的窄窗布局、字体与配色，数字输入框居中。
- 固定单击、固定间隔，移除点击模式和随机变化。
- 修复滚动残影、提示展开时的布局和高 DPI 对齐。
- 保留全局热键、命名方案、累计统计、托盘、置顶、音效和登录启动。
- 为原生控件提供中文无障碍名称与真实状态，间隔滑块读写单位为毫秒。

可见空闲状态各采样 20 秒：旧 WPF 工作集约 130.43 MiB、私有提交约 75.48 MiB；Rust 3.0.0 分别约 **23.84 MiB / 3.35 MiB**。两次采样非同时进行，结果只对应这些空闲样本。

## 源码与说明

- [Rust 工程与使用说明](rust/README.md)
- [验证与性能摘要](rust/docs/verification-3.0.0.md)
- [第三方资源说明](rust/THIRD_PARTY_NOTICES.md)
- [旧 WPF 说明](docs/legacy-wpf.md)

Rust 工程位于 `rust/`。仓库根的 `ClickRun.Core`、`ClickRun.UI` 和 `publish` 保留旧 WPF 版本，当前便携版请从上方新版下载入口获取。
