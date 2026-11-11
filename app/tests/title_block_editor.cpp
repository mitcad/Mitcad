// SPDX-License-Identifier: MIT
// The title block editor (mitcad#1625): a block read from the `drawing`
// query's sheet, lines and fields drawn on its grid, an image placed,
// moves kept inside the block, deletes, the selected field's label, text
// and height, and the block given back as `edit_drawing_title_block` takes
// it; the sheet sizes' text forms of the dialogs.
#include "drawings/DrawingTemplates.hpp"
#include "drawings/TitleBlockEditor.hpp"

#include <cstdio>
#include <optional>

#include <QJsonArray>
#include <QJsonObject>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "title_block_editor.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

// An A3 sheet's frame (20, 10)–(410, 287) and a 180 x 36 block with one
// line, one field and a projection symbol, as the query gives them.
QJsonObject sheet() {
  // A list of one line, appended: MSVC takes QJsonArray{QJsonArray{...}}
  // for a copy of the inner array (one element in braces), not a list
  // holding it.
  QJsonArray lines;
  lines.append(QJsonArray{0.0, 18.0, 180.0, 18.0});
  const QJsonObject layout{
      {QStringLiteral("width"), 180.0},
      {QStringLiteral("height"), 36.0},
      {QStringLiteral("lines"), lines},
      {QStringLiteral("fields"),
       QJsonArray{QJsonObject{{QStringLiteral("label"), QStringLiteral("TITLE")},
                              {QStringLiteral("text"), QStringLiteral("{title}")},
                              {QStringLiteral("at"), QJsonArray{0.0, 18.0}},
                              {QStringLiteral("size"), QJsonArray{180.0, 18.0}}}}},
      {QStringLiteral("projection_symbol"), QJsonArray{150.0, 5.0}}};
  return {{QStringLiteral("title_block_layout"), layout},
          {QStringLiteral("frame"), QJsonArray{QJsonArray{20.0, 10.0}, QJsonArray{410.0, 287.0}}},
          {QStringLiteral("title_box"), QJsonArray{QJsonArray{230.0, 10.0}, QJsonArray{410.0, 46.0}}}};
}

} // namespace

