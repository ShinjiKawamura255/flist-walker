# Failure-path observations for the copied-sandbox test only.
function Read-SelfUpdateBoundedJson {
    param([string]$Path, [switch]$ShareDelete)
    $stream = $null
    try {
        $attributes = [System.IO.File]::GetAttributes($Path)
        if (($attributes -band ([System.IO.FileAttributes]::ReparsePoint -bor [System.IO.FileAttributes]::Directory -bor [System.IO.FileAttributes]::Device)) -ne 0) {
            return @{ status = 'unsafe' }
        }
        $share = [System.IO.FileShare]::ReadWrite
        if ($ShareDelete) { $share = $share -bor [System.IO.FileShare]::Delete }
        $stream = [System.IO.File]::Open($Path, [System.IO.FileMode]::Open, [System.IO.FileAccess]::Read, $share)
        $buffer = [byte[]]::new(16385)
        $count = 0
        while ($count -lt $buffer.Length) {
            $read = $stream.Read($buffer, $count, $buffer.Length - $count)
            if ($read -eq 0) { break }
            $count += $read
        }
        if ($count -gt 16384) { return @{ status = 'oversize' } }
        try {
            $text = [System.Text.UTF8Encoding]::new($false, $true).GetString($buffer, 0, $count)
            $document = ConvertFrom-Json -InputObject $text -AsHashtable -Depth 32 -ErrorAction Stop
            if ($document -isnot [System.Collections.IDictionary]) { return @{ status = 'invalid' } }
            return @{ status = 'parsed'; document = $document }
        }
        catch { return @{ status = 'invalid' } }
    }
    catch [System.IO.FileNotFoundException] { return @{ status = 'missing' } }
    catch [System.IO.DirectoryNotFoundException] { return @{ status = 'missing' } }
    catch { return @{ status = 'read-error' } }
    finally { if ($stream) { $stream.Dispose() } }
}

function Test-SelfUpdateObservationIdentity($Document) {
    ($Document.version -is [long] -or $Document.version -is [int]) -and
        $Document.version -eq 1 -and $Document.transaction_id -is [string] -and
        $Document.transaction_id -cmatch '\A[0-9a-f]{32}\z'
}

function Get-SelfUpdateFailureObservation {
    param([string]$AppDirectory, [int]$ExitCode, [double]$ElapsedMilliseconds)
    $observation = [ordered]@{
        schema = 1; parent_exit_code = $ExitCode; parent_elapsed_ms = $ElapsedMilliseconds
        failure_record = 'unknown'; failure_category = 'unknown'
        marker = 'unknown'; marker_phase = 'unknown'; helper_pid = $null
        ack_current = 'unknown'; ack_history = 'unknown'
    }
    $directoryAttributes = [System.IO.File]::GetAttributes($AppDirectory)
    if (($directoryAttributes -band [System.IO.FileAttributes]::Directory) -eq 0 -or
        ($directoryAttributes -band [System.IO.FileAttributes]::ReparsePoint) -ne 0) {
        $observation.failure_record = 'unsafe'
        $observation.marker = 'unsafe'
        return $observation
    }
    $failure = Read-SelfUpdateBoundedJson (Join-Path $AppDirectory '.flistwalker-update-failure.json')
    $marker = Read-SelfUpdateBoundedJson (Join-Path $AppDirectory '.flistwalker-update.marker.json')
    $observation.failure_record = $failure.status
    $observation.marker = $marker.status
    $failureId = $null
    $markerId = $null
    if ($failure.status -eq 'parsed') {
        $document = $failure.document
        if ((Test-SelfUpdateObservationIdentity $document) -and $document.message -is [string] -and
            $document.message.Length -le 8192) {
            $observation.failure_record = 'valid'
            $failureId = $document.transaction_id
            # Only fixed categories reach logs. Never serialize the raw error/path/token.
            $observation.failure_category = if ($document.message -ceq 'parent process did not exit within 30 seconds') { 'parent-exit-timeout' }
                elseif ($document.message.StartsWith('update activation failed and was rolled back', [System.StringComparison]::Ordinal)) { 'activation-rollback' }
                elseif ($document.message.StartsWith('failed to restart updated application; old bundle restored', [System.StringComparison]::Ordinal)) { 'restart-rollback' }
                else { 'other-recorded-error' }
        }
        else { $observation.failure_record = 'invalid' }
    }
    if ($marker.status -eq 'parsed') {
        $document = $marker.document
        $phases = @('prepared_parent_owned', 'helper_registered', 'applying_sidecars', 'binary_intent', 'binary_committed', 'rolling_back', 'rolled_back')
        $pidValid = $null -eq $document.helper_pid -or
            (($document.helper_pid -is [long] -or $document.helper_pid -is [int]) -and
            $document.helper_pid -gt 0 -and $document.helper_pid -le [uint32]::MaxValue)
        if ((Test-SelfUpdateObservationIdentity $document) -and $document.phase -is [string] -and
            $phases -ccontains $document.phase -and $pidValid) {
            $observation.marker = 'valid'
            $markerId = $document.transaction_id
            $observation.marker_phase = $document.phase
            $observation.helper_pid = $document.helper_pid
        }
        else { $observation.marker = 'invalid' }
    }
    $id = if ($markerId) { $markerId } else { $failureId }
    if ($markerId -and $failureId -and $markerId -cne $failureId) { $id = $null }
    if ($id) {
        try {
            $attributes = [System.IO.File]::GetAttributes((Join-Path $AppDirectory ".flistwalker-update-$id.ack"))
            $observation.ack_current = if (($attributes -band ([System.IO.FileAttributes]::ReparsePoint -bor [System.IO.FileAttributes]::Directory -bor [System.IO.FileAttributes]::Device)) -ne 0) { 'unsafe' } else { 'present' }
        }
        catch [System.IO.FileNotFoundException] { $observation.ack_current = 'absent' }
        catch [System.IO.DirectoryNotFoundException] { $observation.ack_current = 'absent' }
        catch { $observation.ack_current = 'read-error' }
    }
    # This is a post-exit snapshot, never acknowledgement validation or historical proof.
    $observation
}


