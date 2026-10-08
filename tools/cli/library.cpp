// SPDX-License-Identifier: MIT
// mitcad-cli library (mitcad#64, mitcad#63): component libraries and
// community indexes in git repositories, fetched into the cache
// (MITCAD_LIBRARIES_DIR, or --libraries), shown, searched and compared;
// a library of one's own made, filled and checked. mitcad-cli parts: the
// parts list of a design with the designations of its library parts.
#include "library.hpp"

#include <cstdio>
#include <fstream>
#include <iostream>
#include <sstream>

namespace mitcad::cli {

const char* const kLibraryUsage = R"(  mitcad-cli library fetch <url> [--libraries DIR] [--json]
      Fetches a component library or a community index (a git repository
      with mitcad-library.json or mitcad-index.json at its root) into the
      cache of libraries (--libraries, else MITCAD_LIBRARIES_DIR, else
      the user's data folder): a bare clone, updated by later fetches
      without removing versions. Lists its versions (tags, then the
      unreleased tip).
  mitcad-cli library list [--libraries DIR] [--json]
  mitcad-cli library show <url|id> [--rev R] [--libraries DIR] [--json]
      A fetched library at a version (default the newest): its components
      with their sizes, licence and versions; an index's libraries.
  mitcad-cli library search [<words>] [--license SPDX]... [--unlicensed] [--libraries DIR] [--json]
      Components of the fetched libraries and libraries the fetched
      indexes list, by words, with the given licences; items without a
      licence only with --unlicensed.
  mitcad-cli library diff <id> <from> <to> [--url URL] [--part <component>[:<size>]]... [--json]
      What changes for library parts from one version to another.
  mitcad-cli library init <folder> --id ID --name NAME [--license SPDX] [--author NAME]...
                          [--description TEXT]
      Makes a folder a library of one's own (a manifest, a licence note, a
      Mitcad project with a git repository).
  mitcad-cli library add <folder> <design.mitcad> --id ID [--name N] [--category C] [--standard S]
                         [--keyword K]... [--license SPDX] [--designation PATTERN] [--preview <png>]
      Adds a design to it as a component (its manifest entry, the design as
      a single file, the preview image); record and push the folder as a
      project (version save, remote add).
  mitcad-cli library check <folder> [--json]
      What is wrong with a library folder; exit status 1 on errors.
  mitcad-cli library index-entry <folder> --url URL [--rev R]
      The entry a community index takes for the library (libraries/<id>.json).
  mitcad-cli parts <file.mitcad> [--json]
      The parts list: each component with how many times the design places
      it, library parts with their designation, version and licence.
      Library parts are inserted and updated with the commands
      insert_component (with "library") and update_library_parts of a
      script (run --open ... --save ...).
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

std::string json_list(const std::vector<std::string>& items) {
  std::string out = "[";
  for (const std::string& item : items) {
    out += (out.size() > 1 ? ", " : "") + json_text(item);
  }
  return out + "]";
}

int problem(const std::string& text) {
  std::cerr << "mitcad-cli: " << text << "\nusage:\n" << kLibraryUsage;
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

bool parse(const std::vector<std::string>& args, std::size_t from, const std::vector<std::string>& with_value,
           const std::vector<std::string>& flags, Parsed& out, std::string& error) {
  const auto listed = [](const std::vector<std::string>& list, const std::string& name) {
    for (const std::string& item : list) {
      if (item == name) {
        return true;
      }
    }
    return false;
  };
  for (std::size_t i = from; i < args.size(); ++i) {
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

bool read_text(const std::string& path, std::string& text) {
  std::ifstream in(path, std::ios::binary);
  if (!in) {
    return false;
  }
  std::ostringstream content;
  content << in.rdbuf();
  text = content.str();
  return true;
}

std::string base64(const std::string& data) {
  static const char* const kAlphabet = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
  std::string out;
  std::size_t i = 0;
  for (; i + 2 < data.size(); i += 3) {
    const unsigned n = (static_cast<unsigned char>(data[i]) << 16) | (static_cast<unsigned char>(data[i + 1]) << 8) |
                       static_cast<unsigned char>(data[i + 2]);
    out += kAlphabet[(n >> 18) & 63];
    out += kAlphabet[(n >> 12) & 63];
    out += kAlphabet[(n >> 6) & 63];
    out += kAlphabet[n & 63];
  }
  if (i < data.size()) {
    unsigned n = static_cast<unsigned char>(data[i]) << 16;
    if (i + 1 < data.size()) {
      n |= static_cast<unsigned char>(data[i + 1]) << 8;
    }
    out += kAlphabet[(n >> 18) & 63];
    out += kAlphabet[(n >> 12) & 63];
    out += i + 1 < data.size() ? kAlphabet[(n >> 6) & 63] : '=';
    out += '=';
  }
  return out;
}

// Runs a library command; prints JSON (--json) or text. Failures of a
// fetch and checks with errors are exit status 1.
int run(const std::string& command, bool json) {
  const rust::Box<mitcad::SyncControl> control = mitcad::new_sync_control();
  const std::string answer(mitcad::library_command(command, *control));
  if (json) {
    std::cout << answer << "\n";
    const bool failed = answer.find("\"error\":{") != std::string::npos ||
                        answer.find("\"ok\":false") != std::string::npos;
    return failed ? 1 : 0;
  }
  try {
    std::cout << std::string(mitcad::describe_library(command, answer));
  } catch (const rust::Error& error) {
    std::cerr << "mitcad-cli: " << error.what() << "\n";
    return 1;
  }
  return 0;
}

} // namespace

int library(const std::vector<std::string>& args) {
  Parsed a;
  std::string error;
  if (!parse(args, 1,
             {"--libraries", "--rev", "--url", "--part", "--license", "--id", "--name", "--author", "--description",
              "--category", "--standard", "--keyword", "--designation", "--preview"},
             {"--json", "--unlicensed"}, a, error)) {
    return problem(error);
  }
  if (a.positional.empty()) {
    return problem("library needs fetch, list, show, search, diff, init, add, check or index-entry");
  }
  std::string root;
  if (const std::string* libraries = a.one("--libraries")) {
    root = *libraries;
  }
  mitcad::configure_libraries(R"({"root": )" + json_text(root) + "}");
  const std::string what = a.positional[0];
  const bool json = a.flag("--json");
  const std::size_t count = a.positional.size();
  const auto optional = [&a](const char* option, const char* field) {
    const std::string* value = a.one(option);
    return value == nullptr ? std::string() : std::string(", \"") + field + "\": " + json_text(*value);
  };
  if (what == "fetch" && count == 2) {
    return run(R"({"cmd": "library_fetch", "url": )" + json_text(a.positional[1]) + "}", json);
  }
  if (what == "list" && count == 1) {
    return run(R"({"cmd": "library_list"})", json);
  }
  if (what == "show" && count == 2) {
    const std::string& which = a.positional[1];
    const bool url = which.find('/') != std::string::npos || which.find('\\') != std::string::npos ||
                     which.find(':') != std::string::npos;
    return run(R"({"cmd": "library_show", ")" + std::string(url ? "url" : "id") + "\": " + json_text(which) +
                   optional("--rev", "rev") + "}",
               json);
  }
  if (what == "search" && count <= 2) {
    std::string command = R"({"cmd": "library_search", "text": )" + json_text(count == 2 ? a.positional[1] : "");
    const std::vector<std::string> licenses = a.all("--license");
    if (!licenses.empty()) {
      command += R"(, "licenses": )" + json_list(licenses);
    }
    if (a.flag("--unlicensed")) {
      command += R"(, "unlicensed": true)";
    }
    return run(command + "}", json);
  }
  if (what == "diff" && count == 4) {
    std::string parts;
    for (const std::string& part : a.all("--part")) {
      const std::size_t colon = part.find(':');
      parts += std::string(parts.empty() ? "" : ", ") + R"({"component": )" + json_text(part.substr(0, colon)) +
               R"(, "config": )" +
               (colon == std::string::npos ? std::string("null") : json_text(part.substr(colon + 1))) + "}";
    }
    return run(R"({"cmd": "library_diff", "id": )" + json_text(a.positional[1]) + R"(, "from": )" +
                   json_text(a.positional[2]) + R"(, "to": )" + json_text(a.positional[3]) + optional("--url", "url") +
                   R"(, "parts": [)" + parts + "]}",
               json);
  }
  if (what == "check" && count == 2) {
    return run(R"({"cmd": "library_check", "dir": )" + json_text(a.positional[1]) + "}", json);
  }
  if (what == "init" && count == 2) {
    if (a.one("--id") == nullptr || a.one("--name") == nullptr) {
      return problem("library init needs --id and --name");
    }
    const std::string command = R"({"cmd": "library_init", "dir": )" + json_text(a.positional[1]) +
                                optional("--id", "id") + optional("--name", "name") +
                                optional("--license", "license") + optional("--description", "description") +
                                R"(, "authors": )" + json_list(a.all("--author")) + "}";
    const rust::Box<mitcad::SyncControl> control = mitcad::new_sync_control();
    const std::string answer(mitcad::library_command(command, *control));
    std::cout << answer << "\n";
    return 0;
  }
  if (what == "add" && count == 3) {
    if (a.one("--id") == nullptr) {
      return problem("library add needs --id");
    }
    // The design as one file: B-rep data of a project's store inside it.
    const rust::Box<mitcad::Document> document = mitcad::load_project(a.positional[2]);
    std::string command = R"({"cmd": "library_add", "dir": )" + json_text(a.positional[1]) +
                          R"(, "text": )" + json_text(std::string(document->to_json())) + optional("--id", "id") +
                          optional("--name", "name") + optional("--category", "category") +
                          optional("--standard", "standard") + optional("--license", "license") +
                          optional("--designation", "designation") + R"(, "keywords": )" +
                          json_list(a.all("--keyword"));
    if (const std::string* preview = a.one("--preview")) {
      std::string png;
      if (!read_text(*preview, png)) {
        std::cerr << "mitcad-cli: cannot read " << *preview << "\n";
        return 1;
      }
      command += R"(, "preview": )" + json_text(base64(png));
    }
    const rust::Box<mitcad::SyncControl> control = mitcad::new_sync_control();
    const std::string answer(mitcad::library_command(command + "}", *control));
    std::cout << answer << "\n";
    return 0;
  }
  if (what == "index-entry" && count == 2) {
    if (a.one("--url") == nullptr) {
      return problem("library index-entry needs --url (where the library is published)");
    }
    return run(R"({"cmd": "index_entry", "dir": )" + json_text(a.positional[1]) + optional("--url", "url") +
                   optional("--rev", "rev") + "}",
               json);
  }
  return problem("library " + what + ": wrong arguments");
}

int parts(const std::vector<std::string>& args, const OpenDocument& open) {
  Parsed a;
  std::string error;
  if (!parse(args, 1, {}, {"--json"}, a, error)) {
    return problem(error);
  }
  if (a.positional.size() != 1) {
    return problem("parts needs a project file");
  }
  const rust::Box<mitcad::Document> document = open(a.positional[0]);
  if (a.flag("--json")) {
    std::cout << std::string(document->query(R"({"query": "parts_list"})")) << "\n";
    return 0;
  }
  // The text: one line per part, from the query's JSON.
  const std::string answer(document->query(R"({"query": "parts_list"})"));
  std::cout << std::string(mitcad::describe_parts(answer));
  return 0;
}

} // namespace mitcad::cli
