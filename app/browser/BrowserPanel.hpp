// SPDX-License-Identifier: MIT
#pragma once

#include <functional>
#include <utility>

#include <QHash>
#include <QIcon>
#include <QJsonObject>
#include <QPoint>
#include <QRect>
#include <QRegion>
#include <QSet>
#include <QString>
#include <QStringList>
#include <QVector>
#include <QWidget>

#include "../framework/Selection.hpp"
#include "DocumentSnapshot.hpp"

class QScrollBar;
class QTimer;
class QTreeWidgetItem;

namespace mitcad {

class BrowserTree;

// A row of the browser.
struct BrowserNode {
  enum class Type {
    None,
    Settings,   // Design Settings
    Units,      // the document's length unit
    NamedViews, // Named Views
    NamedView,  // uid: home, top, front, right; the document's: named:<name>
    Origin,     // the root's Origin folder
    OriginDatum, // uid: xy, xz, yz, x, y, z, origin
    Component,  // the root (occurrence empty) or an occurrence; uid: the component
    Bodies,     // folders of a component (in an occurrence)
    Sketches,
    Construction,
    Body,   // uid: the body
    Sketch, // uid: the sketch feature
    Datum,  // uid: the construction feature
    Analyses, // the Analysis folder (mitcad#41)
    Analysis, // uid: its name
    Joints,   // a component's joints (mitcad#55); joint origins are Datum rows in it
    Joint,    // uid: a joint, as-built joint or rigid group
    // A feature under a sketch it uses (mitcad#177): uid: the feature,
    // sketch: that sketch; one row under each sketch it uses.
    Feature,
    // Sheet metal (mitcad#210), under a sheet-metal body's row: its rules
    // (uid: the rules feature) and its Flat Pattern (uid: the body; the
    // light bulb shows it flat in place of the folded body).
    SheetRules,
    FlatPattern,
    // Motion studies and contact sets (mitcad#454): the root's folders and
    // their rows (uid: the name).
    MotionStudies,
    MotionStudy,
    ContactSets,
    ContactSet,
    // A component's motion links (mitcad#466); a link's row is a Joint row.
    MotionLinks,
    // Annotations (mitcad#702; the measurement objects of mitcad#649): the
    // root's Annotations folder, which opens their window.
    Measurements,
    // Drawings kept in the design (mitcad#707): a component's Drawings
    // folder (the root's too) and its rows (uid: the drawing, D1).
    Drawings,
    Drawing,
    // Image references (mitcad#782): the root's Images folder and its rows
    // (uid: the name).
    ImageReferences,
    ImageReference,
    // The Center of Mass marker (mitcad#1094): a row of the root's under
    // Origin while the design has the marker; its light bulb shows it.
    CenterOfMass,
  };

  Type type = Type::None;
  QString uid;
  QString occurrence; // the occurrence path ("O1/O4"); empty in the root
  QString component;  // the component the row belongs to
  QString featureType; // sketches, datums and features: construction_plane, ...
  QString sketch;     // a feature row: the sketch it is under
  QString name;       // as shown
  QString path;       // the names from the root, "Root/Bodies/Body1"
  bool visible = true; // its light bulb
  bool hasEye = false;
  // A body or an occurrence drawn see-through (mitcad#1093): its own state,
  // the half-open eye while it is shown.
  bool transparent = false;
  // A body: a mesh body (mitcad#399); its item's geometry is "mesh".
  bool mesh = false;

  bool isValid() const { return type != Type::None; }
  bool isFolder() const {
    return type == Type::Bodies || type == Type::Sketches || type == Type::Construction ||
           type == Type::Origin || type == Type::Analyses || type == Type::Joints ||
           type == Type::MotionLinks || type == Type::Measurements || type == Type::Drawings ||
           type == Type::ImageReferences;
  }
  // The selection item the row stands for; invalid for folders and settings.
  SelectionItem item() const;
};

// The browser, docked on the left: the document settings, named
// views, the origin and the component tree with each occurrence's bodies,
// sketches (each with the features that use it under it, mitcad#177),
// construction geometry and joints, and the analyses kept in the document.
// A row reads: expand arrow, light bulb (an eye; shows and hides), icon,
// name and, for a component, the radio button that activates it. Selecting
// rows selects in the view and the timeline, and the other way round; a
// slow second click on a selected row renames it in place, as F2 does.
// Ctrl+F (Cmd+F) opens the timeline's search (mitcad#179). What the rows
// do is the BrowserController's.
class BrowserPanel : public QWidget {
  Q_OBJECT

public:
  explicit BrowserPanel(QWidget* parent = nullptr);

