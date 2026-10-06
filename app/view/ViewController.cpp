// SPDX-License-Identifier: MIT
#include "ViewController.hpp"

#include <algorithm>
#include <cmath>
#include <functional>
#include <initializer_list>
#include <iterator>
#include <memory>
#include <utility>

#include <QAction>
#include <QActionGroup>
#include <QCheckBox>
#include <QColorDialog>
#include <QComboBox>
#include <QCoreApplication>
#include <QDesktopServices>
#include <QDialog>
#include <QDialogButtonBox>
#include <QDir>
#include <QDoubleSpinBox>
#include <QFormLayout>
#include <QFrame>
#include <QFont>
#include <QGroupBox>
#include <QHBoxLayout>
#include <QHeaderView>
#include <QJsonArray>
#include <QLabel>
#include <QLineEdit>
#include <QListWidget>
#include <QMenu>
#include <QPushButton>
#include <QRadioButton>
#include <QSettings>
#include <QSpinBox>
#include <QStackedWidget>
#include <QTreeWidget>
#include <QUrl>
#include <QVBoxLayout>
#include <QtLogging>

#include "../browser/DocumentHost.hpp"
#include "../framework/Dialogs.hpp"
#include "../framework/AppSettings.hpp"
#include "../framework/CachePreferences.hpp"
#include "../framework/CommandRegistry.hpp"
#include "../framework/Icons.hpp"
#include "../framework/SettingsWindow.hpp"
#include "../framework/Theme.hpp"
#include "../platform/MacChrome.hpp"
#include "../framework/Numbers.hpp"
// 3D Print's slicer (mitcad#13).
#include "../files/Print3d.hpp"
#include "../update/UpdatePreferences.hpp"
// Remote repositories' git program (P12 remote).
#include "../files/RemotePreferences.hpp"

