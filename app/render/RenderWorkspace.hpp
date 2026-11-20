// SPDX-License-Identifier: MIT
#pragma once

#include <functional>

#include <QHash>
#include <QIcon>
#include <QJsonArray>
#include <QJsonObject>
#include <QObject>
#include <QPointer>
#include <QString>
#include <QStringList>
#include <QVector>
#include <QWidget>

#include "framework/Appearances.hpp"
#include "framework/Selection.hpp"

class QAction;
class QButtonGroup;
class QComboBox;
class QHBoxLayout;
class QLabel;
class QLineEdit;
class QListWidget;
class QListWidgetItem;
class QPushButton;
class QStackedWidget;
class QTabBar;
class QToolButton;
class QTreeWidget;
class QTreeWidgetItem;

namespace mitcad {

class DocumentHost;
class OcctViewer;
class RenderEnvironmentPanel;
class RenderImagePanel;
class RenderMode;

// The Render workspace (mitcad#2013): the toolbar's Render tab, where an
// image of the design is made in four stages instead of separate windows.
// Its parts are placed by the main window (MainWindowRender.cpp): the
// stages as the Render tab's own tab row (stageBar, Ribbon::setTabRow) with
// Render and Cancel at its right beside the search (stageButtons), under it
// the stage's actions as the toolbar's groups (stageActions: icon buttons
// with their labels under them, a caption with ˅ under each group, thin
// lines between them): Live (Pause or Resume, the render method, its
// caption the live view's status "Live · W × H · n/m"), then the stage's
// own (Look: Pick and Look; Light; Frame; Output), the stage's list at the
// left (leftPanel), the settings
// of what is chosen in it at the right (rightPanel) and the Shots bar at
// the bottom (bottomBar, a chip per shot to look through). One stage is
// one task: its own list and its own card, and nothing chosen in a list
// ever shows another stage. The panels have no headings of their own:
// their cards' (or docks') titles are leftTitle and rightTitle; their
// sections are flat headings that fold (Ready to render, Image, ...), never
// framed boxes, and they scroll down, never sideways, their fields as wide
// as the card leaves (the scroll bar's room taken from it when it shows).
// While it is open the view shows the rendered view, refining, and the
// final image refines in the view itself.
//
//   1 Look: the Appearances library at the left (search, In Design /
//     Library / Recent / ★ Favorites, the classes as filters, swatches);
//     at the right what is picked in the view (the Pick group's Body, Face
//     or Component) and its appearance, Apply to Selection and Edit
//     Appearances…; a click on a swatch gives it to the picks, Paint (the
//     Look group) applies the chosen appearance to each pick, Pick Up
//     copies one pick's appearance to paint with.
//   2 Light: the Environment (its summary, "Studio") and the Lights (an eye
//     each, on or off) at the left; the chosen row's settings at the right
//     (RenderEnvironmentPanel's environment or the light's), the card
//     titled by the row; the Light group's Add Light and Reset Environment.
//   3 Frame: the Shots (Rename… and Delete under them) and the saved views
//     at the left; the chosen shot's camera at the right; the Frame group's
//     Save Shot, Update Shot to View and Look Through.
//   4 Output: the shots to render at the left (the view or a shot: what
//     Render renders); Render Image's settings at the right
//     (RenderImagePanel, Ready to render first); the Output group's Check
//     Now and Render All Shots (each shot after the other): Render renders
//     the chosen shot (or the view).
//
// Every change is a model command and an undo step; the design's geometry
// is never changed here. The places it logs are in the main window's
// coordinates: "Render stage Look at x,y", "Render workspace render at",
// "Render shot save at", "Render light row Environment at", ...
class RenderWorkspace : public QObject {
  Q_OBJECT

public:
  enum class Stage { Look, Light, Frame, Output };
  enum class PickMode { Body, Face, Component };

  // What the workspace asks of the window.
  struct Hooks {
    // The rendered view (null without the render worker); the window turns
    // it on with the Render workspace (MainWindow::setRenderWorkspace).
    std::function<RenderMode*()> renderMode;
    // The window's selection, a new one, and the pick filter applied again
    // (takesPicks, pickFilter).
    std::function<Selection()> selection;
    std::function<void(const Selection&)> select;
    std::function<void()> pickFilterChanged;
    // A command of the registry by id (solid.appearances).
    std::function<void(const QString&)> runCommand;
    // The render device's choice changed: the live view starts again on it.
    std::function<void()> deviceChanged;
  };

