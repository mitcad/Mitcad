// SPDX-License-Identifier: MIT
#include "LibraryPicker.hpp"

#include <algorithm>
#include <utility>

#include <QApplication>
#include <QColor>
#include <QEvent>
#include <QFont>
#include <QFontMetrics>
#include <QGuiApplication>
#include <QHideEvent>
#include <QIcon>
#include <QKeyEvent>
#include <QLineEdit>
#include <QListWidget>
#include <QMargins>
#include <QModelIndex>
#include <QPainter>
#include <QPalette>
#include <QPen>
#include <QPixmap>
#include <QPointF>
#include <QRect>
#include <QScreen>
#include <QScrollBar>
#include <QShowEvent>
#include <QSize>
#include <QStringList>
#include <QStyle>
#include <QStyleOptionViewItem>
#include <QStyledItemDelegate>
#include <QVBoxLayout>
#include <QtLogging>

#include "../framework/TestSync.hpp"
#include "../framework/Theme.hpp"
#include "../platform/MacDialogs.hpp"
#include "../platform/NativePicker.hpp"
#include "../framework/UiSizes.hpp"

namespace mitcad {
namespace {

constexpr int kValueRole = Qt::UserRole;          // an entry's value
constexpr int kHeadingRole = Qt::UserRole + 1;    // true on a heading
constexpr int kTagRole = PickerTagRole;           // the small text at the right (mitcad#1108)
constexpr int kLineRole = PickerLineRole;         // true: a line above the entry
constexpr int kSearchRole = Qt::UserRole + 4;     // more text the search matches
constexpr int kSwatchSize = 20;
constexpr int kCheckSize = 14; // the current choice's check (mitcad#1108)
constexpr int kLineSpace = 7;  // the room of a line above an entry
constexpr int kTagGap = 16;    // between an entry's name and its tag
const QSize kPickerSize(320, 400);

bool isHeading(const QListWidgetItem* item) { return item->data(kHeadingRole).toBool(); }

// A tag's font: the list's, smaller.
QFont tagFont(QFont font) {
  if (font.pointSizeF() > 0.0) {
    font.setPointSizeF(font.pointSizeF() * 0.85);
  } else if (font.pixelSize() > 0) {
    font.setPixelSize(std::max(1, font.pixelSize() * 85 / 100));
  }
  return font;
}

// The check before the current choice, in the selection's color; blank
// beside the other entries, so that their names line up.
QIcon checkIcon(bool checked, const QColor& color) {
  const qreal ratio = qGuiApp != nullptr ? qGuiApp->devicePixelRatio() : 1.0;
  QPixmap pixmap(static_cast<int>(kCheckSize * ratio), static_cast<int>(kCheckSize * ratio));
  pixmap.setDevicePixelRatio(ratio);
  pixmap.fill(Qt::transparent);
  if (checked) {
    QPainter painter(&pixmap);
    painter.setRenderHint(QPainter::Antialiasing);
    painter.setPen(QPen(color, 1.8, Qt::SolidLine, Qt::RoundCap, Qt::RoundJoin));
    const QPointF points[] = {{2.5, 7.5}, {5.5, 10.5}, {11.5, 3.5}};
    painter.drawPolyline(points, 3);
  }
  return QIcon(pixmap);
}

// An entry with its tag at the right in the secondary text's color, its
// name elided in the middle when the two do not fit, and a line above it
// when it has one (mitcad#1108).
class EntryDelegate : public QStyledItemDelegate {
public:
  using QStyledItemDelegate::QStyledItemDelegate;

  void paint(QPainter* painter, const QStyleOptionViewItem& option, const QModelIndex& index) const override {
    QRect rect = option.rect;
    if (index.data(kLineRole).toBool()) {
      painter->save();
      painter->setPen(QPen(themeColor(ColorRole::Line), 1));
      const int y = rect.top() + kLineSpace / 2;
      painter->drawLine(rect.left() + 4, y, rect.right() - 4, y);
      painter->restore();
      rect.setTop(rect.top() + kLineSpace);
    }
    QStyleOptionViewItem item(option);
    initStyleOption(&item, index);
    item.rect = rect;
    const QString tag = index.data(kTagRole).toString();
    const QFont tagFace = tagFont(item.font);
    const int tagWidth = tag.isEmpty() ? 0 : QFontMetrics(tagFace).horizontalAdvance(tag);
    if (tagWidth > 0) {
      // The name in the room the tag leaves.
      const int icon = item.icon.isNull() ? 0 : item.decorationSize.width() + 8;
      const int room = std::max(0, rect.width() - icon - tagWidth - kTagGap - 8);
      item.text = item.fontMetrics.elidedText(item.text, Qt::ElideMiddle, room);
    }
    const QWidget* widget = option.widget;
    QStyle* style = widget != nullptr ? widget->style() : QApplication::style();
    style->drawControl(QStyle::CE_ItemViewItem, &item, painter, widget);
    if (tagWidth > 0) {
      painter->save();
      painter->setFont(tagFace);
      const bool selected = (option.state & QStyle::State_Selected) != 0;
      painter->setPen(selected ? option.palette.color(QPalette::HighlightedText) : themeColor(ColorRole::TextSecondary));
      painter->drawText(rect.adjusted(0, 0, -6, 0), Qt::AlignRight | Qt::AlignVCenter, tag);
      painter->restore();
    }
  }

