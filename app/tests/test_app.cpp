// SPDX-License-Identifier: MIT
// Unit tests of the application's own helpers, without windows (ctest runs
// them on Qt's offscreen platform). Plain checks, as in the geometry tests.

#include <algorithm>
#include <array>
#include <cmath>
#include <cstdint>
#include <cstdio>
#include <memory>
#include <mutex>
#include <optional>
#include <string>
#include <vector>

#include <QApplication>
#include <QBuffer>
#include <QColor>
#include <QCursor>
#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QImage>
#include <QJsonArray>
#include <QJsonObject>
#include <QPixmap>
#include <QSettings>
#include <QSemaphore>
#include <QStandardItemModel>
#include <QString>
#include <QTemporaryDir>

#include "browser/DocumentSnapshot.hpp"
#include "browser/TimelineSearch.hpp"
#include "files/CloudAddress.hpp"
#include "files/Print3d.hpp"
#include "framework/Cursors.hpp"
#include "framework/AtomicCopy.hpp"
#include "framework/CommandRegistry.hpp"
#include "framework/WorkerThreadStart.hpp"
#include "framework/DesktopEntry.hpp"
#include "framework/Numbers.hpp"
#include "framework/Selection.hpp"
#include "framework/ShortcutBindings.hpp"
#include "render/RenderCamera.hpp"
#include "projects/ProjectRowDrag.hpp"
#include "render/RenderDevices.hpp"
#include "render/RenderImage.hpp"
#include "render/RenderWorkerFailure.hpp"
#include "report/ReportText.hpp"
#include "sketch/AutoConstraint.hpp"
#include "view/ViewMatch.hpp"
#include "view/ViewNavigation.hpp"
#include "NavigationTests.hpp"

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "test_app.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

void atomicCopyTests() {
  QTemporaryDir directory;
  CHECK(directory.isValid());
  const QString path = directory.filePath(QStringLiteral("render.png"));
  QFile destination(path);
  CHECK(destination.open(QIODevice::WriteOnly));
  CHECK(destination.write("old image") == 9);
  destination.close();

  class FailingSource : public QIODevice {
  public:
    FailingSource() { open(QIODevice::ReadOnly | QIODevice::Unbuffered); }
    bool isSequential() const override { return true; }
  protected:
    qint64 readData(char* data, qint64 size) override {
      if (m_read || size <= 0) {
        setErrorString(QStringLiteral("synthetic image read failure"));
        return -1;
      }
      data[0] = 'x';
      m_read = true;
      return 1;
    }
    qint64 writeData(const char*, qint64) override { return -1; }
  private:
    bool m_read = false;
  } broken;
  QString error;
  CHECK(!mitcad::copyToFileAtomically(broken, path, error));
  CHECK(error.contains(QStringLiteral("synthetic image read failure")));
  CHECK(destination.open(QIODevice::ReadOnly));
  CHECK(destination.readAll() == QByteArray("old image"));
  destination.close();
  CHECK(QDir(directory.path()).entryList(QDir::Files).size() == 1);

  QByteArray bytes("replacement image");
  QBuffer source(&bytes);
  CHECK(source.open(QIODevice::ReadOnly));
  CHECK(mitcad::copyToFileAtomically(source, path, error));
  CHECK(destination.open(QIODevice::ReadOnly));
  CHECK(destination.readAll() == bytes);
  destination.close();
  source.seek(0);
  CHECK(!mitcad::copyToFileAtomically(source, directory.filePath(QStringLiteral("missing/render.png")), error));
  CHECK(!error.isEmpty());
}

void projectRowDragTests() {
  QStandardItemModel rows;
  rows.appendRow(new QStandardItem(QStringLiteral("original")));
  mitcad::ProjectRowDrag drag;
  drag.press(rows.index(0, 0), QPoint(5, 5));
  CHECK(!drag.move(QPoint(7, 5), 10));
  // The async read completes between press and the first drag movement.
  rows.clear();
  rows.appendRow(new QStandardItem(QStringLiteral("replacement")));
  CHECK(!drag.item().isValid());
  CHECK(!drag.move(QPoint(40, 5), 10));
  CHECK(!drag.dragging());
  // A fresh press can drag; inserting another row preserves its identity.
  drag.press(rows.index(0, 0), QPoint(5, 5));
  rows.insertRow(0, new QStandardItem(QStringLiteral("inserted")));
  CHECK(drag.move(QPoint(20, 5), 10));
  CHECK(drag.item().data().toString() == QStringLiteral("replacement"));
  rows.removeRow(1);
  CHECK(!drag.dragging());
  CHECK(!drag.move(QPoint(40, 5), 10));
  drag.press(rows.index(0, 0), QPoint());
  CHECK(drag.move(QPoint(30, 0), 10));
  drag.clear();
  CHECK(!drag.item().isValid() && !drag.dragging());
}

void renderWorkerFailureTests() {
  using mitcad::render::workerExitReason;
  const QString stale = QStringLiteral("cannot read the texture image");
  const QString crash = workerExitReason(stale, 9, true, false);
  CHECK(crash.contains(QStringLiteral("crashed")));
  CHECK(crash.contains(QStringLiteral("exit code 9")));
  CHECK(!crash.contains(stale));
  CHECK(workerExitReason(QString(), 0, false, false).contains(QStringLiteral("exit code 0")));
  const QString failed = workerExitReason(QStringLiteral("cannot create renderer"), 2, false, false);
  CHECK(failed.contains(QStringLiteral("cannot create renderer")));
  CHECK(failed.contains(QStringLiteral("exit code 2")));
  const QString mismatch = workerExitReason(QStringLiteral("protocol mismatch"), 9, true, true);
  CHECK(mismatch.contains(QStringLiteral("protocol mismatch")));
  CHECK(mismatch.contains(QStringLiteral("exit code 9")));
}

void workerThreadStartTests() {
  class HeldThread : public QThread {
  public:
    ~HeldThread() override { m_release.release(); wait(); }
  protected:
    void run() override { m_release.acquire(); }
  private:
    QSemaphore m_release;
  } worker;
  worker.setStackSize(256u << 20);
  QList<unsigned> attempted;
  QString error;
  CHECK(mitcad::startWorkerThread(worker, error, [&](QThread& thread) {
    attempted.append(thread.stackSize());
    if (attempted.size() == 3) { thread.start(); }
  }));
  CHECK(error.isEmpty());
  CHECK(worker.isRunning());
  CHECK(attempted == QList<unsigned>({256u << 20, 128u << 20, 64u << 20}));
  // Success does not require handling a started signal on the UI thread.
  CHECK(mitcad::startWorkerThread(worker, error, [&](QThread&) { CHECK(false); }));

  HeldThread unavailable;
  CHECK(!mitcad::startWorkerThread(unavailable, error, [](QThread&) {}));
  CHECK(!unavailable.isRunning());
  CHECK(error.contains(QStringLiteral("no computation was queued")));
  CHECK(unavailable.stackSize() == (256u << 20));
}

// Contexts, default-only alternatives and legacy settings (mitcad#187).
void shortcutBindingTests() {
  using namespace mitcad;
  const auto make = [](const char* id, unsigned contexts, const QKeySequence& key) {
    return ShortcutBinding{QString::fromLatin1(id), QString::fromLatin1(id), contexts, key, key, {}, false};
  };
  const QKeySequence m(Qt::Key_M);
  ShortcutBinding model = make("solid.move", ModelShortcutContext, m);
  ShortcutBinding sketch = make("sketch.move", SketchShortcutContext, m);
  CHECK(shortcutConflicts({model, sketch}).isEmpty());
  // Extrude/Revolve's profile selection makes the model action available
  // in both states; a Model label alone does not make it exclusive.
  ShortcutBinding profile = make("solid.extrude", ModelShortcutContext | SketchShortcutContext,
                                QKeySequence(Qt::Key_E));
  profile.primary = m;
  CHECK(shortcutConflicts({profile, sketch}).size() == 1);
  model.contexts |= CommandShortcutContext;
  sketch.contexts |= CommandShortcutContext;
  CHECK(shortcutConflicts({model, sketch}).size() == 1);

  ShortcutBinding fit = make("view.fit", AllShortcutContexts, QKeySequence(Qt::CTRL | Qt::Key_0));
  fit.alternates = {fit.primary, QKeySequence(Qt::Key_F6), QKeySequence(Qt::Key_F6)};
  CHECK(fit.effectiveKeys().size() == 2); // same-command aliases are harmless
  const auto f6 = make("solid.other", ModelShortcutContext, QKeySequence(Qt::Key_F6));
  CHECK(shortcutConflicts({fit, f6}).size() == 1);
  fit.primary = QKeySequence(Qt::CTRL | Qt::Key_9);
  CHECK(shortcutConflicts({fit, f6}).isEmpty()); // proposed primary retires F6
  fit.primary = QKeySequence();
  CHECK(fit.effectiveKeys().isEmpty()); // clearing also removes alternatives
  fit.primary = fit.defaultPrimary;
  CHECK(fit.effectiveKeys().size() == 2); // restoring brings them back
  ShortcutBinding alias = fit;
  alias.primary = QKeySequence(Qt::Key_F6);
  CHECK(shortcutConflicts({fit, alias}).isEmpty());

  const auto fixed = fixedShortcutBindings();
  for (const auto& reserved : fixed) {
    ShortcutBinding custom = make("custom.action", AllShortcutContexts, QKeySequence());
    custom.primary = reserved.primary;
    const auto conflicts = shortcutConflicts({reserved, custom});
    CHECK(conflicts.size() == 1);
    CHECK(conflictingShortcutIds(conflicts) == QSet<QString>{custom.id});
    CHECK(conflicts.front().description().contains(reserved.name));
    custom.id = reserved.id;
    CHECK(shortcutConflicts({reserved, custom}).isEmpty());
    // The names users see (mitcad#730), not where the key works.
    CHECK(!reserved.name.contains(QLatin1Char('(')) && !reserved.name.contains(QLatin1Char('/')));
  }
  const auto reservedName = [&fixed](const char* id) {
    for (const auto& binding : fixed) {
      if (binding.id == QLatin1String(id)) return binding.name;
    }
    return QString();
  };
  CHECK(reservedName("view.search_timeline") == QStringLiteral("Search Timeline"));
  CHECK(reservedName("browser.rename") == QStringLiteral("Rename"));
  CHECK(reservedName("browser.delete") == QStringLiteral("Delete"));
  CHECK(reservedName("command.accept") == QStringLiteral("OK"));
  CHECK(reservedName("command.cancel") == QStringLiteral("Cancel"));
  {
    ShortcutBinding findKey = make("custom.find", ModelShortcutContext, QKeySequence(QKeySequence::Find));
    QList<ShortcutBinding> withFind = fixed;
    withFind.append(findKey);
    const auto conflicts = shortcutConflicts(withFind);
    CHECK(conflicts.size() == 1);
    CHECK(!conflicts.isEmpty() &&
          conflicts.front().description().contains(QStringLiteral(" is reserved for Search Timeline. ")));
  }
  // Keyboard Shortcuts lists the reserved keys as read-only rows, one per
  // name with all its keys (mitcad#730): Ctrl+F under Search Timeline.
  {
    const QList<ReservedShortcut> rows = reservedShortcuts();
    QSet<QString> ids;
    qsizetype keyCount = 0;
    for (const ReservedShortcut& row : rows) {
      CHECK(!ids.contains(row.id));
      ids.insert(row.id);
      CHECK(!row.keys.isEmpty() && row.contexts != 0);
      keyCount += row.keys.size();
    }
    CHECK(keyCount == fixed.size());
    const auto rowOf = [&rows](const char* id) {
      for (const ReservedShortcut& row : rows) {
        if (row.id == QLatin1String(id)) return row;
      }
      return ReservedShortcut{};
    };
    const ReservedShortcut timeline = rowOf("view.search_timeline");
    CHECK(timeline.name == QStringLiteral("Search Timeline"));
    CHECK(timeline.keys == QList<QKeySequence>{QKeySequence(QKeySequence::Find)});
    CHECK(timeline.contexts == unsigned(AllShortcutContexts));
    const ReservedShortcut ok = rowOf("command.accept");
    CHECK(ok.name == QStringLiteral("OK"));
    CHECK((ok.keys == QList<QKeySequence>{QKeySequence(Qt::Key_Return), QKeySequence(Qt::Key_Enter)}));
    CHECK(rowOf("browser.rename").contexts == unsigned(ModelShortcutContext));
    // Ctrl+F, F2 and Delete work while the Browser or the timeline has the
    // keyboard focus; OK and Save everywhere.
    CHECK(timeline.browserFocus && rowOf("browser.rename").browserFocus && rowOf("browser.delete").browserFocus);
    CHECK(!ok.browserFocus && !rowOf("file.save").browserFocus);
#ifdef Q_OS_MACOS
    CHECK(rowOf("browser.delete").keys.size() == 2);
#else
    CHECK(rowOf("browser.delete").keys.size() == 1);
#endif
  }

  ShortcutBinding removeSketch = make("sketch.delete", SketchShortcutContext, QKeySequence(Qt::Key_Delete));
#ifdef Q_OS_MACOS
  removeSketch.alternates = {QKeySequence(Qt::Key_Backspace)};
#endif
  QList<ShortcutBinding> defaults = fixed;
  defaults.append(removeSketch);
  CHECK(shortcutConflicts(defaults).isEmpty());
  // Cut, Copy and Paste of sketch geometry take the reserved keys in a
  // sketch (mitcad#788); a command of the model cannot.
  {
    QList<ShortcutBinding> withClipboard = fixed;
    withClipboard.append(make("sketch.cut", SketchShortcutContext, QKeySequence(QKeySequence::Cut)));
    withClipboard.append(make("sketch.copy", SketchShortcutContext, QKeySequence(QKeySequence::Copy)));
    withClipboard.append(make("sketch.paste", SketchShortcutContext, QKeySequence(QKeySequence::Paste)));
    CHECK(shortcutConflicts(withClipboard).isEmpty());
    withClipboard.append(make("solid.copy", ModelShortcutContext, QKeySequence(QKeySequence::Copy)));
    CHECK(shortcutConflicts(withClipboard).size() == 1);
  }
  // Create Sketch's Alt+S (mitcad#267) beside the reserved keys and the
  // S keys of the registry's defaults: Command Search (S) and Save with
  // Description (Ctrl+Alt+S). The whole registry's defaults are checked by
  // the UI tests (tools/ui-command-test.sh: no "Conflicting saved
  // shortcuts" line, Alt+S starts Create Sketch).
  {
    QList<ShortcutBinding> withSketch = defaults;
    withSketch.append(make("sketch.create", ModelShortcutContext, QKeySequence(Qt::ALT | Qt::Key_S)));
    withSketch.append(make("tools.search", AllShortcutContexts, QKeySequence(Qt::Key_S)));
    withSketch.append(make("file.save_version", AllShortcutContexts,
                           QKeySequence(Qt::CTRL | Qt::ALT | Qt::Key_S)));
    CHECK(shortcutConflicts(withSketch).isEmpty());
    CHECK(QKeySequence(Qt::ALT | Qt::Key_S).toString(QKeySequence::PortableText) == QStringLiteral("Alt+S"));
  }
  removeSketch.contexts = ModelShortcutContext;
  defaults.back() = removeSketch;
  CHECK(!shortcutConflicts(defaults).isEmpty());

  // A legacy custom assignment conflicting with a default is disabled,
  // but remains persisted/editable and does not change the command list.
  QTemporaryDir directory;
  CHECK(directory.isValid());
  QSettings settings(directory.filePath(QStringLiteral("shortcuts.ini")), QSettings::IniFormat);
  const QString setting = QStringLiteral("shortcuts/solid.legacy");
  settings.setValue(setting, QStringLiteral("F6"));
  settings.sync();
  QSettings reloaded(settings.fileName(), QSettings::IniFormat);
  ShortcutBinding legacy = make("solid.legacy", ModelShortcutContext, QKeySequence(Qt::Key_K));
  legacy.primary = QKeySequence(reloaded.value(setting).toString(), QKeySequence::PortableText);
  const QList<ShortcutBinding> bindings{fit, legacy};
  const auto disabled = conflictingShortcutIds(shortcutConflicts(bindings));
  CHECK(disabled == QSet<QString>{legacy.id});
  CHECK(bindings.size() == 2);
  CHECK(reloaded.value(setting).toString() == QStringLiteral("F6"));
  legacy.primary = legacy.defaultPrimary;
  CHECK(shortcutConflicts({fit, legacy}).isEmpty());
  ShortcutBinding another = make("solid.another", ModelShortcutContext, QKeySequence(Qt::Key_J));
  another.primary = legacy.primary = QKeySequence(Qt::Key_B);
  CHECK(conflictingShortcutIds(shortcutConflicts({legacy, another})).size() == 2);
}