  RenderWorkspace(DocumentHost& host, OcctViewer& viewer, Hooks hooks, QObject* parent = nullptr);
  ~RenderWorkspace() override;

  // The stages (the Render tab's own tab row), Render and Cancel (at the
  // row's right) and the stage's actions (the tab's page).
  QWidget* stageBar() const;
  QWidget* stageButtons() const { return m_stageButtons; }
  QWidget* stageActions() const { return m_actions; }
  QWidget* leftPanel() const;
  QWidget* rightPanel() const;
  QWidget* bottomBar() const { return m_bottom; }
  // The panels' titles: the stage's list at the left ("Appearances",
  // "Lights", "Shots", "Shots to Render"); at the right what is chosen in
  // it ("Look", "Environment" or the light's name, "Shot" or the shot's,
  // "Output"). titlesChanged tells a change; logged as "Render panels:
  // Lights | Environment".
  QString leftTitle() const;
  QString rightTitle() const;

  // Opened with the Render tab, closed when it goes: a running render
  // stopped at the close, Paint and Pick Up off, the live view resumed.
  // Logged "Render workspace opened|closed".
  void setOpen(bool open);
  bool isOpen() const { return m_open; }
  // The model changed (undo, redo, a command, another design).
  void refresh();
  // The design's folder and name (the images' folder and file names).
  void setDocument(const QString& folder, const QString& name);
  Stage stage() const { return m_stage; }
  void showStage(Stage stage);

  // Look's picks (mitcad#2013): while the Look stage shows, the view picks
  // what the pick mode names; with Paint or Pick Up on, a pick is theirs
  // (takePick: true) and selects nothing.
  bool takesPicks() const { return m_open && m_stage == Stage::Look; }
  SelectFilter pickFilter() const;
  bool takePick(const Selection& items);
  // The window's selection changed: Look's card shows what is picked and
  // its appearance ("Render look for Body2: steel_satin").
  void selectionChanged();

  // Logs the places of the shown parts again (after the window placed them).
  void logLayout();

  static QString stageName(Stage stage);

signals:
  // Another stage shows: the window places the panels again.
  void stageChanged();
  // leftTitle or rightTitle changed.
  void titlesChanged();

private:
  void buildStageBar();
  void buildActions();
  void buildLook();
  void buildLight();
  void buildFrame();
  void buildOutput();
  void buildBottom();
  // The panels' framed group boxes as flat sections that fold: a heading
  // in sentence case with an arrow, the box without its frame under it.
  void flattenSections(QWidget* root);
  void updateTitles();

  // Look.
  void refreshLibrary();
  void fillLibrary();
  void chooseAppearance(const QString& id, bool apply);
  void applyAppearance(const Selection& items, const QString& id);
  QString appearanceOfPick(const SelectionItem& item) const;
  void setPickMode(PickMode mode);
  void setFavorite(const QString& id, bool favorite);
  void showChosen();
  void showPicked();
  QIcon swatch(const Appearance& appearance);

  // Light's list: the Environment and the Lights, made anew after a model
  // change with the chosen row and the scroll position kept; a row chosen
  // shows its settings (never another stage).
  void refreshLightList();
  void lightRowChosen(const QString& row);

  // Frame, Output's list and the Shots row.
  void refreshShots();
  void chooseOutputShot(const QString& id);
  void saveShot();
  void renameShot();
  void updateShotToView();
  void deleteShot();
  void lookThrough(const QString& id);
  // The Output group's Render All Shots enabled while nothing renders and
  // there are shots.
  void updateRenderAll();
  QJsonObject shotObject(const QString& id) const;

  // Output.
  // Ready to Render's findings shown; false when one is a problem.
  bool checkReady(bool allShots);
  void render();
  void cancel();

  // The live view's line.
  void showLive(int width, int height, int samples, int total);
  void setPaused(bool paused);
  void updateLive();
  void connectRenderMode();

  bool runCommands(const QJsonArray& commands, const QString& label);
  QString bodyName(const QString& uid) const;

  DocumentHost& m_host;
  OcctViewer& m_viewer;
  Hooks m_hooks;
  bool m_open = false;
  Stage m_stage = Stage::Look;
  QJsonObject m_settings;   // render_settings
  QJsonArray m_bodies;      // the bodies query
  QVector<Appearance> m_appearances;
  QString m_documentFolder;
  QString m_documentName;

