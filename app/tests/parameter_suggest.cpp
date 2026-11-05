// SPDX-License-Identifier: MIT
// Parameter suggestions of expression fields (mitcad#567, mitcad#568): the
// name at the caret, what matches it and in what order, what a choice does
// to the text, the choices a field cannot take, new names; and on a plain
// field of a window on the offscreen platform, the list's keys (Enter puts
// the reference in without the field's own Enter, Esc only closes the
// list), the fx browser searching comments, and `name = …`; Edit
// Formula… keeping the `name =` (mitcad#755), and the context menu acting
// on the field's caret and selection (mitcad#756); Up and Down stepping a
// plain value with the list closed (mitcad#1254).
#include "framework/FormulaEditor.hpp"
#include "framework/ParameterField.hpp"
#include "framework/ParameterSuggest.hpp"
#include "framework/ValueStep.hpp"

#include <cstdio>
#include <functional>
#include <vector>

#include <QAction>
#include <QApplication>
#include <QClipboard>
#include <QContextMenuEvent>
#include <QEvent>
#include <QFocusEvent>
#include <QGuiApplication>
#include <QHelpEvent>
#include <QJsonArray>
#include <QJsonObject>
#include <QKeyEvent>
#include <QLineEdit>
#include <QList>
#include <QMenu>
#include <QPlainTextEdit>
#include <QPoint>
#include <QPointF>
#include <QString>
#include <QStringList>
#include <QTimer>
#include <QToolTip>
#include <QWheelEvent>
#include <QWidget>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "parameter_suggest.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

using namespace mitcad;

QJsonObject parameter(const char* name, const char* expression, const char* text, const char* unit,
                      const char* kind, const char* comment, const QStringList& uses = {},
                      const char* owner = "") {
  return {{QStringLiteral("name"), QString::fromUtf8(name)},
          {QStringLiteral("expression"), QString::fromUtf8(expression)},
          {QStringLiteral("text"), QString::fromUtf8(text)},
          {QStringLiteral("unit"), QString::fromUtf8(unit)},
          {QStringLiteral("kind"), QString::fromUtf8(kind)},
          {QStringLiteral("comment"), QString::fromUtf8(comment)},
          {QStringLiteral("owner"), QString::fromUtf8(owner)},
          {QStringLiteral("dependencies"), QJsonArray::fromStringList(uses)}};
}

// A small design's parameters, as the `parameters` query gives them: three
// user parameters (two lengths, an angle), model parameters of which d1
// uses width and d2 uses d1, and Sketch1's (F1) driven reference d3.
QJsonArray designParameters() {
  return {parameter("width", "30 mm", "30 mm", "mm", "user", ""),
          parameter("thickness", "4 mm", "4 mm", "mm", "user", "Wall thickness"),
          parameter("tilt", "15 deg", "15 deg", "deg", "user", ""),
          parameter("d1", "width + 5 mm", "35 mm", "mm", "model", "Extrude1 distance", {QStringLiteral("width")},
                    "F2"),
          parameter("d10", "2 mm", "2 mm", "mm", "model", "Fillet1 radius", {}, "F3"),
          parameter("d2", "d1 * 2", "70 mm", "mm", "model", "Sketch1 length", {QStringLiteral("d1")}, "F1"),
          parameter("d3", "25", "25 mm", "mm", "reference", "Sketch1 length (driven)", {}, "F1")};
}

QStringList names(const std::vector<ParameterChoice>& choices) {
  QStringList found;
  for (const ParameterChoice& choice : choices) {
    found << choice.parameter->name;
  }
  return found;
}

const ParameterChoice* choiceOf(const std::vector<ParameterChoice>& choices, const QString& name) {
  for (const ParameterChoice& choice : choices) {
    if (choice.parameter->name == name) {
      return &choice;
    }
  }
  return nullptr;
}

void fragmentTests() {
  // In `width / 2 + thi` the list is for `thi`.
  NameFragment f = nameFragmentAt(QStringLiteral("width / 2 + thi"), 15);
  CHECK(f.active);
  CHECK(f.typed == QStringLiteral("thi"));
  CHECK(f.start == 12 && f.length == 3);
  // In the middle of an expression and of a name: filtered by what is
  // before the caret, the whole name replaced.
  f = nameFragmentAt(QStringLiteral("a + thi * 2"), 6);
  CHECK(f.active && f.typed == QStringLiteral("th") && f.start == 4 && f.length == 3);
  // After an operator: a name.
  CHECK(nameFragmentAt(QStringLiteral("1.5 + x"), 7).active);
  CHECK(nameFragmentAt(QStringLiteral("max(a; wi"), 9).active);
  // Units, numbers, functions: no list.
  CHECK(!nameFragmentAt(QStringLiteral("30 mm"), 5).active);
  CHECK(!nameFragmentAt(QStringLiteral("d1 mm"), 5).active);
  CHECK(!nameFragmentAt(QStringLiteral("(a + b) in"), 10).active);
  CHECK(!nameFragmentAt(QStringLiteral("12mm"), 4).active);
  CHECK(!nameFragmentAt(QStringLiteral("2e3"), 3).active);
  CHECK(!nameFragmentAt(QStringLiteral("ma(1; 2)"), 2).active);
  CHECK(!nameFragmentAt(QStringLiteral("d1 * "), 5).active);
  CHECK(!nameFragmentAt(QString(), 0).active);
  // `name = …` names the field's own parameter: not a reference.
  f = nameFragmentAt(QStringLiteral("width = 3"), 2);
  CHECK(!f.active && f.assigned);
  CHECK(nameFragmentAt(QStringLiteral("wi == 3"), 2).active); // a comparison
  CHECK(nameFragmentAt(QStringLiteral("width = thi"), 11).active);
}

