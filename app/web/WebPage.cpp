// SPDX-License-Identifier: MIT
// A design as one web page (web/WebPage.hpp): the visible bodies as placed,
// meshed on copies of their shapes, with their appearances, as the
// viewer's glb (web/WebGlb.hpp) and data, inlined into the viewer's page.
#include "web/WebPage.hpp"

#include <algorithm>
#include <array>
#include <cmath>
#include <exception>
#include <limits>
#include <map>
#include <memory>
#include <utility>
#include <vector>

#include <QCoreApplication>
#include <QDateTime>
#include <QDir>
#include <QFileInfo>
#include <QHash>
#include <QJsonArray>
#include <QJsonDocument>
#include <QObject>
#include <QSaveFile>

#include <Standard_Failure.hxx>
#include <TopoDS_Shape.hxx>

#include "framework/Appearances.hpp"
#include "framework/Command.hpp"
#include "framework/Json.hpp"
#include "framework/ResultCache.hpp"
#include "mitcad/io/mesh.hpp"
#include "mitcad_bridge/lib.h"
#include "render/RenderBatch.hpp"
#include "render/RenderMesh.hpp"
#include "rust/cxx.h"
#include "web/WebGlb.hpp"

namespace mitcad::web {

namespace {

// A doubly curved face gets about this many triangles at most (as the
// export's Medium refinement, io::MeshOptions): a large dome meshes a
// little coarser rather than taking millions of triangles.
constexpr std::size_t kFaceTriangles = 20000;

// The units the viewer knows, with their size in mm.
struct Unit {
  const char* name;
  double millimetres;
};
constexpr std::array<Unit, 5> kUnits{
    {{"mm", 1.0}, {"cm", 10.0}, {"m", 1000.0}, {"in", 25.4}, {"ft", 304.8}}};

// The `instances` query's path: an array of occurrence uids or a text.
QString pathText(const QJsonValue& value) {
  if (value.isString()) {
    return value.toString();
  }
  QStringList uids;
  for (const QJsonValue& uid : value.toArray()) {
    uids << uid.toString();
  }
  return uids.join(QLatin1Char('/'));
}

// A placement of the model's queries (3 or 4 rows of 4 numbers) as a
// row-major 3 x 4 matrix.
std::array<double, 12> transformOf(const QJsonArray& rows) {
  std::array<double, 12> m{1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0};
  if (rows.size() < 3) {
    return m;
  }
  for (int r = 0; r < 3; ++r) {
    const QJsonArray row = rows.at(r).toArray();
    for (int c = 0; c < 4; ++c) {
      m[static_cast<std::size_t>(r * 4 + c)] = row.at(c).toDouble(m[static_cast<std::size_t>(r * 4 + c)]);
    }
  }
  return m;
}

QJsonArray scaled(const QJsonValue& value, double scale) {
  const QJsonArray v = value.toArray();
  return QJsonArray{v.at(0).toDouble() * scale, v.at(1).toDouble() * scale, v.at(2).toDouble() * scale};
}

QJsonArray json(const std::array<double, 3>& v) { return QJsonArray{v[0], v[1], v[2]}; }

// The design's bodies, placed, in mm: the low and high corners of their
// meshes' boxes as placed (none when there are no triangles).
bool boundsOf(const std::vector<PageBody>& bodies, std::array<double, 3>& low, std::array<double, 3>& high) {
  low.fill(std::numeric_limits<double>::max());
  high.fill(std::numeric_limits<double>::lowest());
  bool any = false;
  for (const PageBody& body : bodies) {
    if (!body.mesh || body.mesh->positions.empty()) {
      continue;
    }
    std::array<double, 3> a{std::numeric_limits<double>::max(), std::numeric_limits<double>::max(),
                            std::numeric_limits<double>::max()};
    std::array<double, 3> b{std::numeric_limits<double>::lowest(), std::numeric_limits<double>::lowest(),
                            std::numeric_limits<double>::lowest()};
    const std::vector<float>& p = body.mesh->positions;
    for (std::size_t i = 0; i + 2 < p.size(); i += 3) {
      for (std::size_t k = 0; k < 3; ++k) {
        a[k] = std::min(a[k], static_cast<double>(p[i + k]));
        b[k] = std::max(b[k], static_cast<double>(p[i + k]));
      }
    }
    // The box's corners as placed.
    const std::array<double, 12>& m = body.transform;
    for (int corner = 0; corner < 8; ++corner) {
      const std::array<double, 3> c{(corner & 1) != 0 ? b[0] : a[0], (corner & 2) != 0 ? b[1] : a[1],
                                    (corner & 4) != 0 ? b[2] : a[2]};
      for (std::size_t r = 0; r < 3; ++r) {
        const double value = m[r * 4] * c[0] + m[r * 4 + 1] * c[1] + m[r * 4 + 2] * c[2] + m[r * 4 + 3];
        low[r] = std::min(low[r], value);
        high[r] = std::max(high[r], value);
      }
    }
    any = true;
  }
  return any;
}

// The data of the design in `context` (WebPage.hpp), its bodies counted
// and what is not as in the design in `result`.
QJsonObject pageData(const CommandContext& context, const PageOptions& options, const QString& title,
                     PageResult& result) {
  // The document's unit: the viewer's own names, else millimetres.
  const QString unitName = context.lengthUnit();
  QString units = QStringLiteral("mm");
  double unitMm = 1.0;
  for (const Unit& unit : kUnits) {
    if (unitName == QLatin1String(unit.name)) {
      units = unitName;
      unitMm = unit.millimetres;
    }
  }
  const double scale = 1.0 / unitMm; // model units per mm

  // The appearances with their textures' image files (as the view and the
  // final render find them).
  QHash<QString, Appearance> known = appearancesById(appearancesOf(context.queryArray(QStringLiteral("appearances"))));
  for (Appearance& appearance : known) {
    if (appearance.hasTexture) {
      const QString id = appearance.id;
      resolveTextureMaps(appearance.texture, options.designFolder, [&context, id](const QString& map) {
        try {
          return QByteArray::fromBase64(context
                                            .queryObject({{QStringLiteral("query"), QStringLiteral("appearance_image")},
                                                          {QStringLiteral("id"), id},
                                                          {QStringLiteral("map"), map}})
                                            .value(QStringLiteral("data"))
                                            .toString()
                                            .toLatin1());
        } catch (const std::exception&) {
          return QByteArray();
        }
      });
    }
  }
  QHash<QString, QString> lookOf;
  QHash<QString, QString> nameOf;
  QHash<QString, QVector<QPair<QStringList, QString>>> faceAppearances;
  for (const QJsonValue& value : context.queryArray(QStringLiteral("bodies"))) {
    const QJsonObject body = value.toObject();
    const QString uid = body.value(QStringLiteral("uid")).toString();
    nameOf.insert(uid, body.value(QStringLiteral("name")).toString(uid));
    lookOf.insert(uid, body.value(QStringLiteral("appearance")).toString());
    for (const QJsonValue& face : body.value(QStringLiteral("face_appearances")).toArray()) {
      QStringList names;
      for (const QJsonValue& name : face.toObject().value(QStringLiteral("faces")).toArray()) {
        names << name.toString();
      }
      faceAppearances[uid].append({names, face.toObject().value(QStringLiteral("appearance")).toString()});
    }
  }
  const auto materialOf = [&known](const QString& id) {
    // An unknown id (an imported one) or none: the default look.
    const auto appearance = known.constFind(id);
    return render::materialOf(appearance != known.cend() ? appearance.value() : Appearance());
  };

  // The visible bodies as placed; each body meshed once, on a copy of its
  // shape (io::triangulate), its faces in the shape's order.
  const io::MeshOptions meshing{options.linearDeflection, options.angularDeflection, false, kFaceTriangles,
                                false, false};
  std::map<QString, std::shared_ptr<const render::MeshData>> meshes;
  std::map<QString, std::vector<render::FaceMaterial>> faceMaterials;
  std::vector<PageBody> bodies;
  for (const QJsonValue& value : context.queryArray(QStringLiteral("instances"))) {
    const QJsonObject instance = value.toObject();
    const QString uid = instance.value(QStringLiteral("body")).toString();
    auto mesh = meshes.find(uid);
    if (mesh == meshes.end()) {
      std::shared_ptr<const render::MeshData> made;
      std::vector<render::FaceMaterial> faces;
      if (const std::shared_ptr<geometry::Shape> shape = context.bodyShape(uid)) {
        // A body that cannot be meshed is left out, and the page says so.
        try {
          made = std::make_shared<const render::MeshData>(render::meshOf(io::triangulate(shape->occt(), meshing)));
        } catch (const Standard_Failure& failure) {
          result.warnings << QObject::tr("%1 is left out: it cannot be meshed (%2)")
                                 .arg(nameOf.value(uid, uid), QString::fromUtf8(failure.what()));
        } catch (const std::exception& e) {
          result.warnings << QObject::tr("%1 is left out: it cannot be meshed (%2)")
                                 .arg(nameOf.value(uid, uid), QString::fromUtf8(e.what()));
        }
        // The faces with appearances of their own by their names, grouped
        // by appearance (a face named twice takes the later one).
        std::map<int, QString> byFace;
        for (const auto& [names, id] : faceAppearances.value(uid)) {
          for (const QString& name : names) {
            for (const int face : shape->find_faces(name.toStdString())) {
              byFace[face] = id;
            }
          }
        }
        std::map<QString, std::vector<std::uint32_t>> byAppearance;
        for (const auto& [face, id] : byFace) {
          if (face >= 0) {
            byAppearance[id].push_back(static_cast<std::uint32_t>(face));
          }
        }
        for (const auto& [id, list] : byAppearance) {
          render::FaceMaterial group;
          group.faces = list;
          group.material = materialOf(id);
          faces.push_back(std::move(group));
        }
      }
      faceMaterials[uid] = std::move(faces);
      mesh = meshes.emplace(uid, made).first;
    }
    if (!mesh->second || mesh->second->indices.empty()) {
      continue;
    }
    PageBody body;
    body.uid = uid;
    body.name = instance.value(QStringLiteral("name")).toString(nameOf.value(uid, uid));
    body.occurrence = pathText(instance.value(QStringLiteral("path")));
    body.mesh = mesh->second;
    body.transform = transformOf(instance.value(QStringLiteral("transform")).toArray());
    body.material = materialOf(lookOf.value(uid));
    body.faces = faceMaterials[uid];
    bodies.push_back(std::move(body));
  }
  // Textures of the page's bodies whose images are missing (the view shows
  // their base colour).
  std::vector<Appearance> looks;
  for (const auto& [uid, made] : meshes) {
    if (!made) {
      continue;
    }
    QStringList ids{lookOf.value(uid)};
    for (const QPair<QStringList, QString>& group : faceAppearances.value(uid)) {
      ids << group.second;
    }
    for (const QString& id : std::as_const(ids)) {
      if (known.contains(id)) {
        looks.push_back(known.value(id));
      }
    }
  }
  std::vector<const Appearance*> lookPointers;
  for (const Appearance& look : looks) {
    lookPointers.push_back(&look);
  }
  result.warnings += render::missingTextures(lookPointers);

  const PageGlb glb = pageGlb(bodies, scale);
  result.warnings += glb.warnings;
  result.bodies = static_cast<int>(std::count_if(glb.nodes.cbegin(), glb.nodes.cend(),
                                                 [](const QString& node) { return !node.isEmpty(); }));
  const QJsonArray occurrences =
      context.queryObject({{QStringLiteral("query"), QStringLiteral("components")}}).value(QStringLiteral("occurrences")).toArray();

  // The named views, and Home: the design's, else from the front, right
  // and above, the bodies' box in sight.
  QJsonArray views;
  QJsonObject home;
  for (const QJsonValue& value : context.queryArray(QStringLiteral("named_views"))) {
    const QJsonObject view = value.toObject();
    const QJsonObject entry{{QStringLiteral("name"), view.value(QStringLiteral("name")).toString()},
                            {QStringLiteral("eye"), scaled(view.value(QStringLiteral("eye")), scale)},
                            {QStringLiteral("target"), scaled(view.value(QStringLiteral("target")), scale)},
                            {QStringLiteral("up"), view.value(QStringLiteral("up")).toArray()},
                            {QStringLiteral("orthographic"), !view.value(QStringLiteral("perspective")).toBool()},
                            {QStringLiteral("height"), view.value(QStringLiteral("height")).toDouble(250.0) * scale}};
    views.append(entry);
    if (entry.value(QStringLiteral("name")).toString().compare(QLatin1String("Home"), Qt::CaseInsensitive) == 0) {
      home = QJsonObject{{QStringLiteral("eye"), entry.value(QStringLiteral("eye"))},
                         {QStringLiteral("target"), entry.value(QStringLiteral("target"))},
                         {QStringLiteral("up"), entry.value(QStringLiteral("up"))}};
    }
  }
  if (home.isEmpty()) {
    std::array<double, 3> low{};
    std::array<double, 3> high{};
    std::array<double, 3> center{0, 0, 0};
    double radius = 100.0;
    if (boundsOf(bodies, low, high)) {
      center = {(low[0] + high[0]) / 2, (low[1] + high[1]) / 2, (low[2] + high[2]) / 2};
      radius = std::max(1.0, std::hypot(high[0] - low[0], high[1] - low[1], high[2] - low[2]) / 2);
    }
    // The box's sphere fills a 45 degree view from this far.
    const double distance = radius * 2.6 / std::sqrt(3.0);
    const std::array<double, 3> eye{center[0] + distance, center[1] - distance, center[2] + distance};
    home = QJsonObject{{QStringLiteral("eye"), json({eye[0] * scale, eye[1] * scale, eye[2] * scale})},
                       {QStringLiteral("target"), json({center[0] * scale, center[1] * scale, center[2] * scale})},
                       {QStringLiteral("up"), QJsonArray{0, 0, 1}}};
  }

  const QString version = QCoreApplication::applicationVersion();
  return QJsonObject{
      {QStringLiteral("format"), 1},
      {QStringLiteral("title"), title},
      {QStringLiteral("units"), units},
      {QStringLiteral("unit_scale"), unitMm / 1000.0},
      {QStringLiteral("up"), QJsonArray{0, 0, 1}},
      {QStringLiteral("glb"), QString::fromLatin1(glb.glb.toBase64())},
      {QStringLiteral("tree"), pageTree(title, occurrences, bodies, glb.nodes)},
      {QStringLiteral("views"), views},
      {QStringLiteral("home"), home},
      {QStringLiteral("created"), QDateTime::currentDateTimeUtc().toString(Qt::ISODate)},
      {QStringLiteral("generator"),
       version.isEmpty() ? QStringLiteral("MitCad") : QStringLiteral("MitCad %1").arg(version)}};
}

// A design loaded without a window, for the page (writePageOfFile): what
// the page reads of the model, as the main window answers it.
class DocumentModel : public CommandContext {
public:
  explicit DocumentModel(const Document& document) : m_document(document) {}

