// SPDX-License-Identifier: MIT
#include "mesh_compare.hpp"

#include <algorithm>
#include <chrono>
#include <cmath>
#include <cstdio>
#include <iostream>
#include <map>
#include <memory>
#include <set>
#include <sstream>
#include <thread>
#include <utility>

#include <QCryptographicHash>
#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QJsonValue>

#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepTools.hxx>
#include <NCollection_IndexedMap.hxx>
#include <TopExp.hxx>
#include <TopTools_ShapeMapHasher.hxx>

#include "import.hpp"
#include "mitcad/geometry/shape.hpp"
#include "mitcad/io/mesh_compare.hpp"
// The planes' variants: the default insertion (defaultPlaneMeshOptions),
// the view's final tier, mitcad#644's checked Delabella
// (checkedPlaneMeshOptions), and the
// interactive tier's planes (previewMeshOptions), as the application meshes
// them (mitcad#635).
#include "view/ViewTessellation.hpp"

namespace mitcad::cli {

const char* const kMeshCompareUsage =
    R"(  mitcad-cli mesh-compare <design> [--report <report.json>] [--a <variant>] [--b <variant>] [--b-first]
                           [--dump <folder>]
      Opens a design (.mitcad, or a script .json) or imports another file
      as the application imports it (.f3d, .FCStd, .ipt, .iam, .ifc, .rvt,
      .sc, .solidcraft, .dwg, .3dm; others by the generic import), and meshes every
      body's faces twice, each time all bodies together as the 3D view's
      final tier meshes them (commit, opening a design), with the view's
      deviation, angle and face bound, (a) and (b) each one of three
      variants for the planes: "default" (the default insertion: the final
      tier; (a)'s default), "delabella" (OCCT's Delabella triangulation
      unchecked: the interactive preview tier's planes; (b)'s default) or
      "checked" (Delabella, a plane left without a triangulation or with
      its area off its boundary's made again by the default: mitcad#644's
      final tier, held back by mitcad#846). Compares them face by face:
      surface kind, triangles, the deviation of triangle centroids and edge
      midpoints from the surface, the triangulated area against the face's
      own (BRepGProp), triangles folded over others, faces left without a
      triangulation, and each face's triangulation time. --report writes
      every face (JSON); --b-first meshes (b) first; --dump writes the
      faces the two meshings disagree on as BREP files. Prints one line of
      totals. Exit status 1 when the design cannot be opened.
  mitcad-cli mesh-compare --summary <folder>... [--area-tolerance R] [--list N]
      Totals over the reports (*.json) in each folder, one group a folder,
      and over all of them (a design in several folders once), as Markdown
      tables of "achieved / target", the largest speed-ups and slowdowns,
      and the faces where (b) does worse than (a) (at most N of each kind,
      default 40), by the design's content id (the first 8 hex digits of
      the file's SHA-256), body and face index. A face's area is off when
      it differs from the true area by more than R of it (default 0.001)
      or 0.001 mm², whichever is larger.
)";

namespace {

using Clock = std::chrono::steady_clock;

double ms_since(Clock::time_point start) {
  return std::chrono::duration<double, std::milli>(Clock::now() - start).count();
}

std::string lower_suffix(const std::string& path) {
  return QFileInfo(QString::fromStdString(path)).suffix().toLower().toStdString();
}

// The first 8 hex digits of the file's SHA-256: how a design is named in
// public text.
std::string content_id(const std::string& path) {
  QFile file(QString::fromStdString(path));
  if (!file.open(QIODevice::ReadOnly)) {
    return "unknown";
  }
  QCryptographicHash hash(QCryptographicHash::Sha256);
  hash.addData(&file);
  return hash.result().toHex().left(8).toStdString();
}

// The import command the application gives a file (app/files/F3dImport.cpp,
// importWorker): the same command and options, so the bodies are those the
// application shows after opening it.
QJsonObject import_command(const std::string& path) {
  const QString file = QString::fromStdString(path);
  const std::string suffix = lower_suffix(path);
  // The application's hang limit (F3dImport.cpp kHangLimit) and threads
  // (importThreads: the logical cores, at most 8).
  constexpr int kHangLimit = 90;
  const int threads = static_cast<int>(std::clamp(std::thread::hardware_concurrency(), 1u, 8u));
  if (suffix == "fcstd") {
    return {{"cmd", "import_fcstd"}, {"path", file}, {"bodies_only", false}};
  }
  if (suffix == "ifc" || suffix == "ifczip") {
    return {{"cmd", "import_ifc"}, {"path", file}};
  }
  if (suffix == "rvt" || suffix == "rfa") {
    return {{"cmd", "import_rvt"}, {"path", file}};
  }
  if (suffix == "sc" || suffix == "solidcraft") {
    return {{"cmd", "import_solidcraft"}, {"path", file}};
  }
  if (suffix == "dwg") {
    return {{"cmd", "import_dwg"}, {"path", file}};
  }
  if (suffix == "3dm") {
    return {{"cmd", "import_3dm"}, {"path", file}};
  }
  if (suffix == "ipt" || suffix == "iam") {
    return {{"cmd", "import_ipt"}, {"path", file}, {"hang_limit", kHangLimit}};
  }
  if (suffix == "f3d" || suffix == "f3z") {
    return {{"cmd", "import_f3d"}, {"path", file}, {"hang_limit", kHangLimit}, {"threads", threads}};
  }
  return {{"cmd", "import_file"}, {"path", file}};
}

std::string compact(const QJsonObject& object) {
  return QJsonDocument(object).toJson(QJsonDocument::Compact).toStdString();
}

// The report's per-face columns.
const char* const kColumns[] = {"body",        "face",        "surface",    "tolerance",  "area",
                                "triangulated_a", "triangulated_b", "triangles_a", "triangles_b",
                                "deviation_a", "deviation_b", "recorded_a", "recorded_b", "area_a",
                                "area_b",      "folded_a",    "folded_b",   "ms_a",       "ms_b"};

QJsonObject meshing_totals(const io::MeshReport& report) {
  return {{"total_ms", report.total},
          {"triangulate_ms", report.triangulate},
          {"faces", static_cast<qint64>(report.faces)},
          {"meshed", static_cast<qint64>(report.meshed)},
          {"failed", static_cast<qint64>(report.failed)},
          {"planes_redone", static_cast<qint64>(report.planes_redone)}};
}

// The options of a variant of the planes' meshing (kMeshCompareUsage), all
// with the view's deviation, angle and face bound; false for an unknown name.
bool variant_options(const std::string& name, io::MeshOptions& options) {
  if (name == "default") {
    options = defaultPlaneMeshOptions();
  } else if (name == "delabella") {
    options = previewMeshOptions();
  } else if (name == "checked") {
    options = checkedPlaneMeshOptions();
  } else {
    return false;
  }
  return true;
}

bool write_json(const std::string& path, const QJsonObject& object) {
  QFile file(QString::fromStdString(path));
  if (!file.open(QIODevice::WriteOnly | QIODevice::Truncate)) {
    return false;
  }
  return file.write(QJsonDocument(object).toJson(QJsonDocument::Compact)) >= 0;
}

int compare_design(const Arguments& a,
                   const std::function<rust::Box<mitcad::Document>(const std::string&)>& open) {
  const std::string& path = a.positional[0];
  const std::string* report_path = a.option("--report");
  QJsonObject report{{"id", QString::fromStdString(content_id(path))},
                     {"file", QString::fromStdString(path)},
                     {"format", QString::fromStdString(lower_suffix(path))}};
  const std::string* chosen_a = a.option("--a");
  const std::string* chosen_b = a.option("--b");
  const std::string variant_a = chosen_a != nullptr ? *chosen_a : "default";
  const std::string variant_b = chosen_b != nullptr ? *chosen_b : "delabella";
  io::MeshOptions options_a;
  io::MeshOptions options_b;
  if (!variant_options(variant_a, options_a) || !variant_options(variant_b, options_b)) {
    return usage("mesh-compare: a variant is default, delabella or checked");
  }
  report.insert("variants", QJsonObject{{"a", QString::fromStdString(variant_a)},
                                        {"b", QString::fromStdString(variant_b)}});
  report.insert("options", QJsonObject{{"deviation", options_a.linear_deflection},
                                       {"angle", options_a.angular_deflection},
                                       {"face_triangles", static_cast<qint64>(options_a.face_triangles)}});
  const auto fail = [&](const std::string& status) {
    report.insert("status", QString::fromStdString(status));
    if (report_path != nullptr) {
      write_json(*report_path, report);
    }
    std::cerr << "mesh-compare: " << status << "\n";
    return 1;
  };

  const Clock::time_point opening = Clock::now();
  std::unique_ptr<rust::Box<mitcad::Document>> document;
  try {
    const std::string suffix = lower_suffix(path);
    if (suffix == "mitcad" || suffix == "json") {
      document = std::make_unique<rust::Box<mitcad::Document>>(open(path));
    } else {
      document = std::make_unique<rust::Box<mitcad::Document>>(mitcad::new_document());
      (*document)->command(compact(import_command(path)));
    }
  } catch (const std::exception& error) {
    return fail(std::string("open failed: ") + error.what());
  }
  report.insert("open_ms", ms_since(opening));

  // Every body once, as the view meshes each body's shape once however
  // often it is placed.
  const QJsonDocument bodies = QJsonDocument::fromJson(
      QByteArray::fromStdString(std::string((*document)->query(R"({"query": "bodies"})"))));
  std::vector<TopoDS_Shape> shapes;
  QJsonArray uids;
  for (const QJsonValue& value : bodies.array()) {
    const std::string uid = value.toObject().value("uid").toString().toStdString();
    if (uid.empty()) {
      continue;
    }
    const std::shared_ptr<geometry::Shape> shape = (*document)->body_shape(uid);
    if (!shape || shape->occt().IsNull()) {
      continue;
    }
    shapes.push_back(shape->occt());
    uids.append(QString::fromStdString(uid));
  }
  report.insert("bodies", uids);
  if (shapes.empty()) {
    return fail("no bodies");
  }

  const bool b_first = a.flag("--b-first");
  io::MeshComparison comparison;
  try {
    // What the first meshing in a process pays once (OCCT's thread pool,
    // the mesher's factories), on a box both ways, so that neither side
    // of the comparison pays it.
    io::compare_meshing({BRepPrimAPI_MakeBox(10.0, 10.0, 10.0).Shape()}, options_a, options_b, b_first);
    comparison = io::compare_meshing(shapes, options_a, options_b, b_first);
  } catch (const std::exception& error) {
    return fail(std::string("meshing failed: ") + error.what());
  }
  report.insert("status", "ok");
  report.insert("order", b_first ? "b,a" : "a,b");
  report.insert("surfaceless_faces", static_cast<qint64>(comparison.surfaceless));
  report.insert("a", meshing_totals(comparison.a));
  report.insert("b", meshing_totals(comparison.b));
  QJsonArray columns;
  for (const char* column : kColumns) {
    columns.append(column);
  }
  report.insert("columns", columns);
  QJsonArray faces;
  std::size_t planes = 0;
  std::size_t missing_a = 0;
  std::size_t missing_b = 0;
  double plane_ms_a = 0.0;
  double plane_ms_b = 0.0;
  for (const io::FaceMeshComparison& face : comparison.faces) {
    faces.append(QJsonArray{static_cast<qint64>(face.body), static_cast<qint64>(face.face),
                            QString::fromStdString(face.surface), face.tolerance, face.area, face.a.triangulated,
                            face.b.triangulated, static_cast<qint64>(face.a.triangles),
                            static_cast<qint64>(face.b.triangles), face.a.deviation, face.b.deviation,
                            face.a.recorded, face.b.recorded, face.a.area, face.b.area,
                            static_cast<qint64>(face.a.folded), static_cast<qint64>(face.b.folded), face.ms_a,
                            face.ms_b});
    missing_a += face.a.triangulated ? 0 : 1;
    missing_b += face.b.triangulated ? 0 : 1;
    if (face.surface == "plane") {
      ++planes;
      plane_ms_a += std::max(0.0, face.ms_a);
      plane_ms_b += std::max(0.0, face.ms_b);
    }
  }
  report.insert("faces", faces);
  // --dump: the faces the two meshings disagree on, each as a BREP file
  // (<id>-b<body>-f<face>.brep), to look into them.
  if (const std::string* folder = a.option("--dump")) {
    std::vector<NCollection_IndexedMap<TopoDS_Shape, TopTools_ShapeMapHasher>> body_faces(shapes.size());
    for (const io::FaceMeshComparison& face : comparison.faces) {
      const bool differ = face.a.triangulated != face.b.triangulated || face.a.triangles != face.b.triangles ||
                          std::abs(face.a.area - face.b.area) > 1e-6 * std::max(1.0, face.a.area) ||
                          face.a.folded != face.b.folded;
      if (!differ) {
        continue;
      }
      auto& map = body_faces[face.body];
      if (map.IsEmpty()) {
        TopExp::MapShapes(shapes[face.body], TopAbs_FACE, map);
      }
      const std::string file = *folder + "/" + content_id(path) + "-b" + std::to_string(face.body) + "-f" +
                               std::to_string(face.face) + ".brep";
      BRepTools::Write(map(static_cast<int>(face.face) + 1), file.c_str());
    }
  }
  if (report_path != nullptr && !write_json(*report_path, report)) {
    std::cerr << "mesh-compare: cannot write " << *report_path << "\n";
    return 1;
  }
  std::printf("mesh-compare %s: %zu bodies, %zu faces (%zu planes), without a triangulation a %zu b %zu; "
              "meshing a %.1f ms b %.1f ms, planes' triangulation a %.1f ms b %.1f ms; planes redone a %zu b %zu "
              "(a %s, b %s)\n",
              content_id(path).c_str(), shapes.size(), comparison.faces.size(), planes, missing_a, missing_b,
              comparison.a.total, comparison.b.total, plane_ms_a, plane_ms_b, comparison.a.planes_redone,
              comparison.b.planes_redone, variant_a.c_str(), variant_b.c_str());
  return 0;
}

// ---- The totals over many reports ----

// One face of a report, read back.
struct Face {
  int body = 0;
  int face = 0;
  std::string surface;
  double tolerance = 0.0;
  double area = 0.0;
  bool triangulated[2] = {false, false};
  qint64 triangles[2] = {0, 0};
  double deviation[2] = {0.0, 0.0};
  double mesh_area[2] = {0.0, 0.0};
  qint64 folded[2] = {0, 0};
  double ms[2] = {-1.0, -1.0};
};

struct Design {
  std::string id;
  std::string format;
  std::string status;
  double total[2] = {0.0, 0.0};
  double triangulate[2] = {0.0, 0.0};
  std::size_t planes_redone[2] = {0, 0};
  // The planes' variants, "(a) default, (b) delabella" (the reports before
  // the variants were chosen: the trial of mitcad#644).
  std::string variants;
  std::vector<Face> faces;
};

Design read_design(const QJsonObject& object) {
  Design design;
  design.id = object.value("id").toString().toStdString();
  design.format = object.value("format").toString().toStdString();
  design.status = object.value("status").toString("no status").toStdString();
  for (int s = 0; s < 2; ++s) {
    const QJsonObject totals = object.value(s == 0 ? "a" : "b").toObject();
    design.total[s] = totals.value("total_ms").toDouble();
    design.triangulate[s] = totals.value("triangulate_ms").toDouble();
    design.planes_redone[s] = static_cast<std::size_t>(totals.value("planes_redone").toDouble());
  }
  const QJsonObject variants = object.value("variants").toObject();
  design.variants = "(a) " + variants.value("a").toString("default").toStdString() + ", (b) " +
                    variants.value("b").toString("delabella").toStdString();
  std::map<std::string, int> column;
  const QJsonArray columns = object.value("columns").toArray();
  for (int i = 0; i < columns.size(); ++i) {
    column[columns[i].toString().toStdString()] = i;
  }
  const auto at = [&column](const QJsonArray& row, const char* name) {
    const auto found = column.find(name);
    return found == column.end() ? QJsonValue() : row[found->second];
  };
  for (const QJsonValue& value : object.value("faces").toArray()) {
    const QJsonArray row = value.toArray();
    Face face;
    face.body = at(row, "body").toInt();
    face.face = at(row, "face").toInt();
    face.surface = at(row, "surface").toString().toStdString();
    face.tolerance = at(row, "tolerance").toDouble();
    face.area = at(row, "area").toDouble();
    const char* const suffix[2] = {"_a", "_b"};
    for (int s = 0; s < 2; ++s) {
      const auto field = [&](const char* name) { return at(row, (std::string(name) + suffix[s]).c_str()); };
      face.triangulated[s] = field("triangulated").toBool();
      face.triangles[s] = static_cast<qint64>(field("triangles").toDouble());
      face.deviation[s] = field("deviation").toDouble();
      face.mesh_area[s] = field("area").toDouble();
      face.folded[s] = static_cast<qint64>(field("folded").toDouble());
      face.ms[s] = field("ms").toDouble(-1.0);
    }
    design.faces.push_back(std::move(face));
  }
  return design;
}

// What the totals count; s = 0 for (a), 1 for (b).
struct Rules {
  double area_relative = 1e-3;
  double area_absolute = 1e-3; // mm²
  // Planes have the same boundary points both ways: a triangulation of the
  // same polygon has the same area up to rounding.
  double same_relative = 1e-6;
  double same_absolute = 1e-6; // mm²

