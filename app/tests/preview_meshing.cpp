// SPDX-License-Identifier: MIT
// The view's meshes of a dragged Extrude's preview (mitcad#635), on the
// synthetic fixture of the ticket: a 120 x 100 x 20 mm block with 83
// through holes of 4 mm (centres x = 8, 18 ... 108 and y = 8, 18 ... 88,
// none with 45 < x < 80 and 35 < y < 70), and a 20 x 10 mm rectangle at
// (50, 45) extruded from 20 mm above its sketch, Join: a body of 94 faces
// and 273 edges; and the same without the holes (11 faces, 24 edges).
//
// The document is made through the model's commands as the application
// makes it, its shown body meshed as the view's job meshes it, and then
// each preview of a new distance is computed and meshed as the preview job
// does (DocumentSession::preview: Document::preview, then meshForDisplay
// of previewShapes) on this thread.
//
//   test_preview_meshing                 the checks (ctest app.preview_meshing)
//   test_preview_meshing --compare [--plain] [--samples n]
//                                        mesh options for the preview tier
//                                        compared on the preview's bodies
//   test_preview_meshing --paired [--plain] [--samples n]
//                                        before (the final tier, nothing reused)
//                                        and after (interactive; final with
//                                        reuse), interleaved sample by sample
//   test_preview_meshing --bench [--final] [--plain] [--samples n]
//                                        the timings: each preview's document
//                                        time and display meshing (interactive
//                                        tier; --final: the final tier, as a
//                                        commit or a refinement meshes), median
//                                        and p95 (MITCAD_MESH_TRACE=1 adds the
//                                        meshings' phases and faces)
//   test_preview_meshing --abandon-at-exit
//                                        the process the checks start to quit
//                                        with a cancelled meshing's face that
//                                        never ends (mitcad#936)
#include <algorithm>
#include <atomic>
#include <chrono>
#include <cmath>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <memory>
#include <string>
#include <thread>
#include <utility>
#include <vector>

#include <QByteArray>
#include <QCoreApplication>
#include <QDir>
#include <QDirIterator>
#include <QFile>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QProcess>
#include <QProcessEnvironment>
#include <QString>
#include <QStringList>
#include <QTemporaryDir>

#include <BRepBuilderAPI_Copy.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakePolygon.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepMesh_DelabellaMeshAlgoFactory.hxx>
#include <BRepMesh_MeshAlgoFactory.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <IMeshTools_MeshAlgo.hxx>
#include <IMeshTools_MeshAlgoFactory.hxx>
#include <BRep_Tool.hxx>
#include <Poly_Triangulation.hxx>
#include <Prs3d_Drawer.hxx>
#include <StdPrs_ToolTriangulatedShape.hxx>
#include <gp_Pln.hxx>
#include <gp_Pnt.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <NCollection_IndexedMap.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <TopLoc_Location.hxx>
#include <TopoDS.hxx>

#include "framework/DisplayMeshes.hpp"
#include "framework/Json.hpp"
#include "framework/ModelWorker.hpp"
#include "framework/ProcessEnd.hpp"
#include "mitcad/geometry/guard.hpp"
#include "mitcad/io/mesh.hpp"
#include "report/CrashHandler.hpp"
#include "view/ViewTessellation.hpp"

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "preview_meshing.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

using mitcad::Document;
using mitcad::parseObject;
using mitcad::rustStr;

const char* const kRectangle = "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}";

QJsonObject run(Document& document, const QJsonObject& command) {
  return parseObject(document.command(rustStr(mitcad::compactJson(command))));
}

QJsonObject rectangle(const QString& sketch, double x, double y, double width, double height) {
  return QJsonObject{{"cmd", "sketch.add_rectangle"},
                     {"sketch", sketch},
                     {"corner", QJsonArray{x, y}},
                     {"width", width},
                     {"height", height}};
}

QJsonObject profile(const QString& sketch, const QString& region) {
  return QJsonObject{{"sketch", sketch}, {"region", region}};
}

// The fixture before the dragged Extrude: the block (with its holes), and
// the boss's sketch. Returns the boss sketch's uid.
QString makeFixture(Document& document, bool holes) {
  const QString block = run(document, QJsonObject{{"cmd", "sketch.create"}}).value("uid").toString();
  run(document, rectangle(block, 0, 0, 120, 100));
  run(document, QJsonObject{{"cmd", "add_feature"},
                            {"def", QJsonObject{{"type", "extrude"},
                                                {"profiles", QJsonArray{profile(block, kRectangle)}},
                                                {"extent", QJsonObject{{"type", "distance"}, {"distance", 20}}},
                                                {"operation", "new_body"}}}});
  if (holes) {
    const QString drill = run(document, QJsonObject{{"cmd", "sketch.create"}}).value("uid").toString();
    QJsonArray regions;
    for (int x = 8; x <= 108; x += 10) {
      for (int y = 8; y <= 88; y += 10) {
        if (x > 45 && x < 80 && y > 35 && y < 70) {
          continue;
        }
        const QJsonObject circle = run(document, QJsonObject{{"cmd", "sketch.add_circle"},
                                                             {"sketch", drill},
                                                             {"center", QJsonArray{x, y}},
                                                             {"diameter", 4}});
        regions.append(profile(drill, circle.value("region").toString()));
      }
    }
    CHECK(regions.size() == 83);
    run(document, QJsonObject{{"cmd", "add_feature"},
                              {"def", QJsonObject{{"type", "extrude"},
                                                  {"profiles", regions},
                                                  {"start", QJsonObject{{"type", "offset"}, {"offset", -5}}},
                                                  {"extent", QJsonObject{{"type", "distance"}, {"distance", 30}}},
                                                  {"operation", "cut"}}}});
  }
  const QString boss = run(document, QJsonObject{{"cmd", "sketch.create"}}).value("uid").toString();
  run(document, rectangle(boss, 50, 45, 20, 10));
  return boss;
}

QByteArray extrudeRequest(const QString& sketch, double distance) {
  return mitcad::compactJson(QJsonObject{
      {"cmd", "add_feature"},
      {"def", QJsonObject{{"type", "extrude"},
                          {"profiles", QJsonArray{profile(sketch, kRectangle)}},
                          {"start", QJsonObject{{"type", "offset"}, {"offset", 20}}},
                          {"extent", QJsonObject{{"type", "distance"}, {"distance", distance}}},
                          {"operation", "join"}}}});
}

// The view's meshing after a model command (DocumentSession::command).
mitcad::DisplayMeshing meshShown(const Document& document) {
  mitcad::ModelJob job;
  const mitcad::DisplayMeshing meshing = mitcad::meshForDisplay(mitcad::shownBodyShapes(document), job);
  mitcad::logDisplayMeshing(meshing, QStringLiteral("shown"));
  return meshing;
}

struct Preview {
  QJsonObject report;
  mitcad::DisplayShapes shapes;
  mitcad::DisplayMeshing meshing;
  double documentMs = 0.0;
  double meshMs = 0.0;
};

// One preview as the preview job computes it (DocumentSession::preview):
// the interactive tier, with the changed bodies as shown for donors.
Preview preview(Document& document, const QByteArray& request,
                mitcad::DisplayTier tier = mitcad::DisplayTier::Interactive, bool donors = true) {
  Preview result;
  mitcad::ModelJob job;
  const auto start = std::chrono::steady_clock::now();
  {
    const mitcad::AttachedJob attached(document, job);
    result.report = parseObject(document.preview(rustStr(request)));
  }
  const auto computed = std::chrono::steady_clock::now();
  result.shapes = mitcad::previewShapes(document, result.report);
  result.meshing =
      mitcad::meshForDisplay(result.shapes, job, tier,
                             donors ? mitcad::previewDonors(document, result.report) : mitcad::DisplayShapes());
  const auto meshed = std::chrono::steady_clock::now();
  result.documentMs = std::chrono::duration<double, std::milli>(computed - start).count();
  result.meshMs = std::chrono::duration<double, std::milli>(meshed - computed).count();
  mitcad::logDisplayMeshing(result.meshing, QStringLiteral("preview"));
  return result;
}

