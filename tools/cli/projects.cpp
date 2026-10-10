// SPDX-License-Identifier: MIT
// mitcad-cli for Local and Cloud projects (mitcad#89): what a folder is,
// a project made Local or Cloud, its settings, what a remote holds
// without a project, a Local project shared onto a repository's files, a
// repository with files opened as a project, and SSH servers' host keys.
#include "projects.hpp"

#include <cstdio>
#include <iostream>

#include "mitcad_bridge/lib.h"
#include "rust/cxx.h"

namespace mitcad::cli {

const char* const kProjectsUsage = R"(  mitcad-cli project inspect <folder> [--json]
      What a folder is: missing, empty, a project (Local, or Cloud when its
      repository has a remote), inside a project, a git repository, a
      folder of designs, or other files; the project's remote, designs,
      settings and the design opened last.
  mitcad-cli project create <folder> --author "Name <email>" [--url <url>] [--no-push]
                            [--design <name.mitcad>] [--json]
      Makes a project in a new folder, an empty one, or one of designs or
      other files: the marker, .gitattributes, .gitignore, a git
      repository, the author in its configuration, and a first version with
      the designs there (and a new empty design with --design). With --url
      a Cloud project: an empty remote gets the versions; a remote with
      files but no project is cloned into the (new or empty) folder and the
      project made beside its files; a remote with a project is refused.
  mitcad-cli project settings <folder> [--set <json>] [--author "Name <email>"] [--json]
      The project's settings: shared (edit locks, live updates; a change is
      recorded as a version) and this computer's (live updates, sync).
      --set takes {"shared": {...}, "local": {...}}, merged into them.
  mitcad-cli remote check <url> [--json]
      What a remote holds, without a project: reachable, empty, a Mitcad
      project, the names at its root, its versions and the latest one.
  mitcad-cli remote share <folder> <url> [--author A] [--resolve <path>=mine|theirs|copy]...
                          [--resolve-all C] [--no-push] [--json]
      Connects a project to a remote as remote add does; a remote with
      files but no project gets the project's versions after its own
      (.gitignore and .gitattributes merged by lines, a file on both sides
      a choice as in sync; without one nothing changes and the exit status
      is 3).
  mitcad-cli clone <url> <folder> --adopt [--author A] [--json]
      Opens a remote as clone does; one with files but no project is made a
      project (its first version "Make this repository a Mitcad project")
      and pushed.
  mitcad-cli host-keys <host> [--port P] [--trust] [--json]
      The SSH host keys the server offers with their fingerprints, compared
      with those GitHub, GitLab and Codeberg publish and with
      ~/.ssh/known_hosts; --trust adds them to known_hosts (never a key that
      differs from what the service publishes).
)";

namespace {

std::string json_text(const std::string& text) {
  std::string out = "\"";
  for (const char c : text) {
    switch (c) {
    case '"':
      out += "\\\"";
      break;
    case '\\':
      out += "\\\\";
      break;
    case '\n':
      out += "\\n";
      break;
    case '\r':
      out += "\\r";
      break;
    case '\t':
      out += "\\t";
      break;
    default:
      if (static_cast<unsigned char>(c) < 0x20) {
        char escaped[8];
        std::snprintf(escaped, sizeof escaped, "\\u%04x", static_cast<unsigned>(c));
        out += escaped;
      } else {
        out += c;
      }
    }
  }
  return out + "\"";
}

int problem(const std::string& text) {
  std::cerr << "mitcad-cli: " << text << "\nusage:\n" << kProjectsUsage;
  return 2;
}

// Positional arguments and options (repeatable ones collect).
struct Parsed {
  std::vector<std::string> positional;
  std::vector<std::pair<std::string, std::string>> options;

