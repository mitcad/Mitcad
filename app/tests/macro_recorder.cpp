// SPDX-License-Identifier: MIT
// Tools › Macro's recorder (mitcad#2245, framework/MacroRecorder): what a
// recording keeps of the commands the model accepted (not a command
// panel's checkpoints, the caches, merged undo steps, a temporary timeline
// marker, a component activated as the design opens, the saved state or
// the scripts' own commands), only while it records, in order; the macro
// file Save Macro writes and the steps Run Script reads back.
#include "UnitTests.hpp"
#include "framework/MacroRecorder.hpp"

#include <cstdio>
#include <initializer_list>

#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "macro_recorder.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

using mitcad::MacroRecorder;

QJsonObject command(const char* name) { return QJsonObject{{QStringLiteral("cmd"), QString::fromLatin1(name)}}; }

void recordableTests() {
  CHECK(MacroRecorder::recordable(command("sketch.create")));
  CHECK(MacroRecorder::recordable(command("add_feature")));
  CHECK(MacroRecorder::recordable(command("undo")));
  CHECK(MacroRecorder::recordable(QJsonObject{{QStringLiteral("cmd"), QStringLiteral("set_marker")},
                                              {QStringLiteral("position"), 2}}));
  for (const char* left : {"begin_command", "restore_command", "discard_command", "clear_cache", "clear_preview",
                           "mark_saved", "merge_undo", "export_script", "run_script"}) {
    CHECK(!MacroRecorder::recordable(command(left)));
  }
  // An editor's rollback, a component activated as the design opens.
  CHECK(!MacroRecorder::recordable(QJsonObject{{QStringLiteral("cmd"), QStringLiteral("set_marker")},
                                               {QStringLiteral("position"), 1},
                                               {QStringLiteral("temporary"), true}}));
  CHECK(!MacroRecorder::recordable(QJsonObject{{QStringLiteral("cmd"), QStringLiteral("activate_component")},
                                               {QStringLiteral("component"), QStringLiteral("C1")},
                                               {QStringLiteral("opening"), true}}));
  CHECK(!MacroRecorder::recordable(QJsonObject{{QStringLiteral("query"), QStringLiteral("bodies")}}));
}

void recordingTests() {
  MacroRecorder macro;
  CHECK(!macro.recording());
  CHECK(!macro.note(command("sketch.create"))); // not recording
  CHECK(macro.count() == 0);
  macro.start();
  CHECK(macro.recording());
  CHECK(macro.note(command("sketch.create")));
  CHECK(!macro.note(command("merge_undo")));
  CHECK(macro.note(command("add_feature")));
  CHECK(macro.count() == 2);
  CHECK(macro.steps().at(0).toObject().value(QStringLiteral("cmd")).toString() == QStringLiteral("sketch.create"));
  CHECK(macro.steps().at(1).toObject().value(QStringLiteral("cmd")).toString() == QStringLiteral("add_feature"));
  macro.stop();
  CHECK(!macro.recording());
  CHECK(!macro.note(command("add_feature")));
  CHECK(macro.count() == 2); // kept until the next recording
  macro.start();
  CHECK(macro.count() == 0);
}

void fileTests() {
  const QJsonArray steps{command("sketch.create"), command("add_feature")};
  const QByteArray file = MacroRecorder::fileJson(steps, QStringLiteral("Macro recorded in MitCad"));
  const QJsonArray written = QJsonDocument::fromJson(file).array();
  CHECK(written.size() == 3);
  CHECK(written.at(0).toObject() == (QJsonObject{{QStringLiteral("comment"), QStringLiteral("Macro recorded in MitCad")}}));
  CHECK(written.at(2).toObject().value(QStringLiteral("cmd")).toString() == QStringLiteral("add_feature"));
  CHECK(file.contains('\n')); // indented, for a person to read and change

  QString error;
  const QJsonArray read = MacroRecorder::readSteps(file, &error);
  CHECK(error.isEmpty());
  CHECK(read == written);
  CHECK(MacroRecorder::readSteps("{\"cmd\": \"undo\"}", &error).isEmpty());
  CHECK(error.contains(QStringLiteral("list of steps")));
  CHECK(MacroRecorder::readSteps("[{\"cmd\": \"undo\"}, 3]", &error).isEmpty());
  CHECK(error.contains(QStringLiteral("Step 2")));
  CHECK(MacroRecorder::readSteps("[{\"cmd\": ", &error).isEmpty());
  CHECK(error.contains(QStringLiteral("not JSON")));
}

} // namespace

int macroRecorderTests() {
  failures = 0;
  recordableTests();
  recordingTests();
  fileTests();
  return failures;
}
MITCAD_UNIT_TEST(macroRecorderTests);
