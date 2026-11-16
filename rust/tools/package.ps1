[CmdletBinding()]
param([string]$OutputDirectory,[string]$RuntimePath)
$ErrorActionPreference='Stop'
$root=(Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..')).Path
if(-not $OutputDirectory){$OutputDirectory=Join-Path $root 'dist'}
$exe=Join-Path $root 'target\release\liandianqi.exe'
if(-not(Test-Path -LiteralPath $exe)){throw '请先执行 cargo build --release --locked。'}
if(-not $RuntimePath){
    $redist='C:\BuildTools\VC\Redist\MSVC'
    if(Test-Path -LiteralPath $redist){
        $RuntimePath=Get-ChildItem -LiteralPath $redist -Directory|Sort-Object Name -Descending|
            ForEach-Object{Join-Path $_.FullName 'x64\Microsoft.VC143.CRT\vcruntime140.dll'}|
            Where-Object{Test-Path -LiteralPath $_}|Select-Object -First 1
    }
}
if(-not $RuntimePath -or -not(Test-Path -LiteralPath $RuntimePath)){throw '请用 -RuntimePath 指定微软 x64 vcruntime140.dll。'}
if((Get-AuthenticodeSignature -LiteralPath $RuntimePath).Status -ne 'Valid'){throw '微软运行库签名校验失败。'}
$app=Join-Path ([IO.Path]::GetFullPath($OutputDirectory)) '连点器 Modern'
if(Test-Path -LiteralPath $app){throw '输出目录已存在，请指定新目录以避免混入旧文件。'}
New-Item -ItemType Directory -Path (Join-Path $app 'assets') -Force|Out-Null
Copy-Item -LiteralPath $exe -Destination (Join-Path $app '连点器 Modern.exe')
Copy-Item -LiteralPath $RuntimePath -Destination (Join-Path $app 'vcruntime140.dll')
Copy-Item -LiteralPath (Join-Path $root 'assets\FZGWKaiS.ttf') -Destination (Join-Path $app 'assets\FZGWKaiS.ttf')
Copy-Item -LiteralPath (Join-Path $root 'README.md') -Destination (Join-Path $app '使用说明.md')
Copy-Item -LiteralPath (Join-Path $root 'docs') -Destination (Join-Path $app 'docs') -Recurse
$notice=[IO.File]::ReadAllText((Join-Path $root 'THIRD_PARTY_NOTICES.md')).Replace('](../LICENSE)','](LICENSE)').Replace('](Cargo.lock)','](https://github.com/gura8390/ClickRun.Modern/blob/v3.0.0/rust/Cargo.lock)').Replace('](Cargo.toml)','](https://github.com/gura8390/ClickRun.Modern/blob/v3.0.0/rust/Cargo.toml)')
[IO.File]::WriteAllText((Join-Path $app 'THIRD_PARTY_NOTICES.md'),$notice,[Text.UTF8Encoding]::new($false))
Copy-Item -LiteralPath (Join-Path $root '..\LICENSE') -Destination (Join-Path $app 'LICENSE')
$files=Get-ChildItem -LiteralPath $app -Recurse -File|ForEach-Object{[ordered]@{
    Path=[IO.Path]::GetRelativePath($app,$_.FullName);Bytes=$_.Length;SHA256=(Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash
}}
[IO.File]::WriteAllText((Join-Path $app '文件校验.json'),([ordered]@{Version='3.0.0';Files=@($files)}|ConvertTo-Json -Depth 5),[Text.UTF8Encoding]::new($false))
$zip=Join-Path ([IO.Path]::GetFullPath($OutputDirectory)) 'ClickRun-Modern-v3.0.0-Windows-x64.zip'
Compress-Archive -LiteralPath $app -DestinationPath $zip
Write-Output "已生成公开便携包：$zip"
