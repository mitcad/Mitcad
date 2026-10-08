// SPDX-License-Identifier: MIT
// Runs a list of commands, several at a time, each a child process under
// its own memory limit, and prints their output in the list's order
// (mitcad#70, parallel_runs.hpp). The FreeCAD corpus run
// (tools/cli/fcstd-corpus.cmake) runs its files with it.
//
//   mitcad_run_parallel [--jobs N] [--memory SIZE] [--file-timeout S] <list>
//
// Each line of the list is a command: a label, the program and its
// arguments, separated by tabs. A command that does not exit with 0 adds
// "FAILED <label>: ..." after its output. Exits with 0 when every command
// did, else 1.
//
//   mitcad_run_parallel --self-test     the ctest core.parallel_runs: runs
//                                       itself as children that print,
//                                       crash, hang and allocate too much

#include <algorithm>
#include <chrono>
#include <cstdio>
#include <cstdlib>
#include <exception>
#include <fstream>
#include <map>
#include <memory>
#include <new>
#include <string>
#include <thread>
#include <vector>

#include "parallel_runs.hpp"

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "run_parallel.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(condition) check((condition), #condition, __LINE__)

bool contains(const std::string& text, const std::string& part) { return text.find(part) != std::string::npos; }

// The self test's children.
int test_child(const std::string& mode, const std::string& value) {
  if (mode == "print") {
    const int n = std::atoi(value.c_str());
    // The later ones end first.
    std::this_thread::sleep_for(std::chrono::milliseconds(50 * (6 - n)));
    std::printf("line %d\n", n);
    std::fprintf(stderr, "error %d\n", n);
    mitcad::runs::print_totals({{"n", n}, {"max:largest", n}});
    return 0;
  }
  if (mode == "crash") {
    std::printf("before the crash\n");
    std::fflush(stdout);
#ifdef _MSC_VER
    // No message box, no error report: the process just ends.
    _set_abort_behavior(0, _WRITE_ABORT_MSG | _CALL_REPORTFAULT);
#endif
    std::abort();
  }
  if (mode == "sleep") {
    std::this_thread::sleep_for(std::chrono::seconds(std::atoi(value.c_str())));
    return 0;
  }
  if (mode == "alloc") {
    try {
      const std::size_t size = static_cast<std::size_t>(mitcad::runs::parse_size(value));
      std::unique_ptr<char[]> block(new char[size]);
      for (std::size_t i = 0; i < size; i += 4096) {
        block[i] = 1;
      }
      std::printf("allocated %d\n", block[size / 2]);
    } catch (const std::bad_alloc&) {
      std::printf("allocation failed\n");
    }
    return 0;
  }
  return 2;
}

