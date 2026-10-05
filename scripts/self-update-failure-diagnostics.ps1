# Failure-path observations for the copied-sandbox test only.
function Read-SelfUpdateBoundedJson {
    param([string]$Path)
    $stream = $null
    try {
        $attributes = [System.IO.File]::GetAttributes($Path)
        if (($attributes -band ([System.IO.FileAttributes]::ReparsePoint -bor [System.IO.FileAttributes]::Directory -bor [System.IO.FileAttributes]::Device)) -ne 0) {
            return @{ status = 'unsafe' }
        }
        $stream = [System.IO.File]::Open($Path, [System.IO.FileMode]::Open, [System.IO.FileAccess]::Read, [System.IO.FileShare]::ReadWrite)
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
