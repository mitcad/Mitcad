// SPDX-License-Identifier: MIT
#pragma once

#include <functional>

#include <QMap>
#include <QString>
#include <QStringList>

namespace mitcad {

// The commands the user pinned to the toolbar (mitcad#1001): per tab
// (SOLID, BUILD, SKETCH), in the order pinned or as dragged since. The
// Ribbon shows each as a button in its own group, after the group's own
// buttons (mitcad#1084). They are a preference of the user's, kept in the
// settings: "toolbarPins/<TAB>", the command ids joined by commas
// ("solid.sweep,solid.loft"); a tab without pins has no key. A command is
// kept under its own tab, the one its group is on. Every change is saved at
// once. Plain Qt Core.
class ToolbarPins {
public:
  // A command as the toolbar places it: its id, its tab and its group
  // ("solid.shell", "SOLID", "MODIFY"). An empty id: no such command.
  struct Command {
    QString id;
    QString tab;
    QString group;
  };
  // The command for an id read from the settings or pinned (the command's
  // own, or the one an alias names, CommandRegistry::find), with its place.
  using Resolve = std::function<Command(const QString& id)>;

  // The pins in the settings. An id `resolve` does not know is dropped
  // with a log line ("Toolbar pins: dropped solid.gone from SOLID: no such
  // command"), an alias becomes its command's id, a repeated id is kept
  // once and a command kept under another tab than its own (one an
  // earlier version moved, mitcad#1049) goes to its own tab's pins, at
  // their end ("Toolbar pins: moved <id> from SOLID to MAKE");
  // the settings are written again without what was dropped or moved.
  // `resolve` is kept for the groups' pins (pins(tab, group), moveInGroup).
  static ToolbarPins load(const Resolve& resolve);

  // The settings' key of a tab's pins.
  static QString settingsKey(const QString& tab);

  // A tab's pinned commands, in order.
  QStringList pins(const QString& tab) const { return m_pins.value(tab); }
  // A group's pinned commands, in the tab's order: the buttons the Ribbon
  // adds after the group's own (mitcad#1084). None without a `resolve`.
  QStringList pins(const QString& tab, const QString& group) const;
  bool isPinned(const QString& tab, const QString& id) const;
  // The tabs that have pins.
  QStringList tabs() const { return m_pins.keys(); }

  // Each saves the tab's pins and is false when nothing changed: pin()
  // adds the command at the end, unpin() removes it, move() puts it at
  // `index` of the tab's pins (clamped to them), moveInGroup() at `index`
  // of its group's pins (clamped to them), the other groups' pins where
  // they were.
  bool pin(const QString& tab, const QString& id);
  bool unpin(const QString& tab, const QString& id);
  bool move(const QString& tab, const QString& id, int index);
  bool moveInGroup(const QString& tab, const QString& id, int index);

private:
  void save(const QString& tab) const;

  QMap<QString, QStringList> m_pins; // by tab; no empty lists
  Resolve m_resolve;                 // load()'s
};

} // namespace mitcad
