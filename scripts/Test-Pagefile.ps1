$ErrorActionPreference = 'Stop'
$scriptPath = Join-Path $PSScriptRoot Pagefile.ps1
$tokens = $null; $parseErrors = $null
$ast = [System.Management.Automation.Language.Parser]::ParseFile($scriptPath,[ref]$tokens,[ref]$parseErrors)
if ($parseErrors.Count -ne 0) { throw ($parseErrors | Out-String) }
# Extract only the pure sizing function; never execute registry/CIM mutations.
$function = $ast.Find({param($node) $node -is [System.Management.Automation.Language.FunctionDefinitionAst] -and $node.Name -eq 'Get-FixedPagefileMiB'},$true)
. ([scriptblock]::Create($function.Extent.Text))
foreach ($case in @(@(8GB,16384),@(16GB,16384),@(32GB,16384),@(64GB,32768),@(128GB,65536),@((32GB+1MB),16385))) {
 $actual = Get-FixedPagefileMiB $case[0]
 if ($actual -ne $case[1]) { throw "Sizing failed: $($case[0]) produced $actual" }
}
$rejected = $false
try { Get-FixedPagefileMiB 0 | Out-Null } catch { $rejected = $true }
if (-not $rejected) { throw 'Unknown RAM must not become a guessed size' }
'Pagefile parser and 7 sizing/unknown-memory cases passed; no settings changed.'