// Keys per context (mitcad#720): a command of several contexts (Command
// Search) has its own key in each, so S can draw a spline in sketches and
// still search in the model; a single key saved before reads as the key of
// every context. The registry reads and writes the settings of the test's
// own.
void shortcutContextTests() {
  using namespace mitcad;
  QTemporaryDir directory;
  CHECK(directory.isValid());
  const QString organization = QCoreApplication::organizationName();
  const QString application = QCoreApplication::applicationName();
  const QSettings::Format format = QSettings::defaultFormat();
  QCoreApplication::setOrganizationName(QStringLiteral("MitcadShortcutContextTest"));
  QCoreApplication::setApplicationName(QStringLiteral("MitcadShortcutContextTest"));
  QSettings::setDefaultFormat(QSettings::IniFormat);
  QSettings::setPath(QSettings::IniFormat, QSettings::UserScope, directory.path());
  {
    QSettings settings;
    settings.clear();
    settings.sync();
  }
  const auto command = [](const char* id, CommandDef::Mode mode, const QKeySequence& key, bool during) {
    CommandDef def;
    def.id = QString::fromLatin1(id);
    def.name = def.id;
    def.kind = CommandDef::Kind::Action;
    def.mode = mode;
    def.shortcut = key;
    def.duringCommands = during;
    return def;
  };
  const QString search = QStringLiteral("tools.search");
  const QString spline = QStringLiteral("sketch.spline");
  const QString create = QStringLiteral("sketch.create");
  const QKeySequence s(Qt::Key_S);
  const QKeySequence f3(Qt::Key_F3);
  const QKeySequence altS(Qt::ALT | Qt::Key_S);
  {
    CommandRegistry registry;
    registry.add(command("tools.search", CommandDef::Mode::Any, s, true));
    registry.add(command("sketch.spline", CommandDef::Mode::Sketch, QKeySequence(), false));
    registry.add(command("sketch.create", CommandDef::Mode::Model, altS, false));
    CHECK(registry.shortcutContexts(search) == AllShortcutContexts);
    CHECK(registry.shortcutContexts(spline) == SketchShortcutContext);
    CHECK(registry.shortcutContexts(create) == ModelShortcutContext);
    CHECK(registry.defaultShortcutConflicts().isEmpty());
    const ContextShortcuts defaults = registry.shortcuts(search);
    CHECK(defaults.size() == 3 && defaults.value(ModelShortcutContext) == s &&
          defaults.value(SketchShortcutContext) == s && defaults.value(CommandShortcutContext) == s);

    // S for the spline meets Command Search in sketches only...
    QHash<QString, ContextShortcuts> proposal;
    proposal[spline].insert(SketchShortcutContext, s);
    QList<ShortcutConflict> conflicts = registry.shortcutConflicts(proposal);
    CHECK(conflicts.size() == 1);
    CHECK(!conflicts.isEmpty() && conflicts.front().contexts == SketchShortcutContext &&
          conflicts.front().description() ==
              QStringLiteral("S is used by both tools.search and sketch.spline in a sketch. Change one of them."));
    // ...and is free once Command Search has another key there; S stays
    // its key in Model and Command Panel.
    proposal[search].insert(SketchShortcutContext, f3);
    CHECK(registry.shortcutConflicts(proposal).isEmpty());
    // Create Sketch on S then meets Command Search in the model...
    proposal[create].insert(ModelShortcutContext, s);
    conflicts = registry.shortcutConflicts(proposal);
    CHECK(conflicts.size() == 1);
    CHECK(!conflicts.isEmpty() && conflicts.front().contexts == ModelShortcutContext &&
          conflicts.front().description().contains(QStringLiteral(" in the model. ")));
    // Contexts in a sentence, and their names.
    CHECK(shortcutContextPhrase(ModelShortcutContext | SketchShortcutContext) ==
          QStringLiteral("in the model and in a sketch"));
    CHECK(shortcutContextPhrase(AllShortcutContexts) ==
          QStringLiteral("in the model, in a sketch and while a command panel is open"));
    CHECK(shortcutContextName(CommandShortcutContext) == QStringLiteral("Command Panel"));
    CHECK(shortcutContextName(ModelShortcutContext | SketchShortcutContext) == QStringLiteral("Model, Sketch"));
    // ...unless Command Search moves there too.
    proposal[search].insert(ModelShortcutContext, QKeySequence(Qt::CTRL | Qt::Key_K));
    CHECK(registry.shortcutConflicts(proposal).isEmpty());

    // Saved: the key of one context where they differ, one key for all.
    proposal.remove(create);
    proposal[search].remove(ModelShortcutContext);
    registry.setShortcuts(proposal);
    QSettings settings;
    CHECK(settings.value(QStringLiteral("shortcuts.sketch/tools.search")).toString() == QStringLiteral("F3"));
    CHECK(!settings.contains(QStringLiteral("shortcuts/tools.search")));
    CHECK(!settings.contains(QStringLiteral("shortcuts.model/tools.search"))); // the default
    CHECK(!settings.contains(QStringLiteral("shortcuts.commands/tools.search")));
    CHECK(settings.value(QStringLiteral("shortcuts/sketch.spline")).toString() == QStringLiteral("S"));
    CHECK(!settings.contains(QStringLiteral("shortcuts.sketch/sketch.spline")));
    CHECK(registry.shortcut(search, ModelShortcutContext) == s);
    CHECK(registry.shortcut(search, SketchShortcutContext) == f3);
    CHECK(registry.shortcut(search, CommandShortcutContext) == s);
    CHECK(registry.shortcutConflicts().isEmpty());
    CHECK(registry.shortcutWarnings().isEmpty());

    // The actions' keys follow the current context.
    CHECK(registry.shortcutContext() == ModelShortcutContext);
    CHECK(registry.effectiveShortcuts(search) == QList<QKeySequence>{s});
    registry.setShortcutContext(SketchShortcutContext);
    CHECK(registry.effectiveShortcuts(search) == QList<QKeySequence>{f3});
    CHECK(registry.shortcut(search) == f3);
    CHECK(registry.shortcutHint(search).contains(f3.toString(QKeySequence::NativeText)));
    CHECK(registry.effectiveShortcuts(spline) == QList<QKeySequence>{s});
    registry.setShortcutContext(CommandShortcutContext);
    CHECK(registry.effectiveShortcuts(search) == QList<QKeySequence>{s});
    // A command outside the current context keeps its own context's key.
    CHECK(registry.effectiveShortcuts(spline) == QList<QKeySequence>{s});
    CHECK(registry.effectiveShortcuts(create) == QList<QKeySequence>{altS});
    registry.setShortcutContext(ModelShortcutContext);

    // One key in every context again, the default: nothing saved.
    registry.setShortcut(search, s);
    CHECK(!settings.contains(QStringLiteral("shortcuts.sketch/tools.search")));
    CHECK(!settings.contains(QStringLiteral("shortcuts/tools.search")));
    // One context alone: cleared during commands.
    registry.setShortcut(search, CommandShortcutContext, QKeySequence());
    CHECK(settings.contains(QStringLiteral("shortcuts.commands/tools.search")));
    CHECK(registry.shortcut(search, CommandShortcutContext).isEmpty());
    CHECK(registry.shortcut(search, ModelShortcutContext) == s);
    CHECK(registry.shortcut(search, SketchShortcutContext) == s);
  }
  {
    // A single key saved before mitcad#720 is the key of every context.
    QSettings settings;
    settings.clear();
    settings.setValue(QStringLiteral("shortcuts/tools.search"), QStringLiteral("F3"));
    settings.sync();
    CommandRegistry registry;
    registry.add(command("tools.search", CommandDef::Mode::Any, s, true));
    registry.add(command("sketch.create", CommandDef::Mode::Model, altS, false));
    for (const unsigned context : shortcutContextList(AllShortcutContexts)) {
      CHECK(registry.shortcut(search, context) == f3);
    }
    CHECK(registry.shortcuts(search).size() == 3);
    // A context's own key wins there.
    settings.setValue(QStringLiteral("shortcuts.sketch/tools.search"), QStringLiteral("Q"));
    CHECK(registry.shortcut(search, SketchShortcutContext) == QKeySequence(Qt::Key_Q));
    CHECK(registry.shortcut(search, ModelShortcutContext) == f3);
    // The key of a context a command does not live in is not read.
    settings.setValue(QStringLiteral("shortcuts.sketch/sketch.create"), QStringLiteral("S"));
    CHECK(registry.shortcuts(create).size() == 1 && registry.shortcuts(create).value(ModelShortcutContext) == altS);
    settings.clear();
    settings.sync();
  }
  QCoreApplication::setOrganizationName(organization);
  QCoreApplication::setApplicationName(application);
  QSettings::setDefaultFormat(format);
}

// Numbers typed with a decimal comma or point (mitcad#2).
void numberTests() {
  using mitcad::parsePlainNumber;
  CHECK(parsePlainNumber(QStringLiteral("12.5")) == 12.5);
  CHECK(parsePlainNumber(QStringLiteral("12,5")) == 12.5);
  CHECK(parsePlainNumber(QStringLiteral(" ,5 ")) == 0.5);
  CHECK(parsePlainNumber(QStringLiteral("5,")) == 5.0);
  CHECK(parsePlainNumber(QStringLiteral("-2,5e3")) == -2500.0);
  CHECK(parsePlainNumber(QStringLiteral("20")) == 20.0);
  // No thousands separators: as the model reads it.
  CHECK(parsePlainNumber(QStringLiteral("1,000")) == 1.0);
  // Expressions and the like are no plain numbers.
  for (const char* text : {"", "d1 * 2", "20 mm", "1,5 mm", "1,000.5", "1,2,3", "max(1, 5)", "inf", "nan"}) {
    if (parsePlainNumber(QString::fromLatin1(text))) {
      std::fprintf(stderr, "'%s' is no plain number\n", text);
      ++failures;
    }
  }
  QString normalized;
  CHECK(parsePlainNumber(QStringLiteral(" 12,5"), &normalized) && normalized == QStringLiteral("12.5"));

  mitcad::DecimalSpinBox box;
  box.setRange(0.0, 1000.0);
  box.setDecimals(2);
  box.setSuffix(QStringLiteral(" mm"));
  QString typed = QStringLiteral("12,5 mm");
  int position = 4;
  CHECK(box.validate(typed, position) == QValidator::Acceptable);
  CHECK(typed == QStringLiteral("12.5 mm"));
  CHECK(box.valueFromText(QStringLiteral("12,5 mm")) == 12.5);
  CHECK(box.valueFromText(QStringLiteral("12.5 mm")) == 12.5);
  CHECK(box.textFromValue(1234.5) == QStringLiteral("1234.50"));
}

// Pixels of `image` in `rect` that are not transparent.
int opaquePixels(const QImage& image, const QRect& rect) {
  int count = 0;
  const QRect inside = rect.intersected(image.rect());
  for (int y = inside.top(); y <= inside.bottom(); ++y) {
    for (int x = inside.left(); x <= inside.right(); ++x) {
      count += image.pixelColor(x, y).alpha() > 0 ? 1 : 0;
    }
  }
  return count;
}

bool closeTo(double a, double b) { return std::abs(a - b) <= 1.0; }

// The sketch tools' precision cursor (mitcad#3), at 100, 150 and 200 %.
void cursorTests() {
  const QString line = QStringLiteral("line");
  for (const qreal dpr : {1.0, 1.5, 2.0}) {
    std::fprintf(stderr, "cursor at ratio %g\n", dpr);
    const mitcad::PrecisionCursorLayout layout = mitcad::precisionCursorLayout(dpr);
    const QPixmap pixmap = mitcad::precisionCursorPixmap(line, dpr, false);
    const QImage image = pixmap.toImage().convertToFormat(QImage::Format_ARGB32);
    // 32 x 32 logical pixels, the hot spot in the middle of the cross.
    CHECK(pixmap.devicePixelRatio() == dpr);
    CHECK(pixmap.width() == layout.size && pixmap.height() == layout.size);
    CHECK(layout.size == qRound(32 * dpr));
    CHECK(pixmap.deviceIndependentSize() == QSizeF(32, 32));
    CHECK(layout.hotSpot == QPoint(10, 10));
    const QPoint hot(qRound(layout.hotSpot.x() * dpr), qRound(layout.hotSpot.y() * dpr));
    CHECK(layout.gap.contains(hot) && layout.arms.contains(hot));
    // The cross: 19 logical pixels of arms, 21 with the halo, around the
    // hot spot; the 5 px gap in its middle is empty.
    CHECK(closeTo(layout.arms.width(), 21 * dpr) && layout.arms.height() == layout.arms.width());
    CHECK(closeTo(layout.arms.center().x(), hot.x()) && closeTo(layout.arms.center().y(), hot.y()));
    CHECK(closeTo(layout.gap.width(), 5 * dpr) && layout.gap.height() == layout.gap.width());
    CHECK(closeTo(layout.gap.center().x(), hot.x()) && closeTo(layout.gap.center().y(), hot.y()));
    CHECK(opaquePixels(image, layout.gap) == 0);
    // Whole pixels: a dark arm, its light halo beside it, nothing between.
    const int width = std::max(1, static_cast<int>(dpr)); // the arms' width
    const QColor arm = image.pixelColor(layout.arms.left() + width, hot.y());
    const QColor halo = image.pixelColor(layout.arms.left() + width, hot.y() - 1);
    CHECK(arm == QColor(0x1b, 0x1e, 0x23));
    CHECK(halo == QColor(255, 255, 255, 217));
    CHECK(image.pixelColor(layout.arms.left(), hot.y()) == halo);
    CHECK(image.pixelColor(layout.arms.left() + width, hot.y() + width) == halo);
    CHECK(image.pixelColor(layout.arms.left() + width, hot.y() - width - 1).alpha() == 0);
    // The badge: 12 px, 10 px right of and below the hot spot, clear of
    // the gap and the arms, the tool's icon in it.
    CHECK(closeTo(layout.badge.width(), 12 * dpr) && layout.badge.height() == layout.badge.width());
    CHECK(closeTo(layout.badge.left() - hot.x(), 10 * dpr) && closeTo(layout.badge.top() - hot.y(), 10 * dpr));
    CHECK(!layout.badge.intersects(layout.gap));
    CHECK(layout.badge.left() > hot.x() + 2 * width && layout.badge.top() > hot.y() + 2 * width);
    CHECK(image.rect().contains(layout.badge));
    CHECK(opaquePixels(image, layout.badge) > layout.badge.width() * layout.badge.height() / 2);
    const QImage bare = mitcad::precisionCursorPixmap(QString(), dpr, false).toImage();
    CHECK(opaquePixels(bare, layout.badge) == 0);
    // Snapping: a hollow square in the gap; the pick point stays visible.
    const QImage snapped =
        mitcad::precisionCursorPixmap(line, dpr, true).toImage().convertToFormat(QImage::Format_ARGB32);
    CHECK(snapped.pixelColor(layout.gap.topLeft()) == arm);
    CHECK(snapped.pixelColor(layout.gap.bottomRight()) == arm);
    CHECK(snapped.pixelColor(hot).alpha() == 0);
    CHECK(opaquePixels(snapped, layout.gap.adjusted(width, width, -width, -width)) == 0);
    // The cursor: a bitmap cursor with that hot spot, built once.
    const QCursor cursor = mitcad::precisionCursor(line, dpr);
    CHECK(cursor.shape() == Qt::BitmapCursor);
    CHECK(cursor.hotSpot() == mitcad::precisionCursorLayout(dpr, mitcad::pointerScale()).hotSpot);
    CHECK(cursor.pixmap().cacheKey() == mitcad::precisionCursor(line, dpr).pixmap().cacheKey());
    CHECK(cursor.pixmap().cacheKey() != mitcad::precisionCursor(line, dpr, true).pixmap().cacheKey());
  }
  // A larger system pointer makes a larger cursor.
  const mitcad::PrecisionCursorLayout large = mitcad::precisionCursorLayout(1.0, 1.5);
  CHECK(large.size == 48 && large.hotSpot == QPoint(15, 15));
  CHECK(mitcad::pointerScale() >= 1.0 && mitcad::pointerScale() <= 2.0);
}

// An empty file, executable.
void touch(const QString& path) {
  QDir().mkpath(QFileInfo(path).path());
  QFile file(path);
  CHECK(file.open(QIODevice::WriteOnly));
  file.write("#!/bin/sh\n");
  file.close();
  file.setPermissions(file.permissions() | QFile::ExeOwner | QFile::ExeUser);
}