  // The stages' tab row and the stage's groups (each stage's own in its
  // holder, shown with the stage).
  QWidget* m_stageButtons = nullptr;
  QTabBar* m_stages = nullptr;
  QToolButton* m_render = nullptr;
  QToolButton* m_cancel = nullptr;
  QWidget* m_actions = nullptr;
  QWidget* m_liveGroup = nullptr;
  QWidget* m_lookActions = nullptr;
  QWidget* m_lightActions = nullptr;
  QWidget* m_frameActions = nullptr;
  QWidget* m_outputActions = nullptr;
  QString m_leftTitle;
  QString m_rightTitle;

  // The side panels.
  QStackedWidget* m_left = nullptr;
  QStackedWidget* m_right = nullptr;

  // Look: the library and its settings.
  QWidget* m_library = nullptr;
  QLineEdit* m_search = nullptr;
  QComboBox* m_scope = nullptr; // In Design, Library, Recent, Favorites
  QButtonGroup* m_filters = nullptr;
  QListWidget* m_swatches = nullptr;
  QWidget* m_lookSettings = nullptr;
  // The Pick group's modes (exclusive) and their buttons, by PickMode.
  QList<QAction*> m_modes;
  QList<QToolButton*> m_modeButtons;
  QLabel* m_chosenSwatch = nullptr;
  QLabel* m_chosenName = nullptr;
  // The Look group's Paint and Pick Up (checkable, one at a time).
  QAction* m_paint = nullptr;
  QAction* m_pickUp = nullptr;
  QToolButton* m_paintButton = nullptr;
  QToolButton* m_pickUpButton = nullptr;
  QPushButton* m_apply = nullptr;
  QPushButton* m_editAppearances = nullptr;
  QLabel* m_pickedName = nullptr;
  QLabel* m_lookMessage = nullptr;
  PickMode m_mode = PickMode::Body;
  QString m_chosen; // the appearance id; "default" for the default look
  QHash<QString, QPair<Appearance, QIcon>> m_swatchCache;

  // Light: its list and the chosen row's settings.
  QWidget* m_lightsPanel = nullptr;
  QTreeWidget* m_lightList = nullptr;
  QToolButton* m_addLight = nullptr; // the Light group's
  QString m_lightRow = QStringLiteral("environment"); // or a light's id
  bool m_listBuilding = false;
  QWidget* m_lightSettings = nullptr;
  RenderEnvironmentPanel* m_environment = nullptr;
  QToolButton* m_resetEnvironment = nullptr; // the Light group's

  // Frame: the shots and the saved views, the chosen shot's camera.
  QWidget* m_shotsPanel = nullptr;
  QListWidget* m_shotList = nullptr;
  QWidget* m_frameSettings = nullptr;
  QListWidget* m_views = nullptr;
  QLabel* m_shotTitle = nullptr;
  // The Frame group's.
  QToolButton* m_lookThrough = nullptr;
  QToolButton* m_updateShot = nullptr;

  // Output: the shots to render, the image's settings.
  QWidget* m_outputPanel = nullptr;
  QListWidget* m_outputShots = nullptr;
  QWidget* m_outputSettings = nullptr;
  RenderImagePanel* m_output = nullptr;
  QLabel* m_findings = nullptr;
  // The Output group's.
  QToolButton* m_checkNow = nullptr;
  QToolButton* m_renderAll = nullptr;

  // The Live group: Pause or Resume, the render method (its menu's
  // choices), its caption the live view's status.
  QAction* m_pauseAction = nullptr;
  QToolButton* m_pause = nullptr;
  QAction* m_methodAction = nullptr;
  QAction* m_methodAuto = nullptr;
  QAction* m_methodCpu = nullptr;
  QToolButton* m_method = nullptr;
  // The bottom bar: the Shots row (its chips); Frame's list holds Rename…
  // and Delete (Save Shot is the Frame group's).
  QWidget* m_bottom = nullptr;
  QWidget* m_shotButtons = nullptr;
  QHBoxLayout* m_shotRow = nullptr;
  QButtonGroup* m_shotGroup = nullptr;
  QToolButton* m_saveShot = nullptr; // the Frame group's
  QPushButton* m_renameShot = nullptr;
  QPushButton* m_deleteShot = nullptr;
  QString m_shot; // the current shot's id; empty: the view
  bool m_userPaused = false;
  QPointer<RenderMode> m_renderMode;
  QString m_liveText;
  QString m_logged;
};

} // namespace mitcad
