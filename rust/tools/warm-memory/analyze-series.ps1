[CmdletBinding()]
param(
    [string]$AreaPath=(Join-Path $PSScriptRoot '..\..\verification\warm-memory-20261116'),
    [string[]]$CommonRunNames=@(),
    [string[]]$ExcludedPilotNames=@('common-C1','common-R1','full-pilot-A')
)
$ErrorActionPreference='Stop';Set-StrictMode -Version Latest
$area=(Resolve-Path -LiteralPath $AreaPath).Path
$project=(Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..\..')).Path
$expected=Join-Path $project 'verification\warm-memory-20261116'
if(-not $area.Equals($expected,[StringComparison]::OrdinalIgnoreCase)){throw '只允许本次暖态验证目录。'}
$output=Join-Path $area 'analysis-generated'
foreach($path in @($area,$output)){if((Test-Path -LiteralPath $path) -and ((Get-Item -LiteralPath $path -Force).Attributes -band [IO.FileAttributes]::ReparsePoint)){throw '分析路径不能是重解析点。'}}
[IO.Directory]::CreateDirectory($output)|Out-Null
function Prop($Object,[string]$Name,$Default=$null){if($null -ne $Object){$p=$Object.PSObject.Properties[$Name];if($null -ne $p){return $p.Value}};$Default}
function Sum-Values($Rows,[string]$Name){$sum=0.0;foreach($row in @($Rows)){$value=Prop $row $Name;if($null -ne $value){$sum+=[double]$value}};$sum}
function New-Stats($Values){
    $v=@($Values|Where-Object {$null -ne $_}|ForEach-Object {[double]$_}|Sort-Object)
    if($v.Count -eq 0){return $null}
    $sum=0.0;foreach($x in $v){$sum+=$x};$mean=$sum/$v.Count
    $median=if($v.Count%2){$v[[int][Math]::Floor($v.Count/2)]}else{($v[$v.Count/2-1]+$v[$v.Count/2])/2}
    $variance=0.0;foreach($x in $v){$variance+=($x-$mean)*($x-$mean)}
    [ordered]@{N=$v.Count;Minimum=$v[0];Maximum=$v[-1];Mean=$mean;Median=$median;SampleStandardDeviation=if($v.Count -gt 1){[Math]::Sqrt($variance/($v.Count-1))}else{$null}}
}
function Save-Text([string]$Name,[string]$Text){$path=Join-Path $output $Name;if((Test-Path -LiteralPath $path) -and ((Get-Item -LiteralPath $path -Force).Attributes -band [IO.FileAttributes]::ReparsePoint)){throw '生成文件不能是重解析点。'};[IO.File]::WriteAllText($path,$Text,[Text.UTF8Encoding]::new($false))}
function Save-Csv([string]$Name,$Rows){$lines=@($Rows|ConvertTo-Csv -NoTypeInformation);Save-Text $Name (($lines -join [Environment]::NewLine)+[Environment]::NewLine)}
function Name-Key([string]$Path){[IO.Path]::GetFileName($Path).ToLowerInvariant()}
$valid=[Collections.Generic.List[object]]::new();$excluded=[Collections.Generic.List[object]]::new()
$runRows=[Collections.Generic.List[object]]::new();$stageRows=[Collections.Generic.List[object]]::new();$moduleRows=[Collections.Generic.List[object]]::new();$mappedRows=[Collections.Generic.List[object]]::new();$moduleGroupRows=[Collections.Generic.List[object]]::new();$inputFiles=[Collections.Generic.List[object]]::new()
$moduleSets=[ordered]@{
    UIA=@('uiautomationcore.dll','uiautomationtypes.dll','uiautomationprovider.dll','oleacc.dll')
    Shell=@('shell32.dll','windows.storage.dll','propsys.dll','shcore.dll','shlwapi.dll')
    COM=@('combase.dll','ole32.dll','oleaut32.dll','rpcrt4.dll','actxprxy.dll')
    GDIText=@('gdi32.dll','gdi32full.dll','win32u.dll','gdiplus.dll','dwrite.dll','usp10.dll','textshaping.dll','msctf.dll','uxtheme.dll')
}
# 验证轮次均为area直属目录；不扫描候选载荷、构建target或生成结果。
foreach($directory in Get-ChildItem -LiteralPath $area -Directory|Sort-Object Name){
    if($directory.Attributes -band [IO.FileAttributes]::ReparsePoint){continue}
    $reportFile=Join-Path $directory.FullName 'run-report.json';if(-not(Test-Path -LiteralPath $reportFile -PathType Leaf)){continue}
    $name=$directory.Name;$reason=$null
    try{
        $report=Get-Content -LiteralPath $reportFile -Raw|ConvertFrom-Json
        if((Prop $report 'Passed') -ne $true -or (Prop $report 'NormalExit') -ne $true){throw ('未同时Passed和NormalExit；'+[string](Prop $report 'Failure')+' '+[string](Prop $report 'ExitFailure'))}
        $steadyFile=Join-Path $directory.FullName 'steady20.json';$steady=Get-Content -LiteralPath $steadyFile -Raw|ConvertFrom-Json
        if(-not $steady.Completed -or $steady.ProcessId -ne $report.ProcessId -or $steady.RequestedSeconds -ne 20 -or $steady.Summary.SampleCount -ne 20){throw '20秒稳态未完整或PID不符。'}
        $required=@('startup-ready-before-own-uia','common-client-exited','uia-exited-0s','uia-exited-30s','uia-exited-60s');if($report.Scope -eq 'Full'){$required+='full-client-exited'}
        foreach($phase in $required){if(@($report.Phases|Where-Object {$_.Native.Phase -eq $phase}).Count -ne 1){throw "缺失或重复阶段：$phase"}}
        foreach($phase in $report.Phases){$residentFile=Join-Path $directory.FullName $phase.ResidentFile;if(-not(Test-Path -LiteralPath $residentFile -PathType Leaf)){throw '缺失驻留页证据。'};$resident=Get-Content -LiteralPath $residentFile -Raw|ConvertFrom-Json;if($resident.Snapshot.ProcessId -ne $report.ProcessId -or -not $resident.Snapshot.ProcessPath.Equals($report.Executable,[StringComparison]::OrdinalIgnoreCase)){throw '驻留页证据目标不符。'}}
    }catch{$reason=$_.Exception.Message}
    if($reason){$excluded.Add([ordered]@{Run=$name;Reason=$reason});continue}
    $metric=[ordered]@{
        SteadyWorkingSetAverageMiB=[double]$steady.Summary.WorkingSetMiB.Average
        SteadyWorkingSetMinimumMiB=[double]$steady.Summary.WorkingSetMiB.Minimum
        SteadyWorkingSetMaximumMiB=[double]$steady.Summary.WorkingSetMiB.Maximum
        SteadyPrivateCommitAverageMiB=[double]$steady.Summary.PrivateMemoryMiB.Average
        SteadyPrivateCommitMinimumMiB=[double]$steady.Summary.PrivateMemoryMiB.Minimum
        SteadyPrivateCommitMaximumMiB=[double]$steady.Summary.PrivateMemoryMiB.Maximum
        CpuOneCorePercent=[double]$steady.Summary.CpuOneCorePercent
    }
    $experiment=if($name -match '^font-(\d+)-([ABD])$'){'Font'+$Matches[2]}elseif($report.Scope -eq 'Common'){'Common'+$report.Flavor}else{'Other'}
    $isPilot=$ExcludedPilotNames -contains $name
    $run=[pscustomobject]@{Name=$name;Directory=$directory.FullName;Report=$report;Steady=$steady;Metrics=$metric;Experiment=$experiment;Pilot=$isPilot}
    $valid.Add($run)
    foreach($f in @($reportFile,$steadyFile)){$inputFiles.Add([ordered]@{RelativePath=[IO.Path]::GetRelativePath($area,$f);Sha256=(Get-FileHash -LiteralPath $f).Hash})}
    foreach($phase in $report.Phases){
        $native=$phase.Native;$phaseName=$native.Phase;$residentFile=Join-Path $directory.FullName $phase.ResidentFile;$resident=Get-Content -LiteralPath $residentFile -Raw|ConvertFrom-Json;$snapshot=$resident.Snapshot
        $image=@($snapshot.Files|Where-Object Kind -eq 'MEM_IMAGE');$mapped=@($snapshot.Files|Where-Object Kind -eq 'MEM_MAPPED')
        $imageCow=Sum-Values $image 'PrivateResidentMiB';$imageTotal=Sum-Values $image 'ResidentMiB'
        $row=[ordered]@{Run=$name;Phase=$phaseName;ActualCaptureStartSeconds=Prop $phase 'CaptureStartedAfterWarmupSeconds';ActualCaptureEndSeconds=Prop $phase 'CaptureEndedAfterWarmupSeconds';CapturedUtc=$snapshot.CapturedUtc;WorkingSetMiB=$native.WorkingSetBytes/1MB;PrivateCommitMiB=$native.PrivateCommitBytes/1MB;LifetimePeakWorkingSetMiB=$native.LifetimePeakWorkingSetBytes/1MB;LifetimePeakPrivateCommitMiB=$native.LifetimePeakPrivateCommitBytes/1MB;ResidentMiB=$snapshot.ResidentMiB;PrivateResidentMiB=$snapshot.PrivateResidentMiB;ShareableResidentMiB=$snapshot.ShareableResidentMiB;ImageResidentMiB=$imageTotal;ImagePrivateCowMiB=$imageCow;ImageShareableMiB=$imageTotal-$imageCow;MappedResidentMiB=(Sum-Values $mapped 'ResidentMiB');GdiObjects=$native.GdiObjects;UserObjects=$native.UserObjects;Threads=$native.Threads;Handles=$native.Handles;LoadedModuleCount=@($phase.Modules).Count;VirtualQueryFailures=$snapshot.VirtualQueryFailures}
        $stageRows.Add([pscustomobject]$row)
        if($phaseName -match '^uia-exited-(0|30|60)s$'){
            $suffix=$Matches[1]+'s'
            foreach($key in @('WorkingSetMiB','PrivateCommitMiB','LifetimePeakWorkingSetMiB','LifetimePeakPrivateCommitMiB','ResidentMiB','PrivateResidentMiB','ShareableResidentMiB','ImageResidentMiB','ImagePrivateCowMiB','ImageShareableMiB','MappedResidentMiB','GdiObjects','UserObjects','Threads','Handles','LoadedModuleCount')){$metric[$key+'At'+$suffix]=$row[$key]}
        }
        $loadedByName=@{};foreach($module in $phase.Modules){$key=Name-Key $module.Name;if(-not $loadedByName.ContainsKey($key)){$loadedByName[$key]=[Collections.Generic.List[object]]::new()};$loadedByName[$key].Add($module)}
        $imageByName=@{};foreach($file in $image){$key=Name-Key $file.File;if(-not $imageByName.ContainsKey($key)){$imageByName[$key]=[Collections.Generic.List[object]]::new()};$imageByName[$key].Add($file)}
        $localModules=[Collections.Generic.List[object]]::new()
        foreach($key in @(@($loadedByName.Keys)+@($imageByName.Keys)|Sort-Object -Unique)){
            $loaded=if($loadedByName.ContainsKey($key)){@($loadedByName[$key].ToArray())}else{@()};$files=if($imageByName.ContainsKey($key)){@($imageByName[$key].ToArray())}else{@()}
            $total=Sum-Values $files 'ResidentMiB';$cow=Sum-Values $files 'PrivateResidentMiB'
            $m=[pscustomobject][ordered]@{Run=$name;Phase=$phaseName;Module=$key;Loaded=($loaded.Count -gt 0);LoadedCount=$loaded.Count;LoadedPaths=(@($loaded|ForEach-Object Path)-join ' | ');ResidentImagePaths=(@($files|ForEach-Object File)-join ' | ');VirtualImageBytes=(Sum-Values $loaded 'VirtualImageBytes');ActualResidentMiB=$total;PrivateImageCowMiB=$cow;ShareableImageMiB=$total-$cow;AmbiguousFilenameJoin=($loaded.Count -gt 1 -or $files.Count -gt 1)}
            $localModules.Add($m);$moduleRows.Add($m)
        }
        foreach($group in $moduleSets.Keys){$members=@($localModules|Where-Object {$moduleSets[$group] -contains $_.Module});$moduleGroupRows.Add([pscustomobject][ordered]@{Run=$name;Phase=$phaseName;Group=$group;LoadedModuleCount=@($members|Where-Object Loaded).Count;ActualResidentMiB=(Sum-Values $members 'ActualResidentMiB');PrivateImageCowMiB=(Sum-Values $members 'PrivateImageCowMiB');ShareableImageMiB=(Sum-Values $members 'ShareableImageMiB');Members=(@($members|ForEach-Object Module)-join ' | ')})}
        foreach($file in $mapped){$mappedRows.Add([pscustomobject][ordered]@{Run=$name;Phase=$phaseName;MappedFile=$file.File;ResidentMiB=$file.ResidentMiB;PrivateResidentMiB=$file.PrivateResidentMiB;ShareableResidentMiB=$file.ResidentMiB-$file.PrivateResidentMiB})}
        $inputFiles.Add([ordered]@{RelativePath=[IO.Path]::GetRelativePath($area,$residentFile);Sha256=(Get-FileHash -LiteralPath $residentFile).Hash})
    }
    $flat=[ordered]@{Run=$name;Experiment=$experiment;PilotExcludedFromExperiment=$isPilot;Flavor=$report.Flavor;Scope=$report.Scope;StartedUtc=$report.StartedUtc;ExecutableSha256=$report.ExecutableSha256;SteadyMeasuredSeconds=$steady.Summary.MeasuredSeconds}
    foreach($key in $metric.Keys){$flat[$key]=$metric[$key]};$runRows.Add([pscustomobject]$flat)
}
$byName=@{};foreach($run in $valid){$byName[$run.Name]=$run}
$order=@('A','B','D','D','B','A','D','A','B','B','A','D');$fontPlan=[Collections.Generic.List[object]]::new();$fontGroups=[ordered]@{}
for($i=1;$i -le 12;$i++){$name='font-{0:D2}-{1}' -f $i,$order[$i-1];$fontPlan.Add([ordered]@{Index=$i;Variant=$order[$i-1];Run=$name;Available=$byName.ContainsKey($name)})}
function Group-Stats($Runs){$runsArray=@($Runs);$stats=[ordered]@{};if($runsArray.Count){foreach($key in $runsArray[0].Metrics.Keys){$stats[$key]=New-Stats @($runsArray|ForEach-Object {$_.Metrics[$key]})}};[ordered]@{N=$runsArray.Count;Runs=@($runsArray|ForEach-Object Name);Metrics=$stats}}
foreach($variant in @('A','B','D')){$selected=@($fontPlan|Where-Object {$_.Variant -eq $variant -and $_.Available}|ForEach-Object {$byName[$_.Run]});$fontGroups[$variant]=Group-Stats $selected}
$pairRows=[Collections.Generic.List[object]]::new();$pairStatus=[Collections.Generic.List[object]]::new()
function Add-Pair([string]$Contrast,[string]$Left,[string]$Right){
    $available=$byName.ContainsKey($Left) -and $byName.ContainsKey($Right)
    $pairStatus.Add([ordered]@{Contrast=$Contrast;Left=$Left;Right=$Right;Available=$available})
    if(-not $available){return}
    $l=$byName[$Left];$r=$byName[$Right]
    foreach($key in $l.Metrics.Keys){if($r.Metrics.Contains($key)){$lv=[double]$l.Metrics[$key];$rv=[double]$r.Metrics[$key];$pairRows.Add([pscustomobject][ordered]@{Contrast=$Contrast;Left=$Left;Right=$Right;Metric=$key;LeftValue=$lv;RightValue=$rv;DifferenceLeftMinusRight=$lv-$rv;PercentOfRight=if($rv -ne 0){($lv-$rv)/$rv*100}else{$null}})}}
}
foreach($indices in @(@(1,2),@(6,5),@(8,9),@(11,10))){Add-Pair 'A-B私有注册路径' ('font-{0:D2}-A' -f $indices[0]) ('font-{0:D2}-B' -f $indices[1])}
foreach($indices in @(@(2,3),@(5,4),@(9,7),@(10,12))){Add-Pair 'B-D请求字体family' ('font-{0:D2}-B' -f $indices[0]) ('font-{0:D2}-D' -f $indices[1])}
$commonSelectionReason='显式CommonRunNames'
if($CommonRunNames.Count -eq 0){
    $candidates=@($valid|Where-Object {$_.Report.Scope -eq 'Common' -and -not $_.Pilot}|Sort-Object {$_.Report.StartedUtc})
    $commonSelectionReason='自动寻找时间顺序C/R/R/C的最后完整四轮，排除pilot。'
    if($candidates.Count -ge 4){for($i=0;$i -le $candidates.Count-4;$i++){$window=@($candidates[$i..($i+3)]);if(($window|ForEach-Object {$_.Report.Flavor}) -join '/' -eq 'LegacyC/Rust/Rust/LegacyC'){$CommonRunNames=@($window|ForEach-Object Name)}}}
}
$commonValid=$CommonRunNames.Count -eq 4;$expectedFlavors=@('LegacyC','Rust','Rust','LegacyC')
if($commonValid){for($i=0;$i -lt 4;$i++){if(-not $byName.ContainsKey($CommonRunNames[$i]) -or $byName[$CommonRunNames[$i]].Report.Flavor -ne $expectedFlavors[$i] -or $byName[$CommonRunNames[$i]].Report.Scope -ne 'Common' -or $byName[$CommonRunNames[$i]].Pilot){$commonValid=$false}}}
if($commonValid){Add-Pair 'Common原C-Rust' $CommonRunNames[0] $CommonRunNames[1];Add-Pair 'Common原C-Rust' $CommonRunNames[3] $CommonRunNames[2]}
$pairedStats=[ordered]@{}
foreach($contrast in @('A-B私有注册路径','B-D请求字体family','Common原C-Rust')){
    $metrics=[ordered]@{};foreach($group in @($pairRows|Where-Object Contrast -eq $contrast|Group-Object Metric)){$metrics[$group.Name]=New-Stats @($group.Group|ForEach-Object DifferenceLeftMinusRight)}
    $pairedStats[$contrast]=[ordered]@{CompletePairs=@($pairStatus|Where-Object {$_.Contrast -eq $contrast -and $_.Available}).Count;Metrics=$metrics}
}
$installedFont=$null;$fontEvidenceFile=Join-Path $area 'installed-font-evidence.json';if(Test-Path -LiteralPath $fontEvidenceFile){$f=Get-Content -LiteralPath $fontEvidenceFile -Raw|ConvertFrom-Json;$installedFont=[ordered]@{EvidenceFile='installed-font-evidence.json';Name=$f.Name;Hash=$f.Hash;ExistsAtProbe=$f.Exists}}
$summary=[ordered]@{
    SchemaVersion=1;GeneratedUtc=[DateTimeOffset]::UtcNow.ToString('o');ValidRunCount=$valid.Count;Excluded=$excluded.ToArray();ExcludedPilotNames=$ExcludedPilotNames
    Definitions=[ordered]@{Admission='仅Passed=true、NormalExit=true、20个完整稳态样本且全部要求阶段及驻留目标身份一致的run。pilot即使有效也不进入实验统计。';A='正式3.0.1请求方正公文楷体并从assets文件执行私有注册。';B='相同正式二进制，无assets私有字体文件；仍请求方正公文楷体，本机存在全局安装同hash字体。';D='无assets私有字体文件，诊断版本只将请求family改为Microsoft YaHei UI。';AB='A/B比较私有字体注册路径，不能称为有字体/无字体。';BD='B/D比较请求字体family；D诊断二进制与正式不同，差异范围依据diagnostic-font-only-manifest/diff。';Differences='成对先计算Left-Right，再对差值求均值/范围/中位数/样本标准差；正数表示左侧更多。小样本范围不作统计显著性推断。';Resident='QueryWorkingSet真实驻留页；Shareable位不表示当前正与另一进程共享。MEM_IMAGE的PrivatePages归为映像私有/COW，包含多种写时复制原因。';Modules='loaded模块表与真实驻留并列，loaded但resident=0仍保留。以文件名连接模块驱动路径与native映射路径，歧义单独标记。VirtualImageBytes不是驻留内存。';Mapped='未知MEM_MAPPED仅列映射文件证据，不推测为字体。';ModuleSets='UIA/Shell/COM/GDIText只是明确文件名集合的加总，不代表单一功能的因果归属；同一DLL可服务多个功能。';Timing='0/30/60为名义目标，ActualCaptureStart/End为从末次暖机客户端退出统一起点的实际耗时。OSpeak为截至阶段的生命周期高水位。';Cache='测试进程串行且正常退出，未修剪工作集、未强制GC；系统字体服务/文件缓存、外部辅助技术不受工具控制。'}
    InstalledFontEvidence=$installedFont;FontPlan=$fontPlan.ToArray();FontGroups=$fontGroups;PairAvailability=$pairStatus.ToArray();PairedDifferenceStatistics=$pairedStats
    Common=[ordered]@{SelectionReason=$commonSelectionReason;RequestedRuns=$CommonRunNames;CompleteOrderedMatrix=$commonValid;LegacyC=Group-Stats @($CommonRunNames|Where-Object {$byName.ContainsKey($_) -and $byName[$_].Report.Flavor -eq 'LegacyC' -and -not $byName[$_].Pilot}|ForEach-Object {$byName[$_]});Rust=Group-Stats @($CommonRunNames|Where-Object {$byName.ContainsKey($_) -and $byName[$_].Report.Flavor -eq 'Rust' -and -not $byName[$_].Pilot}|ForEach-Object {$byName[$_]})}
    SourceHashes=$inputFiles.ToArray()
}
Save-Csv 'runs.csv' $runRows.ToArray();Save-Csv 'stages.csv' $stageRows.ToArray();Save-Csv 'modules.csv' $moduleRows.ToArray();Save-Csv 'module-groups.csv' $moduleGroupRows.ToArray();Save-Csv 'mapped-files.csv' $mappedRows.ToArray();Save-Csv 'pairs.csv' $pairRows.ToArray()
Save-Text 'summary.json' ($summary|ConvertTo-Json -Depth 24)
$lines=[Collections.Generic.List[string]]::new();$lines.Add('# 暖态测量自动汇总');$lines.Add('');$lines.Add("有效完整轮次：$($valid.Count)。字体A/B/D已齐：$($fontGroups.A.N)/$($fontGroups.B.N)/$($fontGroups.D.N)，预定各4轮。Common完整C/R/R/C：$commonValid。");$lines.Add('');$lines.Add('A/B表示私有字体注册路径差异；本机全局安装同hash方正字体，B不能解释为没有方正字体。B/D表示请求family差异。Full结果和Common结果分别比较。');$lines.Add('');$lines.Add('| 分组 | 完整轮次 | 20秒WS均值的范围 MiB | 私有提交均值的范围 MiB |');$lines.Add('| --- | ---: | ---: | ---: |')
foreach($variant in @('A','B','D')){$g=$fontGroups[$variant];if($g.N){$ws=$g.Metrics.SteadyWorkingSetAverageMiB;$pc=$g.Metrics.SteadyPrivateCommitAverageMiB;$lines.Add(('| {0} | {1} | {2:F3}–{3:F3} | {4:F3}–{5:F3} |' -f $variant,$g.N,$ws.Minimum,$ws.Maximum,$pc.Minimum,$pc.Maximum))}}
$lines.Add('');$lines.Add('逐阶段资源见 stages.csv；实际模块驻留/COW见 modules.csv 和 module-groups.csv；未知文件映射见 mapped-files.csv；成对差值见 pairs.csv。summary.json保留范围统计、配对缺失、排除原因和原始证据SHA256。');$lines.Add('');$lines.Add('系统字体服务缓存与外部读屏器未受控；模块文件集合归因只是加总证据，不能把全部共享DLL成本归于单一功能。')
Save-Text 'summary.md' ($lines -join [Environment]::NewLine)
[ordered]@{GeneratedDirectory=$output;ValidRuns=$valid.Count;FontA=$fontGroups.A.N;FontB=$fontGroups.B.N;FontD=$fontGroups.D.N;CommonComplete=$commonValid;Excluded=@($excluded).Count}|ConvertTo-Json
