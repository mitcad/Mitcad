// SPDX-License-Identifier: MIT
// Preferences › Export Settings and Import Settings (mitcad#1889, GitHub
// mitcad/Mitcad#357; app/COMMANDS.md "Exporting and importing settings"):
// the groups of settings chosen saved to a `.mitcad-settings` file
// (framework/SettingsTransfer.hpp), and such a file read back after a
// question that says what it holds and which groups it replaces. The
// imported settings then apply as a change in Preferences applies: each
// setting whose change the open windows follow goes through the function
// that saves it, the view takes the view's settings again, the keyboard
// shortcuts and the pins are read again; what takes effect at the next
// start (language, interface, text and icon size, font, layout) does so
// then.

#include "ViewController.hpp"

#include <functional>
#include <optional>
#include <utility>

#include <QCheckBox>
#include <QCoreApplication>
#include <QDateTime>
#include <QDebug>
#include <QDialog>
#include <QDialogButtonBox>
#include <QDir>
#include <QFileInfo>
#include <QLabel>
#include <QLocale>
#include <QMap>
#include <QPushButton>
#include <QSettings>
#include <QStandardPaths>
#include <QVBoxLayout>
#include <QVariant>
#include <QtLogging>

#include "../agent/ChatSettings.hpp"
#include "../browser/BrowserBackground.hpp"
#include "../browser/BrowserPreferences.hpp"
#include "../browser/DocumentHost.hpp"
#include "../drawings/DrawingPreferences.hpp"
#include "../framework/AppSettings.hpp"
#include "../framework/AppTheme.hpp"
#include "../framework/CommandRegistry.hpp"
#include "../framework/DebugLog.hpp"
#include "../framework/Dialogs.hpp"
#include "../framework/MenuBarReveal.hpp"
#include "../framework/NoticeLevel.hpp"
#include "../framework/QuickAccess.hpp"
#include "../framework/SettingsTransfer.hpp"
#include "../print/MaterialProperties.hpp"
#include "../render/RenderDevice.hpp"
#include "../render/RenderMode.hpp"
#include "../render/RenderWorkerChoice.hpp"
#include "../sketch/FontFiles.hpp"
#include "OriginPreference.hpp"
#include "SelectionLook.hpp"
#include "SmallCirclesPreference.hpp"
#include "VersionWatermark.hpp"

