[CmdletBinding()]
param(
    [Parameter(ValueFromRemainingArguments = $true)]
    [string[]]$GuiArgs
)

$ErrorActionPreference = 'Stop'

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$RepoDir = Split-Path -Parent $ScriptDir
Set-Location -LiteralPath $RepoDir

$PythonExe = Join-Path $RepoDir '.venv\Scripts\python.exe'
if (-not (Test-Path -LiteralPath $PythonExe)) {
    $PythonExe = 'python'
}

try {
    & $PythonExe -c "import PySide6" *> $null
}
catch {
    Write-Error "PySide6 が見つかりません。先に 'pip install -e .[dev,gui]' を実行してください。"
    exit 1
}

$env:PYTHONPATH = (Join-Path $RepoDir 'src') + $(if ($env:PYTHONPATH) { ';' + $env:PYTHONPATH } else { '' })

& $PythonExe -m fast_file_finder --gui @GuiArgs
exit $LASTEXITCODE
