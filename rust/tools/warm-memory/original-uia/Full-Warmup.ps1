[CmdletBinding()]
param([Parameter(Mandatory=$true)][string]$ContextPath)
& (Join-Path $PSScriptRoot 'Invoke-Client.ps1') -ContextPath $ContextPath -Action full
