// SPDX-License-Identifier: MIT
#pragma once

#include <functional>
#include <optional>

#include <QColor>
#include <QHash>
#include <QJsonObject>
#include <QObject>
#include <QPointer>
#include <QString>

#include "../OcctViewer.hpp"
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
  Background background = Background::Gradient;
  // Custom: the default gradient's colours (sRGB).
  QColor top = QColor(247, 248, 250);
  QColor bottom = QColor(223, 227, 234);
  bool gridShown = true;
  double gridSpacing = 0.0; // mm; 0: automatic, the spacing follows the zoom
  bool snapToGrid = true;
  NavigationSettings navigation;

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
  // Help > Diagnostics, from Preferences' Cache group (P7d).
  std::function<void()> showDiagnostics;
  void setSnapToGrid(bool on);

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
  // `version_control`, `updates`, `print`); the last one shown when empty.
  void showPreferences(const QString& page = QString());
  void showGridSettings();
  // The document's render settings (the `render_settings` query, mitcad#47)
  // and its file's folder, after each change of the model: the rendered
  // view and Render Environment follow them. Nothing without MITCAD_RENDER.
  void setRenderSettings(const QJsonObject& settings, const QString& documentFolder);
  // Render Environment (only with the render worker).
  void showRenderEnvironment();
  // File > Render Image (mitcad#48; only with the render worker).
  void showRenderImage();
  // The design's file name without its folder and extension (the rendered
  // image's default name).
  void setDocumentName(const QString& name) { m_documentName = name; }

private:
  void apply();
  void setStyle(VisualStyle style);
  void setPerspective(bool perspective);
  void setBackground(ViewSettings::Background background);
  void setGridShown(bool shown);
  void syncChecks();

  OcctViewer& m_viewer;
  CommandRegistry& m_registry;
  DocumentHost& m_host;
  std::function<void()> m_lookAt;
  QWidget* m_window;
  QPointer<QWidget> m_settingsWindow; // macOS: the Settings window, when open
  ViewSettings m_settings;
  QHash<QString, QAction*> m_checks; // checkable actions by command id
  // View > Rendered (docs/rendering.md); only in builds with MITCAD_RENDER.
  RenderMode* m_render = nullptr;
  QPointer<RenderEnvironmentDialog> m_renderEnvironment;
  QPointer<RenderImageDialog> m_renderImage;
  QString m_documentFolder;
  QString m_documentName;
  void setRendered(bool on);
  QString m_preferencesPage = QStringLiteral("general"); // the page Preferences opens on
};

// The visual styles by command id and name, in menu order.
QString visualStyleName(VisualStyle style);

} // namespace mitcad