// The changed body of a preview (the Join's result).
std::shared_ptr<mitcad::geometry::Shape> changedBody(const Document& document, const QJsonObject& report) {
  for (const QJsonValue& value : report.value("bodies").toArray()) {
    const QJsonObject body = value.toObject();
    if (body.value("changed").toBool()) {
      return document.preview_body_shape(body.value("uid").toString().toStdString());
    }
  }
  return nullptr;
}

int countOf(const TopoDS_Shape& shape, TopAbs_ShapeEnum type) {
  NCollection_IndexedMap<TopoDS_Shape, TopTools_ShapeMapHasher> distinct;
  TopExp::MapShapes(shape, type, distinct);
  return distinct.Extent();
}

double nearestRank(std::vector<double> values, double fraction) {
  if (values.empty()) {
    return 0.0;
  }
  std::sort(values.begin(), values.end());
  const auto rank = static_cast<std::size_t>(std::ceil(fraction * static_cast<double>(values.size())));
  return values[std::min(values.size(), std::max<std::size_t>(rank, 1)) - 1];
}

// Before and after under the same conditions, sample by sample: each
// sample previews three new distances, meshed as before mitcad#635 (the
// final tier in the body, nothing reused), as a preview now (the
// interactive tier, reused faces) and as a commit or refinement now (the
// final tier, reused faces).
int paired(bool holes, int samples) {
  rust::Box<Document> document = mitcad::new_document();
  const QString sketch = makeFixture(*document, holes);
  meshShown(*document);
  std::vector<double> before;
  std::vector<double> interactive;
  std::vector<double> final;
  for (int i = 0; i < samples; ++i) {
    const double distance = 5.0 + 0.25 * (i + 1);
    before.push_back(preview(*document, extrudeRequest(sketch, distance), mitcad::DisplayTier::Final, false).meshMs);
    interactive.push_back(preview(*document, extrudeRequest(sketch, distance + 0.0625)).meshMs);
    final.push_back(preview(*document, extrudeRequest(sketch, distance + 0.125), mitcad::DisplayTier::Final).meshMs);
  }
  std::printf("%s, n=%d each, meshForDisplay median / p95: before %.2f / %.2f ms; interactive preview %.2f / %.2f ms; "
              "final with reuse %.2f / %.2f ms\n",
              holes ? "83 holes (94 faces)" : "no holes (11 faces)", samples, nearestRank(before, 0.5),
              nearestRank(before, 0.95), nearestRank(interactive, 0.5), nearestRank(interactive, 0.95),
              nearestRank(final, 0.5), nearestRank(final, 0.95));
  return 0;
}

int bench(bool holes, int samples, mitcad::DisplayTier tier) {
  rust::Box<Document> document = mitcad::new_document();
  const QString sketch = makeFixture(*document, holes);
  meshShown(*document);
  std::vector<double> documentMs;
  std::vector<double> meshMs;
  std::size_t kept = 0;
  std::size_t meshed = 0;
  for (int i = 0; i < samples; ++i) {
    // A held drag upwards: a new distance every time (no cached result).
    const Preview p = preview(*document, extrudeRequest(sketch, 5.0 + 0.25 * (i + 1)), tier);
    documentMs.push_back(p.documentMs);
    meshMs.push_back(p.meshMs);
    kept += p.meshing.report.kept;
    meshed += p.meshing.report.meshed;
    std::printf("sample %d: document %.2f ms, meshForDisplay %.2f ms (faces reused %zu, kept %zu, meshed %zu)\n",
                i + 1, p.documentMs, p.meshMs, p.meshing.reused, p.meshing.report.kept, p.meshing.report.meshed);
  }
  std::printf("%s tier, %s, n=%d: document median %.2f / p95 %.2f ms; meshForDisplay median %.2f / p95 %.2f ms; "
              "faces kept %zu, meshed %zu in all\n",
              tier == mitcad::DisplayTier::Final ? "final" : "interactive",
              holes ? "83 holes (94 faces)" : "no holes (11 faces)", samples, nearestRank(documentMs, 0.5),
              nearestRank(documentMs, 0.95), nearestRank(meshMs, 0.5), nearestRank(meshMs, 0.95), kept, meshed);
  return 0;
}

// The bounded comparison of mesh options for the preview tier: the
// preview's bodies (the Join's result and the tool) meshed again from no
// triangulation with each candidate, `samples` times each.
int compare(bool holes, int samples) {
  rust::Box<Document> document = mitcad::new_document();
  const QString sketch = makeFixture(*document, holes);
  meshShown(*document);
  const Preview p = preview(*document, extrudeRequest(sketch, 5.0));
  struct Candidate {
    const char* name;
    mitcad::io::MeshOptions options;
  };
  const double degree = 3.14159265358979323846 / 180.0;
  const std::size_t bound = mitcad::kViewFaceTriangles;
  const Candidate candidates[] = {
      {"default planes: 0.01 mm, 5 deg", mitcad::defaultPlaneMeshOptions()},
      {"final, checked planes (mitcad#644)", mitcad::viewMeshOptions()},
      {"fast planes: 0.01 mm, 5 deg", mitcad::io::MeshOptions{0.01, 5 * degree, false, bound, true, false}},
      {"0.02 mm, 10 deg", mitcad::io::MeshOptions{0.02, 10 * degree, false, bound, false, false}},
      {"0.05 mm, 20 deg", mitcad::io::MeshOptions{0.05, 20 * degree, false, bound, false, false}},
      {"fast planes: 0.02 mm, 10 deg", mitcad::io::MeshOptions{0.02, 10 * degree, false, bound, true, false}},
  };
  for (const Candidate& candidate : candidates) {
    std::vector<double> times;
    std::size_t triangles = 0;
    std::size_t nodes = 0;
    for (int i = 0; i < samples; ++i) {
      std::vector<TopoDS_Shape> bare;
      for (const auto& shape : p.shapes) {
        bare.push_back(BRepBuilderAPI_Copy(shape->occt(), false, false).Shape());
      }
      const auto start = std::chrono::steady_clock::now();
      mitcad::io::mesh_all_in_place(bare, candidate.options);
      times.push_back(std::chrono::duration<double, std::milli>(std::chrono::steady_clock::now() - start).count());
      triangles = 0;
      nodes = 0;
      for (const TopoDS_Shape& shape : bare) {
        const mitcad::io::MeshStats stats = mitcad::io::mesh_stats(shape);
        triangles += stats.triangles;
        nodes += stats.nodes;
      }
    }
    std::printf("%-30s n=%d: median %.2f / p95 %.2f ms, %zu triangles, %zu nodes\n", candidate.name, samples,
                nearestRank(times, 0.5), nearestRank(times, 0.95), triangles, nodes);
  }
  return 0;
}

using ShapePtr = std::shared_ptr<mitcad::geometry::Shape>;

// The faces' triangulations of a shape, in face order (null where none).
std::vector<const Poly_Triangulation*> triangulations(const TopoDS_Shape& shape) {
  std::vector<const Poly_Triangulation*> found;
  for (TopExp_Explorer it(shape, TopAbs_FACE); it.More(); it.Next()) {
    TopLoc_Location location;
    found.push_back(BRep_Tool::Triangulation(TopoDS::Face(it.Current()), location).get());
  }
  return found;
}

// How many edges of the faces with a triangulation have no polygon on it.
int edgesWithoutPolygons(const TopoDS_Shape& shape) {
  int missing = 0;
  for (TopExp_Explorer face(shape, TopAbs_FACE); face.More(); face.Next()) {
    TopLoc_Location location;
    const auto mesh = BRep_Tool::Triangulation(TopoDS::Face(face.Current()), location);
    if (mesh.IsNull()) {
      continue;
    }
    for (TopExp_Explorer edge(face.Current(), TopAbs_EDGE); edge.More(); edge.Next()) {
      missing += BRep_Tool::PolygonOnTriangulation(TopoDS::Edge(edge.Current()), mesh, location).IsNull() ? 1 : 0;
    }
  }
  return missing;
}

bool anyTriangulation(const TopoDS_Shape& shape) {
  for (const Poly_Triangulation* triangulation : triangulations(shape)) {
    if (triangulation != nullptr) {
      return true;
    }
  }
  return false;
}

