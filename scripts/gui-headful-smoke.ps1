param(
    [int]$DurationSeconds = 10,
    [string]$BaseDir = "",
    [switch]$NoBuild
)

$ErrorActionPreference = "Stop"

function New-GuiSmokeFixture {
    param(
        [Parameter(Mandatory = $true)][string]$RootDir,
        [Parameter(Mandatory = $true)][string]$EvidenceDir,
        [Parameter(Mandatory = $true)][string]$DebugDir
    )

    New-Item -ItemType Directory -Force -Path `
        $RootDir, `
        (Join-Path $RootDir "docs"), `
        (Join-Path $RootDir "nested"), `
        (Join-Path $RootDir "ignored"), `
        (Join-Path $RootDir "actions"), `
        (Join-Path $RootDir "empty-dir"), `
        $EvidenceDir, `
        $DebugDir | Out-Null

    Set-Content -LiteralPath (Join-Path $RootDir "README-preview.txt") -Value @"
FlistWalker GUI smoke fixture

This file is used for preview, search highlighting, and action routing checks.
Search terms: alpha beta gamma exact-start final-end
"@
    Set-Content -LiteralPath (Join-Path $RootDir "docs\alpha-report.md") -Value "# Alpha Report`n`nUse this file for non-empty query, preview, and sort checks."
    Set-Content -LiteralPath (Join-Path $RootDir "docs\beta_notes.txt") -Value "beta note`nThis file is intentionally plain text for preview checks."
    Set-Content -LiteralPath (Join-Path $RootDir "actions\open-target.txt") -Value "Open action target for manual GUI smoke."
    Set-Content -LiteralPath (Join-Path $RootDir "actions\space name.txt") -Value "Path with a space for copy/open path checks."
    Set-Content -LiteralPath (Join-Path $RootDir "ignored\old.tmp") -Value "This file should be hidden when the executable-local ignore list is enabled."
    Set-Content -LiteralPath (Join-Path $RootDir "nested\child-one.txt") -Value "nested child one"
    Set-Content -LiteralPath (Join-Path $RootDir "nested\child-two.log") -Value "nested child two"
    Set-Content -LiteralPath (Join-Path $RootDir "nested\FileList.txt") -Value "child-one.txt`nchild-two.log"
    Set-Content -LiteralPath (Join-Path $RootDir "FileList.txt") -Value @"
README-preview.txt
docs/alpha-report.md
docs/beta_notes.txt
actions/open-target.txt
actions/space name.txt
ignored/old.tmp
nested/FileList.txt
empty-dir
"@
    Set-Content -LiteralPath (Join-Path $DebugDir "flistwalker.ignore.txt") -Value "old.tmp`nignored"
}

function New-QuotedArgument {
    param([Parameter(Mandatory = $true)][string]$Value)
    return '"' + ($Value -replace '"', '\"') + '"'
}

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$RepoRoot = (Resolve-Path (Join-Path $ScriptDir "..")).Path
if ([string]::IsNullOrWhiteSpace($BaseDir)) {
    $BaseDir = Join-Path $RepoRoot "rust\target\gui-smoke"
}

$RootDir = Join-Path $BaseDir "root"
$EvidenceDir = Join-Path $BaseDir "evidence"
$DebugDir = Join-Path $RepoRoot "rust\target\debug"
$LogPath = Join-Path $EvidenceDir "gui-headful-smoke.log"
$ReportPath = Join-Path $EvidenceDir "GUI-HEADFUL-SMOKE.local.md"

New-GuiSmokeFixture -RootDir $RootDir -EvidenceDir $EvidenceDir -DebugDir $DebugDir

if (-not $NoBuild) {
    Push-Location (Join-Path $RepoRoot "rust")
    try {
        & cargo build --bin flistwalker
        if ($LASTEXITCODE -ne 0) {
            throw "cargo build failed with code $LASTEXITCODE"
        }
    }
    finally {
        Pop-Location
    }
}

$AppPath = Join-Path $DebugDir "flistwalker.exe"
if (-not (Test-Path -LiteralPath $AppPath)) {
    throw "Missing executable: $AppPath"
}

Set-Content -LiteralPath $LogPath -Value ""

$psi = [System.Diagnostics.ProcessStartInfo]::new()
$psi.FileName = $AppPath
$psi.Arguments = "--root " + (New-QuotedArgument -Value $RootDir) + " --limit 1000"
$psi.UseShellExecute = $false
$psi.RedirectStandardOutput = $true
$psi.RedirectStandardError = $true
$psi.Environment["FLISTWALKER_DISABLE_SELF_UPDATE"] = "1"
$psi.Environment["FLISTWALKER_RESTORE_TABS"] = "0"

$process = [System.Diagnostics.Process]::new()
$process.StartInfo = $psi
[void]$process.Start()

$status = "PASS"
$notes = "Process stayed alive for ${DurationSeconds}s and was stopped by the smoke script."
Start-Sleep -Seconds $DurationSeconds

if ($process.HasExited) {
    $status = "FAIL"
    $notes = "Process exited early with code $($process.ExitCode). See log: $LogPath"
}
else {
    $process.Kill()
    $process.WaitForExit()
}

$stdout = $process.StandardOutput.ReadToEnd()
$stderr = $process.StandardError.ReadToEnd()
Set-Content -LiteralPath $LogPath -Value ($stdout + $stderr)

Set-Content -LiteralPath $ReportPath -Value @"
# GUI HEADFUL SMOKE

## Summary
- Status: $status
- Duration: ${DurationSeconds}s
- Root: $RootDir
- Log: $LogPath

## Notes
- $notes
"@

Write-Host "GUI headful smoke: $status"
Write-Host "Report: $ReportPath"
Write-Host "Log: $LogPath"

if ($status -ne "PASS") {
    exit 1
}
