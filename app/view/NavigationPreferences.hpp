// SPDX-License-Identifier: MIT
#pragma once

#include "Navigation.hpp"
#include "../platform/MacDialogs.hpp"

#include <algorithm>
#include <cmath>
#include <cstddef>
#include <functional>
#include <utility>

#include <QCheckBox>
#include <QComboBox>
#include <QFormLayout>
#include <QGridLayout>
#include <QHeaderView>
#include <QLabel>
#include <QPainter>
#include <QPainterPath>
#include <QPushButton>
#include <QShowEvent>
#include <QSignalBlocker>
#include <QStringList>
#include <QSlider>
#include <QTabWidget>
#include <QTableWidget>
#include <QTimer>
#include <QVBoxLayout>
#include <QWidget>

namespace mitcad {

inline QString navigationActionText(NavigationAction action) {
  switch (action) {
  case NavigationAction::Orbit: return QObject::tr("Orbit");
  case NavigationAction::Pan: return QObject::tr("Pan");
  case NavigationAction::Zoom: return QObject::tr("Zoom");
  case NavigationAction::None: return QObject::tr("None");
  }
  return {};
}

inline QString navigationInputText(const NavigationBinding& binding) {
  QStringList keys;
  if (binding.f4) keys << QStringLiteral("F4");
  if (binding.modifiers.testFlag(Qt::ShiftModifier)) keys << QStringLiteral("Shift");
#ifdef Q_OS_MACOS
  if (binding.modifiers.testFlag(Qt::ControlModifier)) keys << QStringLiteral("Command");
  if (binding.modifiers.testFlag(Qt::AltModifier)) keys << QStringLiteral("Option");
#else
  if (binding.modifiers.testFlag(Qt::ControlModifier)) keys << QStringLiteral("Ctrl");
  if (binding.modifiers.testFlag(Qt::AltModifier)) keys << QStringLiteral("Alt");
#endif
  switch (binding.input) {
  case NavigationInput::LeftButton: keys << QObject::tr("Left drag"); break;
  case NavigationInput::MiddleButton: keys << QObject::tr("Middle drag"); break;
  case NavigationInput::RightButton: keys << QObject::tr("Right drag"); break;
  case NavigationInput::TrackpadScroll: keys << QObject::tr("Two-finger scroll"); break;
  }
  return keys.join(QStringLiteral(" + "));
}

// Pure artwork shared by the Qt guide and its native AppKit image view.
// The caller supplies font, palette and logical size; no QWidget is drawn.
inline void paintNavigationGuide(QPainter& p, const QSizeF& size, const NavigationSettings& settings,
                                 const QPalette& colors, const QFont& font) {
  p.save();
  p.setFont(font);
  p.setRenderHint(QPainter::Antialiasing);
  p.setPen(QPen(colors.color(QPalette::Text), 1.5));
  const double half = size.width() / 2.0;
  p.drawRoundedRect(QRectF(8, 9, 55, 76), 23, 23);
  p.drawLine(QPointF(35, 10), QPointF(35, 41));
  p.drawLine(QPointF(9, 42), QPointF(62, 42));
  p.drawRoundedRect(QRectF(31, 19, 8, 18), 3, 3);
  const auto bindings = settings.scheme == NavigationScheme::Custom
                            ? settings.customBindings : navigationBindings(settings.scheme);
  QStringList mouse, scroll;
  // A primary mapping for each physical input fits the drawing at every
  // scale. The scrollable table below always lists every modifier binding.
  for (const auto input : {NavigationInput::LeftButton, NavigationInput::MiddleButton,
                           NavigationInput::RightButton, NavigationInput::TrackpadScroll}) {
    const NavigationBinding* primary = nullptr;
    for (const auto& binding : bindings) {
      if (binding.input != input || binding.action == NavigationAction::None || !navigationBindingAllowed(binding)) continue;
      if (primary == nullptr || (binding.modifiers == Qt::NoModifier && !binding.f4)) primary = &binding;
      if (binding.modifiers == Qt::NoModifier && !binding.f4) break;
    }
    if (primary != nullptr) {
      const QString text = navigationInputText(*primary) + QStringLiteral(": ") + navigationActionText(primary->action);
      (input == NavigationInput::TrackpadScroll ? scroll : mouse) << text;
    }
  }
  scroll << QObject::tr("Pinch: Zoom") << QObject::tr("Modifiers: see bindings below");
  QFont smaller = p.font();
  smaller.setPointSizeF(std::max(8.0, smaller.pointSizeF() - 1.0));
  p.setFont(smaller);
  const auto drawBindings = [&p](double x, double width, const QStringList& labels) {
    const int available = std::max(1, int(width));
    for (int row = 0; row < labels.size(); ++row) {
      p.drawText(QRectF(x, 8 + row * 24, available, 24), Qt::AlignLeft | Qt::AlignVCenter,
                 p.fontMetrics().elidedText(labels[row], Qt::ElideMiddle, available));
    }
  };
  drawBindings(74, half - 78, mouse.isEmpty() ? QStringList{QObject::tr("Mouse wheel: Zoom")} : mouse);
  p.drawRoundedRect(QRectF(half + 5, 15, 62, 58), 7, 7);
  p.drawLine(QPointF(half + 29, 30), QPointF(half + 29, 53));
  p.drawLine(QPointF(half + 43, 30), QPointF(half + 43, 53));
  drawBindings(half + 78, half - 82, scroll);
  // Orbit: an arc around the model; Pan: parallel arrows; Zoom: toward it.
  const double step = size.width() / 3.0;
  const QColor accent = colors.color(QPalette::Highlight);
  for (int i = 0; i != 3; ++i) {
    const double x = i * step + 18;
    p.setPen(QPen(accent, 1.8));
    p.drawRect(QRectF(x + 11, 108, 17, 17));
    if (i == 0) {
      p.drawArc(QRectF(x, 98, 39, 36), 20 * 16, 285 * 16);
      p.drawLine(QPointF(x + 36, 105), QPointF(x + 37, 99));
      p.drawLine(QPointF(x + 37, 99), QPointF(x + 31, 101));
      if (settings.constrainedOrbit) {
        p.drawLine(QPointF(x + 20, 119), QPointF(x + 20, 98));
        p.drawLine(QPointF(x + 20, 98), QPointF(x + 17, 103));
        p.drawLine(QPointF(x + 20, 98), QPointF(x + 23, 103));
        p.drawText(QPointF(x + 25, 104), QStringLiteral("Z"));
      }
    } else if (i == 1) {
      p.drawLine(QPointF(x, 102), QPointF(x + 38, 102));
      p.drawLine(QPointF(x + 38, 102), QPointF(x + 33, 98));
      p.drawLine(QPointF(x + 38, 102), QPointF(x + 33, 106));
    } else {
      p.drawLine(QPointF(x, 101), QPointF(x + 10, 109));
      p.drawLine(QPointF(x + 10, 109), QPointF(x + 4, 108));
      p.drawLine(QPointF(x + 10, 109), QPointF(x + 8, 103));
    }
    p.setPen(colors.color(QPalette::Text));
    const QString text = i == 0 ? (settings.constrainedOrbit ? QObject::tr("Orbit: turn with Z upright")
                                                                  : QObject::tr("Orbit: turn around the design"))
                         : i == 1 ? QObject::tr("Pan: slide the view")
                                  : QObject::tr("Zoom: closer or farther");
    p.drawText(QRectF(x + 47, 96, step - 66, 42), Qt::AlignLeft | Qt::AlignVCenter | Qt::TextWordWrap, text);
  }
  p.restore();
}

// The Qt presenter paints the shared vector artwork at its device scale.
class NavigationGuide : public QWidget {
public:
  explicit NavigationGuide(QWidget* parent = nullptr) : QWidget(parent) {
    setObjectName(QStringLiteral("navigationVisualGuide"));
    setMinimumHeight(142);
    setSizePolicy(QSizePolicy::Expanding, QSizePolicy::Fixed);
  }
  QSize sizeHint() const override { return {560, 142}; }
  const NavigationSettings& navigation() const { return m_settings; }
  quint64 artworkRevision() const { return m_artworkRevision; }
  void setNavigation(const NavigationSettings& settings) {
    m_settings = settings;
    ++m_artworkRevision;
    QStringList labels;
    const auto bindings = settings.scheme == NavigationScheme::Custom
                              ? settings.customBindings : navigationBindings(settings.scheme);
    for (const auto& binding : bindings) {
      if (binding.action != NavigationAction::None && navigationBindingAllowed(binding)) {
        labels << navigationInputText(binding) + QStringLiteral(": ") + navigationActionText(binding.action);
      }
    }
    setAccessibleName(QObject::tr("Mouse and trackpad navigation"));
    setAccessibleDescription(labels.join(QStringLiteral("; ")));
    update();
  }
protected:
  void paintEvent(QPaintEvent*) override {
    QPainter p(this);
    paintNavigationGuide(p, size(), m_settings, palette(), font());
  }
private:
  NavigationSettings m_settings;
  quint64 m_artworkRevision = 0;
};

class NavigationPreferences : public QWidget {
public:
  // `buttonCommands`: the commands a 3D mouse's button can run, by id and
  // name (Fit and Home when empty).
  NavigationPreferences(const NavigationSettings& settings,
                        const QVector<QPair<NavigationScheme, QString>>& schemes, QWidget* parent = nullptr,
                        const QVector<QPair<QString, QString>>& buttonCommands = {})
      : QWidget(parent), m_settings(settings), m_buttonCommands(buttonCommands) {
    if (m_buttonCommands.isEmpty()) {
      m_buttonCommands = {{QStringLiteral("view.fit"), tr("Fit")}, {QStringLiteral("view.home"), tr("Home")}};
    }
    setObjectName(QStringLiteral("navigationPreferences"));
    auto* outer = new QVBoxLayout(this);
    outer->setContentsMargins(0, 0, 0, 0);
    // The mouse and the trackpad, and the 3D mouse (mitcad#951), on tabs
    // of their own, so that the page keeps a height a low screen shows.
    auto* tabs = new QTabWidget;
    tabs->setObjectName(QStringLiteral("navigationTabs"));
    tabs->setDocumentMode(true);
    outer->addWidget(tabs);
    auto* mousePage = new QWidget;
    auto* layout = new QVBoxLayout(mousePage);
    layout->setContentsMargins(0, 6, 0, 0);
    auto* top = new QFormLayout;
    m_scheme = new QComboBox;
    m_scheme->setObjectName(QStringLiteral("navigationScheme"));
    m_seed = new QComboBox;
    m_seed->setObjectName(QStringLiteral("navigationStartFrom"));
    for (const auto& entry : schemes) {
      m_scheme->addItem(entry.second, int(entry.first));
      if (entry.first != NavigationScheme::Custom) m_seed->addItem(entry.second, int(entry.first));
    }
    m_scheme->setCurrentIndex(m_scheme->findData(int(settings.scheme)));
    m_seed->setCurrentIndex(m_seed->findData(int(settings.scheme == NavigationScheme::Custom
                                                   ? NavigationScheme::MiddlePan : settings.scheme)));
    top->addRow(tr("Pan, zoom, orbit shortcuts:"), m_scheme);
    auto* seedRow = new QHBoxLayout;
    seedRow->addWidget(m_seed, 1);
    auto* seed = new QPushButton(tr("Start from"));
    seed->setObjectName(QStringLiteral("navigationSeed"));
    seedRow->addWidget(seed);
    top->addRow(tr("Custom bindings:"), seedRow);
    layout->addLayout(top);
    m_guide = new NavigationGuide;
    layout->addWidget(m_guide);
    m_table = new QTableWidget;
    m_table->setObjectName(QStringLiteral("navigationBindings"));
    mac::styleListTable(m_table);
    m_table->setColumnCount(2);
    m_table->setHorizontalHeaderLabels({tr("Input"), tr("Action")});
    m_table->horizontalHeader()->setSectionResizeMode(0, QHeaderView::Stretch);
    m_table->horizontalHeader()->setSectionResizeMode(1, QHeaderView::ResizeToContents);
    m_table->verticalHeader()->hide();
    m_table->setSelectionMode(QAbstractItemView::NoSelection);
    m_table->setFixedHeight(138);
    m_table->setAccessibleName(tr("Navigation bindings"));
    layout->addWidget(m_table);
    auto* reserved = new QLabel(tr("Left drags used for selection and component movement are reserved. "
                                  "The orientation cube keeps its own gestures."));
    reserved->setObjectName(QStringLiteral("navigationConflicts"));
    reserved->setWordWrap(true);
    layout->addWidget(reserved);
    auto* speeds = new QHBoxLayout;
    auto* mouseSpeeds = new QFormLayout;
    auto* scrollSpeeds = new QFormLayout;
    addSpeed(mouseSpeeds, tr("Orbit speed:"), "navigationOrbitSpeed", &NavigationSettings::orbitSpeed);
    addSpeed(mouseSpeeds, tr("Pan speed:"), "navigationPanSpeed", &NavigationSettings::panSpeed);
    addSpeed(scrollSpeeds, tr("Zoom speed:"), "navigationZoomSpeed", &NavigationSettings::zoomSpeed);
    addSpeed(scrollSpeeds, tr("Trackpad scroll speed:"), "navigationTrackpadSpeed", &NavigationSettings::trackpadSpeed);
    speeds->addLayout(mouseSpeeds);
    speeds->addLayout(scrollSpeeds);
    layout->addLayout(speeds);
    auto* flags = new QGridLayout;
    addFlag(flags, 0, 0, tr("Zoom about the cursor"), "navigationZoomToCursor", &NavigationSettings::zoomToCursor);
    addFlag(flags, 0, 1, tr("Reverse the zoom direction"), "preferencesReverseZoom", &NavigationSettings::reverseZoom);
    addFlag(flags, 1, 0, tr("Constrained orbit (Z stays up)"), "navigationConstrainedOrbit", &NavigationSettings::constrainedOrbit);
    addFlag(flags, 1, 1, tr("Smooth view transitions"), "navigationAnimate", &NavigationSettings::animate);
    layout->addLayout(flags);
    auto* hint = new QLabel(tr("Constrained orbit keeps Z upright. Zoom follows the cursor when enabled. "
                              "The wheel zooms; pinch zooms, rotation rolls, and a two-finger double tap fits."));
    hint->setObjectName(QStringLiteral("navigationGuideExplanation"));
    hint->setWordWrap(true);
    layout->addWidget(hint);
    tabs->addTab(mousePage, tr("Mouse and Trackpad"));
    auto* spacePage = new QWidget;
    auto* spaceLayout = new QVBoxLayout(spacePage);
    spaceLayout->setContentsMargins(0, 6, 0, 0);
    addSpaceMouse(spaceLayout);
    spaceLayout->addStretch(1);
    tabs->addTab(spacePage, tr("3D Mouse"));
    connect(m_scheme, &QComboBox::currentIndexChanged, this, [this] {
      m_settings.scheme = static_cast<NavigationScheme>(m_scheme->currentData().toInt());
      if (m_settings.scheme == NavigationScheme::Custom && m_settings.customBindings.isEmpty())
        m_settings.customBindings = navigationBindings(static_cast<NavigationScheme>(m_seed->currentData().toInt()));
      if (m_settings.scheme != NavigationScheme::Custom) {
        const QSignalBlocker blocker(m_seed);
        m_seed->setCurrentIndex(m_seed->findData(int(m_settings.scheme)));
      }
      rebuildBindings();
      notify();
    });
    connect(seed, &QPushButton::clicked, this, [this] {
      m_settings.customBindings = navigationBindings(static_cast<NavigationScheme>(m_seed->currentData().toInt()));
      chooseCustom();
      rebuildBindings();
      notify();
    });
    rebuildBindings();
  }
  NavigationSettings chosen() const { return m_settings; }
  std::function<void()> changed;
protected:
  void showEvent(QShowEvent* event) override {
    QWidget::showEvent(event);
    // The table's real viewport geometry is available after the page is
    // shown, including when Settings switches to this previously hidden pane.
    QTimer::singleShot(0, m_table, [this] { revealFirstBinding(); });
  }
private:
  void revealFirstBinding() {
    int firstEditable = -1;
    for (int row = 0; row < m_bindings.size(); ++row) {
      const auto& binding = m_bindings[row];
      if (!navigationBindingAllowed(binding)) continue;
      if (firstEditable < 0) firstEditable = row;
      if (binding.action != NavigationAction::None) {
        m_table->scrollToItem(m_table->item(row, 0), QAbstractItemView::PositionAtTop);
        return;
      }
    }
    if (firstEditable >= 0)
      m_table->scrollToItem(m_table->item(firstEditable, 0), QAbstractItemView::PositionAtTop);
  }
  void notify() {
    m_guide->setNavigation(m_settings);
    if (changed) changed();
  }
  void chooseCustom() {
    m_settings.scheme = NavigationScheme::Custom;
    const QSignalBlocker blocker(m_scheme);
    m_scheme->setCurrentIndex(m_scheme->findData(int(NavigationScheme::Custom)));
  }
  void rebuildBindings() {
    m_bindings = m_settings.scheme == NavigationScheme::Custom
                     ? m_settings.customBindings : navigationBindings(m_settings.scheme);
    m_table->setRowCount(0);
    m_table->setRowCount(m_bindings.size());
    for (int row = 0; row < m_bindings.size(); ++row) {
      const auto binding = m_bindings.at(row);
      const bool allowed = navigationBindingAllowed(binding);
      auto* label = new QTableWidgetItem(navigationInputText(binding));
      label->setFlags(Qt::ItemIsEnabled);
      m_table->setItem(row, 0, label);
      auto* action = new QComboBox;
      action->setObjectName(QStringLiteral("navigationBinding%1").arg(row));
      for (const auto value : {NavigationAction::None, NavigationAction::Orbit, NavigationAction::Pan, NavigationAction::Zoom})
        action->addItem(navigationActionText(value), int(value));
      action->setCurrentIndex(action->findData(int(binding.action)));
      action->setEnabled(allowed);
      if (!allowed) {
        label->setText(label->text() + tr(" (reserved)"));
        label->setForeground(palette().color(QPalette::Disabled, QPalette::Text));
        action->setToolTip(tr("Reserved for selection and component movement"));
      }
      m_table->setCellWidget(row, 1, action);
      connect(action, &QComboBox::currentIndexChanged, this, [this, row, action] {
        if (m_settings.scheme != NavigationScheme::Custom) m_settings.customBindings = m_bindings;
        chooseCustom();
        m_settings.customBindings[row].action = static_cast<NavigationAction>(action->currentData().toInt());
        m_bindings = m_settings.customBindings;
        notify();
      });
    }
    m_table->resizeRowsToContents();
    m_guide->setNavigation(m_settings);
    QTimer::singleShot(0, m_table, [this] { revealFirstBinding(); });
  }
  void addSpeed(QFormLayout* form, const QString& title, const char* name, double NavigationSettings::*field) {
    auto* row = new QHBoxLayout;
    auto* slider = new QSlider(Qt::Horizontal);
    slider->setObjectName(QString::fromLatin1(name));
    slider->setMinimumWidth(70);
    slider->setRange(10, 400);
    slider->setSingleStep(10);
    slider->setPageStep(25);
    slider->setValue(int(std::lround(sanitizeNavigationSpeed(m_settings.*field) * 100)));
    auto* value = new QLabel(QStringLiteral("%1×").arg(slider->value() / 100.0, 0, 'f', 2));
    value->setMinimumWidth(48);
    auto* reset = new QPushButton(tr("Reset"));
    reset->setObjectName(QString::fromLatin1(name) + QStringLiteral("Reset"));
    reset->setToolTip(tr("Restores the default navigation speed"));
    row->addWidget(slider, 1);
    row->addWidget(value);
    row->addWidget(reset);
    form->addRow(title, row);
    connect(slider, &QSlider::valueChanged, this, [this, value, field](int position) {
      m_settings.*field = position / 100.0;
      value->setText(QStringLiteral("%1×").arg(m_settings.*field, 0, 'f', 2));
      notify();
    });
    connect(reset, &QPushButton::clicked, slider, [slider] { slider->setValue(100); });
  }
  void addFlag(QGridLayout* grid, int row, int column, const QString& title, const char* name, bool NavigationSettings::*field) {
    auto* box = new QCheckBox(title);
    box->setObjectName(QString::fromLatin1(name));
    box->setChecked(m_settings.*field);
    grid->addWidget(box, row, column);
    connect(box, &QCheckBox::toggled, this, [this, field](bool value) { m_settings.*field = value; notify(); });
  }
  // The 3D mouse's tab (mitcad#951): on or off, its speed, its Navigation
  // mode, each axis on and reversed, the locks, the dominant axis, its
  // Lock horizon (mitcad#978) and its buttons' commands.
  void addSpaceMouse(QVBoxLayout* layout) {
    auto* group = new QWidget;
    group->setObjectName(QStringLiteral("navigationSpaceMouse"));
    auto* box = new QVBoxLayout(group);
    box->setContentsMargins(0, 0, 0, 0);
    const QString status = spaceMouseStatus().text;
    auto* found = new QLabel(status.isEmpty() ? tr("No 3D mouse found") : status);
    found->setObjectName(QStringLiteral("navigationSpaceMouseStatus"));
    found->setWordWrap(true);
    box->addWidget(found);
    auto* on = new QCheckBox(tr("Use a 3D mouse"));
    on->setObjectName(QStringLiteral("navigationSpaceMouseOn"));
    on->setChecked(m_settings.spaceMouse.enabled);
    box->addWidget(on);
    connect(on, &QCheckBox::toggled, this, [this](bool value) {
      m_settings.spaceMouse.enabled = value;
      notify();
    });
    auto* speedForm = new QFormLayout;
    auto* speedRow = new QHBoxLayout;
    auto* slider = new QSlider(Qt::Horizontal);
    slider->setObjectName(QStringLiteral("navigationSpaceMouseSpeed"));
    slider->setMinimumWidth(70);
    slider->setRange(10, 400);
    slider->setSingleStep(10);
    slider->setPageStep(25);
    slider->setValue(int(std::lround(sanitizeSpaceMouseSpeed(m_settings.spaceMouse.speed) * 100)));
    auto* speedValue = new QLabel(QStringLiteral("%1×").arg(slider->value() / 100.0, 0, 'f', 2));
    speedValue->setMinimumWidth(48);
    auto* speedReset = new QPushButton(tr("Reset"));
    speedReset->setObjectName(QStringLiteral("navigationSpaceMouseSpeedReset"));
    speedReset->setToolTip(tr("Restores the default 3D mouse speed"));
    speedRow->addWidget(slider, 1);
    speedRow->addWidget(speedValue);
    speedRow->addWidget(speedReset);
    speedForm->addRow(tr("3D mouse speed:"), speedRow);
    // What follows the cap (mitcad#978).
    auto* mode = new QComboBox;
    mode->setObjectName(QStringLiteral("navigationSpaceMouseMode"));
    mode->addItem(tr("Object"), int(SpaceMouseMode::Object));
    mode->addItem(tr("Camera"), int(SpaceMouseMode::Camera));
    mode->addItem(tr("Fly"), int(SpaceMouseMode::Fly));
    mode->setCurrentIndex(std::max(0, mode->findData(int(m_settings.spaceMouse.mode))));
    mode->setToolTip(tr("What follows the cap: the model (Object) or the camera (Camera, Fly)"));
    speedForm->addRow(tr("Navigation mode:"), mode);
    box->addLayout(speedForm);
    connect(mode, &QComboBox::currentIndexChanged, this, [this, mode] {
      m_settings.spaceMouse.mode = static_cast<SpaceMouseMode>(mode->currentData().toInt());
      notify();
    });
    connect(slider, &QSlider::valueChanged, this, [this, speedValue](int position) {
      m_settings.spaceMouse.speed = position / 100.0;
      speedValue->setText(QStringLiteral("%1×").arg(m_settings.spaceMouse.speed, 0, 'f', 2));
      notify();
    });
    connect(speedReset, &QPushButton::clicked, slider, [slider] { slider->setValue(100); });
    // One row per axis: whether it moves the view, and its direction.
    auto* axes = new QGridLayout;
    axes->addWidget(new QLabel(tr("Axis")), 0, 0);
    axes->addWidget(new QLabel(tr("On")), 0, 1);
    axes->addWidget(new QLabel(tr("Reverse")), 0, 2);
    const QString axisNames[6] = {tr("Pan left and right"), tr("Pan up and down"), tr("Zoom (pull and push)"),
                                  tr("Tilt forward and back"), tr("Spin"), tr("Roll sideways")};
    for (int axis = 0; axis < 6; ++axis) {
      const auto index = static_cast<std::size_t>(axis);
      axes->addWidget(new QLabel(axisNames[axis]), axis + 1, 0);
      auto* axisOn = new QCheckBox;
      axisOn->setObjectName(QStringLiteral("navigationSpaceMouseAxis%1").arg(axis));
      axisOn->setAccessibleName(tr("%1 on").arg(axisNames[axis]));
      axisOn->setChecked(m_settings.spaceMouse.axisOn[index]);
      axes->addWidget(axisOn, axis + 1, 1);
      auto* reverse = new QCheckBox;
      reverse->setObjectName(QStringLiteral("navigationSpaceMouseReverse%1").arg(axis));
      reverse->setAccessibleName(tr("%1 reversed").arg(axisNames[axis]));
      reverse->setChecked(m_settings.spaceMouse.reversed[index]);
      axes->addWidget(reverse, axis + 1, 2);
      connect(axisOn, &QCheckBox::toggled, this, [this, index](bool value) {
        m_settings.spaceMouse.axisOn[index] = value;
        notify();
      });
      connect(reverse, &QCheckBox::toggled, this, [this, index](bool value) {
        m_settings.spaceMouse.reversed[index] = value;
        notify();
      });
    }
    axes->setColumnStretch(0, 1);
    box->addLayout(axes);
    auto* locks = new QGridLayout;
    const auto lock = [this, locks](int row, int column, const QString& title, const char* name,
                                    bool SpaceMouseSettings::*field) {
      auto* check = new QCheckBox(title);
      check->setObjectName(QString::fromLatin1(name));
      check->setChecked(m_settings.spaceMouse.*field);
      locks->addWidget(check, row, column);
      connect(check, &QCheckBox::toggled, this, [this, field](bool value) {
        m_settings.spaceMouse.*field = value;
        notify();
      });
      return check;
    };
    lock(0, 0, tr("Lock rotation"), "navigationSpaceMouseLockRotation", &SpaceMouseSettings::lockRotation);
    lock(0, 1, tr("Lock translation"), "navigationSpaceMouseLockTranslation", &SpaceMouseSettings::lockTranslation);
    lock(1, 0, tr("Dominant axis only"), "navigationSpaceMouseDominant", &SpaceMouseSettings::dominantAxis);
    lock(1, 1, tr("Lock horizon"), "navigationSpaceMouseLockHorizon", &SpaceMouseSettings::lockHorizon)
        ->setToolTip(tr("Keeps Z upright and the view level; the roll axis does nothing"));
    box->addLayout(locks);
    // The buttons the device has, at least two, and those bound before.
    const int rows = std::clamp(std::max({2, int(m_settings.spaceMouse.buttons.size()), spaceMouseStatus().buttonsSeen}),
                                2, 32);
    auto* buttons = new QTableWidget(rows, 2);
    buttons->setObjectName(QStringLiteral("navigationSpaceMouseButtons"));
    mac::styleListTable(buttons);
    buttons->setHorizontalHeaderLabels({tr("Button"), tr("Command")});
    buttons->horizontalHeader()->setSectionResizeMode(0, QHeaderView::ResizeToContents);
    buttons->horizontalHeader()->setSectionResizeMode(1, QHeaderView::Stretch);
    buttons->verticalHeader()->hide();
    buttons->setSelectionMode(QAbstractItemView::NoSelection);
    buttons->setAccessibleName(tr("3D mouse buttons"));
    for (int row = 0; row < rows; ++row) {
      auto* label = new QTableWidgetItem(tr("Button %1").arg(row + 1));
      label->setFlags(Qt::ItemIsEnabled);
      buttons->setItem(row, 0, label);
      auto* command = new QComboBox;
      command->setObjectName(QStringLiteral("navigationSpaceMouseButton%1").arg(row));
      command->addItem(tr("None"), QString());
      for (const auto& entry : m_buttonCommands) {
        command->addItem(entry.second, entry.first);
      }
      const QString bound = m_settings.spaceMouse.buttons.value(row);
      if (!bound.isEmpty() && command->findData(bound) < 0) {
        command->addItem(bound, bound); // a command this list does not offer keeps its binding
      }
      command->setCurrentIndex(std::max(0, command->findData(bound)));
      buttons->setCellWidget(row, 1, command);
      connect(command, &QComboBox::currentIndexChanged, this, [this, row, command] {
        QStringList& bindings = m_settings.spaceMouse.buttons;
        while (bindings.size() <= row) {
          bindings.append(QString());
        }
        bindings[row] = command->currentData().toString();
        notify();
      });
    }
    buttons->resizeRowsToContents();
    buttons->setFixedHeight(std::min(rows, 4) * buttons->rowHeight(0) +
                            buttons->horizontalHeader()->sizeHint().height() + 2 * buttons->frameWidth());
    box->addWidget(buttons);
    layout->addWidget(group);
  }

  NavigationSettings m_settings;
  QVector<QPair<QString, QString>> m_buttonCommands;
  QVector<NavigationBinding> m_bindings;
  QComboBox* m_scheme = nullptr;
  QComboBox* m_seed = nullptr;
  QTableWidget* m_table = nullptr;
  NavigationGuide* m_guide = nullptr;
};

} // namespace mitcad