void insertionTests() {
  // Only the name typed is replaced; the rest of the expression stays.
  Insertion in = insertParameter(QStringLiteral("width / 2 + thi"), 15, -1, 0, QStringLiteral("thickness"));
  CHECK(in.text == QStringLiteral("width / 2 + thickness"));
  CHECK(in.caret == 21);
  CHECK(in.from == 12 && in.length == 3);
  in = insertParameter(QStringLiteral("a + thi * 2"), 6, -1, 0, QStringLiteral("thickness"));
  CHECK(in.text == QStringLiteral("a + thickness * 2"));
  CHECK(in.caret == 13);
  // At the caret after an operator, and into an empty field.
  in = insertParameter(QStringLiteral("d1 * "), 5, -1, 0, QStringLiteral("width"));
  CHECK(in.text == QStringLiteral("d1 * width") && in.caret == 10);
  CHECK(insertParameter(QString(), 0, -1, 0, QStringLiteral("width")).text == QStringLiteral("width"));
  // A selection (a value not edited yet) is replaced.
  CHECK(insertParameter(QStringLiteral("10 mm"), 5, 0, 5, QStringLiteral("width")).text == QStringLiteral("width"));
  // At the caret where no name is; never in place of a unit.
  CHECK(insertParameter(QStringLiteral("2 * 3 mm"), 3, -1, 0, QStringLiteral("w")).text ==
        QStringLiteral("2 *w 3 mm"));
  CHECK(insertParameter(QStringLiteral("2 * 3 mm"), 8, -1, 0, QStringLiteral("w")).text ==
        QStringLiteral("2 * 3 mmw"));
  CHECK(expressionPart(QStringLiteral("width = d1 * 2")) == QStringLiteral("d1 * 2"));
  CHECK(expressionPart(QStringLiteral(" w=3")) == QStringLiteral("3"));
  CHECK(expressionPart(QStringLiteral("d1 == 2")) == QStringLiteral("d1 == 2"));
  // Where the expression starts: after the `name =` the match found, not
  // where its text first occurs (mitcad#755).
  CHECK(expressionStart(QStringLiteral("t2 = 2")) == 5);
  CHECK(expressionStart(QStringLiteral("width = w")) == 8);
  CHECK(expressionStart(QStringLiteral("depth = ")) == 8);
  CHECK(expressionStart(QStringLiteral(" w=3")) == 3);
  CHECK(expressionStart(QStringLiteral("d1 == 2")) == 0);
  CHECK(expressionStart(QStringLiteral("  2 * d1")) == 2);
}

void nameTests() {
  CHECK(isValidParameterName(QStringLiteral("width")));
  CHECK(isValidParameterName(QStringLiteral("_a1")));
  CHECK(isValidParameterName(QStringLiteral("breite_ä")));
  CHECK(!isValidParameterName(QStringLiteral("1a")));
  CHECK(!isValidParameterName(QStringLiteral("a b")));
  CHECK(!isValidParameterName(QString()));
  // Units, functions and constants are reserved.
  CHECK(!isValidParameterName(QStringLiteral("mm")));
  CHECK(!isValidParameterName(QStringLiteral("deg")));
  CHECK(!isValidParameterName(QStringLiteral("sin")));
  CHECK(!isValidParameterName(QStringLiteral("PI")));
  // Functions and PI in any case (mitcad#663); units and E only as written.
  CHECK(!isValidParameterName(QStringLiteral("pi")));
  CHECK(!isValidParameterName(QStringLiteral("Max")));
  CHECK(!isValidParameterName(QStringLiteral("SUM")));
  CHECK(!isValidParameterName(QStringLiteral("value")));
  CHECK(isValidParameterName(QStringLiteral("MM")));
  CHECK(isValidParameterName(QStringLiteral("e")));
  CHECK(isValidParameterName(QStringLiteral("maximum")));
  CHECK(kindOfUnit(QString()) == ParameterKind::Unitless);
  CHECK(kindOfUnit(QStringLiteral("deg")) == ParameterKind::Angle);
  CHECK(kindOfUnit(QStringLiteral("rad")) == ParameterKind::Angle);
  CHECK(kindOfUnit(QStringLiteral("mm")) == ParameterKind::Length);
  CHECK(kindOfUnit(QStringLiteral("in")) == ParameterKind::Length);
  CHECK(naturalLess(QStringLiteral("d2"), QStringLiteral("d10")));
  CHECK(!naturalLess(QStringLiteral("d10"), QStringLiteral("d2")));
  CHECK(naturalLess(QStringLiteral("a"), QStringLiteral("B")));
  CHECK(naturalLess(QStringLiteral("d1"), QStringLiteral("d1a")));
  CHECK(kindProblem(ParameterKind::Length, ParameterKind::Unitless).isEmpty());
  CHECK(kindProblem(ParameterKind::Angle, ParameterKind::Angle).isEmpty());
  CHECK(!kindProblem(ParameterKind::Angle, ParameterKind::Length).isEmpty());
  CHECK(!kindProblem(ParameterKind::Unitless, ParameterKind::Length).isEmpty());
  // Texts (mitcad#664): a text field takes texts only, a number field no text.
  CHECK(kindOfUnit(QStringLiteral("Text")) == ParameterKind::Text);
  CHECK(kindProblem(ParameterKind::Text, ParameterKind::Text).isEmpty());
  CHECK(!kindProblem(ParameterKind::Text, ParameterKind::Unitless).isEmpty());
  CHECK(!kindProblem(ParameterKind::Length, ParameterKind::Text).isEmpty());
  CHECK(!kindProblem(ParameterKind::Unitless, ParameterKind::Text).isEmpty());
  CHECK(isValidParameterName(QStringLiteral("Left")));
  CHECK(isValidParameterName(QStringLiteral("len")));
  CHECK(numberWithUnit(QStringLiteral("30"), QStringLiteral("mm")) == QStringLiteral("30 mm"));
  CHECK(numberWithUnit(QStringLiteral("2,5"), QStringLiteral("mm")) == QStringLiteral("2.5 mm"));
  CHECK(numberWithUnit(QStringLiteral("d1 * 2"), QStringLiteral("mm")) == QStringLiteral("d1 * 2"));
  CHECK(numberWithUnit(QStringLiteral("4"), QString()) == QStringLiteral("4"));
}

