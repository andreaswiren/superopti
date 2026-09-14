param([string]$SourceExe = (Join-Path $PSScriptRoot 'superopti.exe'), [switch]$AutoStart)
$ErrorActionPreference = 'Stop'
$target = [IO.Path]::GetFullPath((Join-Path $env:LOCALAPPDATA 'SuperOpti'))
$source = [IO.Path]::GetFullPath($SourceExe)
if (!(Test-Path -LiteralPath $source -PathType Leaf)) { throw "Executable not found: $source. Build first or use the binary distribution." }
New-Item -ItemType Directory -Path $target -Force | Out-Null
$exe = Join-Path $target 'superopti.exe'
if ($source -ne $exe) { Copy-Item -LiteralPath $source -Destination $exe -Force }
$uninstaller = Join-Path $target 'Uninstall.ps1'
$sourceUninstaller = Join-Path $PSScriptRoot 'Uninstall.ps1'
if ($sourceUninstaller -ne $uninstaller) { Copy-Item -LiteralPath $sourceUninstaller -Destination $uninstaller -Force }
$programs = [Environment]::GetFolderPath('Programs')
$shell = New-Object -ComObject WScript.Shell
$link = $shell.CreateShortcut((Join-Path $programs 'SuperOpti.lnk'))
$link.TargetPath = $exe
$link.WorkingDirectory = $target
$link.Description = 'On-demand Windows performance diagnostics'
$link.Save()
$key = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\SuperOpti'
New-Item -Path $key -Force | Out-Null
$version = (Get-Item -LiteralPath $exe).VersionInfo.ProductVersion
if ($version -notmatch '^\d+\.\d+\.\d+$') { throw 'Executable product version is missing or invalid.' }
$systemPowerShell = Join-Path ([Environment]::SystemDirectory) 'WindowsPowerShell\v1.0\powershell.exe'
$props = @{ DisplayName='SuperOpti'; DisplayVersion=$version; Publisher='SuperOpti'; InstallLocation=$target; DisplayIcon=$exe; UninstallString="`"$systemPowerShell`" -NoProfile -ExecutionPolicy Bypass -File `"$uninstaller`"" }
foreach ($name in $props.Keys) { New-ItemProperty -Path $key -Name $name -Value $props[$name] -PropertyType String -Force | Out-Null }
$run = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run'
if ($AutoStart -or (Get-ItemProperty -Path $run -Name SuperOpti -ErrorAction SilentlyContinue)) {
 New-Item -Path $run -Force | Out-Null
 New-ItemProperty -Path $run -Name SuperOpti -Value "`"$exe`" --tray" -PropertyType String -Force | Out-Null
}
Write-Output "Installed to $target. Start-menu shortcut and Settings uninstall entry created. Monitoring remains off at startup."

