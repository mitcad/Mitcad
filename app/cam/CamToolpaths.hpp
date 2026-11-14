// SPDX-License-Identifier: MIT
#pragma once

// Toolpaths in the 3D view (mitcad#1013, app/COMMANDS.md "The CNC group"):
// the moves of the `cam_toolpaths` query (work coordinates, millimetres)
// turned into line segments in the design's coordinates by the setup's
// work frame, sorted by how the tool moves (rapids, cuts, and the moves
// down into the material: plunges, ramps and helixes), and the tool tip's
// places along them for Simulate. Plain Qt Core and arithmetic: no view,
// no model.

#include <array>
#include <optional>
#include <utility>
#include <vector>

#include <QJsonArray>
#include <QJsonObject>
#include <QString>
#include <QStringList>

namespace mitcad::cam {

using Point3 = std::array<double, 3>;

// A setup's work frame in the design: the `work` object of `cam_setups`
// and `cam_toolpaths` (`origin`, `x`, `y`, `z` in model coordinates);
// invalid when the setup's frame could not be found (its `error`).
struct WorkFrame {
  Point3 origin{0.0, 0.0, 0.0};
  Point3 x{1.0, 0.0, 0.0};
  Point3 y{0.0, 1.0, 0.0};
  Point3 z{0.0, 0.0, 1.0};
  bool valid = false;

  static WorkFrame of(const QJsonObject& work);
  // A point of the work coordinates in the design's.
  Point3 toModel(const Point3& p) const;
  // A direction of the work coordinates in the design's.
  Point3 directionToModel(const Point3& d) const;
};

// How the tool moves along a segment.
enum class MoveStyle {
  Rapid, // through the air at the rapid rate (drawn dashed)
  Feed,  // a cut
  Entry, // a cut going down: a plunge, a ramp or a helix
};

// A toolpath's moves as segments (six numbers each: from x, y, z, to x, y,
// z; design coordinates), by style, with the moves counted.
struct PathLines {
  std::vector<float> rapids;
  std::vector<float> feeds;
  std::vector<float> entries;
  int moves = 0;
  int rapidMoves = 0;
  int feedMoves = 0;
  int entryMoves = 0;

  int segments() const { return static_cast<int>((rapids.size() + feeds.size() + entries.size()) / 6); }
};

// The style of a move from `from` to `to` (work coordinates): a rapid
// whatever its way, a cut that ends lower than it starts an entry, any
// other cut a feed.
MoveStyle styleOf(const QString& kind, const Point3& from, const Point3& to);

// The points of an arc move (work coordinates) after `from`, ending at
// `to`: about `center` (X and Y) in the XY plane, counter-clockwise when
// `ccw`, Z going evenly from `from`'s to `to`'s (a helix); `to` equal to
// `from` in X and Y is a full turn. At most 5 degrees apart.
std::vector<Point3> arcPoints(const Point3& from, const Point3& to, const std::array<double, 2>& center, bool ccw);

// The segments of a toolpath's moves (`cam_toolpaths`' `toolpath.moves`),
// starting where the tool is (`at`, work coordinates; none: the first
// move's end, its way there not drawn), which is left where the last move
// ends.
PathLines pathLines(const QJsonArray& moves, const WorkFrame& frame, std::optional<Point3>& at);

// A place of the tool tip along a setup's toolpaths, for Simulate: where
// (design coordinates), in which operation (its index in the playback's
// operations) and at which of its moves.
struct PlaybackPoint {
  Point3 at{0.0, 0.0, 0.0};
  int operation = 0;
  int move = 0;
};

// The tool tip's places along the moves (arcs at most 5 degrees apart),
// appended to `points`; `at` as for pathLines.
void appendPlayback(const QJsonArray& moves, const WorkFrame& frame, int operation, std::optional<Point3>& at,
                    std::vector<PlaybackPoint>& points);

// What the 3D view draws (OcctViewer::setCamToolpaths): groups of
// segments by their look, and the tool where Simulate has it. Empty: none.
struct ToolpathScene {
  enum class Look {
    Rapid,        // dashed, thin
    Feed,         // solid
    Entry,        // solid, the entries' colour
    Stale,        // an operation out of date: dotted, muted
    Current,      // the current operation's cuts: thicker, the selection's colour
    CurrentRapid, // its rapids
  };
  struct Group {
    Look look = Look::Feed;
    std::vector<float> segments;
  };
  // The tool as a cylinder: its tip, its axis (up, out of the material),
  // its diameter and the length drawn.
  struct Tool {
    Point3 tip{0.0, 0.0, 0.0};
    Point3 axis{0.0, 0.0, 1.0};
    double diameter = 0.0;
    double length = 0.0;
  };
  std::vector<Group> groups;
  std::optional<Tool> tool;

  bool empty() const;
  // "CAM toolpath overlay: 3 groups, 412 segments (rapid 40, feed 300,
  // entry 72); tool at 10, 20, 5", "CAM toolpath overlay: none": the view's
  // log line (UI tests).
  QString log() const;
};

// "Contour 1: 120 moves (rapid 12, feed 96, entry 12), 118 segments": an
// operation's line of the log.
QString pathLinesLog(const QString& name, const PathLines& lines);

// A number for the log, without trailing zeros ("12.5", "3").
QString shortNumber(double value);

} // namespace mitcad::cam
