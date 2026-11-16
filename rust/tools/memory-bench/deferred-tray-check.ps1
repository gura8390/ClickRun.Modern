[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$ExecutablePath,
    [Parameter(Mandatory)][string]$OutputDirectory,
    [string]$UiAToolPath=(Join-Path $PSScriptRoot '..\uia-tests\target\release\liandianqi-uia-tests.exe')
)
$ErrorActionPreference='Stop'
Set-StrictMode -Version Latest
. (Join-Path $PSScriptRoot 'special-support.ps1')
$UiAToolPath=(Resolve-Path -LiteralPath $UiAToolPath).Path
$script:MemorySpecialLastCase=$null
$case=$null;$failure=$null;$exitFailure=$null;$exited=$false
$steps=[Collections.Generic.List[object]]::new()
$started=[DateTimeOffset]::UtcNow
function Add-DeferredStep([string]$Name,$Detail){$steps.Add([ordered]@{Step=$Name;Passed=$true;Utc=[DateTimeOffset]::UtcNow.ToString('o');Detail=$Detail})}
function Get-DeferredNode($Report,[string]$Id){$nodes=@($Report.结果.节点|Where-Object {$_.'自动化ID' -eq $Id});if($nodes.Count -ne 1){throw "UIA节点不唯一或缺失：$Id"};$nodes[0]}
function Get-DeferredText($Node){if($null -ne $Node.'值'){[string]$Node.'值'}else{[string]$Node.'名称'}}
try{
    $case=New-MemCase $ExecutablePath $OutputDirectory $true 15 7 2 30
    $before=Get-MemSnapshot $case 'first-hidden'
    if($before.WindowVisible -or $before.ChildWindowCount -ne 0){throw '首次托盘启动必须隐藏且尚无子控件。'}
    Add-DeferredStep '首次托盘启动隐藏且未创建UI控件' $before
    $null=Invoke-MemUia $case $UiAToolPath 'observe' 'hidden-before-first-ui'
    $observed=Get-MemSnapshot $case 'hidden-after-own-uia-observe'
    if($observed.WindowVisible -or $observed.ChildWindowCount -ne 0){throw 'UIA访问裸窗意外创建或显示UI。'}
    Add-DeferredStep 'UIA先访问隐藏裸窗仍无子控件' $observed
    # 只对已严格核实的 --test-mode 本PID窗口投递全局热键系统消息。
    Assert-MemIdentity $case
    if(-not [ClickRunMemorySpecial.Native]::PostMessage($case.Window,0x0312,[UIntPtr]::new(1),[IntPtr]::new((121 -shl 16) -bor 7))){throw '定向模拟热键消息投递失败。'}
    $end=[DateTimeOffset]::UtcNow.AddSeconds(10);$config=$null
    while($true){
        Assert-MemIdentity $case
        try{$config=Get-Content -LiteralPath (Join-Path $case.Fixture 'config.json') -Raw|ConvertFrom-Json}catch{$config=$null}
        if($null -ne $config -and $config.totalClicks -eq 7 -and $config.mode -eq 0 -and $config.randomDelay -eq 0){break}
        if([DateTimeOffset]::UtcNow -gt $end){throw '隐藏热键运行未在时限内保存七次与旧参数清零。'}
        Start-Sleep -Milliseconds 40
    }
    $after=Get-MemSnapshot $case 'hidden-after-seven'
    if($after.WindowVisible -or $after.ChildWindowCount -ne 0){throw '隐藏热键运行意外创建或显示UI。'}
    Add-DeferredStep '隐藏热键模拟单击七次并清零旧参数' ([ordered]@{TotalClicks=$config.totalClicks;Mode=$config.mode;RandomDelay=$config.randomDelay;Memory=$after;SystemMessage='WM_HOTKEY id=1, F10+CtrlAltShift；目标仅本次模拟输入进程'})
    Activate-MemCase $case
    $shown=Get-MemSnapshot $case 'first-standard-activation'
    if(-not $shown.WindowVisible -or $shown.ChildWindowCount -eq 0){throw '标准二次启动未创建并显示首次UI。'}
    $settings=Invoke-MemUia $case $UiAToolPath 'invoke' 'first-settings' @('--id','101')
    foreach($item in @(@('302',0),@('303',1),@('304',0),@('305',0))){$node=Get-DeferredNode $settings $item[0];if($node.'勾选' -ne $item[1]){throw "设置真实状态不符：$($item[0])"}}
    Add-DeferredStep '标准激活首次创建UI及设置真实状态' $shown
    $main=Invoke-MemUia $case $UiAToolPath 'invoke' 'first-main' @('--id','100')
    $status=Get-DeferredText (Get-DeferredNode $main '401')
    if(-not $status.Contains('已停止') -or -not $status.Contains('隔离测试')){throw '首次显示的运行状态未停止或未标识模拟输入。'}
    foreach($id in @('402','403')){if((Get-DeferredText (Get-DeferredNode $main $id)) -ne '7'){throw "首次显示计数不为七：$id"}}
    Add-DeferredStep '首次主窗读回停止及本次累计七次' ([ordered]@{Status=$status;Session=7;Total=7})
    for($round=1;$round -le 3;$round++){
        Activate-MemCase $case
        $null=Invoke-MemUia $case $UiAToolPath 'invoke' "revisit-$round-settings" @('--id','101')
        $null=Invoke-MemUia $case $UiAToolPath 'invoke' "revisit-$round-main" @('--id','100')
        Add-DeferredStep "重复标准激活及设置返回 $round" (Get-MemSnapshot $case "revisit-$round")
    }
    $null=Invoke-MemUia $case $UiAToolPath 'tray-system' 'tray-system'
    Add-DeferredStep '现有托盘系统与UIA专项' (Get-MemSnapshot $case 'after-tray-system')
}catch{$failure=$_.Exception.Message}
finally{
    if($null -eq $case){$case=$script:MemorySpecialLastCase}
    if($null -ne $case){try{$exited=Close-MemCase $case $UiAToolPath}catch{$exitFailure=$_.Exception.Message}}
    $report=[ordered]@{SchemaVersion=1;StartedUtc=$started.ToString('o');EndedUtc=[DateTimeOffset]::UtcNow.ToString('o');Passed=($null -eq $failure -and $null -eq $exitFailure);Failure=$failure;ExitFailure=$exitFailure;NormalExit=$exited;Scope='隔离 --config-dir --test-mode；定向热键系统专项与UIA语义动作；没有真实鼠标输入';Steps=$steps.ToArray()}
    if($null -ne $case){$report.ExecutableSha256=$case.Sha256;$report.ProcessId=$case.Process.Id;$report.CreationFiletime=$case.Creation;[IO.File]::WriteAllText((Join-Path $case.Output 'deferred-tray-report.json'),($report|ConvertTo-Json -Depth 15),[Text.UTF8Encoding]::new($false));$case.Process.Dispose()}
}
if($failure){throw $failure};if($exitFailure){throw $exitFailure}
Write-Output '托盘延迟创建专项通过，已正常退出。'
