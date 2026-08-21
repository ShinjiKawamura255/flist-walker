[CmdletBinding()]
param(
    [ValidateSet('SameVersion', 'Downgrade', 'Custom')]
    [string]$Mode = 'SameVersion',

    [string]$AppPath,

    [string]$UpdateBinaryPath,

    [string]$FeedVersion,

    [string]$RootPath,

    [int]$Port = 0,

    [string]$SandboxDir,

    [switch]$CleanupSandbox,

    [switch]$Automated
)

$ErrorActionPreference = 'Stop'

function Get-RepoVersion {
    param(
        [Parameter(Mandatory = $true)]
        [string]$CargoTomlPath
    )

    $line = Get-Content -LiteralPath $CargoTomlPath |
        Select-String -Pattern '^\s*version\s*=\s*"([^"]+)"' |
        Select-Object -First 1
    if (-not $line) {
        throw "version not found in $CargoTomlPath"
    }
    $line.Matches[0].Groups[1].Value
}

function Get-PreviousPatchVersion {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Version
    )

    $parts = $Version.Split('.')
    if ($parts.Length -ne 3) {
        throw "version must be semver-like: $Version"
    }
    $major = [int]$parts[0]
    $minor = [int]$parts[1]
    $patch = [int]$parts[2]
    if ($patch -le 0) {
        throw "cannot auto-compute downgrade target from patch version 0: $Version. Use -FeedVersion."
    }
    '{0}.{1}.{2}' -f $major, $minor, ($patch - 1)
}

function Get-FreeTcpPort {
    $listener = [System.Net.Sockets.TcpListener]::new([System.Net.IPAddress]::Loopback, 0)
    $listener.Start()
    try {
        ([System.Net.IPEndPoint]$listener.LocalEndpoint).Port
    }
    finally {
        $listener.Stop()
    }
}

function New-QuotedArgument {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Value
    )

    '"' + $Value.Replace('"', '\"') + '"'
}

function Start-StaticHttpServerJob {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Prefix,

        [Parameter(Mandatory = $true)]
        [string]$ContentRoot
    )

    $serverScript = {
        param($ListenPrefix, $ServeRoot)

        $ErrorActionPreference = 'Stop'
        $serveRootFull = [System.IO.Path]::GetFullPath($ServeRoot).TrimEnd('\', '/')
        $serveRootPrefix = $serveRootFull + [System.IO.Path]::DirectorySeparatorChar
        $listener = [System.Net.HttpListener]::new()
        $listener.Prefixes.Add($ListenPrefix)
        $listener.Start()
        try {
            while ($listener.IsListening) {
                $context = $listener.GetContext()
                try {
                    $relative = [Uri]::UnescapeDataString($context.Request.Url.AbsolutePath.TrimStart('/'))
                    if ([string]::IsNullOrWhiteSpace($relative)) {
                        $relative = 'latest.json'
                    }
                    $target = [System.IO.Path]::GetFullPath((Join-Path $serveRootFull $relative))
                    if (-not $target.StartsWith($serveRootPrefix, [StringComparison]::OrdinalIgnoreCase)) {
                        $context.Response.StatusCode = 400
                        $payload = [System.Text.Encoding]::UTF8.GetBytes('request escaped content root')
                        $context.Response.ContentType = 'text/plain; charset=utf-8'
                        $context.Response.OutputStream.Write($payload, 0, $payload.Length)
                        continue
                    }
                    if (-not (Test-Path -LiteralPath $target -PathType Leaf)) {
                        $context.Response.StatusCode = 404
                        $payload = [System.Text.Encoding]::UTF8.GetBytes("not found: $relative")
                        $context.Response.ContentType = 'text/plain; charset=utf-8'
                        $context.Response.OutputStream.Write($payload, 0, $payload.Length)
                        continue
                    }

                    $ext = [System.IO.Path]::GetExtension($target).ToLowerInvariant()
                    switch ($ext) {
                        '.json' { $contentType = 'application/json; charset=utf-8' }
                        '.txt' { $contentType = 'text/plain; charset=utf-8' }
                        '.exe' { $contentType = 'application/octet-stream' }
                        default { $contentType = 'application/octet-stream' }
                    }

                    $bytes = [System.IO.File]::ReadAllBytes($target)
                    $context.Response.StatusCode = 200
                    $context.Response.ContentType = $contentType
                    $context.Response.ContentLength64 = $bytes.Length
                    $context.Response.OutputStream.Write($bytes, 0, $bytes.Length)
                }
                finally {
                    $context.Response.OutputStream.Close()
                    $context.Response.Close()
                }
            }
        }
        finally {
            $listener.Stop()
            $listener.Close()
        }
    }

    Start-Job -ScriptBlock $serverScript -ArgumentList $Prefix, $ContentRoot
}

