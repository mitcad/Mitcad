// SPDX-License-Identifier: MIT
#include "ProjectDialogs.hpp"

#include <memory>
#include <utility>

#include <QButtonGroup>
#include <QComboBox>
#include <QCoreApplication>
#include <QDateTime>
#include <QDialog>
#include <QDialogButtonBox>
#include <QDir>
#include <QEvent>
#include <QFileDialog>
#include <QFileInfo>
#include <QFormLayout>
#include <QGroupBox>
#include <QHBoxLayout>
#include <QIcon>
#include <QJsonArray>
#include <QLabel>
#include <QLineEdit>
#include <QListWidget>
#include <QMessageBox>
#include <QPixmap>
#include <QPointer>
#include <QProgressBar>
#include <QPushButton>
#include <QRadioButton>
#include <QTimer>
#include <QVBoxLayout>
#include <QtLogging>

#include "../framework/AppSettings.hpp"
#include "../framework/Dialogs.hpp"
#include "../framework/Icons.hpp"
#include "../framework/Json.hpp"
#include "../framework/TestSync.hpp"
#include "../framework/Theme.hpp"
#include "CloudSection.hpp"
#include "Projects.hpp"
#include "RemoteTask.hpp"
#include "mitcad_bridge/lib.h"

namespace mitcad {
namespace {

QString tr(const char* text) { return QCoreApplication::translate("ProjectDialogs", text); }
QString tr(const char* text, const char* disambiguation, int n) {
  return QCoreApplication::translate("ProjectDialogs", text, disambiguation, n);
}

// The pause after typing a folder before it is inspected.
constexpr int kFolderDelayMs = 300;

QLabel* note(const QString& text = QString()) {
  auto* label = new QLabel(text);
  label->setWordWrap(true);
  label->setTextFormat(Qt::PlainText);
  return label;
}

QString cleanFolder(const QString& text) {
  const QString path = QDir::fromNativeSeparators(text.trimmed());
  return path.isEmpty() ? QString() : QDir::cleanPath(path);
}

// "Robot arm <you@example.com>" of the fields, or "" when one is missing.
QString authorOf(const QLineEdit* name, const QLineEdit* email) {
  const QString who = name->text().trimmed();
  const QString address = email->text().trimmed();
  if (who.isEmpty() || !address.contains(QLatin1Char('@'))) {
    return {};
  }
  return QStringLiteral("%1 <%2>").arg(who, address);
}

// The author offered: git's configured one, else Mitcad's default author.
void fillAuthor(QLineEdit* name, QLineEdit* email) {
  const GitIdentity git = gitIdentity();
  const VersionSettings settings = VersionSettings::load();
  name->setText(!git.name.isEmpty() ? git.name : settings.name);
  email->setText(!git.email.isEmpty() ? git.email : settings.email);
}

// A new design's file text: an empty document (a project's first design).
QString emptyDesignText() {
  const rust::Box<Document> document = new_document();
  const rust::String json = document->to_json();
  return QString::fromUtf8(json.data(), static_cast<qsizetype>(json.size()));
}

// The first free Documents/Mitcad/Project<n>.
QString freeProjectFolder() {
  const QString location = projectsDirectory();
  for (int n = 1;; ++n) {
    const QString candidate = QDir(location).filePath(QStringLiteral("Project%1").arg(n));
    if (!QFileInfo::exists(candidate)) {
      return candidate;
    }
  }
}

QStringList names(const QJsonValue& list, int most) {
  QStringList out;
  for (const QJsonValue& value : list.toArray()) {
    if (out.size() == most) {
      out << QStringLiteral("...");
      break;
    }
    out << value.toString();
  }
  return out;
}

// A folder inspected (inspect_folder) on a thread of its own, after a pause
// while it is typed.
class FolderCheck {
public:
  FolderCheck(QObject* owner, std::function<void()> done) : m_owner(owner), m_done(std::move(done)) {
    m_delay = new QTimer(owner);
    m_delay->setSingleShot(true);
    m_delay->setInterval(kFolderDelayMs);
    TestSync::watch(m_delay);
    QObject::connect(m_delay, &QTimer::timeout, owner, [this] { start(); });
  }

  void check(const QString& dir) {
    if (dir == m_wanted) {
      return;
    }
    m_wanted = dir;
    m_answer = QJsonObject();
    m_delay->start();
  }

  // The answer for the folder last given, empty while it is inspected.
  const QJsonObject& answer() const { return m_answer; }
  const QString& folder() const { return m_wanted; }
  bool pending() const { return m_answer.isEmpty() && !m_wanted.isEmpty(); }
  // The folder's kind; a new folder inside a project is in that project.
  QString kind() const {
    const QString kind = m_answer.value(QStringLiteral("kind")).toString();
    if (kind == QLatin1String("missing") && m_answer.value(QStringLiteral("project_root")).isString()) {
      return QStringLiteral("inside_project");
    }
    return kind;
  }

private:
  void start() {
    if (m_task != nullptr) {
      RemoteTask* old = m_task;
      m_task = nullptr;
      QObject::disconnect(old, nullptr, m_owner, nullptr);
      QObject::connect(old, &RemoteTask::finished, old, &QObject::deleteLater);
    }
    const QString dir = m_wanted;
    if (dir.isEmpty() || QDir::isRelativePath(dir)) {
      m_done();
      return;
    }
    RemoteTask* task = RemoteTask::projects(
        {{QStringLiteral("cmd"), QStringLiteral("inspect_folder")}, {QStringLiteral("dir"), dir}}, m_owner);
    m_task = task;
    QObject::connect(task, &RemoteTask::finished, m_owner, [this, task, dir] {
      if (task != m_task) {
        return;
      }
      m_task = nullptr;
      task->deleteLater();
      if (dir != m_wanted) {
        return;
      }
      m_answer = task->answer();
      if (m_answer.isEmpty()) {
        m_answer = {{QStringLiteral("kind"), QStringLiteral("other")}};
      }
      m_done();
    });
    task->start();
  }

  QObject* m_owner;
  std::function<void()> m_done;
  QTimer* m_delay = nullptr;
  QPointer<RemoteTask> m_task;
  QString m_wanted;
  QJsonObject m_answer;
};

// A row of a running operation's progress: what it does and git's phase.
class ProgressRow : public QWidget {
public:
  ProgressRow() {
    auto* row = new QHBoxLayout(this);
    row->setContentsMargins(0, 0, 0, 0);
    m_text = new QLabel;
    m_text->setTextFormat(Qt::PlainText);
    m_bar = new QProgressBar;
    m_bar->setRange(0, 0);
    m_bar->setMaximumWidth(160);
    row->addWidget(m_text, 1);
    row->addWidget(m_bar);
    hide();
  }