  QSize sizeHint(const QStyleOptionViewItem& option, const QModelIndex& index) const override {
    QSize size = QStyledItemDelegate::sizeHint(option, index);
    if (index.data(kLineRole).toBool()) {
      size.setHeight(size.height() + kLineSpace);
    }
    return size;
  }
};

} // namespace

LibraryPicker::LibraryPicker(QWidget* parent, const QString& log, std::function<void(const QString&)> chosen)
    : QFrame(parent, Qt::Tool | Qt::FramelessWindowHint), m_log(log), m_chosen(std::move(chosen)) {
  setAttribute(Qt::WA_DeleteOnClose);
  setObjectName(QStringLiteral("libraryPicker"));
  setFrameShape(QFrame::StyledPanel);
  m_window = parent != nullptr ? parent->window() : nullptr;
  auto* layout = new QVBoxLayout(this);
  layout->setContentsMargins(6, 6, 6, 6);
  layout->setSpacing(4);
  m_search = new QLineEdit;
  m_search->setObjectName(QStringLiteral("libraryPickerSearch"));
  m_search->setPlaceholderText(tr("Search"));
  m_search->setClearButtonEnabled(true);
  mac::styleSearchField(m_search);
  m_search->installEventFilter(this);
  layout->addWidget(m_search);
  m_list = new QListWidget;
  m_list->setObjectName(QStringLiteral("libraryPickerList"));
  m_list->setIconSize(uiIconSize(kSwatchSize, kSwatchSize));
  m_list->setSelectionMode(QAbstractItemView::SingleSelection);
  m_list->setUniformItemSizes(false);
  m_list->setItemDelegate(new EntryDelegate(m_list));
  m_list->installEventFilter(this);
  layout->addWidget(m_list, 1);
  connect(m_search, &QLineEdit::textChanged, this, &LibraryPicker::filter);
  connect(m_list, &QListWidget::itemClicked, this, &LibraryPicker::choose);
  connect(m_list, &QListWidget::itemActivated, this, &LibraryPicker::choose);
  resize(kPickerSize);
}

LibraryPicker* LibraryPicker::open(QWidget* parent, const QString& title, const QString& log,
                                   const QVector<PickerEntry>& entries, const QString& current, const QPoint& at,
                                   std::function<void(const QString&)> chosen, const QString& anchor,
                                   int minimumWidth) {
  auto* picker = new LibraryPicker(parent, log, std::move(chosen));
  picker->setWindowTitle(title);
  picker->fill(entries, current);
  picker->resize(std::max({kPickerSize.width(), minimumWidth, picker->fittingWidth()}), kPickerSize.height());
  // The AppKit frontend's popover binds its search field and list.
  if (showNativePicker(picker, picker->m_search, picker->m_list, anchor)) {
    return picker;
  }
  // Under the pointer, all of it on the screen.
  QScreen* screen = QGuiApplication::screenAt(at);
  if (screen == nullptr && parent != nullptr) {
    screen = parent->screen();
  }
  // As wide as its names need (mitcad#1108), within the screen.
  int pickerWidth = std::max({kPickerSize.width(), minimumWidth, picker->fittingWidth()});
  if (screen != nullptr) {
    pickerWidth = std::min(pickerWidth, screen->availableGeometry().width());
  }
  picker->resize(pickerWidth, kPickerSize.height());
  QPoint place = at;
  if (screen != nullptr) {
    const QRect area = screen->availableGeometry();
    place.setX(std::max(area.left(), std::min(place.x(), area.right() - picker->width())));
    place.setY(std::max(area.top(), std::min(place.y(), area.bottom() - picker->height())));
  }
  picker->move(place);
  picker->show();
  picker->raise();
  picker->activateWindow();
  picker->m_search->setFocus(Qt::ActiveWindowFocusReason);
  return picker;
}

void LibraryPicker::fill(const QVector<PickerEntry>& entries, const QString& current) {
  m_list->clear();
  QString section;
  bool first = true;
  QString currentName;
  QStringList sections; // "Acme 2", logged when there are headings
  int inSection = 0;
  // A check before the current choice (mitcad#1108): a gutter for it before
  // every entry without a swatch.
  const bool checks =
      std::any_of(entries.begin(), entries.end(), [](const PickerEntry& entry) { return entry.checked; });
  const QColor checkColor = palette().color(QPalette::Highlight);
  const auto closeSection = [&sections, &section, &inSection] {
    if (!section.isEmpty()) {
      sections << QStringLiteral("%1 %2").arg(section).arg(inSection);
    }
    inSection = 0;
  };
  for (const PickerEntry& entry : entries) {
    if (first || entry.section != section) {
      closeSection();
      section = entry.section;
      first = false;
      if (!section.isEmpty()) {
        auto* heading = new QListWidgetItem(section, m_list);
        heading->setFlags(Qt::NoItemFlags);
        heading->setData(kHeadingRole, true);
        QFont font = heading->font();
        font.setBold(true);
        heading->setFont(font);
      }
    }
    ++inSection;
    auto* item = new QListWidgetItem(entry.name, m_list);
    if (!entry.swatch.isNull()) {
      item->setIcon(QIcon(QPixmap::fromImage(entry.swatch)));
    } else if (checks) {
      item->setIcon(checkIcon(entry.checked, checkColor));
    }
    item->setData(kValueRole, entry.value);
    item->setData(kTagRole, entry.tag);
    item->setData(kLineRole, entry.lineBefore);
    item->setData(kSearchRole, entry.search);
    item->setToolTip(entry.tooltip);
    if (entry.value == current) {
      m_list->setCurrentItem(item);
      currentName = entry.name;
    }
  }
  if (m_list->currentItem() == nullptr) {
    step(1);
  } else {
    m_list->scrollToItem(m_list->currentItem(), QAbstractItemView::PositionAtCenter);
  }
  qDebug().noquote() << QStringLiteral("%1 opened: %2 entries, current %3")
                            .arg(m_log)
                            .arg(entries.size())
                            .arg(currentName.isEmpty() ? QStringLiteral("none") : currentName);
  closeSection();
  if (!sections.isEmpty()) {
    qDebug().noquote() << QStringLiteral("%1 sections: %2").arg(m_log, sections.join(QStringLiteral(" | ")));
  }
}

int LibraryPicker::fittingWidth() const {
  const QFontMetrics names(m_list->font());
  QFont bold = m_list->font();
  bold.setBold(true);
  const QFontMetrics headings(bold);
  const QFontMetrics tags(tagFont(m_list->font()));
  int widest = 0;
  for (int i = 0; i < m_list->count(); ++i) {
    const QListWidgetItem* item = m_list->item(i);
    int needed = (isHeading(item) ? headings : names).horizontalAdvance(item->text());
    if (!item->icon().isNull()) {
      needed += m_list->iconSize().width() + 8;
    }
    const QString tag = item->data(kTagRole).toString();
    if (!tag.isEmpty()) {
      needed += kTagGap + tags.horizontalAdvance(tag) + 6;
    }
    widest = std::max(widest, needed);
  }
  // The list's frame, its scroll bar, the item's padding and the window's
  // margins.
  const QMargins margins = contentsMargins() + layout()->contentsMargins();
  return widest + 2 * m_list->frameWidth() + m_list->verticalScrollBar()->sizeHint().width() + 16 + margins.left() +
         margins.right();
}

void LibraryPicker::filter(const QString& text) {
  const QString wanted = text.trimmed();
  // The entries that contain the text; a heading while one of its entries
  // shows.
  QListWidgetItem* heading = nullptr;
  bool headingUsed = false;
  for (int i = 0; i < m_list->count(); ++i) {
    QListWidgetItem* item = m_list->item(i);
    if (isHeading(item)) {
      if (heading != nullptr) {
        heading->setHidden(!headingUsed);
      }
      heading = item;
      headingUsed = false;
      continue;
    }
    // Also the entry's more text (mitcad#1108: a printer's vendor and nozzle).
    const bool shown = wanted.isEmpty() || item->text().contains(wanted, Qt::CaseInsensitive) ||
                       item->data(kSearchRole).toString().contains(wanted, Qt::CaseInsensitive);
    item->setHidden(!shown);
    headingUsed = headingUsed || shown;
  }
  if (heading != nullptr) {
    heading->setHidden(!headingUsed);
  }
  // The first match takes Enter.
  QListWidgetItem* current = m_list->currentItem();
  if (current == nullptr || current->isHidden() || !wanted.isEmpty()) {
    m_list->setCurrentItem(nullptr);
    step(1);
  }
  qDebug().noquote() << QStringLiteral("%1 search: %2, first %3")
                            .arg(m_log, wanted,
                                 m_list->currentItem() != nullptr ? m_list->currentItem()->text() : QStringLiteral("none"));
  scheduleLogPlaces();
}

void LibraryPicker::step(int by) {
  const int count = m_list->count();
  int at = m_list->currentRow();
  for (int tried = 0; tried < count; ++tried) {
    at = at < 0 ? (by > 0 ? 0 : count - 1) : at + by;
    if (at < 0 || at >= count) {
      return; // stays at the first or the last
    }
    QListWidgetItem* item = m_list->item(at);
    if (!item->isHidden() && !isHeading(item)) {
      m_list->setCurrentItem(item);
      m_list->scrollToItem(item);
      return;
    }
  }
}

void LibraryPicker::choose(QListWidgetItem* item) {
  if (item == nullptr || isHeading(item) || item->isHidden() || m_done) {
    return;
  }
  m_done = true;
  const QString value = item->data(kValueRole).toString();
  qDebug().noquote() << QStringLiteral("%1 chose: %2").arg(m_log, item->text());
  // The picker goes first: what is chosen may open other windows.
  const std::function<void(const QString&)> chosen = m_chosen;
  close();
  if (chosen) {
    chosen(value);
  }
}

bool LibraryPicker::event(QEvent* event) {
  // As a popover: a click elsewhere activates another window, which closes
  // it (once it had the activation; a test's input may never give it).
  if (event->type() == QEvent::WindowActivate) {
    m_activated = true;
  } else if (event->type() == QEvent::WindowDeactivate && m_activated && !m_done) {
    TestSync::singleShot(0, this, [this] {
      if (!isActiveWindow() && !m_done) {
        close();
      }
    });
  }
  return QFrame::event(event);
}

bool LibraryPicker::eventFilter(QObject* watched, QEvent* event) {
  const bool search = watched == m_search;
  if ((search || watched == m_list) && event->type() == QEvent::ShortcutOverride) {
    // The picker's keys before the main window's shortcuts: Enter would
    // otherwise confirm the command behind it and Esc cancel it, and the
    // picker would never see them.
    auto* key = static_cast<QKeyEvent*>(event);
    switch (key->key()) {
    case Qt::Key_Return:
    case Qt::Key_Enter:
    case Qt::Key_Escape:
    case Qt::Key_Up:
    case Qt::Key_Down:
      event->accept();
      return true;
    default:
      break;
    }
  }
  if ((search || watched == m_list) && event->type() == QEvent::KeyPress) {
    const auto* key = static_cast<QKeyEvent*>(event);
    switch (key->key()) {
    case Qt::Key_Down:
    case Qt::Key_Up:
      if (search) {
        step(key->key() == Qt::Key_Down ? 1 : -1);
        return true;
      }
      break; // the list steps by itself
    case Qt::Key_Return:
    case Qt::Key_Enter:
      choose(m_list->currentItem());
      return true;
    case Qt::Key_Escape:
      close();
      return true;
    default:
      break;
    }
  }
  return QFrame::eventFilter(watched, event);
}

void LibraryPicker::showEvent(QShowEvent* event) {
  QFrame::showEvent(event);
  scheduleLogPlaces();
}

void LibraryPicker::hideEvent(QHideEvent* event) {
  QFrame::hideEvent(event);
  if (!m_done) {
    m_done = true;
    qDebug().noquote() << QStringLiteral("%1 closed").arg(m_log);
  }
}

void LibraryPicker::scheduleLogPlaces() {
  TestSync::singleShot(0, this, [this] { logPlaces(); });
}

void LibraryPicker::logPlaces() {
  if (!isVisible()) {
    return;
  }
  const QPoint origin = m_window != nullptr ? m_window->mapToGlobal(QPoint(0, 0)) : QPoint();
  const auto at = [&origin](const QWidget* widget, const QPoint& point) {
    const QPoint global = widget->mapToGlobal(point) - origin;
    return QStringLiteral("%1,%2").arg(global.x()).arg(global.y());
  };
  QStringList lines;
  lines << QStringLiteral("%1 search at %2").arg(m_log, at(m_search, m_search->rect().center()));
  for (int i = 0; i < m_list->count(); ++i) {
    const QListWidgetItem* item = m_list->item(i);
    if (item->isHidden() || isHeading(item)) {
      continue;
    }
    const QRect rect = m_list->visualItemRect(item);
    if (rect.isValid() && m_list->viewport()->rect().contains(rect.center())) {
      lines << QStringLiteral("%1 item %2 at %3").arg(m_log, item->text(), at(m_list->viewport(), rect.center()));
    }
  }
  const QString logged = lines.join(QLatin1Char('\n'));
  if (logged == m_logged) {
    return;
  }
  m_logged = logged;
  for (const QString& line : std::as_const(lines)) {
    qDebug().noquote() << line;
  }
}

} // namespace mitcad
