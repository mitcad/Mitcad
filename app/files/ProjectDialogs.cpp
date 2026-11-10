// SPDX-License-Identifier: MIT
#include "ProjectDialogs.hpp"

#include <memory>
#include <utility>

#include <QButtonGroup>
#include <QCheckBox>
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
#include <QJsonArray>
#include <QLabel>
#include <QLineEdit>
#include <QMessageBox>
#include <QPointer>
#include <QProgressBar>
#include <QPushButton>
#include <QRadioButton>
#include <QRegularExpression>
#include <QScreen>
#include <QScrollArea>
#include <QScrollBar>
#include <QShowEvent>
#include <QTimer>
#include <QVBoxLayout>
#include <QtLogging>

#include "../framework/AppSettings.hpp"
#include "../framework/Dialogs.hpp"
#include "../framework/FitToScreen.hpp"
#include "../framework/Json.hpp"
#include "../framework/TestSync.hpp"
#include "../framework/Theme.hpp"
#include "../platform/MacDialogs.hpp"
#include "CloudSection.hpp"
#include "ProjectSettingsFields.hpp"
#include "Projects.hpp"
#include "RemoteTask.hpp"
#include "mitcad_bridge/lib.h"

namespace mitcad {
namespace {

// The file's texts in the context ProjectDialogs, where lupdate records them
// too (mitcad#1472).
struct Text {
  Q_DECLARE_TR_FUNCTIONS(ProjectDialogs)
};

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

// The author offered: git's configured one, else MitCad's default author.
void fillAuthor(QLineEdit* name, QLineEdit* email) {
  const GitIdentity git = gitIdentity();
  const VersionSettings settings = VersionSettings::load();
  name->setText(!git.name.isEmpty() ? git.name : settings.name);
  email->setText(!git.email.isEmpty() ? git.email : settings.email);
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
      out << QStringLiteral("…");
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
    return Text::tr("✓ An empty repository is made in this folder: ready.");
  }
  if (answer.value(QStringLiteral("has_project")).toBool()) {
    ok = false;
    return Text::tr("This repository holds a MitCad project: Open It Instead opens it into the folder.");
  }
  if (!answer.value(QStringLiteral("reachable")).toBool(true)) {
    ok = false;
    return Text::tr("The repository cannot be reached.");
  }
  if (answer.value(QStringLiteral("empty")).toBool()) {
    return Text::tr("✓ Reachable and empty: ready.");
  }
  return Text::tr("✓ MitCad adds the project beside the repository's files (%1).")
      .arg(names(answer.value(QStringLiteral("files")), 5).join(QStringLiteral(", ")));
}

// What a check of a remote to open found (Open from Cloud).
QString describeForOpening(const QJsonObject& answer, bool& ok) {
  if (answer.value(QStringLiteral("new_folder")).toBool()) {
    return Text::tr("An empty repository is made in this folder: Create Project Here makes a Cloud project in it.");
  }
  if (!answer.value(QStringLiteral("reachable")).toBool(true)) {
    ok = false;
    return Text::tr("The repository cannot be reached.");
  }
  if (answer.value(QStringLiteral("has_project")).toBool()) {
    const QString name = repositoryNameOf(answer.value(QStringLiteral("url")).toString());
    const int versions = answer.value(QStringLiteral("versions")).toInt();
    const QJsonObject latest = answer.value(QStringLiteral("latest")).toObject();
    QString text = Text::tr("✓ Project \"%1\": %n version(s)", nullptr, versions).arg(name);
    if (!latest.isEmpty()) {
      const QDateTime when = QDateTime::fromSecsSinceEpoch(latest.value(QStringLiteral("time")).toInteger());
      text += Text::tr(", latest %1 by %2")
                  .arg(when.toLocalTime().toString(QStringLiteral("yyyy-MM-dd")),
                       latest.value(QStringLiteral("author")).toString());
    }
    return text;
  }
  if (answer.value(QStringLiteral("empty")).toBool()) {
    return Text::tr("The repository is empty: Create Project Here makes a Cloud project in it.");
  }
  return Text::tr("The repository has files but no MitCad project (%1): Make It a Project adds one beside them.")
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
  auto* label = new QLabel(Text::tr("Cloud projects need the installed git program, which was not found: "
                              "<a href=\"https://git-scm.com/downloads\">install git</a> and start MitCad again. "
                              "Component libraries over https:// work without it."));
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
  void showEvent(QShowEvent* event) override;
  void reject() override;

private:
  QString folder() const { return cleanFolder(m_folder->text()); }
  // The dialog as tall as its contents, within the screen (mitcad#631).
  void fitToContents(FitMode mode = FitMode::Fit);
  // Logs the layout once it has settled, for the UI tests.
  void logLayout();
  bool cloud() const { return m_cloudChoice->isChecked(); }
  void folderChanged();
  void folderChecked();
  void updateState();
  void createClicked();
  // `into`: the project's folder, else the Folder field's.
  void create(bool asCloud, const QString& into = QString());
  void finished(RemoteTask* task, bool asCloud);
  void setBusy(bool busy);
  // The folder holds files or folders (mitcad#1562): all of it becomes the
  // project, unless it is made in a new subfolder.
  bool holdsFiles() const;
  // The new subfolder's path, or "" while its name is no folder name (with
  // why in `why`).
  QString subfolder(QString* why = nullptr) const;

  std::optional<NewProjectResult> m_result;
  std::unique_ptr<FolderCheck> m_check;
  QScrollArea* m_scroll = nullptr;
  QLineEdit* m_folder = nullptr;
  QPushButton* m_browse = nullptr;
  QRadioButton* m_local = nullptr;
  QRadioButton* m_cloudChoice = nullptr;
  QLineEdit* m_name = nullptr;
  QLineEdit* m_email = nullptr;
  QLabel* m_authorNote = nullptr;
  QGroupBox* m_cloudBox = nullptr;
  CloudSection* m_section = nullptr;
  ProjectSettingsFields* m_fields = nullptr;
  QLabel* m_folderNote = nullptr;
  QLabel* m_problem = nullptr;
  ProgressRow* m_progress = nullptr;
  QDialogButtonBox* m_buttons = nullptr;
  QPushButton* m_create = nullptr;
  QPushButton* m_openInstead = nullptr;
  QPushButton* m_localForNow = nullptr;
  // Create in a New Subfolder (mitcad#1562) and the subfolder's name.
  QWidget* m_subfolderRow = nullptr;
  QLineEdit* m_subfolder = nullptr;
  QPushButton* m_inSubfolder = nullptr;
  QPointer<RemoteTask> m_task;
  QString m_loggedFolder;
  bool m_gitMissing = false;
  bool m_folderFixed = false;
};

NewProjectDialog::NewProjectDialog(QWidget* parent, const NewProjectOptions& options)
    : QDialog(parent) {
  setWindowTitle(options.title.isEmpty() ? Text::tr("New Project") : options.title);
  auto* outer = new QVBoxLayout(this);
  // As tall as its contents (no fixed maximum): the dialog opens at its
  // full height, and the scroll bar is for a screen too low for it.
  auto* scroll = new FittingScrollArea;
  // The AppKit secondary window already scrolls the full form above its
  // buttons, as this fitting area does in Qt; do not nest another scroller.
  scroll->setProperty("nativeWindowBody", true);
  m_scroll = scroll;
  scroll->setWidgetResizable(true);
  scroll->setFrameShape(QFrame::NoFrame);
  scroll->setMinimumSize(620, 240);
  auto* contents = new QWidget;
  auto* layout = new QVBoxLayout(contents);
  scroll->setWidget(contents);
  outer->addWidget(scroll);
  layout->addWidget(note(Text::tr("A project is a folder whose designs keep their versions: Local on this computer, or "
                            "Cloud in a git repository that others share.")));
  auto* form = new QFormLayout;
  m_folder = new QLineEdit(QDir::toNativeSeparators(options.folder.isEmpty() ? freeProjectFolder() : options.folder));
  m_folder->setObjectName(QStringLiteral("projectFolder"));
  m_folder->setMinimumWidth(420);
  m_browse = new QPushButton(Text::tr("Brow&se…"));
  m_browse->setAutoDefault(false);
  auto* folderRow = new QHBoxLayout;
  folderRow->addWidget(m_folder, 1);
  folderRow->addWidget(m_browse);
  auto* folderLabel = new QLabel(Text::tr("&Folder:"));
  folderLabel->setBuddy(m_folder);
  form->addRow(folderLabel, folderRow);
  if (options.folderFixed) {
    m_folder->setReadOnly(true);
    m_browse->hide();
  }
  m_local = new QRadioButton(Text::tr("&Local – versions on this computer"));
  m_cloudChoice = new QRadioButton(Text::tr("Cl&oud (git) – versions also in a git repository, shared"));
  m_local->setObjectName(QStringLiteral("projectStorageLocal"));
  m_cloudChoice->setObjectName(QStringLiteral("projectStorageCloud"));
  auto* storage = new QButtonGroup(this);
  storage->addButton(m_local);
  storage->addButton(m_cloudChoice);
  auto* storageColumn = new QVBoxLayout;
  storageColumn->addWidget(m_local);
  storageColumn->addWidget(m_cloudChoice);
  form->addRow(Text::tr("Storage:"), storageColumn);
  layout->addLayout(form);
  m_cloudBox = new QGroupBox(Text::tr("Cloud"));
  mac::styleSection(m_cloudBox);
  auto* cloudColumn = new QVBoxLayout(m_cloudBox);
  m_section = new CloudSection(checkRemote, describeForNewProject);
  cloudColumn->addWidget(m_section);
  layout->addWidget(m_cloudBox);
  m_fields = new ProjectSettingsFields(ProjectSettingsFields::Host::NewProject);
  m_name = m_fields->m_name;
  m_email = m_fields->m_email;
  layout->addWidget(m_fields);
  m_authorNote = note(Text::tr("With Cloud, the name and email go into every version: anyone who can read the repository "
                         "sees them."));
  setWarningStyleSheet(m_authorNote);
  layout->addWidget(m_authorNote);
  m_folderNote = note();
  layout->addWidget(m_folderNote);
  // A folder with files in it (mitcad#1562): the project can go into a new
  // subfolder of it instead, named here.
  m_folderFixed = options.folderFixed;
  m_subfolderRow = new QWidget;
  auto* subfolderLayout = new QHBoxLayout(m_subfolderRow);
  subfolderLayout->setContentsMargins(0, 0, 0, 0);
  auto* subfolderLabel = new QLabel(Text::tr("New subfolder:"));
  m_subfolder = new QLineEdit(Text::tr("New Project"));
  m_subfolder->setObjectName(QStringLiteral("newProjectSubfolder"));
  m_subfolder->setToolTip(Text::tr("The name of the folder Create in a New Subfolder makes for the project"));
  subfolderLabel->setBuddy(m_subfolder);
  subfolderLayout->addWidget(subfolderLabel);
  subfolderLayout->addWidget(m_subfolder, 1);
  layout->addWidget(m_subfolderRow);
  m_subfolderRow->hide();
  const GitState git = gitState();
  m_gitMissing = git.missing;
  if (git.missing) {
    m_cloudChoice->setEnabled(false);
    layout->addWidget(gitMissingNote());
  }
  m_section->setDefaultConnect(git.credentialHelper ? CloudConnect::Https : CloudConnect::Ssh);
  m_problem = note();
  setErrorStyleSheet(m_problem);
  outer->addWidget(m_problem);
  m_progress = new ProgressRow;
  outer->addWidget(m_progress);
  m_buttons = new QDialogButtonBox(QDialogButtonBox::Cancel);
  m_openInstead = m_buttons->addButton(Text::tr("Open It &Instead"), QDialogButtonBox::ActionRole);
  m_localForNow = m_buttons->addButton(Text::tr("Create as Local for &Now"), QDialogButtonBox::ActionRole);
  m_inSubfolder = m_buttons->addButton(Text::tr("Create in a New Subfolder"), QDialogButtonBox::ActionRole);
  m_inSubfolder->setObjectName(QStringLiteral("newProjectInSubfolder"));
  m_create = m_buttons->addButton(Text::tr("&Create"), QDialogButtonBox::AcceptRole);
  for (QPushButton* button : {m_openInstead, m_localForNow, m_inSubfolder}) {
    button->setAutoDefault(false);
    button->hide();
  }
  m_buttons->button(QDialogButtonBox::Cancel)->setAutoDefault(false);
  outer->addWidget(m_buttons);
  // Enter creates, also while Create waits for a check (the button is the
  // dialog's default only once it is in the dialog).
  mac::styleDialogButtons(m_buttons, m_create, true);
  mac::styleDialog(this);

  m_check = std::make_unique<FolderCheck>(this, [this] { folderChecked(); });
  const bool preset = (options.cloud || !options.url.isEmpty()) && !git.missing;
  m_cloudChoice->setChecked(preset);
  m_local->setChecked(!preset);
  m_cloudBox->setVisible(preset);
  m_fields->m_everyone->setVisible(preset);
  m_authorNote->setVisible(preset);
  fillAuthor(m_name, m_email);
  m_section->setFolderName(QFileInfo(folder()).fileName());
  if (!options.url.isEmpty()) {
    m_section->setAddress(options.url);
  }

  connect(m_folder, &QLineEdit::textChanged, this, [this] { folderChanged(); });
  connect(m_browse, &QPushButton::clicked, this, [this] {
    const QString chosen =
        dialogDirectory(this, Text::tr("Project Folder"), QFileInfo(folder()).absolutePath());
    if (!chosen.isEmpty()) {
      m_folder->setText(QDir::toNativeSeparators(chosen));
    }
  });
  connect(m_cloudChoice, &QRadioButton::toggled, this, [this](bool on) {
    m_cloudBox->setVisible(on);
    m_fields->m_everyone->setVisible(on);
    m_authorNote->setVisible(on);
    qInfo().noquote() << QStringLiteral("New Project dialog: storage %1")
                             .arg(on ? QStringLiteral("cloud") : QStringLiteral("local"));
    if (on && !m_section->url().isEmpty()) {
      m_section->recheck();
    }
    updateState();
    fitToContents();
    logLayout();
  });
  for (QLineEdit* edit : {m_name, m_email}) {
    connect(edit, &QLineEdit::textChanged, this, [this] { updateState(); });
  }
  connect(m_section, &CloudSection::addressChanged, this, [this] { updateState(); });
  connect(m_section, &CloudSection::checked, this, [this] { updateState(); });
  // The Details opened or closed: the section already fitted the window.
  connect(m_section, &CloudSection::detailsToggled, this, [this] { logLayout(); });
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
  connect(m_subfolder, &QLineEdit::textChanged, this, [this] { updateState(); });
  connect(m_inSubfolder, &QPushButton::clicked, this, [this] {
    QString why;
    const QString dir = subfolder(&why);
    if (dir.isEmpty()) {
      m_problem->setText(why);
      return;
    }
    qInfo().noquote() << QStringLiteral("New Project: Create in a New Subfolder %1").arg(dir);
    create(cloud(), dir);
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
  // The size before it is shown, where it is centred over the window.
  fitToContents();
}

void NewProjectDialog::fitToContents(FitMode mode) { fitWindowToContents(this, mode); }

void NewProjectDialog::showEvent(QShowEvent* event) {
  QDialog::showEvent(event);
  // The window frame is known now: its real size is taken off the screen.
  fitToContents();
  logLayout();
}

void NewProjectDialog::logLayout() {
  // After the resize and the scroll area's layout have been handled.
  QTimer::singleShot(50, this, [this] {
    if (!isVisible()) {
      return;
    }
    QCoreApplication::sendPostedEvents(nullptr, QEvent::LayoutRequest);
    const QScreen* screenNow = screen();
    const int room = screenNow != nullptr ? screenNow->availableGeometry().height() : 0;
    qInfo().noquote() << QStringLiteral("New Project dialog: layout storage %1, details %2, vertical scroll bar %3 "
                                        "(dialog height %4, screen room %5)")
                             .arg(cloud() ? QStringLiteral("cloud") : QStringLiteral("local"),
                                  m_section->detailsOpen() ? QStringLiteral("open") : QStringLiteral("closed"),
                                  m_scroll->verticalScrollBar()->isVisible() ? QStringLiteral("shown")
                                                                             : QStringLiteral("hidden"))
                             .arg(height())
                             .arg(room);
  });
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
    text = Text::tr("%1 is a MitCad project already: Open It opens it.").arg(dir);
  } else if (kind == QLatin1String("inside_project")) {
    text = Text::tr("%1 is inside the project %2: Open It opens that project.")
               .arg(dir, QDir::toNativeSeparators(answer.value(QStringLiteral("project_root")).toString()));
  } else if (kind == QLatin1String("repository")) {
    text = Text::tr("%1 is a git repository: the project goes into it.").arg(dir);
  }
  // A folder with files in it (mitcad#1562): all of it, its subfolders too,
  // becomes the project, which a user who expected a new folder must know.
  const bool holds = holdsFiles();
  const int designs = static_cast<int>(answer.value(QStringLiteral("designs")).toArray().size());
  if (holds) {
    QString whole;
    if (answer.value(QStringLiteral("designs_truncated")).toBool()) {
      whole = Text::tr("%1 and everything below it, its subfolders too, become the project: more than %n "
                       "design(s) go into its first version.",
                       nullptr, designs)
                  .arg(dir);
    } else if (designs > 0) {
      whole = Text::tr("%1 and everything below it, its subfolders too, become the project: %n design(s) go "
                       "into its first version.",
                       nullptr, designs)
                  .arg(dir);
    } else {
      whole = Text::tr("%1 and everything below it, its subfolders too, become the project.").arg(dir);
    }
    text += (text.isEmpty() ? QString() : QStringLiteral(" ")) + whole + QStringLiteral(" ") +
            Text::tr("Other files stay where they are, not versioned.");
    if (!m_folderFixed) {
      text += QStringLiteral(" ") + Text::tr("Create in a New Subfolder makes the project in a new folder inside it "
                                             "instead.");
    }
  }
  const QString outer = answer.value(QStringLiteral("outer_repository")).toString();
  if (!outer.isEmpty() && kind != QLatin1String("project") && kind != QLatin1String("inside_project")) {
    text += (text.isEmpty() ? QString() : QStringLiteral(" ")) +
            Text::tr("It is inside the git repository %1: the project gets a repository of its own, which the other one "
               "does not record.")
                .arg(QDir::toNativeSeparators(outer));
  }
  m_folderNote->setText(text);
  if (!outer.isEmpty() || holds) {
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
    if (holds) {
      qInfo().noquote() << QStringLiteral("New Project dialog: %1 holds files: it becomes the project with its "
                                          "subfolders, %2 designs%3")
                               .arg(folder())
                               .arg(designs)
                               .arg(m_folderFixed ? QString() : QStringLiteral(", Create in a New Subfolder offered"));
    }
  }
  updateState();
}

bool NewProjectDialog::holdsFiles() const {
  if (m_check->folder() != folder()) {
    return false;
  }
  const QString kind = m_check->kind();
  return kind == QLatin1String("designs") || kind == QLatin1String("other") || kind == QLatin1String("repository");
}

QString NewProjectDialog::subfolder(QString* why) const {
  const QString name = m_subfolder->text().trimmed();
  QString problem;
  if (name.isEmpty()) {
    problem = Text::tr("The new subfolder needs a name.");
  } else if (name == QLatin1String(".") || name == QLatin1String("..") || name.startsWith(QLatin1Char('.')) ||
             name.contains(QLatin1Char('/')) || name.contains(QLatin1Char('\\')) ||
             name.contains(QRegularExpression(QStringLiteral("[<>:\"|?*\\x00-\\x1f]")))) {
    problem = Text::tr("%1 is no folder name: leave out / \\ : * ? \" < > | and a dot at the start.").arg(name);
  } else if (QFileInfo::exists(QDir(folder()).filePath(name))) {
    problem = Text::tr("%1 is in the folder already: choose another name.").arg(name);
  }
  if (why != nullptr) {
    *why = problem;
  }
  return problem.isEmpty() ? QDir::cleanPath(QDir(folder()).filePath(name)) : QString();
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
    why = Text::tr("The folder is a full path.");
    can = false;
  } else if (QFileInfo(dir).exists() && !QFileInfo(dir).isDir()) {
    why = Text::tr("%1 is a file.").arg(QDir::toNativeSeparators(dir));
    can = false;
  } else if (m_check->pending() || kind.isEmpty()) {
    can = false; // the folder is looked at
  } else if (!opens) {
    if (authorOf(m_name, m_email).isEmpty()) {
      why = Text::tr("Versions need an author: a name and an email address.");
      can = false;
    } else if (cloud()) {
      can = !m_section->url().isEmpty() && m_section->hasAnswer() && m_section->answerOk();
    }
  }
  m_create->setText(opens ? Text::tr("Open &It") : Text::tr("&Create"));
  m_create->setEnabled(can);
  // Create in a New Subfolder (mitcad#1562) beside Create.
  const bool offer = !opens && !m_folderFixed && holdsFiles();
  QString unnamed;
  const bool named = offer && !subfolder(&unnamed).isEmpty();
  m_subfolderRow->setVisible(offer);
  m_inSubfolder->setVisible(offer);
  m_inSubfolder->setEnabled(can && named);
  if (offer && can && !named) {
    why = unnamed;
  }
  m_problem->setText(why);
  const QString cls = errorClassOf(m_section->answer());
  m_openInstead->setVisible(cloud() && !opens && m_section->answer().value(QStringLiteral("has_project")).toBool());
  m_localForNow->setVisible(cloud() && !opens &&
                            (cls == QLatin1String("network") || cls == QLatin1String("timed_out")));
  m_localForNow->setEnabled(!authorOf(m_name, m_email).isEmpty());
  if (isVisible()) {
    // A longer note or message: the window grows with it instead of the
    // contents scrolling.
    fitToContents(FitMode::GrowOnly);
  }
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
                                                         m_section, m_fields, m_create, m_openInstead,
                                                         m_localForNow, m_subfolder, m_inSubfolder}) {
    widget->setEnabled(!busy);
  }
  if (!busy) {
    m_progress->hide();
    m_cloudChoice->setEnabled(!m_gitMissing);
  }
}

