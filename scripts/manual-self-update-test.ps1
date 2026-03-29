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

    [switch]$CleanupSandbox
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
                    $target = Join-Path $ServeRoot $relative
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
if ($Port -le 0) {
    $Port = Get-FreeTcpPort
}
if (-not $RootPath) {
    $RootPath = Split-Path -Parent $AppPath
}

$AppSandboxDir = Join-Path $SandboxDir 'app'
$FeedDir = Join-Path $SandboxDir 'feed'
$SandboxExe = Join-Path $AppSandboxDir 'flistwalker.exe'
$AssetName = "FlistWalker-$FeedVersion-windows-x86_64.exe"
$AssetPath = Join-Path $FeedDir $AssetName
$ChecksumPath = Join-Path $FeedDir 'SHA256SUMS'
$ChecksumSigPath = Join-Path $FeedDir 'SHA256SUMS.sig'
$LatestJsonPath = Join-Path $FeedDir 'latest.json'
$FeedUrl = "http://127.0.0.1:$Port/latest.json"
$ReleaseUrl = "http://127.0.0.1:$Port/"

New-Item -ItemType Directory -Path $AppSandboxDir -Force | Out-Null
New-Item -ItemType Directory -Path $FeedDir -Force | Out-Null

Copy-Item -LiteralPath $AppPath -Destination $SandboxExe -Force
Copy-Item -LiteralPath $UpdateBinaryPath -Destination $AssetPath -Force

$AssetHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $AssetPath).Hash.ToLowerInvariant()
"$AssetHash  $AssetName" | Set-Content -LiteralPath $ChecksumPath -Encoding ASCII

if (-not $env:FLISTWALKER_UPDATE_SIGNING_KEY_HEX) {
    throw "FLISTWALKER_UPDATE_SIGNING_KEY_HEX is required for manual self-update tests."
}
cargo run --manifest-path (Join-Path (Split-Path -Parent $PSScriptRoot) 'rust\Cargo.toml') --quiet --bin sign_update_manifest -- $ChecksumPath $ChecksumSigPath
if ($LASTEXITCODE -ne 0) {
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
    $psi.Arguments = '--root ' + (New-QuotedArgument -Value $RootPath)
    $psi.EnvironmentVariables['FLISTWALKER_UPDATE_FEED_URL'] = $FeedUrl
    if ($Mode -eq 'SameVersion') {
        $psi.EnvironmentVariables['FLISTWALKER_UPDATE_ALLOW_SAME_VERSION'] = '1'
    }
    if ($Mode -eq 'Downgrade') {
        $psi.EnvironmentVariables['FLISTWALKER_UPDATE_ALLOW_DOWNGRADE'] = '1'
    }

    $process = [System.Diagnostics.Process]::Start($psi)

    Write-Host "Started manual self-update test."
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
    Write-Host '- 元の build 出力は変更されない'
    Write-Host ''
    Write-Host 'Close the launched app to stop the local feed server.'

    $process.WaitForExit()
}
finally {
    if ($job) {
        Stop-Job -Id $job.Id -ErrorAction SilentlyContinue | Out-Null
        Remove-Job -Id $job.Id -Force -ErrorAction SilentlyContinue | Out-Null
    }
    if ($CleanupSandbox -and (Test-Path -LiteralPath $SandboxDir)) {
        Remove-Item -LiteralPath $SandboxDir -Recurse -Force
    }
}
