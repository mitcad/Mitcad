# SPDX-License-Identifier: MIT
# check-all sources: app/framework/Language.cpp app/framework/Language.hpp app/framework/CommandSearch.cpp app/framework/Ribbon.cpp app/main.cpp app/view/ViewController.cpp app/translations cmake/Translations.cmake tools/translations
# check-all sources: app/framework/ModelMessages.cpp app/framework/ModelMessages.hpp app/framework/ModelMessageTemplates.cpp
# Each start also shows a model message (mitcad#1284), the model's refusal of
# a feature that does not exist: English, Finnish, the pseudo-language.
# The interface language (mitcad#1460) in the build VM, as
# tools/ui-language-test.sh on Linux: the shared steps
# tools/ui-scripts/language-*.mitcad-ui. Four starts:
#   1. default: as every UI test starts MitCad (ui-windows-lib.ps1 sets
#      MITCAD_LANGUAGE=en): English.
#   2. en: --language en over a stored Finnish preference: English.
#   3. fi: --language fi: the tabs in Finnish, the command search finds a
#      command by its English name, Preferences > General > Language set to
#      English is saved for the next start.
#   4. pseudo: --language xx, the tests' pseudo-language beside mitcad.exe in
#      a test build: every text marked and longer, no group caption cut.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-language-test.ps1 -App build\dev\mitcad.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-language'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$env:MITCAD_TEST_STEP_MS = '100'
$ini = Join-Path $script:UiSettings 'Mitcad\Mitcad.ini'
$baseIni = @(Get-Content $ini -ErrorAction SilentlyContinue)

# One start of MitCad with the steps of tools\ui-scripts\<Script>.mitcad-ui;
# the app ends with them.
function Run-Language([string]$Name, [string]$Script, [string[]]$Arguments) {
  $env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot "ui-scripts/$Script.mitcad-ui"
  Ui-StartApp $Name (@('--ui=qt', '--no-mcp') + $Arguments)
  $null = $script:UiProcess.Handle
  if (-not $script:UiProcess.WaitForExit(180000)) { Ui-Fail "${Name}: the driver timed out" }
  $exitCode = $script:UiProcess.ExitCode
  if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
    Ui-Fail "${Name}: the driver failed (exit code $exitCode)"
  }
  Ui-ExpectLog 'TestDriver: done' "${Name}: the steps of $Script"
  Get-Content $script:UiLog | Where-Object { $_ -like '*Language: *' -or $_ -like '*Preferences: language *' } |
    ForEach-Object { Write-Host "     $_" }
  Ui-StopApp
}

$env:MITCAD_TEST_LANGUAGE_REASON = 'MITCAD_LANGUAGE'
Run-Language 'default' 'language-en' @()
# Ui-StartApp sets it (unlike the Linux library, which exports it when it is
# sourced), so it is checked after the first start.
if ($env:MITCAD_LANGUAGE -ne 'en') { Ui-Fail 'ui-windows-lib.ps1 does not set MITCAD_LANGUAGE=en' }
Write-Host 'ok   default: English, as every UI test starts'

Set-Content -Encoding ascii $ini ($baseIni + @('[interface]', 'language=fi'))
$env:MITCAD_TEST_LANGUAGE_REASON = '--language'
Run-Language 'en' 'language-en' @('--language=en')
Write-Host 'ok   en: --language en over a stored Finnish preference'

Run-Language 'fi' 'language-fi' @('--language=fi')
if (-not ((Get-Content $ini) -contains 'language=en')) { Ui-Fail 'fi: English is not saved as interface/language' }
Write-Host 'ok   fi: Finnish tabs, the command search by the English name, the preference saved'

if (Test-Path (Join-Path (Split-Path $App) 'translations\mitcad_xx.qm')) {
  Run-Language 'pseudo' 'language-pseudo' @('--language=xx')
  Write-Host 'ok   pseudo: every text marked and longer, no group caption cut'
} else {
  Write-Host 'skip pseudo: no translations\mitcad_xx.qm beside mitcad.exe (not a test build)'
}
Write-Host 'PASS: the interface language: English by default and with --language en, Finnish with --language fi, the pseudo-language'
