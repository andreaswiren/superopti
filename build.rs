use std::{env, fs, path::PathBuf, process::Command};

fn main() {
    for file in [
        "assets/superopti.ico",
        "assets/superopti-active.ico",
        "resources/app.manifest",
    ] {
        println!("cargo:rerun-if-changed={file}");
    }
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let root = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let path = |p: &str| root.join(p).to_string_lossy().replace('\\', "/");
    let version = env::var("CARGO_PKG_VERSION").unwrap();
    let numeric = format!("{},0", version.replace('.', ","));
    let rc = format!(
        r#"101 ICON "{}"
102 ICON "{}"
1 24 "{}"
1 VERSIONINFO
FILEVERSION {numeric}
PRODUCTVERSION {numeric}
FILEOS 0x40004
FILETYPE 1
BEGIN
 BLOCK "StringFileInfo"
 BEGIN
  BLOCK "040904b0"
  BEGIN
   VALUE "CompanyName", "SuperOpti contributors\0"
   VALUE "FileDescription", "SuperOpti performance diagnostics\0"
   VALUE "FileVersion", "{version}\0"
   VALUE "ProductName", "SuperOpti\0"
   VALUE "ProductVersion", "{version}\0"
  END
 END
 BLOCK "VarFileInfo"
 BEGIN
  VALUE "Translation", 0x409, 1200
 END
END
"#,
        path("assets/superopti.ico"),
        path("assets/superopti-active.ico"),
        path("resources/app.manifest")
    );
    let script = out.join("superopti.rc");
    let resource = out.join("superopti.res");
    fs::write(&script, rc).unwrap();
    let sdk = PathBuf::from(
        env::var_os("ProgramFiles(x86)").unwrap_or_else(|| "C:/Program Files (x86)".into()),
    )
    .join("Windows Kits/10/bin");
    let mut versions: Vec<_> = fs::read_dir(sdk)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .map(|e| e.path().join("x64/rc.exe"))
        .filter(|p| p.exists())
        .collect();
    versions.sort();
    let compiler = versions.pop().unwrap_or_else(|| PathBuf::from("rc.exe"));
    let status = Command::new(compiler)
        .arg("/nologo")
        .arg("/fo")
        .arg(&resource)
        .arg(&script)
        .status()
        .expect("Windows SDK resource compiler rc.exe is required");
    assert!(status.success(), "Windows resource compilation failed");
    println!("cargo:rustc-link-arg={}", resource.display());
    // The checked-in manifest is embedded by rc.exe; prevent a duplicate linker manifest.
    println!("cargo:rustc-link-arg=/MANIFEST:NO");
}