function Get-ProcessesForExecutablePath {
    param([Parameter(Mandatory = $true)][string]$ExecutablePath)
    $expected = [System.IO.Path]::GetFullPath($ExecutablePath)
    @(Get-Process -ErrorAction SilentlyContinue | ForEach-Object {
        try {
            if ([System.IO.Path]::GetFullPath($_.Path) -eq $expected) { $_ }
        }
        catch {}
    })
}

function Test-PathIsSameOrAncestor {
    param(
        [Parameter(Mandatory = $true)][string]$Candidate,
        [Parameter(Mandatory = $true)][string]$Path
    )
    $candidateFull = [System.IO.Path]::GetFullPath($Candidate).TrimEnd('\', '/')
    $pathFull = [System.IO.Path]::GetFullPath($Path).TrimEnd('\', '/')
    $pathFull -eq $candidateFull -or $pathFull.StartsWith($candidateFull + [System.IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)
}

function Assert-OwnedSandboxForCleanup {
    param(
        [Parameter(Mandatory = $true)][string]$SandboxPath,
        [Parameter(Mandatory = $true)][string]$SentinelPath,
        [Parameter(Mandatory = $true)][string]$ExpectedToken,
        [Parameter(Mandatory = $true)][string[]]$ProtectedPaths
    )
    $resolved = [System.IO.Path]::GetFullPath($SandboxPath)
    $root = [System.IO.Path]::GetPathRoot($resolved).TrimEnd('\', '/')
    if ($resolved.TrimEnd('\', '/') -eq $root) {
        throw "refusing cleanup of drive root: $resolved"
    }
    $sandboxItem = Get-Item -LiteralPath $resolved -Force
    if (($sandboxItem.Attributes -band [System.IO.FileAttributes]::ReparsePoint) -ne 0) {
        throw "refusing cleanup of reparse-point sandbox: $resolved"
    }
    if (-not (Test-Path -LiteralPath $SentinelPath -PathType Leaf)) {
        throw "sandbox ownership sentinel is missing: $SentinelPath"
    }
    if ((Get-Content -LiteralPath $SentinelPath -Raw).Trim() -ne $ExpectedToken) {
        throw "sandbox ownership sentinel does not match this run"
    }
    foreach ($protected in $ProtectedPaths) {
        if (Test-PathIsSameOrAncestor -Candidate $resolved -Path $protected) {
            throw "refusing cleanup because sandbox contains protected path: $protected"
        }
    }
    $allowed = @('.flistwalker-update-sandbox-owner', 'app', 'feed', 'profile')
    $entries = @(Get-ChildItem -LiteralPath $resolved -Force)
    $unexpected = @($entries | Where-Object { $_.Name -notin $allowed })
    if ($unexpected.Count -gt 0) {
        throw "refusing cleanup because sandbox contains unexpected entries: $($unexpected.Name -join ', ')"
    }
    # Walk owned descendants without following reparse-point directories. A nested junction or
    # symlink must make recursive cleanup fail closed just like a top-level one.
    $reparseEntries = @()
    $pending = [System.Collections.Generic.Stack[string]]::new()
    $pending.Push($resolved)
    while ($pending.Count -gt 0) {
        $current = $pending.Pop()
        foreach ($entry in @(Get-ChildItem -LiteralPath $current -Force)) {
            if (($entry.Attributes -band [System.IO.FileAttributes]::ReparsePoint) -ne 0) {
                $reparseEntries += $entry.FullName
            }
            elseif ($entry.PSIsContainer) {
                $pending.Push($entry.FullName)
            }
        }
    }
    if ($reparseEntries.Count -gt 0) {
        throw "refusing cleanup because sandbox contains reparse points: $($reparseEntries -join ', ')"
    }
    $resolved
}

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$RepoDir = Split-Path -Parent $ScriptDir
$CargoTomlPath = Join-Path $RepoDir 'rust\Cargo.toml'
$DefaultExeCandidates = @(
    (Join-Path $RepoDir 'rust\target\x86_64-pc-windows-gnu\release\flistwalker.exe'),
    (Join-Path $RepoDir 'rust\target\x86_64-pc-windows-gnu\release\FlistWalker.exe')
)

if (-not $AppPath) {
    $AppPath = $DefaultExeCandidates | Where-Object { Test-Path -LiteralPath $_ } | Select-Object -First 1
}
if (-not $AppPath) {
    throw "app exe not found. Build Windows binary first or pass -AppPath."
}
if (-not (Test-Path -LiteralPath $AppPath -PathType Leaf)) {
    throw "app exe not found: $AppPath"
}

if (-not $UpdateBinaryPath) {
    $UpdateBinaryPath = $AppPath
}
if (-not (Test-Path -LiteralPath $UpdateBinaryPath -PathType Leaf)) {
    throw "update binary not found: $UpdateBinaryPath"
}

$CurrentVersion = Get-RepoVersion -CargoTomlPath $CargoTomlPath
switch ($Mode) {
    'SameVersion' {
        if (-not $FeedVersion) {
            $FeedVersion = $CurrentVersion
        }
    }
    'Downgrade' {
        if (-not $FeedVersion) {
            $FeedVersion = Get-PreviousPatchVersion -Version $CurrentVersion
        }
    }
    'Custom' {
        if (-not $FeedVersion) {
            throw '-Mode Custom requires -FeedVersion.'
        }
    }
}

if (-not $SandboxDir) {
    $SandboxDir = Join-Path ([System.IO.Path]::GetTempPath()) ("flistwalker-update-manual-" + [guid]::NewGuid().ToString('N'))
}
$SandboxDir = [System.IO.Path]::GetFullPath($SandboxDir)
if (Test-Path -LiteralPath $SandboxDir) {
    throw "SandboxDir must not already exist: $SandboxDir"
}
if ($Port -le 0) {
    $Port = Get-FreeTcpPort
}

$AppSandboxDir = Join-Path $SandboxDir 'app'
$FeedDir = Join-Path $SandboxDir 'feed'
$ProfileDir = Join-Path $SandboxDir 'profile'
$LocalAppDataDir = Join-Path $ProfileDir 'LocalAppData'
$RoamingAppDataDir = Join-Path $ProfileDir 'RoamingAppData'
$UserProfileDir = Join-Path $ProfileDir 'UserProfile'
$SandboxExe = Join-Path $AppSandboxDir 'flistwalker.exe'
$AssetName = "FlistWalker-$FeedVersion-windows-x86_64.exe"
$AssetPath = Join-Path $FeedDir $AssetName
$ReadmeAssetName = "FlistWalker-$FeedVersion-windows-x86_64.README.txt"
$ReadmeAssetPath = Join-Path $FeedDir $ReadmeAssetName
$LicenseAssetName = "FlistWalker-$FeedVersion-windows-x86_64.LICENSE.txt"
$LicenseAssetPath = Join-Path $FeedDir $LicenseAssetName
$NoticesAssetName = "FlistWalker-$FeedVersion-windows-x86_64.THIRD_PARTY_NOTICES.txt"
$NoticesAssetPath = Join-Path $FeedDir $NoticesAssetName
$ChecksumPath = Join-Path $FeedDir 'SHA256SUMS'
$ChecksumSigPath = Join-Path $FeedDir 'SHA256SUMS.sig'
$LatestJsonPath = Join-Path $FeedDir 'latest.json'
$FeedUrl = "http://127.0.0.1:$Port/latest.json"
$ReleaseUrl = "http://127.0.0.1:$Port/"
$SandboxToken = [guid]::NewGuid().ToString('N')
$SandboxSentinel = Join-Path $SandboxDir '.flistwalker-update-sandbox-owner'

if (-not $RootPath) {
    $RootPath = $AppSandboxDir
}

New-Item -ItemType Directory -Path $SandboxDir | Out-Null
Set-Content -LiteralPath $SandboxSentinel -Value $SandboxToken -Encoding ASCII
New-Item -ItemType Directory -Path $AppSandboxDir | Out-Null
New-Item -ItemType Directory -Path $FeedDir -Force | Out-Null
New-Item -ItemType Directory -Path $LocalAppDataDir -Force | Out-Null
New-Item -ItemType Directory -Path $RoamingAppDataDir -Force | Out-Null
New-Item -ItemType Directory -Path $UserProfileDir -Force | Out-Null

Copy-Item -LiteralPath $AppPath -Destination $SandboxExe -Force
Copy-Item -LiteralPath $UpdateBinaryPath -Destination $AssetPath -Force
# Regression guard:
# This manual feed must mirror production self-update assets, including README/LICENSE/notices sidecars.
# Do not simplify/remove without updating the paired regression tests: docs/TESTPLAN.md manual self-update Regression Guard.
@"
FlistWalker v$FeedVersion

Manual self-update test sidecar.
"@ | Set-Content -LiteralPath $ReadmeAssetPath -Encoding UTF8
Set-Content -LiteralPath $LicenseAssetPath -Value "manual self-update license stub for v$FeedVersion" -Encoding UTF8
Set-Content -LiteralPath $NoticesAssetPath -Value "manual self-update notices stub for v$FeedVersion" -Encoding UTF8

$AssetHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $AssetPath).Hash.ToLowerInvariant()
$InitialSandboxHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $SandboxExe).Hash.ToLowerInvariant()
if ($Automated -and $InitialSandboxHash -eq $AssetHash) {
    throw 'automated update payload must differ from the initial sandbox binary'
}
$ReadmeHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $ReadmeAssetPath).Hash.ToLowerInvariant()
$LicenseHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $LicenseAssetPath).Hash.ToLowerInvariant()
$NoticesHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $NoticesAssetPath).Hash.ToLowerInvariant()
@(
    "$AssetHash  $AssetName"
    "$ReadmeHash  $ReadmeAssetName"
    "$LicenseHash  $LicenseAssetName"
    "$NoticesHash  $NoticesAssetName"
) | Set-Content -LiteralPath $ChecksumPath -Encoding ASCII