  // Toolkit-independent row identities and hierarchy for other
  // presentations of the same browser actions.
  struct PresentationRow {
    QString key;
    QString parent;
    BrowserNode node;
    QString tooltip;
    QIcon icon;
    bool expanded = false;
    bool hasActivation = false;
    bool active = false;
    int bindings = 0; // Same fx count as the shared row's delegate.
    // The name's colour here (mitcad#1079): "failed" (red), "warning",
    // "dimmed" (grey: suppressed, rolled back or hidden) or none.
    QString tone;
    // A light bulb that follows a Show When condition, or a folder's that
    // holds such rows (mitcad#863): its tooltip in place of Hide / Show;
    // `eyeBound`: the row's own condition, the bulb's fx mark.
    QString eyeTip;
    bool eyeBound = false;
    // The Make tab's marker after the name (mitcad#1049): its tooltip,
    // empty without one.
    QString makeTip;
    // A shown transparent body or component (mitcad#1093): the half-open
    // eye. Hidden, its bulb is the closed eye as any other.
    bool transparent = false;
  };
  QVector<PresentationRow> presentationRows() const;
  void setPresentationExpanded(const QString& key, bool expanded);
  // The row under the pointer in another presentation (mitcad#1072: the
  // AppKit outline), an invalid node when it is over none: the same
  // jointHovered and bodiesHovered as the pointer over these rows.
  void hoverNode(const mitcad::BrowserNode& node);

  // Rebuilds the rows, keeping what was expanded, selected and scrolled to.
  void rebuild(const DocumentSnapshot& snapshot, bool originShown);
  // Selects the rows of these items (from the view) and the feature rows
  // of these features (picked in the timeline or here); others are
  // deselected. Emits nothing. With scroll, it scrolls to the newest
  // selected row when none of them is in sight (mitcad#318); a selection
  // picked in the tree itself does not scroll it. With keepPicked, the rows
  // selected in the tree stay selected too, also a folder or the origin
  // that stands for no item: the user's own pick of rows.
  void showSelection(const Selection& items, const QStringList& features = {}, bool scroll = true,
                     bool keepPicked = false);
  QVector<BrowserNode> selectedNodes() const;
  QStringList selectedPresentationKeys() const;
  void setPresentationSelection(const QStringList& keys);
  // The rows of a feature: its sketch or datum, or the bodies it made.
  QVector<BrowserNode> nodesOfFeature(const QString& feature) const;
  // Expands to, selects and scrolls to the rows of a feature, its rows under
  // its sketches too; false when it has none (Find in Browser).
  bool reveal(const QString& feature);
  // The same for the row of a body (of a face, edge or vertex picked in
  // the view), a sketch or a datum.
  bool revealItem(const SelectionItem& item);
  // Collapses every row below the top row, the descendants of a folded row
  // too, and forgets what was open: the remembered states of those rows are
  // "folded", so neither an arrow click (which opens only that row) nor a
  // rebuild reopens a subtree (mitcad#312). The top row (the root) is open,
  // its children in sight and folded (mitcad#426); its own arrow still folds
  // it. Only the presentation changes.
  void collapseAll();
  // A design was opened (mitcad#426): the next rebuild gives the rows their
  // default states (what the design before left open is forgotten). Once the
  // panel shows with its size, a tree with more rows in sight than
  // availableHeight holds opens with the top row open and every row under it
  // folded; a tree that fits stays as it opened. Logged: "Browser opened
  // folded: <n> rows would not fit (<visible> visible)" or "Browser opened
  // expanded: <n> rows (<visible> visible)". Later rebuilds keep the rows as
  // they are.
  void designOpened();
  // The height the rows can have without scrolling, which designOpened fits
  // them to: the tree's viewport (the Docked layout's dock), unless a
  // layout whose panel grows with its rows gives its largest height here
  // (the Floating layout: FloatingLayout::maximumContentHeight; a negative
  // height, before its first layout, falls back to the viewport).
  int availableHeight() const;
  void setAvailableHeight(std::function<int()> height);
  // Renames a row in place: in these rows' editor, or in another
  // presentation's when one is set (mitcad#1356: the AppKit outline). The
  // presentation gets the row's presentation key (PresentationRow::key) and
  // returns false when it shows no such editable row. Logged "Renaming
  // <path>" when an editor opens.
  void startRename(const BrowserNode& node);
  void setPresentationRenamer(std::function<bool(const QString& key)> renamer) {
    m_presentationRenamer = std::move(renamer);
  }
  // Closes an open rename editor without renaming; false when none is open.
  bool cancelEditing();
  // Logs where the rows are, for UI tests (when it changed): "Browser <path>
  // at x,y" for a row whose middle is in sight (also "Browser eye ...",
  // "Browser radio ..."), and "<the same> out of sight" once, for a place
  // logged before whose row has left it (scrolled away, folded, gone), so
  // that a test never clicks a place that is not valid any more. Logged again
  // when the tree scrolls and when the panel moves in its window.
  void logLayout();
  // The height of all the rows in sight (a floating card is as high as
  // they, up to a share of the view).
  int contentHeight() const;
  // A compact width, which the dock opens with (mitcad#10).
  QSize sizeHint() const override;

