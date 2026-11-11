// SPDX-License-Identifier: MIT
#include "TitleBlockEditor.hpp"

#include <algorithm>
#include <cmath>
#include <limits>

#include <QJsonArray>
#include <QJsonValue>

namespace mitcad::drawings {
namespace {

// The smallest block, mm.
constexpr double kMinBlock = 5.0;

QPointF pointOf(const QJsonValue& value) {
  const QJsonArray a = value.toArray();
  return {a.at(0).toDouble(), a.at(1).toDouble()};
}

QJsonArray arrayOf(const QPointF& p) { return QJsonArray{p.x(), p.y()}; }

// A cell's or an image's box from its `at` and `size`.
QRectF boxFrom(const QJsonObject& json) {
  const QPointF at = pointOf(json.value(QStringLiteral("at")));
  const QPointF size = pointOf(json.value(QStringLiteral("size")));
  return {at.x(), at.y(), size.x(), size.y()};
}

double segmentDistance(const QPointF& p, const QLineF& line) {
  const QPointF d = line.p2() - line.p1();
  const double length2 = d.x() * d.x() + d.y() * d.y();
  double t = 0.0;
  if (length2 > 0.0) {
    t = std::clamp(((p.x() - line.x1()) * d.x() + (p.y() - line.y1()) * d.y()) / length2, 0.0, 1.0);
  }
  const QPointF nearest = line.p1() + d * t;
  return std::hypot(p.x() - nearest.x(), p.y() - nearest.y());
}

// `value` no less than `low` and no more than `high` (`high` winning when
// they cross: an item already outside a block made smaller).
double bounded(double value, double low, double high) { return std::min(std::max(value, low), high); }

QString number(double value) { return QString::number(value, 'g', 6); }

QString pointText(const QPointF& p) { return QStringLiteral("%1,%2").arg(number(p.x()), number(p.y())); }

} // namespace

std::optional<TitleBlockEditor> TitleBlockEditor::fromSheet(const QJsonObject& sheet) {
  const QJsonObject layout = sheet.value(QStringLiteral("title_block_layout")).toObject();
  const QJsonArray frame = sheet.value(QStringLiteral("frame")).toArray();
  if (layout.isEmpty() || frame.size() != 2) {
    return std::nullopt;
  }
  TitleBlockEditor editor;
  editor.m_layout = layout;
  editor.m_width = std::max(kMinBlock, layout.value(QStringLiteral("width")).toDouble());
  editor.m_height = std::max(kMinBlock, layout.value(QStringLiteral("height")).toDouble());
  editor.m_right = pointOf(frame.at(1)).x();
  editor.m_bottom = pointOf(frame.at(0)).y();
  for (const QJsonValue& value : layout.value(QStringLiteral("lines")).toArray()) {
    const QJsonArray l = value.toArray();
    editor.m_lines.append(QLineF(l.at(0).toDouble(), l.at(1).toDouble(), l.at(2).toDouble(), l.at(3).toDouble()));
  }
  for (const QJsonValue& value : layout.value(QStringLiteral("fields")).toArray()) {
    const QJsonObject f = value.toObject();
    BlockField field;
    field.label = f.value(QStringLiteral("label")).toString();
    field.text = f.value(QStringLiteral("text")).toString();
    field.cell = boxFrom(f);
    field.height = f.value(QStringLiteral("height")).toDouble(0.0);
    editor.m_fields.append(field);
  }
  for (const QJsonValue& value : layout.value(QStringLiteral("images")).toArray()) {
    const QJsonObject i = value.toObject();
    BlockImage image;
    image.box = boxFrom(i);
    image.data = i.value(QStringLiteral("data")).toString();
    image.format = titleBlockImageFormat(QByteArray::fromBase64(image.data.toLatin1()));
    editor.m_images.append(image);
  }
  return editor;
}

QJsonObject TitleBlockEditor::toJson() const {
  QJsonObject out = m_layout;
  out.insert(QStringLiteral("width"), m_width);
  out.insert(QStringLiteral("height"), m_height);
  QJsonArray lines;
  for (const QLineF& l : m_lines) {
    lines.append(QJsonArray{l.x1(), l.y1(), l.x2(), l.y2()});
  }
  QJsonArray fields;
  for (const BlockField& f : m_fields) {
    QJsonObject field{{QStringLiteral("text"), f.text},
                      {QStringLiteral("at"), arrayOf(f.cell.topLeft())},
                      {QStringLiteral("size"), QJsonArray{f.cell.width(), f.cell.height()}}};
    if (!f.label.isEmpty()) {
      field.insert(QStringLiteral("label"), f.label);
    }
    if (f.height > 0.0) {
      field.insert(QStringLiteral("height"), f.height);
    }
    fields.append(field);
  }
  QJsonArray images;
  for (const BlockImage& i : m_images) {
    images.append(QJsonObject{{QStringLiteral("at"), arrayOf(i.box.topLeft())},
                              {QStringLiteral("size"), QJsonArray{i.box.width(), i.box.height()}},
                              {QStringLiteral("data"), i.data}});
  }
  out.insert(QStringLiteral("lines"), lines);
  out.insert(QStringLiteral("fields"), fields);
  out.insert(QStringLiteral("images"), images);
  return out;
}

void TitleBlockEditor::setSize(double w, double h) {
  const double newWidth = std::max(kMinBlock, w);
  const double newHeight = std::max(kMinBlock, h);
  if (newWidth != m_width || newHeight != m_height) {
    m_width = newWidth;
    m_height = newHeight;
    m_changed = true;
  }
}

QPointF TitleBlockEditor::origin() const { return {m_right - m_width, m_bottom}; }

void TitleBlockEditor::setGrid(double step) {
  if (step > 0.0 && std::isfinite(step)) {
    m_grid = step;
  }
}

QPointF TitleBlockEditor::clampedPoint(const QPointF& block) const {
  return {std::clamp(block.x(), 0.0, m_width), std::clamp(block.y(), 0.0, m_height)};
}

QPointF TitleBlockEditor::snapped(const QPointF& sheet) const {
  const QPointF block = toBlock(sheet);
  const QPointF onGrid(std::round(block.x() / m_grid) * m_grid, std::round(block.y() / m_grid) * m_grid);
  return clampedPoint(onGrid);
}

void TitleBlockEditor::setTool(Tool next) {
  m_tool = next;
  m_first.reset();
  m_moving = false;
}

void TitleBlockEditor::setPendingImage(const QString& base64, const QString& format) {
  m_pendingData = base64;
  m_pendingFormat = format;
  setTool(Tool::Image);
}

void TitleBlockEditor::select(Kind kind, int index) {
  const int count = kind == Kind::Line    ? static_cast<int>(m_lines.size())
                    : kind == Kind::Field ? static_cast<int>(m_fields.size())
                    : kind == Kind::Image ? static_cast<int>(m_images.size())
                                          : 0;
  if (index < 0 || index >= count) {
    m_selectedKind = Kind::None;
    m_selectedIndex = -1;
    return;
  }
  m_selectedKind = kind;
  m_selectedIndex = index;
}

QRectF TitleBlockEditor::boxOf(const QPointF& a, const QPointF& b) const {
  return QRectF(QPointF(std::min(a.x(), b.x()), std::min(a.y(), b.y())),
                QPointF(std::max(a.x(), b.x()), std::max(a.y(), b.y())));
}

std::pair<TitleBlockEditor::Kind, int> TitleBlockEditor::itemAt(const QPointF& block, double reach) const {
  // A line before what it crosses: it is the thinnest.
  int best = -1;
  double nearest = reach;
  for (int i = 0; i < static_cast<int>(m_lines.size()); ++i) {
    const double d = segmentDistance(block, m_lines.at(i));
    if (d <= nearest) {
      nearest = d;
      best = i;
    }
  }
  if (best >= 0) {
    return {Kind::Line, best};
  }
  for (int i = static_cast<int>(m_images.size()) - 1; i >= 0; --i) {
    if (m_images.at(i).box.contains(block)) {
      return {Kind::Image, i};
    }
  }
  // The smallest cell under it.
  double smallest = std::numeric_limits<double>::max();
  for (int i = 0; i < static_cast<int>(m_fields.size()); ++i) {
    const QRectF cell = m_fields.at(i).cell;
    const double area = cell.width() * cell.height();
    if (cell.contains(block) && area < smallest) {
      smallest = area;
      best = i;
    }
  }
  if (best >= 0) {
    return {Kind::Field, best};
  }
  return {Kind::None, -1};
}

QString TitleBlockEditor::press(const QPointF& sheet, double reach) {
  const QPointF at = snapped(sheet);
  m_pointer = at;
  if (m_tool == Tool::Select) {
    const auto [kind, index] = itemAt(clampedPoint(toBlock(sheet)), reach);
    select(kind, index);
    if (kind == Kind::None) {
      return QStringLiteral("nothing selected");
    }
    m_moving = true;
    m_moved = false;
    m_moveFrom = at;
    if (kind == Kind::Line) {
      m_lineBefore = m_lines.at(index);
    } else if (kind == Kind::Field) {
      m_boxBefore = m_fields.at(index).cell;
    } else {
      m_boxBefore = m_images.at(index).box;
    }
    const QString what = kind == Kind::Line    ? QStringLiteral("line")
                         : kind == Kind::Field ? QStringLiteral("field")
                                               : QStringLiteral("image");
    return QStringLiteral("%1 %2 selected").arg(what).arg(index + 1);
  }
  if (m_tool == Tool::Image && !hasPendingImage()) {
    return QStringLiteral("no image chosen");
  }
  if (!m_first) {
    m_first = at;
    return QStringLiteral("first point %1").arg(pointText(at));
  }
  const QPointF first = *m_first;
  m_first.reset();
  if (m_tool == Tool::Line) {
    if (first == at) {
      return QStringLiteral("line of no length");
    }
    m_lines.append(QLineF(first, at));
    m_changed = true;
    select(Kind::Line, static_cast<int>(m_lines.size()) - 1);
    return QStringLiteral("line added: %1 %2").arg(pointText(first), pointText(at));
  }
  const QRectF box = boxOf(first, at);
  if (box.width() <= 0.0 || box.height() <= 0.0) {
    return m_tool == Tool::Field ? QStringLiteral("field of no size") : QStringLiteral("image of no size");
  }
  if (m_tool == Tool::Field) {
    BlockField field;
    field.cell = box;
    m_fields.append(field);
    m_changed = true;
    select(Kind::Field, static_cast<int>(m_fields.size()) - 1);
    m_tool = Tool::Select;
    return QStringLiteral("field added: %1 %2x%3").arg(pointText(box.topLeft()), number(box.width()),
                                                         number(box.height()));
  }
  BlockImage image;
  image.box = box;
  image.data = m_pendingData;
  image.format = m_pendingFormat;
  m_images.append(image);
  m_pendingData.clear();
  m_pendingFormat.clear();
  m_changed = true;
  select(Kind::Image, static_cast<int>(m_images.size()) - 1);
  m_tool = Tool::Select;
  return QStringLiteral("image added: %1 %2 %3x%4")
      .arg(image.format, pointText(box.topLeft()), number(box.width()), number(box.height()));
}

void TitleBlockEditor::moveTo(const QPointF& sheet) {
  m_pointer = snapped(sheet);
  if (!m_moving) {
    return;
  }
  // By whole grid steps, kept inside the block.
  QPointF delta = m_pointer - m_moveFrom;
  if (m_selectedKind == Kind::Line) {
    const QLineF& before = m_lineBefore;
    const double minX = std::min(before.x1(), before.x2());
    const double maxX = std::max(before.x1(), before.x2());
    const double minY = std::min(before.y1(), before.y2());
    const double maxY = std::max(before.y1(), before.y2());
    delta.setX(bounded(delta.x(), -minX, m_width - maxX));
    delta.setY(bounded(delta.y(), -minY, m_height - maxY));
    m_lines[m_selectedIndex] = before.translated(delta);
  } else if (m_selectedKind == Kind::Field || m_selectedKind == Kind::Image) {
    const QRectF& before = m_boxBefore;
    delta.setX(bounded(delta.x(), -before.left(), m_width - before.right()));
    delta.setY(bounded(delta.y(), -before.top(), m_height - before.bottom()));
    if (m_selectedKind == Kind::Field) {
      m_fields[m_selectedIndex].cell = before.translated(delta);
    } else {
      m_images[m_selectedIndex].box = before.translated(delta);
    }
  }
  if (delta != QPointF()) {
    m_moved = true;
  }
}

QString TitleBlockEditor::release(const QPointF& sheet) {
  if (!m_moving) {
    return QString();
  }
  moveTo(sheet);
  m_moving = false;
  if (!m_moved) {
    return QString();
  }
  m_changed = true;
  if (m_selectedKind == Kind::Line) {
    const QLineF& l = m_lines.at(m_selectedIndex);
    return QStringLiteral("line %1 moved: %2 %3").arg(m_selectedIndex + 1).arg(pointText(l.p1()), pointText(l.p2()));
  }
  const QRectF box =
      m_selectedKind == Kind::Field ? m_fields.at(m_selectedIndex).cell : m_images.at(m_selectedIndex).box;
  return QStringLiteral("%1 %2 moved: %3")
      .arg(m_selectedKind == Kind::Field ? QStringLiteral("field") : QStringLiteral("image"))
      .arg(m_selectedIndex + 1)
      .arg(pointText(box.topLeft()));
}

bool TitleBlockEditor::escape() {
  if (m_first) {
    m_first.reset();
    return true;
  }
  if (m_tool != Tool::Select) {
    m_tool = Tool::Select;
    m_pendingData.clear();
    m_pendingFormat.clear();
    return true;
  }
  if (m_selectedKind != Kind::None) {
    select(Kind::None, -1);
    return true;
  }
  return false;
}

QString TitleBlockEditor::deleteSelected() {
  const int i = m_selectedIndex;
  QString what;
  switch (m_selectedKind) {
  case Kind::Line:
    m_lines.removeAt(i);
    what = QStringLiteral("line %1").arg(i + 1);
    break;
  case Kind::Field:
    m_fields.removeAt(i);
    what = QStringLiteral("field %1").arg(i + 1);
    break;
  case Kind::Image:
    m_images.removeAt(i);
    what = QStringLiteral("image %1").arg(i + 1);
    break;
  case Kind::None:
    return QString();
  }
  m_moving = false;
  m_changed = true;
  select(Kind::None, -1);
  return what;
}

bool TitleBlockEditor::setFieldLabel(const QString& value) {
  if (m_selectedKind != Kind::Field || m_fields.at(m_selectedIndex).label == value) {
    return false;
  }
  m_fields[m_selectedIndex].label = value;
  m_changed = true;
  return true;
}

bool TitleBlockEditor::setFieldText(const QString& value) {
  if (m_selectedKind != Kind::Field || m_fields.at(m_selectedIndex).text == value) {
    return false;
  }
  m_fields[m_selectedIndex].text = value;
  m_changed = true;
  return true;
}

bool TitleBlockEditor::setFieldHeight(double value) {
  const double h = value > 0.0 && std::isfinite(value) ? value : 0.0;
  if (m_selectedKind != Kind::Field || m_fields.at(m_selectedIndex).height == h) {
    return false;
  }
  m_fields[m_selectedIndex].height = h;
  m_changed = true;
  return true;
}

QString TitleBlockEditor::summary() const {
  const auto count = [](qsizetype n, const char* one, const char* many) {
    return QStringLiteral("%1 %2").arg(n).arg(QString::fromLatin1(n == 1 ? one : many));
  };
  return QStringLiteral("%1, %2, %3")
      .arg(count(m_lines.size(), "line", "lines"), count(m_fields.size(), "field", "fields"),
           count(m_images.size(), "image", "images"));
}

QString titleBlockToolName(TitleBlockEditor::Tool tool) {
  switch (tool) {
  case TitleBlockEditor::Tool::Line:
    return QStringLiteral("line");
  case TitleBlockEditor::Tool::Field:
    return QStringLiteral("field");
  case TitleBlockEditor::Tool::Image:
    return QStringLiteral("image");
  case TitleBlockEditor::Tool::Select:
    break;
  }
  return QStringLiteral("select");
}

QString titleBlockImageFormat(const QByteArray& bytes) {
  if (bytes.startsWith(QByteArray("\x89PNG\r\n\x1a\n", 8))) {
    return QStringLiteral("png");
  }
  if (bytes.startsWith(QByteArray("\xff\xd8\xff", 3))) {
    return QStringLiteral("jpeg");
  }
  const QByteArray head = bytes.left(4096);
  const qsizetype at = head.indexOf("<svg");
  if (at >= 0 && at + 4 < head.size()) {
    const char next = head.at(at + 4);
    if (next == ' ' || next == '\t' || next == '\n' || next == '\r' || next == '>' || next == '/') {
      return QStringLiteral("svg");
    }
  }
  return QString();
}

} // namespace mitcad::drawings
