# SPDX-License-Identifier: MIT
# Applies a staged copy of the working tree to a checkout in the build VM;
# tools/dev-env/sync-to-vm.sh unpacks the stage and runs this script from it.
#
# Files that are new or whose content differs are moved over and get the
# current time, so a build directory last built from another checkout
# rebuilds them however old their times in the working tree are; identical
# files are left alone and keep their times, so they are not rebuilt. Files
# and folders that are not in the stage are removed, except the target's
# build folder and CMakeUserPresets.json. The stage is removed at the end.
param(
  [Parameter(Mandatory = $true)][string]$Stage,
  [Parameter(Mandatory = $true)][string]$Target
)
$ErrorActionPreference = 'Stop'
$Stage = (Resolve-Path -LiteralPath $Stage).Path.TrimEnd('\')
$Target = (Resolve-Path -LiteralPath $Target).Path.TrimEnd('\')
$keep = @('build', 'CMakeUserPresets.json')

# Remove what the stage does not have (or has as a file instead of a
# folder, or the other way round), parents before their contents.
$removed = 0
$items = @(Get-ChildItem -LiteralPath $Target -Force | Where-Object { $keep -notcontains $_.Name } |
    ForEach-Object { $_; if ($_.PSIsContainer) { Get-ChildItem -LiteralPath $_.FullName -Recurse -Force } })
foreach ($item in $items) {
  $staged = $Stage + $item.FullName.Substring($Target.Length)
  if ($item.PSIsContainer) {
    if ([IO.Directory]::Exists($staged) -or -not [IO.Directory]::Exists($item.FullName)) { continue }
  } elseif ([IO.File]::Exists($staged) -or -not [IO.File]::Exists($item.FullName)) {
    continue
  } else {
    $removed++
  }
  Remove-Item -LiteralPath $item.FullName -Recurse -Force
}

# Move over what is new or differs, with a fresh time.
$changed = 0
$now = Get-Date
foreach ($file in @(Get-ChildItem -LiteralPath $Stage -Recurse -Force -File)) {
  $dest = $Target + $file.FullName.Substring($Stage.Length)
  if ([IO.File]::Exists($dest)) {
    if ((New-Object IO.FileInfo $dest).Length -eq $file.Length -and
      [Linq.Enumerable]::SequenceEqual([IO.File]::ReadAllBytes($dest), [IO.File]::ReadAllBytes($file.FullName))) {
      continue
    }
    [IO.File]::Delete($dest)
  } else {
    [void][IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName($dest))
  }
  [IO.File]::Move($file.FullName, $dest)
  [IO.File]::SetLastWriteTime($dest, $now)
  $changed++
}

Remove-Item -LiteralPath $Stage -Recurse -Force
"$changed files changed, $removed removed"
