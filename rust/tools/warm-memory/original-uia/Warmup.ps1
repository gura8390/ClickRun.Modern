[CmdletBinding()]
param([Parameter(Mandatory=$true)][string]$ContextPath)
$ErrorActionPreference='Stop'
$context=Get-Content -LiteralPath $ContextPath -Raw | ConvertFrom-Json
$action=if($context.Stage -eq 'Full'){'full'}else{'common'}
& (Join-Path $PSScriptRoot 'Invoke-Client.ps1') -ContextPath $ContextPath -Action $action