// Whether the shapes share a face (a TShape) or a triangulation.
bool shareFacesOrMeshes(const TopoDS_Shape& a, const TopoDS_Shape& b) {
  NCollection_IndexedMap<TopoDS_Shape, TopTools_ShapeMapHasher> faces;
  TopExp::MapShapes(a, TopAbs_FACE, faces);
  for (TopExp_Explorer it(b, TopAbs_FACE); it.More(); it.Next()) {
    if (faces.Contains(it.Current())) {
      return true;
    }
  }
  const std::vector<const Poly_Triangulation*> mine = triangulations(a);
  for (const Poly_Triangulation* triangulation : triangulations(b)) {
    if (triangulation != nullptr && std::find(mine.begin(), mine.end(), triangulation) != mine.end()) {
      return true;
    }
  }
  return false;
}

// A body meshed in the final tier: a triangulation on every face, its own,
// marked meshed, no stand-in, not one itself.
bool finalTier(const ShapePtr& body) {
  return body && mitcad::io::has_triangulation(body->occt()) && body->display_meshed() &&
         !body->display_interactive() && !body->display_stand_in() && mitcad::displayShape(body) == body &&
         edgesWithoutPolygons(body->occt()) == 0;
}

// A model command as the command job runs it (DocumentSession::command):
// the shown bodies before it are the donors of the faces it keeps.
QJsonObject command(Document& document, const QJsonObject& request, mitcad::DisplayMeshing* meshing = nullptr) {
  mitcad::ModelJob job;
  const mitcad::DisplayShapes before = mitcad::shownBodyShapes(document);
  QJsonObject answer;
  {
    const mitcad::AttachedJob attached(document, job);
    answer = run(document, request);
  }
  const mitcad::DisplayMeshing made =
      mitcad::meshForDisplay(mitcad::shownBodyShapes(document), job, mitcad::DisplayTier::Final, before);
  mitcad::logDisplayMeshing(made, request.value("cmd").toString());
  if (meshing != nullptr) {
    *meshing = made;
  }
  return answer;
}

// The fixture is the ticket's: one body of 94 faces and 273 edges with the
// holes, 11 and 24 without.
void testFixture() {
  for (const bool holes : {true, false}) {
    rust::Box<Document> document = mitcad::new_document();
    const QString sketch = makeFixture(*document, holes);
    meshShown(*document);
    const Preview p = preview(*document, extrudeRequest(sketch, 5.0));
    CHECK(p.report.value("status").toString() == QStringLiteral("ok"));
    const ShapePtr body = changedBody(*document, p.report);
    CHECK(body != nullptr);
    if (!body) {
      continue;
    }
    CHECK(countOf(body->occt(), TopAbs_FACE) == (holes ? 94 : 11));
    CHECK(countOf(body->occt(), TopAbs_EDGE) == (holes ? 273 : 24));
  }
}

// A preview in the interactive tier (mitcad#635): the body and the tool
// stay without triangulations, their stand-ins (copies sharing no face)
// are meshed, the faces the Join left as they were taken from the body
// as shown (88 of 94: the bottom, the sides and the 83 holes), only the
// top and the boss meshed; the view's drawers find the stand-ins meshed
// finely enough and never mesh them; the same preview again meshes
// nothing; a refinement meshes the body and the tool in the final tier,
// drops the stand-ins and shares nothing with them.
void testInteractiveTier() {
  rust::Box<Document> document = mitcad::new_document();
  const QString sketch = makeFixture(*document, true);
  meshShown(*document);
  const Preview p = preview(*document, extrudeRequest(sketch, 5.0));
  const ShapePtr body = changedBody(*document, p.report);
  const ShapePtr tool = document->preview_tool_shape();
  CHECK(body && tool);
  if (!body || !tool) {
    return;
  }
  std::printf("Interactive preview: %zu faces reused, %zu meshed, %zu stand-ins\n", p.meshing.reused,
              p.meshing.report.meshed, p.meshing.standIns.size());
  CHECK(p.meshing.tier == mitcad::DisplayTier::Interactive);
  CHECK(p.meshing.reused == 88);
  CHECK(p.meshing.report.kept == 88);
  CHECK(p.meshing.report.meshed == 6 + 5); // the top and the boss; the tool's 6 faces, 5 TShapes
  CHECK(p.meshing.standIns.size() == 2);
  for (const ShapePtr& shape : {body, tool}) {
    CHECK(!anyTriangulation(shape->occt()));
    CHECK(!shape->display_meshed());
    const ShapePtr standIn = shape->display_stand_in();
    CHECK(standIn != nullptr);
    if (!standIn) {
      continue;
    }
    CHECK(mitcad::displayShape(shape) == standIn);
    CHECK(standIn->display_interactive());
    CHECK(standIn->stand_in_for() == shape);
    CHECK(standIn->display_meshed());
    CHECK(mitcad::io::has_triangulation(standIn->occt()));
    CHECK(!shareFacesOrMeshes(standIn->occt(), shape->occt()));
    CHECK(countOf(standIn->occt(), TopAbs_FACE) == countOf(shape->occt(), TopAbs_FACE));
    CHECK(!mitcad::viewMeshes(*standIn));
  }
  // The shown body's own triangulations were copied, not shared.
  const mitcad::DisplayShapes shown = mitcad::shownBodyShapes(*document);
  CHECK(shown.size() == 1);
  const ShapePtr standIn = body->display_stand_in();
  if (!shown.empty() && standIn) {
    CHECK(!shareFacesOrMeshes(shown.front()->occt(), standIn->occt()));
    CHECK(finalTier(shown.front()));
  }

  // Every edge of every face has its polygon on the face's triangulation,
  // also the prism's top edges, the same edges as its base's moved up
  // (BRepTools::Triangulation, as the view's drawers check).
  if (standIn) {
    CHECK(edgesWithoutPolygons(standIn->occt()) == 0);
    const ShapePtr trial = mitcad::geometry::independent_copy(*body);
    CHECK(mitcad::io::reuse_triangulations(trial->occt(), {shown.front()->occt()}) == 88);
    CHECK(edgesWithoutPolygons(trial->occt()) == 0);
  }

  // The view's drawers (OcctViewer's setup): the stand-in is meshed finely
  // enough for them, and their presentations mesh nothing.
  mitcad::useViewMeshing();
  const occ::handle<Prs3d_Drawer> defaults = new Prs3d_Drawer();
  defaults->SetupOwnDefaults();
  mitcad::setViewTessellation(defaults);
  const occ::handle<Prs3d_Drawer> drawer = new Prs3d_Drawer();
  drawer->SetLink(defaults);
  if (standIn) {
    const std::size_t before = mitcad::io::presentation_faces_meshed();
    const auto meshes = triangulations(standIn->occt());
    CHECK(StdPrs_ToolTriangulatedShape::IsTessellated(standIn->occt(), drawer));
    StdPrs_ToolTriangulatedShape::Tessellate(standIn->occt(), drawer);
    CHECK(mitcad::io::presentation_faces_meshed() == before);
    CHECK(triangulations(standIn->occt()) == meshes);
  }


  // The same preview again: its stand-ins are there.
  const Preview again = preview(*document, extrudeRequest(sketch, 5.0));
  if (changedBody(*document, again.report) == body) {
    CHECK(again.meshing.bodies == 0);
    CHECK(again.meshing.ready == 2);
    // ... and handed back with the new preview, so that whoever keeps the
    // newest preview's stand-ins keeps these too (mitcad#636).
    CHECK(again.meshing.standIns.size() == 2);
    CHECK(std::find(again.meshing.standIns.begin(), again.meshing.standIns.end(), standIn) !=
          again.meshing.standIns.end());
  }

  // Refined: the final tier in the body and the tool themselves.
  const mitcad::DisplayShapes drawn{standIn, tool->display_stand_in()};
  const mitcad::DisplayShapes refinable = mitcad::refinableShapes(drawn);
  CHECK(refinable.size() == 2);
  mitcad::ModelJob job;
  const mitcad::DisplayMeshing refined = mitcad::meshForDisplay(refinable, job, mitcad::DisplayTier::Final);
  CHECK(refined.bodies == 2);
  CHECK(finalTier(body));
  CHECK(finalTier(tool));
  CHECK(edgesWithoutPolygons(body->occt()) == 0);
  CHECK(mitcad::viewMeshes(*body));
  if (standIn) {
    CHECK(!shareFacesOrMeshes(standIn->occt(), body->occt()));
  }
  CHECK(mitcad::refinableShapes(drawn).empty());
  CHECK(mitcad::io::mesh_stats(body->occt()).triangles == mitcad::io::mesh_stats(standIn->occt()).triangles);
}