if (-not $env:FLISTWALKER_UPDATE_SIGNING_KEY_HEX) {
    throw "FLISTWALKER_UPDATE_SIGNING_KEY_HEX is required for manual self-update tests."
}
try {
    cargo run --manifest-path (Join-Path (Split-Path -Parent $PSScriptRoot) 'rust\Cargo.toml') --quiet --bin sign_update_manifest -- $ChecksumPath $ChecksumSigPath
    $signExitCode = $LASTEXITCODE
}
finally {
    # The signer is the only child that may receive signing material. In particular, keep the
    # sandbox application, updater helper, and restarted process from inheriting this variable.
    Remove-Item Env:FLISTWALKER_UPDATE_SIGNING_KEY_HEX -ErrorAction SilentlyContinue
}
if ($signExitCode -ne 0) {
    throw "failed to sign SHA256SUMS for manual self-update test"
}

$release = [ordered]@{
    tag_name = "v$FeedVersion"
    html_url = $ReleaseUrl
    assets = @(
        @{
            name = $AssetName
            browser_download_url = "$ReleaseUrl$AssetName"
        },
        @{
            name = $ReadmeAssetName
            browser_download_url = "$ReleaseUrl$ReadmeAssetName"
        },
        @{
            name = $LicenseAssetName
            browser_download_url = "$ReleaseUrl$LicenseAssetName"
        },
        @{
            name = $NoticesAssetName
            browser_download_url = "$ReleaseUrl$NoticesAssetName"
        },
        @{
            name = 'SHA256SUMS'
            browser_download_url = "${ReleaseUrl}SHA256SUMS"
        },
        @{
            name = 'SHA256SUMS.sig'
            browser_download_url = "${ReleaseUrl}SHA256SUMS.sig"
        }
    )
}
$release | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath $LatestJsonPath -Encoding UTF8