void matchTests() {
  const std::vector<SuggestedParameter> parameters = suggestedParameters(designParameters());
  CHECK(parameters.size() == 7);
  CHECK(parameters[0].group == SuggestedParameter::Group::User);
  CHECK(parameters[3].group == SuggestedParameter::Group::Model);
  CHECK(parameters[6].reference);
  CHECK(parameters[2].kind == ParameterKind::Angle);
  const auto whole = [](const QString& name) { return name; };
  SuggestContext length;
  // Names that start with the text first, then those that contain it.
  CHECK(names(matchParameters(parameters, QStringLiteral("th"), false, length, whole)) ==
        QStringList({QStringLiteral("thickness"), QStringLiteral("width")}));
  // User parameters first, then the model's in natural order.
  CHECK(names(matchParameters(parameters, QStringLiteral("d"), false, length, whole)) ==
        QStringList({QStringLiteral("width"), QStringLiteral("d1"), QStringLiteral("d2"), QStringLiteral("d3"),
                     QStringLiteral("d10")}));
  // Case-insensitive; similar names all listed, the exact one first.
  CHECK(names(matchParameters(parameters, QStringLiteral("D1"), false, length, whole)) ==
        QStringList({QStringLiteral("d1"), QStringLiteral("d10")}));
  // No match.
  CHECK(matchParameters(parameters, QStringLiteral("xyz"), false, length, whole).empty());
  // Comments only in the browser's search.
  CHECK(matchParameters(parameters, QStringLiteral("wall"), false, length, whole).empty());
  CHECK(names(matchParameters(parameters, QStringLiteral("wall"), true, length, whole)) ==
        QStringList({QStringLiteral("thickness")}));
  // Everything for an empty filter.
  CHECK(matchParameters(parameters, QString(), true, length, whole).size() == 7);

  // An angle as a length field's whole expression is explained, not
  // offered as valid; inside an expression it is fine.
  std::vector<ParameterChoice> choices = matchParameters(parameters, QStringLiteral("ti"), false, length, whole);
  CHECK(choices.size() == 1 && !choices.front().problem.isEmpty());
  const auto inside = [](const QString& name) { return QStringLiteral("d1 * tan(%1)").arg(name); };
  choices = matchParameters(parameters, QStringLiteral("ti"), false, length, inside);
  CHECK(choices.size() == 1 && choices.front().problem.isEmpty());
  SuggestContext angle;
  angle.kind = ParameterKind::Angle;
  choices = matchParameters(parameters, QStringLiteral("t"), false, angle, whole);
  CHECK(choiceOf(choices, QStringLiteral("tilt")) && choiceOf(choices, QStringLiteral("tilt"))->problem.isEmpty());
  CHECK(choiceOf(choices, QStringLiteral("thickness")) &&
        !choiceOf(choices, QStringLiteral("thickness"))->problem.isEmpty());

  // The field's own parameter is not offered; those that use it would
  // make a cycle.
  SuggestContext own;
  own.own = QStringLiteral("d1");
  choices = matchParameters(parameters, QStringLiteral("d"), false, own, whole);
  CHECK(!choiceOf(choices, QStringLiteral("d1")));
  CHECK(choiceOf(choices, QStringLiteral("d2")) && !choiceOf(choices, QStringLiteral("d2"))->problem.isEmpty());
  CHECK(choiceOf(choices, QStringLiteral("d10")) && choiceOf(choices, QStringLiteral("d10"))->problem.isEmpty());
  CHECK(dependentsOf(parameters, QStringLiteral("width")) == QStringList({QStringLiteral("d1"), QStringLiteral("d2")}));
  // A sketch cannot use what its own driven dimensions measure.
  SuggestContext sketch;
  sketch.sketch = QStringLiteral("F1");
  choices = matchParameters(parameters, QStringLiteral("d3"), false, sketch, whole);
  CHECK(choices.size() == 1 && !choices.front().problem.isEmpty());
  sketch.sketch = QStringLiteral("F4");
  choices = matchParameters(parameters, QStringLiteral("d3"), false, sketch, whole);
  CHECK(choices.size() == 1 && choices.front().problem.isEmpty());

  // New names: taken, reserved, invalid, free.
  const SuggestedParameter* existing = nullptr;
  CHECK(!newNameProblem(parameters, QStringLiteral("width"), &existing).isEmpty());
  CHECK(existing != nullptr && existing->name == QStringLiteral("width"));
  CHECK(!newNameProblem(parameters, QStringLiteral("mm"), &existing).isEmpty() && existing == nullptr);
  CHECK(!newNameProblem(parameters, QStringLiteral("1x")).isEmpty());
  CHECK(!newNameProblem(parameters, QString()).isEmpty());
  CHECK(newNameProblem(parameters, QStringLiteral("depth")).isEmpty());
}