// Committing a preview shown in the interactive tier (the command's job):
// the body the document keeps is meshed in the final tier, from the shown
// body's untouched faces and its own, never the stand-in's; undo and redo
// show final meshes only; an edit's preview is interactive again and its
// commit final.
void testCommitUndoEdit() {
  rust::Box<Document> document = mitcad::new_document();
  const QString sketch = makeFixture(*document, true);
  meshShown(*document);
  const QByteArray request = extrudeRequest(sketch, 5.0);
  const Preview p = preview(*document, request);
  const ShapePtr standIn = mitcad::displayShape(changedBody(*document, p.report));
  CHECK(standIn && standIn->display_interactive());
  mitcad::DisplayMeshing committed;
  const QJsonObject added = command(*document, QJsonDocument::fromJson(request).object(), &committed);
  const QString extrude = added.value("uid").toString();
  CHECK(!extrude.isEmpty());
  std::printf("Committed: %zu bodies meshed, %zu faces reused, %zu meshed\n", committed.bodies, committed.reused,
              committed.report.meshed);
  CHECK(committed.reused == 88);
  mitcad::DisplayShapes shown = mitcad::shownBodyShapes(*document);
  CHECK(shown.size() == 1);
  for (const ShapePtr& body : shown) {
    CHECK(finalTier(body));
    if (standIn) {
      CHECK(!shareFacesOrMeshes(standIn->occt(), body->occt()));
    }
  }

  for (const char* step : {"undo", "redo", "undo", "redo"}) {
    command(*document, QJsonObject{{"cmd", step}});
    for (const ShapePtr& body : mitcad::shownBodyShapes(*document)) {
      CHECK(finalTier(body));
    }
  }

  // An edit of the Extrude from the timeline, 8 mm.
  QJsonObject def = QJsonDocument::fromJson(extrudeRequest(sketch, 8.0)).object().value("def").toObject();
  const QJsonObject edit{{"cmd", "edit_feature"}, {"uid", extrude}, {"def", def}};
  const Preview edited = preview(*document, mitcad::compactJson(edit));
  CHECK(edited.report.value("status").toString() == QStringLiteral("ok"));
  const ShapePtr editedBody = changedBody(*document, edited.report);
  CHECK(editedBody && !anyTriangulation(editedBody->occt()) && editedBody->display_stand_in());
  std::printf("Edit preview: %zu faces reused, %zu meshed\n", edited.meshing.reused, edited.meshing.report.meshed);
  CHECK(edited.meshing.reused >= 88);
  command(*document, edit);
  for (const ShapePtr& body : mitcad::shownBodyShapes(*document)) {
    CHECK(finalTier(body));
  }
}

// A preview job cancelled while meshing: no stand-ins, the bodies left as
// they were (the preview itself is dropped); a face OCCT's mesher fails
// on leaves the stand-in incomplete, never meshed by the view, and its
// body's refinement leaves that face out as well (mitcad#589); stand-ins
// outlive their document safely, with nothing left to refine.
void testCancelFailureDestruction() {
  rust::Box<Document> document = mitcad::new_document();
  const QString sketch = makeFixture(*document, false);
  meshShown(*document);
  {
    mitcad::ModelJob job;
    QJsonObject report;
    {
      const mitcad::AttachedJob attached(*document, job);
      report = parseObject(document->preview(rustStr(extrudeRequest(sketch, 6.0))));
    }
    job.control().cancel();
    const mitcad::DisplayShapes shapes = mitcad::previewShapes(*document, report);
    const mitcad::DisplayMeshing meshing =
        mitcad::meshForDisplay(shapes, job, mitcad::DisplayTier::Interactive, mitcad::previewDonors(*document, report));
    CHECK(meshing.bodies == 0);
    CHECK(meshing.standIns.empty());
    for (const ShapePtr& shape : shapes) {
      CHECK(!shape->display_stand_in());
      CHECK(!anyTriangulation(shape->occt()));
      CHECK(!shape->display_meshed());
    }
  }

  // A plane bounded by a wire that crosses itself, in a body of its own.
  BRepBuilderAPI_MakePolygon bowTie(gp_Pnt(0, 0, 0), gp_Pnt(10, 10, 0), gp_Pnt(10, 0, 0), gp_Pnt(0, 10, 0), true);
  const auto crossed =
      std::make_shared<mitcad::geometry::Shape>(BRepBuilderAPI_MakeFace(gp_Pln(), bowTie.Wire(), true).Shape());
  {
    mitcad::ModelJob job;
    const mitcad::DisplayMeshing meshing = mitcad::meshForDisplay({crossed}, job, mitcad::DisplayTier::Interactive);
    CHECK(meshing.incomplete == 1);
    const ShapePtr standIn = crossed->display_stand_in();
    CHECK(standIn && standIn->display_meshed() && !mitcad::viewMeshes(*standIn));
    const mitcad::DisplayMeshing refined =
        mitcad::meshForDisplay(mitcad::refinableShapes({standIn}), job, mitcad::DisplayTier::Final);
    CHECK(refined.incomplete == 1);
    CHECK(crossed->display_meshed() && !mitcad::viewMeshes(*crossed));
    CHECK(!crossed->display_stand_in());
  }

  // The document goes while a stand-in is drawn.
  Preview p = preview(*document, extrudeRequest(sketch, 7.0));
  const ShapePtr standIn = p.meshing.standIns.empty() ? nullptr : p.meshing.standIns.front();
  CHECK(standIn != nullptr);
  p = Preview();
  document = mitcad::new_document();
  if (standIn) {
    CHECK(standIn->stand_in_for() == nullptr);
    CHECK(mitcad::refinableShapes({standIn}).empty());
    CHECK(mitcad::io::has_triangulation(standIn->occt()));
  }
}

// A stand-in for a face whose triangulation does not end (mitcad#846, io::
// use_face_algorithms_for_tests): a cylinder's face loops until released,
// never testing the stop, then leaves a triangulation in the face it was
// given; other faces are triangulated by the inner factory's algorithm and
// counted. Installed for Delabella's planes too
// (use_fast_plane_algorithms_for_tests), as the final tier's planes go
// through them.
struct Endless {
  std::atomic<bool> entered{false};
  std::atomic<bool> release{false};
  std::atomic<bool> returned{false};
  std::atomic<int> others{0};
  // Set by a static destructor while the face loops (abandonFaceAtExit):
  // the face then aborts, as one inside OCCT crashes when the static
  // destructors tear OCCT's state down under it (mitcad#936).
  std::atomic<bool> tornDown{false};
};

class EndlessFace : public IMeshTools_MeshAlgo {
public:
  EndlessFace(Endless& state, occ::handle<IMeshTools_MeshAlgo> inner, bool endless)
      : m_state(state), m_inner(std::move(inner)), m_endless(endless) {}

  void Perform(const IMeshData::IFaceHandle& face, const IMeshTools_Parameters& parameters,
               const Message_ProgressRange& range) override {
    if (!m_endless) {
      m_inner->Perform(face, parameters, range);
      ++m_state.others;
      return;
    }
    m_state.entered = true;
    while (!m_state.release) {
      if (m_state.tornDown) {
        std::fprintf(stderr, "the endless face's state was torn down under it\n");
        std::abort();
      }
      std::this_thread::sleep_for(std::chrono::milliseconds(5));
    }
    m_inner->Perform(face, parameters, Message_ProgressRange());
    m_state.returned = true;
  }

private:
  Endless& m_state;
  occ::handle<IMeshTools_MeshAlgo> m_inner;
  bool m_endless;
};

