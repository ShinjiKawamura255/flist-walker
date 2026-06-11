$ErrorActionPreference = 'Stop'

function Get-FlistWalkerInstallMode {
    param(
        [switch]$CheckOnly,
        [switch]$NoInstall,
        [switch]$InstallMissing
    )

    if ($NoInstall -and $InstallMissing) {
        throw '-NoInstall and -InstallMissing cannot be used together.'
    }
    if ($CheckOnly) {
        return 'CheckOnly'
    }
    if ($NoInstall) {
        return 'NoInstall'
    }
    if ($InstallMissing) {
        return 'InstallMissing'
    }
    return 'Interactive'
}

function Get-FlistWalkerDependencyInstallPlan {
    param(
        [Parameter(Mandatory = $true)]
        [ValidateSet('Rustup', 'RustTarget', 'MSYS2', 'GnuTools')]
        [string]$Dependency,

        [string]$RustupPath = 'rustup.exe',
        [string]$PacmanPath = 'C:\msys64\usr\bin\pacman.exe'
    )

    switch ($Dependency) {
        'Rustup' {
            return [pscustomobject]@{
                Name = 'Rustup'
                FilePath = 'winget.exe'
                ArgumentList = @(
                    'install', '--exact', '--id', 'Rustlang.Rustup', '--source', 'winget',
                    '--accept-package-agreements', '--accept-source-agreements'
                )
                ManualCommand = 'winget install --exact --id Rustlang.Rustup --source winget'
                Description = 'Rustup (the installer may request administrator approval)'
            }
        }
        'RustTarget' {
            return [pscustomobject]@{
                Name = 'Rust GNU target'
                FilePath = $RustupPath
                ArgumentList = @('target', 'add', 'x86_64-pc-windows-gnu')
                ManualCommand = 'rustup target add x86_64-pc-windows-gnu'
                Description = 'Rust target x86_64-pc-windows-gnu'
            }
        }
        'MSYS2' {
            return [pscustomobject]@{
                Name = 'MSYS2'
                FilePath = 'winget.exe'
                ArgumentList = @(
                    'install', '--exact', '--id', 'MSYS2.MSYS2', '--source', 'winget',
                    '--accept-package-agreements', '--accept-source-agreements'
                )
                ManualCommand = 'winget install --exact --id MSYS2.MSYS2 --source winget'
                Description = 'MSYS2 (the installer may request administrator approval)'
            }
        }
        'GnuTools' {
            return [pscustomobject]@{
                Name = 'MSYS2 MinGW GNU tools'
                FilePath = $PacmanPath
                ArgumentList = @('-S', '--needed', '--noconfirm', 'mingw-w64-x86_64-gcc')
                ManualCommand = "$PacmanPath -S --needed mingw-w64-x86_64-gcc"
                Description = 'MSYS2 package mingw-w64-x86_64-gcc and its binutils dependencies'
            }
        }
    }
}

function Invoke-FlistWalkerChecked {
    param(
        [Parameter(Mandatory = $true)][string]$FilePath,
        [string[]]$ArgumentList = @()
    )

    & $FilePath @ArgumentList
    if ($LASTEXITCODE -ne 0) {
        $commandLine = @($FilePath) + $ArgumentList
        throw "Command failed with exit code ${LASTEXITCODE}: $($commandLine -join ' ')"
    }
}

function Remove-FlistWalkerBuildArtifact {
    param(
        [Parameter(Mandatory = $true)][string]$Path
    )

    if (-not (Test-Path -LiteralPath $Path)) {
        return
    }
    Remove-Item -LiteralPath $Path -Force -ErrorAction Stop
    if (Test-Path -LiteralPath $Path) {
        throw "Failed to remove stale build artifact: $Path"
    }
}

function Add-FlistWalkerProcessPath {
    param([string[]]$Paths)

    $items = New-Object System.Collections.Generic.List[string]
    foreach ($path in @($Paths) + @(
        $env:PATH,
        [Environment]::GetEnvironmentVariable('Path', 'User'),
        [Environment]::GetEnvironmentVariable('Path', 'Machine')
    )) {
        if (-not $path) {
            continue
        }
        foreach ($item in ($path -split ';')) {
            if ($item -and -not $items.Contains($item)) {
                $items.Add($item)
            }
        }
    }
    $env:PATH = $items -join ';'
}

