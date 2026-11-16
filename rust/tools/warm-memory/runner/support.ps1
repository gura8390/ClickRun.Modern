$script:WarmProject=(Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..\..\..')).Path
$script:WarmVerification=Join-Path $script:WarmProject 'verification'
if(-not('ClickRunWarmRunner.Native' -as [type])){
Add-Type -TypeDefinition @'
using System;using System.Text;using System.Runtime.InteropServices;
namespace ClickRunWarmRunner{public static class Native{
 [StructLayout(LayoutKind.Sequential)]public struct Memory{public uint cb,PageFaultCount;public UIntPtr PeakWorkingSetSize,WorkingSetSize,QuotaPeakPagedPoolUsage,QuotaPagedPoolUsage,QuotaPeakNonPagedPoolUsage,QuotaNonPagedPoolUsage,PagefileUsage,PeakPagefileUsage,PrivateUsage;}
 [StructLayout(LayoutKind.Sequential)]struct FT{public uint Lo,Hi;public ulong Ticks{get{return((ulong)Hi<<32)|Lo;}}}
 [DllImport("psapi.dll",SetLastError=true)]static extern bool GetProcessMemoryInfo(IntPtr p,ref Memory m,uint cb);
 [DllImport("kernel32.dll",SetLastError=true)]static extern bool GetProcessTimes(IntPtr p,out FT c,out FT e,out FT k,out FT u);
 [DllImport("kernel32.dll",SetLastError=true)]public static extern bool GetProcessHandleCount(IntPtr p,out uint n);
 [DllImport("user32.dll")]public static extern uint GetGuiResources(IntPtr p,uint f);
 [DllImport("user32.dll")]public static extern uint GetWindowThreadProcessId(IntPtr h,out uint p);
 [DllImport("user32.dll",CharSet=CharSet.Unicode)]static extern int GetClassName(IntPtr h,StringBuilder s,int n);
 [DllImport("user32.dll")]public static extern bool IsWindowVisible(IntPtr h);
 delegate bool EnumProc(IntPtr h,IntPtr s);
 [DllImport("user32.dll")]static extern bool EnumWindows(EnumProc c,IntPtr s);
 public static string Class(IntPtr h){var s=new StringBuilder(256);GetClassName(h,s,256);return s.ToString();}
 public static IntPtr Find(uint pid,string cls){IntPtr r=IntPtr.Zero;EnumWindows((h,s)=>{uint p;GetWindowThreadProcessId(h,out p);if((pid==0||p==pid)&&Class(h)==cls){r=h;return false;}return true;},IntPtr.Zero);return r;}
 public static Memory Read(IntPtr p){var m=new Memory();m.cb=(uint)Marshal.SizeOf<Memory>();if(!GetProcessMemoryInfo(p,ref m,m.cb))throw new System.ComponentModel.Win32Exception();return m;}
 public static ulong Created(IntPtr p){FT c,e,k,u;if(!GetProcessTimes(p,out c,out e,out k,out u))throw new System.ComponentModel.Win32Exception();return c.Ticks;}
 public static double Cpu(IntPtr p){FT c,e,k,u;if(!GetProcessTimes(p,out c,out e,out k,out u))throw new System.ComponentModel.Win32Exception();return(k.Ticks+u.Ticks)/1e7;}
}}
'@
}
function Get-WarmPath([string]$Path,[switch]$ExistingFile){
    $full=[IO.Path]::GetFullPath($Path);$prefix=$script:WarmVerification.TrimEnd('\')+'\'
    if(-not $full.StartsWith($prefix,[StringComparison]::OrdinalIgnoreCase)){throw '测试exe/输出必须位于本项目verification内。'}
    if($ExistingFile -and -not(Test-Path -LiteralPath $full -PathType Leaf)){throw "文件不存在：$full"}
    $item=$full
    while($item.Length -ge $script:WarmVerification.Length){if((Test-Path -LiteralPath $item) -and ((Get-Item -LiteralPath $item -Force).Attributes -band [IO.FileAttributes]::ReparsePoint)){throw '测试路径不能经过重解析点。'};if($item.Equals($script:WarmVerification,[StringComparison]::OrdinalIgnoreCase)){break};$item=Split-Path -Parent $item}
    $full
}
function Assert-WarmIdentity($Case){
    if($Case.Process.HasExited){throw '本次进程已结束。'}
    if([ClickRunWarmRunner.Native]::Created($Case.Process.Handle) -ne $Case.Creation){throw '进程创建身份不符。'}
    $cim=Get-CimInstance Win32_Process -Filter "ProcessId=$($Case.Process.Id)"
    if(-not $cim -or -not $cim.ExecutablePath.Equals($Case.Exe,[StringComparison]::OrdinalIgnoreCase) -or $cim.CommandLine -match '--config-dir|--test-mode'){throw '正常模式候选进程路径/命令行身份不符。'}
    if($Case.Window -ne [IntPtr]::Zero){$owner=[uint32]0;[void][ClickRunWarmRunner.Native]::GetWindowThreadProcessId($Case.Window,[ref]$owner);if($owner -ne $Case.Process.Id -or [ClickRunWarmRunner.Native]::Class($Case.Window) -ne $Case.Class){throw '窗口身份不符。'}}
}
function Get-WarmNative($Case,[string]$Phase){
    Assert-WarmIdentity $Case;$m=[ClickRunWarmRunner.Native]::Read($Case.Process.Handle);$handles=[uint32]0;[void][ClickRunWarmRunner.Native]::GetProcessHandleCount($Case.Process.Handle,[ref]$handles);$Case.Process.Refresh()
    [ordered]@{Phase=$Phase;Utc=[DateTimeOffset]::UtcNow.ToString('o');WorkingSetBytes=$m.WorkingSetSize.ToUInt64();PrivateCommitBytes=$m.PrivateUsage.ToUInt64();LifetimePeakWorkingSetBytes=$m.PeakWorkingSetSize.ToUInt64();LifetimePeakPrivateCommitBytes=$m.PeakPagefileUsage.ToUInt64();Handles=$handles;Threads=$Case.Process.Threads.Count;GdiObjects=[ClickRunWarmRunner.Native]::GetGuiResources($Case.Process.Handle,0);UserObjects=[ClickRunWarmRunner.Native]::GetGuiResources($Case.Process.Handle,1);CpuSeconds=[ClickRunWarmRunner.Native]::Cpu($Case.Process.Handle);Visible=if($Case.Window -ne [IntPtr]::Zero){[ClickRunWarmRunner.Native]::IsWindowVisible($Case.Window)}else{$false}}
}
function Write-WarmJson([string]$Path,$Value){if(Test-Path -LiteralPath $Path){throw "结果已存在，未覆盖：$Path"};[IO.File]::WriteAllText($Path,($Value|ConvertTo-Json -Depth 20),[Text.UTF8Encoding]::new($false))}
function Invoke-WarmHook($Case,[string]$Script,[string]$Kind,[int]$TimeoutSeconds=120){
    Assert-WarmIdentity $Case
    $info=[Diagnostics.ProcessStartInfo]::new((Get-Process -Id $PID).Path);$info.UseShellExecute=$false;$info.WindowStyle=[Diagnostics.ProcessWindowStyle]::Hidden
    foreach($arg in @('-NoProfile','-File',$Script,'-ContextPath',$Case.ContextPath)){$info.ArgumentList.Add($arg)}
    $info.RedirectStandardOutput=$true;$info.RedirectStandardError=$true
    $child=[Diagnostics.Process]::Start($info);$stdout=$child.StandardOutput.ReadToEndAsync();$stderr=$child.StandardError.ReadToEndAsync()
    try{if(-not $child.WaitForExit($TimeoutSeconds*1000)){throw "$Kind 客户端超时；未强杀任何进程。"};$exitCode=$child.ExitCode;$out=$stdout.GetAwaiter().GetResult();$err=$stderr.GetAwaiter().GetResult();[IO.File]::WriteAllText((Join-Path $Case.Output "$Kind.stdout.txt"),$out);[IO.File]::WriteAllText((Join-Path $Case.Output "$Kind.stderr.txt"),$err);if($exitCode -ne 0){throw "$Kind 客户端返回 $exitCode"}}
    finally{$child.Dispose()}
    $resultPath=Join-Path $Case.Output "$Kind-result.json"
    $result=Get-Content -LiteralPath $resultPath -Raw|ConvertFrom-Json
    if(-not $result.Passed -or $result.ProcessId -ne $Case.Process.Id -or [uint64]$result.CreationFiletime -ne $Case.Creation){throw "$Kind 结果未通过或目标身份不符。"}
    if($Kind -ne 'exit' -and -not $result.ClientsExited){throw '暖机必须结束所有自身UIA客户端后才测量。'}
    $result
}
function Get-WarmEvidence($Case,[string]$Phase){
    Assert-WarmIdentity $Case
    $native=Get-WarmNative $Case $Phase
    $Case.Process.Refresh();$modules=@($Case.Process.Modules|ForEach-Object {[ordered]@{Name=$_.ModuleName;Path=$_.FileName;BaseAddress=('0x{0:X}' -f $_.BaseAddress.ToInt64());VirtualImageBytes=$_.ModuleMemorySize}})
    $residentFile=Join-Path $Case.Output "$Phase-resident.json"
    & (Join-Path $script:WarmProject 'tools\memory-profile\measure-resident.ps1') -ProcessId $Case.Process.Id -OutFile $residentFile | Out-Null
    Assert-WarmIdentity $Case
    $resident=Get-Content -LiteralPath $residentFile -Raw|ConvertFrom-Json
    if($resident.Snapshot.ProcessId -ne $Case.Process.Id){throw '驻留快照PID不符。'}
    $detail=[ordered]@{Native=$native;Modules=$modules;ResidentFile=[IO.Path]::GetFileName($residentFile);ResidentSummary=[ordered]@{ResidentMiB=$resident.Snapshot.ResidentMiB;PrivateResidentMiB=$resident.Snapshot.PrivateResidentMiB;ShareableResidentMiB=$resident.Snapshot.ShareableResidentMiB}}
    Write-WarmJson (Join-Path $Case.Output "$Phase-evidence.json") $detail
    $detail
}
