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
 $rows.Add(@('Info','Startup entries',"$($startup.Count) entries",'Count alone is not a fault; may include disabled entries','Startup apps'))
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
ConvertTo-Json -InputObject @($rows.ToArray()) -Depth 4 -Compress
