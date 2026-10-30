// SPDX-License-Identifier: MIT
#include "ViewController.hpp"
#include "NavigationPreferences.hpp"

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
#include <QMessageBox>
#include <QMenu>
#include <QPushButton>
#include <QRadioButton>
#include <QScreen>
#include <QSettings>
#include <QSignalBlocker>
#include <QSpinBox>
#include <QStackedWidget>
#include <QStyle>
#include <QTimer>
#include <QTreeWidget>
#include <QUrl>
#include <QVBoxLayout>
#include <QtLogging>

#include "../browser/BrowserBackground.hpp"
#include "../browser/DocumentHost.hpp"
#include "../framework/Dialogs.hpp"
#include "../platform/NativeDialogs.hpp"
#include "../framework/AppSettings.hpp"
#include "../framework/CachePreferences.hpp"
#include "../framework/ChromeStyle.hpp"
#include "../framework/CommandRegistry.hpp"
#include "../framework/DebugLog.hpp"
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
// View > Rendered (mitcad#32).
#include "../render/RenderClient.hpp"
#include "../render/RenderDevice.hpp"
#include "../render/RenderEnvironmentDialog.hpp"
#include "../render/RenderImageDialog.hpp"
#include "../render/RenderMode.hpp"
// Texture folders for Edit Appearances (mitcad#315).
#include "../materials/TexturePreferences.hpp"
#include "../agent/ChatPreferences.hpp"
#include "../mcp/AgentFolders.hpp"
// Font folders for sketch text (mitcad#473).
#include "../sketch/FontFiles.hpp"
#include "../platform/MacDialogs.hpp"
// The Sketch page: the first dimension scales the sketch (mitcad#860).
#include "../sketch/SketchPreferences.hpp"
// General: ask to activate the component of a picked face (mitcad#960).
#include "../browser/ActivateQuestion.hpp"

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
    {NavigationScheme::TrackpadShift, "trackpad-shift",
     QT_TRANSLATE_NOOP("View", "Trackpad (two-finger scroll pans, pinch zooms, Shift orbits)")},
    {NavigationScheme::Custom, "custom", QT_TRANSLATE_NOOP("View", "Custom")},
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
    {"general", QT_TRANSLATE_NOOP("View", "General"), "autosave recovery activate component ask picked face"},
    {"navigation", QT_TRANSLATE_NOOP("View", "Navigation"), "mouse orbit pan zoom scheme"},
    {"display", QT_TRANSLATE_NOOP("View", "Display"),
     "camera perspective orthographic orientation view cube projection follow background environment render device gpu cuda ghost inactive components pick priority layout docked floating browser background rows card"},
    {"cache", QT_TRANSLATE_NOOP("View", "Cache"), "results memory disk diagnostics"},
    {"cloud", QT_TRANSLATE_NOOP("View", "Cloud"),
     "git remote sync push fetch version control author email name mqtt live broker"},
    {"updates", QT_TRANSLATE_NOOP("View", "Updates"), "release channel pre-release"},
    {"print", QT_TRANSLATE_NOOP("View", "3D Print"), "slicer printing"},
    // The debug log (mitcad#269).
    {"diagnostics", QT_TRANSLATE_NOOP("View", "Diagnostics"), "debug log logging trace problem report"},
    // Texture folders for Edit Appearances (mitcad#315).
    {"textures", QT_TRANSLATE_NOOP("View", "Textures"), "texture folders maps materials appearance images"},
    // The folders for designs, which AI assistants save to and open from
    // (mitcad#402).
    {"agents", QT_TRANSLATE_NOOP("View", "AI Assistants"), "mcp agent agents ai folders save open connection assistant assistants designs codex claude"},
    // Folders of font files for sketch text (mitcad#473).
    {"fonts", QT_TRANSLATE_NOOP("View", "Fonts"), "font folders text ttf otf woff typeface"},
    // The first dimension scales the sketch (mitcad#860).
    {"sketch", QT_TRANSLATE_NOOP("View", "Sketch"), "sketch dimension first scale proportion drawn size"},
};

// The pages whose folder list takes the page's height (mitcad#727,
// mitcad#732); the others keep their natural height at the top.
bool pageFills(const char* id) {
  const QLatin1String page(id);
  return page == QLatin1String("textures") || page == QLatin1String("agents") || page == QLatin1String("fonts");
}

QString onOff(bool on) { return on ? QStringLiteral("on") : QStringLiteral("off"); }

QJsonArray xyz(const gp_XYZ& v) { return QJsonArray{v.X(), v.Y(), v.Z()}; }

gp_XYZ xyzOf(const QJsonValue& value) {
  const QJsonArray a = value.toArray();
  return gp_XYZ(a.at(0).toDouble(), a.at(1).toDouble(), a.at(2).toDouble());
}

// The mouse of a navigation scheme, for the overview.
QVector<QPair<QString, QString>> mouseRows(const NavigationSettings& settings) {
  const NavigationScheme scheme = settings.scheme;
  if (scheme == NavigationScheme::Custom) {
    QVector<QPair<QString, QString>> result;
    for (const NavigationBinding& binding : settings.customBindings) {
      if (navigationBindingAllowed(binding) && binding.action != NavigationAction::None)
        result.append({navigationInputText(binding), navigationActionText(binding.action)});
    }
    result.append({QObject::tr("Mouse wheel"), QObject::tr("Zoom")});
    return result;
  }
  const QString wheel = QObject::tr("Mouse wheel");
  switch (scheme) {
  case NavigationScheme::Trackpad:
  case NavigationScheme::TrackpadShift: {
    const QString alt = altKey();
    const QString shift = QStringLiteral("Shift");
    const bool shiftOrbit = scheme == NavigationScheme::TrackpadShift;
    return {{QObject::tr("Two-finger scroll"), QObject::tr("Pan")},
            {QObject::tr("%1 + two-finger scroll").arg(shiftOrbit ? shift : alt), QObject::tr("Orbit")},
            {QObject::tr("%1 or %2 + two-finger scroll").arg(shiftOrbit ? alt : shift, ctrlKey()), QObject::tr("Zoom")},
            {QObject::tr("%1 + left drag").arg(alt), QObject::tr("Orbit")},
            {QObject::tr("%1 + %2 + left drag, middle drag").arg(shift, alt), QObject::tr("Pan")},
            {QObject::tr("Right drag"), QObject::tr("Orbit")},
            {wheel, QObject::tr("Zoom")}};
  }
  case NavigationScheme::AltButtons:
    return {{QObject::tr("%1 + left drag").arg(altKey()), QObject::tr("Orbit")},
            {QObject::tr("%1 + middle drag").arg(altKey()), QObject::tr("Pan")},
            {QObject::tr("%1 + right drag").arg(altKey()), QObject::tr("Zoom")},
            {wheel, QObject::tr("Zoom")}};
  case NavigationScheme::MiddlePanF4:
    return {{QObject::tr("Middle drag"), QObject::tr("Pan")},
            {QObject::tr("Shift + middle drag, F4 + left drag"), QObject::tr("Orbit")},
            {wheel, QObject::tr("Zoom")}};
  case NavigationScheme::MiddleOrbit:
    return {{QObject::tr("Middle drag"), QObject::tr("Orbit")},
            {QObject::tr("%1 + middle drag").arg(ctrlKey()), QObject::tr("Pan")},
            {QObject::tr("Shift + middle drag"), QObject::tr("Zoom")},
            {wheel, QObject::tr("Zoom")}};
  case NavigationScheme::RightOrbit:
    return {{QObject::tr("Right drag"), QObject::tr("Orbit")},
            {QObject::tr("Middle drag, Shift + right drag"), QObject::tr("Pan")},
            {wheel, QObject::tr("Zoom")}};
  case NavigationScheme::MiddlePan:
  case NavigationScheme::Custom:
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
  s.componentColors = flag("componentColors", s.componentColors);
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
  loadNavigationDetails(store, s.navigation);
  s.ghostInactive = flag("ghostInactive", s.ghostInactive);
  s.activePickPriority = flag("activePickPriority", s.activePickPriority);
  const QString cube = store.value(kGroup + QStringLiteral("cubeProjection")).toString();
  for (const CubeProjection projection :
       {CubeProjection::FollowView, CubeProjection::Orthographic, CubeProjection::Perspective}) {
    if (cube == cubeProjectionId(projection)) {
      s.cubeProjection = projection;
    }
  }
  return s;
}

void ViewSettings::save() const {
  QSettings store;
  store.setValue(kGroup + QStringLiteral("style"), int(style));
  store.setValue(kGroup + QStringLiteral("perspective"), perspective);
  store.setValue(kGroup + QStringLiteral("componentColors"), componentColors);
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
  saveNavigationDetails(store, navigation);
  store.setValue(kGroup + QStringLiteral("ghostInactive"), ghostInactive);
  store.setValue(kGroup + QStringLiteral("activePickPriority"), activePickPriority);
  store.setValue(kGroup + QStringLiteral("cubeProjection"), cubeProjectionId(cubeProjection));
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
  // Offered when the render worker is installed next to the application
  // (a package without it has no rendered view).
  if (!render::RenderClient::workerPath().isEmpty()) {
    m_render = new RenderMode(
        m_viewer, [this](QObject* context, std::function<void()> call) { m_host.whenIdle(context, std::move(call)); },
        this);
    connect(m_render, &RenderMode::stopped, this, [this] { syncChecks(); });
  } else {
    qInfo().noquote() << QStringLiteral("No render worker (%1) next to the application: no rendered view")
                             .arg(QString::fromLatin1(render::kWorkerName));
  }
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
    if (m_render != nullptr) {
      m_render->redisplay();
    }
    qDebug().noquote() << QStringLiteral("Background %1 %2").arg(top.name(), bottom.name());
  });
}

