// SPDX-License-Identifier: MIT
// Comments for AI agents (mitcad#1119): the `comments` query's answer read
// into the comments' states, their pins' looks (open, dimmed, warning,
// hidden), the Comments panel's filter, the UI tests' log lines, the
// targets that picks of the view stand for, and the pin's picture and its
// click area.
#include "view/Comments.hpp"
#include "framework/Selection.hpp"
#include "framework/Theme.hpp"

#include <algorithm>
#include <array>
#include <cstdio>
#include <cstdlib>
#include <optional>

#include <QColor>
#include <QImage>
#include <QJsonArray>
#include <QJsonObject>
#include <QPainter>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "comments.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

using namespace mitcad;

int channelGap(const QColor& a, const QColor& b) {
  return std::max({std::abs(a.red() - b.red()), std::abs(a.green() - b.green()), std::abs(a.blue() - b.blue())});
}

QJsonArray xyz(double x, double y, double z) {
  QJsonArray point;
  point.append(x);
  point.append(y);
  point.append(z);
  return point;
}

QJsonObject comment(const char* id, const char* status, const QJsonValue& place, bool orphaned = false) {
  return {{QStringLiteral("id"), QString::fromLatin1(id)},
          {QStringLiteral("text"), QStringLiteral("make this hole\nan M6 thread")},
          {QStringLiteral("target"), QJsonObject{{QStringLiteral("body"), QStringLiteral("F2.b0")},
                                                 {QStringLiteral("face"), QStringLiteral("F2:end(r1)")}}},
          {QStringLiteral("status"), QString::fromLatin1(status)},
          {QStringLiteral("orphaned"), orphaned},
          {QStringLiteral("author"), QStringLiteral("user")},
          {QStringLiteral("resolution"), QJsonValue()},
          {QStringLiteral("replies"), QJsonArray()},
          {QStringLiteral("created"), QStringLiteral("2026-11-01T17:59:00Z")},
          {QStringLiteral("label"), QStringLiteral("Body1 › face")},
          {QStringLiteral("place"), place}};
}

QJsonObject answer() {
  QJsonArray comments;
  comments.append(comment("C1", "open", xyz(15.0, 10.0, 10.0)));
  QJsonObject addressed = comment("C2", "addressed", xyz(0.0, -2.5, 1.0));
  addressed.insert(QStringLiteral("resolution"), QStringLiteral("Threaded the hole M6"));
  addressed.insert(QStringLiteral("author"), QStringLiteral("claude-code"));
  QJsonArray replies;
  replies.append(QJsonObject{{QStringLiteral("author"), QStringLiteral("user")},
                             {QStringLiteral("text"), QStringLiteral("and countersink it")},
                             {QStringLiteral("created"), QStringLiteral("2026-11-01T18:00:00Z")}});
  addressed.insert(QStringLiteral("replies"), replies);
  comments.append(addressed);
  comments.append(comment("C3", "resolved", xyz(1.0, 2.0, 3.0)));
  comments.append(comment("C4", "open", xyz(4.0, 5.0, 6.0), true));
  QJsonObject parameter = comment("C12", "open", QJsonValue());
  parameter.insert(QStringLiteral("target"), QJsonObject{{QStringLiteral("parameter"), QStringLiteral("width")}});
  parameter.insert(QStringLiteral("label"), QStringLiteral("width"));
  comments.append(parameter);
  return {{QStringLiteral("comments"), comments}, {QStringLiteral("open"), 3}, {QStringLiteral("revision"), 7}};
}

void readTests() {
  CHECK(commentsOf(QJsonObject()).comments.isEmpty());
  CHECK(commentsOf(QJsonObject()).open == 0);
  const CommentsShown shown = commentsOf(answer());
  CHECK(shown.open == 3);
  CHECK(shown.comments.size() == 5);
  const CommentShown* first = commentById(shown.comments, QStringLiteral("C1"));
  CHECK(first != nullptr);
  if (first != nullptr) {
    CHECK(first->number == 1);
    CHECK(first->status == QStringLiteral("open"));
    CHECK(first->place.has_value() && (*first->place)[0] == 15.0 && (*first->place)[2] == 10.0);
    CHECK(first->resolution.isEmpty());
    CHECK(first->target.value(QStringLiteral("face")).toString() == QStringLiteral("F2:end(r1)"));
  }
  const CommentShown* second = commentById(shown.comments, QStringLiteral("C2"));
  CHECK(second != nullptr);
  if (second != nullptr) {
    CHECK(second->resolution == QStringLiteral("Threaded the hole M6"));
    CHECK(second->author == QStringLiteral("claude-code"));
    CHECK(second->replies.size() == 1 && second->replies.first().text == QStringLiteral("and countersink it"));
  }
  const CommentShown* twelfth = commentById(shown.comments, QStringLiteral("C12"));
  CHECK(twelfth != nullptr && twelfth->number == 12 && !twelfth->place.has_value());
  CHECK(commentById(shown.comments, QStringLiteral("C9")) == nullptr);

  CHECK(commentNumber(QStringLiteral("C7")) == 7);
  CHECK(commentNumber(QStringLiteral("C123")) == 123);
  CHECK(commentNumber(QStringLiteral("C")) == 0);
  CHECK(commentNumber(QStringLiteral("C0")) == 0);
  CHECK(commentNumber(QStringLiteral("X3")) == 0);
  CHECK(commentNumber(QStringLiteral("Cx")) == 0);
}