class EndlessCylinders : public IMeshTools_MeshAlgoFactory {
public:
  EndlessCylinders(Endless& state, occ::handle<IMeshTools_MeshAlgoFactory> inner)
      : m_state(state), m_inner(std::move(inner)) {}

  occ::handle<IMeshTools_MeshAlgo> GetAlgo(const GeomAbs_SurfaceType type,
                                           const IMeshTools_Parameters& parameters) const override {
    return new EndlessFace(m_state, m_inner->GetAlgo(type, parameters), type == GeomAbs_Cylinder);
  }

private:
  Endless& m_state;
  occ::handle<IMeshTools_MeshAlgoFactory> m_inner;
};

// A display meshing (the final tier, as opening a design meshes it)
// cancelled while one face's triangulation does not end (mitcad#846):
// meshForDisplay returns within a few seconds of the cancel, that face
// abandoned; the body is marked as meshed to the end, so the view draws the
// faces it has and never meshes the body itself; the abandoned face, once
// it ends, writes nothing into the body.
void testCancelEndlessFace() {
  Endless state;
  mitcad::io::use_face_algorithms_for_tests(new EndlessCylinders(state, new BRepMesh_MeshAlgoFactory()));
  mitcad::io::use_fast_plane_algorithms_for_tests(
      new EndlessCylinders(state, new BRepMesh_DelabellaMeshAlgoFactory()));
  const auto body = std::make_shared<mitcad::geometry::Shape>(BRepPrimAPI_MakeCylinder(10.0, 30.0).Shape());
  mitcad::ModelJob job;
  // Cancel as the window's Cancel does, once the endless face is under way
  // and the two planes are done.
  std::thread cancel([&state, &job] {
    for (int i = 0; i < 6000 && !(state.entered && state.others >= 2); ++i) {
      std::this_thread::sleep_for(std::chrono::milliseconds(5));
    }
    job.control().cancel();
  });
  const auto start = std::chrono::steady_clock::now();
  const mitcad::DisplayMeshing meshing = mitcad::meshForDisplay({body}, job, mitcad::DisplayTier::Final);
  const double seconds = std::chrono::duration<double>(std::chrono::steady_clock::now() - start).count();
  cancel.join();
  mitcad::io::use_face_algorithms_for_tests(occ::handle<IMeshTools_MeshAlgoFactory>());
  mitcad::io::use_fast_plane_algorithms_for_tests(occ::handle<IMeshTools_MeshAlgoFactory>());
  mitcad::logDisplayMeshing(meshing, QStringLiteral("endless face"));
  std::printf("display meshing with an endless face cancelled: %.2f s; %s\n", seconds,
              meshing.report.line().c_str());
  CHECK(state.entered);
  CHECK(seconds < mitcad::io::kMeshAbandonSeconds + 5.0);
  CHECK(meshing.report.abandoned == 1);
  CHECK(mitcad::io::abandoned_faces() >= 1);
  CHECK(meshing.incomplete == 1);
  CHECK(body->display_meshed());
  CHECK(!mitcad::viewMeshes(*body));
  int planes = 0;
  TopoDS_Face round;
  for (TopExp_Explorer it(body->occt(), TopAbs_FACE); it.More(); it.Next()) {
    const TopoDS_Face& face = TopoDS::Face(it.Current());
    TopLoc_Location location;
    if (BRepAdaptor_Surface(face, false).GetType() == GeomAbs_Cylinder) {
      round = face;
      CHECK(BRep_Tool::Triangulation(face, location).IsNull());
    } else {
      planes += BRep_Tool::Triangulation(face, location).IsNull() ? 0 : 1;
    }
  }
  CHECK(planes == 2);

  state.release = true;
  for (int i = 0; i < 2000 && !state.returned; ++i) {
    std::this_thread::sleep_for(std::chrono::milliseconds(5));
  }
  CHECK(state.returned);
  // Its worker leaves the process's abandoned faces once it ends
  // (mitcad#936).
  for (int i = 0; i < 2000 && mitcad::io::abandoned_faces() > 0; ++i) {
    std::this_thread::sleep_for(std::chrono::milliseconds(5));
  }
  CHECK(mitcad::io::abandoned_faces() == 0);
  std::this_thread::sleep_for(std::chrono::milliseconds(100));
  TopLoc_Location location;
  CHECK(!round.IsNull() && BRep_Tool::Triangulation(round, location).IsNull());

  // The next display meshing is as before.
  const auto next = std::make_shared<mitcad::geometry::Shape>(BRepPrimAPI_MakeCylinder(10.0, 30.0).Shape());
  mitcad::ModelJob again;
  const mitcad::DisplayMeshing meshed = mitcad::meshForDisplay({next}, again, mitcad::DisplayTier::Final);
  CHECK(meshed.bodies == 1 && meshed.incomplete == 0 && meshed.report.abandoned == 0);
  CHECK(mitcad::io::has_triangulation(next->occt()));
}

// The endless face's state in abandonFaceAtExit: static, as OCCT's own
// state is, so that the static destructors at the end of a process that
// runs them tear it down under the face, which never ends.
struct StaticEndless {
  Endless state;
  StaticEndless() = default;
  StaticEndless(const StaticEndless&) = delete;
  StaticEndless& operator=(const StaticEndless&) = delete;
  ~StaticEndless() {
    state.tornDown = true;
    // Time for the face to see it, as the process's end would give a
    // thread inside OCCT.
    std::this_thread::sleep_for(std::chrono::milliseconds(500));
  }
};

// The process of testAbandonedFaceAtExit (--abandon-at-exit): the crash
// reports and OCCT's handlers as the application installs them, then
// testCancelEndlessFace's display meshing, cancelled while the face does
// not end, which is never released. main then ends the process as the
// application's does (framework/ProcessEnd.hpp) once this has returned and
// its QCoreApplication is gone. Returns 1 when a check failed.
int abandonFaceAtExit(int argc, char** argv) {
  mitcad::crash::install("app", "test");
  mitcad::geometry::catch_occt_crashes();
  QCoreApplication app(argc, argv);
  static StaticEndless held;
  Endless& state = held.state;
  mitcad::io::use_face_algorithms_for_tests(new EndlessCylinders(state, new BRepMesh_MeshAlgoFactory()));
  mitcad::io::use_fast_plane_algorithms_for_tests(
      new EndlessCylinders(state, new BRepMesh_DelabellaMeshAlgoFactory()));
  const auto body = std::make_shared<mitcad::geometry::Shape>(BRepPrimAPI_MakeCylinder(10.0, 30.0).Shape());
  mitcad::ModelJob job;
  std::thread cancel([&job] {
    for (int i = 0; i < 6000 && !(state.entered && state.others >= 2); ++i) {
      std::this_thread::sleep_for(std::chrono::milliseconds(5));
    }
    job.control().cancel();
  });
  const mitcad::DisplayMeshing meshing = mitcad::meshForDisplay({body}, job, mitcad::DisplayTier::Final);
  cancel.join();
  mitcad::io::use_face_algorithms_for_tests(occ::handle<IMeshTools_MeshAlgoFactory>());
  mitcad::io::use_fast_plane_algorithms_for_tests(occ::handle<IMeshTools_MeshAlgoFactory>());
  std::printf("display meshing with an endless face cancelled, never released: %s\n",
              meshing.report.line().c_str());
  CHECK(state.entered);
  CHECK(!state.returned);
  CHECK(meshing.report.abandoned == 1);
  CHECK(mitcad::io::abandoned_faces() == 1);
  std::fflush(stdout);
  return failures > 0 ? 1 : 0;
}

