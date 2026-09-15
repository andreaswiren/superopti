use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    time::{SystemTime, UNIX_EPOCH},
};
use windows::{
    Win32::System::{Performance::*, SystemInformation::*},
    core::PCWSTR,
};
pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}
pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
pub(crate) fn trace(message: &str) {
    if let Some(path) = std::env::var_os("SUPEROPTI_TRACE") {
        use std::io::Write;
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
        {
            let _ = writeln!(file, "{} {message}", now());
        }
    }
}

// Query ownership stays on the worker thread. Dropping closes every counter.
pub struct Query {
    handle: PDH_HQUERY,
    counters: HashMap<String, PDH_HCOUNTER>,
}
impl Query {
    pub fn new() -> Result<Self, String> {
        unsafe {
            let mut handle = PDH_HQUERY::default();
            let code = PdhOpenQueryW(PCWSTR::null(), 0, &mut handle);
            if code != 0 {
                return Err(format!("PDH query failed: 0x{code:08x}"));
            }
            Ok(Self {
                handle,
                counters: HashMap::new(),
            })
        }
    }
    pub fn add(&mut self, key: &str, path: &str) -> bool {
        unsafe {
            trace(&format!("Adding {key}: {path}"));
            let mut h = PDH_HCOUNTER::default();
            let p = wide(path);
            let code = PdhAddEnglishCounterW(self.handle, PCWSTR(p.as_ptr()), 0, &mut h);
            trace(&format!("Added {key}: 0x{code:08x}"));
            if code != 0 {
                return false;
            }
            self.counters.insert(key.into(), h);
            true
        }
    }
    pub fn collect(&self) -> Result<(), String> {
        unsafe {
            trace("Collecting query");
            let code = PdhCollectQueryData(self.handle);
            trace(&format!("Collected query: 0x{code:08x}"));
            if code == 0 {
                Ok(())
            } else {
                Err(format!("Counter collection failed: 0x{code:08x}"))
            }
        }
    }
    pub fn scalar(&self, key: &str) -> Option<f64> {
        unsafe {
            let mut v = PDH_FMT_COUNTERVALUE::default();
            let h = *self.counters.get(key)?;
            if PdhGetFormattedCounterValue(h, PDH_FMT(PDH_FMT_DOUBLE.0 | 0x00008000), None, &mut v)
                != 0
            {
                return None;
            }
            valid(v)
        }
    }
    pub fn array(&self, key: &str) -> HashMap<String, f64> {
        unsafe {
            let mut result = HashMap::new();
            let Some(&h) = self.counters.get(key) else {
                return result;
            };
            // u64 storage provides the alignment required by PDH's item structure.
            for _ in 0..3 {
                let (mut bytes, mut count) = (0, 0);
                PdhGetFormattedCounterArrayW(
                    h,
                    PDH_FMT(PDH_FMT_DOUBLE.0 | 0x00008000),
                    &mut bytes,
                    &mut count,
                    None,
                );
                if bytes == 0 || bytes > 64 * 1024 * 1024 {
                    break;
                }
                let mut buffer = vec![0u64; (bytes as usize).div_ceil(8)];
                let ptr = buffer.as_mut_ptr().cast::<PDH_FMT_COUNTERVALUE_ITEM_W>();
                let code = PdhGetFormattedCounterArrayW(
                    h,
                    PDH_FMT(PDH_FMT_DOUBLE.0 | 0x00008000),
                    &mut bytes,
                    &mut count,
                    Some(ptr),
                );
                if code != 0 {
                    continue;
                }
                if count as usize * size_of::<PDH_FMT_COUNTERVALUE_ITEM_W>() > buffer.len() * 8 {
                    break;
                }
                for item in std::slice::from_raw_parts(ptr, count as usize) {
                    if let Some(value) = valid(item.FmtValue)
                        && !item.szName.is_null()
                    {
                        result.insert(item.szName.to_string().unwrap_or_default(), value);
                    }
                }
                break;
            }
            result
        }
    }
}
fn valid(v: PDH_FMT_COUNTERVALUE) -> Option<f64> {
    unsafe {
        let n = v.Anonymous.doubleValue;
        ((v.CStatus == 0 || v.CStatus == 1) && n.is_finite() && n >= 0.0).then_some(n)
    }
}
impl Drop for Query {
    fn drop(&mut self) {
        unsafe {
            PdhCloseQuery(self.handle);
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Process {
    #[serde(default)]
    pub parent_pid: u32,
    #[serde(default)]
    pub commit_mb: Option<f64>,
    #[serde(default)]
    pub created_ticks: Option<u64>,
    pub pid: u32,
    pub name: String,
    pub cpu: Option<f64>,
    pub ram_mb: Option<f64>,
    pub io_mb: Option<f64>,
    pub threads: Option<f64>,
    pub handles: Option<f64>,
    pub gpu: Option<f64>,
    pub score: f64,
    pub reason: String,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Sample {
    #[serde(default)]
    pub commit_limit_mb: Option<f64>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub cores: HashMap<String, f64>,
    pub unix_seconds: u64,
    pub elapsed: f64,
    pub cpu: Option<f64>,
    pub ram: Option<f64>,
    pub commit: Option<f64>,
    pub swap: Option<f64>,
    pub gpu: Option<f64>,
    pub disk: Option<f64>,
    pub disk_mb: Option<f64>,
    pub disk_latency_ms: Option<f64>,
    pub disk_queue: Option<f64>,
    pub page_reads: Option<f64>,
    pub cpu_queue: Option<f64>,
    pub dpc: Option<f64>,
    pub network_mb: Option<f64>,
    #[serde(default)]
    pub network_in_mb: Option<f64>,
    #[serde(default)]
    pub network_out_mb: Option<f64>,
    pub context_switches: Option<f64>,
    pub total_ram_mb: f64,
    pub processes: Vec<Process>,
    pub collection_ms: f64,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub gpu_processes: HashMap<u32, f64>,
}
pub struct Monitor {
    q: Query,
}
impl Monitor {
    pub fn new(group: &str) -> Result<Self, String> {
        let mut q = Query::new()?;
        for (key, path) in [
            ("cpu", r"\Processor(_Total)\% Processor Time"),
            ("commit", r"\Memory\% Committed Bytes In Use"),
            ("swap", r"\Paging File(_Total)\% Usage"),
            ("disk_idle", r"\PhysicalDisk(_Total)\% Idle Time"),
            ("disk_mb", r"\PhysicalDisk(_Total)\Disk Bytes/sec"),
            ("latency", r"\PhysicalDisk(_Total)\Avg. Disk sec/Transfer"),
            (
                "disk_queue",
                r"\PhysicalDisk(_Total)\Current Disk Queue Length",
            ),
            ("pages", r"\Memory\Pages Input/sec"),
            ("cpu_queue", r"\System\Processor Queue Length"),
            ("dpc", r"\Processor(_Total)\% DPC Time"),
            ("switch", r"\System\Context Switches/sec"),
            ("net", r"\Network Interface(*)\Bytes Total/sec"),
            ("net_in", r"\Network Interface(*)\Bytes Received/sec"),
            ("net_out", r"\Network Interface(*)\Bytes Sent/sec"),
            ("gpu", r"\GPU Engine(*)\Utilization Percentage"),
            ("cores", r"\Processor Information(*)\% Processor Time"),
        ] {
            let category = match key {
                "disk_idle" | "disk_mb" | "latency" | "disk_queue" => "disk",
                "gpu" => "gpu",
                "cores" => "cores",
                "net" | "net_in" | "net_out" => "network",
                _ => "extra",
            };
            if category == group {
                q.add(key, path);
            }
        }
        q.collect()?;
        Ok(Self { q })
    }
    pub fn sample(&self, elapsed: f64) -> Result<Sample, String> {
        let started = std::time::Instant::now();
        self.q.collect()?;
        let q = &self.q;
        let mut m = MEMORYSTATUSEX {
            dwLength: size_of::<MEMORYSTATUSEX>() as u32,
            ..Default::default()
        };
        let mem_ok = unsafe { GlobalMemoryStatusEx(&mut m) }.is_ok();
        let gpu_raw = q.array("gpu");
        let mut gpu_by_pid: HashMap<u32, f64> = HashMap::new();
        let mut gpu_by_engine: HashMap<String, f64> = HashMap::new();
        for (name, value) in &gpu_raw {
            if let Some((pid, engine)) = gpu_identity(name) {
                *gpu_by_pid.entry(pid).or_default() += value;
                *gpu_by_engine.entry(engine).or_default() += value;
            }
        }
        let mut s = Sample {
            unix_seconds: now(),
            elapsed,
            cpu: q.scalar("cpu"),
            ram: mem_ok.then_some(m.dwMemoryLoad as f64),
            total_ram_mb: m.ullTotalPhys as f64 / 1048576.0,
            commit: q.scalar("commit"),
            swap: q.scalar("swap"),
            gpu: gpu_by_engine
                .values()
                .copied()
                .reduce(f64::max)
                .map(|v| v.min(100.0)),
            disk: q.scalar("disk_idle").map(|v| (100.0 - v).clamp(0.0, 100.0)),
            disk_mb: q.scalar("disk_mb").map(|v| v / 1048576.0),
            disk_latency_ms: q.scalar("latency").map(|v| v * 1000.0),
            disk_queue: q.scalar("disk_queue"),
            page_reads: q.scalar("pages"),
            cpu_queue: q.scalar("cpu_queue"),
            dpc: q.scalar("dpc"),
            context_switches: q.scalar("switch"),
            network_mb: {
                let a = q.array("net");
                (!a.is_empty()).then(|| a.values().sum::<f64>() / 1048576.0)
            },
            network_in_mb: {
                let a = q.array("net_in");
                (!a.is_empty()).then(|| a.values().sum::<f64>() / 1048576.)
            },
            network_out_mb: {
                let a = q.array("net_out");
                (!a.is_empty()).then(|| a.values().sum::<f64>() / 1048576.)
            },
            ..Default::default()
        };
        s.gpu_processes = gpu_by_pid;
        s.cores = q
            .array("cores")
            .into_iter()
            .filter(|(name, _)| !name.contains("_Total"))
            .map(|(name, v)| (name, v.clamp(0., 100.)))
            .collect();
        s.collection_ms = started.elapsed().as_secs_f64() * 1000.0;
        Ok(s)
    }
}
pub fn gpu_identity(name: &str) -> Option<(u32, String)> {
    let rest = name.strip_prefix("pid_")?;
    let (pid, engine) = rest.split_once('_')?;
    Some((pid.parse().ok()?, engine.into()))
}
pub fn rank(p: &mut Process, s: &Sample) {
    let memory_share = if s.total_ram_mb > 0.0 {
        p.ram_mb.unwrap_or(0.0) / s.total_ram_mb * 100.0
    } else {
        0.0
    };
    let commit_share = p
        .commit_mb
        .zip(s.commit_limit_mb)
        .filter(|(_, limit)| *limit > 0.)
        .map_or(0., |(used, limit)| 100. * used / limit);
    let candidates = [
        (
            (commit_share
                * if s.commit.unwrap_or(0.) >= 85. {
                    1.
                } else {
                    0.2
                })
            .min(100.),
            "Commit footprint",
        ),
        (p.cpu.unwrap_or(0.0), "CPU"),
        (
            memory_share
                * if s.ram.unwrap_or(0.0) >= 85.0 {
                    1.0
                } else {
                    0.2
                },
            "RAM footprint",
        ),
        (
            (p.io_mb.unwrap_or(0.0) * 2.0).min(100.0)
                * if s.disk.unwrap_or(0.0) >= 80.0 {
                    1.0
                } else {
                    0.2
                },
            "I/O activity",
        ),
        (p.gpu.unwrap_or(0.0), "GPU engines"),
    ];
    let (score, reason) = candidates
        .into_iter()
        .max_by(|a, b| a.0.total_cmp(&b.0))
        .unwrap();
    p.score = score;
    p.reason = if score < 0.1 {
        "Low activity".into()
    } else {
        reason.into()
    };
}
pub fn fmt(v: Option<f64>, suffix: &str) -> String {
    v.map(|n| format!("{n:.1}{suffix}"))
        .unwrap_or_else(|| "N/A".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_gpu_engine_identity() {
        assert_eq!(
            gpu_identity("pid_123_luid_0x0_0x123_phys_0_eng_1_engtype_3D"),
            Some((123, "luid_0x0_0x123_phys_0_eng_1_engtype_3D".into()))
        );
        assert!(gpu_identity("bad").is_none());
    }
    #[test]
    fn memory_pressure_changes_ranking() {
        let mut p = Process {
            ram_mb: Some(8000.0),
            cpu: Some(20.0),
            ..Default::default()
        };
        let mut s = Sample {
            total_ram_mb: 16000.0,
            ram: Some(50.0),
            ..Default::default()
        };
        rank(&mut p, &s);
        assert_eq!(p.reason, "CPU");
        s.ram = Some(90.0);
        rank(&mut p, &s);
        assert_eq!(p.reason, "RAM footprint");
    }
    #[test]
    fn commit_pressure_changes_ranking_without_calling_it_swap() {
        let mut p = Process {
            commit_mb: Some(8000.),
            cpu: Some(20.),
            ..Default::default()
        };
        let mut system = Sample {
            commit_limit_mb: Some(16000.),
            commit: Some(60.),
            ..Default::default()
        };
        rank(&mut p, &system);
        assert_eq!(p.reason, "CPU");
        system.commit = Some(90.);
        rank(&mut p, &system);
        assert_eq!(p.reason, "Commit footprint");
        assert_eq!(p.score, 50.);
        system.commit_limit_mb = None;
        rank(&mut p, &system);
        assert_eq!(p.reason, "CPU");
    }
    #[test]
    fn missing_counters_are_not_claimed_as_zero() {
        assert_eq!(fmt(None, "%"), "N/A");
        let mut p = Process::default();
        rank(&mut p, &Sample::default());
        assert_eq!(p.score, 0.0);
    }
}
