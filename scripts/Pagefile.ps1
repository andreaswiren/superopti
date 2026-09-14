param([ValidateSet('Check','Apply','Undo','Inspect')][string]$Mode = 'Check')
$ErrorActionPreference = 'Stop'
$memoryKey = 'HKLM:\SYSTEM\CurrentControlSet\Control\Session Manager\Memory Management'
$backupKey = 'HKLM:\SOFTWARE\SuperOpti'
function Get-FixedPagefileMiB([UInt64]$Bytes) {
    if ($Bytes -eq 0) { throw 'Installed physical memory is unavailable.' }
    [UInt32][math]::Max(16384, [math]::Ceiling($Bytes / 2 / 1MB))
}
function Read-PagingFiles { @((Get-ItemProperty -LiteralPath $memoryKey -Name PagingFiles).PagingFiles) }
function Assert-Admin {
    $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
    if (-not ([Security.Principal.WindowsPrincipal]::new($identity)).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
        throw 'Pagefile changes require Run as administrator. No settings were changed.'
    }
}
function Restore-Pagefile($Saved) {
    $system = Get-CimInstance Win32_ComputerSystem -OperationTimeoutSec 5
    $system | Set-CimInstance -Property @{AutomaticManagedPagefile=[bool]$Saved.Automatic} | Out-Null
    New-ItemProperty -LiteralPath $memoryKey -Name PagingFiles -PropertyType MultiString -Value ([string[]]$Saved.Entries) -Force | Out-Null
    $actual = @(Read-PagingFiles)
    $auto = (Get-CimInstance Win32_ComputerSystem -OperationTimeoutSec 5).AutomaticManagedPagefile
    if (($actual -join "`n") -cne (@($Saved.Entries) -join "`n") -or $auto -ne [bool]$Saved.Automatic) { throw 'Restoration verification failed; backup retained.' }
}
if ($Mode -eq 'Undo') {
    if (-not (Test-Path -LiteralPath $backupKey)) { 'No saved pagefile changes.'; return }
    $savedText = (Get-ItemProperty -LiteralPath $backupKey -Name PagefileBackup -ErrorAction SilentlyContinue).PagefileBackup
    if (-not $savedText) { 'No saved pagefile changes.'; return }
    Assert-Admin
    Restore-Pagefile ($savedText | ConvertFrom-Json)
    Remove-ItemProperty -LiteralPath $backupKey -Name PagefileBackup
    'Original pagefile configuration restored and verified. Restart Windows to activate it.'
    return
}
$ram = [UInt64](Get-CimInstance Win32_PhysicalMemory -OperationTimeoutSec 5 | Measure-Object -Property Capacity -Sum).Sum
$target = Get-FixedPagefileMiB $ram
$system = Get-CimInstance Win32_ComputerSystem -OperationTimeoutSec 5
$entries = @(Read-PagingFiles)
$os = Get-CimInstance Win32_OperatingSystem -OperationTimeoutSec 5
$path = $os.SystemDrive + '\pagefile.sys'
# Retain a single existing drive choice. Multiple/custom configurations need manual review.
$supported = $true
if ($entries.Count -gt 1) { $supported = $false }
if ($entries.Count -eq 1 -and $entries[0] -match '^([A-Za-z]:\\[^"\r\n]+)\s+\d+\s+\d+$') { $path = $Matches[1] }
elseif ($entries.Count -eq 1 -and $entries[0] -notmatch '^\?:\\pagefile.sys') { $supported = $false }
$desired = "$path $target $target"
$disk = Get-CimInstance Win32_LogicalDisk -Filter ("DeviceID='" + $path.Substring(0,2) + "'") -OperationTimeoutSec 5
$existing = @(Get-CimInstance Win32_PageFileUsage -OperationTimeoutSec 5 | Where-Object Name -EQ $path)
$allocated = [double]($existing | Measure-Object -Property AllocatedBaseSize -Sum).Sum * 1MB
$growth = [math]::Max([double]0, [double]$target * 1MB - $allocated)
$reserve = [math]::Max([double]2GB, [double]$disk.Size * 0.05)
$spaceOK = $null -ne $disk -and $disk.DriveType -eq 3 -and ([double]$disk.FreeSpace - $growth) -ge $reserve
$matched = -not $system.AutomaticManagedPagefile -and $entries.Count -eq 1 -and $entries[0] -eq $desired
if ($Mode -eq 'Inspect') {
    [pscustomobject]@{matched=$matched;targetMiB=$target;installedGiB=($ram/1GB);path=$path;automatic=[bool]$system.AutomaticManagedPagefile;allocatedMiB=($allocated/1MB);spaceOK=$spaceOK;supported=$supported;growthGiB=($growth/1GB);reserveGiB=($reserve/1GB)} | ConvertTo-Json -Compress
    return
}
$state = if ($matched) {'OK'} else {'REVIEW'}
"$state`: Fixed pagefile policy: $target MiB ($($target/1024) GiB), initial = maximum; installed RAM $([math]::Round($ram/1GB,2)) GiB. Target: $path."
"Configured: $($entries -join '; '); automatic management: $($system.AutomaticManagedPagefile). Running allocation: $([math]::Round($allocated/1MB)) MiB (may differ until restart)."
"Disk-space guard: $spaceOK. Needs $([math]::Round($growth/1GB,2)) GiB additional space plus $([math]::Round($reserve/1GB,2)) GiB reserve."
if (-not $supported) { 'REVIEW: Multiple or custom pagefiles are preserved; use Windows Virtual memory to review them.' }
'Fixed sizing is your selected policy, not a universal performance optimum. It limits commit headroom and may be too small for complete crash dumps. Windows swapfile.sys is managed separately.'
if ($Mode -eq 'Check') { return }
if ($matched) { 'Policy already configured. No change.'; return }
Assert-Admin
if (-not $supported) { throw 'Automatic fix refused for multiple/custom pagefiles; no changes made.' }
if (-not $spaceOK) { throw 'Insufficient disk space for the fixed pagefile and reserve; no changes made.' }
$before = @{Automatic=[bool]$system.AutomaticManagedPagefile; Entries=[string[]]$entries}
New-Item -Path $backupKey -Force | Out-Null
if (-not (Get-ItemProperty -LiteralPath $backupKey -Name PagefileBackup -ErrorAction SilentlyContinue).PagefileBackup) {
    New-ItemProperty -LiteralPath $backupKey -Name PagefileBackup -PropertyType String -Value ($before | ConvertTo-Json -Compress) | Out-Null
}
try {
    $system | Set-CimInstance -Property @{AutomaticManagedPagefile=$false} | Out-Null
    New-ItemProperty -LiteralPath $memoryKey -Name PagingFiles -PropertyType MultiString -Value ([string[]]@($desired)) -Force | Out-Null
    $actual = @(Read-PagingFiles)
    if ((Get-CimInstance Win32_ComputerSystem -OperationTimeoutSec 5).AutomaticManagedPagefile -or $actual.Count -ne 1 -or $actual[0] -ne $desired) { throw 'Configuration verification failed.' }
} catch {
    $failure = $_.Exception.Message
    try { Restore-Pagefile $before } catch { throw "$failure Rollback failed: $_. Backup retained; review Virtual memory before restarting." }
    throw "$failure Previous configuration restored; backup retained."
}
"Fixed pagefile configured and verified: initial = maximum = $target MiB. Original settings saved for Undo. Restart Windows to activate; no automatic restart."
