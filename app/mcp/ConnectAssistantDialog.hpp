// SPDX-License-Identifier: MIT
#pragma once

#include "AgentIndicator.hpp"
#include "AssistantSetup.hpp"

#include <functional>
#include <QDateTime>
#include <QDialog>
#include <QList>
#include <QStringList>

class QButtonGroup;
class QLabel;
class QLineEdit;
class QMoveEvent;
class QPlainTextEdit;
class QResizeEvent;
class QShowEvent;
class QProcess;
class QPushButton;
class QToolButton;

namespace mitcad::mcp {

// Help › Connect AI Assistant (mitcad#682, laid out for clarity in
// mitcad#706): the assistant as a row of large choices, one state line that
// names the next step beside one primary button, the folder line, and the
// manual configuration and command under Advanced. The primary button's
// action follows the state: an installation page, Connect, Try again,
// Disconnect (the chosen assistant's clients only), Open in Claude Desktop
// again, Show in Explorer (the extension saved for a Claude Desktop that
// takes it from its own settings, mitcad#763), or Copy configuration for
// another assistant; a waiting state has none. Connect (and Copy configuration, Connect anyway) is the consent that
// allows the folder shown. Connected clients are named by the assistant
// their label matches (Claude Code), else by their own label.
class ConnectAssistantDialog : public QDialog {
public:
  struct Host {
    // The connected clients (initialized MCP sessions).
    std::function<QList<ConnectedAgent>()> clients;
    // Ends these clients' connections (ConnectedAgent::id).
    std::function<void(const QStringList& ids)> disconnect;
    QString suggestedFolder;
  };
  explicit ConnectAssistantDialog(Host host, QWidget* parent = nullptr);

  // The folder the next Connect allows; Change… sets it (and the test
  // driver, which cannot answer a native folder dialog).
  QString folder() const { return m_folderPath; }
  void setFolder(const QString& path);
  // What Connect and Connect anyway do: allow the folder shown, then set
  // the chosen assistant up (copy the configuration for another one). The
  // test driver repeats a setup with it where no button offers it.
  void connectAssistant();

protected:
  bool eventFilter(QObject* watched, QEvent* event) override;
  void changeEvent(QEvent* event) override;
  void showEvent(QShowEvent* event) override;
  void moveEvent(QMoveEvent* event) override;
  void resizeEvent(QResizeEvent* event) override;

private:
  // A client's settings file as last read: read again only when its size
  // or modification time changed (the state is polled every second).
  struct SettingsRead {
    QString path;
    qint64 size = -2;
    QDateTime modified;
    bool configured = false;
  };

  AssistantClient client() const;
  bool installed(AssistantClient which) const;
  bool configured(AssistantClient which) const;
  bool cachedCheck(SettingsRead& cache, const QString& path, const std::function<bool(const QString&)>& check,
                   qint64 limit = kAssistantConfigurationLimit) const;
  QStringList chosenClientIds() const;
  QString configPath() const;
  void applyStyles();
  void updateFolderText();
  void updateClient();
  // The command under Advanced, again when the client's place changed.
  void updateCommand();
  void fitConfigurationHeight();
  void refreshConnection();
  void refreshWindows();
  // The window's height to its contents, both ways; its width only grows.
  void refit();
  void primaryAction();
  void setup();
  void setupCodex();
  void setupClaudeCode();
  void setupBundle();
  void bundleNotOpened(const QString& path);
  void openClaudeDesktop();
  bool grantFolder();
  void message(const QString& text, bool error = false, const QString& detail = {});
  void clearMessage();
  void runRegistration(const QString& program, const QStringList& arguments,
                       std::function<void()> success);
  QString manualConfiguration() const;
  QString manualCommand() const;
  // The choices' places in the main window's coordinates, once laid out
  // (UI tests, mitcad#760); logged again only when they change.
  void logPlaces();

  Host m_host;
  QString m_bridge;
  QString m_folderPath;
  QString m_ownInstance;
  QString m_bundleSaved; // the extension saved for a Claude Desktop that no application opened it in
  QString m_shownState;  // the state whose look is applied (the error colour)
  QString m_shownLayout; // which rows were shown at the last fit
  QString m_loggedState;  // the choice and state line logged last
  QString m_loggedPlaces; // the choices' places logged last
  QButtonGroup* m_clients = nullptr;
  QLineEdit* m_config = nullptr;
  QWidget* m_configRow = nullptr;
  QLabel* m_folder = nullptr;
  QWidget* m_folderRow = nullptr;
  QLabel* m_state = nullptr;
  QLabel* m_result = nullptr;
  QLabel* m_also = nullptr;
  QPlainTextEdit* m_configuration = nullptr;
  QLineEdit* m_command = nullptr;
  QWidget* m_commandBox = nullptr;
  QPushButton* m_setup = nullptr;
  QPushButton* m_checkAgain = nullptr;
  QPushButton* m_connectAnyway = nullptr;
  QPushButton* m_openDesktop = nullptr;
  QPushButton* m_copyConfiguration = nullptr;
  QPushButton* m_copyCommand = nullptr;
  QToolButton* m_advancedToggle = nullptr;
  QWidget* m_advanced = nullptr;
  QLabel* m_windows = nullptr;
  QWidget* m_windowsRow = nullptr;
  QProcess* m_process = nullptr;
  mutable SettingsRead m_claudeRead;
  mutable SettingsRead m_codexRead;
  bool m_bundleOpened = false;    // Claude Desktop's installer opened in this dialog
  bool m_desktopNotFound = false; // no application takes the Claude Desktop extension
  bool m_copied = false;          // another assistant's configuration was copied
};

// Whether Claude Desktop's application is on this computer (an isolated
// test run looks at its client executables): the chat panel, which needs
// Claude Code, points it to Connect AI Assistant (mitcad#947).
bool claudeDesktopOnComputer();

} // namespace mitcad::mcp
