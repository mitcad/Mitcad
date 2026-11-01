// SPDX-License-Identifier: MIT
// Standalone binary packaged inside an MCP Bundle. No shell or third-party
// runtime is needed; the already installed CLI keeps its own library paths.
#include <cstdio>
#include <filesystem>
#include <string>

#ifdef _WIN32
#ifndef NOMINMAX
#define NOMINMAX
#endif
#include <windows.h>

namespace {
std::wstring quote(const wchar_t* argument) {
  std::wstring result = L"\"";
  unsigned slashes = 0;
  for (const wchar_t* at = argument; *at; ++at) {
    if (*at == L'\\') { ++slashes; continue; }
    if (*at == L'\"') result.append(slashes * 2 + 1, L'\\');
    else result.append(slashes, L'\\');
    result += *at;
    slashes = 0;
  }
  result.append(slashes * 2, L'\\');
  return result + L'\"';
}
}

int wmain(int argc, wchar_t** argv) {
  if (argc != 4 || !std::filesystem::path(argv[1]).is_absolute() ||
      std::wstring(argv[2]) != L"mcp" || std::wstring(argv[3]) != L"--connect") {
    std::fputs("MitCad MCP launcher: invalid installed bridge arguments\n", stderr); return 2;
  }
  std::wstring command;
  for (int i = 1; i < argc; ++i) { if (i > 1) command += L' '; command += quote(argv[i]); }
  STARTUPINFOW startup{}; startup.cb = static_cast<DWORD>(sizeof(startup)); startup.dwFlags = STARTF_USESTDHANDLES;
  startup.hStdInput = GetStdHandle(STD_INPUT_HANDLE);
  startup.hStdOutput = GetStdHandle(STD_OUTPUT_HANDLE);
  startup.hStdError = GetStdHandle(STD_ERROR_HANDLE);
  for (HANDLE handle : {startup.hStdInput, startup.hStdOutput, startup.hStdError}) {
    SetHandleInformation(handle, HANDLE_FLAG_INHERIT, HANDLE_FLAG_INHERIT);
  }
  PROCESS_INFORMATION process{};
  if (!CreateProcessW(argv[1], command.data(), nullptr, nullptr, TRUE, CREATE_NO_WINDOW,
                      nullptr, nullptr, &startup, &process)) {
    std::fputs("MitCad MCP launcher: installed bridge could not start\n", stderr); return 1;
  }
  CloseHandle(process.hThread);
  WaitForSingleObject(process.hProcess, INFINITE);
  DWORD status = 1; GetExitCodeProcess(process.hProcess, &status); CloseHandle(process.hProcess);
  return static_cast<int>(status);
}
#else
#include <unistd.h>

int main(int argc, char** argv) {
  if (argc != 4 || !std::filesystem::path(argv[1]).is_absolute() ||
      std::string(argv[2]) != "mcp" || std::string(argv[3]) != "--connect") {
    std::fputs("MitCad MCP launcher: invalid installed bridge arguments\n", stderr); return 2;
  }
  execv(argv[1], argv + 1);
  std::perror("MitCad MCP launcher: installed bridge could not start");
  return 1;
}
#endif
