// SPDX-License-Identifier: MIT
#pragma once

#include <functional>

#include <QDialog>
#include <QJsonObject>
#include <QPair>
#include <QString>
#include <QVector>

class QCheckBox;
class QComboBox;
class QDialogButtonBox;
class QLabel;
class QLineEdit;
class QPushButton;
class QRadioButton;

namespace mitcad {

// What Replace Component (mitcad#2640) replaces and offers.
struct ReplaceComponentSetup {
  // The occurrence picked: its path from the root ("O1/O4") and its name
  // (Bracket:1), its component's name and how many occurrences place it.
  QString occurrence;
  QString occurrenceName;
  QString component;
  int occurrences = 1;
  // The other components of the design that may replace it: uid and name.
  QVector<QPair<QString, QString>> components;
  // The folder relative paths are relative to (the design file's).
  QString base;
  // Asks for a design file; empty when none was chosen.
  std::function<QString()> chooseFile;
  // Asks for a library part: {"library": insert_component's library,
  // "link": bool, "name": the part's name}; empty when none was chosen.
  std::function<QJsonObject()> chooseLibrary;
};

// Replace Component (mitcad#2640), from a component's context menu or the
// Assemble group: the component of an occurrence, in all its places or in
// that one, replaced by a design file (linked or copied), a library part
// or another component of the design. The design shows the replacement while the
// dialog is open: the dialog keeps a command checkpoint (`begin_command`)
// and runs `replace_component` on it for each choice, taking the previous
// one back first (`restore_command`), and lists what became of each joint,
// constraint and bound length on the old component's geometry (kept by
// name, kept by place, or left broken and marked). OK keeps the last one
// (one undo step, `Replace Bracket with Bracket_v2`), Cancel takes it back.
// A replacement the model refuses shows its reason and disables OK.
class ReplaceComponentDialog : public QDialog {
  Q_OBJECT

public:
  // Runs a model command and shows the model as it is afterwards (the
  // host's runModelCommand); false when the model refused it.
  using Run = std::function<bool(const QJsonObject&, QJsonObject*)>;
  // Why the model refused the last command, in the user's language.
  using LastError = std::function<QString()>;
  ReplaceComponentDialog(Run run, LastError lastError, ReplaceComponentSetup setup, QWidget* parent = nullptr);
  ~ReplaceComponentDialog() override;

  // The command of the choices made; empty while no replacement is chosen.
  QJsonObject command() const;
  // The model's answer to it, once OK kept it.
  QJsonObject result() const { return m_result; }

  void accept() override;
  void reject() override;

private:
  void preview();
  void takeBack();
  void release();
  // The replacement as the log names it: the file's name or the component's.
  QString replacementLabel() const;

  Run m_run;
  LastError m_lastError;
  ReplaceComponentSetup m_setup;
  QString m_token;
  bool m_replaced = false;
  QJsonObject m_result;
  QRadioButton* m_fromFile = nullptr;
  QLineEdit* m_path = nullptr;
  QPushButton* m_browse = nullptr;
  QCheckBox* m_link = nullptr;
  QRadioButton* m_fromLibrary = nullptr;
  QLabel* m_libraryName = nullptr;
  QPushButton* m_chooseLibrary = nullptr;
  // The library part chosen (see ReplaceComponentSetup::chooseLibrary).
  QJsonObject m_library;
  QRadioButton* m_fromComponent = nullptr;
  QComboBox* m_components = nullptr;
  QRadioButton* m_all = nullptr;
  QRadioButton* m_one = nullptr;
  QLabel* m_details = nullptr;
  QDialogButtonBox* m_buttons = nullptr;
};

} // namespace mitcad
