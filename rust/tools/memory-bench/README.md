# 内存与界面回归工具

使用 64 位 PowerShell 7。先构建 `tools/uia-tests`，并正常退出正在运行的正式程序，再执行默认单实例的测量。所有输出目录必须是新目录。

- `run.ps1`：全新进程，启动峰值、20 秒稳态、UIA 访问后再 20 秒；`-Tray` 测首次显示前的托盘，`-SkipUiaObserve` 跳过中间访问。默认用独立 APPDATA/LOCALAPPDATA，读取真实系统登录启动状态。
- `warm.ps1`：正常模式，进入设置、返回主窗、UIA 客户端退出后采样 20 秒；不切换设置或启停。
- `deferred-tray-check.ps1`：强制隔离模拟输入，验证隐藏裸窗、先行 UIA 访问、七次模拟单击、首次唤回、设置及托盘。
- `stress.ps1`：强制隔离模拟输入，默认六轮完整 UIA 交互，记录各轮资源计数和末尾 20 秒稳态。
- `special-support.ps1`：专项共用的只读计数器、身份核验及标准入口唤回、UIA 退出。

示例（专项输出必须位于工程的 `verification/memory-optimization` 内）：

```powershell
./tools/memory-bench/run.ps1 -ExecutablePath './verification/memory-optimization/final/连点器 Modern.exe' -OutputDirectory './verification/memory-optimization/new-visible'
./tools/memory-bench/run.ps1 -ExecutablePath './verification/memory-optimization/final/连点器 Modern.exe' -OutputDirectory './verification/memory-optimization/new-tray' -Tray -SkipUiaObserve
./tools/memory-bench/stress.ps1 -ExecutablePath './verification/memory-optimization/final/连点器 Modern.exe' -OutputDirectory './verification/memory-optimization/new-stress'
```

峰值来自系统累计高水位，不把采样最大值称为瞬时峰值。工作集、私有提交、共享驻留三者按各自口径记录。工具不修剪工作集，不强制 GC，不修改优先级或亲和性；退出阶段与采样阶段分开。正常模式没有启停动作；专项只向明确隔离的模拟输入进程操作。