// Sends a key to the field as typing does: its shortcut override first
// (the window's Enter and Esc shortcuts only fire when it is not
// accepted), then the press. Returns whether the override was accepted.
bool press(QLineEdit& edit, int key, const QString& text = QString()) {
  QKeyEvent override(QEvent::ShortcutOverride, key, Qt::NoModifier, text);
  override.ignore();
  QApplication::sendEvent(&edit, &override);
  const bool taken = override.isAccepted();
  QKeyEvent down(QEvent::KeyPress, key, Qt::NoModifier, text);
  QApplication::sendEvent(&edit, &down);
  QKeyEvent up(QEvent::KeyRelease, key, Qt::NoModifier, text);
  QApplication::sendEvent(&edit, &up);
  return taken;
}

// Letters, spaces and `+`, as typed.
void type(QLineEdit& edit, const QString& text) {
  for (const QChar c : text) {
    int key = static_cast<int>(Qt::Key_Space);
    if (c.isLetter()) {
      key = static_cast<int>(Qt::Key_A) + (static_cast<int>(c.toUpper().unicode()) - static_cast<int>('A'));
    } else if (c == QLatin1Char('+')) {
      key = static_cast<int>(Qt::Key_Plus);
    }
    press(edit, key, QString(c));
  }
}

void fieldTests() {
  QWidget window;
  window.resize(640, 480);
  int added = 0;
  setParameterSource(&window, {[] { return designParameters(); },
                               [&added](const QJsonObject&, QString*) {
                                 ++added;
                                 return true;
                               },
                               [] { return QStringLiteral("mm"); }});
  auto* edit = new QLineEdit(&window);
  edit->setGeometry(20, 20, 200, 24);
  ParameterFieldOptions options;
  options.label = QStringLiteral("test distance");
  options.create = false; // no modal dialog in a unit test
  ParameterField* field = ParameterField::attach(edit, options);
  CHECK(field != nullptr && ParameterField::of(edit) == field);
  CHECK(ParameterField::attach(edit, options) == field); // once per field
  CHECK(parameterSourceOf(edit) != nullptr);
  int returns = 0;
  int edits = 0;
  QString edited;
  QObject::connect(edit, &QLineEdit::returnPressed, [&returns] { ++returns; });
  QObject::connect(edit, &QLineEdit::textEdited, [&edits, &edited](const QString& text) {
    ++edits;
    edited = text;
  });
  window.show();

  // Completion after an operator: Enter puts the reference in place of
  // the name typed, and is not the field's Enter.
  edit->setText(QStringLiteral("width / 2 + "));
  edit->setCursorPosition(static_cast<int>(edit->text().size()));
  type(*edit, QStringLiteral("thi"));
  CHECK(field->isOpen());
  edits = 0;
  CHECK(press(*edit, Qt::Key_Return));
  CHECK(edit->text() == QStringLiteral("width / 2 + thickness"));
  CHECK(edit->cursorPosition() == 21);
  CHECK(!field->isOpen());
  CHECK(returns == 0);
  CHECK(edits == 1 && edited == QStringLiteral("width / 2 + thickness")); // the field's reactions follow
  // With the list closed, Enter is the field's (and the window's) again.
  CHECK(!press(*edit, Qt::Key_Return));
  CHECK(returns == 1);

  // Esc closes the list only: the text stays as typed.
  type(*edit, QStringLiteral(" + w"));
  CHECK(field->isOpen());
  CHECK(press(*edit, Qt::Key_Escape));
  CHECK(!field->isOpen());
  CHECK(edit->text() == QStringLiteral("width / 2 + thickness + w"));
  CHECK(!press(*edit, Qt::Key_Escape)); // the window's Esc again

  // In the middle of an expression; Down moves to the next choice.
  edit->setText(QStringLiteral("2 *  + 1"));
  edit->setCursorPosition(4);
  type(*edit, QStringLiteral("d"));
  CHECK(field->isOpen()); // width (contains d), d1, d2, d3, d10
  CHECK(press(*edit, Qt::Key_Down));
  CHECK(press(*edit, Qt::Key_Return));
  CHECK(edit->text() == QStringLiteral("2 * d1 + 1"));

  // A choice the field cannot take is explained, not put in.
  edit->clear();
  type(*edit, QStringLiteral("ti"));
  CHECK(field->isOpen());
  press(*edit, Qt::Key_Return);
  CHECK(edit->text() == QStringLiteral("ti"));
  CHECK(field->isOpen());
  press(*edit, Qt::Key_Escape);

  // Exactly the one name typed: no list, Enter is the field's.
  edit->clear();
  type(*edit, QStringLiteral("width"));
  CHECK(!field->isOpen());

  // No match: the list says so.
  edit->clear();
  type(*edit, QStringLiteral("xyz"));
  CHECK(field->isOpen());
  press(*edit, Qt::Key_Escape);
  // A unit: no list.
  edit->setText(QStringLiteral("30 "));
  edit->setCursorPosition(3);
  type(*edit, QStringLiteral("mm"));
  CHECK(!field->isOpen());

  // The fx browser: what is typed searches names and comments, the field
  // keeps its text until a choice.
  edit->setText(QStringLiteral("2 * "));
  edit->setCursorPosition(4);
  field->openBrowser();
  CHECK(field->isOpen());
  type(*edit, QStringLiteral("wall"));
  CHECK(edit->text() == QStringLiteral("2 * "));
  CHECK(field->isOpen());
  CHECK(press(*edit, Qt::Key_Return));
  CHECK(edit->text() == QStringLiteral("2 * thickness"));
  // Opened on an empty field: the first choice it can take.
  edit->clear();
  field->openBrowser();
  CHECK(field->isOpen());
  press(*edit, Qt::Key_Return);
  CHECK(edit->text() == QStringLiteral("thickness"));
  // fx again closes it.
  field->openBrowser();
  field->openBrowser();
  CHECK(!field->isOpen());

  // The field's own parameter is not offered (mitcad#231 fields): nothing
  // matches, and Enter puts nothing in.
  field->setOwnParameter(QStringLiteral("width"));
  edit->clear();
  type(*edit, QStringLiteral("wid"));
  CHECK(field->isOpen());
  press(*edit, Qt::Key_Return);
  CHECK(edit->text() == QStringLiteral("wid"));
  press(*edit, Qt::Key_Escape);
  CHECK(!field->isOpen());
  field->setOwnParameter(QString());

  // `name = …` (mitcad#568): a new name typed first is offered as the
  // field's own parameter's name.
  auto* named = new QLineEdit(&window);
  named->setGeometry(20, 60, 200, 24);
  ParameterFieldOptions naming;
  naming.label = QStringLiteral("test dimension");
  naming.naming = true;
  naming.create = false;
  ParameterField* namedField = ParameterField::attach(named, naming);
  named->show();
  type(*named, QStringLiteral("depth"));
  CHECK(namedField->isOpen()); // no match, and the action
  press(*named, Qt::Key_Return);
  CHECK(named->text() == QStringLiteral("depth = "));
  CHECK(!namedField->isOpen());
  // On a value: its parameter's name before it, selected to type over.
  named->setText(QStringLiteral("30 mm"));
  namedField->setOwnParameter(QStringLiteral("d4"));
  namedField->startNaming();
  CHECK(named->text() == QStringLiteral("d4 = 30 mm"));
  CHECK(named->selectedText() == QStringLiteral("d4"));
  CHECK(added == 0);
}