void NewProjectDialog::create(bool asCloud, const QString& into) {
  const QString dir = into.isEmpty() ? folder() : into;
  const QString author = authorOf(m_name, m_email);
  QJsonObject command{{QStringLiteral("cmd"), QStringLiteral("create_project")},
                      {QStringLiteral("dir"), dir},
                      {QStringLiteral("author"), author},
                      {QStringLiteral("include_designs"), true},
                      {QStringLiteral("local"), m_fields->localChosen(true)}};
  // No first design (mitcad#145): the project starts empty, or with the
  // designs its folder has; New Design in the Projects panel adds one.
  const QString url = asCloud ? m_section->url() : QString();
  const bool bare = asCloud && m_section->needsBareRepository();
  if (asCloud) {
    command.insert(QStringLiteral("url"), url);
    command.insert(QStringLiteral("shared"), m_fields->sharedChosen());
    command.insert(QStringLiteral("push"), m_fields->m_sendAtOnce->isChecked());
  }
  // Validate through the core before even making a shared-folder remote:
  // invalid broker/prefix or local settings must leave no repository behind.
  QJsonObject validation = command;
  validation.insert(QStringLiteral("cmd"), QStringLiteral("validate_create_project"));
  const QJsonObject checked = projectsCommand(validation);
  if (!errorClassOf(checked).isEmpty()) {
    m_problem->setText(errorMessageOf(checked));
    return;
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
  m_progress->follow(task, asCloud ? Text::tr("Creating the project and sending it to %1…").arg(url)
                                   : Text::tr("Creating the project…"));
  // The progress row is shown: room for it, so that the contents do not
  // start to scroll.
  fitToContents(FitMode::GrowOnly);
  connect(task, &RemoteTask::finished, this, [this, task, asCloud] { finished(task, asCloud); });
  qInfo().noquote() << QStringLiteral("New Project: creating %1 (%2)")
                           .arg(dir, asCloud ? QStringLiteral("cloud %1").arg(url) : QStringLiteral("local"));
  task->start();
}

void NewProjectDialog::finished(RemoteTask* task, bool asCloud) {
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
    const QString liveHere = m_fields->m_liveHere->currentData().toString();
    if (asCloud && liveHere != QLatin1String("off")) {
      result.broker = liveHere == QLatin1String("broker") ? m_fields->m_hereBroker->text().trimmed()
                      : m_fields->m_liveBroker->isChecked() ? m_fields->m_broker->text().trimmed() : QString();
    }
    m_result = result;
    const QString cls = errorClassOf(answer);
    qInfo().noquote() << QStringLiteral("New Project: created %1: version %2 on %3%4")
                             .arg(result.folder, answer.value(QStringLiteral("commit")).toString().left(7),
                                  answer.value(QStringLiteral("branch")).toString(),
                                  cls.isEmpty() ? (asCloud && m_fields->m_sendAtOnce->isChecked()
                                                        ? QStringLiteral(", sent") : QString())
                                                : QStringLiteral(", not sent (%1)").arg(cls));
    QDialog::accept();
    return;
  }
  setBusy(false);
  const QString cls = errorClassOf(answer);
  qInfo().noquote() << QStringLiteral("New Project failed (%1): %2").arg(cls, errorMessageOf(answer));
  updateState();
  if (cls == QLatin1String("cancelled")) {
    m_problem->setText(Text::tr("Canceled: the folder is as it was."));
  } else {
    m_problem->setText(errorMessageOf(answer).isEmpty() ? Text::tr("The project could not be made.")
                                                        : errorMessageOf(answer));
  }
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
  void finished(RemoteTask* task, const QString& action);

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
  setWindowTitle(Text::tr("Open from Cloud"));
  auto* layout = new QVBoxLayout(this);
  layout->addWidget(note(Text::tr("Opens a Cloud project from its git repository into a new folder on this computer; each "
                            "version saved there is sent back. Signing in is git's: an SSH key or a credential "
                            "helper.")));
  m_section = new CloudSection(checkRemote, describeForOpening);
  layout->addWidget(m_section);
  auto* form = new QFormLayout;
  m_folder = new QLineEdit(QDir::toNativeSeparators(options.folder));
  m_folder->setMinimumWidth(420);
  m_browse = new QPushButton(Text::tr("Brow&se…"));
  m_browse->setAutoDefault(false);
  auto* folderRow = new QHBoxLayout;
  folderRow->addWidget(m_folder, 1);
  folderRow->addWidget(m_browse);
  auto* folderLabel = new QLabel(Text::tr("&Folder:"));
  folderLabel->setBuddy(m_folder);
  form->addRow(folderLabel, folderRow);
  m_name = new QLineEdit;
  m_name->setPlaceholderText(Text::tr("Your Name"));
  m_email = new QLineEdit;
  m_email->setPlaceholderText(Text::tr("you@example.com"));
  m_authorRow = new QWidget;
  auto* authorLayout = new QHBoxLayout(m_authorRow);
  authorLayout->setContentsMargins(0, 0, 0, 0);
  authorLayout->addWidget(m_name, 1);
  authorLayout->addWidget(m_email, 1);
  m_authorLabel = new QLabel(Text::tr("Au&thor:"));
  m_authorLabel->setBuddy(m_name);
  form->addRow(m_authorLabel, m_authorRow);
  layout->addLayout(form);
  m_problem = note();
  layout->addWidget(m_problem);
  m_progress = new ProgressRow;
  layout->addWidget(m_progress);
  auto* buttons = new QDialogButtonBox(QDialogButtonBox::Cancel);
  m_open = buttons->addButton(Text::tr("&Open"), QDialogButtonBox::AcceptRole);
  buttons->button(QDialogButtonBox::Cancel)->setAutoDefault(false);
  layout->addWidget(buttons);
  mac::styleDialogButtons(buttons, m_open, true);
  mac::styleDialog(this);
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
    const QString chosen = dialogDirectory(this, Text::tr("Location"), m_location);
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
    why = Text::tr("The folder is a full path.");
    can = false;
  } else if (m_check->pending() || kind.isEmpty()) {
    can = false;
  } else if (what != QLatin1String("open") && kind != QLatin1String("missing") && kind != QLatin1String("empty")) {
    why = kind == QLatin1String("project") || kind == QLatin1String("inside_project")
              ? Text::tr("%1 is another project: choose a new or empty folder.").arg(QDir::toNativeSeparators(dir))
              : Text::tr("%1 is not empty: choose a new or empty folder.").arg(QDir::toNativeSeparators(dir));
    can = false;
  } else if ((what == QLatin1String("create") || what == QLatin1String("adopt")) &&
             authorOf(m_name, m_email).isEmpty()) {
    why = Text::tr("Making a project needs its author: a name and an email address.");
    can = false;
  }
  QString text = Text::tr("&Open");
  if (what == QLatin1String("create")) {
    text = Text::tr("&Create Project Here");
  } else if (what == QLatin1String("adopt")) {
    text = Text::tr("&Make It a Project");
  } else if (what == QLatin1String("open")) {
    text = Text::tr("&Open It");
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
    // Create Project Here: an empty project (mitcad#145).
    command = {{QStringLiteral("cmd"), QStringLiteral("create_project")},
               {QStringLiteral("dir"), dir},
               {QStringLiteral("author"), author},
               {QStringLiteral("url"), url}};
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
  m_progress->follow(task, what == QLatin1String("clone") ? Text::tr("Opening %1…").arg(url)
                                                          : Text::tr("Making the project in %1…").arg(url));
  connect(task, &RemoteTask::finished, this, [this, task, what] { finished(task, what); });
  task->start();
}