  void follow(RemoteTask* task, const QString& text) {
    m_label = text;
    m_text->setText(text);
    m_bar->setRange(0, 0);
    QObject::connect(task, &RemoteTask::progressed, this, [this](const QString& phase, int percent) {
      m_text->setText(phase.isEmpty() ? m_label : QStringLiteral("%1 %2").arg(m_label, phase));
      if (percent >= 0) {
        m_bar->setRange(0, 100);
        m_bar->setValue(percent);
      }
    });
    show();
  }

private:
  QLabel* m_text = nullptr;
  QProgressBar* m_bar = nullptr;
  QString m_label;
};

// What a check of a remote for a new project found (New Project).
QString describeForNewProject(const QJsonObject& answer, bool& ok) {
  if (answer.value(QStringLiteral("new_folder")).toBool()) {
    return tr("✓ An empty repository is made in this folder: ready.");
  }
  if (answer.value(QStringLiteral("has_project")).toBool()) {
    ok = false;
    return tr("This repository holds a Mitcad project: Open It Instead opens it into the folder.");
  }
  if (!answer.value(QStringLiteral("reachable")).toBool(true)) {
    ok = false;
    return tr("The repository cannot be reached.");
  }
  if (answer.value(QStringLiteral("empty")).toBool()) {
    return tr("✓ Reachable and empty: ready.");
  }
  return tr("✓ Mitcad adds the project beside the repository's files (%1).")
      .arg(names(answer.value(QStringLiteral("files")), 5).join(QStringLiteral(", ")));
}

// What a check of a remote to open found (Open from Cloud).
QString describeForOpening(const QJsonObject& answer, bool& ok) {
  if (answer.value(QStringLiteral("new_folder")).toBool()) {
    return tr("An empty repository is made in this folder: Create Project Here makes a Cloud project in it.");
  }
  if (!answer.value(QStringLiteral("reachable")).toBool(true)) {
    ok = false;
    return tr("The repository cannot be reached.");
  }
  if (answer.value(QStringLiteral("has_project")).toBool()) {
    const QString name = repositoryNameOf(answer.value(QStringLiteral("url")).toString());
    const int versions = answer.value(QStringLiteral("versions")).toInt();
    const QJsonObject latest = answer.value(QStringLiteral("latest")).toObject();
    QString text = tr("✓ Project \"%1\": %n version(s)", nullptr, versions).arg(name);
    if (!latest.isEmpty()) {
      const QDateTime when = QDateTime::fromSecsSinceEpoch(latest.value(QStringLiteral("time")).toInteger());
      text += tr(", latest %1 by %2")
                  .arg(when.toLocalTime().toString(QStringLiteral("yyyy-MM-dd")),
                       latest.value(QStringLiteral("author")).toString());
    }
    return text;
  }
  if (answer.value(QStringLiteral("empty")).toBool()) {
    return tr("The repository is empty: Create Project Here makes a Cloud project in it.");
  }
  return tr("The repository has files but no Mitcad project (%1): Make It a Project adds one beside them.")
      .arg(names(answer.value(QStringLiteral("files")), 5).join(QStringLiteral(", ")));
}

// The check of an address by the projects command check_remote.
RemoteTask* checkRemote(const QString& url, QObject* parent) {
  return RemoteTask::projects({{QStringLiteral("cmd"), QStringLiteral("check_remote")}, {QStringLiteral("url"), url}},
                              parent);
}

// git's state for the Cloud choice: missing (Cloud is off, with a link to
// install it), and whether it has a credential helper (HTTPS by default).
struct GitState {
  bool missing = false;
  bool credentialHelper = false;
};

GitState gitState() {
  const QJsonObject info = projectsCommand({{QStringLiteral("cmd"), QStringLiteral("git_info")}});
  GitState state;
  state.missing = errorClassOf(info) == QLatin1String("git_missing") ||
                  info.value(QStringLiteral("error")).toObject().value(QStringLiteral("class")).toString() ==
                      QLatin1String("git_missing");
  state.credentialHelper =
      info.value(QStringLiteral("credential_helper")).toObject().value(QStringLiteral("configured")).toBool();
  return state;
}

QLabel* gitMissingNote() {
  auto* label = new QLabel(tr("Cloud projects need the git program, which was not found: "
                              "<a href=\"https://git-scm.com/downloads\">install git</a> and start Mitcad again."));
  label->setWordWrap(true);
  label->setOpenExternalLinks(true);
  return label;
}

// The project create_project made: true when its first version is there
// (a push that failed after it leaves the project made).
bool projectMade(const QJsonObject& answer) {
  return answer.value(QStringLiteral("commit")).isString() || answer.value(QStringLiteral("commit")).isObject();
}

// ---------------------------------------------------------------------------
// New Project

class NewProjectDialog : public QDialog {
public:
  NewProjectDialog(QWidget* parent, const NewProjectOptions& options);
  const std::optional<NewProjectResult>& result() const { return m_result; }

protected:
  void changeEvent(QEvent* event) override;
  void reject() override;

private:
  QString folder() const { return cleanFolder(m_folder->text()); }
  bool cloud() const { return m_cloudChoice->isChecked(); }
  void folderChanged();
  void folderChecked();
  void updateState();
  void createClicked();
  void create(bool asCloud);
  void finished(RemoteTask* task, bool asCloud, const QString& design);
  void setBusy(bool busy);

