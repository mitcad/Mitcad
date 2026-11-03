// SPDX-License-Identifier: MIT
#pragma once

#include <functional>
#include <memory>

#include <QByteArray>
#include <QHash>
#include <QJsonArray>
#include <QJsonObject>
#include <QJsonValue>
#include <QString>
#include <QVector>

class QObject;

namespace mitcad {

class CommandContext;
class DocumentHost;

// What the panels of materials, printers and processes ask of the model
// they work through (mitcad#704): the open design's (Materials, Printers
// and Processes) or a model of their own that holds no design (My
// Library), whose commands change only the libraries' files.
class PrintItemsHost {
public:
  virtual ~PrintItemsHost() = default;

  virtual const CommandContext& model() const = 0;
  // Runs a model command; false, with lastError(), when it was refused or
  // cancelled.
  virtual bool runModelCommand(const QJsonObject& command, QJsonObject* result = nullptr) = 0;
  // Runs commands as one undo step named `label`.
  virtual bool runModelCommands(const QJsonArray& commands, const QString& label) = 0;
  virtual QString lastError() const = 0;
  virtual bool lastCancelled() const = 0;
  // Whether the model can change now, and why not.
  virtual bool canChangePrintSettings() const = 0;
  virtual QString changeBlockedReason() const = 0;
  // A command changed the model: a running command's panel follows it.
  virtual void commandInputsChanged() {}
  // The folder of the design's project (its library); empty outside one.
  virtual QString projectFolder() const = 0;
  // Whether a design stands behind the model: its bodies, its choices,
  // Assign, Copy to This design and Use in this design. False for the
  // libraries' own model (a manager without a design, mitcad#1135).
  virtual bool hasDesign() const { return true; }
  // Answers read ahead (LibraryItemsHost::setAnswers) are read again from
  // the files: before a look that must see what is there now (Import from
  // Slicer's click).
  virtual void readAgain() {}
};

// The answers of queries read ahead, by the query (its compact JSON).
using QueryAnswers = QHash<QByteArray, QJsonValue>;

// Reads `requests` off the UI thread (mitcad#1018): a model of its own, an
// empty design, answers them on a worker thread from the files they read
// (the libraries of materials and print profiles, an installed slicer's
// presets), and `done` gets the answers and how long the reading took on
// `context`'s (the UI) thread; nothing when `context` is gone by then. A
// request the model refused has no answer (whoever asks it again gets the
// refusal). The UI tests' sync waits meanwhile (TestSync::watch), as for
// what a panel logs next.
void readAhead(QObject* context, const QVector<QJsonObject>& requests,
               std::function<void(const QueryAnswers& answers, qint64 milliseconds)> done);

// The open design's model, through the main window (DocumentHost).
class DesignItemsHost final : public PrintItemsHost {
public:
  explicit DesignItemsHost(DocumentHost& host) : m_host(host) {}

  const CommandContext& model() const override;
  bool runModelCommand(const QJsonObject& command, QJsonObject* result = nullptr) override;
  bool runModelCommands(const QJsonArray& commands, const QString& label) override;
  QString lastError() const override;
  bool lastCancelled() const override;
  bool canChangePrintSettings() const override;
  QString changeBlockedReason() const override;
  void commandInputsChanged() override;
  QString projectFolder() const override;

private:
  DocumentHost& m_host;
};

// A model of its own for the managers without a design (mitcad#704,
// mitcad#1135): an empty design that
// is never shown or saved. The library commands (`create_material`,
// `edit_print_profile`, ... with `library`, `import_print_profile` into a
// library) read and write the libraries' files and leave the open design
// alone, which learns of the changes as of another design's.
class LibraryItemsHost final : public PrintItemsHost {
public:
  LibraryItemsHost();
  ~LibraryItemsHost() override;

  const CommandContext& model() const override;
  bool runModelCommand(const QJsonObject& command, QJsonObject* result = nullptr) override;
  bool runModelCommands(const QJsonArray& commands, const QString& label) override;
  QString lastError() const override { return m_lastError; }
  bool lastCancelled() const override { return false; }
  bool canChangePrintSettings() const override { return true; }
  QString changeBlockedReason() const override { return {}; }
  QString projectFolder() const override { return m_project; }
  bool hasDesign() const override { return false; }
  void readAgain() override { setAnswers({}); }

  // The open design's project, whose library is the second place.
  void setProjectFolder(const QString& folder) { m_project = folder; }
  // Answers read ahead (readAhead, mitcad#1018): the model's queries take
  // them instead of reading the files again, until a command changes the
  // libraries or readAgain.
  void setAnswers(const QueryAnswers& answers);

private:
  struct Model;
  std::unique_ptr<Model> m_model;
  QString m_project;
  QString m_lastError;
};

// The model a print manager (Materials, Printers, Processes; mitcad#1135)
// works through: the open design's, or with no design open
// (DocumentHost::designOpen) a model of its own that holds none
// (LibraryItemsHost), whose commands change only the libraries, so that
// the empty window does not become a changed design.
class ManagerHosts {
public:
  explicit ManagerHosts(DocumentHost& host) : m_document(host), m_design(host) {}

  // Chooses the model for an opening; true when it is another than
  // before.
  bool choose();
  PrintItemsHost& current();
  // Whether the managers work on the open design.
  bool onDesign() const { return m_onDesign; }
  LibraryItemsHost& library() { return m_library; }

private:
  DocumentHost& m_document;
  DesignItemsHost m_design;
  LibraryItemsHost m_library;
  bool m_onDesign = true;
};

// What a manager of `kind` (material, printer or process) asks of the
// model as it opens, as its panel asks it (mitcad#1018, mitcad#1135): the
// items by place, the print keys and the design's rest;
// readAhead reads them off the UI thread when the manager has no design.
QVector<QJsonObject> managerRequests(const QString& kind, const QString& project);

} // namespace mitcad
