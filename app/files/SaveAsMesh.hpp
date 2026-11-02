// SPDX-License-Identifier: MIT
#pragma once

// Save As Mesh (mitcad#734): the bodies of what was chosen in a body's or
// a component's context menu (the browser's or the view's) to mesh files,
// written by the model's `export` (core/model-api/src/commands.md, "Save As
// Mesh"). This is what is saved and how the files are named, in plain Qt
// Core; MainWindowSaveAsMesh.cpp asks and writes.

#include <QHash>
#include <QJsonArray>
#include <QJsonObject>
#include <QString>
#include <QStringList>
#include <QVector>

namespace mitcad {

// What a context menu's Save As Mesh… stands for: a body (of a picked face,
// edge or vertex too) where an occurrence places it, or a component where
// an occurrence places it (the root, without one: the whole design).
struct MeshItem {
  enum class Kind { Body, Component };
  Kind kind = Kind::Body;
  QString uid;        // the body (F2.b0) or the component (C1)
  QString occurrence; // its occurrence uids from the root ("O1/O4"); empty in the root
};

// A body of the design as Save As Mesh needs it.
struct MeshBody {
  QString name;
  // A solid or a mesh body; surface and empty bodies are not saved.
  bool printable = true;
  // It has a print bed face that can lie on the bed (mitcad#731), else why
  // not when it has one ("the print bed face is gone"); both empty: none.
  bool bedFace = false;
  QString bedError;
};

// What Save As Mesh writes.
struct MeshPlan {
  QStringList bodies;   // the solid and mesh bodies, each once, in the items' order
  QStringList names;    // their names
  QStringList skipped;  // the names of surface and empty bodies left out
  // The occurrence every item shares ("O1/O4"), which `export` writes only
  // the placements of; empty: the bodies where the design shows them.
  QString occurrence;
  // Each body's own occurrence for a file per body: the one its items
  // share, else empty.
  QStringList occurrences;
  // The name of one file, without the extension: one body's name, one
  // component's name, else the design's (owner 26.10.2026).
  QString name;
  // Why nothing can be saved (no solid or mesh body); empty when it can.
  QString blocked;
  // Why "On its print bed face" cannot be chosen (no body has a usable
  // print bed face); empty when it can.
  QString bedBlocked;
};

// The plan for `items`: bodies by the `instances` query (with hidden ones:
// `body`, `path`; a component's bodies are those placed in its occurrence
// and the occurrences in it, the root's every body), `bodies` by uid,
// `components` the components' names by uid, `design` the design's name.
MeshPlan planSaveAsMesh(const QVector<MeshItem>& items, const QJsonArray& instances,
                        const QHash<QString, MeshBody>& bodies, const QHash<QString, QString>& components,
                        const QString& design);

// The name of one file for `items`, without the extension (owner
// 26.10.2026; File › Export too since mitcad#1164): one body's name (the
// same body picked twice is one), one component's name where an occurrence
// places it, else `design`; `bodyNames` the bodies' names by uid. An empty
// name is the design's.
QString meshItemsName(const QVector<MeshItem>& items, const QHash<QString, QString>& bodyNames,
                      const QHash<QString, QString>& components, const QString& design);

// The file names Save As Mesh proposes, with the extension: one file named
// after the plan's `name`, or with `perBody` a file per body named after
// it, unique ignoring case ("Body1.stl", "Body1_2.stl"); characters a file
// name cannot hold replaced (safeFileName).
QStringList meshFileNames(const MeshPlan& plan, bool perBody, const QString& extension);

// The dialog's choices.
struct SaveMeshOptions {
  QString format = QStringLiteral("3mf"); // 3mf, stl or obj
  bool ascii = false;                     // STL as text
  QString unit = QStringLiteral("mm");    // mm, cm, m, in or ft
  bool perBody = false;                   // a file per body
  QString refinement = QStringLiteral("medium"); // low, medium, high or custom
  double deviation = 0.03;                // custom: surface deviation, mm
  double angle = 15.0;                    // custom: normal deviation, degrees
  double maxEdge = 0.0;                   // custom: maximum edge length, mm (0: none)
  bool onBed = false;                     // on the print bed face

  // The file extension: 3mf, stl or obj.
  QString extension() const { return format; }
  // "3mf, mm, one file, medium" and so on, for the log.
  QString describe() const;
};

// The model's `export` of `bodies` (uids) to `path` with the options,
// placed by `occurrence` when not empty; `colors` (3MF, OBJ: the colours
// the view shows) when not empty.
QJsonObject meshExportCommand(const QString& path, const QStringList& bodies, const QString& occurrence,
                              const SaveMeshOptions& options, const QJsonObject& colors);

} // namespace mitcad
