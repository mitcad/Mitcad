// SPDX-License-Identifier: MIT
#include "AppearancePicker.hpp"

#include <algorithm>

#include <QApplication>
#include <QColor>
#include <QEvent>
#include <QFont>
#include <QGuiApplication>
#include <QImage>
#include <QKeyEvent>
#include <QLineEdit>
#include <QListWidget>
#include <QListWidgetItem>
#include <QPainter>
#include <QPalette>
#include <QPen>
#include <QPixmap>
#include <QPoint>
#include <QRect>
#include <QRectF>
#include <QScreen>
#include <QSize>
#include <QVBoxLayout>
#include <QtLogging>

#include "../framework/TestSync.hpp"
#include "../framework/Theme.hpp"
#include "../platform/NativePicker.hpp"
#include "PrintLibrary.hpp"
#include "../framework/UiSizes.hpp"

namespace mitcad {
namespace {

constexpr int kIdRole = Qt::UserRole; // an entry's appearance id; empty on a heading
const char* kNoneId = "__none";
const char* kEditId = "__edit_appearances";
// The list's swatches at Icon size (mitcad#1556).
int listIconSize() { return uiIconSize(22); }

qreal screenRatio() {
  return qGuiApp != nullptr ? qGuiApp->devicePixelRatio() : 1.0;
}

// A ring around a swatch: a material's default.
QPixmap withRing(QPixmap pixmap, int size) {
  QPainter painter(&pixmap);
  painter.setRenderHint(QPainter::Antialiasing);
  QPen pen(QApplication::palette().color(QPalette::Highlight), 2.0);
  painter.setPen(pen);
  painter.setBrush(Qt::NoBrush);
  painter.drawEllipse(QRectF(1.0, 1.0, size - 2.0, size - 2.0));
  return pixmap;
}

} // namespace

QColor swatchEdgeColor() {
  // The muted title text's role: at least 3:1 on the panel in both themes,
  // so that a black or white swatch keeps its outline (mitcad#1083).
  return themeColor(ColorRole::TitleText);
}

QIcon appearanceIcon(const Appearance& appearance, int size, bool marked) {
  QPixmap pixmap = QPixmap::fromImage(appearanceSwatch(appearance, size, screenRatio()));
  {
    // A 1 px edge round the sphere (appearanceSwatch's radius, 0.9).
    QPainter painter(&pixmap);
    painter.setRenderHint(QPainter::Antialiasing);
    painter.setPen(QPen(swatchEdgeColor(), 1.0));
    painter.setBrush(Qt::NoBrush);
    painter.drawEllipse(QRectF(size * 0.05 + 0.5, size * 0.05 + 0.5, size * 0.9 - 1.0, size * 0.9 - 1.0));
  }
  return QIcon(marked ? withRing(pixmap, size) : pixmap);
}

QIcon colorIcon(const QColor& color, int size, bool marked) {
  const qreal ratio = screenRatio();
  QPixmap pixmap(static_cast<int>(size * ratio), static_cast<int>(size * ratio));
  pixmap.setDevicePixelRatio(ratio);
  pixmap.fill(Qt::transparent);
  {
    QPainter painter(&pixmap);
    painter.setRenderHint(QPainter::Antialiasing);
    // No color: a body's default look (the view's role).
    const QColor fill = color.isValid() ? color : viewColor(ColorRole::Body, false);
    painter.setBrush(fill);
    painter.setPen(QPen(swatchEdgeColor(), 1));
    painter.drawEllipse(QRectF(2.5, 2.5, size - 5.0, size - 5.0));
  }
  return QIcon(marked ? withRing(pixmap, size) : pixmap);
}

AppearancePopover::AppearancePopover(const QString& logName, QWidget* parent)
    : QFrame(parent, Qt::Tool | Qt::FramelessWindowHint), m_log(logName) {
  setFrameShape(QFrame::StyledPanel);
  setObjectName(QStringLiteral("appearancePicker"));
  auto* layout = new QVBoxLayout(this);
  layout->setContentsMargins(6, 6, 6, 6);
  layout->setSpacing(4);
  m_search = new QLineEdit;
  m_search->setObjectName(QStringLiteral("appearancePickerSearch"));
  m_search->setPlaceholderText(tr("Search appearances"));
  m_search->setClearButtonEnabled(true);
  m_search->installEventFilter(this);
  layout->addWidget(m_search);
  m_list = new QListWidget;
  m_list->setObjectName(QStringLiteral("appearancePickerList"));
  m_list->setIconSize(QSize(listIconSize(), listIconSize()));
  m_list->setUniformItemSizes(true);
  m_list->installEventFilter(this);
  layout->addWidget(m_list, 1);
  resize(280, 360);
  connect(m_search, &QLineEdit::textChanged, this, [this] { filter(); });
  connect(m_list, &QListWidget::itemClicked, this, [this](QListWidgetItem* item) { choose(item); });
  connect(m_list, &QListWidget::itemActivated, this, [this](QListWidgetItem* item) { choose(item); });
}

void AppearancePopover::setAppearances(const QVector<Appearance>& appearances) {
  m_appearances = appearances;
}

QString designSectionKey() {
  return QStringLiteral("design");
}

QVector<AppearanceSection> appearanceSections(const QVector<Appearance>& appearances, const QString& materialClass,
                                              const QString& designTitle) {
  QVector<AppearanceSection> sections;
  QVector<const Appearance*> placed;
  // An appearance from a model before classes (none), or of a class not
  // known here, is Other.
  const auto classOf = [](const Appearance* a) {
    return materialClasses().contains(a->materialClass) ? a->materialClass : QStringLiteral("other");
  };
  // The design's first, then the library's, in the query's order.
  const auto ordered = [&appearances] {
    QVector<const Appearance*> list;
    for (const Appearance& a : appearances) {
      if (!a.library) {
        list.append(&a);
      }
    }
    for (const Appearance& a : appearances) {
      if (a.library) {
        list.append(&a);
      }
    }
    return list;
  }();
  const auto add = [&sections, &placed](const QString& key, const QString& title,
                                        const QVector<const Appearance*>& list) {
    if (!list.isEmpty()) {
      sections.append(AppearanceSection{key, title, list});
      placed += list;
    }
  };
  // Those of a class not placed yet: of the design and the library, or of
  // the library only.
  const auto ofClass = [&ordered, &placed, &classOf](const QString& wanted, bool libraryOnly) {
    QVector<const Appearance*> list;
    for (const Appearance* a : ordered) {
      if (classOf(a) == wanted && (a->library || !libraryOnly) && !placed.contains(a)) {
        list.append(a);
      }
    }
    return list;
  };
  if (materialClasses().contains(materialClass)) {
    add(materialClass, materialClassTitle(materialClass), ofClass(materialClass, false));
    // A printed plastic is a plastic too: General Plastic's next.
    if (materialClass == QStringLiteral("fdm_plastic") || materialClass == QStringLiteral("resin")) {
      add(QStringLiteral("plastic"), materialClassTitle(QStringLiteral("plastic")),
          ofClass(QStringLiteral("plastic"), false));
    }
  }
  // The design's others, then the library's by class.
  QVector<const Appearance*> design;
  for (const Appearance* a : ordered) {
    if (!a->library && !placed.contains(a)) {
      design.append(a);
    }
  }
  add(designSectionKey(), designTitle, design);
  for (const QString& cls : materialClasses()) {
    add(cls, materialClassTitle(cls), ofClass(cls, true));
  }
  return sections;
}

QString appearanceSectionsText(const QVector<AppearanceSection>& sections) {
  QStringList parts;
  for (const AppearanceSection& section : sections) {
    parts << QStringLiteral("%1 %2").arg(section.title).arg(section.appearances.size());
  }
  return parts.join(QStringLiteral(" | "));
}

void AppearancePopover::open(QWidget* anchor, const QString& current, const QStringList& marked,
                             const QString& materialClass) {
  m_search->clear();
  fill(current, marked, materialClass);
  // Under the anchor, kept on its screen.
  QPoint at = anchor != nullptr ? anchor->mapToGlobal(QPoint(0, anchor->height())) : QPoint();
  if (const QScreen* screen = anchor != nullptr ? anchor->screen() : nullptr) {
    const QRect room = screen->availableGeometry();
    at.setX(std::clamp(at.x(), room.left(), std::max(room.left(), room.right() - width())));
    if (at.y() + height() > room.bottom() && anchor != nullptr) {
      at.setY(std::max(room.top(), anchor->mapToGlobal(QPoint(0, 0)).y() - height()));
    }
  }
  move(at);
  m_activated = false;
  // The AppKit frontend's popover under the anchor's native control binds
  // the search field and the list (mitcad#1068).
  if (!showNativePicker(this, m_search, m_list, anchor != nullptr ? anchor->objectName() : QString())) {
    show();
    raise();
    activateWindow();
    m_search->setFocus(Qt::ActiveWindowFocusReason);
  }
  int shown = 0;
  for (int row = 0; row < m_list->count(); ++row) {
    shown += m_list->item(row)->data(kIdRole).toString().isEmpty() ? 0 : 1;
  }
  qDebug().noquote() << QStringLiteral("%1 picker opened: %2 appearances").arg(m_log).arg(shown);
}

void AppearancePopover::fill(const QString& current, const QStringList& marked, const QString& materialClass) {
  m_list->clear();
  const auto heading = [this](const QString& title) {
    auto* item = new QListWidgetItem(title, m_list);
    item->setFlags(Qt::ItemIsEnabled);
    QFont bold = item->font();
    bold.setBold(true);
    item->setFont(bold);
    return item;
  };
  QListWidgetItem* selected = nullptr;
  const auto entry = [this, &current, &marked, &selected](const Appearance& a) {
    const QString key = a.id + QLatin1Char('|') + a.baseColor.name() + QString::number(a.transmission);
    if (!m_icons.contains(key)) {
      m_icons.insert(key, appearanceIcon(a, listIconSize()));
    }
    QString label = a.name;
    if (marked.contains(a.id)) {
      label += QStringLiteral("  ✓");
    }
    auto* item = new QListWidgetItem(m_icons.value(key), label, m_list);
    item->setData(kIdRole, a.id);
    item->setData(Qt::AccessibleTextRole, a.name); // without the check mark
    item->setToolTip(a.name);
    if (a.id == current) {
      selected = item;
    }
  };
  if (offerNone) {
    auto* none = new QListWidgetItem(tr("None"), m_list);
    none->setData(kIdRole, QString::fromLatin1(kNoneId));
    if (current.isEmpty()) {
      selected = none;
    }
  }
  // The material's class first (mitcad#1102), then the design's own (those
  // of its materials and imports), then the library's by class
  // (mitcad#1133).
  const QVector<AppearanceSection> sections = appearanceSections(m_appearances, materialClass, tr("This design"));
  for (const AppearanceSection& section : sections) {
    heading(section.title);
    for (const Appearance* a : section.appearances) {
      entry(*a);
    }
  }
  qDebug().noquote() << QStringLiteral("%1 picker sections: %2").arg(m_log, appearanceSectionsText(sections));
  if (editAppearances) {
    auto* edit = new QListWidgetItem(tr("Edit Appearances…"), m_list);
    edit->setData(kIdRole, QString::fromLatin1(kEditId));
  }
  if (selected != nullptr) {
    m_list->setCurrentItem(selected);
    m_list->scrollToItem(selected, QAbstractItemView::PositionAtCenter);
  }
}

void AppearancePopover::filter() {
  const QString wanted = m_search->text().trimmed();
  int shown = 0;
  QListWidgetItem* first = nullptr;
  // A section's heading shows while one of its entries does (mitcad#1133).
  QListWidgetItem* heading = nullptr;
  bool headingUsed = false;
  for (int row = 0; row < m_list->count(); ++row) {
    QListWidgetItem* item = m_list->item(row);
    const QString id = item->data(kIdRole).toString();
    const bool special = id == QLatin1String(kNoneId) || id == QLatin1String(kEditId);
    if (id.isEmpty() || special) {
      if (heading != nullptr) {
        heading->setHidden(!headingUsed);
      }
      heading = id.isEmpty() ? item : nullptr;
      headingUsed = false;
      if (id.isEmpty()) {
        continue;
      }
    }
    const bool match = wanted.isEmpty() || (!special && (item->toolTip().contains(wanted, Qt::CaseInsensitive) ||
                                                         id.contains(wanted, Qt::CaseInsensitive)));
    item->setHidden(!match);
    headingUsed = headingUsed || match;
    if (match && !special) {
      ++shown;
      if (first == nullptr) {
        first = item;
      }
    }
  }
  if (heading != nullptr) {
    heading->setHidden(!headingUsed);
  }
  // The first match current, which Return chooses.
  if (!wanted.isEmpty() && first != nullptr) {
    m_list->setCurrentItem(first);
    m_list->scrollToItem(first);
  }
  qDebug().noquote() << QStringLiteral("%1 picker search: %2, %3 shown").arg(m_log, wanted).arg(shown);
}

void AppearancePopover::choose(QListWidgetItem* item) {
  if (item == nullptr || item->isHidden()) {
    return;
  }
  const QString id = item->data(kIdRole).toString();
  if (id.isEmpty()) {
    return;
  }
  hide();
  if (id == QLatin1String(kEditId)) {
    qDebug().noquote() << QStringLiteral("%1 picker: Edit Appearances…").arg(m_log);
    if (editAppearances) {
      // Not while the list still handles its click.
      const std::function<void()> edit = editAppearances;
      TestSync::singleShot(0, this, [edit] { edit(); });
    }
    return;
  }
  const QString chosenId = id == QLatin1String(kNoneId) ? QString() : id;
  qDebug().noquote() << QStringLiteral("%1 picker chosen: %2 (%3)")
                            .arg(m_log, item->toolTip().isEmpty() ? item->text() : item->toolTip(),
                                 chosenId.isEmpty() ? QStringLiteral("none") : chosenId);
  // The owner's command runs after the popover's own event.
  TestSync::singleShot(0, this, [this, chosenId] { emit chosen(chosenId); });
}

bool AppearancePopover::event(QEvent* event) {
  // As a popup: a click elsewhere activates another window, which closes
  // it (once it had the activation; a test's input may never give it).
  if (event->type() == QEvent::WindowActivate) {
    m_activated = true;
  } else if (event->type() == QEvent::WindowDeactivate && m_activated && isVisible()) {
    TestSync::singleShot(0, this, [this] {
      if (isVisible() && !isActiveWindow()) {
        qDebug().noquote() << QStringLiteral("%1 picker closed").arg(m_log);
        hide();
      }
    });
  }
  return QFrame::event(event);
}

bool AppearancePopover::eventFilter(QObject* watched, QEvent* event) {
  if ((watched == m_search || watched == m_list) && event->type() == QEvent::ShortcutOverride) {
    // Its keys before the shortcuts of the window under it (Qt applies a
    // tool window's parent window's shortcuts in it too).
    switch (static_cast<QKeyEvent*>(event)->key()) {
    case Qt::Key_Return:
    case Qt::Key_Enter:
    case Qt::Key_Escape:
    case Qt::Key_Down:
      event->accept();
      return true;
    default:
      break;
    }
  }
  if (event->type() == QEvent::KeyPress) {
    const auto* key = static_cast<QKeyEvent*>(event);
    if (key->key() == Qt::Key_Escape) {
      qDebug().noquote() << QStringLiteral("%1 picker closed").arg(m_log);
      hide();
      return true;
    }
    if (watched == m_search && (key->key() == Qt::Key_Return || key->key() == Qt::Key_Enter)) {
      choose(m_list->currentItem());
      return true;
    }
    if (watched == m_search && key->key() == Qt::Key_Down) {
      m_list->setFocus(Qt::OtherFocusReason);
      if (m_list->currentItem() == nullptr || m_list->currentItem()->isHidden()) {
        for (int row = 0; row < m_list->count(); ++row) {
          QListWidgetItem* item = m_list->item(row);
          if (!item->isHidden() && !item->data(kIdRole).toString().isEmpty()) {
            m_list->setCurrentItem(item);
            break;
          }
        }
      }
      return true;
    }
    if (watched == m_list && (key->key() == Qt::Key_Return || key->key() == Qt::Key_Enter)) {
      choose(m_list->currentItem());
      return true;
    }
  }
  return QFrame::eventFilter(watched, event);
}

} // namespace mitcad