// Runs `opener`, which shows a modal dialog or a menu and returns once it
// closes, with `act` given the dialog or menu as soon as it shows; a
// watchdog closes it when `act` did not.
void whileOpen(const std::function<void()>& opener, const std::function<void(QWidget*)>& act) {
  QTimer watchdog;
  watchdog.setSingleShot(true);
  QObject::connect(&watchdog, &QTimer::timeout, [] {
    check(false, "the dialog or menu is closed by the test's action", __LINE__);
    if (QWidget* popup = QApplication::activePopupWidget()) {
      popup->close();
    } else if (QWidget* modal = QApplication::activeModalWidget()) {
      modal->close();
    }
  });
  watchdog.start(3000);
  QObject alive; // the action is not run after this returns
  QTimer::singleShot(0, &alive, [&act] {
    QWidget* shown = QApplication::activePopupWidget();
    if (shown == nullptr) {
      shown = QApplication::activeModalWidget();
    }
    CHECK(shown != nullptr);
    if (shown != nullptr) {
      act(shown);
    }
  });
  opener();
  watchdog.stop();
}

// Chooses the menu's action whose text starts with `name` (without its
// `&` and shortcut) as a user does: made current, then Enter.
void choose(QMenu* menu, const QString& name) {
  CHECK(menu != nullptr);
  if (menu == nullptr) {
    return;
  }
  QAction* found = nullptr;
  const QList<QAction*> actions = menu->actions();
  for (QAction* action : actions) {
    if (action->text().remove(QLatin1Char('&')).startsWith(name)) {
      found = action;
      break;
    }
  }
  CHECK(found != nullptr && found->isEnabled());
  if (found == nullptr) {
    return;
  }
  menu->setActiveAction(found);
  QKeyEvent enter(QEvent::KeyPress, Qt::Key_Return, Qt::NoModifier);
  QApplication::sendEvent(menu, &enter);
}

// The field's context menu, as a right click opens it.
void openContextMenu(QLineEdit* edit) {
  const QPoint at(5, edit->height() / 2);
  QContextMenuEvent request(QContextMenuEvent::Mouse, at, edit->mapToGlobal(at));
  QApplication::sendEvent(edit, &request);
}

// What a command panel does as its value field loses the keyboard
// (CommandPanel::eventFilter).
class PanelFocus : public QObject {
public:
  explicit PanelFocus(QLineEdit* edit) : QObject(edit), m_edit(edit) { edit->installEventFilter(this); }
  int leftAt = -1;

protected:
  bool eventFilter(QObject* watched, QEvent* event) override {
    if (watched == m_edit && event->type() == QEvent::FocusOut) {
      const auto& focus = *static_cast<QFocusEvent*>(event);
      // The offscreen platform the unit tests run on activates a menu's
      // own window and, as the menu closes, leaves no window active: the
      // field then loses the keyboard once more (ActiveWindowFocusReason)
      // after the menu gave it back. A desktop's menu takes no activation,
      // so that loss is not the user's and is ignored here.
      if (focus.reason() == Qt::ActiveWindowFocusReason &&
          QGuiApplication::platformName() == QLatin1String("offscreen")) {
        return true;
      }
      const int was = leaveValueField(m_edit, focus);
      if (was >= 0) {
        leftAt = was;
      }
    }
    return QObject::eventFilter(watched, event);
  }

private:
  QLineEdit* m_edit;
};

