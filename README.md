# ClickRun Modern · 连点器

当前版本 **3.0.2**，采用 Rust 与 Windows 原生控件，以低内存占用为首要目标。

## 下载

[下载 Windows x64 便携包](https://github.com/gura8390/ClickRun.Modern/releases/download/v3.0.2/ClickRun-Modern-v3.0.2-Windows-x64.zip) · [版本说明](https://github.com/gura8390/ClickRun.Modern/releases/tag/v3.0.2)

解压完整压缩包后运行 `连点器 Modern.exe`。包内含运行库和字体，无需安装 .NET Desktop Runtime。

## 3.0.2 更新

- 图标六种尺寸由 PNG 无损改为原生 DIB 编码，逐像素、透明度及系统渲染均一致。
- 避免加载 IconCodecService、WindowsCodecs 两个图标编解码模块。
- 字体、窄窗布局、参数、方案、热键、托盘、登录启动和无障碍功能保留。

完整界面暖机后，3.0.1→3.0.2→3.0.2→3.0.1 平衡复测中，私有提交均值 **3.4938→3.3589 MiB**，两对均下降，平均约省 **138 KiB**。整体工作集差值方向不一致，不宣传稳定净降。

本轮另完成原 C 1.0 与 Rust 3.0.1 的共同操作对照、12轮严格字体实验，以及 UIA、Shell、GDI 实际驻留归因。原 C 在共同操作下仍更省内存；更换字体未带来收益，继续保留方正公文楷体。详见测试记录。

21项测试、完整UIA暖机、滚动、数字居中和托盘回归通过。

## 源码与说明

- [Rust 工程与使用说明](rust/README.md)
- [3.0.2 内存优化与测试记录](rust/docs/verification-3.0.2.md)
- [第三方资源说明](rust/THIRD_PARTY_NOTICES.md)
- [旧 WPF 说明](docs/legacy-wpf.md)

Rust 工程位于 `rust/`。仓库根的 `ClickRun.Core`、`ClickRun.UI` 和 `publish` 保留旧 WPF 版本，当前便携版请从上方新版下载入口获取。
