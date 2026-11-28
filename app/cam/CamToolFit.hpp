// SPDX-License-Identifier: MIT
#pragma once

// Which of a machine's tools a milling operation's panel offers and starts
// with (mitcad#2590, GitHub mitcad/Mitcad#748; app/COMMANDS.md "The CNC
// group"): the cutters its toolpath's generator takes (core/cam/src/ops:
// `check_mill` and `check_cutter` of the 2.5-axis operations,
// `SurfaceCutter::of_tool` of the finishing). Tools are the `cam_machines`
// query's (`number`, `name`, `shape.kind`: `flat`, `ball`, `bull`, `drill`,
// `chamfer`, `engraver` and the lathe's tools). Plain Qt Core: no panel, no
// model.

#include <QJsonArray>
#include <QJsonObject>
#include <QString>

namespace mitcad::cam {

// The cutters an operation takes.
enum class Cutters {
  Any,           // Drill: every tool (it starts with a drill)
  EndMill,       // a flat or bull nose end mill
  EndMillOrVBit, // Contour on the line: also an engraving cutter or a chamfer mill
  Finishing,     // a flat, ball or bull nose end mill
};

// The cutters of an operation of `type` (the model's kind: `facing`,
// `contour`, `pocket`, ...) whose contour runs on `side` (`outside`,
// `inside`, `on`; other kinds ignore it).
Cutters cuttersOf(const QString& type, const QString& side);

// Whether `tool` is one of `cutters`.
bool toolFits(const QJsonObject& tool, Cutters cutters);

// Whether `tool` is an engraving cutter or a chamfer mill.
bool isVBit(const QJsonObject& tool);

// The tool of `number` among `tools`; empty when there is none.
QJsonObject toolOf(const QJsonArray& tools, int number);

// The tools the panel offers, in the machine's order: those of `cutters`,
// and the one of `current` too when it does not fit (a saved operation's,
// kept in sight to be replaced).
QJsonArray offeredTools(const QJsonArray& tools, Cutters cutters, int current);

// The number (as text) of the tool an operation starts with: a drill for
// Drill (else the first tool), a ball end mill for the finishing when there
// is one, else the first tool that fits; empty when none fits.
QString defaultTool(const QJsonArray& tools, const QString& type, const QString& side);

} // namespace mitcad::cam
