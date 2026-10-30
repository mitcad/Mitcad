// SPDX-License-Identifier: MIT
#pragma once

#include <QObject>
#include <QPoint>
#include <QPointer>
#include <QRect>
#include <QString>
#include <QStringList>

#include "../framework/HybridAssembly.hpp"
#include "BindingList.hpp"
#include "BrowserPanel.hpp"
#include "DocumentSnapshot.hpp"
#include "FindInDesign.hpp"
#include "../print/MaterialsDialog.hpp"
#include "../print/MyLibraryDialog.hpp"
#include "../print/OwnSettingsDialog.hpp"
#include "../print/ProfilesDialog.hpp"
#include "../print/PurgeVolumesDialog.hpp"
#include "PropertySetsDialog.hpp"

class QEvent;
class QToolButton;
class QWidget;

namespace mitcad {

class AppearanceDialog;
class BindingsCard;
class ConfigurationBar;
class ConfigurationsDialog;
class FindDialog;
class DocumentHost;
class ParametersDialog;
class TimelineWidget;

// What the browser, the timeline, the parameters dialog and the status
// bar's list of failed features do (U3): their menus and the model commands
// behind them, the history marker's drags as one undo step, and keeping
// them in step with the model (refresh) and the selection (showSelection).
class BrowserController : public QObject {
  Q_OBJECT

public:
  BrowserController(DocumentHost& host, QWidget* window);

  BrowserPanel* browser() const { return m_browser; }
  TimelineWidget* timeline() const { return m_timeline; }
  bool markerDragActive() const { return m_markerDepth >= 0; }
  // Exact feature context while the controller passes a timeline or feature-row
  // pick to the host. A body-row or any independent view pick has no context.
  QStringList pickingFeatures() const { return m_picking ? m_pickedFeatures : QStringList(); }
  // The status bar's summary of failed features; a click goes to them.
  QToolButton* failures() const { return m_failures; }
  // Beside it, yellow: features with warnings, the import's fallbacks,
  // partial items and placeholders among them (mitcad#170); a click goes
  // to them.
  QToolButton* warnings() const { return m_warnings; }
  // The warnings' button shows only their count beside its triangle (a
  // narrow card's header, mitcad#961); its tooltip and accessible name
  // say the rest.
  void setCompactBadges(bool compact) { m_compactBadges = compact; }
  // Shared issue navigation, anchored by either frontend's actual badge.
  void showIssues(bool warnings, const QPoint& globalPosition);

