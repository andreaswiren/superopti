$ErrorActionPreference = 'Stop'
$expected = [IO.Path]::GetFullPath((Join-Path $env:LOCALAPPDATA 'SuperOpti'))
$actual = [IO.Path]::GetFullPath($PSScriptRoot)
if ($actual -ne $expected) { throw 'Run the installed uninstaller from LocalAppData\SuperOpti.' }
$exe = Join-Path $expected 'superopti.exe'
$running = @(Get-Process -Name superopti -ErrorAction SilentlyContinue | Where-Object { $_.Path -eq $exe })
if ($running.Count -gt 0) { throw 'Exit SuperOpti from its tray menu, then run Uninstall again.' }
$run = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run'
if (Get-ItemProperty -Path $run -Name SuperOpti -ErrorAction SilentlyContinue) { Remove-ItemProperty -Path $run -Name SuperOpti }
Remove-Item -LiteralPath 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\SuperOpti' -Recurse -ErrorAction SilentlyContinue
$link = Join-Path ([Environment]::GetFolderPath('Programs')) 'SuperOpti.lnk'
if (Test-Path -LiteralPath $link) { Remove-Item -LiteralPath $link }
# Only known installed files are removed. Captures and undo backup are retained.
foreach ($name in @('superopti.exe','Install.ps1','Uninstall.ps1')) {
 $item = [IO.Path]::GetFullPath((Join-Path $expected $name))
 if ([IO.Path]::GetDirectoryName($item) -ne $expected) { throw 'Invalid uninstall target.' }
 if (Test-Path -LiteralPath $item) { Remove-Item -LiteralPath $item -Force }
}
Write-Output 'SuperOpti uninstalled. Captures and the fix backup remain in LocalAppData\SuperOpti. Use Undo before uninstalling if you want settings restored.'

