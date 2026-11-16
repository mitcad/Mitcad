// SPDX-License-Identifier: MIT
#include "UnitTests.hpp"
#include <cstdio>

#include <QApplication>
#include <QComboBox>
#include <QDialogButtonBox>
#include <QJsonArray>
#include <QJsonObject>
#include <QKeyEvent>
#include <QListWidget>
#include <QTextCursor>
#include <QLineEdit>
#include <QPlainTextEdit>
#include <QPushButton>
#include <QTreeWidget>

#include "framework/FormulaEditor.hpp"

namespace {
int failures=0;
void check(bool value,const char* expression,int line) {
  if (!value) { std::fprintf(stderr,"formula_editor.cpp:%d: %s\n",line,expression); ++failures; }
}
#define CHECK(value) check((value),#value,__LINE__)

// A key as the keyboard gives it, then what the event loop does after it.
void press(QWidget* widget,int key,const QString& text=QString()) {
  QKeyEvent down(QEvent::KeyPress,key,Qt::NoModifier,text);
  QApplication::sendEvent(widget,&down);
  QKeyEvent up(QEvent::KeyRelease,key,Qt::NoModifier,text);
  QApplication::sendEvent(widget,&up);
  QApplication::processEvents();
}

void typeText(QWidget* widget,const QString& text) {
  for (const QChar c:text) press(widget,0,QString(c));
}

// Completion (mitcad#667): only typed text opens the list, a caret move
// closes it and leaves its keys to the text, `Screws.` goes on with the
// table's columns, and a `.` that follows already is not doubled.
void completionTests(mitcad::ParameterSource source,const mitcad::ParameterFieldOptions& options) {
  using namespace mitcad;
  source.query=[](const QJsonObject&) {
    return QJsonObject{{QStringLiteral("functions"),QJsonArray{
        QJsonObject{{QStringLiteral("name"),QStringLiteral("SIN")},{QStringLiteral("description"),QStringLiteral("Sine")},{QStringLiteral("example"),QStringLiteral("SIN(30 deg)")}}}},
        {QStringLiteral("tables"),QJsonArray{QJsonObject{{QStringLiteral("name"),QStringLiteral("Screws")},
            {QStringLiteral("columns"),QJsonArray{
                QJsonObject{{QStringLiteral("name"),QStringLiteral("Size")},{QStringLiteral("unit"),QStringLiteral("Text")}},
                QJsonObject{{QStringLiteral("name"),QStringLiteral("dk")},{QStringLiteral("unit"),QStringLiteral("mm")}}}}}}}};
  };
  FormulaEditor editor(source,options,QString());
  editor.show();
  auto* text=editor.findChild<QPlainTextEdit*>(QStringLiteral("formulaText"));
  auto* list=editor.findChild<QListWidget*>(QStringLiteral("formulaCompletions"));
  CHECK(text && list);
  if (!text || !list) return;
  const auto caretAt=[text](int position) {
    QTextCursor cursor=text->textCursor(); cursor.setPosition(position); text->setTextCursor(cursor);
    QApplication::processEvents();
  };
  // Text put in and a caret moved after a name: no list.
  text->setPlainText(QStringLiteral("SIN"));
  caretAt(3);
  CHECK(!list->isVisible());
  // Typed: the tables and functions that start with it; Enter takes the
  // first, Screws., and the list goes on with its columns.
  text->clear();
  typeText(text,QStringLiteral("S"));
  CHECK(list->isVisible() && list->count()==2);
  press(text,Qt::Key_Return);
  CHECK(text->toPlainText()==QStringLiteral("Screws."));
  CHECK(list->isVisible() && list->count()==2);
  typeText(text,QStringLiteral("d"));
  CHECK(list->isVisible() && list->count()==1);
  press(text,Qt::Key_Tab);
  CHECK(text->toPlainText()==QStringLiteral("Screws.dk"));
  CHECK(!list->isVisible());
  // A caret move closes an open list: Enter is a new line again.
  text->setPlainText(QStringLiteral("1 + "));
  caretAt(4);
  typeText(text,QStringLiteral("Sc"));
  CHECK(list->isVisible());
  press(text,Qt::Key_Left);
  CHECK(!list->isVisible());
  press(text,Qt::Key_Return);
  CHECK(text->toPlainText().contains(QLatin1Char('\n')));
  // A `.` that follows already: `Scr|.dk` gives `Screws.dk`.
  text->setPlainText(QStringLiteral("Sc.dk"));
  caretAt(2);
  typeText(text,QStringLiteral("r"));
  CHECK(list->isVisible());
  press(text,Qt::Key_Return);
  CHECK(text->toPlainText()==QStringLiteral("Screws.dk"));
  editor.reject();
}

// Assignment formulas return library identifiers as text. OK must validate
// the edited text with that kind before accepting the editor's result.
void textFormulaTests() {
  using namespace mitcad;
  const QString expression=QStringLiteral("if(Holes == 'Yes'; 'paint_red'; 'paint_black')");
  QString queriedExpression;
  QString queriedKind;
  ParameterSource source;
  source.query=[&](const QJsonObject& query) {
    if (!query.contains(QStringLiteral("expression"))) return QJsonObject{};
    queriedExpression=query.value(QStringLiteral("expression")).toString();
    queriedKind=query.value(QStringLiteral("kind")).toString();
    if (queriedKind!=QStringLiteral("text")) {
      return QJsonObject{{QStringLiteral("error"),QJsonObject{
          {QStringLiteral("message"),QStringLiteral("text where a number is expected")}}}};
    }
    return QJsonObject{{QStringLiteral("result"),QJsonObject{
        {QStringLiteral("text"),QStringLiteral("paint_red")}}}};
  };
  ParameterFieldOptions options;
  options.kind=ParameterKind::Text;
  options.anyKind=true;
  FormulaEditor editor(source,options,QStringLiteral("'paint_black'"));
  auto* text=editor.findChild<QPlainTextEdit*>(QStringLiteral("formulaText"));
  auto* buttons=editor.findChild<QDialogButtonBox*>();
  CHECK(text && buttons && buttons->button(QDialogButtonBox::Ok));
  if (!text || !buttons || !buttons->button(QDialogButtonBox::Ok)) return;
  text->selectAll();
  typeText(text,expression);
  buttons->button(QDialogButtonBox::Ok)->click();
  CHECK(queriedKind==QStringLiteral("text"));
  CHECK(queriedExpression==expression);
  CHECK(editor.result()==QDialog::Accepted);
  CHECK(editor.expression()==expression);
}
}

