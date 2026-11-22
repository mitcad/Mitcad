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
#include <QTextBlock>
#include <QTextCharFormat>
#include <QTextDocument>
#include <QTextLayout>
#include <QTreeWidget>

#include "framework/FormulaEditor.hpp"
#include "framework/Theme.hpp"

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

// The code editor (mitcad#2179): the text coloured by the model's tokens
// (byte ranges, here after a two-byte character), an unknown name in the
// error's colour with its underline, the bracket at the caret and its match
// marked; completion with a parameter's value, a function's arguments and,
// after a number, the units, what is typed first.
void codeEditorTests() {
  using namespace mitcad;
  ParameterSource source;
  source.parameters=[] { return QJsonArray{QJsonObject{{QStringLiteral("name"),QStringLiteral("width")},
      {QStringLiteral("kind"),QStringLiteral("user")},{QStringLiteral("unit"),QStringLiteral("mm")},
      {QStringLiteral("text"),QStringLiteral("40 mm")}}}; };
  int tokenQueries=0;
  source.query=[&tokenQueries](const QJsonObject& query) {
    if (query.value(QStringLiteral("tokens_only")).toBool()) {
      ++tokenQueries;
      // "é + MAX(width; gap) * 2 m": é is two bytes in UTF-8.
      const auto token=[](const char* kind,int start,int end) {
        return QJsonObject{{QStringLiteral("kind"),QLatin1String(kind)},{QStringLiteral("start"),start},
                           {QStringLiteral("end"),end}};
      };
      if (query.value(QStringLiteral("expression")).toString()!=QStringLiteral("é + MAX(width; gap) * 2 m")) {
        return QJsonObject{{QStringLiteral("tokens"),QJsonArray{}}};
      }
      return QJsonObject{{QStringLiteral("tokens"),QJsonArray{
          token("unknown",0,2),token("operator",3,4),token("function",5,8),token("bracket",8,9),
          token("name",9,14),token("separator",14,15),token("unknown",16,19),token("bracket",19,20),
          token("operator",21,22),token("number",23,24),token("unit",25,26)}}};
    }
    return QJsonObject{{QStringLiteral("functions"),QJsonArray{
        QJsonObject{{QStringLiteral("name"),QStringLiteral("MAX")},{QStringLiteral("description"),QStringLiteral("Largest")},
                    {QStringLiteral("example"),QStringLiteral("MAX(1; 2)")},
                    {QStringLiteral("arguments"),QJsonArray{QStringLiteral("value"),QStringLiteral("...")}}}}},
        {QStringLiteral("units"),QJsonArray{QStringLiteral("micron"),QStringLiteral("mm"),QStringLiteral("m"),
                                            QStringLiteral("mi")}}};
  };
  ParameterFieldOptions options;
  FormulaEditor editor(source,options,QStringLiteral("é + MAX(width; gap) * 2 m"));
  editor.show();
  QApplication::processEvents();
  auto* text=editor.findChild<QPlainTextEdit*>(QStringLiteral("formulaText"));
  auto* list=editor.findChild<QListWidget*>(QStringLiteral("formulaCompletions"));
  CHECK(text && list);
  if (!text || !list) return;
  CHECK(tokenQueries>0);
  // UTF-16 positions: é is one character.
  CHECK(editor.tokenKindAt(0)==QStringLiteral("unknown"));
  CHECK(editor.tokenKindAt(1).isEmpty());
  CHECK(editor.tokenKindAt(4)==QStringLiteral("function"));
  CHECK(editor.tokenKindAt(8)==QStringLiteral("name"));
  CHECK(editor.tokenKindAt(15)==QStringLiteral("unknown"));
  CHECK(editor.tokenKindAt(24)==QStringLiteral("unit"));
  // The colours shown are the kinds' (light or dark by the palette).
  const QPalette palette=text->palette();
  const auto shown=[text](int position) {
    QTextCharFormat format;
    const QTextBlock block=text->document()->findBlock(position);
    for (const QTextLayout::FormatRange& range : block.layout()->formats()) {
      if (range.start<=position-block.position() && position-block.position()<range.start+range.length) format=range.format;
    }
    return format;
  };
  CHECK(shown(4).foreground().color()==formulaTokenColor(QStringLiteral("function"),palette));
  CHECK(shown(8).foreground().color()==themeColor(ColorRole::FormulaName,palette));
  CHECK(shown(22).foreground().color()==themeColor(ColorRole::FormulaNumber,palette));
  CHECK(shown(15).foreground().color()==themeColor(ColorRole::ErrorText,palette));
  CHECK(shown(15).underlineStyle()==QTextCharFormat::WaveUnderline);
  CHECK(shown(4).foreground().color()!=shown(8).foreground().color());
  CHECK(formulaTokenFormat(QStringLiteral("keyword"),palette).fontWeight()==QFont::Bold);
  CHECK(formulaTokenColor(QStringLiteral("unit"),palette)!=formulaTokenColor(QStringLiteral("number"),palette));
  // The caret before `(`: the line and the two brackets marked.
  QTextCursor cursor=text->textCursor();
  cursor.setPosition(7);
  text->setTextCursor(cursor);
  QApplication::processEvents();
  CHECK(text->extraSelections().size()==3);
  // Between the brackets: only the line.
  cursor.setPosition(10);
  text->setTextCursor(cursor);
  QApplication::processEvents();
  CHECK(text->extraSelections().size()==1);
  // Completion: a parameter with its value, a function with its arguments.
  text->clear();
  typeText(text,QStringLiteral("w"));
  CHECK(list->isVisible() && list->count()==1 && list->item(0)->text().contains(QStringLiteral("40 mm")));
  text->clear();
  typeText(text,QStringLiteral("MA"));
  CHECK(list->isVisible() && list->count()==1 && list->item(0)->text().contains(QStringLiteral("value; ...")));
  // After a number the units, as written; what is typed first.
  text->clear();
  typeText(text,QStringLiteral("5 m"));
  CHECK(list->isVisible() && list->count()==4);
  CHECK(list->isVisible() && list->item(0)->data(Qt::UserRole).toString()==QStringLiteral("m"));
  press(text,Qt::Key_Tab);
  CHECK(text->toPlainText()==QStringLiteral("5 m"));
  text->clear();
  typeText(text,QStringLiteral("5 mm"));
  CHECK(!list->isVisible());
  // Not where a value stands.
  text->clear();
  typeText(text,QStringLiteral("1 + mi"));
  CHECK(!list->isVisible());
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
  codeEditorTests();
  textFormulaTests();
  return failures;
}
MITCAD_UNIT_TEST(formulaEditorTests);