// 3D Print (mitcad#13): the slicers found in their install folders, on
// PATH, as Flatpaks and in desktop files; file and folder names.
void printTests() {
  using mitcad::Slicer;
  QTemporaryDir root;
  CHECK(root.isValid());
  const QString dir = root.path();
  // Install folders (Windows' layout; the search does not care where).
  touch(dir + QStringLiteral("/programs/Bambu Studio/bambu-studio.exe"));
  touch(dir + QStringLiteral("/programs/Prusa3D/PrusaSlicer/prusa-slicer.exe"));
  touch(dir + QStringLiteral("/programs/UltiMaker Cura 5.7.0/UltiMaker-Cura.exe"));
  touch(dir + QStringLiteral("/programs/UltiMaker Cura 5.8.1/UltiMaker-Cura.exe"));
  // PATH, a Flatpak and the desktop file of an AppImage.
#ifdef _WIN32
  touch(dir + QStringLiteral("/bin/orca-slicer.exe"));
  touch(dir + QStringLiteral("/bin/flatpak.exe"));
#else
  touch(dir + QStringLiteral("/bin/orca-slicer"));
  touch(dir + QStringLiteral("/bin/flatpak"));
#endif
  QDir().mkpath(dir + QStringLiteral("/flatpak/com.bambulab.BambuStudio/current"));
  touch(dir + QStringLiteral("/apps/OrcaSlicer.AppImage"));
  QDir().mkpath(dir + QStringLiteral("/applications"));
  QFile desktop(dir + QStringLiteral("/applications/orca.desktop"));
  CHECK(desktop.open(QIODevice::WriteOnly));
  desktop.write(QStringLiteral("[Desktop Entry]\nName=OrcaSlicer\nExec=\"%1\" --single-instance %U\n"
                               "[Desktop Action New]\nName=Other\nExec=other\n")
                    .arg(dir + QStringLiteral("/apps/OrcaSlicer.AppImage"))
                    .toUtf8());
  desktop.close();
  mitcad::SlicerSearch search;
  search.programDirectories = {dir + QStringLiteral("/programs")};
  search.path = {dir + QStringLiteral("/bin")};
  search.flatpakDirectories = {dir + QStringLiteral("/flatpak")};
  search.applicationDirectories = {dir + QStringLiteral("/applications")};
  const QVector<Slicer> found = mitcad::findSlicers(search);
  QStringList described;
  for (const Slicer& slicer : found) {
    described << slicer.describe();
    std::fprintf(stderr, "slicer: %s\n", qPrintable(slicer.describe()));
  }
  CHECK(found.size() == 6);
  if (found.size() == 6) {
    const auto native = [&dir](const char* path) { return QDir::toNativeSeparators(dir + QLatin1String(path)); };
    CHECK(found[0].name == QStringLiteral("Bambu Studio"));
    CHECK(found[0].program == native("/programs/Bambu Studio/bambu-studio.exe"));
    CHECK(found[1].name == QStringLiteral("OrcaSlicer") && found[1].arguments.isEmpty());
    CHECK(found[2].program == native("/programs/Prusa3D/PrusaSlicer/prusa-slicer.exe"));
    // The newest version of a folder that has one.
    CHECK(found[3].program == native("/programs/UltiMaker Cura 5.8.1/UltiMaker-Cura.exe"));
    CHECK(found[4].name == QStringLiteral("Bambu Studio (Flatpak)"));
    CHECK(found[4].arguments == QStringList({QStringLiteral("run"), QStringLiteral("--file-forwarding"),
                                             QStringLiteral("com.bambulab.BambuStudio"), QStringLiteral("@@")}));
    CHECK(found[4].argumentsAfter == QStringList{QStringLiteral("@@")});
    CHECK(found[5].program == native("/apps/OrcaSlicer.AppImage"));
    CHECK(found[5].arguments == QStringList{QStringLiteral("--single-instance")});
  }
  CHECK(mitcad::findSlicers(mitcad::SlicerSearch{}).isEmpty());
  // QIDI Studio, and each slicer's dialect of print settings (mitcad#640).
  touch(dir + QStringLiteral("/qidi/QIDIStudio/qidi-studio.exe"));
  mitcad::SlicerSearch qidi;
  qidi.programDirectories = {dir + QStringLiteral("/qidi")};
  const QVector<Slicer> qidiFound = mitcad::findSlicers(qidi);
  CHECK(qidiFound.size() == 1 && qidiFound.first().name == QStringLiteral("QIDI Studio"));
  CHECK(!qidiFound.isEmpty() && mitcad::slicerDialect(qidiFound.first()) == QStringLiteral("qidi_studio"));
  if (found.size() == 6) {
    CHECK(mitcad::slicerDialect(found[0]) == QStringLiteral("bambu_studio"));
    CHECK(mitcad::slicerDialect(found[1]) == QStringLiteral("orcaslicer"));
    CHECK(mitcad::slicerDialect(found[2]) == QStringLiteral("prusaslicer"));
    CHECK(mitcad::slicerDialect(found[3]) == QStringLiteral("cura"));
  }
  CHECK(mitcad::slicerDialect(Slicer{QStringLiteral("viewer"), QStringLiteral("/usr/bin/viewer"), {}, {}}).isEmpty());

  // Application bundles (macOS): started with `open -a`, the bundle shown.
  QDir().mkpath(dir + QStringLiteral("/Applications/OrcaSlicer.app/Contents"));
  QDir().mkpath(dir + QStringLiteral("/Applications/Original Prusa Drivers/PrusaSlicer.app"));
  touch(dir + QStringLiteral("/Applications/BambuStudio.app")); // a file, not a bundle
  mitcad::SlicerSearch bundles;
  bundles.bundleDirectories = {dir + QStringLiteral("/Applications")};
  const QVector<Slicer> apps = mitcad::findSlicers(bundles);
  CHECK(apps.size() == 2);
  if (apps.size() == 2) {
    const QString orca = dir + QStringLiteral("/Applications/OrcaSlicer.app");
    CHECK(apps[0].name == QStringLiteral("OrcaSlicer"));
    CHECK(apps[0].program == QStringLiteral("/usr/bin/open"));
    CHECK(apps[0].arguments == QStringList({QStringLiteral("-a"), orca}));
    CHECK(apps[0].location() == orca);
    CHECK(apps[1].location() == dir + QStringLiteral("/Applications/Original Prusa Drivers/PrusaSlicer.app"));
  }
  CHECK(found.isEmpty() || found[0].location() == found[0].program);

  CHECK(mitcad::safeFileName(QStringLiteral("Body1")) == QStringLiteral("Body1"));
  CHECK(mitcad::safeFileName(QStringLiteral("a/b\\c:d*e?\"<>|")) == QStringLiteral("a_b_c_d_e_____"));
  CHECK(mitcad::safeFileName(QStringLiteral(" Lid. ")) == QStringLiteral("Lid"));
  CHECK(mitcad::safeFileName(QStringLiteral("con")) == QStringLiteral("_con"));
  CHECK(mitcad::safeFileName(QStringLiteral("COM1.part")) == QStringLiteral("_COM1.part"));
  CHECK(mitcad::safeFileName(QStringLiteral("Console")) == QStringLiteral("Console"));
  CHECK(mitcad::safeFileName(QStringLiteral("..")) == QStringLiteral("Body"));
  CHECK(mitcad::printFileNames({QStringLiteral("Body1"), QStringLiteral("body1"), QStringLiteral("Body1"),
                                QStringLiteral("Pin/2")},
                               QStringLiteral("stl")) ==
        QStringList({QStringLiteral("Body1.stl"), QStringLiteral("body1_2.stl"), QStringLiteral("Body1_3.stl"),
                     QStringLiteral("Pin_2.stl")}));

  // STL files (mitcad#17): a body placed once one file, a body shown twice
  // one per occurrence, a hidden placement left out unless none is shown.
  const auto instance = [](const char* body, QStringList path, const char* occurrence, bool visible) {
    return QJsonObject{{QStringLiteral("body"), QString::fromLatin1(body)},
                       {QStringLiteral("path"), QJsonArray::fromStringList(path)},
                       {QStringLiteral("occurrence"), QString::fromLatin1(occurrence)},
                       {QStringLiteral("visible"), visible}};
  };
  const QJsonArray instances{instance("F1.b0", {}, "", true),
                             instance("F2.b0", {QStringLiteral("O1")}, "Arm:1", true),
                             instance("F2.b0", {QStringLiteral("O2"), QStringLiteral("O4")}, "Base:1/Arm:2", true),
                             instance("F2.b0", {QStringLiteral("O3")}, "Arm:3", false),
                             instance("F3.b0", {QStringLiteral("O5")}, "Lid:1", false),
                             instance("F3.b0", {QStringLiteral("O6")}, "Lid:2", false)};
  const QVector<mitcad::PrintPiece> pieces =
      mitcad::printPieces({QStringLiteral("F1.b0"), QStringLiteral("F2.b0"), QStringLiteral("F3.b0")},
                          {QStringLiteral("Plate"), QStringLiteral("Pin"), QStringLiteral("Lid")}, instances);
  QStringList files;
  for (const mitcad::PrintPiece& piece : pieces) {
    files << QStringLiteral("%1 %2 %3").arg(piece.body, piece.occurrence, piece.name);
  }
  CHECK(files == QStringList({QStringLiteral("F1.b0  Plate"), QStringLiteral("F2.b0 O1 Pin (Arm:1)"),
                                  QStringLiteral("F2.b0 O2/O4 Pin (Base:1/Arm:2)"),
                                  QStringLiteral("F3.b0 O5 Lid (Lid:1)"), QStringLiteral("F3.b0 O6 Lid (Lid:2)")}));
  // The placements picked in the panel (mitcad#234): of a body shown twice
  // only the one picked; a body none of whose placements is picked keeps
  // them all.
  const auto body = [](const char* uid, const char* occurrence) {
    return mitcad::SelectionItem{mitcad::SelectKind::Body, QString::fromLatin1(uid), QString(), QString(),
                                 QString::fromLatin1(occurrence)};
  };
  files.clear();
  for (const mitcad::PrintPiece& piece :
       mitcad::printPieces({QStringLiteral("F1.b0"), QStringLiteral("F2.b0"), QStringLiteral("F3.b0")},
                           {QStringLiteral("Plate"), QStringLiteral("Pin"), QStringLiteral("Lid")}, instances,
                           {body("F1.b0", ""), body("F2.b0", "O2/O4"), body("F3.b0", "O9")})) {
    files << QStringLiteral("%1 %2 %3").arg(piece.body, piece.occurrence, piece.name);
  }
  CHECK(files == QStringList({QStringLiteral("F1.b0  Plate"), QStringLiteral("F2.b0 O2/O4 Pin (Base:1/Arm:2)"),
                              QStringLiteral("F3.b0 O5 Lid (Lid:1)"), QStringLiteral("F3.b0 O6 Lid (Lid:2)")}));

  // The bodies the panel starts with: what the selection stands for, each
  // placement once, else every shown one.
  bool selected = true;
  CHECK(mitcad::printStartBodies({}, instances, &selected) ==
        mitcad::Selection({body("F1.b0", ""), body("F2.b0", "O1"), body("F2.b0", "O2/O4")}));
  CHECK(!selected);
  const mitcad::SelectionItem face{mitcad::SelectKind::Face, QStringLiteral("F2.b0"), QStringLiteral("F2:end"),
                                   QStringLiteral("plane"), QStringLiteral("O1")};
  const mitcad::SelectionItem base{mitcad::SelectKind::Component, QStringLiteral("C2"), QString(), QString(),
                                   QStringLiteral("O2")};
  const mitcad::SelectionItem sketch{mitcad::SelectKind::Sketch, QStringLiteral("F7"), QString(), QString()};
  CHECK(mitcad::printStartBodies({face, body("F2.b0", "O1"), base, sketch}, instances, &selected) ==
        mitcad::Selection({body("F2.b0", "O1"), body("F2.b0", "O2/O4")}));
  CHECK(selected);
  CHECK(mitcad::printStartBodies({sketch}, instances, &selected).size() == 3);
  CHECK(!selected);

  // The pieces with print bed faces to choose from (mitcad#857): two or
  // more choices, the file's own object first; the key does not depend on
  // the order of the faces.
  const auto choice = [](const char* uid, const char* occurrence) {
    return QJsonObject{{QStringLiteral("body"), QString::fromLatin1(uid)},
                       {QStringLiteral("name"), QStringLiteral("Body")},
                       {QStringLiteral("face"), QStringLiteral("F1:top")},
                       {QStringLiteral("occurrence"), QString::fromLatin1(occurrence)},
                       {QStringLiteral("occurrence_name"), QStringLiteral("Pair:1")}};
  };
  const QJsonObject planned{
      {QStringLiteral("choices"), QJsonArray{choice("F1.b0", "O1"), choice("F2.b0", "O1")}},
      {QStringLiteral("objects"),
       QJsonArray{QJsonObject{{QStringLiteral("name"), QStringLiteral("Nut (Nuts:1/Nut:2)")},
                              {QStringLiteral("choices"), QJsonArray{choice("F3.b0", "O2")}}},
                  QJsonObject{{QStringLiteral("name"), QStringLiteral("Nuts:1")},
                              {QStringLiteral("choices"),
                               QJsonArray{choice("F3.b0", "O2/O4"), choice("F3.b0", "O2/O3")}}}}}};
  const QVector<mitcad::PrintBedAsk> asks = mitcad::printBedAsks(planned);
  CHECK(asks.size() == 2);
  CHECK(asks.value(0).piece.isEmpty() && asks.value(0).faces.size() == 2);
  CHECK(asks.value(0).key == QStringLiteral("|F1.b0@O1,F2.b0@O1"));
  CHECK(asks.value(1).key == QStringLiteral("Nuts:1|F3.b0@O2/O3,F3.b0@O2/O4"));
  CHECK(asks.value(1).faces.value(0).describe() == QStringLiteral("Body F1:top in Pair:1"));
  CHECK(mitcad::printBedAsks(QJsonObject()).isEmpty());
  // Its texts (mitcad#857 review): the question names the piece and the
  // count, the file's own object by the occurrence its faces share; a
  // body with faces in several places is named with the place.
  CHECK(asks.value(0).pieceName() == QStringLiteral("Pair:1"));
  CHECK(asks.value(1).pieceName() == QStringLiteral("Nuts:1"));
  CHECK(mitcad::printBedQuestion(asks.value(0)) ==
        QStringLiteral("Not sent yet: Pair:1 has 2 print bed faces. Click the green one to put on the bed."));
  CHECK(mitcad::printBedQuestion(asks.value(1)).startsWith(QStringLiteral("Not sent yet: Nuts:1 has 2 ")));
  CHECK(mitcad::printBedHover(asks.value(0), asks.value(0).faces.value(0)) == QStringLiteral("Body: print bed face"));
  CHECK(mitcad::printBedHover(asks.value(1), asks.value(1).faces.value(0)) ==
        QStringLiteral("Body (Pair:1): print bed face"));
  CHECK(mitcad::printBedChosen(asks.value(0), asks.value(0).faces.value(1)) ==
        QStringLiteral("Pair:1 on Body's print bed face."));
  CHECK(mitcad::printBedRefused() == QStringLiteral("Not one of the print bed faces: click a green face."));
  mitcad::PrintBedAsk apart;
  mitcad::PrintBedFace one;
  one.body = QStringLiteral("F1.b0");
  one.name = QStringLiteral("Tall");
  one.occurrenceName = QStringLiteral("Pair:1");
  mitcad::PrintBedFace second = one;
  second.body = QStringLiteral("F2.b0");
  second.occurrenceName = QStringLiteral("Pair:2");
  apart.faces = {one, second};
  CHECK(apart.pieceName().isEmpty());
  CHECK(mitcad::printBedQuestion(apart).startsWith(QStringLiteral("Not sent yet: the piece has 2 ")));
  CHECK(mitcad::printBedChosen(apart, one) == QStringLiteral("The piece on Tall's print bed face."));
  // The last line: the faces chosen, then the other placements on theirs.
  CHECK(mitcad::printBedLine(QStringLiteral("Sent."), 0) == QStringLiteral("Sent."));
  CHECK(mitcad::printBedLine(QStringLiteral("Sent."), 2) == QStringLiteral("Sent. 2 bodies on their print bed faces."));
  CHECK(mitcad::printBedLine(QStringLiteral("Sent."), 1, {QStringLiteral("Pair:1 on Tall's print bed face.")}) ==
        QStringLiteral("Sent. Pair:1 on Tall's print bed face."));
  CHECK(mitcad::printBedLine(QStringLiteral("Sent."), 2, {QStringLiteral("Pair:1 on Tall's print bed face.")}) ==
        QStringLiteral("Sent. Pair:1 on Tall's print bed face. 1 other body on its print bed face."));

  // The Slicer row: none, the settings' slicer and one chosen by hand
  // unless found, the found ones, another program; a choice is its slicer.
  const Slicer saved{QStringLiteral("Saved"), QStringLiteral("/opt/saved"), {QStringLiteral("--a")}, {}};
  const Slicer other{QStringLiteral("Other"), QStringLiteral("/opt/other"), {}, {QStringLiteral("@@")}};
  CHECK(mitcad::slicerOfChoice(mitcad::slicerChoice(other)) == other);
  CHECK(mitcad::slicerOfChoice(mitcad::slicerChoice(other)).name == QStringLiteral("Other"));
  CHECK(!mitcad::slicerOfChoice(mitcad::slicerChoice(Slicer{})).isValid());
  CHECK(!mitcad::slicerOfChoice(mitcad::otherSlicerChoice()).isValid());
  const QVector<QPair<QString, QString>> choices = mitcad::slicerChoices(found, saved, other);
  CHECK(choices.size() == found.size() + 4);
  if (choices.size() == found.size() + 4) {
    CHECK(choices.first().first == mitcad::slicerChoice(Slicer{}));
    CHECK(choices[1].first == mitcad::slicerChoice(saved));
    CHECK(choices[2].first == mitcad::slicerChoice(other));
    CHECK(found.isEmpty() || choices[3].first == mitcad::slicerChoice(found.first()));
    CHECK(choices.last().first == mitcad::otherSlicerChoice());
  }
  CHECK(found.isEmpty() || mitcad::slicerChoices(found, found.first(), Slicer{}).size() == found.size() + 2);

  // The folder of a design, emptied for each send.
  QString error;
  const QString folder = mitcad::preparePrintFolder(dir, QStringLiteral("My: part"), &error);
  CHECK(folder == QDir(dir).filePath(QStringLiteral("mitcad-print/My_ part")));
  touch(folder + QStringLiteral("/old.stl"));
  CHECK(mitcad::preparePrintFolder(dir, QStringLiteral("My: part"), &error) == folder);
  CHECK(QDir(folder).isEmpty());
#ifndef _WIN32
  // A link put in the shared temporary folder decides nothing.
  QTemporaryDir shared;
  touch(dir + QStringLiteral("/kept/part/file.txt"));
  CHECK(QFile::link(dir + QStringLiteral("/kept"), shared.path() + QStringLiteral("/mitcad-print")));
  error.clear();
  CHECK(mitcad::preparePrintFolder(shared.path(), QStringLiteral("part"), &error).isEmpty());
  CHECK(error.contains(QStringLiteral("not a folder of yours")));
  CHECK(QFileInfo::exists(dir + QStringLiteral("/kept/part/file.txt")));
#endif
}

// The AppImage's desktop file and icons in the user's data folder.
void desktopEntryTests() {
  // Exec runs the AppImage, quoted, with the specification's escapes.
  const QString bundled = QStringLiteral("[Desktop Entry]\nName=Mitcad\nExec=mitcad\nIcon=mitcad\n");
  CHECK(mitcad::desktopEntryFor(bundled, QStringLiteral("/home/a b/Mitcad.AppImage")) ==
        QStringLiteral("[Desktop Entry]\nName=Mitcad\nExec=\"/home/a b/Mitcad.AppImage\"\nIcon=mitcad\n"));
  CHECK(mitcad::desktopEntryFor(bundled, QStringLiteral("/x/$a\"b")) ==
        QStringLiteral("[Desktop Entry]\nName=Mitcad\nExec=\"/x/\\\\$a\\\\\"b\"\nIcon=mitcad\n"));

  QTemporaryDir temp;
  const QString appDir = temp.filePath(QStringLiteral("AppDir"));
  const QString data = temp.filePath(QStringLiteral("share"));
  const auto write = [](const QString& path, const QByteArray& content) {
    QDir().mkpath(QFileInfo(path).absolutePath());
    QFile file(path);
    return file.open(QIODevice::WriteOnly) && file.write(content) == content.size();
  };
  CHECK(write(appDir + QStringLiteral("/usr/share/applications/mitcad.desktop"), bundled.toUtf8()));
  CHECK(write(appDir + QStringLiteral("/usr/share/icons/hicolor/16x16/apps/mitcad.png"), "16"));
  CHECK(write(appDir + QStringLiteral("/usr/share/icons/hicolor/256x256/apps/mitcad.png"), "256"));
  const auto read = [](const QString& path) {
    QFile file(path);
    return file.open(QIODevice::ReadOnly) ? file.readAll() : QByteArray();
  };
  mitcad::DesktopIntegration result = mitcad::integrateAppImage(QStringLiteral("/apps/Mitcad.AppImage"), appDir, data);
  CHECK(result.error.isEmpty());
  CHECK(result.written.size() == 3);
  CHECK(result.written.last() == data + QStringLiteral("/applications/mitcad.desktop"));
  CHECK(read(data + QStringLiteral("/applications/mitcad.desktop")).contains("\nExec=\"/apps/Mitcad.AppImage\"\n"));
  CHECK(read(data + QStringLiteral("/icons/hicolor/256x256/apps/mitcad.png")) == "256");
  // Started again: nothing to write; the AppImage moved: only the desktop file.
  CHECK(mitcad::integrateAppImage(QStringLiteral("/apps/Mitcad.AppImage"), appDir, data).written.isEmpty());
  result = mitcad::integrateAppImage(QStringLiteral("/opt/Mitcad.AppImage"), appDir, data);
  CHECK(result.written == QStringList{data + QStringLiteral("/applications/mitcad.desktop")});
  // No AppDir: an error, nothing written.
  result = mitcad::integrateAppImage(QStringLiteral("/a"), temp.filePath(QStringLiteral("none")), data);
  CHECK(!result.error.isEmpty() && result.written.isEmpty());
}

// IEEE 754 binary16 of a float in the normal range (for frames).
std::uint16_t half(float value) {
  if (value == 0.0f) {
    return 0;
  }
  int exponent = 0;
  const float mantissa = std::frexp(value, &exponent); // value = mantissa * 2^exponent, 0.5 <= mantissa < 1
  const auto bits = static_cast<std::uint16_t>(std::lround((mantissa * 2.0f - 1.0f) * 1024.0f));
  return static_cast<std::uint16_t>(((exponent + 14) << 10) | bits);
}

// The rendered view's display (mitcad#47): exposure and view transforms
// change the render's light, not the background it is composited over.
void renderLookTests() {
  using mitcad::render::Look;
  using mitcad::render::ViewTransform;
  using mitcad::render::viewTransform;
  const auto near = [](float a, float b, float tolerance) { return std::abs(a - b) <= tolerance; };
  // Standard leaves the light as it is; the curves keep it in 0..1, keep
  // mid grey about where it was, and grow with the light up to white.
  CHECK(viewTransform({0.5f, 2.0f, 0.1f}, ViewTransform::Standard)[1] == 2.0f);
  for (const ViewTransform transform : {ViewTransform::Filmic, ViewTransform::Neutral}) {
    float last = -1.0f;
    for (const float light : {0.0f, 0.05f, 0.18f, 0.5f, 1.0f, 2.0f, 8.0f, 100.0f}) {
      const float shown = viewTransform({light, light, light}, transform)[0];
      CHECK((shown > last || (shown == last && shown > 0.99f)) && shown >= 0.0f && shown <= 1.0001f);
      last = shown;
    }
    CHECK(near(viewTransform({0.18f, 0.18f, 0.18f}, transform)[0], 0.16f, 0.03f));
  }
  // Neutral keeps a base colour's hue and values below its compression.
  const std::array<float, 3> red = viewTransform({0.6f, 0.1f, 0.05f}, ViewTransform::Neutral);
  CHECK(near(red[0], 0.56f, 0.01f) && near(red[1], 0.06f, 0.01f));
  // Filmic compresses white more than Neutral does.
  CHECK(viewTransform({1.0f, 1.0f, 1.0f}, ViewTransform::Filmic)[0] <
        viewTransform({1.0f, 1.0f, 1.0f}, ViewTransform::Neutral)[0]);

  // A frame of two pixels: a grey body (opaque) and a half shadow
  // (transparent black, alpha 0.5) over a white background.
  const std::uint16_t frame[8] = {half(0.2f), half(0.2f), half(0.2f), half(1.0f), 0, 0, 0, half(0.5f)};
  const auto shown = [&frame](const Look& look) {
    const QImage image = mitcad::render::displayImage(frame, 2, 1, Qt::white, Qt::white, look);
    return std::make_pair(QColor(image.pixel(0, 0)), QColor(image.pixel(1, 0)));
  };
  const auto plain = shown(Look());
  CHECK(plain.first.red() == 124);  // sRGB of 0.2
  CHECK(plain.second.red() == 188); // white at half: sRGB of 0.5
  const auto brighter = shown(Look{1.0f, ViewTransform::Standard});
  CHECK(brighter.first.red() == 170); // 0.4
  CHECK(brighter.second == plain.second); // the shadow over the background stays
  const auto filmic = shown(Look{0.0f, ViewTransform::Filmic});
  CHECK(filmic.first.red() != plain.first.red() && filmic.second == plain.second);
}

// The final render's image (mitcad#48): opaque 8 bits as the view shows
// it, 16 bits the same colours finer, transparent with straight alpha.
void renderOutputTests() {
  using mitcad::render::Look;
  using mitcad::render::ViewTransform;
  using mitcad::render::outputImage;
  const std::uint16_t frame[8] = {half(0.2f), half(0.2f), half(0.2f), half(1.0f), 0, 0, 0, half(0.5f)};
  const Look look{0.5f, ViewTransform::Filmic};
  const QImage shown = mitcad::render::displayImage(frame, 2, 1, Qt::white, Qt::white, look);
  const QImage opaque = outputImage(frame, nullptr, 2, 1, Qt::white, Qt::white, look, false, false);
  CHECK(opaque == shown);
  const QImage deep = outputImage(frame, nullptr, 2, 1, Qt::white, Qt::white, look, false, true);
  CHECK(deep.format() == QImage::Format_RGBX64);
  for (int x = 0; x < 2; ++x) {
    CHECK(std::abs(deep.pixelColor(x, 0).red() - shown.pixelColor(x, 0).red()) <= 1);
  }
  const QImage transparent = outputImage(frame, nullptr, 2, 1, Qt::white, Qt::white, Look(), true, false);
  CHECK(transparent.hasAlphaChannel());
  CHECK(transparent.pixelColor(0, 0).red() == 124 && transparent.pixelColor(0, 0).alpha() == 255);
  // The shadow: black at half coverage, the background left out.
  CHECK(transparent.pixelColor(1, 0).red() == 0 && transparent.pixelColor(1, 0).alpha() == 128);
  const QImage transparentDeep = outputImage(frame, nullptr, 2, 1, Qt::white, Qt::white, Look(), true, true);
  CHECK(transparentDeep.format() == QImage::Format_RGBA64 &&
        transparentDeep.pixel(1, 0) == transparent.pixel(1, 0));
}