void ViewController::registerCommands() {
  const auto add = [this](const char* id, const QString& name, const char* icon, std::function<void()> run,
                          const QStringList& keywords = {}, const QString& tooltip = QString(),
                          const char* ribbonGroup = nullptr) {
    CommandDef def;
    def.id = QString::fromLatin1(id);
    def.name = name;
    def.icon = QString::fromLatin1(icon);
    def.kind = CommandDef::Kind::Action;
    def.mode = CommandDef::Mode::Any;
    def.tab.clear(); // the View menu, the command search and shortcuts
    if (ribbonGroup != nullptr) {
      // Also a button of the SOLID tab's group.
      def.tab = QStringLiteral("SOLID");
      def.group = QString::fromLatin1(ribbonGroup);
      def.pinned = true;
    }
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
  if (m_render != nullptr) {
    add("view.rendered", tr("Rendered"), "visual-style", [this] { setRendered(!m_render->isEnabled()); },
        {QStringLiteral("render"), QStringLiteral("path tracing"), QStringLiteral("cycles"), QStringLiteral("visual style")},
        tr("Shows the visible bodies path traced (a preview of rendering)"));
    // The document's render settings (mitcad#47).
    add("view.render_environment", tr("Render Environment…"), "environment", [this] { showRenderEnvironment(); },
        {QStringLiteral("render"), QStringLiteral("lighting"), QStringLiteral("light"), QStringLiteral("studio"),
         QStringLiteral("hdr"), QStringLiteral("exposure"), QStringLiteral("ground"), QStringLiteral("shadow")},
        tr("The rendered view's light, background, ground and exposure, kept in the design"));
    // The final render to an image file (mitcad#48): a button of the
    // ribbon's MAKE group next to 3D Print (mitcad#175), and View > Visual
    // Style.
    add("file.render_image", tr("Render Image…"), "render-image", [this] { showRenderImage(); },
        {QStringLiteral("render"), QStringLiteral("image"), QStringLiteral("picture"), QStringLiteral("png"),
         QStringLiteral("jpeg"), QStringLiteral("exr"), QStringLiteral("photo"), QStringLiteral("cycles")},
        tr("Renders the view or a named view to an image file at a chosen size and quality"), "MAKE");
  }
  add("view.component_colors", tr("Component Colors"), "visual-style", [this] {
    m_settings.componentColors = !m_settings.componentColors;
    if (m_settings.componentColors) {
      setRendered(false);
    }
    m_viewer.setComponentColors(m_settings.componentColors);
    m_settings.save();
    syncChecks();
    qDebug() << "Component colors" << (m_settings.componentColors ? "on" : "off");
  }, {QStringLiteral("component"), QStringLiteral("color"), QStringLiteral("colour"), QStringLiteral("assembly")},
      tr("Shows each component in a distinct color"));
  // The design printer's bed and height (mitcad#640, view/BuildVolume.cpp).
  add("view.build_volume", tr("Show Build Volume"), "print-3d", [this] { setBuildVolumeShown(!m_buildVolume); },
      {QStringLiteral("printer"), QStringLiteral("bed"), QStringLiteral("print"), QStringLiteral("3d print"),
       QStringLiteral("build plate")},
      tr("Shows the design printer's bed and height; bodies that do not fit are marked"));
  add("view.grid", tr("Layout Grid"), "grid", [this] { setGridShown(!m_settings.gridShown); },
      {QStringLiteral("grid")}, tr("Shows the grid on the ground or sketch plane"));
  add("view.snap_grid", tr("Snap to Grid"), "grid", [this] { setSnapToGrid(!m_settings.snapToGrid); },
      {QStringLiteral("grid"), QStringLiteral("snap")}, tr("Sketch points that snap to nothing else snap to the grid"));
  add("view.grid_settings", tr("Grid Settings…"), "grid", [this] { showGridSettings(); },
      {QStringLiteral("grid"), QStringLiteral("spacing")});
  for (const BackgroundEntry& entry : kBackgrounds) {
    const ViewSettings::Background background = entry.background;
    add(entry.id, QCoreApplication::translate("View", entry.name), "environment",
        [this, background] { setBackground(background); },
        {QStringLiteral("environment"), QStringLiteral("background")});
  }
  // A picture of the view with its measurements (mitcad#649,
  // view/ViewSnapshot.cpp).
  add("view.export_snapshot", tr("Export Snapshot…"), "render-image", [this] { exportSnapshot(); },
      {QStringLiteral("snapshot"), QStringLiteral("screenshot"), QStringLiteral("image"), QStringLiteral("picture"),
       QStringLiteral("png"), QStringLiteral("measurement"), QStringLiteral("dimension")},
      tr("Saves a large picture of the view with the shown annotations"));
  // The Annotations window (mitcad#702, MainWindowAnnotations.cpp).
  add("view.annotations", tr("Annotations…"), "measure", [this] { m_host.openAnnotations(); },
      {QStringLiteral("annotation"), QStringLiteral("dimension"), QStringLiteral("measurement"),
       QStringLiteral("tolerance"), QStringLiteral("note"), QStringLiteral("leader"), QStringLiteral("fit")},
      tr("Lists, filters and edits the dimensions and notes kept in the design"));
  add("view.named_view", tr("New Named View"), "named-view", [this] { saveNamedView(QString(), false); },
      {QStringLiteral("save view"), QStringLiteral("camera")}, tr("Saves the current view in the design"));
  add("view.set_home", tr("Set Current View as Home"), "home",
      [this] { saveNamedView(QStringLiteral("Home"), true); }, {QStringLiteral("home")});
  add("view.reset_home", tr("Reset Home"), "home", [this] {
    m_host.runModelCommand({{QStringLiteral("cmd"), QStringLiteral("delete_named_view")},
                            {QStringLiteral("name"), QStringLiteral("Home")}});
  }, {QStringLiteral("home")});

  CommandDef preferences;
  preferences.id = QStringLiteral("tools.preferences");
#ifdef Q_OS_MACOS
  preferences.name = tr("Settings…"); // as macOS names it since 13
#else
  preferences.name = tr("Preferences…");
#endif
  preferences.icon = QStringLiteral("settings");
  preferences.kind = CommandDef::Kind::Action;
  preferences.mode = CommandDef::Mode::Any;
  preferences.tab.clear();
  preferences.keywords = {QStringLiteral("options"), QStringLiteral("settings")};
  preferences.tooltip = tr("General, navigation, display, cache, cloud, updates and other settings");
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
    page.shortcut = QKeySequence();
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
  if (m_render != nullptr) {
    styles->addSeparator();
    styles->addAction(checkable(QStringLiteral("view.rendered")));
    styles->addAction(action(QStringLiteral("view.render_environment")));
    styles->addAction(action(QStringLiteral("file.render_image")));
  }
  QMenu* environment = view->addMenu(themeIcon(QStringLiteral("environment")), tr("Environment"));
  view->addAction(checkable(QStringLiteral("view.component_colors")));
  view->addAction(checkable(QStringLiteral("view.build_volume")));
  auto* backgrounds = new QActionGroup(this);
  for (const BackgroundEntry& entry : kBackgrounds) {
    environment->addAction(checkable(QString::fromLatin1(entry.id), backgrounds));
  }
  if (m_render != nullptr) {
    environment->addSeparator();
    environment->addAction(action(QStringLiteral("view.render_environment")));
  }
  QMenu* grid = view->addMenu(themeIcon(QStringLiteral("grid")), tr("Grid and Snaps"));
  grid->addAction(checkable(QStringLiteral("view.grid")));
  grid->addAction(checkable(QStringLiteral("view.snap_grid")));
  grid->addAction(action(QStringLiteral("view.grid_settings")));
  QMenu* named = view->addMenu(themeIcon(QStringLiteral("named-view")), tr("Named Views"));
  named->addAction(action(QStringLiteral("view.named_view")));
  named->addAction(action(QStringLiteral("view.set_home")));
  named->addAction(action(QStringLiteral("view.reset_home")));
  view->addAction(action(QStringLiteral("view.annotations")));      // mitcad#702
  view->addAction(action(QStringLiteral("view.export_snapshot"))); // mitcad#649
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
  m_viewer.setComponentColors(m_settings.componentColors);
  m_viewer.setNavigation(m_settings.navigation);
  m_viewer.setPerspective(m_settings.perspective);
  m_viewer.setCubeProjection(m_settings.cubeProjection); // mitcad#774
  QColor top;
  QColor bottom;
  bool gradient = true;
  m_settings.colors(top, bottom, gradient);
  m_viewer.setBackground(top, bottom, gradient);
  m_viewer.setGrid(m_settings.gridShown, m_settings.gridSpacing);
  if (gridSnapChanged) {
    gridSnapChanged(m_settings.snapToGrid, m_settings.gridSpacing);
  }
  m_viewer.setGhostInactive(m_settings.ghostInactive); // mitcad#420
  m_viewer.setActivePickPriority(m_settings.activePickPriority);
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
  check(QStringLiteral("view.component_colors"), m_settings.componentColors);
  check(QStringLiteral("view.grid"), m_settings.gridShown);
  check(QStringLiteral("view.snap_grid"), m_settings.snapToGrid);
  check(QStringLiteral("view.build_volume"), m_buildVolume);
  if (m_render != nullptr) {
    check(QStringLiteral("view.rendered"), m_render->isEnabled());
    if (m_renderEnvironment) {
      m_renderEnvironment->setRenderedShown(m_render->isEnabled());
    }
  }
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
  emit displaySettingsChanged();
  qDebug().noquote() << (perspective ? QStringLiteral("Camera perspective")
                                     : QStringLiteral("Camera orthographic"));
}

void ViewController::setBackground(ViewSettings::Background background) {
  m_settings.background = background;
  m_settings.save();
  emit displaySettingsChanged();
  QColor top;
  QColor bottom;
  bool gradient = true;
  m_settings.colors(top, bottom, gradient);
  m_viewer.setBackground(top, bottom, gradient);
  // A rendered frame over the view's background is shown over the new one.
  if (m_render != nullptr) {
    m_render->redisplay();
  }
  syncChecks();
  qDebug().noquote() << QStringLiteral("Background %1 %2").arg(top.name(), bottom.name());
}

void ViewController::setRendered(bool on) {
  if (on && m_settings.componentColors) {
    m_settings.componentColors = false;
    m_viewer.setComponentColors(false);
    m_settings.save();
  }
  if (m_render != nullptr) {
    m_render->setEnabled(on);
  }
  syncChecks();
}

void ViewController::setRenderSettings(const QJsonObject& settings, const QString& documentFolder) {
  if (m_render == nullptr) {
    return;
  }
  m_render->setSettings(settings, documentFolder);
  if (m_renderEnvironment && m_renderEnvironment->isVisible()) {
    m_renderEnvironment->refresh();
  }
  m_documentFolder = documentFolder;
  if (m_renderImage) {
    m_renderImage->setDocument(m_documentFolder, m_documentName);
    if (m_renderImage->isVisible()) {
      m_renderImage->refresh();
    }
  }
}

void ViewController::showRenderImage() {
  if (m_render == nullptr) {
    return;
  }
  if (!m_renderImage) {
    m_renderImage = new RenderImageDialog(m_host, m_viewer, m_window);
  }
  m_renderImage->setDocument(m_documentFolder, m_documentName);
  showDialog(m_renderImage);
  if (!m_renderImage->testAttribute(Qt::WA_DontShowOnScreen)) {
    m_renderImage->raise();
    m_renderImage->activateWindow();
  }
  m_renderImage->refresh();
  qDebug().noquote() << "Render image dialog opened";
}

void ViewController::showRenderEnvironment() {
  if (m_render == nullptr) {
    return;
  }
  if (!m_renderEnvironment) {
    m_renderEnvironment =
        new RenderEnvironmentDialog(m_host, m_viewer, [this](bool on) { setRendered(on); }, m_window);
  }
  showDialog(m_renderEnvironment);
  if (!m_renderEnvironment->testAttribute(Qt::WA_DontShowOnScreen)) {
    m_renderEnvironment->raise();
    m_renderEnvironment->activateWindow();
  }
  m_renderEnvironment->setRenderedShown(m_render->isEnabled());
  m_renderEnvironment->refresh();
  qDebug().noquote() << "Render environment dialog opened";
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
  const auto addId = [this, &add](const char* id) {
    if (QAction* a = m_registry.action(QString::fromLatin1(id))) {
      add(a);
    }
  };
  addId("view.home");
  // The camera's projection in a submenu of its own, as View > Camera has
  // it: the shared checkable actions show the current choice, and the
  // title keeps it apart from Preferences' "Orientation cube" projection
  // (mitcad#774).
  QMenu* camera = menu.addMenu(themeIcon(QStringLiteral("perspective")), tr("Camera"));
  entries << camera->title();
  QStringList cameraEntries;
  for (const char* id : {"view.orthographic", "view.perspective"}) {
    if (QAction* a = m_registry.action(QString::fromLatin1(id))) {
      camera->addAction(a);
      cameraEntries << a->text().remove(QLatin1Char('&'));
    }
  }
  for (const char* id : {"view.set_home", "view.reset_home", "view.fit"}) {
    addId(id);
  }
  qDebug().noquote() << QStringLiteral("Context menu: %1").arg(entries.join(QStringLiteral(" | ")));
  qDebug().noquote() << QStringLiteral("Context menu Camera: %1").arg(cameraEntries.join(QStringLiteral(" | ")));
  execMenu(&menu, globalPosition);
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
  if (execDialog(&dialog) != QDialog::Accepted) {
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
    showDialog(open);
#ifdef MITCAD_MACOS_NATIVE_UI
    if (!nativeDialogsActive())
#endif
    {
      open->raise();
      open->activateWindow();
    }
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
  folderPath->setToolTip(tr("Never a project's folder. What is here goes when the design is saved or "
                            "MitCad ends normally."));
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
  // The .f3d import's geometric tolerance (mitcad#224).
  auto* importTolerance = new QDoubleSpinBox;
  importTolerance->setObjectName(QStringLiteral("importTolerance"));
  importTolerance->setDecimals(3);
  importTolerance->setRange(GeneralSettings::kMinImportTolerance, GeneralSettings::kMaxImportTolerance);
  importTolerance->setSingleStep(0.001);
  importTolerance->setSuffix(tr(" mm"));
  importTolerance->setValue(general->importTolerance);
  importTolerance->setToolTip(tr("How far replayed geometry of an imported design may be from the file's"));
  generalForm->addRow(tr("Import tolerance:"), importTolerance);
  // Create Sketch and the Construct commands on another component's face
  // ask whether to activate it (mitcad#960); the question's Don't ask
  // again turns this off. Saved when the dialog is accepted, at once in
  // the Settings window.
  auto* askActivate = new QCheckBox(tr("Ask to &activate the component of a picked face"));
  askActivate->setObjectName(QStringLiteral("askActivateComponent"));
  askActivate->setChecked(askToActivateComponent());
  askActivate->setToolTip(tr("Ask before sketching or constructing on another component's geometry"));
  control(generalForm, askActivate);
  const auto applyAskActivate = [askActivate] {
    const bool on = askActivate->isChecked();
    if (on == askToActivateComponent()) {
      return;
    }
    saveAskToActivateComponent(on);
    qDebug().noquote() << QStringLiteral("Preferences: ask to activate the component of a picked face %1")
                              .arg(onOff(on));
  };
  if (window != nullptr) {
    connect(askActivate, &QCheckBox::toggled, host, applyAskActivate);
  } else {
    connect(dialog.get(), &QDialog::accepted, askActivate, applyAskActivate);
  }
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
  // Cloud projects: the git program, the default author, checks and pushes,
  // live updates (P12 remote, mitcad#89).
  auto* remoteBox = new RemotePreferencesBox;
  groups.insert(QStringLiteral("cloud"), remoteBox);
  // Diagnostics (mitcad#269): the debug log, switched on or off at once.
  QWidget* diagnosticsBox = section(tr("Diagnostics"));
  auto* diagnosticsForm = new QFormLayout(diagnosticsBox);
  auto* debugLog = new QCheckBox(tr("&Write a debug log"));
  debugLog->setObjectName(QStringLiteral("debugLog"));
  debugLog->setChecked(DebugLog::instance().running());
  debugLog->setToolTip(tr("Writes what MitCad does to a file in the log folder, so that a problem can be traced"));
  control(diagnosticsForm, debugLog);
  auto* logFolderPath = new QLineEdit(QDir::toNativeSeparators(DebugLog::instance().folder()));
  logFolderPath->setReadOnly(true);
  if (window != nullptr) {
    logFolderPath->setMinimumWidth(240);
  }
  auto* openLogFolder = new QPushButton(tr("Open Log Folder"));
  connect(openLogFolder, &QPushButton::clicked, host, [this] {
    if (QAction* open = m_registry.action(QStringLiteral("help.open_log_folder"))) {
      open->trigger();
    }
  });
  auto* logFolderRow = new QHBoxLayout;
  logFolderRow->addWidget(logFolderPath, 1);
  logFolderRow->addWidget(openLogFolder);
  diagnosticsForm->addRow(tr("Log folder:"), logFolderRow);
  auto* logPrivacy = new QLabel(tr("Logs stay on this computer: a problem report offers the latest one, attached only "
                                   "if you choose. A log may contain file paths, design names and entity names; "
                                   "not what you type."));
  logPrivacy->setWordWrap(true);
  control(diagnosticsForm, logPrivacy);
  groups.insert(QStringLiteral("diagnostics"), diagnosticsBox);
  // What the switch was when the window opened or last applied: a change
  // starts or stops the log and is saved; the log of a run started with
  // --debug-log stays a run's unless the switch is changed.
  const auto debugLogApplied = std::make_shared<bool>(debugLog->isChecked());
  // Texture folders for Edit Appearances (mitcad#315).
  auto* texturesBox = new TexturePreferencesBox;
  groups.insert(QStringLiteral("textures"), texturesBox);
  // Folders for MCP agents (mitcad#402).
  auto* agentsBox = new mcp::AgentPreferencesBox;
  connect(agentsBox, &mcp::AgentPreferencesBox::connectRequested, host, [this] {
    if (auto* action = m_registry.action(QStringLiteral("help.connect_assistant"))) action->trigger();
  });
  // The chat panel's settings below them (mitcad#947).
  auto* chatBox = new agent::ChatPreferencesBox;
  auto* agentsPage = new QWidget;
  auto* agentsColumn = new QVBoxLayout(agentsPage);
  agentsColumn->setContentsMargins(0, 0, 0, 0);
  agentsColumn->addWidget(agentsBox, 1);
  agentsColumn->addWidget(chatBox);
  groups.insert(QStringLiteral("agents"), agentsPage);
  // Font folders for sketch text (mitcad#473).
  auto* fontsBox = new sketch::FontPreferencesBox;
  groups.insert(QStringLiteral("fonts"), fontsBox);
  // Sketch (mitcad#860): the first dimension of a sketch drawn out of
  // proportion scales the whole sketch to it. Saved when the dialog is
  // accepted, at once in the Settings window.
  QWidget* sketchBox = section(tr("Sketch"));
  auto* sketchForm = new QFormLayout(sketchBox);
  auto* firstDimensionScale = new QCheckBox(tr("&Scale the sketch to its first dimension"));
  firstDimensionScale->setObjectName(QStringLiteral("scaleToFirstDimension"));
  firstDimensionScale->setChecked(sketch::scaleToFirstDimension());
  firstDimensionScale->setToolTip(
      tr("A sketch's first length dimension scales the whole sketch so that it holds"));
  control(sketchForm, firstDimensionScale);
  const auto applyScaleToFirstDimension = [firstDimensionScale] {
    const bool on = firstDimensionScale->isChecked();
    if (on == sketch::scaleToFirstDimension()) {
      return;
    }
    sketch::saveScaleToFirstDimension(on);
    qDebug().noquote() << QStringLiteral("Preferences: scale the sketch to its first dimension %1").arg(onOff(on));
  };
  if (window != nullptr) {
    connect(firstDimensionScale, &QCheckBox::toggled, host, applyScaleToFirstDimension);
  } else {
    connect(dialog.get(), &QDialog::accepted, firstDimensionScale, applyScaleToFirstDimension);
  }
  groups.insert(QStringLiteral("sketch"), sketchBox);

  QWidget* navigationBox = section(tr("Navigation"));
  auto* navigationLayout = new QVBoxLayout(navigationBox);
  navigationLayout->setContentsMargins(0, 0, 0, 0);
  QVector<QPair<NavigationScheme, QString>> navigationSchemes;
  for (const SchemeEntry& entry : kSchemes) {
    navigationSchemes.append({entry.scheme, schemeName(entry.scheme)});
  }
  // The view commands a 3D mouse's button can run (mitcad#951).
  QVector<QPair<QString, QString>> buttonCommands;
  const auto offer = [this, &buttonCommands](const QString& id) {
    if (const CommandDef* def = m_registry.find(id)) {
      buttonCommands.append({def->id, def->name});
    }
  };
  offer(QStringLiteral("view.fit"));
  offer(QStringLiteral("view.home"));
  for (const StandardView& view : kStandardViews) {
    offer(QStringLiteral("view.%1").arg(QString::fromLatin1(view.key)));
  }
  offer(QStringLiteral("view.look_at"));
  offer(QStringLiteral("view.orthographic"));
  offer(QStringLiteral("view.perspective"));
  auto* navigation = new NavigationPreferences(m_settings.navigation, navigationSchemes, nullptr, buttonCommands);
  navigationLayout->addWidget(navigation);
  navigation->changed = [this, navigation, window] {
    const NavigationSettings chosen = navigation->chosen();
    m_viewer.setNavigation(chosen);
    if (window != nullptr) {
      m_settings.navigation = chosen;
      m_settings.save();
    }
    qDebug().noquote() << QStringLiteral("Preferences: navigation %1, orbit speed %2, pan speed %3, "
                                         "zoom speed %4, trackpad speed %5, 3D mouse %6 speed %7, %8 mode, "
                                         "horizon %9")
                              .arg(QLatin1String(schemeEntry(chosen.scheme).id))
                              .arg(chosen.orbitSpeed)
                              .arg(chosen.panSpeed)
                              .arg(chosen.zoomSpeed)
                              .arg(chosen.trackpadSpeed)
                              .arg(onOff(chosen.spaceMouse.enabled))
                              .arg(chosen.spaceMouse.speed)
                              .arg(QLatin1String(spaceMouseModeId(chosen.spaceMouse.mode)))
                              .arg(chosen.spaceMouse.lockHorizon ? QStringLiteral("locked") : QStringLiteral("free"));
  };
  groups.insert(QStringLiteral("navigation"), navigationBox);

  QWidget* displayBox = section(tr("Display"));
  auto* display = new QFormLayout(displayBox);
  auto* camera = new QComboBox;
  camera->setObjectName(QStringLiteral("preferencesCamera"));
  camera->addItems({tr("Orthographic"), tr("Perspective")});
  camera->setCurrentIndex(m_settings.perspective ? 1 : 0);
  display->addRow(tr("C&amera:"), camera);
  // The orientation cube's projection (mitcad#774), in CubeProjection's
  // order.
  auto* cubeProjection = new QComboBox;
  cubeProjection->setObjectName(QStringLiteral("preferencesCubeProjection"));
  cubeProjection->addItems({tr("Follow the View"), tr("Orthographic"), tr("Perspective")});
  cubeProjection->setCurrentIndex(int(m_settings.cubeProjection));
  display->addRow(tr("&Orientation cube:"), cubeProjection);
  auto* background = new QComboBox;
  background->setObjectName(QStringLiteral("preferencesBackground"));
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
      const QColor chosen = dialogColor(*color, host, title);
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
  if (window != nullptr) {
    connect(this, &ViewController::displaySettingsChanged, host,
            [this, camera, background, top, bottom, topButton, bottomButton] {
      const QSignalBlocker cameraSignals(camera);
      const QSignalBlocker backgroundSignals(background);
      camera->setCurrentIndex(m_settings.perspective ? 1 : 0);
      background->setCurrentIndex(int(m_settings.background));
      *top = m_settings.top;
      *bottom = m_settings.bottom;
      topButton->setIcon(swatchIcon(*top));
      topButton->setText(top->name());
      bottomButton->setIcon(swatchIcon(*bottom));
      bottomButton->setText(bottom->name());
      const bool custom = m_settings.background == ViewSettings::Background::Custom;
      topButton->setEnabled(custom);
      bottomButton->setEnabled(custom);
    });
  }
  display->addRow(tr("Custom top:"), topButton);
  display->addRow(tr("Custom bottom:"), bottomButton);
  const auto enableCustom = [background, topButton, bottomButton] {
    const bool custom = background->currentIndex() == int(ViewSettings::Background::Custom);
    topButton->setEnabled(custom);
    bottomButton->setEnabled(custom);
  };
  enableCustom();
  connect(background, &QComboBox::currentIndexChanged, host, enableCustom);
  // The render device (mitcad#50): a setting of this machine, not of the
  // design. The worker lists its devices; GPUs come after the automatic
  // choice and the CPU.
  QComboBox* renderDevice = nullptr;
  if (m_render != nullptr) {
    renderDevice = new QComboBox;
    renderDevice->setObjectName(QStringLiteral("renderDevice"));
    // The devices' names come from the system: a long one neither widens
    // Preferences nor sets its minimum width (mitcad#690).
    renderDevice->setSizeAdjustPolicy(QComboBox::AdjustToMinimumContentsLengthWithIcon);
    renderDevice->setMinimumContentsLength(32);
    renderDevice->addItem(tr("Automatic (the best GPU, else the CPU)"), QStringLiteral("auto"));
    QString error;
    const QList<render::RenderDevice> devices = render::renderDevices(render::RenderClient::workerPath(), error);
    for (const render::RenderDevice& device : devices) {
      renderDevice->addItem(device.type == QLatin1String("CPU") ? tr("CPU: %1").arg(device.label()) : device.label(),
                            device.type == QLatin1String("CPU") ? QStringLiteral("cpu") : device.id);
      renderDevice->setItemData(renderDevice->count() - 1,
                                device.denoiserAvailable ? tr("Supports rendering and denoising on this device")
                                                         : tr("Renders on this device; uses CPU denoising"),
                                Qt::ToolTipRole);
    }
    if (devices.isEmpty()) {
      renderDevice->addItem(tr("CPU"), QStringLiteral("cpu"));
      qWarning().noquote() << QStringLiteral("Render devices not listed: %1").arg(error);
    }
    const QString chosen = render::renderDeviceChoice();
    if (renderDevice->findData(chosen) < 0) {
      // A device of an earlier run that is not there now: kept until another
      // is chosen (the worker renders on the CPU meanwhile).
      renderDevice->addItem(tr("Not found: %1").arg(chosen), chosen);
    }
    renderDevice->setCurrentIndex(renderDevice->findData(chosen));
    renderDevice->setToolTip(tr("Where the rendered view and Render Image render; the CPU takes over when a "
                                "GPU fails"));
    display->addRow(tr("Render device:"), renderDevice);
    QStringList labels;
    for (int i = 0; i < renderDevice->count(); ++i) {
      labels << renderDevice->itemData(i).toString();
    }
    qDebug().noquote() << QStringLiteral("Preferences: render devices %1").arg(labels.join(QStringLiteral(", ")));
  }
  // The window's layout (mitcad#419), for the next starts: Docked or
  // Floating, the platform's until one is chosen. Not in the native macOS
  // frontend, which has its own.
  QComboBox* layoutChoice = nullptr;
  if (layoutChoosable()) {
    layoutChoice = new QComboBox;
    layoutChoice->setObjectName(QStringLiteral("layout"));
    layoutChoice->addItem(tr("Docked"), int(ChromeStyle::Docked));
    layoutChoice->addItem(tr("Floating"), int(ChromeStyle::Floating));
    layoutChoice->setCurrentIndex(layoutChoice->findData(int(layoutPreference().value_or(defaultChromeStyle()))));
    layoutChoice->setToolTip(tr("Docked: panels in docks around the 3D view; Floating: cards over it"));
    display->addRow(tr("&Layout:"), layoutChoice);
  }
  // The layout chosen when the window opened or last applied.
  const auto layoutApplied = std::make_shared<int>(layoutChoice != nullptr ? layoutChoice->currentData().toInt() : -1);
  // The Browser's background in the Floating layout (mitcad#419): its card,
  // or a box of its own behind each row; it applies at once. Also in the
  // Docked layout (mitcad#429), for the Floating layout's next start, so that
  // the layout and the background are chosen in one visit.
  QComboBox* browserBackground = nullptr;
  if (chromeStyle() == ChromeStyle::Floating || layoutChoosable()) {
    browserBackground = new QComboBox;
    browserBackground->setObjectName(QStringLiteral("browserBackground"));
    browserBackground->addItem(tr("Card"), browserBackgroundId(BrowserBackground::Card));
    browserBackground->addItem(tr("Rows"), browserBackgroundId(BrowserBackground::Rows));
    browserBackground->setCurrentIndex(
        browserBackground->findData(browserBackgroundId(savedBrowserBackground())));
    browserBackground->setToolTip(tr("Card: the Browser on a card; Rows: each row on a small box of its own"));
    display->addRow(tr("&Browser background:"), browserBackground);
    if (chromeStyle() != ChromeStyle::Floating) {
      auto* note = new QLabel(tr("Applies in the Floating layout."));
      note->setObjectName(QStringLiteral("browserBackgroundNote"));
      note->setEnabled(false); // the quiet colour of a hint
      display->addRow(QString(), note);
    }
    if (window == nullptr) {
      // Where it is once the dialog shows (UI tests).
      QTimer::singleShot(0, browserBackground, [this, browserBackground] {
        if (browserBackground->isVisible()) {
          const QPoint at = browserBackground->mapToGlobal(browserBackground->rect().center()) -
                            m_window->mapToGlobal(QPoint(0, 0));
          qDebug().noquote() << QStringLiteral("Preferences browser background at %1,%2").arg(at.x()).arg(at.y());
        }
      });
    }
  }
  // Everything outside the active component as ghosts (mitcad#420): saved
  // and applied when the dialog is accepted, at once in the Settings window.
  auto* ghostInactive = new QCheckBox(tr("&Ghost inactive components"));
  ghostInactive->setObjectName(QStringLiteral("ghostInactive"));
  ghostInactive->setChecked(m_settings.ghostInactive);
  ghostInactive->setToolTip(tr("Draws everything outside the active component faded"));
  control(display, ghostInactive);
  const auto applyGhostInactive = [this, ghostInactive] {
    if (ghostInactive->isChecked() == m_settings.ghostInactive) {
      return;
    }
    m_settings.ghostInactive = ghostInactive->isChecked();
    m_settings.save();
    m_viewer.setGhostInactive(m_settings.ghostInactive);
    qDebug().noquote()
        << QStringLiteral("Preferences: ghost inactive components %1").arg(onOff(m_settings.ghostInactive));
  };
  // Picks take the active component's items first (mitcad#420).
  auto* activePickPriority = new QCheckBox(tr("&Pick the active component first"));
  activePickPriority->setObjectName(QStringLiteral("activePickPriority"));
  activePickPriority->setChecked(m_settings.activePickPriority);
  activePickPriority->setToolTip(
      tr("A click takes the active component's items before nearer ones of other components"));
  control(display, activePickPriority);
  const auto applyActivePickPriority = [this, activePickPriority] {
    if (activePickPriority->isChecked() == m_settings.activePickPriority) {
      return;
    }
    m_settings.activePickPriority = activePickPriority->isChecked();
    m_settings.save();
    m_viewer.setActivePickPriority(m_settings.activePickPriority);
    qDebug().noquote()
        << QStringLiteral("Preferences: active component first in picks %1").arg(onOff(m_settings.activePickPriority));
  };
  if (window != nullptr) {
    connect(ghostInactive, &QCheckBox::toggled, host, applyGhostInactive);
    connect(activePickPriority, &QCheckBox::toggled, host, applyActivePickPriority);
  } else {
    connect(dialog.get(), &QDialog::accepted, ghostInactive, applyGhostInactive);
    connect(dialog.get(), &QDialog::accepted, activePickPriority, applyActivePickPriority);
  }
  groups.insert(QStringLiteral("display"), displayBox);

  *commit = [this, general, top, bottom, navigation, camera, cubeProjection, background,
             autosave, minutes, importTolerance, cacheBox, updatesBox, slicerBox, remoteBox, renderDevice, debugLog,
             debugLogApplied, texturesBox, agentsBox, chatBox, fontsBox, layoutChoice, layoutApplied] {
    // Font folders for sketch text (mitcad#473).
    const QStringList fontFolders = fontsBox->chosen();
    if (fontFolders != sketch::savedFontFolders()) {
      sketch::saveFontFolders(fontFolders);
      qDebug().noquote() << QStringLiteral("Preferences: font folders %1")
                                .arg(fontFolders.isEmpty() ? QStringLiteral("none")
                                                           : fontFolders.join(QStringLiteral(", ")));
    }
    // Folders for designs (mitcad#402).
    // The chat (mitcad#947).
    const agent::ChatSettings chat = chatBox->chosen();
    const agent::ChatSettings chatBefore = agent::ChatSettings::load();
    if (chat != chatBefore) {
      chat.save();
      qDebug().noquote() << QStringLiteral("Preferences: chat agent %1, approvals %2, history %3")
                                .arg(chat.agent, agent::approvalModeKey(chat.approvals), onOff(chat.history));
      if (chat.agent != chatBefore.agent) {
        emit agent::ChatNotifier::instance().agentChosen(chat.agent);
      }
    }
    const QStringList agentFolders = agentsBox->chosen();
    if (agentFolders != mcp::savedAgentFolders()) {
      mcp::saveAgentFolders(agentFolders);
      qDebug().noquote() << QStringLiteral("Preferences: agent folders %1")
                                .arg(agentFolders.isEmpty() ? QStringLiteral("none")
                                                            : agentFolders.join(QStringLiteral(", ")));
    }
    // Texture folders (mitcad#315).
    const TextureSettings textures = texturesBox->chosen();
    if (textures != TextureSettings::load()) {
      textures.save();
      qDebug().noquote() << QStringLiteral("Preferences: texture folders %1")
                                .arg(textures.folders.isEmpty() ? QStringLiteral("none")
                                                                : textures.folders.join(QStringLiteral(", ")));
    }
    // The debug log (mitcad#269), at once.
    if (debugLog->isChecked() != *debugLogApplied) {
      *debugLogApplied = debugLog->isChecked();
      DebugLog::setSettingOn(*debugLogApplied);
      qDebug().noquote() << QStringLiteral("Preferences: debug log %1").arg(onOff(*debugLogApplied));
      if (!*debugLogApplied) {
        DebugLog::instance().stop();
      } else if (!DebugLog::instance().running()) {
        QString error;
        if (!DebugLog::instance().start(QString(), QStringLiteral("Preferences"), &error)) {
          qWarning().noquote() << QStringLiteral("Debug log not started: %1").arg(error);
        }
      }
    }
    m_settings.navigation = navigation->chosen();
    m_settings.perspective = camera->currentIndex() == 1;
    // The orientation cube's projection (mitcad#774): applied below.
    const auto cube = static_cast<CubeProjection>(cubeProjection->currentIndex());
    if (cube != m_settings.cubeProjection) {
      m_settings.cubeProjection = cube;
      qDebug().noquote() << QStringLiteral("Preferences: orientation cube %1").arg(cubeProjectionId(cube));
    }
    m_settings.background = static_cast<ViewSettings::Background>(background->currentIndex());
    m_settings.top = *top;
    m_settings.bottom = *bottom;
    m_settings.save();
    GeneralSettings chosen = *general;
    chosen.autosave = autosave->isChecked();
    chosen.autosaveMinutes = minutes->value();
    chosen.importTolerance = importTolerance->value();
    chosen.save();
    const VersionSettings author = remoteBox->chosenAuthor();
    author.save();
    qDebug().noquote() << QStringLiteral("Preferences: versions by git's user, else %1")
                              .arg(author.complete() ? author.author() : QStringLiteral("nobody"));
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
    qDebug().noquote() << QStringLiteral("Preferences: git %1, remote checks %2, each version sent %3, live updates "
                                         "%4, default broker %5")
                              .arg(remote.git.isEmpty() ? QStringLiteral("found automatically") : remote.git,
                                   remote.checkMinutes > 0 ? QStringLiteral("every %1 min").arg(remote.checkMinutes)
                                                           : QStringLiteral("never"),
                                   onOff(remote.autoPush), onOff(remote.allowLive),
                                   remote.defaultBroker.isEmpty() ? QStringLiteral("none") : remote.defaultBroker);
    if (renderDevice != nullptr && renderDevice->currentData().toString() != render::renderDeviceChoice()) {
      render::setRenderDeviceChoice(renderDevice->currentData().toString());
      qDebug().noquote() << QStringLiteral("Preferences: render device %1").arg(render::renderDeviceChoice());
      if (m_render != nullptr) {
        m_render->deviceChanged();
      }
    }
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
    // The window's layout (mitcad#419): saved for the next starts; one other
    // than the window's asks whether to restart now, once the dialog is gone.
    if (layoutChoice != nullptr && layoutChoice->currentData().toInt() != *layoutApplied) {
      *layoutApplied = layoutChoice->currentData().toInt();
      const ChromeStyle chosenLayout = static_cast<ChromeStyle>(*layoutApplied);
      setLayoutPreference(chosenLayout);
      const bool restart = chosenLayout != chromeStyle();
      qDebug().noquote() << QStringLiteral("Layout preference: %1 (%2)")
                                .arg(chromeStyleName(chosenLayout),
                                     restart ? QStringLiteral("restart asked") : QStringLiteral("in use"));
      if (restart) {
        QTimer::singleShot(0, this, [this, chosenLayout] { askLayoutRestart(chosenLayout); });
      }
    }
  };
  if (browserBackground != nullptr) {
    // The Browser's background (mitcad#419), with the rest: saved and
    // applied at once when it changed.
    *commit = [rest = *commit, browserBackground] {
      rest();
      const BrowserBackground chosen = browserBackground->currentData().toString() == QLatin1String("rows")
                                           ? BrowserBackground::Rows
                                           : BrowserBackground::Card;
      if (chosen != savedBrowserBackground()) {
        qDebug().noquote() << QStringLiteral("Preferences: browser background %1").arg(browserBackgroundId(chosen));
        saveBrowserBackground(chosen);
      }
    };
    connect(browserBackground, &QComboBox::currentIndexChanged, host, changed);
  }

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
        {"cloud", "cloud", ""},
        {"updates", "arrow.down.circle", ""},
        {"print", "printer", ""},
        {"diagnostics", "ladybug", "wrench"},
        {"textures", "photo.on.rectangle", "photo"},
        {"agents", "person.badge.key", "person.crop.circle"},
        {"fonts", "textformat", "textformat.alt"},
        {"sketch", "pencil.and.ruler", "pencil"},
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
      const bool fills = pageFills(entry.id);
      contentLayout->addWidget(group, fills ? 1 : 0);
      if (!fills) contentLayout->addStretch(1);
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
    connect(camera, &QComboBox::currentIndexChanged, host, changed);
    connect(cubeProjection, &QComboBox::currentIndexChanged, host, changed);
    connect(background, &QComboBox::currentIndexChanged, host, changed);
    if (renderDevice != nullptr) {
      connect(renderDevice, &QComboBox::currentIndexChanged, host, changed);
    }
    if (layoutChoice != nullptr) {
      connect(layoutChoice, &QComboBox::currentIndexChanged, host, changed);
    }
    for (QCheckBox* box : {autosave, debugLog}) {
      connect(box, &QCheckBox::toggled, host, changed);
    }
    connect(minutes, &QSpinBox::valueChanged, host, changed);
    connect(importTolerance, &QDoubleSpinBox::valueChanged, host, changed);
    connect(cacheBox, &CachePreferencesBox::changed, host, changed);
    connect(texturesBox, &TexturePreferencesBox::changed, host, changed);
    connect(agentsBox, &mcp::AgentPreferencesBox::changed, host, changed);
    connect(chatBox, &agent::ChatPreferencesBox::changed, host, changed);
    connect(fontsBox, &sketch::FontPreferencesBox::changed, host, changed);
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
    window->setClosedHandler([window] {
      // Flush only an unfinished editor; closing an unchanged window commits nothing.
      if (QWidget* editor = window->focusWidget()) editor->clearFocus();
    });
    window->setWindowTitle(tr("Settings"));
    window->showPane(wantedIndex);
    showDialog(window);
    return;
  }

  auto* layout = new QVBoxLayout(dialog.get());
  auto* pages = new QListWidget;
  auto* stack = new QStackedWidget;
  for (const PreferencePage& entry : kPreferencePages) {
    auto* content = new QWidget;
    auto* contentLayout = new QVBoxLayout(content);
    contentLayout->setContentsMargins(0, 0, 0, 0);
    const bool fills = pageFills(entry.id);
    contentLayout->addWidget(groups.value(QLatin1String(entry.id)), fills ? 1 : 0);
    if (!fills) contentLayout->addStretch(1);
    stack->addWidget(content);
    auto* item = new QListWidgetItem(QCoreApplication::translate("View", entry.name), pages);
    item->setData(Qt::UserRole, QLatin1String(entry.id));
  }
  pages->setSpacing(2);
  // Only the page shown counts for the window's minimum size (mitcad#690):
  // the hidden ones are Ignored, as in the Settings window, so that no page
  // makes the window wider or taller than the one shown needs.
  const auto showPage = [layout, stack](int row) {
    if (row < 0) {
      return;
    }
    for (int i = 0; i < stack->count(); ++i) {
      const QSizePolicy::Policy policy = i == row ? QSizePolicy::Preferred : QSizePolicy::Ignored;
      stack->widget(i)->setSizePolicy(policy, policy);
    }
    stack->setCurrentIndex(row);
    stack->updateGeometry();
    layout->invalidate();
    layout->activate();
  };
  connect(pages, &QListWidget::currentRowChanged, stack, showPage);
  connect(pages, &QListWidget::currentRowChanged, dialog.get(), [this, pages](int row) {
    if (row >= 0) {
      // Preferences opens there again (until MitCad ends).
      m_preferencesPage = pages->item(row)->data(Qt::UserRole).toString();
      // The minimum size the page shown sets (UI tests).
      const QSize minimum = pages->window()->minimumSize();
      qDebug().noquote() << QStringLiteral("Preferences page: %1, minimum %2 x %3")
                                .arg(pages->item(row)->text())
                                .arg(minimum.width())
                                .arg(minimum.height());
    }
  });
  auto* pagesRow = new QHBoxLayout;
  pagesRow->addWidget(pages);
  pagesRow->addWidget(stack, 1);
  layout->addLayout(pagesRow, 1);

  auto* buttons = new QDialogButtonBox(QDialogButtonBox::Ok | QDialogButtonBox::Cancel);
  connect(buttons, &QDialogButtonBox::accepted, dialog.get(), &QDialog::accept);
  connect(buttons, &QDialogButtonBox::rejected, dialog.get(), &QDialog::reject);
  layout->addWidget(buttons);
  // The page list as wide as its entries in the style and fonts the dialog
  // shows them in (mitcad#690: measured before the dialog was polished, the
  // list could clip them), with room for a scroll bar on a low screen; it
  // keeps that width when the window is made narrower.
  dialog->ensurePolished();
  const int entriesWidth = pages->sizeHintForColumn(0) + 2 * pages->spacing();
  pages->setFixedWidth(entriesWidth + 2 * pages->frameWidth() +
                       pages->style()->pixelMetric(QStyle::PM_ScrollBarExtent, nullptr, pages) +
                       pages->fontMetrics().averageCharWidth());
  pages->setHorizontalScrollBarPolicy(Qt::ScrollBarAlwaysOff);
  // The size the window opens at: the largest page's, so that it keeps its
  // size between pages (UI tests check that it fits a 1366 x 768 screen).
  const QSize size = dialog->sizeHint();
  pages->setCurrentRow(wantedIndex);
  showPage(wantedIndex);
  pages->setFocus();
  qDebug().noquote() << QStringLiteral("Preferences opened: %1, %2 x %3")
                            .arg(pages->currentItem()->text())
                            .arg(size.width())
                            .arg(size.height());
  // Never larger than the screen's available area (mitcad#690), and still
  // resizable: only the page shown sets the minimum size.
  QDialog* shown = dialog.get();
  const QScreen* screen = m_window != nullptr ? m_window->screen() : shown->screen();
  if (screen != nullptr) {
    shown->resize(size.boundedTo(screen->availableGeometry().size()).expandedTo(shown->minimumSizeHint()));
  }
  // Once shown, with its frame: the frame within the available area, moved
  // there if the window system placed it partly off the screen; where it is
  // and how small it can be made, for UI tests.
  QTimer::singleShot(0, shown, [shown, pages, entriesWidth] {
    const QScreen* on = shown->screen();
    if (on == nullptr) {
      return;
    }
    const QRect available = on->availableGeometry();
    const QSize margins = shown->frameGeometry().size() - shown->size();
    const QSize fitted = shown->size().boundedTo(available.size() - margins).expandedTo(shown->minimumSize());
    if (fitted != shown->size()) {
      shown->resize(fitted);
    }
    const QSize frame = fitted + margins;
    const QPoint from = shown->frameGeometry().topLeft();
    // The frame's furthest place right and down that still shows it whole.
    const int lastX = std::max(available.left(), available.right() + 1 - frame.width());
    const int lastY = std::max(available.top(), available.bottom() + 1 - frame.height());
    const QPoint at(std::clamp(from.x(), available.left(), lastX), std::clamp(from.y(), available.top(), lastY));
    if (at != from) {
      shown->move(at);
    }
    const QSize minimum = shown->minimumSize();
    const QSize maximum = shown->maximumSize();
    qDebug().noquote() << QStringLiteral("Preferences frame %1 x %2 at %3,%4 on a screen of %5 x %6 at %7,%8")
                              .arg(frame.width())
                              .arg(frame.height())
                              .arg(at.x())
                              .arg(at.y())
                              .arg(available.width())
                              .arg(available.height())
                              .arg(available.x())
                              .arg(available.y());
    qDebug().noquote() << QStringLiteral("Preferences minimum %1 x %2, %3")
                              .arg(minimum.width())
                              .arg(minimum.height())
                              .arg(maximum.width() > minimum.width() && maximum.height() > minimum.height()
                                       ? QStringLiteral("resizable")
                                       : QStringLiteral("fixed"));
    qDebug().noquote() << QStringLiteral("Preferences page list %1 wide, its entries %2")
                              .arg(pages->viewport()->width())
                              .arg(entriesWidth);
  });
  if (renderDevice != nullptr) {
    // Where the render device's list is once the dialog shows (UI tests).
    QTimer::singleShot(0, renderDevice, [this, renderDevice] {
      if (renderDevice->isVisible()) {
        const QPoint at =
            renderDevice->mapToGlobal(renderDevice->rect().center()) - m_window->mapToGlobal(QPoint(0, 0));
        qDebug().noquote() << QStringLiteral("Preferences render device at %1,%2").arg(at.x()).arg(at.y());
      }
    });
  }
  if (execDialog(dialog.get()) != QDialog::Accepted) {
    m_viewer.setNavigation(m_settings.navigation);
    return;
  }
  (*commit)();
}

void ViewController::askLayoutRestart(ChromeStyle chosen) {
  const QString name = chosen == ChromeStyle::Floating ? tr("Floating") : tr("Docked");
  QMessageBox box(QMessageBox::Question, tr("Restart MitCad"),
                  tr("The %1 layout takes effect when MitCad starts again. Restart now?").arg(name),
                  QMessageBox::Yes | QMessageBox::No, m_window);
  box.setInformativeText(tr("A design with unsaved changes is saved or asked about first, as when MitCad quits."));
  box.button(QMessageBox::Yes)->setText(tr("&Restart Now"));
  box.button(QMessageBox::No)->setText(tr("&Later"));
  box.setDefaultButton(QMessageBox::Yes);
  box.setEscapeButton(QMessageBox::No);
  prepareModal(&box);
  const bool now = execDialog(&box) == QMessageBox::Yes;
  qDebug().noquote() << QStringLiteral("Layout restart: %1")
                            .arg(now ? QStringLiteral("now")
                                     : QStringLiteral("later (%1 at the next start)").arg(chromeStyleName(chosen)));
  if (now && restartRequested) {
    restartRequested();
  }
}

void ViewController::showShortcutOverview() {
  QDialog dialog(m_window);
  dialog.setWindowTitle(tr("Keyboard and Mouse Overview"));
  auto* layout = new QVBoxLayout(&dialog);
  auto* filter = new QLineEdit;
  filter->setObjectName(QStringLiteral("shortcutOverviewFilter"));
  filter->setPlaceholderText(tr("Search"));
  filter->setClearButtonEnabled(true);
  mac::styleSearchField(filter);
  layout->addWidget(filter);
  auto* tree = new QTreeWidget;
  tree->setObjectName(QStringLiteral("shortcutOverviewTable"));
  tree->setColumnCount(2);
  tree->setHeaderLabels({tr("Keys or mouse"), tr("Does")});
  // Group captions span both columns, so they do not steal the action
  // column's width. Both columns keep their content widths; a small window
  // scrolls horizontally instead of reducing actions to a few letters.
  tree->header()->setStretchLastSection(false);
  tree->header()->setSectionResizeMode(QHeaderView::ResizeToContents);
  tree->setRootIsDecorated(true);
  layout->addWidget(tree);

  int count = 0;
  const auto section = [tree](const QString& title) {
    auto* item = new QTreeWidgetItem(tree, {title});
    QFont bold = item->font(0);
    bold.setBold(true);
    item->setFont(0, bold);
    item->setFirstColumnSpanned(true);
    item->setExpanded(true);
    return item;
  };
  const auto addRow = [](QTreeWidgetItem* parent, const QString& keys, const QString& what, const QString& id) {
    auto* row = new QTreeWidgetItem(parent, {keys, what});
    for (int column = 0; column < 2; ++column) {
      row->setData(column, NativeControlIdentifierRole,
                   QStringLiteral("shortcutOverview.%1.%2").arg(id).arg(column));
    }
    return row;
  };
  QTreeWidgetItem* mouse = section(tr("Mouse (%1)").arg(schemeName(m_settings.navigation.scheme)));
  int mouseRow = 0;
  for (const auto& [keys, what] : mouseRows(m_settings.navigation)) {
    addRow(mouse, keys, what, QStringLiteral("mouse.%1").arg(mouseRow++));
  }
  for (const auto& [keys, what] :
       QVector<QPair<QString, QString>>{{tr("Left click"), tr("Select; %1 or Shift adds or removes").arg(ctrlKey())},
                                        {tr("Left drag to the right"), tr("Select what lies inside")},
                                        {tr("Left drag to the left"), tr("Select what the window crosses")},
                                        {tr("Right click"), tr("The commands that fit the selection")},
                                        {tr("Trackpad: pinch"), tr("Zoom (in every scheme)")},
                                        {tr("Trackpad: two-finger rotation"), tr("Roll the view (in every scheme)")},
                                        {tr("Trackpad: two-finger double tap"), tr("Fit (in every scheme)")},
                                        {tr("Orientation cube: click"), tr("Turn to a face, edge or corner")},
                                        {tr("Orientation cube: drag"), tr("Orbit")}}) {
    addRow(mouse, keys, what, QStringLiteral("mouse.%1").arg(mouseRow++));
  }
  QTreeWidgetItem* keys = section(tr("Keys"));
  // Ctrl is Cmd on macOS, where Backspace (the delete key) deletes too.
  const QString fileKeys =
      QStringList{QKeySequence(QKeySequence::Open).toString(QKeySequence::NativeText),
                  QKeySequence(QKeySequence::Save).toString(QKeySequence::NativeText),
                  QKeySequence(Qt::CTRL | Qt::SHIFT | Qt::Key_S).toString(QKeySequence::NativeText)}
          .join(QStringLiteral(", "));
#ifdef Q_OS_MACOS
  const QString deleteKeys = tr("Delete, Backspace");
#else
  const QString deleteKeys = tr("Delete");
#endif
  int keyRow = 0;
  for (const auto& [key, what] :
       QVector<QPair<QString, QString>>{{tr("Enter"), tr("OK: finish the command, or the typed value")},
                                        {tr("Esc"), tr("Cancel the command, step back in a sketch tool, clear the selection")},
                                        {tr("Tab"), tr("The next value box of a sketch tool")},
                                        {deleteKeys, tr("Delete the selection")},
                                        {tr("F2"), tr("Rename in the Browser or history; Return activates")},
                                        {fileKeys, tr("Open, Save, Save As")}}) {
    addRow(keys, key, what, QStringLiteral("key.%1").arg(keyRow++));
  }
  // The commands with shortcuts, by tab and group.
  QHash<QString, QTreeWidgetItem*> groups;
  for (const auto& def : m_registry.commands()) {
    const QList<QKeySequence> sequences = m_registry.effectiveShortcuts(def->id);
    if (sequences.isEmpty()) {
      continue;
    }
    const QString group = def->tab.isEmpty() ? tr("General")
                                             : QStringLiteral("%1 / %2").arg(def->tab, def->group);
    QTreeWidgetItem*& parent = groups[group];
    if (parent == nullptr) {
      parent = section(group);
    }
    QStringList labels;
    for (const QKeySequence& sequence : sequences) labels.append(sequence.toString(QKeySequence::NativeText));
    auto* row = addRow(parent, labels.join(QStringLiteral(", ")), def->name, def->id);
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
  QPushButton* edit = buttons->addButton(tr("Change Shortcuts…"), QDialogButtonBox::ActionRole);
  connect(edit, &QPushButton::clicked, &dialog, [this, &dialog] {
    dialog.accept();
    if (QAction* shortcuts = m_registry.action(QStringLiteral("tools.shortcuts"))) {
      shortcuts->trigger();
    }
  });
  connect(buttons, &QDialogButtonBox::rejected, &dialog, &QDialog::reject);
  layout->addWidget(buttons);
  mac::styleDialogButtons(buttons);
  mac::styleListTree(tree);
  mac::styleDialog(&dialog);
  dialog.resize(560, 620);
  qDebug().noquote() << QStringLiteral("Shortcut overview: %1 command shortcuts").arg(count);
  prepareModal(&dialog);
  execDialog(&dialog);
}

} // namespace mitcad
