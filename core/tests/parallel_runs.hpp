// SPDX-License-Identifier: MIT
// Corpus runs in parallel (mitcad#70): each file in a child process of its
// own, several at a time, each under its own memory limit, the results
// collected in the order of the files.
//
// A corpus test (test_f3d_import, test_brep_import, test_exchange and
// mitcad_run_parallel for the FreeCAD corpus) starts itself again per file
// with an option naming the file. The child prints the file's lines as the
// sequential run does and then its share of the totals as "totals lines"
// (print_totals); the parent prints each child's text in the files' order
// and adds up the totals. A child that crashes, runs out of its memory or
// over its time ends only its own file: it is reported, and counted as
// failed, with whatever it printed.
//
// --jobs N (MITCAD_CORPUS_JOBS) sets how many children run at once; 1 keeps
// the sequential run in one process. The default is the cores / 4, bounded
// by the memory available (MemAvailable and the cgroup's memory.max on
// Linux, the free physical memory on Windows) / the limit per child.
// --memory SIZE (MITCAD_CORPUS_MEMORY, e.g. 3G, 1500M; "none") is that
// limit on the memory a child commits: RLIMIT_DATA on Linux (its private
// writable memory: heap, thread stacks; not the address space it only
// reserves, which is many times larger), inherited by the child's own
// children; a job object's process memory limit on Windows (committed
// memory). macOS does not enforce it. A child over it fails to allocate
// (its file fails), or hangs in a thread that did: --file-timeout S
// (MITCAD_CORPUS_TIMEOUT, default 3600; 0: none) ends a child that runs
// longer.
#pragma once

#include <cstddef>
#include <cstdint>
#include <functional>
#include <map>
#include <string>
#include <vector>

namespace mitcad::runs {

// A program and its arguments.
using Command = std::vector<std::string>;

struct Settings {
  int jobs = 1;
  std::uint64_t memory = 0;  // bytes per child, 0: no limit
  double timeout = 0.0;      // seconds per child, 0: none
};

// How a child ended.
struct Outcome {
  std::string output;   // its standard output and error, together, with
                        // "\r\n" as "\n" (Windows' text mode writes "\r\n")
  int exit_code = -1;   // when it exited
  std::string failure;  // empty when it exited by itself, else why not
                        // ("crashed (signal 11)", "was ended after 60 s", ...)
  std::uint64_t peak_memory = 0;  // bytes: its largest resident set (POSIX,
                                  // with its children), committed memory
                                  // (Windows); 0 when unknown
  double seconds = 0.0;
};

// The default limit per child.
constexpr std::uint64_t kDefaultMemory = 4ull << 30;
constexpr double kDefaultTimeout = 3600.0;

// The settings from the options (empty when not given), else the
// environment, else the defaults; throws on malformed values.
Settings settings(const std::string& jobs, const std::string& memory, const std::string& timeout);

// Takes --jobs N, --memory SIZE and --file-timeout S out of `args` (the
// values into the strings, unchanged when absent).
void take_options(std::vector<std::string>& args, std::string& jobs, std::string& memory, std::string& timeout);

// "4G", "1500M", "800K" or bytes; "none" or "0" is 0. Throws when malformed.
std::uint64_t parse_size(const std::string& text);
std::string format_size(std::uint64_t bytes);

// The memory this process may still use (bytes; 0 when unknown).
std::uint64_t available_memory();
int processor_count();
// min(cores / 4, available memory / memory per child), at least 1.
int default_jobs(std::uint64_t memory);

// This program's path (argv0 as a fallback).
std::string executable_path(const char* argv0);

// Runs every command, up to settings.jobs at a time, each a child process
// whose standard output and error go into one pipe, and calls done(i,
// outcome) for each in the commands' order on the calling thread, as soon
// as it and every one before it have ended. With `weights` (one per
// command), the heaviest start first. At the end a line on standard error
// tells how many ran at once, their memory limit, the largest peak memory
// and the longest time of a child.
void run_all(const std::vector<Command>& commands, const Settings& settings,
             const std::function<void(std::size_t, const Outcome&)>& done, const std::vector<double>& weights = {});

// A file's size as its weight (0 when it cannot be read).
double file_weight(const std::string& path);

// The child's side: prints its totals after its text (flushing stdout).
void print_totals(const std::map<std::string, double>& totals);

// The parent's side: the child's text without the totals lines, and the
// totals added to `totals` (keys "max:..." keep the largest value instead);
// false when the child printed none (it did not finish).
bool split_totals(const std::string& output, std::string& text, std::map<std::string, double>& totals);

// The child's text and why it did not finish, as a line naming `id`, or
// nothing when it did.
std::string describe_failure(const std::string& id, const Outcome& outcome, bool finished);

}  // namespace mitcad::runs