namespace mitcad {
namespace {

// A group as a dialog offers it.
struct GroupRow {
  QString id;
  QString label;
  bool checked = false;
};

QString groupName(const QString& id) {
  const SettingsGroup* group = settingsGroup(id);
  return group != nullptr ? QCoreApplication::translate("Settings", group->name) : id;
}

// The groups the user checks in a dialog of `rows`; nullopt when
// cancelled. `acceptDefault`: Return takes the accept button (else Cancel).
std::optional<QStringList> chooseGroups(QWidget* parent, const QString& title, const QString& text,
                                        const QString& detail, const QList<GroupRow>& rows,
                                        const QString& acceptText, bool acceptDefault, const QString& objectName) {
  QDialog dialog(parent);
  dialog.setObjectName(objectName);
  dialog.setWindowTitle(title);
  auto* column = new QVBoxLayout(&dialog);
  auto* heading = new QLabel(text);
  heading->setWordWrap(true);
  column->addWidget(heading);
  QList<QCheckBox*> checks;
  for (const GroupRow& row : rows) {
    auto* check = new QCheckBox(row.label);
    check->setObjectName(QStringLiteral("settingsGroup_") + row.id);
    check->setChecked(row.checked);
    check->setProperty("groupId", row.id);
    column->addWidget(check);
    checks << check;
  }
  if (!detail.isEmpty()) {
    auto* note = new QLabel(detail);
    note->setObjectName(QStringLiteral("settingsTransferDetail"));
    note->setWordWrap(true);
    note->setTextInteractionFlags(Qt::TextSelectableByMouse);
    column->addWidget(note);
  }
  auto* buttons = new QDialogButtonBox;
  QPushButton* accept = buttons->addButton(acceptText, QDialogButtonBox::AcceptRole);
  accept->setObjectName(QStringLiteral("settingsTransferAccept"));
  QPushButton* cancel = buttons->addButton(QDialogButtonBox::Cancel);
  (acceptDefault ? accept : cancel)->setDefault(true);
  QObject::connect(buttons, &QDialogButtonBox::accepted, &dialog, &QDialog::accept);
  QObject::connect(buttons, &QDialogButtonBox::rejected, &dialog, &QDialog::reject);
  // Nothing to do without a group.
  const auto update = [checks, accept] {
    bool any = false;
    for (const QCheckBox* check : checks) {
      any = any || check->isChecked();
    }
    accept->setEnabled(any);
  };
  for (QCheckBox* check : std::as_const(checks)) {
    QObject::connect(check, &QCheckBox::toggled, &dialog, update);
  }
  update();
  column->addWidget(buttons);
  prepareModal(&dialog);
  if (execDialog(&dialog) != QDialog::Accepted) {
    return std::nullopt;
  }
  QStringList chosen;
  for (const QCheckBox* check : std::as_const(checks)) {
    if (check->isChecked()) {
      chosen << check->property("groupId").toString();
    }
  }
  return chosen;
}

QString startFolder() {
  const QString documents = QStandardPaths::writableLocation(QStandardPaths::DocumentsLocation);
  return documents.isEmpty() ? QDir::homePath() : documents;
}

QString fileFilter() {
  return QCoreApplication::translate("Settings", "MitCad settings (*.%1)").arg(QLatin1String(kSettingsFileSuffix));
}

// The raw values of `keys` as they are now (an invalid value: no key).
QMap<QString, QVariant> snapshot(const QStringList& keys) {
  const QSettings settings;
  QMap<QString, QVariant> values;
  for (const QString& key : keys) {
    values.insert(key, settings.contains(key) ? settings.value(key) : QVariant());
  }
  return values;
}

// The keys whose values differ from `before` now.
bool changed(const QMap<QString, QVariant>& before, const QStringList& keys) {
  const QSettings settings;
  for (const QString& key : keys) {
    const QVariant now = settings.contains(key) ? settings.value(key) : QVariant();
    const QVariant was = before.value(key);
    if (now.isValid() != was.isValid() || (now.isValid() && now.toString() != was.toString()) ||
        (now.isValid() && now.toStringList() != was.toStringList())) {
      return true;
    }
  }
  return false;
}

// A setting the open windows follow through the function that saves it
// (which compares with what is saved and tells the windows): its new value
// is read from the imported settings, its keys put back as they were, and
// the value saved again through that function.
template <typename T>
void through(const QMap<QString, QVariant>& before, const QStringList& keys, const std::function<T()>& load,
             const std::function<void(const T&)>& save) {
  if (!changed(before, keys)) {
    return;
  }
  const T next = load();
  {
    QSettings settings;
    for (const QString& key : keys) {
      const QVariant was = before.value(key);
      if (was.isValid()) {
        settings.setValue(key, was);
      } else {
        settings.remove(key);
      }
    }
  }
  save(next);
}

} // namespace

struct ViewController::SettingsBeforeImport {
  QMap<QString, QVariant> raw; // the keys the import writes or removes
  GeneralSettings general;
  ThemeChoice theme = ThemeChoice::System;
  QString device;
  QString worker;
  bool debugLog = false;
  QString chatAgent;
  bool navigationBar = true;
};

void ViewController::exportSettings(QWidget* parent) {
  QList<GroupRow> rows;
  for (const SettingsGroup& group : settingsGroups()) {
    rows.append({QLatin1String(group.id), QCoreApplication::translate("Settings", group.name), group.defaultOn});
  }
  const std::optional<QStringList> groups =
      chooseGroups(parent, tr("Export Settings"), tr("Choose the settings to save to a file."),
                   tr("Recent files and this computer's folders and programs are left out unless checked: "
                      "another computer may not have them."),
                   rows, tr("Export…"), true, QStringLiteral("exportSettingsDialog"));
  if (!groups) {
    qDebug().noquote() << QStringLiteral("Preferences: settings export cancelled");
    return;
  }
  const QString suffix = QLatin1Char('.') + QLatin1String(kSettingsFileSuffix);
  QString path = dialogSaveFileName(parent, tr("Save Settings File"),
                                    QDir(startFolder()).filePath(QStringLiteral("MitCad") + suffix), fileFilter());
  if (path.isEmpty()) {
    qDebug().noquote() << QStringLiteral("Preferences: settings export cancelled");
    return;
  }
  if (!path.endsWith(suffix, Qt::CaseInsensitive)) {
    path += suffix;
  }
  QString error;
  int count = 0;
  if (!writeSettingsFile(QSettings(), *groups, path, &error, &count)) {
    qWarning().noquote() << QStringLiteral("Preferences: settings not exported to %1: %2")
                                .arg(QDir::toNativeSeparators(path), error);
    sheetWarning(parent, tr("Export Settings"), tr("The settings were not saved.") + QStringLiteral("\n\n") + error);
    return;
  }
  qDebug().noquote() << QStringLiteral("Preferences: settings exported to %1: %2 (%3 settings)")
                            .arg(QDir::toNativeSeparators(path), groups->join(QStringLiteral(", ")))
                            .arg(count);
  m_host.showStatus(tr("Settings saved to %1.").arg(QFileInfo(path).fileName()), NoticeLevel::Success);
}

void ViewController::importSettings(QWidget* parent) {
  const QString path =
      dialogOpenFileName(parent, tr("Choose a Settings File"), startFolder(),
                         fileFilter() + QStringLiteral(";;") + tr("All files (*)"));
  if (path.isEmpty()) {
    qDebug().noquote() << QStringLiteral("Preferences: settings import cancelled");
    return;
  }
  const QString shownPath = QDir::toNativeSeparators(path);
  const SettingsFileContents contents = readSettingsFile(path);
  QString problem = settingsFileProblemText(contents);
  if (problem.isEmpty() && contents.groups.isEmpty()) {
    problem = tr("The file holds no settings this MitCad can take.");
  }
  if (!problem.isEmpty()) {
    qWarning().noquote() << QStringLiteral("Preferences: settings not imported from %1: %2").arg(shownPath, problem);
    sheetWarning(parent, tr("Import Settings"),
                 tr("The settings were not imported.") + QStringLiteral("\n\n") + problem);
    return;
  }

  // What the file holds, and what would not be written here.
  const SettingsImportPlan offered = planSettingsImport(contents, contents.groups, QSettings());
  QList<GroupRow> rows;
  QStringList held;
  for (const QString& id : contents.groups) {
    const int count = contents.count(id);
    rows.append({id, QStringLiteral("%1 (%2)").arg(groupName(id)).arg(count), true});
    held << QStringLiteral("%1 (%2)").arg(id).arg(count);
  }
  const QDateTime written = QDateTime::fromString(contents.written, Qt::ISODate);
  QString text = tr("Import the settings in %1?").arg(QFileInfo(path).fileName());
  if (!contents.application.isEmpty()) {
    text += QStringLiteral(" ");
    text += written.isValid()
                ? tr("Written by MitCad %1 on %2, %3.")
                      .arg(contents.application, settingsPlatformName(contents.platform),
                           QLocale().toString(written.toLocalTime(), QLocale::ShortFormat))
                : tr("Written by MitCad %1 on %2.").arg(contents.application, settingsPlatformName(contents.platform));
  }
  QString detail = tr("The groups checked replace this computer's settings of the same groups: those not in the "
                      "file go back to their defaults. The others stay as they are.");
  QStringList skippedLog;
  if (!offered.skipped.isEmpty()) {
    detail += QStringLiteral("\n\n");
    detail += tr("Not on this computer, so skipped and not written:");
    for (const SkippedSettingsPath& skipped : offered.skipped) {
      detail += QStringLiteral("\n• %1: %2").arg(groupName(skipped.group), QDir::toNativeSeparators(skipped.path));
      skippedLog << QDir::toNativeSeparators(skipped.path);
    }
  }
  qDebug().noquote() << QStringLiteral("Preferences import settings asked: %1: written by MitCad %2 on %3; groups %4; "
                                       "skipped %5")
                            .arg(shownPath, contents.application, contents.platform,
                                 held.join(QStringLiteral(", ")),
                                 skippedLog.isEmpty() ? QStringLiteral("none") : skippedLog.join(QStringLiteral(", ")));
  const std::optional<QStringList> groups = chooseGroups(parent, tr("Import Settings"), text, detail, rows,
                                                         tr("Import"), false, QStringLiteral("importSettingsDialog"));
  if (!groups) {
    qDebug().noquote() << QStringLiteral("Preferences: settings import cancelled");
    return;
  }

  // Applied: as they are now, then the file's, then what follows them.
  const SettingsImportPlan plan = planSettingsImport(contents, *groups, QSettings());
  QStringList touched = plan.remove;
  for (auto it = plan.write.cbegin(); it != plan.write.cend(); ++it) {
    touched << it.key();
  }
  SettingsBeforeImport was;
  was.raw = snapshot(touched);
  was.general = GeneralSettings::load();
  was.theme = themePreference();
  was.device = render::renderDeviceChoice();
  was.worker = render::pickedWorker();
  was.debugLog = DebugLog::settingOn();
  was.chatAgent = agent::ChatSettings::load().agent;
  was.navigationBar = m_settings.navigationBar;
  {
    QSettings settings;
    applySettingsImport(settings, plan);
  }
  applyImportedSettings(was, plan.groups);
  const QMap<QString, QVariant>& before = was.raw;

  QStringList skipped;
  for (const SkippedSettingsPath& entry : plan.skipped) {
    skipped << QDir::toNativeSeparators(entry.path);
  }
  qDebug().noquote() << QStringLiteral("Preferences: settings imported from %1: %2; %3 written, %4 back to their "
                                       "defaults, skipped %5")
                            .arg(shownPath, plan.groups.join(QStringLiteral(", ")))
                            .arg(plan.write.size())
                            .arg(plan.remove.size())
                            .arg(skipped.isEmpty() ? QStringLiteral("none") : skipped.join(QStringLiteral(", ")));
  // What takes effect at the next start, as when it is chosen.
  const QStringList atStart = {QStringLiteral("interface/language"),  QStringLiteral("appearance/interfaceSize"),
                               QStringLiteral("appearance/textSize"),  QStringLiteral("appearance/iconSize"),
                               QStringLiteral("appearance/fontFamily"), QStringLiteral("appearance/layout")};
  QStringList restart;
  for (const QString& key : atStart) {
    if (before.contains(key) && changed(before, {key})) {
      restart << key;
    }
  }
  m_host.showStatus(restart.isEmpty()
                        ? tr("Settings imported from %1.").arg(QFileInfo(path).fileName())
                        : tr("Settings imported from %1. Some take effect when MitCad starts again.")
                              .arg(QFileInfo(path).fileName()),
                    NoticeLevel::Success);
}

void ViewController::applyImportedSettings(const SettingsBeforeImport& was, const QStringList& groups) {
  const auto has = [&groups](const char* id) { return groups.contains(QLatin1String(id)); };
  const QMap<QString, QVariant>& before = was.raw;

  if (has("preferences")) {
    // Those the windows follow as Preferences saves them.
    through<bool>(before, {QStringLiteral("view/showVersion")}, [] { return versionWatermarkShown(); },
                  [](const bool& shown) { setVersionWatermarkShown(shown); });
    through<bool>(before, {QStringLiteral("browser/consumeSketches")}, [] { return browserConsumeSketches(); },
                  [](const bool& on) { setBrowserConsumeSketches(on); });
    through<SelectionLook>(before, {QStringLiteral("view/selectionColor"), QStringLiteral("view/selectedLineWidth")},
                           [] { return SelectionLook::load(QSettings()); },
                           [](const SelectionLook& look) { saveSelectionLook(look); });
    through<OriginDisplay>(
        before,
        {QStringLiteral("view/originStyle"), QStringLiteral("view/originSize"), QStringLiteral("view/originSizeMm")},
        [] { return OriginDisplay::load(QSettings()); },
        [](const OriginDisplay& display) { saveOriginDisplay(display); });
    through<drawings::CornerDots>(before, {QStringLiteral("drawing/alwaysShowCorners"), QStringLiteral("drawing/cornerDotSize")},
                        [] { return drawings::CornerDots::load(QSettings()); },
                        [](const drawings::CornerDots& dots) { drawings::saveCornerDots(dots); });
    through<SmallCircles>(before, {QStringLiteral("view/smallCircles")}, [] { return savedSmallCircles(); },
                          [](const SmallCircles& choice) { saveSmallCircles(choice); });
    through<bool>(before, {MenuBarSettings::settingsKey()}, [] { return MenuBarSettings::loadShown(); },
                  [](const bool& shown) { MenuBarSettings::saveShown(shown); });
    through<bool>(before, {QuickAccessSettings::visibleKey()}, [] { return QuickAccessSettings::loadVisible(); },
                  [](const bool& visible) { QuickAccessSettings::saveVisible(visible); });
    through<BrowserBackground>(before, {QStringLiteral("appearance/browserBackground")},
                               [] { return savedBrowserBackground(); },
                               [](const BrowserBackground& background) { saveBrowserBackground(background); });
    // The share of the view the Browser's rows take at most (mitcad#1932).
    through<int>(before, {QStringLiteral("appearance/browserRowsShare")}, [] { return savedBrowserRowsShare(); },
                 [](const int& percent) { saveBrowserRowsShare(percent); });
    const ThemeChoice themeNow = themePreference();
    if (themeNow != was.theme) {
      applyThemePreference(themeNow);
    }
    if (m_render != nullptr && render::renderDeviceChoice() != was.device) {
      m_render->deviceChanged();
    }
    // General: autosave, units, the temperature unit.
    const GeneralSettings generalNow = GeneralSettings::load();
    if (generalNow.temperatureUnit != was.general.temperatureUnit) {
      applyTemperaturePreference();
    }
    if (generalChanged) {
      generalChanged();
    }
    if (newDesignUnitsChanged && generalNow.newDesignUnits != was.general.newDesignUnits) {
      newDesignUnitsChanged();
    }
    // The debug log, at once (a log this run was started with stays).
    const bool debugLogNow = DebugLog::settingOn();
    if (debugLogNow != was.debugLog) {
      if (!debugLogNow) {
        DebugLog::instance().stop();
      } else if (!DebugLog::instance().running()) {
        QString error;
        if (!DebugLog::instance().start(QString(), QStringLiteral("Preferences"), &error)) {
          qWarning().noquote() << QStringLiteral("Debug log not started: %1").arg(error);
        }
      }
    }
    // AI Assistants: the connections start again with the settings.
    const QString agentNow = agent::ChatSettings::load().agent;
    agent::resetAgentConnections();
    if (agentNow != was.chatAgent) {
      emit agent::ChatNotifier::instance().agentChosen(agentNow);
    }
    emit agent::ChatNotifier::instance().apiSettingsChanged();
  }

  if (has("folders")) {
    if (m_render != nullptr && render::pickedWorker() != was.worker) {
      m_render->workerChanged();
    }
    RemoteSettings::load().apply(); // the git program
    sketch::forgetFolderFonts();
  }

  if (has("preferences") || has("navigation")) {
    // The view's settings: the navigation, camera, lenses, cube,
    // background, grid, ghosts and picks.
    m_settings = ViewSettings::load();
    apply();
    if (m_render != nullptr) {
      m_render->redisplay();
    }
    emit displaySettingsChanged();
    if (m_settings.navigationBar != was.navigationBar && navigationBarChanged) {
      navigationBarChanged(m_settings.navigationBar);
    }
  }

  if (has("shortcuts")) {
    m_registry.reloadShortcuts();
  }
  if (settingsImported) {
    settingsImported(groups);
  }
}

} // namespace mitcad
