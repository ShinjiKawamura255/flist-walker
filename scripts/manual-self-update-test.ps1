[CmdletBinding()]
param(
    [ValidateSet('Universal', 'Fw')]
    [string]$Variant = 'Universal',

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
        $listenUri = [Uri]$ListenPrefix
        $listener = [System.Net.Sockets.TcpListener]::new(
            [System.Net.IPAddress]::Loopback,
            $listenUri.Port
        )
        $listener.Start()
        try {
            while ($true) {
                $client = $listener.AcceptTcpClient()
                $reader = $null
                $stream = $null
                $relative = $null
                try {
                    $stream = $client.GetStream()
                    $reader = [System.IO.StreamReader]::new(
                        $stream,
                        [System.Text.Encoding]::ASCII,
                        $false,
                        1024,
                        $true
                    )
                    $requestLine = $reader.ReadLine()
                    while (($headerLine = $reader.ReadLine()) -ne $null -and $headerLine.Length -gt 0) {}
                    $requestParts = @($requestLine -split ' ')
                    $status = '400 Bad Request'
                    $contentType = 'text/plain; charset=utf-8'
                    $payload = [System.Text.Encoding]::UTF8.GetBytes('invalid request')
                    if ($requestParts.Count -ge 2 -and $requestParts[0] -eq 'GET') {
                        try {
                            $requestUri = [Uri]::new($listenUri, $requestParts[1])
                            $relative = [Uri]::UnescapeDataString($requestUri.AbsolutePath.TrimStart('/'))
                            if ([string]::IsNullOrWhiteSpace($relative)) {
                                $relative = 'latest.json'
                            }
                            $target = [System.IO.Path]::GetFullPath((Join-Path $serveRootFull $relative))
                            if (-not $target.StartsWith($serveRootPrefix, [StringComparison]::OrdinalIgnoreCase)) {
                                $payload = [System.Text.Encoding]::UTF8.GetBytes('request escaped content root')
                            }
                            elseif (-not (Test-Path -LiteralPath $target -PathType Leaf)) {
                                $status = '404 Not Found'
                                $payload = [System.Text.Encoding]::UTF8.GetBytes("not found: $relative")
                            }
                            else {
                                $ext = [System.IO.Path]::GetExtension($target).ToLowerInvariant()
                                switch ($ext) {
                                    '.json' { $contentType = 'application/json; charset=utf-8' }
                                    '.txt' { $contentType = 'text/plain; charset=utf-8' }
                                    '.exe' { $contentType = 'application/octet-stream' }
                                    default { $contentType = 'application/octet-stream' }
                                }
                                $status = '200 OK'
                                $payload = [System.IO.File]::ReadAllBytes($target)
                            }
                        }
                        catch {}
                    }
                    $headers = "HTTP/1.1 $status`r`nContent-Type: $contentType`r`nContent-Length: $($payload.Length)`r`nConnection: close`r`n`r`n"
                    $headerBytes = [System.Text.Encoding]::ASCII.GetBytes($headers)
                    $stream.Write($headerBytes, 0, $headerBytes.Length)
                    $stream.Write($payload, 0, $payload.Length)
                    $stream.Flush()
                }
                finally {
                    if ($reader) { $reader.Dispose() }
                    if ($stream) { $stream.Dispose() }
                    $client.Dispose()
                }
            }
        }
        finally {
            $listener.Stop()
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
$DefaultExeCandidates = if ($Variant -eq 'Fw') {
    @((Join-Path $RepoDir 'rust\target\x86_64-pc-windows-gnu\release\fw.exe'))
} else {
    @(
        (Join-Path $RepoDir 'rust\target\x86_64-pc-windows-gnu\release\flistwalker.exe'),
        (Join-Path $RepoDir 'rust\target\x86_64-pc-windows-gnu\release\FlistWalker.exe')
    )
}

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
$SandboxBinaryName = if ($Variant -eq 'Fw') { 'fw.exe' } else { 'flistwalker.exe' }
$SandboxExe = Join-Path $AppSandboxDir $SandboxBinaryName
$CounterpartBinaryName = if ($Variant -eq 'Fw') { 'flistwalker.exe' } else { 'fw.exe' }
$CounterpartExe = Join-Path $AppSandboxDir $CounterpartBinaryName
$CounterpartSidecarPrefix = if ($Variant -eq 'Fw') { '' } else { 'fw.' }
$AssetName = if ($Variant -eq 'Fw') { "fw-$FeedVersion-windows-x86_64.exe" } else { "FlistWalker-$FeedVersion-windows-x86_64.exe" }
$OtherAssetName = if ($Variant -eq 'Fw') { "FlistWalker-$FeedVersion-windows-x86_64.exe" } else { "fw-$FeedVersion-windows-x86_64.exe" }
$AssetPath = Join-Path $FeedDir $AssetName
$OtherAssetPath = Join-Path $FeedDir $OtherAssetName
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
Copy-Item -LiteralPath $AppPath -Destination $CounterpartExe -Force
Copy-Item -LiteralPath $UpdateBinaryPath -Destination $AssetPath -Force
Copy-Item -LiteralPath $UpdateBinaryPath -Destination $OtherAssetPath -Force
# Regression guard: the mixed-family feed must contain two independently valid PE
# payloads with different hashes. Otherwise selecting the wrong family can pass the
# final installed-hash assertion and leave the asset discriminator untested.
$OtherAssetMarker = [System.Text.Encoding]::ASCII.GetBytes("FLISTWALKER_UPDATE_E2E_OTHER_FAMILY_${Variant}_V1")
$OtherAssetStream = [System.IO.File]::Open($OtherAssetPath, [System.IO.FileMode]::Append, [System.IO.FileAccess]::Write, [System.IO.FileShare]::None)
try {
    $OtherAssetStream.Write($OtherAssetMarker, 0, $OtherAssetMarker.Length)
    $OtherAssetStream.Flush($true)
}
finally {
    $OtherAssetStream.Dispose()
}

# Seed the non-target family in every fresh sandbox. Its binary and local sidecars
# are sentinels proving that a family-specific update never mutates its counterpart.
$CounterpartPaths = @($CounterpartExe)
foreach ($name in @('README.txt', 'LICENSE.txt', 'THIRD_PARTY_NOTICES.txt')) {
    $path = Join-Path $AppSandboxDir ($CounterpartSidecarPrefix + $name)
    Set-Content -LiteralPath $path -Value "counterpart family sentinel: $CounterpartBinaryName/$name" -Encoding ASCII
    $CounterpartPaths += $path
}
$CounterpartHashes = @{}
foreach ($path in $CounterpartPaths) {
    $CounterpartHashes[$path] = (Get-FileHash -Algorithm SHA256 -LiteralPath $path).Hash.ToLowerInvariant()
}
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
$OtherAssetHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $OtherAssetPath).Hash.ToLowerInvariant()
$MixedFamilyPayloadsAreDistinct = $AssetHash -ne $OtherAssetHash
if (-not $MixedFamilyPayloadsAreDistinct) {
    throw 'mixed-family updater payload discriminator requires different valid payload hashes'
}
$InitialSandboxHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $SandboxExe).Hash.ToLowerInvariant()
if ($Automated -and $InitialSandboxHash -eq $AssetHash) {
    throw 'automated update payload must differ from the initial sandbox binary'
}
$ReadmeHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $ReadmeAssetPath).Hash.ToLowerInvariant()
$LicenseHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $LicenseAssetPath).Hash.ToLowerInvariant()
$NoticesHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $NoticesAssetPath).Hash.ToLowerInvariant()
@(
    "$AssetHash  $AssetName"
    "$OtherAssetHash  $OtherAssetName"
    "$ReadmeHash  $ReadmeAssetName"
    "$LicenseHash  $LicenseAssetName"
    "$NoticesHash  $NoticesAssetName"
) | Set-Content -LiteralPath $ChecksumPath -Encoding ASCII

