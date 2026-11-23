// SPDX-License-Identifier: MIT
// Preferences › Restore Defaults (mitcad#1887, GitHub mitcad/Mitcad#357;
// app/COMMANDS.md "Restoring the defaults"): the settings of the page shown
// or of every page back to their defaults, after a question that says what
// is reset and what is kept. Each setting goes back through the function
// that saves it, so that what follows a change in Preferences (the view,
// the open windows' notifiers, the theme, the debug log) follows this one
// too; a setting whose default depends on the environment (the UI tests'
// variables) is removed instead. What no page shows stays as it is:
// recent files, library sources and fetched libraries, favourites, window
// geometry, keyboard shortcuts (Keyboard Shortcuts resets them), API keys
// in the keychain, chat histories and what a chat allowed always.

#include "ViewController.hpp"

#include <QDebug>
#include <QMessageBox>
#include <QPushButton>
#include <QSettings>
#include <QTemporaryDir>
#include <QtLogging>

#include "../DocumentFrame.hpp"
#include "../agent/ChatSettings.hpp"
#include "../browser/ActivateQuestion.hpp"
#include "../browser/BrowserBackground.hpp"
#include "../browser/BrowserPreferences.hpp"
#include "../framework/ExtrudeStartPreference.hpp"
#include "../browser/JointHolderQuestion.hpp"
#include "../drawings/DrawingPreferences.hpp"
#include "../files/Print3d.hpp"
#include "../framework/AppSettings.hpp"
#include "../framework/AppTheme.hpp"
#include "../framework/ChromeStyle.hpp"
#include "../framework/DebugLog.hpp"
#include "../framework/Dialogs.hpp"
#include "../framework/InterfaceSize.hpp"
#include "../framework/Language.hpp"
#include "../framework/MenuBarReveal.hpp"
#include "../framework/BoostButton.hpp"
#include "../framework/DiscordButton.hpp"
#include "../framework/QuickAccess.hpp"
#include "../framework/ResultCache.hpp"
#include "../framework/UiSizes.hpp"
#include "../library/SupplierShops.hpp"
#include "../materials/TextureSource.hpp"
#include "../mcp/AgentFolders.hpp"
#include "../print/MaterialProperties.hpp"
#include "../render/RenderDevice.hpp"
#include "../render/RenderMode.hpp"
#include "../render/RenderWorkerChoice.hpp"
#include "../sketch/FontFiles.hpp"
#include "../sketch/SketchPreferences.hpp"
#include "../update/UpdateSettings.hpp"
#include "Comments.hpp"
#include "OriginPreference.hpp"
#include "SelectionLook.hpp"
#include "SmallCirclesPreference.hpp"
#include "VersionWatermark.hpp"

