# 暖态内存测量编排

使用 64 位 Windows PowerShell 7。应用进程逐一串行；运行前先正常退出其他连点器。工具不裁剪工作集、不强制 GC、不改正式配置或系统字体。源码下的 `measure.ps1`、`ResidentProfile.cs` 和现有 UIA 工具被直接复用。

## 单轮入口

```powershell
& .\tools\warm-memory\runner\run.ps1 `
  -ExecutablePath '<本项目verification中的候选exe>' `
  -OutputDirectory '<本项目verification中的全新目录>' `
  -Flavor Rust -Scope Full -WarmupScript '<UIA暖机脚本>'
```

`-Scope Common` 只做共同矩阵；`Full` 先做 Common，再做 Full。`-FullWarmupScript` 可单独指定完整矩阵。默认初始参数是左键、500 毫秒、F1、无修饰键、无限次数、关闭声音、关闭最小化到托盘。正常模式仅通过子进程环境重定向 APPDATA/LOCALAPPDATA，清除模拟系统变量，不使用 `--config-dir` 或 `--test-mode`。

`LegacyC` 只允许 Common，必须显式传入 `-ExitScript`，由 C 原版 UIA `WindowPattern.Close` 正常关闭。原 C 的锁定、全局热键或真实点击均不属于本工具入口。

## 暖机和退出脚本协议

脚本以独立 `pwsh -NoProfile -File <脚本> -ContextPath <context.json>` 执行。Context 提供 `Executable`、`ProcessId`、`WindowHandle`、`WindowClass`、`CreationFiletime`、`Flavor`、`Stage`（Common/Full）、隔离路径、`ResultPath`、`WarmupResultPath` 和 `ExitResultPath`。

暖机脚本必须在 `ResultPath` 写 JSON：

```json
{"Passed":true,"ProcessId":123,"CreationFiletime":123456789,"ClientsExited":true}
```

退出脚本写 `ExitResultPath`，相同身份字段及 `Passed`。暖机脚本结束其自身及其创建的全部 UIA 客户端，并通过 UIA 读回实际状态；C Common 在结束前恢复初始参数，不锁定或写 HKLM。Rust 默认退出脚本调用现有 UIA 控件 212。

首个系统快照不创建 UIA 客户端。Common、Full 客户端退出后分别留下原生计数、模块清单和真实驻留页快照。最后矩阵退出后以统一起点采样 20 秒，并在名义 0、30、60 秒点复查模块和驻留页。报告同时记录实际采集起止耗时，处理开销不被当成零。

模块虚拟映像大小不能用作驻留成本。`resident` 文件按真实驻留页区分私有、可共享以及文件/映像映射；OS peak 是截至该阶段的整个进程生命周期高水位。退出阶段不进入测量。外部常驻读屏器及系统字体服务缓存不受本工具控制。

## 字体顺序

`font-series.ps1` 接收 `-FontExecutablePath`、`-FallbackExecutablePath`、`-OutputDirectory` 和 `-WarmupScript`。两个目录下全部非字体文件必须逐字节 hash 相同，默认唯一差异是 B 不含 `assets\FZGWKaiS.ttf`。使用同一 Full 矩阵和参数，串行执行 ABBA/BAAB，共四对。当前进程未正常退出时不启动下一进程。