  // After the model or the origin's visibility changed.
  void refresh(const DocumentSnapshot& snapshot);
  // A design was opened (mitcad#426), before its refresh: the browser is
  // rebuilt with the rows' default states, folded when they would not fit
  // (BrowserPanel::designOpened).
  void designOpened();
  // Whether the row's light bulb works in every mode, during commands and
  // while Create Sketch asks for a plane too (mitcad#295); the others only
  // when the model can change.
  static bool visibilityInEveryMode(const BrowserNode& node);
  // The selection outside commands changed (in the view or here).
  void showSelection(const Selection& items);
  // Closes an open rename editor; false when none was open.
  bool cancelEditing();
  // Change Parameters.
  void openParameters();
  // Its window once opened, else null (mitcad#564: back to the edit when
  // it closes).
  ParametersDialog* parametersDialog() const;
  // Configurations (mitcad#807): the configuration table's window
  // (Options, Rows, Rules); the window follows the model.
  void openConfigurations();
  ConfigurationsDialog* configurationsDialog() const;
  // The Configuration bar at the top of the browser (mitcad#807).
  ConfigurationBar* configurationBar() const { return m_configurationBar; }
  // Edit Appearances (mitcad#46); Assign gives the targets (bodies, and
  // faces: mitcad#53) the selected appearance, or fills the rows of the
  // option it varies with (the assignment panel, mitcad#861).
  void openAppearances(const Selection& targets);
  // Design Policy (mitcad#220) of a component (C0 the root): its seven
  // settings, presets and violations; OK sets it as one undo step.
  void openPolicy(const QString& component);
  // Edit Options… (mitcad#810) of an inserted component (its uid): its
  // options chosen or bound to the design's parameters; OK keeps them as
  // one undo step (`set_component_parameters`).
  void editOptions(const QString& component, const QString& name);
  // Property Sets (mitcad#496) of a body, an occurrence and its component
  // (the window follows the model; another target replaces the shown one).
  void openPropertySets(const PropertyTarget& target);
  // Materials (mitcad#655; Material... and Edit Materials): Assign gives
  // the bodies (uids) the selected material, or with `component` (a uid)
  // makes it that component's default; neither: no Assign. The window
  // follows the model; the bodies' material can vary with an option there
  // (the assignment panel, mitcad#861).
  // `keepOpen`: Assign leaves the window open (3D Print's Assign materials…).
  void openMaterial(const QStringList& bodies, const QString& component = QString(), bool keepOpen = false);
  // Printers and Processes (mitcad#655; Edit Printers and Processes, Edit…
  // of 3D Print's and Property Sets' Printer and Process rows): the item
  // `key` ("<place>/<id>") of `kind` (printer or process) selected, none
  // for an empty key. The window follows the model.
  void openProfiles(const QString& kind, const QString& key);
  // A body's, an occurrence's or a component's own process settings in
  // the settings form (mitcad#655): `owner` its uid, `name` as the design
  // shows it (3D Print's own-settings links, Property Sets' Edit… of
  // Process).
  void openOwnSettings(const QString& owner, const QString& name);
  // My Library (mitcad#704; Tools › My Library…, My Library… of Materials
  // and Printers and Processes): the user's and the project's materials,
  // printers and processes without a design. When it closes after a
  // change, the open print windows read the libraries again.
  void openMyLibrary();
  // Purge Volumes (mitcad#705; 3D Print's Purge volumes row): the table of
  // the filament slots of `bodies` (uids, in the order printed). The window
  // follows the model.
  void openPurgeVolumes(const QStringList& bodies);
  // Reorient Design (mitcad#502): the whole design turned by quarter turns,
  // previewed while its dialog is open; OK keeps it as one undo step.
  void openReorient();
  // Find in Browser for an item picked in the view (a face finds its body).
  void findInBrowser(const SelectionItem& item);
  // The view's context menu Hide submenu (mitcad#347): the bodies (uids) and
  // the occurrences (paths) of the faces, edges, vertices and bodies of the
  // selection, each once; no occurrence for the root component's bodies.
  static QStringList hideableBodies(const Selection& items);
  static QStringList hideableOccurrences(const Selection& items);
  // Hide Body and Hide Component: the same visibility commands as the light
  // bulbs, one undo step.
  void hideBodies(const Selection& items);
  void hideOccurrences(const Selection& items);
  // The view's context menu Delete submenu (mitcad#494), each one undo
  // step: Delete Body takes the bodies out (a remove_body feature in each
  // body's component), Delete Component deletes the occurrences, and a
  // feature is deleted as the browser's Delete does (the features that use
  // it too, when the user says so).
  void deleteBodies(const Selection& items);
  void deleteOccurrences(const Selection& items);
  void deleteFeature(const QString& uid);
  // Find in Design (mitcad#350): the window that finds components and
  // bodies by name and shows, hides, selects or gives them an appearance
  // all at once; opened by the magnifier button (on the browser's title
  // row) or the command.
  QToolButton* findButton() const { return m_findButton; }
  void openFind();
  // Bindings (mitcad#809; Bindings.hpp): Suppress When… of a feature and
  // Show When… of a body or an occurrence (its path, "O1/O4"). Each sets
  // the expression the dialog gives (one undo step), or removes it. An
  // appearance or a material varies with an option in the assignment panel
  // of Appearance… and Material… (mitcad#861, AssignmentPanel.hpp).
  void suppressWhen(const QString& feature);
  void showBodyWhen(const QString& body);
  void showOccurrenceWhen(const QString& path);

protected:
  // Logs where the magnifier button is, for UI tests.
  bool eventFilter(QObject* watched, QEvent* event) override;

private:
  // What the window's buttons do with its targets (selected results, or all).
  void findVisibility(bool show);
  void findShow();
  void findHide();
  void findAppearance();
  void findSelect();
  void findPicked();
  void findReveal();
  // The items of results: bodies by uid and occurrence, components by their
  // occurrence.
  static Selection findItems(const QVector<FindResult>& results);
  void logFindButton();