// A display meshing cancelled while a face does not end, in a process that
// then quits without the face ever ending (mitcad#936): the process ends
// with its own exit code 0 and leaves no crash report in MITCAD_CRASH_DIR.
// Its static destructors would run under the face (the stand-in's state
// torn down under it aborts it, as OCCT's does to a face inside OCCT);
// the process ends without them.
void testAbandonedFaceAtExit() {
  QTemporaryDir crashes;
  CHECK(crashes.isValid());
  QProcessEnvironment environment = QProcessEnvironment::systemEnvironment();
  environment.insert(QStringLiteral("MITCAD_CRASH_DIR"), crashes.path());
  QProcess child;
  child.setProcessEnvironment(environment);
  child.setProcessChannelMode(QProcess::ForwardedChannels);
  child.start(QCoreApplication::applicationFilePath(), QStringList{QStringLiteral("--abandon-at-exit")});
  CHECK(child.waitForStarted(30000));
  const bool finished = child.waitForFinished(120000);
  CHECK(finished);
  if (!finished) {
    child.kill();
    child.waitForFinished(10000);
  }
  const QStringList reports =
      QDir(crashes.path()).entryList(QStringList{QStringLiteral("*.crash")}, QDir::Files);
  std::printf("process with an abandoned face at its end: %s, exit code %d, %d crash report(s)\n",
              child.exitStatus() == QProcess::NormalExit ? "ended" : "crashed", child.exitCode(),
              static_cast<int>(reports.size()));
  CHECK(child.exitStatus() == QProcess::NormalExit);
  CHECK(child.exitCode() == 0);
  CHECK(reports.isEmpty());
}

// The triangles of the shown bodies' faces.
std::size_t shownTriangles(const Document& document) {
  std::size_t triangles = 0;
  for (const ShapePtr& body : mitcad::shownBodyShapes(document)) {
    triangles += mitcad::io::mesh_stats(body->occt()).triangles;
  }
  return triangles;
}

// A design opened twice with the result store (mitcad#858), as the window
// opens it (the recompute, its results stored, the shown bodies meshed
// with the store): the second open takes the body's triangulations from
// the store and meshes no face, with the same triangles, which the view's
// drawers keep; the block's state the cut replaced is never unpacked. A
// body changed since (another cut) is meshed again, and stored.
void testWarmOpen() {
  const QString dir =
      QDir::temp().filePath(QStringLiteral("mitcad-warm-open-%1").arg(QCoreApplication::applicationPid()));
  QDir(dir).removeRecursively();
  const QByteArray store = dir.toUtf8();
  rust::Box<Document> design = mitcad::new_document();
  makeFixture(*design, true);
  const auto open = [&store](const std::string& json, QJsonObject* recomputed) {
    rust::Box<Document> document = mitcad::load_document(rust::Str(json));
    document->set_result_store(rustStr(store), rustStr(QByteArrayLiteral("warm open test")),
                               rustStr(QByteArrayLiteral("warm.mitcad")));
    *recomputed = run(*document, QJsonObject{{"cmd", "recompute"}});
    document->persist_results(0.0);
    return document;
  };
  const std::string json = std::string(design->to_json());

  QJsonObject recomputed;
  rust::Box<Document> cold = open(json, &recomputed);
  mitcad::ModelJob first;
  const mitcad::DisplayMeshing made = mitcad::meshOpenedForDisplay(*cold, first, true);
  mitcad::logDisplayMeshing(made, QStringLiteral("cold open"));
  CHECK(made.restored == 0);
  CHECK(made.bodies == 1 && made.report.meshed > 0);
  CHECK(made.stored == 1 && made.storedBytes > 0 && made.storeErrors.isEmpty());
  const std::size_t triangles = shownTriangles(*cold);
  CHECK(triangles > 0);

  rust::Box<Document> warm = open(json, &recomputed);
  // The block's extrusion and the holes' cut from the store; the block's
  // state, which the cut replaced, stays packed (mitcad#858).
  CHECK(recomputed.value("restored").toInt() == 2);
  CHECK(recomputed.value("restored_packed").toInt() == 1);
  mitcad::ModelJob second;
  const mitcad::DisplayMeshing taken = mitcad::meshOpenedForDisplay(*warm, second, true);
  mitcad::logDisplayMeshing(taken, QStringLiteral("warm open"));
  std::printf("Warm open: %zu bodies, %zu faces from the result store in %lld ms; %zu faces meshed\n",
              taken.restored, taken.restoredFaces, static_cast<long long>(taken.restoreMilliseconds),
              taken.report.meshed);
  // Every face of the block with its 83 holes (6 + 83 = 89; the fixture's
  // 94 of testFixture include the joined boss, which is not made here).
  const mitcad::DisplayShapes shown = mitcad::shownBodyShapes(*warm);
  CHECK(shown.size() == 1);
  CHECK(!shown.empty() && countOf(shown.front()->occt(), TopAbs_FACE) == 89);
  CHECK(taken.restored == 1 && taken.restoredFaces == 89);
  CHECK(taken.bodies == 0 && taken.report.meshed == 0);
  CHECK(taken.stored == 0);
  CHECK(shownTriangles(*warm) == triangles);
  mitcad::useViewMeshing();
  const occ::handle<Prs3d_Drawer> defaults = new Prs3d_Drawer();
  defaults->SetupOwnDefaults();
  mitcad::setViewTessellation(defaults);
  const occ::handle<Prs3d_Drawer> drawer = new Prs3d_Drawer();
  drawer->SetLink(defaults);
  for (const ShapePtr& body : shown) {
    CHECK(finalTier(body));
    CHECK(mitcad::viewMeshes(*body));
    // Meshed to the end, as one meshed on the open: the view's drawers
    // keep its triangulations without running the mesher (mitcad#878).
    CHECK(body->display_meshed());
    CHECK(!mitcad::drawerMeshes(*body));
    const std::size_t before = mitcad::io::presentation_faces_meshed();
    CHECK(StdPrs_ToolTriangulatedShape::IsTessellated(body->occt(), drawer));
    StdPrs_ToolTriangulatedShape::Tessellate(body->occt(), drawer);
    CHECK(mitcad::io::presentation_faces_meshed() == before);
  }

  // Another hole through the block: the body's shape is another, meshed
  // and stored again.
  const QString drill = run(*design, QJsonObject{{"cmd", "sketch.create"}}).value("uid").toString();
  const QJsonObject circle = run(*design, QJsonObject{{"cmd", "sketch.add_circle"},
                                                      {"sketch", drill},
                                                      {"center", QJsonArray{60, 50}},
                                                      {"diameter", 6}});
  run(*design, QJsonObject{{"cmd", "add_feature"},
                           {"def", QJsonObject{{"type", "extrude"},
                                               {"profiles", QJsonArray{profile(drill, circle.value("region").toString())}},
                                               {"start", QJsonObject{{"type", "offset"}, {"offset", -5}}},
                                               {"extent", QJsonObject{{"type", "distance"}, {"distance", 30}}},
                                               {"operation", "cut"}}}});
  rust::Box<Document> changed = open(std::string(design->to_json()), &recomputed);
  mitcad::ModelJob third;
  const mitcad::DisplayMeshing again = mitcad::meshOpenedForDisplay(*changed, third, true);
  mitcad::logDisplayMeshing(again, QStringLiteral("changed open"));
  CHECK(again.restored == 0);
  CHECK(again.bodies == 1 && again.report.meshed > 0);
  CHECK(again.stored == 1);
  QDir(dir).removeRecursively();
}

// The display mesh files (.mms) in the store's folder.
int storedMeshFiles(const QString& dir) {
  int files = 0;
  QDirIterator it(dir, QStringList{QStringLiteral("*.mms")}, QDir::Files | QDir::Hidden, QDirIterator::Subdirectories);
  while (it.hasNext()) {
    it.next();
    ++files;
  }
  return files;
}