  NewProjectOptions m_options;
  std::optional<NewProjectResult> m_result;
  std::unique_ptr<FolderCheck> m_check;
  QLineEdit* m_folder = nullptr;
  QPushButton* m_browse = nullptr;
  QRadioButton* m_local = nullptr;
  QRadioButton* m_cloudChoice = nullptr;
  QLineEdit* m_name = nullptr;
  QLineEdit* m_email = nullptr;
  QLabel* m_authorNote = nullptr;
  QGroupBox* m_cloudBox = nullptr;
  CloudSection* m_section = nullptr;
  QComboBox* m_live = nullptr;
  QLabel* m_folderNote = nullptr;
  QLabel* m_problem = nullptr;
  ProgressRow* m_progress = nullptr;
  QDialogButtonBox* m_buttons = nullptr;
  QPushButton* m_create = nullptr;
  QPushButton* m_openInstead = nullptr;
  QPushButton* m_localForNow = nullptr;
  QPointer<RemoteTask> m_task;
  QString m_loggedFolder;
  bool m_gitMissing = false;
};

NewProjectDialog::NewProjectDialog(QWidget* parent, const NewProjectOptions& options)
    : QDialog(parent), m_options(options) {
  setWindowTitle(options.title.isEmpty() ? tr("New Project") : options.title);
  auto* layout = new QVBoxLayout(this);
  layout->addWidget(note(tr("A project is a folder whose designs keep their versions: Local on this computer, or "
                            "Cloud in a git repository that others share.")));
  auto* form = new QFormLayout;
  m_folder = new QLineEdit(QDir::toNativeSeparators(options.folder.isEmpty() ? freeProjectFolder() : options.folder));
  m_folder->setMinimumWidth(420);
  m_browse = new QPushButton(tr("Bro&wse..."));
  m_browse->setAutoDefault(false);
  auto* folderRow = new QHBoxLayout;
  folderRow->addWidget(m_folder, 1);
  folderRow->addWidget(m_browse);
  auto* folderLabel = new QLabel(tr("&Folder:"));
  folderLabel->setBuddy(m_folder);
  form->addRow(folderLabel, folderRow);
  if (options.folderFixed) {
    m_folder->setReadOnly(true);
    m_browse->hide();
  }
  m_local = new QRadioButton(tr("&Local – versions on this computer"));
  m_cloudChoice = new QRadioButton(tr("Cl&oud (git) – versions also in a git repository, shared"));
  auto* storage = new QButtonGroup(this);
  storage->addButton(m_local);
  storage->addButton(m_cloudChoice);
  auto* storageColumn = new QVBoxLayout;
  storageColumn->addWidget(m_local);
  storageColumn->addWidget(m_cloudChoice);
  form->addRow(tr("Storage:"), storageColumn);
  m_name = new QLineEdit;
  m_name->setPlaceholderText(tr("Your Name"));
  m_email = new QLineEdit;
  m_email->setPlaceholderText(tr("you@example.com"));
  auto* authorRow = new QHBoxLayout;
  authorRow->addWidget(m_name, 1);
  authorRow->addWidget(m_email, 1);
  auto* authorLabel = new QLabel(tr("Au&thor:"));
  authorLabel->setBuddy(m_name);
  form->addRow(authorLabel, authorRow);
  layout->addLayout(form);
  m_authorNote = note(tr("With Cloud, the name and email go into every version: anyone who can read the repository "
                         "sees them."));
  setWarningStyleSheet(m_authorNote);
  layout->addWidget(m_authorNote);
  m_cloudBox = new QGroupBox(tr("Cloud"));
  auto* cloudColumn = new QVBoxLayout(m_cloudBox);
  m_section = new CloudSection(checkRemote, describeForNewProject);
  cloudColumn->addWidget(m_section);
  auto* liveRow = new QFormLayout;
  m_live = new QComboBox;
  m_live->addItem(tr("None"), QString());
  const RemoteSettings remote = RemoteSettings::load();
  if (remote.allowLive && !remote.defaultBroker.isEmpty()) {
    m_live->addItem(remote.defaultBroker, remote.defaultBroker);
    m_live->setCurrentIndex(1);
  }
  m_live->setToolTip(tr("Edit locks and new versions reach the others at once through an MQTT broker; without "
                        "one they are polled"));
  liveRow->addRow(tr("Live upd&ates:"), m_live);
  cloudColumn->addLayout(liveRow);
  layout->addWidget(m_cloudBox);
  m_folderNote = note();
  layout->addWidget(m_folderNote);
  const GitState git = gitState();
  m_gitMissing = git.missing;
  if (git.missing) {
    m_cloudChoice->setEnabled(false);
    layout->addWidget(gitMissingNote());
  }
  m_section->setDefaultConnect(git.credentialHelper ? CloudConnect::Https : CloudConnect::Ssh);
  m_problem = note();
  setErrorStyleSheet(m_problem);
  layout->addWidget(m_problem);
  m_progress = new ProgressRow;
  layout->addWidget(m_progress);
  m_buttons = new QDialogButtonBox(QDialogButtonBox::Cancel);
  m_openInstead = m_buttons->addButton(tr("Open It &Instead"), QDialogButtonBox::ActionRole);
  m_localForNow = m_buttons->addButton(tr("Create as Local for &Now"), QDialogButtonBox::ActionRole);
  m_create = m_buttons->addButton(tr("&Create"), QDialogButtonBox::AcceptRole);
  for (QPushButton* button : {m_openInstead, m_localForNow}) {
    button->setAutoDefault(false);
    button->hide();
  }
  m_buttons->button(QDialogButtonBox::Cancel)->setAutoDefault(false);
  layout->addWidget(m_buttons);
  // Enter creates, also while Create waits for a check (the button is the
  // dialog's default only once it is in the dialog).
  m_create->setDefault(true);

  m_check = std::make_unique<FolderCheck>(this, [this] { folderChecked(); });
  const bool preset = (options.cloud || !options.url.isEmpty()) && !git.missing;
  m_cloudChoice->setChecked(preset);
  m_local->setChecked(!preset);
  m_cloudBox->setVisible(preset);
  m_authorNote->setVisible(preset);
  fillAuthor(m_name, m_email);
  m_section->setFolderName(QFileInfo(folder()).fileName());
  if (!options.url.isEmpty()) {
    m_section->setAddress(options.url);
  }

  connect(m_folder, &QLineEdit::textChanged, this, [this] { folderChanged(); });
  connect(m_browse, &QPushButton::clicked, this, [this] {
    const QString chosen =
        QFileDialog::getExistingDirectory(this, tr("Project Folder"), QFileInfo(folder()).absolutePath());
    if (!chosen.isEmpty()) {
      m_folder->setText(QDir::toNativeSeparators(chosen));
    }
  });
  connect(m_cloudChoice, &QRadioButton::toggled, this, [this](bool on) {
    m_cloudBox->setVisible(on);
    m_authorNote->setVisible(on);
    qInfo().noquote() << QStringLiteral("New Project dialog: storage %1")
                             .arg(on ? QStringLiteral("cloud") : QStringLiteral("local"));
    if (on && !m_section->url().isEmpty()) {
      m_section->recheck();
    }
    updateState();
    adjustSize();
  });
  for (QLineEdit* edit : {m_name, m_email}) {
    connect(edit, &QLineEdit::textChanged, this, [this] { updateState(); });
  }
  connect(m_section, &CloudSection::addressChanged, this, [this] { updateState(); });
  connect(m_section, &CloudSection::checked, this, [this] { updateState(); });
  connect(m_create, &QPushButton::clicked, this, [this] { createClicked(); });
  connect(m_openInstead, &QPushButton::clicked, this, [this] {
    m_section->stopCheck();
    NewProjectResult result;
    result.kind = NewProjectResult::Kind::OpenFromCloud;
    result.folder = folder();
    result.url = m_section->url();
    m_result = result;
    qInfo().noquote() << QStringLiteral("New Project: Open It Instead %1 into %2").arg(result.url, result.folder);
    accept();
  });
  connect(m_localForNow, &QPushButton::clicked, this, [this] {
    qInfo().noquote() << QStringLiteral("New Project: Create as Local for Now");
    create(false);
  });
  connect(m_buttons, &QDialogButtonBox::rejected, this, &NewProjectDialog::reject);

  folderChanged();
  // The first field to type into: the author when it is missing.
  if (m_name->text().isEmpty()) {
    m_name->setFocus();
  } else if (m_email->text().isEmpty()) {
    m_email->setFocus();
  } else if (!options.folderFixed) {
    m_folder->setFocus();
    m_folder->selectAll();
  } else {
    m_create->setFocus();
  }
  qInfo().noquote() << QStringLiteral("New Project dialog: %1").arg(folder());
  if (preset) {
    qInfo().noquote() << QStringLiteral("New Project dialog: storage cloud");
  }
}

void NewProjectDialog::changeEvent(QEvent* event) {
  QDialog::changeEvent(event);
  // Back from the browser (a repository made there): the address again.
  if (event->type() == QEvent::ActivationChange && isActiveWindow() && cloud() && m_task == nullptr) {
    m_section->recheck();
  }
}

void NewProjectDialog::reject() {
  if (m_task != nullptr) {
    // While creating, Cancel stops it (the folder stays as it was).
    m_task->cancel();
    return;
  }
  m_section->stopCheck();
  qInfo().noquote() << QStringLiteral("New Project cancelled");
  QDialog::reject();
}

void NewProjectDialog::folderChanged() {
  m_section->setFolderName(QFileInfo(folder()).fileName());
  m_check->check(folder());
  updateState();
}

void NewProjectDialog::folderChecked() {
  const QJsonObject answer = m_check->answer();
  const QString kind = m_check->kind();
  const QString dir = QDir::toNativeSeparators(folder());
  QString text;
  if (kind == QLatin1String("project")) {
    text = tr("%1 is a Mitcad project already: Open It opens it.").arg(dir);
  } else if (kind == QLatin1String("inside_project")) {
    text = tr("%1 is inside the project %2: Open It opens that project.")
               .arg(dir, QDir::toNativeSeparators(answer.value(QStringLiteral("project_root")).toString()));
  } else if (kind == QLatin1String("repository")) {
    text = tr("%1 is a git repository: the project goes into it.").arg(dir);
  } else if (kind == QLatin1String("designs")) {
    text = tr("The designs in %1 become the project's, in its first version.").arg(dir);
  } else if (kind == QLatin1String("other")) {
    text = tr("The files in %1 stay where they are, and are not versioned.").arg(dir);
  }
  const QString outer = answer.value(QStringLiteral("outer_repository")).toString();
  if (!outer.isEmpty() && kind != QLatin1String("project") && kind != QLatin1String("inside_project")) {
    text += (text.isEmpty() ? QString() : QStringLiteral(" ")) +
            tr("It is inside the git repository %1: the project gets a repository of its own, which the other one "
               "does not record.")
                .arg(QDir::toNativeSeparators(outer));
  }
  m_folderNote->setText(text);
  if (!outer.isEmpty()) {
    setWarningStyleSheet(m_folderNote);
  } else {
    m_folderNote->setStyleSheet(QString());
  }
  const QString logged = QStringLiteral("%1: %2%3").arg(folder(), kind,
                                                        outer.isEmpty() ? QString()
                                                                        : QStringLiteral(" (inside %1)").arg(outer));
  if (logged != m_loggedFolder) {
    m_loggedFolder = logged;
    qInfo().noquote() << QStringLiteral("New Project dialog: folder %1").arg(logged);
  }
  updateState();
}

void NewProjectDialog::updateState() {
  if (m_task != nullptr) {
    return;
  }
  const QString dir = folder();
  const QString kind = m_check->folder() == dir ? m_check->kind() : QString();
  const bool opens = kind == QLatin1String("project") || kind == QLatin1String("inside_project");
  QString why;
  bool can = true;
  if (dir.isEmpty() || QDir::isRelativePath(dir)) {
    why = tr("The folder is a full path.");
    can = false;
  } else if (QFileInfo(dir).exists() && !QFileInfo(dir).isDir()) {
    why = tr("%1 is a file.").arg(QDir::toNativeSeparators(dir));
    can = false;
  } else if (m_check->pending() || kind.isEmpty()) {
    can = false; // the folder is looked at
  } else if (!opens) {
    if (authorOf(m_name, m_email).isEmpty()) {
      why = tr("Versions need an author: a name and an email address.");
      can = false;
    } else if (cloud()) {
      can = !m_section->url().isEmpty() && m_section->hasAnswer() && m_section->answerOk();
    }
  }
  m_create->setText(opens ? tr("Open &It") : tr("&Create"));
  m_create->setEnabled(can);
  m_problem->setText(why);
  const QString cls = errorClassOf(m_section->answer());
  m_openInstead->setVisible(cloud() && !opens && m_section->answer().value(QStringLiteral("has_project")).toBool());
  m_localForNow->setVisible(cloud() && !opens &&
                            (cls == QLatin1String("network") || cls == QLatin1String("timed_out")));
  m_localForNow->setEnabled(!authorOf(m_name, m_email).isEmpty());
}

void NewProjectDialog::createClicked() {
  const QString kind = m_check->kind();
  if (kind == QLatin1String("project") || kind == QLatin1String("inside_project")) {
    m_section->stopCheck();
    NewProjectResult result;
    result.kind = NewProjectResult::Kind::OpenProject;
    result.folder = kind == QLatin1String("project")
                        ? folder()
                        : QDir::cleanPath(m_check->answer().value(QStringLiteral("project_root")).toString());
    m_result = result;
    qInfo().noquote() << QStringLiteral("New Project: Open It %1").arg(result.folder);
    accept();
    return;
  }
  create(cloud());
}

void NewProjectDialog::setBusy(bool busy) {
  for (QWidget* widget : std::initializer_list<QWidget*>{m_folder, m_browse, m_local, m_cloudChoice, m_name, m_email,
                                                         m_section, m_live, m_create, m_openInstead,
                                                         m_localForNow}) {
    widget->setEnabled(!busy);
  }
  if (!busy) {
    m_progress->hide();
    m_cloudChoice->setEnabled(!m_gitMissing);
  }
}

void NewProjectDialog::create(bool asCloud) {
  const QString dir = folder();
  const QString author = authorOf(m_name, m_email);
  QJsonObject command{{QStringLiteral("cmd"), QStringLiteral("create_project")},
                      {QStringLiteral("dir"), dir},
                      {QStringLiteral("author"), author},
                      {QStringLiteral("include_designs"), true}};
  QString design;
  const bool hasDesigns = !m_check->answer().value(QStringLiteral("designs")).toArray().isEmpty();
  if (m_options.firstDesign && !hasDesigns) {
    design = QFileInfo(dir).fileName() + QStringLiteral(".mitcad");
    command.insert(QStringLiteral("design"),
                   QJsonObject{{QStringLiteral("path"), design}, {QStringLiteral("text"), emptyDesignText()}});
  }
  const QString url = asCloud ? m_section->url() : QString();
  const bool bare = asCloud && m_section->needsBareRepository();
  if (asCloud) {
    command.insert(QStringLiteral("url"), url);
    const QString broker = m_live->currentData().toString();
    if (!broker.isEmpty()) {
      // Live updates for everyone, in the first version's marker.
      command.insert(QStringLiteral("shared"),
                     QJsonObject{{QStringLiteral("live_updates"), QJsonObject{{QStringLiteral("broker"), broker}}}});
    }
  }
  m_section->stopCheck();
  m_problem->clear();
  const QByteArray createJson = compactJson(command);
  const QByteArray bareJson =
      compactJson({{QStringLiteral("cmd"), QStringLiteral("init_bare")}, {QStringLiteral("dir"), url}});
  RemoteTask* task = RemoteTask::custom(
      QStringLiteral("create_project"),
      [createJson, bareJson, bare](const SyncControl& control) {
        if (bare) {
          // A shared folder made a repository first.
          const QJsonObject made = parseObject(projects_command(rustStr(bareJson), control));
          if (!errorClassOf(made).isEmpty()) {
            return made;
          }
        }
        return parseObject(projects_command(rustStr(createJson), control));
      },
      this);
  m_task = task;
  setBusy(true);
  m_progress->follow(task, asCloud ? tr("Creating the project and sending it to %1...").arg(url)
                                   : tr("Creating the project..."));
  connect(task, &RemoteTask::finished, this, [this, task, asCloud, design] { finished(task, asCloud, design); });
  qInfo().noquote() << QStringLiteral("New Project: creating %1 (%2)")
                           .arg(dir, asCloud ? QStringLiteral("cloud %1").arg(url) : QStringLiteral("local"));
  task->start();
}

void NewProjectDialog::finished(RemoteTask* task, bool asCloud, const QString& design) {
  const QJsonObject answer = task->answer();
  m_task = nullptr;
  task->deleteLater();
  if (projectMade(answer)) {
    NewProjectResult result;
    result.kind = NewProjectResult::Kind::Created;
    result.folder = QDir::cleanPath(answer.value(QStringLiteral("root")).toString());
    if (result.folder.isEmpty()) {
      result.folder = folder();
    }
    result.url = asCloud ? m_section->url() : QString();
    result.answer = answer;
    result.design = design;
    result.broker = asCloud ? m_live->currentData().toString() : QString();
    m_result = result;
    const QString cls = errorClassOf(answer);
    qInfo().noquote() << QStringLiteral("New Project: created %1: version %2 on %3%4")
                             .arg(result.folder, answer.value(QStringLiteral("commit")).toString().left(7),
                                  answer.value(QStringLiteral("branch")).toString(),
                                  cls.isEmpty() ? (asCloud ? QStringLiteral(", sent") : QString())
                                                : QStringLiteral(", not sent (%1)").arg(cls));
    QDialog::accept();
    return;
  }
  setBusy(false);
  const QString cls = errorClassOf(answer);
  qInfo().noquote() << QStringLiteral("New Project failed (%1): %2").arg(cls, errorMessageOf(answer));
  if (cls == QLatin1String("cancelled")) {
    m_problem->setText(tr("Cancelled: the folder is as it was."));
  } else {
    m_problem->setText(errorMessageOf(answer).isEmpty() ? tr("The project could not be made.")
                                                        : errorMessageOf(answer));
  }
  updateState();
  if (asCloud && (cls == QLatin1String("network") || cls == QLatin1String("timed_out"))) {
    m_localForNow->show();
  }
}

// ---------------------------------------------------------------------------
// Open from Cloud

class OpenFromCloudDialog : public QDialog {
public:
  OpenFromCloudDialog(QWidget* parent, const OpenFromCloudOptions& options);
  const std::optional<OpenFromCloudResult>& result() const { return m_result; }

protected:
  void changeEvent(QEvent* event) override;
  void reject() override;

private:
  QString folder() const { return cleanFolder(m_folder->text()); }
  // What Open does now: "clone", "create", "adopt", "open" or "".
  QString action() const;
  void addressChanged();
  void updateState();
  void openClicked();
  void finished(RemoteTask* task, const QString& action, const QString& design);