  bool area_off(const Face& f, int s) const {
    return f.triangulated[s] && f.area >= 0.0 &&
           std::abs(f.mesh_area[s] - f.area) > std::max(area_relative * f.area, area_absolute);
  }
  static bool deviation_over(const Face& f, int s) {
    return f.triangulated[s] && f.deviation[s] > f.tolerance * (1.0 + 1e-6) + 1e-9;
  }
  bool plane_area_differs(const Face& f) const {
    return f.surface == "plane" && f.triangulated[0] && f.triangulated[1] &&
           std::abs(f.mesh_area[1] - f.mesh_area[0]) > std::max(same_relative * f.mesh_area[0], same_absolute);
  }
};

// A face found worse in (b), or a large time difference, by content id.
struct Listed {
  std::string id;
  const Face* face = nullptr;
  double key = 0.0;
};

struct Totals {
  std::size_t designs = 0;
  std::size_t compared = 0;
  std::map<std::string, std::size_t> not_compared; // by status
  std::size_t faces = 0;
  std::size_t triangulated[2] = {0, 0};
  std::size_t planes = 0;
  std::size_t planes_both = 0;
  std::size_t fewer = 0;
  std::size_t equal = 0;
  std::size_t more = 0;
  std::size_t triangles[2] = {0, 0}; // on planes triangulated both ways
  std::size_t lost[2] = {0, 0};       // [0]: (b) none where (a) had one; [1]: the reverse
  std::size_t area_off[2] = {0, 0};
  std::size_t area_off_only[2] = {0, 0};
  std::size_t plane_area_differs = 0;
  std::size_t folded[2] = {0, 0};
  std::size_t folded_only[2] = {0, 0};
  std::size_t deviation_over[2] = {0, 0};
  std::size_t deviation_over_only[2] = {0, 0};
  double total[2] = {0.0, 0.0};
  double triangulate[2] = {0.0, 0.0};
  double plane_ms[2] = {0.0, 0.0};
  double other_ms[2] = {0.0, 0.0};
  std::size_t plane_faster = 0; // planes whose (b) took less time
  std::size_t plane_slower = 0;
  std::size_t planes_redone[2] = {0, 0}; // made again by the default (variant "checked")
  std::vector<Listed> speedups;  // per face: ms (a) - ms (b)
  std::vector<Listed> slowdowns; // ms (b) - ms (a)
  std::vector<std::pair<std::string, std::pair<double, double>>> design_times; // id, (a, b)
  // The faces (b) does worse, by kind.
  std::map<std::string, std::vector<Listed>> problems;

