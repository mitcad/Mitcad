// SPDX-License-Identifier: MIT
#include "build/OpeningsWindow.hpp"

#include <algorithm>
#include <cmath>
#include <utility>

#include <QApplication>
#include <QComboBox>
#include <QDialogButtonBox>
#include <QEvent>
#include <QGridLayout>
#include <QHBoxLayout>
#include <QHideEvent>
#include <QLabel>
#include <QLayoutItem>
#include <QLineEdit>
#include <QMenu>
#include <QPushButton>
#include <QScrollArea>
#include <QSignalBlocker>
#include <QVBoxLayout>
#include <QtLogging>

#include "framework/TestSync.hpp"
#include "framework/Theme.hpp"

#include "../platform/MacDialogs.hpp"

namespace mitcad {
namespace {

using build::OpeningRow;

// The row a field belongs to (from 1), kept on the field.
const char* const kRowProperty = "mitcadOpeningRow";

QString number(double value) { return QString::number(value, 'g', 10); }

QLineEdit* numberField(const QString& name, int row, double value, const QString& tip) {
  auto* edit = new QLineEdit(number(value));
  edit->setObjectName(QStringLiteral("openings_%1_%2").arg(name).arg(row));
  edit->setProperty(kRowProperty, row);
  edit->setToolTip(tip);
  edit->setMinimumWidth(64);
  return edit;
}

QPushButton* button(const QString& text, const QString& name, const QString& tip) {
  auto* push = new QPushButton(text);
  push->setObjectName(name);
  push->setToolTip(tip);
  // Auxiliary row actions never steal Return from Close.
  push->setAutoDefault(false);
  push->setDefault(false);
  return push;
}

} // namespace

OpeningsWindow* OpeningsWindow::of(QWidget* window) {
  QWidget* top = window != nullptr ? window->window() : nullptr;
  if (top != nullptr) {
    if (auto* found = top->findChild<OpeningsWindow*>(QString(), Qt::FindDirectChildrenOnly)) {
      return found;
    }
  }
  return new OpeningsWindow(top);
}

OpeningsWindow::OpeningsWindow(QWidget* parent) : QDialog(parent) {
  setObjectName(QStringLiteral("openingsWindow"));
  setWindowTitle(tr("Openings"));
  setModal(false);
  resize(640, 320);
  auto* layout = new QVBoxLayout(this);
  auto* top = new QHBoxLayout;
  m_summary = new QLabel;
  top->addWidget(m_summary, 1);
  m_bearingLabel = new QLabel(tr("Wall:"));
  m_bearing = new QComboBox;
  m_bearing->setObjectName(QStringLiteral("openingsBearing"));
  m_bearing->addItem(tr("Non-load-bearing (partition)"), false);
  m_bearing->addItem(tr("Load-bearing"), true);
  m_bearing->setToolTip(tr("Partition: a flat header and no trimmers; load-bearing: trimmer studs and a header on edge"));
  top->addWidget(m_bearingLabel);
  top->addWidget(m_bearing);
  layout->addLayout(top);
  auto* scroll = new QScrollArea;
  scroll->setWidgetResizable(true);
  m_grid = new QWidget;
  m_layout = new QGridLayout(m_grid);
  m_layout->setAlignment(Qt::AlignTop);
  scroll->setWidget(m_grid);
  layout->addWidget(scroll, 1);
  auto* buttons = new QDialogButtonBox;
  m_addWindow = button(tr("Add &Window"), QStringLiteral("openingsAddWindow"), tr("Adds a window after the last opening"));
  m_addDoor = button(tr("Add &Door"), QStringLiteral("openingsAddDoor"), tr("Adds a door after the last opening"));
  m_addOpening =
      button(tr("Add &Opening"), QStringLiteral("openingsAddOpening"), tr("Adds an opening to the floor"));
  m_remove = button(tr("&Remove"), QStringLiteral("openingsRemove"), tr("Removes the opening selected"));
  m_close = button(tr("Close"), QStringLiteral("openingsClose"), tr("Closes the window; the openings stay"));
  buttons->addButton(m_addWindow, QDialogButtonBox::ResetRole);
  buttons->addButton(m_addDoor, QDialogButtonBox::ResetRole);
  buttons->addButton(m_addOpening, QDialogButtonBox::ResetRole);
  buttons->addButton(m_remove, QDialogButtonBox::ResetRole);
  buttons->addButton(m_close, QDialogButtonBox::RejectRole);
  mac::styleListButtons(m_addWindow, m_remove);
#ifdef MITCAD_MAC_UI
  auto* addMenu = new QMenu(m_addWindow);
  addMenu->addAction(tr("Window"), this, [this] { addRow(OpeningRow::Kind::Window); });
  addMenu->addAction(tr("Door"), this, [this] { addRow(OpeningRow::Kind::Door); });
  m_addWindow->setMenu(addMenu);
  m_addWindow->setAccessibleName(tr("Add window or door"));
  m_addWindow->setToolTip(tr("Add a window or door"));
#endif
  mac::styleIconToggle(m_addOpening, QStringLiteral("plus"));
  mac::styleDialogButtons(buttons, m_close);
  layout->addWidget(buttons);
#ifndef MITCAD_MAC_UI
  connect(m_addWindow, &QPushButton::clicked, this, [this] { addRow(OpeningRow::Kind::Window); });
#endif
  connect(m_addDoor, &QPushButton::clicked, this, [this] { addRow(OpeningRow::Kind::Door); });
  connect(m_addOpening, &QPushButton::clicked, this, [this] { addRow(OpeningRow::Kind::Floor); });
  connect(m_remove, &QPushButton::clicked, this, [this] { removeRow(); });
  connect(buttons, &QDialogButtonBox::rejected, this, &QDialog::reject);
  connect(m_bearing, qOverload<int>(&QComboBox::currentIndexChanged), this, [this](int) {
    const bool bearing = m_bearing->currentData().toBool();
    m_target.loadBearing = bearing;
    qInfo().noquote() << QStringLiteral("Openings bearing: %1")
                             .arg(bearing ? QStringLiteral("load-bearing") : QStringLiteral("non-load-bearing"));
    check();
    emit bearingChanged(m_target.command, bearing);
  });
  m_placesTimer.setSingleShot(true);
  m_placesTimer.setInterval(150);
  connect(&m_placesTimer, &QTimer::timeout, this, [this] { logPlaces(); });
  TestSync::watch(&m_placesTimer);
  mac::styleDialog(this);
}

OpeningsWindow::~OpeningsWindow() { m_placesTimer.stop(); }

void OpeningsWindow::open(const Target& target) {
  m_target = target;
  QString problem;
  m_rows = build::openingRows(target.text, target.floor, &problem);
  if (!problem.isEmpty()) {
    qInfo().noquote() << QStringLiteral("Openings text not read: %1").arg(problem);
  }
  m_written = target.text;
  m_selected = 0;
  m_badFields.clear();
  {
    const QSignalBlocker blocker(m_bearing);
    m_bearing->setCurrentIndex(target.loadBearing ? 1 : 0);
  }
  m_bearing->setVisible(!target.floor);
  m_bearingLabel->setVisible(!target.floor);
  m_addWindow->setVisible(!target.floor);
#ifdef MITCAD_MAC_UI
  m_addDoor->hide(); // the adjacent + menu contains both wall opening kinds
#else
  m_addDoor->setVisible(!target.floor);
#endif
  m_addOpening->setVisible(target.floor);
  rebuild();
  check();
  if (!m_placed && parentWidget() != nullptr) {
    // Beside the panel the first time, over the view's left side.
    move(parentWidget()->window()->mapToGlobal(QPoint(380, 140)));
    m_placed = true;
  }
  qInfo().noquote() << QStringLiteral("Openings window opened for %1: %2 row(s)%3")
                           .arg(target.command)
                           .arg(m_rows.size())
                           .arg(target.floor ? QStringLiteral(", a floor's")
                                             : target.loadBearing ? QStringLiteral(", load-bearing")
                                                                  : QStringLiteral(", non-load-bearing"));
  show();
  raise();
  activateWindow();
  logPlacesLater();
}

void OpeningsWindow::follow(const Target& target) {
  if (!isVisible() || target.command != m_target.command) {
    return;
  }
  if (target.floor != m_target.floor) {
    open(target); // Floor / Roof's other kind
    return;
  }
  // The sizes as known (NaN when not): changed when either differs.
  const auto same = [](double a, double b) { return a == b || (std::isnan(a) && std::isnan(b)); };
  const build::WallSize& now = target.wall;
  const build::WallSize& was = m_target.wall;
  bool changed = !same(now.length, was.length) || !same(now.height, was.height) || now.topPlates != was.topPlates ||
                 !same(now.studThick, was.studThick) || !same(now.studDepth, was.studDepth) ||
                 !same(target.floorLength, m_target.floorLength) || !same(target.floorWidth, m_target.floorWidth);
  m_target.wall = target.wall;
  m_target.floorLength = target.floorLength;
  m_target.floorWidth = target.floorWidth;
  if (target.loadBearing != m_target.loadBearing) {
    m_target.loadBearing = target.loadBearing;
    const QSignalBlocker blocker(m_bearing);
    m_bearing->setCurrentIndex(target.loadBearing ? 1 : 0);
    changed = true;
  }
  if (target.text != m_written) {
    changed = true;
    m_rows = build::openingRows(target.text, target.floor);
    m_written = target.text;
    m_selected = std::min(m_selected, static_cast<int>(m_rows.size()));
    m_badFields.clear();
    rebuild();
    qInfo().noquote() << QStringLiteral("Openings rows followed: %1").arg(target.text);
  }
  if (changed) {
    check();
  }
}

void OpeningsWindow::commandEnded(const QString& command) {
  if (command == m_target.command && isVisible()) {
    qInfo().noquote() << QStringLiteral("Openings window closed with %1").arg(command);
    hide();
  }
}

void OpeningsWindow::rebuild() {
  // The rows made before go (later: a field's signal may still be running).
  while (QLayoutItem* item = m_layout->takeAt(0)) {
    if (QWidget* widget = item->widget()) {
      widget->hide();
      widget->deleteLater();
    }
    delete item;
  }
  m_widgets.clear();
  const bool floor = m_target.floor;
  const QStringList headings = floor ? QStringList{QString(), tr("Offset Along"), tr("Offset Across"), tr("Width"),
                                                   tr("Length"), QString()}
                                     : QStringList{QString(), tr("Kind"), tr("From"), tr("Offset"), tr("Width"),
                                                   tr("Height"), tr("Sill"), QString()};
  for (int column = 0; column < headings.size(); ++column) {
    auto* heading = new QLabel(QStringLiteral("<b>%1</b>").arg(headings[column].toHtmlEscaped()));
    m_layout->addWidget(heading, 0, column);
  }
  for (std::size_t i = 0; i < m_rows.size(); ++i) {
    const OpeningRow& row = m_rows[i];
    const int r = static_cast<int>(i) + 1;
    RowWidgets widgets;
    widgets.number = new QLabel(QString::number(r));
    int column = 0;
    m_layout->addWidget(widgets.number, r, column++);
    const auto edited = [this] {
      readRows();
      check();
      write();
    };
    if (floor) {
      widgets.offset = numberField(QStringLiteral("offset"), r, row.offset,
                                   tr("Across the joists, from the outline's first corner, mm"));
      widgets.across = numberField(QStringLiteral("across"), r, row.across,
                                   tr("Along the joists, from the outline's first corner, mm"));
      widgets.width = numberField(QStringLiteral("width"), r, row.width, tr("The opening's size across the joists, mm"));
      widgets.height = numberField(QStringLiteral("height"), r, row.height, tr("The opening's size along the joists, mm"));
      for (QLineEdit* edit : {widgets.offset, widgets.across, widgets.width, widgets.height}) {
        m_layout->addWidget(edit, r, column++);
      }
    } else {
      widgets.kind = new QComboBox;
      widgets.kind->setObjectName(QStringLiteral("openings_kind_%1").arg(r));
      widgets.kind->addItem(tr("Window"));
      widgets.kind->addItem(tr("Door"));
      widgets.kind->setCurrentIndex(row.kind == OpeningRow::Kind::Door ? 1 : 0);
      widgets.kind->setToolTip(tr("A window, with its sill, or a door, which cuts the bottom plate"));
      widgets.from = new QComboBox;
      widgets.from->setObjectName(QStringLiteral("openings_from_%1").arg(r));
      widgets.from->addItem(tr("End A"));
      widgets.from->addItem(tr("End B"));
      widgets.from->setCurrentIndex(row.fromEnd ? 1 : 0);
      widgets.from->setToolTip(tr("End A: the offset from the wall's start to the opening; End B: from its end"));
      widgets.offset = numberField(QStringLiteral("offset"), r, row.offset,
                                   tr("From the end named to the opening's near side, mm"));
      widgets.width = numberField(QStringLiteral("width"), r, row.width, tr("The rough opening's width, mm"));
      widgets.height = numberField(QStringLiteral("height"), r, row.height, tr("The rough opening's height, mm"));
      widgets.sill = numberField(QStringLiteral("sill"), r, row.sill, tr("A window's sill above the wall's base, mm"));
      const bool door = row.kind == OpeningRow::Kind::Door;
      widgets.sill->setEnabled(!door);
      if (door) {
        widgets.sill->clear();
      }
      for (QComboBox* combo : {widgets.kind, widgets.from}) {
        combo->setProperty(kRowProperty, r);
        combo->installEventFilter(this);
        m_layout->addWidget(combo, r, column++);
      }
      for (QLineEdit* edit : {widgets.offset, widgets.width, widgets.height, widgets.sill}) {
        m_layout->addWidget(edit, r, column++);
      }
      QComboBox* kind = widgets.kind;
      QLineEdit* sill = widgets.sill;
      connect(kind, qOverload<int>(&QComboBox::currentIndexChanged), this, [kind, sill, edited](int) {
        // A door has no sill; a window gets one at 900 again.
        const bool door = kind->currentIndex() == 1;
        sill->setEnabled(!door);
        if (door) {
          sill->clear();
        } else if (!(build::openingNumber(sill->text()).value_or(0.0) > 0.0)) {
          sill->setText(QStringLiteral("900"));
        }
        edited();
      });
      connect(widgets.from, qOverload<int>(&QComboBox::currentIndexChanged), this, [edited](int) { edited(); });
    }
    for (QLineEdit* edit : {widgets.offset, widgets.across, widgets.width, widgets.height, widgets.sill}) {
      if (edit != nullptr) {
        edit->installEventFilter(this);
        connect(edit, &QLineEdit::editingFinished, this, edited);
      }
    }
    widgets.problem = new QLabel;
    setErrorStyleSheet(widgets.problem);
    m_layout->addWidget(widgets.problem, r, column);
    m_widgets.push_back(widgets);
  }
  m_layout->setColumnStretch(floor ? 5 : 7, 1);
  m_remove->setEnabled(!m_rows.empty());
  select(m_selected);
  logPlacesLater();
}

void OpeningsWindow::readRows() {
  m_badFields.clear();
  for (std::size_t i = 0; i < m_rows.size() && i < m_widgets.size(); ++i) {
    OpeningRow& row = m_rows[i];
    const RowWidgets& widgets = m_widgets[i];
    const int r = static_cast<int>(i) + 1;
    const auto read = [this, r](QLineEdit* edit, const QString& name, double& value) {
      if (edit == nullptr || !edit->isEnabled()) {
        return;
      }
      if (const std::optional<double> typed = build::openingNumber(edit->text())) {
        value = *typed;
      } else {
        m_badFields << QStringLiteral("%1.%2").arg(name).arg(r);
      }
    };
    if (!m_target.floor) {
      row.kind = widgets.kind->currentIndex() == 1 ? OpeningRow::Kind::Door : OpeningRow::Kind::Window;
      row.fromEnd = widgets.from->currentIndex() == 1;
    }
    read(widgets.offset, QStringLiteral("offset"), row.offset);
    read(widgets.across, QStringLiteral("across"), row.across);
    read(widgets.width, QStringLiteral("width"), row.width);
    read(widgets.height, QStringLiteral("height"), row.height);
    read(widgets.sill, QStringLiteral("sill"), row.sill);
    if (row.kind == OpeningRow::Kind::Door) {
      row.sill = 0.0;
    }
  }
}

void OpeningsWindow::check() {
  build::WallSize wall = m_target.wall;
  wall.loadBearing = m_target.loadBearing;
  QStringList problems = m_target.floor
                             ? build::floorOpeningProblems(m_rows, m_target.floorLength, m_target.floorWidth)
                             : build::openingProblems(m_rows, wall);
  for (const QString& field : std::as_const(m_badFields)) {
    const int r = field.section(QLatin1Char('.'), 1).toInt();
    if (r >= 1 && r <= problems.size()) {
      problems[r - 1] = tr("%1: not a number").arg(field.section(QLatin1Char('.'), 0, 0));
    }
  }
  bool any = false;
  for (int i = 0; i < problems.size(); ++i) {
    if (static_cast<std::size_t>(i) < m_widgets.size()) {
      m_widgets[static_cast<std::size_t>(i)].problem->setText(problems[i]);
    }
    if (!problems[i].isEmpty()) {
      any = true;
      qInfo().noquote() << QStringLiteral("Openings row %1: %2").arg(i + 1).arg(problems[i]);
    }
  }
  if (!any) {
    qInfo().noquote() << QStringLiteral("Openings rows ok");
  }
  m_summary->setText(build::openingsSummary(m_rows));
}

void OpeningsWindow::write() {
  const QString text = build::openingsText(m_rows, m_target.wall.length);
  if (text == m_written) {
    return;
  }
  m_written = text;
  qInfo().noquote() << QStringLiteral("Openings rows: %1").arg(text.isEmpty() ? QStringLiteral("none") : text);
  emit textChanged(m_target.command, text);
}

void OpeningsWindow::select(int row) {
  row = std::clamp(row, 0, static_cast<int>(m_rows.size()));
  for (std::size_t i = 0; i < m_widgets.size(); ++i) {
    const int r = static_cast<int>(i) + 1;
    m_widgets[i].number->setText(r == row ? QStringLiteral("<b>&gt; %1</b>").arg(r) : QString::number(r));
  }
  if (row == m_selected) {
    return;
  }
  m_selected = row;
  qInfo().noquote() << QStringLiteral("Openings selected row %1").arg(row);
  emit selectedChanged(m_target.command, row);
}

void OpeningsWindow::addRow(OpeningRow::Kind kind) {
  readRows();
  OpeningRow row;
  row.kind = kind;
  if (kind == OpeningRow::Kind::Floor) {
    row.offset = 600.0;
    row.across = 600.0;
    row.width = 900.0;
    row.height = 2400.0;
  } else {
    // 600 mm after the last opening (from the wall's start).
    double after = 0.0;
    for (const OpeningRow& other : m_rows) {
      after = std::max(after, build::offsetFromStart(other, m_target.wall.length) + other.width);
    }
    row.offset = after + 600.0;
    row.width = 900.0;
    row.height = kind == OpeningRow::Kind::Door ? 2100.0 : 1200.0;
    row.sill = kind == OpeningRow::Kind::Door ? 0.0 : 900.0;
  }
  m_rows.push_back(row);
  qInfo().noquote() << QStringLiteral("Openings added %1 as row %2")
                           .arg(kind == OpeningRow::Kind::Door     ? QStringLiteral("a door")
                                : kind == OpeningRow::Kind::Window ? QStringLiteral("a window")
                                                                   : QStringLiteral("an opening"))
                           .arg(m_rows.size());
  rebuild();
  select(static_cast<int>(m_rows.size()));
  check();
  write();
}

void OpeningsWindow::removeRow() {
  if (m_rows.empty()) {
    return;
  }
  readRows();
  const int row = m_selected >= 1 ? m_selected : static_cast<int>(m_rows.size());
  m_rows.erase(m_rows.begin() + (row - 1));
  qInfo().noquote() << QStringLiteral("Openings removed row %1").arg(row);
  m_selected = 0;
  rebuild();
  select(std::min(row, static_cast<int>(m_rows.size())));
  check();
  write();
}

bool OpeningsWindow::eventFilter(QObject* watched, QEvent* event) {
  if (event->type() == QEvent::FocusIn || event->type() == QEvent::MouseButtonPress) {
    const QVariant row = watched->property(kRowProperty);
    if (row.isValid()) {
      select(row.toInt());
    }
  }
  return QDialog::eventFilter(watched, event);
}

void OpeningsWindow::moveEvent(QMoveEvent* event) {
  QDialog::moveEvent(event);
  logPlacesLater();
}

void OpeningsWindow::resizeEvent(QResizeEvent* event) {
  QDialog::resizeEvent(event);
  logPlacesLater();
}

void OpeningsWindow::showEvent(QShowEvent* event) {
  QDialog::showEvent(event);
  logPlacesLater();
}

void OpeningsWindow::hideEvent(QHideEvent* event) {
  QDialog::hideEvent(event);
  // The window took the keyboard when it opened (open: activateWindow), and
  // the main window's Enter (OK) and Escape are window shortcuts: they work
  // only while it is the active window. The system does not always give it
  // the activation back (Windows' session 0 did not, mitcad#532), so it is
  // asked for here, unless another window of the application has taken it
  // (by the time the hide event comes the system may already have moved it
  // away from this window, to none). Not when the window is minimized with
  // the others, or the main window closes.
  if (event->spontaneous()) {
    return;
  }
  QWidget* main = parentWidget() != nullptr ? parentWidget()->window() : nullptr;
  if (main == nullptr || !main->isVisible() || main->isMinimized()) {
    return;
  }
  const QWidget* active = QApplication::activeWindow();
  if (active != nullptr && active != this && active != main) {
    return;
  }
  main->activateWindow();
  qInfo().noquote() << QStringLiteral("Openings window hidden: the main window active again");
}

void OpeningsWindow::logPlacesLater() {
  if (isVisible()) {
    m_placesTimer.start();
  }
}

void OpeningsWindow::logPlaces() const {
  if (!isVisible()) {
    return;
  }
  // As the part picker's places: in the main window, and in this window.
  const QWidget* host = parentWidget() != nullptr ? parentWidget()->window() : nullptr;
  const auto log = [this, host](const QString& name, const QWidget* widget) {
    if (widget == nullptr || !widget->isVisible()) {
      return;
    }
    const QPoint global = widget->mapToGlobal(widget->rect().center());
    const QPoint inHost = host != nullptr ? host->mapFromGlobal(global) : global;
    const QPoint own = mapFromGlobal(global);
    qDebug().noquote() << QStringLiteral("Openings place %1 at %2,%3; in the window %4,%5")
                              .arg(name)
                              .arg(inHost.x())
                              .arg(inHost.y())
                              .arg(own.x())
                              .arg(own.y());
  };
  log(QStringLiteral("add_window"), m_addWindow);
  log(QStringLiteral("add_door"), m_addDoor);
  log(QStringLiteral("add_opening"), m_addOpening);
  log(QStringLiteral("remove"), m_remove);
  log(QStringLiteral("close"), m_close);
  log(QStringLiteral("bearing"), m_bearing);
  for (std::size_t i = 0; i < m_widgets.size(); ++i) {
    const RowWidgets& row = m_widgets[i];
    const QString n = QString::number(static_cast<int>(i) + 1);
    log(QStringLiteral("kind.") + n, row.kind);
    log(QStringLiteral("from.") + n, row.from);
    log(QStringLiteral("offset.") + n, row.offset);
    log(QStringLiteral("across.") + n, row.across);
    log(QStringLiteral("width.") + n, row.width);
    log(QStringLiteral("height.") + n, row.height);
    log(QStringLiteral("sill.") + n, row.sill);
  }
}

} // namespace mitcad
