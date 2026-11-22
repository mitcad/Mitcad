// SPDX-License-Identifier: MIT
#pragma once

#include <QColor>
#include <QCoreApplication>
#include <QHash>
#include <QDialog>
#include <QJsonObject>
#include <QString>

#include "ParameterField.hpp"

class QCheckBox;
class QLabel;
class QLineEdit;
class QPlainTextEdit;
class QTreeWidget;
class QTreeWidgetItem;
class QShowEvent;
class QDialogButtonBox;
class QListWidget;
class QListWidgetItem;
class QPalette;
class QTextCharFormat;

namespace mitcad {

class FormulaHighlighter;

// Text is always the expression: templates and formatting only edit that text.
class FormulaEditor : public QDialog {
  Q_DECLARE_TR_FUNCTIONS(mitcad::FormulaEditor)

public:
  FormulaEditor(ParameterSource source, ParameterFieldOptions options, const QString& expression,
                QWidget* parent = nullptr);
  QString expression() const;
  bool limitsChanged() const;
  QJsonObject parameterCommand() const;
  void accept() override;
  QSize sizeHint() const override;
  // The kind of the formula's token at a position of the text (UTF-16), as
  // the colours show it ("name", "unknown", "function", "unit", ...;
  // formulaTokenColor), empty between tokens: for UI tests (mitcad#2179).
  QString tokenKindAt(int position) const;

protected:
  void showEvent(QShowEvent* event) override;
  void changeEvent(QEvent* event) override;
  bool eventFilter(QObject* watched, QEvent* event) override;

private:
  void updateHelp();
  void filterFunctions(const QString& filter);
  void insertFunction(QTreeWidgetItem* item);
  void insertTemplate(int index);
  void indentLet();
  void inspectAt(int position, int end = -1);
  QJsonObject help(int position, int end = -1) const;

  ParameterSource m_source;
  ParameterFieldOptions m_options;
  QPlainTextEdit* m_expression = nullptr;
  QTreeWidget* m_functions = nullptr;
  QLabel* m_signature = nullptr;
  QLabel* m_value = nullptr;
  QLabel* m_part = nullptr;
  QLabel* m_error = nullptr;
  QLineEdit* m_min = nullptr;
  QLineEdit* m_max = nullptr;
  QLineEdit* m_step = nullptr;
  QCheckBox* m_strict = nullptr;
  QJsonObject m_originalLimits;
  bool m_canLimit = false;

  // The code editor (mitcad#2179): the text coloured by the kinds of its
  // tokens, which the model's tokenizer gives on each change (the
  // `formula_help` query's `tokens`); the line with the caret, the bracket
  // at the caret and its match, and the error's range marked over them.
  void updateTokens();
  void updateMarks();
  FormulaHighlighter* m_highlighter = nullptr;
  bool m_updatingTokens = false;
  int m_errorStart = -1; // the error's range in the text (UTF-16), -1 none
  int m_errorEnd = -1;

  // Completion of what is typed (mitcad#667): a name offers the
  // parameters, the tables (`Screws.`) and the functions (`XLOOKUP(`)
  // that start with it; after `Screws.` the table's columns. Up and Down
  // choose, Enter or Tab puts the choice in place of the name typed, Esc
  // closes the list only.
  void loadCompletions();
  // mayOpen: what was typed opens the list; otherwise only a list shown
  // follows (Backspace) or closes.
  void updateCompletions(bool mayOpen);
  void complete(QListWidgetItem* item);
  void hideCompletions();
  QListWidget* m_completions = nullptr;
  // OK and Cancel, whose places are logged for UI tests.
  QDialogButtonBox* m_buttons = nullptr;
  void logButtons();
  QStringList m_parameterNames;
  QStringList m_functionNames;
  // What the list says beside a name (mitcad#2179): a parameter's value in
  // its unit, a function's arguments ("function (value; [step])"); the
  // units, offered after a value.
  QHash<QString, QString> m_parameterValues;
  QHash<QString, QString> m_functionSignatures;
  QStringList m_unitNames;
  std::vector<SuggestedTable> m_tables;
  int m_completionStart = 0;
  int m_completionLength = 0;
  bool m_completing = false;
  // What the key being handled does to the text: 1 types, 2 erases, 0
  // neither (a caret move closes the list, never opens it).
  int m_typing = 0;
  QString m_loggedCompletions;
};

QString formulaSignatureHtml(const QJsonObject& signature);
QString formulaKindName(ParameterKind kind);
// The colour and the format of a formula token's kind (mitcad#2179) on a
// palette, light or dark: names, functions, keywords, numbers, units,
// operators and texts each their own ColorRole, an unknown name or an
// error the error's text with its wavy underline.
QColor formulaTokenColor(const QString& kind, const QPalette& palette);
QTextCharFormat formulaTokenFormat(const QString& kind, const QPalette& palette);

} // namespace mitcad