function Get-FlistWalkerMsys2Root {
    if ($env:FLISTWALKER_MSYS2_ROOT) {
        return $env:FLISTWALKER_MSYS2_ROOT
    }
    return 'C:\msys64'
}

function Refresh-FlistWalkerProcessPath {
    $cargoHome = $env:CARGO_HOME
    if (-not $cargoHome) {
        $cargoHome = Join-Path $env:USERPROFILE '.cargo'
    }
    $msys2Root = Get-FlistWalkerMsys2Root
    Add-FlistWalkerProcessPath @(
        (Join-Path $cargoHome 'bin'),
        (Join-Path $msys2Root 'mingw64\bin')
    )
}

function Resolve-FlistWalkerCommand {
    param(
        [Parameter(Mandatory = $true)][string[]]$Names,
        [string[]]$AbsoluteCandidates = @()
    )

    foreach ($candidate in $AbsoluteCandidates) {
        if ($candidate -and (Test-Path -LiteralPath $candidate -PathType Leaf)) {
            return (Resolve-Path -LiteralPath $candidate).Path
        }
    }
    foreach ($name in $Names) {
        $command = Get-Command $name -ErrorAction SilentlyContinue
        if ($command) {
            return $command.Source
        }
    }
    return $null
}

function Resolve-FlistWalkerGnuTool {
    param(
        [Parameter(Mandatory = $true)][string]$EnvName,
        [Parameter(Mandatory = $true)][string[]]$ToolNames,
        [Parameter(Mandatory = $true)][string]$MingwBin
    )

    $override = [Environment]::GetEnvironmentVariable($EnvName)
    if ($override) {
        if (-not (Test-Path -LiteralPath $override -PathType Leaf)) {
            throw "$EnvName points to a missing file: $override"
        }
        return (Resolve-Path -LiteralPath $override).Path
    }

    $fixedCandidates = @()
    foreach ($name in $ToolNames) {
        $fixedCandidates += Join-Path $MingwBin $name
    }
    return Resolve-FlistWalkerCommand -Names $ToolNames -AbsoluteCandidates $fixedCandidates
}

function Test-FlistWalkerCanPrompt {
    if (-not [Environment]::UserInteractive) {
        return $false
    }
    try {
        if ([Console]::IsInputRedirected) {
            return $false
        }
    } catch {
        return $false
    }
    return $true
}

function Confirm-FlistWalkerInstall {
    param(
        [Parameter(Mandatory = $true)]$Plan,
        [Parameter(Mandatory = $true)][string]$Mode
    )

    Write-Warning "Missing dependency: $($Plan.Name)"
    Write-Host "Install action: $($Plan.Description)"
    Write-Host "Command: $($Plan.FilePath) $($Plan.ArgumentList -join ' ')"

    if ($Mode -eq 'InstallMissing') {
        return $true
    }
    if ($Mode -eq 'CheckOnly' -or $Mode -eq 'NoInstall') {
        Write-Host "Manual install: $($Plan.ManualCommand)"
        return $false
    }
    if (-not (Test-FlistWalkerCanPrompt)) {
        Write-Host "Input is not interactive. Manual install: $($Plan.ManualCommand)"
        return $false
    }

    $answer = Read-Host "Install $($Plan.Name) now? [y/N]"
    return $answer -match '^(?i:y|yes)$'
}

function Install-FlistWalkerDependency {
    param(
        [Parameter(Mandatory = $true)]$Plan,
        [Parameter(Mandatory = $true)][string]$Mode
    )

    if (-not (Confirm-FlistWalkerInstall -Plan $Plan -Mode $Mode)) {
        throw "Required dependency was not installed: $($Plan.Name). Manual command: $($Plan.ManualCommand)"
    }
    $installer = Resolve-FlistWalkerCommand -Names @($Plan.FilePath)
    if (-not $installer) {
        throw "Installer was not found: $($Plan.FilePath). Manual command: $($Plan.ManualCommand)"
    }
    Invoke-FlistWalkerChecked -FilePath $installer -ArgumentList $Plan.ArgumentList
    Write-Host "Installed dependency: $($Plan.Name)"
    Refresh-FlistWalkerProcessPath
}

