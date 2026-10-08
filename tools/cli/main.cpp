// SPDX-License-Identifier: MIT
// mitcad-cli: opens or builds a part without the user interface and prints
// its timeline and bodies. The kernel and import tests run through it.

#include <algorithm>
#include <cstdio>
#include <cstdlib>
#include <exception>
#include <filesystem>
#include <fstream>
#include <iostream>
#include <sstream>
#include <stdexcept>
#include <string>
#include <utility>
#include <vector>

#include "mitcad/geometry/guard.hpp"
#include "mitcad/geometry/persist.hpp"
#include "mitcad/io/body.hpp"
#include "mitcad_bridge/lib.h"
#include "rust/cxx.h"
#ifdef MITCAD_RENDER
#include "render.hpp"
#endif
// Component libraries (mitcad#64, mitcad#63).
#include "library.hpp"

namespace {

const char* const kUsage = R"(usage:
  mitcad-cli info <file.mitcad> [--json] [--properties [--density G_PER_CM3]] [--timings]
                  [--result-store DIR [--persist-min-ms MS]] [--diagnostics] [--names]
      Opens a project file, recomputes it and prints the timeline with the
      status of each feature and the bodies with volume, area, centre of
      mass, bounding box and face and edge counts. --timings adds how long
      the recompute took and its slowest features. --result-store takes
      computed results from the store in DIR and writes there those whose
      evaluation took at least MS milliseconds (default 100), and prints
      "restored from store: N, evaluated: M" (also for run --open).
      --diagnostics adds the caches' diagnostics (the cache query, JSON):
      memory and store sizes, hits, and where each result came from.
      --names adds the names of every body's faces and edges (the faces and
      edges queries, JSON), to compare the topology of two builds.
  mitcad-cli run <script.json> [--open <file.mitcad>] [--save <out.mitcad>] [--json]
                 [--properties [--density G_PER_CM3]]
      Runs a script of JSON commands and expectations on a new document, or
      on an opened one, then prints the same report. A script is a JSON
      array of commands ({"cmd": ...}), expectations ({"expect": {...}}) and
      comments ({"comment": "..."}); see core/model/src/api/commands.md.
      --save writes the resulting project file.
      --properties adds the physical properties of each body: mass, centre
      of mass and inertia, with its material (steel by default), or with
      --density (g/cm3) for bodies without a material.
  mitcad-cli measure <part> <a> [<b>] [--json]
      Measures a body, a face, edge or vertex of it, or a datum: volume,
      area, length, radius or coordinates; with two, also the distance and
      the angle between them. A selection is a body (uid like F2.b0 or name
      like Body1), a body and a topological name (Body1/F2:end(r{c5})), or
      a datum (xy, F5).
  mitcad-cli compare-step <part> <file.step> [--body <body>]... [--step-body <name|index>]
                          [--samples N] [--max-deviation MM] [--max-relative X] [--json]
      Compares the bodies (all, or those given) with the bodies of a STEP
      file (all, or one): volumes, volume differences, surface deviation
      and bounding boxes. Exits with 1 when a given limit is exceeded.
      <part> is a project file (.mitcad) or a script (.json) that builds one.
  mitcad-cli import <file> [--unit-mm N] [--open <file.mitcad>] [--save <out.mitcad>] [--json]
      Imports a STEP, IGES, BRep, STL or OBJ file as a base feature (bodies
      without history; STL and OBJ give mesh bodies) into a new or opened
      project and prints the report. --unit-mm is the length of one STL or
      OBJ unit in millimetres (default 1).
  mitcad-cli import-f3d <file.f3d> [--save <out.mitcad>] [--report <report.json>] [--json]
                        [--design <name>] [--dump <dump.json>] [--no-verify] [--no-fallback]
                        [--no-compare] [--time-limit <seconds>] [--hang-limit <seconds>]
      Imports an .f3d or .f3z design with its timeline: parameters,
      sketches and features replayed as Mitcad features, checked against
      the file's ASM history; an item that cannot be replayed becomes a
      base feature with the file's bodies after it. Prints the import report
      (per item: parametric, partial, fallback or skipped, and the final
      bodies against the file's) and the document report. --report writes
      the import report as JSON; --design picks a document of an .f3z;
      --dump replays an external dump (JSON) with the file's bodies;
      after --time-limit seconds the remaining items take the file's bodies;
      with --hang-limit the import runs again when the geometry kernel does
      not return for that long, the item it hung on taking the file's bodies.
  mitcad-cli import-f3d <file.f3d> --bodies-only [--history] [--owners] [--save <out.mitcad>] [--json]
      Imports the bodies of an .f3d or .f3z file without their
      history: one base feature per body, built from the file's B-rep data.
      Prints each body (solid or sheet, validity, volume) and the report.
      --history also takes the bodies of the .smbh blobs (with ASM history,
      often copies), --owners also bodies of which only faces are saved.
  mitcad-cli import-fcstd <file.FCStd> [--save <out.mitcad>] [--report <report.json>] [--bodies-only]
                          [--reference <dump.json>] [--set <name>=<expression>]... [--json]
      Imports a FreeCAD document: the bodies FreeCAD stored (Bodies and
      the Part workbench's results) as base features in the document's
      structure: App::Part and Assembly as components, App::Link (also
      arrays and links to other files, read next to it) as occurrences,
      with visibility and colours; then (not with --bodies-only) the
      spreadsheets, VarSets and named constraints as parameters, and the
      sketches and the history's features, editable, with the expressions
      FreeCAD binds to their values. Prints the import report (per object:
      body, component, occurrence, parametric, partial or fallback,
      included or skipped, with the reason; the parameters and
      expressions) and the document report; --report writes the import
      report as JSON. --reference compares the import with a dump of the
      document made by tools/freecad-export/dump.py (volumes, areas and
      world centres of mass; the sketches' lengths and centres; the
      parameters' values); exits with 1 when they differ. --set changes a
      parameter after the import, before the report measures it (to
      compare with FreeCAD's document after the same change).
  mitcad-cli import-ipt <file.ipt> [--save <out.mitcad>] [--report <report.json>]
                        [--reference <file.step> [--max-relative X] [--deviation]] [--json]
                        [--bodies-only] [--no-verify] [--no-fallback] [--no-compare]
                        [--time-limit S] [--dump <design.json>] [--design <design.json>]
      Imports an .ipt part file: its parameters with their expressions and
      its features, each checked against the ASM history stored with the
      bodies, those that are not translated as the bodies of their history
      state (as import-f3d does); --bodies-only imports the bodies stored in
      the file as base features, one per body. The part's length unit and,
      when Mitcad's library has it, its material apply. Prints the import
      report (the features with their outcomes; each body: solid or sheet,
      validity, volume, area; the part number, material and units; the
      parameters and expressions) and the document report; --report writes
      the import report as JSON, --dump the decoded design (the dump IR);
      --design replays such a dump instead of the decoded design.
      --reference compares the solids with the solids of a STEP file of the
      same part: volume and area of each, relatively, within --max-relative
      (default 1e-6), and every imported solid valid; --deviation also
      measures the surface deviation. Exits with 1 when they differ.
  mitcad-cli export <file.mitcad> <out> [--bodies <b>,...] [--schema ap214|ap242]
                    [--unit mm|cm|m|in|ft] [--refinement low|medium|high]
                    [--deviation MM --angle DEG] [--ascii]
                    [--coordinates design|component] [--occurrence <path>]
      Writes bodies at the timeline marker (all, or the listed uids or
      names) to a .step/.stp, .iges/.igs, .stl, .obj, .brep or .3mf file.
      STEP and IGES keep body names and imported colours; --refinement or
      --deviation and --angle set the STL, OBJ and 3MF triangulation. A
      3MF file (for slicers) holds the solid and mesh bodies as the parts
      of one object, in their places, with names and imported colours.
      Every format has the bodies where the design shows them (a STEP
      file of moved components is an assembly); --coordinates component
      writes each body once in its component's coordinates, --occurrence
      (Arm:1/Pin:2) only what that occurrence places.
  mitcad-cli export-sketch <file.mitcad> <sketch> <out.dxf> [--r12]
      Writes the geometry of a sketch (uid or name) to a DXF file (R2000,
      or R12 with --r12).
  mitcad-cli project init <folder> [--author "Name <email>"] [--no-history]
      Makes a folder a Mitcad project: writes .mitcad/project.json and the
      recommended .gitattributes and .gitignore lines, makes the folder a
      git repository (unless it is the root of one) and records these files
      as the first version, by --author or by git's configured author
      (user.name, user.email). --no-history writes the files only. Project
      files in a project (in the folder or below it) are saved in version
      3: the B-rep data of base features goes into the project's store
      (.mitcad/brep), each body's data once, and the file refers to it.
  mitcad-cli history <file.mitcad> [--json]
      Lists the versions of a project file of a project with version
      history, newest first: number, id, time, author and message (the
      commits that changed it, following renames).
  mitcad-cli version save <file.mitcad>... [-m <message>] [--author "Name <email>"] [--json]
      Records the saved files as a new version: a commit of these files and
      of the B-rep files they refer to, without the B-rep files no project
      file of the version refers to (they leave the folder too). The author
      is --author, else git's configured one. Nothing changed: no version.
  mitcad-cli version show <file.mitcad> <version> [--save <out.mitcad>] [--json] [--properties]
      Opens a project file as it was in a version (an id or a unique prefix
      of one, HEAD, HEAD~n), with its B-rep data read from that version,
      recomputes it and prints the report as info does; --save writes it.
  mitcad-cli version changes <file-or-folder> <version> [<version>] [--json]
      Lists the project's files that differ between two versions (the
      second defaults to the latest): A added, D deleted, M modified, R
      renamed.
  mitcad-cli version restore <file.mitcad> <version> [-m <message>] [--author "Name <email>"] [--json]
      Writes a project file as it was in a version and records it as a new
      version.
  mitcad-cli diff <a.mitcad> <b.mitcad> [--json] [--geometry]
  mitcad-cli diff <file.mitcad> <version> [<version>] [--json] [--geometry]
      Compares two project files, or two versions of a project file (the
      second defaults to the file as saved in the folder), in the model's
      terms: parameters, features with their changed fields and values
      ("Extrude1 (F2): distance 10 mm -> 15 mm"), features added, deleted
      and moved, sketches' entities, constraints and dimensions,
      components, occurrences, bodies, timeline groups and named views.
      Nothing is computed unless --geometry, which recomputes both (without
      following linked components) and adds the bodies' volumes and areas.
  mitcad-cli remote add <folder> <url> [--name <name>] [--author "Name <email>"] [--json]
      Connects a project with version history to a remote repository
      (default name origin): an empty one gets the project's versions, one
      that shares the project's history is fetched (and gets the versions it
      lacks when it has none of its own), another history is refused without
      changes. The project's .gitattributes gets merge=binary for project
      files, recorded as a version. <url>: https://host/owner/repo.git,
      git@host:owner/repo.git, ssh://host/path or a folder, never with a
      password or token: git signs in with SSH keys (and the SSH agent) or a
      credential helper (Git Credential Manager).
  mitcad-cli remote check <folder> <url> [--json]
      What a remote holds: reachable, empty, a Mitcad project, a history in
      common with the project.
  mitcad-cli remote show <folder> [--json]
      The git program, the project's remote and branch, the versions to push
      and those newer on the remote (as the last fetch or push left them),
      the last fetch and push, and linked files outside the project.
  mitcad-cli remote remove <folder> [--name <name>] [--json]
  mitcad-cli fetch <folder> [--json]
      Fetches the remote's versions; the project's files stay as they are.
  mitcad-cli push <folder> [--json]
      Pushes the versions the remote lacks, never forced: a remote with
      versions the project lacks refuses (exit status 1). A file over
      100 MiB holds the push back (git hosts refuse it).
  mitcad-cli sync <folder> [--resolve <path>=mine|theirs|copy]... [--resolve-all mine|theirs|copy]
                  [--author "Name <email>"] [--no-push] [--dry-run] [--json]
      Fetches, then brings the project and its remote together: pushes the
      project's new versions, takes the remote's, or, when both have new
      ones, replays the project's after the remote's file by file (one line
      of history; the old one is kept in refs/mitcad/sync-backup/<time>),
      then pushes (not with --no-push). A file changed on both sides needs
      a choice for the whole file: mine (keep mine), theirs (take theirs)
      or copy (save mine next to it, as <name> (conflict copy <author>
      <date>) with its extension); without one the sync lists the files
      with their versions, changes nothing and exits with 3. Changes no
      version holds in a file the sync would change stop it. The replayed
      versions keep their authors and are recorded by --author, else git's
      configured user. --dry-run tells what a sync would do.
  mitcad-cli clone <url> <folder> [--json]
      Opens a project from a remote into a new (or empty) folder and lists
      its project files; a repository without a project is refused.
      Remote repositories need the git program (MITCAD_GIT, else PATH);
      with --json the answer's "error" is null on success.
  mitcad-cli render <file.mitcad|script.json> -o <image> [--view <name>] [--size WxH] [--samples N]
                    [--time-limit SECONDS] [--format png|png16|jpeg|exr] [--quality 1-100]
                    [--transparent] [--no-denoise] [--worker <mitcad-render>]
      Renders the visible bodies to an image file with the design's render
      settings (environment, background, ground, film and output), as File >
      Render Image does, in the render worker mitcad-render (next to
      mitcad-cli, or --worker, or MITCAD_RENDER_WORKER); only in builds with
      the renderer. --view is a named view of the design, or a standard view
      (front, back, left, right, top, bottom, iso) fitted to the bodies; the
      default is the design's Home view, else iso. --size, --samples,
      --time-limit (stop after that long), --format (else from the file's
      extension: .png, .jpg, .exr), --quality (JPEG), --transparent and
      --no-denoise change the settings' output section for this render. PNG
      and JPEG are as the view shows the render (exposure, view transform);
      EXR is the render's linear light without them.
  mitcad-cli convert <file.mitcad> <out.mitcad> [--format v2|v3|auto]
      Writes a project file again without computing it: v3 refers to B-rep
      data in the project's store (the output must be in a project), v2 is
      a single file with the data inside, auto (default) is v3 in a
      project and v2 elsewhere. --save of the other commands writes as
      auto does.
)";

// After the commands of other files (library.cpp).
const char* const kExitStatus = R"(Exit status: 0 on success, 1 on errors (also of the network, signing in and
the remote), failed expectations and exceeded limits, 2 on wrong arguments,
3 when a sync needs a choice for files changed on both sides.
)";

