// SPDX-License-Identifier: MIT
#pragma once

#include <QKeySequence>
#include <QList>
#include <QMap>
#include <QObject>
#include <QSet>
#include <QString>
#include <QStringList>

namespace mitcad {

// Potentially simultaneous states, independent of the current selection.
// A model feature which accepts profiles also has SketchShortcutContext.
enum ShortcutContext : unsigned {
  ModelShortcutContext = 1,
  SketchShortcutContext = 2,
  CommandShortcutContext = 4,
  AllShortcutContexts = ModelShortcutContext | SketchShortcutContext | CommandShortcutContext,
};

// A command's key in each context it lives in (mitcad#720), by context.
using ContextShortcuts = QMap<unsigned, QKeySequence>;

// The single contexts of `contexts`, in the order the Keyboard Shortcuts
// window lists them: Model, Sketch, Command Panel.
inline QList<unsigned> shortcutContextList(unsigned contexts) {
  QList<unsigned> list;
  for (const unsigned context : {unsigned(ModelShortcutContext), unsigned(SketchShortcutContext),
                                 unsigned(CommandShortcutContext)}) {
    if ((contexts & context) != 0) list.append(context);
  }
  return list;
}

// "Model", "Sketch", "Command Panel" (while a command's panel is open);
// several joined by commas.
inline QString shortcutContextName(unsigned contexts) {
  QStringList names;
  for (const unsigned context : shortcutContextList(contexts)) {
    names.append(context == ModelShortcutContext    ? QObject::tr("3D View")
                 : context == SketchShortcutContext ? QObject::tr("Sketch")
                                                    : QObject::tr("Command Panel"));
  }
  return names.join(QStringLiteral(", "));
}

// Where keys meet, for a sentence: "in the 3D view", "in a sketch", "while a
// command panel is open", the last joined by "and".
inline QString shortcutContextPhrase(unsigned contexts) {
  QStringList parts;
  for (const unsigned context : shortcutContextList(contexts)) {
    parts.append(context == ModelShortcutContext    ? QObject::tr("in the 3D view")
                 : context == SketchShortcutContext ? QObject::tr("in a sketch")
                                                    : QObject::tr("while a command panel is open"));
  }
  if (parts.size() < 2) return parts.value(0);
  const QString last = parts.takeLast();
  return QObject::tr("%1 and %2").arg(parts.join(QStringLiteral(", ")), last);
}

struct ShortcutBinding {
  QString id;
  QString name;
  unsigned contexts = AllShortcutContexts;
  QKeySequence primary;
  QKeySequence defaultPrimary;
  QList<QKeySequence> alternates;
  bool fixed = false;

  QList<QKeySequence> effectiveKeys() const {
    QList<QKeySequence> keys;
    if (!primary.isEmpty()) {
      keys.append(primary);
      if (primary == defaultPrimary) {
        for (const QKeySequence& alternate : alternates) {
          if (!alternate.isEmpty() && !keys.contains(alternate)) keys.append(alternate);
        }
      }
    }
    return keys;
  }
};

struct ShortcutConflict {
  ShortcutBinding first;
  ShortcutBinding second;
  QKeySequence keys;
  unsigned contexts = 0; // where both are active

