// SPDX-License-Identifier: MIT
#pragma once

// Continuity Analysis (mitcad#2521, app/COMMANDS.md "Continuity
// Analysis"): the model's `edge_continuity` query (commands.md,
// "Continuity analysis") asked for the picked edges, the edges of the
// picked faces and every edge of the picked bodies, and its answer in
// words, in the log's line and as what the view draws: each edge in the
// colour of the class it reaches. Plain Qt and OCCT: the command
// (InspectCommands.cpp) and the unit tests share it.

#include <array>
#include <utility>
#include <vector>

#include <QColor>
#include <QJsonArray>
#include <QJsonObject>
#include <QString>

#include <TopoDS_Shape.hxx>

#include "../framework/Selection.hpp"
#include "../framework/Theme.hpp"

namespace mitcad {

// The query's defaults (the model's own): samples along an edge, the gap
// (mm), the angle (degrees) and the curvature difference (percent).
constexpr int kDefaultContinuitySamples = 16;
constexpr double kDefaultContinuityGap = 0.01;
constexpr double kDefaultContinuityAngle = 0.1;
constexpr double kDefaultContinuityCurvature = 2.0;
// The worst edges the panel lists.
constexpr int kContinuityListed = 8;

// The classes an edge reaches, in the answer's words, from the best:
// "g2", "g1", "g0", "none"; the colour roles they are drawn in.
extern const std::array<const char*, 4> kContinuityClasses;
ColorRole continuityRole(const QString& className);

// The query's `bodies` from the picks: per body and occurrence, in the
// order first picked, the edges and the faces picked; a body picked whole
// asks for every edge of it (what else of it was picked is then left out).
// Picks of other kinds are left out.
QJsonArray continuityBodies(const Selection& picks);

// The `edge_continuity` query of those bodies with the settings.
QJsonObject continuityQuery(const QJsonArray& bodies, int samples, double gap, double angle, double curvature);

// The panel's result area: the tolerances, each class with its swatch
// (`colors` in kContinuityClasses' order) and count, the edges not
// measured, the worst values and the worst edges (at most
// kContinuityListed), each named by its body and its place in the answer
// ("Body1, edge 3"), never by its topological name.
QString continuityText(const QJsonObject& answer, const std::array<QColor, 4>& colors);
// The log's words (UI tests): "12 edges: g2 0, g1 2, g0 10, none 0,
// unmeasured 0; worst gap 0.0000 mm, angle 90.000 deg, curvature 100.0 %".
QString continuityLog(const QJsonObject& answer);
// What the view draws: per class reached (kContinuityClasses' order, those
// with edges only) the edges' polylines through their sampled points, in
// the class's colour role; edges not measured are not drawn.
std::vector<std::pair<TopoDS_Shape, ColorRole>> continuityEdges(const QJsonObject& answer);

} // namespace mitcad
