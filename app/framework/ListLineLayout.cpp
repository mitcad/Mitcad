// SPDX-License-Identifier: MIT
#include "ListLineLayout.hpp"

#include <algorithm>

#include <QAction>
#include <QFontMetrics>
#include <QLineEdit>
#include <QMargins>
#include <QStyle>
#include <QStyleOptionFrame>
#include <QWidget>
#include <QWidgetAction>

namespace mitcad {

namespace {

// The text a value field shows whole at its readable width.
const QString kReadableText = QStringLiteral("0000000");
// QLineEdit's own margin at each side of its text (QLineEditPrivate's
// horizontalMargin).
constexpr int kLineEditTextMargin = 2;
// Row spacing (docs/ui-conventions.md, Spacing): between the items of a
// line and between the lines.
constexpr int kSpacing = 4;

bool shown(const QLayoutItem* item) { return item != nullptr && !item->isEmpty(); }

int hintWidth(const QLayoutItem* item) { return shown(item) ? item->sizeHint().width() : 0; }

int hintHeight(const QLayoutItem* item) { return shown(item) ? item->sizeHint().height() : 0; }

// An item's height at a width: its text's for a field with a wrapped note.
int heightAt(const QLayoutItem* item, int width) {
  if (!shown(item)) {
    return 0;
  }
  const int height = item->hasHeightForWidth() ? item->heightForWidth(width) : item->sizeHint().height();
  return std::max(height, item->minimumSize().height());
}

void place(QLayoutItem* item, const QRect& rect, bool apply) {
  if (apply && shown(item)) {
    item->setGeometry(rect);
  }
}

// An item at its size hint's height, on the center line of a band of
// height band at y.
void placeCentred(QLayoutItem* item, int x, int y, int width, int band, bool apply) {
  const int height = hintHeight(item);
  place(item, QRect(x, y + std::max(0, band - height) / 2, width, height), apply);
}

// The height of a field's line edit (the line its label stands beside).
int editHeight(const QLineEdit* edit) { return edit != nullptr ? edit->sizeHint().height() : 0; }

} // namespace

ListLineLayout::ListLineLayout(QWidget* parent) : QLayout(parent) {
  setContentsMargins(0, 0, 0, 0);
  setSpacing(kSpacing);
}

ListLineLayout::~ListLineLayout() {
  delete m_lead;
  delete m_trail;
  for (const Cell& cell : m_cells) {
    delete cell.label;
    delete cell.field;
  }
}

void ListLineLayout::setLead(QWidget* widget) {
  delete m_lead;
  m_lead = nullptr;
  if (widget != nullptr) {
    addChildWidget(widget);
    m_lead = new QWidgetItem(widget);
  }
  invalidate();
}

void ListLineLayout::setTrail(QWidget* widget) {
  delete m_trail;
  m_trail = nullptr;
  if (widget != nullptr) {
    addChildWidget(widget);
    m_trail = new QWidgetItem(widget);
  }
  invalidate();
}

void ListLineLayout::addCell(QWidget* label, QWidget* field, QLineEdit* edit) {
  if (field == nullptr) {
    return;
  }
  Cell cell;
  if (label != nullptr) {
    addChildWidget(label);
    cell.label = new QWidgetItem(label);
  }
  addChildWidget(field);
  cell.field = new QWidgetItem(field);
  cell.edit = edit;
  m_cells.push_back(cell);
  invalidate();
}

const char* ListLineLayout::arrangementName(Arrangement arrangement) {
  switch (arrangement) {
  case Arrangement::OneLine:
    return "one line";
  case Arrangement::Stacked:
    return "stacked";
  case Arrangement::Under:
    return "under";
  }
  return "one line";
}

int ListLineLayout::readableWidth(const QLineEdit* edit) {
  if (edit == nullptr) {
    return 0;
  }
  const QStyle* style = edit->style();
  const QFontMetrics metrics = edit->fontMetrics();
  const QMargins text = edit->textMargins();
  int width = metrics.horizontalAdvance(kReadableText) + text.left() + text.right() + 2 * kLineEditTextMargin;
  // The trailing (and leading) buttons, as QLineEdit lays them out: a
  // button the line edit icon size and 6 pixels wide after a margin, a
  // widget action's widget at its own width.
  const int iconMargin = style->pixelMetric(QStyle::PM_LineEditIconMargin, nullptr, edit);
  const int iconRoom = style->pixelMetric(QStyle::PM_LineEditIconSize, nullptr, edit) + 6 + iconMargin;
  for (const QAction* action : edit->actions()) {
    if (!action->isVisible()) {
      continue;
    }
    const auto* widgetAction = qobject_cast<const QWidgetAction*>(action);
    const QWidget* widget = widgetAction != nullptr ? widgetAction->defaultWidget() : nullptr;
    if (widget != nullptr) {
      if (!widget->isHidden()) {
        width += widget->sizeHint().width() + iconMargin;
      }
    } else {
      width += iconRoom;
    }
  }
  // The frame (and a style sheet's padding and border) round the text.
  QStyleOptionFrame option;
  option.initFrom(edit);
  option.lineWidth = edit->hasFrame() ? style->pixelMetric(QStyle::PM_DefaultFrameWidth, &option, edit) : 0;
  option.midLineWidth = 0;
  option.state |= QStyle::State_Sunken;
  if (edit->isReadOnly()) {
    option.state |= QStyle::State_ReadOnly;
  }
  option.features = QStyleOptionFrame::None;
  const QSize framed = style->sizeFromContents(QStyle::CT_LineEdit, &option, QSize(width, metrics.height()), edit);
  return std::max(framed.width(), edit->minimumSizeHint().width());
}

std::vector<QLayoutItem*> ListLineLayout::items() const {
  std::vector<QLayoutItem*> all;
  if (m_lead != nullptr) {
    all.push_back(m_lead);
  }
  for (const Cell& cell : m_cells) {
    if (cell.label != nullptr) {
      all.push_back(cell.label);
    }
    if (cell.field != nullptr) {
      all.push_back(cell.field);
    }
  }
  if (m_trail != nullptr) {
    all.push_back(m_trail);
  }
  return all;
}

void ListLineLayout::addItem(QLayoutItem* item) {
  // An item added as to any layout: a field without a label.
  if (item == nullptr) {
    return;
  }
  Cell cell;
  cell.field = item;
  m_cells.push_back(cell);
  invalidate();
}

QLayoutItem* ListLineLayout::itemAt(int index) const {
  const std::vector<QLayoutItem*> all = items();
  return index >= 0 && index < static_cast<int>(all.size()) ? all[static_cast<std::size_t>(index)] : nullptr;
}

QLayoutItem* ListLineLayout::takeAt(int index) {
  QLayoutItem* item = itemAt(index);
  if (item == nullptr) {
    return nullptr;
  }
  if (item == m_lead) {
    m_lead = nullptr;
  } else if (item == m_trail) {
    m_trail = nullptr;
  } else {
    for (auto it = m_cells.begin(); it != m_cells.end(); ++it) {
      if (it->label == item) {
        it->label = nullptr;
      } else if (it->field == item) {
        it->field = nullptr;
      } else {
        continue;
      }
      if (it->label == nullptr && it->field == nullptr) {
        m_cells.erase(it);
      }
      break;
    }
  }
  invalidate();
  return item;
}

int ListLineLayout::count() const { return static_cast<int>(items().size()); }

int ListLineLayout::fieldNeed(const Cell& cell) const {
  if (!shown(cell.field)) {
    return 0;
  }
  return std::max(readableWidth(cell.edit), cell.field->minimumSize().width());
}

int ListLineLayout::labelColumn() const {
  int column = 0;
  for (const Cell& cell : m_cells) {
    if (shown(cell.field)) {
      column = std::max(column, hintWidth(cell.label));
    }
  }
  return column;
}

int ListLineLayout::oneLineWidth() const {
  int width = 0;
  int parts = 0;
  const auto add = [&width, &parts](int itemWidth) {
    width += itemWidth;
    ++parts;
  };
  if (shown(m_lead)) {
    add(hintWidth(m_lead));
  }
  for (const Cell& cell : m_cells) {
    if (!shown(cell.field)) {
      continue;
    }
    if (shown(cell.label)) {
      add(hintWidth(cell.label));
    }
    add(fieldNeed(cell));
  }
  if (shown(m_trail)) {
    add(hintWidth(m_trail));
  }
  return width + std::max(0, parts - 1) * std::max(0, spacing());
}

ListLineLayout::Arrangement ListLineLayout::arrangementAt(int width) const {
  if (width >= oneLineWidth()) {
    return Arrangement::OneLine;
  }
  const int column = labelColumn();
  int need = 0;
  for (const Cell& cell : m_cells) {
    need = std::max(need, fieldNeed(cell));
  }
  if ((column > 0 ? column + std::max(0, spacing()) : 0) + need <= width) {
    return Arrangement::Stacked;
  }
  return Arrangement::Under;
}

int ListLineLayout::arrange(const QRect& rect, bool apply) const {
  switch (arrangementAt(rect.width())) {
  case Arrangement::OneLine:
    return arrangeOneLine(rect, apply);
  case Arrangement::Stacked:
    return arrangeStacked(rect, false, apply);
  case Arrangement::Under:
    return arrangeStacked(rect, true, apply);
  }
  return 0;
}

int ListLineLayout::arrangeOneLine(const QRect& rect, bool apply) const {
  const int gap = std::max(0, spacing());
  // The fields share what the line has beyond their readable widths.
  int fields = 0;
  int band = std::max(hintHeight(m_lead), hintHeight(m_trail));
  for (const Cell& cell : m_cells) {
    if (shown(cell.field)) {
      ++fields;
      band = std::max({band, hintHeight(cell.label), editHeight(cell.edit)});
    }
  }
  const int extra = std::max(0, rect.width() - oneLineWidth());
  int height = band;
  int x = rect.x();
  if (shown(m_lead)) {
    placeCentred(m_lead, x, rect.y(), hintWidth(m_lead), band, apply);
    x += hintWidth(m_lead) + gap;
  }
  int index = 0;
  for (const Cell& cell : m_cells) {
    if (!shown(cell.field)) {
      continue;
    }
    if (shown(cell.label)) {
      placeCentred(cell.label, x, rect.y(), hintWidth(cell.label), band, apply);
      x += hintWidth(cell.label) + gap;
    }
    const int width = fieldNeed(cell) + extra / fields + (index < extra % fields ? 1 : 0);
    ++index;
    // The line edit on the band's center line, the note under it.
    const int top = cell.edit != nullptr ? std::max(0, band - editHeight(cell.edit)) / 2 : 0;
    const int fieldHeight = heightAt(cell.field, width);
    place(cell.field, QRect(x, rect.y() + top, width, fieldHeight), apply);
    height = std::max(height, top + fieldHeight);
    x += width + gap;
  }
  if (shown(m_trail)) {
    placeCentred(m_trail, rect.x() + rect.width() - hintWidth(m_trail), rect.y(), hintWidth(m_trail), band, apply);
  }
  return height;
}

int ListLineLayout::placeEnds(const QRect& rect, int y, bool apply) const {
  const int band = std::max(hintHeight(m_lead), hintHeight(m_trail));
  if (band == 0) {
    return 0;
  }
  const int gap = std::max(0, spacing());
  const int trail = hintWidth(m_trail);
  if (shown(m_lead)) {
    const int room = rect.width() - (shown(m_trail) ? trail + gap : 0);
    placeCentred(m_lead, rect.x(), y, std::max(0, std::min(hintWidth(m_lead), room)), band, apply);
  }
  if (shown(m_trail)) {
    placeCentred(m_trail, rect.x() + rect.width() - trail, y, trail, band, apply);
  }
  return band;
}

int ListLineLayout::arrangeStacked(const QRect& rect, bool under, bool apply) const {
  const int gap = std::max(0, spacing());
  int y = rect.y();
  const int ends = placeEnds(rect, y, apply);
  bool first = ends == 0;
  y += ends;
  const int column = under ? 0 : labelColumn();
  const int fieldX = rect.x() + (column > 0 ? column + gap : 0);
  const int fieldWidth = std::max(0, rect.x() + rect.width() - fieldX);
  for (const Cell& cell : m_cells) {
    if (!shown(cell.field)) {
      continue;
    }
    if (!first) {
      y += gap;
    }
    first = false;
    if (under) {
      // The label on a line of its own, the field under it across the
      // whole width.
      if (shown(cell.label)) {
        const int labelHeight = hintHeight(cell.label);
        place(cell.label, QRect(rect.x(), y, rect.width(), labelHeight), apply);
        y += labelHeight + gap / 2;
      }
      const int fieldHeight = heightAt(cell.field, rect.width());
      place(cell.field, QRect(rect.x(), y, rect.width(), fieldHeight), apply);
      y += fieldHeight;
      continue;
    }
    // The label in the column, on the center line of the field's line
    // edit; the field (and its note) beside it as wide as the rest.
    const int edit = editHeight(cell.edit);
    const int band = std::max(hintHeight(cell.label), edit);
    const int top = cell.edit != nullptr ? std::max(0, band - edit) / 2 : 0;
    placeCentred(cell.label, rect.x(), y, column, band, apply);
    const int fieldHeight = heightAt(cell.field, fieldWidth);
    place(cell.field, QRect(fieldX, y + top, fieldWidth, fieldHeight), apply);
    y += std::max(band, top + fieldHeight);
  }
  return y - rect.y();
}

QSize ListLineLayout::sizeHint() const {
  const QMargins margins = contentsMargins();
  const int width = oneLineWidth();
  const int height = arrange(QRect(0, 0, width, 0), false);
  return {width + margins.left() + margins.right(), height + margins.top() + margins.bottom()};
}

QSize ListLineLayout::minimumSize() const {
  // At its narrowest each field is under its label; the height is the one
  // line's, the parents asking heightForWidth for the height at a width.
  const QMargins margins = contentsMargins();
  int width = hintWidth(m_lead) + hintWidth(m_trail) + (shown(m_lead) && shown(m_trail) ? std::max(0, spacing()) : 0);
  for (const Cell& cell : m_cells) {
    if (shown(cell.field)) {
      width = std::max({width, hintWidth(cell.label), cell.field->minimumSize().width()});
    }
  }
  return {width + margins.left() + margins.right(), sizeHint().height()};
}

bool ListLineLayout::hasHeightForWidth() const { return true; }

int ListLineLayout::heightForWidth(int width) const {
  if (width != m_cachedWidth) {
    const QMargins margins = contentsMargins();
    const int inner = std::max(0, width - margins.left() - margins.right());
    m_cachedHeight = arrange(QRect(0, 0, inner, 0), false) + margins.top() + margins.bottom();
    m_cachedWidth = width;
  }
  return m_cachedHeight;
}

Qt::Orientations ListLineLayout::expandingDirections() const { return Qt::Horizontal; }

void ListLineLayout::setGeometry(const QRect& rect) {
  QLayout::setGeometry(rect);
  const QRect inner = rect.marginsRemoved(contentsMargins());
  m_arranged = arrangementAt(inner.width());
  arrange(inner, true);
}

void ListLineLayout::invalidate() {
  m_cachedWidth = -1;
  m_cachedHeight = -1;
  QLayout::invalidate();
}

} // namespace mitcad