bool read_file(const std::string& path, std::string& text) {
  std::ifstream in(path, std::ios::binary);
  if (!in) {
    return false;
  }
  std::ostringstream content;
  content << in.rdbuf();
  text = content.str();
  return true;
}

// Writes the project file whole or not at all: version 3 in a project
// (P12a; the B-rep data in the project's store), else one file. Throws
// rust::Error when it cannot be written.
void save_document(const mitcad::Document& document, const std::string& path) {
  document.save_project(path, "auto");
}

int fail(const std::string& message) {
  std::cerr << "mitcad-cli: " << message << "\n";
  return 1;
}

int usage(const std::string& problem) {
  std::cerr << "mitcad-cli: " << problem << "\n" << kUsage << mitcad::cli::kLibraryUsage << kExitStatus;
  return 2;
}

// A JSON string literal.
std::string json_string(const std::string& text) {
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
        std::snprintf(escaped, sizeof escaped, "\\u%04x", c);
        out += escaped;
      } else {
        out += c;
      }
    }
  }
  return out + "\"";
}

// A JSON array of strings from a comma-separated list.
std::string string_list(const std::string& text) {
  std::string out = "[";
  std::size_t start = 0;
  while (start <= text.size()) {
    const std::size_t comma = text.find(',', start);
    const std::size_t end = comma == std::string::npos ? text.size() : comma;
    if (out.size() > 1) {
      out += ", ";
    }
    out += json_string(text.substr(start, end - start));
    start = end + 1;
  }
  return out + "]";
}

