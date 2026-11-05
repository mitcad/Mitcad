// SPDX-License-Identifier: MIT
// The folders for designs, which a live MCP agent saves to and opens from
// (mitcad#402): a
// path inside the allowed folder is taken (relative ones too), '..', other
// folders, other suffixes, missing folders and files, an existing file
// without overwrite and links out of the folder are refused.
#include "mcp/AgentFolders.hpp"

#include <cstdio>

#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QLineEdit>
#include <QListWidget>
#include <QPushButton>
#include <QString>
#include <QTemporaryDir>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "agent_folders.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

bool touch(const QString& path) {
  QFile file(path);
  return file.open(QIODevice::WriteOnly) && file.write("{}") == 2;
}

} // namespace

int agentFolderTests() {
  using mitcad::mcp::agentDesignPath;
  QTemporaryDir directory;
  CHECK(directory.isValid());
  const QString root = QFileInfo(directory.path()).canonicalFilePath();
  const QString allowed = root + QStringLiteral("/allowed");
  const QString outside = root + QStringLiteral("/outside");
  CHECK(QDir().mkpath(allowed + QStringLiteral("/sub")));
  CHECK(QDir().mkpath(outside));
  CHECK(touch(allowed + QStringLiteral("/kept.mitcad")));
  CHECK(touch(outside + QStringLiteral("/other.mitcad")));
  mitcad::mcp::setRunAgentFolders({allowed});
  CHECK(mitcad::mcp::agentFolders().contains(allowed));

  QString reason;
  // New files in the folder and below it; relative to the folder.
  CHECK(agentDesignPath(allowed + QStringLiteral("/new.mitcad"), true, false, reason) ==
        allowed + QStringLiteral("/new.mitcad"));
  CHECK(reason.isEmpty());
  CHECK(agentDesignPath(QStringLiteral("sub/new.mitcad"), true, false, reason) ==
        allowed + QStringLiteral("/sub/new.mitcad"));
  CHECK(agentDesignPath(QStringLiteral("NEW.MITCAD"), true, false, reason) == allowed + QStringLiteral("/NEW.MITCAD"));
  // Refused: '..', another folder, another suffix, a missing folder.
  CHECK(agentDesignPath(QStringLiteral("../outside/x.mitcad"), true, false, reason).isEmpty());
  CHECK(reason.contains(QStringLiteral("..")));
  CHECK(agentDesignPath(allowed + QStringLiteral("/sub/../../outside/x.mitcad"), true, false, reason).isEmpty());
  CHECK(agentDesignPath(outside + QStringLiteral("/x.mitcad"), true, false, reason).isEmpty());
  CHECK(reason.contains(QStringLiteral("not in a folder for designs")));
  CHECK(agentDesignPath(allowed + QStringLiteral("/x.step"), true, false, reason).isEmpty());
  CHECK(reason.contains(QStringLiteral(".mitcad")));
  CHECK(agentDesignPath(QStringLiteral("missing/x.mitcad"), true, false, reason).isEmpty());
  CHECK(reason.contains(QStringLiteral("does not exist")));
  // A folder whose name only starts like the allowed one.
  CHECK(QDir().mkpath(allowed + QStringLiteral("-not")));
  CHECK(agentDesignPath(allowed + QStringLiteral("-not/x.mitcad"), true, false, reason).isEmpty());
  // An existing file: only with overwrite; reading needs it there.
  CHECK(agentDesignPath(QStringLiteral("kept.mitcad"), true, false, reason).isEmpty());
  CHECK(reason.contains(QStringLiteral("overwrite")));
  CHECK(agentDesignPath(QStringLiteral("kept.mitcad"), true, true, reason) == allowed + QStringLiteral("/kept.mitcad"));
  CHECK(agentDesignPath(QStringLiteral("kept.mitcad"), false, false, reason) == allowed + QStringLiteral("/kept.mitcad"));
  CHECK(agentDesignPath(QStringLiteral("absent.mitcad"), false, false, reason).isEmpty());
  CHECK(agentDesignPath(outside + QStringLiteral("/other.mitcad"), false, false, reason).isEmpty());
#ifndef Q_OS_WIN
  // Links out of the folder: a linked folder, a linked file.
  CHECK(QFile::link(outside, allowed + QStringLiteral("/linked")));
  CHECK(agentDesignPath(QStringLiteral("linked/x.mitcad"), true, false, reason).isEmpty());
  CHECK(reason.contains(QStringLiteral("not in a folder for designs")));
  CHECK(QFile::link(outside + QStringLiteral("/other.mitcad"), allowed + QStringLiteral("/link.mitcad")));
  CHECK(agentDesignPath(QStringLiteral("link.mitcad"), true, true, reason).isEmpty());
  CHECK(reason.contains(QStringLiteral("not a regular file")));
  CHECK(agentDesignPath(QStringLiteral("link.mitcad"), false, false, reason).isEmpty());
  CHECK(reason.contains(QStringLiteral("not in a folder for designs")));
#endif
  // An agent's output (mitcad#689: model_export, ui_screenshot's path): a
  // new file of any or of the given suffixes, by the same rules.
  using mitcad::mcp::agentOutputPath;
  CHECK(agentOutputPath(QStringLiteral("part.step"), false, reason) == allowed + QStringLiteral("/part.step"));
  CHECK(reason.isEmpty());
  CHECK(agentOutputPath(QStringLiteral("sub/view.JPG"), false, reason, {QStringLiteral("jpg"), QStringLiteral("jpeg")}) ==
        allowed + QStringLiteral("/sub/view.JPG"));
  CHECK(agentOutputPath(QStringLiteral("view.png"), false, reason, {QStringLiteral("jpg")}).isEmpty());
  CHECK(reason.contains(QStringLiteral(".jpg")));
  CHECK(agentOutputPath(QStringLiteral("kept.mitcad"), false, reason).isEmpty());
  CHECK(reason.contains(QStringLiteral("overwrite")));
  CHECK(agentOutputPath(QStringLiteral("kept.mitcad"), true, reason) == allowed + QStringLiteral("/kept.mitcad"));
  CHECK(agentOutputPath(outside + QStringLiteral("/part.stl"), false, reason).isEmpty());
  CHECK(reason.contains(QStringLiteral("not in a folder for designs")));
  CHECK(agentOutputPath(QStringLiteral("../outside/part.stl"), false, reason).isEmpty());
  // document_new's folder and file (mitcad#1266): an allowed folder or
  // below it, a plain name with .mitcad added, the first free DesignN, an
  // existing name refused with the name.
  {
    using mitcad::mcp::agentDesignFolder;
    using mitcad::mcp::agentNewDesignPath;
    CHECK(agentDesignFolder(QStringLiteral("sub"), reason) == allowed + QStringLiteral("/sub"));
    CHECK(reason.isEmpty());
    CHECK(agentDesignFolder(allowed, reason) == allowed);
    CHECK(agentDesignFolder(outside, reason).isEmpty());
    CHECK(reason.contains(QStringLiteral("not in a folder for designs")));
    CHECK(agentDesignFolder(QStringLiteral("../outside"), reason).isEmpty());
    CHECK(reason.contains(QStringLiteral("..")));
    CHECK(agentDesignFolder(QStringLiteral("missing"), reason).isEmpty());
    CHECK(reason.contains(QStringLiteral("does not exist")));
    CHECK(agentDesignFolder(allowed + QStringLiteral("-not"), reason).isEmpty());
    CHECK(agentNewDesignPath(allowed, QStringLiteral("nipple"), reason) == allowed + QStringLiteral("/nipple.mitcad"));
    CHECK(agentNewDesignPath(allowed, QStringLiteral("Nut.MITCAD"), reason) == allowed + QStringLiteral("/Nut.MITCAD"));
    CHECK(agentNewDesignPath(allowed, QStringLiteral("kept"), reason).isEmpty());
    CHECK(reason.contains(QStringLiteral("kept.mitcad")) && reason.contains(QStringLiteral("already exists")));
    CHECK(agentNewDesignPath(allowed, QStringLiteral("KEPT.mitcad"), reason).isEmpty());
    CHECK(agentNewDesignPath(allowed, QStringLiteral("sub/x"), reason).isEmpty());
    CHECK(reason.contains(QStringLiteral("not a design's name")));
    CHECK(agentNewDesignPath(allowed, QStringLiteral(".."), reason).isEmpty());
    CHECK(agentNewDesignPath(allowed, QStringLiteral(".mitcad"), reason).isEmpty());
    CHECK(agentNewDesignPath(allowed, QString(), reason) == allowed + QStringLiteral("/Design1.mitcad"));
    CHECK(touch(allowed + QStringLiteral("/design1.mitcad")));
    CHECK(agentNewDesignPath(allowed, QString(), reason) == allowed + QStringLiteral("/Design2.mitcad"));
  }
  mitcad::mcp::setRunAgentFolders({});
  // No folder (mitcad#732): the reason names the window as the platform
  // does, and no command-line option.
  if (mitcad::mcp::agentFolders().isEmpty()) {
    CHECK(agentDesignPath(QStringLiteral("x.mitcad"), true, false, reason).isEmpty());
#ifdef Q_OS_MACOS
    CHECK(reason.contains(QStringLiteral("Settings › AI Assistants")));
#else
    CHECK(reason.contains(QStringLiteral("Preferences › AI Assistants")));
#endif
    CHECK(!reason.contains(QStringLiteral("--mcp-folder")));
  }

  // Preferences › AI Assistants (mitcad#727): the list named for assistive
  // technology also while empty, Add… (a folder chooser) and Remove, no
  // typed field; the folders chosen are the list's.
  {
    mitcad::mcp::AgentPreferencesBox box;
    auto* list = box.findChild<QListWidget*>(QStringLiteral("agentFolders"));
    auto* add = box.findChild<QPushButton*>(QStringLiteral("agentFolderAdd"));
    CHECK(list != nullptr && list->accessibleName() == QStringLiteral("Folders for designs"));
    // On macOS a + button, its words its accessible name.
    CHECK(add != nullptr && (add->text() == QStringLiteral("Add…") || add->accessibleName() == QStringLiteral("Add…")));
    CHECK(box.findChildren<QLineEdit*>().isEmpty());
    CHECK(box.chosen() == mitcad::mcp::savedAgentFolders());
  }
  return failures;
}