// The ground's catcher factors (mitcad#54): the background behind the
// ground is multiplied by them in the view and the final render's
// images; a transparent image takes their mean darkening as alpha.
void renderCatcherTests() {
  using mitcad::render::Look;
  using mitcad::render::outputImage;
  // A grey body (opaque) and the bare ground with a red reflection
  // (factors 1, 0.5, 0.5) and a half shadow (0.5 all).
  const std::uint16_t frame[12] = {half(0.2f), half(0.2f), half(0.2f), half(1.0f), 0, 0, 0, 0, 0, 0, 0, 0};
  const std::uint16_t catcher[12] = {half(0.1f), half(0.1f), half(0.1f), half(1.0f),
                                     half(1.0f), half(0.5f), half(0.5f), half(1.0f),
                                     half(0.5f), half(0.5f), half(0.5f), half(1.0f)};
  const QImage shown = mitcad::render::displayImage(frame, 3, 1, Qt::white, Qt::white, Look(), catcher);
  CHECK(shown.pixelColor(0, 0).red() == 124); // the body covers the ground: no factor
  CHECK(shown.pixelColor(1, 0).red() == 255 && shown.pixelColor(1, 0).green() == 188 &&
        shown.pixelColor(1, 0).blue() == 188);
  CHECK(shown.pixelColor(2, 0).red() == 188 && shown.pixelColor(2, 0).green() == 188);
  // Over a dark background the reflection tints it as much.
  const QImage dark = mitcad::render::displayImage(frame, 3, 1, QColor(60, 60, 60), QColor(60, 60, 60), Look(), catcher);
  CHECK(dark.pixelColor(1, 0).red() > dark.pixelColor(1, 0).green() + 10);
  CHECK(outputImage(frame, catcher, 3, 1, Qt::white, Qt::white, Look(), false, false) == shown);
  const QImage deep = outputImage(frame, catcher, 3, 1, Qt::white, Qt::white, Look(), false, true);
  CHECK(std::abs(deep.pixelColor(1, 0).green() - 188) <= 1 && deep.pixelColor(1, 0).red() == 255);
  // Transparent: black with the mean darkening as alpha (1/3, 1/2).
  const QImage transparent = outputImage(frame, catcher, 3, 1, Qt::white, Qt::white, Look(), true, false);
  CHECK(transparent.pixelColor(1, 0).alpha() == 85 && transparent.pixelColor(1, 0).red() == 0);
  CHECK(transparent.pixelColor(2, 0).alpha() == 128 && transparent.pixelColor(0, 0).alpha() == 255);
}

// The render device's choice (mitcad#50): automatic prefers a GPU, an
// unknown one gives the CPU and says so.
void renderDeviceChoiceTests() {
  using mitcad::render::chooseDevice;
  using mitcad::render::DeviceInfo;
  const DeviceInfo cpu{"CPU", "CPU", "A processor", false};
  const DeviceInfo cuda{"CUDA_GPU_0000:07:00", "CUDA", "A GPU", false};
  const DeviceInfo hip{"HIP_GPU_0000:08:00", "HIP", "Another GPU", true};
  const DeviceInfo metal{"METAL_GPU", "METAL", "An Apple GPU", false, true, false, false};
  const DeviceInfo metalRT{"METAL_RT_GPU", "METAL", "A newer Apple GPU", false, true, true, true};
  std::string message;
  CHECK(chooseDevice("auto", {cpu}, message).id == "CPU" && message.empty());
  CHECK(chooseDevice("", {cpu, hip, cuda}, message).id == cuda.id && message.empty());
  CHECK(chooseDevice("auto", {cpu, hip}, message).id == hip.id);
  CHECK(chooseDevice("cpu", {cpu, cuda}, message).id == "CPU" && message.empty());
  CHECK(chooseDevice("CPU", {cpu, cuda}, message).id == "CPU");
  CHECK(chooseDevice(hip.id, {cpu, cuda, hip}, message).id == hip.id && message.empty());
  CHECK(chooseDevice("hip", {cpu, cuda, hip}, message).id == hip.id);
  CHECK(chooseDevice("CUDA_gone", {cpu, hip}, message).id == "CPU" &&
        message == "There is no render device \"CUDA_gone\"; rendering on the CPU");
  CHECK(chooseDevice("auto", {cpu, metal}, message).id == metal.id && message.empty());
  CHECK(chooseDevice("metal", {cpu, metal}, message).id == metal.id && message.empty());
  CHECK(chooseDevice(metalRT.id, {cpu, metal, metalRT}, message).metalRT);
  CHECK(chooseDevice("cpu", {cpu, metalRT}, message).type == "CPU" && message.empty());
  CHECK(chooseDevice("METAL_missing", {cpu, metal}, message).type == "CPU" &&
        message == "There is no render device \"METAL_missing\"; rendering on the CPU");
  CHECK(chooseDevice("METAL", {cpu}, message).type == "CPU" &&
        message == "There is no render device \"METAL\"; rendering on the CPU");
  // Capability alone must not opt older Apple GPUs into MetalRT. Cycles'
  // automatic policy also cannot opt in a device without the capability.
  using mitcad::render::automaticRayTracing;
  CHECK(!automaticRayTracing("METAL", false, false));
  CHECK(!automaticRayTracing("METAL", true, false));
  CHECK(!automaticRayTracing("METAL", false, true));
  CHECK(automaticRayTracing("METAL", true, true));
  CHECK(automaticRayTracing("OPTIX", true, false));
  CHECK(!automaticRayTracing("CPU", false, true));
  // The virtual-GPU workaround must not disable asynchronous compilation
  // on physical Macs, and explicit diagnostics must override either case.
  using mitcad::render::metalAsyncPipelineCreation;
  const DeviceInfo virtualMetal{"METAL_VIRTUAL", "METAL", "Apple Paravirtual device (GPU)", false};
  CHECK(metalAsyncPipelineCreation({cpu, metal, metalRT}, nullptr));
  CHECK(!metalAsyncPipelineCreation({cpu, virtualMetal}, nullptr));
  CHECK(!metalAsyncPipelineCreation({metal, virtualMetal}, nullptr));
  CHECK(metalAsyncPipelineCreation({virtualMetal}, "1"));
  CHECK(!metalAsyncPipelineCreation({metal}, "0"));
  CHECK(!metalAsyncPipelineCreation({metal}, ""));
  CHECK(metalAsyncPipelineCreation({}, nullptr));
  // A usable CPU image or recovered Metal session is not hardware acceptance.
  using mitcad::render::requiredMetalError;
  CHECK(!requiredMetalError(cpu, false).empty());
  CHECK(!requiredMetalError(cuda, false).empty());
  CHECK(requiredMetalError(metal, false).empty());
  CHECK(requiredMetalError(metalRT, false).empty());
  CHECK(!requiredMetalError(metal, true).empty());
  DeviceInfo wrongPolicy = metal;
  wrongPolicy.metalRT = true;
  CHECK(!requiredMetalError(wrongPolicy, false).empty());
  wrongPolicy = metalRT;
  wrongPolicy.metalRT = false;
  CHECK(!requiredMetalError(wrongPolicy, false).empty());
  // Without a list (none known yet) the CPU still is the choice.
  CHECK(chooseDevice("auto", {}, message).type == "CPU");
}

// A renderer that only records what it is given (the fallback's tests).
class FakeRenderer : public mitcad::render::Renderer {
public:
  FakeRenderer(mitcad::render::FrameSink& sink, mitcad::render::DeviceInfo device, std::string startError)
      : m_sink(sink), m_device(std::move(device)), m_error(std::move(startError)) {}
  std::string description() const override { return "fake on " + m_device.type; }
  mitcad::render::SceneChanges updateScene(const mitcad::render::SceneUpdate& update) override {
    scene = update;
    meshes = static_cast<int>(update.meshes.size());
    bodies = static_cast<int>(update.bodies.size());
    return {};
  }
  void setView(const mitcad::render::ViewData& view) override {
    camera = view;
    sequence = view.sequence;
    if (m_sink.directDisplayBuffers()) {
      if (auto frame = m_sink.beginDisplayFrame(1, 1, 1, 1)) {
        frame->buffers().light[0] = 0;
        if (m_device.type != "CPU" && m_sink.metalDisplayBuffers() && frame->nativeMetalBuffer()) m_sink.displayInteropUsed();
        frame->publish(view.sequence, 1, false, false);
        frame->publish(view.sequence, 1, false, false); // A retained callback cannot publish it twice.
      }
      return;
    }
    // A frame of the view.
    if (std::uint16_t* pixels = m_sink.beginFrame(1, 1, 1, 1, false).light) {
      pixels[0] = 0;
      m_sink.endFrame(view.sequence, 1, false);
    }
  }
  void setSamples(int count) override { samples = count; }
  void setPreviews(const std::vector<int>& counts) override { previews = counts; }
  std::string setEnvironment(const mitcad::render::EnvironmentData& environment) override {
    world = environment;
    lights = static_cast<int>(environment.lights.size());
    return {};
  }
  void wait() override {}
  void setFinal(bool denoise, double seconds) override {
    finalDenoise = denoise;
    timeLimit = seconds;
  }
  int currentSample() const override { return samples; }
  mitcad::render::DeviceInfo device() const override { return m_device; }
  std::string deviceError() const override {
    const std::lock_guard<std::mutex> lock(m_mutex);
    return m_error;
  }
  void fail(const std::string& why) {
    const std::lock_guard<std::mutex> lock(m_mutex);
    m_error = why;
  }

  mitcad::render::SceneUpdate scene;
  mitcad::render::ViewData camera;
  mitcad::render::EnvironmentData world;
  std::vector<int> previews;
  bool finalDenoise = false;
  double timeLimit = 0.0;
  int meshes = 0;
  int bodies = 0;
  int lights = 0;
  std::uint64_t sequence = 0;
  int samples = 0;

private:
  mitcad::render::FrameSink& m_sink;
  mitcad::render::DeviceInfo m_device;
  mutable std::mutex m_mutex;
  std::string m_error;
};

class CountingSink : public mitcad::render::FrameSink {
public:
  mitcad::render::FrameBuffers beginFrame(int, int, int, int, bool) override { return {&m_pixel, nullptr}; }
  void endFrame(std::uint64_t, int, bool) override { ++frames; }
  void finished(std::uint64_t, int, double) override {}
  int frames = 0;

private:
  std::uint16_t m_pixel = 0;
};

class DirectCountingSink : public mitcad::render::FrameSink {
  class Frame final : public mitcad::render::DirectFrame {
  public:
    explicit Frame(DirectCountingSink& owner) : m_owner(owner) {}
    mitcad::render::FrameBuffers buffers() const override { return {m_pixels.data(), nullptr}; }
    void* nativeMetalBuffer() const override { return m_pixels.data(); }
    std::size_t nativeMetalBufferSize() const override { return sizeof(m_pixels); }
    void* nativeCatcherMetalBuffer() const override { return m_pixels.data() + 1; }
    std::size_t nativeCatcherMetalBufferSize() const override { return sizeof(m_pixels); }
    void publish(std::uint64_t, int, bool, bool) override { ++m_owner.frames; }
    void discard() override { ++m_owner.discards; }
  private:
    DirectCountingSink& m_owner;
    mutable std::array<std::uint16_t, 4> m_pixels{};
  };
public:
  mitcad::render::FrameBuffers beginFrame(int, int, int, int, bool) override { CHECK(false); return {}; }
  void endFrame(std::uint64_t, int, bool) override { CHECK(false); }
  void finished(std::uint64_t, int, double) override {}
  bool directDisplayBuffers() const override { return true; }
  bool metalDisplayBuffers() const override { return true; }
  void displayInteropUsed() override { ++interop; }
  void auxiliaryNativeUsed(bool graphics, bool composite) override { auxiliary += graphics && composite ? 1 : 100; }
  std::shared_ptr<mitcad::render::DirectFrame> beginDisplayFrame(int, int, int, int) override {
    return std::make_shared<Frame>(*this);
  }
  int frames = 0;
  int discards = 0;
  int interop = 0;
  int auxiliary = 0;
};

void renderDirectFallbackTests() {
  using namespace mitcad::render;
  DirectCountingSink sink;
  std::shared_ptr<DirectFrame> retiredFrame;
  FrameSink* first = nullptr;
  FrameSink* latest = nullptr;
  const auto factory = [&](FrameSink& target, const DeviceInfo& device, bool,
                            std::string&) -> std::unique_ptr<Renderer> {
    latest = &target;
    if (!first) {
      first = &target;
      retiredFrame = target.beginDisplayFrame(1, 1, 1, 1);
    }
    const DeviceInfo selected = device.id.empty() ? DeviceInfo{"CPU", "CPU", "A processor", false} : device;
    return std::make_unique<FakeRenderer>(target, selected, std::string());
  };
  std::string error;
  auto renderer = makeDeviceRenderer(sink, DeviceInfo{"METAL_GPU", "METAL", "A GPU", false}, factory,
      [](const DeviceInfo&, const std::string&) {}, "simulated", error);
  CHECK(renderer && retiredFrame && first->directDisplayBuffers() && first->metalDisplayBuffers());
  if (!renderer || !retiredFrame) return;
  CHECK(retiredFrame->nativeMetalBuffer() == retiredFrame->buffers().light && retiredFrame->nativeMetalBufferSize() == 8);
  CHECK(retiredFrame->nativeCatcherMetalBuffer() == retiredFrame->buffers().light + 1 &&
        retiredFrame->nativeCatcherMetalBufferSize() == 8);
  first->auxiliaryNativeUsed(true, true);
  CHECK(sink.auxiliary == 1);
  ViewData view;
  view.sequence = 5;
  renderer->setView(view);
  renderer->wait();
  CHECK(renderer->device().type == "CPU" && sink.frames == 2 && sink.interop == 1 && sink.discards == 0);
  CHECK(first->directDisplayBuffers() && first->metalDisplayBuffers() && !first->beginDisplayFrame(1, 1, 1, 1));
  first->displayInteropUsed();
  first->auxiliaryNativeUsed(true, true);
  CHECK(sink.auxiliary == 1);
  retiredFrame->publish(99, 2, true, false);
  retiredFrame->publish(99, 2, true, false);
  retiredFrame->discard();
  CHECK(sink.frames == 2 && sink.discards == 1 && sink.interop == 1);
  auto stoppedFrame = latest->beginDisplayFrame(1, 1, 1, 1);
  CHECK(stoppedFrame != nullptr);
  renderer.reset();
  if (stoppedFrame) {
    stoppedFrame->publish(100, 2, true, false);
    stoppedFrame->discard();
  }
  CHECK(sink.frames == 2 && sink.discards == 2);
}

