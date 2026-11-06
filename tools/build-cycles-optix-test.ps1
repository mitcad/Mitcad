# SPDX-License-Identifier: MIT
# tools\dev-env\build-cycles.ps1 -Optix (mitcad#1332, docs/rendering.md
# "Building an OptiX worker yourself") refuses, before it fetches or builds
# anything (and before it asks for the Visual Studio environment):
#   - a folder without include\optix.h, and one that is not there (the
#     script never downloads the OptiX SDK);
#   - the ordinary render prefix (MITCAD_RENDER_DEPS) as the OptiX worker's
#     prefix.
# No SDK and no network: the script stops at its argument checks, and no
# prefix is made. The ctest repo.build_cycles_optix_args on Windows.
$ErrorActionPreference = 'Stop'
$script = Join-Path $PSScriptRoot 'dev-env\build-cycles.ps1'
$work = Join-Path ([IO.Path]::GetTempPath()) ('mitcad-optix-args-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Force (Join-Path $work 'sdk\include') | Out-Null
$ordinary = Join-Path $work 'ordinary'
$failures = 0

# The script exits with status 2 and says $Text, and makes no prefix.
function Test-Refused([string]$What, [string]$Text, [string[]]$Arguments) {
  $env:MITCAD_RENDER_DEPS = $ordinary
  # The script's stderr must not stop this one.
  $ErrorActionPreference = 'Continue'
  $output = (& powershell -NoProfile -ExecutionPolicy Bypass -File $script @Arguments 2>&1 | Out-String -Width 4096)
  $status = $LASTEXITCODE
  $problem = ''
  if ($status -ne 2) { $problem = "exit status $status, not 2" }
  elseif (-not $output.Contains($Text)) { $problem = "no ""$Text""" }
  elseif (Test-Path $ordinary) { $problem = "the prefix $ordinary was made" }
  if ($problem) {
    Write-Host "FAIL: ${What}: $problem`n$output"
    $script:failures++
  } else {
    Write-Host "ok   $What"
  }
}

try {
  Test-Refused 'a folder without include\optix.h' 'has no include\optix.h' @('-Optix', (Join-Path $work 'sdk'))
  Test-Refused 'a folder that is not there' 'has no include\optix.h' @('-Optix', (Join-Path $work 'no-such-sdk'))
  # A header in place (an empty file of the test's own, no SDK file): the
  # ordinary prefix is refused all the same.
  New-Item -ItemType File -Force (Join-Path $work 'sdk\include\optix.h') | Out-Null
  Test-Refused 'the ordinary prefix' 'is the ordinary render prefix' @('-Optix', (Join-Path $work 'sdk'), '-Prefix', $ordinary)
} finally {
  Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
}
if ($failures -gt 0) {
  Write-Host "FAIL: $failures checks of build-cycles.ps1 -Optix"
  exit 1
}
Write-Host 'PASS: build-cycles.ps1 -Optix checks its arguments'
