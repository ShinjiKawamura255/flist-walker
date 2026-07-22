param(
    [string]$BaseDir = ""
)

$ErrorActionPreference = "Stop"

function Write-Utf8NoBom {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][string]$Content
    )

    [System.IO.File]::WriteAllText($Path, $Content, [System.Text.UTF8Encoding]::new($false))
}

function Invoke-CargoCapture {
    param(
        [Parameter(Mandatory = $true)][string]$RustDir,
        [Parameter(Mandatory = $true)][string[]]$Arguments,
        [Parameter(Mandatory = $true)][string]$LogPath
    )

    Push-Location $RustDir
    try {
        $priorColor = $env:CARGO_TERM_COLOR
        $priorErrorAction = $ErrorActionPreference
        $env:CARGO_TERM_COLOR = "never"
        $ErrorActionPreference = "Continue"
        try {
            $output = @(& cargo @Arguments 2>&1 | ForEach-Object { $_.ToString() })
            $exitCode = $LASTEXITCODE
        }
        finally {
            $ErrorActionPreference = $priorErrorAction
            if ($null -eq $priorColor) {
                Remove-Item Env:CARGO_TERM_COLOR -ErrorAction SilentlyContinue
            }
            else {
                $env:CARGO_TERM_COLOR = $priorColor
            }
        }
    }
    finally {
        Pop-Location
    }
    Write-Utf8NoBom -Path $LogPath -Content (($output -join "`n") + "`n")
    return [pscustomobject]@{ ExitCode = $exitCode; Lines = $output }
}

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$RepoRoot = (Resolve-Path (Join-Path $ScriptDir "..")).Path
$RustDir = Join-Path $RepoRoot "rust"
$InventoryPath = Join-Path $ScriptDir "gui-deterministic-scenarios.tsv"
if ([string]::IsNullOrWhiteSpace($BaseDir)) {
    $BaseDir = Join-Path $RustDir "target\gui-smoke"
}
New-Item -ItemType Directory -Force -Path $BaseDir | Out-Null
$BaseDir = (Resolve-Path $BaseDir).Path
$EvidenceDir = Join-Path $BaseDir "evidence"
New-Item -ItemType Directory -Force -Path $EvidenceDir | Out-Null

$header = [System.IO.File]::ReadLines($InventoryPath) | Select-Object -First 1
if ($header -ne "group`tfilter`tmin_discovered`tmin_executed`tskip") {
    throw "Invalid deterministic GUI inventory header"
}
$groups = @(Import-Csv -LiteralPath $InventoryPath -Delimiter "`t")
if ($groups.Count -eq 0) {
    throw "Deterministic GUI inventory has no groups"
}

$runId = (Get-Date).ToUniversalTime().ToString("yyyyMMddTHHmmssZ") + "-$PID"
$listLog = Join-Path $EvidenceDir "gui-deterministic-list-$runId.log"
$discovery = Invoke-CargoCapture -RustDir $RustDir -Arguments @("test", "--locked", "--lib", "--", "--list") -LogPath $listLog
if ($discovery.ExitCode -ne 0) {
    throw "Deterministic GUI test discovery failed; see $listLog"
}

$rows = [System.Collections.Generic.List[string]]::new()
$overall = "PASS"
$index = 0
foreach ($group in $groups) {
    $index++
    $minDiscovered = 0
    $minExecuted = 0
    if ([string]::IsNullOrWhiteSpace($group.group) -or
        -not [int]::TryParse($group.min_discovered, [ref]$minDiscovered) -or $minDiscovered -le 0 -or
        -not [int]::TryParse($group.min_executed, [ref]$minExecuted) -or $minExecuted -le 0) {
        throw "Invalid deterministic GUI inventory row: $($group.group)"
    }

    $discovered = @($discovery.Lines | Where-Object {
        $_.EndsWith(": test", [System.StringComparison]::Ordinal) -and
        $_.Contains($group.filter)
    }).Count
    if ($discovered -lt $minDiscovered) {
        $rows.Add("| $($group.group) | FAIL | $($group.filter) | $discovered | 0 | 0 | discovery under minimum $minDiscovered |")
        $overall = "FAIL"
        break
    }
    if ($group.skip -ne "-" -and -not ($discovery.Lines | Where-Object { $_.Contains($group.skip) })) {
        $rows.Add("| $($group.group) | FAIL | $($group.filter) | $discovered | 0 | 0 | explicit skip not discovered |")
        $overall = "FAIL"
        break
    }

    $groupLog = Join-Path $EvidenceDir ("gui-deterministic-{0:D2}-{1}-{2}.log" -f $index, $group.group, $runId)
    $arguments = @("test", "--locked", "--lib", $group.filter, "--")
    if ($group.skip -ne "-") {
        $arguments += @("--skip", $group.skip)
    }
    $run = Invoke-CargoCapture -RustDir $RustDir -Arguments $arguments -LogPath $groupLog
    $summary = [string]($run.Lines | Where-Object { $_ -match 'test result:' } | Select-Object -Last 1)
    $passed = if ($summary -match ' ([0-9]+) passed;') { [int]$Matches[1] } else { 0 }
    $ignored = if ($summary -match '; ([0-9]+) ignored;') { [int]$Matches[1] } else { 0 }
    if ($run.ExitCode -ne 0 -or $passed -lt $minExecuted -or $ignored -ne 0) {
        $rows.Add("| $($group.group) | FAIL | $($group.filter) | $discovered | $passed | $ignored | exit=$($run.ExitCode); log=$groupLog |")
        $overall = "FAIL"
        break
    }
    $rows.Add("| $($group.group) | PASS | $($group.filter) | $discovered | $passed | $ignored | $groupLog |")
}

$reportPath = Join-Path $EvidenceDir "GUI-DETERMINISTIC-$runId.local.md"
$reportLines = @(
    "# GUI DETERMINISTIC SCENARIOS",
    "",
    "## Summary",
    "- Status: $overall",
    "- Inventory: $InventoryPath",
    "- Discovery log: $listLog",
    "- Native interaction: NOT RUN (this harness is deterministic only)",
    "- Liveness: NOT RUN (use an isolated headful smoke script)",
    "",
    "## Groups",
    "| Group | Status | Filter | Discovered | Passed | Ignored | Evidence |",
    "| --- | --- | --- | ---: | ---: | ---: | --- |"
) + $rows
Write-Utf8NoBom -Path $reportPath -Content (($reportLines -join "`n") + "`n")

Write-Host "GUI deterministic scenarios: $overall"
Write-Host "Report: $reportPath"
if ($overall -ne "PASS") {
    exit 1
}
