// SPDX-License-Identifier: MIT
// Corpus runs in parallel (mitcad#70); see parallel_runs.hpp.

#include "parallel_runs.hpp"

#ifdef _WIN32
#ifndef NOMINMAX
#define NOMINMAX
#endif
#include <windows.h>
#else
#include <cerrno>
#include <csignal>
#include <fcntl.h>
#include <sys/resource.h>
#include <sys/wait.h>
#include <unistd.h>
#endif
#ifdef __linux__
#include <sys/prctl.h>
#endif
#ifdef __APPLE__
#include <mach-o/dyld.h>
#endif

#include <algorithm>
#include <cctype>
#include <chrono>
#include <condition_variable>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <filesystem>
#include <fstream>
#include <memory>
#include <mutex>
#include <sstream>
#include <stdexcept>
#include <thread>

namespace mitcad::runs {

namespace {

namespace fs = std::filesystem;

const char kTotals[] = "@@totals\t";

std::string env(const char* name) {
#ifdef _MSC_VER
  char* value = nullptr;
  std::size_t length = 0;
  if (_dupenv_s(&value, &length, name) != 0 || value == nullptr) {
    return {};
  }
  std::string out(value);
  std::free(value);
  return out;
#else
  const char* value = std::getenv(name);
  return value != nullptr ? value : "";
#endif
}

// Takes the "\r" of each "\r\n" out (as CMake's execute_process does): on
// Windows the C runtime writes a child's text-mode "\n" as "\r\n", and its
// lines and totals lines are to read the same on every platform (mitcad#73).
void unix_line_ends(std::string& text) {
  std::size_t out = 0;
  for (std::size_t in = 0; in < text.size(); ++in) {
    if (text[in] != '\r' || in + 1 == text.size() || text[in + 1] != '\n') {
      text[out++] = text[in];
    }
  }
  text.resize(out);
}

double parse_number(const std::string& text, const char* what) {
  char* end = nullptr;
  const double value = std::strtod(text.c_str(), &end);
  if (text.empty() || end == nullptr || *end != '\0' || value < 0.0) {
    throw std::runtime_error(std::string("invalid ") + what + ": " + text);
  }
  return value;
}

#ifdef __linux__
// The memory a cgroup v2 hierarchy still allows this process: the smallest
// memory.max - memory.current of its cgroup and the cgroups above it.
std::uint64_t cgroup_available() {
  std::ifstream groups("/proc/self/cgroup");
  std::string line;
  std::uint64_t out = 0;
  while (std::getline(groups, line)) {
    if (line.rfind("0::", 0) != 0) {
      continue;
    }
    fs::path group = line.substr(3);
    for (;;) {
      const fs::path dir = fs::path("/sys/fs/cgroup") / group.relative_path();
      std::string max;
      std::uint64_t current = 0;
      std::ifstream(dir / "memory.max") >> max;
      std::ifstream(dir / "memory.current") >> current;
      if (!max.empty() && max != "max") {
        const std::uint64_t limit = std::strtoull(max.c_str(), nullptr, 10);
        const std::uint64_t left = limit > current ? limit - current : 0;
        out = out == 0 ? left : std::min(out, left);
      }
      if (group == group.root_path() || group.empty()) {
        break;
      }
      group = group.parent_path();
    }
  }
  return out;
}
#endif

#ifdef _WIN32
std::wstring widen(const std::string& text) {
  if (text.empty()) {
    return {};
  }
  const int size = MultiByteToWideChar(CP_ACP, 0, text.data(), static_cast<int>(text.size()), nullptr, 0);
  std::wstring out(static_cast<std::size_t>(size), L'\0');
  MultiByteToWideChar(CP_ACP, 0, text.data(), static_cast<int>(text.size()), out.data(), size);
  return out;
}

// An argument quoted as CommandLineToArgvW reads it back.
std::wstring quote(const std::wstring& argument) {
  if (!argument.empty() && argument.find_first_of(L" \t\n\v\"") == std::wstring::npos) {
    return argument;
  }
  std::wstring out = L"\"";
  for (auto at = argument.begin();; ++at) {
    std::size_t backslashes = 0;
    while (at != argument.end() && *at == L'\\') {
      ++at;
      ++backslashes;
    }
    if (at == argument.end()) {
      out.append(backslashes * 2, L'\\');
      break;
    }
    if (*at == L'"') {
      out.append(backslashes * 2 + 1, L'\\');
    } else {
      out.append(backslashes, L'\\');
    }
    out.push_back(*at);
  }
  out.push_back(L'"');
  return out;
}

std::string last_error() {
  const DWORD code = GetLastError();
  char text[256] = {};
  FormatMessageA(FORMAT_MESSAGE_FROM_SYSTEM | FORMAT_MESSAGE_IGNORE_INSERTS, nullptr, code, 0, text,
                 static_cast<DWORD>(sizeof text), nullptr);
  std::string out(text);
  while (!out.empty() && std::isspace(static_cast<unsigned char>(out.back()))) {
    out.pop_back();
  }
  return out.empty() ? "error " + std::to_string(code) : out;
}
#endif

// A child process and how it ended.
struct Child {
  Outcome outcome;
  bool started = false;
  bool exited = false;  // ended (not yet reaped on POSIX): not to be killed
  bool done = false;    // outcome complete
  bool ended_by_timeout = false;
  std::chrono::steady_clock::time_point start;
  std::thread reader;
#ifdef _WIN32
  HANDLE process = nullptr;
  HANDLE job = nullptr;
  HANDLE output = nullptr;
#else
  pid_t pid = -1;
  int output = -1;
#endif
};

class Runner {
 public:
  explicit Runner(const Settings& settings) : settings_(settings) {}

