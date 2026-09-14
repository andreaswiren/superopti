$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [Text.UTF8Encoding]::new($false)
$targets = [Collections.Generic.HashSet[string]]::new()
$scopes = [Collections.Generic.List[string]]::new()
foreach ($address in @(Get-NetIPAddress -AddressFamily IPv4 -AddressState Preferred)) {
    $octets = [Net.IPAddress]::Parse($address.IPAddress).GetAddressBytes()
    $private = $octets[0] -eq 10 -or ($octets[0] -eq 172 -and $octets[1] -ge 16 -and $octets[1] -le 31) -or ($octets[0] -eq 192 -and $octets[1] -eq 168)
    if (-not $private -or $address.PrefixLength -lt 24 -or $address.PrefixLength -gt 30) { continue }
    $adapter = Get-NetAdapter -InterfaceIndex $address.InterfaceIndex -ErrorAction SilentlyContinue
    if (-not $adapter -or $adapter.Status -ne 'Up' -or -not $adapter.HardwareInterface) { continue }
    $count = [int][Math]::Pow(2, 32 - $address.PrefixLength)
    $start = [int]([Math]::Floor($octets[3] / $count) * $count)
    $scopes.Add("$($address.IPAddress)/$($address.PrefixLength)")
    for ($i = $start + 1; $i -lt $start + $count - 1; $i++) {
        if ($targets.Count -ge 254) { break }
        $ip = "$($octets[0]).$($octets[1]).$($octets[2]).$i"
        if ($ip -ne $address.IPAddress) { [void]$targets.Add($ip) }
    }
}
# IPv6 has enormous address spaces. Reprobe known on-link neighbors and routers
# with their interface scope instead of brute-forcing a /64.
foreach ($neighbor in @(Get-NetNeighbor -AddressFamily IPv6 -ErrorAction SilentlyContinue)) {
    if ($targets.Count -ge 508) { break }
    $adapter = Get-NetAdapter -InterfaceIndex $neighbor.InterfaceIndex -ErrorAction SilentlyContinue
    if (-not $adapter -or $adapter.Status -ne 'Up' -or -not $adapter.HardwareInterface) { continue }
    $ip = [Net.IPAddress]::Parse($neighbor.IPAddress)
    if ($ip.IsIPv6Multicast -or [Net.IPAddress]::IsLoopback($ip) -or $ip.Equals([Net.IPAddress]::IPv6Any)) { continue }
    if ($ip.IsIPv6LinkLocal) { $ip.ScopeId = $neighbor.InterfaceIndex }
    [void]$targets.Add($ip.ToString())
    if (-not $scopes.Contains("IPv6 known neighbors · interface $($neighbor.InterfaceIndex)")) { $scopes.Add("IPv6 known neighbors · interface $($neighbor.InterfaceIndex)") }
}
if ($targets.Count -eq 0) { throw 'No eligible private IPv4 subnet or known physical-adapter IPv6 neighbor. No addresses were probed.' }
$items = @($targets)
$replies = 0
for ($offset = 0; $offset -lt $items.Count; $offset += 8) {
    $batch = @()
    for ($i = $offset; $i -lt [Math]::Min($offset + 8, $items.Count); $i++) {
        $ping = [Net.NetworkInformation.Ping]::new()
        $batch += @{ Ping = $ping; Task = $ping.SendPingAsync($items[$i], 200) }
    }
    foreach ($entry in $batch) {
        try { if ($entry.Task.GetAwaiter().GetResult().Status -eq 'Success') { $replies++ } } catch {} finally { $entry.Ping.Dispose() }
    }
}
@{ probes = $items.Count; replies = $replies; scopes = @($scopes) } | ConvertTo-Json -Compress