int formulaEditorTests() {
  using namespace mitcad;
  int mutations=0;
  ParameterSource source;
  source.parameters=[] { return QJsonArray{QJsonObject{{QStringLiteral("name"),QStringLiteral("count")},
      {QStringLiteral("kind"),QStringLiteral("user")}, {QStringLiteral("limits"),QJsonObject{{QStringLiteral("min"),QStringLiteral("2")}}}}}; };
  source.addParameter=[&mutations](const QJsonObject&,QString*) { ++mutations; return true; };
  source.query=[](const QJsonObject&) {
    return QJsonObject{{QStringLiteral("functions"),QJsonArray{
        QJsonObject{{QStringLiteral("name"),QStringLiteral("FLOOR")},{QStringLiteral("description"),QStringLiteral("Round down")},{QStringLiteral("example"),QStringLiteral("FLOOR(4.5)")}},
        QJsonObject{{QStringLiteral("name"),QStringLiteral("SIN")},{QStringLiteral("description"),QStringLiteral("Sine")},{QStringLiteral("example"),QStringLiteral("SIN(30 deg)")}}}}};
  };
  ParameterFieldOptions options;
  options.own=QStringLiteral("count");
  options.kind=ParameterKind::Unitless;
  FormulaEditor editor(source,options,QStringLiteral("count + 1"));
  auto* text=editor.findChild<QPlainTextEdit*>(QStringLiteral("formulaText"));
  auto* search=editor.findChild<QLineEdit*>(QStringLiteral("formulaFunctionSearch"));
  auto* functions=editor.findChild<QTreeWidget*>(QStringLiteral("formulaFunctions"));
  auto* templates=editor.findChild<QComboBox*>(QStringLiteral("formulaTemplates"));
  auto* minimum=editor.findChild<QLineEdit*>(QStringLiteral("formulaMin"));
  auto* maximum=editor.findChild<QLineEdit*>(QStringLiteral("formulaMax"));
  CHECK(text && search && functions && templates && minimum && maximum);
  if (!text || !search || !functions || !templates || !minimum || !maximum) return failures;
  CHECK(functions->topLevelItemCount()==2);
  search->setText(QStringLiteral("round down"));
  CHECK(functions->topLevelItemCount()==1);
  CHECK(functions->topLevelItem(0)->text(0)==QStringLiteral("FLOOR"));
  CHECK(functions->topLevelItem(0)->text(2)==QStringLiteral("FLOOR(4.5)"));
  CHECK(editor.expression()==QStringLiteral("count + 1"));
  text->selectAll();
  templates->setCurrentIndex(1);
  CHECK(editor.expression()==QStringLiteral("FLOOR(length / pitch)"));
  CHECK(minimum->isEnabled() && minimum->text()==QStringLiteral("2"));
  maximum->setText(QStringLiteral("20"));
  CHECK(editor.limitsChanged());
  const QJsonObject command=editor.parameterCommand();
  CHECK(command.value(QStringLiteral("name")).toString()==QStringLiteral("count"));
  CHECK(command.value(QStringLiteral("max")).toString()==QStringLiteral("20"));
  CHECK(command.value(QStringLiteral("step")).isNull());
  editor.reject();
  CHECK(mutations==0); // Opening and canceling only edit local formula text.
  source.limitProblem=[](const QJsonObject&) { return QStringLiteral("Held by the open feature editor"); };
  FormulaEditor held(source,options,QStringLiteral("5"));
  CHECK(!held.findChild<QLineEdit*>(QStringLiteral("formulaMin"))->isEnabled());
  CHECK(!held.findChild<QLineEdit*>(QStringLiteral("formulaMax"))->isEnabled());
  const QString signature=formulaSignatureHtml({{QStringLiteral("name"),QStringLiteral("FLOOR")},
      {QStringLiteral("arguments"),QJsonArray{QStringLiteral("value"),QStringLiteral("[step]")}},
      {QStringLiteral("argument"),1}});
  CHECK(signature==QStringLiteral("FLOOR(value; <b><u>[step]</u></b>)"));
  completionTests(source,options);
  textFormulaTests();
  return failures;
}
MITCAD_UNIT_TEST(formulaEditorTests);
