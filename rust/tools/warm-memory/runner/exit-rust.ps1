param([Parameter(Mandatory)][string]$ContextPath)
$ErrorActionPreference='Stop';Set-StrictMode -Version Latest
$context=Get-Content -LiteralPath $ContextPath -Raw|ConvertFrom-Json
if($context.Flavor -ne 'Rust'){throw '默认退出只适用于Rust。'}
. (Join-Path $PSScriptRoot 'support.ps1')
$target=Get-Process -Id $context.ProcessId
try{
    $case=[pscustomobject]@{Exe=$context.Executable;Process=$target;Creation=[uint64]$context.CreationFiletime;Window=[IntPtr]::new([long]$context.WindowHandle);Class=$context.WindowClass}
    Assert-WarmIdentity $case
    $uia=Join-Path $script:WarmProject 'tools\uia-tests\target\release\liandianqi-uia-tests.exe'
    & $uia --pid $context.ProcessId --hwnd $context.WindowHandle --action invoke --id 212 --output (Join-Path $context.OutputDirectory 'exit-uia.json') | Out-Null
    if($LASTEXITCODE -ne 0){throw 'UIA正常退出失败。'}
    $result=Get-Content -LiteralPath (Join-Path $context.OutputDirectory 'exit-uia.json') -Raw|ConvertFrom-Json
    if(-not $result.通过 -or $result.进程 -ne $context.ProcessId){throw 'UIA退出结果身份不符。'}
    Write-WarmJson $context.ExitResultPath ([ordered]@{Passed=$true;ProcessId=$context.ProcessId;CreationFiletime=$context.CreationFiletime;Method='UIA Invoke 212'})
}finally{$target.Dispose()}