  // "S is used by both Command Search and Fit Point Spline in a sketch.
  // Change one of them."
  QString description() const {
    const QString key = keys.toString(QKeySequence::NativeText);
    if (first.fixed || second.fixed) {
      const ShortcutBinding& reserved = first.fixed ? first : second;
      const ShortcutBinding& other = first.fixed ? second : first;
      return QObject::tr("%1 is reserved for %2. Choose another key for %3.").arg(key, reserved.name, other.name);
    }
    return QObject::tr("%1 is used by both %2 and %3 %4. Change one of them.")
        .arg(key, first.name, second.name, shortcutContextPhrase(contexts));
  }
};

inline QList<ShortcutConflict> shortcutConflicts(const QList<ShortcutBinding>& bindings) {
  QList<ShortcutConflict> conflicts;
  for (qsizetype i = 0; i < bindings.size(); ++i) {
    for (qsizetype j = i + 1; j < bindings.size(); ++j) {
      const auto& first = bindings[i];
      const auto& second = bindings[j];
      if (first.id == second.id || !(first.contexts & second.contexts)) continue;
      for (const auto& a : first.effectiveKeys()) {
        for (const auto& b : second.effectiveKeys()) {
          // Prefixes are ambiguous too if a legacy setting contains a chord.
          if (a.matches(b) != QKeySequence::NoMatch || b.matches(a) != QKeySequence::NoMatch) {
            conflicts.append({first, second, a, first.contexts & second.contexts});
          }
        }
      }
    }
  }
  return conflicts;
}

// Do not erase legacy settings. Disable their runtime bindings until the
// user repairs them, preserving every menu action. Defaults and fixed keys
// win over custom bindings; two colliding custom bindings are both disabled.
inline QSet<QString> conflictingShortcutIds(const QList<ShortcutConflict>& conflicts) {
  QSet<QString> disabled;
  for (const auto& conflict : conflicts) {
    const bool customFirst = !conflict.first.fixed &&
                             conflict.first.primary != conflict.first.defaultPrimary;
    const bool customSecond = !conflict.second.fixed &&
                              conflict.second.primary != conflict.second.defaultPrimary;
    if (customFirst) disabled.insert(conflict.first.id);
    if (customSecond) disabled.insert(conflict.second.id);
    if (!customFirst && !customSecond) {
      if (!conflict.first.fixed) disabled.insert(conflict.first.id);
      if (!conflict.second.fixed) disabled.insert(conflict.second.id);
    }
  }
  return disabled;
}

// These keys are installed outside the configurable command registry.
// Their names are the ones users see in menus, context menus and command
// panels (conflict messages name them), not descriptions of where a key
// works.
inline QList<ShortcutBinding> fixedShortcutBindings() {
  QList<ShortcutBinding> bindings;
  const auto add = [&bindings](const char* id, const QString& name, const QKeySequence& key,
                               unsigned contexts = AllShortcutContexts) {
    if (!key.isEmpty()) bindings.append({QString::fromLatin1(id), name, contexts, key, key, {}, true});
  };
  // Open, Close, Save, Save As and Exit are registry commands with keys of
  // their own since mitcad#1090 (files/FileCommands.cpp), changed like any
  // other command's.
  // A text field's Cut, Copy and Paste. In a sketch they are the sketch's
  // own commands (sketch.cut, sketch.copy, sketch.paste; mitcad#788), which
  // a focused text field still takes first: QLineEdit accepts the
  // ShortcutOverride for them.
  add("edit.cut", QObject::tr("Cut Text"), QKeySequence::Cut, ModelShortcutContext | CommandShortcutContext);
  add("edit.copy", QObject::tr("Copy Text"), QKeySequence::Copy, ModelShortcutContext | CommandShortcutContext);
  add("edit.paste", QObject::tr("Paste Text"), QKeySequence::Paste, ModelShortcutContext | CommandShortcutContext);
  add("edit.select_all", QObject::tr("Select All"), QKeySequence::SelectAll);
  // Search Timeline, Rename and Delete work while the Browser or the
  // timeline has the keyboard focus.
  add("view.search_timeline", QObject::tr("Search Timeline"), QKeySequence::Find);
  add("browser.rename", QObject::tr("Rename"), QKeySequence(Qt::Key_F2), ModelShortcutContext);
  // Collection mutations require model idle, so they are exclusive with
  // sketch.delete. A global Model/Any command still conflicts with them.
  add("browser.delete", QObject::tr("Delete"), QKeySequence(Qt::Key_Delete), ModelShortcutContext);
#ifdef Q_OS_MACOS
  add("browser.delete", QObject::tr("Delete"), QKeySequence(Qt::Key_Backspace), ModelShortcutContext);
#endif
  // Enter and Esc are OK and Cancel of a command panel; Esc also clears
  // the selection.
  add("command.accept", QObject::tr("OK"), QKeySequence(Qt::Key_Return));
  add("command.accept", QObject::tr("OK"), QKeySequence(Qt::Key_Enter));
  add("command.cancel", QObject::tr("Cancel"), QKeySequence(Qt::Key_Escape));
#ifdef Q_OS_MACOS
  add("window.minimize", QObject::tr("Minimize"), QKeySequence(Qt::CTRL | Qt::Key_M));
  add("application.hide", QObject::tr("Hide MitCad"), QKeySequence(Qt::CTRL | Qt::Key_H));
  add("application.hide_others", QObject::tr("Hide Others"), QKeySequence(Qt::CTRL | Qt::ALT | Qt::Key_H));
  // Qt Meta represents the physical Control key on macOS.
  add("window.full_screen", QObject::tr("Enter Full Screen"), QKeySequence(Qt::CTRL | Qt::META | Qt::Key_F));
#endif
  return bindings;
}

// A reserved key's read-only row in Keyboard Shortcuts (mitcad#730): one
// per name, with every key of it (OK: Return and Enter) and the contexts
// where they work.
struct ReservedShortcut {
  QString id;
  QString name;
  unsigned contexts = 0;
  QList<QKeySequence> keys;
  // Search Timeline, Rename and Delete: only while the Browser or the
  // timeline has the keyboard focus (BrowserPanel, TimelineWidget).
  bool browserFocus = false;
};

// The reserved keys by name, in the order of fixedShortcutBindings().
inline QList<ReservedShortcut> reservedShortcuts() {
  QList<ReservedShortcut> rows;
  for (const ShortcutBinding& binding : fixedShortcutBindings()) {
    ReservedShortcut* row = nullptr;
    for (ReservedShortcut& entry : rows) {
      if (entry.id == binding.id) row = &entry;
    }
    if (row == nullptr) {
      const bool browserFocus = binding.id == QLatin1String("view.search_timeline") ||
                                binding.id == QLatin1String("browser.rename") ||
                                binding.id == QLatin1String("browser.delete");
      rows.append({binding.id, binding.name, 0, {}, browserFocus});
      row = &rows.last();
    }
    row->contexts |= binding.contexts;
    if (!row->keys.contains(binding.primary)) row->keys.append(binding.primary);
  }
  return rows;
}

} // namespace mitcad
