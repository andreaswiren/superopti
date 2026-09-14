//! Local network inventory and observation changes, independently implemented.
use crate::{actions, model::Report, traffic};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fs};
#[derive(Clone, Debug, Serialize, Deserialize)]
struct Device {
    ip: String,
    mac: String,
    interface: String,
    state: String,
    first: u64,
    last: u64,
    observed: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
struct Change {
    at: u64,
    kind: String,
    ip: String,
    mac: String,
    interface: String,
}
#[derive(Default, Serialize, Deserialize)]
struct Inventory {
    devices: BTreeMap<String, Device>,
    changes: Vec<Change>,
}
#[derive(Deserialize)]
struct Neighbor {
    ip: String,
    mac: String,
    interface: String,
    state: String,
}
fn load() -> Result<Inventory, String> {
    let p = actions::data_dir().join("network-devices.json");
    if !p.exists() {
        return Ok(Inventory::default());
    }
    if p.metadata().map_err(|e| e.to_string())?.len() > 8 * 1024 * 1024 {
        return Err("Device inventory exceeds 8 MiB. Archive it before refreshing.".into());
    }
    serde_json::from_slice(&fs::read(p).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}
fn merge(
    inventory: &mut Inventory,
    neighbors: Vec<Neighbor>,
    at: u64,
) -> Result<(usize, usize), String> {
    let old = inventory.devices.clone();
    for d in inventory.devices.values_mut() {
        d.observed = false;
    }
    let mut added = 0;
    let mut changed = 0;
    for n in neighbors {
        let key = format!("{}|{}|{}", n.interface, n.mac.to_uppercase(), n.ip);
        let previous = old.get(&key).or_else(|| {
            old.values()
                .filter(|d| d.interface == n.interface && d.mac.eq_ignore_ascii_case(&n.mac))
                .max_by_key(|d| d.last)
        });
        let kind = match previous {
            None => {
                added += 1;
                Some("New device")
            }
            Some(d) if d.ip != n.ip => {
                changed += 1;
                Some("Address added / changed")
            }
            Some(d) if !d.observed => {
                changed += 1;
                Some("Observed again")
            }
            _ => None,
        };
        if let Some(kind) = kind {
            inventory.changes.push(Change {
                at,
                kind: kind.into(),
                ip: n.ip.clone(),
                mac: n.mac.clone(),
                interface: n.interface.clone(),
            });
        }
        inventory.devices.insert(
            key,
            Device {
                ip: n.ip,
                mac: n.mac,
                interface: n.interface,
                state: n.state,
                first: previous.map_or(at, |d| d.first),
                last: at,
                observed: true,
            },
        );
    }
    for (key, d) in &old {
        if d.observed && !inventory.devices.get(key).is_some_and(|v| v.observed) {
            inventory.changes.push(Change {
                at,
                kind: "Not in neighbor cache".into(),
                ip: d.ip.clone(),
                mac: d.mac.clone(),
                interface: d.interface.clone(),
            });
        }
    }
    if inventory.devices.len() > 10000 {
        return Err("Inventory exceeds 10,000 devices. Archive the local inventory first.".into());
    }
    // A documented bounded observation journal, separate from persistent first/last seen.
    if inventory.changes.len() > 10000 {
        inventory.changes.drain(..inventory.changes.len() - 10000);
    }
    Ok((added, changed))
}
pub fn scan(active: bool) -> Result<Report, String> {
    let discovery = if active {
        Some(actions::ps(include_str!("../scripts/Discover.ps1"))?)
    } else {
        None
    };
    let output = actions::ps(
        r#"$ErrorActionPreference='Stop'; [Console]::OutputEncoding=[Text.UTF8Encoding]::new($false); $rows=@(Get-NetNeighbor | Where-Object { $_.LinkLayerAddress -and $_.LinkLayerAddress -notmatch '^(00-00-00-00-00-00|FF-FF-FF-FF-FF-FF)$' -and $_.State -ne 'Unreachable' -and $_.IPAddress -notmatch '^(224\.|239\.|ff)' } | ForEach-Object { @{ip=$_.IPAddress;mac=$_.LinkLayerAddress;interface=[string]$_.InterfaceIndex;state=[string]$_.State} }); ConvertTo-Json -InputObject $rows -Compress"#,
    )?;
    let neighbors: Vec<Neighbor> =
        serde_json::from_str(&output).map_err(|e| format!("Invalid neighbor inventory: {e}"))?;
    let mut inventory = load()?;
    let at = crate::metrics::now();
    let (added, changed) = merge(&mut inventory, neighbors, at)?;
    fs::create_dir_all(actions::data_dir()).map_err(|e| e.to_string())?;
    let path = actions::data_dir().join("network-devices.json");
    let tmp = path.with_extension("tmp");
    fs::write(
        &tmp,
        serde_json::to_vec(&inventory).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    fs::rename(tmp, path).map_err(|e| e.to_string())?;
    let mut r = Report::new(
        "Network devices · cached observations, not an online/offline guarantee",
        &[
            "IP address",
            "MAC address",
            "Interface index",
            "Neighbor state",
            "Observation",
            "First UTC",
            "Last UTC",
        ],
    );
    r.metric("Known", inventory.devices.len());
    r.metric(
        "Observed",
        inventory.devices.values().filter(|d| d.observed).count(),
    );
    r.metric("New", added);
    r.metric("Changed", changed);
    for d in inventory.devices.values() {
        r.row(&[
            &d.ip,
            &d.mac,
            &d.interface,
            &d.state,
            if d.observed {
                "In cache"
            } else {
                "Not observed"
            },
            &traffic::utc(d.first),
            &traffic::utc(d.last),
        ]);
    }
    if let Some(json) = discovery {
        let v: serde_json::Value = serde_json::from_str(&json).map_err(|e| e.to_string())?;
        r.title = format!(
            "LAN discovery · {} probes · {} ICMP replies · cache observations below",
            v["probes"], v["replies"]
        );
        r.debug = json;
    }
    Ok(r)
}
pub fn changes() -> Result<Report, String> {
    let i = load()?;
    let mut r = Report::new(
        "Network changes · last 10,000 observations",
        &[
            "UTC",
            "Change",
            "IP address",
            "MAC address",
            "Interface index",
        ],
    );
    r.metric("Known", i.devices.len());
    r.metric("Changes", i.changes.len());
    r.metric("Discovery", "On demand");
    r.metric("DNS lookups", "Off");
    for c in i.changes.iter().rev() {
        r.row(&[&traffic::utc(c.at), &c.kind, &c.ip, &c.mac, &c.interface]);
    }
    Ok(r)
}
#[cfg(test)]
mod tests {
    use super::*;
    fn n(ip: &str) -> Neighbor {
        Neighbor {
            ip: ip.into(),
            mac: "AA-BB-CC-DD-EE-01".into(),
            interface: "2".into(),
            state: "Stale".into(),
        }
    }
    #[test]
    fn baseline_and_changes() {
        let mut i = Inventory::default();
        assert_eq!(merge(&mut i, vec![n("10.0.0.2")], 100).unwrap(), (1, 0));
        assert_eq!(merge(&mut i, vec![n("10.0.0.3")], 101).unwrap(), (0, 1));
        merge(&mut i, vec![], 102).unwrap();
        assert!(i.devices.values().all(|d| !d.observed));
        assert_eq!(i.changes.last().unwrap().kind, "Not in neighbor cache");
        assert_eq!(merge(&mut i, vec![n("10.0.0.3")], 103).unwrap(), (0, 1));
        assert_eq!(i.devices.values().next().unwrap().first, 100);
    }
}
