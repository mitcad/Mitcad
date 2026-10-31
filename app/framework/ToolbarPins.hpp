// SPDX-License-Identifier: MIT
#pragma once

#include <functional>

#include <QMap>
#include <QString>
#include <QStringList>

namespace mitcad {

// The commands the user pinned to the toolbar (mitcad#1001): per tab
// (SOLID, BUILD, SKETCH), in the order pinned or as dragged since. The
// Ribbon shows them as the Pinned group at the end of their tab. They are a
// preference of the user's, kept in the settings: "toolbarPins/<TAB>", the
// command ids joined by commas ("solid.sweep,solid.loft"); a tab without
// pins has no key. Every change is saved at once. Plain Qt Core.
class ToolbarPins {
public:
  // The command's id for an id read from the settings (the command's own,
  // or the one an alias names, CommandRegistry::find), or an empty string
  // when no such command exists.
  using Resolve = std::function<QString(const QString& id)>;

  // The pins in the settings. An id `resolve` does not know is dropped
  // with a log line ("Toolbar pins: dropped solid.gone from SOLID: no such
  // command"), an alias becomes its command's id and a repeated id is kept
  // once; the settings are written again without what was dropped.
  static ToolbarPins load(const Resolve& resolve);

  // The settings' key of a tab's pins.
  static QString settingsKey(const QString& tab);

  // A tab's pinned commands, in order.
  QStringList pins(const QString& tab) const { return m_pins.value(tab); }
  bool isPinned(const QString& tab, const QString& id) const;
  // The tabs that have pins.
  QStringList tabs() const { return m_pins.keys(); }

  // Each saves the tab's pins and is false when nothing changed: pin()
  // adds the command at the end, unpin() removes it, move() puts it at
  // `index` of the tab's pins (clamped to them).
  bool pin(const QString& tab, const QString& id);
  bool unpin(const QString& tab, const QString& id);
  bool move(const QString& tab, const QString& id, int index);

private:
  void save(const QString& tab) const;

  QMap<QString, QStringList> m_pins; // by tab; no empty lists
};

} // namespace mitcad