  const std::string* one(const std::string& name) const {
    for (const auto& [key, value] : options) {
      if (key == name) {
        return &value;
      }
    }
    return nullptr;
  }
  std::vector<std::string> all(const std::string& name) const {
    std::vector<std::string> out;
    for (const auto& [key, value] : options) {
      if (key == name) {
        out.push_back(value);
      }
    }
    return out;
  }
  bool flag(const std::string& name) const { return one(name) != nullptr; }
};

bool parse(const std::vector<std::string>& args, const std::vector<std::string>& with_value,
           const std::vector<std::string>& flags, Parsed& out, std::string& error) {
  const auto listed = [](const std::vector<std::string>& list, const std::string& name) {
    for (const std::string& item : list) {
      if (item == name) {
        return true;
      }
    }
    return false;
  };
  for (std::size_t i = 1; i < args.size(); ++i) {
    const std::string& arg = args[i];
    if (arg.size() > 1 && arg[0] == '-') {
      if (listed(flags, arg)) {
        out.options.emplace_back(arg, "");
      } else if (listed(with_value, arg) && i + 1 < args.size()) {
        out.options.emplace_back(arg, args[++i]);
      } else {
        error = "unexpected argument '" + arg + "'";
        return false;
      }
    } else {
      out.positional.push_back(arg);
    }
  }
  return true;
}

bool failed(const std::string& answer) { return answer.find("\"error\":null") == std::string::npos; }

// Runs a command without a project; prints JSON (--json) or text. A
// failure in the answer is the exit status 1 (in text, an error).
int run(const std::string& command, bool json) {
  const rust::Box<mitcad::SyncControl> control = mitcad::new_sync_control();
  if (json) {
    const std::string answer(mitcad::projects_command(command, *control));
    std::cout << answer << "\n";
    return failed(answer) ? 1 : 0;
  }
  std::cout << std::string(mitcad::projects_command_text(command, *control));
  return 0;
}

// Runs a command of the project at `folder`; prints JSON or text.
int run_in(const std::string& folder, const std::string& command, bool json) {
  const rust::Box<mitcad::Project> project = mitcad::open_project(folder);
  if (json) {
    std::cout << std::string(project->command(command)) << "\n";
  } else {
    std::cout << std::string(project->command_text(command));
  }
  return 0;
}

// A port: 1 to 65535.
bool port_of(const std::string& text, std::string& port) {
  if (text.empty() || text.size() > 5 || text.find_first_not_of("0123456789") != std::string::npos) {
    return false;
  }
  const long value = std::stol(text);
  if (value < 1 || value > 65535) {
    return false;
  }
  port = std::to_string(value);
  return true;
}

} // namespace

int project(const std::vector<std::string>& args) {
  Parsed a;
  std::string error;
  if (!parse(args, {"--author", "--url", "--design", "--set"}, {"--json", "--no-push"}, a, error)) {
    return problem(error);
  }
  if (a.positional.size() != 2) {
    return problem("project needs inspect, create or settings and a folder");
  }
  const std::string& what = a.positional[0];
  const std::string& folder = a.positional[1];
  const bool json = a.flag("--json");
  const std::string* author = a.one("--author");
  if (what == "inspect") {
    if (a.options.size() != (json ? 1U : 0U)) {
      return problem("project inspect takes only --json");
    }
    return run(R"({"cmd": "inspect_folder", "dir": )" + json_text(folder) + "}", json);
  }
  if (what == "create") {
    if (author == nullptr) {
      return problem("project create needs --author \"Name <email>\"");
    }
    if (a.flag("--set")) {
      return problem("--set is for project settings");
    }
    std::string command =
        R"({"cmd": "create_project", "dir": )" + json_text(folder) + R"(, "author": )" + json_text(*author);
    if (const std::string* url = a.one("--url")) {
      command += R"(, "url": )" + json_text(*url);
    } else if (a.flag("--no-push")) {
      return problem("--no-push goes with --url");
    }
    if (a.flag("--no-push")) {
      command += R"(, "push": false)";
    }
    if (const std::string* design = a.one("--design")) {
      const rust::Box<mitcad::Document> empty = mitcad::new_document();
      command += R"(, "design": {"path": )" + json_text(*design) + R"(, "text": )" +
                 json_text(std::string(empty->to_json())) + "}";
    }
    return run(command + "}", json);
  }
  if (what == "settings") {
    if (a.one("--url") != nullptr || a.one("--design") != nullptr || a.flag("--no-push")) {
      return problem("project settings takes --set, --author and --json");
    }
    const std::string* set = a.one("--set");
    if (set == nullptr) {
      if (author != nullptr) {
        return problem("--author goes with --set");
      }
      return run_in(folder, R"({"cmd": "project_settings"})", json);
    }
    const std::size_t open = set->find('{');
    const std::size_t close = set->rfind('}');
    if (open == std::string::npos || close == std::string::npos || close < open ||
        set->find_first_not_of(" \t\r\n", close + 1) != std::string::npos ||
        set->find_first_not_of(" \t\r\n") != open) {
      return problem("--set takes a JSON object: {\"shared\": {...}, \"local\": {...}}");
    }
    const std::string inner = set->substr(open + 1, close - open - 1);
    std::string command = R"({"cmd": "set_project_settings")";
    if (author != nullptr) {
      command += R"(, "author": )" + json_text(*author);
    }
    if (inner.find_first_not_of(" \t\r\n") != std::string::npos) {
      command += ", " + inner;
    }
    return run_in(folder, command + "}", json);
  }
  return problem("project " + what + ": wrong arguments");
}