// Edit Formula… keeps the field's `name =` where the expression's text
// also occurs in the name (mitcad#755).
void formulaPrefixTests() {
  QWidget window;
  window.resize(640, 480);
  setParameterSource(&window, {[] { return designParameters(); },
                               [](const QJsonObject&, QString*) { return true; },
                               [] { return QStringLiteral("mm"); },
                               [](const QJsonObject&) { return QJsonObject(); }});
  auto* edit = new QLineEdit(&window);
  edit->setGeometry(20, 20, 200, 24);
  ParameterFieldOptions options;
  options.label = QStringLiteral("test dimension");
  options.naming = true;
  options.create = false;
  ParameterField* field = ParameterField::attach(edit, options);
  window.show();
  // The field's text after the editor's OK, with `formula` typed in it
  // (left as it opened when empty).
  const auto apply = [edit, field](const QString& text, const QString& formula) {
    edit->setText(text);
    whileOpen([field] { field->openFormulaEditor(); },
              [&formula](QWidget* shown) {
                auto* dialog = dynamic_cast<FormulaEditor*>(shown);
                CHECK(dialog != nullptr);
                if (dialog == nullptr) {
                  shown->close();
                  return;
                }
                auto* input = dialog->findChild<QPlainTextEdit*>(QStringLiteral("formulaText"));
                CHECK(input != nullptr);
                if (input != nullptr && !formula.isEmpty()) {
                  input->setPlainText(formula);
                }
                dialog->accept();
              });
    return edit->text();
  };
  CHECK(apply(QStringLiteral("t2 = 2"), QString()) == QStringLiteral("t2 = 2"));
  CHECK(apply(QStringLiteral("t2 = 2"), QStringLiteral("3")) == QStringLiteral("t2 = 3"));
  CHECK(apply(QStringLiteral("width = w"), QString()) == QStringLiteral("width = w"));
  CHECK(apply(QStringLiteral("width = w"), QStringLiteral("w * 2")) == QStringLiteral("width = w * 2"));
  CHECK(apply(QStringLiteral("20 + 5"), QStringLiteral("25")) == QStringLiteral("25"));
}

// A field's own tooltip (its parameter's name, expression and comment,
// mitcad#250) shows where the formula help has nothing to say; the
// value of the part under the pointer where it has (mitcad#761).
void tooltipTests() {
  QWidget window;
  window.resize(640, 480);
  setParameterSource(&window, {[] { return designParameters(); },
                               [](const QJsonObject&, QString*) { return true; },
                               [] { return QStringLiteral("mm"); },
                               [](const QJsonObject& asked) {
                                 if (!asked.value(QStringLiteral("expression")).toString().contains(
                                         QStringLiteral("d1"))) {
                                   return QJsonObject();
                                 }
                                 return QJsonObject{
                                     {QStringLiteral("selection"),
                                      QJsonObject{{QStringLiteral("text"), QStringLiteral("d1 = 35 mm")}}}};
                               }});
  auto* edit = new QLineEdit(&window);
  edit->setGeometry(20, 20, 200, 24);
  edit->setToolTip(QStringLiteral("own tip"));
  ParameterFieldOptions options;
  options.label = QStringLiteral("test distance");
  options.create = false;
  ParameterField::attach(edit, options);
  window.show();
  const QPoint at(5, edit->height() / 2);
  edit->setText(QStringLiteral("20 + 5"));
  QHelpEvent plain(QEvent::ToolTip, at, edit->mapToGlobal(at));
  QApplication::sendEvent(edit, &plain);
  CHECK(QToolTip::text() == QStringLiteral("own tip"));
  edit->setText(QStringLiteral("d1 * 2"));
  QHelpEvent onPart(QEvent::ToolTip, at, edit->mapToGlobal(at));
  QApplication::sendEvent(edit, &onPart);
  CHECK(QToolTip::text() == QStringLiteral("d1 = 35 mm"));
  QToolTip::hideText();
}

// A value field's context menu acts on the caret and the selection the
// field had: opening it does not take the field back to its start
// (mitcad#756), leaving the field for good does (mitcad#656).
void fieldMenuTests() {
  QWidget window;
  window.resize(640, 480);
  setParameterSource(&window, {[] { return designParameters(); },
                               [](const QJsonObject&, QString*) { return true; },
                               [] { return QStringLiteral("mm"); }});
  auto* edit = new QLineEdit(&window);
  edit->setGeometry(20, 20, 200, 24);
  ParameterFieldOptions options;
  options.label = QStringLiteral("test distance");
  options.create = false;
  ParameterField* field = ParameterField::attach(edit, options);
  auto* panel = new PanelFocus(edit);
  window.show();
  window.activateWindow();
  QApplication::processEvents();
  edit->setFocus(Qt::OtherFocusReason);
  QApplication::processEvents();
  CHECK(edit->hasFocus()); // else the menu would take no focus from it

  // "20" of "20 + 5" selected: Copy copies it.
  edit->setText(QStringLiteral("20 + 5"));
  edit->setSelection(0, 2);
  QApplication::clipboard()->setText(QStringLiteral("nothing"));
  whileOpen([edit] { openContextMenu(edit); },
            [edit](QWidget* shown) {
              CHECK(edit->selectedText() == QStringLiteral("20"));
              CHECK(edit->cursorPosition() == 2);
              choose(qobject_cast<QMenu*>(shown), QStringLiteral("Copy"));
            });
  CHECK(QApplication::clipboard()->text() == QStringLiteral("20"));
  CHECK(edit->selectedText() == QStringLiteral("20"));
  CHECK(panel->leftAt == -1);

  // The caret at the end of "2 * ": Insert Parameter… puts the choice there.
  edit->setText(QStringLiteral("2 * "));
  edit->setCursorPosition(4);
  whileOpen([edit] { openContextMenu(edit); },
            [edit](QWidget* shown) {
              CHECK(edit->cursorPosition() == 4);
              choose(qobject_cast<QMenu*>(shown), QStringLiteral("Insert Parameter"));
            });
  QApplication::processEvents(); // the browser opens after the menu
  CHECK(field->isOpen());
  type(*edit, QStringLiteral("wall"));
  CHECK(press(*edit, Qt::Key_Return));
  CHECK(edit->text() == QStringLiteral("2 * thickness"));

  // Leaving the field otherwise shows it from its start again and keeps
  // where the caret was.
  edit->setCursorPosition(4);
  QFocusEvent away(QEvent::FocusOut, Qt::MouseFocusReason);
  QApplication::sendEvent(edit, &away);
  CHECK(panel->leftAt == 4);
  CHECK(edit->cursorPosition() == 0);
}

