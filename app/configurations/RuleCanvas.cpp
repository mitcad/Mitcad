// SPDX-License-Identifier: MIT
#include "RuleCanvas.hpp"

#include <algorithm>
#include <cmath>
#include <utility>

#include <QApplication>
#include <QComboBox>
#include <QCompleter>
#include <QContextMenuEvent>
#include <QDebug>
#include <QDialog>
#include <QDialogButtonBox>
#include <QFormLayout>
#include <QFontMetricsF>
#include <QGraphicsItem>
#include <QGraphicsScene>
#include <QGraphicsSimpleTextItem>
#include <QTreeWidgetItemIterator>
#include <QJsonDocument>
#include <QKeyEvent>
#include <QLabel>
#include <QLineEdit>
#include <QMenu>
#include <QMouseEvent>
#include <QPainter>
#include <QPainterPath>
#include <QPushButton>
#include <QScrollBar>
#include <QTimer>

#include "../framework/Theme.hpp"
#include "../framework/Dialogs.hpp"

namespace mitcad::rules {

namespace {

constexpr qreal kPad = 4.0;
constexpr qreal kGap = 5.0;
constexpr qreal kSocketWidth = 34.0;
constexpr qreal kAddWidth = 22.0;
constexpr qreal kSlotHeight = 6.0;
constexpr int kIdRole = Qt::UserRole + 1;

bool isPrefix(const BlockPath& prefix, const BlockPath& path) {
  if (prefix.section != path.section || prefix.indices.size() > path.indices.size()) {
    return false;
  }
  return std::equal(prefix.indices.begin(), prefix.indices.end(), path.indices.begin());
}

bool isLeaf(BlockKind kind) {
  return kind == BlockKind::Name || kind == BlockKind::Number || kind == BlockKind::Text ||
         kind == BlockKind::Column || kind == BlockKind::Formula;
}

QColor fillFor(BlockKind kind) {
  switch (kind) {
  case BlockKind::Name:
    return {0x2b, 0x86, 0x7e};
  case BlockKind::Number:
  case BlockKind::Text:
  case BlockKind::Column:
    return {0x3a, 0x8c, 0x4f};
  case BlockKind::Formula:
    return {0x69, 0x70, 0x7a};
  default:
    break;
  }
  if (isAction(kind)) {
    return {0xc0, 0x72, 0x1e};
  }
  return {0x3d, 0x6b, 0xc2}; // logic
}

QColor frameColor() {
  return {0x7b, 0x55, 0xbd};
}

// Lays a rule out as pieces: the frame first, then what is in it, each
// block before the blocks in it (the keyboard's order).
class Builder {
public:
  Builder(const Rule& rule, const QFontMetricsF& metrics, std::vector<Piece>& out)
      : m_rule(rule), m_metrics(metrics), m_out(out), m_line(std::ceil(metrics.height()) + 8.0) {}

  void build() {
    const qreal left = 8.0;
    const qreal top = 8.0;
    const std::size_t frame = add(Piece());
    qreal right = left + 160.0;
    qreal y = top + 6.0;
    qreal x = left + 10.0;
    // The when (or "always").
    {
      Piece label;
      label.role = Piece::Role::Label;
      label.field = m_rule.hasWhen ? QStringLiteral("when") : QStringLiteral("always");
      label.text = label.field;
      label.depth = 1;
      label.rect = QRectF(x, y, width(label.text) + 8.0, m_line);
      add(label);
      x += label.rect.width() + kGap;
      qreal height = m_line;
      if (m_rule.hasWhen) {
        const QSizeF size = value(m_rule.when, BlockPath{0, {}}, QPointF(x, y), 2);
        height = std::max(height, size.height());
        right = std::max(right, x + size.width() + 10.0);
      }
      y += height + 6.0;
    }
    std::vector<QRectF> holes;
    const auto section = [&](int index, const std::vector<Block>& actions, const QString& word) {
      Piece label;
      label.role = Piece::Role::Label;
      label.field = word;
      label.text = word;
      label.depth = 1;
      label.rect = QRectF(left + 10.0, y, width(word) + 8.0, m_line);
      add(label);
      y += m_line + 2.0;
      const qreal stackLeft = left + 28.0;
      const qreal stackTop = y;
      for (std::size_t i = 0; i < actions.size(); ++i) {
        Piece slot;
        slot.role = Piece::Role::Slot;
        slot.path = BlockPath{index, {static_cast<int>(i)}};
        slot.insert = true;
        slot.depth = 1;
        slot.rect = QRectF(stackLeft, y, 160.0, kSlotHeight);
        add(slot);
        y += kSlotHeight;
        const QSizeF size = action(actions[i], BlockPath{index, {static_cast<int>(i)}}, QPointF(stackLeft, y), 2);
        y += size.height();
        right = std::max(right, stackLeft + size.width() + 10.0);
      }
      Piece end;
      end.role = Piece::Role::Slot;
      end.path = BlockPath{index, {static_cast<int>(actions.size())}};
      end.insert = true;
      end.depth = 1;
      end.text = QStringLiteral("+ action");
      end.rect = QRectF(stackLeft, y + 3.0, width(end.text) + 16.0, m_line);
      add(end);
      y += m_line + 9.0;
      right = std::max(right, end.rect.right() + 10.0);
      holes.emplace_back(stackLeft - 4.0, stackTop - 2.0, 0.0, y - stackTop - 2.0);
    };
    section(1, m_rule.then, QStringLiteral("then"));
    if (m_rule.hasElse) {
      section(2, m_rule.otherwise, QStringLiteral("else"));
    }
    Piece& piece = m_out[frame];
    piece.role = Piece::Role::Frame;
    piece.depth = 0;
    piece.rect = QRectF(left, top, right - left, y - top + 4.0);
    for (QRectF& hole : holes) {
      hole.setRight(piece.rect.right() + 1.0);
      piece.holes.push_back(hole);
    }
  }

private:
  std::size_t add(Piece piece) {
    m_out.push_back(std::move(piece));
    return m_out.size() - 1;
  }

  qreal width(const QString& text) const { return m_metrics.horizontalAdvance(text); }

  // A row of a block: words, fields and the blocks in it, left to right.
  struct Row {
    qreal x = 0.0;
    qreal y = 0.0;
    qreal height = 0.0;
    std::vector<std::pair<QPointF, QString>> words;
  };

  void word(Row& row, const QPointF& at, const QString& text) {
    row.words.emplace_back(QPointF(row.x - at.x(), 0.0), text);
    row.x += width(text) + kGap;
  }

  void field(Row& row, const BlockPath& path, const QString& name, const QString& text, int depth) {
    Piece piece;
    piece.role = Piece::Role::Field;
    piece.path = path;
    piece.field = name;
    piece.text = text;
    piece.depth = depth;
    const qreal shown = width(text.isEmpty() ? name : text);
    piece.rect = QRectF(row.x, row.y, std::max(shown + 12.0, 36.0), m_line);
    add(piece);
    row.x += piece.rect.width() + kGap;
    row.height = std::max(row.height, m_line);
  }

  void child(Row& row, const Block& block, const BlockPath& path, int index, int depth) {
    BlockPath inner = path;
    inner.indices.push_back(index);
    const QSizeF size = value(block, inner, QPointF(row.x, row.y), depth);
    row.x += size.width() + kGap;
    row.height = std::max(row.height, size.height());
  }

  void addSocket(Row& row, const BlockPath& path, int index, int depth) {
    Piece piece;
    piece.role = Piece::Role::Add;
    piece.path = path;
    piece.path.indices.push_back(index);
    piece.insert = true;
    piece.depth = depth;
    piece.text = QStringLiteral("+");
    piece.rect = QRectF(row.x, row.y, kAddWidth, m_line);
    add(piece);
    row.x += kAddWidth + kGap;
    row.height = std::max(row.height, m_line);
  }

