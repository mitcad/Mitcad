// SPDX-License-Identifier: MIT
#include "SyncState.hpp"

#include <QCoreApplication>

namespace mitcad {
namespace {

struct Text {
  Q_DECLARE_TR_FUNCTIONS(SyncState)
};

QString shortTime(const QDateTime& time, const QDate& today) {
  const QDateTime local = time.toLocalTime();
  return local.date() == today ? local.toString(QStringLiteral("HH:mm"))
                               : local.toString(QStringLiteral("yyyy-MM-dd HH:mm"));
}

} // namespace

SyncWords syncWords(const SyncState& state, const QDate& today) {
  SyncWords words;
  const QString cls = state.problem;
  const bool waiting = state.ahead > 0;
  if (!state.busy.isEmpty()) {
    if (state.queued) {
      words.text = Text::tr("Waiting for the project's other server work…");
      words.logged = QStringLiteral("waiting");
    } else if (state.busy == QLatin1String("push")) {
      words.text = Text::tr("Sending to the project's server…");
      words.logged = QStringLiteral("sending");
    } else if (state.busy == QLatin1String("sync")) {
      words.text = Text::tr("Syncing with the project's server…");
      words.logged = QStringLiteral("syncing");
    } else if (state.busy == QLatin1String("connect")) {
      words.text = Text::tr("Connecting to the project's server…");
      words.logged = QStringLiteral("connecting");
    } else {
      words.text = Text::tr("Checking the project's server…");
      words.logged = QStringLiteral("checking");
    }
    return words;
  }
  if (state.conflict) {
    words.text = Text::tr("Not synced: files changed here and on the server need your choice (Sync)");
    words.logged = QStringLiteral("conflict");
    words.problem = true;
  } else if (cls == QLatin1String("network") || cls == QLatin1String("timed_out")) {
    words.text = waiting ? Text::tr("Offline: changes are kept and sent later (%n version(s) waiting)", nullptr,
                                    state.ahead)
                         : Text::tr("Offline: changes are kept and sent later");
    words.logged = QStringLiteral("offline");
    words.problem = true;
  } else if (cls == QLatin1String("auth_failed") || cls == QLatin1String("host_key_unknown")) {
    words.text = Text::tr("Sign-in needed: changes are kept and sent after signing in");
    words.logged = QStringLiteral("sign-in needed");
    words.problem = true;
  } else if (cls == QLatin1String("not_found")) {
    words.text = Text::tr("The project's server was not found: changes are kept and sent later");
    words.logged = QStringLiteral("server not found");
    words.problem = true;
  } else if (cls == QLatin1String("git_missing")) {
    words.text = Text::tr("Not synced: git is missing; changes are kept on this computer");
    words.logged = QStringLiteral("git missing");
    words.problem = true;
  } else if (cls == QLatin1String("untrusted_config")) {
    words.text = Text::tr("Not synced: the repository's own git settings name programs");
    words.logged = QStringLiteral("untrusted git settings");
    words.problem = true;
  } else if (!cls.isEmpty() && cls != QLatin1String("rejected") && cls != QLatin1String("cancelled")) {
    words.text = Text::tr("Not sent: the server did not take the versions; they are kept and tried again later");
    words.logged = QStringLiteral("not sent");
    words.problem = true;
  } else if (state.ahead > 0 && state.behind > 0) {
    words.text = Text::tr("%n version(s) to send, and newer ones on the server: Sync takes them", nullptr,
                          state.ahead);
    words.logged = QStringLiteral("ahead %1, behind %2").arg(state.ahead).arg(state.behind);
  } else if (state.ahead > 0) {
    words.text = Text::tr("%n version(s) waiting to be sent", nullptr, state.ahead);
    words.logged = QStringLiteral("ahead %1").arg(state.ahead);
  } else if (state.behind > 0) {
    words.text = Text::tr("Newer versions on the server: Sync takes them");
    words.logged = QStringLiteral("behind %1").arg(state.behind);
  } else if (state.ahead == 0 && state.behind == 0) {
    words.text = Text::tr("Up to date");
    words.logged = QStringLiteral("up to date");
  } else {
    words.text = Text::tr("Not synced yet");
    words.logged = QStringLiteral("not synced yet");
  }
  if (state.lastDone.isValid()) {
    const QString when = shortTime(state.lastDone, today);
    words.text = Text::tr("%1, last sync %2").arg(words.text, when);
    words.logged += QStringLiteral(", last sync %1").arg(when);
  }
  return words;
}

} // namespace mitcad