// The CPU takes over from a device that cannot start or fails while it
// renders, with what the device had (mitcad#50).
void renderFallbackTests() {
  using mitcad::render::DeviceInfo;
  const std::vector<DeviceInfo> gpus = {
      {"CUDA_GPU", "CUDA", "A GPU", false},
      {"METAL_GPU", "METAL", "An Apple GPU", false, true, false, false},
      {"METAL_RT_GPU", "METAL", "A newer Apple GPU", false, true, true, true}};
  struct Made {
    std::vector<FakeRenderer*> renderers;
    std::vector<bool> cpuDenoising;
    std::vector<std::string> reports;
    std::vector<DeviceInfo> reportedDevices;
    std::mutex mutex;
  };
  const auto factoryFor = [](Made& made, std::string gpuStartError, bool cpuStarts) {
    return [&made, gpuStartError, cpuStarts](mitcad::render::FrameSink& sink, const DeviceInfo& device, bool cpuDenoising,
                                             std::string& error) -> std::unique_ptr<mitcad::render::Renderer> {
      const bool onCpu = device.id.empty() || device.type == "CPU";
      if (onCpu && !cpuStarts) {
        error = "no CPU";
        return nullptr;
      }
      DeviceInfo on = device;
      if (onCpu && on.id.empty()) {
        on = DeviceInfo{"CPU", "CPU", "A processor", false};
      }
      if (cpuDenoising) {
        on.denoiser.type = "CPU";
      }
      auto renderer = std::make_unique<FakeRenderer>(sink, on, onCpu ? std::string() : gpuStartError);
      made.renderers.push_back(renderer.get());
      made.cpuDenoising.push_back(cpuDenoising);
      return renderer;
    };
  };
  const auto reportTo = [](Made& made) {
    return [&made](const DeviceInfo& device, const std::string& message) {
      const std::lock_guard<std::mutex> lock(made.mutex);
      made.reports.push_back(message);
      made.reportedDevices.push_back(device);
    };
  };
  mitcad::render::SceneUpdate first;
  first.meshes = {{"a", {}}, {"b", {}}};
  first.meshes[1].second.positions = {0, 0, 0, 10, 0, 0, 0, 10, 0};
  first.meshes[1].second.indices = {0, 1, 2};
  first.bodies.resize(2);
  mitcad::render::SceneUpdate second;
  second.meshes = {{"c", {}}};
  second.released = {"a"};
  second.bodies.resize(3);
  second.bodies[0].mesh = "b";
  second.bodies[0].material.metallic = 0.75f;
  second.bodies[0].material.roughness = 0.25f;
  second.bodies[0].transform[3] = 42.0f;
  second.ground = false;
  mitcad::render::SceneUpdate whole = second;
  whole.meshes.insert(whole.meshes.begin(), first.meshes[1]);
  whole.released.clear();
  mitcad::render::ViewData view;
  view.sequence = 7;
  view.perspective = true;
  view.eye = {12, -34, 56};
  view.target = {1, 2, 3};
  view.width = 96;
  view.height = 64;
  view.halfHeight = 75.0;
  mitcad::render::EnvironmentData environment;
  environment.preset = mitcad::render::EnvironmentData::Preset::Outdoor;
  environment.strength = 0.5f;
  environment.rotation = 0.25f;
  environment.background = true;
  environment.groundReflections = true;
  environment.groundAtHeight = true;
  environment.groundHeight = -12.0;
  environment.lights.resize(2);
  environment.lights[1].kind = mitcad::render::LightData::Kind::Sun;
  environment.lights[1].power = 3.5f;
  const auto settings = [&environment](mitcad::render::Renderer& renderer) {
    renderer.setEnvironment(environment);
    renderer.setSamples(16);
    renderer.setPreviews({4, 8});
    renderer.setFinal(true, 12.5);
  };
  const auto checkState = [&view, &environment, &first](const FakeRenderer& cpu) {
    CHECK(cpu.samples == 16 && cpu.meshes == 2 && cpu.bodies == 3 && cpu.sequence == 7 && cpu.lights == 2);
    CHECK(cpu.previews == std::vector<int>({4, 8}) && cpu.finalDenoise && cpu.timeLimit == 12.5);
    CHECK(cpu.camera.perspective && cpu.camera.eye == view.eye && cpu.camera.target == view.target);
    CHECK(cpu.camera.width == 96 && cpu.camera.height == 64 && cpu.camera.halfHeight == 75.0);
    CHECK(cpu.world.preset == environment.preset && cpu.world.strength == 0.5f && cpu.world.rotation == 0.25f);
    CHECK(cpu.world.background && cpu.world.groundReflections && cpu.world.groundAtHeight);
    CHECK(cpu.world.groundHeight == -12.0);
    if (cpu.world.lights.size() != 2 || cpu.scene.meshes.size() != 2 || cpu.scene.bodies.size() != 3) {
      return;
    }
    CHECK(cpu.world.lights[1].kind == mitcad::render::LightData::Kind::Sun && cpu.world.lights[1].power == 3.5f);
    CHECK(!cpu.scene.ground && cpu.scene.released.empty());
    CHECK(cpu.scene.meshes[0].first == "b" && cpu.scene.meshes[1].first == "c");
    CHECK(cpu.scene.meshes[0].second.positions == first.meshes[1].second.positions);
    CHECK(cpu.scene.meshes[0].second.indices == first.meshes[1].second.indices);
    CHECK(cpu.scene.bodies[0].material.metallic == 0.75f && cpu.scene.bodies[0].material.roughness == 0.25f);
    CHECK(cpu.scene.bodies[0].mesh == "b" && cpu.scene.bodies[0].transform[3] == 42.0f);
  };
  CountingSink sink;
  std::string error;
  for (const DeviceInfo& gpu : gpus) {
    const std::string label = gpu.name + " (" + gpu.type + ")";
    {
      // A startup failure must still accept the requested scene/settings.
      Made made;
      auto renderer = mitcad::render::makeDeviceRenderer(sink, gpu, factoryFor(made, "no kernel", true), reportTo(made),
                                                         {}, error);
      CHECK(renderer && renderer->device().type == "CPU" && !renderer->device().metalRT);
      settings(*renderer);
      renderer->updateScene(whole);
      renderer->setView(view);
      renderer->wait();
      checkState(*made.renderers.back());
      CHECK(made.reports.size() == 1 && made.reports[0] == "Rendering on the CPU: " + label + " failed (no kernel)");
      CHECK(made.reportedDevices.size() == 1 && made.reportedDevices[0].type == "CPU" && !made.reportedDevices[0].metalRT);
    }
    {
      // Runtime failure replays all unreleased meshes and the last bodies,
      // camera, lights, previews, denoising and final-render time limit.
      Made made;
      auto renderer =
          mitcad::render::makeDeviceRenderer(sink, gpu, factoryFor(made, {}, true), reportTo(made), {}, error);
      CHECK(renderer && renderer->device().type == gpu.type && renderer->device().metalRT == gpu.metalRT);
      CHECK(made.reports.empty());
      settings(*renderer);
      renderer->updateScene(first);
      renderer->updateScene(second);
      renderer->setView(view);
      made.renderers.front()->fail("out of memory");
      renderer->wait();
      CHECK(renderer->device().type == "CPU" && !renderer->device().metalRT && made.renderers.size() == 2);
      CHECK(renderer->device().denoiser.type == "CPU");
      CHECK(made.cpuDenoising == std::vector<bool>({false, true}));
      checkState(*made.renderers.back());
      // Join the watcher before checking its report: wait() can observe
      // the replacement renderer before the callback has returned.
      renderer.reset();
      const std::lock_guard<std::mutex> lock(made.mutex);
      CHECK(made.reports.size() == 1 && made.reports[0] == "Rendering on the CPU: " + label + " failed (out of memory)");
      CHECK(made.reportedDevices.size() == 1 && made.reportedDevices[0].type == "CPU" && !made.reportedDevices[0].metalRT);
    }
    {
      // Neither starts.
      Made made;
      auto renderer =
          mitcad::render::makeDeviceRenderer(sink, gpu, factoryFor(made, "no kernel", false), reportTo(made), {}, error);
      CHECK(!renderer && error == "no kernel; the CPU: no CPU");
    }
  }
  DeviceInfo denoisingGpu{"CUDA_GPU", "CUDA", "A GPU", false};
  denoisingGpu.denoiser.id = denoisingGpu.id;
  denoisingGpu.denoiser.type = "CUDA";
  denoisingGpu.denoiser.name = denoisingGpu.name;
  {
    // A GPU that also denoises on itself (mitcad#202): when it fails, the
    // same GPU with CPU denoising takes over first, with what it had; when
    // that one fails too, the CPU.
    Made made;
    auto renderer =
        mitcad::render::makeDeviceRenderer(sink, denoisingGpu, factoryFor(made, {}, true), reportTo(made), {}, error);
    CHECK(renderer && renderer->device().denoiser.type == "CUDA");
    settings(*renderer);
    renderer->updateScene(first);
    renderer->updateScene(second);
    renderer->setView(view);
    made.renderers.front()->fail("Failed to create OIDN device");
    renderer->wait();
    CHECK(renderer->device().type == "CUDA" && renderer->device().denoiser.type == "CPU");
    CHECK(made.renderers.size() == 2 && made.cpuDenoising == std::vector<bool>({false, true}));
    checkState(*made.renderers.back());
    {
      const std::lock_guard<std::mutex> lock(made.mutex);
      CHECK(made.reports.size() == 1 &&
            made.reports[0] == "Denoising on the CPU: Open Image Denoise on A GPU (CUDA) failed "
                               "(Failed to create OIDN device)");
      CHECK(made.reportedDevices.size() == 1 && made.reportedDevices[0].type == "CUDA" &&
            made.reportedDevices[0].denoiser.type == "CPU");
    }
    made.renderers.back()->fail("out of memory");
    renderer->wait();
    CHECK(renderer->device().type == "CPU" && renderer->device().denoiser.type == "CPU");
    CHECK(made.cpuDenoising == std::vector<bool>({false, true, true}));
    checkState(*made.renderers.back());
    renderer.reset();
    const std::lock_guard<std::mutex> lock(made.mutex);
    CHECK(made.reports.size() == 2 && made.reports[1] == "Rendering on the CPU: A GPU (CUDA) failed (out of memory)");
    CHECK(made.reportedDevices.size() == 2 && made.reportedDevices[1].type == "CPU");
  }
  {
    // The same GPU with CPU denoising cannot start: the CPU, saying why
    // the first one failed.
    Made made;
    const auto inner = factoryFor(made, {}, true);
    const auto factory = [&inner](mitcad::render::FrameSink& target, const DeviceInfo& device, bool cpuDenoising,
                                  std::string& why) -> std::unique_ptr<mitcad::render::Renderer> {
      if (cpuDenoising && device.type != "CPU" && !device.id.empty()) {
        why = "no kernel";
        return nullptr;
      }
      return inner(target, device, cpuDenoising, why);
    };
    auto renderer = mitcad::render::makeDeviceRenderer(sink, denoisingGpu, factory, reportTo(made), {}, error);
    settings(*renderer);
    renderer->updateScene(whole);
    renderer->setView(view);
    made.renderers.front()->fail("Failed to create OIDN device");
    renderer->wait();
    CHECK(renderer->device().type == "CPU" && made.renderers.size() == 2);
    checkState(*made.renderers.back());
    renderer.reset();
    const std::lock_guard<std::mutex> lock(made.mutex);
    CHECK(made.reports.size() == 1 &&
          made.reports[0] == "Rendering on the CPU: A GPU (CUDA) failed (Failed to create OIDN device)");
  }
  {
    // A GPU that already denoises on the CPU fails in its path tracer: the
    // CPU at once.
    Made made;
    DeviceInfo gpu = denoisingGpu;
    gpu.denoiser.type = "CPU";
    auto renderer = mitcad::render::makeDeviceRenderer(sink, gpu, factoryFor(made, {}, true), reportTo(made), {}, error);
    settings(*renderer);
    renderer->updateScene(whole);
    renderer->setView(view);
    made.renderers.front()->fail("out of memory");
    renderer->wait();
    CHECK(renderer->device().type == "CPU" && made.renderers.size() == 2);
    checkState(*made.renderers.back());
    renderer.reset();
    const std::lock_guard<std::mutex> lock(made.mutex);
    CHECK(made.reports.size() == 1 && made.reports[0] == "Rendering on the CPU: A GPU (CUDA) failed (out of memory)");
  }
  {
    // The test hook's failure is the path tracer's: the CPU at once, also
    // for a GPU that denoises on itself.
    Made made;
    auto renderer = mitcad::render::makeDeviceRenderer(sink, denoisingGpu, factoryFor(made, {}, true), reportTo(made),
                                                       "simulated", error);
    renderer->setView(view);
    renderer->wait();
    CHECK(renderer->device().type == "CPU" && made.renderers.size() == 2);
    renderer.reset();
    const std::lock_guard<std::mutex> lock(made.mutex);
    CHECK(made.reports.size() == 1 && made.reports[0] == "Rendering on the CPU: A GPU (CUDA) failed (simulated)");
  }
  {
    // A denoising GPU can fail while the path tracer is already on the
    // CPU. Retry with CPU denoising too, retaining the complete scene.
    Made made;
    DeviceInfo cpu{"CPU", "CPU", "A processor", false};
    cpu.denoiser.type = "METAL";
    auto renderer = mitcad::render::makeDeviceRenderer(sink, cpu, factoryFor(made, {}, true), reportTo(made), {}, error);
    settings(*renderer);
    renderer->updateScene(first);
    renderer->updateScene(second);
    renderer->setView(view);
    made.renderers.front()->fail("OIDN device failed");
    renderer->wait();
    CHECK(renderer->device().type == "CPU" && renderer->device().denoiser.type == "CPU");
    CHECK(made.cpuDenoising == std::vector<bool>({false, true}));
    checkState(*made.renderers.back());
    // A failed CPU retry is terminal: wait must not retry forever.
    made.renderers.back()->fail("CPU OIDN unavailable");
    renderer->wait();
    CHECK(renderer->deviceError() == "CPU OIDN unavailable" && made.renderers.size() == 2);
  }
  {
    // Initialization of a separate Metal denoiser can also fail before
    // the CPU path tracer has received a scene.
    Made made;
    const auto cpuFactory = factoryFor(made, {}, true);
    const auto factory = [&cpuFactory](mitcad::render::FrameSink& target, const DeviceInfo& device,
                                       bool cpuDenoising, std::string& why) -> std::unique_ptr<mitcad::render::Renderer> {
      if (!cpuDenoising) {
        why = "OIDN device initialization failed";
        return nullptr;
      }
      return cpuFactory(target, device, cpuDenoising, why);
    };
    auto renderer = mitcad::render::makeDeviceRenderer(sink, DeviceInfo{"CPU", "CPU", "A processor", false},
                                                       factory, reportTo(made), {}, error);
    CHECK(renderer && renderer->device().denoiser.type == "CPU");
    CHECK(made.cpuDenoising == std::vector<bool>({true}));
    settings(*renderer);
    renderer->updateScene(whole);
    renderer->setView(view);
    renderer->wait();
    checkState(*made.renderers.back());
    CHECK(made.reports.size() == 1 && made.reports[0].find("OIDN device initialization failed") != std::string::npos);
  }
  {
    // Explicit CPU selection remains CPU with Metal available.
    Made made;
    std::string message;
    const DeviceInfo cpu = mitcad::render::chooseDevice("cpu", gpus, message);
    auto renderer = mitcad::render::makeDeviceRenderer(sink, cpu, factoryFor(made, {}, true), reportTo(made), {}, error);
    CHECK(renderer && renderer->device().type == "CPU" && !renderer->device().metalRT && made.renderers.size() == 1);
    settings(*renderer);
    renderer->updateScene(whole);
    renderer->setView(view);
    renderer->wait();
    checkState(*made.renderers.back());
    CHECK(made.reports.empty());
  }
  {
    // The test hook fails the first renderer, even on the CPU, after its
    // first frame.
    Made made;
    auto renderer = mitcad::render::makeDeviceRenderer(sink, DeviceInfo{"CPU", "CPU", "A processor", false},
                                                       factoryFor(made, {}, true), reportTo(made), "simulated", error);
    view.sequence = 3;
    renderer->setView(view);
    renderer->wait();
    CHECK(made.renderers.size() == 2 && made.renderers.back()->sequence == 3);
    renderer.reset();
    const std::lock_guard<std::mutex> lock(made.mutex);
    CHECK(made.reports.size() == 1 && made.reports[0] == "Rendering on the CPU: A processor failed (simulated)");
  }
}

// Feedback and error reports (mitcad#61, mitcad#62): masking, crash files,
// duplicate keys, the prefilled issue form.
void reportMaskTests() {
  using mitcad::report::mask;
  const mitcad::report::MaskContext alice{QStringLiteral("/home/alice"), QStringLiteral("alice"),
                                          QStringLiteral("alice-laptop.example.org")};
  const auto masked = [&alice](const char* text) { return mask(QString::fromUtf8(text), alice); };
  CHECK(masked("Could not open /home/alice/designs/bracket.f3d.") ==
        QStringLiteral("Could not open <path>/<file>.f3d."));
  CHECK(masked("/usr/lib/x86_64-linux-gnu/libc.so.6(+0x45330) [0x7f0012345678]") ==
        QStringLiteral("<path>/libc.so.6(+0x45330) [0x7f0012345678]"));
  CHECK(masked("/home/alice/src/build/app/mitcad(_ZN6mitcad5crash8crashNowEv+0x10) [0x55]") ==
        QStringLiteral("<path>/mitcad(_ZN6mitcad5crash8crashNowEv+0x10) [0x55]"));
  CHECK(masked("at /opt/src/geometry/src/guard.cpp:42") == QStringLiteral("at <path>/guard.cpp:42"));
  CHECK(masked("~/designs/plate.mitcad") == QStringLiteral("<path>/<file>.mitcad"));
  CHECK(masked("\\\\fileserver\\share\\plate.step") == QStringLiteral("<path>/<file>.step"));
  CHECK(masked("in /srv/projects/") == QStringLiteral("in <path>"));
  CHECK(masked("home is /home/alice") == QStringLiteral("home is <home>"));
  // Another user's home is a path like any other; the user's name stays a
  // word of its own.
  CHECK(masked("/home/alice2/x.f3d by alice2") == QStringLiteral("<path>/<file>.f3d by alice2"));
  CHECK(masked("user alice on alice-laptop.example.org (alice-laptop)") ==
        QStringLiteral("user <user> on <host> (<host>)"));
  CHECK(masked("ALICE wrote") == QStringLiteral("<user> wrote"));
  // Ordinary text, numbers, units and addresses stay.
  CHECK(masked("Mitcad 0.1.0: and/or 1/2 mm/s, 2026/10/08, https://example.com/a/b") ==
        QStringLiteral("Mitcad 0.1.0: and/or 1/2 mm/s, 2026/10/08, https://example.com/a/b"));
  // Windows: the home folder with either slash and any case.
  const mitcad::report::MaskContext jane{QStringLiteral("C:/Users/Jane Doe"), QStringLiteral("jdoe"),
                                         QStringLiteral("DESKTOP-1234")};
  CHECK(mask(QStringLiteral("C:\\Users\\Jane Doe\\Documents\\part.step"), jane) ==
        QStringLiteral("<path>/<file>.step"));
  CHECK(mask(QStringLiteral("c:/users/jane doe/AppData/Local/Mitcad/mitcad.exe"), jane) ==
        QStringLiteral("<path>/mitcad.exe"));
  CHECK(mask(QStringLiteral("D:\\Work\\gear.f3d on DESKTOP-1234 by jdoe"), jane) ==
        QStringLiteral("<path>/<file>.f3d on <host> by <user>"));
  // Names that would mask ordinary words are left to the path masking.
  const mitcad::report::MaskContext developer{QStringLiteral("/home/mitcad"), QStringLiteral("mitcad"),
                                              QStringLiteral("localhost")};
  CHECK(mask(QStringLiteral("Mitcad 0.1 on localhost: /home/mitcad/a.f3d"), developer) ==
        QStringLiteral("Mitcad 0.1 on localhost: <path>/<file>.f3d"));
}

void reportCrashTests() {
  using namespace mitcad::report;
  const QByteArray file =
      "Mitcad crash report 1\nprocess: import-worker\nversion: 0.1.0\nplatform: Linux 6.8.0 x86_64\npid: 4242\n"
      "parent: 4000\ntime: 1760000000\nsignal: SIGSEGV\naddress: 0x0\nmessage: \nstack:\n"
      "/opt/mitcad/bin/mitcad(_ZN6mitcad5crash6detail8onSignalEiP9siginfo_tPv+0x3c) [0x5555555a1000]\n"
      "/lib/x86_64-linux-gnu/libc.so.6(+0x45330) [0x7ffff7845330]\n"
      "/opt/mitcad/bin/mitcad(_ZN6mitcad5crash8crashNowEv+0x10) [0x5555555a2000]\n"
      "/opt/mitcad/bin/mitcad(main+0x55) [0x5555555a3000]\n"
      "/lib/x86_64-linux-gnu/libc.so.6(__libc_start_main+0x8b) [0x7ffff782a28b]\n"
      "actions:\ncommand help.send_feedback\nmodel add_feature\nend\n";
  const CrashReport report = parseCrashReport(file);
  CHECK(report.valid);
  CHECK(report.process == QStringLiteral("import-worker"));
  CHECK(report.parent == 4000 && report.pid == 4242 && report.time == 1760000000);
  CHECK(report.signal == QStringLiteral("SIGSEGV"));
  CHECK(report.stack.size() == 5);
  CHECK(report.actions == QStringList({QStringLiteral("command help.send_feedback"), QStringLiteral("model add_feature")}));
  CHECK(!parseCrashReport("something else\nsignal: SIGSEGV\n").valid);

  const Frame frame = parseFrame(report.stack[2]);
  CHECK(frame.module == QStringLiteral("mitcad"));
#if __has_include(<cxxabi.h>)
  CHECK(frame.symbol == QStringLiteral("mitcad::crash::crashNow()"));
  // The handler and the signal's trampoline are left out.
  CHECK(normalizedFrames(report.stack, 2) ==
        QStringList({QStringLiteral("mitcad!mitcad::crash::crashNow()"), QStringLiteral("mitcad!main")}));
#endif
  // The handler's frame is left out without a demangler too (mitcad#73).
  CHECK(trimmedStack(report.stack) == report.stack.mid(2));
  CHECK(parseFrame(QStringLiteral("C:\\Mitcad\\bin\\TKernel.dll+0x1a2b")).module == QStringLiteral("TKernel.dll"));
  CHECK(parseFrame(QStringLiteral("C:\\Mitcad\\bin\\TKernel.dll+0x1a2b")).offset == QStringLiteral("0x1a2b"));
  const Frame mac = parseFrame(QStringLiteral("3   mitcad    0x0000000100003f40 _ZN6mitcad5crash8crashNowEv + 52"));
  CHECK(mac.module == QStringLiteral("mitcad") && mac.offset == QStringLiteral("0x34"));
  // Windows frames with their functions (dbghelp, mitcad#76): an abort's
  // stack loses the C runtime's frames, a function's offset does not count.
  const QStringList windows{QStringLiteral("C:\\Windows\\System32\\ucrtbase.dll!raise+0x1de"),
                            QStringLiteral("C:\\Windows\\System32\\ucrtbase.dll!abort+0x4e"),
                            QStringLiteral("C:\\Mitcad\\bin\\mitcad.exe!mitcad::Vector::operator+=+0x8"),
                            QStringLiteral("C:\\Mitcad\\bin\\mitcad.exe!main+0x55")};
  const Frame named = parseFrame(windows[2]);
  CHECK(named.module == QStringLiteral("mitcad.exe"));
  CHECK(named.symbol == QStringLiteral("mitcad::Vector::operator+="));
  CHECK(named.offset == QStringLiteral("0x8"));
  CHECK(trimmedStack(windows) == windows.mid(2));
  CHECK(normalizedFrames(windows, 2) == QStringList({QStringLiteral("mitcad.exe!mitcad::Vector::operator+="),
                                                     QStringLiteral("mitcad.exe!main")}));
  QStringList rebuilt = windows;
  rebuilt[2].replace(QStringLiteral("+0x8"), QStringLiteral("+0x1c"));
  CHECK(crashKey(QStringLiteral("SIGABRT"), windows) == crashKey(QStringLiteral("SIGABRT"), rebuilt));

  // The same crash at other addresses (another run, another install
  // folder): the same key. Another place: another key.
  QStringList moved = report.stack;
  for (QString& line : moved) {
    line.replace(QStringLiteral("0x55555"), QStringLiteral("0x56789"));
    line.replace(QStringLiteral("/opt/mitcad/bin/"), QStringLiteral("/home/x/Mitcad/"));
  }
  CHECK(crashKey(QStringLiteral("SIGSEGV"), report.stack) == crashKey(QStringLiteral("SIGSEGV"), moved));
  CHECK(crashKey(QStringLiteral("SIGSEGV"), report.stack).size() == 12);
  QStringList other = report.stack;
  other[2] = QStringLiteral("/opt/mitcad/bin/mitcad(_ZN6mitcad10MainWindow4saveEv+0x10) [0x5555555a2000]");
  CHECK(crashKey(QStringLiteral("SIGSEGV"), report.stack) != crashKey(QStringLiteral("SIGSEGV"), other));
  CHECK(crashKey(QStringLiteral("SIGSEGV"), report.stack) != crashKey(QStringLiteral("SIGBUS"), report.stack));
  // Without the handler's frames on top (a stack from another platform's
  // handler): still the same key.
  CHECK(crashKey(QStringLiteral("SIGSEGV"), report.stack) ==
        crashKey(QStringLiteral("SIGSEGV"), report.stack.mid(2)));

  // Internal errors: numbers, addresses and paths do not matter.
  CHECK(errorKey(QStringLiteral("recompute"), QStringLiteral("F6 Fillet1 (fillet): SIGSEGV at 0x7f12 in /home/a/x.f3d")) ==
        errorKey(QStringLiteral("recompute"), QStringLiteral("F9 Fillet3 (fillet): SIGSEGV at 0x0 in /tmp/y/z.f3d")));
  CHECK(isKernelCrashMessage(QStringLiteral("Recompute failed: F6 Fillet1: fillet: SIGSEGV 'segmentation violation' detected. Address 0.")));
  CHECK(isKernelCrashMessage(QStringLiteral("fillet: ACCESS VIOLATION at address 0x00000000 during 'READ' operation")));
  CHECK(!isKernelCrashMessage(QStringLiteral("fillet: BRep_API: command not done")));
  // A worker's crash has a report of its own.
  CHECK(!isKernelCrashMessage(QStringLiteral("Mitcad crashed: SIGSEGV; crash report: /tmp/x.crash")));
  CHECK(errorKey(QStringLiteral("recompute"), QStringLiteral("fillet: SIGSEGV")) !=
        errorKey(QStringLiteral("recompute"), QStringLiteral("chamfer: SIGSEGV")));
}

