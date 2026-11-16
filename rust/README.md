# 连点器 Modern · Rust 3.0.0

轻量 Windows 鼠标连点器。使用 Rust 和原生 Windows 控件，保留旧 ClickRun 的窄窗布局、方正公文楷体、宣纸白背景及细线分隔，以常驻内存为首要优化目标。

仓库根目录保留 WPF 工程；Rust 实现位于本目录 `rust/`。

[下载 v3.0.0](https://github.com/gura8390/ClickRun.Modern/releases/tag/v3.0.0) · [验证与性能记录](docs/verification-3.0.0.md) · [第三方资源说明](THIRD_PARTY_NOTICES.md)

## 功能

- 当前鼠标位置连点，支持左键、右键和中键。
- 固定单击、固定间隔，范围 1–10000 毫秒；滑块与数字输入框双向联动。
- 无限次数或指定单次点击数，支持命名方案保存、切换和删除。
- 全局启停热键，默认 F6；可修改组合键，注册冲突时保留原组合。
- 显示本次点击、累计次数、点击速度和运行时长。
- 提供登录启动、最小化到托盘、窗口置顶和启停音效设置。
- 中文界面、键盘导航及系统 UI Automation 语义，数字框水平和垂直居中。

3.0.0 删除点击模式与随机变化入口，统一为固定间隔单击。旧配置和方案中的 `mode`、`randomDelay` 字段继续兼容，加载及应用时归零。

## 使用

解压发布包，保留程序、运行库和 `assets` 文件夹后运行“连点器 Modern.exe”。启动程序不会自动开始连点。

1. 选择鼠标按键，设置间隔和次数。
2. 按 F6 或“开始连点”启动，再按一次停止。已有配置保留原热键。
3. 点击“存”保存方案；选择方案即可应用，“删”删除选中方案。
4. 右上角“设置”打开应用设置；托盘菜单可打开主窗、设置、启停或退出。

间隔为目标值，实际速率随系统调度和目标程序而变化。指定 7 次即提交 7 次单击；统计按完整提交的单次输入计数。

Tab / Shift+Tab 移动焦点，空格操作按钮和开关，方向键操作分段选择、下拉框与间隔滑块。间隔滑块向辅助技术提供真实毫秒范围；错误和统计可作为文本读取。小窗口提供纵向滚动，焦点进入屏外控件时自动滚入视野。

## 配置

- 配置：`%APPDATA%\ClickRun\config.json`。
- 上一有效配置：同目录 `config.json.bak`；主文件损坏时留存原件并尝试恢复备份。
- 日志：`%LOCALAPPDATA%\ClickRun\logs\clickrun.log`，按大小轮转。
- 普通设置在变更停止 500 毫秒后保存；运行统计每 5 秒保存检查点，停止、隐藏和退出时立即保存。

`--tray` 表示启动后隐藏到托盘，仍不自动连点。登录启动开关读取系统注册状态。

## 构建

环境：Windows 10/11 x64、Rust MSVC 工具链、Visual Studio C++ 构建工具及 Windows SDK。3.0.0 使用 Rust 1.95 验证。请在已加载 MSVC x64 环境的终端执行：

```powershell
git clone https://github.com/gura8390/ClickRun.Modern.git
cd ClickRun.Modern/rust
cargo test --release --locked
cargo build --release --locked
```

程序位于 `target/release/liandianqi.exe`。开发运行时可从仓库已有资源准备字体：

```powershell
New-Item -ItemType Directory -Force target/release/assets | Out-Null
Copy-Item ../ClickRun.UI/Fonts/FZGWKaiS.ttf target/release/assets/FZGWKaiS.ttf
./target/release/liandianqi.exe
```

`build.rs` 直接生成应用图标、版本信息和 DPI 清单资源。发布包附带 `vcruntime140.dll`，不需要 .NET Desktop Runtime。字体和图标来源见[第三方资源说明](THIRD_PARTY_NOTICES.md)。

## 资源占用

在窗口可见、停止连点的各 20 秒样本中，旧 WPF 平均工作集为 **130.43 MiB**、私有提交量为 **75.48 MiB**；Rust 3.0.0 分别为 **23.84 MiB** 和 **3.35 MiB**。两次采样并非同时进行；Rust 样本的进程 CPU 时间未分辨出增量。

采样口径及对应验证项目见[验证与性能记录](docs/verification-3.0.0.md)。
