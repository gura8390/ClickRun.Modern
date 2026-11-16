[CmdletBinding()]
param([int]$Seconds = 10, [switch]$SkipBuild)
$ErrorActionPreference = 'Stop'
$root = (Resolve-Path -LiteralPath $PSScriptRoot).Path
$project = (Resolve-Path -LiteralPath (Join-Path $root '..\..')).Path
$dotnet = 'D:\codex\05-环境\dotnet-sdk\dotnet.exe'
$sdk = 'D:\codex\01-桌面工具\WfZiUiBp\verification\work\sdk\x64\c'
$savedPath = $env:PATH
$savedLib = $env:LIB
$sourceFiles = @((Join-Path $project 'src\engine.rs'), (Join-Path $project 'src\config.rs'))
$sourceStamp = Join-Path $root 'rust\target-perf\source-sha256.json'
$initialSources = @($sourceFiles | ForEach-Object { Get-FileHash -LiteralPath $_ -Algorithm SHA256 | Select-Object Path,Hash })
try {
    $env:PATH = "$env:USERPROFILE\.cargo\bin;$savedPath"
    $env:LIB = "$sdk\um\x64;$sdk\ucrt\x64;$savedLib"
    if (-not $SkipBuild) {
        & $dotnet build (Join-Path $root 'dotnet\Perf.Dotnet.csproj') -c Release --nologo
        if ($LASTEXITCODE -ne 0) { throw '原版核心测试工具编译失败。' }
        Push-Location -LiteralPath (Join-Path $root 'rust')
        try {
            & $env:ComSpec /d /c 'call "C:\BuildTools\VC\Auxiliary\Build\vcvars64.bat" >nul && cargo build --release --offline --target-dir target-perf'
            if ($LASTEXITCODE -ne 0) { throw 'Rust 核心测试工具编译失败。' }
        } finally { Pop-Location }
        $initialSources | ConvertTo-Json | Set-Content -LiteralPath $sourceStamp -Encoding utf8
    }
    if (-not (Test-Path -LiteralPath $sourceStamp -PathType Leaf)) { throw '缺少测试工具源版本记录，请先运行一次不带 -SkipBuild 的命令。' }
    $compiledSources = @(Get-Content -LiteralPath $sourceStamp -Raw | ConvertFrom-Json)
    foreach ($source in $initialSources) {
        if (-not ($compiledSources | Where-Object { $_.Path -eq $source.Path -and $_.Hash -eq $source.Hash })) { throw 'Rust 核心源码已改变，请重新编译测试工具。' }
    }
    $publishedCore = 'D:\codex\ClickRun\ClickRun.Modern\publish\ClickRun.Core.dll'
    $testedCore = Join-Path $root 'dotnet\bin\Release\net8.0\ClickRun.Core.dll'
    if ((Get-FileHash -LiteralPath $publishedCore).Hash -ne (Get-FileHash -LiteralPath $testedCore).Hash) { throw '原版 Core.dll 已改变，请重新编译测试工具。' }
    function Measure-Core([string]$Implementation, [int]$Interval) {
        $start = [System.Diagnostics.ProcessStartInfo]::new()
        $start.UseShellExecute = $false
        $start.CreateNoWindow = $true
        $start.RedirectStandardInput = $true
        $start.RedirectStandardOutput = $true
        $start.RedirectStandardError = $true
        if ($Implementation -eq 'dotnet') {
            $start.FileName = $dotnet
            $start.ArgumentList.Add((Join-Path $root 'dotnet\bin\Release\net8.0\ClickRun.Core.Perf.dll'))
        } else { $start.FileName = Join-Path $root 'rust\target-perf\release\clickrun-core-perf.exe' }
        $start.ArgumentList.Add([string]$Interval)
        $start.ArgumentList.Add([string]$Seconds)
        $process = [System.Diagnostics.Process]::Start($start)
        try {
            $ready = $process.StandardOutput.ReadLine()
            if ($ready -ne 'READY') { throw "测试工具未就绪：$ready $($process.StandardError.ReadToEnd())" }
            $samples = [System.Collections.Generic.List[object]]::new()
            $process.StandardInput.WriteLine('START')
            $process.StandardInput.Flush()
            $clock = [System.Diagnostics.Stopwatch]::StartNew()
            while (-not $process.HasExited) {
                $process.Refresh()
                if ($process.HasExited) { break }
                $samples.Add([pscustomobject]@{
                    elapsedSeconds = $clock.Elapsed.TotalSeconds
                    workingSetBytes = $process.WorkingSet64
                    privateCommitBytes = $process.PrivateMemorySize64
                })
                if ($clock.Elapsed.TotalSeconds -gt $Seconds + 15) { throw '核心测量超时。' }
                Start-Sleep -Milliseconds 250
            }
            $output = $process.StandardOutput.ReadToEnd().Trim()
            $errors = $process.StandardError.ReadToEnd().Trim()
            if ($process.ExitCode -ne 0 -or $errors) { throw "核心测量失败：$errors" }
            $metrics = $output | ConvertFrom-Json
            $steady = @($samples | Where-Object elapsedSeconds -ge 1)
            function Median($Values) { $ordered = @($Values | Sort-Object); $n=$ordered.Count; if ($n -eq 0) { return $null }; if ($n % 2) { return $ordered[[int][Math]::Floor($n/2)] }; return ($ordered[$n/2-1]+$ordered[$n/2])/2 }
            [pscustomobject]@{
                implementation = $Implementation
                metrics = $metrics
                memory = [pscustomobject]@{
                    medianWorkingSetBytes = Median @($steady.workingSetBytes)
                    maxWorkingSetBytes = ($steady.workingSetBytes | Measure-Object -Maximum).Maximum
                    medianPrivateCommitBytes = Median @($steady.privateCommitBytes)
                    maxPrivateCommitBytes = ($steady.privateCommitBytes | Measure-Object -Maximum).Maximum
                    note = '外部进程250ms采样，剔除最初1秒后取中位数；仅核心harness，包含各自运行时，不代表完整应用。'
                }
                samples = @($samples)
            }
        } finally { $process.Dispose() }
    }
    $runs = [System.Collections.Generic.List[object]]::new()
    foreach ($interval in @(1, 100)) {
        foreach ($implementation in @('dotnet', 'rust')) {
            Write-Host "测量 $implementation，目标间隔 $interval ms，时长 $Seconds 秒"
            $runs.Add((Measure-Core $implementation $interval))
        }
    }
    foreach ($source in $initialSources) {
        if ((Get-FileHash -LiteralPath $source.Path -Algorithm SHA256).Hash -ne $source.Hash) { throw '测量期间 Rust 核心源码发生变化，结果未作为有效记录保存。' }
    }
    $result = [pscustomobject]@{
        recordedAt = [DateTimeOffset]::Now.ToString('o')
        environment = [pscustomobject]@{ os = [Environment]::OSVersion.VersionString; logicalProcessors = [Environment]::ProcessorCount }
        scope = '两个独立核心harness；原版已发布Core.dll注入返回true的sender，Rust直接引用当前Engine/Config源码并dry_run=true。均单击、无随机、无限、不发送桌面输入，不加载UI/热键/配置/自启。'
        caveats = @('不是完整应用内存对比；工作集和私有提交包含各自harness运行时。', 'CPU为进程累计时间/墙钟时间的单逻辑核百分比；全机百分比需再除逻辑核数。', "每个进程先用1ms预热500ms；每档仅一次${Seconds}秒采样，结果受系统调度与负载影响。", 'CPU累计增量为0表示本次采样未分辨出消耗，不证明绝对零CPU。', 'dry sender排除了SendInput和旧版每次创建输入数组的成本；实际桌面输入需另验。')
        hashes = @(
            Get-FileHash -LiteralPath 'D:\codex\ClickRun\ClickRun.Modern\publish\ClickRun.Core.dll' -Algorithm SHA256 | Select-Object Path,Hash
            Get-FileHash -LiteralPath (Join-Path $project 'src\engine.rs') -Algorithm SHA256 | Select-Object Path,Hash
            Get-FileHash -LiteralPath (Join-Path $project 'src\config.rs') -Algorithm SHA256 | Select-Object Path,Hash
            Get-FileHash -LiteralPath (Join-Path $root 'rust\target-perf\release\clickrun-core-perf.exe') -Algorithm SHA256 | Select-Object Path,Hash
        )
        runs = @($runs)
    }
    $out = Join-Path $project 'verification\core-perf-comparison.json'
    $result | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $out -Encoding utf8
    $runs | ForEach-Object { [pscustomobject]@{ implementation=$_.implementation;intervalMs=$_.metrics.intervalMs;clicksPerSecond=[Math]::Round($_.metrics.clicksPerSecond,2);cpuPercentOneCore=[Math]::Round($_.metrics.cpuPercentOneCore,3);workingSetMiB=[Math]::Round($_.memory.medianWorkingSetBytes/1MB,2);privateCommitMiB=[Math]::Round($_.memory.medianPrivateCommitBytes/1MB,2) } } | Format-Table -AutoSize
    Write-Host "结果：$out"
} finally { $env:PATH=$savedPath; $env:LIB=$savedLib }
