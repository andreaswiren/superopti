#!/usr/bin/env python3
"""Generate and validate a full resolved-Cargo CycloneDX 1.5 BOM plus build/runtime inventory.
Uses cargo-cyclonedx 0.5.9 as the upstream generator; no application telemetry is read.
"""
from __future__ import annotations
import argparse, datetime as dt, hashlib, json, os, pathlib, re, shutil, struct, subprocess, sys, tomllib, uuid
from urllib.parse import quote
ROOT = pathlib.Path(__file__).resolve().parents[1]
SCHEMA = ROOT / "scripts" / "schemas"

def run(*args: str) -> str:
    return subprocess.check_output(args, cwd=ROOT, text=True, encoding="utf-8", errors="strict").strip()

def digest(path: pathlib.Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()

def save(path: pathlib.Path, value) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")

def imports(path: pathlib.Path) -> list[str]:
    """Read direct PE imports without executing the supplied binary."""
    data = path.read_bytes()
    if data[:2] != b"MZ": raise ValueError("Not a PE executable")
    pe = struct.unpack_from("<I", data, 0x3c)[0]
    if data[pe:pe+4] != b"PE\0\0": raise ValueError("Invalid PE header")
    sections, optsize = struct.unpack_from("<H",data,pe+6)[0],struct.unpack_from("<H",data,pe+20)[0]
    opt = pe+24
    magic = struct.unpack_from("<H",data,opt)[0]
    directory = opt + (112 if magic == 0x20b else 96)
    rva, length = struct.unpack_from("<II",data,directory+8)
    section_start = opt+optsize
    def offset(address):
        for i in range(sections):
            s = section_start + 40*i
            virtual_size, virtual_address, raw_size, raw_address = struct.unpack_from("<IIII",data,s+8)
            if virtual_address <= address < virtual_address+max(virtual_size,raw_size):
                result = raw_address+address-virtual_address
                if not 0 <= result < len(data): raise ValueError("PE address outside file")
                return result
        raise ValueError("Unknown PE address")
    if not rva: return []
    pos, result = offset(rva), set()
    for _ in range(min(length//20+1,4096)):
        fields=struct.unpack_from("<IIIII",data,pos)
        if not any(fields): break
        start=offset(fields[3]); end=data.index(b"\0",start,min(start+512,len(data)))
        result.add(data[start:end].decode("ascii").lower()); pos+=20
    return sorted(result)

def validate(bom: dict) -> None:
    import jsonschema
    from referencing import Registry, Resource
    store = Registry()
    for name in ["bom-1.5.schema.json", "spdx.schema.json", "jsf-0.82.schema.json"]:
        raw=json.loads((SCHEMA/name).read_text(encoding="utf-8"))
        resource=Resource.from_contents(raw)
        for uri in [raw.get("$id", ""), "http://cyclonedx.org/schema/"+name,"https://cyclonedx.org/schema/"+name]:
            if uri: store=store.with_resource(uri,resource)
    schema=json.loads((SCHEMA/"bom-1.5.schema.json").read_text(encoding="utf-8"))
    jsonschema.Draft7Validator(schema, registry=store).validate(bom)
    seen=set()
    def visit(c):
        ref=c.get("bom-ref")
        if ref:
            if ref in seen: raise ValueError(f"Duplicate bom-ref: {ref}")
            seen.add(ref)
        for child in c.get("components",[]): visit(child)
    visit(bom["metadata"]["component"])
    for component in bom.get("components",[]): visit(component)
    for dep in bom["dependencies"]:
        if dep["ref"] not in seen or not set(dep.get("dependsOn",[])) <= seen:
            raise ValueError(f"Dangling dependency: {dep}")
    serialized=json.dumps(bom)
    if "file:///" in serialized or "C:/Users/" in serialized or "C:\\\\Users\\\\" in serialized:
        raise ValueError("Local filesystem paths must not appear in published BOMs")

def generate(output: pathlib.Path, binary: pathlib.Path | None) -> None:
    lock_before=digest(ROOT/"Cargo.lock")
    metadata=json.loads(run("cargo","metadata","--locked","--format-version","1"))
    tool=run("cargo","cyclonedx","--version")
    if not tool.endswith("0.5.9"): raise RuntimeError("Install pinned cargo-cyclonedx 0.5.9")
    run("cargo","cyclonedx","--all","--target","all","--format","json","--spec-version","1.5","--override-filename","superopti-cargo")
    raw_file=ROOT/"superopti-cargo.json"
    bom=json.loads(raw_file.read_text(encoding="utf-8"))
    # Move intermediate data under work; it contains Cargo's machine-local package IDs.
    work=ROOT/"work";work.mkdir(exist_ok=True)
    raw_file.replace(work/"superopti-cargo.json")
    if digest(ROOT/"Cargo.lock") != lock_before: raise RuntimeError("SBOM tool changed the lockfile")
    lock=tomllib.loads((ROOT/"Cargo.lock").read_text(encoding="utf-8"))
    packages={(p["name"],p["version"]):p for p in metadata["packages"]}
    root=packages[("superopti",tomllib.loads((ROOT/"Cargo.toml").read_text())["package"]["version"])]
    version=root["version"]; rootref=f"pkg:generic/superopti@{version}"
    oldroot=bom["metadata"]["component"]["bom-ref"]
    mapping={oldroot:rootref}
    for c in bom["components"]: mapping[c["bom-ref"]]=c["purl"]
    for i,c in enumerate(bom["metadata"]["component"].get("components",[])):
        mapping[c["bom-ref"]]=f"{rootref}#cargo-target-{i}"
    def rewrite(value):
        if isinstance(value,dict): return {k:rewrite(v) for k,v in value.items()}
        if isinstance(value,list): return [rewrite(v) for v in value]
        if isinstance(value,str):
            if value in mapping: return mapping[value]
            if value.startswith("pkg:cargo/superopti@"): return rootref
        return value
    bom=rewrite(bom)
    bom["metadata"]["component"]["purl"]=rootref
    required={(p["name"],p["version"]):p for p in lock["package"] if p.get("source")}
    actual={(p["name"],p["version"]):p for p in bom["components"]}
    if set(required)!=set(actual): raise ValueError(f"Incomplete Cargo inventory: {set(required)^set(actual)}")
    output.mkdir(parents=True,exist_ok=True)
    direct={p["name"] for p in root["dependencies"]}
    inventory=[]; notices=["# Third-party notices\n\nGenerated from every resolved Cargo package, including build/proc-macro dependencies. Declared SPDX expressions and upstream license texts follow. These sources may offer alternative license choices. The Rust standard library and Windows runtime are listed separately in the inventory; Windows DLLs are supplied by Windows and not redistributed.\n"]
    licenses_dir=output/"licenses";licenses_dir.mkdir(exist_ok=True)
    for key,c in sorted(actual.items()):
        p=packages[key]; locked=required[key]; checksum=locked["checksum"]
        if not any(h["alg"]=="SHA-256" and h["content"]==checksum for h in c.get("hashes",[])):
            raise ValueError(f"Missing or mismatched lockfile checksum: {key}")
        manifest=pathlib.Path(p["manifest_path"])
        archive=manifest.parents[3]/"cache"/manifest.parents[1].name/f"{p['name']}-{p['version']}.crate"
        if not archive.exists() or digest(archive)!=checksum: raise ValueError(f"Cached crate archive not verified: {key}; run cargo fetch --locked")
        rootdir=manifest.parent
        candidates={f for f in rootdir.iterdir() if f.is_file() and re.match(r"^(licen[cs]e|copying|copyright|notice|unlicense)",f.name,re.I)}
        if p.get("license_file"): candidates.add(rootdir/p["license_file"])
        if not candidates: raise ValueError(f"No upstream license text found: {key}")
        license_files=[]; folder=licenses_dir/f"{p['name']}-{p['version']}";folder.mkdir(exist_ok=True)
        for source in sorted(candidates):
            shutil.copyfile(source,folder/source.name)
            license_files.append({"file":f"licenses/{folder.name}/{source.name}","sha256":digest(source)})
        kind="proc-macro/build host" if any("proc-macro" in t["kind"] for t in p["targets"]) else "Cargo dependency (may be runtime or build-transitive)"
        record={"name":p["name"],"version":p["version"],"purl":c["purl"],"direct":p["name"] in direct,"category":kind,"declared_license":p.get("license"),"archive_sha256":checksum,"archive_verified":True,"source":p["source"],"repository":p.get("repository"),"license_files":license_files}
        inventory.append(record)
        c.setdefault("properties",[]).extend([{"name":"superopti:direct-dependency","value":str(record["direct"]).lower()},{"name":"superopti:dependency-category","value":kind},{"name":"superopti:crate-archive-verified","value":"true"}])
        notices.append(f"\n## {p['name']} {p['version']}\n\nDeclared license: `{p.get('license')}`. Source: {p.get('repository') or 'https://crates.io/crates/'+p['name']}.\n")
        for source in sorted(candidates):
            notices.append(f"\n### {source.name}\n\n```text\n{source.read_text(encoding='utf-8',errors='replace').rstrip()}\n```\n")
    lucide_revision=(ROOT/"assets/lucide/REVISION").read_text(encoding="utf-8-sig").strip()
    lucide_license=ROOT/"assets/lucide/LICENSE"
    lucide_folder=licenses_dir/"lucide";lucide_folder.mkdir(exist_ok=True)
    shutil.copyfile(lucide_license,lucide_folder/"LICENSE")
    notices.append(f"\n## Lucide native interface icons\n\nRevision: `{lucide_revision}`. ISC license; minimize-2 also includes Feather MIT terms. Adapted for the SuperOpti app/tray brand mark and native interface.\n\n```text\n{lucide_license.read_text(encoding='utf-8')}\n```\n")
    (output/"THIRD_PARTY_NOTICES.md").write_text("".join(notices),encoding="utf-8")
    rust=run("rustc","-Vv"); rustver=re.search(r"^release: (.+)$",rust,re.M)[1]
    target="x86_64-pc-windows-msvc"
    sysroot=pathlib.Path(run("rustc","--print","sysroot"))
    rustdocs=sysroot/"share/doc/rust"
    rustlicenses=output/"rust-toolchain-licenses"
    shutil.copytree(rustdocs/"licenses",rustlicenses,dirs_exist_ok=True)
    shutil.copyfile(rustdocs/"COPYRIGHT-library.html",rustlicenses/"COPYRIGHT-library.html")
    sysroot_files=[{"name":p.name,"sha256":digest(p)} for p in sorted((sysroot/"lib/rustlib"/target/"lib").glob("*.rlib"))]

    stdref=f"pkg:generic/rust-std@{quote(rustver)}?target={target}"
    bom["components"].append({"type":"library","bom-ref":stdref,"name":"Rust standard library","version":rustver,"purl":stdref,"licenses":[{"expression":"MIT OR Apache-2.0"}],"properties":[{"name":"superopti:delivery","value":"toolchain-provided; Rust standard library code may be statically linked; not a Cargo.lock package"}]})
    bom["dependencies"].append({"ref":stdref,"dependsOn":[]})
    rootdeps=next(d for d in bom["dependencies"] if d["ref"]==rootref)
    rootdeps["dependsOn"].append(stdref)
    lucide_ref=f"pkg:github/lucide-icons/lucide@{lucide_revision}#icons/audio-lines.svg"
    bom["components"].append({"type":"file","bom-ref":lucide_ref,"name":"Lucide audio-lines","version":lucide_revision,"purl":lucide_ref,"licenses":[{"license":{"id":"ISC"}}],"hashes":[{"alg":"SHA-256","content":digest(ROOT/"assets/lucide/audio-lines.svg")}],"properties":[{"name":"superopti:delivery","value":"Adapted icon embedded in executable and shipped as SVG, PNG and ICO"}]})
    bom["dependencies"].append({"ref":lucide_ref,"dependsOn":[]});rootdeps["dependsOn"].append(lucide_ref)
    for asset in sorted((ROOT/"assets/lucide").glob("*.svg")):
        if asset.stem == "audio-lines": continue
        ref=f"pkg:github/lucide-icons/lucide@{lucide_revision}#icons/{asset.name}"
        bom["components"].append({"type":"file","bom-ref":ref,"name":f"Lucide {asset.stem}","version":lucide_revision,"purl":ref,"licenses":[{"expression":"ISC AND MIT" if asset.stem == "minimize-2" else "ISC"}],"hashes":[{"alg":"SHA-256","content":digest(asset)}],"properties":[{"name":"superopti:delivery","value":"Adapted native GDI stroke geometry; pinned source SVG included"}]})
        bom["dependencies"].append({"ref":ref,"dependsOn":[]});rootdeps["dependsOn"].append(ref)
    native_imports=[]
    if binary:
        native_imports=imports(binary)
        for name in native_imports:
            ref="urn:superopti:windows-import:"+name
            bom["components"].append({"type":"library","bom-ref":ref,"name":name,"scope":"required","properties":[{"name":"superopti:delivery","value":"Windows-provided direct PE import; not bundled; version varies with user's OS servicing"}]})
            bom["dependencies"].append({"ref":ref,"dependsOn":[]});rootdeps["dependsOn"].append(ref)
        bom["metadata"]["component"]["hashes"]=[{"alg":"SHA-256","content":digest(binary)}]
    source_files=sorted([ROOT/"Cargo.toml",ROOT/"Cargo.lock",ROOT/"build.rs",*ROOT.glob("src/*.rs"),*(ROOT/"assets").rglob("*"),*ROOT.glob("resources/*"),ROOT/"scripts/Pagefile.ps1",ROOT/"scripts/Checks.ps1",ROOT/"scripts/Discover.ps1",ROOT/"Install.ps1",ROOT/"Uninstall.ps1",ROOT/"rust-toolchain.toml"])
    source_hash=hashlib.sha256()
    for p in source_files:
        if not p.is_file(): continue
        content=p.read_bytes()
        if p.suffix in {".rs",".toml",".lock",".svg",".manifest",".ps1"}: content=content.replace(b"\r\n",b"\n")
        source_hash.update(p.relative_to(ROOT).as_posix().encode()+b"\0"+hashlib.sha256(content).digest())
    stamp=dt.datetime.now(dt.timezone.utc).isoformat(timespec="seconds").replace("+00:00","Z")
    commit=run("git","rev-parse","HEAD")
    dirty=bool(run("git","status","--porcelain","--untracked-files=no"))
    build={"schema_version":1,"generated_at":stamp,"application":"superopti","version":version,"target":target,"base_git_commit":commit,"tracked_worktree_dirty":dirty,"source_fingerprint_sha256":source_hash.hexdigest(),"cargo_lock_sha256":lock_before,"cargo_package_count":len(inventory),"coverage":{"cargo":"all Cargo.lock registry packages, all targets, including transitive and build/proc-macro dependencies","runtime":"Rust standard library plus direct PE DLL imports; OS servicing versions and transitively loaded OS DLLs are environment-provided and not exhaustively inventoried","limitations":["Source/dependency inventory is not proof that every package function was linked","Windows SDK, compiler/linker and audit/SBOM tools are build tools, not shipped Cargo dependencies","No bundled third-party fonts or UI runtimes; All ten pinned Lucide icons are inventoried separately as adapted native assets","Vulnerability assessment is recorded separately and is time-bounded"]},"tools":{"rustc":rust,"cargo":run("cargo","--version"),"cargo_cyclonedx":tool,"generator":"scripts/generate_sbom.py v1","jsonschema":"4.26.0"},"components":inventory,"platform_dependencies":[{"name":n,"version":"OS-dependent / not bundled","relationship":"direct PE import"} for n in native_imports],"binary":{"name":binary.name,"sha256":digest(binary),"size_bytes":binary.stat().st_size} if binary else None}
    build["rust_sysroot_rlib_inputs"] = sysroot_files
    build["bundled_assets"] = [{"name":f"Lucide {asset.stem}","revision":lucide_revision,"license":"ISC AND MIT" if asset.stem == "minimize-2" else "ISC","source_sha256":digest(asset),"license_file":"licenses/lucide/LICENSE"} for asset in sorted((ROOT/"assets/lucide").glob("*.svg"))]
    build["rust_license_evidence"] = "rust-toolchain-licenses/COPYRIGHT-library.html and bundled license texts; sysroot libraries are compiler inputs, not all necessarily linked"
    sdk=pathlib.Path(os.environ.get("ProgramFiles(x86)",r"C:\Program Files (x86)"))/"Windows Kits/10/bin"
    versions=sorted(p.parent.parent.name for p in sdk.glob("*/x64/rc.exe"))
    build["tools"]["windows_sdk_resource_compiler"]=versions[-1] if versions else "not detected"
    bom["metadata"]["timestamp"]=stamp
    bom["metadata"].setdefault("tools",[]).append({"name":"SuperOpti SBOM enrichment and validation","version":"1"})
    bom["metadata"]["properties"]=[{"name":"superopti:source-fingerprint-sha256","value":source_hash.hexdigest()},{"name":"superopti:base-git-commit","value":commit},{"name":"superopti:tracked-worktree-dirty","value":str(dirty).lower()},{"name":"superopti:coverage","value":"Complete resolved Cargo graph; Rust standard library and direct PE imports included. OS-internal transitive runtime dependencies remain environment-provided, so overall composition is marked incomplete."}]
    bom["compositions"]=[{"aggregate":"incomplete","assemblies":[rootref]}]
    bom["components"].sort(key=lambda c:c["bom-ref"])
    validate(bom)
    save(output/"superopti.cdx.json",bom);save(output/"inventory.json",build)
    lines=["# Software inventory\n",f"Generated {stamp}. SuperOpti {version}, `{target}`.\n",f"All **{len(inventory)} Cargo dependencies** are included and their cached registry archives verified against Cargo.lock. Full license texts: [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md). Machine-readable graph: [superopti.cdx.json](superopti.cdx.json).\n","| Package | Version | Direct | Declared SPDX license |\n|---|---|---|---|\n"]
    lines += [f"| {p['name']} | {p['version']} | {'Yes' if p['direct'] else 'No'} | {p['declared_license']} |\n" for p in inventory]
    lines += [f"\nBundled visual assets: ten Lucide icons including `audio-lines`, revision `{lucide_revision}`, ISC licenses, with the retained Feather MIT notice for `minimize-2`. Adapted native drawings and SVG/PNG/ICO resources are embedded and shipped; upstream source hash and license are included in CycloneDX and inventory.json.\n"]
    lines += ["\nThe overall CycloneDX composition is intentionally marked incomplete because Windows-internal runtime dependencies vary by installed OS. This does not omit Cargo packages: the complete Cargo.lock set is cross-checked, including both syn versions and proc-macro/build dependencies.\n",f"\nRust standard library: {rustver}. Windows SDK resource compiler: {build['tools']['windows_sdk_resource_compiler']}.\n","\nDirect Windows PE imports (platform-provided, not bundled): "+", ".join(f"`{n}`" for n in native_imports)+".\n","\nBuild source fingerprint, executable hash, dependency archive hashes and toolchain metadata: [inventory.json](inventory.json). The checked-in snapshot may describe an uncommitted working tree; each release regenerates evidence against its exact checked-out revision and built binary.\n"]
    (output/"INVENTORY.md").write_text("".join(lines),encoding="utf-8")
    print(f"Validated CycloneDX 1.5: {len(inventory)} Cargo components, {len(native_imports)} direct PE imports; archive checksums and dependency references verified.")

def main():
    parser=argparse.ArgumentParser();parser.add_argument("--output",type=pathlib.Path,default=ROOT/"sbom");parser.add_argument("--binary",type=pathlib.Path);parser.add_argument("--validate",type=pathlib.Path)
    args=parser.parse_args()
    if args.validate: validate(json.loads(args.validate.read_text(encoding="utf-8")));print("CycloneDX schema and graph validation passed")
    else: generate(args.output.resolve(),args.binary.resolve() if args.binary else None)
if __name__=="__main__": main()
