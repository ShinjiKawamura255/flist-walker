param()
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot '../self-update-failure-diagnostics.ps1')
function Assert-LiveEqual($Actual, $Expected, [string]$Label) {
    if ($Actual -cne $Expected) { throw "Live diagnostic assertion failed: $Label" }
}
$root = Join-Path ([IO.Path]::GetTempPath()) ('fw-live-observation-' + [guid]::NewGuid().ToString('N'))
[void][IO.Directory]::CreateDirectory($root)
try {
    $id = '0123456789abcdef0123456789abcdef'
    $token = 'a' * 64
    $marker = Join-Path $root '.flistwalker-update.marker.json'
    $failure = Join-Path $root '.flistwalker-update-failure.json'
    $ack = Join-Path $root ('.flistwalker-update-' + $id + '.ack')
    $encoding = [Text.UTF8Encoding]::new($false)
    $h = New-SelfUpdateLiveHistory
    Add-SelfUpdateLiveSample $h (Get-SelfUpdateLiveSample $root) 0
    Assert-LiveEqual (Get-SelfUpdateLivePublicObservation $h).context_observed $false 'missing is not context'
    $document = @{ version = 1; transaction_id = $id; helper_pid = 42; helper_start_token = $token; phase = 'helper_registered'; helper_hash = 'PRIVATE_HASH_DO_NOT_LOG' }
    [IO.File]::WriteAllText($marker, ($document | ConvertTo-Json -Compress), $encoding)
    [IO.File]::WriteAllText($ack, ($id + "`n" + $token + "`n"), $encoding)
    $beforeMarker = [IO.File]::ReadAllText($marker)
    $sample = Get-SelfUpdateLiveSample $root
    Assert-LiveEqual $sample.ack_match $true 'actual matching bytes'
    Add-SelfUpdateLiveSample $h $sample 1
    Assert-LiveEqual ([IO.File]::ReadAllText($marker)) $beforeMarker 'observation is nonconsumptive'
    [IO.File]::Delete($ack)
    Add-SelfUpdateLiveSample $h (Get-SelfUpdateLiveSample $root) 2
    Assert-LiveEqual (Get-SelfUpdateLivePublicObservation $h).matching_ack_bytes_seen $true 'ACK history survives deletion'
    [IO.File]::WriteAllText($ack, ($id + "`n" + ('b' * 64) + "`n"), $encoding)
    Assert-LiveEqual (Get-SelfUpdateLiveSample $root).ack_match $false 'wrong token is not a match'
    $document.helper_start_token = 'b' * 64
    [IO.File]::WriteAllText($marker, ($document | ConvertTo-Json -Compress), $encoding)
    Add-SelfUpdateLiveSample $h (Get-SelfUpdateLiveSample $root) 3
    Assert-LiveEqual (Get-SelfUpdateLivePublicObservation $h).context_changed $true 'token identity change is not merged'
    $document.transaction_id = '1123456789abcdef0123456789abcdef'
    [IO.File]::WriteAllText($marker, ($document | ConvertTo-Json -Compress), $encoding)
    Add-SelfUpdateLiveSample $h (Get-SelfUpdateLiveSample $root) 4
    Assert-LiveEqual (Get-SelfUpdateLivePublicObservation $h).context_changed $true 'transaction identity change is not merged'
    $document.transaction_id = '../escape'
    [IO.File]::WriteAllText($marker, ($document | ConvertTo-Json -Compress), $encoding)
    Assert-LiveEqual (Get-SelfUpdateLiveSample $root).invalid $true 'invalid path identity rejected'
    $document.transaction_id = $id
    $document.helper_start_token = $token
    [IO.File]::WriteAllText($marker, ($document | ConvertTo-Json -Compress), $encoding)
    [IO.File]::WriteAllText($ack, ('x' * 257), $encoding)
    Assert-LiveEqual (Read-SelfUpdateLiveAck $ack).status 'oversize' 'ACK read bounded'
    [IO.File]::WriteAllText($failure, (@{ version = 1; transaction_id = $id; message = 'OpenProcess failed while waiting for PID 42: PRIVATE_ERROR_DO_NOT_LOG' } | ConvertTo-Json -Compress), $encoding)
    [IO.File]::WriteAllText($ack, ($id + "`n" + $token + "`n"), $encoding)
    $h = New-SelfUpdateLiveHistory
    $sample = Get-SelfUpdateLiveSample $root
    Add-SelfUpdateLiveSample $h $sample 5
    $public = Get-SelfUpdateLivePublicObservation $h
    Assert-LiveEqual $public.failure_category 'parent-wait-openprocess-error' 'finite existing-record category'
    $serialized = $public | ConvertTo-Json -Depth 8 -Compress
    foreach ($private in @($id, $token, $root, 'PRIVATE_HASH_DO_NOT_LOG', 'PRIVATE_ERROR_DO_NOT_LOG')) {
        if ($serialized.Contains($private)) { throw 'private live observation data escaped projection' }
    }
    for ($i = 0; $i -lt 205; $i++) { Add-SelfUpdateLiveSample $h $sample ($i + 6) }
    Assert-LiveEqual $h.samples 200 'hard sample count cap'
    Assert-LiveEqual $h.budget $true 'cap explicit'
    # Mutation-sensitive source guard ties the platform control to the actual
    # reader's opt-in flag and Open call, without injecting a production hook.
    $readerSource = ${function:Read-SelfUpdateBoundedJson}.ToString()
    function Test-LiveReaderShareContract([string]$Body) {
        $Body -match '(?s)if \(\$ShareDelete\) \{ \$share = \$share -bor \[System\.IO\.FileShare\]::Delete \}' -and
            $Body -match '\[System\.IO\.File\]::Open\(\$Path, \[System\.IO\.FileMode\]::Open, \[System\.IO\.FileAccess\]::Read, \$share\)'
    }
    Assert-LiveEqual (Test-LiveReaderShareContract $readerSource) $true 'actual reader uses opt-in Delete share'
    Assert-LiveEqual (Test-LiveReaderShareContract ($readerSource.Replace('::Delete', '::None'))) $false 'removing actual Delete flag is rejected'
    Assert-LiveEqual (Test-LiveReaderShareContract ($readerSource.Replace(', $share)', ', [System.IO.FileShare]::ReadWrite)'))) $false 'ignoring actual computed share is rejected'
    # Platform sharing control: a Delete-sharing diagnostic handle must not veto
    # cleanup. The real live reader uses exactly these flags and closes finally.
    $sharing = Join-Path $root 'sharing.json'
    [IO.File]::WriteAllText($sharing, '{}', $encoding)
    $held = [IO.File]::Open($sharing, [IO.FileMode]::Open, [IO.FileAccess]::Read, ([IO.FileShare]::ReadWrite -bor [IO.FileShare]::Delete))
    try {
        Assert-LiveEqual (Read-SelfUpdateBoundedJson -Path $sharing -ShareDelete).status 'parsed' 'actual live reader parses with held sharing'
        [IO.File]::Delete($sharing)
    }
    finally { $held.Dispose() }
    Assert-LiveEqual ([IO.File]::Exists($sharing)) $false 'sharing permits removal'
    $probe = [pscustomobject]@{ Checks = 0 }
    $probe | Add-Member -MemberType ScriptProperty -Name HasExited -Value { $this.Checks++; $this.Checks -ge 3 }
    $watched = Watch-SelfUpdateParentHistory $probe $root ([Diagnostics.Stopwatch]::StartNew())
    Assert-LiveEqual $watched.samples 1 'actual watcher stops on parent exit'
    $source = [IO.File]::ReadAllText((Join-Path $PSScriptRoot '../manual-self-update-test.ps1'))
    $tokens = $null; $errors = $null
    [void][Management.Automation.Language.Parser]::ParseInput($source, [ref]$tokens, [ref]$errors)
    if ($errors.Count -ne 0) { throw 'actual automated harness parser failed' }
    $begin = $source.IndexOf('    $liveObservation = $null', [StringComparison]::Ordinal)
    $end = $source.IndexOf('    $settleDeadline =', $begin, [StringComparison]::Ordinal)
    if ($begin -lt 0 -or $end -le $begin) { throw 'actual observation/wait/failure block missing' }
    $scriptsRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
    $block = [scriptblock]::Create('$PSScriptRoot = $ScriptsRoot' + [Environment]::NewLine + $source.Substring($begin, $end - $begin))
    foreach ($exitCode in @(0, 1)) {
        & {
            param($ActualBlock, $ExitCode, $Directory, $ScriptsRoot)
            function Watch-SelfUpdateParentHistory { throw 'PRIVATE_OBSERVER_ERROR' }
            $Automated = $true; $Variant = 'Universal'; $AppSandboxDir = $Directory
            $PSScriptRoot = $ScriptsRoot
            $process = [pscustomobject]@{ ExitCode = $ExitCode; Waited = $false }
            $process | Add-Member -MemberType ScriptMethod -Name WaitForExit -Value { $this.Waited = $true }
            $parentStopwatch = [Diagnostics.Stopwatch]::StartNew()
            $completion = [Threading.Tasks.TaskCompletionSource[string]]::new(); $completion.SetResult('ORIGINAL_PARENT_ERROR')
            $standardErrorTask = $completion.Task
            $caught = $null
            try { & $ActualBlock 6>&1 | Out-Null } catch { $caught = $_.Exception.Message }
            Assert-LiveEqual $process.Waited $true 'observer failure still executes original wait'
            if ($ExitCode -eq 1) { Assert-LiveEqual $caught 'headless update command failed with exit code 1: ORIGINAL_PARENT_ERROR' 'observer cannot mask original failure' }
            else { Assert-LiveEqual $caught $null 'observer cannot fail successful original result' }
        } $block $exitCode $root $scriptsRoot
    }
    Write-Host 'PASS: live ACK history, identity/token isolation, secret omission, bounded reads/counts, delete-sharing, parent-exit stop, actual wait/failure preservation'
}
finally { Remove-Item -LiteralPath $root -Recurse -Force }
