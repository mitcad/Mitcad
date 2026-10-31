// SPDX-License-Identifier: MIT
#pragma once

// A drawing open in its own view (mitcad#707, app/COMMANDS.md "Drawings"):
// it takes the place of the 3D view, as a workspace does, until Close
// Drawing leads back. Its bar has the sheets as tabs (and + for another),
// Sheet… for the sheet's size, projection method, units, template and
// title block, Add Revision… for a row of the drawing's revision table, and
// under them the drawing tools (DrawingTools.hpp, each also a command of
// the window: drawing.*) in captioned groups with Export… (PDF, SVG, DXF)
// last; beside the sheet a panel shows the active tool's options or the
// selected view's properties. Everything goes to the model as its drawing
// commands through the main window (DocumentHost), so each is an undo
// step and the drawing follows undo and the model's changes (`refresh`).
// The tool's hint shows over the sheet; results and refusals go to the
// window's status bar.
//
// While it is open, Delete and Esc are its own wherever the focus is in
// it: Delete deletes the selected items, never the model's selection
// behind the sheet; Esc ends the tool, then clears the selection, and
// never closes the drawing (mitcad#725). The single-key shortcuts of the
// model's commands (those of the ribbon's tabs: E, H, …) do nothing while
// the focus is in it, so a stray key does not close the drawing (the
// window's menus' shortcuts, Undo, Redo and Save, still work).
//
// It is a child of the 3D view and covers all of it, edge to edge
// (mitcad#1008): its bar and sheet start below the rows that float over the
// view's top (OcctViewer::overlayInsets), the bar's controls beside a card
// at the view's side (the Floating layout's browser), and the sheet goes on
// under that card, fitted beside it; so it shows the same in every layout.
// While it is open the window shows the ribbon's DRAWING tab and, in the
// Floating layout, no card but the browser (MainWindow).
//
// Logs "Drawing area x,y WxH, view x,y WxH": its rectangle and the 3D
// view's in the window.
//
// Logs: "Drawing opened: D1 Drawing1, sheet 1 of 2", "Drawing closed",
// "Drawing tool: Section View" (or "selection"), "Drawing selection
// cleared", "Drawing tool <name> at x,y", "Drawing group <caption> at x,y"
// and "Drawing <button> at x,y" (Close Drawing, Sheet, Export, Add Sheet,
// Sheet tab <n>), "Drawing sheet tab pressed: <n>" (or "none"), "Drawing
// sheet <n> shown", "Drawing command: <json>" and "Drawing command
// refused: <why>", "Drawing panel <field> at x,y", "Drawing picked <what>
// (<n>): <the model's pick>" and "Drawing pick: nothing at x,y" (an
// annotation tool's clicks), "Drawing hover: <what>" (what a click would
// pick, "nothing"), "Drawing shortcut ignored: E".
//
// A cut sheet (mitcad#853) opens here too: its parts are the sheet's views
// (dragged to move them, turned from their menu or their panel's Rotation),
// the bar shows Add Parts… and Export… (DXF, SVG, the parts' names on the
// layer LABEL or not) instead of the drawing tools, and Material… instead
// of Sheet…. Logs "Cut sheet Add Parts dialog opened: <n> bodies", "Cut
// sheet warnings: <the model's>" (or "none") when they change, "Cut sheet
// labels: on" and "Exported <path>: cut sheet <name>, <n> parts".

#include <functional>

#include <QHash>
#include <QJsonArray>
#include <QJsonObject>
#include <QKeySequence>
#include <QList>
#include <QPair>
#include <QPoint>
#include <QPointF>
#include <QPointer>
#include <QString>
#include <QStringList>
#include <QVector>
#include <QWidget>

#include "DrawingTemplates.hpp"
#include "DrawingTools.hpp"

class QAction;
class QFormLayout;
class QKeyEvent;
class QVBoxLayout;
class QLabel;
class QTabBar;
class QTimer;
class QToolButton;

namespace mitcad {

class DocumentHost;
class OcctViewer;

namespace drawings {

class DrawingSheetView;

class DrawingWorkspace : public QWidget {
  Q_OBJECT

public:
  // `title`: the design's name, for the title block of the root's
  // drawings without a title of their own. `camera`: the 3D view's
  // direction as a base view's `direction` takes it (`{"toward_eye",
  // "up"}`), for Current View. `modelShortcut`: whether a key is the
  // shortcut of a command of the model's ribbon tabs.
  DrawingWorkspace(OcctViewer* viewer, DocumentHost& host, std::function<QString()> title,
                   std::function<QJsonObject()> camera, std::function<bool(const QKeySequence&)> modelShortcut);

