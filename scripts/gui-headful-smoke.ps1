param(
    [int]$DurationSeconds = 10,
    [string]$BaseDir = "",
    [switch]$NoBuild,
    [switch]$ScriptedQueryProbe
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

function Initialize-NativeWindowInterop {
    if ("FlistWalker.GuiSmoke.NativeWindow" -as [type]) {
        return
    }

    Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;

namespace FlistWalker.GuiSmoke {
    public static class NativeWindow {
        private const uint WmNull = 0x0000;
        private const uint SmtoAbortIfHung = 0x0002;

        private delegate bool EnumWindowsProc(IntPtr hWnd, IntPtr lParam);

        [DllImport("user32.dll", SetLastError = true)]
        private static extern bool EnumWindows(EnumWindowsProc callback, IntPtr lParam);

        [DllImport("user32.dll", SetLastError = true)]
        private static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint processId);

        [DllImport("user32.dll")]
        private static extern bool IsWindowVisible(IntPtr hWnd);

        [DllImport("user32.dll", SetLastError = true)]
        private static extern IntPtr SendMessageTimeout(
            IntPtr hWnd,
            uint message,
            UIntPtr wParam,
            IntPtr lParam,
            uint flags,
            uint timeout,
            out UIntPtr result);

        public static IntPtr FindVisibleWindowForProcess(uint targetProcessId) {
            IntPtr result = IntPtr.Zero;
            EnumWindows((hWnd, _) => {
                uint processId;
                GetWindowThreadProcessId(hWnd, out processId);
                if (processId == targetProcessId && IsWindowVisible(hWnd)) {
                    result = hWnd;
                    return false;
                }
                return true;
            }, IntPtr.Zero);
            return result;
        }

        public static bool IsResponsive(IntPtr hWnd) {
            UIntPtr ignored;
            return SendMessageTimeout(
                hWnd,
                WmNull,
                UIntPtr.Zero,
                IntPtr.Zero,
                SmtoAbortIfHung,
                1000,
                out ignored) != IntPtr.Zero;
        }

    }
}
'@
}

function Wait-ForVisibleWindow {
    param(
        [Parameter(Mandatory = $true)][int]$ProcessId,
        [int]$TimeoutSeconds = 10
    )

    $deadline = [DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
    do {
        $window = [FlistWalker.GuiSmoke.NativeWindow]::FindVisibleWindowForProcess([uint32]$ProcessId)
        if ($window -ne [IntPtr]::Zero) {
            return $window
        }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $deadline)
    throw "Timed out waiting for a visible window owned by staged process $ProcessId"
}

function Wait-ForTraceLine {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][string]$ExpectedLine,
        [int]$TimeoutSeconds = 10
    )

    $deadline = [DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
    do {
        if (Test-Path -LiteralPath $Path) {
            $lines = [System.IO.File]::ReadAllLines($Path)
            if ($lines | Where-Object { $_.IndexOf($ExpectedLine, [System.StringComparison]::Ordinal) -ge 0 }) {
                return
            }
        }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $deadline)
    throw "Timed out waiting for staged GUI trace: $ExpectedLine"
}

