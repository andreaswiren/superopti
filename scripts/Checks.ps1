$ErrorActionPreference='Stop'
function Get-DriveCapacityStatus([double]$FreePercent) {
 if([double]::IsNaN($FreePercent) -or [double]::IsInfinity($FreePercent) -or $FreePercent -lt 0 -or $FreePercent -gt 100) { return 'Unknown' }
 if($FreePercent -lt 5) { return 'Critical' }
 if($FreePercent -lt 15) { return 'Warning' }
 return 'OK'
}
$rows=[System.Collections.Generic.List[object]]::new()
try {
 foreach($d in @(Get-CimInstance Win32_LogicalDisk -Filter 'DriveType=3' -OperationTimeoutSec 5)) {
  if($d.Size -gt 0) {
   $percent=100*$d.FreeSpace/$d.Size
   $state=Get-DriveCapacityStatus $percent
   $rows.Add(@($state,"Drive $($d.DeviceID)",('{0:N1}% free' -f $percent),'Free space: review below 15%; critical below 5%','Storage'))
  }
 }
} catch {$rows.Add(@('Unknown','Drive capacity','Unavailable','CIM query failed','Retry'))}
try {
 $startup=@(Get-CimInstance Win32_StartupCommand -OperationTimeoutSec 5)
 $rows.Add(@('Info','Startup entries',"$($startup.Count) entries",'Entries are listed below; review optional launchers','Startup apps'))
 foreach($entry in $startup | Select-Object -First 16) {
  $name = if($entry.Name){$entry.Name}else{'Unnamed'}
  $command = if($entry.Command){$entry.Command}else{'Command unavailable'}
  $known = $name -match '(?i)logi|logitech|logitune' -or $command -match '(?i)logi|logitech|logitune'
  $state = if($known){'Review'}else{'Info'}
  $target = if($known){'Known optional Logitech launcher; disable only if unused'}else{'Review publisher and need before disabling'}
  $rows.Add(@($state,"Startup: $name",$command,$target,'Startup apps'))
 }
} catch {$rows.Add(@('Unknown','Startup entries','Unavailable','CIM query failed','Retry'))}
$restart=(Test-Path 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Component Based Servicing\RebootPending') -or (Test-Path 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\WindowsUpdate\Auto Update\RebootRequired')
$rows.Add(@($(if($restart){'Warning'}else{'OK'}),'Servicing restart',$(if($restart){'Pending'}else{'No flag'}),'Not a full update check','Windows Update'))
try {
 $os=Get-CimInstance Win32_OperatingSystem -OperationTimeoutSec 5
 $rows.Add(@('Info','Uptime',('{0:N1} days' -f ((Get-Date)-$os.LastBootUpTime).TotalDays),'Long uptime alone is not a fault',''))
} catch {$rows.Add(@('Unknown','Uptime','Unavailable','CIM query failed','Retry'))}
try {
 $events=@(Get-WinEvent -FilterHashtable @{LogName='System';ProviderName='Microsoft-Windows-Kernel-Processor-Power';Id=37;StartTime=(Get-Date).AddHours(-24)} -MaxEvents 250 -ErrorAction Stop)
 $latest=$events[0].TimeCreated.ToUniversalTime().ToString('yyyy-MM-ddTHH:mm:ssZ')
 $rows.Add(@('Warning','CPU firmware limiting',"$($events.Count) reports in last 24 hours (maximum 250)","Latest $latest; historical Event 37 does not identify a thermal cause",'Inspect firmware / power policy'))
} catch {
 if($_.FullyQualifiedErrorId -like 'NoMatchingEventsFound*') {
  $rows.Add(@('Info','CPU firmware limiting','No Event 37 in last 24 hours','Absence of logged events does not rule out throttling',''))
 } else {
  $rows.Add(@('Unknown','CPU firmware limiting','Event log unavailable','Throttling cannot be assessed from this source','Retry with appropriate access'))
 }
}
$rows.Add(@('Unknown','GPU throttling','Not measured by system checks','Requires supported device telemetry during capture','GPU capture details'))
try {
 $dev=@(Get-Process -ErrorAction Stop | Where-Object { $_.ProcessName -match '(?i)wsl|vmmem|docker|com\.docker' })
 if($dev.Count -gt 0) { $rows.Add(@('Info','WSL / Docker processes',"$($dev.Count) running",'Review container workloads when diagnosing sustained CPU, RAM or I/O pressure','Processes')) }
 $devtools=@(Get-Process -ErrorAction Stop | Where-Object { $_.ProcessName -match '(?i)^node$|npm|yarn|pnpm|vite|webpack|react' })
 foreach($p in $devtools | Sort-Object StartTime | Select-Object -First 12) {
  $age=[math]::Round(((Get-Date)-$p.StartTime).TotalHours,1)
  $state=if($age -ge 4){'Review'}else{'Info'}
  $rows.Add(@($state,"Dev process: $($p.ProcessName)","PID $($p.Id); $age h",'Long-lived development workers can remain after tests; stop only when confirmed idle','Processes'))
 }
} catch {$rows.Add(@('Unknown','Container / dev processes','Unavailable','Process query failed','Retry'))}
ConvertTo-Json -InputObject @($rows.ToArray()) -Depth 4 -Compress