  // Opens a drawing (`D1`) on its first sheet; false when the model has
  // no such drawing.
  bool open(const QString& uid);
  void closeDrawing();
  bool isOpen() const { return !m_drawing.isEmpty(); }
  QString drawing() const { return m_drawing; }
  // The model changed (a command, undo, another design): the sheet is
  // asked for again; the workspace closes when its drawing is gone.
  void refresh();
  // Runs a tool by its id ("drawing.base_view"), as its button and its
  // command do.
  void startTool(const QString& id);
  // Esc: a tool's picks go (the sheet's own), else the tool, else the
  // selection; the drawing stays open (mitcad#725).
  void escape();
  // Exports the drawing to a file (pdf, svg, dxf by its extension); false
  // with the model's reason shown.
  bool exportTo(const QString& path);
  // Add Revision (the bar's button and the command drawing.add_revision,
  // mitcad#707 phase 4): asks for its description, date and author.
  void addRevision();
  // A cut sheet's Add Parts… (mitcad#853): the bodies of its component to
  // add as parts (those selected in the model chosen first) and how many
  // of each.
  void addParts();
  bool isCutSheet() const { return m_cutSheet; }
  // Export… of the bar and the ribbon's (mitcad#1008): the drawing as
  // "pdf", "svg", "dxf" or "dwg" (a cut sheet's: "dxf" or "svg"), its file
  // asked for.
  void exportDrawing(const QString& format);
  // A cut sheet's export puts the parts' names on the layer LABEL; logs
  // "Cut sheet labels: on".
  void setCutLabels(bool on);
  bool cutLabels() const { return m_cutLabels; }

  // The window's ribbon has the tools (mitcad#1008): its DRAWING tab (CUT
  // SHEET for a cut sheet) holds Close Drawing, the tools' groups and
  // Export, so the bar is one row (the drawing's name, the sheets, +,
  // Sheet…, Add Revision…). `buttonOf` finds a control there, by a
  // command's id or a group's "DRAWING/EXPORT", for the logged places
  // ("Drawing tool Base View at x,y" is the ribbon's button); null when it
  // is not in sight. Unset (the AppKit frontend, which has no DRAWING tab
  // yet): the bar's own two rows.
  void setRibbonTools(std::function<QWidget*(const QString&)> buttonOf);
  // The places logged again (the ribbon's moved), when they changed.
  void logPlaces();

signals:
  void closed();
  // The tool on (its id, "drawing.base_view"), or none (empty): the
  // ribbon's buttons show it checked.
  void toolChanged(const QString& id);

protected:
  bool event(QEvent* event) override;
  bool eventFilter(QObject* watched, QEvent* event) override;
  void keyPressEvent(QKeyEvent* event) override;
  void resizeEvent(QResizeEvent* event) override;
  void showEvent(QShowEvent* event) override;

private:
  void place();
  // The views' tools with their names, or their icons only when the bar
  // is narrower than its row with names (`width`, pixels).
  void fitTools(int width);
  void showSheet(const QJsonObject& answer);
  void setTool(const DrawingToolDef* tool);
  void fillPanel();
  void fillToolPanel(const DrawingToolDef& tool);
  void fillViewPanel(int view);
  // An annotation's properties (mitcad#707 phase 2): its value, type,
  // format, tolerance (#702's dialog), text, letter, frame or finish.
  void fillAnnotationPanel(int annotation);
  // A hole table's or a parts list's settings (mitcad#707 phase 3): its
  // columns, sort, origin and tags or rows, title.
  void fillTablePanel(int annotation, const QJsonObject& json);
  void picked(ToolPicks picks);
  // An annotation tool's click: the model says what it picks; enough
  // picks lead to its placing click, or make it.
  void pickRequested(int view, QPointF at);
  // Drops an annotation tool's picks so far; false when there were none.
  bool clearToolPicks();
  // What an annotation tool asks before it is made: a note's text (#702's
  // dialog) when its Text option is empty, a frame's tolerance; false when
  // that was cancelled.
  bool askAnnotationFields(const DrawingToolDef& tool, QHash<QString, QString>& options);
  // The datum letters a feature control frame can refer to: the model's
  // and the drawings' labels (the `drawing` query's `datum_letters`).
  QStringList datumLetters() const;
  void annotationMoved(int annotation, QPointF delta);
  void deleteAnnotation(int annotation);
  // Runs one field's change of an annotation after the event that made it.
  void editAnnotation(const QJsonObject& command);
  // A base view's Shows list (subjectChoices): the drawing's component,
  // its occurrences and its bodies by name.
  QList<QPair<QString, QString>> subjects() const;
  // The 3D view's direction for Current View; empty, with why in the
  // status bar, when there is none.
  QJsonObject cameraDirection();
  // A base view's Orientation changed in its panel ("current": the 3D
  // view's).
  void setDirection(int view, const QString& direction);
  void viewMoved(int view, QPointF delta);
  void deleteView(int view);
  // A sheet tab clicked (or chosen otherwise): its sheet shown.
  void switchSheet(int index);
  // Whether a key is one of the model's commands' shortcuts (`modelShortcut`).
  bool isModelShortcut(const QKeyEvent* key) const;
  // The pointer over the sheet while an annotation tool waits for a pick:
  // the model is asked what a click there would pick, at most every
  // 60 ms (mitcad#725).
  void hoverRequested(int view, QPointF at);
  void hoverPick();
  // The sheet's menu (mitcad#725) for what is selected, or the sheet.
  void contextMenu(QPointF at, const QPoint& global);
  // A note's or a text's text in the note dialog (its panel's Text…, a
  // double-click, the menu's Edit Text…).
  void editNoteText(int annotation);
  // Several selected items (Ctrl+click): deleted, moved, in the panel.
  void deleteSelection();
  void selectionMoved(QPointF delta);
  void fillSelectionPanel();
  // The sheet's preview of what is placed after an annotation tool's
  // picks: their points as the model found them.
  void showPlacingPreview();
  // Runs a model command; false with its reason in the status bar.
  bool run(const QJsonObject& command, QJsonObject* result = nullptr);
  // Runs one field's change of a view after the event that made it (the
  // panel is built again once the model has it).
  void editView(int view, const QString& field, const QString& value);
  void editSheet();
  void addSheet();
  // Delete Sheet of a sheet tab's menu: the sheet shown stays shown, at its
  // new place (mitcad#759).
  void deleteSheet(int index);
  // Another sheet shown: the active tool's picks so far, its half-drawn
  // line or circle and a placing click awaited go, the tool stays
  // (mitcad#759).
  void dropPicks();
  void sheetMenu(int index, const QPoint& at);
  // A revision of the table edited (or deleted) in its dialog.
  void editRevision(const QString& letter);
  // Save as Template… of a sheet tab's menu: the sheet's template under a
  // name, to the user's or the project's library.
  void saveTemplate(int sheet);
  // The sheet's template and title block fields and the drawing's
  // revisions, logged when they change (the UI tests read them).
  void logTitleBlock(const QJsonObject& sheet);
  // The window's status bar; empty text says nothing.
  void setStatus(const QString& text, bool error);
  // The active tool's hint over the sheet.
  void showHint();
  void logButtons();
  // A cut sheet's panel (mitcad#853): its material, parts and warnings,
  // or the selected part's face, rotation and copies.
  void fillCutSheetPanel();
  void fillPartPanel(int part);
  // Material… of a cut sheet: its width and height.
  void editCutSheet();
  // Runs edit_cut_sheet_part with these fields after the event that made
  // them.
  void editPart(int part, const QJsonObject& fields);
  // Another part of the same body and face.
  void addCopy(int part);

