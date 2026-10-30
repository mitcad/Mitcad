// SPDX-License-Identifier: MIT
// The chat panel's group rows of tool calls (mitcad#983) as the panel asks
// for them (agent/ChatLogic.hpp, core/mcp/src/chat/groups.rs): the
// conversation's entries as ChatController keeps them; text, three calls
// (one failed), an approval card, two calls and text are a group of three
// and one of two around the card; a group's head follows its latest call;
// one call is its own line; the stored history's calls group the same way.
// An executable of its own (app.chat_tool_groups): it needs the Rust
// bridge, which test_app does not link.
#include "agent/ChatLogic.hpp"

#include <cstdio>

#include <QJsonArray>
#include <QJsonObject>
#include <QList>
#include <QString>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "chat_tool_groups.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

QJsonObject text(const QString& role, const QString& value) {
  QJsonObject entry;
  entry.insert(QStringLiteral("role"), role);
  entry.insert(QStringLiteral("text"), value);
  return entry;
}

// A call as ChatController keeps it: its input and result too.
QJsonObject call(const QString& id, const QString& name, const QString& summary, const QString& state) {
  QJsonObject entry;
  entry.insert(QStringLiteral("role"), QStringLiteral("tool"));
  entry.insert(QStringLiteral("id"), id);
  entry.insert(QStringLiteral("name"), name);
  entry.insert(QStringLiteral("summary"), summary);
  entry.insert(QStringLiteral("state"), state);
  entry.insert(QStringLiteral("input"), QStringLiteral("{\"document\":\"d1\"}"));
  entry.insert(QStringLiteral("result"), QStringLiteral("{}"));
  return entry;
}

QJsonObject card() {
  QJsonObject entry;
  entry.insert(QStringLiteral("role"), QStringLiteral("card"));
  entry.insert(QStringLiteral("id"), QStringLiteral("c1"));
  entry.insert(QStringLiteral("kind"), QStringLiteral("edit"));
  entry.insert(QStringLiteral("tool"), QStringLiteral("model_command"));
  entry.insert(QStringLiteral("state"), QStringLiteral("open"));
  return entry;
}

} // namespace

int main() {
  using namespace mitcad::agent;
  QList<QJsonObject> entries;
  entries << text(QStringLiteral("user"), QStringLiteral("measure the box"))
          << text(QStringLiteral("assistant"), QStringLiteral("Measuring."))
          << call(QStringLiteral("t1"), QStringLiteral("model_query"), QStringLiteral("bodies"), QStringLiteral("done"))
          << call(QStringLiteral("t2"), QStringLiteral("model_query"), QStringLiteral("faces"), QStringLiteral("failed"))
          << call(QStringLiteral("t3"), QStringLiteral("model_command"), QStringLiteral("add_feature box"),
                  QStringLiteral("running"))
          << card()
          << call(QStringLiteral("t4"), QStringLiteral("document_list"), QString(), QStringLiteral("done"))
          << call(QStringLiteral("t5"), QStringLiteral("model_query"), QStringLiteral("measure"), QStringLiteral("done"))
          << text(QStringLiteral("assistant"), QStringLiteral("The box is 30 mm long."));

  // Two groups around the card.
  const QJsonArray groups = toolGroups(entries);
  CHECK(groups.size() == 2);
  const QJsonObject first = groups.at(0).toObject();
  const QJsonObject second = groups.at(1).toObject();
  CHECK(first.value(QStringLiteral("first")).toInt() == 2);
  CHECK(first.value(QStringLiteral("last")).toInt() == 4);
  CHECK(first.value(QStringLiteral("count")).toInt() == 3);
  CHECK(first.value(QStringLiteral("failed")).toInt() == 1);
  CHECK(first.value(QStringLiteral("head")).toString() ==
        QStringLiteral("3 calls (1 failed) · model_command · add_feature box — running"));
  CHECK(first.value(QStringLiteral("state")).toString() == QStringLiteral("running"));
  CHECK(second.value(QStringLiteral("first")).toInt() == 6);
  CHECK(second.value(QStringLiteral("last")).toInt() == 7);
  CHECK(second.value(QStringLiteral("head")).toString() == QStringLiteral("2 calls · model_query · measure"));
  CHECK(second.value(QStringLiteral("state")).toString() == QStringLiteral("done"));

  // The group of an entry; none for the card or a text.
  CHECK(toolGroupOf(entries, 3).value(QStringLiteral("first")).toInt() == 2);
  CHECK(toolGroupOf(entries, 7).value(QStringLiteral("first")).toInt() == 6);
  CHECK(toolGroupOf(entries, 5).isEmpty());
  CHECK(toolGroupOf(entries, 0).isEmpty());
  CHECK(toolGroupOf(entries, -1).isEmpty());

  // The head follows its latest call: done, then failed (the error colour
  // is the panel's while the state is failed).
  entries[4].insert(QStringLiteral("state"), QStringLiteral("done"));
  QJsonObject group = toolGroupOf(entries, 4);
  CHECK(group.value(QStringLiteral("state")).toString() == QStringLiteral("done"));
  CHECK(group.value(QStringLiteral("head")).toString() ==
        QStringLiteral("3 calls (1 failed) · model_command · add_feature box"));
  entries[4].insert(QStringLiteral("state"), QStringLiteral("failed"));
  group = toolGroupOf(entries, 2);
  CHECK(group.value(QStringLiteral("state")).toString() == QStringLiteral("failed"));
  CHECK(group.value(QStringLiteral("failed")).toInt() == 2);
  CHECK(group.value(QStringLiteral("head")).toString() ==
        QStringLiteral("3 calls (2 failed) · model_command · add_feature box — failed"));

  // A new call after the text starts a new group, alone on its line.
  entries << call(QStringLiteral("t6"), QStringLiteral("ui_state"), QString(), QStringLiteral("running"));
  group = toolGroupOf(entries, 9);
  CHECK(group.value(QStringLiteral("first")).toInt() == 9);
  CHECK(group.value(QStringLiteral("count")).toInt() == 1);
  CHECK(group.value(QStringLiteral("head")).toString() == QStringLiteral("ui_state — running"));
  CHECK(toolGroups(entries).size() == 3);

  // The stored history: calls carry error (ChatController adds the state).
  QList<QJsonObject> history;
  history << text(QStringLiteral("assistant"), QStringLiteral("Measuring."));
  const bool errors[] = {false, true, false};
  for (const bool error : errors) {
    QJsonObject line;
    line.insert(QStringLiteral("role"), QStringLiteral("tool"));
    line.insert(QStringLiteral("name"), QStringLiteral("model_query"));
    line.insert(QStringLiteral("summary"), QStringLiteral("bodies"));
    line.insert(QStringLiteral("error"), error);
    history << line;
  }
  history << text(QStringLiteral("notice"), QStringLiteral("Allow Once: edit"));
  const QJsonArray stored = toolGroups(history);
  CHECK(stored.size() == 1);
  CHECK(stored.at(0).toObject().value(QStringLiteral("head")).toString() ==
        QStringLiteral("3 calls (1 failed) · model_query · bodies"));
  if (failures != 0) {
    std::fprintf(stderr, "%d check(s) failed\n", failures);
    return 1;
  }
  std::printf("chat tool group tests passed\n");
  return 0;
}
