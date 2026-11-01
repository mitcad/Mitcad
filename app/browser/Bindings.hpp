// SPDX-License-Identifier: MIT
#pragma once

#include <QDialog>
#include <QIcon>
#include <QPair>
#include <QString>
#include <QVector>

class QLineEdit;
class QPushButton;
class QShowEvent;

namespace mitcad {

// Bindings (mitcad#809; the model's mitcad#800, commands.md "Bindings:
// values from expressions"): a body's appearance and material, a face's
// appearance, a feature's suppression and the visibility of a body or an
// occurrence follow an expression of parameters. The context menus open the
// dialogs below for the conditions; an appearance or a material varies with
// an option by the fx of the Material and Appearance tools (mitcad#861,
// mitcad#1059). The browser and the timeline mark a bound item with an fx
// badge, the expression in its tooltip.

// An icon with a small "fx" in its lower right corner: the item follows an
// expression.
QIcon boundIcon(const QIcon& icon);

// A text as an expression's text literal: 'It''s'.
QString quotedText(const QString& text);
// The text of a text literal ('It''s' gives It's); the text itself when it
// is none.
QString unquotedText(const QString& expression);

// A mapping from an option's values to results, as the assignment panel
// writes it (mitcad#861):
// switch(Option; v1; r1; v2; r2; ...; otherwise). Each value and result is
// an expression text ('Black', 800 mm, 'paint_black').
struct OptionMapping {
  QString option;
  QVector<QPair<QString, QString>> rows;
  QString otherwise;

  QString expression() const;
  // The mapping of an expression of that form; false for any other form
  // (the Formula Editor edits those).
  static bool parse(const QString& source, OptionMapping* mapping);
};

// Suppress When… and Show When…: a condition (an expression field with the
// parameters' suggestions and the Formula Editor). OK sets it (an empty
// field removes the condition and keeps the value it gave); Remove
// Condition removes it.
class ConditionDialog : public QDialog {
  Q_OBJECT

public:
  // `what` is the dialog's title ("Suppress Extrude2 When"), `help` the line
  // above the field.
  ConditionDialog(const QString& what, const QString& help, const QString& initial, QWidget* parent = nullptr);

  // The condition to set; empty: remove it.
  QString condition() const;

protected:
  void showEvent(QShowEvent* event) override;

private:
  void openFormulaEditor();
  void logPlaces();

  QLineEdit* m_condition = nullptr;
  QPushButton* m_formula = nullptr;
  QPushButton* m_remove = nullptr;
  QPushButton* m_ok = nullptr;
  bool m_removed = false;
};

} // namespace mitcad