  QJsonValue query(const QJsonObject& request) const override {
    const QJsonDocument answer = parseJson(m_document.query(rustStr(compactJson(request))));
    return answer.isArray() ? QJsonValue(answer.array()) : QJsonValue(answer.object());
  }
  std::shared_ptr<geometry::Shape> bodyShape(const QString& uid) const override {
    return m_document.body_shape(uid.toStdString());
  }
  TopoDS_Shape itemShape(const SelectionItem& /*item*/) const override { return TopoDS_Shape(); }
  TopoDS_Shape analysisShape(const QJsonObject& /*request*/) const override { return TopoDS_Shape(); }

private:
  const Document& m_document;
};

} // namespace

PageResult writePage(const CommandContext& context, const QString& path, const PageOptions& options) {
  PageResult result;
  const QString title = options.title.isEmpty() ? QFileInfo(path).completeBaseName() : options.title;
  QJsonObject data;
  try {
    data = pageData(context, options, title, result);
  } catch (const Standard_Failure& failure) {
    result.error = QObject::tr("the bodies cannot be meshed: %1").arg(QString::fromUtf8(failure.what()));
    return result;
  } catch (const std::exception& e) {
    result.error = QString::fromUtf8(e.what());
    return result;
  }
  QString error;
  const QByteArray html = pageHtml(data, &error);
  if (html.isEmpty()) {
    result.error = error;
    return result;
  }
  QDir().mkpath(QFileInfo(path).absolutePath());
  QSaveFile file(path);
  if (!file.open(QIODevice::WriteOnly) || file.write(html) != html.size() || !file.commit()) {
    result.error = QObject::tr("%1 cannot be written: %2").arg(QDir::toNativeSeparators(path), file.errorString());
    return result;
  }
  result.ok = true;
  result.bytes = html.size();
  return result;
}

PageResult writePageOfFile(const QString& designPath, const QString& path, const PageOptions& options) {
  const QFileInfo design(designPath);
  PageOptions own = options;
  if (own.title.isEmpty()) {
    own.title = design.completeBaseName();
  }
  if (own.designFolder.isEmpty()) {
    own.designFolder = design.absolutePath();
  }
  try {
    // A version 3 file's B-rep data from its project's store (P12a); linked
    // components follow their files; computed with the result store (P7d)
    // as opening the design computes it.
    rust::Box<Document> document = load_design(rustStr(design.absoluteFilePath().toUtf8()));
    const QByteArray store = resultStoreDirectory().toUtf8();
    const QByteArray buildId = resultStoreBuildId().toUtf8();
    const QByteArray label = design.fileName().toUtf8();
    document->set_result_store(rustStr(store), rustStr(buildId), rustStr(label));
    document->command(rustStr(compactJson(
        {{QStringLiteral("cmd"), QStringLiteral("update_links")}, {QStringLiteral("base"), design.absolutePath()}})));
    const QJsonObject recomputed =
        parseObject(document->command(rustStr(compactJson({{QStringLiteral("cmd"), QStringLiteral("recompute")}}))));
    if (!store.isEmpty() && recomputed.value(QStringLiteral("recomputed")).toInt() > 0) {
      document->persist_results(resultStoreMinMs());
    }
    const DocumentModel model(*document);
    return writePage(model, path, own);
  } catch (const std::exception& e) {
    PageResult result;
    result.error = QObject::tr("%1 cannot be read: %2").arg(QDir::toNativeSeparators(designPath), QString::fromUtf8(e.what()));
    return result;
  }
}

} // namespace mitcad::web
