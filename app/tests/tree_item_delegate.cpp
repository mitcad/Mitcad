// SPDX-License-Identifier: MIT
#include "UnitTests.hpp"
#include "framework/TreeItemDelegate.hpp"

#include <cstdio>
#include <QImage>
#include <QStandardItemModel>
#include <QSpinBox>
#include <QTreeView>

namespace {
int failures = 0;
void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "tree_item_delegate.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}
#define CHECK(expr) check((expr), #expr, __LINE__)
}

int treeItemDelegateTests() {
  using namespace mitcad;
  const QPalette original = QApplication::palette();
  for (bool dark : {false, true}) {
    QPalette palette = QApplication::palette();
    palette.setColor(QPalette::Base, dark ? QColor(30, 30, 30) : QColor(250, 250, 250));
    palette.setColor(QPalette::Text, dark ? Qt::white : Qt::black);
    QApplication::setPalette(palette);
    QTreeView view;
    // A Docked tree uses a transparent selection and the window's accent
    // text. A copied but unresolved application palette must not inherit
    // this invisible selection pair into the line editor.
    QPalette tree;
    tree.setColor(QPalette::Base, Qt::transparent);
    tree.setColor(QPalette::Highlight, Qt::transparent);
    tree.setColor(QPalette::HighlightedText, palette.color(QPalette::Base));
    view.setPalette(tree);
    QStandardItemModel model;
    model.appendRow(new QStandardItem(QStringLiteral("Old name still visible")));
    view.setModel(&model);
    TreeItemDelegate delegate;
    const QModelIndex index = model.index(0, 0);
    QStyleOptionViewItem option;
    option.initFrom(&view);
    option.rect = QRect(0, 0, 240, 28);
    option.widget = &view;
    option.features = QStyleOptionViewItem::HasDisplay;
    const auto painted = [&] {
      QImage image(240, 28, QImage::Format_ARGB32_Premultiplied);
      image.fill(Qt::transparent);
      QPainter painter(&image);
      delegate.paint(&painter, option, index);
      return image;
    };
    const QImage before = painted();
    QWidget* editor = delegate.createEditor(view.viewport(), option, index);
    delegate.setEditorData(editor, index);
    delegate.updateEditorGeometry(editor, option, index);
    auto* line = qobject_cast<QLineEdit*>(editor);
    CHECK(line != nullptr);
    CHECK(delegate.editing(index));
    CHECK(!line->hasFrame());
    CHECK(line->palette().color(QPalette::Base).alpha() == 255);
    for (QPalette::ColorGroup group : {QPalette::Active, QPalette::Inactive, QPalette::Disabled}) {
      for (QPalette::ColorRole role : {QPalette::Text, QPalette::Highlight, QPalette::HighlightedText}) {
        CHECK(line->palette().color(group, role) == QApplication::palette().color(group, role));
      }
    }
    CHECK(editor->testAttribute(Qt::WA_OpaquePaintEvent));
    CHECK(option.rect.contains(editor->geometry()));
    CHECK(editor->geometry().contains(delegate.nameTextRect(option, index)));
    CHECK(editor->x() + line->textMargins().left() == delegate.nameTextRect(option, index).left());
    CHECK(editor->geometry().height() == option.rect.height() - 2);
    // The open design in Projects uses a bold name. Its resolved font and
    // the native style's focus margins must not offset the rename text.
    QFont bold = view.font();
    bold.setBold(true);
    model.setData(index, bold, Qt::FontRole);
    delegate.updateEditorGeometry(editor, option, index);
    CHECK(editor->geometry().contains(delegate.nameTextRect(option, index)));
    CHECK(editor->x() + line->textMargins().left() == delegate.nameTextRect(option, index).left());
    const QImage whileEditing = painted();
    CHECK(before != whileEditing);
    // Changing the underlying name while the editor is open cannot alter
    // the painted row: the delegate does not draw that name at all.
    model.setData(index, QStringLiteral("Another long underlying name"));
    CHECK(painted() == whileEditing);
    line->clear();
    editor->resize(220, 26);
    QImage opaque(editor->size(), QImage::Format_ARGB32_Premultiplied);
    opaque.fill(Qt::transparent);
    editor->render(&opaque);
    CHECK(opaque.pixelColor(10, 10).alpha() == 255);
    CHECK(opaque.pixelColor(10, 10) == palette.color(QPalette::Base));
    model.insertRow(0, new QStandardItem(QStringLiteral("Inserted above the editor")));
    CHECK(delegate.editing(model.index(1, 0)));
    CHECK(!delegate.editing(model.index(0, 0)));
    model.removeRow(0);
    CHECK(delegate.editing(index));
    delegate.destroyEditor(editor, index);
    CHECK(!delegate.editing(index));
    model.setData(index, 42, Qt::EditRole);
    QWidget* number = delegate.createEditor(view.viewport(), option, index);
    CHECK(qobject_cast<QSpinBox*>(number) != nullptr);
    CHECK(!delegate.editing(index));
    delegate.destroyEditor(number, index);
    model.setData(index, QStringLiteral("Reset while editing"), Qt::EditRole);
    QWidget* reset = delegate.createEditor(view.viewport(), option, index);
    CHECK(delegate.editing(index));
    model.clear();
    CHECK(!delegate.editing(QModelIndex()));
    delete reset;
  }
  QApplication::setPalette(original);
  return failures;
}
MITCAD_UNIT_TEST(treeItemDelegateTests);
