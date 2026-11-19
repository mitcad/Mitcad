// SPDX-License-Identifier: MIT
#include "render/RenderReadyCheck.hpp"

#include <algorithm>

#include <QCoreApplication>
#include <QDir>
#include <QFileInfo>
#include <QJsonValue>
#include <QTemporaryFile>

namespace mitcad::render {

namespace {

// Whether a file can be made in `folder` (and is removed again).
bool writable(const QString& folder) {
  const QFileInfo info(folder);
  if (!info.isDir()) {
    return false;
  }
  QTemporaryFile probe(QDir(folder).filePath(QStringLiteral(".mitcad-render-check-XXXXXX")));
  return probe.open();
}

} // namespace

// The texts are QCoreApplication::translate calls written out (not through a
// helper) so that lupdate finds them, and the counts' singular and plural.
QList<ReadyFinding> readyFindings(const ReadyInput& input) {
  QList<ReadyFinding> findings;
  int shown = 0;
  int hidden = 0;
  int defaultLook = 0;
  for (const QJsonValue& value : input.bodies) {
    const QJsonObject body = value.toObject();
    if (!body.value(QStringLiteral("visible")).toBool(true)) {
      ++hidden;
      continue;
    }
    ++shown;
    if (body.value(QStringLiteral("appearance")).toString().isEmpty() &&
        !body.contains(QStringLiteral("face_appearances"))) {
      ++defaultLook;
    }
  }
  if (shown == 0) {
    findings.append({ReadyFinding::Level::Problem,
                     QCoreApplication::translate("RenderReadyCheck",
                                                 "No body is shown: the image would show none of the design.")});
  }
  if (hidden > 0) {
    findings.append({ReadyFinding::Level::Warning,
                     QCoreApplication::translate("RenderReadyCheck",
                                                 "%n hidden body(ies) will not be in the image. Show them in the "
                                                 "Browser if they belong in it.",
                                                 nullptr, hidden)});
  }
  if (defaultLook > 0) {
    findings.append({ReadyFinding::Level::Info,
                     QCoreApplication::translate("RenderReadyCheck",
                                                 "%n shown body(ies) have no appearance of their own and render in "
                                                 "the default look.",
                                                 nullptr, defaultLook)});
  }
  const QJsonObject environment = input.settings.value(QStringLiteral("environment")).toObject();
  if (environment.value(QStringLiteral("preset")).toString() == QLatin1String("image")) {
    const QString image = environment.value(QStringLiteral("image")).toString();
    const QString path = image.isEmpty() || QDir::isAbsolutePath(image) || input.documentFolder.isEmpty()
                             ? image
                             : QDir(input.documentFolder).filePath(image);
    if (path.isEmpty() || !QFileInfo::exists(path)) {
      findings.append({ReadyFinding::Level::Problem,
                       QCoreApplication::translate("RenderReadyCheck", "The environment's HDR image cannot be found: "
                                                                       "choose it again in the Light stage.")});
    }
  }
  if (input.allShots) {
    if (input.settings.value(QStringLiteral("shots")).toArray().isEmpty()) {
      findings.append({ReadyFinding::Level::Problem,
                       QCoreApplication::translate("RenderReadyCheck",
                                                   "There are no shots yet: save one in the Frame stage.")});
    }
    if (input.folder.isEmpty()) {
      findings.append({ReadyFinding::Level::Problem,
                       QCoreApplication::translate("RenderReadyCheck", "There is no folder for the images: save the "
                                                                       "design or choose a folder.")});
    } else if (!writable(input.folder)) {
      findings.append({ReadyFinding::Level::Problem,
                       QCoreApplication::translate("RenderReadyCheck",
                                                   "The folder for the images cannot be written: choose another one.")});
    }
  }
  std::stable_sort(findings.begin(), findings.end(), [](const ReadyFinding& a, const ReadyFinding& b) {
    return static_cast<int>(a.level) > static_cast<int>(b.level);
  });
  return findings;
}

bool hasProblem(const QList<ReadyFinding>& findings) {
  return std::any_of(findings.begin(), findings.end(),
                     [](const ReadyFinding& f) { return f.level == ReadyFinding::Level::Problem; });
}

} // namespace mitcad::render