bool is_number(const std::string& text) {
  if (text.empty()) {
    return false;
  }
  std::istringstream in(text);
  double value = 0.0;
  in >> value;
  return !in.fail() && in.eof();
}

// Command line: positional arguments, `--name value` options and flags.
struct Arguments {
  std::vector<std::string> positional;
  std::vector<std::pair<std::string, std::string>> options;

  const std::string* option(const std::string& name) const {
    for (const auto& [key, value] : options) {
      if (key == name) {
        return &value;
      }
    }
    return nullptr;
  }
  bool flag(const std::string& name) const { return option(name) != nullptr; }
};

// Splits the arguments after the mode; options not in `with_value` or
// `flags` are an error.
bool parse_arguments(const std::vector<std::string>& args, const std::vector<std::string>& with_value,
                     const std::vector<std::string>& flags, Arguments& out, std::string& problem) {
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
        problem = "unexpected argument '" + arg + "'";
        return false;
      }
    } else {
      out.positional.push_back(arg);
    }
  }
  return true;
}

// The number after "key": in compact JSON, or -1.
long long json_count(const std::string& json, const std::string& key) {
  const std::string field = "\"" + key + "\":";
  const std::size_t at = json.find(field);
  if (at == std::string::npos) {
    return -1;
  }
  return std::strtoll(json.c_str() + at + field.size(), nullptr, 10);
}

// The result store (P7d): with a folder, opening takes results from it
// and writes those that took at least min_ms to evaluate.
struct StoreOptions {
  std::string dir;
  double min_ms = 100.0;
};

rust::Box<mitcad::Document> open_document(const std::string& path, const StoreOptions& store = {},
                                          std::ostream* log = nullptr) {
  // A version 3 file's B-rep data comes from its project's store (P12a).
  rust::Box<mitcad::Document> document = mitcad::load_project(path);
  if (!store.dir.empty()) {
    document->set_result_store(store.dir, mitcad::geometry::kernel_build_id(),
                               std::filesystem::path(path).filename().string());
  }
  // Linked components follow their files, relative to the project's.
  std::string base = std::filesystem::path(path).parent_path().string();
  if (base.empty()) {
    base = ".";
  }
  document->command(R"({"cmd": "update_links", "base": )" + json_string(base) + "}");
  const std::string recomputed(document->command(R"({"cmd": "recompute"})"));
  if (!store.dir.empty()) {
    const std::string persisted(document->persist_results(store.min_ms));
    if (log != nullptr) {
      *log << "Result store: restored from store: " << std::max(0LL, json_count(recomputed, "restored"))
           << ", evaluated: " << json_count(recomputed, "recomputed")
           << "; stored: " << json_count(persisted, "results") << " ("
           << json_count(persisted, "bytes") << " bytes)\n";
    }
  }
  return document;
}

// A project file, or a script (.json) run on a new document.
rust::Box<mitcad::Document> open_part(const std::string& path) {
  const std::string suffix = ".json";
  if (path.size() < suffix.size() || path.compare(path.size() - suffix.size(), suffix.size(), suffix) != 0) {
    return open_document(path);
  }
  std::string script;
  if (!read_file(path, script)) {
    throw std::runtime_error("cannot read " + path);
  }
  rust::Box<mitcad::Document> document = mitcad::new_document();
  document->run_script(script);
  return document;
}

// The report, the physical properties with --properties and the times of
// the last recompute with --timings.
// The uids of the bodies, from the `bodies` query.
std::vector<std::string> body_uids(const mitcad::Document& document) {
  const std::string bodies(document.query(R"({"query": "bodies"})"));
  std::vector<std::string> uids;
  const std::string key = R"("uid":)";
  for (std::size_t at = bodies.find(key); at != std::string::npos; at = bodies.find(key, at + 1)) {
    const std::size_t open = bodies.find('"', at + key.size());
    const std::size_t close = open == std::string::npos ? open : bodies.find('"', open + 1);
    if (close != std::string::npos) {
      uids.push_back(bodies.substr(open + 1, close - open - 1));
    }
  }
  return uids;
}

// With --names: the names of every body's faces and edges (the `faces` and
// `edges` queries), to compare the topology two builds give a model.
void print_names(const mitcad::Document& document) {
  for (const std::string& uid : body_uids(document)) {
    const std::string body = R"(, "body": )" + json_string(uid) + "}";
    std::cout << "Faces of " << uid << ": " << std::string(document.query(R"({"query": "faces")" + body))
              << "\n";
    std::cout << "Edges of " << uid << ": " << std::string(document.query(R"({"query": "edges")" + body))
              << "\n";
  }
}

void print_report(const mitcad::Document& document, bool json, bool properties,
                  const std::string& density, bool timings, bool diagnostics = false, bool names = false) {
  const std::string report(document.report(json));
  if (names && !json) {
    std::cout << report;
    print_names(document);
    return;
  }
  if (!properties && !timings && !diagnostics) {
    std::cout << report;
    return;
  }
  // The caches (P7d): the `cache` query with the store's files.
  const std::string cache =
      diagnostics ? std::string(document.query(R"({"query": "cache", "disk": true})")) : std::string();
  std::string physical;
  if (properties) {
    std::string query = R"({"query": "properties")";
    if (!density.empty()) {
      query += R"(, "density": )" + density;
    }
    query += "}";
    physical = std::string(document.analysis(query, json));
  }
  std::string times;
  if (timings) {
    times = std::string(document.analysis(R"({"query": "recompute_times"})", json));
  }
  if (json) {
    std::cout << R"({"report": )" << report;
    if (properties) {
      std::cout << R"(, "properties": )" << physical;
    }
    if (timings) {
      std::cout << R"(, "recompute_times": )" << times;
    }
    if (diagnostics) {
      std::cout << R"(, "cache": )" << cache;
    }
    std::cout << "}\n";
  } else {
    std::cout << report;
    if (properties) {
      std::cout << "Physical properties:\n" << physical;
    }
    std::cout << times;
    if (diagnostics) {
      std::cout << "Cache: " << cache << "\n";
    }
  }
}

