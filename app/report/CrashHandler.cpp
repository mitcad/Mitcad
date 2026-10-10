// SPDX-License-Identifier: MIT
#include "CrashHandler.hpp"

#ifdef _WIN32
#ifndef NOMINMAX
#define NOMINMAX
#endif
#include <windows.h>

#include <dbghelp.h> // after windows.h
#else
#include <fcntl.h>
#include <signal.h>
#include <sys/utsname.h>
#include <unistd.h>
#ifdef __linux__
#include <sys/prctl.h>
#endif
#if __has_include(<execinfo.h>)
#include <execinfo.h>
#define MITCAD_HAS_BACKTRACE 1
#endif
#endif

#include <atomic>
#include <csignal>
#include <cstdlib>
#include <cstdint>
#include <cstring>
#include <ctime>
#include <exception>
#include <string>

namespace mitcad::crash {
namespace {

// Everything the handlers write is prepared here beforehand: they only
// copy bytes out.
constexpr std::size_t kTextSize = 2048;
constexpr std::size_t kPathSize = 4096;
constexpr int kActions = 32;
constexpr std::size_t kActionSize = 120;
constexpr std::size_t kMessageSize = 2048;
constexpr int kFrames = 64;

char g_header[kTextSize];  // the report's first lines (process, version, ...)
char g_process[64];        // "app", "import-worker", ...
char g_directory[kPathSize];
#ifdef _WIN32
wchar_t g_directoryWide[kPathSize];
wchar_t g_symbolPath[kPathSize]; // the executable's folder, where its PDB file is
#endif
char g_actions[kActions][kActionSize];
std::atomic<unsigned> g_actionCount{0};
char g_message[kMessageSize];
std::atomic<int> g_entered{0}; // a report is being written (once per process)

// Bounded string copy that always terminates.
void copy(char* target, std::size_t size, const char* source) {
  std::size_t i = 0;
  for (; source != nullptr && source[i] != '\0' && i + 1 < size; ++i) {
    target[i] = source[i];
  }
  target[i] = '\0';
}

void append(char* target, std::size_t size, const char* source) {
  const std::size_t used = std::strlen(target);
  if (used + 1 < size) {
    copy(target + used, size - used, source);
  }
}

// The decimal or hexadecimal digits of a number, into `buffer`.
const char* number(unsigned long long value, char (&buffer)[32], unsigned base = 10) {
  char* end = buffer + sizeof buffer - 1;
  *end = '\0';
  char* p = end;
  do {
    const unsigned digit = static_cast<unsigned>(value % base);
    *--p = static_cast<char>(digit < 10 ? '0' + digit : 'a' + digit - 10);
    value /= base;
  } while (value != 0 && p > buffer);
  return p;
}

std::string environment(const char* name) {
#ifdef _MSC_VER
  char* value = nullptr;
  std::size_t length = 0;
  if (_dupenv_s(&value, &length, name) != 0 || value == nullptr) {
    return {};
  }
  std::string text(value);
  std::free(value);
  return text;
#else
  const char* value = std::getenv(name);
  return value != nullptr ? value : "";
#endif
}

void prepareHeader(const char* version) {
  char digits[32];
  g_header[0] = '\0';
  append(g_header, sizeof g_header, "Mitcad crash report 1\nprocess: ");
  append(g_header, sizeof g_header, g_process);
  append(g_header, sizeof g_header, "\nversion: ");
  append(g_header, sizeof g_header, version);
  append(g_header, sizeof g_header, "\nplatform: ");
#ifdef _WIN32
  SYSTEM_INFO system;
  GetNativeSystemInfo(&system);
  append(g_header, sizeof g_header,
         system.wProcessorArchitecture == PROCESSOR_ARCHITECTURE_ARM64   ? "Windows arm64"
         : system.wProcessorArchitecture == PROCESSOR_ARCHITECTURE_AMD64 ? "Windows x86_64"
                                                                         : "Windows");
  append(g_header, sizeof g_header, "\npid: ");
  append(g_header, sizeof g_header, number(GetCurrentProcessId(), digits));
#else
  struct utsname name;
  if (uname(&name) == 0) {
    append(g_header, sizeof g_header, name.sysname);
    append(g_header, sizeof g_header, " ");
    append(g_header, sizeof g_header, name.release);
    append(g_header, sizeof g_header, " ");
    append(g_header, sizeof g_header, name.machine);
  }
  append(g_header, sizeof g_header, "\npid: ");
  append(g_header, sizeof g_header, number(static_cast<unsigned long long>(getpid()), digits));
#endif
  append(g_header, sizeof g_header, "\nparent: ");
  const std::string parent = environment("MITCAD_CRASH_PARENT");
  append(g_header, sizeof g_header, parent.empty() ? "0" : parent.c_str());
  append(g_header, sizeof g_header, "\n");
}

// ---------------------------------------------------------------------------
// Writing the report: plain file calls only.

#ifdef _WIN32
using File = HANDLE;
const File kNoFile = INVALID_HANDLE_VALUE;

void put(File file, const char* text, std::size_t length) {
  DWORD written = 0;
  if (file != kNoFile) {
    WriteFile(file, text, static_cast<DWORD>(length), &written, nullptr);
  }
  HANDLE error = GetStdHandle(STD_ERROR_HANDLE);
  if (file == kNoFile && error != nullptr && error != INVALID_HANDLE_VALUE) {
    WriteFile(error, text, static_cast<DWORD>(length), &written, nullptr);
  }
}
#else
using File = int;
const File kNoFile = -1;

void put(File file, const char* text, std::size_t length) {
  const int target = file != kNoFile ? file : STDERR_FILENO;
  while (length > 0) {
    const ssize_t written = ::write(target, text, length);
    if (written <= 0) {
      return;
    }
    text += written;
    length -= static_cast<std::size_t>(written);
  }
}
#endif

void put(File file, const char* text) { put(file, text, std::strlen(text)); }

void putLine(File file, const char* key, const char* value) {
  put(file, key);
  put(file, ": ");
  put(file, value);
  put(file, "\n");
}

void putActions(File file) {
  put(file, "actions:\n");
  const unsigned count = g_actionCount.load();
  const unsigned first = count > static_cast<unsigned>(kActions) ? count - kActions : 0;
  for (unsigned i = first; i < count; ++i) {
    put(file, g_actions[i % kActions]);
    put(file, "\n");
  }
}

// The report's file name, `<folder>/crash-<time>-<pid>.crash`, and the
// temporary name it is written under first (".part").
struct ReportName {
  char path[kPathSize + 64];
  char part[kPathSize + 72];
};

void reportName(ReportName& name, unsigned long long pid) {
  char digits[32];
  name.path[0] = '\0';
  append(name.path, sizeof name.path, "crash-");
  append(name.path, sizeof name.path, number(static_cast<unsigned long long>(std::time(nullptr)), digits));
  append(name.path, sizeof name.path, "-");
  append(name.path, sizeof name.path, number(pid, digits));
  append(name.path, sizeof name.path, ".crash");
  copy(name.part, sizeof name.part, name.path);
  append(name.part, sizeof name.part, ".part");
}

} // namespace

// The handlers have names of their own (not in an anonymous namespace), so
// that a stack names them and report::normalizedFrames can leave them out
// (on Linux the application exports mitcad's symbols for that:
// report/crash-symbols.list).
namespace detail {

#ifdef _WIN32

// The frames' functions come from the debug information (dbghelp, with the
// PDB file next to the executable; a system library's from its exports).
// Loaded at the crash, once, and never through a symbol server.
bool loadSymbols() {
  static bool loaded = false;
  static bool tried = false;
  if (!tried && g_symbolPath[0] != L'\0') {
    tried = true;
    SymSetOptions(SYMOPT_UNDNAME | SYMOPT_DEFERRED_LOADS | SYMOPT_FAIL_CRITICAL_ERRORS | SYMOPT_NO_PROMPTS |
                  SYMOPT_DISABLE_SYMSRV_AUTODETECT);
    loaded = SymInitializeW(GetCurrentProcess(), g_symbolPath, TRUE) != FALSE;
  }
  return loaded;
}

// A frame as `<module path>!<function>+0x<offset in it>`, or without its
// function as `<module path>+0x<offset in the module>`.
void putFrame(File file, void* frame) {
  char digits[32];
  HMODULE module = nullptr;
  char path[MAX_PATH] = "?";
  if (GetModuleHandleExA(GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
                         static_cast<LPCSTR>(frame), &module) != 0 &&
      module != nullptr) {
    if (GetModuleFileNameA(module, path, MAX_PATH) == 0) {
      copy(path, sizeof path, "?");
    }
  }
  const auto address = reinterpret_cast<unsigned long long>(frame);
  const auto base = reinterpret_cast<unsigned long long>(module);
  put(file, path);
  if (module != nullptr && loadSymbols()) {
    constexpr int kName = 512;
    alignas(SYMBOL_INFOW) unsigned char buffer[sizeof(SYMBOL_INFOW) + kName * sizeof(wchar_t)] = {};
    auto* symbol = reinterpret_cast<SYMBOL_INFOW*>(buffer);
    symbol->SizeOfStruct = sizeof(SYMBOL_INFOW);
    symbol->MaxNameLen = kName;
    DWORD64 displacement = 0;
    char name[kName * 3] = "";
    if (SymFromAddrW(GetCurrentProcess(), address, &displacement, symbol) != FALSE &&
        WideCharToMultiByte(CP_UTF8, 0, symbol->Name, -1, name, sizeof name, nullptr, nullptr) > 1) {
      put(file, "!");
      put(file, name);
      put(file, "+0x");
      put(file, number(displacement, digits, 16));
      put(file, "\n");
      return;
    }
  }
  put(file, "+0x");
  put(file, number(module != nullptr ? address - base : address, digits, 16));
  put(file, "\n");
}

void writeReport(const char* signal, const char* address, void* const* frames, int count) {
  ReportName name;
  reportName(name, GetCurrentProcessId());
  constexpr int kWide = static_cast<int>(kPathSize + 80);
  wchar_t path[kWide] = L"";
  wchar_t part[kWide] = L"";
  File file = kNoFile;
  if (g_directoryWide[0] != L'\0') {
    wcscpy_s(part, g_directoryWide);
    wcscat_s(part, L"\\");
    wcscpy_s(path, part);
    const int used = static_cast<int>(wcslen(part));
    MultiByteToWideChar(CP_UTF8, 0, name.part, -1, part + used, kWide - used);
    MultiByteToWideChar(CP_UTF8, 0, name.path, -1, path + used, kWide - used);
    file = CreateFileW(part, GENERIC_WRITE, 0, nullptr, CREATE_NEW, FILE_ATTRIBUTE_NORMAL, nullptr);
  }
  char digits[32];
  put(file, g_header);
  putLine(file, "time", number(static_cast<unsigned long long>(std::time(nullptr)), digits));
  putLine(file, "signal", signal);
  putLine(file, "address", address);
  putLine(file, "message", g_message);
  put(file, "stack:\n");
  for (int i = 0; i < count; ++i) {
    putFrame(file, frames[i]);
  }
  putActions(file);
  put(file, "end\n");
  if (file != kNoFile) {
    CloseHandle(file);
    MoveFileExW(part, path, MOVEFILE_REPLACE_EXISTING);
  }
}

const char* exceptionName(DWORD code) {
  switch (code) {
  case EXCEPTION_ACCESS_VIOLATION:
    return "EXCEPTION_ACCESS_VIOLATION";
  case EXCEPTION_STACK_OVERFLOW:
    return "EXCEPTION_STACK_OVERFLOW";
  case EXCEPTION_ILLEGAL_INSTRUCTION:
    return "EXCEPTION_ILLEGAL_INSTRUCTION";
  case EXCEPTION_INT_DIVIDE_BY_ZERO:
    return "EXCEPTION_INT_DIVIDE_BY_ZERO";
  case EXCEPTION_IN_PAGE_ERROR:
    return "EXCEPTION_IN_PAGE_ERROR";
  case EXCEPTION_PRIV_INSTRUCTION:
    return "EXCEPTION_PRIV_INSTRUCTION";
  case EXCEPTION_DATATYPE_MISALIGNMENT:
    return "EXCEPTION_DATATYPE_MISALIGNMENT";
  default:
    return "EXCEPTION";
  }
}

// The crashed thread's stack from the faulting instruction on, walked from
// the exception's context, so that the handler's own frames are not in it.
// StackWalk64 reads the stack with ReadProcessMemory: a broken stack ends
// the walk instead of faulting again.
int walkStack(const CONTEXT& start, HANDLE thread, void** frames, int capacity) {
  CONTEXT context = start; // StackWalk64 changes it
  STACKFRAME64 frame{};
  DWORD machine = IMAGE_FILE_MACHINE_UNKNOWN;
#if defined(_M_X64)
  machine = IMAGE_FILE_MACHINE_AMD64;
  frame.AddrPC.Offset = context.Rip;
  frame.AddrFrame.Offset = context.Rbp;
  frame.AddrStack.Offset = context.Rsp;
#elif defined(_M_ARM64)
  machine = IMAGE_FILE_MACHINE_ARM64;
  frame.AddrPC.Offset = context.Pc;
  frame.AddrFrame.Offset = context.Fp;
  frame.AddrStack.Offset = context.Sp;
#elif defined(_M_IX86)
  machine = IMAGE_FILE_MACHINE_I386;
  frame.AddrPC.Offset = context.Eip;
  frame.AddrFrame.Offset = context.Ebp;
  frame.AddrStack.Offset = context.Esp;
#endif
  frame.AddrPC.Mode = AddrModeFlat;
  frame.AddrFrame.Mode = AddrModeFlat;
  frame.AddrStack.Mode = AddrModeFlat;
  loadSymbols(); // the modules' function tables, which StackWalk64 unwinds with
  int count = 0;
  while (count < capacity && StackWalk64(machine, GetCurrentProcess(), thread, &frame, &context, nullptr,
                                         SymFunctionTableAccess64, SymGetModuleBase64, nullptr) != FALSE) {
    if (frame.AddrPC.Offset == 0) {
      break;
    }
    frames[count++] = reinterpret_cast<void*>(static_cast<std::uintptr_t>(frame.AddrPC.Offset));
  }
  return count;
}

struct Crash {
  const char* signal;
  char address[40];
  const CONTEXT* context; // the crashed thread's, at the fault
  HANDLE thread;
};

DWORD WINAPI reportCrash(void* parameter) {
  const Crash& crash = *static_cast<const Crash*>(parameter);
  void* frames[kFrames];
  const int count = crash.context != nullptr ? walkStack(*crash.context, crash.thread, frames, kFrames) : 0;
  writeReport(crash.signal, crash.address, frames, count);
  return 0;
}

LONG WINAPI onException(EXCEPTION_POINTERS* info) {
  if (g_entered.exchange(1) == 0) {
    Crash crash{};
    const EXCEPTION_RECORD* record = info != nullptr ? info->ExceptionRecord : nullptr;
    crash.signal = exceptionName(record != nullptr ? record->ExceptionCode : 0);
    char digits[32];
    copy(crash.address, sizeof crash.address, "0x");
    append(crash.address, sizeof crash.address,
           number(record != nullptr ? reinterpret_cast<unsigned long long>(record->ExceptionAddress) : 0ULL, digits,
                  16));
    crash.context = info != nullptr ? info->ContextRecord : nullptr;
    DuplicateHandle(GetCurrentProcess(), GetCurrentThread(), GetCurrentProcess(), &crash.thread, 0, FALSE,
                    DUPLICATE_SAME_ACCESS);
    // On a thread of its own: little may be left of this one's stack (a
    // stack overflow). A report that does not finish in time is left out.
    HANDLE reporter = CreateThread(nullptr, 256 * 1024, reportCrash, &crash, 0, nullptr);
    if (reporter != nullptr) {
      WaitForSingleObject(reporter, 60000);
      CloseHandle(reporter);
    } else {
      reportCrash(&crash);
    }
    if (crash.thread != nullptr) {
      CloseHandle(crash.thread);
    }
  }
  // Windows ends the process as it would have (and its error reporting
  // sees it).
  return EXCEPTION_CONTINUE_SEARCH;
}

void onAbort(int) {
  if (g_entered.exchange(1) == 0) {
    void* frames[kFrames];
    // From the caller on: this handler's own frame left out.
    const int count = static_cast<int>(CaptureStackBackTrace(1, kFrames, frames, nullptr));
    writeReport("SIGABRT", "0x0", frames, count);
  }
  // abort() goes on to end the process.
}

#else // POSIX

const char* signalName(int number) {
  switch (number) {
  case SIGSEGV:
    return "SIGSEGV";
  case SIGBUS:
    return "SIGBUS";
  case SIGILL:
    return "SIGILL";
  case SIGFPE:
    return "SIGFPE";
  case SIGABRT:
    return "SIGABRT";
  default:
    return "signal";
  }
}

constexpr int kSignals[] = {SIGSEGV, SIGBUS, SIGILL, SIGFPE, SIGABRT};

void writeReport(int signal, const void* address) {
  ReportName name;
  reportName(name, static_cast<unsigned long long>(getpid()));
  char path[kPathSize + 80] = "";
  char part[kPathSize + 80] = "";
  File file = kNoFile;
  if (g_directory[0] != '\0') {
    copy(path, sizeof path, g_directory);
    append(path, sizeof path, "/");
    copy(part, sizeof part, path);
    append(path, sizeof path, name.path);
    append(part, sizeof part, name.part);
    file = ::open(part, O_WRONLY | O_CREAT | O_EXCL, 0600);
  }
  char digits[32];
  char at[40] = "0x";
  append(at, sizeof at, number(reinterpret_cast<unsigned long long>(address), digits, 16));
  if (file == kNoFile) {
    put(file, "Mitcad crashed; no crash report written:\n");
  }
  put(file, g_header);
  putLine(file, "time", number(static_cast<unsigned long long>(std::time(nullptr)), digits));
  putLine(file, "signal", signalName(signal));
  putLine(file, "address", at);
  putLine(file, "message", g_message);
  put(file, "stack:\n");
#ifdef MITCAD_HAS_BACKTRACE
  void* frames[kFrames];
  const int count = backtrace(frames, kFrames);
  backtrace_symbols_fd(frames, count, file != kNoFile ? file : STDERR_FILENO);
#endif
  putActions(file);
  put(file, "end\n");
  if (file != kNoFile) {
    ::close(file);
    ::rename(part, path);
    put(kNoFile, "Mitcad crashed: ");
    put(kNoFile, signalName(signal));
    put(kNoFile, "; crash report: ");
    put(kNoFile, path);
    put(kNoFile, "\n");
  }
}

void onSignal(int number, siginfo_t* info, void*) {
  if (g_entered.exchange(1) == 0) {
    writeReport(number, info != nullptr && number != SIGABRT ? info->si_addr : nullptr);
  }
  // As without this handler: the default action, once this returns (the
  // signal is blocked meanwhile; a fault also comes again by itself).
  struct sigaction action {};
  sigemptyset(&action.sa_mask);
  action.sa_handler = SIG_DFL;
  sigaction(number, &action, nullptr);
  raise(number);
}

// A stack of its own for the handler, so that a stack overflow on the
// installing (main) thread is reported too.
alignas(16) char g_signalStack[64 * 1024];

#endif

// An uncaught exception: its message, then abort() (and the report).
void onTerminate() {
  if (std::exception_ptr error = std::current_exception()) {
    try {
      std::rethrow_exception(error);
    } catch (const std::exception& exception) {
      const std::string text = std::string("uncaught exception: ") + exception.what();
      noteMessage(text.data(), text.size());
    } catch (...) {
      const char text[] = "uncaught exception";
      noteMessage(text, sizeof text - 1);
    }
  } else {
    const char text[] = "std::terminate";
    noteMessage(text, sizeof text - 1);
  }
  std::abort();
}

} // namespace detail

void install(const char* process, const char* version) {
  using namespace detail;
  copy(g_process, sizeof g_process, process);
  prepareHeader(version);
  const std::string directory = environment("MITCAD_CRASH_DIR");
  if (!directory.empty()) {
    setDirectory(directory.c_str());
  }
  std::set_terminate(onTerminate);
#ifdef _WIN32
  const DWORD length = GetModuleFileNameW(nullptr, g_symbolPath, static_cast<DWORD>(kPathSize));
  if (length == 0 || length >= kPathSize) {
    g_symbolPath[0] = L'\0';
  } else if (wchar_t* slash = wcsrchr(g_symbolPath, L'\\')) {
    *slash = L'\0';
  }
  SetUnhandledExceptionFilter(onException);
  std::signal(SIGABRT, onAbort);
#else
#ifdef MITCAD_HAS_BACKTRACE
  // The first backtrace() loads the unwinder, which allocates: not in a
  // handler.
  void* frames[2];
  backtrace(frames, 2);
#endif
  stack_t stack {};
  stack.ss_sp = g_signalStack;
  stack.ss_size = sizeof g_signalStack;
  sigaltstack(&stack, nullptr);
  struct sigaction action {};
  sigemptyset(&action.sa_mask);
  action.sa_flags = SA_SIGINFO | SA_ONSTACK;
  action.sa_sigaction = onSignal;
  for (const int number : kSignals) {
    sigaction(number, &action, nullptr);
  }
#endif
}

void setDirectory(const char* directory) {
  copy(g_directory, sizeof g_directory, directory);
#ifdef _WIN32
  g_directoryWide[0] = L'\0';
  MultiByteToWideChar(CP_UTF8, 0, g_directory, -1, g_directoryWide, static_cast<int>(kPathSize));
#endif
}

void noteAction(const char* action) {
  const unsigned index = g_actionCount.load();
  char* slot = g_actions[index % kActions];
  // Only the line's own text: no line breaks in the report.
  std::size_t i = 0;
  for (; action[i] != '\0' && i + 1 < kActionSize; ++i) {
    slot[i] = action[i] == '\n' || action[i] == '\r' ? ' ' : action[i];
  }
  slot[i] = '\0';
  g_actionCount.store(index + 1);
}

void noteMessage(const char* message, std::size_t length) {
  std::size_t used = std::strlen(g_message);
  if (used > 0 && used + 3 < kMessageSize) {
    copy(g_message + used, kMessageSize - used, " | ");
    used += 3;
  }
  for (std::size_t i = 0; i < length && used + 1 < kMessageSize; ++i) {
    g_message[used++] = message[i] == '\n' || message[i] == '\r' ? ' ' : message[i];
  }
  g_message[used] = '\0';
}

bool testCrashRequested(const char* where) { return environment("MITCAD_TEST_CRASH") == where; }

void crashNow() {
#ifdef __linux__
  // No core dump of a test's crash: a system that captures them (WSL's
  // crash capture through a pipe in core_pattern, which ignores
  // RLIMIT_CORE) would write one per run, and WSL's takes all of its
  // distributions down with it. A process that is not dumpable is never
  // dumped; the crash and its report are as before.
  prctl(PR_SET_DUMPABLE, 0, 0, 0, 0);
#endif
  volatile int* volatile nowhere = nullptr;
  *nowhere = 0;
  std::abort();
}

} // namespace mitcad::crash
