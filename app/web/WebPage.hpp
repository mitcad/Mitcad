// SPDX-License-Identifier: MIT
#pragma once

// A design as one self-contained web page (mitcad#423): the web viewer
// (app/web/viewer, its libraries in third_party/three) with the design's
// bodies as a binary glTF file and its data inlined, so that the file
// opens from the disk in any browser without a server or the internet
// (browsers block module scripts and fetch() from file://). File › Export
// › Web page writes one; a project's web/ folder holds one per design.
//
// The page is the viewer's template.html with its three placeholders
// replaced: <!--MITCAD:IMPORTMAP--> by an import map whose modules
// (modules.txt: "<specifier> <path>" a line, the path relative to the
// repository) are data: URLs, <!--MITCAD:VIEWER--> by viewer.js as a
// module script, <!--MITCAD:DATA--> by the design's data as
// <script id="mitcad-data" type="application/json">, and {{TITLE}} by the
// title, HTML-escaped. The viewer's files are Qt resources under
// :/web-viewer/<repository path> (app/CMakeLists.txt).
//
// The data (format 1): {"format": 1, "title", "units" (mm, cm, m, in or
// ft), "unit_scale" (metres per model unit), "up" ([0, 0, 1]), "glb" (the
// bodies' glTF file, base64, in model units: web/WebGlb.hpp), "tree" (the
// components with the glb's nodes, pageTree), "views" (the named views:
// "name", "eye", "target", "up", "orthographic", "height"; model units),
// "home" ("eye", "target", "up": the named view Home, else the design
// seen from the front, right and above), "created" (UTC, ISO 8601),
// "generator" ("MitCad <version>")}.

#include <QByteArray>
#include <QJsonObject>
#include <QString>
#include <QStringList>
#include <QtGlobal>

namespace mitcad {

class CommandContext;

namespace web {

struct PageOptions {
  double linearDeflection = 0.1;  // mm, as Export's mesh quality
  double angularDeflection = 0.5; // radians
  QString title;                  // empty: the design's name
  // The folder that relative texture paths are relative to (the design
  // file's); empty: such textures are left out. writePageOfFile sets it.
  QString designFolder;
};

struct PageResult {
  bool ok = false;
  QString error;
  qint64 bytes = 0;
  // The bodies written (placements of bodies: a component placed twice
  // counts twice).
  int bodies = 0;
  // What is not as in the design: texture images that could not be read.
  QStringList warnings;
};

// The design open in `context`, as one self-contained HTML page at `path`:
// its visible bodies as placed, meshed within the options' deflections
// (each on a copy: the design's shapes are not changed), with their
// appearances. Reads the model through `context` only.
PageResult writePage(const CommandContext& context, const QString& path, const PageOptions& options);

// A design file (its newest version on disk), loaded without a window
// (as Version History loads other versions: links followed, computed with
// the result store), as one page at `path`. Usable from a worker thread.
PageResult writePageOfFile(const QString& designPath, const QString& path, const PageOptions& options);

// The page from a data JSON object (the format above, "glb" filled) — used
// by both and by tests. Empty when a viewer file is missing from the
// build or the template lacks a placeholder, with the reason in `error`
// (the missing file named).
QByteArray pageHtml(const QJsonObject& data, QString* error = nullptr);

// The same with the viewer's files read from `root` (a folder or a
// resource prefix holding app/web/viewer/... and third_party/...): tests
// and pageHtml (":/web-viewer").
QByteArray pageHtmlFrom(const QJsonObject& data, const QString& root, QString* error = nullptr);

} // namespace web
} // namespace mitcad
