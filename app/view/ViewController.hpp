// SPDX-License-Identifier: MIT
#pragma once

#include <functional>
#include <optional>

#include <QColor>
#include <QHash>
#include <QJsonObject>
#include <QList>
#include <QObject>
#include <QPointer>
#include <QString>

#include "../OcctViewer.hpp"
#include "../framework/ChromeStyle.hpp"
#include "../framework/Selection.hpp"

class QAction;
class QActionGroup;
class QMenu;
class QWidget;

namespace mitcad {

class CommandRegistry;
class DocumentHost;
class RenderEnvironmentDialog;
class RenderImageDialog;
class RenderMode;

// The display settings of the view (U5), kept in the user's settings
// (group "view").
struct ViewSettings {
  // Automatic follows the system: the default gradient in light mode, a
  // dark one in dark mode. It comes last: the settings keep the number.
  enum class Background { Gradient, Light, White, Dark, Custom, Automatic };

  VisualStyle style = VisualStyle::ShadedVisibleEdges;
  bool perspective = false;
  bool componentColors = false;
  Background background = Background::Gradient;
  // Custom: the default gradient's colours (sRGB).
  QColor top = QColor(247, 248, 250);
  QColor bottom = QColor(223, 227, 234);
  bool gridShown = true;
  double gridSpacing = 0.0; // mm; 0: automatic, the spacing follows the zoom
  bool snapToGrid = true;
  NavigationSettings navigation;
  // Everything outside the active component drawn as ghosts (mitcad#420,
  // `view/ghostInactive`).
  bool ghostInactive = true;
  // Picks take the active component's items first (mitcad#420,
  // `view/activePickPriority`).
  bool activePickPriority = true;
  // The orientation cube's projection (mitcad#774, `view/cubeProjection`
  // by its id): as the view's by default (owner 27.10.2026).
  CubeProjection cubeProjection = CubeProjection::FollowView;
  // The navigation bar at the bottom of the view (mitcad#1086,
  // `view/navigationBar`): shown by default.
  bool navigationBar = true;
  // Anaglyph 3D (mitcad#1449, `view/anaglyph`): off by default. Its stereo
  // depth (`view/stereoDepth`), the eyes' separation in percent of the
  // focus distance, and convergence (`view/stereoConvergence`), the focus
  // distance in percent of the distance to the view's centre; OCCT's own
  // defaults.
  bool anaglyph = false;
  int stereoDepth = 5;
  int stereoConvergence = 100;
  static constexpr int kMinStereoDepth = 1;
  static constexpr int kMaxStereoDepth = 20;
  static constexpr int kMinStereoConvergence = 25;
  static constexpr int kMaxStereoConvergence = 400;

  static ViewSettings load();
  void save() const;
  // The background's colours (a gradient from top to bottom, or `top`).
  void colors(QColor& top, QColor& bottom, bool& gradient) const;
};

// The View menu (U5): standard views and Home, Look At, the camera's
// projection, visual styles, the layout grid and grid snap, the
// environment's background, named views (saved in the document), the view
// cube's menu, Preferences (navigation) and the keyboard and mouse
// overview. Its commands are registry actions without a tab (menus,
// command search, shortcuts), as COMMANDS.md asks of view commands.
class ViewController : public QObject {
  Q_OBJECT

public:
  // `lookAt` turns the view to the selection (a plane, a planar face, a
  // sketch; the sketch plane in sketch mode).
  ViewController(OcctViewer& viewer, CommandRegistry& registry, DocumentHost& host,
                 std::function<void()> lookAt, QWidget* window);

  // Adds the view commands to the registry (before its actions are made).
  void registerCommands();
  // Fills the View, Tools and Help menus (after the actions are made) and
  // applies the saved settings.
  void createMenus(QMenu* view, QMenu* tools, QMenu* help);

  const ViewSettings& settings() const { return m_settings; }
  // Grid snap in sketches: on or off, and the step when the grid is fixed.
  std::function<void(bool on, double step)> gridSnapChanged;
  // Preferences saved new general settings (GeneralSettings: autosave).
  std::function<void()> generalChanged;
  // Preferences changed the default units for new designs (mitcad#1452).
  std::function<void()> newDesignUnitsChanged;
  // Help > Diagnostics, from Preferences' Cache group (P7d).
  std::function<void()> showDiagnostics;
  // Preferences' Layout changed and Restart Now was chosen (mitcad#419):
  // Mitcad starts again, its window closed as when it quits.
  std::function<void()> restartRequested;
  // Preferences' Show the navigation bar changed (mitcad#1086): the bar
  // shows or hides at once.
  std::function<void(bool shown)> navigationBarChanged;
  void setSnapToGrid(bool on);
  // The View menu's Visual Style actions in menu order (after createMenus),
  // Rendered after a null entry (a separator) when there is a render worker:
  // the navigation bar's drop-down (mitcad#1086).
  QList<QAction*> visualStyleActions() const;

