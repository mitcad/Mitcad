// SPDX-License-Identifier: MIT
#include "MeasureHistoryView.hpp"

#include <algorithm>
#include <utility>

#include <QAction>
#include <QClipboard>
#include <QCoreApplication>
#include <QEvent>
#include <QFont>
#include <QGuiApplication>
#include <QHBoxLayout>
#include <QListWidget>
#include <QListWidgetItem>
#include <QMenu>
#include <QPalette>
#include <QPointer>
#include <QPushButton>
#include <QRect>
#include <QScrollArea>
#include <QScrollBar>
#include <QToolButton>
#include <QVBoxLayout>
#include <QtLogging>

#include "../framework/Dialogs.hpp"
#include "../framework/GlassCard.hpp"
#include "../framework/TestSync.hpp"
#include "../platform/NativeDialogs.hpp"
#include "MeasureHistory.hpp"

namespace mitcad {
namespace {

// Rows shown before the list scrolls: the panel does not grow past the
// screen however many measurements there are.
constexpr int kShownRows = 4;

// The row's number is kept in the item.
constexpr int kNumberRole = Qt::UserRole + 1;

// A row's one line (mitcad#1010): its number, its main value and the
// picks' kinds ("3.  6.843 mm   Face ↔ Face").
QString rowText(const MeasureHistory::Row& row) {
  return row.main.isEmpty() ? QStringLiteral("%1.  %2").arg(row.number).arg(row.kinds.isEmpty() ? row.picked : row.kinds)
                            : QStringLiteral("%1.  %2   %3").arg(row.number).arg(row.main, row.kinds);
}

// All of it in the row's tooltip: the picks in full and every value.
QString rowTip(const MeasureHistory::Row& row) {
  QStringList lines{row.picked};
  lines << row.values;
  if (row.gone) {
    lines << QObject::tr("The geometry is not there any more.");
  }
  return lines.join(QLatin1Char('\n'));
}

// Tabs and line breaks of copied text, readable on one log line.
QString loggable(QString text) {
  return text.replace(QLatin1Char('\t'), QStringLiteral(" | ")).replace(QLatin1Char('\n'), QStringLiteral(" / "));
}

// A menu's entries as the log names them ("Context menu: Note… | Datum
// Label…"), their mnemonics' marks left out.
void logEntries(const QMenu& menu) {
  QStringList entries;
  for (const QAction* action : menu.actions()) {
    if (!action->isSeparator()) {
      entries << QString(action->text()).remove(QLatin1Char('&'));
    }
  }
  qDebug().noquote() << QStringLiteral("Context menu: %1").arg(entries.join(QStringLiteral(" | ")));
}

QString at(const QWidget* widget, const QPoint& point) {
  const QPoint host = GlassCard::mapToHost(widget, point);
  return QStringLiteral("%1,%2").arg(host.x()).arg(host.y());
}

} // namespace

MeasureHistoryView::MeasureHistoryView(const MeasureHistory& history, QWidget* parent)
    : QWidget(parent), m_history(history) {
  setObjectName(QStringLiteral("measureHistory"));
  auto* layout = new QVBoxLayout(this);
  layout->setContentsMargins(0, 0, 0, 0);
  layout->setSpacing(6);

  // The actions on one row: Keep as Dimension (mitcad#649), the
  // measurement shown kept in the design; Annotate ▾, its menu.
  auto* actions = new QHBoxLayout;
  actions->setSpacing(6);
  m_keep = new QPushButton(tr("Keep as Dimension"), this);
  m_keep->setObjectName(QStringLiteral("measureKeep"));
  m_keepTip = tr("Keep this measurement in the design as a dimension, drawn in the view");
  m_keep->setToolTip(m_keepTip);
  m_keep->setEnabled(false);
  m_annotate = new QPushButton(tr("Annotate ▾"), this);
  m_annotate->setObjectName(QStringLiteral("measureAnnotate"));
  m_annotate->setToolTip(tr("A note, a datum label, a frame or a hole callout on the pick, or the Annotations window"));
  for (QPushButton* button : {m_keep, m_annotate}) {
    button->setFocusPolicy(Qt::NoFocus);
    button->setAutoDefault(false);
    actions->addWidget(button);
  }
  actions->addStretch();
  layout->addLayout(actions);
  // Annotate's menu: a leader on the one pick shown (mitcad#702: Note…;
  // phase 2: Datum Label…, Frame…; mitcad#902: Hole Callout), then the
  // Annotations window. Their letters choose them from the keyboard.
  m_annotateMenu = new QMenu(this);
  m_annotateMenu->setToolTipsVisible(true);
  m_note = m_annotateMenu->addAction(tr("&Note…"));
  m_note->setToolTip(tr("A note with an arrow to the face, edge or vertex picked"));
  m_datum = m_annotateMenu->addAction(tr("&Datum Label…"));
  m_datum->setToolTip(tr("A datum label on the face, edge or datum picked"));
  m_frame = m_annotateMenu->addAction(tr("&Frame…"));
  m_frame->setToolTip(tr("A feature control frame with an arrow to the face or edge picked"));
  // Its text from the hole's feature, no dialog.
  m_callout = m_annotateMenu->addAction(tr("Hole &Callout"));
  m_callout->setToolTip(tr("A hole callout on the hole picked (its round face or edge), written from the hole"));
  for (QAction* action : {m_note, m_datum, m_frame, m_callout}) {
    action->setEnabled(false);
  }
  m_annotateMenu->addSeparator();
  m_annotations = m_annotateMenu->addAction(tr("&Annotations…"));
  m_annotations->setToolTip(tr("The dimensions, notes, datum labels and frames kept in the design"));

  // The history, folded: its header unfolds it, ⋯ has Copy All and Clear.
  auto* header = new QHBoxLayout;
  header->setSpacing(4);
  m_toggle = new QToolButton(this);
  m_toggle->setObjectName(QStringLiteral("measureHistoryToggle"));
  m_toggle->setToolButtonStyle(Qt::ToolButtonTextBesideIcon);
  m_toggle->setArrowType(Qt::RightArrow);
  m_toggle->setCheckable(true);
  m_toggle->setAutoRaise(true);
  QFont bold = m_toggle->font();
  bold.setBold(true);
  m_toggle->setFont(bold);
  m_more = new QToolButton(this);
  m_more->setObjectName(QStringLiteral("measureHistoryMenu"));
  m_more->setText(QStringLiteral("⋯"));
  m_more->setAutoRaise(true);
  m_more->setToolTip(tr("Copy All, Clear"));
  for (QToolButton* button : {m_toggle, m_more}) {
    button->setFocusPolicy(Qt::NoFocus);
  }
  header->addWidget(m_toggle);
  header->addStretch();
  header->addWidget(m_more);
  layout->addLayout(header);
  m_list = new QListWidget(this);
  m_list->setObjectName(QStringLiteral("measureHistoryRows"));
  m_list->setProperty(NativeListRowLimitProperty, kShownRows);
  m_list->setSelectionMode(QAbstractItemView::SingleSelection);
  // One line a row, cut at the panel's width; the tooltip has all of it.
  m_list->setWordWrap(false);
  m_list->setTextElideMode(Qt::ElideRight);
  m_list->setHorizontalScrollBarPolicy(Qt::ScrollBarAlwaysOff);
  m_list->setVerticalScrollMode(QAbstractItemView::ScrollPerPixel);
  m_list->setContextMenuPolicy(Qt::CustomContextMenu);
  // The keyboard stays with the panel's fields and the view.
  m_list->setFocusPolicy(Qt::NoFocus);
  m_list->setVisible(false);
  layout->addWidget(m_list);

  connect(m_keep, &QPushButton::clicked, this, [this] {
    qDebug().noquote() << QStringLiteral("Measure Keep as Dimension clicked");
    if (m_actions.keep) {
      m_actions.keep();
    }
  });
  connect(m_annotate, &QPushButton::clicked, this, [this] { annotateMenu(); });
  const auto chosen = [this](QAction* action, const char* name, std::function<void()> Actions::*call) {
    connect(action, &QAction::triggered, this, [this, name, call] {
      qDebug().noquote() << QStringLiteral("Measure %1 clicked").arg(QString::fromLatin1(name));
      if (m_actions.*call) {
        (m_actions.*call)();
      }
    });
  };
  chosen(m_note, "Note", &Actions::note);
  chosen(m_datum, "Datum Label", &Actions::datum);
  chosen(m_frame, "Frame", &Actions::frame);
  chosen(m_callout, "Hole Callout", &Actions::callout);
  chosen(m_annotations, "Annotations", &Actions::annotations);
  connect(m_toggle, &QToolButton::clicked, this, [this](bool open) {
    qDebug().noquote() << (open ? QStringLiteral("Measure history opened") : QStringLiteral("Measure history closed"));
    setOpen(open);
    if (open) {
      // Its rows in sight: a low window shows the panel's top part, and
      // the history unfolds under the result (mitcad#1010).
      TestSync::singleShot(0, this, [this] {
        QCoreApplication::sendPostedEvents(nullptr, QEvent::LayoutRequest);
        for (QWidget* widget = parentWidget(); widget != nullptr; widget = widget->parentWidget()) {
          if (auto* area = qobject_cast<QScrollArea*>(widget)) {
            area->ensureWidgetVisible(m_list->isVisible() ? static_cast<QWidget*>(m_list) : m_toggle, 0, 0);
            break;
          }
        }
      });
    }
  });
  connect(m_more, &QToolButton::clicked, this, [this] { historyMenu(); });
  connect(m_list, &QListWidget::itemClicked, this, [this](QListWidgetItem* item) {
    TestSync::singleShot(0, this, [this] { logPlaces(); });
    if (item != nullptr && m_actions.show) {
      m_actions.show(item->data(kNumberRole).toInt());
    }
  });
  connect(m_list, &QWidget::customContextMenuRequested, this, [this](const QPoint& point) {
    if (QListWidgetItem* item = m_list->itemAt(point)) {
      rowMenu(item, m_list->viewport()->mapToGlobal(point));
    }
  });
  // Its places again when the list scrolls.
  connect(m_list->verticalScrollBar(), &QScrollBar::valueChanged, this,
          [this] { TestSync::singleShot(0, this, [this] { logPlaces(); }); });
  refresh();
  logAnnotateEntries();
}

void MeasureHistoryView::refresh() {
  const int current = m_list->currentItem() != nullptr ? m_list->currentItem()->data(kNumberRole).toInt() : 0;
  m_list->clear();
  const QColor grey = palette().color(QPalette::Disabled, QPalette::Text);
  const auto& rows = m_history.rows();
  qDebug().noquote() << QStringLiteral("Measure history: %1 rows").arg(static_cast<int>(rows.size()));
  for (const MeasureHistory::Row& row : rows) {
    auto* item = new QListWidgetItem(rowText(row), m_list);
    item->setData(kNumberRole, row.number);
    item->setData(NativeControlIdentifierRole, QStringLiteral("measureHistory.row.%1").arg(row.number));
    item->setToolTip(rowTip(row));
    if (row.gone) {
      item->setForeground(grey);
    }
    if (row.number == current) {
      m_list->setCurrentItem(item);
    }
    qDebug().noquote() << QStringLiteral("Measure history row %1: %2: %3%4")
                              .arg(row.number)
                              .arg(row.picked, row.values.join(QStringLiteral(", ")),
                                   row.gone ? QStringLiteral(" (gone)") : QString());
  }
  m_toggle->setText(tr("History (%1)").arg(static_cast<int>(rows.size())));
  m_more->setEnabled(!rows.empty());
  fitHeight();
  m_list->setVisible(m_open && m_list->count() > 0);
  TestSync::singleShot(0, this, [this] { logPlaces(); });
}

void MeasureHistoryView::setOpen(bool open) {
  m_open = open;
  m_toggle->setChecked(open);
  m_toggle->setArrowType(open ? Qt::DownArrow : Qt::RightArrow);
  m_list->setVisible(open && m_list->count() > 0);
  TestSync::singleShot(0, this, [this] { logPlaces(); });
}

void MeasureHistoryView::setKeepable(bool keepable, const QString& why) {
  // A greyed-out Keep as Dimension says why (mitcad#869).
  const QString reason = keepable ? QString() : why;
  const QString tip = reason.isEmpty() ? m_keepTip : reason;
  if (m_keep->isEnabled() == keepable && m_keep->toolTip() == tip) {
    return;
  }
  m_keep->setEnabled(keepable);
  m_keep->setToolTip(tip);
  qDebug().noquote() << QStringLiteral("Measure Keep as Dimension %1%2")
                            .arg(keepable ? QStringLiteral("enabled") : QStringLiteral("disabled"),
                                 reason.isEmpty() ? QString() : QStringLiteral(": ") + reason);
  TestSync::singleShot(0, this, [this] { logPlaces(); });
}

void MeasureHistoryView::setNoteable(bool noteable, bool feature) {
  const bool featured = noteable && feature;
  m_note->setEnabled(noteable);
  m_datum->setEnabled(featured);
  m_frame->setEnabled(featured);
  m_callout->setEnabled(featured);
  logAnnotateEntries();
}

void MeasureHistoryView::logAnnotateEntries() {
  QStringList entries;
  for (const QAction* action : m_annotateMenu->actions()) {
    if (!action->isSeparator() && action->isEnabled()) {
      entries << QString(action->text()).remove(QLatin1Char('&'));
    }
  }
  const QString line = entries.join(QStringLiteral(", "));
  if (line == m_entries) {
    return;
  }
  m_entries = line;
  qDebug().noquote() << QStringLiteral("Measure Annotate entries: %1").arg(line);
}

void MeasureHistoryView::annotateMenu() {
  qDebug().noquote() << QStringLiteral("Measure Annotate clicked");
  logEntries(*m_annotateMenu);
  // Under the button; the entry chosen calls its action (connected above).
  execMenu(m_annotateMenu, m_annotate->mapToGlobal(QPoint(0, m_annotate->height())));
}

void MeasureHistoryView::historyMenu() {
  qDebug().noquote() << QStringLiteral("Measure history menu clicked");
  // No parent, and this guarded: the panel may close while the menu's own
  // event loop runs.
  const QPointer<MeasureHistoryView> self(this);
  QMenu menu;
  QAction* all = menu.addAction(tr("Copy All"));
  all->setToolTip(tr("Copy every row as tab-separated text"));
  QAction* clear = menu.addAction(tr("Clear"));
  clear->setToolTip(tr("Empty the history"));
  logEntries(menu);
  QAction* chosen = execMenu(&menu, m_more->mapToGlobal(QPoint(0, m_more->height())));
  if (self.isNull()) {
    return;
  }
  if (chosen == all) {
    copyAll();
  } else if (chosen == clear) {
    qDebug().noquote() << QStringLiteral("Measure history Clear clicked");
    if (m_actions.clear) {
      m_actions.clear();
    }
  }
}

void MeasureHistoryView::rowMenu(QListWidgetItem* item, const QPoint& global) {
  // The row by its number: the rows are made again while the menu is open
  // when a measurement finishes.
  const int number = item->data(kNumberRole).toInt();
  const MeasureHistory::Row* row = m_history.row(number);
  qDebug().noquote() << QStringLiteral("Measure history row %1 menu").arg(number);
  const QPointer<MeasureHistoryView> self(this);
  QMenu menu;
  QAction* show = menu.addAction(tr("Show"));
  QAction* keep = menu.addAction(tr("Keep as Dimension"));
  keep->setEnabled(row != nullptr && !row->gone);
  QAction* copy = menu.addAction(tr("Copy"));
  logEntries(menu);
  QAction* chosen = execMenu(&menu, global);
  if (self.isNull()) {
    return;
  }
  if (chosen == show) {
    for (int i = 0; i < m_list->count(); ++i) {
      if (m_list->item(i)->data(kNumberRole).toInt() == number) {
        m_list->setCurrentItem(m_list->item(i));
      }
    }
    if (m_actions.show) {
      m_actions.show(number);
    }
  } else if (chosen == keep) {
    qDebug().noquote() << QStringLiteral("Measure history Keep as Dimension clicked: row %1").arg(number);
    if (m_actions.keepRow) {
      m_actions.keepRow(number);
    }
  } else if (chosen == copy) {
    copyRow(number);
  }
}

void MeasureHistoryView::fitHeight() {
  // As high as its rows up to kShownRows of them, then it scrolls.
  const int line = m_list->fontMetrics().height() + 4;
  int listHeight = 2 * m_list->frameWidth();
  for (int i = 0; i < m_list->count() && i < kShownRows; ++i) {
    listHeight += std::max(m_list->sizeHintForRow(i), line);
  }
  if (m_list->count() == 0) {
    listHeight += line;
  }
  if (m_list->height() != listHeight) {
    m_list->setFixedHeight(listHeight);
  }
}

QStringList MeasureHistoryView::places() const {
  QStringList lines;
  if (!isVisible()) {
    return lines;
  }
  const auto disabled = [](const QWidget* widget) {
    return widget->isEnabled() ? QString() : QStringLiteral(" (disabled)");
  };
  lines << QStringLiteral("Measure Keep as Dimension at %1%2").arg(at(m_keep, m_keep->rect().center()), disabled(m_keep));
  lines << QStringLiteral("Measure Annotate at %1").arg(at(m_annotate, m_annotate->rect().center()));
  lines << QStringLiteral("Measure history at %1 (%2)")
               .arg(at(m_toggle, m_toggle->rect().center()), m_open ? QStringLiteral("open") : QStringLiteral("folded"));
  lines << QStringLiteral("Measure history menu at %1%2").arg(at(m_more, m_more->rect().center()), disabled(m_more));
  const QRect shown = m_list->viewport()->rect();
  for (int i = 0; i < m_list->count(); ++i) {
    const QListWidgetItem* item = m_list->item(i);
    const QRect rect = m_list->visualItemRect(item);
    const QString name = QStringLiteral("Measure history row %1").arg(item->data(kNumberRole).toInt());
    // In sight by its height: a long row is wider than the list (its end
    // elided), so its middle can lie right of the view while the row
    // shows. Clicked in the view's middle across.
    const QPoint middle(shown.center().x(), rect.center().y());
    if (m_list->isVisible() && shown.contains(middle) && rect.left() <= middle.x()) {
      lines << QStringLiteral("%1 at %2").arg(name, at(m_list->viewport(), middle));
    } else {
      lines << QStringLiteral("%1 out of sight").arg(name);
    }
  }
  // Each control's text whole at the panel's width (mitcad#1010).
  QStringList clipped;
  const std::pair<const char*, const QWidget*> controls[] = {
      {"Keep as Dimension", m_keep}, {"Annotate", m_annotate}, {"History", m_toggle}, {"History menu", m_more}};
  for (const auto& [name, control] : controls) {
    const int needed = control->sizeHint().width();
    if (control->width() < needed) {
      clipped << QStringLiteral("%1 (%2 of %3)").arg(QString::fromLatin1(name)).arg(needed).arg(control->width());
    }
  }
  lines << (clipped.isEmpty() ? QStringLiteral("Measure controls: none clipped")
                              : QStringLiteral("Measure controls clipped: %1").arg(clipped.join(QStringLiteral(", "))));
  return lines;
}

void MeasureHistoryView::logPlaces() {
  // The rows laid out now: the list lays them out late, when it paints,
  // and a window that is never painted (Windows session 0) had only the
  // current row in place.
  m_list->doItemsLayout();
  fitHeight();
  QCoreApplication::sendPostedEvents(nullptr, QEvent::LayoutRequest);
  const QStringList lines = places();
  if (lines.isEmpty() || lines == m_logged) {
    return;
  }
  m_logged = lines;
  for (const QString& line : lines) {
    qDebug().noquote() << line;
  }
}

void MeasureHistoryView::copyRow(int number) {
  const MeasureHistory::Row* row = m_history.row(number);
  if (row == nullptr) {
    return;
  }
  const QString text = MeasureHistory::tabSeparated(*row);
  if (QClipboard* clipboard = QGuiApplication::clipboard()) {
    clipboard->setText(text);
  }
  qDebug().noquote() << QStringLiteral("Measure history copied row %1: %2").arg(row->number).arg(loggable(text));
}

void MeasureHistoryView::copyAll() {
  const QString text = m_history.tabSeparated();
  if (QClipboard* clipboard = QGuiApplication::clipboard()) {
    clipboard->setText(text);
  }
  qDebug().noquote() << QStringLiteral("Measure history copied %1 rows: %2")
                            .arg(static_cast<int>(m_history.rows().size()))
                            .arg(loggable(text));
}

} // namespace mitcad
