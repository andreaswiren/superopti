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
    let mut child = Command::new("powershell.exe")
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
    let start = Instant::now();
    loop {
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
            let mut out = String::new();
            let mut err = String::new();
            if let Some(mut pipe) = child.stdout.take() {
                let _ = pipe.read_to_string(&mut out);
            }
            if let Some(mut pipe) = child.stderr.take() {
                let _ = pipe.read_to_string(&mut err);
            }
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
    save_backup(&b)?;
    if results.is_empty() {
        results.push("No saved changes to undo.".into());
    }
    Ok(results.join("\r\n"))
}
pub fn checks() -> String {
    let anim = match animation() {
        Ok(true) => {
            "REVIEW: Client-area animations enabled. Optional fix reduces motion; benefit may be small."
        }
        Ok(false) => "OK: Client-area animations disabled.",
        Err(_) => "UNKNOWN: Animation setting unavailable.",
    };
    let power = match active_power() {
        Ok(g) if g == SAVER => {
            "REVIEW: Power saver is active. Balanced can improve responsiveness at a battery cost."
                .into()
        }
        Ok(g) => format!(
            "OK/REVIEW: Power plan {g:?}. Custom plans and power-mode overlays need manual review."
        ),
        Err(e) => format!("UNKNOWN: Power plan: {e}"),
    };
    let script = r#"
$ErrorActionPreference='Stop'
try {
 Get-CimInstance Win32_LogicalDisk -Filter 'DriveType=3' -OperationTimeoutSec 5 | ForEach-Object {
 if ($_.Size -gt 0) { $p=[math]::Round(100*$_.FreeSpace/$_.Size,1); $state=if($p -lt 15){'REVIEW'}else{'OK'}; "$state`: Drive $($_.DeviceID) has $p% free ($([math]::Round($_.FreeSpace/1GB,1)) GiB). Review storage if low." }
 }
} catch { 'UNKNOWN: Free disk space could not be queried.' }
try {
 $c=Get-CimInstance Win32_ComputerSystem -OperationTimeoutSec 5
 if($c.AutomaticManagedPagefile){'OK: System-managed pagefile enabled.'}else{'REVIEW: Pagefile is manually managed. Review Virtual memory; do not disable paging to reduce disk activity.'}
} catch { 'UNKNOWN: Pagefile policy unavailable.' }
try {
 $s=@(Get-CimInstance Win32_StartupCommand -OperationTimeoutSec 5)
 "REVIEW: $($s.Count) startup entries found (includes entries that may be disabled). Use Startup apps to choose what is needed."
} catch { 'UNKNOWN: Startup entries unavailable.' }
$r=(Test-Path 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Component Based Servicing\RebootPending') -or (Test-Path 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\WindowsUpdate\Auto Update\RebootRequired')
if($r){'REVIEW: Windows servicing indicates a pending restart. Save work and review Windows Update.'}else{'OK: No servicing/update restart flag found; this does not prove all updates are installed.'}
try {
 $os=Get-CimInstance Win32_OperatingSystem -OperationTimeoutSec 5
 "INFO: Uptime $([math]::Round(((Get-Date)-$os.LastBootUpTime).TotalDays,1)) days. Long uptime alone is not a fault."
} catch { 'UNKNOWN: Uptime unavailable.' }
"#;
    format!(
        "SYSTEM CHECKS / on demand\r\n\r\n{anim}\r\n\r\n{power}\r\n\r\n{}\r\n\r\nFix all applies ONLY the two reversible fixes above when applicable.\r\nStorage, startup, updates and paging require your choices in Windows.\r\nNo processes are terminated and no services, security features or pagefiles are disabled.",
        ps(script)
            .unwrap_or_else(|e| format!("Checks failed: {e}"))
            .replace('\n', "\r\n")
    )
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
