// SPDX-License-Identifier: MIT
// A design as one web page (web/WebPage.hpp): the visible bodies as placed,
// meshed on copies of their shapes, with their appearances, as the
// viewer's glb (web/WebGlb.hpp) and data, inlined into the viewer's page.
// The bodies themselves: pageBodies (web/WebBodies.cpp).
#include "web/WebPage.hpp"

#include <algorithm>
#include <array>
#include <cmath>
#include <exception>
#include <limits>
#include <vector>

#include <QCoreApplication>
#include <QDateTime>
#include <QDir>
#include <QFileInfo>
#include <QJsonArray>
#include <QJsonDocument>
#include <QObject>
#include <QSaveFile>

#include <Standard_Failure.hxx>

#include "framework/Command.hpp"
#include "framework/Json.hpp"
#include "framework/ResultCache.hpp"
#include "mitcad_bridge/lib.h"
#include "rust/cxx.h"
#include "web/DocumentContext.hpp"
#include "web/WebGlb.hpp"

namespace mitcad::web {

namespace {

// The units the viewer knows, with their size in mm.
struct Unit {
  const char* name;
  double millimetres;
};
constexpr std::array<Unit, 5> kUnits{
    {{"mm", 1.0}, {"cm", 10.0}, {"m", 1000.0}, {"in", 25.4}, {"ft", 304.8}}};

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

  const std::vector<PageBody> bodies = pageBodies(context, options, result.warnings);

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
    const DocumentContext model(*document);
    return writePage(model, path, own);
  } catch (const std::exception& e) {
    PageResult result;
    result.error = QObject::tr("%1 cannot be read: %2").arg(QDir::toNativeSeparators(designPath), QString::fromUtf8(e.what()));
    return result;
  }
}

} // namespace mitcad::web
