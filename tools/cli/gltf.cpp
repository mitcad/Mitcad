// SPDX-License-Identifier: MIT
// mitcad-cli export to .glb and .gltf (mitcad#2210, docs/user-guide.md
// "Command line"): the same file File > Export writes for the design, by
// the same writer (web::writeGltf, app/web/GltfExport.hpp, with the web
// page's bodies, web/WebPage.hpp pageBodies) reading the model as the main
// window answers it (web/DocumentContext.hpp): the component tree as nested
// nodes, one mesh per body and look shared by its placements, appearances
// and textures, metres, Y up. The extension chooses binary (.glb) or JSON
// (.gltf, its buffer and images beside it).

#include "gltf.hpp"

#include <algorithm>
#include <cctype>
#include <iostream>
#include <locale>
#include <memory>
#include <sstream>
#include <string>

#include <QCoreApplication>
#include <QDir>
#include <QFileInfo>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QString>
#include <QStringList>

#include "mitcad/io/mesh.hpp"
#include "mitcad_bridge/lib.h"
#include "rust/cxx.h"
#include "web/DocumentContext.hpp"
#include "web/GltfExport.hpp"

namespace mitcad::cli {

namespace {

// The export's options for other formats, which a glTF file has no use
// for: it is in metres, holds the component tree as placed, and is no STEP
// or STL file.
const char* const kNotForGltf[] = {"--unit", "--schema", "--ascii", "--coordinates", "--occurrence"};

std::string lower(std::string text) {
  std::transform(text.begin(), text.end(), text.begin(),
                 [](unsigned char c) { return static_cast<char>(std::tolower(c)); });
  return text;
}

bool ends_with(const std::string& text, const std::string& end) {
  return text.size() >= end.size() && text.compare(text.size() - end.size(), end.size(), end) == 0;
}

// A number of the command line, with a point for decimals whatever the
// locale says.
bool number_of(const std::string& text, double& value) {
  std::istringstream in(text);
  in.imbue(std::locale::classic());
  in >> value;
  return !in.fail();
}

// The uids of the bodies in `list` (uids or names, separated by commas, as
// the other formats take them); false with the problem when a name is no
// body's.
bool body_uids(const mitcad::Document& document, const std::string& list, QStringList& uids,
               std::string& problem) {
  const QJsonArray bodies =
      QJsonDocument::fromJson(QByteArray::fromStdString(std::string(document.query(R"({"query": "bodies"})"))))
          .array();
  std::size_t start = 0;
  while (start <= list.size()) {
    const std::size_t comma = list.find(',', start);
    const std::size_t end = comma == std::string::npos ? list.size() : comma;
    const QString wanted = QString::fromStdString(list.substr(start, end - start));
    start = end + 1;
    QString uid;
    for (const QJsonValue& value : bodies) {
      if (value.toObject().value(QStringLiteral("uid")).toString() == wanted) {
        uid = wanted;
        break;
      }
    }
    for (const QJsonValue& value : bodies) {
      if (uid.isEmpty() && value.toObject().value(QStringLiteral("name")).toString() == wanted) {
        uid = value.toObject().value(QStringLiteral("uid")).toString();
      }
    }
    if (uid.isEmpty()) {
      problem = "no body at the timeline marker is named " + wanted.toStdString();
      return false;
    }
    uids << uid;
  }
  return true;
}

} // namespace

bool is_gltf_file(const std::string& path) {
  const std::string name = lower(path);
  return ends_with(name, ".glb") || ends_with(name, ".gltf");
}

int export_gltf(const Arguments& a) {
  // Refused before anything is read or written.
  for (const char* option : kNotForGltf) {
    if (a.flag(option)) {
      std::cerr << "mitcad-cli: " << option
                << " does not apply to glTF files (.glb, .gltf): they are in metres, Y up, with every body "
                   "where the design places it\n";
      return 2;
    }
  }
  web::GltfOptions options;
  // As for STL: a refinement (medium by default), or a deviation and an
  // angle.
  const std::string* deviation = a.option("--deviation");
  const std::string* angle = a.option("--angle");
  if (deviation != nullptr || angle != nullptr) {
    double millimetres = 0.0;
    double degrees = 0.0;
    if (deviation == nullptr || angle == nullptr || !number_of(*deviation, millimetres) ||
        !number_of(*angle, degrees)) {
      return usage("--deviation and --angle go together and need numbers");
    }
    if (a.flag("--refinement")) {
      return usage("give --refinement or --deviation and --angle, not both");
    }
    if (!(millimetres > 0.0) || !(degrees > 0.0)) {
      return usage("--deviation and --angle need numbers greater than zero");
    }
    options.linearDeflection = millimetres;
    options.angularDeflection = degrees * 3.14159265358979323846 / 180.0;
  } else {
    io::Refinement refinement = io::Refinement::Medium;
    if (const std::string* given = a.option("--refinement")) {
      if (*given == "low") {
        refinement = io::Refinement::Low;
      } else if (*given == "high") {
        refinement = io::Refinement::High;
      } else if (*given != "medium") {
        return usage("--refinement is low, medium or high");
      }
    }
    // As File > Export takes its Refinement.
    const io::MeshOptions tolerance = io::MeshOptions::from(refinement);
    options.linearDeflection = tolerance.linear_deflection;
    options.angularDeflection = tolerance.angular_deflection;
  }
  const std::string& output = a.positional[1];
  options.binary = ends_with(lower(output), ".glb");
  // As File > Export: the root named by the design file, relative texture
  // paths in its folder.
  const QFileInfo design(QString::fromStdString(a.positional[0]));
  options.title = design.completeBaseName();
  options.designFolder = design.absolutePath();

  rust::Box<mitcad::Document> document = open_document(a.positional[0]);
  if (const std::string* bodies = a.option("--bodies")) {
    std::string problem;
    if (!body_uids(*document, *bodies, options.bodies, problem)) {
      std::cerr << "mitcad-cli: " << problem << "\n";
      return 1;
    }
  }
  // Texture images are read and written by Qt's image plugins, which the
  // application object finds (after the design is read: on Unix it sets the
  // locale).
  int argc = 1;
  char name[] = "mitcad-cli";
  char* argv[] = {name, nullptr};
  std::unique_ptr<QCoreApplication> application;
  if (QCoreApplication::instance() == nullptr) {
    application = std::make_unique<QCoreApplication>(argc, argv);
  }
  const web::DocumentContext context(*document);
  const web::GltfResult written = web::writeGltf(context, QString::fromStdString(output), options);
  if (!written.ok) {
    std::cerr << "mitcad-cli: " << written.error.toStdString() << "\n";
    return 1;
  }
  // The counts File > Export logs: body nodes (a body placed twice counts
  // twice), triangles, the files written (the file first) and their bytes.
  QStringList files;
  for (const QString& file : written.files) {
    files << QDir::toNativeSeparators(file);
  }
  const QJsonObject result{{QStringLiteral("path"), QString::fromStdString(output)},
                           {QStringLiteral("format"), options.binary ? QStringLiteral("glb") : QStringLiteral("gltf")},
                           {QStringLiteral("bodies"), written.bodies},
                           {QStringLiteral("triangles"), written.triangles},
                           {QStringLiteral("files"), QJsonArray::fromStringList(files)},
                           {QStringLiteral("bytes"), written.bytes},
                           {QStringLiteral("warnings"), QJsonArray::fromStringList(written.warnings)}};
  const std::string text = QJsonDocument(result).toJson(QJsonDocument::Compact).toStdString();
  std::cout << (a.flag("--json") ? text : "Exported: " + text) << "\n";
  return 0;
}

} // namespace mitcad::cli
