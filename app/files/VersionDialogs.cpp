// SPDX-License-Identifier: MIT
#include "VersionDialogs.hpp"

#include <QCheckBox>
#include <QDialog>
#include <QDialogButtonBox>
#include <QDir>
#include <QFileDialog>
#include <QFileInfo>
#include <QFormLayout>
#include <QHBoxLayout>
#include <QLabel>
#include <QLineEdit>
#include <QMessageBox>
#include <QPlainTextEdit>
#include <QPushButton>
#include <QShortcut>
#include <QVBoxLayout>
#include <QtLogging>

#include "../framework/AppSettings.hpp"
#include "../framework/Dialogs.hpp"
#include "../framework/Theme.hpp"

namespace mitcad {
namespace {

// A summary line git and the history list show whole.
constexpr int kSummaryLength = 72;

QLabel* note(const QString& text) {
  auto* label = new QLabel(text);
  label->setWordWrap(true);
  return label;
}

// A folder a new project can take: one that is not there yet, or empty.
bool usableFolder(const QString& dir) {
  const QDir folder(dir);
  return !folder.exists() || folder.isEmpty(QDir::AllEntries | QDir::NoDotAndDotDot | QDir::Hidden | QDir::System);
}

} // namespace

std::optional<QString> askNewProject(QWidget* parent, const QString& title, const QString& name,
                                     const QString& location) {
  QDialog dialog(parent);
  dialog.setWindowTitle(title);
  auto* layout = new QVBoxLayout(&dialog);
  layout->addWidget(note(QObject::tr("A project is a folder whose designs keep their versions: every save "
                                     "records one.")));
  auto* form = new QFormLayout;
  auto* nameEdit = new QLineEdit(name);
  auto* locationEdit = new QLineEdit(QDir::toNativeSeparators(location));
  auto* browse = new QPushButton(QObject::tr("&Browse..."));
  auto* locationRow = new QHBoxLayout;
  locationRow->addWidget(locationEdit, 1);
  locationRow->addWidget(browse);
  form->addRow(QObject::tr("&Name:"), nameEdit);
  form->addRow(QObject::tr("&Location:"), locationRow);
  auto* folderLabel = new QLabel;
  folderLabel->setTextInteractionFlags(Qt::TextSelectableByMouse);
  form->addRow(QObject::tr("Folder:"), folderLabel);
  layout->addLayout(form);
  auto* problem = new QLabel;
  setErrorStyleSheet(problem);
  problem->setWordWrap(true);
  layout->addWidget(problem);
  auto* buttons = new QDialogButtonBox(QDialogButtonBox::Ok | QDialogButtonBox::Cancel);
  buttons->button(QDialogButtonBox::Ok)->setText(QObject::tr("Create"));
  layout->addWidget(buttons);
  const auto folder = [&] {
    return QDir::cleanPath(QDir(QDir::fromNativeSeparators(locationEdit->text().trimmed()))
                               .filePath(nameEdit->text().trimmed()));
  };
  const auto check = [&] {
    const QString dir = folder();
    folderLabel->setText(QDir::toNativeSeparators(dir));
    QString why;
    const QString projectName = nameEdit->text().trimmed();
    const QString place = QDir::fromNativeSeparators(locationEdit->text().trimmed());
    if (projectName.isEmpty()) {
      why = QObject::tr("The project needs a name.");
    } else if (projectName.contains(QLatin1Char('/')) || projectName.contains(QLatin1Char('\\'))) {
      why = QObject::tr("The name is a folder's name: no / or \\.");
    } else if (place.isEmpty() || QDir::isRelativePath(place)) {
      why = QObject::tr("The location is a full path to a folder.");
    } else if (!usableFolder(dir)) {
      why = QObject::tr("%1 is there and not empty: choose another name.").arg(QDir::toNativeSeparators(dir));
    }
    problem->setText(why);
    buttons->button(QDialogButtonBox::Ok)->setEnabled(why.isEmpty());
  };
  QObject::connect(nameEdit, &QLineEdit::textChanged, &dialog, check);
  QObject::connect(locationEdit, &QLineEdit::textChanged, &dialog, check);
  QObject::connect(browse, &QPushButton::clicked, &dialog, [&] {
    const QString chosen = QFileDialog::getExistingDirectory(&dialog, QObject::tr("Location"),
                                                             QDir::fromNativeSeparators(locationEdit->text()));
    if (!chosen.isEmpty()) {
      locationEdit->setText(QDir::toNativeSeparators(chosen));
    }
  });
  QObject::connect(buttons, &QDialogButtonBox::accepted, &dialog, &QDialog::accept);
  QObject::connect(buttons, &QDialogButtonBox::rejected, &dialog, &QDialog::reject);
  check();
  nameEdit->selectAll();
  nameEdit->setFocus();
  qInfo().noquote() << QStringLiteral("New Project dialog: %1").arg(folder());
  prepareModal(&dialog);
  if (dialog.exec() != QDialog::Accepted) {
    qInfo().noquote() << QStringLiteral("New Project cancelled");
    return std::nullopt;
  }
  return folder();
}

std::optional<VersionSettings> askVersionAuthor(QWidget* parent, const QString& gitName, const QString& gitEmail,
                                                const VersionSettings& settings) {
  const bool hasGit = !gitName.isEmpty() && !gitEmail.isEmpty();
  QDialog dialog(parent);
  dialog.setWindowTitle(QObject::tr("Version Author"));
  auto* layout = new QVBoxLayout(&dialog);
  layout->addWidget(note(QObject::tr("Every version of a project records who made it: a name and an email "
                                     "address.")));
  auto* useGit = new QCheckBox(hasGit ? QObject::tr("Use &git's settings: %1 <%2>").arg(gitName, gitEmail)
                                      : QObject::tr("Use &git's settings (git has no user.name and user.email)"));
  useGit->setChecked(hasGit && settings.useGit);
  useGit->setEnabled(hasGit);
  layout->addWidget(useGit);
  auto* form = new QFormLayout;
  auto* name = new QLineEdit;
  auto* email = new QLineEdit;
  form->addRow(QObject::tr("&Name:"), name);
  form->addRow(QObject::tr("E&mail:"), email);
  layout->addLayout(form);
  auto* warning = note(QObject::tr("The email address is part of every version: anyone the project is "
                                   "shared with, or who sees a repository it is pushed to, sees it. A "
                                   "no-reply address can be used instead."));
  setWarningStyleSheet(warning);
  layout->addWidget(warning);
  auto* buttons = new QDialogButtonBox(QDialogButtonBox::Ok | QDialogButtonBox::Cancel);
  layout->addWidget(buttons);
  const auto show = [&] {
    const bool git = useGit->isChecked();
    name->setEnabled(!git);
    email->setEnabled(!git);
    if (git) {
      name->setText(gitName);
      email->setText(gitEmail);
    } else if (name->text() == gitName && email->text() == gitEmail) {
      // The settings' own, or git's to start from.
      name->setText(settings.name.isEmpty() ? gitName : settings.name);
      email->setText(settings.email.isEmpty() ? gitEmail : settings.email);
    }
  };
  const auto check = [&] {
    buttons->button(QDialogButtonBox::Ok)
        ->setEnabled(useGit->isChecked() ||
                     (!name->text().trimmed().isEmpty() && email->text().trimmed().contains(QLatin1Char('@'))));
  };
  name->setText(settings.name);
  email->setText(settings.email);
  show();
  check();
  QObject::connect(useGit, &QCheckBox::toggled, &dialog, [&] {
    show();
    check();
  });
  QObject::connect(name, &QLineEdit::textChanged, &dialog, check);
  QObject::connect(email, &QLineEdit::textChanged, &dialog, check);
  QObject::connect(buttons, &QDialogButtonBox::accepted, &dialog, &QDialog::accept);
  QObject::connect(buttons, &QDialogButtonBox::rejected, &dialog, &QDialog::reject);
  if (useGit->isChecked()) {
    buttons->button(QDialogButtonBox::Ok)->setFocus();
  } else {
    name->setFocus();
  }
  QString shown = QStringLiteral("git has none");
  if (useGit->isChecked()) {
    shown = QStringLiteral("git's %1 <%2>").arg(gitName, gitEmail);
  } else if (hasGit) {
    shown = QStringLiteral("the settings' (git has %1 <%2>)").arg(gitName, gitEmail);
  }
  qInfo().noquote() << QStringLiteral("Version author dialog: %1").arg(shown);
  prepareModal(&dialog);
  if (dialog.exec() != QDialog::Accepted) {
    qInfo().noquote() << QStringLiteral("Version author dialog cancelled");
    return std::nullopt;
  }
  VersionSettings chosen = settings;
  chosen.useGit = useGit->isChecked();
  if (!chosen.useGit) {
    chosen.name = name->text().trimmed();
    chosen.email = email->text().trimmed();
  }
  chosen.confirmed = true;
  return chosen;
}

std::optional<QString> askVersionDescription(QWidget* parent, const QString& file, const QString& automatic) {
  QDialog dialog(parent);
  dialog.setWindowTitle(QObject::tr("Save Version"));
  auto* layout = new QVBoxLayout(&dialog);
  layout->addWidget(note(QObject::tr("Saves %1 and records a version with this description. Its first line "
                                     "is what the version list shows.")
                             .arg(file)));
  auto* text = new QPlainTextEdit;
  text->setPlaceholderText(automatic);
  text->setTabChangesFocus(true);
  layout->addWidget(text);
  auto* buttons = new QDialogButtonBox(QDialogButtonBox::Ok | QDialogButtonBox::Cancel);
  buttons->button(QDialogButtonBox::Ok)->setText(QObject::tr("Save"));
  buttons->button(QDialogButtonBox::Ok)->setToolTip(QObject::tr("Ctrl+Enter"));
  layout->addWidget(buttons);
  QObject::connect(buttons, &QDialogButtonBox::accepted, &dialog, &QDialog::accept);
  QObject::connect(buttons, &QDialogButtonBox::rejected, &dialog, &QDialog::reject);
  // Enter makes a new line; Ctrl+Enter saves.
  for (const QKeySequence& key : {QKeySequence(Qt::CTRL | Qt::Key_Return), QKeySequence(Qt::CTRL | Qt::Key_Enter)}) {
    QObject::connect(new QShortcut(key, &dialog), &QShortcut::activated, &dialog, &QDialog::accept);
  }
  dialog.resize(520, 240);
  text->setFocus();
  qInfo().noquote() << QStringLiteral("Save Version dialog: %1").arg(automatic.section(QLatin1Char('\n'), 0, 0));
  prepareModal(&dialog);
  if (dialog.exec() != QDialog::Accepted) {
    qInfo().noquote() << QStringLiteral("Save Version cancelled");
    return std::nullopt;
  }
  return text->toPlainText().trimmed();
}

std::optional<HistoryStart> askStartHistory(QWidget* parent, const QString& file, const QString& folder,
                                            const QString& outer, bool repository) {
  QMessageBox box(QMessageBox::Question, QObject::tr("Start Version History"),
                  QObject::tr("%1 is in no project, so its versions are not kept.").arg(file), QMessageBox::Cancel,
                  parent);
  QString details;
  if (!outer.isEmpty()) {
    details = QObject::tr("Its folder is inside the git repository %1, so it cannot be a project of its own: "
                          "move the design into a new project, which is made in a folder of its own.")
                  .arg(QDir::toNativeSeparators(outer));
  } else if (repository) {
    details = QObject::tr("Its folder %1 is a git repository: Mitcad can add its project files "
                          "(.mitcad/project.json, .gitattributes, .gitignore) and record the versions of its "
                          "designs in it, or move the design into a new project.")
                  .arg(QDir::toNativeSeparators(folder));
  } else {
    details = QObject::tr("Its folder %1 can become a project with version history (a git repository), or the "
                          "design can move into a new project. Other files in the folder are not recorded.")
                  .arg(QDir::toNativeSeparators(folder));
  }
  box.setInformativeText(details);
  QPushButton* useFolder = box.addButton(QObject::tr("Use the &Folder"), QMessageBox::AcceptRole);
  useFolder->setEnabled(outer.isEmpty());
  QPushButton* move = box.addButton(QObject::tr("&Move to a New Project..."), QMessageBox::AcceptRole);
  box.setDefaultButton(outer.isEmpty() ? useFolder : move);
  qInfo().noquote() << QStringLiteral("Start Version History dialog: %1%2")
                           .arg(folder, outer.isEmpty() ? (repository ? QStringLiteral(" (a repository)") : QString())
                                                        : QStringLiteral(" (inside %1)").arg(outer));
  prepareModal(&box);
  box.exec();
  if (box.clickedButton() == useFolder) {
    return HistoryStart::UseFolder;
  }
  if (box.clickedButton() == move) {
    return HistoryStart::MoveToProject;
  }
  qInfo().noquote() << QStringLiteral("Start Version History cancelled");
  return std::nullopt;
}

std::optional<ExternalChange> askExternalChange(QWidget* parent, const QString& file, bool newerVersion,
                                                bool unrecorded) {
  QMessageBox box(QMessageBox::Warning, QObject::tr("Mitcad"),
                  QObject::tr("%1 has changed since you opened it.").arg(file), QMessageBox::Cancel, parent);
  QString details = newerVersion ? QObject::tr("The project's history has a newer version of it (saved by "
                                               "another Mitcad, or brought in with git).")
                                 : QObject::tr("The file in the folder was changed by another program.");
  details += QLatin1Char(' ');
  details += unrecorded ? QObject::tr("Save as New Version records that change as a version first, then yours "
                                      "on top, so both stay in the history.")
                        : QObject::tr("Save as New Version records yours on top of it; it stays in the history.");
  box.setInformativeText(details);
  QPushButton* version = box.addButton(QObject::tr("Save as &New Version"), QMessageBox::AcceptRole);
  QPushButton* copy = box.addButton(QObject::tr("Save &As..."), QMessageBox::AcceptRole);
  QPushButton* compare = box.addButton(QObject::tr("C&ompare"), QMessageBox::ActionRole);
  box.setDefaultButton(copy);
  prepareModal(&box);
  box.exec();
  if (box.clickedButton() == version) {
    return ExternalChange::NewVersion;
  }
  if (box.clickedButton() == copy) {
    return ExternalChange::SaveAs;
  }
  if (box.clickedButton() == compare) {
    return ExternalChange::Compare;
  }
  return std::nullopt;
}

void showComparison(QWidget* parent, const QString& title, const QString& text) {
  QDialog dialog(parent);
  dialog.setWindowTitle(title);
  auto* layout = new QVBoxLayout(&dialog);
  auto* view = new QPlainTextEdit(text);
  view->setReadOnly(true);
  view->setLineWrapMode(QPlainTextEdit::NoWrap);
  layout->addWidget(view);
  auto* buttons = new QDialogButtonBox(QDialogButtonBox::Close);
  QObject::connect(buttons, &QDialogButtonBox::rejected, &dialog, &QDialog::reject);
  layout->addWidget(buttons);
  dialog.resize(640, 420);
  prepareModal(&dialog);
  dialog.exec();
}

std::optional<bool> askRestoreVersion(QWidget* parent, const QString& file, const QString& version,
                                      const QString& when, int next, bool modified) {
  QMessageBox box(QMessageBox::Question, QObject::tr("Restore Version"),
                  QObject::tr("Restore %1 of %2?").arg(version, file), QMessageBox::Cancel, parent);
  QString details = QObject::tr("The design as it was in %1 (saved %2) becomes the latest version, v%3. The "
                                "versions since stay in the history.")
                        .arg(version, when)
                        .arg(next);
  QPushButton* restore = nullptr;
  QPushButton* discard = nullptr;
  if (modified) {
    details += QLatin1Char(' ');
    details += QObject::tr("The open design has unsaved changes: Save and Restore records them as a version "
                           "first; Discard and Restore drops them.");
    restore = box.addButton(QObject::tr("&Save and Restore"), QMessageBox::AcceptRole);
    discard = box.addButton(QObject::tr("&Discard and Restore"), QMessageBox::DestructiveRole);
  } else {
    restore = box.addButton(QObject::tr("&Restore"), QMessageBox::AcceptRole);
  }
  box.setInformativeText(details);
  box.setDefaultButton(restore);
  qInfo().noquote() << QStringLiteral("Restore dialog: %1 of %2%3")
                           .arg(version, file, modified ? QStringLiteral(" (unsaved changes)") : QString());
  prepareModal(&box);
  box.exec();
  if (box.clickedButton() == restore) {
    return true;
  }
  if (discard != nullptr && box.clickedButton() == discard) {
    return false;
  }
  qInfo().noquote() << QStringLiteral("Restore cancelled");
  return std::nullopt;
}

QString automaticVersionMessage(const QString& file, const QStringList& steps) {
  const QString plain = QObject::tr("Save %1").arg(file);
  QStringList distinct;
  for (const QString& step : steps) {
    if (distinct.isEmpty() || distinct.last() != step) {
      distinct << step;
    }
  }
  if (distinct.isEmpty()) {
    return plain;
  }
  QString summary = plain + QStringLiteral(": ") + distinct.first();
  qsizetype shown = 1;
  while (shown < distinct.size()) {
    const QString next = summary + QStringLiteral(", ") + distinct[shown];
    const QString more = QObject::tr(" and %1 more").arg(distinct.size() - shown - 1);
    if ((shown + 1 < distinct.size() ? next + more : next).size() > kSummaryLength) {
      break;
    }
    summary = next;
    ++shown;
  }
  if (shown == distinct.size()) {
    return summary;
  }
  summary += QObject::tr(" and %1 more").arg(distinct.size() - shown);
  QStringList lines;
  for (const QString& step : distinct) {
    lines << QStringLiteral("- ") + step;
  }
  return summary + QStringLiteral("\n\n") + lines.join(QLatin1Char('\n'));
}

} // namespace mitcad
