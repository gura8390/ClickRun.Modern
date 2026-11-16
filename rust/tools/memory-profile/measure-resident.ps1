[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][ValidateRange(1,2147483647)][int]$ProcessId,
    [string]$OutFile
)
$ErrorActionPreference = 'Stop'
if (-not ('ClickRun.ReadOnlyMemoryProfile.Reader' -as [type])) {
    Add-Type -Path (Join-Path $PSScriptRoot 'ResidentProfile.cs')
}
$snapshot = [ClickRun.ReadOnlyMemoryProfile.Reader]::Capture($ProcessId)
$report = [ordered]@{
    SchemaVersion = 1
    Method = 'QueryWorkingSet真实驻留页 + VirtualQueryEx区域类型 + GetMappedFileNameW映射文件名'
    ReadOnly = $true
    Definitions = [ordered]@{
        ResidentMiB = '驻留页数 × 4096 / 1048576；按QueryWorkingSet返回的页统计。'
        PrivateResidentMiB = '工作集条目的Shared位为0的驻留页；包括映像写时复制私有页。'
        ShareableResidentMiB = 'Shared位为1的驻留页；不表示这些页当前一定同时由其他进程使用。'
        Categories = 'MEM_PRIVATE / MEM_IMAGE / MEM_MAPPED 是VirtualQueryEx映射类型，与私有工作集不是同一分类。'
        Files = '文件和模块数值均为真实驻留页累计，未使用文件尺寸或模块虚拟大小。'
        Timing = '驻留页是一时刻快照，区域和文件名随后查询。未读取目标页内容，未修剪工作集。'
    }
    Snapshot = $snapshot
}
$json = $report | ConvertTo-Json -Depth 8
if ($OutFile) {
    $destination = [IO.Path]::GetFullPath($OutFile)
    if (Test-Path -LiteralPath $destination) { throw "文件已存在，未覆盖：$destination" }
    if (-not (Test-Path -LiteralPath ([IO.Path]::GetDirectoryName($destination)) -PathType Container)) { throw '输出目录不存在。' }
    [IO.File]::WriteAllText($destination, $json, [Text.UTF8Encoding]::new($false))
}
$json
