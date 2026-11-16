[CmdletBinding()]
param([string]$OutputDirectory)
$ErrorActionPreference = 'Stop'
$projectRoot=(Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..')).Path
$versionMatch=[regex]::Match([IO.File]::ReadAllText((Join-Path $projectRoot 'Cargo.toml')),'(?m)^version\s*=\s*"([0-9.]+)"')
if(-not $versionMatch.Success){throw '无法读取应用版本。'}
$version=$versionMatch.Groups[1].Value
if (-not $OutputDirectory) { $OutputDirectory=Join-Path $projectRoot "dist\$version" }
$output=[IO.Path]::GetFullPath($OutputDirectory)
$app=Join-Path $output '连点器 Modern'
$exe=Join-Path $projectRoot 'target\release\liandianqi.exe'
if (-not (Test-Path -LiteralPath $exe -PathType Leaf)) { throw '请先运行 tools/build.ps1 -Action all。' }
$runtime=Get-ChildItem -LiteralPath 'C:\BuildTools\VC\Redist\MSVC' -Directory |
    Sort-Object Name -Descending |
    ForEach-Object { Join-Path $_.FullName 'x64\Microsoft.VC143.CRT\vcruntime140.dll' } |
    Where-Object { Test-Path -LiteralPath $_ -PathType Leaf } |
    Select-Object -First 1
if (-not $runtime) { throw '未找到微软 x64 可再分发运行库。' }
if ((Get-AuthenticodeSignature -LiteralPath $runtime).Status -ne 'Valid') { throw '微软运行库签名校验未通过。' }
[IO.Directory]::CreateDirectory((Join-Path $app 'assets')) | Out-Null
Copy-Item -LiteralPath $exe -Destination (Join-Path $app '连点器 Modern.exe')
Copy-Item -LiteralPath $runtime -Destination (Join-Path $app 'vcruntime140.dll')
Copy-Item -LiteralPath (Join-Path $projectRoot 'assets\FZGWKaiS.ttf') -Destination (Join-Path $app 'assets\FZGWKaiS.ttf')
$readme=[IO.File]::ReadAllText((Join-Path $projectRoot 'README.md')).Replace("verification/内存优化记录-$version.md",'测试记录.md')
[IO.File]::WriteAllText((Join-Path $app '使用说明.md'),$readme,[Text.UTF8Encoding]::new($false))
Copy-Item -LiteralPath (Join-Path $projectRoot 'LICENSE') -Destination (Join-Path $app 'LICENSE')
$notices=[IO.File]::ReadAllText((Join-Path $projectRoot 'THIRD_PARTY_NOTICES.md')).Replace('](../LICENSE)','](LICENSE)')
[IO.File]::WriteAllText((Join-Path $app 'THIRD_PARTY_NOTICES.md'),$notices,[Text.UTF8Encoding]::new($false))
$acceptance=Join-Path $projectRoot "verification\内存优化记录-$version.md"
if(-not(Test-Path -LiteralPath $acceptance)){$acceptance=Join-Path $projectRoot "docs\verification-$version.md"}
if (-not(Test-Path -LiteralPath $acceptance)){throw '缺少本版本测试记录。'}
Copy-Item -LiteralPath $acceptance -Destination (Join-Path $app '测试记录.md')
$files=Get-ChildItem -LiteralPath $app -File -Recurse | Where-Object Name -ne '文件校验.json' | ForEach-Object {
    [ordered]@{Path=[IO.Path]::GetRelativePath($app,$_.FullName);Bytes=$_.Length;SHA256=(Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash}
}
$manifest=[ordered]@{Product='连点器 Modern';Version=$version;Files=@($files)}
[IO.File]::WriteAllText((Join-Path $app '文件校验.json'),($manifest|ConvertTo-Json -Depth 6),[Text.UTF8Encoding]::new($false))
$zip=Join-Path $output "连点器Modern-$version-Windows-x64.zip"
Compress-Archive -LiteralPath $app -DestinationPath $zip -Force
# Read and hash every archive payload, including the bundled font/runtime.
Add-Type -AssemblyName System.IO.Compression.FileSystem
$archive=[IO.Compression.ZipFile]::OpenRead($zip)
try {
    foreach($file in $files) {
        $suffix='/'+$file.Path.Replace('\','/')
        $entry=$archive.Entries|Where-Object {$_.FullName.Replace('\','/').EndsWith($suffix)}|Select-Object -First 1
        if (-not $entry -or $entry.Length -ne $file.Bytes) { throw "压缩包载荷缺失：$($file.Path)" }
        $stream=$entry.Open()
        try { $sha=[Security.Cryptography.SHA256]::Create(); $hash=[Convert]::ToHexString($sha.ComputeHash($stream)); $sha.Dispose() } finally { $stream.Dispose() }
        if ($hash -ne $file.SHA256) { throw "压缩包载荷校验失败：$($file.Path)" }
    }
} finally { $archive.Dispose() }
Write-Output "已打包并逐文件校验：$zip"
