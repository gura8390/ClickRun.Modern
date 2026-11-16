# 暖态内存归因辅助探针

`font-probe.ps1` 只在自己的独立进程中创建内存 DC 和字体，不创建可见窗口、不操作鼠标键盘、不修改正式应用、配置或系统字体安装。每个分支必须使用新的 64 位 PowerShell 进程。

```powershell
pwsh -NoProfile -File .\font-probe.ps1 -FontFile '完整路径\FZGWKaiS.ttf' -Mode Absent -Dpi 144 -OutFile '新目录\font-absent.json'
pwsh -NoProfile -File .\font-probe.ps1 -FontFile '完整路径\FZGWKaiS.ttf' -Mode Present -Dpi 144 -OutFile '新目录\font-present.json'
```

`Absent` 分支不调用字体注册 API，仅读取源文件表指纹用于比对。`Present` 分支使用 `FR_PRIVATE`，四档字体销毁后移除该进程的私有注册。输出实际 `GetTextFaceW` 字体名称、`GetTextMetricsW`、六张字体表的 SHA-256 与源文件匹配结果。输入 DPI 应取对应目标窗口的 `GetDpiForWindow`，不能混合逻辑像素与物理像素。

只有两分支实际选中名称或字体表指纹不同，才证明本机具备有效的字体有/无对照。若 `Absent` 仍完整匹配源文件，说明同字体在当前环境已经可用，应将该组标为同字体环境，不能称为字体替换试验。缺少字体文件也可能改用别的系统字体并改变系统模块、GDI 和字形缓存；应用总工作集差值只代表完整映射路径的净变化，不能全算作字体文件本身的驻留。

探针读取源TTF和字体表会预热系统缓存，须在应用 A/B 测量前运行，并让后续测量使用 AB/BA 平衡顺序。不要把探针自身的内存用作应用内存。没有调用 trim、强制 GC、COM 卸载、系统字体安装或广播字体更改。

目标窗口 `WM_GETFONT` / `GetObjectW` 可作为逻辑字体信息；GDI 官方文档声明对象句柄归创建进程，跨进程选择 HFONT 不用作实际物理字体证明。这里通过独立探针复现相同字体请求验证环境，不操作目标 DC。

依据：[GetFontData](https://learn.microsoft.com/en-us/windows/win32/api/wingdi/nf-wingdi-getfontdata)、[GetTextFaceW](https://learn.microsoft.com/en-us/windows/win32/api/wingdi/nf-wingdi-gettextfacew)、[AddFontResourceExW](https://learn.microsoft.com/en-us/windows/win32/api/wingdi/nf-wingdi-addfontresourceexw)、[GDI Objects](https://learn.microsoft.com/en-us/windows/win32/sysinfo/gdi-objects)。
