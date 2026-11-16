[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$ExecutablePath,
    [Parameter(Mandatory)][string]$OutputDirectory,
    [Parameter(Mandatory)][ValidateSet('Rust','LegacyC')][string]$Flavor,
    [Parameter(Mandatory)][string]$WarmupScript,
    [ValidateSet('Common','Full')][string]$Scope='Common',
    [string]$FullWarmupScript,
    [ValidateRange(1,1000000)][int]$Interval=500,
    [ValidateRange(1,255)][int]$HotKey=112,
    [ValidateRange(0,15)][int]$HotKeyModifiers=0,
    [string]$ExitScript=(Join-Path $PSScriptRoot 'exit-rust.ps1'),
    [ValidateRange(20,25)][int]$SteadySeconds=20
)
$ErrorActionPreference='Stop';Set-StrictMode -Version Latest
. (Join-Path $PSScriptRoot 'support.ps1')
if(-not $IsWindows -or [IntPtr]::Size -ne 8){throw '需要64位Windows PowerShell 7。'}
$exe=Get-WarmPath $ExecutablePath -ExistingFile;$out=Get-WarmPath $OutputDirectory
$WarmupScript=(Resolve-Path -LiteralPath $WarmupScript).Path;$ExitScript=(Resolve-Path -LiteralPath $ExitScript).Path
if(-not $FullWarmupScript){$FullWarmupScript=$WarmupScript}else{$FullWarmupScript=(Resolve-Path -LiteralPath $FullWarmupScript).Path}
if($Flavor -eq 'LegacyC' -and $Scope -ne 'Common'){throw 'C原版只运行共同暖机矩阵。'}
if($Flavor -eq 'LegacyC' -and $ExitScript.Equals((Join-Path $PSScriptRoot 'exit-rust.ps1'),[StringComparison]::OrdinalIgnoreCase)){throw 'C原版须明确提供安全的UIA WindowPattern.Close退出脚本。'}
if(Test-Path -LiteralPath $out){throw '输出目录已存在，未覆盖。'}
$mutex=[Threading.Mutex]::new($false,'Local\ClickRun.WarmMemory.Runner');$locked=$false;$target=$null;$case=$null;$normalExit=$false;$failure=$null;$exitFailure=$null
$phases=[Collections.Generic.List[object]]::new();$started=[DateTimeOffset]::UtcNow
try{
    $locked=$mutex.WaitOne(0);if(-not $locked){throw '已有暖态测量在运行；必须逐进程串行。'}
    foreach($class in @('ClickRun.Rust.Main','WND')){if([ClickRunWarmRunner.Native]::Find(0,$class) -ne [IntPtr]::Zero){throw '检测到另一连点器主窗，须先正常退出，避免单实例/字体生命周期交叉。'}}
    [IO.Directory]::CreateDirectory($out)|Out-Null
    $roaming=Join-Path $out 'data\roaming';$local=Join-Path $out 'data\local';$fixture=Join-Path $roaming 'ClickRun'
    [IO.Directory]::CreateDirectory($fixture)|Out-Null;[IO.Directory]::CreateDirectory($local)|Out-Null
    if($Flavor -eq 'Rust'){$config=[ordered]@{hotKey=$HotKey;hotKeyModifiers=$HotKeyModifiers;button=0;interval=$Interval;randomDelay=0;mode=0;clickLimit=0;startWithWindows=$false;minimizeToTray=$false;playSound=$false;alwaysOnTop=$false;totalClicks=0;profiles=@()};Write-WarmJson (Join-Path $fixture 'config.json') $config}
    $info=[Diagnostics.ProcessStartInfo]::new($exe);$info.UseShellExecute=$false;$info.WorkingDirectory=[IO.Path]::GetDirectoryName($exe);$info.WindowStyle=[Diagnostics.ProcessWindowStyle]::Normal
    $info.Environment['APPDATA']=$roaming;$info.Environment['LOCALAPPDATA']=$local
    foreach($key in @('CLICKRUN_TEST_SYSTEM_DIR','CLICKRUN_TEST_LOG_DIR')){[void]$info.Environment.Remove($key)}
    $target=[Diagnostics.Process]::Start($info)
    $case=[pscustomobject]@{Exe=$exe;Output=$out;Process=$target;Creation=[ClickRunWarmRunner.Native]::Created($target.Handle);Window=[IntPtr]::Zero;Class=if($Flavor -eq 'Rust'){'ClickRun.Rust.Main'}else{'WND'};ContextPath=Join-Path $out 'context.json'}
    $startup=Get-WarmNative $case 'process-start-first-read'
    if(-not $target.WaitForInputIdle(15000)){throw 'GUI启动未在15秒内达到初始空闲。'}
    $until=[DateTimeOffset]::UtcNow.AddSeconds(15)
    while($case.Window -eq [IntPtr]::Zero){Assert-WarmIdentity $case;$case.Window=[ClickRunWarmRunner.Native]::Find([uint32]$target.Id,$case.Class);if([DateTimeOffset]::UtcNow -gt $until){throw '未找到本次进程主窗。'};if($case.Window -eq [IntPtr]::Zero){Start-Sleep -Milliseconds 50}}
    $context=[ordered]@{Executable=$exe;ProcessId=$target.Id;WindowHandle=$case.Window.ToInt64();WindowClass=$case.Class;CreationFiletime=$case.Creation;Flavor=$Flavor;AppData=$roaming;LocalAppData=$local;Fixture=$fixture;OutputDirectory=$out;Stage='Common';WarmupResultPath=Join-Path $out 'common-result.json';ResultPath=Join-Path $out 'common-result.json';ExitResultPath=Join-Path $out 'exit-result.json';RequestedInterval=$Interval;RequestedHotKey=$HotKey;RequestedHotKeyModifiers=$HotKeyModifiers}
    Write-WarmJson $case.ContextPath $context
    $phases.Add((Get-WarmEvidence $case 'startup-ready-before-own-uia'))
    $warmResult=Invoke-WarmHook $case $WarmupScript 'common'
    $warmClock=[Diagnostics.Stopwatch]::StartNew()
    $captureStart=$warmClock.Elapsed.TotalSeconds;$evidence=Get-WarmEvidence $case 'common-client-exited';$evidence.CaptureStartedAfterWarmupSeconds=$captureStart;$evidence.CaptureEndedAfterWarmupSeconds=$warmClock.Elapsed.TotalSeconds;$phases.Add($evidence)
    if($Scope -eq 'Full'){
        $context.Stage='Full';$context.WarmupResultPath=Join-Path $out 'full-result.json';$context.ResultPath=$context.WarmupResultPath
        $case.ContextPath=Join-Path $out 'full-context.json';Write-WarmJson $case.ContextPath $context
        $warmResult=Invoke-WarmHook $case $FullWarmupScript 'full'
        $warmClock.Restart()
        $captureStart=$warmClock.Elapsed.TotalSeconds;$evidence=Get-WarmEvidence $case 'full-client-exited';$evidence.CaptureStartedAfterWarmupSeconds=$captureStart;$evidence.CaptureEndedAfterWarmupSeconds=$warmClock.Elapsed.TotalSeconds;$phases.Add($evidence)
    }
    $captureStart=$warmClock.Elapsed.TotalSeconds;$evidence=Get-WarmEvidence $case 'uia-exited-0s';$evidence.NominalSecondsAfterWarmup=0;$evidence.CaptureStartedAfterWarmupSeconds=$captureStart;$evidence.CaptureEndedAfterWarmupSeconds=$warmClock.Elapsed.TotalSeconds;$phases.Add($evidence)
    & (Join-Path $script:WarmProject 'tools\measure.ps1') -ProcessId $target.Id -Label "$Flavor 同样全界面暖机客户端退出后稳态" -Seconds $SteadySeconds -OutFile (Join-Path $out 'steady20.json') | Out-Null
    foreach($point in @(30,60)){
        while($warmClock.Elapsed.TotalSeconds -lt $point){Assert-WarmIdentity $case;$remainingMilliseconds=($point-$warmClock.Elapsed.TotalSeconds)*1000;if($remainingMilliseconds -gt 0){Start-Sleep -Milliseconds ([int][Math]::Max(1,[Math]::Min(1000,[Math]::Ceiling($remainingMilliseconds))))}}
        $captureStart=$warmClock.Elapsed.TotalSeconds;$evidence=Get-WarmEvidence $case "uia-exited-$($point)s";$evidence.NominalSecondsAfterWarmup=$point;$evidence.CaptureStartedAfterWarmupSeconds=$captureStart;$evidence.CaptureEndedAfterWarmupSeconds=$warmClock.Elapsed.TotalSeconds;$phases.Add($evidence)
    }
    $steady=Get-Content -LiteralPath (Join-Path $out 'steady20.json') -Raw|ConvertFrom-Json
    if(-not $steady.Completed -or $steady.ProcessId -ne $target.Id){throw '20秒稳态采样未完成或身份不符。'}
}catch{$failure=$_.Exception.Message}
finally{
    if($null -ne $case -and -not $case.Process.HasExited){
        try{Assert-WarmIdentity $case;if(-not(Test-Path -LiteralPath $case.ContextPath)){throw '没有已核验的完整Context供退出脚本；保留进程。'};$null=Invoke-WarmHook $case $ExitScript 'exit' 30;if(-not $target.WaitForExit(10000)){throw 'UIA退出后进程未结束。'};$normalExit=$true}catch{$exitFailure=$_.Exception.Message}
    }elseif($null -ne $case){$normalExit=$true}
    if(Test-Path -LiteralPath $out){$report=[ordered]@{SchemaVersion=1;Passed=($null -eq $failure -and $null -eq $exitFailure -and $normalExit);Failure=$failure;ExitFailure=$exitFailure;NormalExit=$normalExit;StartedUtc=$started.ToString('o');EndedUtc=[DateTimeOffset]::UtcNow.ToString('o');Executable=$exe;ExecutableSha256=(Get-FileHash -LiteralPath $exe).Hash;Flavor=$Flavor;Scope=$Scope;InitialParameters=[ordered]@{Interval=$Interval;HotKey=$HotKey;HotKeyModifiers=$HotKeyModifiers};ProcessId=if($target){$target.Id}else{$null};Definitions=[ordered]@{Isolation='仅重定向APPDATA/LOCALAPPDATA；不带test-mode/config-dir；清除模拟系统环境。C原版HKLM只读约束由暖机脚本落实。';OsPeaks='阶段快照中的OS进程生命周期高水位，包含所有先前阶段；正常退出成本不计入。';NativeStructureBytes=[Runtime.InteropServices.Marshal]::SizeOf([type][ClickRunWarmRunner.Native+Memory]);Modules='枚举目标进程加载模块；VirtualImageBytes是映像虚拟大小，不是实际驻留成本。真实驻留成本见对应resident文件。';Timeline='UIA暖机主机退出后起算，0/30/60秒点记录实际耗时；不控制外部常驻辅助技术、系统字体服务缓存。';NoWorkingSetTrim=$true;NoForcedGc=$true};Phases=$phases.ToArray()};Write-WarmJson (Join-Path $out 'run-report.json') $report}
    if($target){$target.Dispose()};if($locked){$mutex.ReleaseMutex()};$mutex.Dispose()
}
if($failure){throw $failure};if($exitFailure){throw $exitFailure}
Write-Output "暖态测量完成：$out"