  std::optional<OpenFromCloudResult> m_result;
  QString m_location;
  CloudSection* m_section = nullptr;
  std::unique_ptr<FolderCheck> m_check;
  QLineEdit* m_folder = nullptr;
  QPushButton* m_browse = nullptr;
  QLabel* m_authorLabel = nullptr;
  QWidget* m_authorRow = nullptr;
  QLineEdit* m_name = nullptr;
  QLineEdit* m_email = nullptr;
  QLabel* m_problem = nullptr;
  ProgressRow* m_progress = nullptr;
  QPushButton* m_open = nullptr;
  QPointer<RemoteTask> m_task;
  bool m_folderTyped = false;
  QString m_loggedAction;
};

OpenFromCloudDialog::OpenFromCloudDialog(QWidget* parent, const OpenFromCloudOptions& options)
    : QDialog(parent), m_location(projectsDirectory()) {
  setWindowTitle(tr("Open from Cloud"));
  auto* layout = new QVBoxLayout(this);
  layout->addWidget(note(tr("Opens a Cloud project from its git repository into a new folder on this computer; each "
                            "version saved there is sent back. Signing in is git's: an SSH key or a credential "
                            "helper.")));
  m_section = new CloudSection(checkRemote, describeForOpening);
  layout->addWidget(m_section);
  auto* form = new QFormLayout;
  m_folder = new QLineEdit(QDir::toNativeSeparators(options.folder));
  m_folder->setMinimumWidth(420);
  m_browse = new QPushButton(tr("Bro&wse..."));
  m_browse->setAutoDefault(false);
  auto* folderRow = new QHBoxLayout;
  folderRow->addWidget(m_folder, 1);
  folderRow->addWidget(m_browse);
  auto* folderLabel = new QLabel(tr("&Folder:"));
  folderLabel->setBuddy(m_folder);
  form->addRow(folderLabel, folderRow);
  m_name = new QLineEdit;
  m_name->setPlaceholderText(tr("Your Name"));
  m_email = new QLineEdit;
  m_email->setPlaceholderText(tr("you@example.com"));
  m_authorRow = new QWidget;
  auto* authorLayout = new QHBoxLayout(m_authorRow);
  authorLayout->setContentsMargins(0, 0, 0, 0);
  authorLayout->addWidget(m_name, 1);
  authorLayout->addWidget(m_email, 1);
  m_authorLabel = new QLabel(tr("Au&thor:"));
  m_authorLabel->setBuddy(m_name);
  form->addRow(m_authorLabel, m_authorRow);
  layout->addLayout(form);
  m_problem = note();
  layout->addWidget(m_problem);
  m_progress = new ProgressRow;
  layout->addWidget(m_progress);
  auto* buttons = new QDialogButtonBox(QDialogButtonBox::Cancel);
  m_open = buttons->addButton(tr("&Open"), QDialogButtonBox::AcceptRole);
  buttons->button(QDialogButtonBox::Cancel)->setAutoDefault(false);
  layout->addWidget(buttons);
  m_open->setDefault(true); // once in the dialog
  fillAuthor(m_name, m_email);
  form->setRowVisible(m_authorLabel, false);
  m_check = std::make_unique<FolderCheck>(this, [this] { updateState(); });
  const GitState git = gitState();
  m_section->setDefaultConnect(git.credentialHelper ? CloudConnect::Https : CloudConnect::Ssh);
  if (git.missing) {
    layout->insertWidget(1, gitMissingNote());
    m_section->setEnabled(false);
  }
  m_folderTyped = !options.folder.isEmpty();

  connect(m_section, &CloudSection::addressChanged, this, [this] { addressChanged(); });
  connect(m_section, &CloudSection::checked, this, [this, form] {
    const QJsonObject answer = m_section->answer();
    const bool makes = errorClassOf(answer).isEmpty() && !answer.value(QStringLiteral("has_project")).toBool() &&
                       !answer.isEmpty();
    form->setRowVisible(m_authorLabel, makes);
    updateState();
  });
  connect(m_folder, &QLineEdit::textEdited, this, [this] { m_folderTyped = true; });
  connect(m_folder, &QLineEdit::textChanged, this, [this] {
    m_check->check(folder());
    updateState();
  });
  for (QLineEdit* edit : {m_name, m_email}) {
    connect(edit, &QLineEdit::textChanged, this, [this] { updateState(); });
  }
  connect(m_browse, &QPushButton::clicked, this, [this] {
    const QString chosen = QFileDialog::getExistingDirectory(this, tr("Location"), m_location);
    if (!chosen.isEmpty()) {
      m_folderTyped = true;
      const QString name = repositoryNameOf(m_section->url());
      m_folder->setText(QDir::toNativeSeparators(name.isEmpty() ? chosen : QDir(chosen).filePath(name)));
    }
  });
  connect(m_open, &QPushButton::clicked, this, [this] { openClicked(); });
  connect(buttons, &QDialogButtonBox::rejected, this, &OpenFromCloudDialog::reject);
  if (!options.url.isEmpty()) {
    m_section->setAddress(options.url);
  }
  m_check->check(folder());
  updateState();
  if (QWidget* first = m_section->firstField()) {
    first->setFocus();
  }
  qInfo().noquote() << QStringLiteral("Open from Cloud dialog: %1").arg(m_location);
}

void OpenFromCloudDialog::changeEvent(QEvent* event) {
  QDialog::changeEvent(event);
  if (event->type() == QEvent::ActivationChange && isActiveWindow() && m_task == nullptr) {
    m_section->recheck();
  }
}

void OpenFromCloudDialog::reject() {
  if (m_task != nullptr) {
    m_task->cancel();
    return;
  }
  m_section->stopCheck();
  qInfo().noquote() << QStringLiteral("Open from Cloud cancelled");
  QDialog::reject();
}

void OpenFromCloudDialog::addressChanged() {
  if (!m_folderTyped) {
    const QString name = repositoryNameOf(m_section->url());
    const QSignalBlocker blocker(m_folder);
    m_folder->setText(name.isEmpty() ? QString() : QDir::toNativeSeparators(QDir(m_location).filePath(name)));
    m_check->check(folder());
  }
  updateState();
}

QString OpenFromCloudDialog::action() const {
  const QJsonObject inspection = m_check->answer();
  const QString kind = m_check->kind();
  const QString url = m_section->url();
  if (kind == QLatin1String("project") || kind == QLatin1String("inside_project")) {
    const QString remote = inspection.value(QStringLiteral("remote")).toObject().value(QStringLiteral("url")).toString();
    return !url.isEmpty() && !remote.isEmpty() && sameRepository(remote, url) ? QStringLiteral("open") : QString();
  }
  if (!m_section->hasAnswer() || !m_section->answerOk()) {
    return {};
  }
  const QJsonObject answer = m_section->answer();
  if (answer.value(QStringLiteral("has_project")).toBool()) {
    return QStringLiteral("clone");
  }
  if (answer.value(QStringLiteral("empty")).toBool() || answer.value(QStringLiteral("new_folder")).toBool()) {
    return QStringLiteral("create");
  }
  return QStringLiteral("adopt");
}

void OpenFromCloudDialog::updateState() {
  if (m_task != nullptr) {
    return;
  }
  const QString dir = folder();
  const QString kind = m_check->folder() == dir ? m_check->kind() : QString();
  const QString what = action();
  QString why;
  bool can = !what.isEmpty();
  if (m_section->url().isEmpty()) {
    can = false;
  } else if (dir.isEmpty() || QDir::isRelativePath(dir)) {
    why = tr("The folder is a full path.");
    can = false;
  } else if (m_check->pending() || kind.isEmpty()) {
    can = false;
  } else if (what != QLatin1String("open") && kind != QLatin1String("missing") && kind != QLatin1String("empty")) {
    why = kind == QLatin1String("project") || kind == QLatin1String("inside_project")
              ? tr("%1 is another project: choose a new or empty folder.").arg(QDir::toNativeSeparators(dir))
              : tr("%1 is not empty: choose a new or empty folder.").arg(QDir::toNativeSeparators(dir));
    can = false;
  } else if ((what == QLatin1String("create") || what == QLatin1String("adopt")) &&
             authorOf(m_name, m_email).isEmpty()) {
    why = tr("Making a project needs its author: a name and an email address.");
    can = false;
  }
  QString text = tr("&Open");
  if (what == QLatin1String("create")) {
    text = tr("&Create Project Here");
  } else if (what == QLatin1String("adopt")) {
    text = tr("&Make It a Project");
  } else if (what == QLatin1String("open")) {
    text = tr("&Open It");
  }
  m_open->setText(text);
  m_open->setEnabled(can);
  m_problem->setText(why);
  if (!why.isEmpty()) {
    setErrorStyleSheet(m_problem);
  }
  const QString logged = QStringLiteral("%1, %2").arg(what.isEmpty() ? QStringLiteral("none") : what,
                                                      can ? QStringLiteral("enabled") : QStringLiteral("disabled"));
  if (logged != m_loggedAction && (!kind.isEmpty() || !why.isEmpty())) {
    m_loggedAction = logged;
    qInfo().noquote() << QStringLiteral("Open from Cloud dialog: %1%2")
                             .arg(logged, why.isEmpty() ? QString() : QStringLiteral(": %1").arg(why));
  }
}

void OpenFromCloudDialog::openClicked() {
  const QString what = action();
  const QString dir = folder();
  const QString url = m_section->url();
  if (what.isEmpty()) {
    return;
  }
  m_section->stopCheck();
  qInfo().noquote() << QStringLiteral("Open from Cloud: %1 %2 into %3").arg(what, url, dir);
  if (what == QLatin1String("open")) {
    OpenFromCloudResult result;
    result.action = what;
    result.url = url;
    result.root = m_check->kind() == QLatin1String("project")
                      ? dir
                      : QDir::cleanPath(m_check->answer().value(QStringLiteral("project_root")).toString());
    m_result = result;
    QDialog::accept();
    return;
  }
  const QString author = authorOf(m_name, m_email);
  QJsonObject command;
  QString design;
  if (what == QLatin1String("clone")) {
    command = {{QStringLiteral("cmd"), QStringLiteral("clone_project")},
               {QStringLiteral("url"), url},
               {QStringLiteral("dir"), dir}};
  } else if (what == QLatin1String("adopt")) {
    command = {{QStringLiteral("cmd"), QStringLiteral("clone_project")},
               {QStringLiteral("url"), url},
               {QStringLiteral("dir"), dir},
               {QStringLiteral("adopt"), true},
               {QStringLiteral("author"), author}};
  } else {
    design = QFileInfo(dir).fileName() + QStringLiteral(".mitcad");
    command = {{QStringLiteral("cmd"), QStringLiteral("create_project")},
               {QStringLiteral("dir"), dir},
               {QStringLiteral("author"), author},
               {QStringLiteral("url"), url},
               {QStringLiteral("design"),
                QJsonObject{{QStringLiteral("path"), design}, {QStringLiteral("text"), emptyDesignText()}}}};
  }
  const bool bare = what == QLatin1String("create") && m_section->needsBareRepository();
  const QByteArray json = compactJson(command);
  const QByteArray bareJson =
      compactJson({{QStringLiteral("cmd"), QStringLiteral("init_bare")}, {QStringLiteral("dir"), url}});
  RemoteTask* task = RemoteTask::custom(
      command.value(QStringLiteral("cmd")).toString(),
      [json, bareJson, bare](const SyncControl& control) {
        if (bare) {
          const QJsonObject made = parseObject(projects_command(rustStr(bareJson), control));
          if (!errorClassOf(made).isEmpty()) {
            return made;
          }
        }
        return parseObject(projects_command(rustStr(json), control));
      },
      this);
  m_task = task;
  for (QWidget* widget : std::initializer_list<QWidget*>{m_section, m_folder, m_browse, m_name, m_email, m_open}) {
    widget->setEnabled(false);
  }
  m_problem->clear();
  m_progress->follow(task, what == QLatin1String("clone") ? tr("Opening %1...").arg(url)
                                                          : tr("Making the project in %1...").arg(url));
  connect(task, &RemoteTask::finished, this, [this, task, what, design] { finished(task, what, design); });
  task->start();
}

void OpenFromCloudDialog::finished(RemoteTask* task, const QString& action, const QString& design) {
  const QJsonObject answer = task->answer();
  m_task = nullptr;
  task->deleteLater();
  const QString cls = errorClassOf(answer);
  const bool made = cls.isEmpty() || (action != QLatin1String("clone") && projectMade(answer));
  if (made) {
    OpenFromCloudResult result;
    result.action = action;
    result.url = m_section->url();
    result.answer = answer;
    result.design = design;
    result.root = QDir::cleanPath(answer.value(QStringLiteral("root")).toString());
    if (result.root.isEmpty()) {
      result.root = folder();
    }
    m_result = result;
    qInfo().noquote() << QStringLiteral("Open from Cloud: %1 done into %2%3")
                             .arg(action, result.root,
                                  cls.isEmpty() ? QString() : QStringLiteral(", not sent (%1)").arg(cls));
    QDialog::accept();
    return;
  }
  for (QWidget* widget : std::initializer_list<QWidget*>{m_section, m_folder, m_browse, m_name, m_email, m_open}) {
    widget->setEnabled(true);
  }
  m_progress->hide();
  qInfo().noquote() << QStringLiteral("Open from Cloud failed (%1): %2").arg(cls, errorMessageOf(answer));
  m_problem->setText(cls == QLatin1String("cancelled") ? tr("Cancelled: nothing was opened.")
                                                       : errorMessageOf(answer));
  setErrorStyleSheet(m_problem);
  m_check->check(QString());
  m_check->check(folder());
  updateState();
}

} // namespace

std::optional<NewProjectResult> askNewProject(QWidget* parent, const NewProjectOptions& options) {
  NewProjectDialog dialog(parent, options);
  prepareModal(&dialog);
  dialog.exec();
  return dialog.result();
}

std::optional<OpenFromCloudResult> askOpenFromCloud(QWidget* parent, const OpenFromCloudOptions& options) {
  OpenFromCloudDialog dialog(parent, options);
  prepareModal(&dialog);
  dialog.exec();
  return dialog.result();
}

// ---------------------------------------------------------------------------
// The design chooser

std::optional<QString> askChooseDesign(QWidget* parent, const QString& project, const QStringList& designs,
                                       const QString& last, const std::function<QString(const QString&)>& preview) {
  QDialog dialog(parent);
  dialog.setWindowTitle(tr("Open Project"));
  auto* layout = new QVBoxLayout(&dialog);
  layout->addWidget(note(tr("The designs of %1:").arg(project)));
  auto* list = new QListWidget;
  list->setViewMode(QListView::IconMode);
  list->setIconSize(QSize(128, 96));
  list->setGridSize(QSize(176, 140));
  list->setResizeMode(QListView::Adjust);
  list->setMovement(QListView::Static);
  list->setWordWrap(true);
  list->setMinimumSize(560, 320);
  const QIcon placeholder = themeIcon(QStringLiteral("new-project"));
  QListWidgetItem* selected = nullptr;
  for (const QString& design : designs) {
    const QString image = preview ? preview(design) : QString();
    const QPixmap picture = image.isEmpty() ? QPixmap() : QPixmap(image);
    auto* item = new QListWidgetItem(picture.isNull() ? placeholder : QIcon(picture), design, list);
    item->setData(Qt::UserRole, design);
    item->setToolTip(design);
    if (design == last) {
      selected = item;
    }
  }
  if (selected == nullptr && list->count() > 0) {
    selected = list->item(0);
  }
  list->setCurrentItem(selected);
  layout->addWidget(list, 1);
  auto* buttons = new QDialogButtonBox(QDialogButtonBox::Open | QDialogButtonBox::Cancel);
  QPushButton* newDesign = buttons->addButton(tr("&New Design"), QDialogButtonBox::ActionRole);
  newDesign->setAutoDefault(false);
  buttons->button(QDialogButtonBox::Cancel)->setAutoDefault(false);
  layout->addWidget(buttons);
  buttons->button(QDialogButtonBox::Open)->setDefault(true); // once in the dialog
  std::optional<QString> chosen;
  QObject::connect(buttons, &QDialogButtonBox::accepted, &dialog, [&] {
    if (list->currentItem() != nullptr) {
      chosen = list->currentItem()->data(Qt::UserRole).toString();
      dialog.accept();
    }
  });
  QObject::connect(list, &QListWidget::itemActivated, &dialog, [&](QListWidgetItem* item) {
    chosen = item->data(Qt::UserRole).toString();
    dialog.accept();
  });
  QObject::connect(newDesign, &QPushButton::clicked, &dialog, [&] {
    chosen = QString();
    dialog.accept();
  });
  QObject::connect(buttons, &QDialogButtonBox::rejected, &dialog, &QDialog::reject);
  QObject::connect(list, &QListWidget::currentItemChanged, &dialog, [](QListWidgetItem* item) {
    if (item != nullptr) {
      qInfo().noquote() << QStringLiteral("Design chooser selected %1").arg(item->data(Qt::UserRole).toString());
    }
  });
  list->setFocus();
  qInfo().noquote() << QStringLiteral("Design chooser: %1: %2 (selected %3)")
                           .arg(project, designs.join(QStringLiteral(" | ")),
                                selected != nullptr ? selected->data(Qt::UserRole).toString() : QString());
  prepareModal(&dialog);
  if (dialog.exec() != QDialog::Accepted || !chosen) {
    qInfo().noquote() << QStringLiteral("Design chooser cancelled");
    return std::nullopt;
  }
  qInfo().noquote() << (chosen->isEmpty() ? QStringLiteral("Design chooser: new design")
                                          : QStringLiteral("Design chooser: chose %1").arg(*chosen));
  return chosen;
}

// ---------------------------------------------------------------------------
// Move to a Project

std::optional<MoveTarget> askMoveToProject(QWidget* parent, const QString& design, const QString& reason) {
  QMessageBox box(QMessageBox::Question, tr("Move to a Project"),
                  tr("Move %1 into a project?").arg(design), QMessageBox::Cancel, parent);
  box.setInformativeText(reason.isEmpty() ? tr("A project keeps the versions of its designs: every save records "
                                               "one. The design moves into a new project or one you have.")
                                          : reason);
  QPushButton* newProject = box.addButton(tr("&New Project..."), QMessageBox::AcceptRole);
  QPushButton* existing = box.addButton(tr("&Existing Project..."), QMessageBox::AcceptRole);
  box.setDefaultButton(newProject);
  qInfo().noquote() << QStringLiteral("Move to a Project dialog: %1").arg(design);
  prepareModal(&box);
  box.exec();
  if (box.clickedButton() == newProject) {
    return MoveTarget::NewProject;
  }
  if (box.clickedButton() == existing) {
    return MoveTarget::ExistingProject;
  }
  qInfo().noquote() << QStringLiteral("Move to a Project cancelled");
  return std::nullopt;
}

// ---------------------------------------------------------------------------
// Change... of the address

std::optional<QString> askChangeAddress(QWidget* parent, const QString& root, const QString& current) {
  QDialog dialog(parent);
  dialog.setWindowTitle(tr("Change Address"));
  auto* layout = new QVBoxLayout(&dialog);
  layout->addWidget(note(tr("The new address of the project's repository, when it moved or was renamed. Only the "
                            "same repository is taken: one that holds the project's history.")));
  const auto checker = [root](const QString& url, QObject* owner) {
    return RemoteTask::command(root, {{QStringLiteral("cmd"), QStringLiteral("remote_check")}, {QStringLiteral("url"), url}},
                               owner);
  };
  const auto describe = [current](const QJsonObject& answer, bool& ok) -> QString {
    if (answer.value(QStringLiteral("new_folder")).toBool() || answer.value(QStringLiteral("empty")).toBool()) {
      ok = false;
      return tr("This repository is empty, not the project's: Change... only takes a new address of the same "
                "repository.");
    }
    if (answer.value(QStringLiteral("related")).toBool()) {
      return tr("✓ The same repository: it holds the project's history.");
    }
    ok = false;
    return tr("This repository has another history: Change... only takes a new address of the same repository. "
              "Choose an empty repository with Stop Syncing and Share, or open that project with Open from Cloud.");
  };
  auto* section = new CloudSection(checker, describe);
  layout->addWidget(section);
  auto* buttons = new QDialogButtonBox(QDialogButtonBox::Ok | QDialogButtonBox::Cancel);
  buttons->button(QDialogButtonBox::Ok)->setText(tr("&Change"));
  buttons->button(QDialogButtonBox::Ok)->setEnabled(false);
  layout->addWidget(buttons);
  section->setAddress(current);
  const auto update = [&] {
    buttons->button(QDialogButtonBox::Ok)
        ->setEnabled(section->hasAnswer() && section->answerOk() && !sameRepository(section->url(), current));
  };
  QObject::connect(section, &CloudSection::addressChanged, &dialog, update);
  QObject::connect(section, &CloudSection::checked, &dialog, [&] {
    update();
    if (!section->answerOk()) {
      qInfo().noquote() << QStringLiteral("Change Address refused: %1").arg(section->statusText());
    }
  });
  QObject::connect(buttons, &QDialogButtonBox::accepted, &dialog, &QDialog::accept);
  QObject::connect(buttons, &QDialogButtonBox::rejected, &dialog, &QDialog::reject);
  if (QWidget* first = section->firstField()) {
    first->setFocus();
  }
  qInfo().noquote() << QStringLiteral("Change Address dialog: %1").arg(current);
  prepareModal(&dialog);
  const bool accepted = dialog.exec() == QDialog::Accepted;
  section->stopCheck();
  if (!accepted) {
    qInfo().noquote() << QStringLiteral("Change Address cancelled");
    return std::nullopt;
  }
  return section->url();
}

} // namespace mitcad