namespace mitcad {

ViewSettings ViewSettings::defaults() {
  // load() from a store with nothing in it: the platform's defaults
  // (Automatic background and the trackpad scheme on macOS) as a fresh
  // installation has them.
  const QTemporaryDir folder;
  if (!folder.isValid()) {
    return ViewSettings();
  }
  const QSettings empty(folder.filePath(QStringLiteral("defaults.ini")), QSettings::IniFormat);
  return load(empty);
}

bool ViewController::confirmRestoreDefaults(QWidget* parent, const QString& pageName) {
  const bool all = pageName.isEmpty();
  const QString question = all ? tr("Restore the defaults of all settings?")
                               : tr("Restore the defaults of the %1 settings?").arg(pageName);
  QString detail = all ? tr("Every setting in Preferences goes back to its default.")
                       : tr("The settings on the %1 page go back to their defaults. The other pages keep "
                            "their settings, and changes made on them are saved.")
                             .arg(pageName);
  detail += QStringLiteral("\n\n");
  detail += tr("Kept: recent files, library sources and fetched libraries, favorites, window sizes and "
               "places, API keys, chat histories, and keyboard shortcuts, which Keyboard Shortcuts "
               "resets on its own.");
  QMessageBox box(QMessageBox::Question, tr("Restore Defaults"), question, QMessageBox::NoButton, parent);
  box.setObjectName(QStringLiteral("restoreDefaultsQuestion"));
  box.setInformativeText(detail);
  QPushButton* restore = box.addButton(tr("&Restore Defaults"), QMessageBox::AcceptRole);
  QPushButton* cancel = box.addButton(QMessageBox::Cancel);
  // Enter alone does not reset anything.
  box.setDefaultButton(cancel);
  box.setEscapeButton(cancel);
  prepareModal(&box);
  qDebug().noquote() << QStringLiteral("Preferences restore defaults asked: %1 %2")
                            .arg(question, QString(detail).replace(QLatin1Char('\n'), QLatin1Char(' ')));
  execDialog(&box);
  const bool yes = box.clickedButton() == restore;
  if (!yes) {
    qDebug().noquote() << QStringLiteral("Preferences restore defaults canceled");
  }
  return yes;
}

void ViewController::restorePreferenceDefaults(const QStringList& pages, bool all) {
  const auto resets = [&pages](const char* id) { return pages.contains(QLatin1String(id)); };
  const ViewSettings fresh = ViewSettings::defaults();

  if (resets("general")) {
    setLanguagePreference(QString()); // Automatic
    const GeneralSettings before = GeneralSettings::load();
    const GeneralSettings general{};
    general.save();
    if (general.temperatureUnit != before.temperatureUnit) {
      applyTemperaturePreference();
    }
    if (generalChanged) {
      generalChanged();
    }
    if (newDesignUnitsChanged && general.newDesignUnits != before.newDesignUnits) {
      newDesignUnitsChanged();
    }
    resetAskToActivateComponent();
    setVersionWatermarkShown(true);
    setBrowserConsumeSketches(false);
    saveExtrudeStart(NewFeatureOperation::Automatic);
#ifdef MITCAD_MACOS_NATIVE_UI
    setNativeUiPreference(false);
#endif
  }

  if (resets("navigation")) {
    m_settings.navigation = fresh.navigation;
  }
  const bool navigationBarWas = m_settings.navigationBar;
  if (resets("display")) {
    if (m_settings.anaglyph && m_settings.perspective && !fresh.perspective) {
      // Orthographic ends Anaglyph 3D, as View › Camera does (mitcad#1449).
      m_settings.anaglyph = false;
    }
    m_settings.perspective = fresh.perspective;
    m_settings.cubeProjection = fresh.cubeProjection;
    m_settings.cubeSize = fresh.cubeSize; // mitcad#2071
    m_settings.focalLength = fresh.focalLength;
    m_settings.anaglyphFocalLength = fresh.anaglyphFocalLength;
    m_settings.stereoDepth = fresh.stereoDepth;
    m_settings.stereoConvergence = fresh.stereoConvergence;
    m_settings.background = fresh.background;
    m_settings.top = fresh.top;
    m_settings.bottom = fresh.bottom;
    m_settings.ghostInactive = fresh.ghostInactive;
    m_settings.activePickPriority = fresh.activePickPriority;
    m_settings.navigationBar = fresh.navigationBar;
    m_settings.sketchesThroughBodies = fresh.sketchesThroughBodies; // mitcad#1931
    m_settings.selectionThroughBodies = fresh.selectionThroughBodies; // mitcad#1956
  }
  if (resets("navigation") || resets("display")) {
    m_settings.save();
    apply(); // the navigation, camera, lenses, cube, background, ghosts and picks
    if (m_render != nullptr) {
      m_render->redisplay(); // a rendered frame over the new background
    }
    emit displaySettingsChanged();
    if (m_settings.navigationBar != navigationBarWas && navigationBarChanged) {
      navigationBarChanged(m_settings.navigationBar);
    }
  }

  if (resets("display")) {
    saveSelectionLook(SelectionLook());
    if (render::renderDeviceChoice() != QLatin1String("auto")) {
      render::setRenderDeviceChoice(QStringLiteral("auto"));
      if (m_render != nullptr) {
        m_render->deviceChanged();
      }
    }
    if (!render::pickedWorker().isEmpty()) {
      render::setPickedWorker(QString()); // the shipped worker
      if (m_render != nullptr) {
        m_render->workerChanged();
      }
    }
    if (themePreference() != ThemeChoice::System) {
      setThemePreference(ThemeChoice::System);
      applyThemePreference(ThemeChoice::System);
    }
    // At the next start, as when they are chosen.
    saveInterfaceSize(kDefaultInterfaceSize);
    saveTextSize(kDefaultUiSize);
    saveIconSize(kDefaultUiSize);
    saveFontFamily(QString());
    clearLayoutPreference();
    QuickAccessSettings::saveVisible(true);
    BoostSettings::saveVisible(true);
    DiscordSettings::saveVisible(true);
    if (MenuBarSettings::offered()) {
      MenuBarSettings::saveShown(true);
    }
#if defined(Q_OS_MACOS) && !defined(MITCAD_MAC_UI)
    DocumentFrame::setWindowMenuBarShown(false);
#endif
    // No choice saved: the default (Rows, mitcad#2144) applies, and a later
    // default too.
    clearBrowserBackground();
    saveBrowserRowsShare(kDefaultBrowserRowsShare);
    // The Browser rows' controls where they always were (mitcad#2223).
    setBrowserActivatePlace(BrowserControlPlace::Classic);
    setBrowserEyePlace(BrowserControlPlace::Classic);
    drawings::saveCornerDots(drawings::CornerDots());
    saveSmallCircles(SmallCircles::Adaptive);
    saveOriginDisplay(OriginDisplay());
  }

  if (resets("cache")) {
    CacheSettings().save();
  }

  if (resets("cloud")) {
    // Known brokers keep their answers and sign-ins (their own Sign Out and
    // Forget).
    RemoteSettings().save();
    VersionSettings().save();
  }

  if (resets("updates")) {
    UpdateSettings updates = UpdateSettings::load();
    const UpdateSettings defaults{};
    updates.automatic = defaults.automatic;
    updates.channel = defaults.channel;
    updates.save();
  }

  if (resets("print")) {
    // The slicer only: Send to Slicer's own choices stay its.
    PrintSettings print = PrintSettings::load();
    print.slicer = Slicer();
    print.folderOnly = false;
    print.save();
  }

  if (resets("diagnostics") && DebugLog::settingOn()) {
    // Off, as the switch turned off does; a log this run was started with
    // (--debug-log) stays the run's.
    DebugLog::setSettingOn(false);
    DebugLog::instance().stop();
  }

  if (resets("textures")) {
    // The browser's resolution is Edit Appearances' own.
    TextureSettings textures = TextureSettings::load();
    const TextureSettings defaults{};
    textures.folders = defaults.folders;
    textures.allowNetwork = defaults.allowNetwork;
    textures.save();
  }

  if (resets("agents")) {
    mcp::saveAgentFolders({});
    const agent::ChatSettings before = agent::ChatSettings::load();
    const agent::ChatSettings chat{};
    chat.save();
    agent::resetAgentConnections();
    if (chat.agent != before.agent) {
      emit agent::ChatNotifier::instance().agentChosen(chat.agent);
    }
    emit agent::ChatNotifier::instance().apiSettingsChanged();
    saveAgentsMayResolveComments(false);
  }

  if (resets("fonts")) {
    sketch::saveFontFolders({});
  }

  if (resets("sketch")) {
    sketch::resetScaleToFirstDimension();
    sketch::saveClickAddsToSelection(false);
    sketch::saveEscLeavesUnchangedSketch(false);
    sketch::saveShowProfilesOnOpen(true);
    sketch::resetProjectFaceOnCreate();
  }

  if (resets("suppliers")) {
    ShopSettings::reset();
  }

  if (resets("assembly")) {
    saveJointHolderPreference(JointHolderChoice::Ask);
  }

  qDebug().noquote() << QStringLiteral("Preferences: restore defaults, %1: %2")
                            .arg(all ? QStringLiteral("all settings") : QStringLiteral("current group"),
                                 pages.join(QStringLiteral(", ")));
}

} // namespace mitcad