void reportLinkTests() {
  using namespace mitcad::report;
  const QString tracker = QStringLiteral("https://github.com/mitcad/Mitcad/issues");
  IssueLink link = issueLink(tracker, QStringLiteral("Bug: a & b"), QStringLiteral("line 1\nline+2 #3"));
  CHECK(!link.shortened && link.clipboard.isEmpty());
  CHECK(QString::fromLatin1(link.url.toEncoded()) ==
        QStringLiteral("https://github.com/mitcad/Mitcad/issues/new?title=Bug%3A%20a%20%26%20b&body=line%201%0Aline%2B2%20%233"));
  CHECK(issueLink(tracker + QStringLiteral("/new/"), QStringLiteral("t"), QStringLiteral("b")).url.toEncoded() ==
        QByteArray("https://github.com/mitcad/Mitcad/issues/new?title=t&body=b"));
  // A query of the tracker's own stays.
  CHECK(issueLink(QStringLiteral("https://example.org/o/r/issues/new?template=bug.md"), QStringLiteral("t"),
                  QStringLiteral("b"))
            .url.toEncoded() == QByteArray("https://example.org/o/r/issues/new?template=bug.md&title=t&body=b"));
  // A template.
  CHECK(issueLink(QStringLiteral("https://tracker.example/new?summary={title}&text={body}"), QStringLiteral("a b"),
                  QStringLiteral("c/d"))
            .url.toEncoded() == QByteArray("https://tracker.example/new?summary=a%20b&text=c%2Fd"));
  // Too long: cut at a line, the whole on the clipboard.
  QStringList lines;
  for (int i = 0; i < 2000; ++i) {
    lines << QStringLiteral("frame %1: mitcad::something::long_function_name(int, double) + 0x%2").arg(i).arg(i, 0, 16);
  }
  const QString body = lines.join(QLatin1Char('\n'));
  link = issueLink(tracker, QStringLiteral("Crash"), body);
  CHECK(link.shortened);
  CHECK(link.clipboard == body);
  CHECK(link.url.toEncoded().size() <= kMaxUrlLength);
  CHECK(link.url.toEncoded().size() > kMaxUrlLength - 200);
  const QString sent = QUrl::fromPercentEncoding(link.url.toEncoded());
  CHECK(sent.contains(QStringLiteral("The whole report is on the clipboard")));
  CHECK(sent.contains(QStringLiteral("frame 0: ")) && !sent.contains(QStringLiteral("frame 1999: ")));
  CHECK(sent.contains(QStringLiteral("[… cut: the rest is on the clipboard]")));

  // The body: included sections with text, code fenced, the key last.
  Report report;
  report.key = QStringLiteral("0123456789ab");
  report.sections = {Section{QStringLiteral("description"), QStringLiteral("What happened"), QStringLiteral("It broke."), true, false, true},
                     Section{QStringLiteral("stack"), QStringLiteral("Stack"), QStringLiteral("a\nb"), true, true, true},
                     Section{QStringLiteral("diagnostics"), QStringLiteral("Diagnostics"), QStringLiteral("x"), false, true, true},
                     Section{QStringLiteral("contact"), QStringLiteral("Contact"), QString(), true, false, false}};
  CHECK(reportBody(report, QStringLiteral("0.1.0")) ==
        QStringLiteral("### What happened\n\nIt broke.\n\n### Stack\n\n```\na\nb\n```\n\n"
                       "<sub>Sent from MitCad 0.1.0 · duplicate key `mitcad-0123456789ab`</sub>\n"));
}

// The addresses of Cloud projects (mitcad#89).
void cloudAddressTests() {
  using namespace mitcad;
  CloudAddress address;
  address.service = CloudService::GitHub;
  address.owner = QStringLiteral("you");
  address.repository = QStringLiteral("robot-arm");
  CHECK(address.url() == QStringLiteral("https://github.com/you/robot-arm.git"));
  address.connect = CloudConnect::Ssh;
  CHECK(address.url() == QStringLiteral("git@github.com:you/robot-arm.git"));
  CHECK(address.webPage() == QStringLiteral("https://github.com/you/robot-arm"));
  CHECK(address.newRepositoryPage() ==
        QStringLiteral("https://github.com/new?owner=you&name=robot-arm&visibility=private"));
  address.service = CloudService::Forgejo;
  CHECK(address.url().isEmpty()); // no server yet
  address.server = QStringLiteral("Code.Example.org");
  CHECK(address.url() == QStringLiteral("git@code.example.org:you/robot-arm.git"));
  CHECK(address.newRepositoryPage() == QStringLiteral("https://code.example.org/repo/create"));
  address.service = CloudService::GitLab;
  address.connect = CloudConnect::Https;
  CHECK(address.url() == QStringLiteral("https://gitlab.com/you/robot-arm.git"));
  // A GitLab group with subgroups as the owner (mitcad#142).
  address.owner = QStringLiteral("group/sub");
  CHECK(address.url() == QStringLiteral("https://gitlab.com/group/sub/robot-arm.git"));
  CHECK(address.webPage() == QStringLiteral("https://gitlab.com/group/sub/robot-arm"));
  address.connect = CloudConnect::Ssh;
  CHECK(address.url() == QStringLiteral("git@gitlab.com:group/sub/robot-arm.git"));
  address.connect = CloudConnect::Https;
  address.repository.clear();
  CHECK(address.url().isEmpty());
  // An organisation as GitHub's owner, also on its new repository page.
  address.service = CloudService::GitHub;
  address.owner = QStringLiteral("acme");
  address.repository = QStringLiteral("robot-arm");
  CHECK(address.url() == QStringLiteral("https://github.com/acme/robot-arm.git"));
  CHECK(address.newRepositoryPage() ==
        QStringLiteral("https://github.com/new?owner=acme&name=robot-arm&visibility=private"));

  // Read back from an address.
  CloudAddress parsed = parseCloudAddress(QStringLiteral("git@github.com:me/bracket.git"));
  CHECK(parsed.service == CloudService::GitHub && parsed.connect == CloudConnect::Ssh);
  CHECK(parsed.owner == QStringLiteral("me") && parsed.repository == QStringLiteral("bracket"));
  parsed = parseCloudAddress(QStringLiteral("https://gitlab.com/me/bracket"));
  CHECK(parsed.service == CloudService::GitLab && parsed.connect == CloudConnect::Https);
  CHECK(parsed.url() == QStringLiteral("https://gitlab.com/me/bracket.git"));
  parsed = parseCloudAddress(QStringLiteral("https://codeberg.org/me/bracket.git"));
  CHECK(parsed.service == CloudService::Forgejo && parsed.server == QStringLiteral("codeberg.org"));
  parsed = parseCloudAddress(QStringLiteral("ssh://git@git.example.org:2222/me/bracket.git"));
  CHECK(parsed.service == CloudService::Other && parsed.url() == QStringLiteral("ssh://git@git.example.org:2222/me/bracket.git"));
  parsed = parseCloudAddress(QStringLiteral("git@git.example.org:me/bracket.git"), QStringLiteral("git.example.org"));
  CHECK(parsed.service == CloudService::Forgejo && parsed.owner == QStringLiteral("me"));
  // Organisations' repositories (mitcad#142).
  parsed = parseCloudAddress(QStringLiteral("https://github.com/acme/bracket.git"));
  CHECK(parsed.service == CloudService::GitHub && parsed.owner == QStringLiteral("acme") &&
        parsed.repository == QStringLiteral("bracket"));
  CHECK(parsed.url() == QStringLiteral("https://github.com/acme/bracket.git"));
  parsed = parseCloudAddress(QStringLiteral("git@codeberg.org:acme/bracket.git"));
  CHECK(parsed.service == CloudService::Forgejo && parsed.server == QStringLiteral("codeberg.org") &&
        parsed.owner == QStringLiteral("acme") && parsed.repository == QStringLiteral("bracket"));
  CHECK(parsed.url() == QStringLiteral("git@codeberg.org:acme/bracket.git"));
  // GitLab's nested groups: every part but the last is the owner.
  parsed = parseCloudAddress(QStringLiteral("https://gitlab.com/group/sub/bracket.git"));
  CHECK(parsed.service == CloudService::GitLab && parsed.connect == CloudConnect::Https);
  CHECK(parsed.owner == QStringLiteral("group/sub") && parsed.repository == QStringLiteral("bracket"));
  CHECK(parsed.url() == QStringLiteral("https://gitlab.com/group/sub/bracket.git"));
  parsed = parseCloudAddress(QStringLiteral("git@gitlab.com:group/sub/deeper/bracket.git"));
  CHECK(parsed.service == CloudService::GitLab && parsed.connect == CloudConnect::Ssh);
  CHECK(parsed.owner == QStringLiteral("group/sub/deeper") && parsed.repository == QStringLiteral("bracket"));
  // GitHub and Forgejo have one owner level: deeper paths stay another
  // address, as do paths with a part starting with '~'.
  parsed = parseCloudAddress(QStringLiteral("https://github.com/acme/sub/bracket.git"));
  CHECK(parsed.service == CloudService::Other && parsed.url() == QStringLiteral("https://github.com/acme/sub/bracket.git"));
  parsed = parseCloudAddress(QStringLiteral("https://codeberg.org/acme/sub/bracket.git"));
  CHECK(parsed.service == CloudService::Other);
  parsed = parseCloudAddress(QStringLiteral("git@gitlab.com:~me/bracket.git"));
  CHECK(parsed.service == CloudService::Other);
  parsed = parseCloudAddress(QStringLiteral("https://gitlab.com/group/~sub/bracket.git"));
  CHECK(parsed.service == CloudService::Other);
  parsed = parseCloudAddress(QStringLiteral("/srv/git/bracket.git"));
  CHECK(parsed.service == CloudService::SharedFolder && parsed.url() == QStringLiteral("/srv/git/bracket.git"));
  parsed = parseCloudAddress(QStringLiteral("C:\\shared\\bracket.git"));
  CHECK(parsed.service == CloudService::SharedFolder && parsed.url() == QStringLiteral("C:/shared/bracket.git"));
  parsed = parseCloudAddress(QStringLiteral("file:///srv/git/bracket.git"));
  CHECK(parsed.service == CloudService::SharedFolder && parsed.url() == QStringLiteral("/srv/git/bracket.git"));

  // A pasted git address (mitcad#381): a self-hosted server needs no Server
  // field first; its owner and repository are read from the path.
  parsed = parseCloudAddress(QStringLiteral("https://git.example.org/team/parts.git"));
  CHECK(parsed.service == CloudService::Forgejo && parsed.server == QStringLiteral("git.example.org"));
  CHECK(parsed.owner == QStringLiteral("team") && parsed.repository == QStringLiteral("parts"));
  CHECK(parsed.connect == CloudConnect::Https && parsed.url() == QStringLiteral("https://git.example.org/team/parts.git"));
  parsed = parseCloudAddress(QStringLiteral("git@git.example.org:team/parts.git"));
  CHECK(parsed.service == CloudService::Forgejo && parsed.server == QStringLiteral("git.example.org"));
  CHECK(parsed.connect == CloudConnect::Ssh && parsed.owner == QStringLiteral("team") &&
        parsed.repository == QStringLiteral("parts"));
  CHECK(parsed.url() == QStringLiteral("git@git.example.org:team/parts.git"));
  parsed = parseCloudAddress(QStringLiteral("ssh://git@git.example.org/team/parts"));
  CHECK(parsed.service == CloudService::Forgejo && parsed.connect == CloudConnect::Ssh);
  CHECK(parsed.url() == QStringLiteral("git@git.example.org:team/parts.git"));
  // Another user, a port or a deeper path: used as it is.
  parsed = parseCloudAddress(QStringLiteral("ssh://git@git.example.org:2222/team/parts.git"));
  CHECK(parsed.service == CloudService::Other && parsed.url() == QStringLiteral("ssh://git@git.example.org:2222/team/parts.git"));
  parsed = parseCloudAddress(QStringLiteral("ssh://me@git.example.org/team/parts.git"));
  CHECK(parsed.service == CloudService::Other && parsed.url() == QStringLiteral("ssh://me@git.example.org/team/parts.git"));
  parsed = parseCloudAddress(QStringLiteral("https://git.example.org/a/b/parts.git"));
  CHECK(parsed.service == CloudService::Other && parsed.url() == QStringLiteral("https://git.example.org/a/b/parts.git"));
  // The address of a repository's page in a browser.
  parsed = parseCloudAddress(QStringLiteral("https://git.example.org/team/parts"));
  CHECK(parsed.service == CloudService::Forgejo && parsed.url() == QStringLiteral("https://git.example.org/team/parts.git"));
  parsed = parseCloudAddress(QStringLiteral("https://git.example.org/team/parts/src/branch/main/README.md"));
  CHECK(parsed.service == CloudService::Forgejo && parsed.owner == QStringLiteral("team") &&
        parsed.repository == QStringLiteral("parts"));
  parsed = parseCloudAddress(QStringLiteral("https://github.com/acme/bracket/tree/main/docs"));
  CHECK(parsed.service == CloudService::GitHub && parsed.owner == QStringLiteral("acme") &&
        parsed.repository == QStringLiteral("bracket"));
  parsed = parseCloudAddress(QStringLiteral("https://gitlab.com/group/sub/bracket/-/tree/main"));
  CHECK(parsed.service == CloudService::GitLab && parsed.owner == QStringLiteral("group/sub") &&
        parsed.repository == QStringLiteral("bracket"));
  // A self-hosted GitLab's page: its address, cut at the page, as it is.
  parsed = parseCloudAddress(QStringLiteral("https://git.example.org/team/parts/-/blob/main/a.txt"));
  CHECK(parsed.service == CloudService::Other && parsed.url() == QStringLiteral("https://git.example.org/team/parts"));
  // No scheme, or the command a page shows.
  parsed = parseCloudAddress(QStringLiteral("git.example.org/team/parts.git"));
  CHECK(parsed.service == CloudService::Forgejo && parsed.url() == QStringLiteral("https://git.example.org/team/parts.git"));
  parsed = parseCloudAddress(QStringLiteral("  git clone https://git.example.org/team/parts.git  "));
  CHECK(parsed.service == CloudService::Forgejo && parsed.repository == QStringLiteral("parts"));
  parsed = parseCloudAddress(QStringLiteral("git clone git@github.com:acme/bracket.git bracket"));
  CHECK(parsed.service == CloudService::GitHub && parsed.connect == CloudConnect::Ssh);
  // One path part, or the malformed git@host/owner/repo.git:owner/repo.git
  // (a path to git), stay another address.
  parsed = parseCloudAddress(QStringLiteral("https://git.example.org/parts.git"));
  CHECK(parsed.service == CloudService::Other);
  parsed = parseCloudAddress(QStringLiteral("git@git.example.org/team/parts.git:team/parts.git"));
  CHECK(parsed.service == CloudService::Other);

  // Pasted into one field: a whole address, or not.
  CHECK(looksLikeRepositoryAddress(QStringLiteral("https://git.example.org/team/parts.git"), false));
  CHECK(looksLikeRepositoryAddress(QStringLiteral("git@git.example.org:team/parts.git"), false));
  CHECK(looksLikeRepositoryAddress(QStringLiteral("git.example.org/team/parts.git"), true));
  CHECK(!looksLikeRepositoryAddress(QStringLiteral("git.example.org/team/parts.git"), false));
  CHECK(!looksLikeRepositoryAddress(QStringLiteral("git.example.org"), true));
  CHECK(!looksLikeRepositoryAddress(QStringLiteral("git.example.org/"), true));
  CHECK(!looksLikeRepositoryAddress(QStringLiteral("https://"), false));
  CHECK(!looksLikeRepositoryAddress(QStringLiteral("group/sub"), true));
  CHECK(!looksLikeRepositoryAddress(QStringLiteral("/srv/git/team/parts.git"), true));

  // What is no address: said before git is asked.
  const QString invalid = QStringLiteral("This is not a valid repository address: ");
  CHECK(repositoryAddressProblem(QString()).isEmpty());
  CHECK(repositoryAddressProblem(QStringLiteral("https://git.example.org/team/parts.git")).isEmpty());
  CHECK(repositoryAddressProblem(QStringLiteral("git@git.example.org:team/parts.git")).isEmpty());
  CHECK(repositoryAddressProblem(QStringLiteral("ssh://git@git.example.org:2222/team/parts.git")).isEmpty());
  CHECK(repositoryAddressProblem(QStringLiteral("git.example.org/team/parts.git")).isEmpty());
  CHECK(repositoryAddressProblem(QStringLiteral("/srv/git/parts.git")).isEmpty());
  CHECK(repositoryAddressProblem(QStringLiteral("C:\\shared\\parts.git")).isEmpty());
  CHECK(repositoryAddressProblem(QStringLiteral("file:///srv/git/parts.git")).isEmpty());
  CHECK(repositoryAddressProblem(QStringLiteral("git@git.example.org/team/parts.git:team/parts.git")) ==
        invalid + QStringLiteral("git@git.example.org/team/parts.git:team/parts.git"));
  CHECK(repositoryAddressProblem(QStringLiteral("parts")) == invalid + QStringLiteral("parts"));
  CHECK(repositoryAddressProblem(QStringLiteral("team/parts.git")) == invalid + QStringLiteral("team/parts.git"));
  CHECK(!repositoryAddressProblem(QStringLiteral("https://")).isEmpty());
  CHECK(!repositoryAddressProblem(QStringLiteral("https://git.example.org")).isEmpty());
  CHECK(!repositoryAddressProblem(QStringLiteral("ftp://git.example.org/team/parts.git")).isEmpty());
  CHECK(!repositoryAddressProblem(QStringLiteral("git@git.example.org:")).isEmpty());

  CHECK(repositoryNameFor(QStringLiteral("Robot arm")) == QStringLiteral("robot-arm"));
  CHECK(repositoryNameFor(QStringLiteral("  My  Part (v2) ")) == QStringLiteral("my-part-v2"));
  CHECK(repositoryNameOf(QStringLiteral("git@github.com:me/bracket.git")) == QStringLiteral("bracket"));
  CHECK(repositoryNameOf(QStringLiteral("/srv/git/bracket.git/")) == QStringLiteral("bracket"));
  CHECK(repositoryAddressOf(QStringLiteral("git@github.com:acme/gripper.git")) ==
        QStringLiteral("github.com/acme/gripper"));
  CHECK(repositoryAddressOf(QStringLiteral("https://gitlab.com/group/sub/part")) ==
        QStringLiteral("gitlab.com/group/sub/part"));
  CHECK(repositoryAddressOf(QStringLiteral("https://codeberg.org/acme/arm.git")) ==
        QStringLiteral("codeberg.org/acme/arm"));
  CHECK(repositoryAddressOf(QStringLiteral("https://code.example.org/git/team/arm.git")) ==
        QStringLiteral("code.example.org/git/team/arm"));
  CHECK(repositoryAddressOf(QStringLiteral("ssh://git@Example.org:2222/team/arm.git/")) ==
        QStringLiteral("example.org/team/arm"));
  CHECK(repositoryAddressOf(QStringLiteral("/srv/git/bracket.git/")) ==
        QDir::toNativeSeparators(QStringLiteral("/srv/git/bracket.git")));
  CHECK(repositoryAddressOf(QStringLiteral("file:///srv/git/bracket.git")) ==
        QDir::toNativeSeparators(QStringLiteral("/srv/git/bracket.git")));
  CHECK(addressHost(QStringLiteral("ssh://git@Example.org:2222/x.git")) == QStringLiteral("example.org"));
  CHECK(addressHost(QStringLiteral("C:\\x")).isEmpty());
  CHECK(isSshAddress(QStringLiteral("git@github.com:me/x.git")) && !isSshAddress(QStringLiteral("https://h/x")));
  CHECK(addressHasPassword(QStringLiteral("https://me:token@github.com/me/x.git")));
  CHECK(!addressHasPassword(QStringLiteral("https://me@github.com/me/x.git")));
  CHECK(sameRepository(QStringLiteral("https://GitHub.com/me/x"), QStringLiteral("https://github.com/me/x.git/")));
  CHECK(!sameRepository(QStringLiteral("https://github.com/me/x"), QStringLiteral("https://github.com/me/y")));
  CHECK(sameRepository(QStringLiteral("/srv/git/x.git"), QStringLiteral("/srv/git/./x.git")));
  CHECK(addressWebPage(QStringLiteral("git@github.com:me/x.git")) == QStringLiteral("https://github.com/me/x"));
}