  // Connects a signal of the browser or the timeline to a slot, queued:
  // the slot may change the model, which rebuilds the widget that emitted
  // it. A call that comes while a job computes the model (P7: posted before
  // the job began, or the end of a drag that the progress dialog cut short)
  // waits until the job is done; a menu is not shown then (dropWhenBusy).
  template <typename Sender, typename... Args>
  void follow(Sender* sender, void (Sender::*signal)(Args...), void (BrowserController::*slot)(Args...),
              bool dropWhenBusy = false);
  void pickItems(const Selection& items);
  // Rows picked in the browser: the items of the rows, and the features of
  // the feature rows under sketches (mitcad#177). Their features are
  // selected in the timeline too.
  void pickRows(const Selection& items, const QStringList& features);
  void pickRowsOfTree(const Selection& items, const QStringList& features);
  // Selects the items; the browser shows the rows of these features too.
  void pick(const Selection& items, const QStringList& features);
  // What selecting features selects: their sketches, datums or bodies, or
  // the features themselves (a command that takes features; one that
  // made no body).
  Selection featureItems(const QStringList& uids) const;
  void editFeature(const QString& uid);
  void markerDragged(int position);
  void browserMenu(const BrowserNode& node, const QPoint& at);
  // What a body row's Appearance… and Material… are for (mitcad#703): every
  // selected body when the row is one of them, else the row's body alone.
  QStringList menuBodies(const BrowserNode& node) const;
  // What a body's or an occurrence's row starts Move/Copy with (mitcad#735):
  // the selected items of the row's kind when the row is one of them, else
  // the row's item alone.
  Selection menuMoveItems(const BrowserNode& node) const;
  void timelineMenu(const QString& uid, const QPoint& at);
  // A feature's menu, the same in the timeline and on its rows under
  // sketches in the browser (mitcad#177): `row` is the browser's row, or
  // none in the timeline.
  void featureMenu(const QString& uid, const QPoint& at, const BrowserNode& row);
  void findInTimeline(const QString& uid);
  // The timeline's search was cleared (mitcad#179): the selection from
  // before it again.
  void restoreSearchSelection();
  // A slow second click on a browser row (mitcad#177).
  void renameClicked(const BrowserNode& node);
  // Timeline groups (P9): a group's menu, and grouping features.
  void groupMenu(const QString& name, const QPoint& at);
  void groupFeatures(const QStringList& uids);
  void browserDoubleClicked(const BrowserNode& node);
  void toggleVisibility(const BrowserNode& node);
  // The names of a component's bodies that follow a Show When condition,
  // which the Bodies folder's light bulb and Show All / Hide All leave as
  // they are (mitcad#863), and what the folder says when they are all it
  // holds.
  QStringList conditionedBodies(const QString& owner) const;
  static QString bodiesFollowConditions(const QStringList& bodies);
  // "Body2 follows its Show When and is left as it is." (mitcad#863): the
  // tooltip of Show All / Hide All of a folder that holds such bodies.
  static QString leftToConditions(const QStringList& bodies);
  // An occurrence's Show When condition by its path, or empty.
  QString occurrenceCondition(const QString& path) const;
  // One item set by hand lost its condition: the status bar says so.
  void conditionRemoved(const QString& name);
  // A bulk change (a folder, several items of the view's Hide, Find in
  // Design's Show or Hide) left items with conditions to them: logged
  // ("<label>[ <where>]: kept 1 following Show When: Body2") and said in
  // the status bar.
  void keptByConditions(const QString& label, const QString& where, const QStringList& kept);
  void activate(const BrowserNode& node);
  void rename(const BrowserNode& node, const QString& name);
  void renameFeature(const QString& uid, const QString& name);
  void deleteNode(const BrowserNode& node);
  // The Delete key in the browser and the timeline: as deleteNode and
  // deleteFeature, except of model items while a drawing is open.
  bool keyDeletesModel();
  void deleteKeyed(const BrowserNode& node);
  void deleteFeatureKeyed(const QString& uid);
  // Opens Flat Pattern on a sheet-metal body: its settings, or a new one
  // (mitcad#210).
  void editFlatPattern(const QString& body);
  void featuresClicked(const QStringList& uids);
  void reorder(const QString& uid, int index);
  void setMarker(int position);
  void markerDragFinished(int position);
  void newComponent(const BrowserNode& node);
  // Hybrid assemblies (mitcad#171): the question before a component is made
  // in a root with modelled bodies, and Move to Component.
  HybridModel hybridModel();
  void componentsFromBodies(const BrowserNode& node);
  void moveToComponent(const QJsonObject& selection);
  // Move to... on an occurrence: another parent for it (mitcad#219).
  void moveOccurrence(const BrowserNode& node);
  void moveHistory(const QString& uid);
  void paste(bool asNew);
  void changeUnits();
  void reveal(const QString& uid);
  void updateFailures();
  bool canChange(bool visibilityOnly = false);
  const DocumentSnapshot::Occurrence* occurrence(const QString& path) const;
  int undoDepth() const;

