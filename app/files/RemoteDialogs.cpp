// SPDX-License-Identifier: MIT
#include "RemoteDialogs.hpp"

#include <algorithm>

#include <QButtonGroup>
#include <QComboBox>
#include <QDateTime>
#include <QDesktopServices>
#include <QDialog>
#include <QDialogButtonBox>
#include <QDir>
#include <QFileDialog>
#include <QFileInfo>
#include <QFormLayout>
#include <QHBoxLayout>
#include <QHeaderView>
#include <QLabel>
#include <QLineEdit>
#include <QPushButton>
#include <QRadioButton>
#include <QTreeWidget>
#include <QUrl>
#include <QUrlQuery>
#include <QVBoxLayout>
#include <QtLogging>

#include "VersionDialogs.hpp"

namespace mitcad {
namespace {

QLabel* note(const QString& text) {
  auto* label = new QLabel(text);
  label->setWordWrap(true);
  return label;
}

QLabel* problemLabel() {
  auto* label = new QLabel;
  label->setStyleSheet(QStringLiteral("color: #b00020;"));
  label->setWordWrap(true);
  return label;
}

// The host and the path of an SSH or web address: [user@]host:path (git's
// short form for SSH) or scheme://[user@]host[:port]/path; nothing for a
// folder or a file:// address.
struct HostPath {
  QString host;
  QString path;
};

std::optional<HostPath> hostPath(const QString& url) {
  const QString text = url.trimmed();
  if (text.contains(QStringLiteral("://"))) {
    const QUrl parsed(text);
    const QString scheme = parsed.scheme().toLower();
    if (!parsed.isValid() || parsed.host().isEmpty() ||
        (scheme != QLatin1String("ssh") && scheme != QLatin1String("https") && scheme != QLatin1String("http"))) {
      return std::nullopt;
    }
    return HostPath{parsed.host().toLower(), parsed.path()};
  }
  // git@host:owner/repo.git; not C:\folder (a drive letter) nor a path
  // with a colon further on.
  const qsizetype colon = text.indexOf(QLatin1Char(':'));
  if (colon <= 1) {
    return std::nullopt;
  }
  const QString before = text.left(colon);
  if (before.contains(QLatin1Char('/')) || before.contains(QLatin1Char('\\'))) {
    return std::nullopt;
  }
  return HostPath{before.section(QLatin1Char('@'), -1).toLower(), text.mid(colon + 1)};
}

// A path without its slashes around it and its ".git".
QString repositoryPath(QString path) {
  path.replace(QLatin1Char('\\'), QLatin1Char('/'));
  while (path.startsWith(QLatin1Char('/'))) {
    path.remove(0, 1);
  }
  while (path.endsWith(QLatin1Char('/'))) {
    path.chop(1);
  }
  if (path.endsWith(QLatin1String(".git"))) {
    path.chop(4);
  }
  return path;
}

QString serviceName(RemoteService service) {
  switch (service) {
  case RemoteService::GitHub:
    return QStringLiteral("GitHub");
  case RemoteService::Forgejo:
    return QStringLiteral("Forgejo or Gitea");
  case RemoteService::GitLab:
    return QStringLiteral("GitLab");
  case RemoteService::Other:
    break;
  }
  return QObject::tr("Another git server or a folder");
}

// Whether the user has an SSH key (~/.ssh/id_*.pub): SSH addresses are
// suggested then, else web ones, which a credential helper signs in to.
bool hasSshKey() {
  const QDir ssh(QDir::home().filePath(QStringLiteral(".ssh")));
  return !ssh.entryList({QStringLiteral("id_*.pub")}, QDir::Files).isEmpty();
}

// The address's example for a service.
QString placeholder(RemoteService service) {
  const bool ssh = hasSshKey();
  const auto example = [ssh](const char* host) {
    return ssh ? QStringLiteral("git@%1:you/project.git").arg(QLatin1String(host))
               : QStringLiteral("https://%1/you/project.git").arg(QLatin1String(host));
  };
  switch (service) {
  case RemoteService::GitHub:
    return example("github.com");
  case RemoteService::Forgejo:
    return example("codeberg.org");
  case RemoteService::GitLab:
    return example("gitlab.com");
  case RemoteService::Other:
    break;
  }
  return QObject::tr("ssh://server/project.git, https://server/project.git or a folder");
}

// "yyyy-MM-dd HH:mm" of a version's time (seconds since 1970).
QString when(const QJsonValue& seconds) {
  return QDateTime::fromSecsSinceEpoch(seconds.toInteger()).toLocalTime().toString(QStringLiteral("yyyy-MM-dd HH:mm"));
}

// A side of a conflict: its latest version's summary, who and when.
QString sideText(const QJsonObject& side, bool withAuthor) {
  const QJsonObject version = side.value(QStringLiteral("version")).toObject();
  QString text;
  if (side.value(QStringLiteral("blob")).isNull()) {
    text = QObject::tr("deleted");
  }
  if (!version.isEmpty()) {
    const QString summary = version.value(QStringLiteral("summary")).toString();
    const QString who = version.value(QStringLiteral("author")).toObject().value(QStringLiteral("name")).toString();
    const QString at = when(version.value(QStringLiteral("time")));
    const QString detail = withAuthor ? QObject::tr("%1 (%2, %3)").arg(summary, who, at)
                                      : QObject::tr("%1 (%2)").arg(summary, at);
    text = text.isEmpty() ? detail : QObject::tr("%1: %2").arg(text, detail);
  }
  return text;
}

QString kindText(const QString& kind) {
  if (kind == QLatin1String("deleted_theirs")) {
    return QObject::tr("deleted on the remote, changed here");
  }
  if (kind == QLatin1String("deleted_mine")) {
    return QObject::tr("deleted here, changed on the remote");
  }
  if (kind == QLatin1String("added_both")) {
    return QObject::tr("added on both sides");
  }
  return QObject::tr("changed on both sides");
}

QString choiceText(const QString& choice) {
  if (choice == QLatin1String("mine")) {
    return QObject::tr("mine");
  }
  if (choice == QLatin1String("theirs")) {
    return QObject::tr("theirs");
  }
  return QObject::tr("mine as a copy");
}

// When a fetch, push or sync was last tried, and how it went.
QString attemptText(const QJsonValue& attempt) {
  const QJsonObject tried = attempt.toObject();
  if (tried.isEmpty()) {
    return QObject::tr("never");
  }
  const QString at = when(tried.value(QStringLiteral("time")));
  const QJsonObject error = tried.value(QStringLiteral("error")).toObject();
  return error.isEmpty() ? QObject::tr("%1, done").arg(at)
                         : QObject::tr("%1, failed: %2").arg(at, error.value(QStringLiteral("message")).toString());
}

} // namespace

std::optional<RemoteService> serviceOf(const QString& url) {
  const QString host = remoteHost(url);
  if (host == QLatin1String("github.com") || host.endsWith(QLatin1String(".github.com"))) {
    return RemoteService::GitHub;
  }
  if (host == QLatin1String("gitlab.com")) {
    return RemoteService::GitLab;
  }
  if (host == QLatin1String("codeberg.org")) {
    return RemoteService::Forgejo;
  }
  return std::nullopt;
}

QString remoteHost(const QString& url) {
  const std::optional<HostPath> parsed = hostPath(url);
  return parsed ? parsed->host : QString();
}

QString remoteWebPage(const QString& url) {
  const std::optional<HostPath> parsed = hostPath(url);
  if (!parsed) {
    return {};
  }
  const QString path = repositoryPath(parsed->path);
  if (path.isEmpty() || path.startsWith(QLatin1Char('~'))) {
    return {};
  }
  return QStringLiteral("https://%1/%2").arg(parsed->host, path);
}

QString repositoryName(const QString& url) {
  QString path = url.trimmed();
  if (const std::optional<HostPath> parsed = hostPath(path)) {
    path = parsed->path;
  } else if (path.startsWith(QLatin1String("file://"))) {
    path = QUrl(path).toLocalFile();
  }
  return repositoryPath(path).section(QLatin1Char('/'), -1);
}

QString newRepositoryPage(RemoteService service, const QString& url, const QString& name) {
  const QString host = remoteHost(url);
  switch (service) {
  case RemoteService::GitHub: {
    // GitHub's form takes the name and the visibility filled in.
    QUrl page(QStringLiteral("https://github.com/new"));
    QUrlQuery query;
    query.addQueryItem(QStringLiteral("name"), name);
    query.addQueryItem(QStringLiteral("visibility"), QStringLiteral("private"));
    page.setQuery(query);
    return page.toString();
  }
  case RemoteService::GitLab:
    return QStringLiteral("https://%1/projects/new").arg(host.isEmpty() ? QStringLiteral("gitlab.com") : host);
  case RemoteService::Forgejo:
    return host.isEmpty() ? QString() : QStringLiteral("https://%1/repo/create").arg(host);
  case RemoteService::Other:
    break;
  }
  return {};
}

QString sshKeyHelpPage(RemoteService service) {
  switch (service) {
  case RemoteService::GitHub:
    return QStringLiteral(
        "https://docs.github.com/en/authentication/connecting-to-github-with-ssh/adding-a-new-ssh-key-to-your-github-account");
  case RemoteService::GitLab:
    return QStringLiteral("https://docs.gitlab.com/user/ssh/");
  case RemoteService::Forgejo:
    return QStringLiteral("https://docs.codeberg.org/security/ssh-key/");
  case RemoteService::Other:
    break;
  }
  return QStringLiteral("https://git-scm.com/book/en/v2/Git-on-the-Server-Generating-Your-SSH-Public-Key");
}

std::optional<QString> askConnectRemote(QWidget* parent, const QString& project, const QString& author,
                                        const QString& url) {
  QDialog dialog(parent);
  dialog.setWindowTitle(QObject::tr("Connect Project to Remote"));
  auto* layout = new QVBoxLayout(&dialog);
  layout->addWidget(note(
      QObject::tr("The versions of the project %1 go to a remote repository, from where others open the "
                  "project and where each saved version is sent. Make an empty, private repository on the "
                  "service first (without a README), then give its address. Mitcad signs in as your other "
                  "git tools do, with an SSH key or a credential helper (Git Credential Manager); it never "
                  "asks for a password.")
          .arg(project)));
  auto* form = new QFormLayout;
  auto* service = new QComboBox;
  for (const RemoteService each :
       {RemoteService::GitHub, RemoteService::Forgejo, RemoteService::GitLab, RemoteService::Other}) {
    service->addItem(serviceName(each), static_cast<int>(each));
  }
  service->setCurrentIndex(service->findData(static_cast<int>(serviceOf(url).value_or(RemoteService::GitHub))));
  form->addRow(QObject::tr("&Service:"), service);
  auto* address = new QLineEdit(url);
  address->setMinimumWidth(420);
  form->addRow(QObject::tr("&Address:"), address);
  layout->addLayout(form);
  auto* help = new QLabel;
  help->setOpenExternalLinks(true);
  help->setWordWrap(true);
  layout->addWidget(help);
  auto* create = new QPushButton(QObject::tr("Create Repository in &Browser..."));
  create->setAutoDefault(false);
  auto* createRow = new QHBoxLayout;
  createRow->addWidget(create);
  createRow->addStretch(1);
  layout->addLayout(createRow);
  auto* privacy = note(QObject::tr("Every version records its author, %1: anyone who can read the repository "
                                   "sees the name and the email address.")
                           .arg(author));
  privacy->setTextFormat(Qt::PlainText);
  privacy->setStyleSheet(QStringLiteral("color: #8a5a00;"));
  layout->addWidget(privacy);
  auto* problem = problemLabel();
  layout->addWidget(problem);
  auto* buttons = new QDialogButtonBox(QDialogButtonBox::Ok | QDialogButtonBox::Cancel);
  buttons->button(QDialogButtonBox::Ok)->setText(QObject::tr("Connect"));
  layout->addWidget(buttons);

  const auto chosen = [service] { return static_cast<RemoteService>(service->currentData().toInt()); };
  const auto update = [&] {
    const RemoteService current = chosen();
    address->setPlaceholderText(placeholder(current));
    help->setText(QObject::tr("<a href=\"%1\">How to add an SSH key to your account</a>. For an address "
                              "starting with https://, git signs in with its credential helper.")
                      .arg(sshKeyHelpPage(current)));
    const QString page = newRepositoryPage(current, address->text(), project);
    create->setEnabled(!page.isEmpty());
    create->setToolTip(page.isEmpty() ? QObject::tr("Type the address of the service first") : page);
    QString why;
    const QString text = address->text().trimmed();
    if (text.contains(QStringLiteral("://")) && !QUrl(text).password().isEmpty()) {
      why = QObject::tr("The address holds a password or a token. Mitcad never gives them to git: use the "
                        "address without it, and sign in with an SSH key or a credential helper.");
    }
    problem->setText(why);
    buttons->button(QDialogButtonBox::Ok)->setEnabled(!text.isEmpty() && why.isEmpty());
  };
  QObject::connect(service, &QComboBox::currentIndexChanged, &dialog, update);
  QObject::connect(address, &QLineEdit::textChanged, &dialog, [&] {
    // A known host's service.
    if (const std::optional<RemoteService> known = serviceOf(address->text())) {
      service->setCurrentIndex(service->findData(static_cast<int>(*known)));
    }
    update();
  });
  QObject::connect(create, &QPushButton::clicked, &dialog, [&] {
    const QString page = newRepositoryPage(chosen(), address->text(), project);
    qInfo().noquote() << QStringLiteral("Connect dialog: new repository page %1").arg(page);
    QDesktopServices::openUrl(QUrl(page));
  });
  QObject::connect(buttons, &QDialogButtonBox::accepted, &dialog, &QDialog::accept);
  QObject::connect(buttons, &QDialogButtonBox::rejected, &dialog, &QDialog::reject);
  update();
  address->setFocus();
  address->selectAll();
  qInfo().noquote() << QStringLiteral("Connect dialog: %1, %2").arg(project, serviceName(chosen()));
  if (dialog.exec() != QDialog::Accepted) {
    qInfo().noquote() << QStringLiteral("Connect cancelled");
    return std::nullopt;
  }
  const QString chosenUrl = address->text().trimmed();
  qInfo().noquote() << QStringLiteral("Connect dialog: %1 (%2)").arg(chosenUrl, serviceName(chosen()));
  return chosenUrl;
}

std::optional<RemoteOpening> askOpenRemote(QWidget* parent, const QString& location, const RemoteOpening& previous) {
  QDialog dialog(parent);
  dialog.setWindowTitle(QObject::tr("Open Project from Remote"));
  auto* layout = new QVBoxLayout(&dialog);
  layout->addWidget(note(QObject::tr("Opens a project with its versions from a remote repository into a new "
                                     "folder on this computer; each version saved there is sent back. Signing "
                                     "in is git's: an SSH key or a credential helper.")));
  auto* form = new QFormLayout;
  auto* address = new QLineEdit(previous.url);
  address->setPlaceholderText(QObject::tr("git@github.com:you/project.git, https://... or a folder"));
  address->setMinimumWidth(420);
  form->addRow(QObject::tr("&Address:"), address);
  auto* folder = new QLineEdit(QDir::toNativeSeparators(previous.folder));
  auto* browse = new QPushButton(QObject::tr("&Browse..."));
  browse->setAutoDefault(false);
  auto* folderRow = new QHBoxLayout;
  folderRow->addWidget(folder, 1);
  folderRow->addWidget(browse);
  auto* folderLabel = new QLabel(QObject::tr("&Folder:"));
  folderLabel->setBuddy(folder);
  form->addRow(folderLabel, folderRow);
  layout->addLayout(form);
  auto* problem = problemLabel();
  layout->addWidget(problem);
  auto* buttons = new QDialogButtonBox(QDialogButtonBox::Ok | QDialogButtonBox::Cancel);
  buttons->button(QDialogButtonBox::Ok)->setText(QObject::tr("Open"));
  layout->addWidget(buttons);

  // The folder follows the address until it is typed.
  bool folderTyped = !previous.folder.isEmpty();
  const auto target = [&] { return QDir::cleanPath(QDir::fromNativeSeparators(folder->text().trimmed())); };
  const auto check = [&] {
    QString why;
    const QString dir = target();
    if (address->text().trimmed().isEmpty()) {
      why = QString(); // nothing to say yet
    } else if (dir.isEmpty() || QDir::isRelativePath(dir)) {
      why = QObject::tr("The folder is a full path.");
    } else if (QDir(dir).exists() &&
               !QDir(dir).isEmpty(QDir::AllEntries | QDir::NoDotAndDotDot | QDir::Hidden | QDir::System)) {
      why = QObject::tr("%1 is there and not empty: choose a new folder.").arg(QDir::toNativeSeparators(dir));
    } else if (QFileInfo(dir).exists() && !QFileInfo(dir).isDir()) {
      why = QObject::tr("%1 is a file.").arg(QDir::toNativeSeparators(dir));
    }
    problem->setText(why);
    buttons->button(QDialogButtonBox::Ok)->setEnabled(!address->text().trimmed().isEmpty() && why.isEmpty());
  };
  QObject::connect(address, &QLineEdit::textChanged, &dialog, [&] {
    if (!folderTyped) {
      const QString name = repositoryName(address->text());
      const QSignalBlocker blocker(folder);
      folder->setText(name.isEmpty() ? QString() : QDir::toNativeSeparators(QDir(location).filePath(name)));
    }
    check();
  });
  QObject::connect(folder, &QLineEdit::textEdited, &dialog, [&] { folderTyped = true; });
  QObject::connect(folder, &QLineEdit::textChanged, &dialog, check);
  QObject::connect(browse, &QPushButton::clicked, &dialog, [&] {
    const QString chosen = QFileDialog::getExistingDirectory(&dialog, QObject::tr("Location"), location);
    if (!chosen.isEmpty()) {
      folderTyped = true;
      const QString name = repositoryName(address->text());
      folder->setText(QDir::toNativeSeparators(name.isEmpty() ? chosen : QDir(chosen).filePath(name)));
    }
  });
  QObject::connect(buttons, &QDialogButtonBox::accepted, &dialog, &QDialog::accept);
  QObject::connect(buttons, &QDialogButtonBox::rejected, &dialog, &QDialog::reject);
  check();
  address->setFocus();
  address->selectAll();
  qInfo().noquote() << QStringLiteral("Open from Remote dialog: %1").arg(location);
  if (dialog.exec() != QDialog::Accepted) {
    qInfo().noquote() << QStringLiteral("Open from Remote cancelled");
    return std::nullopt;
  }
  RemoteOpening chosen{address->text().trimmed(), target()};
  qInfo().noquote() << QStringLiteral("Open from Remote dialog: %1 into %2").arg(chosen.url, chosen.folder);
  return chosen;
}

std::optional<QJsonObject> askResolveConflicts(QWidget* parent, const QJsonArray& conflicts,
                                               const std::function<QString(const QJsonObject& conflict)>& compare) {
  enum Column { kFile, kChange, kRemote, kHere, kKeep, kColumns };
  QDialog dialog(parent);
  dialog.setWindowTitle(QObject::tr("Resolve Sync Conflicts"));
  auto* layout = new QVBoxLayout(&dialog);
  layout->addWidget(note(
      QObject::tr("%n file(s) changed both in this project and on the remote. Choose what to keep of each; "
                  "nothing changes before Apply. Saving yours as a copy keeps both designs; the versions left "
                  "out stay in a backup of the project's history.",
                  nullptr, static_cast<int>(conflicts.size()))));
  auto* list = new QTreeWidget;
  list->setColumnCount(kColumns);
  list->setHeaderLabels({QObject::tr("File"), QObject::tr("Change"), QObject::tr("On the remote"),
                         QObject::tr("Here"), QObject::tr("Keep")});
  list->setRootIsDecorated(false);
  list->setAllColumnsShowFocus(true);
  list->header()->setStretchLastSection(false);
  layout->addWidget(list, 1);

  // The choices, by row.
  QStringList choices;
  QStringList logged;
  for (const QJsonValue& value : conflicts) {
    const QJsonObject conflict = value.toObject();
    const QString path = conflict.value(QStringLiteral("path")).toString();
    const QJsonObject mine = conflict.value(QStringLiteral("mine")).toObject();
    const QJsonObject theirs = conflict.value(QStringLiteral("theirs")).toObject();
    const bool copies = conflict.value(QStringLiteral("choices")).toArray().contains(QStringLiteral("copy"));
    // Nothing is lost: a copy where there can be one, else mine (theirs
    // stays in the history after it).
    choices << (copies ? QStringLiteral("copy") : QStringLiteral("mine"));
    auto* item = new QTreeWidgetItem(list);
    item->setText(kFile, path);
    item->setText(kChange, kindText(conflict.value(QStringLiteral("kind")).toString()));
    item->setText(kRemote, sideText(theirs, true));
    const int unsent = mine.value(QStringLiteral("versions")).toInt();
    item->setText(kHere, QObject::tr("%1; %n version(s) not sent", nullptr, unsent).arg(sideText(mine, false)));
    item->setText(kKeep, choiceText(choices.constLast()));
    logged << QStringLiteral("%1 (%2; remote %3 by %4; here %5 version(s))")
                  .arg(path, conflict.value(QStringLiteral("kind")).toString(),
                       theirs.value(QStringLiteral("version")).toObject().value(QStringLiteral("short_id")).toString(),
                       theirs.value(QStringLiteral("version"))
                           .toObject()
                           .value(QStringLiteral("author"))
                           .toObject()
                           .value(QStringLiteral("name"))
                           .toString())
                  .arg(unsent);
  }
  for (int column = 0; column < kColumns; ++column) {
    list->resizeColumnToContents(column);
  }
  // The versions' descriptions in their tooltips when long: the choice
  // stays in sight.
  for (const int column : {kRemote, kHere}) {
    list->setColumnWidth(column, std::min(list->columnWidth(column), 260));
    for (int row = 0; row < list->topLevelItemCount(); ++row) {
      QTreeWidgetItem* item = list->topLevelItem(row);
      item->setToolTip(column, item->text(column));
    }
  }

  auto* chosenBox = new QHBoxLayout;
  auto* forFile = new QLabel;
  chosenBox->addWidget(forFile);
  auto* keepMine = new QRadioButton(QObject::tr("&Keep mine"));
  keepMine->setToolTip(QObject::tr("Your versions of the file come after the remote's: the file is yours"));
  auto* takeTheirs = new QRadioButton(QObject::tr("Take &theirs"));
  takeTheirs->setToolTip(QObject::tr("The remote's file; your versions of it stay only in the backup"));
  auto* copy = new QRadioButton(QObject::tr("Save mine as a &copy"));
  auto* group = new QButtonGroup(&dialog);
  group->addButton(keepMine);
  group->addButton(takeTheirs);
  group->addButton(copy);
  chosenBox->addWidget(keepMine);
  chosenBox->addWidget(takeTheirs);
  chosenBox->addWidget(copy);
  chosenBox->addStretch(1);
  auto* compareButton = new QPushButton(QObject::tr("Co&mpare..."));
  compareButton->setToolTip(QObject::tr("What differs between the remote's design and yours"));
  chosenBox->addWidget(compareButton);
  layout->addLayout(chosenBox);

  auto* allRow = new QHBoxLayout;
  allRow->addWidget(new QLabel(QObject::tr("For all:")));
  auto* allMine = new QPushButton(QObject::tr("Keep All Mine"));
  auto* allTheirs = new QPushButton(QObject::tr("Take All Theirs"));
  auto* allCopies = new QPushButton(QObject::tr("Copy All Mine"));
  allRow->addWidget(allMine);
  allRow->addWidget(allTheirs);
  allRow->addWidget(allCopies);
  allRow->addStretch(1);
  layout->addLayout(allRow);
  auto* buttons = new QDialogButtonBox(QDialogButtonBox::Ok | QDialogButtonBox::Cancel);
  buttons->button(QDialogButtonBox::Ok)->setText(QObject::tr("Apply"));
  layout->addWidget(buttons);
  for (QPushButton* button : {compareButton, allMine, allTheirs, allCopies}) {
    button->setAutoDefault(false); // Enter applies
  }
  buttons->button(QDialogButtonBox::Ok)->setDefault(true);

  const auto row = [list] { return list->indexOfTopLevelItem(list->currentItem()); };
  const auto conflictAt = [&conflicts](int index) { return conflicts.at(index).toObject(); };
  const auto set = [&](int index, const QString& choice) {
    if (index < 0 || index >= choices.size() ||
        !conflictAt(index).value(QStringLiteral("choices")).toArray().contains(choice)) {
      return;
    }
    choices[index] = choice;
    list->topLevelItem(index)->setText(kKeep, choiceText(choice));
  };
  const auto selected = [&] {
    const int index = row();
    const bool there = index >= 0;
    for (QAbstractButton* button : group->buttons()) {
      button->setEnabled(there);
    }
    compareButton->setEnabled(false);
    if (!there) {
      forFile->clear();
      return;
    }
    const QJsonObject conflict = conflictAt(index);
    const QJsonArray allowed = conflict.value(QStringLiteral("choices")).toArray();
    forFile->setText(QObject::tr("%1:").arg(QFileInfo(conflict.value(QStringLiteral("path")).toString()).fileName()));
    copy->setEnabled(allowed.contains(QStringLiteral("copy")));
    const QString copyPath = conflict.value(QStringLiteral("copy")).toString();
    copy->setToolTip(copyPath.isEmpty() ? QObject::tr("Files of the project's own settings have no copy")
                                        : QObject::tr("The remote's file, and yours next to it as %1").arg(copyPath));
    const QString choice = choices.at(index);
    QRadioButton* button = choice == QLatin1String("mine")     ? keepMine
                           : choice == QLatin1String("theirs") ? takeTheirs
                                                               : copy;
    const QSignalBlocker blocker(group);
    button->setChecked(true);
    const bool bothThere = !conflict.value(QStringLiteral("mine")).toObject().value(QStringLiteral("blob")).isNull() &&
                           !conflict.value(QStringLiteral("theirs")).toObject().value(QStringLiteral("blob")).isNull();
    compareButton->setEnabled(bothThere && static_cast<bool>(compare));
  };
  QObject::connect(list, &QTreeWidget::currentItemChanged, &dialog, selected);
  QObject::connect(keepMine, &QRadioButton::toggled, &dialog, [&](bool on) {
    if (on) {
      set(row(), QStringLiteral("mine"));
    }
  });
  QObject::connect(takeTheirs, &QRadioButton::toggled, &dialog, [&](bool on) {
    if (on) {
      set(row(), QStringLiteral("theirs"));
    }
  });
  QObject::connect(copy, &QRadioButton::toggled, &dialog, [&](bool on) {
    if (on) {
      set(row(), QStringLiteral("copy"));
    }
  });
  const auto all = [&](const QString& choice) {
    for (int index = 0; index < choices.size(); ++index) {
      set(index, choice);
    }
    selected();
  };
  QObject::connect(allMine, &QPushButton::clicked, &dialog, [&] { all(QStringLiteral("mine")); });
  QObject::connect(allTheirs, &QPushButton::clicked, &dialog, [&] { all(QStringLiteral("theirs")); });
  QObject::connect(allCopies, &QPushButton::clicked, &dialog, [&] { all(QStringLiteral("copy")); });
  QObject::connect(compareButton, &QPushButton::clicked, &dialog, [&] {
    const int index = row();
    if (index < 0 || !compare) {
      return;
    }
    const QJsonObject conflict = conflictAt(index);
    const QString path = conflict.value(QStringLiteral("path")).toString();
    const QString text = compare(conflict);
    qInfo().noquote() << QStringLiteral("Resolve Sync Conflicts compare %1: %2")
                             .arg(path, text.split(QLatin1Char('\n'), Qt::SkipEmptyParts).join(QStringLiteral(" | ")));
    showComparison(&dialog, QObject::tr("Compare %1: the remote's and yours").arg(path), text);
  });
  QObject::connect(buttons, &QDialogButtonBox::accepted, &dialog, &QDialog::accept);
  QObject::connect(buttons, &QDialogButtonBox::rejected, &dialog, &QDialog::reject);
  if (list->topLevelItemCount() > 0) {
    list->setCurrentItem(list->topLevelItem(0));
  }
  selected();
  dialog.resize(900, 420);
  qInfo().noquote() << QStringLiteral("Resolve Sync Conflicts dialog: %1").arg(logged.join(QStringLiteral(" | ")));
  if (dialog.exec() != QDialog::Accepted) {
    qInfo().noquote() << QStringLiteral("Resolve Sync Conflicts cancelled");
    return std::nullopt;
  }
  QJsonObject chosen;
  for (int index = 0; index < choices.size(); ++index) {
    chosen.insert(conflictAt(index).value(QStringLiteral("path")).toString(), choices.at(index));
  }
  return chosen;
}

RemoteSettingsChoice askRemoteSettings(QWidget* parent, const QString& project, const QJsonObject& info) {
  QDialog dialog(parent);
  dialog.setWindowTitle(QObject::tr("Remote Settings"));
  auto* layout = new QVBoxLayout(&dialog);
  const QString url = info.value(QStringLiteral("url")).toString();
  auto* form = new QFormLayout;
  auto* address = new QLabel(QStringLiteral("%1: %2").arg(info.value(QStringLiteral("name")).toString(), url));
  address->setTextInteractionFlags(Qt::TextSelectableByMouse);
  form->addRow(QObject::tr("Remote:"), address);
  form->addRow(QObject::tr("Project:"), new QLabel(project));
  form->addRow(QObject::tr("Branch:"), new QLabel(QObject::tr("%1, following %2")
                                                      .arg(info.value(QStringLiteral("branch")).toString(),
                                                           info.value(QStringLiteral("upstream")).toString())));
  QString state;
  const QJsonValue ahead = info.value(QStringLiteral("ahead"));
  const QJsonValue behind = info.value(QStringLiteral("behind"));
  if (ahead.isNull() || behind.isNull()) {
    state = QObject::tr("not sent or checked yet");
  } else {
    state = QObject::tr("%n version(s) not on the remote", nullptr, ahead.toInt()) + QStringLiteral(", ") +
            QObject::tr("%n newer version(s) on the remote", nullptr, behind.toInt());
  }
  form->addRow(QObject::tr("Versions:"), note(state));
  form->addRow(QObject::tr("Last check:"), note(attemptText(info.value(QStringLiteral("last_fetch")))));
  form->addRow(QObject::tr("Last push:"), note(attemptText(info.value(QStringLiteral("last_push")))));
  form->addRow(QObject::tr("Last sync:"), note(attemptText(info.value(QStringLiteral("last_sync")))));
  layout->addLayout(form);
  auto* buttons = new QDialogButtonBox(QDialogButtonBox::Close);
  QPushButton* change = buttons->addButton(QObject::tr("Change &Address..."), QDialogButtonBox::ActionRole);
  QPushButton* disconnect = buttons->addButton(QObject::tr("&Disconnect..."), QDialogButtonBox::ActionRole);
  QPushButton* browser = buttons->addButton(QObject::tr("Open in &Browser"), QDialogButtonBox::ActionRole);
  browser->setEnabled(!remoteWebPage(url).isEmpty());
  layout->addWidget(buttons);
  RemoteSettingsChoice choice = RemoteSettingsChoice::None;
  const auto choose = [&](RemoteSettingsChoice chosen) {
    choice = chosen;
    dialog.accept();
  };
  QObject::connect(change, &QPushButton::clicked, &dialog, [&] { choose(RemoteSettingsChoice::ChangeAddress); });
  QObject::connect(disconnect, &QPushButton::clicked, &dialog, [&] { choose(RemoteSettingsChoice::Disconnect); });
  QObject::connect(browser, &QPushButton::clicked, &dialog, [&] { choose(RemoteSettingsChoice::OpenInBrowser); });
  QObject::connect(buttons, &QDialogButtonBox::rejected, &dialog, &QDialog::reject);
  qInfo().noquote() << QStringLiteral("Remote Settings dialog: %1 %2").arg(info.value(QStringLiteral("name")).toString(), url);
  dialog.exec();
  return choice;
}

} // namespace mitcad
