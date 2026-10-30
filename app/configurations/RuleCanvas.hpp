// SPDX-License-Identifier: MIT
#pragma once

#include <functional>
#include <optional>
#include <vector>

#include <QGraphicsView>
#include <QPointer>
#include <QString>
#include <QStringList>
#include <QTreeWidget>

#include "../framework/ParameterSuggest.hpp"
#include "RuleBlocks.hpp"

class QGraphicsItem;
class QGraphicsScene;
class QLineEdit;
class QMenu;

namespace mitcad::rules {

// A place on the canvas: a block, a socket, a slot for an action, a field
// of a block, a section's word or the rule's frame. Logged by its name for
// the UI tests ("Rule canvas when.0 socket at x,y").
struct Piece {
  enum class Role {
    Frame,  // the rule's frame
    Label,  // its when, then and else words
    Block,  // a value or action block
    Socket, // an empty value socket
    Add,    // the socket at the end of a list (and, or, is one of, allow only, exclude)
    Slot,   // a place for an action in a then or else
    Field,  // a field of a block: its option, operator, message, table or columns
  };
  Role role = Role::Block;
  BlockPath path;
  bool insert = false; // Add and Slot: a new place in a list
  QString field;       // Field: "option", "op", "message", "table", "columns"; Label: "when", "then", "else"
  QRectF rect;
  BlockKind kind = BlockKind::Empty;
  QString text; // what it shows
  std::vector<std::pair<QPointF, QString>> words; // a block's own words, at offsets in it
  std::vector<QRectF> holes; // Frame: the then and else, cut out of it
  int depth = 0;
  // "when socket", "then.0 block", "then.0 option", "then.1 slot", "when label".
  QString name() const;
  Target target() const { return {path, insert}; }
};

class RuleCanvas;

// The palette (mitcad#808): the blocks in their groups Rule, Logic, Values
// and Actions. A block is dragged from it onto the canvas with the mouse
// (the canvas shows where it would go and puts it there), or chosen with
// the arrow keys and Enter, which puts it at the canvas's focus. Logged:
// "Rule palette <id> at x,y".
class RulePalette : public QTreeWidget {
  Q_OBJECT

public:
  explicit RulePalette(QWidget* parent = nullptr);
  void setCanvas(RuleCanvas* canvas);
  RuleCanvas* canvas() const;
  // The palette entry's item, for tests.
  QTreeWidgetItem* entry(const QString& id) const;
  void logPlaces();

protected:
  void mousePressEvent(QMouseEvent* event) override;
  void mouseMoveEvent(QMouseEvent* event) override;
  void mouseReleaseEvent(QMouseEvent* event) override;
  void keyPressEvent(QKeyEvent* event) override;
  void resizeEvent(QResizeEvent* event) override;
  void showEvent(QShowEvent* event) override;

private:
  QString idAt(const QPoint& at) const;

  QPointer<RuleCanvas> m_canvas;
  QString m_pressed;
  QPoint m_pressAt;
  bool m_dragging = false;
  QString m_logged;
};

// The block canvas of one rule (mitcad#808): a native QGraphicsView that
// lays the rule out as blocks (the rule's frame with its when, then and
// else; value blocks in sockets, action blocks in the then and else). Each
// edit makes a new rule and says so (`edited`); the Rules page saves it.
//
// Mouse: a palette entry or a block is dragged to a place it fits (a value
// block to a value socket, an action block to a then or else slot; the
// place is highlighted on the way), a block dropped on the palette is
// deleted, a double-click edits a block, a click a field. Keyboard: the
// arrows move the focus, Enter edits it (or, on an empty socket or a slot,
// goes to the palette to choose what to put there), Delete deletes, the
// menu key or Shift+F10 opens the context menu. A formula block is edited
// in the Formula Editor (`setFormulaEditor`).
class RuleCanvas : public QGraphicsView {
  Q_OBJECT

public:
  explicit RuleCanvas(QWidget* parent = nullptr);

  // The rule shown; the focus stays on the same place when it is there.
  void setRule(const Rule& rule);
  void clearRule();
  bool hasRule() const { return m_hasRule; }
  const Rule& rule() const { return m_rule; }
  // The options (in choice order) and those of them that are numbers, for
  // new actions and the option fields; the parameters for name blocks.
  void setOptions(const QStringList& options, const QStringList& numberOptions);
  // The options' kinds and domains: an action put from the palette comes
  // with its values filled from them (mitcad#966, `fillSockets`).
  void setOptionInfos(std::vector<OptionInfo> infos);
  // What the canvas says while it shows no rule (mitcad#967): the empty
  // state when the design has no rules.
  void setHint(const QString& hint);
  void setParameters(const QStringList& parameters);
  // The design's tables and their columns (mitcad#849): a "combination in
  // table" block's table is chosen among them (completed as it is typed;
  // choosing one maps each option to its column of the same name), and
  // its columns in a dialog of the table's columns.
  void setTables(std::vector<SuggestedTable> tables);
  // Edits a formula's text (the Formula Editor); without it a formula is
  // edited in place. Returns the new text, or nothing when cancelled.
  void setFormulaEditor(std::function<std::optional<QString>(const QString& text, QWidget* parent)> edit);
  // Where a block dropped on it is deleted (the palette).
  void setDeleteArea(QWidget* area);

