// SPDX-License-Identifier: MIT
#include "ConnectAssistantDialog.hpp"

#include <exception>
#include <memory>
#include <string>
#include <utility>

#include <QApplication>
#include <QButtonGroup>
#include <QClipboard>
#include <QCoreApplication>
#include <QDesktopServices>
#include <QDialogButtonBox>
#include <QDir>
#include <QEvent>
#include <QFile>
#include <QFileInfo>
#include <QFont>
#include <QFontDatabase>
#include <QFontMetrics>
#include <QHBoxLayout>
#include <QJsonArray>
#include <QJsonDocument>
#include <QLabel>
#include <QLayout>
#include <QLineEdit>
#include <QMoveEvent>
#include <QPlainTextEdit>
#include <QProcess>
#include <QProcessEnvironment>
#include <QPushButton>
#include <QResizeEvent>
#include <QSaveFile>
#include <QScrollBar>
#include <QSettings>
#include <QShowEvent>
#include <QStandardPaths>
#include <QTextDocument>
#include <QTimer>
#include <QTemporaryDir>
#include <QToolButton>
#include <QUrl>
#include <QVBoxLayout>

#ifdef Q_OS_WIN
#include <windows.h>
#include <appmodel.h>
#include <shlwapi.h>
#endif

#include "AgentFolders.hpp"
#include "LocalEndpoint.hpp"
#include "../framework/Dialogs.hpp"
#include "../framework/FitToScreen.hpp"
#include "../framework/Icons.hpp"
#include "../framework/Json.hpp"
#include "../framework/Theme.hpp"
#include "../platform/MacChrome.hpp"
#include "mitcad_bridge/lib.h"

