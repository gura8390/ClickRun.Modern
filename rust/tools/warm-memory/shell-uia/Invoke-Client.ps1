[CmdletBinding()]
param(
    [Parameter(Mandatory=$true)][string]$ContextPath,
    [Parameter(Mandatory=$true)][ValidateSet('probe','common','full','settings','exit')][string]$Action
)
$ErrorActionPreference='Stop'
$context=Get-Content -LiteralPath $ContextPath -Raw | ConvertFrom-Json
$executableHash=(Get-FileHash -LiteralPath $context.Executable -Algorithm SHA256).Hash
if($context.Flavor -eq 'LegacyC' -and $executableHash -ne 'C790B9592D8D9A405796EC38D82BC7A571746A9D4A90774A3778D9A9004C6DCA'){
    throw '原版C可执行文件不符合官方1.0 x64发布资产哈希。'
}
$client=Join-Path $PSScriptRoot 'target-warm\release\clickrun-common-warm.exe'
if(-not(Test-Path -LiteralPath $client -PathType Leaf)){throw '共同暖机UIA客户端尚未构建。'}
$output=& $client --context $ContextPath --action $Action
$clientExit=$LASTEXITCODE
$json=$output -join [Environment]::NewLine
$report=$json | ConvertFrom-Json
if($clientExit -ne 0 -or -not $report.Passed){throw "暖机客户端失败：$json"}
if([uint64]$report.CreationFiletime -ne [uint64]$context.CreationFiletime -or [int]$report.ProcessId -ne [int]$context.ProcessId){throw '客户端结果进程身份不一致。'}
$report | Add-Member -NotePropertyName ClientsExited -NotePropertyValue $true -Force
$report | Add-Member -NotePropertyName ClientExitCode -NotePropertyValue $clientExit -Force
$report | Add-Member -NotePropertyName ExecutableSha256 -NotePropertyValue $executableHash -Force
$destination=if($Action -eq 'exit'){$context.ExitResultPath}else{$context.WarmupResultPath}
if($destination){
    if(Test-Path -LiteralPath $destination){throw "结果文件已存在，未覆盖：$destination"}
    [IO.File]::WriteAllText([IO.Path]::GetFullPath($destination),($report|ConvertTo-Json -Depth 20),[Text.UTF8Encoding]::new($false))
}
$report | ConvertTo-Json -Depth 20
