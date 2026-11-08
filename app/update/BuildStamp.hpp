// SPDX-License-Identifier: MIT
#pragma once

// The OCCT build stamp (mitcad#1454): which OCCT libraries a program was
// built with, compared at start-up with the ones it loaded. An update
// whose installer could not replace files in use once left the new
// mitcad.exe with the previous release's OCCT DLLs (heap corruption, every
// fillet an access violation). The build (cmake/OcctBuildStamp.cmake)
// records each OCCT DLL's link time stamp and image size from its PE
// header; the loaded module's header has the same two values, and any
// other build of the DLL has others. Plain C++17 without Windows or Qt, so
// that the unit tests (app/tests/update_installer.cpp) run on every
// platform; InstallationCheck.cpp reads the loaded modules on Windows.

#include <cstddef>
#include <cstdint>
#include <functional>
#include <string>
#include <vector>

namespace mitcad::installation {

// A DLL as the build linked it: its file name and two fields of its PE
// header (IMAGE_FILE_HEADER::TimeDateStamp, IMAGE_OPTIONAL_HEADER::SizeOfImage).
struct ModuleStamp {
  const char* name;
  std::uint32_t timeDateStamp;
  std::uint32_t sizeOfImage;
};

// The same fields of a module as loaded; `loaded` false when this process
// has not loaded it (the check passes it over).
struct LoadedModule {
  bool loaded = false;
  std::uint32_t timeDateStamp = 0;
  std::uint32_t sizeOfImage = 0;
};

// The names of the expected modules that are loaded with other stamps:
// files of another build, in the order given.
inline std::vector<std::string> mismatchedModules(const std::vector<ModuleStamp>& expected,
                                                  const std::function<LoadedModule(const char* name)>& read) {
  std::vector<std::string> mismatched;
  for (const ModuleStamp& module : expected) {
    if (module.name == nullptr) {
      continue;
    }
    const LoadedModule loaded = read(module.name);
    if (loaded.loaded &&
        (loaded.timeDateStamp != module.timeDateStamp || loaded.sizeOfImage != module.sizeOfImage)) {
      mismatched.emplace_back(module.name);
    }
  }
  return mismatched;
}

// "a, b, c and 28 more": the first `shown` names.
inline std::string listNames(const std::vector<std::string>& names, std::size_t shown = 5) {
  std::string text;
  for (std::size_t index = 0; index < names.size() && index < shown; ++index) {
    if (index > 0) {
      text += ", ";
    }
    text += names[index];
  }
  if (names.size() > shown) {
    text += " and " + std::to_string(names.size() - shown) + " more";
  }
  return text;
}

// What the start-up tells about a mixed installation (UTF-8): `version` is
// this program's, `mismatched` the DLLs of another build, `running` the
// other programs from the installation, as describePrograms() names them
// (empty: none seen).
inline std::string mixedInstallationText(const std::string& version, const std::vector<std::string>& mismatched,
                                         const std::string& running) {
  std::string text = "This installation is incomplete: files from an earlier version are still in use (" +
                     listNames(mismatched) +
                     (mismatched.size() == 1 ? std::string(" is not the one") : std::string(" are not the ones")) +
                     " MitCad " + version + " was built with).";
  if (!running.empty()) {
    text += " Programs still running from the installation: " + running + ".";
  }
  text += " Close MitCad and the programs listed (AI assistant sessions connected to MitCad run mitcad-cli.exe; "
          "Task Manager can end them), then install MitCad " +
          version + " again.";
  return text;
}

} // namespace mitcad::installation
