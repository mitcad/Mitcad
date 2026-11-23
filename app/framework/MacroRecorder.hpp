// SPDX-License-Identifier: MIT
#pragma once

#include <QByteArray>
#include <QJsonArray>
#include <QJsonObject>
#include <QString>

namespace mitcad {

// Tools › Macro (mitcad#2245, app/COMMANDS.md "Macros and scripts"): the
// model commands the application sends while recording is on, in order, as
// it sent them. DocumentSession notes every command the model accepted;
// only those a replay should repeat are kept (recordable): queries and
// previews never come here, and a command panel's checkpoints, the caches,
// a temporary timeline marker (an editor's rollback), a merge of undo steps
// (it counts the steps of this session), the saved state and the scripts'
// own commands are left out. Save Macro writes the steps as a script file
// that Run Script (the model's run_script) runs again.
class MacroRecorder {
public:
  bool recording() const { return m_recording; }
  // Starts a new recording: the steps of an earlier one go.
  void start();
  // Ends the recording; its steps stay until the next start.
  void stop() { m_recording = false; }
  // A command the model accepted: kept while recording when recordable.
  // True when it was kept.
  bool note(const QJsonObject& command);
  const QJsonArray& steps() const { return m_steps; }
  int count() const { return static_cast<int>(m_steps.size()); }

  // Whether a replay repeats the command (see the class).
  static bool recordable(const QJsonObject& command);
  // The macro file: a JSON array of steps, `{"comment": comment}` first,
  // then the steps, indented.
  static QByteArray fileJson(const QJsonArray& steps, const QString& comment);
  // The steps of a script file (a JSON array of steps, as Save Macro and
  // Export Timeline as Script write them); `error` says in words why a
  // file is not one, and the result is then empty.
  static QJsonArray readSteps(const QByteArray& file, QString* error);

private:
  bool m_recording = false;
  QJsonArray m_steps;
};

} // namespace mitcad
