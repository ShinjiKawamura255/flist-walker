[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string]$ArchivePath
)

$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.IO.Compression.FileSystem -ErrorAction Stop

$resolvedArchive = (Resolve-Path -LiteralPath $ArchivePath).Path
$archive = [System.IO.Compression.ZipFile]::OpenRead($resolvedArchive)
try {
    $actual = @($archive.Entries | ForEach-Object { $_.FullName } | Sort-Object)
    $expected = @(
        'LICENSE.txt'
        'README.txt'
        'THIRD_PARTY_NOTICES.txt'
        'flistwalker.exe'
    ) | Sort-Object

    if (($actual -join "`n") -ne ($expected -join "`n")) {
        throw "TC-178 Windows release archive entries differ.`nExpected:`n$($expected -join "`n")`nActual:`n$($actual -join "`n")"
    }
} finally {
    $archive.Dispose()
}

Write-Host 'TC-178 Windows release archive contents passed.'