void OpenFromCloudDialog::finished(RemoteTask* task, const QString& action) {
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
  m_problem->setText(cls == QLatin1String("cancelled") ? Text::tr("Canceled: nothing was opened.")
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
  execDialog(&dialog);
  return dialog.result();
}

std::optional<OpenFromCloudResult> askOpenFromCloud(QWidget* parent, const OpenFromCloudOptions& options) {
  OpenFromCloudDialog dialog(parent, options);
  prepareModal(&dialog);
  execDialog(&dialog);
  return dialog.result();
}

// ---------------------------------------------------------------------------
// Change... of the address

std::optional<QString> askChangeAddress(QWidget* parent, const QString& root, const QString& current) {
  QDialog dialog(parent);
  dialog.setWindowTitle(Text::tr("Change Address"));
  auto* layout = new QVBoxLayout(&dialog);
  layout->addWidget(note(Text::tr("The new address of the project's repository, when it moved or was renamed. Only the "
                            "same repository is taken: one that holds the project's history.")));
  const auto checker = [root](const QString& url, QObject* owner) {
    return RemoteTask::command(root, {{QStringLiteral("cmd"), QStringLiteral("remote_check")}, {QStringLiteral("url"), url}},
                               owner);
  };
  const auto describe = [current](const QJsonObject& answer, bool& ok) -> QString {
    if (answer.value(QStringLiteral("new_folder")).toBool() || answer.value(QStringLiteral("empty")).toBool()) {
      ok = false;
      return Text::tr("This repository is empty, not the project's: Change… only takes a new address of the same "
                "repository.");
    }
    if (answer.value(QStringLiteral("related")).toBool()) {
      return Text::tr("✓ The same repository: it holds the project's history.");
    }
    ok = false;
    return Text::tr("This repository has another history: Change… only takes a new address of the same repository. "
              "Choose an empty repository with Stop Syncing and Share, or open that project with Open from Cloud.");
  };
  auto* section = new CloudSection(checker, describe);
  layout->addWidget(section);
  auto* buttons = new QDialogButtonBox(QDialogButtonBox::Ok | QDialogButtonBox::Cancel);
  buttons->button(QDialogButtonBox::Ok)->setText(Text::tr("&Change"));
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
  const bool accepted = execDialog(&dialog) == QDialog::Accepted;
  section->stopCheck();
  if (!accepted) {
    qInfo().noquote() << QStringLiteral("Change Address cancelled");
    return std::nullopt;
  }
  return section->url();
}

} // namespace mitcad