void lookTests() {
  const CommentsShown shown = commentsOf(answer());
  const auto look = [&shown](const char* id) {
    return commentPinLook(*commentById(shown.comments, QString::fromLatin1(id)));
  };
  CHECK(look("C1") == CommentPinLook::Open);
  CHECK(look("C2") == CommentPinLook::Addressed);
  CHECK(look("C3") == CommentPinLook::Hidden);   // resolved
  CHECK(look("C4") == CommentPinLook::Orphaned); // its target gone
  CHECK(look("C12") == CommentPinLook::Hidden);  // a parameter: no place
  CHECK(commentPinLookName(CommentPinLook::Open) == QStringLiteral("open"));
  CHECK(commentPinLookName(CommentPinLook::Addressed) == QStringLiteral("dimmed"));
  CHECK(commentPinLookName(CommentPinLook::Orphaned) == QStringLiteral("warning"));
  CHECK(commentPinLookName(CommentPinLook::Hidden) == QStringLiteral("hidden"));

  // The panel's filter: the open and the addressed ones at first.
  CommentFilter filter;
  CHECK(filter.text() == QStringLiteral("Open, Addressed"));
  CHECK(filter.shows(*commentById(shown.comments, QStringLiteral("C1"))));
  CHECK(filter.shows(*commentById(shown.comments, QStringLiteral("C2"))));
  CHECK(!filter.shows(*commentById(shown.comments, QStringLiteral("C3"))));
  filter.open = false;
  filter.addressed = false;
  filter.resolved = true;
  CHECK(filter.text() == QStringLiteral("Resolved"));
  CHECK(!filter.shows(*commentById(shown.comments, QStringLiteral("C1"))));
  CHECK(filter.shows(*commentById(shown.comments, QStringLiteral("C3"))));
  filter.resolved = false;
  CHECK(filter.text() == QStringLiteral("none"));

  // The tooltip: the number, the author and the text; what it is on; the
  // resolution; the replies.
  const QString tip = commentTip(*commentById(shown.comments, QStringLiteral("C2")));
  CHECK(tip.contains(QStringLiteral("Comment 2 (Addressed), claude-code: make this hole")));
  CHECK(tip.contains(QStringLiteral("On: Body1 › face")));
  CHECK(tip.contains(QStringLiteral("Resolution: Threaded the hole M6")));
  CHECK(tip.contains(QStringLiteral("user: and countersink it")));
  CHECK(commentTip(*commentById(shown.comments, QStringLiteral("C4"))).contains(QStringLiteral("Retarget")));
  CHECK(commentStatusText(*commentById(shown.comments, QStringLiteral("C4"))) == QStringLiteral("Open, target gone"));
}

void logTests() {
  CHECK(commentLogLines(CommentsShown()) ==
        QStringList({QStringLiteral("Comments: none"), QStringLiteral("Comments open: 0")}));
  const QStringList lines = commentLogLines(commentsOf(answer()));
  CHECK(lines.contains(QStringLiteral("Comment C1: open, user, Body1 › face: make this hole / an M6 thread")));
  CHECK(lines.contains(QStringLiteral("Comment C1 pin: open at 15.000, 10.000, 10.000")));
  CHECK(lines.contains(QStringLiteral("Comment C2 resolution: Threaded the hole M6")));
  CHECK(lines.contains(QStringLiteral("Comment C2 reply 1: user: and countersink it")));
  CHECK(lines.contains(QStringLiteral("Comment C2 pin: dimmed at 0.000, -2.500, 1.000")));
  CHECK(lines.contains(QStringLiteral("Comment C3 pin: hidden at 1.000, 2.000, 3.000")));
  CHECK(lines.contains(QStringLiteral("Comment C4: open, orphaned, user, Body1 › face: make this hole / an M6 thread")));
  CHECK(lines.contains(QStringLiteral("Comment C4 pin: warning at 4.000, 5.000, 6.000")));
  CHECK(lines.contains(QStringLiteral("Comment C12 pin: none")));
  CHECK(lines.last() == QStringLiteral("Comments open: 3"));
}