  // The palette's entries: put at the focus (Enter in the palette), shown
  // where a drag would put them, dropped.
  bool insertAtFocus(const QString& paletteId);
  void dragOver(const QPoint& global, const QString& paletteId);
  bool dropAt(const QPoint& global, const QString& paletteId);
  // A native scroll view already hit-tests its document in scene coordinates.
  void dragOverScene(const QPointF& scene, const QString& paletteId);
  bool dropAtScene(const QPointF& scene, const QString& paletteId);
  void endDrag();

  // The focus, by its piece's name ("then.0 block").
  QString focusName() const;
  bool focusPiece(const QString& name);
  bool editFocused();
  bool deleteFocused();
  // The pieces as laid out, for tests; the centre of one in the viewport.
  const std::vector<Piece>& pieces() const { return m_pieces; }
  std::optional<QPoint> pieceCenter(const QString& name) const;
  // The same deepest-piece hit test as mouse input: native drag adapters
  // may delete a block on a palette drop only when the press began on one.
  bool isBlockAtScene(const QPointF& scene) const;
  // The open in-place editor and its scene piece, if any. Native adapters
  // use the piece geometry independently of the hidden viewport's offsets.
  QLineEdit* editor() const { return m_editor; }
  QString editingPieceName() const { return m_editing; }
  // Discard the active edit, keeping a newly inserted block's initial value.
  void cancelEditor();
  // A new block waits for its value in its editor before it is saved; this
  // says it is edited (`edited`) as it is now, if one waits.
  void flushPending();
  void logPlaces();

signals:
  // A new rule, after an edit, and what the edit was ("Put compare in when").
  void edited(const mitcad::rules::Rule& rule, const QString& what);
  // Enter on an empty socket or a slot: the palette is to choose for it.
  void paletteRequested();

protected:
  void mousePressEvent(QMouseEvent* event) override;
  void mouseMoveEvent(QMouseEvent* event) override;
  void mouseReleaseEvent(QMouseEvent* event) override;
  void mouseDoubleClickEvent(QMouseEvent* event) override;
  void keyPressEvent(QKeyEvent* event) override;
  void contextMenuEvent(QContextMenuEvent* event) override;
  void resizeEvent(QResizeEvent* event) override;
  void showEvent(QShowEvent* event) override;
  void scrollContentsBy(int dx, int dy) override;
  void focusInEvent(QFocusEvent* event) override;
  void changeEvent(QEvent* event) override;
  bool eventFilter(QObject* watched, QEvent* event) override;

private:
  void rebuild();
  bool takeHighlightedDrop(const QString& paletteId);
  void apply(Rule changed, const QString& what, const std::optional<BlockPath>& focus = std::nullopt,
             bool editNew = false);
  int pieceAt(const QPointF& scene) const;
  int pieceIndex(const QString& name) const;
  // Where a block of the kind (a palette entry, or the block at `from`)
  // would go when let go at a scene point; -1 when nowhere.
  int targetAt(const QPointF& scene, BlockKind kind, const std::optional<BlockPath>& from) const;
  void setHighlight(int index);
  void setFocusIndex(int index);
  void moveFocus(int key);
  bool put(const QString& paletteId, const Target& target);
  void editPiece(int index);
  void openEditor(int index);
  void commitEditor();
  void closeEditor();
  void chooseOperator(int index);
  void chooseColumns(int index);
  // A "combination in table" block's columns for its table, when the table
  // is one of the design's; false when it is none.
  bool mapColumns(Block& block) const;
  void editFormula(const BlockPath& path);
  bool deletePiece(int index);
  void showMenu(int index, const QPoint& global);
  QString optionFor(const QString& paletteId) const;
  BlockKind kindOf(const QString& paletteId) const;
  QWidget* logWindow() const;

  int findPath(const BlockPath& path) const;
  void updateItems();

  QGraphicsScene* m_scene = nullptr;
  std::vector<QGraphicsItem*> m_items; // one per piece
  Rule m_rule;
  bool m_hasRule = false;
  std::vector<Piece> m_pieces;
  int m_focus = -1;
  QString m_focusName;
  int m_highlight = -1;
  QStringList m_options;
  QStringList m_numberOptions;
  std::vector<OptionInfo> m_infos;
  QString m_hint = QStringLiteral("Add a rule, or choose one in the list.");
  QStringList m_parameters;
  std::vector<SuggestedTable> m_tables;
  std::function<std::optional<QString>(const QString&, QWidget*)> m_formulaEditor;
  QPointer<QWidget> m_deleteArea;
  // The mouse: the piece pressed, where, and the block dragged.
  int m_pressed = -1;
  QPoint m_pressAt;
  std::optional<BlockPath> m_dragFrom;
  // The in-place editor and the piece it edits.
  QLineEdit* m_editor = nullptr;
  QString m_editing;
  bool m_editorClosing = false;
  QString m_logged;
  QString m_loggedFocus;
  // The edit that put a new block whose editor is open, not said yet.
  QString m_pending;
};

} // namespace mitcad::rules
