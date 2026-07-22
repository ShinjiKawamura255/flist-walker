param(
    [int]$DurationSeconds = 10,
    [string]$BaseDir = "",
    [switch]$NoBuild
)

$ErrorActionPreference = "Stop"

function Write-Utf8NoBom {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][AllowEmptyString()][string]$Content
    )

    $encoding = [System.Text.UTF8Encoding]::new($false)
    [System.IO.File]::WriteAllText($Path, $Content, $encoding)
}

function Get-SpecValue {
    param(
        [Parameter(Mandatory = $true)][string]$SpecPath,
        [Parameter(Mandatory = $true)][string]$Key
    )

    foreach ($line in [System.IO.File]::ReadAllLines($SpecPath)) {
        $parts = $line -split "`t", 2
        if ($parts.Count -eq 2 -and $parts[0] -eq $Key) {
            return [int]$parts[1]
        }
    }
    throw "Missing GUI fixture spec key: $Key"
}

function Assert-CanonicalFixture {
    param(
        [Parameter(Mandatory = $true)][string]$FixtureDir,
        [Parameter(Mandatory = $true)][string]$DestinationBase
    )

    $manifestPath = Join-Path $FixtureDir "SHA256SUMS"
    foreach ($line in [System.IO.File]::ReadAllLines($manifestPath)) {
        if ([string]::IsNullOrWhiteSpace($line)) {
            continue
        }
        if ($line -notmatch '^([0-9a-f]{64})  (.+)$') {
            throw "Invalid GUI fixture manifest line: $line"
        }
        $expectedHash = $Matches[1]
        $relativePath = $Matches[2].Replace([char]'/', [System.IO.Path]::DirectorySeparatorChar)
        $targetPath = Join-Path $DestinationBase $relativePath
        if (-not (Test-Path -LiteralPath $targetPath -PathType Leaf)) {
            throw "Missing GUI fixture file: $targetPath"
        }
        $actualHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $targetPath).Hash.ToLowerInvariant()
        if ($actualHash -ne $expectedHash) {
            throw "GUI fixture hash mismatch: $relativePath"
        }
    }

    $strictUtf8 = [System.Text.UTF8Encoding]::new($false, $true)
    $rootListPath = Join-Path $DestinationBase "root\FileList.txt"
    $nestedListPath = Join-Path $DestinationBase "root\nested\FileList.txt"
    $rootLines = $strictUtf8.GetString([System.IO.File]::ReadAllBytes($rootListPath)).Split("`n") |
        ForEach-Object { $_.TrimEnd("`r") } | Where-Object { $_.Length -gt 0 }
    $nestedLines = $strictUtf8.GetString([System.IO.File]::ReadAllBytes($nestedListPath)).Split("`n") |
        ForEach-Object { $_.TrimEnd("`r") } | Where-Object { $_.Length -gt 0 }
    $specPath = Join-Path $FixtureDir "fixture-spec.tsv"
    if ($rootLines.Count -ne (Get-SpecValue -SpecPath $specPath -Key "root_filelist_entries")) {
        throw "Root FileList entry count mismatch"
    }
    if ($nestedLines.Count -ne (Get-SpecValue -SpecPath $specPath -Key "nested_filelist_entries")) {
        throw "Nested FileList entry count mismatch"
    }
}

