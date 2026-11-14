// SPDX-License-Identifier: MIT
#pragma once

// Machines and Tools… of the CNC group (mitcad#1013, app/COMMANDS.md "The
// CNC group"): the design's machines with their tool libraries (owner
// 12.11.2026: a tool library per machine in the user's settings, with a
// copy in the project). A machine is added, edited and deleted; its tools
// are added, edited and deleted (number, name, shape, diameters, shank
// diameter, flute length, stick-out, holder, speeds and feeds); a machine
// is imported from the user's or the project's machine library and saved
// back to either. Every change is a model command of its own, one undo
// step (`cam_machine_put`, `cam_tool_put`, …); the window reads the design
// again after each.

#include <functional>

#include <QDialog>
#include <QJsonArray>
#include <QJsonObject>
#include <QJsonValue>
#include <QString>

class QLabel;
class QListWidget;
class QPushButton;
class QTreeWidget;

namespace mitcad {

class CamMachinesDialog : public QDialog {
  Q_OBJECT

public:
  // What the window works through: the model's queries (throwing when
  // refused), its commands (false when refused, `lastError` then says why
  // in words), the project's folder (empty outside a project) and lengths
  // in the design's unit for the tool table.
  struct Host {
    std::function<QJsonValue(const QJsonObject& query)> query;
    std::function<bool(const QJsonObject& command, QJsonObject* result)> run;
    std::function<QString()> lastError;
    std::function<QString(double millimetres)> lengthText;
    QString projectFolder;
  };

  CamMachinesDialog(Host host, QWidget* parent = nullptr);

  // Reads the design's machines again, keeping the one selected.
  void refresh();
  // Selects a machine by its id (none when it is not there).
  void selectMachine(const QString& id);

private:
  QString currentMachine() const;
  QJsonObject machineJson(const QString& id) const;
  int currentTool() const;
  void showMachine();
  void newMachine();
  void editMachine();
  void deleteMachine();
  void importMachine();
  void saveMachine(const QString& library);
  void addTool();
  void editTool();
  void deleteTool();
  // A model command; on refusal a warning with `what` ("Could not …") and
  // the model's reason, logged "CAM machines: <log> refused: <reason>".
  bool run(const QJsonObject& command, const QString& what, const QString& action, QJsonObject* result = nullptr);
  // An expression of a length (or, with `angle`, an angle) in millimetres
  // (degrees), evaluated by the design; none when it is not one.
  bool evaluate(const QString& text, const QString& kind, double* value) const;

  Host m_host;
  QJsonArray m_machines; // the `cam_machines` query's
  QJsonArray m_dialects; // the `cam_dialects` query's
  QListWidget* m_list = nullptr;
  QLabel* m_summary = nullptr;
  QTreeWidget* m_tools = nullptr;
  QPushButton* m_editMachine = nullptr;
  QPushButton* m_deleteMachine = nullptr;
  QPushButton* m_saveMachine = nullptr;
  QPushButton* m_addTool = nullptr;
  QPushButton* m_editTool = nullptr;
  QPushButton* m_deleteTool = nullptr;
  QString m_logged; // the last "CAM machines dialog: …" line
};

} // namespace mitcad
