// SPDX-License-Identifier: MIT
#pragma once

// Ready to Render (mitcad#2013, the Render workspace's Output stage): a
// short check of the design before an image is rendered, its findings in
// words a person reads (no ids, internal names or coordinates). Qt Core
// only, so that a unit test reads it.

#include <QJsonArray>
#include <QJsonObject>
#include <QList>
#include <QString>

namespace mitcad::render {

struct ReadyFinding {
  // Info: worth knowing; Warning: maybe a mistake; Problem: the render
  // cannot give what was asked (Render All Shots does not start).
  enum class Level { Info, Warning, Problem };
  Level level = Level::Info;
  QString text;
};

struct ReadyInput {
  QJsonArray bodies;      // the `bodies` query
  QJsonObject settings;   // the `render_settings` query
  QString documentFolder; // relative environment images are in it
  QString folder;         // where Render All Shots saves (empty: none)
  bool allShots = false;  // the check is for Render All Shots
};

// The findings, the problems first; none: ready to render. Checks that no
// body is hidden by mistake, that every shown body has an appearance of
// its own or knows it renders in the default look, that an HDR image of
// the environment is there, and for Render All Shots that there are
// shots and the folder can be written (a file is made in it and removed).
QList<ReadyFinding> readyFindings(const ReadyInput& input);
// Whether any finding is a problem.
bool hasProblem(const QList<ReadyFinding>& findings);

} // namespace mitcad::render
