// SPDX-License-Identifier: MIT
#include "MacroRecorder.hpp"

#include <QCoreApplication>
#include <QJsonDocument>
#include <QJsonParseError>
#include <QJsonValue>

namespace mitcad {
namespace {

// The texts in the context MacroRecorder.
struct Text {
  Q_DECLARE_TR_FUNCTIONS(MacroRecorder)
};

} // namespace

void MacroRecorder::start() {
  m_steps = QJsonArray();
  m_recording = true;
}

bool MacroRecorder::note(const QJsonObject& command) {
  if (!m_recording || !recordable(command)) {
    return false;
  }
  m_steps.append(command);
  return true;
}

bool MacroRecorder::recordable(const QJsonObject& command) {
  const QString name = command.value(QStringLiteral("cmd")).toString();
  if (name.isEmpty()) {
    return false;
  }
  // The application's own workflow: a command panel's checkpoints, the
  // caches, the saved state, undo steps merged by their depth in this
  // session, and the scripts' own commands.
  static const char* const left[] = {"begin_command", "restore_command", "discard_command", "clear_cache",
                                     "clear_preview", "mark_saved",      "merge_undo",      "export_script",
                                     "run_script"};
  for (const char* other : left) {
    if (name == QLatin1String(other)) {
      return false;
    }
  }
  // An editor's rollback of the timeline marker (`temporary`) and a
  // component activated as the design opens (`opening`): no change of
  // the design.
  if (command.value(QStringLiteral("temporary")).toBool() || command.value(QStringLiteral("opening")).toBool()) {
    return false;
  }
  return true;
}

QByteArray MacroRecorder::fileJson(const QJsonArray& steps, const QString& comment) {
  QJsonArray file;
  file.append(QJsonObject{{QStringLiteral("comment"), comment}});
  for (const QJsonValue& step : steps) {
    file.append(step);
  }
  return QJsonDocument(file).toJson(QJsonDocument::Indented);
}

QJsonArray MacroRecorder::readSteps(const QByteArray& file, QString* error) {
  QJsonParseError parse;
  const QJsonDocument document = QJsonDocument::fromJson(file, &parse);
  const auto refuse = [error](const QString& why) {
    if (error != nullptr) {
      *error = why;
    }
    return QJsonArray();
  };
  if (parse.error != QJsonParseError::NoError) {
    return refuse(Text::tr("The file is not JSON: %1 (at character %2).")
                      .arg(parse.errorString())
                      .arg(parse.offset));
  }
  if (!document.isArray()) {
    return refuse(Text::tr("A script is a list of steps: the file holds something else."));
  }
  const QJsonArray steps = document.array();
  for (qsizetype i = 0; i < steps.size(); ++i) {
    if (!steps.at(i).isObject()) {
      return refuse(Text::tr("Step %1 is not a command, a comment or an expectation.").arg(i + 1));
    }
  }
  if (error != nullptr) {
    error->clear();
  }
  return steps;
}

} // namespace mitcad
