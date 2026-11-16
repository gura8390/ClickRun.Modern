[CmdletBinding()]
param(
    [Parameter(Mandatory=$true)][string]$FontFile,
    [Parameter(Mandatory=$true)][ValidateSet('Absent','Present')][string]$Mode,
    [ValidateRange(48,768)][int]$Dpi = 144,
    [string]$Face = '方正公文楷体',
    [string]$OutFile
)
$ErrorActionPreference = 'Stop'
if (-not [Environment]::Is64BitProcess) { throw '请用64位PowerShell独立进程运行。' }
# Refuse in-process reuse: each branch must get a fresh GDI/font mapper environment.
if ('ClickRun.WarmMemoryAttribution.FontProbe' -as [type]) { throw '请为每个分支启动新的PowerShell进程。' }
Add-Type -Path (Join-Path $PSScriptRoot 'FontProbe.cs')
$result = [ClickRun.WarmMemoryAttribution.FontProbe]::Run($FontFile, $Mode -eq 'Present', $Dpi, $Face)
$report = [ordered]@{
    SchemaVersion = 1
    Mode = $Mode
    Definitions = [ordered]@{
        Absent = '本进程不注册源文件；源TTF仅以FileStream读取指纹，字体选择使用当前会话可用字体。'
        Present = '本进程AddFontResourceExW(FR_PRIVATE)注册源文件，结束时RemoveFontResourceExW；不安装全局字体。'
        SelectedFace = '本探针自己创建并选择HFONT后GetTextFaceW得到的实际物理字体名称；不使用外部目标HFONT。'
        Signature = 'GetFontData读取name/head/maxp/OS2/hhea/cmap表并SHA256；和源TTF对应原始表逐项比较。'
        Metrics = 'GetTextMetricsW原生读回；RequestedHeight与Rust四档字号和指定DPI完全相同。'
        Interpretation = '只证明此环境下字体映射分支。字体缓存和探针内存不计入应用样本；须先完成探针再进行平衡顺序的应用实验。'
    }
    Probe = $result
}
$json = $report | ConvertTo-Json -Depth 10
if ($OutFile) {
    $path = [IO.Path]::GetFullPath($OutFile)
    if (Test-Path -LiteralPath $path) { throw "报告已存在，未覆盖：$path" }
    if (-not (Test-Path -LiteralPath ([IO.Path]::GetDirectoryName($path)) -PathType Container)) { throw '报告父目录不存在。' }
    [IO.File]::WriteAllText($path, $json, [Text.UTF8Encoding]::new($false))
}
$json
if (-not $result.Passed) { exit 1 }
