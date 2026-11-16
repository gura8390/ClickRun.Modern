[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$ExecutablePath,
    [Parameter(Mandatory)][string]$OutputDirectory,
    [ValidateRange(20,300)][int]$SteadySeconds=20,
    [string]$UiAToolPath=(Join-Path $PSScriptRoot '..\uia-tests\target\release\liandianqi-uia-tests.exe')
)
$ErrorActionPreference='Stop'
Set-StrictMode -Version Latest
. (Join-Path $PSScriptRoot 'special-support.ps1')
$UiAToolPath=(Resolve-Path -LiteralPath $UiAToolPath).Path
$script:MemorySpecialLastCase=$null
$case=$null;$failure=$null;$exitFailure=$null;$exited=$false;$steady=$null
$samples=[Collections.Generic.List[object]]::new()
$started=[DateTimeOffset]::UtcNow
try{
    # 此模式保留真实系统读取，必须先正常关闭其他同产品默认单实例。
    # 子进程仅重定向 APPDATA/LOCALAPPDATA；不启停连点，不切换任何设置。
    $case=New-MemCase $ExecutablePath $OutputDirectory $false 100 0 0 0 'RedirectAppData'
    $samples.Add((Get-MemSnapshot $case 'startup-ready-before-own-uia'))
    $null=Invoke-MemUia $case $UiAToolPath 'invoke' 'settings' @('--id','101')
    $samples.Add((Get-MemSnapshot $case 'settings-client-exited'))
    $null=Invoke-MemUia $case $UiAToolPath 'invoke' 'main' @('--id','100')
    $samples.Add((Get-MemSnapshot $case 'main-client-exited'))
    $null=Invoke-MemUia $case $UiAToolPath 'observe' 'observe'
    $samples.Add((Get-MemSnapshot $case 'observe-client-exited-before-steady'))
    $steadyFile=Join-Path $case.Output 'post-settings-steady.json'
    & (Join-Path $PSScriptRoot '..\measure.ps1') -ProcessId $case.Process.Id -Label '真实系统读取路径设置页往返后稳态' -Seconds $SteadySeconds -OutFile $steadyFile | Out-Null
    Assert-MemIdentity $case
    $steady=Get-Content -LiteralPath $steadyFile -Raw|ConvertFrom-Json
    if(-not $steady.Completed -or $steady.ProcessId -ne $case.Process.Id){throw '设置后稳态未完成或目标身份不符。'}
    $samples.Add((Get-MemSnapshot $case 'post-settings-steady-ended'))
}catch{$failure=$_.Exception.Message}
finally{
    if($null -eq $case){$case=$script:MemorySpecialLastCase}
    if($null -ne $case){try{$exited=Close-MemCase $case $UiAToolPath}catch{$exitFailure=$_.Exception.Message}}
    $report=[ordered]@{SchemaVersion=1;StartedUtc=$started.ToString('o');EndedUtc=[DateTimeOffset]::UtcNow.ToString('o');Passed=($null -eq $failure -and $null -eq $exitFailure);Failure=$failure;ExitFailure=$exitFailure;NormalExit=$exited;Isolation='APPDATA/LOCALAPPDATA重定向；没有 --config-dir/--test-mode，清除系统模拟环境变量。';Actions='UIA 设置101 → 主窗100 → observe → 客户端退出 → 稳态采样 → UIA退出212；无启停动作、无设置切换。';Definitions=[ordered]@{RealSystem='允许生产系统接口读取，设置页读取真实系统状态；不写入自启动设置。具体DLL/COM加载是否发生需另以模块证据判断。';Comparison='应与同一工具、同一功能往返顺序的另一候选比较；冷启动与设置访问后暖态分列。';OsPeaks='每个阶段快照的PeakWorkingSet/PeakPagefileUsage均为进程生命周期高水位，包含此前阶段；退出动作的新增峰值排除。';Client='自身各UIA客户端退出后采样；不控制外部常驻读屏器。不修剪工作集，不强制GC。';NativeStructureBytes=[Runtime.InteropServices.Marshal]::SizeOf([type][ClickRunMemorySpecial.Native+Memory]);PointerBytes=[IntPtr]::Size};Samples=$samples.ToArray();SteadySummary=if($null -ne $steady){$steady.Summary}else{$null}}
    if($null -ne $case){$report.ExecutableSha256=$case.Sha256;$report.ProcessId=$case.Process.Id;$report.CreationFiletime=$case.Creation;[IO.File]::WriteAllText((Join-Path $case.Output 'warm-report.json'),($report|ConvertTo-Json -Depth 16),[Text.UTF8Encoding]::new($false));$case.Process.Dispose()}
}
if($failure){throw $failure};if($exitFailure){throw $exitFailure}
Write-Output '设置页往返后暖态测量完成，已正常退出。'
