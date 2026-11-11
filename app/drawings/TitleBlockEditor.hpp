// SPDX-License-Identifier: MIT
#pragma once

// The title block editor (mitcad#1625, app/COMMANDS.md "Drawings"): a
// sheet's title block edited on the sheet. It starts from the `drawing`
// query's `title_block_layout` (the sheet's template's block, the ISO
// template's for the simple block), `title_box` and `frame`, keeps the
// block's lines, fields and images while they are drawn, moved and
// deleted, and gives the block back as `edit_drawing_title_block` takes it
// (core/model-api/src/commands.md, "Drawing templates and revisions"):
// Done in the workspace sends it, Cancel drops it. Points come in sheet
// millimetres and go to the block's own (from its lower left corner), on
// its grid and inside it. Plain Qt Core, so that the tests check it
// without a window; DrawingSheetView paints it.

#include <optional>
#include <utility>

#include <QByteArray>
#include <QJsonObject>
#include <QLineF>
#include <QPointF>
#include <QRectF>
#include <QString>
#include <QVector>

namespace mitcad::drawings {

// A cell of the block: its label, its text with {name}s, its box (mm from
// the block's lower left corner, y up: `y()` is its bottom) and the text's
// height (0: the model's 3.5 mm).
struct BlockField {
  QString label;
  QString text;
  QRectF cell;
  double height = 0.0;
};

// An image of the block (a logo): its box as `BlockField::cell`, the
// file's bytes as base64 and what they are ("png", "jpeg", "svg").
struct BlockImage {
  QRectF box;
  QString data;
  QString format;
};

class TitleBlockEditor {
public:
  enum class Tool { Select, Line, Field, Image };
  enum class Kind { None, Line, Field, Image };

  // The editor of a `drawing` query's sheet; none when the sheet has no
  // layout to edit (a cut sheet).
  static std::optional<TitleBlockEditor> fromSheet(const QJsonObject& sheet);
  // The block as `edit_drawing_title_block`'s `title_block` takes it: the
  // layout it started from with its size, lines, fields and images
  // replaced (its projection symbol kept).
  QJsonObject toJson() const;

  double width() const { return m_width; }
  double height() const { return m_height; }
  // The block's size, mm (each at least 5): it stays in the frame's lower
  // right corner, so its left edge moves.
  void setSize(double width, double height);
  // The block's lower left corner on the sheet.
  QPointF origin() const;
  QPointF toBlock(const QPointF& sheet) const { return sheet - origin(); }
  QPointF toSheet(const QPointF& block) const { return block + origin(); }
  // The grid points snap to, mm (more than 0).
  double grid() const { return m_grid; }
  void setGrid(double grid);
  // A sheet point in the block's own millimetres, on the grid, inside the
  // block.
  QPointF snapped(const QPointF& sheet) const;

  Tool tool() const { return m_tool; }
  // Another tool: a first click of the one before is dropped.
  void setTool(Tool tool);
  // The image the Image tool places with its next two clicks (its box's
  // corners): the file as base64 and what it is.
  void setPendingImage(const QString& base64, const QString& format);
  bool hasPendingImage() const { return !m_pendingData.isEmpty(); }

  // A press of the left button on the sheet (sheet mm); `reach` is how far
  // from a line a press still picks it (mm). The Select tool picks what is
  // there (a line, an image, the smallest field) and starts its move; the
  // other tools take the press as a corner or an end, their second one
  // making what they make. What happened, for the log ("" for nothing).
  QString press(const QPointF& sheet, double reach);
  // The pointer at a sheet point: a move under way follows it (on the
  // grid), a tool's first click shows its line or box to it.
  void moveTo(const QPointF& sheet);
  // The button released: a move ends. What moved, for the log ("" for
  // nothing).
  QString release(const QPointF& sheet);
  bool moving() const { return m_moving; }
  // Esc: a first click dropped, else the tool back to Select; false when
  // there was nothing to end.
  bool escape();
  // Deletes what is selected; what went, for the log ("" for nothing).
  QString deleteSelected();

  Kind selectedKind() const { return m_selectedKind; }
  int selectedIndex() const { return m_selectedIndex; }
  void select(Kind kind, int index);
  const QVector<QLineF>& lines() const { return m_lines; }
  const QVector<BlockField>& fields() const { return m_fields; }
  const QVector<BlockImage>& images() const { return m_images; }
  // The selected field's label, text and text height (0: the model's).
  bool setFieldLabel(const QString& label);
  bool setFieldText(const QString& text);
  bool setFieldHeight(double height);

  // The tool's first click (block mm), none before it.
  std::optional<QPointF> firstPoint() const { return m_first; }
  // The pointer, block mm, on the grid.
  QPointF pointer() const { return m_pointer; }
  // Whether anything changed since it started.
  bool changed() const { return m_changed; }
  // "3 lines, 9 fields, 1 image".
  QString summary() const;

private:
  QPointF clampedPoint(const QPointF& block) const;
  QRectF boxOf(const QPointF& a, const QPointF& b) const;
  // The thing under a block point; Kind::None for nothing.
  std::pair<Kind, int> itemAt(const QPointF& block, double reach) const;

  QJsonObject m_layout;
  double m_width = 180.0;
  double m_height = 36.0;
  // The frame's right edge and bottom on the sheet.
  double m_right = 0.0;
  double m_bottom = 0.0;
  double m_grid = 1.0;
  QVector<QLineF> m_lines;
  QVector<BlockField> m_fields;
  QVector<BlockImage> m_images;
  Tool m_tool = Tool::Select;
  std::optional<QPointF> m_first;
  QPointF m_pointer;
  QString m_pendingData;
  QString m_pendingFormat;
  Kind m_selectedKind = Kind::None;
  int m_selectedIndex = -1;
  // A move: where it started (block mm, on the grid) and the item's place
  // then.
  bool m_moving = false;
  bool m_moved = false;
  QPointF m_moveFrom;
  QLineF m_lineBefore;
  QRectF m_boxBefore;
  bool m_changed = false;
};

// The tool's name in the log ("select", "line", "field", "image").
QString titleBlockToolName(TitleBlockEditor::Tool tool);

// What an image file's bytes are: "png", "jpeg" or "svg" (by the PNG's and
// the JPEG's signatures, an SVG's root element), empty for none of them.
QString titleBlockImageFormat(const QByteArray& bytes);

} // namespace mitcad::drawings
