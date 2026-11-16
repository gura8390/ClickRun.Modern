[CmdletBinding()]
param(
    [Parameter(Mandatory=$true)][string]$SourceIcon,
    [Parameter(Mandatory=$true)][string]$DiagnosticIcon,
    [Parameter(Mandatory=$true)][string]$OutFile
)
$ErrorActionPreference = 'Stop'
if (-not [Environment]::Is64BitProcess) { throw '请使用64位独立PowerShell进程。' }
$path=[IO.Path]::GetFullPath($OutFile)
if (Test-Path -LiteralPath $path) { throw '报告已存在，未覆盖。' }
if (-not (Test-Path -LiteralPath ([IO.Path]::GetDirectoryName($path)) -PathType Container)) { throw '报告父目录不存在。' }
Add-Type -Path (Join-Path $PSScriptRoot 'IconRenderProbe.cs')
$result=[ClickRun.WarmMemoryAttribution.IconRenderProbe]::Compare($SourceIcon,$DiagnosticIcon)
$json=$result|ConvertTo-Json -Depth 8
[IO.File]::WriteAllText($path,$json,[Text.UTF8Encoding]::new($false))
$json
if (-not $result.Passed) { exit 1 }