void targetTests() {
  // A face in an occurrence.
  SelectionItem face{SelectKind::Face, QStringLiteral("F3.b0"), QStringLiteral("F3:side(c1)"), QStringLiteral("plane"),
                     QStringLiteral("O1")};
  std::optional<QJsonObject> target = commentTargetOf(face, QStringLiteral("C1"), false);
  CHECK(target.has_value());
  if (target) {
    CHECK(target->value(QStringLiteral("body")).toString() == QStringLiteral("F3.b0"));
    CHECK(target->value(QStringLiteral("face")).toString() == QStringLiteral("F3:side(c1)"));
    CHECK(target->value(QStringLiteral("occurrence")).toString() == QStringLiteral("O1"));
    CHECK(!target->contains(QStringLiteral("at")));
  }
  // At the point clicked: a free point in the body's component, which the
  // occurrence places.
  QString why;
  CHECK(!commentTargetOf(face, QStringLiteral("C1"), true, &why).has_value());
  CHECK(why.contains(QStringLiteral("Click the point")));
  face.at = std::array<double, 3>{1.5, 2.0, -3.0};
  target = commentTargetOf(face, QStringLiteral("C1"), true);
  CHECK(target.has_value());
  if (target) {
    CHECK(!target->contains(QStringLiteral("body")));
    const QJsonArray at = target->value(QStringLiteral("at")).toArray();
    CHECK(at.size() == 3 && at.at(0).toDouble() == 1.5 && at.at(2).toDouble() == -3.0);
    CHECK(target->value(QStringLiteral("component")).toString() == QStringLiteral("C1"));
    CHECK(target->value(QStringLiteral("occurrence")).toString() == QStringLiteral("O1"));
  }
  // In the root: the root's component written empty, no occurrence.
  face.occurrence.clear();
  target = commentTargetOf(face, QStringLiteral("C0"), true);
  CHECK(target && target->value(QStringLiteral("component")).toString().isEmpty() &&
        !target->contains(QStringLiteral("occurrence")));
  // A body and a vertex stay themselves with At the Point Clicked.
  const SelectionItem body{SelectKind::Body, QStringLiteral("F2.b0"), QString(), QString(), QString()};
  target = commentTargetOf(body, QStringLiteral("C0"), true);
  CHECK(target && target->value(QStringLiteral("body")).toString() == QStringLiteral("F2.b0") &&
        target->size() == 1);
  const SelectionItem vertex{SelectKind::Vertex, QStringLiteral("F2.b0"), QStringLiteral("V{a|b|c}"), QString(),
                             QString()};
  target = commentTargetOf(vertex, QStringLiteral("C0"), true);
  CHECK(target && target->value(QStringLiteral("vertex")).toString() == QStringLiteral("V{a|b|c}"));
  const SelectionItem edge{SelectKind::Edge, QStringLiteral("F2.b0"), QStringLiteral("E{a|b}"), QString(), QString()};
  target = commentTargetOf(edge, QStringLiteral("C0"), false);
  CHECK(target && target->value(QStringLiteral("edge")).toString() == QStringLiteral("E{a|b}"));
  // Sketch entities, sketches and features, construction geometry.
  const SelectionItem curve{SelectKind::SketchCurve, QStringLiteral("F1"), QStringLiteral("c4"), QString(), QString()};
  target = commentTargetOf(curve, QString(), false);
  CHECK(target && target->value(QStringLiteral("sketch")).toString() == QStringLiteral("F1") &&
        target->value(QStringLiteral("curve")).toString() == QStringLiteral("c4"));
  const SelectionItem point{SelectKind::SketchPoint, QStringLiteral("F1"), QStringLiteral("p3"), QString(), QString()};
  target = commentTargetOf(point, QString(), false);
  CHECK(target && target->value(QStringLiteral("point")).toString() == QStringLiteral("p3"));
  const SelectionItem sketch{SelectKind::Sketch, QStringLiteral("F1"), QString(), QString(), QString()};
  target = commentTargetOf(sketch, QString(), false);
  CHECK(target && target->value(QStringLiteral("feature")).toString() == QStringLiteral("F1"));
  const SelectionItem plane{SelectKind::Plane, QStringLiteral("F5"), QString(), QString(), QString()};
  target = commentTargetOf(plane, QString(), false);
  CHECK(target && target->value(QStringLiteral("feature")).toString() == QStringLiteral("F5"));
  // The origin's datums, a component and nothing are no targets.
  for (const char* origin : {"xy", "z", "origin"}) {
    const SelectionItem datum{SelectKind::Plane, QString::fromLatin1(origin), QString(), QString(), QString()};
    CHECK(!commentTargetOf(datum, QString(), false).has_value());
  }
  const SelectionItem component{SelectKind::Component, QStringLiteral("C1"), QString(), QString(), QStringLiteral("O1")};
  why.clear();
  CHECK(!commentTargetOf(component, QString(), false, &why).has_value());
  CHECK(!why.isEmpty());
  CHECK(!commentTargetOf(SelectionItem(), QString(), false).has_value());
}