namespace mitcad::mcp {
namespace {
constexpr bool windows =
#ifdef Q_OS_WIN
    true;
#else
    false;
#endif

// Claude Desktop's extensions install on Windows and macOS only.
constexpr bool desktopExtensions =
#if defined(Q_OS_WIN) || defined(Q_OS_MACOS)
    true;
#else
    false;
#endif

// The Advanced configuration's height at most; a longer one scrolls.
constexpr int kConfigurationHeight = 200;

const QString kClientSetting = QStringLiteral("assistant/client");

QString jsonString(const QString& text) {
  const QByteArray list = QJsonDocument(QJsonArray{text}).toJson(QJsonDocument::Compact);
  return QString::fromUtf8(list.mid(1, list.size() - 2));
}

// A TOML string: a literal one keeps a Windows path's backslashes as they
// are; a basic one (JSON's escapes are TOML's) where a literal cannot be.
QString tomlString(const QString& text) {
  bool literal = !text.contains(QLatin1Char('\''));
  for (const QChar character : text) {
    if ((character.unicode() < 0x20 && character != QLatin1Char('\t')) || character.unicode() == 0x7f) literal = false;
  }
  return literal ? QString(QLatin1Char('\'') + text + QLatin1Char('\'')) : jsonString(text);
}

QString codexToml(const QString& bridge) {
  return QStringLiteral("[mcp_servers.mitcad-desktop]\ncommand = %1\nargs = [\"mcp\", \"--connect\"]\n").arg(tomlString(bridge));
}

QString codexHomeCommand(const QString& command, const QString& path) {
  const QString home = QDir::toNativeSeparators(QFileInfo(path).absolutePath());
  // PowerShell assigns only a quoted string.
  return windows ? QStringLiteral("$env:CODEX_HOME = %1; %2").arg(assistantShellWord(home, true, true), command)
                 : QStringLiteral("CODEX_HOME=%1 %2").arg(assistantShellWord(home, false), command);
}

// The CLI registers only into CODEX_HOME/config.toml; another file chosen
// under Advanced gets the TOML by hand.
bool standardCodexFile(const QString& path) { return QFileInfo(path).fileName() == QStringLiteral("config.toml"); }

QJsonObject codexPlan(const QString& current, const QString& bridge, bool replace = false) {
  return parseObject(plan_codex_mcp_config(rustStr(current.toUtf8()), rustStr(bridge.toUtf8()), replace));
}

// Whether Codex's settings already start this bridge: as setup writes it,
// or with the platform's separators as the manual configuration shows it.
bool codexHasBridge(const QString& current, const QString& bridge) {
  const auto unchanged = [&current](const QString& form) {
    return codexPlan(current, form).value(QStringLiteral("status")).toString() == QStringLiteral("unchanged");
  };
  const QString native = QDir::toNativeSeparators(bridge);
  return unchanged(bridge) || (native != bridge && unchanged(native));
}

// A Claude Code entry that starts this bridge (with either separator).
bool claudeEntryStarts(const QJsonObject& entry, const QString& bridge) {
  return QDir::fromNativeSeparators(entry.value(QStringLiteral("command")).toString()) == QDir::fromNativeSeparators(bridge) &&
      entry.value(QStringLiteral("args")) == QJsonArray{QStringLiteral("mcp"), QStringLiteral("--connect")};
}

bool confirmReplacement(QWidget* parent, const QString& name) {
  return sheetQuestion(parent, QObject::tr("Replace MitCad Connection"),
      QObject::tr("%1 is already connected to another MitCad installation. Connect it to this one instead? "
                  "Your current settings are backed up first.").arg(name), QMessageBox::Yes | QMessageBox::No,
                  QMessageBox::No) == QMessageBox::Yes;
}

QString claudeUserConfiguration() {
  const QString work = qEnvironmentVariable("MITCAD_ASSISTANT_TEST_WORK");
  if (!qEnvironmentVariableIsEmpty("MITCAD_TEST_INPUT") && !work.isEmpty()) {
    return QDir(work).filePath(QStringLiteral("claude.json"));
  }
  const QString custom = qEnvironmentVariable("CLAUDE_CONFIG_DIR");
  if (!custom.isEmpty()) return QDir(custom).absoluteFilePath(QStringLiteral(".claude.json"));
  return QDir::home().filePath(QStringLiteral(".claude.json"));
}

// A test run with its isolated work folder (MITCAD_ASSISTANT_TEST_WORK): the
// dialog opens no other application, it records what it would open.
QString isolatedTestWork() {
  return qEnvironmentVariableIsEmpty("MITCAD_TEST_INPUT") ? QString() : qEnvironmentVariable("MITCAD_ASSISTANT_TEST_WORK");
}

#ifdef Q_OS_WIN
// Whether Claude Desktop's Microsoft Store package is installed for this
// user, by the package manager: its data folder can outlive it.
bool claudeDesktopPackageInstalled() {
  const std::wstring family = QString::fromLatin1(kClaudeDesktopPackageFamily).toStdWString();
  UINT32 count = 0;
  UINT32 length = 0;
  const LONG status = GetPackagesByPackageFamily(family.c_str(), &count, nullptr, &length, nullptr);
  return (status == ERROR_SUCCESS || status == ERROR_INSUFFICIENT_BUFFER) && count > 0;
}
#endif

// Whether Claude Desktop is on this computer (mitcad#763): with
// `application` its application or its Microsoft Store package only;
// without, also the settings folder it makes on its first start, which stays
// when it is uninstalled. A test run looks only for a `claude-desktop` file
// among its isolated client executables and, on Windows, in the work
// folder's `local-app-data` and `roaming-app-data` (its LOCALAPPDATA and
// APPDATA), where the package's folder stands for the package.
bool claudeDesktopFound(bool application) {
  if (!qEnvironmentVariableIsEmpty("MITCAD_TEST_INPUT")) {
    const QString folder = qEnvironmentVariable("MITCAD_TEST_ASSISTANT_EXECUTABLES");
    if (!folder.isEmpty()) {
      if (folder == QStringLiteral("none")) return false;
      if (QFileInfo::exists(QDir(folder).filePath(QStringLiteral("claude-desktop")))) return true;
#ifdef Q_OS_WIN
      const QString work = isolatedTestWork();
      if (work.isEmpty()) return false;
      const QString local = QDir(work).filePath(QStringLiteral("local-app-data"));
      return application ? claudeDesktopApplicationInWindowsFolders(local)
                         : claudeDesktopInWindowsFolders(local, QDir(work).filePath(QStringLiteral("roaming-app-data")));
#else
      return false;
#endif
    }
  }
#if defined(Q_OS_MACOS)
  QStringList paths{QStringLiteral("/Applications/Claude.app"), QDir::home().filePath(QStringLiteral("Applications/Claude.app"))};
  if (!application) paths << QDir::home().filePath(QStringLiteral("Library/Application Support/Claude"));
  for (const QString& path : paths) {
    if (QFileInfo(path).isDir()) return true;
  }
  return false;
#elif defined(Q_OS_WIN)
  const QString local = qEnvironmentVariable("LOCALAPPDATA");
  if (!local.isEmpty() && QFileInfo(QDir(local).filePath(QStringLiteral("AnthropicClaude"))).isDir()) return true;
  if (claudeDesktopPackageInstalled()) return true;
  return !application && claudeDesktopInWindowsFolders(QString(), qEnvironmentVariable("APPDATA"));
#else
  Q_UNUSED(application);
  return false;
#endif
}

bool claudeDesktopInstalled() { return claudeDesktopFound(false); }
bool claudeDesktopApplicationFound() { return claudeDesktopFound(true); }

// Whether an application takes Claude Desktop's extension files. Windows
// offers "Open with" for a file no application takes, so that opening one
// succeeds says nothing there; the association does. Without
// ASSOCF_INIT_IGNOREUNKNOWN the query answers with the "Unknown" type's
// handler, Open With itself, so that is no handler either. Elsewhere
// opening is the test. In an isolated test run,
// MITCAD_TEST_ASSISTANT_BUNDLE_OPEN=none stands for no application, without
// the registry.
bool bundleHandlerRegistered() {
  if (!isolatedTestWork().isEmpty() && qEnvironmentVariableIsSet("MITCAD_TEST_ASSISTANT_BUNDLE_OPEN")) {
    return qEnvironmentVariable("MITCAD_TEST_ASSISTANT_BUNDLE_OPEN") != QStringLiteral("none");
  }
#ifdef Q_OS_WIN
  DWORD size = 0;
  if (AssocQueryStringW(ASSOCF_INIT_IGNOREUNKNOWN, ASSOCSTR_EXECUTABLE, L".mcpb", nullptr, nullptr, &size) != S_FALSE ||
      size <= 1) {
    return false;
  }
  std::wstring executable(size, L'\0');
  if (AssocQueryStringW(ASSOCF_INIT_IGNOREUNKNOWN, ASSOCSTR_EXECUTABLE, L".mcpb", nullptr, executable.data(), &size) !=
      S_OK) {
    return false;
  }
  const QString program = QFileInfo(QString::fromWCharArray(executable.c_str())).fileName();
  return !program.isEmpty() && program.compare(QStringLiteral("OpenWith.exe"), Qt::CaseInsensitive) != 0;
#else
  return true;
#endif
}

// The button that shows a file in the platform's file manager.
QString showFileText() {
#if defined(Q_OS_WIN)
  return QObject::tr("Show in Explorer");
#elif defined(Q_OS_MACOS)
  return QObject::tr("Show in Finder");
#else
  return QObject::tr("Show in Folder");
#endif
}

// Shows the file in the file manager, selected where it can be; else its
// folder.
void showFile(const QString& path) {
#if defined(Q_OS_WIN)
  QProcess explorer;
  explorer.setProgram(QStringLiteral("explorer.exe"));
  // Explorer reads `/select,"path"` its own way, not as quoted arguments.
  explorer.setNativeArguments(QStringLiteral("/select,\"%1\"").arg(QDir::toNativeSeparators(path)));
  if (explorer.startDetached()) return;
#elif defined(Q_OS_MACOS)
  if (QProcess::startDetached(QStringLiteral("/usr/bin/open"), {QStringLiteral("-R"), path})) return;
#endif
  QDesktopServices::openUrl(QUrl::fromLocalFile(QFileInfo(path).absolutePath()));
}

QString installPage(AssistantClient client) {
  switch (client) {
  case AssistantClient::ClaudeDesktop: return QStringLiteral("https://claude.ai/download");
  case AssistantClient::ClaudeCode: return QStringLiteral("https://code.claude.com/docs/en/quickstart");
  case AssistantClient::Codex: return QStringLiteral("https://developers.openai.com/codex");
  case AssistantClient::Other: break;
  }
  return {};
}

// One short line per assistant choice.
QString choiceTip(AssistantClient client) {
  switch (client) {
  case AssistantClient::ClaudeDesktop: return QObject::tr("The Claude app");
  case AssistantClient::ClaudeCode: return QObject::tr("Claude in a terminal");
  case AssistantClient::Codex: return QObject::tr("ChatGPT's desktop app or the Codex CLI");
  case AssistantClient::Other: return QObject::tr("Any assistant that takes an MCP configuration");
  }
  return {};
}

// A copy button says "Copied" for a moment.
void showCopied(QPushButton* button) {
  const QString label = button->property("label").toString();
  button->setText(QObject::tr("Copied ✓"));
  QTimer::singleShot(1500, button, [button, label] { button->setText(label); });
}

QPushButton* copyButton(const QString& text, const char* name, const QString& accessible = {}) {
  auto* button = new QPushButton(text);
  button->setObjectName(QLatin1String(name));
  button->setProperty("label", text);
  button->setAutoDefault(false);
  if (!accessible.isEmpty()) { button->setAccessibleName(accessible); button->setToolTip(accessible); }
  return button;
}

// A labelled field of Advanced: its title above it, the field's button (Copy,
// Change…) at the right of the title's row.
QWidget* labelledField(const QString& title, QPushButton* button, QWidget* field, const char* labelName = nullptr) {
  auto* box = new QWidget;
  auto* column = new QVBoxLayout(box);
  column->setContentsMargins(0, 0, 0, 0);
  column->setSpacing(4);
  auto* header = new QHBoxLayout;
  auto* label = new QLabel(title);
  if (labelName) label->setObjectName(QLatin1String(labelName));
  label->setBuddy(field);
  header->addWidget(label, 1);
  header->addWidget(button);
  column->addLayout(header);
  column->addWidget(field);
  return box;
}

QString rgba(const QColor& colour, int alpha) {
  return QStringLiteral("rgba(%1, %2, %3, %4)").arg(colour.red()).arg(colour.green()).arg(colour.blue()).arg(alpha);
}
}

ConnectAssistantDialog::ConnectAssistantDialog(Host host, QWidget* parent)
    : QDialog(parent), m_host(std::move(host)), m_bridge(installedBridge(QCoreApplication::applicationDirPath())) {
  setObjectName(QStringLiteral("connectAssistantDialog"));
  setWindowTitle(tr("Connect AI Assistant"));
  setAttribute(Qt::WA_DeleteOnClose);
  // A small dialog: its rows straight in its layout, no scroll area.
  auto* layout = new QVBoxLayout(this);
  layout->setSpacing(12);

  // 1. The assistant: large choices with an icon and a name.
  auto* choices = new QWidget;
  choices->setObjectName(QStringLiteral("assistantClient"));
  auto* choiceRow = new QHBoxLayout(choices);
  choiceRow->setContentsMargins(0, 0, 0, 0);
  m_clients = new QButtonGroup(this);
  m_clients->setExclusive(true);
  const struct { AssistantClient client; const char* icon; } kChoices[] = {
      {AssistantClient::ClaudeDesktop, "assistant-desktop"}, {AssistantClient::ClaudeCode, "assistant-terminal"},
      {AssistantClient::Codex, "assistant-chat"}, {AssistantClient::Other, "assistant-other"}};
  for (const auto& choice : kChoices) {
    auto* button = new QToolButton;
    button->setText(assistantName(choice.client));
    button->setToolTip(choiceTip(choice.client));
    button->setIcon(themeIcon(QString::fromLatin1(choice.icon)));
    button->setIconSize(QSize(32, 32));
    button->setToolButtonStyle(Qt::ToolButtonTextUnderIcon);
    button->setCheckable(true);
    button->setAutoRaise(true);
    button->setMinimumWidth(112);
    button->setSizePolicy(QSizePolicy::Expanding, QSizePolicy::Preferred);
    button->setProperty("assistantClient", int(choice.client));
    m_clients->addButton(button, int(choice.client));
    choiceRow->addWidget(button);
  }
  layout->addWidget(choices);

  // 2. Connect: the state line naming the next step and the primary button.
  auto* stateRow = new QHBoxLayout;
  m_state = new QLabel;
  m_state->setTextFormat(Qt::PlainText);
  m_state->setObjectName(QStringLiteral("assistantConnectionState"));
  m_state->setWordWrap(true);
  QFont stateFont = m_state->font();
  stateFont.setBold(true);
  m_state->setFont(stateFont);
  stateRow->addWidget(m_state, 1);
  m_setup = new QPushButton(tr("Connect"));
  m_setup->setObjectName(QStringLiteral("assistantSetup"));
  m_setup->setDefault(true);
  stateRow->addWidget(m_setup);
  layout->addLayout(stateRow);
  // Not found is a guess: a quiet way to set it up all the same.
  auto* anywayRow = new QHBoxLayout;
  anywayRow->addStretch();
  // After How to install: look again without reopening the dialog.
  m_checkAgain = new QPushButton(tr("Check again"));
  m_checkAgain->setObjectName(QStringLiteral("assistantCheckAgain"));
  m_checkAgain->setAutoDefault(false);
  m_checkAgain->setFlat(true);
  m_checkAgain->setCursor(Qt::PointingHandCursor);
  m_checkAgain->hide();
  anywayRow->addWidget(m_checkAgain);
  m_connectAnyway = new QPushButton(tr("Connect anyway"));
  m_connectAnyway->setObjectName(QStringLiteral("assistantConnectAnyway"));
  m_connectAnyway->setAutoDefault(false);
  m_connectAnyway->setFlat(true);
  m_connectAnyway->setCursor(Qt::PointingHandCursor);
  anywayRow->addWidget(m_connectAnyway);
  // The extension saved but not opened: Claude Desktop itself, quietly.
  m_openDesktop = new QPushButton(tr("Open Claude Desktop"));
  m_openDesktop->setObjectName(QStringLiteral("assistantOpenDesktop"));
  m_openDesktop->setAutoDefault(false);
  m_openDesktop->setFlat(true);
  m_openDesktop->setCursor(Qt::PointingHandCursor);
  m_openDesktop->hide();
  anywayRow->addWidget(m_openDesktop);
  layout->addLayout(anywayRow);
  m_result = new QLabel;
  m_result->setTextFormat(Qt::PlainText);
  m_result->setObjectName(QStringLiteral("assistantSetupResult"));
  m_result->setWordWrap(true);
  m_result->setTextInteractionFlags(Qt::TextSelectableByMouse);
  m_result->hide();
  layout->addWidget(m_result);
  m_also = new QLabel;
  m_also->setTextFormat(Qt::PlainText);
  m_also->setObjectName(QStringLiteral("assistantAlsoConnected"));
  m_also->setWordWrap(true);
  m_also->setForegroundRole(QPalette::PlaceholderText);
  m_also->hide();
  layout->addWidget(m_also);

  // 3. The folder Connect allows: the path cut to the room there is.
  m_folderRow = new QWidget;
  auto* folderRow = new QHBoxLayout(m_folderRow);
  folderRow->setContentsMargins(0, 0, 0, 0);
  m_folder = new QLabel;
  m_folder->setObjectName(QStringLiteral("assistantFolder"));
  m_folder->setTextFormat(Qt::PlainText);
  m_folder->setSizePolicy(QSizePolicy::Ignored, QSizePolicy::Preferred);
  m_folder->setMinimumWidth(120);
  m_folder->installEventFilter(this);
  folderRow->addWidget(m_folder, 1);
  auto* changeFolder = new QPushButton(tr("Change…"));
  changeFolder->setObjectName(QStringLiteral("assistantChangeFolder"));
  changeFolder->setToolTip(tr("The folder the assistant can open designs from and save them to"));
  changeFolder->setAutoDefault(false);
  folderRow->addWidget(changeFolder);
  layout->addWidget(m_folderRow);

  // Advanced: the manual configuration and command, the Codex settings file
  // and several MitCad windows.
  m_advancedToggle = new QToolButton;
  m_advancedToggle->setObjectName(QStringLiteral("assistantAdvancedToggle"));
  m_advancedToggle->setText(tr("Advanced"));
  m_advancedToggle->setCheckable(true);
  m_advancedToggle->setAutoRaise(true);
  m_advancedToggle->setToolButtonStyle(Qt::ToolButtonTextBesideIcon);
#ifdef Q_OS_MACOS
  m_advancedToggle->setIcon(mac::symbolIcon(QStringLiteral("chevron.right")));
#else
  m_advancedToggle->setArrowType(Qt::RightArrow);
#endif
  layout->addWidget(m_advancedToggle, 0, Qt::AlignLeft);
  m_advanced = new QWidget;
  m_advanced->setObjectName(QStringLiteral("assistantAdvanced"));
  auto* advanced = new QVBoxLayout(m_advanced);
  advanced->setContentsMargins(0, 0, 0, 0);
  advanced->setSpacing(10);
  // Advanced's labels without a colon, each above its field; its buttons'
  // words as the folder's.
  m_config = new QLineEdit(QDir::toNativeSeparators(codexConfigurationPath()));
  m_config->setObjectName(QStringLiteral("assistantConfigPath"));
  auto* chooseConfig = new QPushButton(tr("Change…"));
  chooseConfig->setObjectName(QStringLiteral("assistantChooseConfig"));
  chooseConfig->setAutoDefault(false);
  m_configRow = labelledField(tr("Codex settings file"), chooseConfig, m_config, "assistantConfigTitle");
  advanced->addWidget(m_configRow);
  const QFont code = QFontDatabase::systemFont(QFontDatabase::FixedFont);
  m_configuration = new QPlainTextEdit;
  m_configuration->setReadOnly(true);
  m_configuration->setObjectName(QStringLiteral("assistantConfiguration"));
  m_configuration->setLineWrapMode(QPlainTextEdit::NoWrap);
  m_configuration->setFont(code);
  m_copyConfiguration = copyButton(tr("Copy"), "assistantCopyConfiguration", tr("Copy configuration"));
  advanced->addWidget(labelledField(tr("Configuration"), m_copyConfiguration, m_configuration));
  m_command = new QLineEdit;
  m_command->setReadOnly(true);
  m_command->setObjectName(QStringLiteral("assistantCommand"));
  m_command->setFont(code);
  m_copyCommand = copyButton(tr("Copy"), "assistantCopyCommand", tr("Copy command"));
  // The commands on Windows are PowerShell's (its quoting, $env:CODEX_HOME).
  m_commandBox = labelledField(windows ? tr("PowerShell command") : tr("Command"), m_copyCommand, m_command);
  m_commandBox->setObjectName(QStringLiteral("assistantCommandBox"));
  advanced->addWidget(m_commandBox);
  m_windowsRow = new QWidget;
  auto* windowsRow = new QHBoxLayout(m_windowsRow);
  windowsRow->setContentsMargins(0, 0, 0, 0);
  m_windows = new QLabel;
  m_windows->setObjectName(QStringLiteral("assistantWindows"));
  windowsRow->addWidget(m_windows, 1);
  auto* copyWindow = copyButton(tr("Copy Command for This Window"), "assistantCopyWindowCommand");
  copyWindow->setToolTip(tr("A command that connects to this window only; a saved setup never names a window"));
  windowsRow->addWidget(copyWindow);
  m_windowsRow->hide();
  advanced->addWidget(m_windowsRow);
  m_advanced->hide();
  layout->addWidget(m_advanced);
  layout->addStretch();
  auto* buttons = new QDialogButtonBox(QDialogButtonBox::Close);
  for (QAbstractButton* button : buttons->buttons()) {
    if (auto* push = qobject_cast<QPushButton*>(button)) push->setAutoDefault(false);
  }
  connect(buttons, &QDialogButtonBox::rejected, this, &QDialog::close);
  layout->addWidget(buttons);

  connect(m_clients, &QButtonGroup::idClicked, this, [this] { updateClient(); });
  connect(m_config, &QLineEdit::editingFinished, this, [this] { updateClient(); });
  connect(m_setup, &QPushButton::clicked, this, [this] { primaryAction(); });
  connect(m_checkAgain, &QPushButton::clicked, this, [this] {
    refreshConnection();
    // Still not found: the button says so for a moment.
    if (m_checkAgain->isVisibleTo(this)) {
      m_checkAgain->setText(tr("Not found yet"));
      QTimer::singleShot(1500, m_checkAgain, [this] { m_checkAgain->setText(tr("Check again")); });
    }
  });
  connect(m_connectAnyway, &QPushButton::clicked, this, [this] { connectAssistant(); });
  connect(m_openDesktop, &QPushButton::clicked, this, [this] { openClaudeDesktop(); });
  connect(changeFolder, &QPushButton::clicked, this, [this] {
    const QString chosen = dialogDirectory(this, tr("Folder for Designs"), m_folderPath);
    if (!chosen.isEmpty()) setFolder(chosen);
  });
  connect(m_advancedToggle, &QToolButton::toggled, this, [this](bool open) {
#ifdef Q_OS_MACOS
    m_advancedToggle->setIcon(mac::symbolIcon(open ? QStringLiteral("chevron.down") : QStringLiteral("chevron.right")));
#else
    m_advancedToggle->setArrowType(open ? Qt::DownArrow : Qt::RightArrow);
#endif
    if (open) refreshWindows();
    m_advanced->setVisible(open);
    if (open) {
      // The configuration's width is known only once Advanced is laid out:
      // its height counts the scroll bar from that width.
      this->layout()->activate();
      m_advanced->layout()->activate();
      fitConfigurationHeight();
    }
    refit();
  });
  connect(chooseConfig, &QPushButton::clicked, this, [this] {
    const QString chosen = dialogOpenFileName(this, tr("Codex Settings File"), configPath(), tr("TOML files (*.toml)"));
    if (!chosen.isEmpty()) { m_config->setText(QDir::toNativeSeparators(chosen)); updateClient(); }
  });
  connect(m_copyConfiguration, &QPushButton::clicked, this, [this] {
    QApplication::clipboard()->setText(manualConfiguration());
    showCopied(m_copyConfiguration);
  });
  connect(m_copyCommand, &QPushButton::clicked, this, [this] {
    QApplication::clipboard()->setText(manualCommand());
    showCopied(m_copyCommand);
  });
  connect(copyWindow, &QPushButton::clicked, this, [this, copyWindow] {
    QApplication::clipboard()->setText(assistantCommand(m_bridge,
        {QStringLiteral("mcp"), QStringLiteral("--connect"), QStringLiteral("--instance"), m_ownInstance}, windows));
    showCopied(copyWindow);
  });

  applyStyles();
  // The folder already allowed, else the suggested one.
  const QStringList allowed = savedAgentFolders();
  setFolder(allowed.isEmpty() ? m_host.suggestedFolder : allowed.first());
  // The last assistant chosen, else the first one installed here, else
  // Claude Desktop where it can be installed.
  QAbstractButton* initial = m_clients->button(QSettings().value(kClientSetting, -1).toInt());
  for (const AssistantClient candidate : {AssistantClient::ClaudeDesktop, AssistantClient::ClaudeCode, AssistantClient::Codex}) {
    if (initial) break;
    if ((candidate != AssistantClient::ClaudeDesktop || desktopExtensions) && installed(candidate)) {
      initial = m_clients->button(int(candidate));
    }
  }
  if (!initial) initial = m_clients->button(int(desktopExtensions ? AssistantClient::ClaudeDesktop : AssistantClient::ClaudeCode));
  initial->setChecked(true);
  // The state is read every second: installations, setups and connections
  // change outside the dialog. Settings files are read again only when they
  // changed (cachedCheck).
  auto* timer = new QTimer(this); timer->setInterval(1000);
  connect(timer, &QTimer::timeout, this, [this] {
    if (m_advanced->isVisible()) refreshWindows();
    refreshConnection();
  });
  timer->start();
  updateClient();
  fitWindowToContents(this);
}

AssistantClient ConnectAssistantDialog::client() const { return AssistantClient(m_clients->checkedId()); }

// The Codex settings file as typed (shown with the platform's separators).
QString ConnectAssistantDialog::configPath() const { return QDir::fromNativeSeparators(m_config->text().trimmed()); }

// The look that follows the palette: tiles outlined and tinted in the
// accent colour when chosen (the icon stays visible), the primary button
// filled with it (the system's default button on macOS), and the Advanced
// disclosure as a plain chevron.
void ConnectAssistantDialog::applyStyles() {
  const QColor accent = accentColor(palette());
  const QString tile = QStringLiteral(
      "QToolButton { border: 1px solid transparent; border-radius: 8px; padding: 6px 4px; background: transparent; }"
      "QToolButton:hover { background: rgba(128, 128, 128, 40); }"
      "QToolButton:checked { border: 1.5px solid %1; background: %2; }")
      .arg(accent.name(), rgba(accent, 36));
  for (QAbstractButton* button : m_clients->buttons()) button->setStyleSheet(tile);
  m_advancedToggle->setStyleSheet(QStringLiteral(
      "QToolButton { border: none; background: transparent; padding: 2px 0; }"));
  for (QPushButton* quiet : {m_checkAgain, m_connectAnyway, m_openDesktop}) {
    quiet->setStyleSheet(QStringLiteral(
        "QPushButton { border: none; background: transparent; color: palette(link); padding: 0 2px; }"
        "QPushButton:hover { text-decoration: underline; }"));
  }
#ifndef Q_OS_MACOS
  const QColor text = palette().color(QPalette::HighlightedText);
  m_setup->setStyleSheet(QStringLiteral(
      "QPushButton { background: %1; color: %2; border: 1px solid %1; border-radius: 5px; padding: 5px 16px;"
      " font-weight: 600; }"
      "QPushButton:hover { background: %3; border-color: %3; }"
      "QPushButton:pressed { background: %4; border-color: %4; }"
      "QPushButton:disabled { background: %5; border-color: transparent; }")
      .arg(accent.name(), text.name(), accent.lighter(112).name(), accent.darker(120).name(), rgba(accent, 90)));
#endif
}

bool ConnectAssistantDialog::eventFilter(QObject* watched, QEvent* event) {
  if (watched == m_folder && event->type() == QEvent::Resize) updateFolderText();
  return QDialog::eventFilter(watched, event);
}

void ConnectAssistantDialog::changeEvent(QEvent* event) {
  QDialog::changeEvent(event);
  if (event->type() == QEvent::PaletteChange || event->type() == QEvent::ApplicationPaletteChange) applyStyles();
  // Back from an installer or a terminal: look again at once.
  if (event->type() == QEvent::ActivationChange && m_clients && m_clients->checkedButton()) refreshConnection();
}

void ConnectAssistantDialog::showEvent(QShowEvent* event) {
  QDialog::showEvent(event);
  // Each time it is shown, also where it was before.
  m_loggedPlaces.clear();
  QTimer::singleShot(0, this, [this] { logPlaces(); });
}

void ConnectAssistantDialog::moveEvent(QMoveEvent* event) {
  QDialog::moveEvent(event);
  QTimer::singleShot(0, this, [this] { logPlaces(); });
}

void ConnectAssistantDialog::resizeEvent(QResizeEvent* event) {
  QDialog::resizeEvent(event);
  QTimer::singleShot(0, this, [this] { logPlaces(); });
}

void ConnectAssistantDialog::logPlaces() {
  if (!isVisible()) return;
  // The main window: the dialog's parent, or the window above Preferences
  // when the dialog was opened from there.
  QWidget* main = parentWidget() != nullptr ? parentWidget()->window() : nullptr;
  while (main != nullptr && main->parentWidget() != nullptr) main = main->parentWidget()->window();
  if (main == nullptr) return;
  QStringList lines;
  for (QAbstractButton* button : m_clients->buttons()) {
    const QPoint at = button->mapToGlobal(button->rect().center()) - main->mapToGlobal(QPoint(0, 0));
    lines << QStringLiteral("Connect AI Assistant choice %1 at %2,%3").arg(button->text()).arg(at.x()).arg(at.y());
  }
  const QString places = lines.join(QLatin1Char('\n'));
  if (places == m_loggedPlaces) return;
  m_loggedPlaces = places;
  for (const QString& line : lines) qDebug().noquote() << line;
}

void ConnectAssistantDialog::setFolder(const QString& path) {
  m_folderPath = path.trimmed();
  if (auto* change = findChild<QPushButton*>(QStringLiteral("assistantChangeFolder"))) {
    change->setText(m_folderPath.isEmpty() ? tr("Choose…") : tr("Change…"));
  }
  updateFolderText();
}

// "Folder for designs: <path>", the path cut in the middle to the label's
// width; the whole path and what the folder is for in its tooltip.
void ConnectAssistantDialog::updateFolderText() {
  const QString native = QDir::toNativeSeparators(m_folderPath);
  const QString purpose = tr("The folder the assistant can open designs from and save them to");
  m_folder->setToolTip(native.isEmpty() ? purpose : QString(native + QLatin1Char('\n') + purpose));
  if (m_folderPath.isEmpty()) { m_folder->setText(tr("Folder for designs: none chosen")); return; }
  const QString format = tr("Folder for designs: %1");
  const QFontMetrics metrics = m_folder->fontMetrics();
  const int room = m_folder->width() - metrics.horizontalAdvance(format.arg(QString())) - 4;
  m_folder->setText(format.arg(metrics.elidedText(native, Qt::ElideMiddle, qMax(room, 40))));
}

void ConnectAssistantDialog::message(const QString& text, bool error, const QString& detail) {
  m_result->setText(text);
  m_result->setToolTip(detail);
  m_result->setProperty("setupError", error);
  // A setup may have written the settings within the same second.
  m_claudeRead = {};
  m_codexRead = {};
  qInfo().noquote() << QStringLiteral("Assistant setup: %1").arg(error ? QStringLiteral("error") : QStringLiteral("updated"));
  refreshConnection();
}

void ConnectAssistantDialog::clearMessage() {
  m_result->clear();
  m_result->setToolTip(QString());
  m_result->setProperty("setupError", false);
}

void ConnectAssistantDialog::updateClient() {
  clearMessage();
  m_copied = false;
  m_desktopNotFound = false;
  m_bundleSaved.clear();
  const auto selected = client();
  QSettings().setValue(kClientSetting, int(selected));
  m_configRow->setVisible(selected == AssistantClient::Codex);
  // Another assistant's primary button is Copy configuration already.
  m_copyConfiguration->setVisible(selected != AssistantClient::Other);
  m_configuration->setPlainText(manualConfiguration().trimmed());
  fitConfigurationHeight();
  refreshConnection();
}

// A Codex settings file the CLI cannot address takes the configuration
// only: no command. A client found outside PATH is named by its full path.
void ConnectAssistantDialog::updateCommand() {
  const QString command = manualCommand();
  m_commandBox->setVisible(!command.isEmpty());
  if (command == m_command->text()) return;
  m_command->setText(command);
  m_command->setCursorPosition(0);
  m_command->setToolTip(command);
}

// The configuration's lines without empty room below them, up to
// kConfigurationHeight; with the horizontal scroll bar's height when a line
// is wider than the box, or the bar covers the last line.
void ConnectAssistantDialog::fitConfigurationHeight() {
  const QStringList lines = m_configuration->toPlainText().split(QLatin1Char('\n'));
  const QFontMetrics metrics = m_configuration->fontMetrics();
  const int margin = int(m_configuration->document()->documentMargin());
  int widest = 0;
  for (const QString& line : lines) widest = qMax(widest, metrics.horizontalAdvance(line));
  const int bar = widest + 2 * margin > m_configuration->viewport()->width()
      ? m_configuration->horizontalScrollBar()->sizeHint().height()
      : 0;
  const int content = int(lines.size()) * metrics.lineSpacing() + 2 * margin + 2 * m_configuration->frameWidth() + 2 + bar;
  m_configuration->setFixedHeight(qMin(content, kConfigurationHeight));
}

// The configuration and command show the bridge with the platform's
// separators (JSON and TOML escape them as their syntax needs).
QString ConnectAssistantDialog::manualConfiguration() const {
  const QString bridge = QDir::toNativeSeparators(m_bridge);
  return client() == AssistantClient::Codex
      ? codexToml(bridge)
      : QString::fromUtf8(QJsonDocument(assistantJsonConfiguration(bridge)).toJson(QJsonDocument::Indented));
}

// The registration command (Claude Code, Codex) or the bridge's own launch;
// empty for a Codex settings file the CLI cannot address.
QString ConnectAssistantDialog::manualCommand() const {
  const AssistantClient selected = client();
  if (selected == AssistantClient::Codex && !standardCodexFile(configPath())) return {};
  const QString bridge = QDir::toNativeSeparators(m_bridge);
  const bool registration = selected == AssistantClient::ClaudeCode || selected == AssistantClient::Codex;
  const QString program = registration
      ? assistantProgram(selected == AssistantClient::ClaudeCode ? QStringLiteral("claude") : QStringLiteral("codex"))
      : bridge;
  const QStringList arguments = registration ? assistantRegistration(selected, bridge)
      : QStringList{QStringLiteral("mcp"), QStringLiteral("--connect")};
  const QString command = assistantCommand(program, arguments, windows);
  return selected == AssistantClient::Codex ? codexHomeCommand(command, configPath()) : command;
}

bool ConnectAssistantDialog::cachedCheck(SettingsRead& cache, const QString& path,
                                         const std::function<bool(const QString&)>& check, qint64 limit) const {
  const QFileInfo info(path);
  const qint64 bytes = info.exists() ? info.size() : -1;
  const QDateTime written = info.exists() ? info.lastModified() : QDateTime();
  if (cache.path != path || cache.size != bytes || cache.modified != written) {
    QString text, error;
    cache.path = path;
    cache.size = bytes;
    cache.modified = written;
    cache.configured = readAssistantConfiguration(path, text, error, limit) && check(text);
  }
  return cache.configured;
}

bool ConnectAssistantDialog::installed(AssistantClient which) const {
  switch (which) {
  // An installer that opened proves Claude Desktop; one that no application
  // took disproves it.
  case AssistantClient::ClaudeDesktop: return m_bundleOpened || (!m_desktopNotFound && claudeDesktopInstalled());
  case AssistantClient::ClaudeCode: return !assistantExecutable(QStringLiteral("claude")).isEmpty();
  case AssistantClient::Codex: {
    // Without its CLI, Codex's settings folder is enough: setup then writes
    // the TOML itself.
    const QFileInfo settings(configPath());
    return !assistantExecutable(QStringLiteral("codex")).isEmpty() ||
        (settings.isAbsolute() && QFileInfo(settings.absolutePath()).isDir());
  }
  case AssistantClient::Other: return true;
  }
  return false;
}

bool ConnectAssistantDialog::configured(AssistantClient which) const {
  if (m_bridge.isEmpty()) return false;
  if (which == AssistantClient::ClaudeDesktop) return m_bundleOpened;
  if (which == AssistantClient::Codex) {
    return cachedCheck(m_codexRead, configPath(), [this](const QString& text) {
      try {
        return codexHasBridge(text, m_bridge);
      } catch (const std::exception&) {
        return false;
      }
    });
  }
  if (which != AssistantClient::ClaudeCode) return false;
  return cachedCheck(m_claudeRead, claudeUserConfiguration(), [this](const QString& text) {
    const auto entry = QJsonDocument::fromJson(text.toUtf8()).object().value(QStringLiteral("mcpServers")).toObject()
        .value(QStringLiteral("mitcad")).toObject();
    return claudeEntryStarts(entry, m_bridge);
  }, kClaudeConfigurationLimit);
}

// The connections of the chosen assistant: what Disconnect ends.
QStringList ConnectAssistantDialog::chosenClientIds() const {
  QStringList ids;
  for (const ConnectedAgent& agent : m_host.clients ? m_host.clients() : QList<ConnectedAgent>()) {
    if (assistantLabelMatches(client(), agent.name)) ids << agent.id;
  }
  return ids;
}

// One state, one line naming the next step, at most one primary action. The
// state and the action are also properties (assistantState,
// assistantAction) for the test driver.
void ConnectAssistantDialog::refreshConnection() {
  const AssistantClient selected = client();
  const QString name = assistantSentenceName(selected);
  // Connected is the chosen assistant's state only when one of the clients
  // is that assistant; the others are named quietly below. Each by the
  // assistant its label matches, else by its own label.
  QStringList mine, others;
  for (const ConnectedAgent& agent : m_host.clients ? m_host.clients() : QList<ConnectedAgent>()) {
    QStringList& names = assistantLabelMatches(selected, agent.name) ? mine : others;
    const QString shown = assistantDisplayName(agent.name);
    if (!names.contains(shown)) names << shown;
  }
  // Claude Desktop installed since its extension found no application.
  if (m_desktopNotFound && windows && bundleHandlerRegistered()) m_desktopNotFound = false;
  // The extension installed by hand and connected: its file's line is done.
  if (!mine.isEmpty() && !m_bundleSaved.isEmpty()) {
    m_bundleSaved.clear();
    clearMessage();
  }
  const bool busy = m_process != nullptr;
  for (QAbstractButton* button : m_clients->buttons()) button->setEnabled(!busy);
  m_config->setEnabled(!busy);
  for (QPushButton* copy : {m_copyConfiguration, m_copyCommand}) copy->setEnabled(!m_bridge.isEmpty());
  if (auto* button = findChild<QPushButton*>(QStringLiteral("assistantCopyWindowCommand"))) button->setEnabled(!m_bridge.isEmpty());
  updateCommand();
  QString state, text, action;
  bool anyway = false;
  if (m_bridge.isEmpty()) {
    state = QStringLiteral("failed");
    text = tr("MitCad's assistant connector was not found. Repair or reinstall MitCad.");
  } else if (busy) {
    state = QStringLiteral("busy");
    text = tr("Connecting %1…").arg(name);
    action = QStringLiteral("connect");
  } else if (selected == AssistantClient::ClaudeDesktop && m_desktopNotFound && mine.isEmpty()) {
    // No application took the extension: not a wait for an approval.
    state = QStringLiteral("failed");
    text = tr("Install Claude Desktop first");
    action = QStringLiteral("install");
  } else if (selected == AssistantClient::ClaudeDesktop && !m_bundleSaved.isEmpty() && mine.isEmpty()) {
    // Claude Desktop is here, but no application opens its extension (a
    // Microsoft Store install, mitcad#763): it is installed from Claude
    // Desktop's settings, from the file shown.
    state = QStringLiteral("install-extension");
    text = tr("Install the extension in Claude Desktop");
    action = QStringLiteral("reveal");
  } else if (m_result->property("setupError").toBool()) {
    state = QStringLiteral("failed");
    text = m_result->text();
    action = QStringLiteral("retry");
  } else if (!mine.isEmpty()) {
    state = QStringLiteral("connected");
    text = tr("✓ Connected: %1").arg(mine.join(QStringLiteral(", ")));
    action = QStringLiteral("disconnect");
  } else if (selected == AssistantClient::Other) {
    state = QStringLiteral("manual");
    text = m_copied ? tr("Copied. Paste it into your assistant's settings")
                    : tr("Copy the configuration into your assistant's settings");
    action = QStringLiteral("copy");
  } else if (selected == AssistantClient::ClaudeDesktop && !desktopExtensions) {
    state = QStringLiteral("unavailable");
    text = tr("Claude Desktop is not available for this system");
  } else if (!installed(selected)) {
    state = QStringLiteral("not-installed");
    text = selected == AssistantClient::Codex ? tr("ChatGPT or Codex was not found on this computer")
                                              : tr("%1 was not found on this computer").arg(name);
    action = QStringLiteral("install");
    // Claude Code's setup runs its CLI: without one found it cannot work.
    anyway = selected != AssistantClient::ClaudeCode;
  } else if (configured(selected)) {
    // The next step is the assistant's; only Claude Desktop's installer can
    // be opened again from here.
    state = QStringLiteral("waiting");
    text = selected == AssistantClient::ClaudeDesktop ? tr("Approve MitCad in Claude Desktop, then start a new chat")
        : selected == AssistantClient::ClaudeCode ? tr("Start a new Claude Code session to use MitCad")
        : tr("Restart ChatGPT or start a new Codex session to use MitCad");
    if (selected == AssistantClient::ClaudeDesktop) action = QStringLiteral("reopen");
  } else {
    state = QStringLiteral("ready");
    text = tr("Ready to connect %1").arg(name);
    action = QStringLiteral("connect");
  }
  const bool failed = state == QStringLiteral("failed");
  m_state->setText(text);
  m_state->setToolTip(failed ? m_result->toolTip() : QString());
  m_state->setProperty("assistantState", state);
  // The choice and its state once each time they change (UI tests,
  // mitcad#760).
  const QString logged = QStringLiteral("Connect AI Assistant: %1, %2: %3").arg(assistantName(selected), state, text);
  if (logged != m_loggedState) {
    m_loggedState = logged;
    qInfo().noquote() << logged;
  }
  if (state != m_shownState) {
    // A failure's line is in the error colour.
    if (failed) setErrorStyleSheet(m_state);
    else if (m_shownState == QStringLiteral("failed")) setErrorStyleSheet(m_state, QString());
    m_shownState = state;
  }
  // An error is the state line itself; other results (a backup) show below it.
  m_result->setVisible(!m_result->text().isEmpty() && !failed);
  m_also->setText(tr("Also connected: %1").arg(others.join(QStringLiteral(", "))));
  m_also->setVisible(!others.isEmpty());
  m_setup->setProperty("assistantAction", action);
  m_setup->setVisible(!action.isEmpty());
  m_setup->setEnabled(!busy);
  // Disconnect ends the chosen assistant's connections only: named, by the
  // names Connected shows (an Other client's own label), when others stay
  // connected.
  const QString disconnectText = others.isEmpty() ? tr("Disconnect") : tr("Disconnect %1").arg(mine.join(QStringLiteral(", ")));
  m_setup->setText(action == QStringLiteral("install")
          ? (selected == AssistantClient::ClaudeDesktop ? tr("Get Claude Desktop") : tr("How to install"))
      : action == QStringLiteral("disconnect") ? disconnectText
      : action == QStringLiteral("retry") ? tr("Try again")
      : action == QStringLiteral("copy") ? tr("Copy configuration")
      : action == QStringLiteral("reopen") ? tr("Open in Claude Desktop again")
      : action == QStringLiteral("reveal") ? showFileText()
      : tr("Connect"));
  // The extension's file in the tooltip as well (also shown below the line).
  m_setup->setToolTip(action == QStringLiteral("reveal") ? QDir::toNativeSeparators(m_bundleSaved) : QString());
  // Check again goes with How to install (Claude Code, Codex).
  const bool checkAgain = state == QStringLiteral("not-installed") && selected != AssistantClient::ClaudeDesktop;
  m_checkAgain->setVisible(checkAgain);
  m_connectAnyway->setVisible(anyway);
  m_connectAnyway->setEnabled(!busy);
  const bool openDesktop = action == QStringLiteral("reveal");
  m_openDesktop->setVisible(openDesktop);
  // The folder line goes with the actions that allow it.
  const bool folderLine = anyway || action == QStringLiteral("connect") || action == QStringLiteral("copy") ||
                          action == QStringLiteral("retry") || action == QStringLiteral("reopen");
  m_folderRow->setVisible(folderLine);
  // A row shown or hidden, a longer line or another configuration: the
  // window fits its contents again.
  const QString shown = state + text + m_setup->text() + others.join(QLatin1Char(',')) +
      QString::number(int(m_result->isVisibleTo(this))) + QString::number(int(folderLine)) + QString::number(int(anyway)) +
      QString::number(int(openDesktop)) + QString::number(int(checkAgain)) +
      QString::number(int(m_advanced->isVisibleTo(this))) + QString::number(int(m_configRow->isVisibleTo(this))) +
      QString::number(int(m_commandBox->isVisibleTo(this))) + QString::number(int(m_windowsRow->isVisibleTo(this))) +
      QString::number(m_configuration->maximumHeight());
  if (shown != m_shownLayout) {
    m_shownLayout = shown;
    refit();
  }
}

void ConnectAssistantDialog::refit() {
  // Every inner layout first, innermost first, then the dialog's: a row
  // shown inside Advanced or the configuration's new height (inside its
  // labelled field) reaches the dialog's hints, and its minimum height, only
  // once each layout around it has been activated again, which a posted
  // request may not have done yet.
  QCoreApplication::sendPostedEvents(nullptr, QEvent::LayoutRequest);
  const QList<QWidget*> widgets = findChildren<QWidget*>();
  for (auto widget = widgets.crbegin(); widget != widgets.crend(); ++widget) {
    if (QLayout* inner = (*widget)->layout()) {
      inner->invalidate();
      inner->activate();
    }
  }
  layout()->invalidate();
  layout()->activate();
  // Grow to what the contents need (the width never jumps back), then give
  // back the height a hidden row or a shorter line left empty.
  fitWindowToContents(this, FitMode::GrowOnly);
  QLayout* rows = layout();
  const int fitted = rows->hasHeightForWidth() ? rows->totalHeightForWidth(width()) : sizeHint().height();
  const int fittedHeight = qMax(fitted, minimumSizeHint().height());
  if (fittedHeight < height()) resize(width(), fittedHeight);
}

void ConnectAssistantDialog::refreshWindows() {
  const QList<Endpoint> endpoints = discover();
  m_ownInstance.clear();
  for (const Endpoint& endpoint : endpoints) {
    if (endpoint.pid == QCoreApplication::applicationPid()) m_ownInstance = endpoint.instance;
  }
  m_windows->setText(tr("%n MitCad windows are open", nullptr, int(endpoints.size())));
  m_windowsRow->setVisible(endpoints.size() > 1 && !m_ownInstance.isEmpty());
}

void ConnectAssistantDialog::primaryAction() {
  const QString action = m_setup->property("assistantAction").toString();
  if (action == QStringLiteral("install")) {
    QDesktopServices::openUrl(QUrl(installPage(client())));
  } else if (action == QStringLiteral("disconnect")) {
    if (m_host.disconnect) m_host.disconnect(chosenClientIds());
    refreshConnection();
  } else if (action == QStringLiteral("reveal")) {
    // An isolated test run records the file instead of opening a window.
    if (!isolatedTestWork().isEmpty()) setProperty("assistantShownPath", m_bundleSaved);
    else showFile(m_bundleSaved);
    qInfo().noquote() << QStringLiteral("Assistant setup: extension shown");
  } else if (!action.isEmpty()) {
    connectAssistant(); // connect, copy, retry, reopen
  }
}

// Claude Desktop through the `claude://` link its installations register
// (the Microsoft Store package declares only that one). Without its
// application (Windows may offer to find an app for the link and report
// success), or when the link does not open, the state becomes Install
// Claude Desktop first. An isolated test run opens nothing: the link opens
// there while its stand-in application is found.
void ConnectAssistantDialog::openClaudeDesktop() {
  const bool test = !isolatedTestWork().isEmpty();
  if (!claudeDesktopApplicationFound() || (!test && !QDesktopServices::openUrl(QUrl(QStringLiteral("claude://"))))) {
    qInfo().noquote() << QStringLiteral("Assistant setup: Claude Desktop could not be opened");
    m_desktopNotFound = true;
    m_bundleSaved.clear();
    message(QString());
    return;
  }
  if (test) setProperty("assistantOpenedDesktop", true);
  qInfo().noquote() << QStringLiteral("Assistant setup: Claude Desktop opened");
}

void ConnectAssistantDialog::connectAssistant() {
  if (m_process) return;
  clearMessage();
  // Pressing the button with the folder line shown is the consent.
  if (!grantFolder()) return;
  if (client() == AssistantClient::Other) {
    QApplication::clipboard()->setText(manualConfiguration());
    m_copied = true;
    refreshConnection();
  } else {
    setup();
  }
}

void ConnectAssistantDialog::setup() {
  if (m_process || m_bridge.isEmpty()) return;
  if (client() == AssistantClient::Codex) setupCodex();
  else if (client() == AssistantClient::ClaudeCode) setupClaudeCode();
  else if (client() == AssistantClient::ClaudeDesktop) setupBundle();
}

void ConnectAssistantDialog::setupCodex() {
  const QString path = QFileInfo(configPath()).absoluteFilePath();
  if (const QString policy = assistantConfigurationPolicy(path); !policy.isEmpty()) { message(policy, true); return; }
  QString current, error;
  if (!readAssistantConfiguration(path, current, error)) { message(error, true); return; }
  QJsonObject plan;
  try {
    // Already there, as setup writes it or as the manual configuration shows it.
    if (codexHasBridge(current, m_bridge)) { message(QString()); return; }
    plan = codexPlan(current, m_bridge);
    if (plan.value(QStringLiteral("status")).toString() == QStringLiteral("conflict") &&
        confirmReplacement(this, assistantSentenceName(AssistantClient::Codex))) {
      plan = codexPlan(current, m_bridge, true);
    }
  } catch (const std::exception& failure) {
    message(tr("Codex's settings file could not be read. Fix it, then try again."), true, errorText(failure)); return;
  }
  const QString status = plan.value(QStringLiteral("status")).toString();
  if (status == QStringLiteral("unchanged")) { message(QString()); return; }
  if (status != QStringLiteral("updated")) {
    message(plan.value(QStringLiteral("error")).toString(tr("Codex's settings were not changed.")), true); return;
  }
  const auto backedUp = [](const QString& backup) {
    return backup.isEmpty() ? QString() : tr("Previous settings saved as %1").arg(QDir::toNativeSeparators(backup));
  };
  QString cli = assistantExecutable(QStringLiteral("codex"));
  // A selected nonstandard TOML file cannot be addressed by the CLI's home
  // argument. The configuration action remains available without a CLI.
  if (!standardCodexFile(path)) cli.clear();
  if (!cli.isEmpty()) {
    // Let the supported CLI operate on a private copy. A faulty client or a
    // concurrent edit of the real config cannot overwrite unrelated settings.
    const QString work = !qEnvironmentVariableIsEmpty("MITCAD_TEST_INPUT") ? qEnvironmentVariable("MITCAD_ASSISTANT_TEST_WORK") : QString();
    auto staged = std::make_shared<QTemporaryDir>(QDir(work.isEmpty() ? QDir::tempPath() : work)
        .filePath(QStringLiteral("mitcad-codex-XXXXXX")));
    if (!staged->isValid()) { message(tr("A private setup folder could not be made. Try again."), true); return; }
    const QString stagedPath = QDir(staged->path()).filePath(QStringLiteral("config.toml"));
    QSaveFile seed(stagedPath);
    const QByteArray initial = current.toUtf8();
    if (!seed.open(QIODevice::WriteOnly)) { message(tr("A private copy of the settings could not be made. Try again."), true); return; }
    seed.setPermissions(QFileDevice::ReadOwner | QFileDevice::WriteOwner);
    if (seed.write(initial) != initial.size() || !seed.commit()) {
      message(tr("A private copy of the settings could not be made. Try again."), true); return;
    }
    auto* process = new QProcess(this);
    auto environment = QProcessEnvironment::systemEnvironment();
    environment.insert(QStringLiteral("CODEX_HOME"), staged->path());
    process->setProcessEnvironment(environment);
    m_process = process;
    runRegistration(cli, assistantRegistration(AssistantClient::Codex, m_bridge), [this, path, current, staged, backedUp] {
      QString written, error;
      bool valid = readAssistantConfiguration(QDir(staged->path()).filePath(QStringLiteral("config.toml")), written, error);
      if (valid) {
        try {
          const auto verified = parseObject(verify_codex_mcp_config(rustStr(current.toUtf8()), rustStr(written.toUtf8()), rustStr(m_bridge.toUtf8())));
          valid = verified.value(QStringLiteral("valid")).toBool();
          error = verified.value(QStringLiteral("error")).toString();
        } catch (const std::exception& failure) { valid = false; error = errorText(failure); }
      }
      if (valid) {
        QString backup;
        if (!writeAssistantConfiguration(path, current, written, backup, error)) { message(error, true); return; }
        message(backedUp(backup));
      } else {
        message(tr("Codex's registration could not be verified; nothing was changed. Try again."), true, error);
      }
    });
    return;
  }
  QString backup;
  if (!writeAssistantConfiguration(path, current, plan.value(QStringLiteral("content")).toString(), backup, error)) {
    message(error, true); return;
  }
  message(backedUp(backup));
}

void ConnectAssistantDialog::setupClaudeCode() {
  const QString cli = assistantExecutable(QStringLiteral("claude"));
  if (cli.isEmpty()) { message(tr("Claude Code was not found on this computer"), true); return; }
  const QString path = claudeUserConfiguration();
  QString current, error;
  if (!readAssistantConfiguration(path, current, error, kClaudeConfigurationLimit)) { message(error, true); return; }
  QJsonParseError parse;
  const auto document = QJsonDocument::fromJson(current.toUtf8(), &parse);
  if (!current.isEmpty() && (parse.error != QJsonParseError::NoError || !document.isObject())) {
    message(tr("Claude Code's settings file is not valid JSON. Fix it, then try again."), true); return;
  }
  const QJsonObject original = document.isObject() ? document.object() : QJsonObject{};
  const auto existing = original.value(QStringLiteral("mcpServers"));
  if (!existing.isUndefined() && !existing.isObject()) {
    message(tr("Claude Code's settings file has an unexpected format. Fix it, then try again."), true); return;
  }
  QJsonObject servers = existing.toObject();
  const auto entry = servers.value(QStringLiteral("mitcad"));
  if (!entry.isUndefined()) {
    if (claudeEntryStarts(entry.toObject(), m_bridge)) { message(QString()); return; }
    if (!confirmReplacement(this, assistantSentenceName(AssistantClient::ClaudeCode))) return;
  }
  // CLAUDE_CONFIG_DIR places user-scope .claude.json inside this private
  // directory. Run the supported CLI there, never remove the real entry.
  const QString work = !qEnvironmentVariableIsEmpty("MITCAD_TEST_INPUT") ? qEnvironmentVariable("MITCAD_ASSISTANT_TEST_WORK") : QString();
  auto staged = std::make_shared<QTemporaryDir>(QDir(work.isEmpty() ? QDir::tempPath() : work)
      .filePath(QStringLiteral("mitcad-claude-XXXXXX")));
  if (!staged->isValid()) { message(tr("A private setup folder could not be made. Try again."), true); return; }
  QJsonObject seed = original;
  servers.remove(QStringLiteral("mitcad"));
  seed.insert(QStringLiteral("mcpServers"), servers);
  QSaveFile file(QDir(staged->path()).filePath(QStringLiteral(".claude.json")));
  const QByteArray initial = QJsonDocument(seed).toJson();
  if (!file.open(QIODevice::WriteOnly)) { message(tr("A private copy of the settings could not be made. Try again."), true); return; }
  file.setPermissions(QFileDevice::ReadOwner | QFileDevice::WriteOwner);
  if (file.write(initial) != initial.size() || !file.commit()) {
    message(tr("A private copy of the settings could not be made. Try again."), true); return;
  }
  auto* process = new QProcess(this);
  auto environment = QProcessEnvironment::systemEnvironment();
  environment.insert(QStringLiteral("CLAUDE_CONFIG_DIR"), staged->path());
  process->setProcessEnvironment(environment);
  m_process = process;
  runRegistration(cli, assistantRegistration(AssistantClient::ClaudeCode, m_bridge), [this, path, current, seed, staged] {
    QString written, error;
    if (!readAssistantConfiguration(QDir(staged->path()).filePath(QStringLiteral(".claude.json")), written, error,
                                    kClaudeConfigurationLimit)) {
      message(error, true); return;
    }
    QJsonParseError parse;
    const auto document = QJsonDocument::fromJson(written.toUtf8(), &parse);
    QJsonObject result = document.object();
    QJsonObject servers = result.value(QStringLiteral("mcpServers")).toObject();
    const auto entry = servers.take(QStringLiteral("mitcad")).toObject();
    result.insert(QStringLiteral("mcpServers"), servers);
    if (parse.error != QJsonParseError::NoError || !document.isObject() || result != seed ||
        entry.value(QStringLiteral("command")) != m_bridge ||
        entry.value(QStringLiteral("args")) != QJsonArray{QStringLiteral("mcp"), QStringLiteral("--connect")} ||
        (entry.contains(QStringLiteral("type")) && entry.value(QStringLiteral("type")) != QStringLiteral("stdio"))) {
      message(tr("Claude Code's registration could not be verified; nothing was changed. Try again."), true); return;
    }
    QString backup;
    if (!writeAssistantConfiguration(path, current, written, backup, error, kClaudeConfigurationLimit)) {
      message(error, true); return;
    }
    message(backup.isEmpty() ? QString() : tr("Previous settings saved as %1").arg(QDir::toNativeSeparators(backup)));
  });
}

void ConnectAssistantDialog::runRegistration(const QString& program, const QStringList& arguments,
                                             std::function<void()> success) {
  QProcess* process = m_process ? m_process : new QProcess(this);
  m_process = process;
  refreshConnection();
  process->setWorkingDirectory(QDir::homePath());
  auto* timeout = new QTimer(process); timeout->setSingleShot(true); timeout->setInterval(30000);
  connect(timeout, &QTimer::timeout, process, [process] { process->kill(); });
  connect(process, &QProcess::errorOccurred, this, [this, process](QProcess::ProcessError error) {
    if (error != QProcess::FailedToStart) return;
    m_process = nullptr; process->deleteLater();
    message(tr("%1 could not be started. Check its installation, then try again.").arg(assistantSentenceName(client())), true);
  });
  connect(process, qOverload<int, QProcess::ExitStatus>(&QProcess::finished), this,
      [this, process, timeout, success = std::move(success)](int code, QProcess::ExitStatus exit) {
        timeout->stop();
        m_process = nullptr;
        if (code == 0 && exit == QProcess::NormalExit) success();
        else message(tr("%1 did not accept the connection. Try again, or set it up by hand under Advanced.")
            .arg(assistantSentenceName(client())), true, QString::fromUtf8(process->readAllStandardError()).left(2000));
        process->deleteLater(); refreshConnection();
      });
  process->start(program, arguments, QIODevice::ReadOnly); timeout->start();
}

void ConnectAssistantDialog::setupBundle() {
  m_bundleSaved.clear();
  m_desktopNotFound = false;
  const QString suffix = windows ? QStringLiteral(".exe") : QString();
  const QString launcher = QDir(QCoreApplication::applicationDirPath()).filePath(QStringLiteral("mitcad-mcp-launcher") + suffix);
  if (!QFileInfo(launcher).isExecutable()) { message(tr("Part of MitCad is missing. Repair or reinstall MitCad."), true, launcher); return; }
  const QString platform = windows ? QStringLiteral("win32") : QStringLiteral("darwin");
  const QByteArray manifest = QJsonDocument(assistantBundleManifest(m_bridge, windows, platform)).toJson(QJsonDocument::Indented);
  try {
    const auto bytes = build_mcp_bundle(rustStr(manifest), rustStr(launcher.toUtf8()));
    const QString work = !qEnvironmentVariableIsEmpty("MITCAD_TEST_INPUT") ? qEnvironmentVariable("MITCAD_ASSISTANT_TEST_WORK") : QString();
    const QString bundleFolder = QDir(work.isEmpty() ? QStandardPaths::writableLocation(QStandardPaths::AppLocalDataLocation) : work)
        .filePath(QStringLiteral("assistant-setup"));
    if (!QDir().mkpath(bundleFolder)) { message(tr("The Claude Desktop extension could not be saved. Try again."), true, bundleFolder); return; }
    const QString path = QDir(bundleFolder).filePath(QStringLiteral("mitcad-desktop.mcpb"));
    QSaveFile file(path);
    if (!file.open(QIODevice::WriteOnly) ||
        file.write(reinterpret_cast<const char*>(bytes.data()), qsizetype(bytes.size())) != qsizetype(bytes.size()) || !file.commit()) {
      message(tr("The Claude Desktop extension could not be saved. Try again."), true, path); return;
    }
    if (!work.isEmpty() && qEnvironmentVariable("MITCAD_TEST_ASSISTANT_BUNDLE_OPEN") == QStringLiteral("capture")) {
      setProperty("assistantBundlePath", path);
      m_bundleOpened = true;
      message(tr("Bundle prepared for the isolated installer test.")); return;
    }
    // No application takes the extension (Windows would offer "Open with"
    // and report success).
    if (!bundleHandlerRegistered()) { bundleNotOpened(path); return; }
    if (!QDesktopServices::openUrl(QUrl::fromLocalFile(path))) {
#ifdef Q_OS_WIN
      // The extension has an application (above), which did not open it.
      message(tr("Claude Desktop did not open the extension. Try again."), true, path); return;
#else
      bundleNotOpened(path); return;
#endif
    }
    // Whether it was installed is known only once Claude Desktop connects.
    m_bundleOpened = true;
    message(QString());
  } catch (const std::exception& failure) {
    message(tr("The Claude Desktop extension could not be made. Try again."), true, errorText(failure));
  }
}

// No application opened the saved extension: Claude Desktop is not there,
// or, its application found here (not just its settings folder), it takes
// the extension from its own settings only (a Microsoft Store install
// registers no `.mcpb` association, mitcad#763); the line below the state
// then says where in Claude Desktop and which file.
void ConnectAssistantDialog::bundleNotOpened(const QString& path) {
  if (claudeDesktopApplicationFound()) {
    m_bundleSaved = path;
    message(tr("In Claude Desktop: Settings › Extensions › Advanced settings › Install Extension…, then choose %1")
                .arg(QDir::toNativeSeparators(path)));
  } else {
    m_desktopNotFound = true;
    message(QString());
  }
}

bool ConnectAssistantDialog::grantFolder() {
  if (m_folderPath.isEmpty()) return true;
  const QFileInfo chosen(m_folderPath);
  if (!chosen.isAbsolute() || !QDir().mkpath(chosen.absoluteFilePath()) || !QFileInfo(chosen.absoluteFilePath()).isDir()) {
    message(tr("The folder %1 cannot be used. Choose another.").arg(QDir::toNativeSeparators(m_folderPath)), true);
    return false;
  }
  QStringList folders = savedAgentFolders(); folders << chosen.canonicalFilePath();
  saveAgentFolders(folders);
  for (QWidget* window : QApplication::topLevelWidgets()) {
    for (auto* preferences : window->findChildren<AgentPreferencesBox*>()) preferences->includeSavedFolders();
  }
  qInfo().noquote() << QStringLiteral("Assistant setup: folder allowed");
  return true;
}

bool claudeDesktopOnComputer() { return claudeDesktopApplicationFound(); }

} // namespace mitcad::mcp
