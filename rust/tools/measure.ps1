[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [ValidateRange(1, 2147483647)]
    [int]$ProcessId,

    [Parameter(Mandatory = $true)]
    [ValidateNotNullOrEmpty()]
    [string]$Label,

    [ValidateRange(10, 3600)]
    [int]$Seconds = 10,

    [Parameter(Mandatory = $true)]
    [ValidateNotNullOrEmpty()]
    [string]$OutFile
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

# 该工具只读取目标进程，绝不调用 EmptyWorkingSet、SetProcessWorkingSetSize、
# GC.Collect 或改变优先级/亲和性。输出文件必须是新文件，避免覆盖既有证据。
$outputPath = [IO.Path]::GetFullPath($OutFile)
if (Test-Path -LiteralPath $outputPath) { throw "结果文件已存在，未覆盖：$outputPath" }
$outputDirectory = [IO.Path]::GetDirectoryName($outputPath)
[IO.Directory]::CreateDirectory($outputDirectory) | Out-Null

$logicalProcessors = [Environment]::ProcessorCount
$processorCountSource = 'Environment.ProcessorCount'
try {
    $computer = Get-CimInstance -ClassName Win32_ComputerSystem -Property NumberOfLogicalProcessors
    if ($computer.NumberOfLogicalProcessors -gt 0) {
        $logicalProcessors = [int]$computer.NumberOfLogicalProcessors
        $processorCountSource = 'Win32_ComputerSystem.NumberOfLogicalProcessors'
    }
} catch {
    # 在限制 WMI 的系统仍可读进程；记录回退来源，避免把归一化值称为未知核数。
}

$process = Get-Process -Id $ProcessId
$process.Refresh()
$processName = $process.ProcessName
$initialCpu = $process.TotalProcessorTime.TotalSeconds
$processStarted = $process.StartTime.ToUniversalTime().ToString('o')
$started = [DateTimeOffset]::UtcNow
$watch = [Diagnostics.Stopwatch]::StartNew()
$previousCpu = $initialCpu
$previousWall = 0.0
$samples = [Collections.Generic.List[object]]::new()
$failure = $null

try {
    for ($sampleIndex = 1; $sampleIndex -le $Seconds; $sampleIndex++) {
        # 相对起点调度每秒采样，消除读取开销造成的累积漂移。
        $remaining = $sampleIndex - $watch.Elapsed.TotalSeconds
        if ($remaining -gt 0) {
            Start-Sleep -Milliseconds ([int][Math]::Ceiling($remaining * 1000))
        }
        $process.Refresh()
        if ($process.HasExited) { throw "目标进程在采样期间退出：$ProcessId" }
        $cpu = $process.TotalProcessorTime.TotalSeconds
        $wall = $watch.Elapsed.TotalSeconds
        $interval = $wall - $previousWall
        $cpuDelta = [Math]::Max(0.0, $cpu - $previousCpu)
        $oneCore = $cpuDelta / $interval * 100
        $samples.Add([ordered]@{
            Index = $sampleIndex
            ElapsedSeconds = $wall
            IntervalSeconds = $interval
            WorkingSetBytes = $process.WorkingSet64
            WorkingSetMiB = $process.WorkingSet64 / 1MB
            PrivateMemoryBytes = $process.PrivateMemorySize64
            PrivateMemoryMiB = $process.PrivateMemorySize64 / 1MB
            CpuSecondsDelta = $cpuDelta
            CpuOneCorePercent = $oneCore
            CpuWholeMachinePercent = $oneCore / $logicalProcessors
            ThreadCount = $process.Threads.Count
            HandleCount = $process.HandleCount
        })
        $previousCpu = $cpu
        $previousWall = $wall
    }
} catch {
    $failure = $_.Exception.Message
} finally {
    $watch.Stop()
    $process.Dispose()
}

$summary = $null
if ($samples.Count -gt 0) {
    $workingSet = $samples | ForEach-Object { $_.WorkingSetMiB } | Measure-Object -Minimum -Maximum -Average
    $privateMemory = $samples | ForEach-Object { $_.PrivateMemoryMiB } | Measure-Object -Minimum -Maximum -Average
    $totalCpu = $previousCpu - $initialCpu
    $summary = [ordered]@{
        SampleCount = $samples.Count
        MeasuredSeconds = $previousWall
        WorkingSetMiB = [ordered]@{ Minimum = $workingSet.Minimum; Average = $workingSet.Average; Maximum = $workingSet.Maximum }
        PrivateMemoryMiB = [ordered]@{ Minimum = $privateMemory.Minimum; Average = $privateMemory.Average; Maximum = $privateMemory.Maximum }
        CpuSecondsDelta = $totalCpu
        CpuOneCorePercent = $totalCpu / $previousWall * 100
        CpuWholeMachinePercent = $totalCpu / $previousWall * 100 / $logicalProcessors
    }
}

$report = [ordered]@{
    SchemaVersion = 1
    Label = $Label
    ProcessId = $ProcessId
    ProcessName = $processName
    ProcessStartedUtc = $processStarted
    StartedUtc = $started.ToString('o')
    EndedUtc = [DateTimeOffset]::UtcNow.ToString('o')
    RequestedSeconds = $Seconds
    Completed = ($null -eq $failure -and $samples.Count -ge 10 -and $samples.Count -eq $Seconds)
    Failure = $failure
    LogicalProcessors = $logicalProcessors
    LogicalProcessorCountSource = $processorCountSource
    Definitions = [ordered]@{
        MiB = '1 MiB = 1048576 字节。'
        WorkingSet = 'WorkingSet64：当前驻留物理内存，包含共享页；不等同于独占内存。'
        PrivateMemory = 'PrivateMemorySize64：私有提交量，含可能已换出的页；不等同于私有工作集。'
        CpuOneCorePercent = '进程CPU时间增量 / 实际墙钟时间增量 × 100；单个逻辑核心满载为100%，多线程可超过100%。'
        CpuWholeMachinePercent = '单核百分比 / LogicalProcessors；表示整机逻辑核心容量的占比。'
        SummaryCpu = '按整个实测墙钟区间计算，避免采样间隔偏差导致简单平均失真。'
        Sampling = '起始读取CPU基线；随后每秒采样一次，默认10次。未人为清空或修剪工作集。'
    }
    Summary = $summary
    Samples = $samples.ToArray()
}

# 原子发布结果。无论目标进程是否提前退出，都留下可复查的部分样本和失败原因。
$temporaryPath = $outputPath + '.' + [guid]::NewGuid().ToString('N') + '.tmp'
try {
    [IO.File]::WriteAllText($temporaryPath, ($report | ConvertTo-Json -Depth 8), [Text.UTF8Encoding]::new($false))
    [IO.File]::Move($temporaryPath, $outputPath)
} finally {
    if (Test-Path -LiteralPath $temporaryPath) { Remove-Item -LiteralPath $temporaryPath -Force }
}
$summary
Write-Output "测量记录：$outputPath"
if ($failure) { throw $failure }