  void add(const Design& design, const Rules& rules) {
    ++designs;
    if (design.status != "ok") {
      // Only the kind: an import's message may name the file.
      const std::string kind = design.status.substr(0, design.status.find(':'));
      ++not_compared[kind];
      problems["design not compared"].push_back(Listed{design.id + " (" + kind + ")", nullptr, 0.0});
      return;
    }
    ++compared;
    for (int s = 0; s < 2; ++s) {
      total[s] += design.total[s];
      triangulate[s] += design.triangulate[s];
      planes_redone[s] += design.planes_redone[s];
    }
    design_times.emplace_back(design.id, std::make_pair(design.total[0], design.total[1]));
    for (const Face& f : design.faces) {
      ++faces;
      const bool plane = f.surface == "plane";
      for (int s = 0; s < 2; ++s) {
        triangulated[s] += f.triangulated[s] ? 1 : 0;
        area_off[s] += rules.area_off(f, s) ? 1 : 0;
        folded[s] += f.folded[s] > 0 ? 1 : 0;
        deviation_over[s] += Rules::deviation_over(f, s) ? 1 : 0;
        (plane ? plane_ms : other_ms)[s] += std::max(0.0, f.ms[s]);
      }
      const auto problem = [&](const char* kind, double key) {
        problems[kind].push_back(Listed{design.id, &f, key});
      };
      if (f.triangulated[0] && !f.triangulated[1]) {
        ++lost[0];
        problem("no triangulation in (b), one in (a)", 0.0);
      }
      if (!f.triangulated[0] && f.triangulated[1]) {
        ++lost[1];
        // Drawn now where it was not: right, or triangles where the face
        // is not (its area tells).
        problem("a triangulation in (b), none in (a)", std::abs(f.mesh_area[1] - f.area));
      }
      // A face without a triangulation has no area to be off: a face (a)
      // leaves undrawn and (b) draws with the wrong area counts for (b).
      const bool off_a = rules.area_off(f, 0);
      const bool off_b = rules.area_off(f, 1);
      if (off_b && !off_a) {
        ++area_off_only[1];
        problem("area off in (b), not in (a)", std::abs(f.mesh_area[1] - f.area));
      }
      if (off_a && !off_b) {
        ++area_off_only[0];
      }
      const bool both = f.triangulated[0] && f.triangulated[1];
      if (both) {
        if (rules.plane_area_differs(f)) {
          ++plane_area_differs;
          problem("plane's area differs between (a) and (b)", std::abs(f.mesh_area[1] - f.mesh_area[0]));
        }
        if (f.folded[1] > 0 && f.folded[0] == 0) {
          ++folded_only[1];
          problem("folded triangles in (b), none in (a)", static_cast<double>(f.folded[1]));
        }
        if (f.folded[0] > 0 && f.folded[1] == 0) {
          ++folded_only[0];
        }
        const bool over_a = Rules::deviation_over(f, 0);
        const bool over_b = Rules::deviation_over(f, 1);
        if (over_b && !over_a) {
          ++deviation_over_only[1];
          problem("deviation over the tolerance in (b), not in (a)", f.deviation[1]);
        }
        if (over_a && !over_b) {
          ++deviation_over_only[0];
        }
      }
      if (plane) {
        ++planes;
        if (both) {
          ++planes_both;
          triangles[0] += static_cast<std::size_t>(f.triangles[0]);
          triangles[1] += static_cast<std::size_t>(f.triangles[1]);
          if (f.triangles[1] < f.triangles[0]) {
            ++fewer;
          } else if (f.triangles[1] == f.triangles[0]) {
            ++equal;
          } else {
            ++more;
            problem("more triangles in (b)", static_cast<double>(f.triangles[1] - f.triangles[0]));
          }
        }
        if (f.ms[0] >= 0.0 && f.ms[1] >= 0.0) {
          plane_faster += f.ms[1] < f.ms[0] ? 1 : 0;
          plane_slower += f.ms[1] > f.ms[0] ? 1 : 0;
        }
      }
      if (f.ms[0] >= 0.0 && f.ms[1] >= 0.0) {
        speedups.push_back(Listed{design.id, &f, f.ms[0] - f.ms[1]});
        slowdowns.push_back(Listed{design.id, &f, f.ms[1] - f.ms[0]});
      }
    }
  }
};

std::string number(double value, int decimals = 1) {
  char text[64];
  std::snprintf(text, sizeof text, "%.*f", decimals, value);
  return text;
}

std::string of(std::size_t achieved, std::size_t target) {
  return std::to_string(achieved) + " / " + std::to_string(target);
}

void top(std::vector<Listed>& list, std::size_t n) {
  std::sort(list.begin(), list.end(), [](const Listed& x, const Listed& y) { return x.key > y.key; });
  if (list.size() > n) {
    list.resize(n);
  }
}

std::string face_row(const Listed& item) {
  const Face& f = *item.face;
  std::ostringstream row;
  row << "| " << item.id << " | " << f.body << " | " << f.face << " | " << f.surface << " | " << f.triangles[0]
      << " / " << f.triangles[1] << " | " << number(f.deviation[0], 4) << " / " << number(f.deviation[1], 4)
      << " (" << number(f.tolerance, 4) << ") | " << number(f.area, 3) << " | " << number(f.mesh_area[0], 3)
      << " / " << number(f.mesh_area[1], 3) << " | " << f.folded[0] << " / " << f.folded[1] << " | "
      << number(f.ms[0]) << " / " << number(f.ms[1]) << " |\n";
  return row.str();
}

const char* const kFaceHeader =
    "| design | body | face | surface | triangles a / b | deviation a / b (tolerance) mm | true area mm² | "
    "area a / b mm² | folded a / b | ms a / b |\n|---|---|---|---|---|---|---|---|---|---|\n";

void print_totals(const std::string& name, const Totals& t, const Rules& rules) {
  std::ostringstream out;
  out << "\n## " << name << "\n\n| group | achieved / target |\n|---|---|\n";
  out << "| designs compared | " << of(t.compared, t.designs) << " |\n";
  for (const auto& [status, count] : t.not_compared) {
    out << "| designs not compared: " << status << " | " << of(count, t.designs) << " |\n";
  }
  out << "| faces triangulated (a) | " << of(t.triangulated[0], t.faces) << " |\n";
  out << "| faces triangulated (b) | " << of(t.triangulated[1], t.faces) << " |\n";
  out << "| faces with a triangulation in (a), none in (b) | " << of(t.lost[0], t.faces) << " |\n";
  out << "| faces with a triangulation in (b), none in (a) | " << of(t.lost[1], t.faces) << " |\n";
  out << "| planes triangulated both ways | " << of(t.planes_both, t.planes) << " |\n";
  out << "| planes with fewer triangles in (b) | " << of(t.fewer, t.planes_both) << " |\n";
  out << "| planes with as many triangles in (b) | " << of(t.equal, t.planes_both) << " |\n";
  out << "| planes with more triangles in (b) | " << of(t.more, t.planes_both) << " |\n";
  out << "| triangles on those planes, (b) / (a) | " << of(t.triangles[1], t.triangles[0]) << " |\n";
  out << "| planes whose area differs between (a) and (b) | " << of(t.plane_area_differs, t.planes_both)
      << " |\n";
  out << "| faces with the area off, (a) | " << of(t.area_off[0], t.triangulated[0]) << " |\n";
  out << "| faces with the area off, (b) | " << of(t.area_off[1], t.triangulated[1]) << " |\n";
  out << "| faces with the area off in (b), not in (a) | " << of(t.area_off_only[1], t.faces) << " |\n";
  out << "| faces with the area off in (a), not in (b) | " << of(t.area_off_only[0], t.faces) << " |\n";
  out << "| faces with folded triangles, (a) | " << of(t.folded[0], t.triangulated[0]) << " |\n";
  out << "| faces with folded triangles, (b) | " << of(t.folded[1], t.triangulated[1]) << " |\n";
  out << "| faces folded in (b), not in (a) | " << of(t.folded_only[1], t.faces) << " |\n";
  out << "| faces with the deviation over the tolerance, (a) | " << of(t.deviation_over[0], t.triangulated[0])
      << " |\n";
  out << "| faces with the deviation over the tolerance, (b) | " << of(t.deviation_over[1], t.triangulated[1])
      << " |\n";
  out << "| faces over it in (b), not in (a) | " << of(t.deviation_over_only[1], t.faces) << " |\n";
  out << "| planes triangulated faster in (b) | " << of(t.plane_faster, t.planes_both) << " |\n";
  out << "| planes triangulated slower in (b) | " << of(t.plane_slower, t.planes_both) << " |\n";
  out << "| planes made again by the default after Delabella, (a) | " << of(t.planes_redone[0], t.planes) << " |\n";
  out << "| planes made again by the default after Delabella, (b) | " << of(t.planes_redone[1], t.planes) << " |\n";
  out << "\nArea off: more than " << number(rules.area_relative * 100.0, 2) << " % of the true area or "
      << number(rules.area_absolute, 4) << " mm², whichever is larger; planes' areas differ: more than "
      << rules.same_relative << " of it or " << rules.same_absolute << " mm².\n";
  out << "\n| time | (a) | (b) | (b) / (a) |\n|---|---|---|---|\n";
  const auto ratio = [](double a, double b) { return a > 0.0 ? number(b / a, 3) : std::string("-"); };
  out << "| meshing, all designs (ms) | " << number(t.total[0]) << " | " << number(t.total[1]) << " | "
      << ratio(t.total[0], t.total[1]) << " |\n";
  out << "| of it, faces' triangulation (ms) | " << number(t.triangulate[0]) << " | " << number(t.triangulate[1])
      << " | " << ratio(t.triangulate[0], t.triangulate[1]) << " |\n";
  out << "| planes, sum over faces (ms, on the mesher's threads) | " << number(t.plane_ms[0]) << " | "
      << number(t.plane_ms[1]) << " | " << ratio(t.plane_ms[0], t.plane_ms[1]) << " |\n";
  out << "| other faces, sum over faces (ms) | " << number(t.other_ms[0]) << " | " << number(t.other_ms[1])
      << " | " << ratio(t.other_ms[0], t.other_ms[1]) << " |\n";
  if (t.compared > 0) {
    out << "| per design, mean (ms) | " << number(t.total[0] / static_cast<double>(t.compared), 2) << " | "
        << number(t.total[1] / static_cast<double>(t.compared), 2) << " | |\n";
  }
  if (t.faces > 0) {
    out << "| per face, mean of the faces' times (ms) | "
        << number((t.plane_ms[0] + t.other_ms[0]) / static_cast<double>(t.faces), 3) << " | "
        << number((t.plane_ms[1] + t.other_ms[1]) / static_cast<double>(t.faces), 3) << " | |\n";
  }
  std::cout << out.str();
}

void print_details(Totals& t, std::size_t list) {
  std::ostringstream out;
  auto designs = t.design_times;
  std::sort(designs.begin(), designs.end(), [](const auto& x, const auto& y) {
    return x.second.first - x.second.second > y.second.first - y.second.second;
  });
  const std::size_t shown = std::min<std::size_t>(10, designs.size());
  out << "\n### Designs with the largest speed-up in (b)\n\n| design | (a) ms | (b) ms |\n|---|---|---|\n";
  for (std::size_t i = 0; i < shown; ++i) {
    out << "| " << designs[i].first << " | " << number(designs[i].second.first) << " | "
        << number(designs[i].second.second) << " |\n";
  }
  out << "\n### Designs with the largest slowdown in (b)\n\n| design | (a) ms | (b) ms |\n|---|---|---|\n";
  for (std::size_t i = 0; i < shown; ++i) {
    const auto& d = designs[designs.size() - 1 - i];
    out << "| " << d.first << " | " << number(d.second.first) << " | " << number(d.second.second) << " |\n";
  }
  top(t.speedups, 10);
  top(t.slowdowns, 10);
  out << "\n### Largest speed-ups of a face\n\n" << kFaceHeader;
  for (const Listed& item : t.speedups) {
    out << face_row(item);
  }
  out << "\n### Largest slowdowns of a face\n\n" << kFaceHeader;
  for (const Listed& item : t.slowdowns) {
    if (item.key > 0.0) {
      out << face_row(item);
    }
  }
  out << "\n### Faces to look at: (b) worse than (a), or different\n";
  if (t.problems.empty()) {
    out << "\nNone.\n";
  }
  for (auto& [kind, items] : t.problems) {
    out << "\n#### " << kind << ": " << items.size() << "\n\n";
    if (items.front().face == nullptr) {
      std::set<std::string> seen;
      for (const Listed& item : items) {
        if (seen.insert(item.id).second) {
          out << "- " << item.id << "\n";
        }
      }
      continue;
    }
    top(items, list);
    out << kFaceHeader;
    for (const Listed& item : items) {
      out << face_row(item);
    }
  }
  std::cout << out.str();
}

int summary(const Arguments& a) {
  Rules rules;
  std::size_t list = 40;
  if (const std::string* value = a.option("--area-tolerance")) {
    if (!is_number(*value)) {
      return usage("--area-tolerance needs a number");
    }
    rules.area_relative = std::stod(*value);
  }
  if (const std::string* value = a.option("--list")) {
    if (!is_number(*value)) {
      return usage("--list needs a number");
    }
    list = static_cast<std::size_t>(std::stod(*value));
  }
  // The designs are kept while the totals point at their faces.
  std::vector<std::unique_ptr<Design>> designs;
  std::vector<std::pair<std::string, Totals>> groups;
  Totals all;
  std::set<std::string> seen;
  std::set<std::string> variants;
  for (const std::string& folder : a.positional) {
    const QDir dir(QString::fromStdString(folder));
    if (!dir.exists()) {
      return usage("no folder " + folder);
    }
    Totals totals;
    for (const QString& name : dir.entryList({"*.json"}, QDir::Files, QDir::Name)) {
      QFile file(dir.filePath(name));
      if (!file.open(QIODevice::ReadOnly)) {
        continue;
      }
      const QJsonObject object = QJsonDocument::fromJson(file.readAll()).object();
      if (object.isEmpty()) {
        continue;
      }
      designs.push_back(std::make_unique<Design>(read_design(object)));
      const Design& design = *designs.back();
      totals.add(design, rules);
      if (design.status == "ok") {
        variants.insert(design.variants);
      }
      if (seen.insert(design.id).second) {
        all.add(design, rules);
      }
    }
    groups.emplace_back(dir.dirName().toStdString(), std::move(totals));
  }
  // The planes' variants (kMeshCompareUsage): one pair, unless reports of
  // several runs are totalled together.
  std::string compared;
  for (const std::string& pair : variants) {
    compared += (compared.empty() ? "" : "; ") + pair;
  }
  std::cout << "# Display meshing, planes: " << (compared.empty() ? "(a) default, (b) delabella" : compared) << "\n";
  for (auto& [name, totals] : groups) {
    print_totals(name, totals, rules);
  }
  print_totals("All (each design once)", all, rules);
  print_details(all, list);
  return 0;
}

} // namespace

int mesh_compare(const std::vector<std::string>& args,
                 const std::function<rust::Box<mitcad::Document>(const std::string&)>& open) {
  Arguments a;
  std::string problem;
  if (!parse_arguments(args, {"--report", "--dump", "--area-tolerance", "--list", "--a", "--b"},
                       {"--b-first", "--summary"}, a, problem)) {
    return usage(problem);
  }
  if (a.flag("--summary")) {
    if (a.positional.empty()) {
      return usage("mesh-compare --summary needs report folders");
    }
    return summary(a);
  }
  if (a.positional.size() != 1) {
    return usage("mesh-compare needs one design");
  }
  return compare_design(a, open);
}

} // namespace mitcad::cli