// Tables (mitcad#667): `Screws.` and its columns, as `formula_help` gives
// them.
QJsonArray designTables() {
  const auto column = [](const char* name, const char* unit) {
    return QJsonObject{{QStringLiteral("name"), QString::fromUtf8(name)},
                       {QStringLiteral("unit"), QString::fromUtf8(unit)}};
  };
  return {QJsonObject{{QStringLiteral("name"), QStringLiteral("Screws")},
                      {QStringLiteral("comment"), QStringLiteral("Socket head cap screws")},
                      {QStringLiteral("columns"), QJsonArray{column("Size", "Text"), column("d", "mm"),
                                                             column("dk", "mm"), column("k", "mm")}}},
          QJsonObject{{QStringLiteral("name"), QStringLiteral("Coins")},
                      {QStringLiteral("columns"), QJsonArray{column("Name", "Text"), column("Dia", "mm")}}}};
}

void tableTests() {
  const std::vector<SuggestedTable> tables = suggestedTables(designTables());
  CHECK(tables.size() == 2 && tables.front().columns.size() == 4);
  CHECK(tableNamed(tables, QStringLiteral("Screws")) == &tables.front());
  CHECK(tableNamed(tables, QStringLiteral("screws")) == nullptr); // names are case-sensitive
  CHECK(columnLabel(tables.front().columns.front()) == QStringLiteral("Size [Text]"));

  // After the dot, also before anything is typed.
  ColumnFragment f = columnFragmentAt(QStringLiteral("XLOOKUP(Size; Screws."), 21);
  CHECK(f.active && f.table == QStringLiteral("Screws") && f.start == 21 && f.length == 0 && f.typed.isEmpty());
  f = columnFragmentAt(QStringLiteral("INDEX(Screws.dk; 2)"), 14);
  CHECK(f.active && f.table == QStringLiteral("Screws") && f.start == 13 && f.length == 2 &&
        f.typed == QStringLiteral("d"));
  // A number's decimal point, a chain, a name without a dot: none.
  CHECK(!columnFragmentAt(QStringLiteral("2.5"), 3).active);
  CHECK(!columnFragmentAt(QStringLiteral("a.b.c"), 5).active);
  CHECK(!columnFragmentAt(QStringLiteral("Screws"), 6).active);
  CHECK(!columnFragmentAt(QStringLiteral("Screws.5"), 8).active);
  // The column's name is no parameter's name being typed.
  CHECK(!nameFragmentAt(QStringLiteral("Screws.dk"), 9).active);
  // The lookup functions are reserved names.
  CHECK(isReservedName(QStringLiteral("XLOOKUP")) && isReservedName(QStringLiteral("match")) &&
        isReservedName(QStringLiteral("Index")));

  // Columns starting with the filter first, then containing it.
  std::vector<SuggestedTable::Column> columns = matchColumns(tables.front(), QStringLiteral("d"));
  CHECK(columns.size() == 2 && columns[0].name == QStringLiteral("d") && columns[1].name == QStringLiteral("dk"));
  columns = matchColumns(tables.front(), QStringLiteral("k"));
  CHECK(columns.size() == 2 && columns[0].name == QStringLiteral("k") && columns[1].name == QStringLiteral("dk"));
  CHECK(matchColumns(tables.front(), QString()).size() == 4);
  // Tables by name; with comments, by their comments too.
  CHECK(matchTables(tables, QStringLiteral("scr"), false).size() == 1);
  CHECK(matchTables(tables, QStringLiteral("cap"), false).empty());
  CHECK(matchTables(tables, QStringLiteral("cap"), true).size() == 1);

  // A field: `Scr` offers the table, choosing it puts `Screws.` in and the
  // list goes on with its columns; `d` and Down choose dk.
  QWidget window;
  window.resize(640, 480);
  ParameterSource source{[] { return designParameters(); }, nullptr, [] { return QStringLiteral("mm"); }};
  source.query = [](const QJsonObject&) {
    return QJsonObject{{QStringLiteral("functions"), QJsonArray()}, {QStringLiteral("tables"), designTables()}};
  };
  setParameterSource(&window, source);
  auto* edit = new QLineEdit(&window);
  edit->setGeometry(20, 20, 300, 24);
  ParameterFieldOptions options;
  options.label = QStringLiteral("table test");
  options.create = false;
  options.anyKind = true;
  ParameterField* field = ParameterField::attach(edit, options);
  window.show();
  edit->setFocus();
  edit->setText(QStringLiteral("XLOOKUP(Size; "));
  edit->setCursorPosition(static_cast<int>(edit->text().size()));
  type(*edit, QStringLiteral("Scr"));
  CHECK(field->isOpen());
  CHECK(press(*edit, Qt::Key_Return));
  QApplication::processEvents(); // the columns are listed from a queued call
  CHECK(edit->text() == QStringLiteral("XLOOKUP(Size; Screws."));
  CHECK(field->isOpen());
  type(*edit, QStringLiteral("d"));
  CHECK(field->isOpen());
  CHECK(press(*edit, Qt::Key_Down));
  CHECK(press(*edit, Qt::Key_Return));
  CHECK(edit->text() == QStringLiteral("XLOOKUP(Size; Screws.dk"));
  CHECK(!field->isOpen());
}

} // namespace

// A key with modifiers, as press() sends one: whether the field took its
// shortcut override.
bool pressWith(QLineEdit& edit, int key, Qt::KeyboardModifiers modifiers) {
  QKeyEvent override(QEvent::ShortcutOverride, key, modifiers);
  override.ignore();
  QApplication::sendEvent(&edit, &override);
  const bool taken = override.isAccepted();
  QKeyEvent down(QEvent::KeyPress, key, modifiers);
  QApplication::sendEvent(&edit, &down);
  QKeyEvent up(QEvent::KeyRelease, key, modifiers);
  QApplication::sendEvent(&edit, &up);
  return taken;
}