int titleBlockEditorTests() {
  using namespace mitcad::drawings;
  CHECK(!TitleBlockEditor::fromSheet(QJsonObject()).has_value());
  std::optional<TitleBlockEditor> made = TitleBlockEditor::fromSheet(sheet());
  CHECK(made.has_value());
  if (!made) {
    return failures;
  }
  TitleBlockEditor& editor = *made;
  // In the frame's lower right corner; points to its own millimetres.
  CHECK(editor.origin() == QPointF(230.0, 10.0));
  CHECK(editor.summary() == QStringLiteral("1 line, 1 field, 0 images"));
  CHECK(!editor.changed());
  // On the grid and inside the block.
  CHECK(editor.snapped(QPointF(240.4, 20.6)) == QPointF(10.0, 11.0));
  editor.setGrid(5.0);
  CHECK(editor.snapped(QPointF(242.4, 20.6)) == QPointF(10.0, 10.0));
  CHECK(editor.snapped(QPointF(100.0, 500.0)) == QPointF(0.0, 36.0));
  editor.setGrid(1.0);

  // A line of two clicks: its first click waits, its second makes it.
  editor.setTool(TitleBlockEditor::Tool::Line);
  CHECK(editor.press(QPointF(230.0, 40.0), 1.0).startsWith(QStringLiteral("first point")));
  CHECK(editor.firstPoint().has_value());
  CHECK(editor.press(QPointF(280.0, 40.0), 1.0) == QStringLiteral("line added: 0,30 50,30"));
  CHECK(editor.lines().size() == 2 && editor.selectedKind() == TitleBlockEditor::Kind::Line);
  CHECK(editor.tool() == TitleBlockEditor::Tool::Line);
  // Esc: the tool back to Select, then the selection goes.
  CHECK(editor.escape() && editor.tool() == TitleBlockEditor::Tool::Select);
  CHECK(editor.escape() && editor.selectedKind() == TitleBlockEditor::Kind::None);
  CHECK(!editor.escape());

  // A field of two corners, selected with Select on after it; its label,
  // text and height.
  editor.setTool(TitleBlockEditor::Tool::Field);
  editor.press(QPointF(230.0, 40.0), 1.0);
  CHECK(editor.press(QPointF(280.0, 46.0), 1.0) == QStringLiteral("field added: 0,30 50x6"));
  CHECK(editor.tool() == TitleBlockEditor::Tool::Select);
  CHECK(editor.selectedKind() == TitleBlockEditor::Kind::Field && editor.selectedIndex() == 1);
  CHECK(editor.setFieldLabel(QStringLiteral("STOCK")) && editor.setFieldText(QStringLiteral("{material}")));
  CHECK(editor.setFieldHeight(2.5) && !editor.setFieldHeight(2.5));

  // An image needs its file first; its two corners then place it.
  editor.setTool(TitleBlockEditor::Tool::Image);
  CHECK(editor.press(QPointF(232.0, 12.0), 1.0) == QStringLiteral("no image chosen"));
  editor.setPendingImage(QStringLiteral("iVBORw0KGgo="), QStringLiteral("png"));
  CHECK(editor.tool() == TitleBlockEditor::Tool::Image && editor.hasPendingImage());
  editor.press(QPointF(232.0, 12.0), 1.0);
  CHECK(editor.press(QPointF(262.0, 22.0), 1.0) == QStringLiteral("image added: png 2,2 30x10"));
  CHECK(!editor.hasPendingImage() && editor.images().size() == 1);

  // Select picks the line before the cell it crosses, the smallest cell,
  // the image over the cells.
  editor.setTool(TitleBlockEditor::Tool::Select);
  CHECK(editor.press(QPointF(300.0, 28.2), 0.5) == QStringLiteral("line 1 selected"));
  editor.release(QPointF(300.0, 28.2));
  CHECK(editor.press(QPointF(240.0, 43.0), 0.5) == QStringLiteral("field 2 selected"));
  editor.release(QPointF(240.0, 43.0));
  CHECK(editor.press(QPointF(240.0, 15.0), 0.5) == QStringLiteral("image 1 selected"));
  // A drag moves it by whole grid steps, kept inside the block.
  editor.moveTo(QPointF(200.0, 15.0));
  CHECK(editor.release(QPointF(200.0, 15.0)) == QStringLiteral("image 1 moved: 0,2"));
  CHECK(editor.images().at(0).box.topLeft() == QPointF(0.0, 2.0));
  // The field moved up 3 mm: no further than the block's top.
  editor.press(QPointF(240.0, 43.0), 0.5);
  editor.moveTo(QPointF(240.0, 46.0));
  editor.release(QPointF(240.0, 46.0));
  CHECK(editor.fields().at(1).cell.topLeft() == QPointF(0.0, 30.0));

  // Delete the line drawn first.
  editor.press(QPointF(300.0, 28.0), 0.5);
  editor.release(QPointF(300.0, 28.0));
  CHECK(editor.deleteSelected() == QStringLiteral("line 1"));
  CHECK(editor.deleteSelected().isEmpty());
  CHECK(editor.summary() == QStringLiteral("1 line, 2 fields, 1 image"));
  CHECK(editor.changed());

  // The block given back: its size, lines, fields and images, and the
  // projection symbol it had.
  editor.setSize(200.0, 40.0);
  CHECK(editor.origin() == QPointF(210.0, 10.0));
  const QJsonObject block = editor.toJson();
  CHECK(block.value(QStringLiteral("width")).toDouble() == 200.0);
  CHECK(block.value(QStringLiteral("lines")).toArray().size() == 1);
  const QJsonObject field = block.value(QStringLiteral("fields")).toArray().at(1).toObject();
  CHECK(field.value(QStringLiteral("label")).toString() == QStringLiteral("STOCK"));
  CHECK(field.value(QStringLiteral("text")).toString() == QStringLiteral("{material}"));
  CHECK(field.value(QStringLiteral("height")).toDouble() == 2.5);
  CHECK(field.value(QStringLiteral("at")).toArray() == (QJsonArray{0.0, 30.0}));
  CHECK(field.value(QStringLiteral("size")).toArray() == (QJsonArray{50.0, 6.0}));
  const QJsonObject image = block.value(QStringLiteral("images")).toArray().at(0).toObject();
  CHECK(image.value(QStringLiteral("data")).toString() == QStringLiteral("iVBORw0KGgo="));
  CHECK(image.value(QStringLiteral("size")).toArray() == (QJsonArray{30.0, 10.0}));
  CHECK(block.value(QStringLiteral("projection_symbol")).toArray() == (QJsonArray{150.0, 5.0}));

  // The images' kinds by their bytes.
  CHECK(titleBlockImageFormat(QByteArray("\x89PNG\r\n\x1a\nxx", 10)) == QStringLiteral("png"));
  CHECK(titleBlockImageFormat(QByteArray("\xff\xd8\xff\xe0", 4)) == QStringLiteral("jpeg"));
  CHECK(titleBlockImageFormat("<?xml version=\"1.0\"?><svg xmlns=\"x\"/>") == QStringLiteral("svg"));
  CHECK(titleBlockImageFormat("plain text").isEmpty());
  CHECK(titleBlockToolName(TitleBlockEditor::Tool::Field) == QStringLiteral("field"));

  // A sheet's size as the dialogs keep it (mitcad#1625).
  CHECK(sheetSizeText(QJsonValue(QStringLiteral("A3"))) == QStringLiteral("A3"));
  const QJsonObject own{{QStringLiteral("width"), 500.0}, {QStringLiteral("height"), 300.0}};
  CHECK(sheetSizeText(own) == QStringLiteral("500x300"));
  CHECK(sheetSizeValue(QStringLiteral("500x300")) == QJsonValue(own));
  CHECK(sheetSizeValue(QStringLiteral("ANSI_B")) == QJsonValue(QStringLiteral("ANSI_B")));
  double w = 0.0;
  double h = 0.0;
  CHECK(customSheetSize(QStringLiteral("250.5x400"), &w, &h) && w == 250.5 && h == 400.0);
  CHECK(!customSheetSize(QStringLiteral("A4"), &w, &h) && !customSheetSize(QStringLiteral("0x10"), &w, &h));
  // A template's size of its own comes to the dialogs as that text.
  const QJsonObject listed{
      {QStringLiteral("templates"),
       QJsonArray{QJsonObject{{QStringLiteral("id"), QStringLiteral("company")},
                              {QStringLiteral("name"), QStringLiteral("Company")},
                              {QStringLiteral("library"), QStringLiteral("user")},
                              {QStringLiteral("choice"), QStringLiteral("user:company")},
                              {QStringLiteral("size"), own}}}}};
  CHECK(templateChoices(listed).at(0).size == QStringLiteral("500x300"));
  return failures;
}
