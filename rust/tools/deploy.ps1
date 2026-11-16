[CmdletBinding()]
param()
$ErrorActionPreference='Stop'
$projectRoot=(Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..')).Path
$publish='D:\codex\ClickRun\ClickRun.Modern\publish'
$destination=Join-Path $publish 'ClickRun.Modern.exe'
$backup='D:\codex\ClickRun\backups\Modern-2.1.1-before-Rust-20261116'
$version=[regex]::Match([IO.File]::ReadAllText((Join-Path $projectRoot 'Cargo.toml')),'(?m)^version\s*=\s*"([0-9.]+)"').Groups[1].Value
if(-not $version){throw '无法读取应用版本。'}
$candidate=Join-Path $projectRoot "dist\$version\连点器 Modern"
if (-not(Test-Path -LiteralPath (Join-Path $backup 'publish\ClickRun.Modern.exe'))) { throw '未找到原版备份，未替换程序。' }
$running=Get-CimInstance Win32_Process|Where-Object {$_.ExecutablePath -and $_.ExecutablePath.Equals($destination,[StringComparison]::OrdinalIgnoreCase)}
if($running){throw '原程序仍在运行，请先正常退出。未强行结束进程。'}
foreach($file in @('连点器 Modern.exe','vcruntime140.dll','assets\FZGWKaiS.ttf')){
    if(-not(Test-Path -LiteralPath (Join-Path $candidate $file) -PathType Leaf)){throw "候选文件缺失：$file"}
}
[IO.Directory]::CreateDirectory((Join-Path $publish 'assets'))|Out-Null
Copy-Item -LiteralPath (Join-Path $candidate '连点器 Modern.exe') -Destination $destination
Copy-Item -LiteralPath (Join-Path $candidate 'vcruntime140.dll') -Destination (Join-Path $publish 'vcruntime140.dll')
Copy-Item -LiteralPath (Join-Path $candidate 'assets\FZGWKaiS.ttf') -Destination (Join-Path $publish 'assets\FZGWKaiS.ttf')
Copy-Item -LiteralPath (Join-Path $projectRoot 'README.md') -Destination (Join-Path $publish '使用说明.md')
if((Get-FileHash -LiteralPath $destination).Hash -ne (Get-FileHash -LiteralPath (Join-Path $candidate '连点器 Modern.exe')).Hash){throw '部署后可执行文件校验失败。'}
$shell=New-Object -ComObject WScript.Shell
try{
    $startupPath=Join-Path ([Environment]::GetFolderPath('Startup')) 'ClickRun.lnk'
    if(Test-Path -LiteralPath $startupPath){
        $startup=$shell.CreateShortcut($startupPath)
        try{
            if($startup.TargetPath.Equals($destination,[StringComparison]::OrdinalIgnoreCase)){
                $startupBackup=Join-Path $backup 'Startup-ClickRun.lnk'
                if(-not(Test-Path -LiteralPath $startupBackup)){Copy-Item -LiteralPath $startupPath -Destination $startupBackup}
                $startup.Arguments='--tray';$startup.Description='连点器 Modern'
                $startup.IconLocation="$destination,0";$startup.Save()
            }
        }finally{[Runtime.InteropServices.Marshal]::FinalReleaseComObject($startup)|Out-Null}
    }
    $shortcutPath=Join-Path ([Environment]::GetFolderPath('Desktop')) '连点器 Modern.lnk'
    $shortcut=$shell.CreateShortcut($shortcutPath)
    $shortcut.TargetPath=$destination;$shortcut.WorkingDirectory=$publish
    $shortcut.IconLocation="$destination,0";$shortcut.Description='连点器 Modern'
    $shortcut.Save()
    [Runtime.InteropServices.Marshal]::FinalReleaseComObject($shortcut)|Out-Null
}finally{[Runtime.InteropServices.Marshal]::FinalReleaseComObject($shell)|Out-Null}
Write-Output 'Rust 版本已部署到原 Modern 入口，并创建中文桌面快捷方式。原版备份已保留。'
