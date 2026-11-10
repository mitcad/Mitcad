// SPDX-License-Identifier: MIT
#pragma once

// mitcad-updater.exe's command line and its second check of the installer
// (mitcad#603), in plain C++17 without Windows or Qt, so that the unit
// tests (app/tests/update_installer.cpp) run them on every platform. The
// Windows updater (UpdaterWindows.cpp) reads its arguments with them and,
// right before it runs the installer, compares the SHA-256 of the file,
// read through the handle it then keeps open, with the one Mitcad passes
// from the verified release manifest; Mitcad (UpdateInstaller.cpp) checks
// the hash it passes with the same rule. It also names the programs still
// running from the installation when it asks whether to end them
// (describeForPerson, mitcad#1584).

#include <cstddef>
#include <string>
#include <utility>
#include <vector>

namespace mitcad::updater {

// A SHA-256 digest: 32 bytes, written as 64 lower-case hex digits, as the
// release manifest has them.
inline constexpr std::size_t kSha256Size = 32;

struct Arguments {
  std::wstring wait;      // Mitcad's process id; empty: no wait
  std::wstring installer; // the verified installer
  std::wstring sha256;    // its SHA-256 from the verified manifest
  std::wstring dir;       // the installation folder
  std::wstring scope;     // "user", "all" or empty (the installer decides)
  std::wstring restart;   // the mitcad.exe to start again
  std::wstring log;       // the log; empty: none
};

// The value after `name`; empty when `name` is missing or last.
inline std::wstring option(const std::vector<std::wstring>& arguments, const wchar_t* name) {
  for (std::size_t i = 1; i + 1 < arguments.size(); ++i) {
    if (arguments[i] == name) {
      return arguments[i + 1];
    }
  }
  return std::wstring();
}

// Whether `text` is a SHA-256 as the manifest writes it: 64 lower-case hex
// digits, nothing else.
inline bool isSha256Hex(const std::wstring& text) {
  if (text.size() != 2 * kSha256Size) {
    return false;
  }
  for (const wchar_t c : text) {
    if (!((c >= L'0' && c <= L'9') || (c >= L'a' && c <= L'f'))) {
      return false;
    }
  }
  return true;
}

inline std::wstring usage() {
  return L"Usage: mitcad-updater --wait <pid> --installer <file> --sha256 <hex> --dir <folder> --scope user|all "
         L"--restart <exe> --log <file>";
}

// Reads the command line (arguments[0] is the program). Every field is
// read first, so that `parsed.log` names the log also when the command
// line is refused; false with the reason in `error` when it cannot be
// used: no installer, folder or program to restart, an unknown scope, or
// no SHA-256 of the installer (an installer is never run unchecked).
inline bool parseArguments(const std::vector<std::wstring>& arguments, Arguments& parsed, std::wstring& error) {
  parsed.wait = option(arguments, L"--wait");
  parsed.installer = option(arguments, L"--installer");
  parsed.sha256 = option(arguments, L"--sha256");
  parsed.dir = option(arguments, L"--dir");
  parsed.scope = option(arguments, L"--scope");
  parsed.restart = option(arguments, L"--restart");
  parsed.log = option(arguments, L"--log");
  if (parsed.installer.empty() || parsed.dir.empty() || parsed.restart.empty()) {
    error = usage();
    return false;
  }
  if (!parsed.scope.empty() && parsed.scope != L"user" && parsed.scope != L"all") {
    error = L"Unknown scope '" + parsed.scope + L"'. " + usage();
    return false;
  }
  if (!isSha256Hex(parsed.sha256)) {
    error = L"No SHA-256 of the installer (64 lower-case hex digits after --sha256), so it was not run";
    return false;
  }
  return true;
}

// `size` bytes as lower-case hex digits.
inline std::wstring toHex(const unsigned char* bytes, std::size_t size) {
  static const wchar_t digits[] = L"0123456789abcdef";
  std::wstring text;
  text.reserve(2 * size);
  for (std::size_t i = 0; i < size; ++i) {
    text += digits[(bytes[i] >> 4) & 0x0f];
    text += digits[bytes[i] & 0x0f];
  }
  return text;
}

// Whether the digest of a file (`size` bytes) is the SHA-256 `expected`
// (lower-case hex, as isSha256Hex).
inline bool digestMatches(const unsigned char* digest, std::size_t size, const std::wstring& expected) {
  return size == kSha256Size && isSha256Hex(expected) && toHex(digest, size) == expected;
}

// Feeds a file's bytes, as `read(buffer, capacity)` gives them (the count
// read, 0 at the end, a negative number on an error), to `add(data, size)`
// (false on an error) until the end. False when a read or an add failed.
template <class Read, class Add> bool hashAll(Read&& read, Add&& add) {
  std::vector<unsigned char> buffer(std::size_t{1} << 16);
  for (;;) {
    const long long got = read(buffer.data(), buffer.size());
    if (got < 0 || static_cast<unsigned long long>(got) > buffer.size()) {
      return false;
    }
    if (got == 0) {
      return true;
    }
    const unsigned char* data = buffer.data();
    if (!add(data, static_cast<std::size_t>(got))) {
      return false;
    }
  }
}

// `text` with the letters A-Z in lower case (file names compare so on
// Windows for the names below).
inline std::wstring lowerAscii(std::wstring text) {
  for (wchar_t& c : text) {
    if (c >= L'A' && c <= L'Z') {
      c = static_cast<wchar_t>(c - L'A' + L'a');
    }
  }
  return text;
}

// The programs still running from an installation, for the question
// whether to end them (mitcad#1584): one line for each program, what it is
// and how many run, MitCad first: "MitCad (mitcad.exe)", "3 MCP bridges of
// AI assistant sessions (mitcad-cli.exe)", "a render worker
// (mitcad-render.exe)". `names` are the programs' file names, one for each
// process.
inline std::vector<std::wstring> describeForPerson(const std::vector<std::wstring>& names) {
  struct Kind {
    const wchar_t* file;
    const wchar_t* one;
    const wchar_t* many; // after the count
  };
  static const Kind kinds[] = {
      {L"mitcad.exe", L"MitCad", L"MitCad processes"},
      {L"mitcad-cli.exe", L"an MCP bridge of an AI assistant session", L"MCP bridges of AI assistant sessions"},
      {L"mitcad-mcp-launcher.exe", L"an AI assistant's MitCad launcher", L"AI assistants' MitCad launchers"},
      {L"mitcad-render.exe", L"a render worker", L"render workers"},
  };
  std::vector<std::pair<std::wstring, int>> counts; // in the order first seen
  for (const std::wstring& name : names) {
    const std::wstring key = lowerAscii(name);
    bool counted = false;
    for (auto& [file, count] : counts) {
      if (lowerAscii(file) == key) {
        ++count;
        counted = true;
        break;
      }
    }
    if (!counted) {
      counts.emplace_back(name, 1);
    }
  }
  std::vector<std::wstring> known;
  std::vector<std::wstring> others;
  for (const Kind& kind : kinds) {
    for (const auto& [file, count] : counts) {
      if (lowerAscii(file) == kind.file) {
        known.push_back((count == 1 ? std::wstring(kind.one) : std::to_wstring(count) + L" " + kind.many) + L" (" +
                        kind.file + L")");
      }
    }
  }
  for (const auto& [file, count] : counts) {
    bool isKnown = false;
    for (const Kind& kind : kinds) {
      isKnown = isKnown || lowerAscii(file) == kind.file;
    }
    if (!isKnown) {
      others.push_back(count == 1 ? file : file + L" (" + std::to_wstring(count) + L" processes)");
    }
  }
  known.insert(known.end(), others.begin(), others.end());
  return known;
}

} // namespace mitcad::updater