if (-not $env:FLISTWALKER_UPDATE_SIGNING_KEY_HEX) {
    throw "FLISTWALKER_UPDATE_SIGNING_KEY_HEX is required for manual self-update tests."
}
cargo run --manifest-path (Join-Path (Split-Path -Parent $PSScriptRoot) 'rust\Cargo.toml') --quiet --bin sign_update_manifest -- $ChecksumPath $ChecksumSigPath
$signExitCode = $LASTEXITCODE
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
            name = $OtherAssetName
            browser_download_url = "$ReleaseUrl$OtherAssetName"
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
    $serverReady = $false
    $serverReadyDeadline = [DateTime]::UtcNow.AddSeconds(5)
    do {
        $jobState = (Get-Job -Id $job.Id).State
        if ($jobState -ne 'Running') {
            $jobOutput = Receive-Job -Id $job.Id -Keep | Out-String
            throw "failed to start local feed server: $jobState`n$jobOutput"
        }
        try {
            $probe = Invoke-WebRequest -Uri $FeedUrl -UseBasicParsing -TimeoutSec 1
            $serverReady = $probe.StatusCode -eq 200
        }
        catch {
            Start-Sleep -Milliseconds 50
        }
    } while (-not $serverReady -and [DateTime]::UtcNow -lt $serverReadyDeadline)
    if (-not $serverReady) {
        throw 'loopback update feed did not become ready within 5 seconds'
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
        $psi.RedirectStandardError = $true
    }
    else {
        $psi.Arguments = '--root ' + (New-QuotedArgument -Value $RootPath)
    }
    $psi.EnvironmentVariables['LOCALAPPDATA'] = $LocalAppDataDir
    $psi.EnvironmentVariables['APPDATA'] = $RoamingAppDataDir
    $psi.EnvironmentVariables['USERPROFILE'] = $UserProfileDir
    $psi.EnvironmentVariables['FLISTWALKER_UPDATE_FEED_URL'] = $FeedUrl
    # Regression guard: consecutive Universal/Fw invocations share the caller process, so keep
    # its signing key intact. Remove it only from the sandbox child's copied environment; helper
    # and restarted descendants then inherit the same key-free boundary.
    [void]$psi.EnvironmentVariables.Remove('FLISTWALKER_UPDATE_SIGNING_KEY_HEX')
    if ($Mode -eq 'SameVersion') {
        $psi.EnvironmentVariables['FLISTWALKER_UPDATE_ALLOW_SAME_VERSION'] = '1'
    }
    if ($Mode -eq 'Downgrade') {
        $psi.EnvironmentVariables['FLISTWALKER_UPDATE_ALLOW_DOWNGRADE'] = '1'
    }

    $process = [System.Diagnostics.Process]::Start($psi)
    $standardErrorTask = if ($Automated) { $process.StandardError.ReadToEndAsync() } else { $null }

    Write-Host "Started sandbox self-update test."
    Write-Host "Mode: $Mode"
    Write-Host "Variant: $Variant"
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
    Write-Host "- Download and Restart を押すと sandbox 内の $SandboxBinaryName が置換されて再起動する"
    $LocalSidecarPrefix = if ($Variant -eq 'Fw') { 'fw.' } else { '' }
    Write-Host "- sandbox 内の ${LocalSidecarPrefix}README.txt / ${LocalSidecarPrefix}LICENSE.txt / ${LocalSidecarPrefix}THIRD_PARTY_NOTICES.txt も feed の sidecar へ更新される"
    Write-Host '- 元の build 出力は変更されない'
    Write-Host ''
    if (-not $Automated) {
        Write-Host 'Close the launched app to stop the local feed server.'
    }

    $process.WaitForExit()
    if ($Automated -and $process.ExitCode -ne 0) {
        $standardError = $standardErrorTask.GetAwaiter().GetResult().Trim()
        throw "headless update command failed with exit code $($process.ExitCode): $standardError"
    }

    $settleDeadline = [DateTime]::UtcNow.AddSeconds(45)
    do {
        $transactionArtifacts = @(Get-ChildItem -LiteralPath $AppSandboxDir -Force -ErrorAction SilentlyContinue |
            Where-Object { $_.Name -like '.flistwalker-update*' })
        $LocalSidecarPrefix = if ($Variant -eq 'Fw') { 'fw.' } else { '' }
        $sidecarsReady = (Test-Path -LiteralPath (Join-Path $AppSandboxDir "${LocalSidecarPrefix}README.txt") -PathType Leaf) -and
            (Test-Path -LiteralPath (Join-Path $AppSandboxDir "${LocalSidecarPrefix}LICENSE.txt") -PathType Leaf) -and
            (Test-Path -LiteralPath (Join-Path $AppSandboxDir "${LocalSidecarPrefix}THIRD_PARTY_NOTICES.txt") -PathType Leaf)
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
        foreach ($path in $CounterpartPaths) {
            $actualHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $path).Hash.ToLowerInvariant()
            if ($actualHash -ne $CounterpartHashes[$path]) {
                throw "counterpart family changed during $Variant update: $path"
            }
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
    if ($Automated -and @(Get-ProcessesForExecutablePath -ExecutablePath $SandboxExe).Count -ne 0) {
        throw "$Variant restarted process did not settle after headless update"
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
