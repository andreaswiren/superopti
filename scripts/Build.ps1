$ErrorActionPreference = 'Stop'
Push-Location (Split-Path -Parent $PSScriptRoot)
try {
 cargo build --release --locked
 if ($LASTEXITCODE -ne 0) { throw 'Rust release build failed' }
 New-Item -ItemType Directory -Force dist, work | Out-Null
 Copy-Item target/release/superopti.exe dist/superopti.exe -Force
 python scripts/generate_sbom.py --output dist/sbom --binary dist/superopti.exe
 if ($LASTEXITCODE -ne 0) { throw 'SBOM generation/validation failed' }
 $audit = if (Get-Command cargo-audit -ErrorAction SilentlyContinue) { (Get-Command cargo-audit).Source } elseif (Test-Path work/tools/bin/cargo-audit.exe) { (Resolve-Path work/tools/bin/cargo-audit.exe).Path } else { throw 'Install cargo-audit 0.22.2 --locked' }
 $auditVersion = & $audit --version
 if ($auditVersion -notmatch '0\.22\.2$') { throw 'cargo-audit must be version 0.22.2' }
 & $audit audit --json | Set-Content -Encoding utf8 dist/sbom/cargo-audit.json
 if ($LASTEXITCODE -ne 0) { throw 'Dependency audit failed; inspect dist/sbom/cargo-audit.json' }
 $auditResult = Get-Content -Raw dist/sbom/cargo-audit.json | ConvertFrom-Json
 if ($auditResult.vulnerabilities.count -ne 0) { throw 'Dependency vulnerabilities detected' }
 & ./scripts/Test-Pagefile.ps1
 $smoke = Start-Process ./dist/superopti.exe -ArgumentList '--smoke-test','work/smoke-result.json' -WindowStyle Hidden -Wait -PassThru
 if ($smoke.ExitCode -ne 0) { throw 'Native smoke test failed; see work/smoke-result.json' }
 $files = @('dist/superopti.exe','Install.ps1','Uninstall.ps1','README.md','LICENSE','CHANGELOG.md','SECURITY.md','VALIDATION.md','docs','assets','dist/sbom')
 Compress-Archive -Path $files -DestinationPath dist/SuperOpti-windows-x64.zip -Force
 Compress-Archive -Path dist/sbom -DestinationPath dist/SuperOpti-SBOM.zip -Force
 Compress-Archive -Path docs,SECURITY.md,CHANGELOG.md,VALIDATION.md,README.md,LICENSE,dist/sbom -DestinationPath dist/SuperOpti-CRA-docs.zip -Force
 Copy-Item dist/sbom/superopti.cdx.json dist/superopti.cdx.json -Force
 Copy-Item dist/sbom/inventory.json dist/inventory.json -Force
 $artifacts = @('superopti.exe','SuperOpti-windows-x64.zip','SuperOpti-SBOM.zip','SuperOpti-CRA-docs.zip','superopti.cdx.json','inventory.json')
 $hashes = $artifacts | ForEach-Object {
  $h = Get-FileHash -Algorithm SHA256 -LiteralPath (Join-Path dist $_)
  "$($h.Hash.ToLowerInvariant())  $_"
 }
 $hashes | Set-Content -Encoding ascii dist/SHA256SUMS.txt
} finally { Pop-Location }
