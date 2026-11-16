[CmdletBinding()]
param(
    [Parameter(Mandatory=$true)][string]$ExecutablePath,
    [Parameter(Mandatory=$true)][string]$OutputDirectory,
    [ValidateSet('RedirectAppData','ConfigDirTest')][string]$Isolation='RedirectAppData',
    [ValidateRange(20,300)][int]$Seconds=20,
    [ValidateRange(50,1000)][int]$SampleMilliseconds=100,
    [string]$UiAToolPath,
    [switch]$SkipUiaObserve,
    [switch]$Tray
)
$ErrorActionPreference='Stop'
Set-StrictMode -Version Latest
if(-not $IsWindows){throw '此测量工具仅支持 Windows。'}
$exe=(Resolve-Path -LiteralPath $ExecutablePath).Path
if([IO.Path]::GetExtension($exe) -ne '.exe'){throw '需要指定 Windows 可执行文件。'}
$out=[IO.Path]::GetFullPath($OutputDirectory)
if(Test-Path -LiteralPath $out){throw '输出目录已存在，未覆盖既有记录。'}
if(-not $UiAToolPath){$UiAToolPath=Join-Path $PSScriptRoot '..\uia-tests\target\release\liandianqi-uia-tests.exe'}
$UiAToolPath=(Resolve-Path -LiteralPath $UiAToolPath).Path
if(-not('ClickRunMemoryBench.Native' -as [type])){
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
using System.Text;
namespace ClickRunMemoryBench {
 public static class Native {
  [StructLayout(LayoutKind.Sequential)] public struct Memory {
   public uint cb, PageFaultCount;
   public UIntPtr PeakWorkingSetSize, WorkingSetSize, QuotaPeakPagedPoolUsage,
    QuotaPagedPoolUsage, QuotaPeakNonPagedPoolUsage, QuotaNonPagedPoolUsage,
    PagefileUsage, PeakPagefileUsage, PrivateUsage;
  }
  [StructLayout(LayoutKind.Sequential)] public struct FT { public uint Low,High; public ulong Ticks {get{return ((ulong)High<<32)|Low;}} }
  [DllImport("psapi.dll",SetLastError=true)] static extern bool GetProcessMemoryInfo(IntPtr p,ref Memory m,uint size);
  [DllImport("kernel32.dll",SetLastError=true)] static extern bool GetProcessTimes(IntPtr p,out FT c,out FT e,out FT k,out FT u);
  [DllImport("kernel32.dll",SetLastError=true)] public static extern bool GetProcessHandleCount(IntPtr p,out uint count);
  [DllImport("user32.dll")] public static extern uint GetGuiResources(IntPtr p,uint flags);
  delegate bool EnumProc(IntPtr h,IntPtr state);
  [DllImport("user32.dll")] static extern bool EnumWindows(EnumProc cb,IntPtr state);
  [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr h,out uint pid);
  [DllImport("user32.dll",CharSet=CharSet.Unicode)] static extern int GetClassName(IntPtr h,StringBuilder b,int n);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll")] public static extern bool IsIconic(IntPtr h);
  public static Memory Read(IntPtr p) {var m=new Memory();m.cb=(uint)Marshal.SizeOf<Memory>();if(!GetProcessMemoryInfo(p,ref m,m.cb))throw new System.ComponentModel.Win32Exception();return m;}
  public static double CpuSeconds(IntPtr p) {FT c,e,k,u;if(!GetProcessTimes(p,out c,out e,out k,out u))throw new System.ComponentModel.Win32Exception();return (k.Ticks+u.Ticks)/10000000.0;}
  public static IntPtr FindWindow(uint wanted) {IntPtr result=IntPtr.Zero;EnumWindows((h,s)=>{uint pid;GetWindowThreadProcessId(h,out pid);if(pid!=wanted)return true;var n=new StringBuilder(128);GetClassName(h,n,128);if(n.ToString()=="ClickRun.Rust.Main"){result=h;return false;}return true;},IntPtr.Zero);return result;}
 }
}
'@
}
[IO.Directory]::CreateDirectory($out)|Out-Null
$roaming=Join-Path $out 'data\roaming'
$local=Join-Path $out 'data\local'
$fixture=if($Isolation -eq 'RedirectAppData'){Join-Path $roaming 'ClickRun'}else{Join-Path $out 'fixture'}
[IO.Directory]::CreateDirectory($fixture)|Out-Null
[IO.Directory]::CreateDirectory($local)|Out-Null
# 所有候选采用相同测试数据；最后使用 UIA Invoke“退出”，不改变正式安装配置。
$config=[ordered]@{hotKey=122;hotKeyModifiers=7;button=0;interval=100;randomDelay=0;mode=0;clickLimit=0;startWithWindows=$false;minimizeToTray=$false;playSound=$false;alwaysOnTop=$false;totalClicks=0;profiles=@()}
[IO.File]::WriteAllText((Join-Path $fixture 'config.json'),($config|ConvertTo-Json),[Text.UTF8Encoding]::new($false))
$start=[Diagnostics.ProcessStartInfo]::new($exe)
$start.UseShellExecute=$false
$start.WorkingDirectory=[IO.Path]::GetDirectoryName($exe)
$start.WindowStyle=[Diagnostics.ProcessWindowStyle]::Normal
$start.Environment['APPDATA']=$roaming
$start.Environment['LOCALAPPDATA']=$local
foreach($key in @('CLICKRUN_TEST_SYSTEM_DIR','CLICKRUN_TEST_LOG_DIR')){[void]$start.Environment.Remove($key)}
if($Isolation -eq 'ConfigDirTest'){
    $start.ArgumentList.Add('--config-dir');$start.ArgumentList.Add($fixture);$start.ArgumentList.Add('--test-mode')
    $start.Environment['CLICKRUN_TEST_SYSTEM_DIR']=Join-Path $fixture 'system'
    $start.Environment['CLICKRUN_TEST_LOG_DIR']=Join-Path $fixture 'logs'
}
if($Tray){$start.ArgumentList.Add('--tray')}
$samples=[Collections.Generic.List[object]]::new()
$phases=[Collections.Generic.List[object]]::new()
$failure=$null;$window=[IntPtr]::Zero;$normalExit=$false;$target=$null;$handle=[IntPtr]::Zero
$clock=[Diagnostics.Stopwatch]::StartNew()
$sha=(Get-FileHash -LiteralPath $exe -Algorithm SHA256).Hash
$toolSha=(Get-FileHash -LiteralPath $PSCommandPath -Algorithm SHA256).Hash
$script:phase='startup'
function Read-Sample {
    $m=[ClickRunMemoryBench.Native]::Read($handle)
    $handles=[uint32]0;[void][ClickRunMemoryBench.Native]::GetProcessHandleCount($handle,[ref]$handles)
    $row=[ordered]@{Phase=$script:phase;ElapsedMilliseconds=$clock.Elapsed.TotalMilliseconds;Utc=[DateTimeOffset]::UtcNow.ToString('o');WorkingSetBytes=$m.WorkingSetSize.ToUInt64();PrivateUsageBytes=$m.PrivateUsage.ToUInt64();PeakWorkingSetBytes=$m.PeakWorkingSetSize.ToUInt64();PeakPagefileUsageBytes=$m.PeakPagefileUsage.ToUInt64();PageFaultCount=$m.PageFaultCount;CpuSeconds=[ClickRunMemoryBench.Native]::CpuSeconds($handle);HandleCount=$handles;GdiObjectCount=[ClickRunMemoryBench.Native]::GetGuiResources($handle,0);UserObjectCount=[ClickRunMemoryBench.Native]::GetGuiResources($handle,1)}
    $samples.Add($row)
}
function Sample-Phase([string]$Name,[int]$Duration){
    $script:phase=$Name;$first=$samples.Count;Read-Sample;$startMs=$clock.Elapsed.TotalMilliseconds
    while(($clock.Elapsed.TotalMilliseconds-$startMs) -lt ($Duration*1000)){
        if($target.HasExited){throw '目标进程提前退出，未改测其他已有实例。'}
        Start-Sleep -Milliseconds $SampleMilliseconds;Read-Sample
    }
    $rows=@($samples.GetRange($first,$samples.Count-$first));$last=$rows[-1]
    $avg=($rows.WorkingSetBytes|Measure-Object -Average -Minimum -Maximum)
    $private=($rows.PrivateUsageBytes|Measure-Object -Average -Minimum -Maximum)
    $elapsed=($last.ElapsedMilliseconds-$rows[0].ElapsedMilliseconds)/1000.0
    $cpu=$last.CpuSeconds-$rows[0].CpuSeconds
    $summary=[ordered]@{Phase=$Name;SampleCount=$rows.Count;MeasuredSeconds=$elapsed;WorkingSetAverageMiB=$avg.Average/1MB;WorkingSetMinimumMiB=$avg.Minimum/1MB;WorkingSetMaximumMiB=$avg.Maximum/1MB;PrivateUsageAverageMiB=$private.Average/1MB;PrivateUsageMinimumMiB=$private.Minimum/1MB;PrivateUsageMaximumMiB=$private.Maximum/1MB;LifetimePeakWorkingSetMiB=$last.PeakWorkingSetBytes/1MB;LifetimePeakPagefileUsageMiB=$last.PeakPagefileUsageBytes/1MB;CpuSecondsDelta=$cpu;CpuOneCorePercent=$cpu/$elapsed*100;HandleCountLast=$last.HandleCount;GdiObjectCountLast=$last.GdiObjectCount;UserObjectCountLast=$last.UserObjectCount}
    $phases.Add($summary);$summary|ConvertTo-Json -Compress|Write-Output
}
try{
    $clock.Restart();$target=[Diagnostics.Process]::Start($start);$handle=$target.Handle
    Read-Sample # Process.Start 返回后的首个读数；峰值字段是 OS 自进程创建起的高水位。
    [IO.File]::WriteAllText((Join-Path $out 'identity.json'),([ordered]@{ProcessId=$target.Id;StartedUtc=$target.StartTime.ToUniversalTime().ToString('o');Executable=$exe;Sha256=$sha;Isolation=$Isolation;Arguments=@($start.ArgumentList);AppData=$roaming;LocalAppData=$local;Fixture=$fixture;Tray=[bool]$Tray}|ConvertTo-Json -Depth 5),[Text.UTF8Encoding]::new($false))
    Write-Output "新进程 PID=$($target.Id)；记录目录：$out"
    if(-not $target.WaitForInputIdle(15000)){throw '15 秒内未到达 GUI 初始空闲状态。'}
    Read-Sample
    $deadline=$clock.Elapsed.TotalMilliseconds+15000
    while($window -eq [IntPtr]::Zero){
        if($target.HasExited){throw '新进程启动后退出；可能存在同产品的单实例。未接管已有进程。'}
        if($clock.Elapsed.TotalMilliseconds -gt $deadline){throw '15 秒内未找到本进程的原生主窗。'}
        $window=[ClickRunMemoryBench.Native]::FindWindow([uint32]$target.Id)
        Read-Sample;if($window -eq [IntPtr]::Zero){Start-Sleep -Milliseconds 50}
    }
    $phases.Add([ordered]@{Phase='startup-ready';ElapsedMilliseconds=$clock.Elapsed.TotalMilliseconds;Window=$window.ToInt64();Visible=[ClickRunMemoryBench.Native]::IsWindowVisible($window);Iconic=[ClickRunMemoryBench.Native]::IsIconic($window);Snapshot=$samples[-1]})
    Sample-Phase 'steady-before-uia' $Seconds
    if(-not $SkipUiaObserve){
        $script:phase='uia-observe'
        $u=[Diagnostics.ProcessStartInfo]::new($UiAToolPath);$u.UseShellExecute=$false;$u.WindowStyle=[Diagnostics.ProcessWindowStyle]::Hidden
        foreach($arg in @('--pid',"$($target.Id)",'--hwnd',"$($window.ToInt64())",'--action','observe','--output',(Join-Path $out 'uia-observe.json'))){$u.ArgumentList.Add($arg)}
        $worker=[Diagnostics.Process]::Start($u);$timeout=$clock.Elapsed.TotalMilliseconds+30000
        while(-not $worker.HasExited){Read-Sample;Start-Sleep -Milliseconds $SampleMilliseconds;if($clock.Elapsed.TotalMilliseconds -gt $timeout){throw 'UIA observe 工具超时，停止本轮有效测量。'}}
        if($worker.ExitCode -ne 0){throw "UIA observe 返回失败：$($worker.ExitCode)"}
        $worker.Dispose()
        Sample-Phase 'steady-after-uia-client-exit' $Seconds
    }
}catch{$failure=$_.Exception.Message}
finally{
    if($target -and -not $target.HasExited){
        $script:phase='normal-exit'
        # 测量已结束：退出 UIA 激活及关闭动作产生的峰值不进入 Samples/Phases。
        if($window -ne [IntPtr]::Zero){
            if(-not [ClickRunMemoryBench.Native]::IsWindowVisible($window)){
                # 隐藏但未最小化的窗口，UIA Normal 不等于显示；重复正式入口唤回。
                $activateInfo=[Diagnostics.ProcessStartInfo]::new($exe);$activateInfo.UseShellExecute=$false;$activateInfo.WindowStyle=[Diagnostics.ProcessWindowStyle]::Hidden;$activateInfo.WorkingDirectory=$start.WorkingDirectory
                foreach($entry in $start.Environment.GetEnumerator()){$activateInfo.Environment[$entry.Key]=$entry.Value}
                foreach($argument in $start.ArgumentList){if($argument -ne '--tray'){$activateInfo.ArgumentList.Add($argument)}}
                try{
                    $activator=[Diagnostics.Process]::Start($activateInfo)
                    [void]$activator.WaitForExit(5000);$activator.Dispose()
                    $untilVisible=[DateTimeOffset]::UtcNow.AddSeconds(5)
                    while(-not [ClickRunMemoryBench.Native]::IsWindowVisible($window) -and [DateTimeOffset]::UtcNow -lt $untilVisible){Start-Sleep -Milliseconds 50}
                }catch{if(-not $failure){$failure="重复入口唤回未成功：$($_.Exception.Message)"}}
            }
            $exitInfo=[Diagnostics.ProcessStartInfo]::new($UiAToolPath);$exitInfo.UseShellExecute=$false;$exitInfo.WindowStyle=[Diagnostics.ProcessWindowStyle]::Hidden
            foreach($arg in @('--pid',"$($target.Id)",'--hwnd',"$($window.ToInt64())",'--action','invoke','--id','212','--output',(Join-Path $out 'uia-normal-exit.json'))){$exitInfo.ArgumentList.Add($arg)}
            try{
                $closer=[Diagnostics.Process]::Start($exitInfo)
                if(-not $closer.WaitForExit(15000)){if(-not $failure){$failure='UIA 退出工具超时，未强行结束目标应用。'}}
                elseif($closer.ExitCode -ne 0 -and -not $failure){$failure="UIA 退出工具返回失败：$($closer.ExitCode)"}
                $closer.Dispose()
            }catch{if(-not $failure){$failure="UIA 正常退出未成功：$($_.Exception.Message)"}}
        }
        [void]$target.WaitForExit(10000)
        $normalExit=$target.HasExited
        if(-not $normalExit -and -not $failure){$failure='目标未通过 UIA“退出”正常结束，未强行结束；请正常关闭该隔离进程。'}
    }
    $clock.Stop()
    $report=[ordered]@{SchemaVersion=2;ToolScriptSha256=$toolSha;Executable=$exe;ExecutableSha256=$sha;Isolation=$Isolation;Tray=[bool]$Tray;RequestedSteadySeconds=$Seconds;SampleMilliseconds=$SampleMilliseconds;ProcessId=if($target){$target.Id}else{$null};Completed=($null -eq $failure -and $normalExit);Failure=$failure;NormalExit=$normalExit;NormalExitMethod='真实UIA InvokePattern，控件212';ExitExcludedFromMeasuredSamples=$true;TargetLeftRunning=($target -and -not $target.HasExited);NoWorkingSetTrim=$true;NoForcedGc=$true;NoStartStopInputActions=$true;RealSystemReadPath=($Isolation -eq 'RedirectAppData');UiaObserveSkipped=[bool]$SkipUiaObserve;UiaClientLifetime='observe 后客户端退出，再采稳态；不是持续连接的读屏器；退出 UIA 动作在全部测量结束后执行';Definitions=[ordered]@{PeakWorkingSet='PROCESS_MEMORY_COUNTERS_EX.PeakWorkingSetSize：截至当前样本的进程生命周期峰值工作集';PeakPagefileUsage='PROCESS_MEMORY_COUNTERS_EX.PeakPagefileUsage：截至当前样本的生命周期峰值提交量；不是单纯文件分页占用';PrivateUsage='PROCESS_MEMORY_COUNTERS_EX.PrivateUsage：当前私有提交量';Startup='新进程；首读后等待WaitForInputIdle和本进程窗口创建；本工具首阶段不创建UIA客户端；不清理文件缓存';Cpu='单逻辑核满载100%，0增量只表示采样未分辨出消耗'};Phases=$phases.ToArray();Samples=$samples.ToArray()}
    [IO.File]::WriteAllText((Join-Path $out 'memory-bench.json'),($report|ConvertTo-Json -Depth 15),[Text.UTF8Encoding]::new($false))
    if($target){$target.Dispose()}
}
Write-Output "结果：$(Join-Path $out 'memory-bench.json')"
if($failure){throw $failure}
