[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$FontExecutablePath,
    [Parameter(Mandatory)][string]$FallbackExecutablePath,
    [Parameter(Mandatory)][string]$OutputDirectory,
    [Parameter(Mandatory)][string]$WarmupScript,
    [string]$FullWarmupScript,
    [string[]]$FontRelativePaths=@('assets\FZGWKaiS.ttf'),
    [ValidateRange(500,500)][int]$Interval=500,
    [ValidateRange(112,112)][int]$HotKey=112
)
$ErrorActionPreference='Stop';Set-StrictMode -Version Latest
. (Join-Path $PSScriptRoot 'support.ps1')
$fontExe=Get-WarmPath $FontExecutablePath -ExistingFile;$fallbackExe=Get-WarmPath $FallbackExecutablePath -ExistingFile;$out=Get-WarmPath $OutputDirectory
if(Test-Path -LiteralPath $out){throw '序列输出已存在，未覆盖。'}
$rootA=Split-Path -Parent $fontExe;$rootB=Split-Path -Parent $fallbackExe
if($rootA.Equals($rootB,[StringComparison]::OrdinalIgnoreCase)){throw 'A/B必须是独立目录。'}
function Get-FontPayload([string]$Root){
    $list=[ordered]@{}
    foreach($item in Get-ChildItem -LiteralPath $Root -Recurse -Force){if($item.Attributes -band [IO.FileAttributes]::ReparsePoint){throw '字体载荷不能含重解析点。'};if(-not $item.PSIsContainer){$relative=[IO.Path]::GetRelativePath($Root,$item.FullName);$list[$relative]=[ordered]@{Bytes=$item.Length;Sha256=(Get-FileHash -LiteralPath $item.FullName).Hash}}}
    $list
}
$a=Get-FontPayload $rootA;$b=Get-FontPayload $rootB
foreach($font in $FontRelativePaths){if([IO.Path]::IsPathRooted($font) -or $font -match '(^|[\\/])\.\.([\\/]|$)' -or -not $font.StartsWith('assets\',[StringComparison]::OrdinalIgnoreCase)){throw '唯一允许缺少的文件必须是assets内明确字体。'};if(-not $a.Contains($font) -or $b.Contains($font)){throw "A必须有该字体且B必须没有：$font"}}
foreach($key in $a.Keys){if($FontRelativePaths -contains $key){continue};if(-not $b.Contains($key) -or $a[$key].Sha256 -ne $b[$key].Sha256){throw "A/B非字体载荷不同：$key"}}
foreach($key in $b.Keys){if(-not $a.Contains($key)){throw "B有额外载荷：$key"}}
if((Get-FileHash -LiteralPath $fontExe).Hash -ne (Get-FileHash -LiteralPath $fallbackExe).Hash){throw 'A/B应用二进制不一致。'}
[IO.Directory]::CreateDirectory($out)|Out-Null
Write-WarmJson (Join-Path $out 'payload-proof.json') ([ordered]@{A=$a;B=$b;AllowedDifference=$FontRelativePaths;Order=@('A','B','B','A','B','A','A','B');Meaning='ABBA/BAAB，共四个A和四个B；每一进程结束后才启动下一进程。';SystemFontsUnchanged=$true})
$runs=[Collections.Generic.List[object]]::new();$failure=$null
try{
    $index=0
    foreach($variant in @('A','B','B','A','B','A','A','B')){
        $index++;$exe=if($variant -eq 'A'){$fontExe}else{$fallbackExe};$runOut=Join-Path $out ('{0:D2}-{1}' -f $index,$variant)
        $args=@{ExecutablePath=$exe;OutputDirectory=$runOut;Flavor='Rust';WarmupScript=$WarmupScript;Scope='Full';Interval=$Interval;HotKey=$HotKey;HotKeyModifiers=0}
        if($FullWarmupScript){$args.FullWarmupScript=$FullWarmupScript}
        & (Join-Path $PSScriptRoot 'run.ps1') @args
        $result=Get-Content -LiteralPath (Join-Path $runOut 'run-report.json') -Raw|ConvertFrom-Json
        if(-not $result.Passed -or -not $result.NormalExit){throw '当前轮未完整通过并正常退出，不启动下一字体进程。'}
        $runs.Add([ordered]@{Index=$index;Variant=$variant;RelativeOutput=[IO.Path]::GetFileName($runOut);ProcessId=$result.ProcessId;ExecutableSha256=$result.ExecutableSha256;Passed=$result.Passed})
    }
}catch{$failure=$_.Exception.Message}
finally{Write-WarmJson (Join-Path $out 'font-series-report.json') ([ordered]@{SchemaVersion=1;Passed=($null -eq $failure -and $runs.Count -eq 8);Failure=$failure;Runs=$runs.ToArray();Definitions='仅改变私有字体文件；完整暖机顺序与参数一致；进程逐一串行，严禁工作集裁剪；系统服务和文件缓存不被清空，顺序通过ABBA/BAAB平衡。'})}
if($failure){throw $failure}