function Invoke-FlistWalkerDependencyCheck {
    $environmentNames = @(
        'PATH',
        'FLISTWALKER_WINDOWS_LINKER',
        'FLISTWALKER_WINDOWS_CXX',
        'FLISTWALKER_WINDOWS_AR',
        'FLISTWALKER_WINDOWS_RANLIB',
        'FLISTWALKER_WINDOWS_WINDRES',
        'FLISTWALKER_WINDOWS_STRIP',
        'CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER',
        'CC_x86_64_pc_windows_gnu',
        'CXX_x86_64_pc_windows_gnu',
        'AR_x86_64_pc_windows_gnu',
        'RANLIB_x86_64_pc_windows_gnu'
    )
    $snapshot = @{}
    foreach ($name in $environmentNames) {
        $snapshot[$name] = [Environment]::GetEnvironmentVariable($name, 'Process')
    }

    try {
        return Initialize-FlistWalkerWindowsGnu -Mode 'CheckOnly'
    } finally {
        foreach ($name in $environmentNames) {
            if ($null -eq $snapshot[$name]) {
                Remove-Item -LiteralPath "Env:$name" -ErrorAction SilentlyContinue
            } else {
                [Environment]::SetEnvironmentVariable($name, $snapshot[$name], 'Process')
            }
        }
    }
}

function Test-FlistWalkerRustTarget {
    param(
        [Parameter(Mandatory = $true)][string]$RustupPath,
        [Parameter(Mandatory = $true)][string]$Target
    )

    $installed = & $RustupPath target list --installed
    if ($LASTEXITCODE -ne 0) {
        throw 'Failed to query installed Rust targets.'
    }
    return $installed -contains $Target
}

function Install-FlistWalkerRustTarget {
    param(
        [Parameter(Mandatory = $true)][string]$RustupPath,
        [Parameter(Mandatory = $true)][string]$Target,
        [Parameter(Mandatory = $true)][string]$Mode
    )

    $plan = Get-FlistWalkerDependencyInstallPlan -Dependency RustTarget -RustupPath $RustupPath
    Install-FlistWalkerDependency -Plan $plan -Mode $Mode
    if (-not (Test-FlistWalkerRustTarget -RustupPath $RustupPath -Target $Target)) {
        throw "Rust target was not found after installation: $Target"
    }
}

function Set-FlistWalkerGnuEnvironment {
    param([Parameter(Mandatory = $true)]$Tools)

    $env:FLISTWALKER_WINDOWS_LINKER = $Tools.Linker
    $env:FLISTWALKER_WINDOWS_CXX = $Tools.Cxx
    $env:FLISTWALKER_WINDOWS_AR = $Tools.Ar
    $env:FLISTWALKER_WINDOWS_RANLIB = $Tools.Ranlib
    $env:FLISTWALKER_WINDOWS_WINDRES = $Tools.Windres
    $env:FLISTWALKER_WINDOWS_STRIP = $Tools.Strip
    $env:CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER = $Tools.Linker
    $env:CC_x86_64_pc_windows_gnu = $Tools.Linker
    $env:CXX_x86_64_pc_windows_gnu = $Tools.Cxx
    $env:AR_x86_64_pc_windows_gnu = $Tools.Ar
    $env:RANLIB_x86_64_pc_windows_gnu = $Tools.Ranlib
}