  // Starts the command; false (with the outcome) when it could not start.
  bool start(const Command& command, Child& child) {
    child.start = std::chrono::steady_clock::now();
    if (command.empty()) {
      child.outcome.failure = "could not start: no command";
      return false;
    }
#ifdef _WIN32
    // Only one inheritable handle exists while a child starts (children
    // start on this thread only), so no child inherits another's pipe.
    SECURITY_ATTRIBUTES inherit{};
    inherit.nLength = sizeof inherit;
    inherit.bInheritHandle = TRUE;
    HANDLE read = nullptr;
    HANDLE write = nullptr;
    if (!CreatePipe(&read, &write, &inherit, 0)) {
      child.outcome.failure = "could not start: " + last_error();
      return false;
    }
    SetHandleInformation(read, HANDLE_FLAG_INHERIT, 0);
    HANDLE input = CreateFileW(L"NUL", GENERIC_READ, FILE_SHARE_READ | FILE_SHARE_WRITE, &inherit, OPEN_EXISTING, 0,
                               nullptr);
    std::wstring line;
    for (const std::string& argument : command) {
      line += (line.empty() ? L"" : L" ") + quote(widen(argument));
    }
    STARTUPINFOW startup{};
    startup.cb = sizeof startup;
    startup.dwFlags = STARTF_USESTDHANDLES;
    startup.hStdInput = input;
    startup.hStdOutput = write;
    startup.hStdError = write;
    PROCESS_INFORMATION info{};
    const BOOL created = CreateProcessW(nullptr, line.data(), nullptr, nullptr, TRUE,
                                        CREATE_SUSPENDED | CREATE_NO_WINDOW, nullptr, nullptr, &startup, &info);
    const std::string error = created ? "" : last_error();
    CloseHandle(write);
    if (input != INVALID_HANDLE_VALUE) {
      CloseHandle(input);
    }
    if (!created) {
      CloseHandle(read);
      child.outcome.failure = "could not start: " + error;
      return false;
    }
    // A job of its own: the memory limit, and the child and its children
    // end with this process.
    child.job = CreateJobObjectW(nullptr, nullptr);
    if (child.job != nullptr) {
      JOBOBJECT_EXTENDED_LIMIT_INFORMATION limits{};
      limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
      if (settings_.memory > 0) {
        limits.BasicLimitInformation.LimitFlags |= JOB_OBJECT_LIMIT_PROCESS_MEMORY;
        limits.ProcessMemoryLimit = static_cast<SIZE_T>(settings_.memory);
      }
      SetInformationJobObject(child.job, JobObjectExtendedLimitInformation, &limits, sizeof limits);
      AssignProcessToJobObject(child.job, info.hProcess);
    }
    ResumeThread(info.hThread);
    CloseHandle(info.hThread);
    child.process = info.hProcess;
    child.output = read;
#else
    int pipe_ends[2];
    if (pipe(pipe_ends) != 0) {
      child.outcome.failure = std::string("could not start: ") + std::strerror(errno);
      return false;
    }
    fcntl(pipe_ends[0], F_SETFD, FD_CLOEXEC);
    fcntl(pipe_ends[1], F_SETFD, FD_CLOEXEC);
    const int input = open("/dev/null", O_RDONLY | O_CLOEXEC);
    std::vector<char*> argv;
    for (const std::string& argument : command) {
      argv.push_back(const_cast<char*>(argument.c_str()));
    }
    argv.push_back(nullptr);
    rlimit limit{};
    const bool limited = settings_.memory > 0 && getrlimit(RLIMIT_DATA, &limit) == 0;
    if (limited) {
      const rlim_t wanted = static_cast<rlim_t>(settings_.memory);
      if (limit.rlim_max == RLIM_INFINITY || wanted < limit.rlim_max) {
        limit.rlim_cur = wanted;
      } else {
        limit.rlim_cur = limit.rlim_max;
      }
    }
    const pid_t pid = fork();
    if (pid == 0) {
      // Only async-signal-safe calls until exec.
#ifdef __linux__
      prctl(PR_SET_PDEATHSIG, SIGKILL);
#endif
      if (input >= 0) {
        dup2(input, 0);
      }
      dup2(pipe_ends[1], 1);
      dup2(pipe_ends[1], 2);
      if (limited) {
        setrlimit(RLIMIT_DATA, &limit);
      }
      execvp(argv[0], argv.data());
      const char message[] = "could not start the child process\n";
      const ssize_t written = write(2, message, sizeof message - 1);
      (void)written;
      _exit(127);
    }
    const int error = errno;
    close(pipe_ends[1]);
    if (input >= 0) {
      close(input);
    }
    if (pid < 0) {
      close(pipe_ends[0]);
      child.outcome.failure = std::string("could not start: ") + std::strerror(error);
      return false;
    }
    child.pid = pid;
    child.output = pipe_ends[0];
#endif
    child.started = true;
    child.reader = std::thread([this, &child] { collect(child); });
    return true;
  }

