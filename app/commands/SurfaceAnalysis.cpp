// SPDX-License-Identifier: MIT
#include "SurfaceAnalysis.hpp"

#include <algorithm>
#include <cmath>
#include <cstddef>
#include <utility>

#include <QLinearGradient>
#include <QObject>
#include <QPainter>
#include <QPointF>
#include <QRadialGradient>
#include <QRectF>

#include "../framework/Theme.hpp"
#include "../framework/UserText.hpp"

namespace mitcad {
namespace {

const QString kZebra = QStringLiteral("zebra");
const QString kCurvature = QStringLiteral("curvature");
const QString kIsocurve = QStringLiteral("isocurve");
const QString kEnvironment = QStringLiteral("environment");

// A number with `decimals` decimals, always with a point, never "-0.0".
QString fixed(double value, int decimals) {
  QString text = QString::number(value, 'f', decimals);
  if (text.startsWith(QLatin1Char('-')) && text.toDouble() == 0.0) {
    text.remove(0, 1);
  }
  return text;
}

// A number without trailing zeros ("1", "2.5").
QString plain(double value) {
  QString text = QString::number(value, 'f', 6);
  while (text.contains(QLatin1Char('.')) && (text.endsWith(QLatin1Char('0')) || text.endsWith(QLatin1Char('.')))) {
    text.chop(1);
  }
  return text == QStringLiteral("-0") ? QStringLiteral("0") : text;
}

// A curvature to four significant digits ("0.04", "-0.125", "1.5e-05").
QString significant(double value) {
  if (value == 0.0 || !std::isfinite(value)) {
    return QStringLiteral("0");
  }
  return QString::number(value, 'g', 4);
}

// A direction of an answer: "(0, 0, 1)", rounded so that one along an
// axis reads as one.
QString directionText(const QJsonValue& value) {
  const QJsonArray d = value.toArray();
  if (d.size() != 3) {
    return QStringLiteral("(?)");
  }
  const auto coordinate = [&d](int i) { return plain(std::round(d.at(i).toDouble() * 1e4) / 1e4); };
  return QStringLiteral("(%1, %2, %3)").arg(coordinate(0), coordinate(1), coordinate(2));
}

// A point of an answer: "(1, 2, 3)" in mm.
QString pointText(const QJsonValue& value) {
  const QJsonArray p = value.toArray();
  if (p.size() != 3) {
    return QStringLiteral("(?)");
  }
  return QStringLiteral("(%1, %2, %3)")
      .arg(fixed(p.at(0).toDouble(), 2), fixed(p.at(1).toDouble(), 2), fixed(p.at(2).toDouble(), 2));
}

// A link that shows a point (printLinkPoint): "<a href='point:x,y,z'>(x, y, z)</a>".
QString pointLink(const QJsonValue& value) {
  const QJsonArray p = value.toArray();
  if (p.size() != 3) {
    return pointText(value);
  }
  return QStringLiteral("<a href='point:%1,%2,%3'>%4</a>")
      .arg(QString::number(p.at(0).toDouble(), 'g', 12), QString::number(p.at(1).toDouble(), 'g', 12),
           QString::number(p.at(2).toDouble(), 'g', 12), pointText(value));
}

// A body's name: the answer's, else `name`'s.
QString nameOf(const QJsonObject& body, const BodyNames& name) {
  const QString given = body.value(QStringLiteral("name")).toString();
  return given.isEmpty() ? name(body.value(QStringLiteral("body")).toString()) : given;
}

// The unit as the UI writes it: "1/mm²".
QString unitText(const QString& unit) {
  return unit == QStringLiteral("1/mm^2") ? QStringLiteral("1/mm²") : unit;
}

// A swatch of a color before words, as Draft Analysis' result area has.
QString swatch(const QColor& color, const QString& words) {
  return QStringLiteral("<span style='color:%1'>&#9632;</span> %2").arg(color.name(), words);
}

// The curvature scale's three entries, the bulging end first: for the
// Gaussian curvature a dome or bowl, single curved or flat, a saddle; for
// the others bulging, flat, hollow.
std::array<QString, 3> scaleWords(const QString& kind, double limit, const QString& unit) {
  const QString reach = QStringLiteral("%1 %2").arg(significant(limit), unitText(unit));
  if (kind == QStringLiteral("gaussian")) {
    return {QObject::tr("Dome or bowl, %1 or more").arg(reach), QObject::tr("Flat or curved one way, 0"),
            QObject::tr("Saddle, −%1 or less").arg(reach)};
  }
  return {QObject::tr("Bulging, %1 or more").arg(reach), QObject::tr("Flat, 0"),
          QObject::tr("Hollow, −%1 or less").arg(reach)};
}

// A curvature kind's words: "Gaussian curvature".
QString kindWords(const QString& kind) {
  if (kind == QStringLiteral("mean")) {
    return QObject::tr("Mean curvature");
  }
  if (kind == QStringLiteral("maximum")) {
    return QObject::tr("Largest principal curvature");
  }
  if (kind == QStringLiteral("minimum")) {
    return QObject::tr("Smallest principal curvature");
  }
  return QObject::tr("Gaussian curvature");
}

// The bodies' entries of an answer.
QJsonArray bodiesOf(const QJsonObject& answer) { return answer.value(QStringLiteral("bodies")).toArray(); }

// Numbers into floats.
void floats(const QJsonValue& value, std::vector<float>& out) {
  for (const QJsonValue& number : value.toArray()) {
    out.push_back(static_cast<float>(number.toDouble()));
  }
}

// A value of a list, none for null.
std::optional<double> optionalAt(const QJsonArray& values, int i) {
  const QJsonValue value = values.at(i);
  return value.isDouble() ? std::optional<double>(value.toDouble()) : std::nullopt;
}

} // namespace

QStringList surfaceAnalysisTypes() { return {kZebra, kCurvature, kIsocurve, kEnvironment}; }

bool isSurfaceAnalysis(const QString& type) { return surfaceAnalysisTypes().contains(type); }

QString surfaceAnalysisCommand(const QString& type) {
  if (type == kZebra) {
    return QStringLiteral("inspect.zebra");
  }
  if (type == kCurvature) {
    return QStringLiteral("inspect.curvature_map");
  }
  if (type == kIsocurve) {
    return QStringLiteral("inspect.isocurve_analysis");
  }
  if (type == kEnvironment) {
    return QStringLiteral("inspect.environment_map");
  }
  return QString();
}

QString surfaceAnalysisIcon(const QString& type) {
  if (type == kZebra) {
    return QStringLiteral("zebra-analysis");
  }
  if (type == kCurvature) {
    return QStringLiteral("curvature-map");
  }
  if (type == kIsocurve) {
    return QStringLiteral("isocurve-analysis");
  }
  if (type == kEnvironment) {
    return QStringLiteral("environment-map");
  }
  return QString();
}

QString surfaceAnalysisTitle(const QString& type) {
  if (type == kZebra) {
    return QObject::tr("Zebra");
  }
  if (type == kCurvature) {
    return QObject::tr("Curvature Map");
  }
  if (type == kIsocurve) {
    return QObject::tr("Isocurve Analysis");
  }
  if (type == kEnvironment) {
    return QObject::tr("Environment Map");
  }
  return QString();
}

QString surfaceAnalysisKind(const QString& type) {
  if (type == kZebra) {
    return QStringLiteral("Zebra analysis");
  }
  if (type == kCurvature) {
    return QStringLiteral("Curvature map");
  }
  if (type == kIsocurve) {
    return QStringLiteral("Isocurve analysis");
  }
  if (type == kEnvironment) {
    return QStringLiteral("Environment map");
  }
  return QString();
}

namespace {

// The fields every surface analysis has: its type, the bodies and, when
// there are any, the faces.
QJsonObject targets(const QString& type, const QJsonArray& bodies, const QJsonArray& faces) {
  QJsonObject def{{QStringLiteral("type"), type}, {QStringLiteral("bodies"), bodies}};
  if (!faces.isEmpty()) {
    def.insert(QStringLiteral("faces"), faces);
  }
  return def;
}

} // namespace

QJsonObject zebraAnalysisDef(const QJsonArray& bodies, const QJsonArray& faces, const QJsonValue& direction,
                             const QString& occurrence, int stripes, double width) {
  QJsonObject def = targets(kZebra, bodies, faces);
  const bool given = direction.isObject() || direction.isString();
  def.insert(QStringLiteral("direction"), given ? direction : QJsonValue(QJsonValue::Null));
  if (given && !occurrence.isEmpty()) {
    def.insert(QStringLiteral("occurrence"), occurrence);
  }
  def.insert(QStringLiteral("stripes"), stripes);
  def.insert(QStringLiteral("width"), width);
  return def;
}

QJsonObject curvatureAnalysisDef(const QJsonArray& bodies, const QJsonArray& faces, const QString& kind,
                                 std::optional<double> limit) {
  QJsonObject def = targets(kCurvature, bodies, faces);
  def.insert(QStringLiteral("kind"), kind.isEmpty() ? QStringLiteral("gaussian") : kind);
  if (limit) {
    def.insert(QStringLiteral("limit"), *limit);
  }
  return def;
}

QJsonObject isocurveAnalysisDef(const QJsonArray& bodies, const QJsonArray& faces, int count, bool curvature,
                                std::optional<double> limit) {
  QJsonObject def = targets(kIsocurve, bodies, faces);
  def.insert(QStringLiteral("count"), count);
  if (curvature) {
    def.insert(QStringLiteral("curvature"), true);
    if (limit) {
      def.insert(QStringLiteral("limit"), *limit);
    }
  }
  return def;
}

QJsonObject environmentAnalysisDef(const QJsonArray& bodies, const QJsonArray& faces, const QString& image) {
  QJsonObject def = targets(kEnvironment, bodies, faces);
  if (!image.trimmed().isEmpty()) {
    def.insert(QStringLiteral("image"), image);
  }
  return def;
}

QJsonObject surfaceAnalysisQuery(const QJsonObject& def) {
  return {{QStringLiteral("query"), QStringLiteral("surface_analysis")}, {QStringLiteral("analysis"), def}};
}

QString runSurfaceAnalysis(const QJsonObject& def, const PrintQuery& query, QJsonObject& answer) {
  answer = query(surfaceAnalysisQuery(def), true);
  // In plain words (mitcad#1612).
  const QJsonValue error = answer.value(QStringLiteral("error"));
  return error.isString() ? userText(error.toString()) : QString();
}

QColor curvatureColor(std::optional<double> value, double limit, const std::array<QColor, 3>& colors) {
  if (!value || !std::isfinite(*value) || !(limit > 0.0)) {
    return colors[1];
  }
  const double t = std::clamp(std::abs(*value) / limit, 0.0, 1.0);
  const QColor& end = *value < 0.0 ? colors[0] : colors[2];
  const QColor& start = colors[1];
  const auto mix = [t](int a, int b) { return static_cast<int>(std::lround(a + (b - a) * t)); };
  return QColor(mix(start.red(), end.red()), mix(start.green(), end.green()), mix(start.blue(), end.blue()));
}

QString curvatureText(double value, const QString& unit) {
  const QString text = QStringLiteral("%1 %2").arg(significant(value), unitText(unit));
  if (unit == QStringLiteral("1/mm") && value != 0.0 && std::isfinite(value)) {
    return QObject::tr("%1 (radius %2 mm)").arg(text, significant(1.0 / std::abs(value)));
  }
  return text;
}

QString surfaceAnalysisText(const QJsonObject& answer, const BodyNames& name, const std::array<QColor, 3>& scale,
                            const std::array<QColor, 2>& zebra) {
  const QString type = answer.value(QStringLiteral("type")).toString();
  QStringList parts;
  const QJsonArray bodies = bodiesOf(answer);
  if (type == kZebra) {
    const int stripes = answer.value(QStringLiteral("stripes")).toInt(kDefaultZebraStripes);
    const double width = answer.value(QStringLiteral("width")).toDouble(kDefaultZebraWidth);
    parts << QObject::tr("Stripes from %1: %2 over the half turn, %3 % of each dark.")
                 .arg(directionText(answer.value(QStringLiteral("direction"))))
                 .arg(stripes)
                 .arg(plain(100.0 * width));
    for (const QJsonValue& value : bodies) {
      const QJsonObject body = value.toObject();
      QStringList lines{QStringLiteral("<b>%1</b>").arg(nameOf(body, name).toHtmlEscaped())};
      lines << swatch(zebra[0], QObject::tr("Dark stripes: %1 mm²")
                                    .arg(fixed(body.value(QStringLiteral("dark_area")).toDouble(), 1)));
      lines << swatch(zebra[1], QObject::tr("Light stripes: %1 mm²")
                                    .arg(fixed(body.value(QStringLiteral("light_area")).toDouble(), 1)));
      parts << lines.join(QStringLiteral("<br>"));
    }
    parts << QObject::tr("<i>Stripes that jump where faces meet show an edge (G0), a kink shows faces that are "
                         "only tangent (G1), and stripes that run on smoothly show curvature continuous faces "
                         "(G2).</i>");
  } else if (type == kCurvature) {
    const QString kind = answer.value(QStringLiteral("kind")).toString();
    const QString unit = answer.value(QStringLiteral("unit")).toString();
    const double limit = answer.value(QStringLiteral("limit")).toDouble();
    parts << QObject::tr("%1 in %2; positive where a face bulges outwards.").arg(kindWords(kind), unitText(unit));
    const std::array<QString, 3> words = scaleWords(kind, limit, unit);
    parts << QStringList{swatch(scale[2], words[0]), swatch(scale[1], words[1]), swatch(scale[0], words[2])}.join(
        QStringLiteral("<br>"));
    for (const QJsonValue& value : bodies) {
      const QJsonObject body = value.toObject();
      QStringList lines{QStringLiteral("<b>%1</b>").arg(nameOf(body, name).toHtmlEscaped())};
      if (body.value(QStringLiteral("min")).isDouble()) {
        lines << QObject::tr("Smallest: %1 at %2")
                     .arg(curvatureText(body.value(QStringLiteral("min")).toDouble(), unit),
                          pointLink(body.value(QStringLiteral("min_at"))));
        lines << QObject::tr("Largest: %1 at %2")
                     .arg(curvatureText(body.value(QStringLiteral("max")).toDouble(), unit),
                          pointLink(body.value(QStringLiteral("max_at"))));
      } else {
        lines << QObject::tr("No curvature known: a mesh body's faces have no surfaces.");
      }
      parts << lines.join(QStringLiteral("<br>"));
    }
  } else if (type == kIsocurve) {
    const int count = answer.value(QStringLiteral("count")).toInt(kDefaultIsocurves);
    const bool colored = answer.value(QStringLiteral("curvature")).toBool();
    parts << QObject::tr("%1 u and %1 v iso curves per face.").arg(count);
    if (colored) {
      const double limit = answer.value(QStringLiteral("limit")).toDouble();
      const std::array<QString, 3> words = scaleWords(QStringLiteral("mean"), limit, QStringLiteral("1/mm"));
      parts << QObject::tr("Colored by the curvature along each curve:") + QStringLiteral("<br>") +
                   QStringList{swatch(scale[2], words[0]), swatch(scale[1], words[1]), swatch(scale[0], words[2])}
                       .join(QStringLiteral("<br>"));
    }
    for (const QJsonValue& value : bodies) {
      const QJsonObject body = value.toObject();
      QStringList lines{QStringLiteral("<b>%1</b>").arg(nameOf(body, name).toHtmlEscaped())};
      lines << QObject::tr("Curves: %1, points: %2")
                   .arg(body.value(QStringLiteral("count")).toInt())
                   .arg(body.value(QStringLiteral("points")).toInt());
      if (colored && body.value(QStringLiteral("min")).isDouble()) {
        lines << QObject::tr("Curvature from %1 to %2")
                     .arg(curvatureText(body.value(QStringLiteral("min")).toDouble(), QStringLiteral("1/mm")),
                          curvatureText(body.value(QStringLiteral("max")).toDouble(), QStringLiteral("1/mm")));
      }
      parts << lines.join(QStringLiteral("<br>"));
    }
  } else if (type == kEnvironment) {
    const QString image = answer.value(QStringLiteral("image")).toString();
    parts << (image.isEmpty() ? QObject::tr("The studio picture reflected in the faces; turn the view to see it "
                                            "move over them.")
                              : QObject::tr("%1 reflected in the faces; turn the view to see it move over them.")
                                    .arg(image.toHtmlEscaped()));
    for (const QJsonValue& value : bodies) {
      const QJsonObject body = value.toObject();
      parts << QStringLiteral("<b>%1</b><br>%2")
                   .arg(nameOf(body, name).toHtmlEscaped(),
                        QObject::tr("Triangles: %1").arg(body.value(QStringLiteral("triangles")).toInt()));
    }
    parts << QObject::tr("<i>A reflection that bends or breaks shows where faces do not join smoothly.</i>");
  }
  return parts.join(QStringLiteral("<br><br>"));
}

QString surfaceAnalysisLog(const QJsonObject& answer, const BodyNames& name) {
  const QString type = answer.value(QStringLiteral("type")).toString();
  QString head;
  QStringList bodies;
  for (const QJsonValue& value : bodiesOf(answer)) {
    const QJsonObject body = value.toObject();
    const QString who = nameOf(body, name);
    if (type == kZebra) {
      bodies << QStringLiteral("%1 dark %2 mm^2 (%3 triangles), light %4 mm^2 (%5 triangles)")
                    .arg(who, fixed(body.value(QStringLiteral("dark_area")).toDouble(), 1))
                    .arg(body.value(QStringLiteral("dark_triangles")).toInt())
                    .arg(fixed(body.value(QStringLiteral("light_area")).toDouble(), 1))
                    .arg(body.value(QStringLiteral("light_triangles")).toInt());
    } else if (type == kCurvature) {
      const QString range = body.value(QStringLiteral("min")).isDouble()
                                ? QStringLiteral("min %1, max %2")
                                      .arg(significant(body.value(QStringLiteral("min")).toDouble()),
                                           significant(body.value(QStringLiteral("max")).toDouble()))
                                : QStringLiteral("no curvature");
      bodies << QStringLiteral("%1 %2, %3 triangles")
                    .arg(who, range)
                    .arg(body.value(QStringLiteral("triangles")).toInt());
    } else if (type == kIsocurve) {
      QString text = QStringLiteral("%1 %2 curves, %3 points")
                         .arg(who)
                         .arg(body.value(QStringLiteral("count")).toInt())
                         .arg(body.value(QStringLiteral("points")).toInt());
      if (answer.value(QStringLiteral("curvature")).toBool() && body.value(QStringLiteral("min")).isDouble()) {
        text += QStringLiteral(", min %1, max %2")
                    .arg(significant(body.value(QStringLiteral("min")).toDouble()),
                         significant(body.value(QStringLiteral("max")).toDouble()));
      }
      bodies << text;
    } else if (type == kEnvironment) {
      bodies << QStringLiteral("%1 %2 triangles").arg(who).arg(body.value(QStringLiteral("triangles")).toInt());
    }
  }
  if (type == kZebra) {
    head = QStringLiteral("direction %1, %2 stripes, width %3")
               .arg(directionText(answer.value(QStringLiteral("direction"))))
               .arg(answer.value(QStringLiteral("stripes")).toInt())
               .arg(plain(answer.value(QStringLiteral("width")).toDouble()));
  } else if (type == kCurvature) {
    head = QStringLiteral("%1 %2, limit %3%4")
               .arg(answer.value(QStringLiteral("kind")).toString(), answer.value(QStringLiteral("unit")).toString(),
                    significant(answer.value(QStringLiteral("limit")).toDouble()),
                    answer.value(QStringLiteral("automatic")).toBool() ? QStringLiteral(" (automatic)") : QString());
  } else if (type == kIsocurve) {
    head = QStringLiteral("%1 per face%2")
               .arg(answer.value(QStringLiteral("count")).toInt())
               .arg(answer.value(QStringLiteral("curvature")).toBool() ? QStringLiteral(", curvature colored")
                                                                        : QString());
  } else if (type == kEnvironment) {
    const QString image = answer.value(QStringLiteral("image")).toString();
    head = image.isEmpty() ? QStringLiteral("studio picture") : QStringLiteral("picture %1").arg(image);
  }
  return QStringLiteral("%1: %2").arg(head, bodies.isEmpty() ? QStringLiteral("no body")
                                                             : bodies.join(QStringLiteral("; ")));
}

QJsonObject surfaceOverlayJson(const QJsonObject& def, const QJsonObject& answer, const QString& title) {
  const QString type = def.value(QStringLiteral("type")).toString();
  QJsonObject surface{{QStringLiteral("type"), type}};
  const QJsonArray bodies = bodiesOf(answer);
  if (type == kZebra) {
    QJsonArray dark;
    QJsonArray light;
    for (const QJsonValue& value : bodies) {
      const QJsonObject triangles = value.toObject().value(QStringLiteral("triangles")).toObject();
      dark.append(triangles.value(QStringLiteral("dark")));
      light.append(triangles.value(QStringLiteral("light")));
    }
    surface.insert(QStringLiteral("dark"), dark);
    surface.insert(QStringLiteral("light"), light);
    surface.insert(QStringLiteral("stripes"), answer.value(QStringLiteral("stripes")));
  } else if (type == kCurvature || type == kEnvironment) {
    QJsonArray meshes;
    for (const QJsonValue& value : bodies) {
      meshes.append(value.toObject().value(QStringLiteral("mesh")));
    }
    surface.insert(QStringLiteral("meshes"), meshes);
    for (const char* key : {"kind", "unit", "limit", "image"}) {
      const QJsonValue field = answer.value(QLatin1String(key));
      if (!field.isUndefined() && !field.isNull()) {
        surface.insert(QLatin1String(key), field);
      }
    }
  } else if (type == kIsocurve) {
    QJsonArray curves;
    for (const QJsonValue& value : bodies) {
      for (const QJsonValue& curve : value.toObject().value(QStringLiteral("curves")).toArray()) {
        curves.append(curve);
      }
    }
    surface.insert(QStringLiteral("curves"), curves);
    surface.insert(QStringLiteral("count"), answer.value(QStringLiteral("count")));
    surface.insert(QStringLiteral("curvature"), answer.value(QStringLiteral("curvature")).toBool());
    surface.insert(QStringLiteral("limit"), answer.value(QStringLiteral("limit")));
  }
  return {{QStringLiteral("surface"), surface}, {QStringLiteral("title"), title}};
}

void addSurfaceOverlay(const QJsonObject& json, const SurfaceColors& colors, PrintOverlay& overlay) {
  const QJsonObject surface = json.value(QStringLiteral("surface")).toObject();
  const QString type = surface.value(QStringLiteral("type")).toString();
  if (!isSurfaceAnalysis(type)) {
    return;
  }
  overlay.surface = true;
  overlay.title = json.value(QStringLiteral("title")).toString();
  overlay.zebraColors = colors.zebra;
  if (type == kZebra) {
    const char* const bands[2] = {"dark", "light"};
    for (std::size_t b = 0; b < 2; ++b) {
      for (const QJsonValue& body : surface.value(QLatin1String(bands[b])).toArray()) {
        for (const QJsonValue& triangle : body.toArray()) {
          if (triangle.toArray().size() == 9) {
            floats(triangle, overlay.zebra[b]);
          }
        }
      }
    }
    overlay.legend.emplace_back(colors.zebra[0],
                                QObject::tr("Dark stripes, %1 over 180°").arg(surface.value(QStringLiteral("stripes")).toInt()));
    overlay.legend.emplace_back(colors.zebra[1], QObject::tr("Light stripes"));
    return;
  }
  if (type == kCurvature) {
    const double limit = surface.value(QStringLiteral("limit")).toDouble();
    for (const QJsonValue& value : surface.value(QStringLiteral("meshes")).toArray()) {
      const QJsonObject mesh = value.toObject();
      const QJsonArray positions = mesh.value(QStringLiteral("positions")).toArray();
      const QJsonArray values = mesh.value(QStringLiteral("values")).toArray();
      const QJsonArray indices = mesh.value(QStringLiteral("indices")).toArray();
      const int vertices = static_cast<int>(positions.size() / 3);
      for (int t = 0; t + 2 < indices.size(); t += 3) {
        bool inside = true;
        for (int k = 0; k < 3; ++k) {
          const int v = indices.at(t + k).toInt(-1);
          inside = inside && v >= 0 && v < vertices;
        }
        if (!inside) {
          continue;
        }
        for (int k = 0; k < 3; ++k) {
          const int v = indices.at(t + k).toInt();
          for (int c = 0; c < 3; ++c) {
            overlay.shaded.push_back(static_cast<float>(positions.at(3 * v + c).toDouble()));
          }
          overlay.shadedColors.push_back(curvatureColor(optionalAt(values, v), limit, colors.scale).rgb());
        }
      }
    }
    const std::array<QString, 3> words = scaleWords(surface.value(QStringLiteral("kind")).toString(), limit,
                                                    surface.value(QStringLiteral("unit")).toString());
    overlay.legend.emplace_back(colors.scale[2], words[0]);
    overlay.legend.emplace_back(colors.scale[1], words[1]);
    overlay.legend.emplace_back(colors.scale[0], words[2]);
    return;
  }
  if (type == kIsocurve) {
    const bool colored = surface.value(QStringLiteral("curvature")).toBool();
    const double limit = surface.value(QStringLiteral("limit")).toDouble();
    for (const QJsonValue& value : surface.value(QStringLiteral("curves")).toArray()) {
      const QJsonObject curve = value.toObject();
      const QJsonArray points = curve.value(QStringLiteral("points")).toArray();
      const QJsonArray normals = curve.value(QStringLiteral("normals")).toArray();
      const QJsonArray curvatures = curve.value(QStringLiteral("curvatures")).toArray();
      const int count = static_cast<int>(points.size() / 3);
      if (count < 2) {
        continue;
      }
      // Each point a little off the face along its normal, so the depth
      // test keeps it over the face it lies on.
      constexpr double kLift = 0.02;
      std::vector<float> line;
      std::vector<QRgb> shades;
      for (int i = 0; i < count; ++i) {
        for (int c = 0; c < 3; ++c) {
          const double n = normals.size() == points.size() ? normals.at(3 * i + c).toDouble() : 0.0;
          line.push_back(static_cast<float>(points.at(3 * i + c).toDouble() + kLift * n));
        }
        shades.push_back(colored ? curvatureColor(optionalAt(curvatures, i), limit, colors.scale).rgb()
                                 : colors.isocurve.rgb());
      }
      overlay.isoLines.push_back(std::move(line));
      overlay.isoColors.push_back(std::move(shades));
    }
    if (colored) {
      const std::array<QString, 3> words = scaleWords(QStringLiteral("mean"), limit, QStringLiteral("1/mm"));
      overlay.legend.emplace_back(colors.scale[2], words[0]);
      overlay.legend.emplace_back(colors.scale[1], words[1]);
      overlay.legend.emplace_back(colors.scale[0], words[2]);
    } else {
      overlay.legend.emplace_back(colors.isocurve, QObject::tr("u and v iso curves, %1 of each per face")
                                                       .arg(surface.value(QStringLiteral("count")).toInt()));
    }
    return;
  }
  // The Environment Map: the faces' triangles with their normals, and the
  // picture they reflect, the studio's when none is given or it cannot be
  // read.
  for (const QJsonValue& value : surface.value(QStringLiteral("meshes")).toArray()) {
    const QJsonObject mesh = value.toObject();
    const QJsonArray positions = mesh.value(QStringLiteral("positions")).toArray();
    const QJsonArray normals = mesh.value(QStringLiteral("normals")).toArray();
    const QJsonArray indices = mesh.value(QStringLiteral("indices")).toArray();
    const int vertices = static_cast<int>(std::min(positions.size(), normals.size()) / 3);
    for (int t = 0; t + 2 < indices.size(); t += 3) {
      bool inside = true;
      for (int k = 0; k < 3; ++k) {
        const int v = indices.at(t + k).toInt(-1);
        inside = inside && v >= 0 && v < vertices;
      }
      if (!inside) {
        continue;
      }
      for (int k = 0; k < 3; ++k) {
        const int v = indices.at(t + k).toInt();
        for (int c = 0; c < 3; ++c) {
          overlay.mirror.push_back(static_cast<float>(positions.at(3 * v + c).toDouble()));
          overlay.mirrorNormals.push_back(static_cast<float>(normals.at(3 * v + c).toDouble()));
        }
      }
    }
  }
  const QString image = surface.value(QStringLiteral("image")).toString();
  QImage picture;
  if (!image.isEmpty() && picture.load(image)) {
    overlay.legend.emplace_back(colors.studio[0], QObject::tr("Reflecting %1").arg(image));
  } else {
    picture = studioEnvironment(colors.studio[0], colors.studio[1], colors.studio[2]);
    overlay.legend.emplace_back(colors.studio[0], image.isEmpty()
                                                      ? QObject::tr("Reflecting the studio picture")
                                                      : QObject::tr("%1 could not be read: the studio picture")
                                                            .arg(image));
  }
  overlay.environment = picture;
}

SurfaceColors surfaceColors() {
  // The same on either background (Theme.cpp): the colors are baked into
  // the drawn vertices and the picture.
  SurfaceColors colors;
  colors.zebra = {viewColor(ColorRole::ZebraDark, false), viewColor(ColorRole::ZebraLight, false)};
  colors.scale = {viewColor(ColorRole::CurvatureConcave, false), viewColor(ColorRole::CurvatureFlat, false),
                  viewColor(ColorRole::CurvatureConvex, false)};
  colors.isocurve = viewColor(ColorRole::Isocurve, false);
  colors.studio = {viewColor(ColorRole::EnvironmentSky, false), viewColor(ColorRole::EnvironmentFloor, false),
                   viewColor(ColorRole::EnvironmentLight, false)};
  return colors;
}

QImage studioEnvironment(const QColor& sky, const QColor& floor, const QColor& light, int size) {
  // A sphere map: what a mirror ball shows, the view's up at the top. The
  // sky above the horizon (a little under the middle), the floor below,
  // and soft bright bands across the sky, as a studio's light boxes.
  QImage image(size, size, QImage::Format_RGB32);
  QPainter painter(&image);
  painter.setRenderHint(QPainter::Antialiasing);
  QLinearGradient ground(0.0, 0.0, 0.0, size);
  QColor horizon = sky;
  horizon.setHsvF(sky.hsvHueF() < 0.0f ? 0.0f : sky.hsvHueF(), sky.hsvSaturationF() * 0.5f, 1.0f);
  ground.setColorAt(0.0, sky.darker(115));
  ground.setColorAt(0.52, horizon);
  ground.setColorAt(0.56, floor.lighter(140));
  ground.setColorAt(1.0, floor);
  painter.fillRect(QRectF(0.0, 0.0, size, size), ground);
  for (const double at : {0.14, 0.28, 0.40}) {
    QLinearGradient band(0.0, (at - 0.04) * size, 0.0, (at + 0.04) * size);
    QColor clear = light;
    clear.setAlphaF(0.0f);
    band.setColorAt(0.0, clear);
    band.setColorAt(0.5, light);
    band.setColorAt(1.0, clear);
    painter.fillRect(QRectF(0.0, (at - 0.04) * size, size, 0.08 * size), band);
  }
  // A soft key light to one side, as a studio's lamp.
  QRadialGradient lamp(QPointF(0.3 * size, 0.22 * size), 0.18 * size);
  QColor clear = light;
  clear.setAlphaF(0.0f);
  lamp.setColorAt(0.0, light);
  lamp.setColorAt(1.0, clear);
  painter.fillRect(QRectF(0.0, 0.0, size, size), lamp);
  painter.end();
  return image;
}

} // namespace mitcad