// The display meshes stored after the open (mitcad#880), by a writer made
// stopped so that the open and what follows it run before it writes:
// - the open only queues the body (nothing stored in it, no file yet);
// - the design changed (another hole) and closed before the writer starts
//   changes nothing: the file written is of the body as opened, and the
//   next open of the file takes every face from it, meshing none;
// - the design opened again while its meshes are still queued waits for
//   them and finds them;
// - drop() (the store cleared, the program quitting) writes nothing, and
//   the next open meshes the body and stores it again; a file cut short
//   (written by no one, as a whole file is renamed into place) is not
//   used, and the body is meshed and stored again.
void testDeferredWrite() {
  const QString dir =
      QDir::temp().filePath(QStringLiteral("mitcad-deferred-write-%1").arg(QCoreApplication::applicationPid()));
  QDir(dir).removeRecursively();
  const QByteArray store = dir.toUtf8();
  const QString label = QStringLiteral("deferred.mitcad");
  rust::Box<Document> design = mitcad::new_document();
  makeFixture(*design, true);
  const std::string json = std::string(design->to_json());
  const auto open = [&store, &json]() {
    rust::Box<Document> document = mitcad::load_document(rust::Str(json));
    document->set_result_store(rustStr(store), rustStr(QByteArrayLiteral("deferred write test")),
                               rustStr(QByteArrayLiteral("deferred.mitcad")));
    run(*document, QJsonObject{{"cmd", "recompute"}});
    document->persist_results(0.0);
    return document;
  };

  std::size_t triangles = 0;
  {
    mitcad::DisplayMeshWrites writes(false);
    {
      rust::Box<Document> cold = open();
      mitcad::ModelJob first;
      const mitcad::DisplayMeshing made = mitcad::meshOpenedForDisplay(*cold, first, true, &writes, label);
      mitcad::logDisplayMeshing(made, QStringLiteral("deferred cold open"));
      CHECK(made.bodies == 1 && made.report.meshed > 0);
      CHECK(made.stored == 0 && made.deferred == 1 && made.storeErrors.isEmpty());
      CHECK(writes.pending() == 1);
      CHECK(storedMeshFiles(dir) == 0);
      triangles = shownTriangles(*cold);
      CHECK(triangles > 0);
      // Changed: another hole through the block, the body's shape another.
      const QString drill = run(*cold, QJsonObject{{"cmd", "sketch.create"}}).value("uid").toString();
      const QJsonObject circle = run(*cold, QJsonObject{{"cmd", "sketch.add_circle"},
                                                        {"sketch", drill},
                                                        {"center", QJsonArray{60, 50}},
                                                        {"diameter", 6}});
      run(*cold, QJsonObject{{"cmd", "add_feature"},
                             {"def", QJsonObject{{"type", "extrude"},
                                                 {"profiles", QJsonArray{profile(drill, circle.value("region").toString())}},
                                                 {"start", QJsonObject{{"type", "offset"}, {"offset", -5}}},
                                                 {"extent", QJsonObject{{"type", "distance"}, {"distance", 30}}},
                                                 {"operation", "cut"}}}});
      CHECK(mitcad::shownBodyShapes(*cold).size() == 1);
      // ... and closed, still before the writer starts.
    }
    CHECK(writes.pending() == 1);
    writes.wait(label);
    CHECK(writes.pending() == 0);
    CHECK(storedMeshFiles(dir) == 1);
  }
  {
    rust::Box<Document> warm = open();
    mitcad::ModelJob second;
    const mitcad::DisplayMeshing taken = mitcad::meshOpenedForDisplay(*warm, second, true);
    mitcad::logDisplayMeshing(taken, QStringLiteral("deferred warm open"));
    CHECK(taken.restored == 1 && taken.restoredFaces == 89);
    CHECK(taken.bodies == 0 && taken.report.meshed == 0);
    CHECK(shownTriangles(*warm) == triangles);
  }

  // Opened again while its meshes are still queued: the open waits for them.
  QDir(dir).removeRecursively();
  {
    mitcad::DisplayMeshWrites writes(false);
    rust::Box<Document> cold = open();
    mitcad::ModelJob first;
    const mitcad::DisplayMeshing made = mitcad::meshOpenedForDisplay(*cold, first, true, &writes, label);
    CHECK(made.deferred == 1 && storedMeshFiles(dir) == 0);
    rust::Box<Document> again = open();
    mitcad::ModelJob second;
    const mitcad::DisplayMeshing taken = mitcad::meshOpenedForDisplay(*again, second, true, &writes, label);
    mitcad::logDisplayMeshing(taken, QStringLiteral("deferred open again"));
    CHECK(taken.restored == 1 && taken.report.meshed == 0 && taken.deferred == 0);
    CHECK(writes.pending() == 0 && storedMeshFiles(dir) == 1);
  }

  // Dropped: nothing written; the next open meshes and stores the body.
  QDir(dir).removeRecursively();
  {
    mitcad::DisplayMeshWrites writes(false);
    rust::Box<Document> cold = open();
    mitcad::ModelJob first;
    const mitcad::DisplayMeshing made = mitcad::meshOpenedForDisplay(*cold, first, true, &writes, label);
    CHECK(made.deferred == 1);
    CHECK(writes.drop() == 1);
    CHECK(writes.pending() == 0 && storedMeshFiles(dir) == 0);
  }
  QString written;
  {
    rust::Box<Document> next = open();
    mitcad::ModelJob job;
    const mitcad::DisplayMeshing meshed = mitcad::meshOpenedForDisplay(*next, job, true);
    CHECK(meshed.restored == 0 && meshed.bodies == 1 && meshed.stored == 1);
    QDirIterator it(dir, QStringList{QStringLiteral("*.mms")}, QDir::Files, QDirIterator::Subdirectories);
    if (it.hasNext()) {
      written = it.next();
    }
  }
  // A file cut short: not used, the body meshed and stored again.
  QFile file(written);
  CHECK(!written.isEmpty() && file.open(QIODevice::ReadWrite) && file.resize(file.size() / 2));
  file.close();
  {
    rust::Box<Document> next = open();
    mitcad::ModelJob job;
    const mitcad::DisplayMeshing meshed = mitcad::meshOpenedForDisplay(*next, job, true);
    CHECK(meshed.restored == 0 && meshed.bodies == 1 && meshed.stored == 1);
  }
  QDir(dir).removeRecursively();
}

// Two bodies (mitcad#881): a 120 x 100 x 20 mm block, final after its own
// extrusion at the start, and a 50 x 50 x 20 mm block beside it that a
// later cut (by name) holes, final only after the last feature.
std::string twoBodies() {
  rust::Box<Document> design = mitcad::new_document();
  const QString first = run(*design, QJsonObject{{"cmd", "sketch.create"}}).value("uid").toString();
  run(*design, rectangle(first, 0, 0, 120, 100));
  const QString block = run(*design, QJsonObject{{"cmd", "add_feature"},
                                                 {"def", QJsonObject{{"type", "extrude"},
                                                                     {"profiles", QJsonArray{profile(first, kRectangle)}},
                                                                     {"extent", QJsonObject{{"type", "distance"},
                                                                                            {"distance", 20}}},
                                                                     {"operation", "new_body"}}}})
                            .value("uid")
                            .toString();
  CHECK(!block.isEmpty());
  const QString second = run(*design, QJsonObject{{"cmd", "sketch.create"}}).value("uid").toString();
  run(*design, rectangle(second, 200, 0, 50, 50));
  const QString other = run(*design, QJsonObject{{"cmd", "add_feature"},
                                                 {"def", QJsonObject{{"type", "extrude"},
                                                                     {"profiles", QJsonArray{profile(second, kRectangle)}},
                                                                     {"extent", QJsonObject{{"type", "distance"},
                                                                                            {"distance", 20}}},
                                                                     {"operation", "new_body"}}}})
                            .value("uid")
                            .toString();
  const QString drill = run(*design, QJsonObject{{"cmd", "sketch.create"}}).value("uid").toString();
  const QJsonObject circle = run(*design, QJsonObject{{"cmd", "sketch.add_circle"},
                                                      {"sketch", drill},
                                                      {"center", QJsonArray{225, 25}},
                                                      {"diameter", 6}});
  run(*design, QJsonObject{{"cmd", "add_feature"},
                           {"def", QJsonObject{{"type", "extrude"},
                                               {"profiles", QJsonArray{profile(drill, circle.value("region").toString())}},
                                               {"start", QJsonObject{{"type", "offset"}, {"offset", -5}}},
                                               {"extent", QJsonObject{{"type", "distance"}, {"distance", 30}}},
                                               {"operation", "cut"},
                                               {"participants", QJsonArray{other + QStringLiteral(".b0")}}}}});
  CHECK(mitcad::shownBodyShapes(*design).size() == 2);
  return std::string(design->to_json());
}