int self_test(const std::string& self) {
  CHECK(mitcad::runs::parse_size("4G") == 4ull << 30);
  CHECK(mitcad::runs::parse_size("1500M") == 1500ull << 20);
  CHECK(mitcad::runs::parse_size("none") == 0);
  CHECK(mitcad::runs::format_size(3ull << 30) == "3G");
  bool threw = false;
  try {
    mitcad::runs::parse_size("4X");
  } catch (const std::exception&) {
    threw = true;
  }
  CHECK(threw);
  CHECK(mitcad::runs::default_jobs(1) >= 1);

  // In the commands' order whatever order they end in, the heaviest
  // started first, the totals apart from the text, its lines ending in
  // "\n" on every platform (mitcad#73).
  mitcad::runs::Settings settings;
  settings.jobs = 3;
  std::vector<mitcad::runs::Command> commands;
  std::vector<double> weights;
  for (int i = 0; i < 6; ++i) {
    commands.push_back({self, "--test-child", "print", std::to_string(i)});
    weights.push_back(i);
  }
  std::vector<std::size_t> order;
  double sum = 0.0;
  double largest = 0.0;
  mitcad::runs::run_all(
      commands, settings,
      [&](std::size_t i, const mitcad::runs::Outcome& outcome) {
        order.push_back(i);
        std::string text;
        std::map<std::string, double> totals;
        CHECK(mitcad::runs::split_totals(outcome.output, text, totals));
        CHECK(contains(text, "line " + std::to_string(i) + "\n"));
        CHECK(contains(text, "error " + std::to_string(i) + "\n"));
        CHECK(!contains(text, "@@totals"));
        CHECK(outcome.failure.empty() && outcome.exit_code == 0);
        sum += totals["n"];
        largest = std::max(largest, totals["max:largest"]);
      },
      weights);
  CHECK((order == std::vector<std::size_t>{0, 1, 2, 3, 4, 5}));
  CHECK(sum == 15.0);
  CHECK(largest == 5.0);

  // A crash, a hang over the time limit, too much memory and a program
  // that does not exist end only their own runs.
  settings.timeout = 1.0;
  settings.memory = 512ull << 20;
  commands = {{self, "--test-child", "crash", ""},
              {self, "--test-child", "sleep", "30"},
              {self, "--test-child", "alloc", "2G"},
              {self + "-missing"},
              {self, "--test-child", "print", "1"}};
  std::vector<mitcad::runs::Outcome> outcomes;
  const auto start = std::chrono::steady_clock::now();
  mitcad::runs::run_all(commands, settings,
                        [&](std::size_t, const mitcad::runs::Outcome& outcome) { outcomes.push_back(outcome); });
  CHECK(std::chrono::duration<double>(std::chrono::steady_clock::now() - start).count() < 20.0);
  CHECK(outcomes.size() == 5);
  if (outcomes.size() == 5) {
    std::string text;
    std::map<std::string, double> totals;
    CHECK(!mitcad::runs::split_totals(outcomes[0].output, text, totals));
    CHECK(contains(text, "before the crash"));
    CHECK(!mitcad::runs::describe_failure("f01", outcomes[0], false).empty());
    CHECK(contains(outcomes[1].failure, "was ended after 1 s"));
#ifndef __APPLE__
    // macOS does not enforce RLIMIT_DATA.
    CHECK(contains(outcomes[2].output, "allocation failed"));
#endif
    CHECK(!outcomes[3].failure.empty() || outcomes[3].exit_code != 0);
    CHECK(mitcad::runs::split_totals(outcomes[4].output, text, totals) && totals["n"] == 1.0);
  }
  if (failures == 0) {
    std::printf("mitcad_run_parallel: all checks passed\n");
  }
  return failures == 0 ? 0 : 1;
}

}  // namespace

int main(int argc, char** argv) {
  std::vector<std::string> args(argv + 1, argv + argc);
  try {
    if (args.size() == 3 && args[0] == "--test-child") {
      return test_child(args[1], args[2]);
    }
    if (args.size() == 1 && args[0] == "--self-test") {
      return self_test(mitcad::runs::executable_path(argv[0]));
    }
    std::string jobs;
    std::string memory;
    std::string timeout;
    mitcad::runs::take_options(args, jobs, memory, timeout);
    if (args.size() != 1) {
      std::fprintf(stderr, "usage: mitcad_run_parallel [--jobs N] [--memory SIZE] [--file-timeout S] <list>\n");
      return 2;
    }
    std::ifstream list(args[0]);
    if (!list) {
      std::fprintf(stderr, "mitcad_run_parallel: cannot read %s\n", args[0].c_str());
      return 2;
    }
    std::vector<std::string> labels;
    std::vector<mitcad::runs::Command> commands;
    std::string line;
    while (std::getline(list, line)) {
      if (!line.empty() && line.back() == '\r') {
        line.pop_back();
      }
      if (line.empty()) {
        continue;
      }
      std::vector<std::string> fields;
      std::size_t at = 0;
      for (;;) {
        const std::size_t tab = line.find('\t', at);
        fields.push_back(line.substr(at, tab == std::string::npos ? std::string::npos : tab - at));
        if (tab == std::string::npos) {
          break;
        }
        at = tab + 1;
      }
      labels.push_back(fields.front());
      commands.emplace_back(fields.begin() + 1, fields.end());
    }
    int failed = 0;
    mitcad::runs::run_all(commands, mitcad::runs::settings(jobs, memory, timeout),
                          [&](std::size_t i, const mitcad::runs::Outcome& outcome) {
                            std::fputs(outcome.output.c_str(), stdout);
                            if (!outcome.failure.empty() || outcome.exit_code != 0) {
                              ++failed;
                              const std::string why =
                                  outcome.failure.empty() ? "exited with status " + std::to_string(outcome.exit_code)
                                                          : outcome.failure;
                              std::printf("FAILED %s: the child process %s\n", labels[i].c_str(), why.c_str());
                            }
                            std::fflush(stdout);
                          });
    return failed == 0 ? 0 : 1;
  } catch (const std::exception& e) {
    std::fprintf(stderr, "mitcad_run_parallel: %s\n", e.what());
    return 2;
  }
}