int check_remote(const std::string& url, bool json) {
  return run(R"({"cmd": "check_remote", "url": )" + json_text(url) + "}", json);
}

int remote_share(const std::vector<std::string>& args) {
  Parsed a;
  std::string error;
  if (!parse(args, {"--author", "--resolve", "--resolve-all", "--name"}, {"--json", "--no-push"}, a, error)) {
    return problem(error);
  }
  if (a.positional.size() != 3 || a.positional[0] != "share") {
    return problem("remote share needs a project's folder and a remote's URL");
  }
  const auto is_choice = [](const std::string& text) { return text == "mine" || text == "theirs" || text == "copy"; };
  std::string resolutions;
  for (const std::string& value : a.all("--resolve")) {
    const std::size_t equals = value.rfind('=');
    if (equals == std::string::npos || equals == 0 || !is_choice(value.substr(equals + 1))) {
      return problem("--resolve takes <path>=mine, <path>=theirs or <path>=copy");
    }
    resolutions += (resolutions.empty() ? "" : ", ") + json_text(value.substr(0, equals)) + ": " +
                   json_text(value.substr(equals + 1));
  }
  std::string command = R"({"cmd": "connect", "onto_files": true, "url": )" + json_text(a.positional[2]) +
                        R"(, "resolutions": {)" + resolutions + "}";
  if (const std::string* all = a.one("--resolve-all")) {
    if (!is_choice(*all)) {
      return problem("--resolve-all takes mine, theirs or copy");
    }
    command += R"(, "resolve_all": )" + json_text(*all);
  }
  if (const std::string* author = a.one("--author")) {
    command += R"(, "author": )" + json_text(*author);
  }
  if (const std::string* name = a.one("--name")) {
    command += R"(, "name": )" + json_text(*name);
  }
  if (a.flag("--no-push")) {
    command += R"(, "push": false)";
  }
  command += "}";
  const rust::Box<mitcad::Project> project = mitcad::open_project(a.positional[1]);
  const rust::Box<mitcad::SyncControl> control = mitcad::new_sync_control();
  const std::string answer(project->command_with(command, *control));
  if (a.flag("--json")) {
    std::cout << answer << "\n";
  } else {
    std::cout << std::string(mitcad::describe_remote("connect", answer));
  }
  if (!failed(answer)) {
    return 0;
  }
  return answer.find("\"class\":\"conflict\"") != std::string::npos ? 3 : 1;
}

int clone_adopt(const std::string& url, const std::string& folder, const std::string* author, bool json) {
  std::string command = R"({"cmd": "clone_project", "adopt": true, "url": )" + json_text(url) + R"(, "dir": )" +
                        json_text(folder);
  if (author != nullptr) {
    command += R"(, "author": )" + json_text(*author);
  }
  return run(command + "}", json);
}

int host_keys(const std::vector<std::string>& args) {
  Parsed a;
  std::string error;
  if (!parse(args, {"--port"}, {"--json", "--trust"}, a, error)) {
    return problem(error);
  }
  if (a.positional.size() != 1) {
    return problem("host-keys needs a server's host name");
  }
  std::string port = "22";
  if (const std::string* given = a.one("--port")) {
    if (!port_of(*given, port)) {
      return problem("--port takes a number from 1 to 65535");
    }
  }
  const std::string fields = R"(, "host": )" + json_text(a.positional[0]) + R"(, "port": )" + port + "}";
  const bool json = a.flag("--json");
  const int shown = run(R"({"cmd": "host_keys")" + fields, json);
  if (shown != 0 || !a.flag("--trust")) {
    return shown;
  }
  return run(R"({"cmd": "trust_host_key")" + fields, json);
}

} // namespace mitcad::cli
