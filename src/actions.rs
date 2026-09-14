use serde::{Deserialize, Serialize};
use std::{
    io::Read,
    os::windows::process::CommandExt,
    path::PathBuf,
    process::{Command, Stdio},
    time::{Duration, Instant},
};
use windows::{
    Win32::{
        Foundation::{HLOCAL, LocalFree},
        System::Power::*,
        UI::WindowsAndMessaging::*,
    },
    core::{BOOL, GUID},
};

pub fn data_dir() -> PathBuf {
    PathBuf::from(std::env::var_os("LOCALAPPDATA").unwrap_or_default()).join("SuperOpti")
}
pub fn ps(script: &str) -> Result<String, String> {
    let mut system = [0u16; 32768];
    let length = unsafe {
        windows::Win32::System::SystemInformation::GetSystemDirectoryW(Some(&mut system))
    } as usize;
    if length == 0 || length >= system.len() {
        return Err("Windows system directory unavailable".into());
    }
    let executable = PathBuf::from(String::from_utf16_lossy(&system[..length]))
        .join("WindowsPowerShell/v1.0/powershell.exe");
    let mut child = Command::new(executable)
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            script,
        ])
        .creation_flags(0x08000000)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;
    fn drain(mut pipe: impl Read) -> String {
        let mut saved = Vec::new();
        let mut chunk = [0u8; 4096];
        while let Ok(count) = pipe.read(&mut chunk) {
            if count == 0 {
                break;
            }
            let keep = count.min(131072usize.saturating_sub(saved.len()));
            saved.extend_from_slice(&chunk[..keep]);
        }
        String::from_utf8_lossy(&saved).into_owned()
    }
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    let output = std::thread::spawn(move || drain(stdout));
    let errors = std::thread::spawn(move || drain(stderr));
    let start = Instant::now();
    loop {
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
            let out = output.join().unwrap_or_default();
            let err = errors.join().unwrap_or_default();
            return if status.success() {
                Ok(out.trim().into())
            } else {
                Err(format!("{} {}", out.trim(), err.trim()))
            };
        }
        if start.elapsed() > Duration::from_secs(30) {
            let _ = child.kill();
            let _ = child.wait();
            return Err("Operation timed out after 30 seconds.".into());
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}
fn quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}
fn active_power() -> Result<GUID, String> {
    unsafe {
        let mut ptr = std::ptr::null_mut();
        PowerGetActiveScheme(None, &mut ptr)
            .ok()
            .map_err(|e| e.to_string())?;
        if ptr.is_null() {
            return Err("No active power plan returned".into());
        }
        let guid = *ptr;
        let _ = LocalFree(Some(HLOCAL(ptr.cast())));
        Ok(guid)
    }
}
fn animation() -> Result<bool, String> {
    unsafe {
        let mut b = BOOL(0);
        SystemParametersInfoW(
            SPI_GETCLIENTAREAANIMATION,
            0,
            Some((&mut b as *mut BOOL).cast()),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )
        .map_err(|e| e.to_string())?;
        Ok(b.as_bool())
    }
}
const SAVER: GUID = GUID::from_u128(0xa1841308_3541_4fab_bc81_f71556f20b4a);
const BALANCED: GUID = GUID::from_u128(0x381b4222_f694_41f0_9685_ff5bb260df2e);
#[derive(Default, Serialize, Deserialize)]
struct Backup {
    animation: Option<bool>,
    power: Option<u128>,
}
fn backup_path() -> PathBuf {
    data_dir().join("fix-backup.json")
}
fn load_backup() -> Result<Backup, String> {
    match std::fs::read(backup_path()) {
        Ok(bytes) => {
            serde_json::from_slice(&bytes).map_err(|e| format!("Cannot read fix backup: {e}"))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Backup::default()),
        Err(e) => Err(e.to_string()),
    }
}
fn save_backup(b: &Backup) -> Result<(), String> {
    std::fs::create_dir_all(data_dir()).map_err(|e| e.to_string())?;
    let temp = data_dir().join("fix-backup.tmp");
    std::fs::write(
        &temp,
        serde_json::to_vec_pretty(b).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    std::fs::rename(temp, backup_path()).map_err(|e| e.to_string())
}
pub fn fix(which: u32) -> Result<String, String> {
    let mut b = load_backup()?;
    let mut log = Vec::new();
    if which == 1 || which == 3 {
        let result = (|| -> Result<String, String> {
            let current = animation()?;
            if !current {
                return Ok("Client-area animations already off.".into());
            }
            if b.animation.is_none() {
                b.animation = Some(current);
                save_backup(&b)?;
            }
            unsafe {
                SystemParametersInfoW(
                    SPI_SETCLIENTAREAANIMATION,
                    0,
                    None,
                    SPIF_UPDATEINIFILE | SPIF_SENDCHANGE,
                )
                .map_err(|e| e.to_string())?;
            }
            if animation()? {
                return Err("Windows did not retain the animation change.".into());
            }
            Ok("Client-area animations disabled; original value saved for Undo.".into())
        })();
        log.push(result.unwrap_or_else(|e| format!("Animation fix failed: {e}")));
    }
    if which == 2 || which == 3 {
        let result = (|| -> Result<String, String> {
            let current = active_power()?;
            if current != SAVER {
                return Ok("Power plan unchanged: not the standard Power saver plan.".into());
            }
            if b.power.is_none() {
                b.power = Some(current.to_u128());
                save_backup(&b)?;
            }
            unsafe {
                PowerSetActiveScheme(None, Some(&BALANCED))
                    .ok()
                    .map_err(|e| e.to_string())?;
            }
            if active_power()? != BALANCED {
                return Err("Windows did not retain the power plan change.".into());
            }
            Ok("Switched Power saver to Balanced; original plan saved for Undo.".into())
        })();
        log.push(result.unwrap_or_else(|e| format!("Power fix failed: {e}")));
    }
    if which == 4 || which == 3 {
        log.push(pagefile("Apply").unwrap_or_else(|e| format!("Pagefile fix failed: {e}")));
    }
    Ok(log.join("\r\n"))
}
pub fn undo() -> Result<String, String> {
    let mut b = load_backup()?;
    let mut results = Vec::new();
    if let Some(value) = b.animation {
        let r = unsafe {
            SystemParametersInfoW(
                SPI_SETCLIENTAREAANIMATION,
                0,
                Some((value as usize) as *mut _),
                SPIF_UPDATEINIFILE | SPIF_SENDCHANGE,
            )
        };
        match r {
            Ok(()) => {
                b.animation = None;
                results.push("Animations restored.".to_string());
            }
            Err(e) => results.push(format!("Animation restore failed: {e}")),
        }
    }
    if let Some(value) = b.power {
        let r = unsafe { PowerSetActiveScheme(None, Some(&GUID::from_u128(value))).ok() };
        match r {
            Ok(()) => {
                b.power = None;
                results.push("Power plan restored.".to_string());
            }
            Err(e) => results.push(format!("Power restore failed: {e}")),
        }
    }
    results.push(pagefile("Undo").unwrap_or_else(|e| format!("Pagefile restore failed: {e}")));
    save_backup(&b)?;
    if results.is_empty() {
        results.push("No saved changes to undo.".into());
    }
    Ok(results.join("\r\n"))
}
pub fn check_report() -> crate::model::Report {
    let mut r = crate::model::Report::new(
        "System checks",
        &["Status", "Check", "Current", "Target / context", "Action"],
    );
    match animation() {
        Ok(enabled) => r.row(&[
            if enabled { "Optional" } else { "OK" },
            "Client animations",
            if enabled { "Enabled" } else { "Disabled" },
            "Optional reduced motion",
            "Reduce animations",
        ]),
        Err(_) => r.row(&[
            "Unknown",
            "Client animations",
            "N/A",
            "Access unavailable",
            "",
        ]),
    }
    match active_power() {
        Ok(g) if g == SAVER => r.row(&[
            "Review",
            "Power plan",
            "Power saver",
            "Balanced",
            "Use Balanced power",
        ]),
        Ok(g) if g == BALANCED => r.row(&["OK", "Power plan", "Balanced", "Balanced", ""]),
        Ok(_) => r.row(&[
            "Review",
            "Power plan",
            "Custom / other",
            "Preserved",
            "Power settings",
        ]),
        Err(_) => r.row(&["Unknown", "Power plan", "N/A", "Access unavailable", ""]),
    }
    let rows = ps(include_str!("../scripts/Checks.ps1"))
        .and_then(|s| serde_json::from_str::<Vec<Vec<String>>>(&s).map_err(|e| e.to_string()));
    match rows {
        Ok(rows) => r.rows.extend(rows),
        Err(e) => r.row(&["Unknown", "Windows checks", "Unavailable", &e, "Retry"]),
    }
    match pagefile("Inspect")
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).map_err(|e| e.to_string()))
    {
        Ok(p) => {
            let target = p["targetMiB"].as_f64().unwrap_or(0.0) / 1024.0;
            let actual = p["allocatedMiB"].as_f64().unwrap_or(0.0) / 1024.0;
            r.row(&[
                if p["matched"] == true {
                    "OK"
                } else {
                    "Warning"
                },
                "Fixed pagefile",
                &format!("{actual:.1} GiB allocated"),
                &format!("{target:.1} GiB initial = maximum"),
                "Set fixed pagefile",
            ]);
            r.row(&[
                if p["spaceOK"] == true {
                    "OK"
                } else {
                    "Blocked"
                },
                "Pagefile disk space",
                if p["spaceOK"] == true {
                    "Sufficient"
                } else {
                    "Insufficient"
                },
                &format!(
                    "Growth {:.1} + reserve {:.1} GiB",
                    p["growthGiB"].as_f64().unwrap_or(0.0),
                    p["reserveGiB"].as_f64().unwrap_or(0.0)
                ),
                "Storage",
            ]);
            if p["supported"] != true {
                r.row(&[
                    "Blocked",
                    "Pagefile layout",
                    "Multiple / custom",
                    "Preserved",
                    "Virtual memory",
                ]);
            }
        }
        Err(e) => r.row(&["Unknown", "Pagefile", "Unavailable", &e, "Retry"]),
    }
    // Put actionable deviations first; optional appearance preferences do not
    // outrank low disk space or a blocked pagefile change.
    r.rows
        .sort_by_key(|row| match row.first().map(String::as_str) {
            Some("Critical") => 0,
            Some("Blocked") => 1,
            Some("Warning") => 2,
            Some("Review") => 3,
            Some("Unknown") | None => 4,
            Some("Optional") => 5,
            Some("OK") => 6,
            _ => 7,
        });
    for (label, statuses) in [
        ("Checks OK", &["OK"][..]),
        ("Deviations", &["Warning", "Critical"][..]),
        ("Blocked", &["Blocked"][..]),
        ("Unavailable", &["Unknown"][..]),
        ("Optional / review", &["Optional", "Review"][..]),
    ] {
        r.metric(
            label,
            r.rows
                .iter()
                .filter(|row| row.first().is_some_and(|v| statuses.contains(&v.as_str())))
                .count(),
        );
    }
    let penalty: i32 = r
        .rows
        .iter()
        .map(|row| match row.first().map(String::as_str) {
            Some("Critical") => 25,
            Some("Blocked") => 20,
            Some("Warning") => 10,
            Some("Unknown") => 5,
            Some("Review") => 3,
            Some("Optional") => 1,
            _ => 0,
        })
        .sum();
    r.metric(
        "System health",
        format!("{}%", (100 - penalty).clamp(0, 100)),
    );
    r
}
pub fn autostart(enable: bool) -> Result<String, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let value = format!("\"{}\" --tray", exe.display());
    let script = if enable {
        format!(
            "$ErrorActionPreference='Stop'; $k='HKCU:\\Software\\Microsoft\\Windows\\CurrentVersion\\Run'; New-Item -Path $k -Force | Out-Null; New-ItemProperty -Path $k -Name SuperOpti -Value {} -PropertyType String -Force | Out-Null",
            quote(&value)
        )
    } else {
        "$ErrorActionPreference='Stop'; $k='HKCU:\\Software\\Microsoft\\Windows\\CurrentVersion\\Run'; if(Get-ItemProperty -Path $k -Name SuperOpti -ErrorAction SilentlyContinue){Remove-ItemProperty -Path $k -Name SuperOpti}".into()
    };
    ps(&script)?;
    Ok(if enable {"Autostart enabled for this user. Starts in tray with monitoring OFF. Keep the executable at its current location, or install first."}else{"Autostart disabled for this user."}.into())
}
pub fn install() -> Result<String, String> {
    let dir = data_dir();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let script = dir.join("Install.ps1");
    std::fs::write(&script, include_str!("../Install.ps1")).map_err(|e| e.to_string())?;
    std::fs::write(dir.join("Uninstall.ps1"), include_str!("../Uninstall.ps1"))
        .map_err(|e| e.to_string())?;
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    ps(&format!(
        "& {} -SourceExe {}",
        quote(&script.to_string_lossy()),
        quote(&exe.to_string_lossy())
    ))
}

fn pagefile(mode: &str) -> Result<String, String> {
    ps(&format!(
        "& {{ {} }} -Mode {}",
        include_str!("../scripts/Pagefile.ps1"),
        quote(mode)
    ))
}