  // The Rows background of the Floating layout (mitcad#419,
  // BrowserBackground.hpp): no card behind the tree; each row in sight on a
  // small box of the card's material, from its arrow to the end of its name
  // or radio button, the selection and the pointer shown on the box. A place
  // beside the rows is logged for UI tests ("Browser view beside the rows at
  // x,y", logLayout).
  void setRowsBackground(bool rows);
  bool rowsBackground() const { return m_rows; }
  // With the Rows background, where the panel takes the mouse, in its own
  // coordinates: the rows' boxes in sight, the scroll bars shown and an
  // open rename editor.
  QRegion rowsRegion() const;
  // The Rows background's vertical scroll bar at the left edge, beside the
  // rows' start (mitcad#425), instead of the tree's at the right: a bar of
  // the panel's own that mirrors the tree's, shown while the rows overflow
  // (the tree moves right of it); in rowsRegion. Logged with the rows
  // ("Browser scroll bar at x,y", either bar).
  void setScrollBarOnLeft(bool left);
  // A bar above the rows, which the panel takes: its height counts in
  // contentHeight while it is shown, and with the Rows background it is in
  // rowsRegion on a box of its own. (The Configuration bar of mitcad#807
  // was one; it is a panel of its own since mitcad#1020.)
  void setTopBar(QWidget* bar);
  // The Make tab's markers (mitcad#1049): a small dot after the name of the
  // rows of these bodies (by uid, every occurrence's row), whose tooltip is
  // the text given (what of the body's material and print settings differs
  // from its component's). Kept over rebuilds; none to take them away.
  // Logged when they change: "Browser Make markers: Root/Bodies/Body1:
  // Own print settings: Walls", "Browser Make markers: none".
  void setMakeMarkers(const QHash<QString, QString>& markers);
  // What the view found of a kept analysis it shows (mitcad#1230), by the
  // analysis' name: a warning (the Section Analysis shows bodies whole
  // because their clip failed) gives its row the warning's badge and
  // colour, and the text is its tooltip; a note that is no warning (its
  // plane cuts no shown body) only its tooltip. An analysis whose plane
  // cannot be found stays an error. Kept over rebuilds; an empty map takes
  // them away. Logged when a row's note changes: "Browser analysis note
  // Root/Analysis/Section1: warning <text>" (the text's lines joined by
  // " | "), "...: none".
  struct AnalysisNote {
    bool warning = false;
    QString text;
    bool operator==(const AnalysisNote& other) const { return warning == other.warning && text == other.text; }
    bool operator!=(const AnalysisNote& other) const { return !(*this == other); }
  };
  void setAnalysisNotes(const QHash<QString, AnalysisNote>& notes);

signals:
  // With the Rows background, rowsRegion changed (the rows scrolled, opened,
  // folded, were rebuilt, renamed or selected; the panel was resized).
  void rowsRegionChanged();
  // The rows in sight changed in number.
  void contentHeightChanged();
  // Rows were selected: the items of the rows that stand for one, and the
  // features of the feature rows under sketches (mitcad#177).
  void rowsPicked(const mitcad::Selection& items, const QStringList& features);
  // The items of rows another presentation selected.
  void itemsPicked(const mitcad::Selection& items);
  void eyeClicked(const mitcad::BrowserNode& node);
  void activateClicked(const mitcad::BrowserNode& node);
  void renamed(const mitcad::BrowserNode& node, const QString& name);
  // A slow second click on a selected row that can be renamed (mitcad#177).
  void renameClicked(const mitcad::BrowserNode& node);
  void doubleClicked(const mitcad::BrowserNode& node);
  void deleteRequested(const QVector<mitcad::BrowserNode>& nodes);
  void contextMenuRequested(const mitcad::BrowserNode& node, const QPoint& globalPosition);
  // Ctrl+F (Cmd+F on macOS) in the rows: the timeline's search (mitcad#179).
  void searchRequested();
  // The pointer came over a joint's, as-built joint's or constraint set's
  // row (mitcad#523), placed by `occurrence`; an empty uid when it left
  // the joint rows.
  void jointHovered(const QString& uid, const QString& occurrence);
  // The pointer came over a body's, a component's or an occurrence's row
  // (mitcad#1022), or left them (an invalid node: another row, or out of
  // the rows). The root's row stands for no bodies of its own here.
  void bodiesHovered(const mitcad::BrowserNode& node);
  // The pointer came over a feature's, a sketch's or a construction
  // feature's row, or left them (an empty uid; mitcad#1327: the view paints
  // the places its notices name).
  void featureHovered(const QString& uid);
  // The fx badge of a bound row (mitcad#862): the pointer rests on it
  // (`pinned` false) or it was clicked; `anchor` is the badge in global
  // coordinates. The controller shows the row's bindings card.
  void bindingsRequested(const mitcad::BrowserNode& node, const QRect& anchor, bool pinned);
  // The rows' looks changed in place, without a rebuild (mitcad#1352): the
  // Make markers (setMakeMarkers) or an analysis row's note
  // (setAnalysisNotes). Another presentation reads presentationRows again.
  void rowsRestyled();

protected:
  bool eventFilter(QObject* watched, QEvent* event) override;
  // The theme changed (mitcad#1104): the rows painted again in its colours
  // and their name colours logged (logRowColours).
  void changeEvent(QEvent* event) override;

private:
  friend class BrowserTree;

