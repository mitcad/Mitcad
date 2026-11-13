// SPDX-License-Identifier: MIT
#include "AgentFolders.hpp"

#include <QDir>
#include <QFileInfo>
#include <QHBoxLayout>
#include <QLabel>
#include <QListWidget>
#include <QPushButton>
#include <QSettings>
#include <QShowEvent>
#include <QSignalBlocker>
#include <QTimer>
#include <QVBoxLayout>
#include <QtLogging>

#include <utility>

#include "../framework/Dialogs.hpp"
#include "../platform/MacDialogs.hpp"

namespace mitcad::mcp {
namespace {

const QString kSetting = QStringLiteral("mcp/folders");

QStringList& runFolders() {
  static QStringList folders;
  return folders;
}

// Absolute and cleaned, with '/' separators.
QString cleanFolder(const QString& folder) {
  return QDir::cleanPath(QFileInfo(QDir::fromNativeSeparators(folder.trimmed())).absoluteFilePath());
}

void addClean(QStringList& list, const QString& folder) {
  if (folder.trimmed().isEmpty()) {
    return;
  }
  const QString clean = cleanFolder(folder);
  if (!list.contains(clean)) {
    list << clean;
  }
}

// `path` is `folder` or below it (both resolved).
bool inside(const QString& path, const QString& folder) {
#ifdef Q_OS_WIN
  constexpr Qt::CaseSensitivity kCase = Qt::CaseInsensitive;
#else
  constexpr Qt::CaseSensitivity kCase = Qt::CaseSensitive;
#endif
  if (folder.isEmpty()) {
    return false;
  }
  const QString prefix = folder.endsWith(QLatin1Char('/')) ? folder : folder + QLatin1Char('/');
  return path.compare(folder, kCase) == 0 || path.startsWith(prefix, kCase);
}

bool insideAny(const QString& path, const QStringList& folders) {
  for (const QString& folder : folders) {
    if (inside(path, QFileInfo(folder).canonicalFilePath())) {
      return true;
    }
  }
  return false;
}

} // namespace

QStringList savedAgentFolders() {
  QStringList folders;
  for (const QString& folder : QSettings().value(kSetting).toStringList()) {
    addClean(folders, folder);
  }
  return folders;
}

void saveAgentFolders(const QStringList& folders) {
  QStringList clean;
  for (const QString& folder : folders) {
    addClean(clean, folder);
  }
  QSettings().setValue(kSetting, clean);
}

void setRunAgentFolders(const QStringList& folders) {
  runFolders().clear();
  for (const QString& folder : folders) {
    addClean(runFolders(), folder);
  }
}

bool runAgentFoldersGiven() {
  return !runFolders().isEmpty();
}

QStringList agentFolders() {
  QStringList folders = savedAgentFolders();
  for (const QString& folder : std::as_const(runFolders())) {
    addClean(folders, folder);
  }
  return folders;
}

namespace {

// A file in an allowed folder (agentDesignPath, agentOutputPath): one of
// `suffixes` (any when empty; `design` says it must be a MitCad design).
// The reasons here and below go to the agent as tool errors, not to the
// user: they stay English in every interface language (mitcad#1471).
QString agentPath(const QString& given, const QStringList& suffixes, bool design, bool forWriting, bool overwrite,
                  QString& reason) {
  reason.clear();
  const QStringList folders = agentFolders();
  if (folders.isEmpty()) {
    // The window as the platform names it (tools.preferences); the
    // command-line option is in docs/mcp.md only.
#ifdef Q_OS_MACOS
    reason = QStringLiteral("No folder for designs is allowed: add one in Settings › AI Assistants.");
#else
    reason = QStringLiteral("No folder for designs is allowed: add one in Preferences › AI Assistants.");
#endif
    return QString();
  }
  const QString text = QDir::fromNativeSeparators(given.trimmed());
  if (text.isEmpty() || text.split(QLatin1Char('/')).contains(QStringLiteral(".."))) {
    reason = QStringLiteral("Paths must be nonempty and cannot contain '..'.");
    return QString();
  }
  const QFileInfo info(QDir::cleanPath(QDir::isAbsolutePath(text) ? text : QDir(folders.first()).filePath(text)));
  if (!suffixes.isEmpty() && !suffixes.contains(info.suffix(), Qt::CaseInsensitive)) {
    reason = design ? QStringLiteral("%1 is not a MitCad design: the path must end in .mitcad.")
                          .arg(QDir::toNativeSeparators(text))
                    : QStringLiteral("%1 must end in .%2.")
                          .arg(QDir::toNativeSeparators(text), suffixes.join(QStringLiteral(" or .")));
    return QString();
  }
  const QString folder = QFileInfo(info.absolutePath()).canonicalFilePath();
  if (folder.isEmpty() || !QFileInfo(folder).isDir()) {
    reason = QStringLiteral("The folder %1 does not exist.").arg(QDir::toNativeSeparators(info.absolutePath()));
    return QString();
  }
  const QString outside = QStringLiteral("%1 is not in a folder for designs (%2).")
                              .arg(QDir::toNativeSeparators(info.absoluteFilePath()),
                                   QDir::toNativeSeparators(folders.join(QStringLiteral(", "))));
  if (!insideAny(folder, folders)) {
    reason = outside;
    return QString();
  }
  const QString path = QDir(folder).filePath(info.fileName());
  const QFileInfo target(path);
  if (forWriting) {
    if (target.exists() || target.isSymLink()) {
      if (target.isSymLink() || !target.isFile()) {
        reason = QStringLiteral("%1 is not a regular file.").arg(QDir::toNativeSeparators(path));
        return QString();
      }
      if (!overwrite) {
        reason = QStringLiteral("%1 already exists: pass overwrite: true to replace it.").arg(QDir::toNativeSeparators(path));
        return QString();
      }
    }
    return path;
  }
  // Reading: a link is followed, and where it leads must be allowed too.
  const QString resolved = target.canonicalFilePath();
  if (resolved.isEmpty() || !QFileInfo(resolved).isFile()) {
    reason = QStringLiteral("%1 does not exist.").arg(QDir::toNativeSeparators(path));
    return QString();
  }
  if (!insideAny(QFileInfo(resolved).absolutePath(), folders)) {
    reason = outside;
    return QString();
  }
  return resolved;
}

} // namespace

QString agentDesignPath(const QString& given, bool forWriting, bool overwrite, QString& reason) {
  return agentPath(given, {QStringLiteral("mitcad")}, true, forWriting, overwrite, reason);
}

QString agentOutputPath(const QString& given, bool overwrite, QString& reason, const QStringList& suffixes) {
  return agentPath(given, suffixes, false, true, overwrite, reason);
}

// document_new's folder and file (mitcad#1266).
QString agentDesignFolder(const QString& given, QString& reason) {
  reason.clear();
  const QStringList folders = agentFolders();
  if (folders.isEmpty()) {
#ifdef Q_OS_MACOS
    reason = QStringLiteral("No folder for designs is allowed: add one in Settings › AI Assistants.");
#else
    reason = QStringLiteral("No folder for designs is allowed: add one in Preferences › AI Assistants.");
#endif
    return QString();
  }
  const QString text = QDir::fromNativeSeparators(given.trimmed());
  if (text.isEmpty() || text.split(QLatin1Char('/')).contains(QStringLiteral(".."))) {
    reason = QStringLiteral("Paths must be nonempty and cannot contain '..'.");
    return QString();
  }
  const QString absolute = QDir::cleanPath(QDir::isAbsolutePath(text) ? text : QDir(folders.first()).filePath(text));
  const QString folder = QFileInfo(absolute).canonicalFilePath();
  if (folder.isEmpty() || !QFileInfo(folder).isDir()) {
    reason = QStringLiteral("The folder %1 does not exist.").arg(QDir::toNativeSeparators(absolute));
    return QString();
  }
  if (!insideAny(folder, folders)) {
    reason = QStringLiteral("%1 is not in a folder for designs (%2).")
                 .arg(QDir::toNativeSeparators(absolute), QDir::toNativeSeparators(folders.join(QStringLiteral(", "))));
    return QString();
  }
  return folder;
}

QString agentNewDesignPath(const QString& folder, const QString& name, QString& reason) {
  reason.clear();
  const QDir dir(folder);
  // Names compared without case, as on Windows and macOS and as the
  // Projects panel picks its free DesignN.
  QStringList taken;
  for (const QString& entry : dir.entryList(QDir::AllEntries | QDir::Hidden | QDir::System | QDir::NoDotAndDotDot)) {
    taken << entry.toCaseFolded();
  }
  QString file = name.trimmed();
  if (file.isEmpty()) {
    int n = 1;
    while (taken.contains(QStringLiteral("design%1.mitcad").arg(n)) || taken.contains(QStringLiteral("design%1").arg(n))) {
      ++n;
    }
    return dir.filePath(QStringLiteral("Design%1.mitcad").arg(n));
  }
  bool plain = file != QStringLiteral(".") && file != QStringLiteral("..");
  for (const QChar character : std::as_const(file)) {
    if (character.unicode() < 32 || QStringLiteral("/\\:*?\"<>|").contains(character)) {
      plain = false;
    }
  }
  if (!file.endsWith(QStringLiteral(".mitcad"), Qt::CaseInsensitive)) {
    file += QStringLiteral(".mitcad");
  }
  if (!plain || QFileInfo(file).completeBaseName().trimmed().isEmpty()) {
    reason = QStringLiteral("%1 is not a design's name: give a file name without a folder (folder takes the folder).")
                 .arg(name.trimmed());
    return QString();
  }
  const QString path = dir.filePath(file);
  if (taken.contains(file.toCaseFolded()) || QFileInfo::exists(path) || QFileInfo(path).isSymLink()) {
    reason = QStringLiteral("%1 already exists in %2: choose another name, or open it with document_open.")
                 .arg(file, QDir::toNativeSeparators(folder));
    return QString();
  }
  return path;
}

AgentPreferencesBox::AgentPreferencesBox(QWidget* parent)
#ifdef MITCAD_MAC_UI
    : QWidget(parent)
#else
    : QGroupBox(tr("AI Assistants"), parent)
#endif
{
  auto* layout = new QVBoxLayout(this);
  m_connect = new QPushButton(tr("Connect AI Assistant…"));
  m_connect->setObjectName(QStringLiteral("connectAssistant"));
  m_connect->setAutoDefault(false);
  connect(m_connect, &QPushButton::clicked, this, &AgentPreferencesBox::connectRequested);
  // Its natural width, as the other pages' buttons.
  layout->addWidget(m_connect, 0, Qt::AlignLeft);
  // The list's name, as the Connect AI Assistant dialog's folder line.
  auto* title = new QLabel(tr("Folders for designs"));
  layout->addWidget(title);
  m_folders = new QListWidget;
  m_folders->setObjectName(QStringLiteral("agentFolders"));
  // Named also while it is empty.
  m_folders->setAccessibleName(tr("Folders for designs"));
  title->setBuddy(m_folders);
  m_folders->setSelectionMode(QAbstractItemView::ExtendedSelection);
  m_folders->setMinimumHeight(90);
  for (const QString& folder : savedAgentFolders()) {
    m_folders->addItem(QDir::toNativeSeparators(folder));
  }
  // The list takes the page's height.
  layout->addWidget(m_folders, 1);
  mac::stylePreview(m_folders);
  // Add… chooses a folder, Remove takes the selected ones off the list.
  auto* add = new QPushButton(tr("Add…"));
  add->setObjectName(QStringLiteral("agentFolderAdd"));
  add->setToolTip(tr("Chooses a folder to add"));
  m_remove = new QPushButton(tr("Remove"));
  m_remove->setToolTip(tr("Removes the selected folders from the list (the files stay)"));
  m_remove->setEnabled(false);
  for (QPushButton* button : {add, m_remove}) {
    button->setAutoDefault(false);
  }
  auto* controls = new QHBoxLayout;
#ifdef MITCAD_MAC_UI
  controls->setSpacing(0);
#endif
  controls->addWidget(add);
  controls->addWidget(m_remove);
  controls->addStretch(1);
  mac::styleListButtons(add, m_remove);
  layout->addLayout(controls);

  connect(add, &QPushButton::clicked, this, [this] {
    const QString folder = dialogDirectory(this, tr("Folder for Designs"));
    if (!folder.isEmpty()) {
      addFolder(folder);
    }
  });
  connect(m_folders, &QListWidget::itemSelectionChanged, this,
          [this] { m_remove->setEnabled(!m_folders->selectedItems().isEmpty()); });
  connect(m_remove, &QPushButton::clicked, this, [this] {
    qDeleteAll(m_folders->selectedItems());
    emit changed();
  });
}

void AgentPreferencesBox::showEvent(QShowEvent* event) {
  AgentPreferencesBase::showEvent(event);
  // Where Connect AI Assistant… is, in the main window's coordinates (UI
  // tests, mitcad#760).
  QTimer::singleShot(0, this, [this] {
    QWidget* main = window()->parentWidget() != nullptr ? window()->parentWidget()->window() : nullptr;
    if (main != nullptr && m_connect->isVisible()) {
      const QPoint at = m_connect->mapToGlobal(m_connect->rect().center()) - main->mapToGlobal(QPoint(0, 0));
      qDebug().noquote() << QStringLiteral("Preferences connect assistant at %1,%2").arg(at.x()).arg(at.y());
    }
  });
}

void AgentPreferencesBox::addFolder(const QString& typed) {
  if (typed.trimmed().isEmpty()) {
    return;
  }
  const QString folder = cleanFolder(typed);
  for (int i = 0; i < m_folders->count(); ++i) {
    if (QDir::fromNativeSeparators(m_folders->item(i)->text()) == folder) {
      return;
    }
  }
  auto* item = new QListWidgetItem(QDir::toNativeSeparators(folder), m_folders);
  if (!QFileInfo(folder).isDir()) {
    item->setToolTip(tr("The folder is not there now"));
  }
  emit changed();
}

void AgentPreferencesBox::includeSavedFolders() {
  const QSignalBlocker blocker(this);
  for (const QString& folder : savedAgentFolders()) addFolder(folder);
}

QStringList AgentPreferencesBox::chosen() const {
  QStringList folders;
  for (int i = 0; i < m_folders->count(); ++i) {
    addClean(folders, m_folders->item(i)->text());
  }
  return folders;
}

} // namespace mitcad::mcp