  QSizeF value(const Block& block, const BlockPath& path, const QPointF& at, int depth) {
    if (block.kind == BlockKind::Empty) {
      Piece socket;
      socket.role = Piece::Role::Socket;
      socket.path = path;
      socket.depth = depth;
      socket.rect = QRectF(at, QSizeF(kSocketWidth, m_line));
      add(socket);
      return socket.rect.size();
    }
    if (isLeaf(block.kind)) {
      Piece leaf;
      leaf.role = Piece::Role::Block;
      leaf.path = path;
      leaf.kind = block.kind;
      leaf.depth = depth;
      QString shown = block.kind == BlockKind::Text ? QStringLiteral("'%1'").arg(block.text)
                                                    : printExpression(block).trimmed();
      if (block.kind == BlockKind::Formula) {
        shown = m_metrics.elidedText(QStringLiteral("ƒ ") + block.text.trimmed(), Qt::ElideRight, 260.0);
      }
      if (shown.isEmpty()) {
        shown = QStringLiteral("…");
      }
      leaf.text = shown;
      leaf.words.emplace_back(QPointF(8.0, 0.0), shown);
      leaf.rect = QRectF(at, QSizeF(width(shown) + 16.0, m_line));
      add(leaf);
      return leaf.rect.size();
    }
    const std::size_t self = add(Piece());
    Row row;
    row.x = at.x() + kPad + 4.0;
    row.y = at.y() + kPad;
    row.height = m_line;
    const int inner = depth + 1;
    const int count = static_cast<int>(block.children.size());
    switch (block.kind) {
    case BlockKind::Compare:
      if (count == 2) {
        child(row, block.children[0], path, 0, inner);
        field(row, path, QStringLiteral("op"), operatorLabel(block.text), inner);
        child(row, block.children[1], path, 1, inner);
      }
      break;
    case BlockKind::And:
    case BlockKind::Or:
      word(row, at, kindLabel(block.kind));
      for (int i = 0; i < count; ++i) {
        child(row, block.children[static_cast<std::size_t>(i)], path, i, inner);
      }
      addSocket(row, path, count, inner);
      break;
    case BlockKind::Not:
      word(row, at, kindLabel(block.kind));
      for (int i = 0; i < count; ++i) {
        child(row, block.children[static_cast<std::size_t>(i)], path, i, inner);
      }
      break;
    case BlockKind::OneOf:
      if (count > 0) {
        child(row, block.children[0], path, 0, inner);
      }
      word(row, at, kindLabel(block.kind));
      for (int i = 1; i < count; ++i) {
        child(row, block.children[static_cast<std::size_t>(i)], path, i, inner);
      }
      addSocket(row, path, std::max(count, 1), inner);
      break;
    default:
      break;
    }
    Piece& piece = m_out[self];
    piece.role = Piece::Role::Block;
    piece.path = path;
    piece.kind = block.kind;
    piece.depth = depth;
    piece.words = std::move(row.words);
    piece.rect = QRectF(at, QSizeF(row.x - kGap + kPad - at.x(), row.height + 2.0 * kPad));
    return piece.rect.size();
  }

  QSizeF action(const Block& block, const BlockPath& path, const QPointF& at, int depth) {
    const std::size_t self = add(Piece());
    Row row;
    row.x = at.x() + kPad + 2.0;
    row.y = at.y() + kPad;
    row.height = m_line;
    const int inner = depth + 1;
    const int count = static_cast<int>(block.children.size());
    switch (block.kind) {
    case BlockKind::AllowOnly:
    case BlockKind::Exclude:
      word(row, at, kindLabel(block.kind));
      field(row, path, QStringLiteral("option"), block.text, inner);
      for (int i = 0; i < count; ++i) {
        child(row, block.children[static_cast<std::size_t>(i)], path, i, inner);
      }
      addSocket(row, path, count, inner);
      break;
    case BlockKind::Min:
    case BlockKind::Max:
    case BlockKind::Step:
      field(row, path, QStringLiteral("option"), block.text, inner);
      word(row, at, kindLabel(block.kind));
      if (count > 0) {
        child(row, block.children[0], path, 0, inner);
      }
      break;
    case BlockKind::Require:
      word(row, at, kindLabel(block.kind));
      if (count > 0) {
        child(row, block.children[0], path, 0, inner);
      }
      field(row, path, QStringLiteral("message"), block.message, inner);
      break;
    case BlockKind::Forbid:
      word(row, at, kindLabel(block.kind));
      field(row, path, QStringLiteral("message"), block.message, inner);
      break;
    case BlockKind::InTable: {
      word(row, at, kindLabel(block.kind));
      field(row, path, QStringLiteral("table"), block.text, inner);
      QStringList columns;
      for (auto it = block.data.begin(); it != block.data.end(); ++it) {
        columns << QStringLiteral("%1=%2").arg(it.key(), it.value().toString());
      }
      field(row, path, QStringLiteral("columns"), columns.join(QStringLiteral(", ")), inner);
      break;
    }
    default:
      word(row, at, kindLabel(block.kind));
      word(row, at,
           m_metrics.elidedText(actionText(block), Qt::ElideRight, 280.0));
      break;
    }
    Piece& piece = m_out[self];
    piece.role = Piece::Role::Block;
    piece.path = path;
    piece.kind = block.kind;
    piece.depth = depth;
    piece.words = std::move(row.words);
    piece.rect = QRectF(at, QSizeF(std::max(row.x - kGap + kPad - at.x(), 120.0), row.height + 2.0 * kPad));
    return piece.rect.size();
  }

  const Rule& m_rule;
  const QFontMetricsF& m_metrics;
  std::vector<Piece>& m_out;
  qreal m_line;
};

// A piece drawn: blocks by their group's colour and shape (a value block
// with a plug on its left, an action block with a notch above and a tab
// below), sockets and slots dashed, fields as small text fields.
class PieceItem : public QGraphicsItem {
public:
  PieceItem(Piece piece, const QPalette& palette, const QFont& font)
      : m_piece(std::move(piece)), m_palette(palette), m_font(font) {}

  void setState(bool focused, bool highlighted) {
    if (focused != m_focused || highlighted != m_highlighted) {
      m_focused = focused;
      m_highlighted = highlighted;
      update();
    }
  }

  QRectF boundingRect() const override { return m_piece.rect.adjusted(-8.0, -6.0, 6.0, 8.0); }

  void paint(QPainter* painter, const QStyleOptionGraphicsItem*, QWidget*) override {
    painter->setRenderHint(QPainter::Antialiasing);
    painter->setFont(m_font);
    const QRectF r = m_piece.rect;
    const QColor accent = actionColor(m_palette);
    const QColor light(Qt::white);
    switch (m_piece.role) {
    case Piece::Role::Frame: {
      QPainterPath path;
      path.addRoundedRect(r, 8.0, 8.0);
      for (const QRectF& hole : m_piece.holes) {
        QPainterPath cut;
        cut.addRoundedRect(hole, 4.0, 4.0);
        path = path.subtracted(cut);
      }
      painter->fillPath(path, frameColor());
      break;
    }
    case Piece::Role::Label: {
      QFont bold = m_font;
      bold.setBold(true);
      painter->setFont(bold);
      painter->setPen(light);
      painter->drawText(r.adjusted(4.0, 0.0, 0.0, 0.0), Qt::AlignVCenter | Qt::AlignLeft, m_piece.text);
      break;
    }
    case Piece::Role::Block: {
      const QColor fill = fillFor(m_piece.kind);
      const QPainterPath path = isAction(m_piece.kind) ? actionShape(r) : valueShape(r);
      painter->fillPath(path, fill);
      painter->setPen(QPen(fill.darker(135), 1.0));
      painter->drawPath(path);
      painter->setPen(light);
      for (const auto& [offset, text] : m_piece.words) {
        const QRectF at(r.left() + offset.x(), r.top(), r.width(), r.height());
        painter->drawText(at, Qt::AlignVCenter | Qt::AlignLeft, text);
      }
      break;
    }
    case Piece::Role::Socket:
    case Piece::Role::Add: {
      painter->setPen(QPen(m_palette.color(QPalette::Mid), 1.0, Qt::DashLine));
      painter->setBrush(m_palette.color(QPalette::Base));
      painter->drawRoundedRect(r, 4.0, 4.0);
      if (!m_piece.text.isEmpty()) {
        painter->setPen(m_palette.color(QPalette::Text));
        painter->drawText(r, Qt::AlignCenter, m_piece.text);
      }
      break;
    }
    case Piece::Role::Slot:
      if (!m_piece.text.isEmpty()) {
        painter->setPen(QPen(m_palette.color(QPalette::Mid), 1.0, Qt::DashLine));
        painter->setBrush(m_palette.color(QPalette::Base));
        painter->drawRoundedRect(r, 3.0, 3.0);
        painter->setPen(m_palette.color(QPalette::Text));
        painter->drawText(r, Qt::AlignCenter, m_piece.text);
      } else if (m_focused || m_highlighted) {
        painter->fillRect(QRectF(r.left(), r.center().y() - 1.5, r.width(), 3.0), accent);
      }
      break;
    case Piece::Role::Field: {
      painter->setPen(QPen(m_palette.color(QPalette::Mid), 1.0));
      painter->setBrush(m_palette.color(QPalette::Base));
      painter->drawRoundedRect(r, 3.0, 3.0);
      const bool empty = m_piece.text.isEmpty();
      painter->setPen(m_palette.color(empty ? QPalette::PlaceholderText : QPalette::Text));
      painter->drawText(r.adjusted(6.0, 0.0, -4.0, 0.0), Qt::AlignVCenter | Qt::AlignLeft,
                        empty ? m_piece.field : m_piece.text);
      break;
    }
    }
    if (m_highlighted && m_piece.role != Piece::Role::Frame) {
      QColor fill = accent;
      fill.setAlpha(70);
      painter->setPen(QPen(accent, 2.0));
      painter->setBrush(fill);
      painter->drawRoundedRect(r.adjusted(-2.0, -2.0, 2.0, 2.0), 5.0, 5.0);
    } else if (m_highlighted) {
      painter->setPen(QPen(accent, 3.0));
      painter->setBrush(Qt::NoBrush);
      painter->drawRoundedRect(r.adjusted(-2.0, -2.0, 2.0, 2.0), 9.0, 9.0);
    }
    if (m_focused) {
      painter->setPen(QPen(accent, 2.0));
      painter->setBrush(Qt::NoBrush);
      painter->drawRoundedRect(r.adjusted(-3.0, -3.0, 3.0, 3.0), 6.0, 6.0);
    }
  }

private:
  static QPainterPath valueShape(const QRectF& r) {
    QPainterPath path;
    path.addRoundedRect(r, 6.0, 6.0);
    QPainterPath plug;
    const qreal middle = r.center().y();
    plug.moveTo(r.left() + 1.0, middle - 5.0);
    plug.lineTo(r.left() - 5.0, middle);
    plug.lineTo(r.left() + 1.0, middle + 5.0);
    plug.closeSubpath();
    return path.united(plug);
  }

