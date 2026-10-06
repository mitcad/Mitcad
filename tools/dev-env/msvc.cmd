@echo off
rem SPDX-License-Identifier: MIT
rem Runs a command with the Visual Studio C++ x64 environment (cl, link),
rem which the Ninja generator needs. Example: msvc.cmd cmake --preset dev
setlocal
rem Shells started before the tools were installed (e.g. sshd sessions) have
rem a stale PATH, so rebuild it from the registry first.
for /f "usebackq delims=" %%p in (`powershell -NoProfile -Command "[Environment]::GetEnvironmentVariable('Path','Machine') + ';' + [Environment]::GetEnvironmentVariable('Path','User')"`) do set "PATH=%%p"
set "VSWHERE=%ProgramFiles(x86)%\Microsoft Visual Studio\Installer\vswhere.exe"
for /f "usebackq delims=" %%i in (`"%VSWHERE%" -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath`) do set "VSDIR=%%i"
if not defined VSDIR (
  echo Visual Studio with the C++ x64 tools was not found. 1>&2
  exit /b 1
)
call "%VSDIR%\VC\Auxiliary\Build\vcvars64.bat" >nul || exit /b 1
%*
