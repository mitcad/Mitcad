// SPDX-License-Identifier: MIT
#include "TestDriver.hpp"
#include "FormulaEditor.hpp"

#include <algorithm>

#include <QApplication>
#include <QComboBox>
#include <QHeaderView>
#include <QLabel>
#include <QLineEdit>
#include <QPlainTextEdit>
#include <QScreen>
#include <QTextBlock>
#include <QTextCursor>
#include <QTextDocument>
#include <QTextLayout>
#include <QToolButton>
#include <QTreeWidget>
#include <QtLogging>

namespace mitcad {

void TestDriver::openFormulaField(const QString& name) {
  auto* edit=targetWindow()->findChild<QLineEdit*>(name);
  if (!edit || !ParameterField::of(edit)) {
    fail(m_line,QStringLiteral("no formula-enabled field %1").arg(name));
    return;
  }
  ParameterField::of(edit)->openFormulaEditor();
}

void TestDriver::testFormulaBuilder() {
  auto* dialog=dynamic_cast<FormulaEditor*>(QApplication::activeModalWidget());
  if (!dialog) { fail(m_line,QStringLiteral("Formula Editor must be open through an actual entry point")); return; }
  const QPointer<FormulaEditor> guard(dialog);
  auto* text=dialog->findChild<QPlainTextEdit*>(QStringLiteral("formulaText"));
  auto* search=dialog->findChild<QLineEdit*>(QStringLiteral("formulaFunctionSearch"));
  auto* functions=dialog->findChild<QTreeWidget*>(QStringLiteral("formulaFunctions"));
  if (!text || !search || !functions || functions->topLevelItemCount()!=60) { // 52 + the text functions (mitcad#664) + XLOOKUP, MATCH, INDEX (mitcad#667)
    fail(m_line,QStringLiteral("formula catalog or multiline text editor missing")); return;
  }
  if (dialog->screen() && dialog->width()<std::min(700,dialog->screen()->availableGeometry().width()-24)) {
    fail(m_line,QStringLiteral("formula editor collapsed to a narrow text-field width")); return;
  }
  for (int column=0;column<3;++column) {
    QHeaderView* header=functions->header();
    const int left=header->sectionViewportPosition(column);
    if (left<0 || header->sectionSize(column)<40 || left+header->sectionSize(column)>header->viewport()->width()+1) {
      fail(m_line,QStringLiteral("formula function catalog column %1 is outside its visible viewport").arg(column)); return;
    }
  }
  search->setText(QStringLiteral("round down, optionally"));
  if (functions->topLevelItemCount()!=1 || functions->topLevelItem(0)->text(0)!=QStringLiteral("FLOOR") ||
      functions->topLevelItem(0)->text(2).isEmpty()) {
    fail(m_line,QStringLiteral("function search does not include descriptions and examples")); return;
  }
  search->clear();
  queue([guard] {
    auto* input=guard->findChild<QPlainTextEdit*>(QStringLiteral("formulaText"));
    input->setPlainText(QStringLiteral("FLOOR("));
    QTextCursor cursor=input->textCursor(); cursor.movePosition(QTextCursor::End); input->setTextCursor(cursor);
  });
  queue([] {}); queue([] {}); queue([] {});
  queue([this,guard] {
    const QString signature=guard->findChild<QLabel*>(QStringLiteral("formulaSignature"))->text();
    if (!signature.contains(QStringLiteral("FLOOR(<b><u>value</u></b>"))) {
      fail(m_line,QStringLiteral("incomplete function signature did not highlight the current argument: %1").arg(signature)); return;
    }
    auto* input=guard->findChild<QPlainTextEdit*>(QStringLiteral("formulaText"));
    input->setPlainText(QStringLiteral("length / pitch"));
    input->selectAll();
  });
  queue([] {}); queue([] {}); queue([] {});
  queue([this,guard] {
    const QString part=guard->findChild<QLabel*>(QStringLiteral("formulaPartValue"))->text();
    if (!part.contains(QStringLiteral("length / pitch → 12.5"))) {
      fail(m_line,QStringLiteral("selected expression did not show its actual model value: %1").arg(part)); return;
    }
    auto* input=guard->findChild<QPlainTextEdit*>(QStringLiteral("formulaText"));
    input->selectAll();
    guard->findChild<QComboBox*>(QStringLiteral("formulaTemplates"))->setCurrentIndex(1);
    if (input->toPlainText()!=QStringLiteral("FLOOR(length / pitch)")) {
      fail(m_line,QStringLiteral("formula template did not insert ordinary editable text")); return;
    }
    guard->findChild<QLineEdit*>(QStringLiteral("formulaMin"))->setText(QStringLiteral("2"));
    guard->findChild<QLineEdit*>(QStringLiteral("formulaMax"))->setText(QStringLiteral("10"));
    guard->findChild<QLineEdit*>(QStringLiteral("formulaStep"))->setText(QStringLiteral("1"));
    qInfo() << "Formula builder: incomplete signature, searchable52function catalog, selected value12.5, plain template and limits verified";
  });
}

namespace {

// Whether the line editor with the keyboard, a table cell's, shows its
// whole text (mitcad#2179); why not in `why`. The start is in sight when
// the editor's left edge is at the text's first position, the end when
// the place just left of the fx button (or of the right edge) is at its
// last: the editor's own mapping, which knows its scrolling. A text longer
// than the table is wide shows whole in the wrapped strip under it.
bool cellEditorWhole(QString* why, QString* what) {
  auto* edit = qobject_cast<QLineEdit*>(QApplication::focusWidget());
  if (edit == nullptr || edit->parentWidget() == nullptr) {
    *why = QStringLiteral("no line editor has the keyboard");
    return false;
  }
  const QWidget* viewport = edit->parentWidget();
  const QRect place = edit->geometry();
  if (place.left() < 0 || place.right() >= viewport->width() + 1) {
    *why = QStringLiteral("the editor (%1 px at %2) leaves the table (%3 px)")
               .arg(place.width()).arg(place.left()).arg(viewport->width());
    return false;
  }
  const QString text = edit->text();
  if (auto* whole = viewport->findChild<QLabel*>(QStringLiteral("parameterExpressionWhole"));
      whole != nullptr && whole->isVisible()) {
    if (whole->text() != text) {
      *why = QStringLiteral("the wrapped strip shows '%1', not '%2'").arg(whole->text(), text);
      return false;
    }
    *what = QStringLiteral("wrapped under a %1 px editor in a %2 px table, %3 characters")
                .arg(place.width()).arg(viewport->width()).arg(text.size());
    return true;
  }
  int right = edit->width() - 3;
  for (const QToolButton* button : edit->findChildren<QToolButton*>()) {
    if (button->isVisible()) {
      right = std::min(right, button->geometry().left() - 2);
    }
  }
  const int middle = edit->height() / 2;
  const int first = edit->cursorPositionAt(QPoint(edit->contentsRect().left() + 3, middle));
  const int last = edit->cursorPositionAt(QPoint(right, middle));
  if (first != 0 || last != text.size()) {
    *why = QStringLiteral("the editor (%1 px) shows positions %2 to %3 of %4 ('%5')")
               .arg(place.width()).arg(first).arg(last).arg(text.size()).arg(text);
    return false;
  }
  *what = QStringLiteral("%1 px editor in a %2 px table, %3 characters")
              .arg(place.width()).arg(viewport->width()).arg(text.size());
  return true;
}

} // namespace

void TestDriver::testCellEditorWhole() {
  const int line = m_line;
  whenReady([] {
    QString why;
    QString what;
    return cellEditorWhole(&why, &what);
  }, 6, QStringLiteral("the cell's editor does not show its whole text"), [] {
    QString why;
    QString what;
    cellEditorWhole(&why, &what);
    qInfo().noquote() << QStringLiteral("TestDriver: cell editor whole: %1").arg(what);
  });
  // The reason when it times out.
  QString why;
  QString what;
  if (!cellEditorWhole(&why, &what)) {
    qInfo().noquote() << QStringLiteral("TestDriver: cell editor not whole yet: %1 (line %2)").arg(why).arg(line);
  }
}

void TestDriver::testFormulaToken(const QString& text, const QString& kind) {
  const int line = m_line;
  // The first occurrence of `text` in the open Formula Editor's formula
  // has the kind, and the text shows the kind's colour (mitcad#2179).
  const auto check = [text, kind](QString* why) {
    auto* dialog = dynamic_cast<FormulaEditor*>(QApplication::activeModalWidget());
    if (dialog == nullptr) {
      *why = QStringLiteral("Formula Editor is not open");
      return false;
    }
    auto* input = dialog->findChild<QPlainTextEdit*>(QStringLiteral("formulaText"));
    const int at = input != nullptr ? input->toPlainText().indexOf(text) : -1;
    if (at < 0) {
      *why = QStringLiteral("no '%1' in the formula").arg(text);
      return false;
    }
    const QString found = dialog->tokenKindAt(at);
    if (found != kind) {
      *why = QStringLiteral("'%1' is a %2 token, not %3").arg(text, found.isEmpty() ? QStringLiteral("no") : found, kind);
      return false;
    }
    const QTextBlock block = input->document()->findBlock(at);
    const int offset = at - block.position();
    QColor shown;
    if (block.layout() != nullptr) {
      for (const QTextLayout::FormatRange& range : block.layout()->formats()) {
        if (range.start <= offset && offset < range.start + range.length) {
          shown = range.format.foreground().color();
        }
      }
    }
    const QColor expected = formulaTokenColor(kind, input->palette());
    if (shown != expected) {
      *why = QStringLiteral("'%1' shows %2, not the %3 colour %4")
                 .arg(text, shown.isValid() ? shown.name() : QStringLiteral("no colour"), kind, expected.name());
      return false;
    }
    return true;
  };
  QString why;
  if (!check(&why)) {
    qInfo().noquote() << QStringLiteral("TestDriver: formula token not yet as expected: %1 (line %2)").arg(why).arg(line);
  }
  whenReady([check] {
    QString reason;
    return check(&reason);
  }, 6, QStringLiteral("formula token '%1' is not shown as %2").arg(text, kind), [text, kind] {
    qInfo().noquote() << QStringLiteral("TestDriver: formula token %1 is %2").arg(text, kind);
  });
}

// This file's steps (mitcad#2173): their specs and their chain.
namespace {

const TestDriver::StepSpec kStepSpecs[] = {
    {"expect-formula-builder", 0, 0, TestDriver::kIdle},
    {"open-formula-field", 1, 1, TestDriver::kInputOfTarget},
    // The cell's editor and the Formula Editor's colours (mitcad#2179).
    {"expect-cell-editor-whole", 0, 0, TestDriver::kIdle},
    {"expect-formula-token", 2, 2, TestDriver::kIdle},
};

} // namespace

const TestDriver::StepArea TestDriver::kFormulaSteps{
    kStepSpecs, std::size(kStepSpecs), &TestDriver::runFormulaStep};

void TestDriver::runFormulaStep(const QString& name, const QStringList& a, int) {
  if (name == QLatin1String("expect-formula-builder")) {
    testFormulaBuilder();
  } else if (name == QLatin1String("open-formula-field")) {
    openFormulaField(a.at(0));
  } else if (name == QLatin1String("expect-cell-editor-whole")) {
    testCellEditorWhole();
  } else if (name == QLatin1String("expect-formula-token")) {
    testFormulaToken(a.at(0), a.at(1));
  }
}

} // namespace mitcad