// Waits until the meshing beside the recompute is done with every body the
// recompute put in (or 60 s went by).
bool waitDone(const mitcad::RecomputeMeshing& during) {
  const auto until = std::chrono::steady_clock::now() + std::chrono::seconds(60);
  while (during.doneSoFar() + during.leftToStore() < during.given()) {
    if (std::chrono::steady_clock::now() > until) {
      return false;
    }
    std::this_thread::sleep_for(std::chrono::milliseconds(10));
  }
  return true;
}

// The bodies of a batch line of RecomputeMeshing::trace, summed.
std::size_t tracedBodies(const QStringList& trace) {
  std::size_t bodies = 0;
  for (const QString& line : trace) {
    bodies += static_cast<std::size_t>(line.section(QLatin1Char(' '), 2, 2).toULongLong());
  }
  return bodies;
}

// Display meshes made while the recompute goes on (mitcad#881), as the
// window's open makes them:
// - each body goes in once, at its last version (the block after its
//   extrusion, the other after the cut that names it), is meshed once
//   beside the recompute, and takes those meshes: nothing is meshed after
//   the recompute, and the triangles are those of an open without it;
// - a body changed after it was meshed (as a feature the classification
//   missed would) is meshed again after the recompute, its stale meshes
//   left;
// - the job cancelled: nothing is meshed beside it, the open meshes
//   nothing either (as a cancelled open), and the meshing stops at once;
// - the meshing gone before the recompute (the design closed) and the
//   recompute then run: nothing breaks, the feed is closed.
void testMeshDuringRecompute() {
  const std::string json = twoBodies();
  std::size_t triangles = 0;
  {
    rust::Box<Document> plain = mitcad::load_document(rust::Str(json));
    run(*plain, QJsonObject{{"cmd", "recompute"}});
    mitcad::ModelJob job;
    const mitcad::DisplayMeshing meshing = mitcad::meshOpenedForDisplay(*plain, job, false);
    CHECK(meshing.bodies == 2 && meshing.during == 0);
    triangles = shownTriangles(*plain);
    CHECK(triangles > 0);
  }
  {
    rust::Box<Document> document = mitcad::load_document(rust::Str(json));
    mitcad::ModelJob job;
    const std::unique_ptr<mitcad::RecomputeMeshing> during = mitcad::RecomputeMeshing::start(*document, job);
    CHECK(during != nullptr);
    {
      const mitcad::AttachedJob attached(*document, job);
      run(*document, QJsonObject{{"cmd", "recompute"}});
    }
    CHECK(waitDone(*during));
    const mitcad::DisplayMeshing meshing =
        mitcad::meshOpenedForDisplay(*document, job, false, nullptr, QStringLiteral("during.mitcad"), during.get());
    mitcad::logDisplayMeshing(meshing, QStringLiteral("during.mitcad"));
    CHECK(during->given() == 2 && during->changedLater() == 0);
    CHECK(tracedBodies(meshing.duringTrace) == 2);
    CHECK(meshing.during == 2 && meshing.duringStale == 0 && meshing.duringLeft == 0);
    CHECK(meshing.bodies == 0 && meshing.incomplete == 0);
    CHECK(meshing.shown == 2);
    CHECK(shownTriangles(*document) == triangles);
    for (const ShapePtr& body : mitcad::shownBodyShapes(*document)) {
      CHECK(body->display_meshed());
      CHECK(finalTier(body));
      CHECK(edgesWithoutPolygons(body->occt()) == 0);
    }
  }
  {
    rust::Box<Document> document = mitcad::load_document(rust::Str(json));
    mitcad::ModelJob job;
    const std::unique_ptr<mitcad::RecomputeMeshing> during = mitcad::RecomputeMeshing::start(*document, job);
    run(*document, QJsonObject{{"cmd", "recompute"}});
    CHECK(waitDone(*during));
    // The block changed after it was meshed: another hole through it.
    const QString drill = run(*document, QJsonObject{{"cmd", "sketch.create"}}).value("uid").toString();
    const QJsonObject circle = run(*document, QJsonObject{{"cmd", "sketch.add_circle"},
                                                          {"sketch", drill},
                                                          {"center", QJsonArray{60, 50}},
                                                          {"diameter", 6}});
    run(*document, QJsonObject{{"cmd", "add_feature"},
                               {"def", QJsonObject{{"type", "extrude"},
                                                   {"profiles", QJsonArray{profile(drill, circle.value("region").toString())}},
                                                   {"start", QJsonObject{{"type", "offset"}, {"offset", -5}}},
                                                   {"extent", QJsonObject{{"type", "distance"}, {"distance", 30}}},
                                                   {"operation", "cut"},
                                                   {"participants", QJsonArray{QStringLiteral("F2.b0")}}}}});
    const mitcad::DisplayMeshing meshing =
        mitcad::meshOpenedForDisplay(*document, job, false, nullptr, QStringLiteral("changed.mitcad"), during.get());
    mitcad::logDisplayMeshing(meshing, QStringLiteral("changed.mitcad"));
    CHECK(meshing.during == 1 && meshing.duringStale == 1);
    CHECK(meshing.bodies == 1);
    for (const ShapePtr& body : mitcad::shownBodyShapes(*document)) {
      CHECK(body->display_meshed() && mitcad::io::has_triangulation(body->occt()));
    }
  }
  {
    rust::Box<Document> document = mitcad::load_document(rust::Str(json));
    mitcad::ModelJob job;
    const std::unique_ptr<mitcad::RecomputeMeshing> during = mitcad::RecomputeMeshing::start(*document, job);
    job.control().cancel();
    run(*document, QJsonObject{{"cmd", "recompute"}});
    const auto started = std::chrono::steady_clock::now();
    const mitcad::DisplayMeshing meshing =
        mitcad::meshOpenedForDisplay(*document, job, false, nullptr, QStringLiteral("cancelled.mitcad"), during.get());
    const double seconds = std::chrono::duration<double>(std::chrono::steady_clock::now() - started).count();
    CHECK(seconds < mitcad::io::kMeshAbandonSeconds + 5.0);
    CHECK(meshing.during == 0 && meshing.bodies == 0);
    CHECK(meshing.duringLeft == during->given());
  }
  {
    rust::Box<Document> document = mitcad::load_document(rust::Str(json));
    mitcad::ModelJob job;
    mitcad::RecomputeMeshing::start(*document, job).reset();
    run(*document, QJsonObject{{"cmd", "recompute"}});
    CHECK(mitcad::shownBodyShapes(*document).size() == 2);
  }
}
} // namespace

int main(int argc, char** argv) {
  if (argc > 1 && std::strcmp(argv[1], "--abandon-at-exit") == 0) {
    return mitcad::endProcess(abandonFaceAtExit(argc, argv));
  }
  QCoreApplication app(argc, argv);
  bool benchmark = false;
  bool comparison = false;
  bool pairs = false;
  mitcad::DisplayTier tier = mitcad::DisplayTier::Interactive;
  bool holes = true;
  int samples = 20;
  for (int i = 1; i < argc; ++i) {
    if (std::strcmp(argv[i], "--bench") == 0) {
      benchmark = true;
    } else if (std::strcmp(argv[i], "--compare") == 0) {
      comparison = true;
    } else if (std::strcmp(argv[i], "--paired") == 0) {
      pairs = true;
    } else if (std::strcmp(argv[i], "--final") == 0) {
      tier = mitcad::DisplayTier::Final;
    } else if (std::strcmp(argv[i], "--plain") == 0) {
      holes = false;
    } else if (std::strcmp(argv[i], "--samples") == 0 && i + 1 < argc) {
      samples = std::max(1, std::atoi(argv[++i]));
    }
  }
  if (comparison) {
    return compare(holes, samples);
  }
  if (pairs) {
    return paired(holes, samples);
  }
  if (benchmark) {
    return bench(holes, samples, tier);
  }
  testFixture();
  testInteractiveTier();
  testCommitUndoEdit();
  testCancelFailureDestruction();
  testCancelEndlessFace();
  testAbandonedFaceAtExit();
  testWarmOpen();
  testDeferredWrite();
  testMeshDuringRecompute();
  if (failures > 0) {
    std::fprintf(stderr, "%d check(s) failed\n", failures);
    return 1;
  }
  std::printf("preview meshing: all checks passed\n");
  return 0;
}