int info_or_run(const std::string& mode, const std::vector<std::string>& args) {
  std::string input;
  std::string open;
  std::string save;
  std::string density;
  bool json = false;
  bool properties = false;
  bool timings = false;
  StoreOptions store;
  std::string min_ms;
  bool diagnostics = false;
  bool names = false;
  for (std::size_t i = 1; i < args.size(); ++i) {
    if (args[i] == "--json") {
      json = true;
    } else if (args[i] == "--names" && mode == "info") {
      names = true;
    } else if (args[i] == "--diagnostics") {
      diagnostics = true;
    } else if (args[i] == "--properties") {
      properties = true;
    } else if (args[i] == "--timings" && mode == "info") {
      timings = true;
    } else if (args[i] == "--result-store" && i + 1 < args.size()) {
      store.dir = args[++i];
    } else if (args[i] == "--persist-min-ms" && i + 1 < args.size()) {
      min_ms = args[++i];
    } else if (args[i] == "--density" && i + 1 < args.size()) {
      density = args[++i];
    } else if (args[i] == "--open" && i + 1 < args.size() && mode == "run") {
      open = args[++i];
    } else if (args[i] == "--save" && i + 1 < args.size() && mode == "run") {
      save = args[++i];
    } else if (input.empty() && !args[i].empty() && args[i][0] != '-') {
      input = args[i];
    } else {
      return usage("unexpected argument '" + args[i] + "'");
    }
  }
  if (input.empty()) {
    return usage(mode == "info" ? "no project file" : "no script");
  }
  if (!density.empty() && (!properties || !is_number(density))) {
    return usage("--density needs --properties and a number");
  }
  const std::string project = mode == "info" ? input : open;
  if (!min_ms.empty()) {
    if (store.dir.empty() || !is_number(min_ms)) {
      return usage("--persist-min-ms needs --result-store and a number");
    }
    store.min_ms = std::stod(min_ms);
  }
  if (!store.dir.empty() && project.empty()) {
    return usage("--result-store needs a project file to open");
  }
  rust::Box<mitcad::Document> document = mitcad::new_document();
  if (!project.empty()) {
    // The store's line goes before the report, or to stderr with --json.
    document = open_document(project, store, json ? &std::cerr : &std::cout);
  }
  if (mode == "run") {
    std::string script;
    if (!read_file(input, script)) {
      return fail("cannot read " + input);
    }
    document->run_script(script);
  }
  if (!save.empty()) {
    save_document(*document, save);
  }
  print_report(*document, json, properties, density, timings, diagnostics, names);
  return 0;
}

int measure(const std::vector<std::string>& args) {
  Arguments a;
  std::string problem;
  if (!parse_arguments(args, {}, {"--json"}, a, problem)) {
    return usage(problem);
  }
  if (a.positional.size() < 2 || a.positional.size() > 3) {
    return usage("measure needs a part and one or two selections");
  }
  std::string query = R"({"query": "measure", "a": )" + json_string(a.positional[1]);
  if (a.positional.size() == 3) {
    query += R"(, "b": )" + json_string(a.positional[2]);
  }
  query += "}";
  rust::Box<mitcad::Document> document = open_part(a.positional[0]);
  std::cout << std::string(document->analysis(query, a.flag("--json")));
  return 0;
}

