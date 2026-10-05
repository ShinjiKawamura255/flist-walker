param()
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot '../self-update-failure-diagnostics.ps1')
function Assert-Equal($Actual, $Expected, [string]$Label) {
    if ($Actual -cne $Expected) { throw "Assertion failed: $Label" }
}
$root = Join-Path ([System.IO.Path]::GetTempPath()) ('fw-failure-observation-' + [guid]::NewGuid().ToString('N'))
[void][System.IO.Directory]::CreateDirectory($root)
try {
    $id = '0123456789abcdef0123456789abcdef'
    $failure = Join-Path $root '.flistwalker-update-failure.json'
    $marker = Join-Path $root '.flistwalker-update.marker.json'
    $ack = Join-Path $root ".flistwalker-update-$id.ack"
    $missing = Get-SelfUpdateFailureObservation $root 1 123.5
    Assert-Equal $missing.failure_record 'missing' 'missing record is explicitly unknown'
    Assert-Equal $missing.ack_current 'unknown' 'missing transaction identity is not ACK-absence proof'
    [System.IO.File]::WriteAllText($failure, (@{ version = 1; transaction_id = $id; message = 'parent process did not exit within 30 seconds' } | ConvertTo-Json -Compress))
    $before = [System.IO.File]::ReadAllText($failure)
    $rollback = Get-SelfUpdateFailureObservation $root 1 123.5
    Assert-Equal $rollback.failure_category 'parent-exit-timeout' 'existing post-ACK rollback record survives observation'
    Assert-Equal $rollback.ack_current 'absent' 'current absence never claims no historical ACK'
    Assert-Equal ([System.IO.File]::ReadAllText($failure)) $before 'record read is nonconsumptive'
    $secret = 'SECRET_TOKEN_ENV_PATH_DO_NOT_LOG'
    [System.IO.File]::WriteAllText($marker, (@{ version = 1; transaction_id = $id; phase = 'helper_registered'; helper_pid = 42; helper_start_token = $secret; helper_hash = $secret; binary_name = $secret } | ConvertTo-Json -Compress))
    [System.IO.File]::WriteAllText($ack, $secret)
    [System.IO.File]::WriteAllText($failure, (@{ version = 1; transaction_id = $id; message = $secret } | ConvertTo-Json -Compress))
    $valid = Get-SelfUpdateFailureObservation $root 1 123.5
    Assert-Equal $valid.marker_phase 'helper_registered' 'allowlisted phase'
    Assert-Equal $valid.helper_pid 42 'validated PID'
    Assert-Equal $valid.ack_current 'present' 'ACK existence observation'
    Assert-Equal $valid.failure_category 'other-recorded-error' 'raw messages never serialized'
    if (($valid | ConvertTo-Json -Compress) -like "*$secret*" -or ($valid | ConvertTo-Json -Compress) -like "*$root*") { throw 'untrusted secret/path escaped finite classification' }
    [System.IO.File]::WriteAllText($failure, ('x' * 16385))
    Assert-Equal (Get-SelfUpdateFailureObservation $root 1 123.5).failure_record 'oversize' 'bounded16KiB+1 read'
    [System.IO.File]::WriteAllText($failure, '{broken')
    Assert-Equal (Get-SelfUpdateFailureObservation $root 1 123.5).failure_record 'invalid' 'malformed JSON'
    [System.IO.File]::WriteAllText($failure, '{"version":2,"transaction_id":"../escape","message":"bad"}')
    Assert-Equal (Get-SelfUpdateFailureObservation $root 1 123.5).failure_record 'invalid' 'unsupported schema/unsafe identity'
    [System.IO.File]::Delete($failure)
    [System.IO.File]::WriteAllText($marker, '{"version":1,"transaction_id":"../escape","phase":"helper_registered","helper_pid":42}')
    Assert-Equal (Get-SelfUpdateFailureObservation $root 1 123.5).ack_current 'unknown' 'invalid identity cannot construct ACK path'
    [System.IO.File]::WriteAllText($marker, (@{ version = 1; transaction_id = ($id + "`n"); phase = 'helper_registered'; helper_pid = 42 } | ConvertTo-Json -Compress))
    Assert-Equal (Get-SelfUpdateFailureObservation $root 1 123.5).ack_current 'unknown' 'transaction ID must match all32 characters exactly'
    [System.IO.File]::Delete($marker)
    $target = Join-Path $root 'private-target.json'
    [System.IO.File]::WriteAllText($target, $secret)
    [void][System.IO.File]::CreateSymbolicLink($failure, $target)
    Assert-Equal (Get-SelfUpdateFailureObservation $root 1 123.5).failure_record 'unsafe' 'reparse target is not read'
    Assert-Equal ([System.IO.File]::ReadAllText($target)) $secret 'target unchanged'
    # Execute the real harness failure block without launching an application.
    $tokens = $null
    $errors = $null
    $scriptsRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
    $ast = [System.Management.Automation.Language.Parser]::ParseFile((Join-Path $scriptsRoot 'manual-self-update-test.ps1'), [ref]$tokens, [ref]$errors)
    if ($errors.Count -ne 0) { throw 'actual harness parser failed' }
    $blocks = @($ast.FindAll({ param($node)
        $node -is [System.Management.Automation.Language.IfStatementAst] -and
            $node.Extent.Text.StartsWith('if ($Automated -and $process.ExitCode -ne 0)')
    }, $true))
    Assert-Equal $blocks.Count 1 'unique actual nonzero branch'
    $block = [scriptblock]::Create('$PSScriptRoot = $ScriptsRoot' + [Environment]::NewLine + $blocks[0].Extent.Text)
    foreach ($case in @(@{ Code = 1; Directory = $root; Observation = 'ack_history' },
            @{ Code = 1; Directory = (Join-Path $root 'missing-app'); Observation = 'observation_error' },
            @{ Code = 0; Directory = (Join-Path $root 'missing-app'); Observation = $null })) {
        & {
            param($Block, $Case, $ScriptsRoot)
            $PSScriptRoot = $ScriptsRoot
            $Automated = $true
            $process = [pscustomobject]@{ ExitCode = $Case.Code }
            $completion = [System.Threading.Tasks.TaskCompletionSource[string]]::new()
            $completion.SetResult('ORIGINAL_PARENT_ERROR')
            $standardErrorTask = $completion.Task
            $AppSandboxDir = $Case.Directory
            $parentStopwatch = [System.Diagnostics.Stopwatch]::StartNew()
            $output = [System.Collections.Generic.List[string]]::new()
            $caught = $null
            try { & $Block 6>&1 | ForEach-Object { $output.Add($_.ToString()) } }
            catch { $caught = $_.Exception.Message }
            if ($Case.Code -eq 1) {
                Assert-Equal $caught 'headless update command failed with exit code 1: ORIGINAL_PARENT_ERROR' 'diagnostics cannot mask original throw'
                if (-not ($output -like "*$($Case.Observation)*")) { throw 'actual branch observation missing' }
            }
            else {
                Assert-Equal $caught $null 'successful branch does not run diagnostics'
                Assert-Equal $output.Count 0 'successful branch does not log/read'
            }
        } $block $case $scriptsRoot
    }
    Write-Host 'PASS: missing, rollback, secret omission, bounded read, malformed/schema, identity, reparse, nonconsumptive controls'
    Write-Host 'PASS: actual failure block preserves original throw on reader success/error; successful block has no observation'
}
finally {
    Remove-Item -LiteralPath $root -Recurse -Force
}
