// SPDX-License-Identifier: MIT
#include "TestDriver.hpp"
#include "../MainWindow.hpp"
#include "../mcp/AssistantSetup.hpp"
#include "../mcp/AgentFolders.hpp"
#include "../mcp/ConnectAssistantDialog.hpp"

#include <memory>
#include <utility>

#include <QAbstractButton>
#include <QApplication>
#include <QClipboard>
#include <QCoreApplication>
#include <QDateTime>
#include <QDir>
#include <QEvent>
#include <QFile>
#include <QFileInfo>
#include <QJsonDocument>
#include <QLabel>
#include <QLayout>
#include <QLineEdit>
#include <QMessageBox>
#include <QPushButton>
#include <QRegularExpression>
#include <QTimer>

namespace mitcad {

namespace {
// The assistant choices of the dialog's first row (mitcad#706).
QList<QAbstractButton*> assistantChoices(QWidget* dialog) {
  QList<QAbstractButton*> choices;
  if (auto* row = dialog->findChild<QWidget*>(QStringLiteral("assistantClient"))) {
    for (QAbstractButton* button : row->findChildren<QAbstractButton*>()) {
      if (button->property("assistantClient").isValid()) choices << button;
    }
  }
  return choices;
}

// Picks an assistant as a click does; a click on the chosen one refreshes it.
void chooseAssistant(QWidget* dialog, mcp::AssistantClient client) {
  for (QAbstractButton* button : assistantChoices(dialog)) {
    if (button->property("assistantClient").toInt() == int(client)) button->click();
  }
}

QString stateOf(const QLabel* status) { return status->property("assistantState").toString(); }
QString actionOf(const QPushButton* setup) { return setup->property("assistantAction").toString(); }

// The height a dialog has beyond what its contents need (mitcad#724). Its
// layouts are activated innermost first, so that a hint not refreshed yet
// cannot hide empty room.
int spareHeight(QWidget* dialog) {
  QCoreApplication::sendPostedEvents(nullptr, QEvent::LayoutRequest);
  const QList<QWidget*> widgets = dialog->findChildren<QWidget*>();
  for (auto widget = widgets.crbegin(); widget != widgets.crend(); ++widget) {
    if (QLayout* inner = (*widget)->layout()) { inner->invalidate(); inner->activate(); }
  }
  QLayout* rows = dialog->layout();
  rows->invalidate();
  rows->activate();
  const int needed = qMax(rows->hasHeightForWidth() ? rows->totalHeightForWidth(dialog->width()) : rows->sizeHint().height(),
                          dialog->minimumSizeHint().height());
  return dialog->height() - needed;
}
}

void TestDriver::testAssistantSetup(const QString& mode) {
  mcp::ConnectAssistantDialog* dialog = nullptr;
  for (QWidget* top : QApplication::topLevelWidgets()) {
    if (top->isVisible()) {
      if (auto* candidate = dynamic_cast<mcp::ConnectAssistantDialog*>(top)) dialog = candidate;
    }
  }
  if (!dialog) { fail(m_line, QStringLiteral("Connect AI Assistant dialog missing")); return; }
  auto* config = dialog->findChild<QLineEdit*>(QStringLiteral("assistantConfigPath"));
  auto* setup = dialog->findChild<QPushButton*>(QStringLiteral("assistantSetup"));
  auto* status = dialog->findChild<QLabel*>(QStringLiteral("assistantConnectionState"));
  auto* result = dialog->findChild<QLabel*>(QStringLiteral("assistantSetupResult"));
  if (assistantChoices(dialog).size() != 4 || !config || !setup || !status || !result) {
    fail(m_line, QStringLiteral("assistant choices or setup controls missing")); return;
  }
  const QString work = qEnvironmentVariable("MITCAD_ASSISTANT_TEST_WORK");
  if (work.isEmpty()) { fail(m_line, QStringLiteral("assistant setup test requires an isolated work folder")); return; }
  const QString original = QStringLiteral("# Keep the user's comment\nmodel = 'unchanged'\n");
  // Connect is the consent for the folder shown: allowed after it, not before.
  const auto folderAllowed = [dialog] {
    const QString folder = QFileInfo(dialog->folder()).canonicalFilePath();
    return !folder.isEmpty() && mcp::savedAgentFolders().contains(folder);
  };
  if (mode == QStringLiteral("other")) {
    chooseAssistant(dialog, mcp::AssistantClient::Other);
    // Copy configuration is the primary button only, not again under Advanced.
    auto* advancedCopy = dialog->findChild<QPushButton*>(QStringLiteral("assistantCopyConfiguration"));
    if (stateOf(status) != QStringLiteral("manual") || actionOf(setup) != QStringLiteral("copy") ||
        !advancedCopy || !advancedCopy->isHidden()) {
      fail(m_line, QStringLiteral("generic client fallback has the wrong format or an automatic setup action")); return;
    }
    setup->click();
    const auto data = QJsonDocument::fromJson(QApplication::clipboard()->text().toUtf8());
    if (!data.object().contains(QStringLiteral("mcpServers"))) {
      fail(m_line, QStringLiteral("generic client fallback has the wrong format or an automatic setup action"));
    }
  } else if (mode == QStringLiteral("folder")) {
    chooseAssistant(dialog, mcp::AssistantClient::Other);
    auto* folder = dialog->findChild<QLabel*>(QStringLiteral("assistantFolder"));
    const QString path = QDir(work).filePath(QStringLiteral("explicitly allowed/new folder"));
    dialog->setFolder(path); // what Change… does with the chosen folder
    if (!folder || !folder->isVisibleTo(dialog) || !folder->toolTip().contains(QDir::toNativeSeparators(path)) ||
        mcp::savedAgentFolders().contains(QDir::cleanPath(path))) {
      fail(m_line, QStringLiteral("the folder line did not show the chosen folder, or allowed it before consent")); return;
    }
    setup->click(); // Copy configuration: another assistant's Connect
    QString reason;
    if (!QFileInfo(path).isDir() || !mcp::savedAgentFolders().contains(QFileInfo(path).canonicalFilePath()) ||
        mcp::agentDesignPath(QDir(path).filePath(QStringLiteral("design.mitcad")), true, false, reason).isEmpty() ||
        !mcp::agentDesignPath(QDir(work).filePath(QStringLiteral("outside.mitcad")), true, false, reason).isEmpty()) {
      fail(m_line, QStringLiteral("explicit folder consent did not preserve file access boundaries"));
    }
  } else if (mode == QStringLiteral("missing-claude")) {
#if !defined(Q_OS_WIN) && !defined(Q_OS_MACOS)
    // Claude Desktop's extensions do not install here: no action at all.
    chooseAssistant(dialog, mcp::AssistantClient::ClaudeDesktop);
    if (stateOf(status) != QStringLiteral("unavailable") || setup->isVisibleTo(dialog)) {
      fail(m_line, QStringLiteral("Claude Desktop offered an action on a system it does not run on")); return;
    }
#endif
    chooseAssistant(dialog, mcp::AssistantClient::ClaudeCode);
    // The missing client is the state with its installation page before any
    // registration; without its CLI, Claude Code offers no Connect anyway.
    auto* anyway = dialog->findChild<QPushButton*>(QStringLiteral("assistantConnectAnyway"));
    if (stateOf(status) != QStringLiteral("not-installed") || actionOf(setup) != QStringLiteral("install") ||
        !status->text().contains(QStringLiteral("was not found")) || setup->text() != QStringLiteral("How to install") ||
        !anyway || anyway->isVisibleTo(dialog)) {
      fail(m_line, QStringLiteral("missing client was not reported before registration"));
    }
  } else if (mode == QStringLiteral("local-bin")) {
    // Claude Code's own installer puts it in the home's .local\bin and may
    // leave PATH without it (mitcad#956): found there, with this platform's
    // suffix, when the dialog is activated again or Check again is pressed;
    // its command then names it by its full path. The isolated home is
    // MITCAD_TEST_ASSISTANT_HOME, searched after the test's client folder.
    const QString home = QDir(work).filePath(QStringLiteral("user home"));
    const QString folder = QDir(home).filePath(QStringLiteral(".local/bin"));
#ifdef Q_OS_WIN
    const QString installed = QDir(folder).filePath(QStringLiteral("claude.exe"));
#else
    const QString installed = QDir(folder).filePath(QStringLiteral("claude"));
#endif
    auto* checkAgain = dialog->findChild<QPushButton*>(QStringLiteral("assistantCheckAgain"));
    auto* command = dialog->findChild<QLineEdit*>(QStringLiteral("assistantCommand"));
    const auto install = [installed, folder] {
      return QDir().mkpath(folder) && QFile::copy(qEnvironmentVariable("MITCAD_TEST_ASSISTANT_STUB"), installed) &&
          QFile::setPermissions(installed, QFileDevice::ReadOwner | QFileDevice::WriteOwner | QFileDevice::ExeOwner);
    };
    const auto ready = [status, setup] {
      return stateOf(status) == QStringLiteral("ready") && actionOf(setup) == QStringLiteral("connect");
    };
    const auto missing = [status, setup, checkAgain, dialog] {
      return stateOf(status) == QStringLiteral("not-installed") && setup->text() == QStringLiteral("How to install") &&
          checkAgain->isVisibleTo(dialog);
    };
    QString problem;
    if (!checkAgain || !command) problem = QStringLiteral("Check again or the command missing");
    qputenv("MITCAD_TEST_ASSISTANT_HOME", home.toLocal8Bit());
    if (problem.isEmpty()) {
      chooseAssistant(dialog, mcp::AssistantClient::ClaudeCode);
      if (!missing()) problem = QStringLiteral("a missing Claude Code did not offer Check again after How to install");
    }
    if (problem.isEmpty()) {
      checkAgain->click();
      if (!missing() || checkAgain->text() != QStringLiteral("Not found yet")) {
        problem = QStringLiteral("Check again without Claude Code did not say Not found yet");
      }
    }
    if (problem.isEmpty() && !install()) problem = QStringLiteral("Claude Code in the isolated .local/bin could not be prepared");
    if (problem.isEmpty()) {
      // What the window system sends when the user comes back to the dialog.
      QEvent activation(QEvent::ActivationChange);
      QCoreApplication::sendEvent(dialog, &activation);
      const QString expected = QDir::toNativeSeparators(QFileInfo(installed).canonicalFilePath());
      if (!ready() || checkAgain->isVisibleTo(dialog)) {
        problem = QStringLiteral("Claude Code in .local/bin was not found on activation: ") + status->text();
      } else if (!command->text().contains(expected)) {
        problem = QStringLiteral("the command did not name Claude Code by its full path: ") + command->text();
      }
    }
    if (problem.isEmpty()) {
      QFile::remove(installed);
      checkAgain->setText(QStringLiteral("Check again"));
      checkAgain->click();
      if (!missing()) problem = QStringLiteral("Check again did not see Claude Code removed: ") + status->text();
    }
    if (problem.isEmpty()) {
      if (!install()) problem = QStringLiteral("Claude Code in the isolated .local/bin could not be prepared again");
      else {
        checkAgain->click();
        if (!ready()) problem = QStringLiteral("Check again did not find Claude Code in .local/bin: ") + status->text();
      }
    }
    // Back to the other modes' environment: the test's client folder only.
    qunsetenv("MITCAD_TEST_ASSISTANT_HOME");
    QDir(home).removeRecursively();
    chooseAssistant(dialog, mcp::AssistantClient::ClaudeCode);
    if (!problem.isEmpty()) { fail(m_line, problem); return; }
  } else if (mode == QStringLiteral("claude")) {
    const QString executable = QDir(qEnvironmentVariable("MITCAD_TEST_ASSISTANT_EXECUTABLES"))
#ifdef Q_OS_WIN
        .filePath(QStringLiteral("claude.exe"));
#else
        .filePath(QStringLiteral("claude"));
#endif
    if (!QFile::copy(qEnvironmentVariable("MITCAD_TEST_ASSISTANT_STUB"), executable)) {
      fail(m_line, QStringLiteral("isolated Claude client could not be prepared")); return;
    }
    QFile::setPermissions(executable, QFileDevice::ReadOwner | QFileDevice::WriteOwner | QFileDevice::ExeOwner);
    chooseAssistant(dialog, mcp::AssistantClient::ClaudeCode);
    if (stateOf(status) != QStringLiteral("ready") || actionOf(setup) != QStringLiteral("connect")) {
      fail(m_line, QStringLiteral("installed Claude Code did not offer Connect")); return;
    }
    QTimer::singleShot(100, dialog, [] {
      for (QWidget* top : QApplication::topLevelWidgets()) {
        if (auto* question = qobject_cast<QMessageBox*>(top); question &&
            dialogMatches(question, QRegularExpression(QStringLiteral("^Replace MitCad Connection$|is already connected to another MitCad installation\\.")))) {
          question->button(QMessageBox::Yes)->click();
        }
      }
    });
    // Establish the driver's wait before a nested confirmation event loop
    // can advance the script to another client choice.
    QTimer::singleShot(0, setup, [setup] { setup->click(); });
    const bool failure = !qEnvironmentVariableIsEmpty("MITCAD_TEST_ASSISTANT_FAIL") ||
                         !qEnvironmentVariableIsEmpty("MITCAD_TEST_ASSISTANT_CORRUPT") ||
                         !qEnvironmentVariableIsEmpty("MITCAD_TEST_ASSISTANT_CONCURRENT");
    // Waiting has no button: the next step is a new Claude Code session.
    whenReady([status, result, setup, failure, work, dialog] {
      return QFileInfo(QDir(work).filePath(QStringLiteral("claude-registration-argv.json"))).isFile() &&
          setup->isEnabled() && (failure ? !result->text().isEmpty() && result->property("setupError").toBool() &&
                                               stateOf(status) == QStringLiteral("failed") && actionOf(setup) == QStringLiteral("retry")
          : stateOf(status) == QStringLiteral("waiting") && actionOf(setup).isEmpty() && !setup->isVisibleTo(dialog));
    }, 10, QStringLiteral("Claude registration did not reach its verified state"), [this, dialog, failure, work] {
      if (!failure) {
        const auto backups = QDir(work).entryList({QStringLiteral("claude.json.mitcad-backup-*")}, QDir::Files);
        dialog->connectAssistant(); // what Connect did, once more
        if (QDir(work).entryList({QStringLiteral("claude.json.mitcad-backup-*")}, QDir::Files) != backups) {
          fail(m_line, QStringLiteral("repeating Claude registration made another backup"));
        }
      }
    });
  } else if (mode == QStringLiteral("bundle")) {
    auto* anyway = dialog->findChild<QPushButton*>(QStringLiteral("assistantConnectAnyway"));
    chooseAssistant(dialog, mcp::AssistantClient::ClaudeDesktop);
    if (stateOf(status) != QStringLiteral("not-installed") || actionOf(setup) != QStringLiteral("install") ||
        setup->text() != QStringLiteral("Get Claude Desktop") || !anyway || !anyway->isVisibleTo(dialog)) {
      fail(m_line, QStringLiteral("missing Claude Desktop did not offer its download and Connect anyway")); return;
    }
    // The isolated stand-in for an installed Claude Desktop.
    QFile marker(QDir(qEnvironmentVariable("MITCAD_TEST_ASSISTANT_EXECUTABLES")).filePath(QStringLiteral("claude-desktop")));
    if (!marker.open(QIODevice::WriteOnly)) { fail(m_line, QStringLiteral("isolated Claude Desktop could not be prepared")); return; }
    marker.close();
    chooseAssistant(dialog, mcp::AssistantClient::ClaudeDesktop);
    if (actionOf(setup) != QStringLiteral("connect") || anyway->isVisibleTo(dialog)) {
      fail(m_line, QStringLiteral("installed Claude Desktop did not offer Connect")); return;
    }
    // Not found is a guess: Connect anyway runs the same setup.
    marker.remove();
    chooseAssistant(dialog, mcp::AssistantClient::ClaudeDesktop);
    anyway->click();
    if (!QFileInfo(dialog->property("assistantBundlePath").toString()).isFile() || stateOf(status) != QStringLiteral("waiting") ||
        actionOf(setup) != QStringLiteral("reopen") || setup->text() != QStringLiteral("Open in Claude Desktop again")) {
      fail(m_line, QStringLiteral("native MCP Bundle was not prepared for the isolated installer"));
    }
  } else if (mode == QStringLiteral("packaged-claude")) {
#ifdef Q_OS_WIN
    // Claude Desktop from the Microsoft Store (mitcad#763): no application
    // takes the extension (MITCAD_TEST_ASSISTANT_BUNDLE_OPEN=none, not the
    // registry), and its package is a folder in the isolated LOCALAPPDATA,
    // its settings folder one in the isolated APPDATA.
    const bool bundleOpenSet = qEnvironmentVariableIsSet("MITCAD_TEST_ASSISTANT_BUNDLE_OPEN");
    const QByteArray bundleOpen = qgetenv("MITCAD_TEST_ASSISTANT_BUNDLE_OPEN");
    qputenv("MITCAD_TEST_ASSISTANT_BUNDLE_OPEN", "none");
    const QString localAppData = QDir(work).filePath(QStringLiteral("local-app-data"));
    const QString roamingAppData = QDir(work).filePath(QStringLiteral("roaming-app-data"));
    const QString package = QDir(localAppData).filePath(QStringLiteral("Packages/") + QLatin1String(mcp::kClaudeDesktopPackageFamily));
    const QString bundle = QDir(work).filePath(QStringLiteral("assistant-setup/mitcad-desktop.mcpb"));
    auto* anyway = dialog->findChild<QPushButton*>(QStringLiteral("assistantConnectAnyway"));
    auto* openDesktop = dialog->findChild<QPushButton*>(QStringLiteral("assistantOpenDesktop"));
    const auto installFirst = [status, setup, openDesktop, dialog] {
      return stateOf(status) == QStringLiteral("failed") && status->text() == QStringLiteral("Install Claude Desktop first") &&
          actionOf(setup) == QStringLiteral("install") && setup->text() == QStringLiteral("Get Claude Desktop") &&
          !openDesktop->isVisibleTo(dialog);
    };
    QString problem;
    if (!anyway || !openDesktop) {
      problem = QStringLiteral("Connect anyway or Open Claude Desktop missing");
    }
    // No Claude Desktop at all: Install Claude Desktop first, as before.
    if (problem.isEmpty()) {
      chooseAssistant(dialog, mcp::AssistantClient::ClaudeDesktop);
      if (stateOf(status) != QStringLiteral("not-installed") || !anyway->isVisibleTo(dialog)) {
        problem = QStringLiteral("missing Claude Desktop did not offer Connect anyway");
      }
    }
    if (problem.isEmpty()) {
      anyway->click();
      if (!installFirst()) {
        problem = QStringLiteral("no Claude Desktop and no extension handler did not say Install Claude Desktop first: ") +
                  status->text();
      }
    }
    // What an uninstalled Claude Desktop leaves behind, its settings folder
    // and a package folder of another family: found enough to offer Connect,
    // but no application, so Install Claude Desktop first.
    if (problem.isEmpty() && (!QDir().mkpath(QDir(roamingAppData).filePath(QStringLiteral("Claude"))) ||
                              !QDir().mkpath(QDir(localAppData).filePath(QStringLiteral("Packages/Claude_other123"))))) {
      problem = QStringLiteral("isolated Claude Desktop settings folder could not be prepared");
    }
    if (problem.isEmpty()) {
      chooseAssistant(dialog, mcp::AssistantClient::ClaudeDesktop);
      if (stateOf(status) != QStringLiteral("ready") || actionOf(setup) != QStringLiteral("connect")) {
        problem = QStringLiteral("Claude Desktop's settings folder did not offer Connect");
      }
    }
    if (problem.isEmpty()) {
      setup->click();
      if (!installFirst()) {
        problem = QStringLiteral("Claude Desktop's settings folder alone did not say Install Claude Desktop first: ") +
                  status->text();
      }
    }
    // The package there: Connect, then the extension's installation in
    // Claude Desktop with the saved file shown, never Install Claude Desktop.
    if (problem.isEmpty() && !QDir().mkpath(package)) {
      problem = QStringLiteral("isolated Claude Desktop package could not be prepared");
    }
    if (problem.isEmpty()) {
      chooseAssistant(dialog, mcp::AssistantClient::ClaudeDesktop);
      if (stateOf(status) != QStringLiteral("ready") || actionOf(setup) != QStringLiteral("connect") || anyway->isVisibleTo(dialog)) {
        problem = QStringLiteral("packaged Claude Desktop did not offer Connect");
      }
    }
    if (problem.isEmpty()) {
      setup->click();
      if (stateOf(status) != QStringLiteral("install-extension") || actionOf(setup) != QStringLiteral("reveal") ||
          status->text() != QStringLiteral("Install the extension in Claude Desktop") ||
          setup->text() != QStringLiteral("Show in Explorer") || !openDesktop->isVisibleTo(dialog) ||
          openDesktop->text() != QStringLiteral("Open Claude Desktop") || !result->isVisibleTo(dialog) ||
          !result->text().startsWith(QStringLiteral("In Claude Desktop: Settings ")) ||
          !result->text().contains(QStringLiteral("Advanced settings")) || !result->text().contains(QStringLiteral("Install Extension")) ||
          !result->text().contains(QDir::toNativeSeparators(bundle)) || !QFileInfo(bundle).isFile()) {
        problem = QStringLiteral("packaged Claude Desktop without an extension handler did not offer the extension's "
                                 "installation: ") + status->text() + QStringLiteral(" / ") + result->text();
      }
    }
    if (problem.isEmpty()) {
      setup->click();
      openDesktop->click();
      if (QDir::cleanPath(dialog->property("assistantShownPath").toString()) != QDir::cleanPath(bundle) ||
          !dialog->property("assistantOpenedDesktop").toBool()) {
        problem = QStringLiteral("Show in Explorer or Open Claude Desktop did not point at the saved extension");
      }
    }
    // Claude Desktop uninstalled meanwhile: Open Claude Desktop does not do
    // nothing, the state becomes Install Claude Desktop first.
    if (problem.isEmpty()) {
      dialog->setProperty("assistantOpenedDesktop", false);
      QDir(package).removeRecursively();
      openDesktop->click();
      if (!installFirst() || dialog->property("assistantOpenedDesktop").toBool() || result->isVisibleTo(dialog)) {
        problem = QStringLiteral("Open Claude Desktop without Claude Desktop did not say Install Claude Desktop first: ") +
                  status->text();
      }
    }
    // Back to the other modes' environment: no Claude Desktop folders, the
    // captured installer.
    if (bundleOpenSet) qputenv("MITCAD_TEST_ASSISTANT_BUNDLE_OPEN", bundleOpen);
    else qunsetenv("MITCAD_TEST_ASSISTANT_BUNDLE_OPEN");
    QDir(localAppData).removeRecursively();
    QDir(roamingAppData).removeRecursively();
    chooseAssistant(dialog, mcp::AssistantClient::ClaudeDesktop);
    if (!problem.isEmpty()) { fail(m_line, problem); return; }
#else
    fail(m_line, QStringLiteral("packaged-claude runs on Windows only")); return;
#endif
  } else if (mode == QStringLiteral("blocked")) {
    chooseAssistant(dialog, mcp::AssistantClient::Codex);
    config->setText(QDir(work).filePath(QStringLiteral("managed/config.toml")));
    emit config->editingFinished(); // as when the field loses focus
    if (actionOf(setup) != QStringLiteral("connect")) { fail(m_line, QStringLiteral("managed Codex settings did not offer Connect")); return; }
    setup->click();
    if (!result->property("setupError").toBool() || !result->text().contains(QStringLiteral("Managed")) ||
        stateOf(status) != QStringLiteral("failed") || !status->text().contains(QStringLiteral("Managed"))) {
      fail(m_line, QStringLiteral("managed settings were not protected"));
    }
  } else if (mode == QStringLiteral("codex")) {
    if (!mcp::savedAgentFolders().isEmpty()) {
      fail(m_line, QStringLiteral("opening the dialog allowed a folder")); return;
    }
    // Claude Code's configuration with Advanced open, then Codex's shorter
    // one: the dialog gives the height back (mitcad#724), and again when
    // Advanced closes.
    auto* advancedToggle = dialog->findChild<QAbstractButton*>(QStringLiteral("assistantAdvancedToggle"));
    if (!advancedToggle) { fail(m_line, QStringLiteral("Advanced's toggle missing")); return; }
    chooseAssistant(dialog, mcp::AssistantClient::ClaudeCode);
    if (!advancedToggle->isChecked()) advancedToggle->click();
    chooseAssistant(dialog, mcp::AssistantClient::Codex);
    if (const int spare = spareHeight(dialog); spare > 4) {
      fail(m_line, QStringLiteral("Codex's Advanced left %1 px of empty height").arg(spare)); return;
    }
    advancedToggle->click();
    if (const int spare = spareHeight(dialog); spare > 4) {
      fail(m_line, QStringLiteral("closing Advanced left %1 px of empty height").arg(spare)); return;
    }
    chooseAssistant(dialog, mcp::AssistantClient::Codex);
    auto* copy = dialog->findChild<QPushButton*>(QStringLiteral("assistantCopyCommand"));
    auto* copyConfiguration = dialog->findChild<QPushButton*>(QStringLiteral("assistantCopyConfiguration"));
    auto* advanced = dialog->findChild<QWidget*>(QStringLiteral("assistantAdvanced"));
    if (!copy || !copyConfiguration || !advanced) { fail(m_line, QStringLiteral("Advanced's copy buttons missing")); return; }
    // The words (mitcad#724): "ChatGPT / Codex" on its choice only; Advanced's
    // labels without a colon, the folder's button word, and the command
    // named PowerShell's on Windows.
    QString tile;
    for (QAbstractButton* button : assistantChoices(dialog)) {
      if (button->property("assistantClient").toInt() == int(mcp::AssistantClient::Codex)) tile = button->text();
    }
    auto* configTitle = dialog->findChild<QLabel*>(QStringLiteral("assistantConfigTitle"));
    auto* chooseConfig = dialog->findChild<QPushButton*>(QStringLiteral("assistantChooseConfig"));
    auto* commandBox = dialog->findChild<QWidget*>(QStringLiteral("assistantCommandBox"));
    auto* commandTitle = commandBox ? commandBox->findChild<QLabel*>() : nullptr;
#ifdef Q_OS_WIN
    const QString commandWord = QStringLiteral("PowerShell command");
#else
    const QString commandWord = QStringLiteral("Command");
#endif
    if (tile != QStringLiteral("ChatGPT / Codex") || !configTitle || configTitle->text() != QStringLiteral("Codex settings file") ||
        !chooseConfig || chooseConfig->text() != QStringLiteral("Change…") || !commandTitle || commandTitle->text() != commandWord) {
      fail(m_line, QStringLiteral("Codex's choice or Advanced's labels and buttons have the wrong words")); return;
    }
    copy->click();
    // Only the words that need it are quoted.
    if (!QApplication::clipboard()->text().contains(QStringLiteral("CODEX_HOME")) ||
        !QApplication::clipboard()->text().contains(QStringLiteral(" mcp add mitcad-desktop "))) {
      fail(m_line, QStringLiteral("copied command omitted the selected Codex home")); return;
    }
    // A settings file the CLI cannot address: the configuration, no command.
    const QString selected = config->text();
    config->setText(QDir(work).filePath(QStringLiteral("custom.toml")));
    emit config->editingFinished(); // as when the field loses focus
    copyConfiguration->click();
    if (copy->isVisibleTo(advanced) || !QApplication::clipboard()->text().startsWith(QStringLiteral("[mcp_servers.mitcad-desktop]"))) {
      fail(m_line, QStringLiteral("nonstandard Codex settings copied a misleading CLI registration")); return;
    }
    config->setText(selected);
    emit config->editingFinished();
    if (actionOf(setup) != QStringLiteral("connect")) { fail(m_line, QStringLiteral("Codex did not offer Connect")); return; }
    if (status->text() != QStringLiteral("Ready to connect Codex")) {
      fail(m_line, QStringLiteral("Codex's state line did not name it Codex: ") + status->text()); return;
    }
    setup->click();
    if (!dialog->folder().isEmpty() && !folderAllowed()) {
      fail(m_line, QStringLiteral("Connect did not allow the folder shown")); return;
    }
    const bool failure = !qEnvironmentVariableIsEmpty("MITCAD_TEST_ASSISTANT_FAIL") ||
                         !qEnvironmentVariableIsEmpty("MITCAD_TEST_ASSISTANT_CORRUPT") ||
                         !qEnvironmentVariableIsEmpty("MITCAD_TEST_ASSISTANT_CONCURRENT");
    const QString expected = original + (!qEnvironmentVariableIsEmpty("MITCAD_TEST_ASSISTANT_CONCURRENT")
        ? QStringLiteral("# Concurrent user edit\n") : QString());
    whenReady([status, result, setup, failure, dialog] {
      return setup->isEnabled() && (failure ? result->property("setupError").toBool() && stateOf(status) == QStringLiteral("failed")
          : stateOf(status) == QStringLiteral("waiting") && !setup->isVisibleTo(dialog));
    }, 10, QStringLiteral("assistant registration did not reach the expected setup state"),
      [this, config, dialog, failure, original, expected] {
        QString content, error;
        if (!mcp::readAssistantConfiguration(config->text(), content, error)) { fail(m_line, error); return; }
        if (!content.startsWith(original) || (failure && content != expected)) {
          fail(m_line, QStringLiteral("assistant setup changed unrelated text or failed to restore a rejected write")); return;
        }
        if (!failure) {
          const QDir folder(QFileInfo(config->text()).absolutePath());
          const auto backups = folder.entryList({QStringLiteral("*.mitcad*backup-*")}, QDir::Files);
          dialog->connectAssistant(); // An existing matching entry is idempotent.
          if (folder.entryList({QStringLiteral("*.mitcad*backup-*")}, QDir::Files) != backups) {
            fail(m_line, QStringLiteral("repeating assistant setup made another backup"));
          }
        }
      });
  } else if (mode == QStringLiteral("disconnect-other")) {
    // Disconnect ends the chosen assistant's clients only and names them
    // while others stay connected; with Other chosen, by the client's own
    // label (mitcad#724). A dialog of the driver's own, never shown, whose
    // host lists a Claude Code client and an `example-client`.
    const QDateTime since = QDateTime::currentDateTime();
    auto connected = std::make_shared<QList<mcp::ConnectedAgent>>(QList<mcp::ConnectedAgent>{
        {QStringLiteral("claude-code"), QStringLiteral("1.0"), since, QStringLiteral("claude"), false},
        {QStringLiteral("example-client"), QStringLiteral("1.0"), since, QStringLiteral("example"), false}});
    mcp::ConnectAssistantDialog::Host host{[connected] { return *connected; },
        [connected](const QStringList& ids) {
          connected->removeIf([&ids](const mcp::ConnectedAgent& agent) { return ids.contains(agent.id); });
        },
        work};
    std::unique_ptr<mcp::ConnectAssistantDialog> own(new mcp::ConnectAssistantDialog(std::move(host)));
    auto* ownSetup = own->findChild<QPushButton*>(QStringLiteral("assistantSetup"));
    auto* ownStatus = own->findChild<QLabel*>(QStringLiteral("assistantConnectionState"));
    if (!ownSetup || !ownStatus) { fail(m_line, QStringLiteral("the dialog's setup controls missing")); return; }
    chooseAssistant(own.get(), mcp::AssistantClient::ClaudeCode);
    if (stateOf(ownStatus) != QStringLiteral("connected") || ownSetup->text() != QStringLiteral("Disconnect Claude Code")) {
      fail(m_line, QStringLiteral("Claude Code's Disconnect did not name it: ") + ownSetup->text()); return;
    }
    chooseAssistant(own.get(), mcp::AssistantClient::Other);
    if (stateOf(ownStatus) != QStringLiteral("connected") || actionOf(ownSetup) != QStringLiteral("disconnect") ||
        ownSetup->text() != QStringLiteral("Disconnect example-client")) {
      fail(m_line, QStringLiteral("Other's Disconnect did not name its client: ") + ownSetup->text()); return;
    }
    ownSetup->click();
    if (connected->size() != 1 || connected->first().name != QStringLiteral("claude-code") ||
        actionOf(ownSetup) == QStringLiteral("disconnect")) {
      fail(m_line, QStringLiteral("Other's Disconnect did not end only its own client")); return;
    }
  } else {
    fail(m_line, QStringLiteral("unknown assistant setup test mode")); return;
  }
  qInfo().noquote() << QStringLiteral("Assistant setup UI: %1").arg(mode);
}

} // namespace mitcad
