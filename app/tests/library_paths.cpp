// SPDX-License-Identifier: MIT
// The GUI and CLI use one cache; existing GUI caches retain their default path.
#include "files/Libraries.hpp"
#include "framework/AppSettings.hpp"

#include <cstdio>

#include <QCoreApplication>
#include <QDir>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QStandardPaths>
#include <QTemporaryDir>

#include "mitcad_bridge/lib.h"
#include "bundled_test.hpp"

namespace {

QString root(const rust::String& value) {
  return QDir::fromNativeSeparators(QString::fromUtf8(value.data(), static_cast<qsizetype>(value.size())));
}

QJsonObject configuration() {
  const rust::String json = mitcad::configure_libraries("");
  return QJsonDocument::fromJson(QByteArray(json.data(), static_cast<qsizetype>(json.size()))).object();
}

} // namespace

MITCAD_BUNDLED_TEST_ARGS(library_paths) {
  QCoreApplication app(argc, argv);
  // The names the application sets (app/main.cpp).
  QCoreApplication::setOrganizationName(mitcad::storedApplicationName());
  QCoreApplication::setApplicationName(mitcad::storedApplicationName());
  qunsetenv("MITCAD_LIBRARIES_DIR");
  const QString expected = QDir::fromNativeSeparators(
      QStandardPaths::writableLocation(QStandardPaths::AppLocalDataLocation)) + QStringLiteral("/libraries");
  // The settings and the data folders keep their spelling Mitcad/Mitcad,
  // although the program is written MitCad (mitcad#822): an installation
  // keeps its settings, libraries and autosaves (Linux paths are
  // case-sensitive).
  if (!expected.endsWith(QStringLiteral("/Mitcad/Mitcad/libraries"))) {
    std::fprintf(stderr, "the data folder moved: %s\n", expected.toUtf8().constData());
    return 1;
  }
  const QString shared = root(mitcad::default_libraries_directory());
  if (shared != expected || QDir::fromNativeSeparators(mitcad::librariesDirectory()) != shared
      || QDir::fromNativeSeparators(configuration().value(QStringLiteral("root")).toString()) != shared) {
    std::fprintf(stderr, "library default mismatch: Qt=%s shared=%s\n",
                 expected.toUtf8().constData(), shared.toUtf8().constData());
    return 1;
  }
  QTemporaryDir directory;
  if (!directory.isValid()) {
    return 1;
  }
  const QString override = directory.path() + QStringLiteral("/libraries");
  qputenv("MITCAD_LIBRARIES_DIR", override.toUtf8());
  const QJsonObject configured = configuration();
  if (root(mitcad::default_libraries_directory()) != override
      || QDir::fromNativeSeparators(mitcad::librariesDirectory()) != override
      || QDir::fromNativeSeparators(configured.value(QStringLiteral("root")).toString()) != override
      || !configured.value(QStringLiteral("extra")).toArray().isEmpty()) {
    std::fprintf(stderr, "library cache override was not shared or isolated\n");
    return 1;
  }
  std::puts("library paths ok");
  return 0;
}
