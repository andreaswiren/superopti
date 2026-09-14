$ErrorActionPreference = 'Stop'
$tokens = $null; $errors = $null
$ast = [System.Management.Automation.Language.Parser]::ParseFile((Join-Path $PSScriptRoot 'Checks.ps1'),[ref]$tokens,[ref]$errors)
if ($errors.Count) { throw ($errors | Out-String) }
$function = $ast.Find({param($n) $n -is [System.Management.Automation.Language.FunctionDefinitionAst] -and $n.Name -eq 'Get-DriveCapacityStatus'},$true)
. ([scriptblock]::Create($function.Extent.Text))
foreach ($case in @(@(0,'Critical'),@(4.99,'Critical'),@(5,'Warning'),@(14.99,'Warning'),@(15,'OK'),@(100,'OK'),@(-1,'Unknown'),@(101,'Unknown'),@([double]::NaN,'Unknown'),@([double]::PositiveInfinity,'Unknown'))) {
 if ((Get-DriveCapacityStatus $case[0]) -ne $case[1]) { throw "Incorrect capacity status at $($case[0])" }
}
'Health check parser and 10 capacity boundary cases passed; no system settings changed.'
