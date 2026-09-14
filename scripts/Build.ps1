$ErrorActionPreference = 'Stop'
Push-Location (Split-Path -Parent $PSScriptRoot)
try {
 cargo build --release --locked
 if ($LASTEXITCODE -ne 0) { throw 'Rust release build failed' }
 New-Item -ItemType Directory -Force dist | Out-Null
 Copy-Item target/release/superopti.exe dist/superopti.exe -Force
 $files = @('dist/superopti.exe','Install.ps1','Uninstall.ps1','README.md','LICENSE')
 Compress-Archive -Path $files -DestinationPath dist/SuperOpti-windows-x64.zip -Force
 $hashes = @('dist/superopti.exe','dist/SuperOpti-windows-x64.zip') | ForEach-Object {
  $h = Get-FileHash -Algorithm SHA256 -LiteralPath $_
  "$($h.Hash.ToLowerInvariant())  $([IO.Path]::GetFileName($h.Path))"
 }
 $hashes | Set-Content dist/SHA256SUMS.txt
} finally { Pop-Location }
