//! On-demand connection inventory and TCP byte accounting. No DNS queries or packet capture.
use std::{
    collections::BTreeMap,
    net::{Ipv4Addr, Ipv6Addr},
    time::{Duration, Instant},
};
use windows::Win32::NetworkManagement::IpHelper::*;
#[derive(Clone, Copy)]
enum TcpRow {
    V4(MIB_TCPROW_LH),
    V6(MIB_TCP6ROW),
}
#[derive(Clone)]
struct Connection {
    key: String,
    local: String,
    remote: String,
    state: u32,
    row: TcpRow,
}
#[derive(Clone, Copy)]
struct Counters {
    incoming: u64,
    outgoing: u64,
}
fn port(raw: u32) -> u16 {
    u16::from_be(raw as u16)
}
fn v4(addr: u32, raw_port: u32) -> String {
    format!("{}:{}", Ipv4Addr::from(addr.to_ne_bytes()), port(raw_port))
}
fn v6(addr: [u8; 16], scope: u32, raw_port: u32) -> String {
    let ip = Ipv6Addr::from(addr);
    if scope == 0 {
        format!("[{ip}]:{}", port(raw_port))
    } else {
        format!("[{ip}%{scope}]:{}", port(raw_port))
    }
}
fn state_name(n: u32) -> &'static str {
    match n {
        1 => "closed",
        2 => "listening",
        3 => "SYN sent",
        4 => "SYN received",
        5 => "established",
        6 => "FIN wait 1",
        7 => "FIN wait 2",
        8 => "close wait",
        9 => "closing",
        10 => "last ACK",
        11 => "time wait",
        12 => "delete TCB",
        _ => "unknown",
    }
}
fn table<T: Copy>(udp: bool, af: u32) -> Result<Vec<T>, String> {
    unsafe {
        for _ in 0..3 {
            let mut size = 0;
            let get = |ptr, size| {
                if udp {
                    GetExtendedUdpTable(ptr, size, false, af, UDP_TABLE_OWNER_PID, 0)
                } else {
                    GetExtendedTcpTable(ptr, size, false, af, TCP_TABLE_OWNER_PID_ALL, 0)
                }
            };
            let code = get(None, &mut size);
            if code != 0 && code != 122 {
                return Err(format!("Connection table unavailable (Windows {code})"));
            }
            if size == 0 {
                return Ok(Vec::new());
            }
            if !(4..=16 * 1024 * 1024).contains(&size) {
                return Err("Unexpected connection table size".into());
            }
            let mut buffer = vec![0u64; (size as usize).div_ceil(8)];
            let cap = buffer.len() * 8;
            let code = get(Some(buffer.as_mut_ptr().cast()), &mut size);
            if code == 122 {
                continue;
            }
            if code != 0 {
                return Err(format!("Connection table unavailable (Windows {code})"));
            }
            let ptr = buffer.as_ptr().cast::<u8>();
            let count = ptr.cast::<u32>().read_unaligned() as usize;
            if size as usize > cap || count > (size as usize - 4) / size_of::<T>() {
                return Err("Invalid connection table bounds".into());
            }
            return Ok((0..count)
                .map(|i| ptr.add(4 + i * size_of::<T>()).cast::<T>().read_unaligned())
                .collect());
        }
        Err("Connection table changed too quickly; refresh to retry".into())
    }
}
fn connections(pid: u32) -> (Vec<Connection>, Vec<String>) {
    let mut list = Vec::new();
    let mut errors = Vec::new();
    match table::<MIB_TCPROW_OWNER_PID>(false, 2) {
        Ok(rows) => {
            for r in rows.into_iter().filter(|r| r.dwOwningPid == pid) {
                let local = v4(r.dwLocalAddr, r.dwLocalPort);
                let remote = v4(r.dwRemoteAddr, r.dwRemotePort);
                list.push(Connection {
                    key: format!("TCP4 {local} {remote}"),
                    local,
                    remote,
                    state: r.dwState,
                    row: TcpRow::V4(MIB_TCPROW_LH {
                        Anonymous: MIB_TCPROW_LH_0 { dwState: r.dwState },
                        dwLocalAddr: r.dwLocalAddr,
                        dwLocalPort: r.dwLocalPort,
                        dwRemoteAddr: r.dwRemoteAddr,
                        dwRemotePort: r.dwRemotePort,
                    }),
                });
            }
        }
        Err(e) => errors.push(format!("IPv4: {e}")),
    }
    match table::<MIB_TCP6ROW_OWNER_PID>(false, 23) {
        Ok(rows) => {
            for r in rows.into_iter().filter(|r| r.dwOwningPid == pid) {
                let local = v6(r.ucLocalAddr, r.dwLocalScopeId, r.dwLocalPort);
                let remote = v6(r.ucRemoteAddr, r.dwRemoteScopeId, r.dwRemotePort);
                let mut row = MIB_TCP6ROW {
                    State: MIB_TCP_STATE(r.dwState as i32),
                    dwLocalScopeId: r.dwLocalScopeId,
                    dwLocalPort: r.dwLocalPort,
                    dwRemoteScopeId: r.dwRemoteScopeId,
                    dwRemotePort: r.dwRemotePort,
                    ..Default::default()
                };
                row.LocalAddr.u.Byte = r.ucLocalAddr;
                row.RemoteAddr.u.Byte = r.ucRemoteAddr;
                list.push(Connection {
                    key: format!("TCP6 {local} {remote}"),
                    local,
                    remote,
                    state: r.dwState,
                    row: TcpRow::V6(row),
                });
            }
        }
        Err(e) => errors.push(format!("IPv6: {e}")),
    }
    list.sort_by(|a, b| a.key.cmp(&b.key));
    (list, errors)
}
fn bytes_mut<T>(value: &mut T) -> &mut [u8] {
    unsafe { std::slice::from_raw_parts_mut((value as *mut T).cast(), size_of::<T>()) }
}
fn read(row: &TcpRow) -> Result<Option<Counters>, u32> {
    unsafe {
        let mut rw = TCP_ESTATS_DATA_RW_v0::default();
        // Query the enable flag first; disabled dynamic structures are not valid measurements.
        let code = match row {
            TcpRow::V4(r) => GetPerTcpConnectionEStats(
                r,
                TcpConnectionEstatsData,
                Some(bytes_mut(&mut rw)),
                0,
                None,
                0,
                None,
                0,
            ),
            TcpRow::V6(r) => GetPerTcp6ConnectionEStats(
                r,
                TcpConnectionEstatsData,
                Some(bytes_mut(&mut rw)),
                0,
                None,
                0,
                None,
                0,
            ),
        };
        if code != 0 {
            return Err(code);
        }
        if !rw.EnableCollection {
            return Ok(None);
        }
        let mut rod = TCP_ESTATS_DATA_ROD_v0::default();
        let code = match row {
            TcpRow::V4(r) => GetPerTcpConnectionEStats(
                r,
                TcpConnectionEstatsData,
                Some(bytes_mut(&mut rw)),
                0,
                None,
                0,
                Some(bytes_mut(&mut rod)),
                0,
            ),
            TcpRow::V6(r) => GetPerTcp6ConnectionEStats(
                r,
                TcpConnectionEstatsData,
                Some(bytes_mut(&mut rw)),
                0,
                None,
                0,
                Some(bytes_mut(&mut rod)),
                0,
            ),
        };
        if code != 0 {
            return Err(code);
        }
        if !rw.EnableCollection {
            return Ok(None);
        }
        Ok(Some(Counters {
            incoming: rod.DataBytesIn,
            outgoing: rod.DataBytesOut,
        }))
    }
}
fn enable(row: &TcpRow, on: bool) -> Result<(), u32> {
    unsafe {
        let mut rw = TCP_ESTATS_DATA_RW_v0 {
            EnableCollection: on,
        };
        let code = match row {
            TcpRow::V4(r) => {
                SetPerTcpConnectionEStats(r, TcpConnectionEstatsData, bytes_mut(&mut rw), 0, 0)
            }
            TcpRow::V6(r) => {
                SetPerTcp6ConnectionEStats(r, TcpConnectionEstatsData, bytes_mut(&mut rw), 0, 0)
            }
        };
        if code == 0 { Ok(()) } else { Err(code) }
    }
}
fn unavailable(code: u32) -> String {
    match code {
        5 => "N/A - administrator rights required".into(),
        87 => "N/A - connection is not established or provider unavailable".into(),
        1168 => "N/A - connection closed".into(),
        _ => format!("N/A - Windows error {code}"),
    }
}
fn amount(v: u64) -> String {
    format!("{v} bytes ({:.2} MiB)", v as f64 / 1048576.0)
}
pub fn snapshot(pid: u32) -> crate::model::Report {
    let (list, errors) = connections(pid);
    let mut report = crate::model::Report::new(
        format!("Connections · PID {pid}"),
        &[
            "Protocol",
            "Local endpoint",
            "Remote endpoint",
            "State",
            "Received",
            "Sent",
        ],
    );
    report.metric("TCP endpoints", list.len());
    report.metric("Accounting", "Since enabled");
    report.metric("UDP peers", "Unavailable");
    report.metric("DNS lookup", "Off");
    for c in list.iter().take(256) {
        let (incoming, outgoing) = match read(&c.row) {
            Ok(Some(v)) => (amount(v.incoming), amount(v.outgoing)),
            Ok(None) => ("N/A · disabled".into(), "N/A · disabled".into()),
            Err(code) => (unavailable(code), "N/A".into()),
        };
        report.row(&[
            "TCP",
            &c.local,
            &c.remote,
            state_name(c.state),
            &incoming,
            &outgoing,
        ]);
    }
    match table::<MIB_UDPROW_OWNER_PID>(true, 2) {
        Ok(rows) => {
            for r in rows.into_iter().filter(|r| r.dwOwningPid == pid).take(128) {
                report.row(&[
                    "UDP",
                    &v4(r.dwLocalAddr, r.dwLocalPort),
                    "Unavailable",
                    "Local socket",
                    "N/A",
                    "N/A",
                ]);
            }
        }
        Err(e) => report.row(&["UDP IPv4", "Unavailable", &e, "Error", "N/A", "N/A"]),
    }
    match table::<MIB_UDP6ROW_OWNER_PID>(true, 23) {
        Ok(rows) => {
            for r in rows.into_iter().filter(|r| r.dwOwningPid == pid).take(128) {
                report.row(&[
                    "UDP v6",
                    &v6(r.ucLocalAddr, r.dwLocalScopeId, r.dwLocalPort),
                    "Unavailable",
                    "Local socket",
                    "N/A",
                    "N/A",
                ]);
            }
        }
        Err(e) => report.row(&["UDP IPv6", "Unavailable", &e, "Error", "N/A", "N/A"]),
    }
    for e in errors {
        report.row(&["TCP", "Unavailable", &e, "Error", "N/A", "N/A"]);
    }
    report
}
struct Track {
    c: Connection,
    previous: Option<Counters>,
    incoming: u64,
    outgoing: u64,
    readings: u32,
    changed: bool,
    status: String,
    last_seen: u64,
}
impl Drop for Track {
    fn drop(&mut self) {
        if self.changed {
            let _ = enable(&self.c.row, false);
        }
    }
}
pub fn measure(pid: u32, expected: Option<u64>) -> Result<crate::model::Report, String> {
    let identity = crate::native::ProcessIdentity::open(pid, expected)?;
    let start = Instant::now();
    let mut tracks: BTreeMap<String, Track> = BTreeMap::new();
    let mut errors = Vec::new();
    loop {
        if !identity.running() {
            errors.push("Process exited; measurement stopped early.".into());
            break;
        }
        let (list, table_errors) = connections(pid);
        errors.extend(table_errors);
        for c in list.into_iter().filter(|c| c.state == 5) {
            if !tracks.contains_key(&c.key) && tracks.len() >= 256 {
                continue;
            }
            let t = tracks.entry(c.key.clone()).or_insert_with(|| {
                let mut changed = false;
                let mut status = "Existing counter collection preserved".into();
                let previous = match read(&c.row) {
                    Ok(Some(v)) => Some(v),
                    Ok(None) => match enable(&c.row, true) {
                        Ok(()) => {
                            changed = true;
                            status = "Temporary byte counters enabled".into();
                            read(&c.row).ok().flatten()
                        }
                        Err(code) => {
                            status = unavailable(code);
                            None
                        }
                    },
                    Err(code) => {
                        status = unavailable(code);
                        None
                    }
                };
                Track {
                    c: c.clone(),
                    previous,
                    incoming: 0,
                    outgoing: 0,
                    readings: 0,
                    changed,
                    status,
                    last_seen: 0,
                }
            });
            t.c = c;
            t.last_seen = start.elapsed().as_secs();
            match read(&t.c.row) {
                Ok(Some(v)) => {
                    if let Some(old) = t.previous {
                        if let (Some(incoming), Some(outgoing)) = (
                            v.incoming.checked_sub(old.incoming),
                            v.outgoing.checked_sub(old.outgoing),
                        ) {
                            t.incoming = t.incoming.saturating_add(incoming);
                            t.outgoing = t.outgoing.saturating_add(outgoing);
                            t.readings += 1;
                        } else {
                            t.status="Counters reset or connection replaced; observed totals are partial".into();
                        }
                    }
                    t.previous = Some(v);
                }
                Ok(None) => t.status = "N/A - counter collection is disabled".into(),
                Err(code) => t.status = unavailable(code),
            }
        }
        if start.elapsed() >= Duration::from_secs(10) {
            break;
        }
        std::thread::sleep(Duration::from_secs(1));
    }
    let mut report = crate::model::Report::new(
        format!("TCP traffic · PID {pid}"),
        &[
            "Protocol",
            "Local endpoint",
            "Remote endpoint",
            "Status",
            "Received",
            "Sent",
        ],
    );
    let mut sums = (0u64, 0u64, 0usize);
    for t in tracks.values_mut() {
        let (incoming, outgoing) = if t.readings > 0 {
            sums.0 = sums.0.saturating_add(t.incoming);
            sums.1 = sums.1.saturating_add(t.outgoing);
            sums.2 += 1;
            (amount(t.incoming), amount(t.outgoing))
        } else {
            ("N/A".into(), "N/A".into())
        };
        if t.changed {
            match enable(&t.c.row, false) {
                Ok(()) => {
                    t.changed = false;
                    t.status.push_str("; restored");
                }
                Err(1168 | 87) => {
                    t.changed = false;
                    t.status.push_str("; closed");
                }
                Err(code) => t.status.push_str(&format!("; cleanup failed ({code})")),
            }
        }
        report.row(&[
            "TCP",
            &t.c.local,
            &t.c.remote,
            &t.status,
            &incoming,
            &outgoing,
        ]);
    }
    report.metric(
        "Received",
        if sums.2 > 0 {
            amount(sums.0)
        } else {
            "N/A".into()
        },
    );
    report.metric(
        "Sent",
        if sums.2 > 0 {
            amount(sums.1)
        } else {
            "N/A".into()
        },
    );
    report.metric("Measured flows", sums.2);
    report.metric(
        "Observation",
        format!("{:.1} s", start.elapsed().as_secs_f64()),
    );
    errors.sort();
    errors.dedup();
    for error in errors {
        report.row(&["Notice", "", "", &error, "N/A", "N/A"]);
    }
    if sums.2 == 0 {
        report.row(&[
            "Access",
            "",
            "",
            "No readable counters; administrator rights may be required",
            "N/A",
            "N/A",
        ]);
    }
    report.row(&[
        "Coverage",
        "1 s polling",
        "TCP only",
        "Lower bounds; short/closed flows may be missed",
        "UDP/QUIC N/A",
        "No history",
    ]);
    Ok(report)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn endpoints_preserve_byte_order() {
        assert_eq!(
            v4(u32::from_ne_bytes([127, 0, 0, 1]), u16::to_be(443) as u32),
            "127.0.0.1:443"
        );
        assert_eq!(
            v6(Ipv6Addr::LOCALHOST.octets(), 0, u16::to_be(80) as u32),
            "[::1]:80"
        );
    }
    #[test]
    fn tcp_table_finds_local_listener() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = listener.local_addr().unwrap().to_string();
        let (rows, errors) = connections(std::process::id());
        assert!(errors.is_empty(), "{errors:?}");
        assert!(rows.iter().any(|r| r.local == endpoint && r.state == 2));
    }
}