// Up and Down step a plain value with the list closed (mitcad#1254): the
// new text as typed (textEdited), Shift by 10, the fine modifier by 0.1,
// the minimum, an expression left alone, a field that does not step; the
// wheel over a field without the keyboard is not the field's; the
// tooltip's line.
void stepTests() {
#ifdef Q_OS_MACOS
  const bool macKeys = true;
  const Qt::KeyboardModifier fine = Qt::AltModifier;
#else
  const bool macKeys = false;
  const Qt::KeyboardModifier fine = Qt::ControlModifier;
#endif
  QWidget window;
  window.resize(640, 480);
  setParameterSource(&window, {[] { return designParameters(); },
                               [](const QJsonObject&, QString*) { return true; },
                               [] { return QStringLiteral("mm"); }});
  auto* edit = new QLineEdit(&window);
  edit->setGeometry(20, 20, 200, 24);
  ParameterFieldOptions options;
  options.kind = ParameterKind::Unitless;
  options.label = QStringLiteral("test quantity");
  options.create = false;
  options.steps = true;
  options.stepLimit.minimum = 1.0;
  ParameterField* field = ParameterField::attach(edit, options);
  int edits = 0;
  QString edited;
  QObject::connect(edit, &QLineEdit::textEdited, [&edits, &edited](const QString& text) {
    ++edits;
    edited = text;
  });
  auto* other = new QLineEdit(&window);
  other->setGeometry(20, 60, 200, 24);
  window.show();

  edit->setText(QStringLiteral("4"));
  CHECK(pressWith(*edit, Qt::Key_Up, Qt::NoModifier)); // the field's key, not a shortcut
  CHECK(edit->text() == QStringLiteral("5"));
  CHECK(edits == 1 && edited == QStringLiteral("5"));
  CHECK(!field->isOpen());
  CHECK(pressWith(*edit, Qt::Key_Up, Qt::ShiftModifier));
  CHECK(edit->text() == QStringLiteral("15"));
  CHECK(pressWith(*edit, Qt::Key_Down, Qt::ShiftModifier));
  CHECK(edit->text() == QStringLiteral("5"));
  // Down stops at the minimum: 5 - 10 is 1, and 1 stays.
  CHECK(pressWith(*edit, Qt::Key_Down, Qt::ShiftModifier));
  CHECK(edit->text() == QStringLiteral("1"));
  edits = 0;
  CHECK(pressWith(*edit, Qt::Key_Down, Qt::NoModifier));
  CHECK(edit->text() == QStringLiteral("1") && edits == 0);
  CHECK(pressWith(*edit, Qt::Key_Up, fine));
  CHECK(edit->text() == QStringLiteral("1.1"));
  // A modifier that does not step leaves the value as it is.
  pressWith(*edit, Qt::Key_Up, Qt::MetaModifier);
  CHECK(edit->text() == QStringLiteral("1.1"));
  // An expression is not stepped; the keys do nothing.
  edit->setText(QStringLiteral("d1*2"));
  edits = 0;
  CHECK(pressWith(*edit, Qt::Key_Up, Qt::NoModifier));
  CHECK(edit->text() == QStringLiteral("d1*2") && edits == 0);

  // A length in the unit it shows.
  auto* length = new QLineEdit(&window);
  length->setGeometry(20, 100, 200, 24);
  ParameterFieldOptions lengthOptions;
  lengthOptions.label = QStringLiteral("test distance");
  lengthOptions.create = false;
  lengthOptions.steps = true;
  ParameterField::attach(length, lengthOptions);
  length->setText(QStringLiteral("10 mm"));
  CHECK(pressWith(*length, Qt::Key_Up, Qt::ShiftModifier));
  CHECK(length->text() == QStringLiteral("20 mm"));

  // A field that does not step (a table's cell): Up leaves its value.
  ParameterFieldOptions plainOptions;
  plainOptions.label = QStringLiteral("test cell");
  plainOptions.create = false;
  ParameterField::attach(other, plainOptions);
  other->setText(QStringLiteral("4"));
  pressWith(*other, Qt::Key_Up, Qt::NoModifier);
  CHECK(other->text() == QStringLiteral("4"));

  // The wheel over a field without the keyboard changes nothing.
  edit->setText(QStringLiteral("4"));
  edits = 0;
  if (!edit->hasFocus()) {
    const QPointF at(5, 12);
    QWheelEvent wheel(at, edit->mapToGlobal(at), QPoint(), QPoint(0, 120), Qt::NoButton, Qt::NoModifier,
                      Qt::NoScrollPhase, false);
    QApplication::sendEvent(edit, &wheel);
    CHECK(edit->text() == QStringLiteral("4") && edits == 0);
  }

  // The tooltip: the field's own, and how the value steps.
  edit->setToolTip(QStringLiteral("own tip"));
  const QPoint at(5, edit->height() / 2);
  QHelpEvent help(QEvent::ToolTip, at, edit->mapToGlobal(at));
  QApplication::sendEvent(edit, &help);
  CHECK(QToolTip::text() == QStringLiteral("own tip\n") + valueStepHint(macKeys));
  QToolTip::hideText();
}

int parameterSuggestTests() {
  failures = 0;
  fragmentTests();
  insertionTests();
  nameTests();
  matchTests();
  fieldTests();
  formulaPrefixTests();
  fieldMenuTests();
  tooltipTests();
  tableTests();
  stepTests();
  return failures;
}