  DocumentHost& m_host;
  QWidget* m_window;
  BrowserPanel* m_browser = nullptr;
  TimelineWidget* m_timeline = nullptr;
  QToolButton* m_failures = nullptr;
  QToolButton* m_warnings = nullptr;
  bool m_compactBadges = false;
  QPointer<ParametersDialog> m_parameters;
  QPointer<ConfigurationsDialog> m_configurations; // mitcad#807
  ConfigurationBar* m_configurationBar = nullptr;   // mitcad#807
  QPointer<AppearanceDialog> m_appearances;
  QPointer<PropertySetsDialog> m_propertySets;
  QPointer<MaterialsDialog> m_materials;     // mitcad#655
  QPointer<ProfilesDialog> m_profiles;       // mitcad#655
  QPointer<OwnSettingsDialog> m_ownSettings; // mitcad#655
  QPointer<MyLibraryDialog> m_myLibrary;     // mitcad#704
  QPointer<PurgeVolumesDialog> m_purgeVolumes; // mitcad#705
  QPointer<FindDialog> m_find;
  QToolButton* m_findButton = nullptr;
  QString m_loggedFindButton;
  DocumentSnapshot m_snapshot;
  bool m_shown = false;      // something was shown
  bool m_originShown = false; // as the browser shows it
  QString m_clipboard;        // a copied occurrence's path
  QString m_clipboardName;
  int m_markerDepth = -1; // undo depth when a marker drag began
  int m_markerStart = 0;
  // While a pick of features runs: the features whose browser rows show it.
  bool m_picking = false;
  bool m_fromTree = false; // the pick came from the browser's rows (mitcad#318)
  QStringList m_pickedFeatures;
  // The selection shown in the browser, and the features whose rows show it.
  Selection m_shownItems;
  QStringList m_shownFeatures;
  // What was selected when the timeline's search began (mitcad#179).
  Selection m_searchItems;
  QStringList m_searchFeatures;
  QString m_loggedFailures;
  QString m_loggedWarnings;
  QString m_loggedBadge; // the warnings badge (mitcad#170)
  QString m_loggedDof;
  QString m_loggedJoints;

  // Bindings at a glance (mitcad#862): the fx card of a browser row or a
  // timeline feature, and what its Edit… and the Configurations window's
  // Bindings tab do (BindingsCard.hpp, ConfigurationBindingsPage.hpp).
  void browserBindings(const BrowserNode& node, const QRect& anchor, bool pinned);
  void timelineBindings(const QString& uid, const QRect& anchor, bool pinned);
  void showBindingsCard(const BindingItem& item, const QRect& anchor, bool pinned);
  // The editor of a property: the Appearance window for a body or a face,
  // Materials for a body, Show When… or Suppress When….
  void editBinding(const BindingEntry& entry);
  // Selects the item and shows its row (Find in Browser; a feature in the
  // timeline too).
  void selectBinding(const BindingEntry& entry);
  void showAllBindings();
  QPointer<BindingsCard> m_bindingsCard;
};

} // namespace mitcad