function Invoke-ScriptedQueryProbe {
    param(
        [Parameter(Mandatory = $true)][System.Diagnostics.Process]$Process,
        [Parameter(Mandatory = $true)][string]$TracePath,
        [Parameter(Mandatory = $true)][string]$ExpectedTrace
    )

    Initialize-NativeWindowInterop
    $window = Wait-ForVisibleWindow -ProcessId $Process.Id
    if (-not [FlistWalker.GuiSmoke.NativeWindow]::IsResponsive($window)) {
        throw "Staged GUI window did not respond to WM_NULL"
    }

    Wait-ForTraceLine -Path $TracePath -ExpectedLine $ExpectedTrace
    return [pscustomobject]@{
        WindowHandle = $window
        ExpectedTrace = $ExpectedTrace
    }
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
$TracePath = Join-Path $RunDir "window-trace.log"
$scriptedQuery = if ($ScriptedQueryProbe) {
    "alpha " + [string][char]0x65E5 + [string][char]0x672C
} else {
    ""
}
$scriptedQueryTrace = "event=launch_query_initialized chars=$(([char[]]$scriptedQuery).Length) has_half_space=true has_full_space=false"

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
    window_trace_enabled = $true
    window_trace_path = $TracePath
} | ConvertTo-Json
Write-Utf8NoBom -Path (Join-Path $SettingsDir ".flistwalker_config.json") -Content $runtimeConfig
Write-Utf8NoBom -Path (Join-Path $SettingsDir ".flistwalker_ui_state.json") -Content "{}"
Write-Utf8NoBom -Path (Join-Path $SettingsDir ".flistwalker_roots.txt") -Content ""

$LogPath = Join-Path $EvidenceDir "gui-headful-smoke-$runId.log"
$ReportPath = Join-Path $EvidenceDir "GUI-HEADFUL-SMOKE-$runId.local.md"

$psi = [System.Diagnostics.ProcessStartInfo]::new()
$psi.FileName = $AppPath
$psi.Arguments = "--root " + (New-QuotedArgument -Value $RootDir) + " --limit 1000"
if ($ScriptedQueryProbe) {
    $psi.Arguments += " " + (New-QuotedArgument -Value $scriptedQuery)
}
$psi.UseShellExecute = $false
$psi.RedirectStandardOutput = $true
$psi.RedirectStandardError = $true
$psi.Environment["LOCALAPPDATA"] = $LocalProfileDir
$psi.Environment["APPDATA"] = $LocalProfileDir
$psi.Environment["USERPROFILE"] = $HomeProfileDir
$psi.Environment["FLISTWALKER_DISABLE_SELF_UPDATE"] = "1"
$psi.Environment["FLISTWALKER_DISABLE_HISTORY_PERSIST"] = "1"
$psi.Environment["FLISTWALKER_RESTORE_TABS"] = "0"
$psi.Environment["FLISTWALKER_WINDOW_TRACE"] = "1"
$psi.Environment["FLISTWALKER_WINDOW_TRACE_PATH"] = $TracePath

$process = [System.Diagnostics.Process]::new()
$process.StartInfo = $psi
[void]$process.Start()
$stdoutTask = $process.StandardOutput.ReadToEndAsync()
$stderrTask = $process.StandardError.ReadToEndAsync()

$status = "PASS"
$notes = "Process stayed alive for ${DurationSeconds}s and was stopped by the smoke script."
$scriptedQueryStatus = "NOT RUN"
$scriptedQueryNotes = "Scripted Unicode query probe was not requested."
if ($ScriptedQueryProbe) {
    try {
        $scriptedQueryResult = Invoke-ScriptedQueryProbe -Process $process -TracePath $TracePath -ExpectedTrace $scriptedQueryTrace
        $scriptedQueryStatus = "PASS"
        $scriptedQueryNotes = "Visible PID-owned window responded to WM_NULL; launch query metadata matched $($scriptedQueryResult.ExpectedTrace) (handle=$($scriptedQueryResult.WindowHandle))."
    }
    catch {
        $status = "FAIL"
        $scriptedQueryStatus = "FAIL"
        $scriptedQueryNotes = $_.Exception.Message
        $notes = "Scripted Unicode query probe failed: $scriptedQueryNotes"
    }
}
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
- Window trace: $TracePath
- Scripted Unicode query probe: $scriptedQueryStatus

## Notes
- $notes
- $scriptedQueryNotes
"@
Write-Utf8NoBom -Path $ReportPath -Content $report

Write-Host "GUI headful smoke: $status"
Write-Host "Report: $ReportPath"
Write-Host "Log: $LogPath"
Write-Host "Staged executable: $AppPath"

if ($status -ne "PASS") {
    exit 1
}