function Initialize-GuiFixture {
    param(
        [Parameter(Mandatory = $true)][string]$FixtureDir,
        [Parameter(Mandatory = $true)][string]$DestinationBase,
        [Parameter(Mandatory = $true)][string]$EvidenceDir,
        [Parameter(Mandatory = $true)][string]$ReportTemplate
    )

    $rootDir = Join-Path $DestinationBase "root"
    New-Item -ItemType Directory -Force -Path $rootDir, (Join-Path $rootDir "empty-dir"), $EvidenceDir | Out-Null
    Copy-Item -Path (Join-Path $FixtureDir "root\*") -Destination $rootDir -Recurse -Force
    Copy-Item -LiteralPath (Join-Path $FixtureDir "flistwalker.ignore.txt") `
        -Destination (Join-Path $DestinationBase "flistwalker.ignore.txt") -Force
    Assert-CanonicalFixture -FixtureDir $FixtureDir -DestinationBase $DestinationBase

    $localReport = Join-Path $EvidenceDir "GUI-TESTREPORT.local.md"
    if (-not (Test-Path -LiteralPath $localReport)) {
        Copy-Item -LiteralPath $ReportTemplate -Destination $localReport
    }
}

function Assert-StagedApp {
    param([Parameter(Mandatory = $true)][string]$AppDir)

    $expected = @("flistwalker.exe", "flistwalker.ignore.txt", "flistwalker.ignore.txt.example")
    $actual = @(Get-ChildItem -LiteralPath $AppDir -Force | Sort-Object Name | ForEach-Object { $_.Name })
    if ($actual.Count -ne $expected.Count -or (Compare-Object -ReferenceObject $expected -DifferenceObject $actual)) {
        throw "Staged app allowlist mismatch: $($actual -join ', ')"
    }
    if (@(Get-ChildItem -LiteralPath $AppDir -Force -Filter ".flistwalker-update*").Count -ne 0) {
        throw "Updater transaction artifact found in staged app directory"
    }
}

function Assert-PathWithinBase {
    param(
        [Parameter(Mandatory = $true)][string]$Base,
        [Parameter(Mandatory = $true)][string]$Candidate
    )

    $separator = [System.IO.Path]::DirectorySeparatorChar
    $baseFull = [System.IO.Path]::GetFullPath($Base).TrimEnd($separator) + $separator
    $candidateFull = [System.IO.Path]::GetFullPath($Candidate)
    if (-not $candidateFull.StartsWith($baseFull, [System.StringComparison]::OrdinalIgnoreCase)) {
        throw "Generated path escaped BaseDir: $candidateFull"
    }
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
New-Item -ItemType Directory -Force -Path $BaseDir | Out-Null
$BaseDir = (Resolve-Path $BaseDir).Path

$FixtureDir = Join-Path $RepoRoot "rust\tests\fixtures\gui-smoke"
$RootDir = Join-Path $BaseDir "root"
$EvidenceDir = Join-Path $BaseDir "evidence"
$DebugDir = Join-Path $RepoRoot "rust\target\debug"
$ReportTemplate = Join-Path $RepoRoot "docs\GUI-TESTREPORT.template.md"

Initialize-GuiFixture -FixtureDir $FixtureDir -DestinationBase $BaseDir `
    -EvidenceDir $EvidenceDir -ReportTemplate $ReportTemplate

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

$SourceAppPath = Join-Path $DebugDir "flistwalker.exe"
if (-not (Test-Path -LiteralPath $SourceAppPath -PathType Leaf)) {
    throw "Missing executable: $SourceAppPath"
}

$runId = (Get-Date).ToUniversalTime().ToString("yyyyMMddTHHmmssZ") + "-$PID-" + [Guid]::NewGuid().ToString("N").Substring(0, 8)
$RunDir = Join-Path $BaseDir "runs\$runId"
$AppDir = Join-Path $RunDir "app"
$ProfileDir = Join-Path $RunDir "profile"
$LocalProfileDir = Join-Path $ProfileDir "local"
$HomeProfileDir = Join-Path $ProfileDir "home"
$SettingsDir = Join-Path $LocalProfileDir "flistwalker"

New-Item -ItemType Directory -Path $RunDir | Out-Null
New-Item -ItemType Directory -Path $AppDir, $ProfileDir | Out-Null
New-Item -ItemType Directory -Path $LocalProfileDir, $HomeProfileDir | Out-Null
New-Item -ItemType Directory -Path $SettingsDir | Out-Null
foreach ($path in @($RunDir, $AppDir, $ProfileDir, $LocalProfileDir, $HomeProfileDir, $SettingsDir)) {
    Assert-PathWithinBase -Base $BaseDir -Candidate $path
}

$AppPath = Join-Path $AppDir "flistwalker.exe"
Copy-Item -LiteralPath $SourceAppPath -Destination $AppPath
Copy-Item -LiteralPath (Join-Path $BaseDir "flistwalker.ignore.txt") `
    -Destination (Join-Path $AppDir "flistwalker.ignore.txt")
Copy-Item -LiteralPath (Join-Path $RepoRoot "flistwalker.ignore.txt.example") `
    -Destination (Join-Path $AppDir "flistwalker.ignore.txt.example")
Assert-StagedApp -AppDir $AppDir

$runtimeConfig = @{
    history_persist_disabled = $true
    restore_tabs_enabled = $false
    disable_self_update = $true
} | ConvertTo-Json
Write-Utf8NoBom -Path (Join-Path $SettingsDir ".flistwalker_config.json") -Content $runtimeConfig
Write-Utf8NoBom -Path (Join-Path $SettingsDir ".flistwalker_ui_state.json") -Content "{}"
Write-Utf8NoBom -Path (Join-Path $SettingsDir ".flistwalker_roots.txt") -Content ""

$LogPath = Join-Path $EvidenceDir "gui-headful-smoke-$runId.log"
$ReportPath = Join-Path $EvidenceDir "GUI-HEADFUL-SMOKE-$runId.local.md"

$psi = [System.Diagnostics.ProcessStartInfo]::new()
$psi.FileName = $AppPath
$psi.Arguments = "--root " + (New-QuotedArgument -Value $RootDir) + " --limit 1000"
$psi.UseShellExecute = $false
$psi.RedirectStandardOutput = $true
$psi.RedirectStandardError = $true
$psi.Environment["LOCALAPPDATA"] = $LocalProfileDir
$psi.Environment["APPDATA"] = $LocalProfileDir
$psi.Environment["USERPROFILE"] = $HomeProfileDir
$psi.Environment["FLISTWALKER_DISABLE_SELF_UPDATE"] = "1"
$psi.Environment["FLISTWALKER_DISABLE_HISTORY_PERSIST"] = "1"
$psi.Environment["FLISTWALKER_RESTORE_TABS"] = "0"

$process = [System.Diagnostics.Process]::new()
$process.StartInfo = $psi
[void]$process.Start()
$stdoutTask = $process.StandardOutput.ReadToEndAsync()
$stderrTask = $process.StandardError.ReadToEndAsync()

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

$stdout = $stdoutTask.GetAwaiter().GetResult()
$stderr = $stderrTask.GetAwaiter().GetResult()
Write-Utf8NoBom -Path $LogPath -Content ($stdout + $stderr)
Assert-StagedApp -AppDir $AppDir

$report = @"
# GUI HEADFUL SMOKE

## Summary
- Status: $status
- Duration: ${DurationSeconds}s
- Root: $RootDir
- Staged executable: $AppPath
- Settings isolation: child LOCALAPPDATA/APPDATA/USERPROFILE point below $ProfileDir; current config/UI state/roots were preseeded
- Pre/post staged allowlist: PASS (binary + ignore + sample)
- Pre/post updater artifacts: PASS (no .flistwalker-update*)
- Fixture hash/FileList count: PASS
- Log: $LogPath

## Notes
- $notes
"@
Write-Utf8NoBom -Path $ReportPath -Content $report

Write-Host "GUI headful smoke: $status"
Write-Host "Report: $ReportPath"
Write-Host "Log: $LogPath"
Write-Host "Staged executable: $AppPath"

if ($status -ne "PASS") {
    exit 1
}
