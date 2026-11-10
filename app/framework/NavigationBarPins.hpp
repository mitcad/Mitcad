// SPDX-License-Identifier: MIT
#pragma once

#include <functional>

#include <QString>
#include <QStringList>

namespace mitcad {

// The commands the user pinned to the navigation bar (mitcad#1589): one
// list for every workspace, in the order pinned or as dragged since. The
// navigation bar shows each as a button after a separator at its end. A
// preference of the user's, kept in the settings: "navigationBarPins", the
// command ids joined by commas ("solid.parameters,inspect.measure"); an
// empty value is no pins. Without the key the defaults are pinned: Edit
// Parameters (mitcad#1582), which is unpinned like any other. Every change
// is saved at once. Plain Qt Core.
class NavigationBarPins {
public:
  // The command for an id read from the settings or pinned: the command's
  // own id, or the one an alias names; "" when there is no such command.
  using Resolve = std::function<QString(const QString& id)>;

  // The settings' key.
  static QString settingsKey();
  // What is pinned while the settings have no key: solid.parameters.
  static QStringList defaults();

  // The pins in the settings (the defaults without the key). An id that
  // `resolve` does not know is dropped with a log line ("Navigation bar
  // pins: dropped solid.gone: no such command"), an alias becomes its
  // command's id and a repeated id is kept once; the settings are then
  // written again without them. Without a `resolve` every id is kept.
  static NavigationBarPins load(const Resolve& resolve);

  // The pinned commands, in order.
  QStringList pins() const { return m_pins; }
  bool isPinned(const QString& id) const { return m_pins.contains(id); }

  // Each saves the pins and is false when nothing changed: pin() adds the
  // command at the end, unpin() removes it, move() puts it at `index` of
  // the pins (clamped to them).
  bool pin(const QString& id);
  bool unpin(const QString& id);
  bool move(const QString& id, int index);

private:
  void save() const;

  QStringList m_pins;
};

} // namespace mitcad