  // Ends a child that ran over its time (under the lock).
  void end(Child& child) {
    if (!child.started || child.exited || child.ended_by_timeout) {
      return;
    }
    child.ended_by_timeout = true;
#ifdef _WIN32
    if (child.job != nullptr) {
      TerminateJobObject(child.job, 1);
    } else {
      TerminateProcess(child.process, 1);
    }
#else
    kill(child.pid, SIGKILL);
#endif
  }

  std::mutex mutex;
  std::condition_variable changed;
  int running = 0;

 private:
  // The reader thread: the child's output until it closes, then its end.
  void collect(Child& child) {
    std::string output;
    char buffer[65536];
#ifdef _WIN32
    for (;;) {
      DWORD count = 0;
      if (!ReadFile(child.output, buffer, static_cast<DWORD>(sizeof buffer), &count, nullptr) || count == 0) {
        break;
      }
      output.append(buffer, count);
    }
    CloseHandle(child.output);
    WaitForSingleObject(child.process, INFINITE);
    DWORD code = 0;
    GetExitCodeProcess(child.process, &code);
    std::unique_lock<std::mutex> lock(mutex);
    child.exited = true;
    lock.unlock();
    CloseHandle(child.process);
    std::uint64_t peak = 0;
    if (child.job != nullptr) {
      JOBOBJECT_EXTENDED_LIMIT_INFORMATION limits{};
      if (QueryInformationJobObject(child.job, JobObjectExtendedLimitInformation, &limits, sizeof limits,
                                    nullptr)) {
        peak = static_cast<std::uint64_t>(limits.PeakProcessMemoryUsed);
      }
      CloseHandle(child.job);
    }
    std::string failure;
    if (code >= 0xC0000000u) {
      char text[64];
      std::snprintf(text, sizeof text, "crashed (exception 0x%08lX)", static_cast<unsigned long>(code));
      failure = text;
    }
    const int exit_code = static_cast<int>(code);
#else
    for (;;) {
      const ssize_t count = read(child.output, buffer, sizeof buffer);
      if (count < 0 && errno == EINTR) {
        continue;
      }
      if (count <= 0) {
        break;
      }
      output.append(buffer, static_cast<std::size_t>(count));
    }
    close(child.output);
    // Waited for without reaping first, so that end() cannot kill another
    // process that got its pid.
    siginfo_t info{};
    while (waitid(P_PID, static_cast<id_t>(child.pid), &info, WEXITED | WNOWAIT) != 0 && errno == EINTR) {
    }
    std::unique_lock<std::mutex> lock(mutex);
    child.exited = true;
    lock.unlock();
    int status = 0;
    rusage usage{};
    while (wait4(child.pid, &status, 0, &usage) < 0 && errno == EINTR) {
    }
#ifdef __APPLE__
    const std::uint64_t peak = static_cast<std::uint64_t>(usage.ru_maxrss);
#else
    const std::uint64_t peak = static_cast<std::uint64_t>(usage.ru_maxrss) * 1024;
#endif
    std::string failure;
    int exit_code = -1;
    if (WIFEXITED(status)) {
      exit_code = WEXITSTATUS(status);
    } else if (WIFSIGNALED(status)) {
      failure = "crashed (signal " + std::to_string(WTERMSIG(status)) + ")";
    }
#endif
    unix_line_ends(output);
    lock.lock();
    if (child.ended_by_timeout) {
      std::ostringstream text;
      text << "was ended after " << settings_.timeout << " s (--file-timeout)";
      failure = text.str();
    }
    child.outcome.output = std::move(output);
    child.outcome.exit_code = exit_code;
    child.outcome.failure = failure;
    child.outcome.peak_memory = peak;
    child.outcome.seconds = std::chrono::duration<double>(std::chrono::steady_clock::now() - child.start).count();
    child.done = true;
    --running;
    changed.notify_all();
  }

