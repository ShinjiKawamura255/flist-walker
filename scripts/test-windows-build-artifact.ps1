[CmdletBinding()]
param(
    [string]$ExePath
)

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'common-win-gnu.ps1')

if (-not $ExePath) {
    $ExePath = Join-Path $PSScriptRoot '..\rust\target\x86_64-pc-windows-gnu\release\FlistWalker.exe'
}
$exe = (Resolve-Path -LiteralPath $ExePath).Path
$alias = Join-Path (Split-Path -Parent $exe) 'flistwalker.exe'
if (-not (Test-Path -LiteralPath $alias -PathType Leaf)) {
    throw "Artifact alias is missing: $alias"
}
if ((Get-FileHash -Algorithm SHA256 -LiteralPath $exe).Hash -ne
    (Get-FileHash -Algorithm SHA256 -LiteralPath $alias).Hash) {
    throw 'TC-147 executable aliases are not byte-identical.'
}

$mingwBin = Join-Path (Get-FlistWalkerMsys2Root) 'mingw64\bin'
$objdump = Resolve-FlistWalkerCommand `
    -Names @('objdump.exe', 'objdump') `
    -AbsoluteCandidates @((Join-Path $mingwBin 'objdump.exe'))
$strings = Resolve-FlistWalkerCommand `
    -Names @('strings.exe', 'strings') `
    -AbsoluteCandidates @((Join-Path $mingwBin 'strings.exe'))
if (-not $objdump -or -not $strings) {
    throw 'TC-147 requires MSYS2 objdump.exe and strings.exe.'
}

$sections = & $objdump -h $exe
if ($LASTEXITCODE -ne 0 -or ($sections -join "`n") -notmatch '\.rsrc') {
    throw 'TC-147 .rsrc section is missing.'
}

$pe = & $objdump -p $exe
$peText = $pe -join "`n"
if ($LASTEXITCODE -ne 0 -or $peText -notmatch 'Subsystem\s+00000002\s+\(Windows GUI\)') {
    throw 'TC-147 Windows GUI subsystem is missing.'
}
$dlls = $pe | Select-String 'DLL Name:' | ForEach-Object { $_.Line.Trim() }
if ($dlls -match 'msys-|mingw|libgcc|libstdc\+\+|libwinpthread') {
    throw "TC-147 unexpected MSYS2 runtime DLL dependency: $($dlls -join ', ')"
}

$text = & $strings -a $exe
if ($LASTEXITCODE -ne 0 -or ($text -join "`n") -notmatch 'requestedExecutionLevel level="asInvoker"') {
    throw 'TC-147 asInvoker manifest is missing.'
}

Write-Host 'Windows build artifact validation passed.'