  // A standard view: front, back, left, right, top, bottom, iso.
  void showStandardView(const QString& view);
  // Saves the camera as a named view (the next free NamedView<n> when the
  // name is empty) as an undo step; `replace` overwrites one of the name.
  bool saveNamedView(const QString& name, bool replace);
  // The camera of a saved view (the `named_views` query's entry).
  static CameraState cameraOf(const QJsonObject& view);
  // The cube's right-click menu.
  void showOrientationCubeMenu(const QPoint& globalPosition);
  void showShortcutOverview();
  // Preferences on a page (`general`, `navigation`, `display`, `cache`,
  // `cloud`, `updates`, `print`); the last one shown when empty.
  void showPreferences(const QString& page = QString());
  void showGridSettings();
  // The design's length unit (mitcad#1210), after each change of the
  // model: Grid Settings shows the fixed spacing in it, and the automatic
  // grid steps through its series (an inch series in a design in in or ft).
  void setLengthUnit(const QString& unit);
  // The document's render settings (the `render_settings` query, mitcad#47)
  // and its file's folder, after each change of the model: the rendered
  // view and Render Environment follow them. Nothing without the render
  // worker.
  void setRenderSettings(const QJsonObject& settings, const QString& documentFolder);
  // Render Environment (only with the render worker).
  void showRenderEnvironment();
  // Render Image (mitcad#48; only with the render worker).
  void showRenderImage();
  // The design's file name without its folder and extension (the rendered
  // image's default name).
  void setDocumentName(const QString& name) { m_documentName = name; }
  // Show Build Volume (mitcad#640, view/BuildVolume.cpp): draws the design
  // printer's bed and height (the `build_volume` query) and the boxes of
  // the bodies that do not fit, after each change of the model while it is
  // on; logs "Build volume: …" when what is drawn changes.
  void refreshBuildVolume();
  bool buildVolumeShown() const { return m_buildVolume; }
  // View > Export Snapshot (mitcad#649, view/ViewSnapshot.cpp): asks for
  // the file and the choices (SnapshotDialog), renders the view at that
  // size with the shown measurements once the model is idle, and writes
  // the image; logs "Snapshot saved <path>: <w> x <h>, …".
  void exportSnapshot();

signals:
  // Camera/background changed outside the non-modal Settings window.
  void displaySettingsChanged();

protected:
  // The arrow keys' turn commands (mitcad#1211) take their keys only while
  // a 3D view has the keyboard: anywhere else (a value or text field, the
  // Browser, a list, a panel, a dialog) the key keeps its usual meaning.
  bool eventFilter(QObject* watched, QEvent* event) override;

private:
  // Turns the view by Preferences › Navigation's step, `right` and `up`
  // steps (-1, 0 or 1) as the arrow keys do (mitcad#1211).
  void turnView(int right, int up);
  void apply();
  void setStyle(VisualStyle style);
  void setPerspective(bool perspective);
  // Anaglyph 3D (mitcad#1449): the view in red/cyan stereo, in perspective
  // while on; the rendered view is left and not offered meanwhile.
  void setAnaglyph(bool on);
  // Its stereo depth and convergence (percent) to the view.
  void applyStereo(int depth, int convergence);
  void setBackground(ViewSettings::Background background);
  void setGridShown(bool shown);
  void syncChecks();
  // Asks whether to restart now for the layout chosen in Preferences
  // (mitcad#419); yes calls restartRequested.
  void askLayoutRestart(ChromeStyle chosen);

  OcctViewer& m_viewer;
  CommandRegistry& m_registry;
  DocumentHost& m_host;
  std::function<void()> m_lookAt;
  QWidget* m_window;
  QPointer<QWidget> m_settingsWindow; // macOS: the Settings window, when open
  ViewSettings m_settings;
  QHash<QString, QAction*> m_checks; // checkable actions by command id
  // View > Rendered (docs/rendering.md); null without the render worker.
  RenderMode* m_render = nullptr;
  QPointer<RenderEnvironmentDialog> m_renderEnvironment;
  QPointer<RenderImageDialog> m_renderImage;
  QString m_documentFolder;
  QString m_documentName;
  QString m_lengthUnit = QStringLiteral("mm"); // the design's (mitcad#1210)
  void setRendered(bool on);
  QString m_preferencesPage = QStringLiteral("general"); // the page Preferences opens on
  // Show Build Volume (mitcad#640): on or off for the window's session.
  void setBuildVolumeShown(bool shown);
  bool m_buildVolume = false;
  QString m_loggedBuildVolume;
};

// The visual styles by command id and name, in menu order.
QString visualStyleName(VisualStyle style);

} // namespace mitcad