int compare_step(const std::vector<std::string>& args) {
  Arguments a;
  std::string problem;
  if (!parse_arguments(args, {"--body", "--step-body", "--samples", "--max-deviation", "--max-relative"},
                       {"--json"}, a, problem)) {
    return usage(problem);
  }
  if (a.positional.size() != 2) {
    return usage("compare-step needs a part and a STEP file");
  }
  std::string query = R"({"query": "compare_step", "file": )" + json_string(a.positional[1]);
  std::string bodies;
  for (const auto& [key, value] : a.options) {
    if (key == "--body") {
      bodies += (bodies.empty() ? "" : ", ") + json_string(value);
    }
  }
  if (!bodies.empty()) {
    query += R"(, "bodies": [)" + bodies + "]";
  }
  if (const std::string* step_body = a.option("--step-body")) {
    query += R"(, "step_body": )" + json_string(*step_body);
  }
  const std::pair<const char*, const char*> numbers[] = {
      {"--samples", "samples"}, {"--max-deviation", "max_deviation"}, {"--max-relative", "max_relative"}};
  for (const auto& [option, field] : numbers) {
    if (const std::string* value = a.option(option)) {
      if (!is_number(*value)) {
        return usage(std::string(option) + " needs a number");
      }
      query += std::string(R"(, ")") + field + R"(": )" + *value;
    }
  }
  query += "}";
  rust::Box<mitcad::Document> document = open_part(a.positional[0]);
  const std::string result(document->analysis(query, a.flag("--json")));
  std::cout << result;
  const bool exceeded = result.find("\"pass\": false") != std::string::npos ||
                        result.find("Limits exceeded") != std::string::npos;
  return exceeded ? 1 : 0;
}

int import(const std::vector<std::string>& args) {
  Arguments a;
  std::string problem;
  if (!parse_arguments(args, {"--unit-mm", "--open", "--save"}, {"--json"}, a, problem)) {
    return usage(problem);
  }
  if (a.positional.size() != 1) {
    return usage(a.positional.empty() ? "no file to import" : "more than one file to import");
  }
  std::string command = R"({"cmd": "import_file", "path": )" + json_string(a.positional[0]);
  if (const std::string* unit = a.option("--unit-mm")) {
    if (!is_number(*unit)) {
      return usage("--unit-mm needs a number");
    }
    command += R"(, "unit_mm": )" + *unit;
  }
  command += "}";
  rust::Box<mitcad::Document> document =
      a.flag("--open") ? open_document(*a.option("--open")) : mitcad::new_document();
  document->command(command);
  if (const std::string* save = a.option("--save")) {
    save_document(*document, *save);
  }
  std::cout << std::string(document->report(a.flag("--json")));
  return 0;
}

// The timeline import (T1): the design replayed as Mitcad features.
int import_f3d_timeline(const Arguments& a) {
  std::string options = "{";
  const auto add = [&options](const std::string& field) {
    options += (options.size() > 1 ? ", " : "") + field;
  };
  if (const std::string* design = a.option("--design")) {
    add(R"("design": )" + json_string(*design));
  }
  if (const std::string* dump = a.option("--dump")) {
    add(R"("dump": )" + json_string(*dump));
  }
  if (a.flag("--no-verify")) {
    add(R"("no_verify": true)");
  }
  if (a.flag("--no-fallback")) {
    add(R"("no_fallback": true)");
  }
  if (a.flag("--no-compare")) {
    add(R"("no_compare": true)");
  }
  if (const std::string* seconds = a.option("--time-limit")) {
    add(R"("time_limit": )" + std::to_string(std::atof(seconds->c_str())));
  }
  if (const std::string* seconds = a.option("--hang-limit")) {
    add(R"("hang_limit": )" + std::to_string(std::atof(seconds->c_str())));
  }
  if (const std::string* path = a.option("--report")) {
    add(R"("report_path": )" + json_string(*path));
  }
  const bool json = a.flag("--json");
  if (!json) {
    add(R"("text": true)");
  }
  options += "}";
  rust::Box<mitcad::Document> document = mitcad::new_document();
  const std::string report(document->import_f3d_timeline(a.positional[0], options));
  if (const std::string* save = a.option("--save")) {
    save_document(*document, *save);
  }
  if (json) {
    std::cout << R"({"import": )" << report << R"(, "document": )"
              << std::string(document->query(R"({"query": "report"})")) << "}\n";
  } else {
    std::cout << report << std::string(document->report(false));
  }
  return 0;
}

int import_f3d(const std::vector<std::string>& args) {
  Arguments a;
  std::string problem;
  if (!parse_arguments(args, {"--save", "--report", "--dump", "--design", "--time-limit", "--hang-limit"},
                       {"--bodies-only", "--history", "--owners", "--json", "--no-verify", "--no-fallback",
                        "--no-compare"},
                       a, problem)) {
    return usage(problem);
  }
  if (a.positional.size() != 1) {
    return usage(a.positional.empty() ? "no .f3d file" : "more than one .f3d file");
  }
  if (!a.flag("--bodies-only")) {
    if (a.flag("--history") || a.flag("--owners")) {
      return usage("--history and --owners go with --bodies-only");
    }
    return import_f3d_timeline(a);
  }
  const bool json = a.flag("--json");
  const auto yes_no = [&a](const char* flag) { return a.flag(flag) ? "true" : "false"; };
  const std::string options = std::string(R"({"history": )") + yes_no("--history") + R"(, "owners": )" +
                              yes_no("--owners") + R"(, "text": )" + (json ? "false" : "true") + "}";
  rust::Box<mitcad::Document> document = mitcad::new_document();
  const std::string imported(document->import_f3d(a.positional[0], options));
  if (const std::string* save = a.option("--save")) {
    save_document(*document, *save);
  }
  if (json) {
    std::cout << R"({"import": )" << imported << R"(, "document": )"
              << std::string(document->query(R"({"query": "report"})")) << "}\n";
  } else {
    std::cout << imported << std::string(document->report(false));
  }
  return 0;
}

// A FreeCAD document's stored bodies in its structure.
int import_fcstd(const std::vector<std::string>& args) {
  Arguments a;
  std::string problem;
  if (!parse_arguments(args, {"--save", "--report", "--reference", "--set"}, {"--bodies-only", "--json"}, a,
                       problem)) {
    return usage(problem);
  }
  if (a.positional.size() != 1) {
    return usage(a.positional.empty() ? "no .FCStd file" : "more than one .FCStd file");
  }
  const bool json = a.flag("--json");
  std::string options = std::string(R"({"bodies_only": )") + (a.flag("--bodies-only") ? "true" : "false") +
                        R"(, "text": )" + (json ? "false" : "true");
  if (const std::string* path = a.option("--report")) {
    options += R"(, "report_path": )" + json_string(*path);
  }
  if (const std::string* path = a.option("--reference")) {
    options += R"(, "reference": )" + json_string(*path);
  }
  // Parameters changed after the import: --set name=expression, repeated.
  std::string changes;
  for (const auto& [key, value] : a.options) {
    if (key != "--set") {
      continue;
    }
    const std::size_t equals = value.find('=');
    if (equals == std::string::npos || equals == 0) {
      return usage("--set needs name=expression");
    }
    changes += (changes.empty() ? "" : ", ") + json_string(value.substr(0, equals)) + ": " +
               json_string(value.substr(equals + 1));
  }
  if (!changes.empty()) {
    options += R"(, "set_parameters": {)" + changes + "}";
  }
  options += "}";
  rust::Box<mitcad::Document> document = mitcad::new_document();
  const std::string report(document->import_fcstd(a.positional[0], options));
  if (const std::string* save = a.option("--save")) {
    save_document(*document, *save);
  }
  if (json) {
    std::cout << R"({"import": )" << report << R"(, "document": )"
              << std::string(document->query(R"({"query": "report"})")) << "}\n";
  } else {
    std::cout << report << std::string(document->report(false));
  }
  const bool differs = report.find("\"pass\": false") != std::string::npos ||
                       report.find("reference check failed") != std::string::npos;
  return differs ? 1 : 0;
}

// The bodies stored in an .ipt part file (mitcad#60).
int import_ipt(const std::vector<std::string>& args) {
  Arguments a;
  std::string problem;
  if (!parse_arguments(args,
                       {"--save", "--report", "--reference", "--max-relative", "--dump", "--design", "--time-limit"},
                       {"--deviation", "--json", "--bodies-only", "--no-verify", "--no-fallback", "--no-compare"}, a,
                       problem)) {
    return usage(problem);
  }
  if (a.positional.size() != 1) {
    return usage(a.positional.empty() ? "no .ipt file" : "more than one .ipt file");
  }
  const bool json = a.flag("--json");
  std::string options = std::string(R"({"text": )") + (json ? "false" : "true");
  for (const auto& [flag, key] : {std::pair<const char*, const char*>{"--bodies-only", "bodies_only"},
                                  {"--no-verify", "no_verify"},
                                  {"--no-fallback", "no_fallback"},
                                  {"--no-compare", "no_compare"}}) {
    if (a.flag(flag)) {
      options += std::string(R"(, ")") + key + R"(": true)";
    }
  }
  if (const std::string* path = a.option("--dump")) {
    options += R"(, "dump_path": )" + json_string(*path);
  }
  if (const std::string* path = a.option("--design")) {
    options += R"(, "design_path": )" + json_string(*path);
  }
  if (const std::string* limit = a.option("--time-limit")) {
    if (!is_number(*limit)) {
      return usage("--time-limit needs a number");
    }
    options += R"(, "time_limit": )" + *limit;
  }
  if (const std::string* path = a.option("--report")) {
    options += R"(, "report_path": )" + json_string(*path);
  }
  if (const std::string* path = a.option("--reference")) {
    options += R"(, "reference": )" + json_string(*path);
  }
  if (const std::string* limit = a.option("--max-relative")) {
    if (!is_number(*limit)) {
      return usage("--max-relative needs a number");
    }
    options += R"(, "max_relative": )" + *limit;
  }
  if (a.flag("--deviation")) {
    options += R"(, "deviation": true)";
  }
  options += "}";
  rust::Box<mitcad::Document> document = mitcad::new_document();
  const std::string report(document->import_ipt(a.positional[0], options));
  if (const std::string* save = a.option("--save")) {
    save_document(*document, *save);
  }
  if (json) {
    std::cout << R"({"import": )" << report << R"(, "document": )"
              << std::string(document->query(R"({"query": "report"})")) << "}\n";
  } else {
    std::cout << report << std::string(document->report(false));
  }
  const bool differs = report.find("\"pass\": false") != std::string::npos ||
                       report.find(": FAILED\n") != std::string::npos;
  return differs ? 1 : 0;
}

int export_bodies(const std::vector<std::string>& args) {
  Arguments a;
  std::string problem;
  if (!parse_arguments(args,
                       {"--bodies", "--schema", "--unit", "--refinement", "--deviation", "--angle", "--coordinates",
                        "--occurrence"},
                       {"--ascii", "--json"}, a, problem)) {
    return usage(problem);
  }
  if (a.positional.size() != 2) {
    return usage("export needs a project file and an output file");
  }
  std::string command = R"({"cmd": "export", "path": )" + json_string(a.positional[1]);
  if (const std::string* bodies = a.option("--bodies")) {
    command += R"(, "bodies": )" + string_list(*bodies);
  }
  if (const std::string* schema = a.option("--schema")) {
    command += R"(, "schema": )" + json_string(*schema);
  }
  if (const std::string* unit = a.option("--unit")) {
    command += R"(, "unit": )" + json_string(*unit);
  }
  const std::string* deviation = a.option("--deviation");
  const std::string* angle = a.option("--angle");
  if (deviation != nullptr || angle != nullptr) {
    if (deviation == nullptr || angle == nullptr || !is_number(*deviation) || !is_number(*angle)) {
      return usage("--deviation and --angle go together and need numbers");
    }
    if (a.flag("--refinement")) {
      return usage("give --refinement or --deviation and --angle, not both");
    }
    std::istringstream degrees(*angle);
    double value = 0.0;
    degrees >> value;
    std::ostringstream radians;
    radians.precision(17);
    radians << value * 3.14159265358979323846 / 180.0;
    command += R"(, "refinement": {"deviation": )" + *deviation + R"(, "angle": )" + radians.str() + "}";
  } else if (const std::string* refinement = a.option("--refinement")) {
    command += R"(, "refinement": )" + json_string(*refinement);
  }
  if (a.flag("--ascii")) {
    command += R"(, "ascii": true)";
  }
  if (const std::string* coordinates = a.option("--coordinates")) {
    command += R"(, "coordinates": )" + json_string(*coordinates);
  }
  if (const std::string* occurrence = a.option("--occurrence")) {
    command += R"(, "occurrence": )" + json_string(*occurrence);
  }
  command += "}";
  rust::Box<mitcad::Document> document = open_document(a.positional[0]);
  const std::string result(document->command(command));
  std::cout << (a.flag("--json") ? result : "Exported: " + result) << "\n";
  return 0;
}

int export_sketch(const std::vector<std::string>& args) {
  Arguments a;
  std::string problem;
  if (!parse_arguments(args, {}, {"--r12", "--json"}, a, problem)) {
    return usage(problem);
  }
  if (a.positional.size() != 3) {
    return usage("export-sketch needs a project file, a sketch and an output file");
  }
  std::string command = R"({"cmd": "export_sketch", "sketch": )" + json_string(a.positional[1]) +
                        R"(, "path": )" + json_string(a.positional[2]);
  if (a.flag("--r12")) {
    command += R"(, "version": "r12")";
  }
  command += "}";
  rust::Box<mitcad::Document> document = open_document(a.positional[0]);
  const std::string result(document->command(command));
  std::cout << (a.flag("--json") ? result : "Exported: " + result) << "\n";
  return 0;
}

// Projects (P12a): a folder with .mitcad/project.json, with version
// history (P12b) unless --no-history.
int project(const std::vector<std::string>& args) {
  Arguments a;
  std::string problem;
  if (!parse_arguments(args, {"--author"}, {"--no-history"}, a, problem)) {
    return usage(problem);
  }
  if (a.positional.size() != 2 || a.positional[0] != "init") {
    return usage("project needs 'init' and a folder");
  }
  const std::string* author = a.option("--author");
  if (a.flag("--no-history")) {
    if (author != nullptr) {
      return usage("--author is for the first version; --no-history makes none");
    }
    std::cout << std::string(mitcad::init_project(a.positional[1])) << "\n";
    return 0;
  }
  std::cout << std::string(mitcad::init_project_history(a.positional[1], author != nullptr ? *author : ""))
            << "\n";
  return 0;
}

// Version history (P12b): the commands of a project whose folder is the
// root of a git repository, answered in text or with --json in JSON.
void print_versions(const mitcad::Project& project, const std::string& command, bool json) {
  if (json) {
    std::cout << std::string(project.command(command)) << "\n";
  } else {
    std::cout << std::string(project.command_text(command));
  }
}

int history(const std::vector<std::string>& args) {
  Arguments a;
  std::string problem;
  if (!parse_arguments(args, {}, {"--json"}, a, problem)) {
    return usage(problem);
  }
  if (a.positional.size() != 1) {
    return usage("history needs a project file");
  }
  const rust::Box<mitcad::Project> project = mitcad::open_project(a.positional[0]);
  print_versions(*project, R"({"cmd": "history", "path": )" + json_string(a.positional[0]) + "}",
                 a.flag("--json"));
  return 0;
}

int version(const std::vector<std::string>& args) {
  Arguments a;
  std::string problem;
  if (!parse_arguments(args, {"-m", "--author", "--save"}, {"--json", "--properties"}, a, problem)) {
    return usage(problem);
  }
  if (a.positional.empty()) {
    return usage("version needs save, show, changes or restore");
  }
  const std::string& what = a.positional[0];
  const std::vector<std::string> rest(a.positional.begin() + 1, a.positional.end());
  const bool json = a.flag("--json");
  const std::string* message = a.option("-m");
  std::string fields;
  if (message != nullptr) {
    fields += R"(, "message": )" + json_string(*message);
  }
  if (const std::string* author = a.option("--author")) {
    fields += R"(, "author": )" + json_string(*author);
  }
  if (what == "save" && !rest.empty()) {
    std::string paths;
    for (const std::string& file : rest) {
      paths += (paths.empty() ? "" : ", ") + json_string(file);
    }
    const rust::Box<mitcad::Project> project = mitcad::open_project(rest[0]);
    print_versions(*project, R"({"cmd": "commit", "paths": [)" + paths + "]" + fields + "}", json);
    return 0;
  }
  if (what == "show" && rest.size() == 2) {
    const rust::Box<mitcad::Project> project = mitcad::open_project(rest[0]);
    rust::Box<mitcad::Document> document = mitcad::load_version(*project, rest[1], rest[0]);
    for (const rust::String& warning : document->load_warnings()) {
      std::cerr << "warning: " << std::string(warning) << "\n";
    }
    document->command(R"({"cmd": "recompute"})");
    if (const std::string* save = a.option("--save")) {
      save_document(*document, *save);
    }
    print_report(*document, json, a.flag("--properties"), "", false);
    return 0;
  }
  if (what == "changes" && (rest.size() == 2 || rest.size() == 3)) {
    const rust::Box<mitcad::Project> project = mitcad::open_project(rest[0]);
    std::string command = R"({"cmd": "changes", "from": )" + json_string(rest[1]);
    if (rest.size() == 3) {
      command += R"(, "to": )" + json_string(rest[2]);
    }
    print_versions(*project, command + "}", json);
    return 0;
  }
  if (what == "restore" && rest.size() == 2) {
    const rust::Box<mitcad::Project> project = mitcad::open_project(rest[0]);
    print_versions(*project,
                   R"({"cmd": "restore", "path": )" + json_string(rest[0]) + R"(, "rev": )" + json_string(rest[1]) +
                       fields + "}",
                   json);
    return 0;
  }
  return usage("version " + what + ": wrong arguments");
}

// The text of field "key" in compact JSON, or "".
std::string json_field(const std::string& json, const std::string& key) {
  const std::string field = "\"" + key + "\":\"";
  const std::size_t at = json.find(field);
  if (at == std::string::npos) {
    return "";
  }
  const std::size_t start = at + field.size();
  return json.substr(start, json.find('"', start) - start);
}

bool ends_with(const std::string& text, const std::string& suffix) {
  return text.size() >= suffix.size() && text.compare(text.size() - suffix.size(), suffix.size(), suffix) == 0;
}

// Comparison of versions (P12c): two project files, or two versions of a
// project file (the second the file as saved when left out), in the
// model's terms; with --geometry both are computed (links not followed) and
// their bodies' volumes and areas compared too.
int diff(const std::vector<std::string>& args) {
  Arguments a;
  std::string problem;
  if (!parse_arguments(args, {}, {"--json", "--geometry"}, a, problem)) {
    return usage(problem);
  }
  if (a.positional.size() < 2 || a.positional.size() > 3) {
    return usage("diff needs two project files, or a project file and one or two versions");
  }
  const bool json = a.flag("--json");
  const bool geometry = a.flag("--geometry");
  const std::string& file = a.positional[0];
  const std::string& second = a.positional[1];
  const auto computed = [geometry](rust::Box<mitcad::Document> document) {
    if (geometry) {
      document->command(R"({"cmd": "recompute"})");
    }
    return document;
  };
  // The options of diff_documents; an empty name is null.
  const auto options = [json, geometry](const std::string& path, const std::string& from, const std::string& to) {
    const auto name = [](const std::string& text) { return text.empty() ? std::string("null") : json_string(text); };
    return std::string(R"({"text": )") + (json ? "false" : "true") + R"(, "geometry": )" +
           (geometry ? "true" : "false") + R"(, "path": )" + name(path) + R"(, "from": )" + name(from) +
           R"(, "to": )" + name(to) + "}";
  };
  const auto print = [json](const std::string& answer) { std::cout << answer << (json ? "\n" : ""); };
  if (a.positional.size() == 2 && (std::filesystem::is_regular_file(second) || ends_with(second, ".mitcad"))) {
    const rust::Box<mitcad::Document> from = computed(mitcad::load_project(file));
    const rust::Box<mitcad::Document> to = computed(mitcad::load_project(second));
    print(std::string(mitcad::diff_documents(*from, *to, options("", file, second))));
    return 0;
  }
  const rust::Box<mitcad::Project> project = mitcad::open_project(file);
  const bool saved = a.positional.size() == 2;
  if (!geometry) {
    std::string command = R"({"cmd": "diff", "path": )" + json_string(file) + R"(, "from": )" + json_string(second);
    if (!saved) {
      command += R"(, "to": )" + json_string(a.positional[2]);
    }
    print_versions(*project, command + "}", json);
    return 0;
  }
  // The versions by their ids (shortened in text), the file by its path in
  // the project, as the version history's diff command names them.
  const auto id = [&project, json](const std::string& rev) {
    const std::string full = json_field(std::string(project->command(R"({"cmd": "resolve", "rev": )" +
                                                                     json_string(rev) + "}")),
                                        "id");
    return json ? full : full.substr(0, 7);
  };
  const std::string path =
      json_field(std::string(project->command(R"({"cmd": "status", "path": )" + json_string(file) + "}")), "path");
  const rust::Box<mitcad::Document> from = computed(mitcad::load_version(*project, second, file));
  const rust::Box<mitcad::Document> to =
      computed(saved ? mitcad::load_project(file) : mitcad::load_version(*project, a.positional[2], file));
  print(std::string(
      mitcad::diff_documents(*from, *to, options(path, id(second), saved ? "" : id(a.positional[2])))));
  return 0;
}

// Remote repositories (P12 remote): through the system's git program, in
// text, or in JSON with --json, whose "error" is null on success. A failure
// of the network, of signing in or of the remote is the exit status 1.
int print_remote(const mitcad::Project& project, const std::string& command, bool json) {
  if (json) {
    const std::string answer(project.command(command));
    std::cout << answer << "\n";
    return answer.find("\"error\":null") == std::string::npos ? 1 : 0;
  }
  std::cout << std::string(project.command_text(command));
  return 0;
}

int remote(const std::vector<std::string>& args) {
  Arguments a;
  std::string problem;
  if (!parse_arguments(args, {"--name", "--author"}, {"--json"}, a, problem)) {
    return usage(problem);
  }
  if (a.positional.empty()) {
    return usage("remote needs add, check, show or remove");
  }
  const std::string& what = a.positional[0];
  const bool json = a.flag("--json");
  std::string fields;
  if (const std::string* name = a.option("--name")) {
    if (what != "add" && what != "remove") {
      return usage("--name is for remote add and remote remove");
    }
    fields += R"(, "name": )" + json_string(*name);
  }
  if (const std::string* author = a.option("--author")) {
    if (what != "add") {
      return usage("--author is for remote add");
    }
    fields += R"(, "author": )" + json_string(*author);
  }
  if ((what == "add" || what == "check") && a.positional.size() == 3) {
    const rust::Box<mitcad::Project> project = mitcad::open_project(a.positional[1]);
    const std::string command = what == "add" ? "connect" : "remote_check";
    return print_remote(*project,
                        R"({"cmd": ")" + command + R"(", "url": )" + json_string(a.positional[2]) + fields + "}",
                        json);
  }
  if (what == "show" && a.positional.size() == 2) {
    const rust::Box<mitcad::Project> project = mitcad::open_project(a.positional[1]);
    if (!json) {
      std::cout << std::string(mitcad::git_info(true));
    }
    return print_remote(*project, R"({"cmd": "remote_info", "links": true})", json);
  }
  if (what == "remove" && a.positional.size() == 2) {
    const rust::Box<mitcad::Project> project = mitcad::open_project(a.positional[1]);
    return print_remote(*project, R"({"cmd": "remote_remove")" + fields + "}", json);
  }
  return usage("remote " + what + ": wrong arguments");
}

// fetch and push of a project's remote.
int fetch_or_push(const std::string& mode, const std::vector<std::string>& args) {
  Arguments a;
  std::string problem;
  if (!parse_arguments(args, {}, {"--json"}, a, problem)) {
    return usage(problem);
  }
  if (a.positional.size() != 1) {
    return usage(mode + " needs a project's folder (or a file in it)");
  }
  const rust::Box<mitcad::Project> project = mitcad::open_project(a.positional[0]);
  return print_remote(*project, R"({"cmd": ")" + mode + R"("})", a.flag("--json"));
}

// Sync: the project and its remote brought together (fetch, then a push, a
// fast-forward or a replay), with a choice for each file changed on both
// sides; a sync that needs such choices is the exit status 3.
int sync_project(const std::vector<std::string>& args) {
  Arguments a;
  std::string problem;
  if (!parse_arguments(args, {"--resolve", "--resolve-all", "--author"}, {"--json", "--no-push", "--dry-run"}, a,
                       problem)) {
    return usage(problem);
  }
  if (a.positional.size() != 1) {
    return usage("sync needs a project's folder (or a file in it)");
  }
  const auto is_choice = [](const std::string& text) { return text == "mine" || text == "theirs" || text == "copy"; };
  std::string resolutions;
  for (const auto& [key, value] : a.options) {
    if (key != "--resolve") {
      continue;
    }
    const std::size_t equals = value.rfind('=');
    if (equals == std::string::npos || equals == 0 || !is_choice(value.substr(equals + 1))) {
      return usage("--resolve takes <path>=mine, <path>=theirs or <path>=copy");
    }
    resolutions += (resolutions.empty() ? "" : ", ") + json_string(value.substr(0, equals)) + ": " +
                   json_string(value.substr(equals + 1));
  }
  const std::string* all = a.option("--resolve-all");
  if (all != nullptr && !is_choice(*all)) {
    return usage("--resolve-all takes mine, theirs or copy");
  }
  const std::string* author = a.option("--author");
  const bool dry_run = a.flag("--dry-run");
  std::string command = R"({"cmd": "sync_plan"})";
  if (dry_run) {
    if (!resolutions.empty() || all != nullptr || author != nullptr || a.flag("--no-push")) {
      return usage("--dry-run tells what a sync would do; it takes no choices");
    }
  } else {
    command = R"({"cmd": "sync", "resolutions": {)" + resolutions + "}";
    if (all != nullptr) {
      command += R"(, "resolve_all": )" + json_string(*all);
    }
    if (author != nullptr) {
      command += R"(, "author": )" + json_string(*author);
    }
    if (a.flag("--no-push")) {
      command += R"(, "push": false)";
    }
    command += "}";
  }
  const rust::Box<mitcad::Project> project = mitcad::open_project(a.positional[0]);
  const std::string answer(project->command(command));
  if (a.flag("--json")) {
    std::cout << answer << "\n";
  } else {
    std::cout << std::string(mitcad::describe_remote(dry_run ? "sync_plan" : "sync", answer));
  }
  if (answer.find("\"error\":null") != std::string::npos) {
    return 0;
  }
  return answer.find("\"class\":\"conflict\"") != std::string::npos ? 3 : 1;
}

// A project opened from a remote into a new folder.
int clone(const std::vector<std::string>& args) {
  Arguments a;
  std::string problem;
  if (!parse_arguments(args, {}, {"--json"}, a, problem)) {
    return usage(problem);
  }
  if (a.positional.size() != 2) {
    return usage("clone needs a remote's URL and a folder");
  }
  const bool json = a.flag("--json");
  const rust::Box<mitcad::SyncControl> control = mitcad::new_sync_control();
  const std::string answer(mitcad::clone_project(a.positional[0], a.positional[1], *control, !json));
  std::cout << answer << (json ? "\n" : "");
  return json && answer.find("\"error\":null") == std::string::npos ? 1 : 0;
}

// Writes a project file in another format without computing it: the
// B-rep data into the project's store (v3) or into the file (v2).
int convert(const std::vector<std::string>& args) {
  Arguments a;
  std::string problem;
  if (!parse_arguments(args, {"--format"}, {}, a, problem)) {
    return usage(problem);
  }
  if (a.positional.size() != 2) {
    return usage("convert needs a project file and an output file");
  }
  std::string format = "auto";
  if (const std::string* given = a.option("--format")) {
    if (*given == "v3") {
      format = "project";
    } else if (*given == "v2") {
      format = "single";
    } else if (*given != "auto") {
      return usage("--format is v2, v3 or auto");
    }
  }
  rust::Box<mitcad::Document> document = mitcad::load_project(a.positional[0]);
  for (const rust::String& warning : document->load_warnings()) {
    std::cerr << "warning: " << std::string(warning) << "\n";
  }
  const std::string text(document->save_project(a.positional[1], format));
  std::cout << "Wrote " << a.positional[1] << " (version " << json_count(text, "version") << ", "
            << text.size() << " bytes)\n";
  return 0;
}

// Ends the program with `code` when import threads that the hang watchdog
// gave up still run (their geometry kernel call has not returned): without
// running static destructors, which would tear down OCCT's and the C++
// runtime's state under them (mitcad#82: a crash at exit after an import
// that hung). Else returns `code` for main to return.
int finish(int code) {
  if (mitcad::abandoned_imports() > 0) {
    std::cout.flush();
    std::cerr.flush();
    std::fflush(nullptr);
    std::_Exit(code);
  }
  return code;
}

int run(int argc, char* argv[]) {
  const std::vector<std::string> args(argv + 1, argv + argc);
  if (args.empty()) {
    return usage("no command");
  }
  if (args[0] == "--help" || args[0] == "-h") {
    std::cout << kUsage << mitcad::cli::kLibraryUsage << kExitStatus;
    return 0;
  }
  const std::string& mode = args[0];
  // OCCT's translators print statistics and warnings to stdout.
  mitcad::io::silence_occt_messages();
  // A crash inside OCCT (an access violation in a fillet of an imported
  // design) fails that operation instead of the program, as on the
  // application's model worker thread; Ctrl+C still ends the program.
  mitcad::geometry::catch_occt_crashes();
  try {
    // Library parts (mitcad#64) are read from the cache of fetched
    // libraries: MITCAD_LIBRARIES_DIR, else the user's data folder.
    mitcad::configure_libraries("");
    if (mode == "info" || mode == "run") {
      return info_or_run(mode, args);
    }
    if (mode == "import") {
      return import(args);
    }
    if (mode == "import-f3d") {
      return import_f3d(args);
    }
    if (mode == "import-fcstd") {
      return import_fcstd(args);
    }
    if (mode == "import-ipt") {
      return import_ipt(args);
    }
    if (mode == "export") {
      return export_bodies(args);
    }
    if (mode == "export-sketch") {
      return export_sketch(args);
    }
    if (mode == "measure") {
      return measure(args);
    }
    if (mode == "compare-step") {
      return compare_step(args);
    }
    if (mode == "project") {
      return project(args);
    }
    if (mode == "convert") {
      return convert(args);
    }
    if (mode == "history") {
      return history(args);
    }
    if (mode == "version") {
      return version(args);
    }
    if (mode == "diff") {
      return diff(args);
    }
    // Remote repositories (P12 remote).
    if (mode == "remote") {
      return remote(args);
    }
    if (mode == "fetch" || mode == "push") {
      return fetch_or_push(mode, args);
    }
    if (mode == "clone") {
      return clone(args);
    }
    if (mode == "sync") {
      return sync_project(args);
    }
    // Component libraries and parts lists (mitcad#64, mitcad#63).
    if (mode == "library") {
      return mitcad::cli::library(args);
    }
    if (mode == "parts") {
      return mitcad::cli::parts(args, [](const std::string& path) { return open_document(path); });
    }
    // The final render (mitcad#48), in builds with the renderer.
    if (mode == "render") {
#ifdef MITCAD_RENDER
      return mitcad::cli::render(argc, argv, [](const std::string& path) { return open_part(path); });
#else
      return fail("this mitcad-cli is built without the renderer (the CMake option MITCAD_RENDER)");
#endif
    }
  } catch (const rust::Error& error) {
    return fail(error.what());
  } catch (const std::exception& error) {
    return fail(error.what());
  }
  return usage("unknown command '" + mode + "'");
}

} // namespace

int main(int argc, char* argv[]) { return finish(run(argc, argv)); }