  // With the Rows background: rowsRegionChanged once the changes in hand
  // are laid out.
  void queueRowsRegion();
  // designOpened: the fit, once the panel shows with its size.
  void queueFit();
  void fitOpenedDesign();
  // Folds every row below the top row, which is opened; remembered so.
  void foldBelowTop();
  // The rows in sight: those whose parents are all open; and a row's height.
  int shownRows() const;
  int rowHeight() const;

  // The features under each sketch (mitcad#177), by the sketch's uid, and
  // the components each feature's bodies are in, by the feature's uid.
  struct SketchUses {
    QHash<QString, QVector<const DocumentSnapshot::Feature*>> features;
    QHash<QString, QSet<QString>> resultComponents;
  };

  QTreeWidgetItem* addRow(QTreeWidgetItem* parent, const BrowserNode& node, const QString& icon,
                          const QString& tooltip = QString());
  void addComponentRows(QTreeWidgetItem* parent, const DocumentSnapshot& snapshot,
                        const QString& component, const QString& occurrence, bool shown,
                        const SketchUses& uses);
  // The rows of the features that use a sketch, under its row (mitcad#177).
  void addFeatureRows(QTreeWidgetItem* parent, const DocumentSnapshot& snapshot,
                      const DocumentSnapshot::Feature& sketch, const QString& occurrence, bool shown,
                      const SketchUses& uses);
  // A sheet-metal body's rules and Flat Pattern under its row (mitcad#210).
  void addSheetRows(QTreeWidgetItem* bodyRow, const DocumentSnapshot& snapshot, const BrowserNode& body,
                    bool shown);
  // A constraint set's rows under its row (mitcad#148).
  void addConstraintRows(const DocumentSnapshot& snapshot, const DocumentSnapshot::Feature& feature,
                         QTreeWidgetItem* parent, bool shown);
  // A failed feature's or a warned one's row (mitcad#1040): its level's
  // badge on its icon, its name in the level's colour and the level's word
  // in its tooltip; logged ("Browser level <path>: warning") when it
  // changes. False for a feature with neither.
  bool markLevel(QTreeWidgetItem* row, const DocumentSnapshot::Feature& feature);
  BrowserNode nodeOf(const QTreeWidgetItem* item) const;
  QString keyOf(const BrowserNode& node) const;
  // The markers of setMakeMarkers on the rows there are (mitcad#1049).
  void applyMakeMarkers();
  QHash<QString, QString> m_makeMarkers; // by body uid
  QString m_loggedMakeMarkers;           // the markers' log line, as last logged
  // An analysis row as its entry of the `analyses` query and its note
  // (setAnalysisNotes) give it: icon, tooltip and name colour (mitcad#1230).
  void styleAnalysisRow(QTreeWidgetItem* item, const QJsonObject& analysis);
  // The notes of setAnalysisNotes on the analysis rows there are.
  void applyAnalysisNotes();
  QHash<QString, AnalysisNote> m_analysisNotes;   // by analysis name
  QHash<QString, QJsonObject> m_analysisRows;     // the rows' analyses, by name
  QHash<QString, QString> m_loggedAnalysisNotes;  // the notes last logged, by path
  // Logs the rows whose names have a colour role, by role, with the colour
  // it gives on the rows' palette now (mitcad#1104), when it changed:
  // "Browser row colours: dark, hidden #d0d4da: Root/Bodies/Body1, Root/Origin".
  // A value that is no level's role is logged as "unknown <n>: <paths>"
  // (mitcad#1143), so that a test sees a wrong role.
  void logRowColours();
  QString m_loggedRowColours; // its line, as last logged
  bool revealKeys(const QSet<QString>& keys);
  void selectionChanged();
  void itemEdited(QTreeWidgetItem* item, int column);
  void logSelection();

