// SPDX-License-Identifier: MIT
#include "Comments.hpp"

#include <cmath>

#include <QFont>
#include <QJsonArray>
#include <QJsonValue>
#include <QObject>
#include <QPainter>
#include <QPainterPath>
#include <QPen>
#include <QSettings>

#include "../framework/Selection.hpp"
#include "../framework/Theme.hpp"

namespace mitcad {
namespace {

// The origin's datums have no feature of their own to comment on.
bool isOriginDatum(const QString& uid) {
  return uid == QLatin1String("xy") || uid == QLatin1String("xz") || uid == QLatin1String("yz") ||
         uid == QLatin1String("x") || uid == QLatin1String("y") || uid == QLatin1String("z") ||
         uid == QLatin1String("origin");
}

QJsonArray pointArray(const std::array<double, 3>& at) {
  QJsonArray point;
  point.append(at[0]);
  point.append(at[1]);
  point.append(at[2]);
  return point;
}

QString oneLine(QString text) { return text.replace(QLatin1Char('\n'), QStringLiteral(" / ")); }

} // namespace

CommentsShown commentsOf(const QJsonObject& answer) {
  CommentsShown shown;
  shown.open = answer.value(QStringLiteral("open")).toInt();
  for (const QJsonValue& value : answer.value(QStringLiteral("comments")).toArray()) {
    const QJsonObject entry = value.toObject();
    CommentShown comment;
    comment.id = entry.value(QStringLiteral("id")).toString();
    if (comment.id.isEmpty()) {
      continue;
    }
    comment.number = commentNumber(comment.id);
    comment.text = entry.value(QStringLiteral("text")).toString();
    comment.status = entry.value(QStringLiteral("status")).toString(QStringLiteral("open"));
    comment.orphaned = entry.value(QStringLiteral("orphaned")).toBool();
    comment.author = entry.value(QStringLiteral("author")).toString(QStringLiteral("user"));
    comment.resolution = entry.value(QStringLiteral("resolution")).toString();
    comment.created = entry.value(QStringLiteral("created")).toString();
    comment.label = entry.value(QStringLiteral("label")).toString();
    comment.target = entry.value(QStringLiteral("target")).toObject();
    for (const QJsonValue& reply : entry.value(QStringLiteral("replies")).toArray()) {
      const QJsonObject fields = reply.toObject();
      comment.replies.append({fields.value(QStringLiteral("author")).toString(QStringLiteral("user")),
                              fields.value(QStringLiteral("text")).toString(),
                              fields.value(QStringLiteral("created")).toString()});
    }
    const QJsonArray place = entry.value(QStringLiteral("place")).toArray();
    if (place.size() == 3) {
      const std::array<double, 3> at{place.at(0).toDouble(), place.at(1).toDouble(), place.at(2).toDouble()};
      if (std::isfinite(at[0]) && std::isfinite(at[1]) && std::isfinite(at[2])) {
        comment.place = at;
      }
    }
    shown.comments.append(comment);
  }
  return shown;
}

int commentNumber(const QString& id) {
  if (id.size() < 2 || !id.startsWith(QLatin1Char('C'))) {
    return 0;
  }
  bool ok = false;
  const int number = id.mid(1).toInt(&ok);
  return ok && number > 0 ? number : 0;
}

const CommentShown* commentById(const QVector<CommentShown>& comments, const QString& id) {
  for (const CommentShown& comment : comments) {
    if (comment.id == id) {
      return &comment;
    }
  }
  return nullptr;
}

CommentPinLook commentPinLook(const CommentShown& comment) {
  if (comment.status == QLatin1String("resolved") || !comment.place) {
    return CommentPinLook::Hidden;
  }
  if (comment.orphaned) {
    return CommentPinLook::Orphaned;
  }
  return comment.status == QLatin1String("addressed") ? CommentPinLook::Addressed : CommentPinLook::Open;
}

QString commentPinLookName(CommentPinLook look) {
  switch (look) {
  case CommentPinLook::Open:
    return QStringLiteral("open");
  case CommentPinLook::Addressed:
    return QStringLiteral("dimmed");
  case CommentPinLook::Orphaned:
    return QStringLiteral("warning");
  case CommentPinLook::Hidden:
    break;
  }
  return QStringLiteral("hidden");
}

bool CommentFilter::shows(const CommentShown& comment) const {
  if (comment.status == QLatin1String("resolved")) {
    return resolved;
  }
  if (comment.status == QLatin1String("addressed")) {
    return addressed;
  }
  return open;
}

QString CommentFilter::text() const {
  QStringList chosen;
  if (open) {
    chosen << QStringLiteral("Open");
  }
  if (addressed) {
    chosen << QStringLiteral("Addressed");
  }
  if (resolved) {
    chosen << QStringLiteral("Resolved");
  }
  return chosen.isEmpty() ? QStringLiteral("none") : chosen.join(QStringLiteral(", "));
}

QString commentStatusText(const CommentShown& comment) {
  QString status = comment.status == QLatin1String("resolved")    ? QObject::tr("Resolved")
                   : comment.status == QLatin1String("addressed") ? QObject::tr("Addressed")
                                                                  : QObject::tr("Open");
  if (comment.orphaned) {
    status = QObject::tr("%1, target gone").arg(status);
  }
  return status;
}

QString commentTip(const CommentShown& comment) {
  QStringList lines;
  lines << QObject::tr("Comment %1 (%2), %3: %4")
               .arg(QString::number(comment.number), commentStatusText(comment), comment.author, comment.text);
  if (!comment.label.isEmpty()) {
    lines << QObject::tr("On: %1").arg(comment.label);
  }
  if (comment.orphaned) {
    lines << QObject::tr("Its target is no longer in the design: Retarget it in the Comments panel.");
  }
  if (!comment.resolution.isEmpty()) {
    lines << QObject::tr("Resolution: %1").arg(comment.resolution);
  }
  for (const CommentShown::Reply& reply : comment.replies) {
    lines << QObject::tr("%1: %2").arg(reply.author, reply.text);
  }
  return lines.join(QLatin1Char('\n'));
}

QStringList commentLogLines(const CommentsShown& shown) {
  QStringList lines;
  if (shown.comments.isEmpty()) {
    lines << QStringLiteral("Comments: none");
  }
  for (const CommentShown& comment : shown.comments) {
    lines << QStringLiteral("Comment %1: %2%3, %4, %5: %6")
                 .arg(comment.id, comment.status, comment.orphaned ? QStringLiteral(", orphaned") : QString(),
                      comment.author, comment.label.isEmpty() ? QStringLiteral("-") : comment.label,
                      oneLine(comment.text));
    if (!comment.resolution.isEmpty()) {
      lines << QStringLiteral("Comment %1 resolution: %2").arg(comment.id, oneLine(comment.resolution));
    }
    for (int i = 0; i < comment.replies.size(); ++i) {
      const CommentShown::Reply& reply = comment.replies.at(i);
      lines << QStringLiteral("Comment %1 reply %2: %3: %4")
                   .arg(comment.id, QString::number(i + 1), reply.author, oneLine(reply.text));
    }
    const CommentPinLook look = commentPinLook(comment);
    if (comment.place) {
      const std::array<double, 3>& at = *comment.place;
      lines << QStringLiteral("Comment %1 pin: %2 at %3, %4, %5")
                   .arg(comment.id, commentPinLookName(look))
                   .arg(at[0], 0, 'f', 3)
                   .arg(at[1], 0, 'f', 3)
                   .arg(at[2], 0, 'f', 3);
    } else {
      lines << QStringLiteral("Comment %1 pin: none").arg(comment.id);
    }
  }
  lines << QStringLiteral("Comments open: %1").arg(shown.open);
  return lines;
}

std::optional<QJsonObject> commentTargetOf(const SelectionItem& item, const QString& component, bool atPoint,
                                           QString* why) {
  const auto refuse = [why](const QString& reason) -> std::optional<QJsonObject> {
    if (why != nullptr) {
      *why = reason;
    }
    return std::nullopt;
  };
  QJsonObject target;
  switch (item.kind) {
  case SelectKind::Face:
  case SelectKind::Edge:
  case SelectKind::Vertex:
  case SelectKind::Body: {
    if (item.owner.isEmpty()) {
      return refuse(QObject::tr("Pick a face, an edge, a vertex or a body."));
    }
    if (atPoint && item.kind != SelectKind::Body && item.kind != SelectKind::Vertex) {
      if (!item.at) {
        return refuse(QObject::tr("Click the point on the face or the edge in the view."));
      }
      target.insert(QStringLiteral("at"), pointArray(*item.at));
      target.insert(QStringLiteral("component"),
                    component == QLatin1String("C0") ? QString() : component);
      if (!item.occurrence.isEmpty()) {
        target.insert(QStringLiteral("occurrence"), item.occurrence);
      }
      return target;
    }
    target.insert(QStringLiteral("body"), item.owner);
    if (item.kind == SelectKind::Face) {
      target.insert(QStringLiteral("face"), item.name);
    } else if (item.kind == SelectKind::Edge) {
      target.insert(QStringLiteral("edge"), item.name);
    } else if (item.kind == SelectKind::Vertex) {
      target.insert(QStringLiteral("vertex"), item.name);
    }
    if (!item.occurrence.isEmpty()) {
      target.insert(QStringLiteral("occurrence"), item.occurrence);
    }
    return target;
  }
  case SelectKind::SketchCurve:
  case SelectKind::SketchPoint:
    if (item.owner.isEmpty() || item.name.isEmpty()) {
      return refuse(QObject::tr("Pick a sketch's curve or point."));
    }
    target.insert(QStringLiteral("sketch"), item.owner);
    target.insert(item.kind == SelectKind::SketchCurve ? QStringLiteral("curve") : QStringLiteral("point"),
                  item.name);
    return target;
  case SelectKind::Sketch:
  case SelectKind::Feature:
    if (item.owner.isEmpty()) {
      return refuse(QObject::tr("Pick a feature."));
    }
    target.insert(QStringLiteral("feature"), item.owner);
    return target;
  case SelectKind::Plane:
  case SelectKind::Axis:
  case SelectKind::Point:
    if (item.owner.isEmpty() || isOriginDatum(item.owner)) {
      return refuse(QObject::tr("The origin's planes, axes and point take no comments: pick something of the "
                                "design."));
    }
    target.insert(QStringLiteral("feature"), item.owner);
    return target;
  default:
    break;
  }
  return refuse(QObject::tr("Pick a face, an edge, a body, a point on a face or a feature."));
}

bool agentsMayResolveComments() {
  return QSettings().value(QLatin1String(kAgentsMayResolveCommentsKey), false).toBool();
}

void saveAgentsMayResolveComments(bool on) { QSettings().setValue(QLatin1String(kAgentsMayResolveCommentsKey), on); }

bool commentPinsShown() { return QSettings().value(QLatin1String(kCommentPinsShownKey), true).toBool(); }

void saveCommentPinsShown(bool shown) { QSettings().setValue(QLatin1String(kCommentPinsShownKey), shown); }

QPointF commentPinHead(const QPointF& tip) {
  return tip - QPointF(0.0, kCommentPinStem + kCommentPinRadius);
}

QRectF commentPinArea(const QPointF& tip) {
  const QPointF head = commentPinHead(tip);
  const double reach = kCommentPinRadius + kCommentPinHalo + 1.0;
  return QRectF(head.x() - reach, head.y() - reach, 2.0 * reach, tip.y() - head.y() + reach + 1.0);
}

bool commentPinHit(const QPointF& tip, const QPointF& at) {
  const QPointF head = commentPinHead(tip);
  if (std::hypot(at.x() - head.x(), at.y() - head.y()) <= kCommentPinRadius + kCommentPinHalo + 1.0) {
    return true;
  }
  // The stem, a few pixels wide.
  return std::abs(at.x() - tip.x()) <= 4.0 && at.y() <= tip.y() + 1.0 && at.y() >= head.y();
}

QColor commentPinFill(CommentPinLook look, bool darkBackground) {
  return look == CommentPinLook::Orphaned ? viewColor(ColorRole::WarningBadge, darkBackground)
                                          : viewColor(ColorRole::PreviewTarget, darkBackground);
}

QColor commentPinInk(CommentPinLook look, bool darkBackground) {
  return look == CommentPinLook::Orphaned ? viewColor(ColorRole::WarningMark, darkBackground)
                                          : viewColor(ColorRole::BadgeMark, darkBackground);
}

QColor commentPinHaloColor(bool darkBackground) { return viewColor(ColorRole::SketchHalo, darkBackground); }

void paintCommentPin(QPainter& painter, const QPointF& tip, int number, CommentPinLook look, bool darkBackground,
                     bool highlighted) {
  if (look == CommentPinLook::Hidden) {
    return;
  }
  painter.save();
  painter.setRenderHint(QPainter::Antialiasing);
  if (look == CommentPinLook::Addressed) {
    painter.setOpacity(painter.opacity() * kStateDimOpacity);
  }
  const QPointF head = commentPinHead(tip);
  const double r = kCommentPinRadius;
  // The head and the stem as one outline: a circle drawn down to a point.
  QPainterPath pin;
  pin.addEllipse(head, r, r);
  QPainterPath stem;
  stem.moveTo(head.x() - r * 0.55, head.y() + r * 0.6);
  stem.lineTo(tip);
  stem.lineTo(head.x() + r * 0.55, head.y() + r * 0.6);
  stem.closeSubpath();
  pin = pin.united(stem);
  // The halo in the background's tone, so that it reads over any body.
  QPen halo(commentPinHaloColor(darkBackground), 2.0 * kCommentPinHalo);
  halo.setJoinStyle(Qt::RoundJoin);
  painter.setPen(halo);
  painter.setBrush(Qt::NoBrush);
  painter.drawPath(pin);
  const QColor fill = commentPinFill(look, darkBackground);
  painter.setPen(QPen(fill.darker(130), 1.0));
  painter.setBrush(fill);
  painter.drawPath(pin);
  if (highlighted) {
    QPen ring(viewColor(ColorRole::Select, darkBackground), 2.0);
    painter.setPen(ring);
    painter.setBrush(Qt::NoBrush);
    painter.drawEllipse(head, r + kCommentPinHalo + 1.5, r + kCommentPinHalo + 1.5);
  }
  // The number, as large as fits the head.
  const QString text = number > 0 ? QString::number(number) : QStringLiteral("?");
  QFont font = painter.font();
  font.setBold(true);
  font.setPixelSize(text.size() > 2 ? 8 : text.size() > 1 ? 10 : 12);
  painter.setFont(font);
  painter.setPen(commentPinInk(look, darkBackground));
  painter.drawText(QRectF(head.x() - r, head.y() - r, 2.0 * r, 2.0 * r), Qt::AlignCenter, text);
  painter.restore();
}

} // namespace mitcad
