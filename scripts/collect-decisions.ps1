# Gather docs/decisions/*.md into RECOMMENDED.md at a release cut (one file
# per PR keeps issue branches from conflicting on RECOMMENDED.md).
# Usage: scripts/collect-decisions.ps1 -Version 0.14.0
param([Parameter(Mandatory = $true)][string]$Version)
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$dir = Join-Path $root 'docs/decisions'
$out = Join-Path $root 'RECOMMENDED.md'
$files = @(Get-ChildItem -Path $dir -Filter '*.md' | Where-Object { $_.Name -ne 'README.md' } |
    Sort-Object { $n = 0; [void][int]::TryParse(($_.BaseName -split '-')[0], [ref]$n); $n }, Name)
if ($files.Count -eq 0) { Write-Host 'no decision files to collect'; exit 0 }
$text = "`n## v$Version - decisions gathered at the cut`n"
foreach ($f in $files) { $text += "`n" + (Get-Content -Raw -Encoding utf8 $f.FullName).TrimEnd() + "`n" }
Add-Content -Path $out -Value $text -Encoding utf8 -NoNewline
foreach ($f in $files) { Remove-Item -LiteralPath $f.FullName }
Write-Host ("collected {0} decision file(s) into RECOMMENDED.md" -f $files.Count)