  BrowserTree* m_tree = nullptr;
  QHash<QTreeWidgetItem*, BrowserNode> m_nodes;
  QHash<QString, bool> m_expanded; // by row key
  QHash<QString, QString> m_loggedLevels; // the rows' levels last logged, by path (mitcad#1040)
  // The rows whose transparent eye was logged last, by path (mitcad#1093).
  QSet<QString> m_loggedTransparent;
  QSet<QString> m_shownSelection;  // what showSelection last selected (items and features)
  QSet<QString> m_foldedSelection; // the selection Collapse All left; showSelection does not reveal it again
  bool m_building = false;
  bool m_originShown = false;        // the Origin folder's light bulb, as last rebuilt
  bool m_originsKindShown = true;    // View › Show's Origins (mitcad#1287), as last rebuilt
  // The origin's rows greyed by its light bulb or View › Show.
  void styleOriginRows(QTreeWidgetItem* folder);
  QString m_logged;
  QSet<QString> m_loggedPlaces; // what logLayout logged a place for, and has not logged out of sight
  QString m_loggedSelection;
  bool m_rows = false; // the Rows background (mitcad#419)
  bool m_rowsQueued = false;
  // setScrollBarOnLeft (mitcad#425): the bar at the left, and its place.
  void placeLeftScrollBar();
  bool m_scrollBarLeft = false;
  QScrollBar* m_leftBar = nullptr;
  bool m_fitPending = false; // designOpened, until the panel showed with its size
  bool m_fitQueued = false;
  std::function<int()> m_availableHeight; // setAvailableHeight
  std::function<bool(const QString&)> m_presentationRenamer; // setPresentationRenamer
  // The joint row under the pointer (mitcad#523): jointHovered when it
  // changes ("uid@occurrence", empty: none).
  void hoverJoint(const BrowserNode& node);
  QString m_hoveredJoint;
  // The body, component or occurrence row under the pointer (mitcad#1022):
  // bodiesHovered when it changes (its key, empty: none).
  void hoverBodies(const BrowserNode& node);
  QString m_hoveredBodies;
  // The feature row under the pointer (mitcad#1327): featureHovered when
  // it changes (its uid, empty: none).
  void hoverFeature(const BrowserNode& node);
  QString m_hoveredFeature;
  QWidget* m_topBar = nullptr; // setTopBar (mitcad#807)
  // The pointer resting on a row's fx badge (mitcad#862): after a moment
  // bindingsRequested (not pinned). The row's key, empty: on no badge.
  void hoverBindings(QTreeWidgetItem* item, const QPoint& at);
  void bindingsRested();
  QString m_bindingsHover;
  QTimer* m_bindingsTimer = nullptr;
};

} // namespace mitcad