  static QPainterPath actionShape(const QRectF& r) {
    QPainterPath path;
    path.addRoundedRect(r, 3.0, 3.0);
    QPainterPath notch;
    notch.addRect(QRectF(r.left() + 10.0, r.top() - 1.0, 14.0, 4.0));
    QPainterPath tab;
    tab.addRect(QRectF(r.left() + 10.0, r.bottom() - 1.0, 14.0, 4.0));
    return path.subtracted(notch).united(tab);
  }

  Piece m_piece;
  QPalette m_palette;
  QFont m_font;
  bool m_focused = false;
  bool m_highlighted = false;
};

QString roleName(Piece::Role role) {
  switch (role) {
  case Piece::Role::Frame:
    return QStringLiteral("frame");
  case Piece::Role::Label:
    return QStringLiteral("label");
  case Piece::Role::Block:
    return QStringLiteral("block");
  case Piece::Role::Socket:
    return QStringLiteral("socket");
  case Piece::Role::Add:
    return QStringLiteral("add");
  case Piece::Role::Slot:
    return QStringLiteral("slot");
  case Piece::Role::Field:
    return QStringLiteral("field");
  }
  return {};
}

// "Color=Color, Length=Length" as an in_table action's columns.
QJsonObject parseColumns(const QString& text) {
  QJsonObject columns;
  const QStringList parts = text.split(QLatin1Char(','), Qt::SkipEmptyParts);
  for (const QString& part : parts) {
    const int equals = static_cast<int>(part.indexOf(QLatin1Char('=')));
    const QString option = (equals < 0 ? part : part.left(equals)).trimmed();
    const QString column = (equals < 0 ? part : part.mid(equals + 1)).trimmed();
    if (!option.isEmpty()) {
      columns.insert(option, column.isEmpty() ? option : column);
    }
  }
  return columns;
}

} // namespace

QString Piece::name() const {
  switch (role) {
  case Role::Frame:
    return QStringLiteral("rule frame");
  case Role::Label:
    return QStringLiteral("%1 label").arg(field);
  case Role::Field:
    return QStringLiteral("%1 %2").arg(path.id(), field);
  default:
    return QStringLiteral("%1 %2").arg(path.id(), roleName(role));
  }
}

// The palette.

RulePalette::RulePalette(QWidget* parent) : QTreeWidget(parent) {
  setObjectName(QStringLiteral("rulePalette"));
  setHeaderHidden(true);
  setRootIsDecorated(false);
  setIndentation(12);
  setSelectionMode(QAbstractItemView::SingleSelection);
  setDragEnabled(false);
  setExpandsOnDoubleClick(false);
  setItemsExpandable(false);
  QTreeWidgetItem* group = nullptr;
  QString groupName;
  for (const PaletteEntry& entry : paletteEntries()) {
    if (group == nullptr || entry.group != groupName) {
      groupName = entry.group;
      group = new QTreeWidgetItem(this, QStringList{entry.group});
      QFont bold = font();
      bold.setBold(true);
      group->setFont(0, bold);
      group->setFlags(Qt::ItemIsEnabled);
    }
    auto* item = new QTreeWidgetItem(group, QStringList{entry.label});
    item->setData(0, kIdRole, entry.id);
    // What the block does, with an example (mitcad#967); how to drag it
    // is the palette's own tooltip.
    item->setToolTip(0, entry.tip);
  }
  setToolTip(QStringLiteral("Drag a block onto the rule, or choose one and press Enter to put it at the focus"));
  expandAll();
  // Every entry in sight: the palette is as tall as its rows (the window
  // grows to it), so that nothing has to be scrolled to.
  int rows = 0;
  for (QTreeWidgetItemIterator it(this); *it != nullptr; ++it) {
    ++rows;
  }
  setMinimumHeight((fontMetrics().height() + 6) * rows + 2 * frameWidth());
  setMinimumWidth(150);
  connect(this, &QTreeWidget::itemDoubleClicked, this, [this](QTreeWidgetItem* item) {
    const QString id = item != nullptr ? item->data(0, kIdRole).toString() : QString();
    if (!id.isEmpty() && m_canvas != nullptr) {
      m_canvas->insertAtFocus(id);
    }
  });
  connect(verticalScrollBar(), &QScrollBar::valueChanged, this, [this] { logPlaces(); });
}

RuleCanvas* RulePalette::canvas() const { return m_canvas.data(); }

void RulePalette::setCanvas(RuleCanvas* canvas) {
  m_canvas = canvas;
}

QTreeWidgetItem* RulePalette::entry(const QString& id) const {
  for (QTreeWidgetItemIterator it(const_cast<RulePalette*>(this)); *it != nullptr; ++it) {
    if ((*it)->data(0, kIdRole).toString() == id) {
      return *it;
    }
  }
  return nullptr;
}

QString RulePalette::idAt(const QPoint& at) const {
  const QTreeWidgetItem* item = itemAt(at);
  return item != nullptr ? item->data(0, kIdRole).toString() : QString();
}

void RulePalette::mousePressEvent(QMouseEvent* event) {
  QTreeWidget::mousePressEvent(event);
  m_dragging = false;
  m_pressed.clear();
  if (event->button() == Qt::LeftButton) {
    m_pressed = idAt(event->position().toPoint());
    m_pressAt = event->position().toPoint();
  }
}

void RulePalette::mouseMoveEvent(QMouseEvent* event) {
  if ((event->buttons() & Qt::LeftButton) != 0 && !m_pressed.isEmpty() && m_canvas != nullptr) {
    if (!m_dragging &&
        (event->position().toPoint() - m_pressAt).manhattanLength() >= QApplication::startDragDistance()) {
      m_dragging = true;
      viewport()->setCursor(Qt::ClosedHandCursor);
      qDebug().noquote() << QStringLiteral("Rule palette drag %1").arg(m_pressed);
    }
    if (m_dragging) {
      m_canvas->dragOver(event->globalPosition().toPoint(), m_pressed);
      event->accept();
      return;
    }
  }
  QTreeWidget::mouseMoveEvent(event);
}

void RulePalette::mouseReleaseEvent(QMouseEvent* event) {
  if (m_dragging) {
    m_dragging = false;
    viewport()->unsetCursor();
    const QString id = std::exchange(m_pressed, QString());
    if (m_canvas != nullptr && !m_canvas->dropAt(event->globalPosition().toPoint(), id)) {
      qDebug().noquote() << QStringLiteral("Rule palette drop %1: no place for it there").arg(id);
    }
    event->accept();
    return;
  }
  m_pressed.clear();
  QTreeWidget::mouseReleaseEvent(event);
}

void RulePalette::keyPressEvent(QKeyEvent* event) {
  if ((event->key() == Qt::Key_Return || event->key() == Qt::Key_Enter) && event->modifiers() == Qt::NoModifier) {
    const QString id = currentItem() != nullptr ? currentItem()->data(0, kIdRole).toString() : QString();
    if (!id.isEmpty() && m_canvas != nullptr) {
      if (m_canvas->insertAtFocus(id)) {
        // Back to the canvas, unless the new block's editor took the keyboard.
        if (m_canvas->editor() == nullptr) {
          m_canvas->setFocus(Qt::OtherFocusReason);
        }
      } else {
        qDebug().noquote() << QStringLiteral("Rule palette %1: no place for it at the focus").arg(id);
      }
    }
    event->accept();
    return;
  }
  QTreeWidget::keyPressEvent(event);
}

void RulePalette::resizeEvent(QResizeEvent* event) {
  QTreeWidget::resizeEvent(event);
  logPlaces();
}

void RulePalette::showEvent(QShowEvent* event) {
  QTreeWidget::showEvent(event);
  QTimer::singleShot(0, this, [this] { logPlaces(); });
}

void RulePalette::logPlaces() {
  if (!isVisible()) {
    return;
  }
  QWidget* top = window();
  if (top->parentWidget() != nullptr) {
    top = top->parentWidget()->window();
  }
  const QPoint origin = top->mapToGlobal(QPoint(0, 0));
  QStringList lines;
  for (QTreeWidgetItemIterator it(this); *it != nullptr; ++it) {
    const QString id = (*it)->data(0, kIdRole).toString();
    if (id.isEmpty()) {
      continue;
    }
    const QRect rect = visualItemRect(*it);
    if (!rect.isValid() || !viewport()->rect().contains(rect.center())) {
      lines << QStringLiteral("Rule palette %1 out of sight").arg(id);
      continue;
    }
    const QPoint at = viewport()->mapToGlobal(QPoint(rect.left() + 30, rect.center().y())) - origin;
    lines << QStringLiteral("Rule palette %1 at %2,%3").arg(id).arg(at.x()).arg(at.y());
  }
  const QString joined = lines.join(QLatin1Char('\n'));
  if (joined == m_logged) {
    return;
  }
  m_logged = joined;
  for (const QString& line : lines) {
    qDebug().noquote() << line;
  }
}

// The canvas.

RuleCanvas::RuleCanvas(QWidget* parent) : QGraphicsView(parent), m_scene(new QGraphicsScene(this)) {
  setObjectName(QStringLiteral("ruleCanvas"));
  setScene(m_scene);
  setAlignment(Qt::AlignLeft | Qt::AlignTop);
  setRenderHint(QPainter::Antialiasing);
  setFocusPolicy(Qt::StrongFocus);
  setDragMode(QGraphicsView::NoDrag);
  setMinimumSize(320, 180);
  rebuild();
}

void RuleCanvas::setRule(const Rule& rule) {
  if (m_editor != nullptr && rule.name != m_rule.name) {
    m_pending.clear(); // another rule: the new block's edit is given up
    closeEditor();
  }
  m_rule = rule;
  m_hasRule = true;
  rebuild();
}

void RuleCanvas::flushPending() {
  if (!m_pending.isEmpty()) {
    const QString what = std::exchange(m_pending, QString());
    emit edited(m_rule, what);
  }
}

void RuleCanvas::clearRule() {
  m_pending.clear();
  closeEditor();
  m_rule = Rule();
  m_hasRule = false;
  m_focusName.clear();
  rebuild();
}

void RuleCanvas::setOptions(const QStringList& options, const QStringList& numberOptions) {
  m_options = options;
  m_numberOptions = numberOptions;
}

void RuleCanvas::setOptionInfos(std::vector<OptionInfo> infos) {
  m_infos = std::move(infos);
}

void RuleCanvas::setHint(const QString& hint) {
  if (hint == m_hint) {
    return;
  }
  m_hint = hint;
  if (!m_hasRule) {
    rebuild();
  }
}

void RuleCanvas::setParameters(const QStringList& parameters) {
  m_parameters = parameters;
}

void RuleCanvas::setTables(std::vector<SuggestedTable> tables) {
  m_tables = std::move(tables);
}

void RuleCanvas::setFormulaEditor(std::function<std::optional<QString>(const QString&, QWidget*)> edit) {
  m_formulaEditor = std::move(edit);
}

void RuleCanvas::setDeleteArea(QWidget* area) {
  m_deleteArea = area;
}

void RuleCanvas::rebuild() {
  m_scene->clear();
  m_items.clear();
  m_pieces.clear();
  m_highlight = -1;
  if (!m_hasRule) {
    QGraphicsSimpleTextItem* hint = m_scene->addSimpleText(m_hint);
    hint->setBrush(palette().color(QPalette::PlaceholderText));
    hint->setPos(12.0, 12.0);
    m_focus = -1;
    m_scene->setSceneRect(QRectF(0.0, 0.0, std::max(300.0, hint->boundingRect().width() + 24.0), 60.0));
    logPlaces();
    return;
  }
  const QFontMetricsF metrics(font());
  Builder(m_rule, metrics, m_pieces).build();
  for (const Piece& piece : m_pieces) {
    auto* item = new PieceItem(piece, palette(), font());
    item->setZValue(piece.depth);
    m_scene->addItem(item);
    m_items.push_back(item);
  }
  m_scene->setSceneRect(m_scene->itemsBoundingRect().adjusted(-8.0, -8.0, 8.0, 8.0).united(QRectF(0, 0, 1, 1)));
  m_focus = m_focusName.isEmpty() ? -1 : pieceIndex(m_focusName);
  updateItems();
  logPlaces();
}

void RuleCanvas::updateItems() {
  for (std::size_t i = 0; i < m_items.size(); ++i) {
    static_cast<PieceItem*>(m_items[i])->setState(static_cast<int>(i) == m_focus, static_cast<int>(i) == m_highlight);
  }
}

int RuleCanvas::pieceIndex(const QString& name) const {
  for (std::size_t i = 0; i < m_pieces.size(); ++i) {
    if (m_pieces[i].name() == name) {
      return static_cast<int>(i);
    }
  }
  return -1;
}

int RuleCanvas::findPath(const BlockPath& path) const {
  BlockPath wanted = path;
  for (;;) {
    for (const Piece::Role role :
         {Piece::Role::Block, Piece::Role::Socket, Piece::Role::Add, Piece::Role::Slot}) {
      for (std::size_t i = 0; i < m_pieces.size(); ++i) {
        if (m_pieces[i].role == role && m_pieces[i].path == wanted) {
          return static_cast<int>(i);
        }
      }
    }
    if (wanted.indices.empty()) {
      return -1;
    }
    wanted.indices.pop_back();
  }
}

int RuleCanvas::pieceAt(const QPointF& scene) const {
  int best = -1;
  for (std::size_t i = 0; i < m_pieces.size(); ++i) {
    const Piece& piece = m_pieces[i];
    if (!piece.rect.contains(scene)) {
      continue;
    }
    if (piece.role == Piece::Role::Frame) {
      bool inHole = false;
      for (const QRectF& hole : piece.holes) {
        inHole = inHole || hole.contains(scene);
      }
      if (inHole) {
        continue;
      }
    }
    if (best < 0 || piece.depth >= m_pieces[static_cast<std::size_t>(best)].depth) {
      best = static_cast<int>(i);
    }
  }
  return best;
}

bool RuleCanvas::isBlockAtScene(const QPointF& scene) const {
  const int index = pieceAt(scene);
  return index >= 0 && m_pieces[static_cast<std::size_t>(index)].role == Piece::Role::Block;
}

int RuleCanvas::targetAt(const QPointF& scene, BlockKind kind, const std::optional<BlockPath>& from) const {
  const auto candidate = [&](const Piece& piece) {
    const bool placeable = piece.role == Piece::Role::Socket || piece.role == Piece::Role::Add ||
                       piece.role == Piece::Role::Slot ||
                       (piece.role == Piece::Role::Block && isValue(piece.kind) && isValue(kind));
    if (!placeable || !fits(m_rule, piece.target(), kind)) {
      return false;
    }
    if (from && isPrefix(*from, piece.path)) {
      return false; // the block dragged, or a place in it
    }
    return true;
  };
  int best = -1;
  for (std::size_t i = 0; i < m_pieces.size(); ++i) {
    const Piece& piece = m_pieces[i];
    if (piece.rect.adjusted(-2.0, -3.0, 2.0, 3.0).contains(scene) && candidate(piece) &&
        (best < 0 || piece.depth >= m_pieces[static_cast<std::size_t>(best)].depth)) {
      best = static_cast<int>(i);
    }
  }
  if (best >= 0) {
    return best;
  }
  // Else the nearest place it fits, within a little distance.
  qreal nearest = 30.0;
  for (std::size_t i = 0; i < m_pieces.size(); ++i) {
    const Piece& piece = m_pieces[i];
    if (!candidate(piece)) {
      continue;
    }
    const QRectF& r = piece.rect;
    const qreal dx = std::max({r.left() - scene.x(), 0.0, scene.x() - r.right()});
    const qreal dy = std::max({r.top() - scene.y(), 0.0, scene.y() - r.bottom()});
    const qreal distance = std::hypot(dx, dy);
    if (distance < nearest) {
      nearest = distance;
      best = static_cast<int>(i);
    }
  }
  return best;
}

void RuleCanvas::setHighlight(int index) {
  if (index == m_highlight) {
    return;
  }
  m_highlight = index;
  updateItems();
  if (index >= 0) {
    qDebug().noquote() << QStringLiteral("Rule canvas target %1").arg(m_pieces[static_cast<std::size_t>(index)].name());
  }
}

void RuleCanvas::setFocusIndex(int index) {
  m_focus = index;
  m_focusName = index >= 0 ? m_pieces[static_cast<std::size_t>(index)].name() : QString();
  updateItems();
  if (index >= 0) {
    ensureVisible(m_pieces[static_cast<std::size_t>(index)].rect, 16, 16);
  }
  if (m_focusName != m_loggedFocus) {
    m_loggedFocus = m_focusName;
    if (!m_focusName.isEmpty()) {
      qDebug().noquote() << QStringLiteral("Rule canvas focus %1").arg(m_focusName);
    }
  }
}

QString RuleCanvas::focusName() const {
  return m_focusName;
}

bool RuleCanvas::focusPiece(const QString& name) {
  const int index = pieceIndex(name);
  if (index < 0) {
    return false;
  }
  setFocusIndex(index);
  return true;
}

std::optional<QPoint> RuleCanvas::pieceCenter(const QString& name) const {
  const int index = pieceIndex(name);
  if (index < 0) {
    return std::nullopt;
  }
  return mapFromScene(m_pieces[static_cast<std::size_t>(index)].rect.center());
}

void RuleCanvas::moveFocus(int key) {
  std::vector<int> order;
  for (std::size_t i = 0; i < m_pieces.size(); ++i) {
    if (m_pieces[i].role != Piece::Role::Frame) {
      order.push_back(static_cast<int>(i));
    }
  }
  if (order.empty()) {
    return;
  }
  const auto at = std::find(order.begin(), order.end(), m_focus);
  if (at == order.end()) {
    setFocusIndex(order.front());
    return;
  }
  if (key == Qt::Key_Left || key == Qt::Key_Right) {
    if (key == Qt::Key_Left && at != order.begin()) {
      setFocusIndex(*(at - 1));
    } else if (key == Qt::Key_Right && at + 1 != order.end()) {
      setFocusIndex(*(at + 1));
    }
    return;
  }
  // Up and Down: the nearest place in the row above or below.
  const QPointF from = m_pieces[static_cast<std::size_t>(m_focus)].rect.center();
  int best = -1;
  qreal bestDy = 0.0;
  qreal bestDx = 0.0;
  for (const int index : order) {
    const QPointF to = m_pieces[static_cast<std::size_t>(index)].rect.center();
    const qreal dy = key == Qt::Key_Down ? to.y() - from.y() : from.y() - to.y();
    if (dy <= 4.0) {
      continue;
    }
    const qreal dx = std::abs(to.x() - from.x());
    if (best < 0 || dy < bestDy - 4.0 || (std::abs(dy - bestDy) <= 4.0 && dx < bestDx)) {
      best = index;
      bestDy = dy;
      bestDx = dx;
    }
  }
  if (best >= 0) {
    setFocusIndex(best);
  }
}

QString RuleCanvas::optionFor(const QString& paletteId) const {
  if (paletteId == QLatin1String("min") || paletteId == QLatin1String("max") || paletteId == QLatin1String("step")) {
    if (!m_numberOptions.isEmpty()) {
      return m_numberOptions.front();
    }
  }
  if (paletteId == QLatin1String("parameter")) {
    for (const QString& parameter : m_parameters) {
      if (!m_options.contains(parameter)) {
        return parameter;
      }
    }
    return m_parameters.isEmpty() ? QString() : m_parameters.front();
  }
  if (paletteId == QLatin1String("option") || paletteId == QLatin1String("allow_only") ||
      paletteId == QLatin1String("exclude") || paletteId == QLatin1String("min") ||
      paletteId == QLatin1String("max") || paletteId == QLatin1String("step")) {
    return m_options.isEmpty() ? QString() : m_options.front();
  }
  return {};
}

BlockKind RuleCanvas::kindOf(const QString& paletteId) const {
  return newBlock(paletteId).kind;
}

bool RuleCanvas::put(const QString& paletteId, const Target& target) {
  Block block = newBlock(paletteId, optionFor(paletteId));
  if (block.kind == BlockKind::Empty) {
    return false;
  }
  // An action comes complete: its values from its option's domain
  // (mitcad#966), never an empty socket to fill first.
  if (isAction(block.kind)) {
    fillSockets(block, m_infos);
  }
  Rule changed = m_rule;
  if (!place(changed, target, block)) {
    return false;
  }
  apply(std::move(changed), QStringLiteral("Put %1 in %2").arg(kindLabel(block.kind), target.path.id()), target.path,
        true);
  return true;
}

bool RuleCanvas::insertAtFocus(const QString& paletteId) {
  if (!m_hasRule) {
    return false;
  }
  if (isRuleEntry(paletteId)) {
    Rule changed = m_rule;
    if (!applyRuleEntry(changed, paletteId)) {
      return false;
    }
    const BlockPath focus = paletteId == QLatin1String("else") ? BlockPath{2, {0}} : BlockPath{0, {}};
    apply(std::move(changed), QStringLiteral("Rule %1").arg(paletteId), focus);
    return true;
  }
  const BlockKind kind = kindOf(paletteId);
  if (kind == BlockKind::Empty) {
    return false;
  }
  std::optional<Target> target;
  if (m_focus >= 0) {
    const Piece& piece = m_pieces[static_cast<std::size_t>(m_focus)];
    const bool placeable = piece.role == Piece::Role::Socket || piece.role == Piece::Role::Add ||
                       piece.role == Piece::Role::Slot || (piece.role == Piece::Role::Block && isValue(piece.kind));
    if (placeable && fits(m_rule, piece.target(), kind)) {
      target = piece.target();
    } else if (isAction(kind) && piece.path.section != 0 && !piece.path.indices.empty()) {
      target = Target{BlockPath{piece.path.section, {piece.path.indices.front() + 1}}, true};
    }
  }
  if (!target && isAction(kind)) {
    target = Target{BlockPath{1, {static_cast<int>(m_rule.then.size())}}, true};
  }
  if (!target) {
    for (const Piece& piece : m_pieces) {
      if (piece.role == Piece::Role::Socket && fits(m_rule, piece.target(), kind)) {
        target = piece.target();
        break;
      }
    }
  }
  return target && put(paletteId, *target);
}

void RuleCanvas::dragOver(const QPoint& global, const QString& paletteId) {
  const QPoint local = viewport()->mapFromGlobal(global);
  if (!m_hasRule || !viewport()->rect().contains(local)) {
    setHighlight(-1);
    return;
  }
  dragOverScene(mapToScene(local), paletteId);
}

void RuleCanvas::dragOverScene(const QPointF& scene, const QString& paletteId) {
  if (!m_hasRule) { setHighlight(-1); return; }
  if (isRuleEntry(paletteId)) {
    setHighlight(m_pieces.empty() ? -1 : 0);
    return;
  }
  setHighlight(targetAt(scene, kindOf(paletteId), std::nullopt));
}

bool RuleCanvas::dropAt(const QPoint& global, const QString& paletteId) {
  dragOver(global, paletteId);
  return takeHighlightedDrop(paletteId);
}

bool RuleCanvas::dropAtScene(const QPointF& scene, const QString& paletteId) {
  dragOverScene(scene, paletteId);
  return takeHighlightedDrop(paletteId);
}

bool RuleCanvas::takeHighlightedDrop(const QString& paletteId) {
  const int target = m_highlight;
  endDrag();
  if (target < 0) {
    return false;
  }
  if (isRuleEntry(paletteId)) {
    Rule changed = m_rule;
    if (!applyRuleEntry(changed, paletteId)) {
      return false;
    }
    const BlockPath focus = paletteId == QLatin1String("else") ? BlockPath{2, {0}} : BlockPath{0, {}};
    apply(std::move(changed), QStringLiteral("Rule %1").arg(paletteId), focus);
    return true;
  }
  return put(paletteId, m_pieces[static_cast<std::size_t>(target)].target());
}

void RuleCanvas::endDrag() {
  setHighlight(-1);
  m_dragFrom.reset();
  viewport()->unsetCursor();
}

void RuleCanvas::apply(Rule changed, const QString& what, const std::optional<BlockPath>& focus, bool editNew) {
  m_rule = std::move(changed);
  m_pending.clear(); // an edit that ends a new block's own edit includes it
  rebuild();
  qDebug().noquote() << QStringLiteral("Rule edited %1: %2").arg(m_rule.name, what);
  // A new block that needs its value opens its editor at once (a name, a
  // number, a text, a column, a formula; a forbid's message, a table): the
  // block and its value are one edit, saved when the editor closes (with
  // the block as it came when the edit is cancelled or changes nothing).
  // Saved at once, an empty text would be a step of its own (`Color ==
  // ''`).
  QString editField;
  bool editBlock = false;
  if (editNew && focus) {
    const Block* block = blockAt(m_rule, *focus);
    if (block != nullptr && isLeaf(block->kind) &&
        (block->kind == BlockKind::Text || block->kind == BlockKind::Formula || block->text.isEmpty() ||
         block->kind == BlockKind::Name)) {
      editBlock = true;
    } else if (block != nullptr && (block->kind == BlockKind::Forbid || block->kind == BlockKind::InTable)) {
      editField = block->kind == BlockKind::Forbid ? QStringLiteral("message") : QStringLiteral("table");
    }
  }
  if (editBlock || !editField.isEmpty()) {
    m_pending = what;
  } else {
    // The page saves the rule, and may show the model's afterwards.
    emit edited(m_rule, what);
  }
  if (!focus) {
    return;
  }
  const int index = findPath(*focus);
  if (index < 0) {
    flushPending();
    return;
  }
  setFocusIndex(index);
  if (!editNew) {
    return;
  }
  const Piece& piece = m_pieces[static_cast<std::size_t>(index)];
  if (!m_pending.isEmpty()) {
    const int edit = editBlock ? index : pieceIndex(QStringLiteral("%1 %2").arg(piece.path.id(), editField));
    if (edit >= 0) {
      setFocusIndex(edit);
      editPiece(edit);
    }
    // No editor open (none found, or the Formula Editor already closed):
    // the new block is saved as it is.
    if (m_editor == nullptr) {
      flushPending();
    }
    return;
  }
  // Else the first empty socket in it.
  for (std::size_t i = 0; i < m_pieces.size(); ++i) {
    if (m_pieces[i].role == Piece::Role::Socket && isPrefix(piece.path, m_pieces[i].path)) {
      setFocusIndex(static_cast<int>(i));
      return;
    }
  }
  // Else, for an action that came with its values (mitcad#966), its first
  // value: Enter edits it at once.
  if (piece.role == Piece::Role::Block && isAction(piece.kind)) {
    const BlockPath at = piece.path;
    for (std::size_t i = 0; i < m_pieces.size(); ++i) {
      const Piece& inner = m_pieces[i];
      if (inner.role == Piece::Role::Block && isValue(inner.kind) &&
          inner.path.indices.size() > at.indices.size() && isPrefix(at, inner.path)) {
        setFocusIndex(static_cast<int>(i));
        return;
      }
    }
  }
}

void RuleCanvas::editPiece(int index) {
  if (index < 0 || index >= static_cast<int>(m_pieces.size())) {
    return;
  }
  const Piece& piece = m_pieces[static_cast<std::size_t>(index)];
  switch (piece.role) {
  case Piece::Role::Field:
    if (piece.field == QLatin1String("op")) {
      chooseOperator(index);
    } else if (piece.field == QLatin1String("columns")) {
      chooseColumns(index);
    } else {
      openEditor(index);
    }
    return;
  case Piece::Role::Block:
    if (piece.kind == BlockKind::Formula) {
      editFormula(piece.path);
    } else if (isLeaf(piece.kind)) {
      openEditor(index);
    }
    return;
  case Piece::Role::Socket:
  case Piece::Role::Add:
  case Piece::Role::Slot:
    setFocusIndex(index);
    emit paletteRequested();
    return;
  case Piece::Role::Label:
  case Piece::Role::Frame:
    return;
  }
}

bool RuleCanvas::editFocused() {
  if (m_focus < 0) {
    return false;
  }
  editPiece(m_focus);
  return true;
}

void RuleCanvas::openEditor(int index) {
  closeEditor();
  const Piece piece = m_pieces[static_cast<std::size_t>(index)];
  const Block* block = blockAt(m_rule, piece.path);
  if (block == nullptr) {
    return;
  }
  QString text;
  bool names = false;
  bool tables = false;
  if (piece.role == Piece::Role::Field) {
    if (piece.field == QLatin1String("option")) {
      text = block->text;
      names = true;
    } else if (piece.field == QLatin1String("message")) {
      text = block->message;
    } else if (piece.field == QLatin1String("table")) {
      text = block->text;
      tables = true;
    } else if (piece.field == QLatin1String("columns")) {
      text = piece.text;
    }
  } else if (block->kind == BlockKind::Text || block->kind == BlockKind::Formula) {
    text = block->text;
  } else {
    text = printExpression(*block).trimmed();
    names = block->kind == BlockKind::Name;
  }
  m_editor = new QLineEdit(viewport());
  m_editor->setObjectName(QStringLiteral("ruleBlockEditor"));
  QRect rect = mapFromScene(piece.rect).boundingRect();
  rect.setWidth(std::max(rect.width() + 40, 160));
  m_editor->setGeometry(rect);
  if (names) {
    QStringList words = m_options;
    for (const QString& parameter : m_parameters) {
      if (!words.contains(parameter)) {
        words << parameter;
      }
    }
    auto* completer = new QCompleter(words, m_editor);
    completer->setCaseSensitivity(Qt::CaseInsensitive);
    completer->setCompletionMode(QCompleter::InlineCompletion);
    m_editor->setCompleter(completer);
  } else if (tables && !m_tables.empty()) {
    // The design's tables (mitcad#849).
    QStringList words;
    for (const SuggestedTable& table : m_tables) {
      words << table.name;
    }
    auto* completer = new QCompleter(words, m_editor);
    completer->setCaseSensitivity(Qt::CaseInsensitive);
    completer->setCompletionMode(QCompleter::InlineCompletion);
    m_editor->setCompleter(completer);
  }
  m_editor->setText(text);
  m_editor->selectAll();
  m_editor->installEventFilter(this);
  connect(m_editor, &QLineEdit::returnPressed, this, [this] { commitEditor(); });
  connect(m_editor, &QLineEdit::editingFinished, this, [this] { commitEditor(); });
  m_editing = piece.name();
  // In a native canvas the line edit is only a value presenter. Giving
  // it Qt focus would take the native field's responder and immediately
  // finish the shared edit when AppKit restores that responder.
  if (property("nativeRuleArtwork").toBool()) {
    m_editor->setAttribute(Qt::WA_DontShowOnScreen);
  } else {
    m_editor->show();
    m_editor->setFocus(Qt::OtherFocusReason);
  }
  qDebug().noquote() << QStringLiteral("Rule canvas editing %1: %2").arg(m_editing, text);
}

void RuleCanvas::cancelEditor() {
  if (m_editor == nullptr) return;
  qDebug().noquote() << QStringLiteral("Rule canvas editing %1 cancelled").arg(m_editing);
  closeEditor();
  flushPending(); // a new block keeps the value it came with
}

void RuleCanvas::closeEditor() {
  if (m_editor == nullptr || m_editorClosing) {
    return;
  }
  m_editorClosing = true;
  QLineEdit* editor = std::exchange(m_editor, nullptr);
  editor->disconnect(this);
  editor->removeEventFilter(this);
  const bool hadFocus = editor->hasFocus();
  editor->hide();
  editor->deleteLater();
  m_editing.clear();
  m_editorClosing = false;
  if (hadFocus) {
    setFocus(Qt::OtherFocusReason);
  }
}

void RuleCanvas::commitEditor() {
  if (m_editor == nullptr || m_editorClosing) {
    return;
  }
  const QString text = m_editor->text();
  const QString name = m_editing;
  closeEditor();
  // An edit that changes nothing still saves the new block it was opened
  // for (apply() takes it into an edit that changes something).
  struct Pending {
    RuleCanvas* canvas;
    ~Pending() { canvas->flushPending(); }
  } pending{this};
  const int index = pieceIndex(name);
  if (index < 0) {
    return;
  }
  const Piece piece = m_pieces[static_cast<std::size_t>(index)];
  Rule changed = m_rule;
  Block* block = blockAt(changed, piece.path);
  if (block == nullptr) {
    return;
  }
  QString what;
  if (piece.role == Piece::Role::Field) {
    const QString trimmed = text.trimmed();
    if (piece.field == QLatin1String("option") || piece.field == QLatin1String("table")) {
      if (block->text == trimmed) {
        return;
      }
      block->text = trimmed;
      // A table of the design: its options' columns follow (mitcad#849).
      if (piece.field == QLatin1String("table")) {
        mapColumns(*block);
      }
    } else if (piece.field == QLatin1String("message")) {
      if (block->message == text) {
        return;
      }
      block->message = text;
    } else if (piece.field == QLatin1String("columns")) {
      const QJsonObject columns = parseColumns(text);
      if (block->data == columns) {
        return;
      }
      block->data = columns;
    }
    what = QStringLiteral("Set %1 of %2 to %3").arg(piece.field, piece.path.id(), text);
  } else if (block->kind == BlockKind::Text) {
    if (block->text == text) {
      return;
    }
    block->text = text;
    touch(changed, piece.path);
    what = QStringLiteral("Set %1 to '%2'").arg(piece.path.id(), text);
  } else {
    const QString trimmed = text.trimmed();
    if (block->kind != BlockKind::Formula && trimmed == printExpression(*block).trimmed()) {
      return;
    }
    if (block->kind == BlockKind::Formula && trimmed == block->text) {
      return;
    }
    Block replacement = parseExpression(trimmed);
    if (replacement.kind == BlockKind::Empty) {
      take(changed, piece.path);
    } else if (piece.path.section == 0 && piece.path.indices.empty()) {
      changed.when = std::move(replacement);
    } else {
      place(changed, Target{piece.path, false}, std::move(replacement));
    }
    what = QStringLiteral("Set %1 to %2").arg(piece.path.id(), trimmed);
  }
  apply(std::move(changed), what, piece.path);
}

void RuleCanvas::chooseOperator(int index) {
  const Piece piece = m_pieces[static_cast<std::size_t>(index)];
  QMenu menu(this);
  QStringList entries;
  for (const QString& op : compareOperators()) {
    QAction* action = menu.addAction(operatorLabel(op));
    action->setData(op);
    entries << operatorLabel(op);
  }
  qDebug().noquote() << QStringLiteral("Context menu: %1").arg(entries.join(QStringLiteral(" | ")));
  const QVariant nativePoint = property("nativeRuleMenuPoint");
  const QPoint point = nativePoint.isValid() ? nativePoint.toPoint()
                                            : viewport()->mapToGlobal(mapFromScene(piece.rect.bottomLeft()));
  const QAction* chosen = execMenu(&menu, point);
  if (chosen == nullptr) {
    return;
  }
  Rule changed = m_rule;
  Block* block = blockAt(changed, piece.path);
  const QString op = chosen->data().toString();
  if (block == nullptr || operatorLabel(block->text) == operatorLabel(op)) {
    return;
  }
  block->text = op;
  touch(changed, piece.path);
  apply(std::move(changed), QStringLiteral("Set operator of %1 to %2").arg(piece.path.id(), op), piece.path);
}

bool RuleCanvas::mapColumns(Block& block) const {
  const SuggestedTable* table = tableNamed(m_tables, block.text);
  if (table == nullptr) {
    return false;
  }
  QStringList names;
  for (const SuggestedTable::Column& column : table->columns) {
    names << column.name;
  }
  block.data = inTableColumns(names, m_options, block.data);
  QStringList shown;
  for (auto it = block.data.begin(); it != block.data.end(); ++it) {
    shown << QStringLiteral("%1=%2").arg(it.key(), it.value().toString());
  }
  qDebug().noquote() << QStringLiteral("Rule canvas columns of %1: %2")
                            .arg(table->name, shown.isEmpty() ? QStringLiteral("none") : shown.join(QStringLiteral(", ")));
  return true;
}

void RuleCanvas::chooseColumns(int index) {
  const Piece piece = m_pieces[static_cast<std::size_t>(index)];
  const Block* block = blockAt(m_rule, piece.path);
  const SuggestedTable* table = block != nullptr ? tableNamed(m_tables, block->text) : nullptr;
  if (table == nullptr) {
    // A table the design does not have (yet): the columns as text.
    openEditor(index);
    return;
  }
  // A column of the table for each option, or none.
  QDialog dialog(this);
  dialog.setObjectName(QStringLiteral("ruleTableColumnsDialog"));
  dialog.setWindowTitle(QStringLiteral("Table Columns"));
  auto* form = new QFormLayout(&dialog);
  auto* about = new QLabel(QStringLiteral("The column of %1 each option is to match:").arg(table->name));
  about->setWordWrap(true);
  form->addRow(about);
  std::vector<std::pair<QString, QComboBox*>> choices;
  for (const QString& option : std::as_const(m_options)) {
    auto* combo = new QComboBox;
    combo->setObjectName(QStringLiteral("ruleTableColumn%1").arg(option));
    combo->addItem(QStringLiteral("(none)"), QString());
    for (const SuggestedTable::Column& column : table->columns) {
      combo->addItem(columnLabel(column), column.name);
    }
    combo->setCurrentIndex(std::max(0, combo->findData(block->data.value(option).toString())));
    form->addRow(QStringLiteral("%1:").arg(option), combo);
    choices.emplace_back(option, combo);
  }
  auto* buttons = new QDialogButtonBox(QDialogButtonBox::Ok | QDialogButtonBox::Cancel);
  buttons->button(QDialogButtonBox::Ok)->setDefault(true);
  buttons->button(QDialogButtonBox::Ok)->setObjectName(QStringLiteral("ruleTableColumnsOK"));
  buttons->button(QDialogButtonBox::Cancel)->setObjectName(QStringLiteral("ruleTableColumnsCancel"));
  form->addRow(buttons);
  connect(buttons, &QDialogButtonBox::accepted, &dialog, &QDialog::accept);
  connect(buttons, &QDialogButtonBox::rejected, &dialog, &QDialog::reject);
  qDebug().noquote() << QStringLiteral("Rule canvas columns dialog for %1 of %2").arg(piece.path.id(), table->name);
  prepareModal(&dialog);
  if (execDialog(&dialog) != QDialog::Accepted) {
    return;
  }
  QJsonObject columns;
  QStringList shown;
  for (const auto& choice : choices) {
    const QString column = choice.second->currentData().toString();
    if (!column.isEmpty()) {
      columns.insert(choice.first, column);
    }
  }
  for (auto it = columns.begin(); it != columns.end(); ++it) {
    shown << QStringLiteral("%1=%2").arg(it.key(), it.value().toString());
  }
  Rule changed = m_rule;
  Block* target = blockAt(changed, piece.path);
  if (target == nullptr || target->data == columns) {
    return;
  }
  target->data = columns;
  const QString what = QStringLiteral("Set columns of %1 to %2").arg(piece.path.id(), shown.join(QStringLiteral(", ")));
  apply(std::move(changed), what, piece.path);
}

void RuleCanvas::editFormula(const BlockPath& path) {
  const Block* block = blockAt(m_rule, path);
  if (block == nullptr) {
    return;
  }
  if (!m_formulaEditor) {
    const int index = findPath(path);
    if (index >= 0) {
      openEditor(index);
    }
    return;
  }
  const QString before = block->text;
  qDebug().noquote() << QStringLiteral("Rule canvas formula %1: %2").arg(path.id(), before);
  const std::optional<QString> after = m_formulaEditor(before, this);
  if (!after || after->trimmed() == before.trimmed()) {
    return;
  }
  Rule changed = m_rule;
  Block replacement = parseExpression(after->trimmed());
  if (replacement.kind == BlockKind::Empty) {
    take(changed, path);
  } else if (path.section == 0 && path.indices.empty()) {
    changed.when = std::move(replacement);
  } else {
    place(changed, Target{path, false}, std::move(replacement));
  }
  apply(std::move(changed), QStringLiteral("Set %1 to %2").arg(path.id(), after->trimmed()), path);
}

bool RuleCanvas::deletePiece(int index) {
  if (index < 0 || index >= static_cast<int>(m_pieces.size())) {
    return false;
  }
  const Piece piece = m_pieces[static_cast<std::size_t>(index)];
  Rule changed = m_rule;
  if (piece.role == Piece::Role::Block) {
    take(changed, piece.path);
    apply(std::move(changed), QStringLiteral("Delete %1").arg(piece.path.id()), piece.path);
    return true;
  }
  if (piece.role == Piece::Role::Label && piece.field == QLatin1String("when")) {
    applyRuleEntry(changed, QStringLiteral("always"));
    apply(std::move(changed), QStringLiteral("Delete the when"), BlockPath{1, {0}});
    return true;
  }
  if (piece.role == Piece::Role::Label && piece.field == QLatin1String("else")) {
    changed.hasElse = false;
    changed.otherwise.clear();
    apply(std::move(changed), QStringLiteral("Delete the else"), BlockPath{1, {0}});
    return true;
  }
  return false;
}

bool RuleCanvas::deleteFocused() {
  return deletePiece(m_focus);
}

void RuleCanvas::showMenu(int index, const QPoint& global) {
  if (!m_hasRule) {
    return;
  }
  QMenu menu(this);
  QStringList entries;
  const auto add = [&menu, &entries](const QString& text, std::function<void()> call) {
    QAction* action = menu.addAction(text);
    QObject::connect(action, &QAction::triggered, &menu, std::move(call));
    entries << text;
  };
  const Piece piece = index >= 0 ? m_pieces[static_cast<std::size_t>(index)] : [] {
    Piece frame;
    frame.role = Piece::Role::Frame;
    return frame;
  }();
  const QString name = piece.name();
  // The menu's actions run after it closed, on the piece by its name.
  const auto later = [this, name](std::function<void(int)> call) {
    return [this, name, call = std::move(call)] {
      QTimer::singleShot(0, this, [this, name, call] {
        const int at = pieceIndex(name);
        if (at >= 0) {
          call(at);
        }
      });
    };
  };
  switch (piece.role) {
  case Piece::Role::Block:
    if (isLeaf(piece.kind)) {
      add(QStringLiteral("Edit"), later([this](int at) { editPiece(at); }));
    }
    if (isValue(piece.kind) && piece.kind != BlockKind::Formula) {
      add(QStringLiteral("Edit as Formula…"), later([this](int at) {
            const BlockPath path = m_pieces[static_cast<std::size_t>(at)].path;
            Rule changed = m_rule;
            Block* block = blockAt(changed, path);
            if (block == nullptr) {
              return;
            }
            Block formula;
            formula.kind = BlockKind::Formula;
            formula.text = printExpression(*block).trimmed();
            *block = formula;
            touch(changed, path);
            apply(std::move(changed), QStringLiteral("Make %1 a formula").arg(path.id()), path);
            editFormula(path);
          }));
    }
    if (isAction(piece.kind) && piece.path.indices.size() == 1) {
      const int at = piece.path.indices.front();
      const std::vector<Block>& list = piece.path.section == 1 ? m_rule.then : m_rule.otherwise;
      const auto moveTo = [this, piece](int to) {
        Rule changed = m_rule;
        if (rules::move(changed, piece.path, Target{BlockPath{piece.path.section, {to}}, true})) {
          const int landed = to > piece.path.indices.front() ? to - 1 : to;
          apply(std::move(changed), QStringLiteral("Move %1").arg(piece.path.id()),
                BlockPath{piece.path.section, {landed}});
        }
      };
      if (at > 0) {
        add(QStringLiteral("Move Up"), [moveTo, at] { moveTo(at - 1); });
      }
      if (at + 1 < static_cast<int>(list.size())) {
        add(QStringLiteral("Move Down"), [moveTo, at] { moveTo(at + 2); });
      }
    }
    add(QStringLiteral("Delete"), later([this](int at) { deletePiece(at); }));
    break;
  case Piece::Role::Field:
    add(QStringLiteral("Edit"), later([this](int at) { editPiece(at); }));
    break;
  case Piece::Role::Socket:
  case Piece::Role::Add:
  case Piece::Role::Slot:
    add(QStringLiteral("Choose from Palette"), later([this](int at) { editPiece(at); }));
    break;
  case Piece::Role::Label:
  case Piece::Role::Frame:
    break;
  }
  const auto ruleEntry = [this](const QString& id) {
    return [this, id] {
      Rule changed = m_rule;
      if (applyRuleEntry(changed, id)) {
        apply(std::move(changed), QStringLiteral("Rule %1").arg(id), BlockPath{0, {}});
      }
    };
  };
  if (piece.role == Piece::Role::Label || piece.role == Piece::Role::Frame) {
    if (m_rule.hasWhen) {
      add(QStringLiteral("Apply Always"), ruleEntry(QStringLiteral("always")));
    } else {
      add(QStringLiteral("Add When"), ruleEntry(QStringLiteral("when")));
    }
    if (m_rule.hasElse) {
      add(QStringLiteral("Remove Else"), [this] {
        Rule changed = m_rule;
        changed.hasElse = false;
        changed.otherwise.clear();
        apply(std::move(changed), QStringLiteral("Delete the else"), BlockPath{1, {0}});
      });
    } else {
      add(QStringLiteral("Add Else"), ruleEntry(QStringLiteral("else")));
    }
  }
  if (entries.isEmpty()) {
    return;
  }
  qDebug().noquote() << QStringLiteral("Context menu: %1").arg(entries.join(QStringLiteral(" | ")));
  execMenu(&menu, global);
}

QWidget* RuleCanvas::logWindow() const {
  QWidget* top = window();
  if (top->parentWidget() != nullptr) {
    top = top->parentWidget()->window();
  }
  return top;
}

void RuleCanvas::logPlaces() {
  if (!isVisible()) {
    return;
  }
  const QPoint origin = logWindow()->mapToGlobal(QPoint(0, 0));
  const QRect shown = viewport()->rect();
  QStringList lines;
  for (const Piece& piece : m_pieces) {
    const QPoint center = mapFromScene(piece.rect.center());
    if (!shown.contains(center)) {
      lines << QStringLiteral("Rule canvas %1 out of sight").arg(piece.name());
      continue;
    }
    const QPoint at = viewport()->mapToGlobal(center) - origin;
    lines << QStringLiteral("Rule canvas %1 at %2,%3").arg(piece.name()).arg(at.x()).arg(at.y());
  }
  const QString joined = lines.join(QLatin1Char('\n'));
  if (joined == m_logged) {
    return;
  }
  m_logged = joined;
  for (const QString& line : lines) {
    qDebug().noquote() << line;
  }
}

void RuleCanvas::mousePressEvent(QMouseEvent* event) {
  if (!property("nativeRuleArtwork").toBool()) setFocus(Qt::MouseFocusReason);
  const QPointF scene = mapToScene(event->position().toPoint());
  const int index = pieceAt(scene);
  m_pressed = -1;
  m_dragFrom.reset();
  if (index >= 0 && m_pieces[static_cast<std::size_t>(index)].role != Piece::Role::Frame) {
    setFocusIndex(index);
  }
  if (event->button() == Qt::LeftButton) {
    m_pressed = index;
    m_pressAt = event->position().toPoint();
  }
  event->accept();
}

void RuleCanvas::mouseMoveEvent(QMouseEvent* event) {
  if ((event->buttons() & Qt::LeftButton) == 0 || m_pressed < 0) {
    QGraphicsView::mouseMoveEvent(event);
    return;
  }
  const Piece& pressed = m_pieces[static_cast<std::size_t>(m_pressed)];
  if (!m_dragFrom && pressed.role == Piece::Role::Block &&
      (event->position().toPoint() - m_pressAt).manhattanLength() >= QApplication::startDragDistance()) {
    m_dragFrom = pressed.path;
    viewport()->setCursor(Qt::ClosedHandCursor);
    qDebug().noquote() << QStringLiteral("Rule canvas drag %1").arg(pressed.name());
  }
  if (m_dragFrom) {
    const Block* block = blockAt(m_rule, *m_dragFrom);
    setHighlight(block != nullptr ? targetAt(mapToScene(event->position().toPoint()), block->kind, m_dragFrom) : -1);
  }
  event->accept();
}

void RuleCanvas::mouseReleaseEvent(QMouseEvent* event) {
  const int pressed = std::exchange(m_pressed, -1);
  if (m_dragFrom) {
    const BlockPath from = *m_dragFrom;
    const int target = m_highlight;
    endDrag();
    const QPoint global = event->globalPosition().toPoint();
    if (m_deleteArea != nullptr && m_deleteArea->isVisible() &&
        m_deleteArea->rect().contains(m_deleteArea->mapFromGlobal(global))) {
      Rule changed = m_rule;
      take(changed, from);
      apply(std::move(changed), QStringLiteral("Delete %1").arg(from.id()), from);
    } else if (target >= 0) {
      Rule changed = m_rule;
      const Target to = m_pieces[static_cast<std::size_t>(target)].target();
      if (rules::move(changed, from, to)) {
        apply(std::move(changed), QStringLiteral("Move %1 to %2").arg(from.id(), to.path.id()), to.path);
      }
    }
    event->accept();
    return;
  }
  const int index = pieceAt(mapToScene(event->position().toPoint()));
  if (event->button() == Qt::LeftButton && index >= 0 && index == pressed &&
      m_pieces[static_cast<std::size_t>(index)].role == Piece::Role::Field) {
    editPiece(index);
  }
  event->accept();
}

void RuleCanvas::mouseDoubleClickEvent(QMouseEvent* event) {
  const int index = pieceAt(mapToScene(event->position().toPoint()));
  if (index >= 0 && m_pieces[static_cast<std::size_t>(index)].role == Piece::Role::Block) {
    editPiece(index);
  }
  event->accept();
}

void RuleCanvas::keyPressEvent(QKeyEvent* event) {
  if (!m_hasRule) {
    QGraphicsView::keyPressEvent(event);
    return;
  }
  const bool plain = (event->modifiers() & ~Qt::KeypadModifier) == Qt::NoModifier;
  switch (event->key()) {
  case Qt::Key_Left:
  case Qt::Key_Right:
  case Qt::Key_Up:
  case Qt::Key_Down:
    if (plain) {
      moveFocus(event->key());
      event->accept();
      return;
    }
    break;
  case Qt::Key_Return:
  case Qt::Key_Enter:
  case Qt::Key_F2:
    if (plain) {
      editFocused();
      event->accept();
      return;
    }
    break;
  case Qt::Key_Delete:
  case Qt::Key_Backspace:
    if (plain) {
      deleteFocused();
      event->accept();
      return;
    }
    break;
  case Qt::Key_Menu:
    showMenu(m_focus, viewport()->mapToGlobal(
                          m_focus >= 0 ? mapFromScene(m_pieces[static_cast<std::size_t>(m_focus)].rect.center())
                                       : QPoint(10, 10)));
    event->accept();
    return;
  case Qt::Key_F10:
    if (event->modifiers() == Qt::ShiftModifier) {
      showMenu(m_focus, viewport()->mapToGlobal(
                            m_focus >= 0 ? mapFromScene(m_pieces[static_cast<std::size_t>(m_focus)].rect.center())
                                         : QPoint(10, 10)));
      event->accept();
      return;
    }
    break;
  case Qt::Key_Escape:
    if (m_dragFrom) {
      endDrag();
      event->accept();
      return;
    }
    break;
  default:
    break;
  }
  QGraphicsView::keyPressEvent(event);
}

void RuleCanvas::contextMenuEvent(QContextMenuEvent* event) {
  int index = m_focus;
  if (event->reason() != QContextMenuEvent::Keyboard) {
    index = pieceAt(mapToScene(event->pos()));
    if (index >= 0 && m_pieces[static_cast<std::size_t>(index)].role != Piece::Role::Frame) {
      setFocusIndex(index);
    }
  }
  showMenu(index, event->globalPos());
  event->accept();
}

void RuleCanvas::resizeEvent(QResizeEvent* event) {
  QGraphicsView::resizeEvent(event);
  logPlaces();
}

void RuleCanvas::showEvent(QShowEvent* event) {
  QGraphicsView::showEvent(event);
  QTimer::singleShot(0, this, [this] { logPlaces(); });
}

void RuleCanvas::scrollContentsBy(int dx, int dy) {
  QGraphicsView::scrollContentsBy(dx, dy);
  if (m_editor != nullptr) {
    m_editor->move(m_editor->pos() + QPoint(dx, dy));
  }
  logPlaces();
}

void RuleCanvas::focusInEvent(QFocusEvent* event) {
  QGraphicsView::focusInEvent(event);
  if (m_focus < 0 && m_hasRule && event->reason() != Qt::MouseFocusReason) {
    moveFocus(Qt::Key_Right);
  }
}

void RuleCanvas::changeEvent(QEvent* event) {
  QGraphicsView::changeEvent(event);
  if (event->type() == QEvent::PaletteChange || event->type() == QEvent::FontChange ||
      event->type() == QEvent::StyleChange) {
    rebuild();
  }
}

bool RuleCanvas::eventFilter(QObject* watched, QEvent* event) {
  if (watched == m_editor && event->type() == QEvent::KeyPress) {
    const int key = static_cast<QKeyEvent*>(event)->key();
    if (key == Qt::Key_Escape) {
      cancelEditor();
      return true;
    }
    // Enter ends the edit here: it neither reaches the canvas (which would
    // edit the focus again) nor the window's default button.
    if (key == Qt::Key_Return || key == Qt::Key_Enter) {
      commitEditor();
      return true;
    }
  }
  return QGraphicsView::eventFilter(watched, event);
}

} // namespace mitcad::rules
