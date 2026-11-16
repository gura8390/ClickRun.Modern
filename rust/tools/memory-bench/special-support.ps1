# 专项测试公共支持：只读本次自建进程，所有应用操作委托已有 UIA 工具。
$script:MemorySpecialProject=(Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..\..')).Path
$script:MemorySpecialArea=[IO.Path]::GetFullPath((Join-Path $script:MemorySpecialProject 'verification\memory-optimization'))
if(-not('ClickRunMemorySpecial.Native' -as [type])){
Add-Type -TypeDefinition @'
using System;using System.Text;using System.Runtime.InteropServices;
namespace ClickRunMemorySpecial {
 public static class Native {
  [StructLayout(LayoutKind.Sequential)]public struct Memory {public uint cb,PageFaultCount;public UIntPtr PeakWorkingSetSize,WorkingSetSize,QuotaPeakPagedPoolUsage,QuotaPagedPoolUsage,QuotaPeakNonPagedPoolUsage,QuotaNonPagedPoolUsage,PagefileUsage,PeakPagefileUsage,PrivateUsage;}
  [StructLayout(LayoutKind.Sequential)]public struct FT {public uint Low,High;public ulong Ticks{get{return((ulong)High<<32)|Low;}}}
  [DllImport("psapi.dll",SetLastError=true)]static extern bool GetProcessMemoryInfo(IntPtr p,ref Memory m,uint size);
  [DllImport("kernel32.dll",SetLastError=true)]static extern bool GetProcessTimes(IntPtr p,out FT c,out FT e,out FT k,out FT u);
  [DllImport("kernel32.dll",SetLastError=true)]public static extern bool GetProcessHandleCount(IntPtr p,out uint n);
  [DllImport("user32.dll")]public static extern uint GetGuiResources(IntPtr p,uint flags);
  [DllImport("user32.dll")]public static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll")]public static extern bool IsIconic(IntPtr h);
  [DllImport("user32.dll")]public static extern uint GetWindowThreadProcessId(IntPtr h,out uint pid);
  [DllImport("user32.dll",CharSet=CharSet.Unicode)]static extern int GetClassName(IntPtr h,StringBuilder s,int n);
  [DllImport("user32.dll",SetLastError=true)]public static extern bool PostMessage(IntPtr h,uint m,UIntPtr w,IntPtr l);
  delegate bool EnumProc(IntPtr h,IntPtr state);
  [DllImport("user32.dll")]static extern bool EnumWindows(EnumProc p,IntPtr state);
  [DllImport("user32.dll")]static extern bool EnumChildWindows(IntPtr h,EnumProc p,IntPtr state);
  [DllImport("shell32.dll",CharSet=CharSet.Unicode,SetLastError=true)]static extern IntPtr CommandLineToArgvW(string s,out int n);
  [DllImport("kernel32.dll")]static extern IntPtr LocalFree(IntPtr p);
  public static string[] Args(string s){int n;IntPtr a=CommandLineToArgvW(s,out n);if(a==IntPtr.Zero)throw new System.ComponentModel.Win32Exception();try{string[] r=new string[n];for(int i=0;i<n;i++)r[i]=Marshal.PtrToStringUni(Marshal.ReadIntPtr(a,i*IntPtr.Size));return r;}finally{LocalFree(a);}}
  public static Memory Read(IntPtr p){var m=new Memory();m.cb=(uint)Marshal.SizeOf<Memory>();if(!GetProcessMemoryInfo(p,ref m,m.cb))throw new System.ComponentModel.Win32Exception();return m;}
  public static ulong Created(IntPtr p){FT c,e,k,u;if(!GetProcessTimes(p,out c,out e,out k,out u))throw new System.ComponentModel.Win32Exception();return c.Ticks;}
  public static double Cpu(IntPtr p){FT c,e,k,u;if(!GetProcessTimes(p,out c,out e,out k,out u))throw new System.ComponentModel.Win32Exception();return(k.Ticks+u.Ticks)/10000000.0;}
  public static bool OwnWindow(IntPtr h,uint wanted){uint p;GetWindowThreadProcessId(h,out p);if(p!=wanted)return false;var b=new StringBuilder(128);GetClassName(h,b,128);return b.ToString()=="ClickRun.Rust.Main";}
  public static IntPtr Find(uint wanted){IntPtr r=IntPtr.Zero;EnumWindows((h,s)=>{if(OwnWindow(h,wanted)){r=h;return false;}return true;},IntPtr.Zero);return r;}
  public static int Children(IntPtr h){int n=0;EnumChildWindows(h,(w,s)=>{n++;return true;},IntPtr.Zero);return n;}
 }
}
'@
}
function Assert-MemPath([string]$Path,[switch]$MustExist){
    $full=[IO.Path]::GetFullPath($Path)
    $prefix=$script:MemorySpecialArea.TrimEnd('\')+'\'
    if(-not $full.StartsWith($prefix,[StringComparison]::OrdinalIgnoreCase)){throw '路径必须位于本项目 verification/memory-optimization 下。'}
    if($MustExist -and -not(Test-Path -LiteralPath $full -PathType Leaf)){throw "文件不存在：$full"}
    $parent=Split-Path -Parent $full
    while($parent -and $parent.Length -ge $script:MemorySpecialArea.Length){
        if(Test-Path -LiteralPath $parent){if((Get-Item -LiteralPath $parent -Force).Attributes -band [IO.FileAttributes]::ReparsePoint){throw '专项路径不能经过重解析目录。'}}
        if($parent.Equals($script:MemorySpecialArea,[StringComparison]::OrdinalIgnoreCase)){break};$parent=Split-Path -Parent $parent
    }
    if((Test-Path -LiteralPath $full) -and ((Get-Item -LiteralPath $full -Force).Attributes -band [IO.FileAttributes]::ReparsePoint)){throw '专项目标不能是重解析点。'}
    $full
}
function New-MemCase([string]$Executable,[string]$Output,[bool]$Tray,[int]$Interval=15,[int]$Limit=7,[int]$Mode=2,[int]$Random=30,[ValidateSet('ConfigDirTest','RedirectAppData')][string]$Isolation='ConfigDirTest'){
    $exe=Assert-MemPath $Executable -MustExist;$out=Assert-MemPath $Output
    if([IO.Path]::GetExtension($exe) -ne '.exe'){throw '候选必须为 exe。'}
    if(Test-Path -LiteralPath $out){throw '专项输出目录已存在，未覆盖。'}
    [IO.Directory]::CreateDirectory($out)|Out-Null
    $fixture=if($Isolation -eq 'RedirectAppData'){Join-Path $out 'roaming\ClickRun'}else{Join-Path $out 'fixture'};[IO.Directory]::CreateDirectory($fixture)|Out-Null
    $cfg=[ordered]@{hotKey=if($Isolation -eq 'RedirectAppData'){122}else{121};hotKeyModifiers=7;button=0;interval=$Interval;randomDelay=$Random;mode=$Mode;clickLimit=$Limit;startWithWindows=$false;minimizeToTray=$Tray;playSound=$false;alwaysOnTop=$false;totalClicks=0;profiles=@()}
    [IO.File]::WriteAllText((Join-Path $fixture 'config.json'),($cfg|ConvertTo-Json),[Text.UTF8Encoding]::new($false))
    $info=[Diagnostics.ProcessStartInfo]::new($exe);$info.UseShellExecute=$false;$info.WindowStyle=[Diagnostics.ProcessWindowStyle]::Normal;$info.WorkingDirectory=[IO.Path]::GetDirectoryName($exe)
    if($Isolation -eq 'ConfigDirTest'){foreach($arg in @('--config-dir',$fixture,'--test-mode')){$info.ArgumentList.Add($arg)}}
    if($Tray){$info.ArgumentList.Add('--tray')}
    $info.Environment['APPDATA']=Join-Path $out 'roaming';$info.Environment['LOCALAPPDATA']=Join-Path $out 'local'
    if($Isolation -eq 'ConfigDirTest'){$info.Environment['CLICKRUN_TEST_SYSTEM_DIR']=Join-Path $fixture 'system';$info.Environment['CLICKRUN_TEST_LOG_DIR']=Join-Path $fixture 'logs'}else{foreach($key in @('CLICKRUN_TEST_SYSTEM_DIR','CLICKRUN_TEST_LOG_DIR')){[void]$info.Environment.Remove($key)}}
    $process=[Diagnostics.Process]::Start($info)
    $case=[pscustomobject]@{Exe=$exe;Sha256=(Get-FileHash -LiteralPath $exe).Hash;Output=$out;Fixture=$fixture;StartInfo=$info;TestMode=($Isolation -eq 'ConfigDirTest');Process=$process;Creation=[ClickRunMemorySpecial.Native]::Created($process.Handle);Window=[IntPtr]::Zero}
    $script:MemorySpecialLastCase=$case
    if(-not $process.WaitForInputIdle(15000)){throw '候选未达到初始输入空闲。'}
    $end=[DateTimeOffset]::UtcNow.AddSeconds(15)
    while($case.Window -eq [IntPtr]::Zero){if($process.HasExited){throw '新候选提前退出，未接管其他实例。'};$case.Window=[ClickRunMemorySpecial.Native]::Find([uint32]$process.Id);if([DateTimeOffset]::UtcNow -gt $end){throw '未找到本PID原生主窗。'};if($case.Window -eq [IntPtr]::Zero){Start-Sleep -Milliseconds 50}}
    Assert-MemIdentity $case
    $case
}
function Assert-MemIdentity($Case){
    if($Case.Process.HasExited){throw '本次测试进程已结束。'}
    if([ClickRunMemorySpecial.Native]::Created($Case.Process.Handle) -ne $Case.Creation){throw '进程创建身份变化。'}
    $p=Get-CimInstance Win32_Process -Filter "ProcessId=$($Case.Process.Id)"
    if(-not $p -or -not $p.ExecutablePath.Equals($Case.Exe,[StringComparison]::OrdinalIgnoreCase)){throw '候选路径身份不符。'}
    $argv=[ClickRunMemorySpecial.Native]::Args($p.CommandLine)
    $expected=@($Case.Exe)+@($Case.StartInfo.ArgumentList)
    if($argv.Count -ne $expected.Count){throw '测试命令行参数数量不符。'}
    for($i=0;$i -lt $expected.Count;$i++){if(-not $argv[$i].Equals($expected[$i],[StringComparison]::OrdinalIgnoreCase)){throw '测试命令行参数身份不符。'}}
    if($Case.TestMode){if($argv -notcontains '--test-mode' -or $argv -notcontains '--config-dir'){throw '必须是明确隔离且模拟输入的进程。'}}else{if($argv -contains '--test-mode' -or $argv -contains '--config-dir'){throw '生产系统接口暖态不得带模拟参数。'}}
    if($Case.Window -ne [IntPtr]::Zero -and -not [ClickRunMemorySpecial.Native]::OwnWindow($Case.Window,[uint32]$Case.Process.Id)){throw '窗口不再属于本PID/原生类。'}
}
function Get-MemSnapshot($Case,[string]$Phase){
    Assert-MemIdentity $Case;$m=[ClickRunMemorySpecial.Native]::Read($Case.Process.Handle);$n=[uint32]0;[void][ClickRunMemorySpecial.Native]::GetProcessHandleCount($Case.Process.Handle,[ref]$n)
    [ordered]@{Phase=$Phase;Utc=[DateTimeOffset]::UtcNow.ToString('o');WorkingSetBytes=$m.WorkingSetSize.ToUInt64();PrivateUsageBytes=$m.PrivateUsage.ToUInt64();PeakWorkingSetBytes=$m.PeakWorkingSetSize.ToUInt64();PeakPagefileUsageBytes=$m.PeakPagefileUsage.ToUInt64();HandleCount=$n;GdiObjects=[ClickRunMemorySpecial.Native]::GetGuiResources($Case.Process.Handle,0);UserObjects=[ClickRunMemorySpecial.Native]::GetGuiResources($Case.Process.Handle,1);CpuSeconds=[ClickRunMemorySpecial.Native]::Cpu($Case.Process.Handle);WindowVisible=[ClickRunMemorySpecial.Native]::IsWindowVisible($Case.Window);ChildWindowCount=[ClickRunMemorySpecial.Native]::Children($Case.Window)}
}
function Activate-MemCase($Case){
    Assert-MemIdentity $Case
    $info=[Diagnostics.ProcessStartInfo]::new($Case.Exe);$info.UseShellExecute=$false;$info.WindowStyle=[Diagnostics.ProcessWindowStyle]::Hidden;$info.WorkingDirectory=$Case.StartInfo.WorkingDirectory
    foreach($entry in $Case.StartInfo.Environment.GetEnumerator()){$info.Environment[$entry.Key]=$entry.Value}
    foreach($arg in $Case.StartInfo.ArgumentList){if($arg -ne '--tray'){$info.ArgumentList.Add($arg)}}
    $again=[Diagnostics.Process]::Start($info);try{if(-not $again.WaitForExit(10000)){throw '二次入口未按单实例退出。'};if($again.ExitCode -ne 0){throw '二次入口返回失败。'}}finally{$again.Dispose()}
    $end=[DateTimeOffset]::UtcNow.AddSeconds(5);while(-not [ClickRunMemorySpecial.Native]::IsWindowVisible($Case.Window)){Assert-MemIdentity $Case;if([DateTimeOffset]::UtcNow -gt $end){throw '标准二次入口未显示原窗口。'};Start-Sleep -Milliseconds 50}
}
function Invoke-MemUia($Case,[string]$Tool,[string]$Action,[string]$Name,[string[]]$Extra=@()){
    Assert-MemIdentity $Case;$out=Join-Path $Case.Output "$Name.json"
    $info=[Diagnostics.ProcessStartInfo]::new($Tool);$info.UseShellExecute=$false;$info.WindowStyle=[Diagnostics.ProcessWindowStyle]::Hidden
    foreach($arg in (@('--pid',"$($Case.Process.Id)",'--hwnd',"$($Case.Window.ToInt64())",'--action',$Action,'--output',$out)+$Extra)){$info.ArgumentList.Add($arg)}
    $worker=[Diagnostics.Process]::Start($info);try{if(-not $worker.WaitForExit(120000)){throw "UIA动作超时：$Action（未强杀目标）"};if($worker.ExitCode -ne 0){throw "UIA动作失败：$Action；报告 $out"}}finally{$worker.Dispose()}
    $result=Get-Content -LiteralPath $out -Raw|ConvertFrom-Json;if(-not $result.通过 -or $result.进程 -ne $Case.Process.Id){throw 'UIA报告身份或结果不符。'}
    $result
}
function Close-MemCase($Case,[string]$Tool){
    if($Case.Process.HasExited){return $true}
    Assert-MemIdentity $Case
    if($Case.Window -eq [IntPtr]::Zero){$Case.Window=[ClickRunMemorySpecial.Native]::Find([uint32]$Case.Process.Id);if($Case.Window -eq [IntPtr]::Zero){throw '无自建主窗可供正常退出，保留进程。'}}
    if(-not [ClickRunMemorySpecial.Native]::IsWindowVisible($Case.Window)){Activate-MemCase $Case}
    $null=Invoke-MemUia $Case $Tool 'invoke' 'normal-exit' @('--id','212')
    if(-not $Case.Process.WaitForExit(10000)){throw 'UIA退出后进程未结束，未强杀。'}
    $true
}