  OcctViewer* m_viewer;
  DocumentHost& m_host;
  std::function<QString()> m_title;
  std::function<QJsonObject()> m_camera;
  std::function<bool(const QKeySequence&)> m_modelShortcut;
  QString m_drawing;
  QString m_name;
  int m_sheet = 0;
  QJsonObject m_answer; // the `drawing` query's last answer
  const DrawingToolDef* m_tool = nullptr;
  // The tools' options, by tool id and option id, kept between uses.
  QHash<QString, QHash<QString, QString>> m_options;
  QString m_problems; // the views not drawn, as last said
  // An annotation tool's picks so far (the model's), what shows them, and
  // the view they are on.
  QJsonArray m_toolPicks;
  QJsonArray m_pickItems;
  QVector<QPointF> m_pickPoints; // where the model found them, sheet mm
  int m_pickView = 0;
  // The hover's pick asked for last (mitcad#725): the view (-1 none) and
  // the point, the timer that spaces the questions, what was logged.
  QTimer* m_hoverTimer = nullptr;
  int m_hoverView = -1;
  QPointF m_hoverAt;
  QString m_loggedHover;
  // The hover's question waits for the job that computes the model
  // (mitcad#753).
  bool m_hoverWaits = false;

  QVBoxLayout* m_column = nullptr; // the bar over the sheet and the panel
  std::function<QWidget*(const QString&)> m_ribbonPlace; // setRibbonTools
  QAction* m_cutLabelsAction = nullptr;
  QWidget* m_bar = nullptr;
  QLabel* m_titleLabel = nullptr;
  QTabBar* m_tabs = nullptr;
  QToolButton* m_close = nullptr;
  QToolButton* m_addSheet = nullptr;
  QToolButton* m_sheetButton = nullptr;
  QToolButton* m_export = nullptr;
  QToolButton* m_addRevision = nullptr;
  QHash<QString, QToolButton*> m_toolButtons;
  // The groups' captions under the tools, logged with their places.
  QList<QLabel*> m_groupCaptions;
  DrawingSheetView* m_sheetView = nullptr;
  QWidget* m_panel = nullptr;
  QLabel* m_panelTitle = nullptr;
  QFormLayout* m_form = nullptr;
  QString m_loggedButtons;
  QString m_loggedTitle;
  QString m_loggedRevisions;
  bool m_updatingTabs = false;
  // A cut sheet is open (mitcad#853): its row instead of the tools'.
  bool m_cutSheet = false;
  bool m_cutLabels = false;
  QString m_cutWarnings;
  QWidget* m_toolRow = nullptr;
  QWidget* m_cutRow = nullptr;
  QToolButton* m_addParts = nullptr;
  QToolButton* m_cutExport = nullptr;
  bool m_compactTools = false;
  int m_toolsWidth = -1;
};

// The sheet templates a drawing can take (the model's `drawing_templates`
// with the design's project), then the simple title block.
QList<TemplateChoice> availableTemplates(const DocumentHost& host);

} // namespace drawings
} // namespace mitcad
