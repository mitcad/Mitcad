// SPDX-License-Identifier: MIT
#include "InstallationCheck.hpp"

#ifdef _WIN32

#include <windows.h>

#include <cstddef>
#include <iterator>
#include <string>
#include <vector>

#include "BuildStamp.hpp"
#include "RunningPrograms.hpp"

namespace mitcad::installation {
namespace {

// The OCCT DLLs this build linked (cmake/OcctBuildStamp.cmake writes the
// lines, {"TKernel.dll", 0x...u, 0x...u}, into the build folder); the last
// entry only keeps the array from being empty.
const ModuleStamp kOcctModules[] = {
#include "OcctBuildStamp.inc"
    {nullptr, 0, 0}};

// The PE header of a module this process loaded, as the loader mapped it
// from the file.
LoadedModule readLoaded(const char* name) {
  LoadedModule module;
  const HMODULE handle = GetModuleHandleA(name);
  if (handle == nullptr) {
    return module;
  }
  const auto* base = reinterpret_cast<const unsigned char*>(handle);
  const auto* dos = reinterpret_cast<const IMAGE_DOS_HEADER*>(base);
  if (dos->e_magic != IMAGE_DOS_SIGNATURE || dos->e_lfanew <= 0) {
    return module;
  }
  const auto* headers = reinterpret_cast<const IMAGE_NT_HEADERS*>(base + dos->e_lfanew);
  if (headers->Signature != IMAGE_NT_SIGNATURE) {
    return module;
  }
  module.loaded = true;
  module.timeDateStamp = headers->FileHeader.TimeDateStamp;
  module.sizeOfImage = headers->OptionalHeader.SizeOfImage;
  return module;
}

std::string utf8(const std::wstring& text) {
  if (text.empty()) {
    return std::string();
  }
  const int size =
      WideCharToMultiByte(CP_UTF8, 0, text.c_str(), static_cast<int>(text.size()), nullptr, 0, nullptr, nullptr);
  std::string result(static_cast<std::size_t>(size > 0 ? size : 0), '\0');
  if (size > 0) {
    WideCharToMultiByte(CP_UTF8, 0, text.c_str(), static_cast<int>(text.size()), result.data(), size, nullptr,
                        nullptr);
  }
  return result;
}

// The folder of this program (bin of an installation).
std::wstring programFolder() {
  std::vector<wchar_t> buffer(32768);
  const DWORD size = GetModuleFileNameW(nullptr, buffer.data(), static_cast<DWORD>(buffer.size()));
  std::wstring path(buffer.data(), size);
  const std::size_t slash = path.find_last_of(L"\\/");
  return slash == std::wstring::npos ? std::wstring() : path.substr(0, slash);
}

} // namespace

std::string mixedInstallationProblem(const std::string& version) {
  // A way out should the check ever be wrong about an installation.
  wchar_t skip[4] = {};
  if (GetEnvironmentVariableW(L"MITCAD_SKIP_INSTALLATION_CHECK", skip, 4) == 1 && skip[0] == L'1') {
    return std::string();
  }
  const std::vector<ModuleStamp> expected(std::begin(kOcctModules), std::end(kOcctModules));
  const std::vector<std::string> mismatched = mismatchedModules(expected, readLoaded);
  if (mismatched.empty()) {
    return std::string();
  }
  const std::wstring folder = programFolder();
  const std::wstring running = folder.empty() ? std::wstring() : describePrograms(runningPrograms(folder));
  return mixedInstallationText(version, mismatched, utf8(running));
}

} // namespace mitcad::installation

#else

namespace mitcad::installation {

// The installers of Linux and macOS replace the whole AppImage or bundle.
std::string mixedInstallationProblem(const std::string&) { return std::string(); }

} // namespace mitcad::installation

#endif
