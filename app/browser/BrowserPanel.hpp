// SPDX-License-Identifier: MIT
#pragma once

#include <QHash>
#include <QPoint>
#include <QSet>
#include <QString>
#include <QWidget>

#include "../framework/Selection.hpp"
#include "DocumentSnapshot.hpp"

class QTreeWidgetItem;

namespace mitcad {

class BrowserTree;

// A row of the browser.
struct BrowserNode {
  enum class Type {
    None,
    Settings,   // Document Settings
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
  };

  Type type = Type::None;
  QString uid;
  QString occurrence; // the occurrence path ("O1/O4"); empty in the root
  QString component;  // the component the row belongs to
  QString featureType; // sketches and datums: construction_plane, ...
  QString name;       // as shown
  QString path;       // the names from the root, "Root/Bodies/Body1"
  bool visible = true; // its light bulb
  bool hasEye = false;

  bool isValid() const { return type != Type::None; }
  bool isFolder() const {
    return type == Type::Bodies || type == Type::Sketches || type == Type::Construction ||
           type == Type::Origin;
  }
  // The selection item the row stands for; invalid for folders and settings.
  SelectionItem item() const;
};

// The browser, docked on the left: the document settings, named
// views, the origin and the component tree with each occurrence's bodies,
// sketches and construction geometry. A row reads: expand arrow, light
// bulb (an eye; shows and hides), icon, name and, for a component, the
// radio button that activates it. Selecting rows selects in the view and
// the other way round; what the rows do is the BrowserController's.
class BrowserPanel : public QWidget {
  Q_OBJECT

public:
  explicit BrowserPanel(QWidget* parent = nullptr);

  // Rebuilds the rows, keeping what was expanded, selected and scrolled to.
  void rebuild(const DocumentSnapshot& snapshot, bool originShown);
  // Selects the rows of these items (from the view); others are deselected.
  void showSelection(const Selection& items);
  // The rows of a feature: its sketch or datum, or the bodies it made.
  QVector<BrowserNode> nodesOfFeature(const QString& feature) const;
  // Expands to, selects and scrolls to the rows of a feature; false when
  // it has none (Find in Browser).
  bool reveal(const QString& feature);
  // The same for the row of a body (of a face, edge or vertex picked in
  // the view), a sketch or a datum.
  bool revealItem(const SelectionItem& item);
  // Renames a row in place.
  void startRename(const BrowserNode& node);
  // Closes an open rename editor without renaming; false when none is open.
  bool cancelEditing();
  // Logs where the rows are, for UI tests (when it changed).
  void logLayout();
  // The height of all the rows in sight (a floating card is as high as
  // they, up to a share of the view).
  int contentHeight() const;
  // A compact width, which the dock opens with (mitcad#10).
  QSize sizeHint() const override;

signals:
  // The rows in sight changed in number.
  void contentHeightChanged();
  void itemsPicked(const mitcad::Selection& items);
  void eyeClicked(const mitcad::BrowserNode& node);
  void activateClicked(const mitcad::BrowserNode& node);
  void renamed(const mitcad::BrowserNode& node, const QString& name);
  void doubleClicked(const mitcad::BrowserNode& node);
  void deleteRequested(const mitcad::BrowserNode& node);
  void contextMenuRequested(const mitcad::BrowserNode& node, const QPoint& globalPosition);

private:
  friend class BrowserTree;

  QTreeWidgetItem* addRow(QTreeWidgetItem* parent, const BrowserNode& node, const QString& icon,
                          const QString& tooltip = QString());
  void addComponentRows(QTreeWidgetItem* parent, const DocumentSnapshot& snapshot,
                        const QString& component, const QString& occurrence, bool shown);
  BrowserNode nodeOf(const QTreeWidgetItem* item) const;
  QString keyOf(const BrowserNode& node) const;
  bool revealKeys(const QSet<QString>& keys);
  void selectionChanged();
  void itemEdited(QTreeWidgetItem* item, int column);
  void logSelection();

  BrowserTree* m_tree = nullptr;
  QHash<QTreeWidgetItem*, BrowserNode> m_nodes;
  QHash<QString, bool> m_expanded; // by row key
  bool m_building = false;
  QString m_logged;
  QString m_loggedSelection;
};

} // namespace mitcad
