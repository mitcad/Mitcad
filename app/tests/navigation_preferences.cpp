// SPDX-License-Identifier: MIT
#include <cstdio>

#include <QCheckBox>
#include <QComboBox>
#include <QCoreApplication>
#include <QEventLoop>
#include <QLabel>
#include <QPushButton>
#include <QSlider>
#include <QTableWidget>
#include <QTemporaryDir>

#include "view/NavigationPreferences.hpp"

namespace {
int failures = 0;
void check(bool value, const char* expression, int line) {
  if (!value) {
    std::fprintf(stderr, "navigation_preferences.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}
#define CHECK(value) check((value), #value, __LINE__)
}

int navigationPreferencesTests() {
  using namespace mitcad;
  const QVector<QPair<NavigationScheme, QString>> schemes{
      {NavigationScheme::MiddlePan, QStringLiteral("Middle pan")},
      {NavigationScheme::AltButtons, QStringLiteral("Alt buttons")},
      {NavigationScheme::MiddlePanF4, QStringLiteral("F4")},
      {NavigationScheme::MiddleOrbit, QStringLiteral("Middle orbit")},
      {NavigationScheme::RightOrbit, QStringLiteral("Right orbit")},
      {NavigationScheme::Trackpad, QStringLiteral("Trackpad")},
      {NavigationScheme::TrackpadShift, QStringLiteral("Trackpad Shift")},
      {NavigationScheme::Custom, QStringLiteral("Custom")}};
  NavigationSettings initial;
  initial.scheme = NavigationScheme::MiddlePan;
  NavigationPreferences form(initial, schemes);
  int changes = 0;
  form.changed = [&changes] { ++changes; };
  auto* scheme = form.findChild<QComboBox*>(QStringLiteral("navigationScheme"));
  auto* seed = form.findChild<QComboBox*>(QStringLiteral("navigationStartFrom"));
  auto* seedButton = form.findChild<QPushButton*>(QStringLiteral("navigationSeed"));
  auto* table = form.findChild<QTableWidget*>(QStringLiteral("navigationBindings"));
  auto* guide = form.findChild<QWidget*>(QStringLiteral("navigationVisualGuide"));
  CHECK(scheme && seed && seedButton && table && guide);
  if (!scheme || !seed || !seedButton || !table || !guide) return failures;
  CHECK(changes == 0);
  CHECK(guide->accessibleDescription().contains(QStringLiteral("Middle drag: Pan")));
  CHECK(table->rowCount() == navigationBindings(NavigationScheme::MiddlePan).size());

  // Editing a preset preserves its other gestures and enters Custom.
  int middle = -1;
  const auto bindings = navigationBindings(NavigationScheme::MiddlePan);
  for (int row = 0; row < bindings.size(); ++row) {
    const auto& binding = bindings[row];
    auto* editor = qobject_cast<QComboBox*>(table->cellWidget(row, 1));
    CHECK(editor != nullptr);
    if (!navigationBindingAllowed(binding)) CHECK(editor && !editor->isEnabled());
    if (binding.input == NavigationInput::MiddleButton && binding.modifiers == Qt::NoModifier) middle = row;
  }
  CHECK(middle >= 0);
  // A short binding table initially reveals a useful gesture, rather than
  // spending its whole viewport on disabled reserved left-button rows.
  form.setAttribute(Qt::WA_DontShowOnScreen);
  form.resize(900, 700);
  form.show();
  QCoreApplication::processEvents(QEventLoop::ExcludeUserInputEvents);
  if (middle >= 0) {
    CHECK(table->rowAt(1) == middle);
    CHECK(table->viewport()->rect().contains(table->visualItemRect(table->item(middle, 0))));
  }
  if (middle >= 0) {
    auto* editor = qobject_cast<QComboBox*>(table->cellWidget(middle, 1));
    if (editor) editor->setCurrentIndex(editor->findData(int(NavigationAction::Zoom)));
    CHECK(form.chosen().scheme == NavigationScheme::Custom);
    CHECK(navigationAction(form.chosen(), NavigationInput::MiddleButton, Qt::NoModifier) == NavigationAction::Zoom);
    CHECK(navigationAction(form.chosen(), NavigationInput::MiddleButton, Qt::ShiftModifier) == NavigationAction::Orbit);
    CHECK(guide->accessibleDescription().contains(QStringLiteral("Middle drag: Zoom")));
  }

  // Start from explicitly replaces the custom table, including trackpad rows.
  seed->setCurrentIndex(seed->findData(int(NavigationScheme::TrackpadShift)));
  seedButton->click();
  CHECK(form.chosen().scheme == NavigationScheme::Custom);
  CHECK(navigationAction(form.chosen(), NavigationInput::TrackpadScroll, Qt::NoModifier) == NavigationAction::Pan);
  CHECK(navigationAction(form.chosen(), NavigationInput::TrackpadScroll, Qt::ShiftModifier) == NavigationAction::Orbit);
  CHECK(guide->accessibleDescription().contains(QStringLiteral("Two-finger scroll: Pan")));
  // Rebuilding for another preset and revealing a hidden pane both use its
  // first active editable row, without changing the canonical row ids.
  scheme->setCurrentIndex(scheme->findData(int(NavigationScheme::RightOrbit)));
  int right = -1;
  const auto rightBindings = navigationBindings(NavigationScheme::RightOrbit);
  for (int row = 0; row < rightBindings.size(); ++row) {
    if (navigationBindingAllowed(rightBindings[row]) && rightBindings[row].action != NavigationAction::None) {
      right = row;
      break;
    }
  }
  QCoreApplication::processEvents(QEventLoop::ExcludeUserInputEvents);
  CHECK(right >= 0 && table->rowAt(1) == right);
  table->scrollToBottom();
  form.hide();
  form.show();
  QCoreApplication::processEvents(QEventLoop::ExcludeUserInputEvents);
  CHECK(right >= 0 && table->rowAt(1) == right);
  form.hide();
  seed->setCurrentIndex(seed->findData(int(NavigationScheme::TrackpadShift)));
  seedButton->click();

  for (const char* name : {"navigationOrbitSpeed", "navigationPanSpeed", "navigationZoomSpeed", "navigationTrackpadSpeed"}) {
    auto* slider = form.findChild<QSlider*>(QString::fromLatin1(name));
    auto* reset = form.findChild<QPushButton*>(QString::fromLatin1(name) + QStringLiteral("Reset"));
    CHECK(slider && reset);
    if (!slider || !reset) continue;
    CHECK(slider->value() == 100);
    slider->setValue(235);
    CHECK(changes > 0);
    reset->click();
    CHECK(slider->value() == 100);
  }
  auto* zoom = form.findChild<QSlider*>(QStringLiteral("navigationZoomSpeed"));
  auto* constrained = form.findChild<QCheckBox*>(QStringLiteral("navigationConstrainedOrbit"));
  CHECK(zoom && constrained);
  if (zoom) zoom->setValue(150);
  if (constrained) constrained->setChecked(true);
  CHECK(form.chosen().zoomSpeed == 1.5);
  CHECK(form.chosen().constrainedOrbit);

  // The 3D mouse's tab (mitcad#951): its speed, a reversed axis, an axis
  // off, a lock and a button's command go into the settings.
  auto* spaceSpeed = form.findChild<QSlider*>(QStringLiteral("navigationSpaceMouseSpeed"));
  auto* reverseTx = form.findChild<QCheckBox*>(QStringLiteral("navigationSpaceMouseReverse0"));
  auto* rollOn = form.findChild<QCheckBox*>(QStringLiteral("navigationSpaceMouseAxis5"));
  auto* lockRotation = form.findChild<QCheckBox*>(QStringLiteral("navigationSpaceMouseLockRotation"));
  auto* firstButton = form.findChild<QComboBox*>(QStringLiteral("navigationSpaceMouseButton0"));
  auto* secondButton = form.findChild<QComboBox*>(QStringLiteral("navigationSpaceMouseButton1"));
  CHECK(spaceSpeed && reverseTx && rollOn && lockRotation && firstButton && secondButton);
  if (firstButton && secondButton) {
    CHECK(firstButton->currentData().toString() == QStringLiteral("view.fit"));
    CHECK(secondButton->currentData().toString() == QStringLiteral("view.home"));
    firstButton->setCurrentIndex(firstButton->findData(QStringLiteral("view.home")));
    secondButton->setCurrentIndex(0); // None
  }
  if (spaceSpeed) spaceSpeed->setValue(200);
  if (reverseTx) reverseTx->setChecked(true);
  if (rollOn) rollOn->setChecked(false);
  if (lockRotation) lockRotation->setChecked(true);
  CHECK(form.chosen().spaceMouse.speed == 2.0 && form.chosen().spaceMouse.reversed[0] &&
        !form.chosen().spaceMouse.axisOn[5] && form.chosen().spaceMouse.lockRotation);
  CHECK(form.chosen().spaceMouse.buttons == (QStringList{QStringLiteral("view.home"), QString()}));
  // Its Navigation mode and Lock horizon (mitcad#978): Object and off by
  // default, Lock horizon apart from the mouse's Constrained orbit.
  auto* spaceMode = form.findChild<QComboBox*>(QStringLiteral("navigationSpaceMouseMode"));
  auto* lockHorizon = form.findChild<QCheckBox*>(QStringLiteral("navigationSpaceMouseLockHorizon"));
  CHECK(spaceMode && lockHorizon);
  if (spaceMode && lockHorizon) {
    CHECK(spaceMode->count() == 3 && spaceMode->currentData().toInt() == int(SpaceMouseMode::Object));
    CHECK(spaceMode->itemText(0) == QStringLiteral("Object") && spaceMode->itemText(1) == QStringLiteral("Camera") &&
          spaceMode->itemText(2) == QStringLiteral("Fly"));
    CHECK(!lockHorizon->isChecked() && lockHorizon->text() == QStringLiteral("Lock horizon"));
    spaceMode->setCurrentIndex(spaceMode->findData(int(SpaceMouseMode::Camera)));
    lockHorizon->setChecked(true);
    if (constrained) constrained->setChecked(false);
  }
  CHECK(form.chosen().spaceMouse.mode == SpaceMouseMode::Camera && form.chosen().spaceMouse.lockHorizon &&
        !form.chosen().constrainedOrbit);

  // Save and reload the widget's actual choices in an isolated settings file.
  QTemporaryDir folder;
  CHECK(folder.isValid());
  if (folder.isValid()) {
    const QString path = folder.filePath(QStringLiteral("navigation.ini"));
    {
      QSettings store(path, QSettings::IniFormat);
      saveNavigationDetails(store, form.chosen());
      store.setValue(QStringLiteral("view/navigationScheme"), QStringLiteral("custom"));
      store.sync();
      CHECK(store.status() == QSettings::NoError);
    }
    NavigationSettings loaded;
    loaded.scheme = NavigationScheme::Custom;
    {
      QSettings store(path, QSettings::IniFormat);
      loadNavigationDetails(store, loaded);
    }
    NavigationPreferences reopened(loaded, schemes);
    CHECK(reopened.chosen().zoomSpeed == 1.5);
    CHECK(reopened.chosen().spaceMouse.speed == 2.0 && reopened.chosen().spaceMouse.reversed[0] &&
          !reopened.chosen().spaceMouse.axisOn[5] && reopened.chosen().spaceMouse.lockRotation);
    CHECK(reopened.chosen().spaceMouse.buttons == (QStringList{QStringLiteral("view.home"), QString()}));
    CHECK(reopened.chosen().spaceMouse.mode == SpaceMouseMode::Camera && reopened.chosen().spaceMouse.lockHorizon);
    auto* reopenedMode = reopened.findChild<QComboBox*>(QStringLiteral("navigationSpaceMouseMode"));
    auto* reopenedHorizon = reopened.findChild<QCheckBox*>(QStringLiteral("navigationSpaceMouseLockHorizon"));
    CHECK(reopenedMode && reopenedMode->currentData().toInt() == int(SpaceMouseMode::Camera));
    CHECK(reopenedHorizon && reopenedHorizon->isChecked());
    auto* reopenedSecond = reopened.findChild<QComboBox*>(QStringLiteral("navigationSpaceMouseButton1"));
    CHECK(reopenedSecond && reopenedSecond->currentIndex() == 0);
    CHECK(navigationAction(reopened.chosen(), NavigationInput::TrackpadScroll, Qt::ShiftModifier) == NavigationAction::Orbit);
    CHECK(navigationAction(reopened.chosen(), NavigationInput::LeftButton, Qt::NoModifier) == NavigationAction::None);
    CHECK(reopened.findChild<QComboBox*>(QStringLiteral("navigationScheme"))->currentData().toInt() == int(NavigationScheme::Custom));
  }
  return failures;
}