// The timeline search's words and filters (mitcad#179).
void timelineSearchTests() {
  using mitcad::TimelineQuery;
  using mitcad::TimelineSearchItem;
  TimelineSearchItem fillet;
  fillet.name = QStringLiteral("Fillet2");
  fillet.type = QStringLiteral("fillet");
  fillet.component = QStringLiteral("Left Arm");
  TimelineSearchItem plane;
  plane.name = QStringLiteral("Datum");
  plane.type = QStringLiteral("construction_plane");
  plane.component = QStringLiteral("Base");
  plane.warned = true;
  plane.rolledBack = true;
  TimelineSearchItem broken = fillet;
  broken.name = QStringLiteral("Fillet21");
  broken.component = QStringLiteral("Base");
  broken.failed = true;

  // Words: the name or the type, ignoring case, all of them.
  CHECK(TimelineQuery::parse(QStringLiteral("fillet2")).matches(fillet));
  CHECK(TimelineQuery::parse(QStringLiteral("FILLET2")).matches(broken)); // a substring
  CHECK(TimelineQuery::parse(QStringLiteral("construction plane")).matches(plane));
  CHECK(TimelineQuery::parse(QStringLiteral("\"construction plane\"")).matches(plane));
  CHECK(!TimelineQuery::parse(QStringLiteral("fillet plane")).matches(fillet));
  CHECK(TimelineQuery::parse(QStringLiteral("  ")).isEmpty());
  // Components: in: or component:, quoted names with spaces, alternatives.
  CHECK(TimelineQuery::parse(QStringLiteral("fillet in:\"left arm\"")).matches(fillet));
  CHECK(!TimelineQuery::parse(QStringLiteral("fillet in:\"left arm\"")).matches(broken));
  CHECK(TimelineQuery::parse(QStringLiteral("component:base")).matches(broken));
  CHECK(TimelineQuery::parse(QStringLiteral("in:arm in:base")).matches(plane));
  CHECK(TimelineQuery::parse(QStringLiteral("in:arm in:base")).matches(fillet));
  // A filter being typed (no value yet) is ignored.
  CHECK(TimelineQuery::parse(QStringLiteral("fillet2 in:")).matches(fillet));
  CHECK(TimelineQuery::parse(QStringLiteral("is:")).isEmpty());
  // Statuses, abbreviated, alternatives; across kinds all apply.
  CHECK(TimelineQuery::parse(QStringLiteral("is:failed")).matches(broken));
  CHECK(!TimelineQuery::parse(QStringLiteral("is:failed")).matches(fillet));
  CHECK(TimelineQuery::parse(QStringLiteral("is:f")).statuses == TimelineQuery::Failed);
  CHECK(TimelineQuery::parse(QStringLiteral("status:error")).statuses == TimelineQuery::Failed);
  CHECK(TimelineQuery::parse(QStringLiteral("is:note")).statuses == TimelineQuery::Warning);
  CHECK(TimelineQuery::parse(QStringLiteral("is:rolled-back")).statuses == TimelineQuery::RolledBack);
  CHECK(TimelineQuery::parse(QStringLiteral("is:rolled_back")).statuses == TimelineQuery::RolledBack);
  CHECK(TimelineQuery::parse(QStringLiteral("is:sup")).statuses == TimelineQuery::Suppressed);
  CHECK(TimelineQuery::parse(QStringLiteral("is:warning")).matches(plane));
  CHECK(TimelineQuery::parse(QStringLiteral("is:failed is:warning")).matches(plane));
  CHECK(TimelineQuery::parse(QStringLiteral("is:failed is:warning")).matches(broken));
  CHECK(!TimelineQuery::parse(QStringLiteral("is:warning in:arm")).matches(plane));
  // A status it does not know matches nothing and is reported.
  const TimelineQuery unknown = TimelineQuery::parse(QStringLiteral("fillet is:broken"));
  CHECK(unknown.unknown == QStringList{QStringLiteral("is:broken")});
  CHECK(!unknown.matches(fillet));

  // The field's menu adds and takes out filters.
  using mitcad::toggleTimelineToken;
  CHECK(mitcad::timelineComponentToken(QStringLiteral("Left Arm")) == QStringLiteral("in:\"Left Arm\""));
  CHECK(mitcad::timelineStatusToken(TimelineQuery::RolledBack) == QStringLiteral("is:rolled-back"));
  CHECK(toggleTimelineToken(QStringLiteral("fillet"), QStringLiteral("is:failed")) ==
        QStringLiteral("fillet is:failed"));
  CHECK(toggleTimelineToken(QStringLiteral("fillet IS:Failed in:base"), QStringLiteral("is:failed")) ==
        QStringLiteral("fillet in:base"));
  CHECK(toggleTimelineToken(QStringLiteral("in:\"left arm\" x"), QStringLiteral("in:\"Left Arm\"")) ==
        QStringLiteral("x"));
  CHECK(toggleTimelineToken(QString(), QStringLiteral("in:\"Left Arm\"")) == QStringLiteral("in:\"Left Arm\""));
  CHECK(mitcad::hasTimelineToken(QStringLiteral("a component:Base"), QStringLiteral("in:base")));
  CHECK(!mitcad::hasTimelineToken(QStringLiteral("base"), QStringLiteral("in:base")));
}

// The sketch's auto constraints while drawing (mitcad#189): the choices a
// tool offers, its session's first and last entities through undo, the
// constraints' definitions and the geometry that satisfies them.
void autoConstraintTests() {
  using namespace mitcad::sketch;
  const auto s = [](const char* text) { return QString::fromLatin1(text); };
  const auto nearV = [](const V2& a, const V2& b) { return dist(a, b) <= 1e-9; };

  // What each tool offers, with short labels that do not name the reference.
  CHECK(autoConstraintTypes(AutoKind::Line) == (QStringList{s("equal"), s("parallel"), s("perpendicular")}));
  CHECK(autoConstraintTypes(AutoKind::Circular) ==
        (QStringList{s("equal"), s("horizontal_vertical"), s("concentric")}));
  CHECK(autoConstraintTypes(AutoKind::None).isEmpty());
  CHECK(autoConstraintTypes().size() == 5);
  CHECK(autoConstraintLabel(s("equal")) == s("Equal"));
  CHECK(autoConstraintLabel(s("horizontal_vertical")) == s("Horizontal/Vertical alignment"));
  CHECK(autoConstraintLabel(s("concentric")) == s("Concentric"));
  CHECK(autoConstraintLabel(s("parallel")) == s("Parallel"));
  CHECK(autoConstraintLabel(s("perpendicular")) == s("Perpendicular"));
  for (const QString& type : autoConstraintTypes()) {
    CHECK(!autoConstraintLabel(type).contains(s("first"), Qt::CaseInsensitive));
    CHECK(!autoConstraintLabel(type).contains(s("last"), Qt::CaseInsensitive));
  }

  // A new session: Link to last, nothing checked, nothing drawn.
  QSet<QString> existing;
  const AutoConstraintSession::Exists exists = [&existing](const QString& id) { return existing.contains(id); };
  AutoConstraintSession session;
  session.reset(AutoKind::Circular);
  CHECK(session.link() == AutoConstraintSession::Link::Last);
  CHECK(session.checkedTypes().isEmpty());
  CHECK(session.reference(exists).isEmpty());
  // Only what the tool offers can be checked; parallel and perpendicular,
  // concentric and the centre alignment exclude each other.
  CHECK(!session.setChecked(s("parallel"), true));
  CHECK(session.setChecked(s("equal"), true));
  CHECK(!session.setChecked(s("equal"), true));
  CHECK(session.setChecked(s("horizontal_vertical"), true));
  CHECK(session.checkedTypes() == (QStringList{s("equal"), s("horizontal_vertical")}));
  CHECK(session.setChecked(s("concentric"), true));
  CHECK(session.checkedTypes() == (QStringList{s("equal"), s("concentric")}));
  AutoConstraintSession lines;
  lines.reset(AutoKind::Line);
  CHECK(lines.setChecked(s("parallel"), true));
  CHECK(lines.setChecked(s("perpendicular"), true));
  CHECK(lines.checkedTypes() == QStringList{s("perpendicular")});
  CHECK(lines.setChecked(s("perpendicular"), false));
  CHECK(lines.checkedTypes().isEmpty());

  // Three circles linked to the last: 1-2 and 2-3; to the first: 1-2 and 1-3.
  const auto draw = [&](AutoConstraintSession& on, const QString& id) {
    const QString reference = on.reference(exists);
    existing.insert(id);
    on.record(id, exists);
    return reference;
  };
  CHECK(draw(session, s("c1")).isEmpty());
  CHECK(draw(session, s("c2")) == s("c1"));
  CHECK(draw(session, s("c3")) == s("c2"));
  // A changed mode counts from the next entity; the entities stay.
  session.setLink(AutoConstraintSession::Link::First);
  CHECK(session.entities() == (QStringList{s("c1"), s("c2"), s("c3")}));
  CHECK(draw(session, s("c4")) == s("c1"));
  CHECK(draw(session, s("c5")) == s("c1"));
  existing.clear();
  AutoConstraintSession first;
  first.reset(AutoKind::Circular);
  first.setLink(AutoConstraintSession::Link::First);
  CHECK(draw(first, s("c1")).isEmpty());
  CHECK(draw(first, s("c2")) == s("c1"));
  CHECK(draw(first, s("c3")) == s("c1"));

  // Undo: the reference is among the entities that remain; none left,
  // none referenced. An id undo freed may come again for a new entity.
  existing.clear();
  AutoConstraintSession undone;
  undone.reset(AutoKind::Line);
  draw(undone, s("c1"));
  draw(undone, s("c2"));
  draw(undone, s("c3"));
  existing.remove(s("c3"));
  CHECK(undone.reference(exists) == s("c2"));
  existing.insert(s("c3")); // redo
  CHECK(undone.reference(exists) == s("c3"));
  existing.remove(s("c3"));
  existing.remove(s("c2"));
  CHECK(draw(undone, s("c2")) == s("c1"));
  CHECK(undone.entities() == (QStringList{s("c1"), s("c2")}));
  undone.setLink(AutoConstraintSession::Link::First);
  existing.remove(s("c1"));
  CHECK(undone.reference(exists) == s("c2"));
  existing.clear();
  CHECK(undone.reference(exists).isEmpty());
  // A new tool starts over.
  undone.reset(AutoKind::Circular);
  CHECK(undone.entities().isEmpty() && undone.link() == AutoConstraintSession::Link::Last);

  // The definitions: from the reference to the new entity.
  AutoEntity c1;
  c1.id = s("c2");
  c1.kind = AutoKind::Circular;
  c1.center = {0.0, 0.0};
  c1.radius = 10.0;
  c1.centerPoint = s("p1");
  AutoEntity c2 = c1;
  c2.id = s("c4");
  c2.center = {30.0, 4.0};
  c2.radius = 5.0;
  c2.centerPoint = s("p3");
  QJsonObject equal = autoConstraintDefinition(s("equal"), c1, c2);
  CHECK(equal.value(s("type")).toString() == s("equal"));
  CHECK(equal.value(s("a")).toString() == s("c2") && equal.value(s("b")).toString() == s("c4"));
  CHECK(autoConstraintDefinition(s("concentric"), c1, c2).value(s("type")).toString() == s("concentric"));
  const QJsonObject level = autoConstraintDefinition(s("horizontal_vertical"), c1, c2);
  CHECK(level.value(s("type")).toString() == s("horizontal_points"));
  CHECK(level.value(s("a")).toString() == s("p1") && level.value(s("b")).toString() == s("p3"));
  c2.center = {3.0, 40.0};
  CHECK(autoConstraintDefinition(s("horizontal_vertical"), c1, c2).value(s("type")).toString() ==
        s("vertical_points"));
  CHECK(autoConstraintDefinition(s("parallel"), c1, c2).isEmpty());
  CHECK(!autoConstraintShared(s("concentric"), c1, c2));
  c2.centerPoint = s("p1");
  CHECK(autoConstraintDefinition(s("horizontal_vertical"), c1, c2).isEmpty());
  CHECK(autoConstraintShared(s("horizontal_vertical"), c1, c2));
  CHECK(autoConstraintShared(s("concentric"), c1, c2));
  CHECK(!autoConstraintShared(s("equal"), c1, c2));
  AutoEntity l1;
  l1.id = s("c1");
  l1.kind = AutoKind::Line;
  l1.a = {0.0, 0.0};
  l1.b = {20.0, 0.0};
  AutoEntity l2 = l1;
  l2.id = s("c5");
  CHECK(autoConstraintDefinition(s("perpendicular"), l1, l2).value(s("type")).toString() == s("perpendicular"));
  CHECK(autoConstraintDefinition(s("parallel"), l1, l2).value(s("b")).toString() == s("c5"));
  CHECK(autoConstraintDefinition(s("concentric"), l1, l2).isEmpty());
  CHECK(autoConstraintDefinition(s("equal"), l1, c1).isEmpty());
  CHECK(autoConstraintDefinition(s("equal"), l1, l1).isEmpty());
  CHECK(alignsHorizontally({0.0, 0.0}, {5.0, 5.0}));
  CHECK(!alignsHorizontally({0.0, 0.0}, {4.0, 5.0}));

  // A line placed for them: square to the reference, as long, both; along
  // it backwards; a typed length kept.
  const V2 start{20.0, 0.0};
  CHECK(nearV(autoLineEnd(start, {23.0, 15.0}, l1, {s("perpendicular")}, false), {20.0, 15.0}));
  CHECK(nearV(autoLineEnd(start, {23.0, 15.0}, l1, {s("perpendicular"), s("equal")}, false), {20.0, 20.0}));
  CHECK(nearV(autoLineEnd(start, {5.0, 3.0}, l1, {s("parallel")}, false), {5.0, 0.0}));
  CHECK(nearV(autoLineEnd(start, {23.0, 4.0}, l1, {s("equal")}, false), {32.0, 16.0}));
  CHECK(nearV(autoLineEnd({0.0, 10.0}, {6.0, 18.0}, l1, {s("parallel")}, true), {10.0, 10.0}));
  CHECK(nearV(autoLineEnd({0.0, 10.0}, {6.0, 18.0}, l1, {s("equal")}, true), {6.0, 18.0}));
  CHECK(nearV(autoLineEnd(start, {23.0, 4.0}, l1, {}, false), {23.0, 4.0}));
  CHECK(nearV(autoLineEnd(start, {23.0, 4.0}, c1, {s("equal")}, false), {23.0, 4.0}));

  // A circle placed for them: equal, aligned (level or plumb by the
  // nearer), concentric (through its rim), a typed size kept.
  const AutoCircle drawn{{30.0, 4.0}, 5.0};
  AutoCircle placed = autoCircle(drawn, c1, {s("equal")}, false);
  CHECK(nearV(placed.center, {30.0, 4.0}) && std::abs(placed.radius - 10.0) <= 1e-9);
  placed = autoCircle(drawn, c1, {s("equal"), s("horizontal_vertical")}, false);
  CHECK(nearV(placed.center, {30.0, 0.0}) && std::abs(placed.radius - 10.0) <= 1e-9);
  placed = autoCircle({{3.0, 40.0}, 5.0}, c1, {s("horizontal_vertical")}, false);
  CHECK(nearV(placed.center, {0.0, 40.0}) && std::abs(placed.radius - 5.0) <= 1e-9);
  placed = autoCircle(drawn, c1, {s("concentric")}, false, V2{40.0, 0.0});
  CHECK(nearV(placed.center, {0.0, 0.0}) && std::abs(placed.radius - 40.0) <= 1e-9);
  placed = autoCircle(drawn, c1, {s("concentric")}, false);
  CHECK(nearV(placed.center, {0.0, 0.0}) && std::abs(placed.radius - 5.0) <= 1e-9);
  placed = autoCircle(drawn, c1, {s("equal")}, true);
  CHECK(std::abs(placed.radius - 5.0) <= 1e-9);
  placed = autoCircle(drawn, l1, {s("equal"), s("concentric")}, false);
  CHECK(nearV(placed.center, drawn.center) && std::abs(placed.radius - 5.0) <= 1e-9);
}