$job = $null
try {
    $job = Start-StaticHttpServerJob -Prefix $ReleaseUrl -ContentRoot $FeedDir
    Start-Sleep -Milliseconds 300
    $jobState = (Get-Job -Id $job.Id).State
    if ($jobState -ne 'Running') {
        $jobOutput = Receive-Job -Id $job.Id -Keep | Out-String
        throw "failed to start local feed server: $jobState`n$jobOutput"
    }

    $psi = [System.Diagnostics.ProcessStartInfo]::new()
    $psi.FileName = $SandboxExe
    $psi.WorkingDirectory = Split-Path -Parent $SandboxExe
    $psi.UseShellExecute = $false
    $psi.CreateNoWindow = $false
    $psi.WindowStyle = [System.Diagnostics.ProcessWindowStyle]::Normal
    if ($Automated) {
        $psi.Arguments = '--update'
        $psi.CreateNoWindow = $true
        $psi.WindowStyle = [System.Diagnostics.ProcessWindowStyle]::Hidden
    }
    else {
        $psi.Arguments = '--root ' + (New-QuotedArgument -Value $RootPath)
    }
    $psi.EnvironmentVariables['LOCALAPPDATA'] = $LocalAppDataDir
    $psi.EnvironmentVariables['APPDATA'] = $RoamingAppDataDir
    $psi.EnvironmentVariables['USERPROFILE'] = $UserProfileDir
    $psi.EnvironmentVariables['FLISTWALKER_UPDATE_FEED_URL'] = $FeedUrl
    if ($Mode -eq 'SameVersion') {
        $psi.EnvironmentVariables['FLISTWALKER_UPDATE_ALLOW_SAME_VERSION'] = '1'
    }
    if ($Mode -eq 'Downgrade') {
        $psi.EnvironmentVariables['FLISTWALKER_UPDATE_ALLOW_DOWNGRADE'] = '1'
    }

    $process = [System.Diagnostics.Process]::Start($psi)

    Write-Host "Started sandbox self-update test."
    Write-Host "Mode: $Mode"
    Write-Host "Current version: $CurrentVersion"
    Write-Host "Feed version: $FeedVersion"
    Write-Host "App under test: $SandboxExe"
    Write-Host "Root: $RootPath"
    Write-Host "Feed URL: $FeedUrl"
    Write-Host "Sandbox: $SandboxDir"
    Write-Host ''
    Write-Host 'Expected checks:'
    if ($Mode -eq 'SameVersion') {
        Write-Host '- 起動時に同一 version でも更新ダイアログが表示される'
    }
    elseif ($Mode -eq 'Downgrade') {
        Write-Host '- 起動時に downgrade 候補でも更新ダイアログが表示される'
    }
    else {
        Write-Host '- 起動時に指定 version を使った更新ダイアログが表示される'
    }
    Write-Host '- Download and Restart を押すと sandbox 内の flistwalker.exe が置換されて再起動する'
    Write-Host '- sandbox 内の README.txt / LICENSE.txt / THIRD_PARTY_NOTICES.txt も feed の sidecar へ更新される'
    Write-Host '- 元の build 出力は変更されない'
    Write-Host ''
    if (-not $Automated) {
        Write-Host 'Close the launched app to stop the local feed server.'
    }

    $process.WaitForExit()
    if ($Automated -and $process.ExitCode -ne 0) {
        throw "headless update command failed with exit code $($process.ExitCode)"
    }

    $settleDeadline = [DateTime]::UtcNow.AddSeconds(45)
    do {
        $transactionArtifacts = @(Get-ChildItem -LiteralPath $AppSandboxDir -Force -ErrorAction SilentlyContinue |
            Where-Object { $_.Name -like '.flistwalker-update*' })
        $sidecarsReady = (Test-Path -LiteralPath (Join-Path $AppSandboxDir 'README.txt') -PathType Leaf) -and
            (Test-Path -LiteralPath (Join-Path $AppSandboxDir 'LICENSE.txt') -PathType Leaf) -and
            (Test-Path -LiteralPath (Join-Path $AppSandboxDir 'THIRD_PARTY_NOTICES.txt') -PathType Leaf)
        if ($transactionArtifacts.Count -eq 0 -and ($sidecarsReady -or -not $Automated)) { break }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $settleDeadline)
    if ($transactionArtifacts.Count -ne 0) {
        throw "update transaction artifacts did not settle: $($transactionArtifacts.Name -join ', ')"
    }
    if ($Automated -and -not $sidecarsReady) {
        throw 'headless update did not install all sidecars'
    }
    if ($Automated) {
        $installedHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $SandboxExe).Hash.ToLowerInvariant()
        if ($installedHash -ne $AssetHash) {
            throw "installed sandbox binary hash mismatch: expected $AssetHash, got $installedHash"
        }
    }

    $restarted = @(Get-ProcessesForExecutablePath -ExecutablePath $SandboxExe)
    foreach ($child in $restarted) {
        if (-not $child.HasExited) {
            if ($Automated) {
                if (-not $child.WaitForExit(15000)) {
                    throw "restarted sandbox process did not exit: PID $($child.Id)"
                }
            }
            else {
                Write-Host "Waiting for restarted sandbox process PID $($child.Id) to close..."
                $child.WaitForExit()
            }
        }
    }
}
finally {
    if ($job) {
        Stop-Job -Id $job.Id -ErrorAction SilentlyContinue | Out-Null
        Remove-Job -Id $job.Id -Force -ErrorAction SilentlyContinue | Out-Null
    }
    if ($CleanupSandbox -and (Test-Path -LiteralPath $SandboxDir)) {
        $running = @(Get-ProcessesForExecutablePath -ExecutablePath $SandboxExe)
        if ($running.Count -gt 0) {
            Write-Warning "Sandbox retained because its executable is still running: $SandboxDir"
        }
        else {
            try {
                $cleanupPath = Assert-OwnedSandboxForCleanup -SandboxPath $SandboxDir -SentinelPath $SandboxSentinel -ExpectedToken $SandboxToken -ProtectedPaths @($RepoDir, $AppPath, $UpdateBinaryPath)
                Remove-Item -LiteralPath $cleanupPath -Recurse -Force
            }
            catch {
                Write-Warning "Sandbox cleanup skipped: $($_.Exception.Message). Retained: $SandboxDir"
            }
        }
    }
}