# Live diagnostics are confined to the automated copied sandbox. No trust or
# acknowledgement decision in the product depends on any of these observations.
function Read-SelfUpdateLiveAck {
    param([string]$Path)
    $stream = $null
    try {
        $attributes = [System.IO.File]::GetAttributes($Path)
        if (($attributes -band ([System.IO.FileAttributes]::ReparsePoint -bor [System.IO.FileAttributes]::Directory -bor [System.IO.FileAttributes]::Device)) -ne 0) { return @{ status = 'unsafe' } }
        $stream = [System.IO.File]::Open($Path, [System.IO.FileMode]::Open, [System.IO.FileAccess]::Read, ([System.IO.FileShare]::ReadWrite -bor [System.IO.FileShare]::Delete))
        $buffer = [byte[]]::new(257)
        $count = 0
        while ($count -lt $buffer.Length) {
            $read = $stream.Read($buffer, $count, $buffer.Length - $count)
            if ($read -eq 0) { break }
            $count += $read
        }
        if ($count -gt 256) { return @{ status = 'oversize' } }
        $text = [System.Text.UTF8Encoding]::new($false, $true).GetString($buffer, 0, $count)
        return @{ status = 'parsed'; text = $text }
    }
    catch [System.IO.FileNotFoundException] { return @{ status = 'missing' } }
    catch [System.IO.DirectoryNotFoundException] { return @{ status = 'missing' } }
    catch { return @{ status = 'read-error' } }
    finally { if ($stream) { $stream.Dispose() } }
}
function Get-SelfUpdateLiveSample {
    param([string]$AppDirectory)
    $sample = @{ id = $null; token_hash = $null; phase = $null; ack_match = $false; category = 'unknown'; invalid = $false }
    $attributes = [System.IO.File]::GetAttributes($AppDirectory)
    if (($attributes -band [System.IO.FileAttributes]::Directory) -eq 0 -or ($attributes -band [System.IO.FileAttributes]::ReparsePoint) -ne 0) { $sample.invalid = $true; return $sample }
    $marker = Read-SelfUpdateBoundedJson -Path (Join-Path $AppDirectory '.flistwalker-update.marker.json') -ShareDelete
    $failure = Read-SelfUpdateBoundedJson -Path (Join-Path $AppDirectory '.flistwalker-update-failure.json') -ShareDelete
    if ($marker.status -eq 'parsed') {
        $m = $marker.document
        $phases = @('prepared_parent_owned', 'helper_registered', 'applying_sidecars', 'binary_intent', 'binary_committed', 'rolling_back', 'rolled_back')
        if ((Test-SelfUpdateObservationIdentity $m) -and $m.phase -is [string] -and $phases -ccontains $m.phase) {
            $sample.id = $m.transaction_id
            $sample.phase = $m.phase
            if ($m.helper_start_token -is [string] -and $m.helper_start_token -cmatch '\A[0-9a-f]{64}\z' -and
                ($m.helper_pid -is [int] -or $m.helper_pid -is [long]) -and $m.helper_pid -gt 0 -and $m.helper_pid -le [uint32]::MaxValue) {
                $hash = [System.Security.Cryptography.SHA256]::Create()
                try { $sample.token_hash = [BitConverter]::ToString($hash.ComputeHash([System.Text.Encoding]::UTF8.GetBytes($m.helper_start_token))) }
                finally { $hash.Dispose() }
                $ack = Read-SelfUpdateLiveAck (Join-Path $AppDirectory ('.flistwalker-update-' + $sample.id + '.ack'))
                if ($ack.status -eq 'parsed') { $sample.ack_match = $ack.text -ceq ($sample.id + "`n" + $m.helper_start_token + "`n") }
                elseif ($ack.status -ne 'missing') { $sample.invalid = $true }
            }
            elseif ($m.phase -ne 'prepared_parent_owned') { $sample.invalid = $true }
        }
        else { $sample.invalid = $true }
    }
    elseif ($marker.status -ne 'missing') { $sample.invalid = $true }
    if ($failure.status -eq 'parsed') {
        $f = $failure.document
        if ((Test-SelfUpdateObservationIdentity $f) -and $f.message -is [string] -and $f.message.Length -le 8192) {
            if ($sample.id -and $sample.id -cne $f.transaction_id) { $sample.invalid = $true; return $sample }
            if (-not $sample.id) { $sample.id = $f.transaction_id }
            $sample.category = if ($f.message -ceq 'parent process did not exit within 30 seconds') { 'parent-exit-timeout' }
                elseif ($f.message.StartsWith('OpenProcess failed while waiting for PID ', [StringComparison]::Ordinal)) { 'parent-wait-openprocess-error' }
                elseif ($f.message.StartsWith('WaitForSingleObject failed with code ', [StringComparison]::Ordinal)) { 'parent-wait-win32-error' }
                elseif ($f.message.StartsWith('update activation failed and was rolled back', [StringComparison]::Ordinal)) { 'activation-rollback' }
                elseif ($f.message.StartsWith('failed to restart updated application; old bundle restored', [StringComparison]::Ordinal)) { 'restart-rollback' }
                else { 'other-recorded-error' }
        }
        else { $sample.invalid = $true }
    }
    elseif ($failure.status -ne 'missing') { $sample.invalid = $true }
    $sample
}
function New-SelfUpdateLiveHistory {
    @{ id = $null; token_hash = $null; changed = $false; samples = 0; errors = 0; ack_seen = $false; first_ack_ms = $null; last_ack_ms = $null; category = 'unknown'; category_ms = $null; phases = [ordered]@{}; budget = $false }
}
function Add-SelfUpdateLiveSample {
    param($History, $Sample, [double]$ElapsedMs)
    if ($History.samples -ge 200) { $History.budget = $true; return }
    $History.samples++
    if ([double]::IsNaN($ElapsedMs) -or [double]::IsInfinity($ElapsedMs) -or $ElapsedMs -lt 0 -or $Sample.invalid) { $History.errors++; return }
    if (-not $Sample.id) { return }
    if ($History.id -and $History.id -cne $Sample.id) { $History.changed = $true; return }
    if (-not $History.id) { $History.id = $Sample.id }
    if ($History.token_hash -and $Sample.token_hash -and $History.token_hash -cne $Sample.token_hash) { $History.changed = $true; return }
    if ($History.changed) { return }
    if (-not $History.token_hash -and $Sample.token_hash) { $History.token_hash = $Sample.token_hash }
    if ($Sample.phase) {
        if (-not $History.phases.Contains($Sample.phase)) { $History.phases[$Sample.phase] = @{ first_ms = $ElapsedMs; last_ms = $ElapsedMs } }
        else { $History.phases[$Sample.phase].last_ms = $ElapsedMs }
    }
    if ($Sample.ack_match) {
        $History.ack_seen = $true
        if ($null -eq $History.first_ack_ms) { $History.first_ack_ms = $ElapsedMs }
        $History.last_ack_ms = $ElapsedMs
    }
    if ($History.category -eq 'unknown' -and $Sample.category -ne 'unknown') { $History.category = $Sample.category; $History.category_ms = $ElapsedMs }
}
function Get-SelfUpdateLivePublicObservation {
    param($History)
    # Explicit projection: private identity/token hashes never enter console JSON.
    [ordered]@{ schema = 1; samples = $History.samples; read_errors = $History.errors; context_observed = [bool]$History.id; context_changed = $History.changed;
        matching_ack_bytes_seen = $History.ack_seen; first_ack_ms = $History.first_ack_ms; last_ack_ms = $History.last_ack_ms;
        failure_category = $History.category; failure_category_ms = $History.category_ms;
        marker_phases = @($History.phases.Keys | ForEach-Object { @{ phase = $_; first_ms = $History.phases[$_].first_ms; last_ms = $History.phases[$_].last_ms } });
        budget_expired = $History.budget; interpretation = 'sample-started-parent-live; nontransactional; negative-history-unknown; matching-bytes-not-helper-success' }
}
function Watch-SelfUpdateParentHistory {
    param($Process, [string]$AppDirectory, $Stopwatch)
    $history = New-SelfUpdateLiveHistory
    for ($attempt = 0; $attempt -lt 200; $attempt++) {
        if ($Stopwatch.Elapsed.TotalMilliseconds -ge 10000) { $history.budget = $true; break }
        if ($Process.HasExited) { break }
        $beforeCount = $history.samples
        try { $sample = Get-SelfUpdateLiveSample $AppDirectory; Add-SelfUpdateLiveSample $history $sample $Stopwatch.Elapsed.TotalMilliseconds }
        catch {
            $history.errors++
            if ($history.samples -eq $beforeCount -and $history.samples -lt 200) { $history.samples++ }
        }
        if ($Process.HasExited) { break }
        Start-Sleep -Milliseconds 50
    }
    if ($history.samples -ge 200) { $history.budget = $true }
    Get-SelfUpdateLivePublicObservation $history
}