  Settings settings_;
};

}  // namespace

std::uint64_t parse_size(const std::string& text) {
  if (text == "none" || text == "0") {
    return 0;
  }
  std::size_t end = 0;
  double value = 0.0;
  try {
    value = std::stod(text, &end);
  } catch (const std::exception&) {
    throw std::runtime_error("invalid memory size: " + text);
  }
  std::string unit = text.substr(end);
  for (char& c : unit) {
    c = static_cast<char>(std::toupper(static_cast<unsigned char>(c)));
  }
  if (!unit.empty() && unit.back() == 'B') {
    unit.pop_back();
  }
  if (!unit.empty() && unit.back() == 'I') {
    unit.pop_back();
  }
  double scale = 1.0;
  if (unit == "K") {
    scale = 1024.0;
  } else if (unit == "M") {
    scale = 1024.0 * 1024.0;
  } else if (unit == "G") {
    scale = 1024.0 * 1024.0 * 1024.0;
  } else if (unit == "T") {
    scale = 1024.0 * 1024.0 * 1024.0 * 1024.0;
  } else if (!unit.empty()) {
    throw std::runtime_error("invalid memory size: " + text);
  }
  if (value < 0.0) {
    throw std::runtime_error("invalid memory size: " + text);
  }
  return static_cast<std::uint64_t>(value * scale);
}

std::string format_size(std::uint64_t bytes) {
  if (bytes == 0) {
    return "none";
  }
  char text[32];
  if (bytes % (1ull << 30) == 0) {
    std::snprintf(text, sizeof text, "%lluG", static_cast<unsigned long long>(bytes >> 30));
  } else {
    std::snprintf(text, sizeof text, "%lluM", static_cast<unsigned long long>(bytes >> 20));
  }
  return text;
}

std::uint64_t available_memory() {
#ifdef _WIN32
  MEMORYSTATUSEX status{};
  status.dwLength = sizeof status;
  return GlobalMemoryStatusEx(&status) ? static_cast<std::uint64_t>(status.ullAvailPhys) : 0;
#else
  std::uint64_t out = 0;
#ifdef __linux__
  std::ifstream info("/proc/meminfo");
  std::string key;
  std::uint64_t kib = 0;
  std::string unit;
  while (info >> key >> kib >> unit) {
    if (key == "MemAvailable:") {
      out = kib * 1024;
      break;
    }
  }
  const std::uint64_t group = cgroup_available();
  if (group > 0) {
    out = out == 0 ? group : std::min(out, group);
  }
#else
  // Half the physical memory where the free memory is not at hand.
  const long pages = sysconf(_SC_PHYS_PAGES);
  const long size = sysconf(_SC_PAGESIZE);
  if (pages > 0 && size > 0) {
    out = static_cast<std::uint64_t>(pages) * static_cast<std::uint64_t>(size) / 2;
  }
#endif
  return out;
#endif
}

int processor_count() {
  const unsigned n = std::thread::hardware_concurrency();
  return n == 0 ? 1 : static_cast<int>(n);
}

int default_jobs(std::uint64_t memory) {
  int jobs = std::max(1, processor_count() / 4);
  const std::uint64_t available = available_memory();
  if (memory > 0 && available > 0) {
    jobs = static_cast<int>(std::min<std::uint64_t>(static_cast<std::uint64_t>(jobs), available / memory));
  }
  return std::max(1, jobs);
}

Settings settings(const std::string& jobs, const std::string& memory, const std::string& timeout) {
  Settings out;
  const std::string memory_text = !memory.empty() ? memory : env("MITCAD_CORPUS_MEMORY");
  out.memory = memory_text.empty() ? kDefaultMemory : parse_size(memory_text);
  const std::string timeout_text = !timeout.empty() ? timeout : env("MITCAD_CORPUS_TIMEOUT");
  out.timeout = timeout_text.empty() ? kDefaultTimeout : parse_number(timeout_text, "--file-timeout");
  const std::string jobs_text = !jobs.empty() ? jobs : env("MITCAD_CORPUS_JOBS");
  if (jobs_text.empty() || jobs_text == "auto") {
    out.jobs = default_jobs(out.memory);
  } else {
    const double n = parse_number(jobs_text, "--jobs");
    if (n < 1.0) {
      throw std::runtime_error("invalid --jobs: " + jobs_text);
    }
    out.jobs = static_cast<int>(n);
  }
  return out;
}

void take_options(std::vector<std::string>& args, std::string& jobs, std::string& memory, std::string& timeout) {
  std::vector<std::string> rest;
  for (std::size_t i = 0; i < args.size(); ++i) {
    if (i + 1 < args.size() && args[i] == "--jobs") {
      jobs = args[++i];
    } else if (i + 1 < args.size() && args[i] == "--memory") {
      memory = args[++i];
    } else if (i + 1 < args.size() && args[i] == "--file-timeout") {
      timeout = args[++i];
    } else {
      rest.push_back(args[i]);
    }
  }
  args = std::move(rest);
}

std::string executable_path(const char* argv0) {
#ifdef _WIN32
  std::wstring path(32768, L'\0');
  const DWORD size = GetModuleFileNameW(nullptr, path.data(), static_cast<DWORD>(path.size()));
  if (size > 0 && size < path.size()) {
    path.resize(size);
    return fs::path(path).string();
  }
#elif defined(__linux__)
  std::error_code ec;
  const fs::path self = fs::read_symlink("/proc/self/exe", ec);
  if (!ec) {
    return self.string();
  }
#elif defined(__APPLE__)
  char path[4096];
  std::uint32_t size = sizeof path;
  if (_NSGetExecutablePath(path, &size) == 0) {
    return path;
  }
#endif
  std::error_code error;
  const fs::path absolute = fs::absolute(argv0, error);
  return error ? std::string(argv0) : absolute.string();
}

void run_all(const std::vector<Command>& commands, const Settings& settings,
             const std::function<void(std::size_t, const Outcome&)>& done, const std::vector<double>& weights) {
  Runner runner(settings);
  const int jobs = std::max(1, settings.jobs);
  // The heaviest first, so that a long one does not start last.
  std::vector<std::size_t> order(commands.size());
  for (std::size_t i = 0; i < order.size(); ++i) {
    order[i] = i;
  }
  if (weights.size() == commands.size()) {
    std::stable_sort(order.begin(), order.end(),
                     [&weights](std::size_t a, std::size_t b) { return weights[a] > weights[b]; });
  }
  std::vector<std::unique_ptr<Child>> children(commands.size());
  std::size_t next = 0;
  std::size_t reported = 0;
  std::uint64_t peak = 0;
  double longest = 0.0;
  std::unique_lock<std::mutex> lock(runner.mutex);
  while (reported < commands.size()) {
    while (runner.running < jobs && next < commands.size()) {
      const std::size_t index = order[next++];
      children[index] = std::make_unique<Child>();
      Child& child = *children[index];
      ++runner.running;
      lock.unlock();
      const bool started = runner.start(commands[index], child);
      lock.lock();
      if (!started) {
        child.done = true;
        --runner.running;
      }
    }
    while (reported < commands.size() && children[reported] && children[reported]->done) {
      Child& child = *children[reported];
      lock.unlock();
      if (child.reader.joinable()) {
        child.reader.join();
      }
      done(reported, child.outcome);
      peak = std::max(peak, child.outcome.peak_memory);
      longest = std::max(longest, child.outcome.seconds);
      children[reported].reset();
      ++reported;
      lock.lock();
    }
    if (reported == commands.size()) {
      std::fprintf(stderr,
                   "parallel run: %zu child processes, %d at a time, memory limit %s each; the largest peak "
                   "memory %s, the longest %.1f s\n",
                   commands.size(), jobs, format_size(settings.memory).c_str(),
                   peak > 0 ? format_size(peak).c_str() : "unknown", longest);
      break;
    }
    runner.changed.wait_for(lock, std::chrono::milliseconds(500));
    if (settings.timeout > 0.0) {
      const auto now = std::chrono::steady_clock::now();
      for (std::size_t i = reported; i < commands.size(); ++i) {
        Child* child = children[i].get();
        if (child != nullptr && !child->done &&
            std::chrono::duration<double>(now - child->start).count() > settings.timeout) {
          runner.end(*child);
        }
      }
    }
  }
}

double file_weight(const std::string& path) {
  std::error_code ec;
  const auto size = fs::file_size(path, ec);
  return ec ? 0.0 : static_cast<double>(size);
}

void no_core_dumps() {
#ifdef __linux__
  const char* keep = std::getenv("MITCAD_CORE_DUMPS");
  if (keep == nullptr || std::string(keep) != "1") {
    prctl(PR_SET_DUMPABLE, 0, 0, 0, 0);
  }
#endif
}

void print_totals(const std::map<std::string, double>& totals) {
  std::fflush(stdout);
  for (const auto& [key, value] : totals) {
    std::printf("%s%s\t%.17g\n", kTotals, key.c_str(), value);
  }
  std::fflush(stdout);
}

bool split_totals(const std::string& output, std::string& text, std::map<std::string, double>& totals) {
  bool found = false;
  text.clear();
  std::size_t at = 0;
  while (at < output.size()) {
    std::size_t end = output.find('\n', at);
    end = end == std::string::npos ? output.size() : end + 1;
    const std::string line = output.substr(at, end - at);
    if (line.rfind(kTotals, 0) == 0) {
      const std::size_t tab = line.find('\t', sizeof kTotals - 1);
      if (tab != std::string::npos) {
        const std::string key = line.substr(sizeof kTotals - 1, tab - (sizeof kTotals - 1));
        const double value = std::strtod(line.c_str() + tab + 1, nullptr);
        double& total = totals[key];
        total = key.rfind("max:", 0) == 0 ? std::max(total, value) : total + value;
      }
      found = true;
    } else {
      text += line;
    }
    at = end;
  }
  return found;
}

std::string describe_failure(const std::string& id, const Outcome& outcome, bool finished) {
  if (finished) {
    return {};
  }
  std::string why = outcome.failure;
  if (why.empty()) {
    why = "exited with status " + std::to_string(outcome.exit_code) + " before its totals";
  }
  return id + ": the child process " + why + "\n";
}

}  // namespace mitcad::runs
