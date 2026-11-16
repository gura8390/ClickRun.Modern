[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$ExecutablePath,
    [Parameter(Mandatory)][string]$OutputDirectory,
    [ValidateRange(1,30)][int]$Rounds=6,
    [ValidateRange(20,300)][int]$SteadySeconds=20,
    [string]$UiAToolPath=(Join-Path $PSScriptRoot '..\uia-tests\target\release\liandianqi-uia-tests.exe')
)
$ErrorActionPreference='Stop'
Set-StrictMode -Version Latest
. (Join-Path $PSScriptRoot 'special-support.ps1')
$UiAToolPath=(Resolve-Path -LiteralPath $UiAToolPath).Path
$script:MemorySpecialLastCase=$null
$case=$null;$failure=$null;$exitFailure=$null;$exited=$false
$roundsDone=[Collections.Generic.List[object]]::new()
$started=[DateTimeOffset]::UtcNow
$initial=$null;$steadyBefore=$null;$steadyAfter=$null;$steady=$null
try{
    $case=New-MemCase $ExecutablePath $OutputDirectory $false 60 7 0 0
    $initial=Get-MemSnapshot $case 'initial-before-own-uia'
    for($round=1;$round -le $Rounds;$round++){
        $before=Get-MemSnapshot $case "round-$round-before"
        $clock=[Diagnostics.Stopwatch]::StartNew()
        $result=Invoke-MemUia $case $UiAToolPath 'suite' "suite-$round" @('--fixture-dir',$case.Fixture,'--single-only','yes')
        $clock.Stop()
        $after=Get-MemSnapshot $case "round-$round-after-client-exit"
        $roundsDone.Add([ordered]@{Round=$round;Passed=$result.通过;ElapsedSeconds=$clock.Elapsed.TotalSeconds;Before=$before;After=$after;Delta=[ordered]@{WorkingSetBytes=([long]$after.WorkingSetBytes-[long]$before.WorkingSetBytes);PrivateUsageBytes=([long]$after.PrivateUsageBytes-[long]$before.PrivateUsageBytes);HandleCount=([long]$after.HandleCount-[long]$before.HandleCount);GdiObjects=([long]$after.GdiObjects-[long]$before.GdiObjects);UserObjects=([long]$after.UserObjects-[long]$before.UserObjects)}})
    }
    $steadyBefore=Get-MemSnapshot $case 'all-rounds-after-before-steady'
    $steadyFile=Join-Path $case.Output 'steady-after-interaction.json'
    $measure=Join-Path $PSScriptRoot '..\measure.ps1'
    & $measure -ProcessId $case.Process.Id -Label '六轮UIA交互客户端退出后稳态' -Seconds $SteadySeconds -OutFile $steadyFile | Out-Null
    Assert-MemIdentity $case
    $steady=Get-Content -LiteralPath $steadyFile -Raw|ConvertFrom-Json
    if(-not $steady.Completed -or $steady.ProcessId -ne $case.Process.Id){throw '交互后稳态报告未完成或目标身份不符。'}
    $steadyAfter=Get-MemSnapshot $case 'all-rounds-after-steady'
}catch{$failure=$_.Exception.Message}
finally{
    if($null -eq $case){$case=$script:MemorySpecialLastCase}
    if($null -ne $case){try{$exited=Close-MemCase $case $UiAToolPath}catch{$exitFailure=$_.Exception.Message}}
    $report=[ordered]@{SchemaVersion=1;StartedUtc=$started.ToString('o');EndedUtc=[DateTimeOffset]::UtcNow.ToString('o');Passed=($null -eq $failure -and $null -eq $exitFailure);Failure=$failure;ExitFailure=$exitFailure;NormalExit=$exited;RequestedRounds=$Rounds;CompletedRounds=$roundsDone.Count;Scope='仅隔离配置模拟输入。每轮复用UIA suite --single-only yes，覆盖无效参数、控件绑定、七次模拟、方案、设置和热键冲突；每轮实际步骤见suite-N.json。';Definitions=[ordered]@{WorkingSet='当前驻留物理内存，含共享页；未裁剪。';PrivateUsage='PROCESS_MEMORY_COUNTERS_EX.PrivateUsage：当前私有提交。';OsPeaks='PeakWorkingSetSize / PeakPagefileUsage：截至该阶段的进程生命周期高水位；正常退出激活阶段不计入。';Client='各轮UIA客户端已退出后采样；其他常驻辅助技术未受本工具控制。';NativeStructureBytes=[Runtime.InteropServices.Marshal]::SizeOf([type][ClickRunMemorySpecial.Native+Memory]);PointerBytes=[IntPtr]::Size};Initial=$initial;Rounds=$roundsDone.ToArray();SteadyBefore=$steadyBefore;SteadyAfter=$steadyAfter;SteadySummary=if($null -ne $steady){$steady.Summary}else{$null}}
    if($null -ne $case){$report.ExecutableSha256=$case.Sha256;$report.ProcessId=$case.Process.Id;$report.CreationFiletime=$case.Creation;[IO.File]::WriteAllText((Join-Path $case.Output 'stress-report.json'),($report|ConvertTo-Json -Depth 16),[Text.UTF8Encoding]::new($false));$case.Process.Dispose()}
}
if($failure){throw $failure};if($exitFailure){throw $exitFailure}
Write-Output '重复UIA交互及交互后稳态测量完成，已正常退出。'