function Initialize-FlistWalkerWindowsGnu {
    param(
        [Parameter(Mandatory = $true)][string]$Mode
    )

    Refresh-FlistWalkerProcessPath
    $cargoHome = $env:CARGO_HOME
    if (-not $cargoHome) {
        $cargoHome = Join-Path $env:USERPROFILE '.cargo'
    }
    $cargoBin = Join-Path $cargoHome 'bin'

    $cargo = Resolve-FlistWalkerCommand -Names @('cargo.exe', 'cargo') -AbsoluteCandidates @((Join-Path $cargoBin 'cargo.exe'))
    $rustc = Resolve-FlistWalkerCommand -Names @('rustc.exe', 'rustc') -AbsoluteCandidates @((Join-Path $cargoBin 'rustc.exe'))
    $rustup = Resolve-FlistWalkerCommand -Names @('rustup.exe', 'rustup') -AbsoluteCandidates @((Join-Path $cargoBin 'rustup.exe'))
    if (-not $cargo -or -not $rustc -or -not $rustup) {
        $plan = Get-FlistWalkerDependencyInstallPlan -Dependency Rustup
        Install-FlistWalkerDependency -Plan $plan -Mode $Mode
        $cargo = Resolve-FlistWalkerCommand -Names @('cargo.exe', 'cargo') -AbsoluteCandidates @((Join-Path $cargoBin 'cargo.exe'))
        $rustc = Resolve-FlistWalkerCommand -Names @('rustc.exe', 'rustc') -AbsoluteCandidates @((Join-Path $cargoBin 'rustc.exe'))
        $rustup = Resolve-FlistWalkerCommand -Names @('rustup.exe', 'rustup') -AbsoluteCandidates @((Join-Path $cargoBin 'rustup.exe'))
    }
    if (-not $cargo -or -not $rustc -or -not $rustup) {
        throw 'Rust tools were not found after installation. Open a new PowerShell window and run the script again.'
    }

    $target = 'x86_64-pc-windows-gnu'
    if (-not (Test-FlistWalkerRustTarget -RustupPath $rustup -Target $target)) {
        Install-FlistWalkerRustTarget -RustupPath $rustup -Target $target -Mode $Mode
    }

    $msys2Root = Get-FlistWalkerMsys2Root
    $mingwBin = Join-Path $msys2Root 'mingw64\bin'
    $pacman = Join-Path $msys2Root 'usr\bin\pacman.exe'
    $definitions = @(
        @('Linker', 'FLISTWALKER_WINDOWS_LINKER', @('gcc.exe', 'x86_64-w64-mingw32-gcc.exe')),
        @('Cxx', 'FLISTWALKER_WINDOWS_CXX', @('g++.exe', 'x86_64-w64-mingw32-g++.exe')),
        @('Ar', 'FLISTWALKER_WINDOWS_AR', @('ar.exe', 'x86_64-w64-mingw32-ar.exe')),
        @('Ranlib', 'FLISTWALKER_WINDOWS_RANLIB', @('ranlib.exe', 'x86_64-w64-mingw32-ranlib.exe')),
        @('Windres', 'FLISTWALKER_WINDOWS_WINDRES', @('windres.exe', 'x86_64-w64-mingw32-windres.exe')),
        @('Strip', 'FLISTWALKER_WINDOWS_STRIP', @('strip.exe', 'x86_64-w64-mingw32-strip.exe'))
    )

    $resolved = @{}
    foreach ($definition in $definitions) {
        $resolved[$definition[0]] = Resolve-FlistWalkerGnuTool -EnvName $definition[1] -ToolNames $definition[2] -MingwBin $mingwBin
    }
    if ($resolved.Values -contains $null) {
        if (-not (Test-Path -LiteralPath $pacman -PathType Leaf)) {
            Install-FlistWalkerDependency -Plan (Get-FlistWalkerDependencyInstallPlan -Dependency MSYS2) -Mode $Mode
        }
        if (-not (Test-Path -LiteralPath $pacman -PathType Leaf)) {
            throw 'MSYS2 pacman was not found after installation. Open a new PowerShell window and run the script again.'
        }
        Install-FlistWalkerDependency -Plan (Get-FlistWalkerDependencyInstallPlan -Dependency GnuTools -PacmanPath $pacman) -Mode $Mode
        foreach ($definition in $definitions) {
            $resolved[$definition[0]] = Resolve-FlistWalkerGnuTool -EnvName $definition[1] -ToolNames $definition[2] -MingwBin $mingwBin
        }
    }
    if ($resolved.Values -contains $null) {
        throw 'GNU tools were not found after installation. Open a new PowerShell window and run the script again.'
    }

    $tools = [pscustomobject]$resolved
    Set-FlistWalkerGnuEnvironment -Tools $tools
    foreach ($name in @('cargo', 'rustc', 'rustup', 'Linker', 'Cxx', 'Ar', 'Ranlib', 'Windres', 'Strip')) {
        $value = if ($name -eq 'cargo') { $cargo } elseif ($name -eq 'rustc') { $rustc } elseif ($name -eq 'rustup') { $rustup } else { $tools.$name }
        Write-Host "OK: $name -> $value"
    }

    return [pscustomobject]@{
        Cargo = $cargo
        Rustc = $rustc
        Rustup = $rustup
        Target = $target
        Tools = $tools
    }
}