// Selection inputs keep a region once, and a click toggles it (mitcad#199).
void selectionEntryTests() {
  {
    using namespace mitcad;
    DocumentSnapshot snapshot;
    DocumentSnapshot::Occurrence bench;
    bench.uid = QStringLiteral("O1");
    bench.name = QStringLiteral("Bench:1");
    DocumentSnapshot::Occurrence screw;
    screw.uid = QStringLiteral("O4");
    screw.name = QStringLiteral("Screw <M8>:3");
    bench.children.append(screw);
    snapshot.occurrences.append(bench);
    DocumentSnapshot::Body body;
    body.uid = QStringLiteral("F2.b0");
    body.name = QStringLiteral("Screw & washer");
    snapshot.bodies.insert(body.uid, body);
    SelectionItem item{SelectKind::Edge, body.uid, QStringLiteral("edge"), QString()};
    CHECK(selectionOwnerText(item, snapshot) == QStringLiteral("Body: Screw &amp; washer"));
    item.occurrence = QStringLiteral("O1/O4");
    const QString expected = QStringLiteral("Component: Bench:1 › Screw &lt;M8&gt;:3<br>Body: Screw &amp; washer");
    for (const SelectKind kind : {SelectKind::Face, SelectKind::Edge, SelectKind::Vertex, SelectKind::Body}) {
      item.kind = kind;
      CHECK(selectionOwnerText(item, snapshot) == expected);
    }
    item.kind = SelectKind::Sketch;
    CHECK(selectionOwnerText(item, snapshot).isEmpty());
    item.kind = SelectKind::Body;
    item.owner = QStringLiteral("F9.b0");
    item.occurrence.clear();
    CHECK(selectionOwnerText(item, snapshot) == QStringLiteral("Body: F9.b0"));
  }
  const QString line = mitcad::selectionMeasurementText(QJsonObject{{QStringLiteral("length"), 12.5}});
  CHECK(line.contains(QStringLiteral("12.5 mm")));
  CHECK(!line.contains(QStringLiteral("Radius")));
  const QString arc = mitcad::selectionMeasurementText(
      QJsonObject{{QStringLiteral("length"), 15.707963},
                  {QStringLiteral("circle"), QJsonObject{{QStringLiteral("radius"), 5.0}}}});
  CHECK(arc.contains(QStringLiteral("Radius: <b>5 mm")));
  CHECK(arc.contains(QStringLiteral("Diameter: <b>10 mm")));
  CHECK(mitcad::selectionMeasurementText(QJsonObject{{QStringLiteral("area"), 0.000025}})
            .contains(QStringLiteral("2.5e-05 mm²")));
  CHECK(mitcad::selectionMeasurementText(QJsonObject()).isEmpty());
  using mitcad::Selection;
  using mitcad::SelectionItem;
  using mitcad::SelectKind;
  const auto placedProfile = [](const char* sketch, const char* region, const char* occurrence) {
    SelectionItem item{SelectKind::Profile, QString::fromLatin1(sketch), QString::fromLatin1(region),
                       QStringLiteral("plane")};
    item.occurrence = QString::fromLatin1(occurrence);
    return item;
  };
  const auto profile = [&placedProfile](const char* sketch, const char* region) {
    return placedProfile(sketch, region, "");
  };
  const SelectionItem ring = profile("F1", "r{c1}");
  const SelectionItem disc = profile("F1", "r{c2}");
  const SelectionItem inner = profile("F1", "r{c3}");
  // The newest profile (no occurrence) and the same region clicked in a
  // placed component's sketch are one entry: the click takes it out.
  const SelectionItem placed = placedProfile("F1", "r{c1}", "O1");
  CHECK(mitcad::sameEntry(ring, placed));
  Selection items{ring};
  CHECK(!mitcad::toggleEntry(items, placed));
  CHECK(items.isEmpty());
  // Clicks in any order give the same entries; a second click on one gives
  // back the selection before it.
  Selection forward;
  for (const SelectionItem& item : {ring, disc, inner}) {
    CHECK(mitcad::toggleEntry(forward, item));
  }
  Selection backward;
  for (const SelectionItem& item : {inner, ring, disc, inner, inner}) {
    mitcad::toggleEntry(backward, item);
  }
  CHECK(forward.size() == 3 && backward.size() == 3);
  for (const SelectionItem& item : forward) {
    CHECK(mitcad::containsEntry(backward, item));
  }
  Selection again = forward;
  CHECK(!mitcad::toggleEntry(again, disc));
  CHECK(again.size() == 2 && !mitcad::containsEntry(again, disc));
  CHECK(mitcad::toggleEntry(again, disc));
  CHECK(again.size() == 3);
  // Other kinds and other sketches are not merged.
  CHECK(!mitcad::sameEntry(ring, profile("F4", "r{c1}")));
  const SelectionItem face{SelectKind::Face, QStringLiteral("F2.b0"), QStringLiteral("r{c1}"), QString()};
  CHECK(!mitcad::sameEntry(ring, face));
  SelectionItem otherFace = face;
  otherFace.occurrence = QStringLiteral("O1");
  CHECK(!mitcad::sameEntry(face, otherFace)); // a body placed twice is two items
  // A key that changed with the sketch: the resolver names the region both
  // keys mean, so a click on the region takes the old entry out.
  const SelectionItem old = profile("F1", "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}");
  const SelectionItem current = profile("F1", "r{c1[c4,c5],c3[c5,c4],c4[c3,c1],c5[c1,c3]}");
  int asked = 0;
  const mitcad::ItemResolver resolve = [&](const SelectionItem& item) {
    ++asked;
    return QStringList{item.name == old.name ? current.name : item.name};
  };
  CHECK(!mitcad::sameEntry(old, current));
  CHECK(mitcad::sameEntry(old, current, resolve));
  CHECK(asked == 2);
  Selection loaded{old, disc};
  CHECK(!mitcad::toggleEntry(loaded, current, resolve));
  CHECK(loaded.size() == 1 && loaded.first() == disc);
  // A resolver that cannot tell (empty) leaves different keys apart.
  const mitcad::ItemResolver unknown = [](const SelectionItem&) { return QStringList(); };
  CHECK(!mitcad::sameEntry(old, current, unknown));
  // A list given as a whole keeps each profile once, the first in place
  // (the old key, the disc without an occurrence), and other items as they
  // are.
  const Selection given{old, face, disc, current, placedProfile("F1", "r{c2}", "O1"), face};
  const Selection unique = mitcad::uniqueProfiles(given, resolve);
  CHECK(unique.size() == 4);
  CHECK(unique.size() == 4 && unique[0] == old && unique[1] == face && unique[2] == disc && unique[3] == face);
  CHECK(mitcad::removeEntry(backward, placed) == 1);
  // A region a sketch edit split (mitcad#410): the old key means all its
  // pieces, the main one first; the view shows each piece under its own
  // key. Every piece is part of the entry, and a click on one takes just
  // that piece out: the others stay, each under its own key, in the
  // entry's place.
  const SelectionItem split = placedProfile("F1", "r{c1[c2,c3],c2[c3,c1],c3[c1,c2]}", "O1");
  const SelectionItem mainPiece = profile("F1", "r{c1[c2,c4],c2[c4,c1],c4[c1,c2]}");
  const SelectionItem side = profile("F1", "r{c3[c4,c5],c4[c5,c3],c5[c3,c4]}");
  const SelectionItem tip = profile("F1", "r{c3[c5,c6],c5[c6,c3],c6[c3,c5]}");
  const mitcad::ItemResolver pieces = [&](const SelectionItem& item) {
    return item.name == split.name ? QStringList{mainPiece.name, side.name, tip.name} : QStringList{item.name};
  };
  CHECK(!mitcad::sameEntry(split, side, pieces));
  CHECK(mitcad::containsEntry({split}, mainPiece, pieces) && mitcad::containsEntry({split}, side, pieces));
  CHECK(!mitcad::containsEntry({mainPiece}, split, pieces)); // the piece is not all of the region
  Selection clicked{disc, split, inner};
  CHECK(!mitcad::toggleEntry(clicked, side, pieces));
  CHECK(clicked.size() == 4 && clicked[0] == disc && clicked[1].name == mainPiece.name &&
        clicked[2].name == tip.name && clicked[3] == inner);
  CHECK(clicked[1].occurrence == QStringLiteral("O1")); // placed where the entry was
  // The same piece again gives back the area selected before.
  CHECK(mitcad::toggleEntry(clicked, side, pieces));
  CHECK(clicked.size() == 5 && mitcad::containsEntry(clicked, side, pieces));
  // The main piece (which carries the old key in the model) the same.
  Selection mainOff{split};
  CHECK(mitcad::removeEntry(mainOff, mainPiece, pieces) == 1);
  CHECK(mainOff.size() == 2 && mainOff[0].name == side.name && mainOff[1].name == tip.name);
  // A piece listed after its region goes; before it, both stay (the region
  // means more), and a click on another piece keeps the listed one once.
  CHECK(mitcad::uniqueProfiles({split, tip}, pieces).size() == 1);
  Selection both = mitcad::uniqueProfiles({tip, split}, pieces);
  CHECK(both.size() == 2);
  CHECK(mitcad::removeEntry(both, mainPiece, pieces) == 1);
  CHECK(both.size() == 2 && both[0] == tip && both[1].name == side.name);
}

// Whether a rendered image still fits the view (mitcad#216): the same
// camera and size match, rounding noise too; a move, a zoom, a turn, another
// size or projection do not.
void viewMatchTests() {
  using mitcad::ViewPose;
  ViewPose view;
  view.eye = {70.0, -19.999999999999993, 49.999999999999993};
  view.center = {30.000000000000007, 20.0, 10.0};
  view.up = {-0.40824829046386296, 0.40824829046386307, 0.81649658092772603};
  view.scale = 81.317279836452968;
  view.orthographic = true;
  view.width = 480;
  view.height = 539;
  CHECK(mitcad::sameView(view, view));
  ViewPose copy = view;
  CHECK(mitcad::sameView(view, copy));
  ViewPose noise = view;
  noise.eye[0] += 1e-12;
  noise.center[2] -= 1e-12;
  noise.up[1] += 1e-15;
  noise.scale *= 1.0 + 1e-13;
  CHECK(mitcad::sameView(view, noise));
  ViewPose moved = view;
  moved.eye[0] += 0.01;
  moved.center[0] += 0.01;
  CHECK(!mitcad::sameView(view, moved));
  ViewPose zoomed = view;
  zoomed.scale *= 0.999;
  CHECK(!mitcad::sameView(view, zoomed));
  ViewPose turned = view;
  turned.up = {0.0, 0.0, 1.0};
  CHECK(!mitcad::sameView(view, turned));
  ViewPose wider = view;
  wider.width += 1;
  CHECK(!mitcad::sameView(view, wider));
  ViewPose perspective = view;
  perspective.orthographic = false;
  CHECK(!mitcad::sameView(view, perspective));
}

// When a navigation of the view ends, for the rendered view (mitcad#214).
void viewNavigationTests() {
  using mitcad::ViewNavigation;
  using Change = ViewNavigation::Change;
  constexpr int quiet = ViewNavigation::kQuietMilliseconds;
  ViewNavigation navigation;
  CHECK(!navigation.active());
  CHECK(navigation.update(0, false) == Change::None);
  // A held button navigates until it goes up, however long it keeps still.
  navigation.setButtons(true);
  CHECK(navigation.update(10, false) == Change::Started && navigation.active());
  CHECK(navigation.update(10 + 10 * quiet, false) == Change::None && navigation.active());
  navigation.setButtons(false);
  CHECK(navigation.update(20 + 10 * quiet, false) == Change::Ended && !navigation.active());
  CHECK(navigation.update(30 + 10 * quiet, false) == Change::None);
  // A burst of wheel steps 50 ms apart is one navigation, which ends once
  // the quiet time has passed after the last step.
  ViewNavigation wheel;
  std::int64_t now = 1000;
  wheel.stepped(now);
  CHECK(wheel.update(now, false) == Change::Started);
  for (int i = 0; i < 5; ++i) {
    now += 50;
    wheel.stepped(now);
    wheel.cameraMoved(now);
    CHECK(wheel.update(now, false) == Change::None && wheel.active());
  }
  CHECK(wheel.quietLeft(now) == quiet);
  CHECK(wheel.update(now + quiet - 1, false) == Change::None && wheel.active());
  CHECK(wheel.quietLeft(now + quiet - 1) == 1);
  CHECK(wheel.update(now + quiet, false) == Change::Ended && !wheel.active());
  CHECK(wheel.quietLeft(now + quiet) == 0);
  // The camera still moving after the last step (an animation of the zoom,
  // a trackpad's momentum) holds the end back; a camera change outside a
  // navigation starts none.
  wheel.stepped(now = 5000);
  CHECK(wheel.update(now, false) == Change::Started);
  wheel.cameraMoved(now + 200);
  CHECK(wheel.update(now + quiet + 10, false) == Change::None);
  CHECK(wheel.update(now + 200 + quiet, false) == Change::Ended);
  wheel.cameraMoved(now + 1000);
  CHECK(wheel.update(now + 1000, false) == Change::None && !wheel.active());
  // A camera animation navigates while it runs; a trackpad scroll's drag
  // while it is on, and its quiet time after it.
  ViewNavigation other;
  CHECK(other.update(0, true) == Change::Started);
  CHECK(other.update(400, true) == Change::None);
  CHECK(other.update(410, false) == Change::Ended);
  other.stepped(1000);
  other.setScrollDrag(true);
  CHECK(other.update(1000, false) == Change::Started);
  CHECK(other.update(1000 + 2 * quiet, false) == Change::None);
  other.setScrollDrag(false);
  CHECK(other.update(1000 + 2 * quiet + 1, false) == Change::Ended);
  // A button pressed during a burst: the navigation goes on until it is
  // released, past the quiet time.
  ViewNavigation mixed;
  mixed.stepped(0);
  CHECK(mixed.update(0, false) == Change::Started);
  mixed.setButtons(true);
  CHECK(mixed.update(2 * quiet, false) == Change::None);
  mixed.setButtons(false);
  CHECK(mixed.update(2 * quiet + 1, false) == Change::Ended);
}

// Where an orthographic render's camera stands (mitcad#215): on the view's
// axis behind the scene's bounds, or where the view's eye is when that is
// behind them already.
void renderCameraTests() {
  mitcad::render::ViewData view;
  view.perspective = false;
  view.target = {0, 0, 10};
  view.up = {0, 0, 1};
  const std::array<double, 3> low{-30, -20, 0};
  const std::array<double, 3> high{30, 20, 20};
  const double radius = std::sqrt(60.0 * 60.0 + 40.0 * 40.0 + 20.0 * 20.0) / 2.0;
  const auto sameEye = [](const std::array<double, 3>& a, const std::array<double, 3>& b) {
    return std::abs(a[0] - b[0]) < 1e-9 && std::abs(a[1] - b[1]) < 1e-9 && std::abs(a[2] - b[2]) < 1e-9;
  };
  // The eye inside the block, looking along +Y: it steps back along the axis
  // past the bounds' sphere and its margin.
  view.eye = {0, -5, 10};
  const std::array<double, 3> placed = mitcad::render::orthographicEye(view, low, high);
  CHECK(std::abs(placed[0]) < 1e-9 && std::abs(placed[2] - 10.0) < 1e-9);
  CHECK(placed[1] < -radius && std::abs(placed[1] + radius + 1.0) < 1e-9);
  // The whole block is in front of the camera.
  CHECK(low[1] - placed[1] > 0.0);
  // An eye far behind the scene stays; one in front of it (the bounds
  // behind it) steps back the same way from the other side.
  view.eye = {0, -500, 10};
  CHECK(sameEye(mitcad::render::orthographicEye(view, low, high), view.eye));
  view.eye = {0, 0, 1000};
  view.target = {0, 0, 900}; // looking down at the block from above, the block behind the target
  const std::array<double, 3> above = mitcad::render::orthographicEye(view, low, high);
  CHECK(sameEye(above, view.eye)); // already behind (above) everything
  view.eye = {10, 0, 5};
  view.target = {10, 0, -5}; // looking down from inside the block
  const std::array<double, 3> down = mitcad::render::orthographicEye(view, low, high);
  CHECK(std::abs(down[0] - 10.0) < 1e-9 && std::abs(down[1]) < 1e-9);
  CHECK(down[2] > high[2] && std::abs(down[2] - (-5.0 + 15.0 + radius + 1.0)) < 1e-9);
  // A large scene takes a margin of a hundredth of its size; no direction
  // (eye and target the same) leaves the eye alone.
  const std::array<double, 3> wide{3000, 3000, 20};
  view.eye = {0, -5, 10};
  view.target = {0, 0, 10};
  const double big = std::sqrt(6000.0 * 6000.0 + 6000.0 * 6000.0 + 20.0 * 20.0) / 2.0;
  CHECK(std::abs(mitcad::render::orthographicEye(view, {-3000, -3000, 0}, wide)[1] + big * 1.01) < 1e-6);
  view.target = view.eye;
  CHECK(sameEye(mitcad::render::orthographicEye(view, low, high), view.eye));
}

} // namespace

int autosaveTests();
int commandMessageTests();
int moveToComponentTests();
int autoselectTests();
int cutTargetsTests();
int autoOperationPolicyTests();
int eventFilterGuardTests();
int moveOccurrenceTests();
int selectionScopeTests();
int iconColourTests();
int previewBodiesTests();
int debugLogTests();
int waitCursorTests();
int sketchSnapTests();
int textureMapsTests();
int textureSourceTests();
int polyHavenSourceTests();
int renderMeshTests();
int agentFolderTests();
int assistantSetupTests();
int partPickerDataTests();
int webPageTests();
int projectWebTests();
int drawingSettingsTests();
int contextMenuTests();
int dialogFolderTests();
int sketchAxesTests();
int iamImportTests();
int commandAliasTests();
int mountFramesTests();
int timberOpeningsTests();
int jointHighlightTests();
int moveFaceTextTests();
int projectRowTests();
int projectBrowserTests();
int firstBodiesFitTests();
int sectionWorkerTests();
int chamferHandleTests();
int patternOptionsTests();
int commandSketchesTests();
int parameterEditsTests();
int decimalAlignTests();
int parameterSuggestTests();
int librarySettingsTests();
int selectionCreateTests();
int updateInstallerTests();
int printAnalysisTests();
int fitToScreenTests();
int measureHistoryTests();
int printImportSummaryTests();
int placeGeometryTests();
int printSettingsFormTests();
int measurementDrawingTests();
int treeItemDelegateTests();
int macToolButtonTests();
int navigationPreferencesTests();
int formulaEditorTests();
int drawingToolsTests();
int saveAsMeshTests();
int drawingSheetViewTests();
int imageReferencesTests();
int configurationsDataTests();
int ruleBlocksTests();
int partOptionsTests();
int documentSnapshotTests();
int heifImageTests();
int profilePicksTests();
int spaceMouseTests();
int mcpEndpointLimitsTests();
int chatHistoryTests();
int cameraHeightTests();
int sketchNormalTests();
int activateQuestionTests();
int featureStatusTests();

int main(int argc, char* argv[]) {
  QApplication app(argc, argv);
  const bool renderDevicesOnly = app.arguments().contains(QStringLiteral("--render-devices"));
  if (renderDevicesOnly) {
    renderDeviceChoiceTests();
    renderFallbackTests();
    renderDirectFallbackTests();
  } else {
    atomicCopyTests();
    projectRowDragTests();
    renderWorkerFailureTests();
    workerThreadStartTests();
    shortcutBindingTests();
    shortcutContextTests();
    numberTests();
    cursorTests();
    printTests();
    desktopEntryTests();
    renderLookTests();
    renderOutputTests();
    renderCatcherTests();
    renderDeviceChoiceTests();
    renderFallbackTests();
    renderDirectFallbackTests();
    reportMaskTests();
    reportCrashTests();
    reportLinkTests();
    cloudAddressTests();
    timelineSearchTests();
    autoConstraintTests();
    selectionEntryTests();
    viewNavigationTests();
    renderCameraTests();
    viewMatchTests();
    failures += autosaveTests();
    failures += commandMessageTests();
    failures += moveToComponentTests();
    failures += autoselectTests();
    failures += cutTargetsTests();
    failures += autoOperationPolicyTests();
    failures += eventFilterGuardTests();
    failures += moveOccurrenceTests();
    failures += selectionScopeTests();
    failures += iconColourTests();
    failures += previewBodiesTests();
    failures += debugLogTests();
    failures += waitCursorTests();
    failures += sketchSnapTests();
    failures += textureMapsTests();
    failures += textureSourceTests();
    failures += polyHavenSourceTests();
    failures += renderMeshTests();
    failures += agentFolderTests();
    failures += assistantSetupTests();
    failures += partPickerDataTests();
    failures += webPageTests();
    failures += projectWebTests();
    failures += drawingSettingsTests();
    failures += contextMenuTests();
    failures += dialogFolderTests();
    failures += sketchAxesTests();
    failures += iamImportTests();
    failures += commandAliasTests();
    failures += mountFramesTests();
    failures += timberOpeningsTests();
    failures += jointHighlightTests();
    failures += moveFaceTextTests();
    failures += projectRowTests();
    failures += projectBrowserTests();
    failures += firstBodiesFitTests();
    failures += sectionWorkerTests();
    failures += chamferHandleTests();
    failures += patternOptionsTests();
    failures += commandSketchesTests();
    failures += parameterEditsTests();
    failures += decimalAlignTests();
    failures += parameterSuggestTests();
    failures += librarySettingsTests();
    failures += selectionCreateTests();
    failures += updateInstallerTests();
    failures += printAnalysisTests();
    failures += fitToScreenTests();
    failures += measureHistoryTests();
    failures += printImportSummaryTests();
    failures += placeGeometryTests();
    failures += printSettingsFormTests();
    failures += measurementDrawingTests();
    failures += treeItemDelegateTests();
    failures += macToolButtonTests();
    failures += navigationPreferencesTests();
    failures += formulaEditorTests();
    failures += drawingToolsTests();
    failures += saveAsMeshTests();
    failures += drawingSheetViewTests();
    failures += imageReferencesTests();
    failures += configurationsDataTests();
    failures += ruleBlocksTests();
    failures += partOptionsTests();
    failures += documentSnapshotTests();
    failures += heifImageTests();
    failures += profilePicksTests();
    failures += spaceMouseTests();
    failures += mcpEndpointLimitsTests();
    failures += chatHistoryTests();
    failures += cameraHeightTests();
    failures += sketchNormalTests();
    failures += activateQuestionTests();
    failures += featureStatusTests();
    mitcad::tests::navigationSettingsTests(check);
  }
  if (failures != 0) {
    std::fprintf(stderr, "%d check(s) failed\n", failures);
    return 1;
  }
  std::printf(renderDevicesOnly ? "render device unit tests passed\n" : "app unit tests passed\n");
  return 0;
}