void pinTests() {
  // The head above the tip, the click area round both.
  const QPointF tip(30.0, 50.0);
  const QPointF head = commentPinHead(tip);
  CHECK(head.x() == 30.0 && head.y() == 50.0 - kCommentPinStem - kCommentPinRadius);
  CHECK(commentPinHit(tip, head));
  CHECK(commentPinHit(tip, head + QPointF(kCommentPinRadius, 0.0)));
  CHECK(commentPinHit(tip, tip - QPointF(0.0, 2.0)));
  CHECK(!commentPinHit(tip, tip + QPointF(0.0, 6.0)));
  CHECK(!commentPinHit(tip, head + QPointF(kCommentPinRadius + 6.0, 0.0)));
  CHECK(commentPinArea(tip).contains(head) && commentPinArea(tip).contains(tip));

  // The warning's look apart from the open one; the ink stands out.
  for (const bool dark : {false, true}) {
    CHECK(channelGap(commentPinFill(CommentPinLook::Open, dark), commentPinFill(CommentPinLook::Orphaned, dark)) > 40);
    CHECK(channelGap(commentPinFill(CommentPinLook::Open, dark), commentPinInk(CommentPinLook::Open, dark)) > 60);
    CHECK(channelGap(commentPinHaloColor(dark), viewColor(ColorRole::SketchHalo, dark)) == 0);
  }

  // Painted: the head's fill beside the number, opaque when open, dimmed
  // when addressed, nothing when hidden; a ring when highlighted.
  const auto painted = [&tip](CommentPinLook look, bool highlighted) {
    QImage image(60, 64, QImage::Format_ARGB32_Premultiplied);
    image.fill(Qt::transparent);
    QPainter painter(&image);
    paintCommentPin(painter, tip, 3, look, false, highlighted);
    painter.end();
    return image;
  };
  const QPoint beside = (head + QPointF(kCommentPinRadius * 0.7, 0.0)).toPoint();
  const QImage open = painted(CommentPinLook::Open, false);
  CHECK(open.pixelColor(beside).alpha() == 255);
  CHECK(channelGap(open.pixelColor(beside), commentPinFill(CommentPinLook::Open, false)) <= 3);
  CHECK(open.pixelColor(0, 0).alpha() == 0);
  // The stem reaches down to the tip.
  CHECK(open.pixelColor(tip.toPoint() - QPoint(0, 3)).alpha() > 0);
  const QImage dimmed = painted(CommentPinLook::Addressed, false);
  const int alpha = dimmed.pixelColor(beside).alpha();
  CHECK(alpha > 60 && alpha < 120);
  const QImage warning = painted(CommentPinLook::Orphaned, false);
  CHECK(channelGap(warning.pixelColor(beside), commentPinFill(CommentPinLook::Orphaned, false)) <= 3);
  const QImage hidden = painted(CommentPinLook::Hidden, false);
  CHECK(hidden.pixelColor(head.toPoint()).alpha() == 0 && hidden.pixelColor(beside).alpha() == 0);
  const QImage ringed = painted(CommentPinLook::Open, true);
  const QPoint ring = (head + QPointF(kCommentPinRadius + kCommentPinHalo + 1.5, 0.0)).toPoint();
  CHECK(channelGap(ringed.pixelColor(ring), viewColor(ColorRole::Select, false)) < 60);
  CHECK(open.pixelColor(ring) != ringed.pixelColor(ring));
}

} // namespace

int commentsTests() {
  readTests();
  lookTests();
  logTests();
  targetTests();
  pinTests();
  return failures;
}