namespace mitcad {
namespace {

const QString kGroup = QStringLiteral("view/");

struct StyleEntry {
  VisualStyle style;
  const char* id;
  const char* name;
};

const StyleEntry kStyles[] = {
    {VisualStyle::Shaded, "view.style.shaded", QT_TRANSLATE_NOOP("View", "Shaded")},
    {VisualStyle::ShadedVisibleEdges, "view.style.visible_edges",
     QT_TRANSLATE_NOOP("View", "Shaded with Visible Edges Only")},
    {VisualStyle::ShadedHiddenEdges, "view.style.hidden_edges",
     QT_TRANSLATE_NOOP("View", "Shaded with Hidden Edges")},
    {VisualStyle::Wireframe, "view.style.wireframe", QT_TRANSLATE_NOOP("View", "Wireframe")},
    {VisualStyle::WireframeHiddenEdges, "view.style.wireframe_hidden",
     QT_TRANSLATE_NOOP("View", "Wireframe with Hidden Edges")},
};

struct StandardView {
  const char* key;
  const char* name;
  gp_Dir direction; // from the eye to the model
  gp_Dir up;
};

const StandardView kStandardViews[] = {
    {"front", QT_TRANSLATE_NOOP("View", "Front"), gp_Dir(0, 1, 0), gp_Dir(0, 0, 1)},
    {"back", QT_TRANSLATE_NOOP("View", "Back"), gp_Dir(0, -1, 0), gp_Dir(0, 0, 1)},
    {"left", QT_TRANSLATE_NOOP("View", "Left"), gp_Dir(1, 0, 0), gp_Dir(0, 0, 1)},
    {"right", QT_TRANSLATE_NOOP("View", "Right"), gp_Dir(-1, 0, 0), gp_Dir(0, 0, 1)},
    {"top", QT_TRANSLATE_NOOP("View", "Top"), gp_Dir(0, 0, -1), gp_Dir(0, 1, 0)},
    {"bottom", QT_TRANSLATE_NOOP("View", "Bottom"), gp_Dir(0, 0, 1), gp_Dir(0, -1, 0)},
    {"iso", QT_TRANSLATE_NOOP("View", "Isometric"), gp_Dir(-1, 1, -1), gp_Dir(0, 0, 1)},
};

struct BackgroundEntry {
  ViewSettings::Background background;
  const char* id;
  const char* name;
};

const BackgroundEntry kBackgrounds[] = {
    {ViewSettings::Background::Gradient, "view.environment.gradient",
     QT_TRANSLATE_NOOP("View", "Gradient Background")},
    {ViewSettings::Background::Light, "view.environment.light", QT_TRANSLATE_NOOP("View", "Light Background")},
    {ViewSettings::Background::White, "view.environment.white", QT_TRANSLATE_NOOP("View", "White Background")},
    {ViewSettings::Background::Dark, "view.environment.dark", QT_TRANSLATE_NOOP("View", "Dark Background")},
    {ViewSettings::Background::Custom, "view.environment.custom",
     QT_TRANSLATE_NOOP("View", "Custom Background")},
    {ViewSettings::Background::Automatic, "view.environment.automatic",
     QT_TRANSLATE_NOOP("View", "Automatic Background (follow system)")},
};

struct SchemeEntry {
  NavigationScheme scheme;
  const char* id; // in the settings and the log
  const char* name;
};

// In the order of NavigationScheme, which is also the order of the index
// that older settings kept under "view/navigation".
const SchemeEntry kSchemes[] = {
    {NavigationScheme::MiddlePan, "middle-pan", QT_TRANSLATE_NOOP("View", "Middle pans, Shift + middle orbits")},
    {NavigationScheme::AltButtons, "alt-buttons",
     QT_TRANSLATE_NOOP("View", "Alt + left orbits, Alt + middle pans, Alt + right zooms")},
    {NavigationScheme::MiddlePanF4, "middle-pan-f4",
     QT_TRANSLATE_NOOP("View", "Middle pans, Shift + middle or F4 + left orbits")},
    {NavigationScheme::MiddleOrbit, "middle-orbit",
     QT_TRANSLATE_NOOP("View", "Middle orbits, Ctrl + middle pans, Shift + middle zooms")},
    {NavigationScheme::RightOrbit, "right-orbit", QT_TRANSLATE_NOOP("View", "Right orbits, middle or Shift + right pans")},
    {NavigationScheme::Trackpad, "trackpad",
     QT_TRANSLATE_NOOP("View", "Trackpad (two-finger scroll pans, pinch zooms, Alt orbits)")},
};

const SchemeEntry& schemeEntry(NavigationScheme scheme) {
  for (const SchemeEntry& entry : kSchemes) {
    if (entry.scheme == scheme) {
      return entry;
    }
  }
  return kSchemes[0];
}

// The Alt and Ctrl keys as the platform names them (Option and Command on
// macOS, where Qt's Ctrl is the Command key).
QString altKey() {
#ifdef Q_OS_MACOS
  return QStringLiteral("\u2325");
#else
  return QStringLiteral("Alt");
#endif
}

QString ctrlKey() {
#ifdef Q_OS_MACOS
  return QStringLiteral("\u2318");
#else
  return QStringLiteral("Ctrl");
#endif
}

// A scheme's name, with the keys as the platform names them.
QString schemeName(NavigationScheme scheme) {
  QString name = QCoreApplication::translate("View", schemeEntry(scheme).name);
#ifdef Q_OS_MACOS
  name.replace(QStringLiteral("Alt"), altKey()).replace(QStringLiteral("Ctrl"), ctrlKey());
#endif
  return name;
}

// The pages of Preferences (mitcad#25) in their order, each also a command
// (`tools.preferences.<id>`, "Preferences: <name>") that the command search
// finds by the page's settings.
struct PreferencePage {
  const char* id;
  const char* name;
  const char* keywords; // separated by spaces
};

const PreferencePage kPreferencePages[] = {
    {"general", QT_TRANSLATE_NOOP("View", "General"), "autosave recovery author email name"},
    {"navigation", QT_TRANSLATE_NOOP("View", "Navigation"), "mouse orbit pan zoom scheme"},
    {"display", QT_TRANSLATE_NOOP("View", "Display"), "camera perspective orthographic background environment"},
    {"cache", QT_TRANSLATE_NOOP("View", "Cache"), "results memory disk diagnostics"},
    {"version_control", QT_TRANSLATE_NOOP("View", "Version Control"), "git remote sync push fetch"},
    {"updates", QT_TRANSLATE_NOOP("View", "Updates"), "release channel pre-release"},
    {"print", QT_TRANSLATE_NOOP("View", "3D Print"), "slicer printing"},
};

QString onOff(bool on) { return on ? QStringLiteral("on") : QStringLiteral("off"); }

QJsonArray xyz(const gp_XYZ& v) { return QJsonArray{v.X(), v.Y(), v.Z()}; }

gp_XYZ xyzOf(const QJsonValue& value) {
  const QJsonArray a = value.toArray();
  return gp_XYZ(a.at(0).toDouble(), a.at(1).toDouble(), a.at(2).toDouble());
}

// The mouse of a navigation scheme, for the overview.
QVector<QPair<QString, QString>> mouseRows(NavigationScheme scheme) {
  const QString wheel = QObject::tr("Mouse wheel");
  switch (scheme) {
  case NavigationScheme::Trackpad: {
    const QString alt = altKey();
    const QString shift = QStringLiteral("Shift");
    return {{QObject::tr("Two-finger scroll"), QObject::tr("Pan")},
            {QObject::tr("%1 + two-finger scroll").arg(alt), QObject::tr("Orbit")},
            {QObject::tr("%1 or %2 + two-finger scroll").arg(shift, ctrlKey()), QObject::tr("Zoom")},
            {QObject::tr("%1 + left drag").arg(alt), QObject::tr("Orbit")},
            {QObject::tr("%1 + %2 + left drag, middle drag").arg(shift, alt), QObject::tr("Pan")},
            {QObject::tr("Right drag"), QObject::tr("Orbit")},
            {wheel, QObject::tr("Zoom")}};
  }
  case NavigationScheme::AltButtons:
    return {{QObject::tr("Alt + left drag"), QObject::tr("Orbit")},
            {QObject::tr("Alt + middle drag"), QObject::tr("Pan")},
            {QObject::tr("Alt + right drag"), QObject::tr("Zoom")},
            {wheel, QObject::tr("Zoom")}};
  case NavigationScheme::MiddlePanF4:
    return {{QObject::tr("Middle drag"), QObject::tr("Pan")},
            {QObject::tr("Shift + middle drag, F4 + left drag"), QObject::tr("Orbit")},
            {wheel, QObject::tr("Zoom")}};
  case NavigationScheme::MiddleOrbit:
    return {{QObject::tr("Middle drag"), QObject::tr("Orbit")},
            {QObject::tr("Ctrl + middle drag"), QObject::tr("Pan")},
            {QObject::tr("Shift + middle drag"), QObject::tr("Zoom")},
            {wheel, QObject::tr("Zoom")}};
  case NavigationScheme::RightOrbit:
    return {{QObject::tr("Right drag"), QObject::tr("Orbit")},
            {QObject::tr("Middle drag, Shift + right drag"), QObject::tr("Pan")},
            {wheel, QObject::tr("Zoom")}};
  case NavigationScheme::MiddlePan:
    break;
  }
  return {{QObject::tr("Middle drag"), QObject::tr("Pan")},
          {QObject::tr("Shift + middle drag"), QObject::tr("Orbit")},
          {wheel, QObject::tr("Zoom")}};
}

} // namespace

QString visualStyleName(VisualStyle style) {
  for (const StyleEntry& entry : kStyles) {
    if (entry.style == style) {
      return QCoreApplication::translate("View", entry.name);
    }
  }
  return QString();
}

// ---------------------------------------------------------------------------
// Settings

ViewSettings ViewSettings::load() {
  const QSettings store;
  ViewSettings s;
  const auto number = [&store](const char* key, int fallback) {
    return store.value(kGroup + QLatin1String(key), fallback).toInt();
  };
  const auto flag = [&store](const char* key, bool fallback) {
    return store.value(kGroup + QLatin1String(key), fallback).toBool();
  };
  s.style = static_cast<VisualStyle>(
      std::clamp(number("style", int(s.style)), 0, int(VisualStyle::WireframeHiddenEdges)));
  s.perspective = flag("perspective", s.perspective);
#ifdef Q_OS_MACOS
  s.background = Background::Automatic; // unless the settings name another
#endif
  s.background = static_cast<Background>(
      std::clamp(number("background", int(s.background)), 0, int(Background::Automatic)));
  s.top = QColor(store.value(kGroup + QStringLiteral("top"), s.top.name()).toString());
  s.bottom = QColor(store.value(kGroup + QStringLiteral("bottom"), s.bottom.name()).toString());
  s.gridShown = flag("grid", s.gridShown);
  s.gridSpacing = std::max(0.0, store.value(kGroup + QStringLiteral("gridSpacing"), 0.0).toDouble());
  s.snapToGrid = flag("snapToGrid", s.snapToGrid);
#ifdef Q_OS_MACOS
  s.navigation.scheme = NavigationScheme::Trackpad; // unless the settings name another
#endif
  // The scheme by its id. Older settings kept its index under
  // "navigation" instead; it is read here and replaced by the id when the
  // settings are next saved.
  const QString scheme = store.value(kGroup + QStringLiteral("navigationScheme")).toString();
  const auto known = std::find_if(std::begin(kSchemes), std::end(kSchemes), [&scheme](const SchemeEntry& entry) {
    return scheme == QLatin1String(entry.id);
  });
  if (known != std::end(kSchemes)) {
    s.navigation.scheme = known->scheme;
  } else if (store.contains(kGroup + QStringLiteral("navigation"))) {
    s.navigation.scheme = static_cast<NavigationScheme>(
        std::clamp(number("navigation", 0), 0, int(NavigationScheme::RightOrbit)));
  }
  s.navigation.zoomToCursor = flag("zoomToCursor", true);
  s.navigation.reverseZoom = flag("reverseZoom", false);
  s.navigation.constrainedOrbit = flag("constrainedOrbit", false);
  s.navigation.animate = flag("animate", true);
  return s;
}

void ViewSettings::save() const {
  QSettings store;
  store.setValue(kGroup + QStringLiteral("style"), int(style));
  store.setValue(kGroup + QStringLiteral("perspective"), perspective);
  store.setValue(kGroup + QStringLiteral("background"), int(background));
  store.setValue(kGroup + QStringLiteral("top"), top.name());
  store.setValue(kGroup + QStringLiteral("bottom"), bottom.name());
  store.setValue(kGroup + QStringLiteral("grid"), gridShown);
  store.setValue(kGroup + QStringLiteral("gridSpacing"), gridSpacing);
  store.setValue(kGroup + QStringLiteral("snapToGrid"), snapToGrid);
  store.setValue(kGroup + QStringLiteral("navigationScheme"), QLatin1String(schemeEntry(navigation.scheme).id));
  store.remove(kGroup + QStringLiteral("navigation"));
  store.setValue(kGroup + QStringLiteral("zoomToCursor"), navigation.zoomToCursor);
  store.setValue(kGroup + QStringLiteral("reverseZoom"), navigation.reverseZoom);
  store.setValue(kGroup + QStringLiteral("constrainedOrbit"), navigation.constrainedOrbit);
  store.setValue(kGroup + QStringLiteral("animate"), navigation.animate);
}

void ViewSettings::colors(QColor& upper, QColor& lower, bool& gradient) const {
  gradient = true;
  switch (background) {
  case Background::Gradient:
    // Linear RGB 0.93, 0.94, 0.96 and 0.74, 0.77, 0.82 as before U5; the
    // view takes sRGB, as colour dialogs give it.
    upper = QColor(247, 248, 250);
    lower = QColor(223, 227, 234);
    break;
  case Background::Light:
    upper = QColor::fromRgbF(0.99f, 0.99f, 1.0f);
    lower = QColor::fromRgbF(0.87f, 0.89f, 0.92f);
    break;
  case Background::White:
    upper = lower = Qt::white;
    gradient = false;
    break;
  case Background::Dark:
    upper = QColor(0x3b, 0x42, 0x4d);
    lower = QColor(0x1c, 0x20, 0x27);
    break;
  case Background::Custom:
    upper = top;
    lower = bottom;
    gradient = top != bottom;
    break;
  case Background::Automatic:
    if (isDarkPalette()) {
      // A dark blue-grey.
      upper = QColor(0x3b, 0x42, 0x4d);
      lower = QColor(0x1c, 0x20, 0x27);
    } else {
      upper = QColor(247, 248, 250); // the default gradient
      lower = QColor(223, 227, 234);
    }
    break;
  }
}

// ---------------------------------------------------------------------------
// Commands and menus

ViewController::ViewController(OcctViewer& viewer, CommandRegistry& registry, DocumentHost& host,
                               std::function<void()> lookAt, QWidget* window)
    : QObject(window), m_viewer(viewer), m_registry(registry), m_host(host),
      m_lookAt(std::move(lookAt)), m_window(window), m_settings(ViewSettings::load()) {
  // The automatic background follows the system's light and dark modes.
  connect(&m_viewer, &OcctViewer::themeChanged, this, [this] {
    if (m_settings.background != ViewSettings::Background::Automatic) {
      return;
    }
    QColor top;
    QColor bottom;
    bool gradient = true;
    m_settings.colors(top, bottom, gradient);
    m_viewer.setBackground(top, bottom, gradient);
    qDebug().noquote() << QStringLiteral("Background %1 %2").arg(top.name(), bottom.name());
  });
}

void ViewController::registerCommands() {
  const auto add = [this](const char* id, const QString& name, const char* icon, std::function<void()> run,
                          const QStringList& keywords = {}, const QString& tooltip = QString()) {
    CommandDef def;
    def.id = QString::fromLatin1(id);
    def.name = name;
    def.icon = QString::fromLatin1(icon);
    def.kind = CommandDef::Kind::Action;
    def.mode = CommandDef::Mode::Any;
    def.tab.clear(); // the View menu, the command search and shortcuts
    def.keywords = keywords;
    def.keywords << QStringLiteral("view");
    def.tooltip = tooltip;
    def.run = std::move(run);
    // The view can change while a command's panel is open; named views are
    // model commands and wait, and so does Look At, which looks at the
    // selection outside commands.
    static const QStringList waiting = {QStringLiteral("view.named_view"), QStringLiteral("view.set_home"),
                                        QStringLiteral("view.reset_home"), QStringLiteral("view.look_at")};
    def.duringCommands = !waiting.contains(def.id);
    m_registry.add(def);
  };
  add("view.home", tr("Home"), "home", [this] {
    qDebug() << "View home";
    m_viewer.goHome();
  }, {QStringLiteral("home view")}, tr("The home view of the whole design"));
  for (const StandardView& view : kStandardViews) {
    const QString key = QString::fromLatin1(view.key);
    add(qPrintable(QStringLiteral("view.%1").arg(key)), tr("%1 View").arg(tr(view.name)), "orientation-cube",
        [this, key] { showStandardView(key); }, {QStringLiteral("standard view"), key},
        tr("Looks at the design from the %1").arg(tr(view.name).toLower()));
  }
  add("view.look_at", tr("Look At"), "look-at", [this] { m_lookAt(); }, {QStringLiteral("normal to")},
      tr("Looks straight at the selected plane, planar face or sketch"));
  add("view.orthographic", tr("Orthographic"), "orientation-cube", [this] { setPerspective(false); },
      {QStringLiteral("camera"), QStringLiteral("parallel")});
  add("view.perspective", tr("Perspective"), "perspective", [this] { setPerspective(true); },
      {QStringLiteral("camera")});
  for (const StyleEntry& entry : kStyles) {
    const VisualStyle style = entry.style;
    add(entry.id, QCoreApplication::translate("View", entry.name), "visual-style",
        [this, style] { setStyle(style); }, {QStringLiteral("visual style"), QStringLiteral("display")});
  }
  add("view.grid", tr("Layout Grid"), "grid", [this] { setGridShown(!m_settings.gridShown); },
      {QStringLiteral("grid")}, tr("Shows the grid on the ground or sketch plane"));
  add("view.snap_grid", tr("Snap to Grid"), "grid", [this] { setSnapToGrid(!m_settings.snapToGrid); },
      {QStringLiteral("grid"), QStringLiteral("snap")}, tr("Sketch points that snap to nothing else snap to the grid"));
  add("view.grid_settings", tr("Grid Settings..."), "grid", [this] { showGridSettings(); },
      {QStringLiteral("grid"), QStringLiteral("spacing")});
  for (const BackgroundEntry& entry : kBackgrounds) {
    const ViewSettings::Background background = entry.background;
    add(entry.id, QCoreApplication::translate("View", entry.name), "environment",
        [this, background] { setBackground(background); },
        {QStringLiteral("environment"), QStringLiteral("background")});
  }
  add("view.named_view", tr("New Named View"), "named-view", [this] { saveNamedView(QString(), false); },
      {QStringLiteral("save view"), QStringLiteral("camera")}, tr("Saves the current view in the document"));
  add("view.set_home", tr("Set Current View as Home"), "home",
      [this] { saveNamedView(QStringLiteral("Home"), true); }, {QStringLiteral("home")});
  add("view.reset_home", tr("Reset Home"), "home", [this] {
    m_host.runModelCommand({{QStringLiteral("cmd"), QStringLiteral("delete_named_view")},
                            {QStringLiteral("name"), QStringLiteral("Home")}});
  }, {QStringLiteral("home")});

  CommandDef preferences;
  preferences.id = QStringLiteral("tools.preferences");
#ifdef Q_OS_MACOS
  preferences.name = tr("Settings..."); // as macOS names it since 13
#else
  preferences.name = tr("Preferences...");
#endif
  preferences.icon = QStringLiteral("settings");
  preferences.kind = CommandDef::Kind::Action;
  preferences.mode = CommandDef::Mode::Any;
  preferences.tab.clear();
  preferences.keywords = {QStringLiteral("options"), QStringLiteral("settings")};
  preferences.tooltip = tr("General settings, navigation, display, cache, version control, updates and 3D print");
  preferences.run = [this] { showPreferences(); };
  preferences.duringCommands = true;
#ifdef Q_OS_MACOS
  preferences.shortcut = QKeySequence::Preferences; // Cmd+,
#endif
  m_registry.add(preferences);
  // Each page on its own (mitcad#25), so that the search finds where a
  // setting is.
  for (const PreferencePage& entry : kPreferencePages) {
    CommandDef page = preferences;
    const QString id = QLatin1String(entry.id);
    const QString name = QCoreApplication::translate("View", entry.name);
    page.id = QStringLiteral("tools.preferences.%1").arg(id);
    page.name = tr("Preferences: %1").arg(name);
    page.keywords = QString::fromLatin1(entry.keywords).split(QLatin1Char(' '));
    page.tooltip = tr("Preferences, opened on its %1 page").arg(name);
    page.run = [this, id] { showPreferences(id); };
    m_registry.add(page);
  }

  CommandDef overview = preferences;
  overview.shortcut = QKeySequence(); // not Preferences' key
  overview.id = QStringLiteral("help.shortcuts");
  overview.name = tr("Keyboard and Mouse Overview");
  overview.icon = QStringLiteral("keyboard");
  overview.keywords = {QStringLiteral("shortcuts"), QStringLiteral("keys"), QStringLiteral("hotkeys"),
                       QStringLiteral("mouse")};
  overview.tooltip = tr("Every shortcut and how the mouse moves the view");
  overview.run = [this] { showShortcutOverview(); };
  overview.duringCommands = true;
  m_registry.add(overview);
}

void ViewController::createMenus(QMenu* view, QMenu* tools, QMenu* help) {
  const auto action = [this](const QString& id) { return m_registry.action(id); };
  const auto checkable = [this, &action](const QString& id, QActionGroup* group = nullptr) {
    QAction* a = action(id);
    a->setCheckable(true);
    if (group != nullptr) {
      group->addAction(a);
    }
    m_checks.insert(id, a);
    return a;
  };
  QMenu* standard = view->addMenu(themeIcon(QStringLiteral("orientation-cube")), tr("Standard Views"));
  standard->addAction(action(QStringLiteral("view.home")));
  for (const StandardView& entry : kStandardViews) {
    standard->addAction(action(QStringLiteral("view.%1").arg(QString::fromLatin1(entry.key))));
  }
  view->addAction(action(QStringLiteral("view.look_at")));
  QMenu* camera = view->addMenu(themeIcon(QStringLiteral("perspective")), tr("Camera"));
  auto* projection = new QActionGroup(this);
  camera->addAction(checkable(QStringLiteral("view.orthographic"), projection));
  camera->addAction(checkable(QStringLiteral("view.perspective"), projection));
  QMenu* styles = view->addMenu(themeIcon(QStringLiteral("visual-style")), tr("Visual Style"));
  auto* styleGroup = new QActionGroup(this);
  for (const StyleEntry& entry : kStyles) {
    styles->addAction(checkable(QString::fromLatin1(entry.id), styleGroup));
  }
  QMenu* environment = view->addMenu(themeIcon(QStringLiteral("environment")), tr("Environment"));
  auto* backgrounds = new QActionGroup(this);
  for (const BackgroundEntry& entry : kBackgrounds) {
    environment->addAction(checkable(QString::fromLatin1(entry.id), backgrounds));
  }
  QMenu* grid = view->addMenu(themeIcon(QStringLiteral("grid")), tr("Grid and Snaps"));
  grid->addAction(checkable(QStringLiteral("view.grid")));
  grid->addAction(checkable(QStringLiteral("view.snap_grid")));
  grid->addAction(action(QStringLiteral("view.grid_settings")));
  QMenu* named = view->addMenu(themeIcon(QStringLiteral("named-view")), tr("Named Views"));
  named->addAction(action(QStringLiteral("view.named_view")));
  named->addAction(action(QStringLiteral("view.set_home")));
  named->addAction(action(QStringLiteral("view.reset_home")));
  view->addSeparator();
  // PreferencesRole: "Settings..." (Cmd+,) in the application menu on macOS,
  // where the Tools menu then has no separator before it.
  QAction* preferences = action(QStringLiteral("tools.preferences"));
  preferences->setMenuRole(QAction::PreferencesRole);
#ifndef Q_OS_MACOS
  tools->addSeparator();
#endif
  tools->addAction(preferences);
  help->addAction(action(QStringLiteral("help.shortcuts")));
  apply();
}

void ViewController::apply() {
  m_viewer.setVisualStyle(m_settings.style);
  m_viewer.setNavigation(m_settings.navigation);
  m_viewer.setPerspective(m_settings.perspective);
  QColor top;
  QColor bottom;
  bool gradient = true;
  m_settings.colors(top, bottom, gradient);
  m_viewer.setBackground(top, bottom, gradient);
  m_viewer.setGrid(m_settings.gridShown, m_settings.gridSpacing);
  if (gridSnapChanged) {
    gridSnapChanged(m_settings.snapToGrid, m_settings.gridSpacing);
  }
  syncChecks();
}

void ViewController::syncChecks() {
  const auto check = [this](const QString& id, bool on) {
    if (QAction* a = m_checks.value(id)) {
      a->setChecked(on);
    }
  };
  for (const StyleEntry& entry : kStyles) {
    check(QString::fromLatin1(entry.id), entry.style == m_settings.style);
  }
  for (const BackgroundEntry& entry : kBackgrounds) {
    check(QString::fromLatin1(entry.id), entry.background == m_settings.background);
  }
  check(QStringLiteral("view.orthographic"), !m_settings.perspective);
  check(QStringLiteral("view.perspective"), m_settings.perspective);
  check(QStringLiteral("view.grid"), m_settings.gridShown);
  check(QStringLiteral("view.snap_grid"), m_settings.snapToGrid);
}

void ViewController::setStyle(VisualStyle style) {
  m_settings.style = style;
  m_settings.save();
  m_viewer.setVisualStyle(style);
  syncChecks();
  qDebug().noquote() << QStringLiteral("Visual style %1").arg(visualStyleName(style));
}

void ViewController::setPerspective(bool perspective) {
  m_settings.perspective = perspective;
  m_settings.save();
  m_viewer.setPerspective(perspective);
  syncChecks();
  qDebug().noquote() << (perspective ? QStringLiteral("Camera perspective")
                                     : QStringLiteral("Camera orthographic"));
}

void ViewController::setBackground(ViewSettings::Background background) {
  m_settings.background = background;
  m_settings.save();
  QColor top;
  QColor bottom;
  bool gradient = true;
  m_settings.colors(top, bottom, gradient);
  m_viewer.setBackground(top, bottom, gradient);
  syncChecks();
  qDebug().noquote() << QStringLiteral("Background %1 %2").arg(top.name(), bottom.name());
}

void ViewController::setGridShown(bool shown) {
  m_settings.gridShown = shown;
  m_settings.save();
  m_viewer.setGrid(shown, m_settings.gridSpacing);
  syncChecks();
  qDebug().noquote() << QStringLiteral("Layout grid %1").arg(onOff(shown));
}

void ViewController::setSnapToGrid(bool on) {
  if (on == m_settings.snapToGrid) {
    syncChecks();
    return;
  }
  m_settings.snapToGrid = on;
  m_settings.save();
  if (gridSnapChanged) {
    gridSnapChanged(on, m_settings.gridSpacing);
  }
  syncChecks();
  qDebug().noquote() << QStringLiteral("Snap to grid %1").arg(onOff(on));
}

void ViewController::showStandardView(const QString& key) {
  for (const StandardView& view : kStandardViews) {
    if (key == QLatin1String(view.key)) {
      m_viewer.turnTo(view.direction, view.up);
      qDebug().noquote() << QStringLiteral("View %1").arg(key);
      return;
    }
  }
}

CameraState ViewController::cameraOf(const QJsonObject& view) {
  CameraState camera;
  camera.eye = gp_Pnt(xyzOf(view.value(QStringLiteral("eye"))));
  camera.target = gp_Pnt(xyzOf(view.value(QStringLiteral("target"))));
  const gp_XYZ up = xyzOf(view.value(QStringLiteral("up")));
  if (up.Modulus() > 1e-12) {
    camera.up = gp_Dir(up);
  }
  camera.perspective = view.value(QStringLiteral("perspective")).toBool();
  camera.height = view.value(QStringLiteral("height")).toDouble(250.0);
  return camera;
}

bool ViewController::saveNamedView(const QString& name, bool replace) {
  const CameraState camera = m_viewer.camera();
  QJsonObject command{{QStringLiteral("cmd"), QStringLiteral("add_named_view")},
                      {QStringLiteral("eye"), xyz(camera.eye.XYZ())},
                      {QStringLiteral("target"), xyz(camera.target.XYZ())},
                      {QStringLiteral("up"), xyz(camera.up.XYZ())},
                      {QStringLiteral("height"), camera.height},
                      {QStringLiteral("replace"), replace}};
  if (camera.perspective) {
    command.insert(QStringLiteral("perspective"), true);
  }
  if (!name.isEmpty()) {
    command.insert(QStringLiteral("name"), name);
  }
  QJsonObject result;
  if (!m_host.runModelCommand(command, &result)) {
    return false;
  }
  qDebug().noquote() << QStringLiteral("Saved named view %1").arg(result.value(QStringLiteral("name")).toString());
  return true;
}

void ViewController::showOrientationCubeMenu(const QPoint& globalPosition) {
  QMenu menu(m_window);
  QStringList entries;
  const auto add = [&menu, &entries](QAction* action) {
    menu.addAction(action);
    entries << action->text().remove(QLatin1Char('&'));
  };
  for (const char* id : {"view.home", "view.orthographic", "view.perspective", "view.set_home",
                         "view.reset_home", "view.fit"}) {
    if (QAction* a = m_registry.action(QString::fromLatin1(id))) {
      add(a);
    }
  }
  qDebug().noquote() << QStringLiteral("Context menu: %1").arg(entries.join(QStringLiteral(" | ")));
  menu.exec(globalPosition);
}

// ---------------------------------------------------------------------------
// Dialogs

void ViewController::showGridSettings() {
  QDialog dialog(m_window);
  dialog.setWindowTitle(tr("Grid Settings"));
  auto* layout = new QVBoxLayout(&dialog);
  auto* automatic = new QRadioButton(tr("Automatic: the spacing follows the zoom"));
  auto* fixed = new QRadioButton(tr("Fixed spacing"));
  auto* spacing = new DecimalSpinBox;
  spacing->setRange(0.01, 10000.0);
  spacing->setDecimals(2);
  spacing->setSuffix(QStringLiteral(" mm"));
  spacing->setValue(m_settings.gridSpacing > 0.0 ? m_settings.gridSpacing : 10.0);
  (m_settings.gridSpacing > 0.0 ? fixed : automatic)->setChecked(true);
  spacing->setEnabled(fixed->isChecked());
  connect(fixed, &QRadioButton::toggled, spacing, &QWidget::setEnabled);
  layout->addWidget(automatic);
  auto* row = new QHBoxLayout;
  row->addWidget(fixed);
  row->addWidget(spacing);
  layout->addLayout(row);
  layout->addWidget(new QLabel(tr("Sketch points snap to the fixed spacing; with the automatic "
                                  "spacing the snap steps follow the zoom.")));
  auto* buttons = new QDialogButtonBox(QDialogButtonBox::Ok | QDialogButtonBox::Cancel);
  connect(buttons, &QDialogButtonBox::accepted, &dialog, &QDialog::accept);
  connect(buttons, &QDialogButtonBox::rejected, &dialog, &QDialog::reject);
  layout->addWidget(buttons);
  // Millimetres, a metric 1-2-5 grid when automatic (P11).
  qDebug().noquote() << QStringLiteral("Grid settings dialog opened: %1, fixed spacing %2%3")
                            .arg(fixed->isChecked() ? QStringLiteral("fixed") : QStringLiteral("automatic"))
                            .arg(spacing->value())
                            .arg(spacing->suffix());
  prepareModal(&dialog);
  if (dialog.exec() != QDialog::Accepted) {
    return;
  }
  m_settings.gridSpacing = fixed->isChecked() ? spacing->value() : 0.0;
  m_settings.save();
  m_viewer.setGrid(m_settings.gridShown, m_settings.gridSpacing);
  if (gridSnapChanged) {
    gridSnapChanged(m_settings.snapToGrid, m_settings.gridSpacing);
  }
  qDebug().noquote() << (m_settings.gridSpacing > 0.0
                             ? QStringLiteral("Grid spacing %1 mm").arg(m_settings.gridSpacing)
                             : QStringLiteral("Grid spacing automatic"));
}

// The Preferences dialog (OK and Cancel) with a list of its pages; on macOS
// the Settings window instead: the same pages as panes, chosen in a row of
// buttons at the top, whose changes apply as they are made.
void ViewController::showPreferences(const QString& page) {
  const QString wanted = page.isEmpty() ? m_preferencesPage : page;
  int wantedIndex = 0;
  for (int i = 0; i < int(std::size(kPreferencePages)); ++i) {
    if (QLatin1String(kPreferencePages[i].id) == wanted) {
      wantedIndex = i;
    }
  }
  SettingsWindow* window = nullptr;
  std::unique_ptr<QDialog> dialog;
  QWidget* host = nullptr;
#ifdef Q_OS_MACOS
  if (auto* open = qobject_cast<SettingsWindow*>(m_settingsWindow.data())) {
    if (!page.isEmpty()) {
      open->showPane(wantedIndex);
    }
    open->show();
    open->raise();
    open->activateWindow();
    return;
  }
  window = new SettingsWindow(m_window);
  m_settingsWindow = window;
  host = window;
#else
  dialog = std::make_unique<QDialog>(m_window);
  dialog->setWindowTitle(tr("Preferences"));
  host = dialog.get();
#endif
  // Pages (mitcad#25): the groups are made first and go to their pages
  // below.
  QHash<QString, QWidget*> groups;
  // A group with a frame and a title in the dialog, a plain area in a pane.
  const auto section = [window](const QString& title) -> QWidget* {
    return window != nullptr ? new QWidget : new QGroupBox(title);
  };
  // A row of only a control: in the dialog it spans the form, in a pane it
  // stands under the other controls, not under their labels.
  const auto control = [window](QFormLayout* form, auto* what) {
    if (window != nullptr) {
      // macOS sizes a check box a few pixels short of its text.
      if (auto* box = qobject_cast<QCheckBox*>(static_cast<QObject*>(what))) {
        box->ensurePolished();
        box->setMinimumWidth(box->sizeHint().width() + 8);
      }
      form->addRow(QString(), what);
    } else {
      form->addRow(what);
    }
  };
  // What a change does: the dialog takes all of them when it is accepted,
  // the window as each is made.
  auto commit = std::make_shared<std::function<void()>>();
  const auto changed = [commit, live = window != nullptr] {
    if (live && *commit) {
      (*commit)();
    }
  };

  // General (P8): how often autosave writes unsaved work, and where.
  const auto general = std::make_shared<GeneralSettings>(GeneralSettings::load());
  QWidget* generalBox = section(tr("General"));
  auto* generalForm = new QFormLayout(generalBox);
  auto* autosave = new QCheckBox(tr("&Save recovery data every"));
  autosave->setChecked(general->autosave);
  autosave->setToolTip(tr("Unsaved changes are written to the recovery folder, from where they can be "
                          "recovered after a crash"));
  auto* minutes = new QSpinBox;
  minutes->setRange(GeneralSettings::kMinAutosaveMinutes, GeneralSettings::kMaxAutosaveMinutes);
  minutes->setSuffix(tr(" min"));
  minutes->setValue(general->autosaveMinutes);
  minutes->setEnabled(general->autosave);
  connect(autosave, &QCheckBox::toggled, minutes, &QWidget::setEnabled);
  auto* autosaveRow = new QHBoxLayout;
  autosaveRow->addWidget(autosave);
  autosaveRow->addWidget(minutes);
  autosaveRow->addStretch(1);
  if (window != nullptr) {
    generalForm->addRow(QString(), autosaveRow);
  } else {
    generalForm->addRow(autosaveRow);
  }
  const QString folder = recoveryDirectory();
  auto* folderPath = new QLineEdit(QDir::toNativeSeparators(folder));
  folderPath->setReadOnly(true);
  folderPath->setToolTip(tr("Never a project's folder. What is here goes when the document is saved or "
                            "Mitcad ends normally."));
  if (window != nullptr) {
    folderPath->setMinimumWidth(240);
  }
  auto* openFolder = new QPushButton(tr("Open Folder"));
  openFolder->setEnabled(!folder.isEmpty());
  connect(openFolder, &QPushButton::clicked, host, [folder] {
    QDir().mkpath(folder);
    QDesktopServices::openUrl(QUrl::fromLocalFile(folder));
  });
  auto* folderRow = new QHBoxLayout;
  folderRow->addWidget(folderPath, 1);
  folderRow->addWidget(openFolder);
  generalForm->addRow(tr("Recovery folder:"), folderRow);
  // Who versions of projects are recorded by (P12d): git's configured user
  // by default, else the name and email here.
  const auto versions = std::make_shared<VersionSettings>(VersionSettings::load());
  auto* useGit = new QCheckBox(tr("Record versions with &git's name and email (user.name, user.email)"));
  useGit->setChecked(versions->useGit);
  useGit->setToolTip(tr("When git has none, or when this is off, the name and email below are used"));
  control(generalForm, useGit);
  auto* authorName = new QLineEdit(versions->name);
  auto* authorEmail = new QLineEdit(versions->email);
  generalForm->addRow(tr("Version author's &name:"), authorName);
  generalForm->addRow(tr("Version author's e&mail:"), authorEmail);
  auto* privacy = new QLabel(tr("The name and email address go into every version of a project; anyone who gets "
                                "its history sees them."));
  privacy->setWordWrap(true);
  control(generalForm, privacy);
  groups.insert(QStringLiteral("general"), generalBox);
  // Automatic updates (mitcad#9).
  auto* updatesBox = new UpdatePreferencesBox;
  groups.insert(QStringLiteral("updates"), updatesBox);
  // Computed results in memory and on disk (P7d).
  auto* cacheBox = new CachePreferencesBox([this] {
    if (showDiagnostics) {
      showDiagnostics();
    }
  });
  groups.insert(QStringLiteral("cache"), cacheBox);
  // The slicer 3D Print starts (mitcad#13).
  auto* slicerBox = new SlicerPreferencesBox;
  groups.insert(QStringLiteral("print"), slicerBox);
  // Remote repositories: the git program, checks and pushes (P12 remote).
  auto* remoteBox = new RemotePreferencesBox;
  groups.insert(QStringLiteral("version_control"), remoteBox);

  QWidget* navigationBox = section(tr("Navigation"));
  auto* navigation = new QFormLayout(navigationBox);
  auto* scheme = new QComboBox;
  for (const SchemeEntry& entry : kSchemes) {
    scheme->addItem(schemeName(entry.scheme), int(entry.scheme));
  }
  scheme->setCurrentIndex(scheme->findData(int(m_settings.navigation.scheme)));
  navigation->addRow(tr("Pan, zoom, orbit shortcuts:"), scheme);
  auto* zoomToCursor = new QCheckBox(tr("Zoom about the cursor"));
  zoomToCursor->setChecked(m_settings.navigation.zoomToCursor);
  auto* reverseZoom = new QCheckBox(tr("Reverse the zoom direction"));
  reverseZoom->setChecked(m_settings.navigation.reverseZoom);
  auto* constrained = new QCheckBox(tr("Constrained orbit (Z stays up)"));
  constrained->setChecked(m_settings.navigation.constrainedOrbit);
  auto* animate = new QCheckBox(tr("Smooth view transitions"));
  animate->setChecked(m_settings.navigation.animate);
  for (QCheckBox* box : {zoomToCursor, reverseZoom, constrained, animate}) {
    control(navigation, box);
  }
  groups.insert(QStringLiteral("navigation"), navigationBox);

  QWidget* displayBox = section(tr("Display"));
  auto* display = new QFormLayout(displayBox);
  auto* camera = new QComboBox;
  camera->addItems({tr("Orthographic"), tr("Perspective")});
  camera->setCurrentIndex(m_settings.perspective ? 1 : 0);
  display->addRow(tr("Camera:"), camera);
  auto* background = new QComboBox;
  for (const BackgroundEntry& entry : kBackgrounds) {
    QString name = QCoreApplication::translate("View", entry.name);
    if (window != nullptr && entry.background == ViewSettings::Background::Automatic) {
      name = tr("Follow System"); // the light or the dark gradient by the appearance
    }
    background->addItem(name);
  }
  background->setCurrentIndex(int(m_settings.background));
  display->addRow(window != nullptr ? tr("View background:") : tr("Environment:"), background);
  // The colours of the custom background; the buttons change them.
  const auto top = std::make_shared<QColor>(m_settings.top);
  const auto bottom = std::make_shared<QColor>(m_settings.bottom);
  const auto colorButton = [host, changed](const std::shared_ptr<QColor>& color, const QString& title) {
    auto* button = new QPushButton;
    const auto show = [button, color] {
      button->setIcon(swatchIcon(*color));
      button->setText(color->name());
    };
    show();
    connect(button, &QPushButton::clicked, host, [host, color, title, show, changed] {
      const QColor chosen = QColorDialog::getColor(*color, host, title);
      if (chosen.isValid()) {
        *color = chosen;
        show();
        changed();
      }
    });
    return button;
  };
  auto* topButton = colorButton(top, tr("Top Colour"));
  auto* bottomButton = colorButton(bottom, tr("Bottom Colour"));
  display->addRow(tr("Custom top:"), topButton);
  display->addRow(tr("Custom bottom:"), bottomButton);
  const auto enableCustom = [background, topButton, bottomButton] {
    const bool custom = background->currentIndex() == int(ViewSettings::Background::Custom);
    topButton->setEnabled(custom);
    bottomButton->setEnabled(custom);
  };
  enableCustom();
  connect(background, &QComboBox::currentIndexChanged, host, enableCustom);
  groups.insert(QStringLiteral("display"), displayBox);

  *commit = [this, general, versions, top, bottom, scheme, zoomToCursor, reverseZoom, constrained, animate, camera,
             background, autosave, minutes, useGit, authorName, authorEmail, cacheBox, updatesBox, slicerBox,
             remoteBox] {
    m_settings.navigation.scheme = static_cast<NavigationScheme>(scheme->currentData().toInt());
    m_settings.navigation.zoomToCursor = zoomToCursor->isChecked();
    m_settings.navigation.reverseZoom = reverseZoom->isChecked();
    m_settings.navigation.constrainedOrbit = constrained->isChecked();
    m_settings.navigation.animate = animate->isChecked();
    m_settings.perspective = camera->currentIndex() == 1;
    m_settings.background = static_cast<ViewSettings::Background>(background->currentIndex());
    m_settings.top = *top;
    m_settings.bottom = *bottom;
    m_settings.save();
    GeneralSettings chosen = *general;
    chosen.autosave = autosave->isChecked();
    chosen.autosaveMinutes = minutes->value();
    chosen.save();
    VersionSettings author = *versions;
    author.useGit = useGit->isChecked();
    author.name = authorName->text().trimmed();
    author.email = authorEmail->text().trimmed();
    // Chosen here, the author need not be shown again on the next version.
    author.confirmed = versions->confirmed || author.useGit != versions->useGit || author.name != versions->name ||
                       author.email != versions->email;
    author.save();
    *versions = author;
    const QString own = author.complete() ? author.author() : QStringLiteral("nobody");
    qDebug().noquote() << QStringLiteral("Preferences: versions by %1")
                              .arg(author.useGit ? QStringLiteral("git's user, else %1").arg(own) : own);
    const CacheSettings cache = cacheBox->chosen();
    cache.save();
    qDebug().noquote() << QStringLiteral("Preferences: results on disk %1, at most %2 MB; in memory at most %3 MB")
                              .arg(onOff(cache.disk))
                              .arg(cache.diskMegabytes)
                              .arg(cache.memoryMegabytes);
    const UpdateSettings updates = updatesBox->chosen();
    updates.save();
    qDebug().noquote() << QStringLiteral("Preferences: update checks %1, channel %2")
                              .arg(onOff(updates.automatic),
                                   updates.channel == UpdateSettings::Channel::Prerelease
                                       ? QStringLiteral("prerelease")
                                       : QStringLiteral("stable"));
    PrintSettings print = PrintSettings::load();
    print.slicer = slicerBox->chosen();
    print.save();
    qDebug().noquote() << QStringLiteral("Preferences: slicer %1")
                              .arg(print.slicer.isValid() ? print.slicer.describe() : QStringLiteral("automatic"));
    const RemoteSettings remote = remoteBox->chosen();
    remote.save();
    qDebug().noquote() << QStringLiteral("Preferences: git %1, remote checks %2, each version sent %3")
                              .arg(remote.git.isEmpty() ? QStringLiteral("found automatically") : remote.git,
                                   remote.checkMinutes > 0 ? QStringLiteral("every %1 min").arg(remote.checkMinutes)
                                                           : QStringLiteral("never"),
                                   onOff(remote.autoPush));
    apply();
    if (generalChanged) {
      generalChanged();
    }
    *general = chosen;
    qDebug().noquote() << QStringLiteral("Preferences: navigation %1, zoom to cursor %2, reverse zoom %3, "
                                         "constrained orbit %4, %5, %6")
                              .arg(QLatin1String(schemeEntry(m_settings.navigation.scheme).id),
                                   onOff(m_settings.navigation.zoomToCursor),
                                   onOff(m_settings.navigation.reverseZoom),
                                   onOff(m_settings.navigation.constrainedOrbit),
                                   m_settings.perspective ? QStringLiteral("perspective")
                                                          : QStringLiteral("orthographic"),
                                   chosen.autosave ? QStringLiteral("autosave every %1 min").arg(chosen.autosaveMinutes)
                                                   : QStringLiteral("autosave off"));
  };

  if (window != nullptr) {
    // A pane per page; the pane's name is the group's title, which the
    // boxes then do without. The pane's margins are the window's; a form
    // that is narrower than the pane (macOS centres it) stands at the left.
    struct PaneIcon {
      const char* id;
      const char* symbol;
      const char* other; // an older system's symbol, if the first is newer
    };
    static constexpr PaneIcon kPaneIcons[] = {
        {"general", "gearshape", ""},
        {"navigation", "hand.draw", "move.3d"},
        {"display", "display", ""},
        {"cache", "internaldrive", ""},
        {"version_control", "arrow.triangle.branch", ""},
        {"updates", "arrow.down.circle", ""},
        {"print", "printer", ""},
    };
    for (const PreferencePage& entry : kPreferencePages) {
      QWidget* group = groups.value(QLatin1String(entry.id));
      if (auto* box = qobject_cast<QGroupBox*>(group)) {
        box->setTitle(QString());
        box->setFlat(true);
      }
      if (auto* form = qobject_cast<QFormLayout*>(group->layout())) {
        form->setContentsMargins(0, 0, 0, 0);
        form->setFormAlignment(Qt::AlignLeft | Qt::AlignTop);
      }
      auto* content = new QWidget;
      auto* contentLayout = new QVBoxLayout(content);
      contentLayout->setContentsMargins(0, 0, 0, 0);
      contentLayout->addWidget(group);
      contentLayout->addStretch(1);
      QIcon icon;
      for (const PaneIcon& pane : kPaneIcons) {
        if (QLatin1String(pane.id) == QLatin1String(entry.id)) {
          icon = mac::symbolIcon(QLatin1String(pane.symbol), mac::symbolIcon(QLatin1String(pane.other)));
        }
      }
      window->addPane(QCoreApplication::translate("View", entry.name), icon, content);
    }
    // No OK: a change is made when it is made. The text boxes when they are
    // left, and what is typed in one when the window closes.
    connect(scheme, &QComboBox::currentIndexChanged, host, changed);
    connect(camera, &QComboBox::currentIndexChanged, host, changed);
    connect(background, &QComboBox::currentIndexChanged, host, changed);
    for (QCheckBox* box : {zoomToCursor, reverseZoom, constrained, animate, autosave, useGit}) {
      connect(box, &QCheckBox::toggled, host, changed);
    }
    connect(minutes, &QSpinBox::valueChanged, host, changed);
    connect(authorName, &QLineEdit::editingFinished, host, changed);
    connect(authorEmail, &QLineEdit::editingFinished, host, changed);
    connect(cacheBox, &CachePreferencesBox::changed, host, changed);
    // The boxes of the other pages tell no change of their own: their
    // controls do.
    for (QWidget* box : std::initializer_list<QWidget*>{updatesBox, slicerBox, remoteBox}) {
      for (auto* check : box->findChildren<QCheckBox*>()) {
        connect(check, &QCheckBox::toggled, host, changed);
      }
      for (auto* combo : box->findChildren<QComboBox*>()) {
        connect(combo, &QComboBox::currentIndexChanged, host, changed);
      }
      for (auto* spin : box->findChildren<QSpinBox*>()) {
        connect(spin, &QSpinBox::valueChanged, host, changed);
      }
      for (auto* edit : box->findChildren<QLineEdit*>()) {
        connect(edit, &QLineEdit::editingFinished, host, changed);
      }
    }
    window->setClosedHandler(changed);
    window->setWindowTitle(tr("Settings"));
    window->showPane(wantedIndex);
    window->show();
    return;
  }

  auto* layout = new QVBoxLayout(dialog.get());
  auto* pages = new QListWidget;
  auto* stack = new QStackedWidget;
  for (const PreferencePage& entry : kPreferencePages) {
    auto* content = new QWidget;
    auto* contentLayout = new QVBoxLayout(content);
    contentLayout->setContentsMargins(0, 0, 0, 0);
    contentLayout->addWidget(groups.value(QLatin1String(entry.id)));
    contentLayout->addStretch(1);
    stack->addWidget(content);
    auto* item = new QListWidgetItem(QCoreApplication::translate("View", entry.name), pages);
    item->setData(Qt::UserRole, QLatin1String(entry.id));
  }
  pages->setSpacing(2);
  pages->setMaximumWidth(pages->sizeHintForColumn(0) + 2 * pages->frameWidth() + 24);
  connect(pages, &QListWidget::currentRowChanged, stack, &QStackedWidget::setCurrentIndex);
  connect(pages, &QListWidget::currentRowChanged, dialog.get(), [this, pages](int row) {
    if (row >= 0) {
      // Preferences opens there again (until Mitcad ends).
      m_preferencesPage = pages->item(row)->data(Qt::UserRole).toString();
    }
  });
  auto* pagesRow = new QHBoxLayout;
  pagesRow->addWidget(pages);
  pagesRow->addWidget(stack, 1);
  layout->addLayout(pagesRow, 1);
  pages->setCurrentRow(wantedIndex);

  auto* buttons = new QDialogButtonBox(QDialogButtonBox::Ok | QDialogButtonBox::Cancel);
  connect(buttons, &QDialogButtonBox::accepted, dialog.get(), &QDialog::accept);
  connect(buttons, &QDialogButtonBox::rejected, dialog.get(), &QDialog::reject);
  layout->addWidget(buttons);
  pages->setFocus();
  // The size the window asks for, the largest page's (UI tests check that
  // it fits a 1366 x 768 screen).
  const QSize size = dialog->sizeHint();
  qDebug().noquote() << QStringLiteral("Preferences opened: %1, %2 x %3")
                            .arg(pages->currentItem()->text())
                            .arg(size.width())
                            .arg(size.height());
  if (dialog->exec() != QDialog::Accepted) {
    return;
  }
  (*commit)();
}

void ViewController::showShortcutOverview() {
  QDialog dialog(m_window);
  dialog.setWindowTitle(tr("Keyboard and Mouse Overview"));
  auto* layout = new QVBoxLayout(&dialog);
  auto* filter = new QLineEdit;
  filter->setPlaceholderText(tr("Search"));
  filter->setClearButtonEnabled(true);
  layout->addWidget(filter);
  auto* tree = new QTreeWidget;
  tree->setColumnCount(2);
  tree->setHeaderLabels({tr("Keys or mouse"), tr("Does")});
  tree->header()->setSectionResizeMode(0, QHeaderView::ResizeToContents);
  tree->setRootIsDecorated(true);
  layout->addWidget(tree);

  int count = 0;
  const auto section = [tree](const QString& title) {
    auto* item = new QTreeWidgetItem(tree, {title});
    QFont bold = item->font(0);
    bold.setBold(true);
    item->setFont(0, bold);
    item->setExpanded(true);
    return item;
  };
  QTreeWidgetItem* mouse = section(tr("Mouse (%1)").arg(schemeName(m_settings.navigation.scheme)));
  for (const auto& [keys, what] : mouseRows(m_settings.navigation.scheme)) {
    new QTreeWidgetItem(mouse, {keys, what});
  }
  for (const auto& [keys, what] :
       QVector<QPair<QString, QString>>{{tr("Left click"), tr("Select; Ctrl or Shift adds or removes")},
                                        {tr("Left drag to the right"), tr("Select what lies inside")},
                                        {tr("Left drag to the left"), tr("Select what the window crosses")},
                                        {tr("Right click"), tr("The commands that fit the selection")},
                                        {tr("Trackpad: pinch"), tr("Zoom (in every scheme)")},
                                        {tr("Trackpad: two-finger rotation"), tr("Roll the view (in every scheme)")},
                                        {tr("Trackpad: two-finger double tap"), tr("Fit (in every scheme)")},
                                        {tr("Orientation cube: click"), tr("Turn to a face, edge or corner")},
                                        {tr("Orientation cube: drag"), tr("Orbit")}}) {
    new QTreeWidgetItem(mouse, {keys, what});
  }
  QTreeWidgetItem* keys = section(tr("Keys"));
  // Ctrl is Cmd on macOS, where Backspace (the delete key) deletes too.
  const QString fileKeys =
      QStringList{QKeySequence(QKeySequence::New).toString(QKeySequence::NativeText),
                  QKeySequence(QKeySequence::Open).toString(QKeySequence::NativeText),
                  QKeySequence(QKeySequence::Save).toString(QKeySequence::NativeText),
                  QKeySequence(Qt::CTRL | Qt::SHIFT | Qt::Key_S).toString(QKeySequence::NativeText)}
          .join(QStringLiteral(", "));
#ifdef Q_OS_MACOS
  const QString deleteKeys = tr("Delete, Backspace");
#else
  const QString deleteKeys = tr("Delete");
#endif
  for (const auto& [key, what] :
       QVector<QPair<QString, QString>>{{tr("Enter"), tr("OK: finish the command, or the typed value")},
                                        {tr("Esc"), tr("Cancel the command, step back in a sketch tool, clear the selection")},
                                        {tr("Tab"), tr("The next value box of a sketch tool")},
                                        {deleteKeys, tr("Delete the selection")},
                                        {tr("F2"), tr("Rename in the browser")},
                                        {fileKeys, tr("New, Open, Save, Save As")}}) {
    new QTreeWidgetItem(keys, {key, what});
  }
  // The commands with shortcuts, by tab and group.
  QHash<QString, QTreeWidgetItem*> groups;
  for (const auto& def : m_registry.commands()) {
    const QKeySequence sequence = m_registry.shortcut(def->id);
    if (sequence.isEmpty()) {
      continue;
    }
    const QString group = def->tab.isEmpty() ? tr("General")
                                             : QStringLiteral("%1 / %2").arg(def->tab, def->group);
    QTreeWidgetItem*& parent = groups[group];
    if (parent == nullptr) {
      parent = section(group);
    }
    auto* row = new QTreeWidgetItem(parent, {sequence.toString(QKeySequence::NativeText), def->name});
    row->setIcon(1, themeIcon(def->icon));
    ++count;
  }
  connect(filter, &QLineEdit::textChanged, &dialog, [tree](const QString& text) {
    for (int i = 0; i < tree->topLevelItemCount(); ++i) {
      QTreeWidgetItem* top = tree->topLevelItem(i);
      int shown = 0;
      for (int j = 0; j < top->childCount(); ++j) {
        QTreeWidgetItem* row = top->child(j);
        const bool match = text.isEmpty() || row->text(0).contains(text, Qt::CaseInsensitive) ||
                           row->text(1).contains(text, Qt::CaseInsensitive);
        row->setHidden(!match);
        shown += match ? 1 : 0;
      }
      top->setHidden(shown == 0);
    }
  });
  auto* buttons = new QDialogButtonBox(QDialogButtonBox::Close);
  QPushButton* edit = buttons->addButton(tr("Change Shortcuts..."), QDialogButtonBox::ActionRole);
  connect(edit, &QPushButton::clicked, &dialog, [this, &dialog] {
    dialog.accept();
    if (QAction* shortcuts = m_registry.action(QStringLiteral("tools.shortcuts"))) {
      shortcuts->trigger();
    }
  });
  connect(buttons, &QDialogButtonBox::rejected, &dialog, &QDialog::reject);
  layout->addWidget(buttons);
  dialog.resize(560, 620);
  qDebug().noquote() << QStringLiteral("Shortcut overview: %1 command shortcuts").arg(count);
  prepareModal(&dialog);
  dialog.exec();
}

} // namespace mitcad
