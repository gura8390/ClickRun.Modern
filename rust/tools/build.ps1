[CmdletBinding()]
param(
    [ValidateSet('build', 'test', 'check', 'all')][string]$Action = 'build',
    [switch]$DebugBuild,
    [string]$SdkLibraryRoot,
    [string]$VisualStudioPath = 'C:\BuildTools'
)
$ErrorActionPreference = 'Stop'
$projectRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..')).Path
$cargoBin = Join-Path $env:USERPROFILE '.cargo\bin'
$cargo = Join-Path $cargoBin 'cargo.exe'
if (-not (Test-Path -LiteralPath $cargo -PathType Leaf)) { throw '没有找到 Rust Cargo。' }
$vcvars = Join-Path $VisualStudioPath 'VC\Auxiliary\Build\vcvars64.bat'
if (-not (Test-Path -LiteralPath $vcvars -PathType Leaf)) { throw '没有找到 MSVC x64 构建工具。' }
if (-not $SdkLibraryRoot) {
    $localSdk = 'D:\codex\01-桌面工具\WfZiUiBp\verification\work\sdk\x64\c'
    if (Test-Path -LiteralPath (Join-Path $localSdk 'um\x64\kernel32.lib') -PathType Leaf) { $SdkLibraryRoot = $localSdk }
}
$savedPath = $env:PATH
$savedLib = $env:LIB
Push-Location -LiteralPath $projectRoot
try {
    $env:PATH = "$cargoBin;$savedPath"
    if ($SdkLibraryRoot) {
        $sdk = (Resolve-Path -LiteralPath $SdkLibraryRoot).Path
        foreach ($required in @('um\x64\kernel32.lib', 'ucrt\x64\ucrt.lib')) {
            if (-not (Test-Path -LiteralPath (Join-Path $sdk $required) -PathType Leaf)) { throw "SDK 缺少 $required。" }
        }
        $env:LIB = "$sdk\um\x64;$sdk\ucrt\x64;$savedLib"
    }
    $profile = if ($DebugBuild) { '' } else { ' --release' }
    $operations = if ($Action -eq 'all') { @('test', 'build') } else { @($Action) }
    foreach ($operation in $operations) {
        $command = 'call "' + $vcvars + '" >nul && cargo ' + $operation + $profile + ' --offline --locked'
        & $env:ComSpec /d /c $command
        if ($LASTEXITCODE -ne 0) { throw "Rust $operation 失败，退出码 $LASTEXITCODE。" }
    }
} finally {
    $env:PATH = $savedPath
    $env:LIB = $savedLib
    Pop-Location
}
