// SPDX-License-Identifier: MIT
#include "ProjectBrowser.hpp"

#include <algorithm>
#include <functional>
#include <initializer_list>
#include <utility>

#include <QAbstractButton>
#include <QCheckBox>
#include <QComboBox>
#include <QCoreApplication>
#include <QDateTime>
#include <QDialog>
#include <QDialogButtonBox>
#include <QDir>
#include <QEvent>
#include <QHBoxLayout>
#include <QHash>
#include <QHeaderView>
#include <QJsonObject>
#include <QKeyEvent>
#include <QLabel>
#include <QLineEdit>
#include <QMoveEvent>
#include <QPushButton>
#include <QResizeEvent>
#include <QShowEvent>
#include <QSignalBlocker>
#include <QSplitter>
#include <QStringList>
#include <QToolButton>
#include <QTreeWidget>
#include <QTreeWidgetItem>
#include <QTreeWidgetItemIterator>
#include <QVBoxLayout>
#include <QVector>
#include <QtLogging>

#include "../files/RemoteTask.hpp"
#include "../framework/AppSettings.hpp"
#include "../framework/Dialogs.hpp"
#include "../framework/Icons.hpp"
#include "../framework/PanelTree.hpp"
#include "../framework/TestSync.hpp"
#include "../platform/MacDialogs.hpp"
#include "ProjectBrowserData.hpp"
#include "ProjectRowText.hpp"
#include "ProjectsTree.hpp"

namespace mitcad {
namespace {

QString tr(const char* text) { return QCoreApplication::translate("ProjectBrowser", text); }

// What a row of the folders' tree stands for.
constexpr int kRootRole = Qt::UserRole + 1;
constexpr int kFolderRole = Qt::UserRole + 2;
constexpr int kKeyRole = Qt::UserRole + 3;
// A row of the contents: its entry's index in the entries shown.
constexpr int kEntryRole = Qt::UserRole + 4;

// The contents' columns.
enum Column { kName = 0, kVersion, kLock, kChanged, kBy, kWhere, kColumns };

// A tree whose keys a function may take first (Enter on a folder goes into
// it instead of pressing the dialog's default button; Backspace goes up).
class KeyedTree : public QTreeWidget {
public:
  std::function<bool(QKeyEvent*)> keys;

protected:
  void keyPressEvent(QKeyEvent* event) override {
    if (keys && keys(event)) {
      event->accept();
      return;
    }
    QTreeWidget::keyPressEvent(event);
  }
};

// A button's text without its mnemonic and dots ("Another File").
QString buttonName(const QAbstractButton* button) {
  return button->text().remove(QLatin1Char('&')).remove(QStringLiteral("...")).remove(QChar(0x2026)).trimmed();
}

// The window the UI tests' places are relative to: the main window.
QWidget* placesWindow(QWidget* parent) {
  QWidget* top = parent;
  while (top != nullptr && top->parentWidget() != nullptr) {
    top = top->parentWidget();
  }
  return top != nullptr ? top->window() : nullptr;
}

QString kindText(const ProjectRow& row) {
  return row.kind == ProjectRow::Kind::Cloud ? QStringLiteral("cloud") : QStringLiteral("local");
}

// A design's edit lock in words, for its column.
QString lockText(const ProjectDesignRow& design) {
  if (!design.locked) {
    return {};
  }
  if (design.lockMine) {
    return tr("Editing");
  }
  return design.lockSameOwner ? tr("Editing elsewhere") : tr("Locked: %1").arg(design.lockOwner);
}

class BrowserDialog : public QDialog {
public:
  BrowserDialog(QWidget* parent, ProjectBrowserOptions options);

  std::optional<ProjectBrowserResult> run();

protected:
  // Shown, moved or resized: where things are, again (UI tests).
  void showEvent(QShowEvent* event) override {
    QDialog::showEvent(event);
    logPlaces();
  }
  void moveEvent(QMoveEvent* event) override {
    QDialog::moveEvent(event);
    logPlaces();
  }
  void resizeEvent(QResizeEvent* event) override {
    QDialog::resizeEvent(event);
    logPlaces();
  }

private:
  void fill(const QJsonObject& answer);
  // Shows a place's contents (a search ends).
  void showPlace(const BrowserPlace& place);
  void showEntries(const QVector<BrowserEntry>& entries, bool searching);
  void rebuildPathBar();
  // The search field or its scope changed.
  void search();
  // A row of the contents taken (a double-click, Enter, Open): a folder is
  // gone into, a design chosen.
  void activate(QTreeWidgetItem* item);
  void finish(std::optional<ProjectBrowserResult> result, const QString& line, const BrowserPlace& place);
  void updateButtons();
  void logPlaces();
  QString keyOfPlace(const BrowserPlace& place) const;
  BrowserPlace placeOf(const QTreeWidgetItem* item) const;
  const BrowserEntry* entryOf(const QTreeWidgetItem* item) const;

  ProjectBrowserOptions m_options;
  QWidget* m_pathBar = nullptr;
  QHBoxLayout* m_pathLayout = nullptr;
  QLineEdit* m_search = nullptr;
  QComboBox* m_scope = nullptr;
  KeyedTree* m_folders = nullptr;
  KeyedTree* m_contents = nullptr;
  QLabel* m_status = nullptr;
  QCheckBox* m_readOnly = nullptr;
  QPushButton* m_other = nullptr;
  QPushButton* m_open = nullptr;
  QPushButton* m_cancel = nullptr;
  QVector<QToolButton*> m_pathButtons;
  QStringList m_pathKeys;

  QVector<ProjectRow> m_projects;
  QHash<QString, QString> m_keys; // by root
  BrowserPlace m_place;
  bool m_hasPlace = false;
  bool m_searching = false;
  QVector<BrowserEntry> m_entries; // the rows of the contents
  bool m_building = false;
  bool m_closed = false;
  std::optional<ProjectBrowserResult> m_result;
  QString m_loggedPlaces;
  QString m_loggedSelection;
  bool m_placesQueued = false;
};

BrowserDialog::BrowserDialog(QWidget* parent, ProjectBrowserOptions options)
    : QDialog(parent), m_options(std::move(options)) {
  setWindowTitle(m_options.title);
  resize(820, 500);
  auto* layout = new QVBoxLayout(this);

  // The path bar, the search and its scope.
  auto* top = new QHBoxLayout;
  m_pathBar = new QWidget;
  m_pathLayout = new QHBoxLayout(m_pathBar);
  m_pathLayout->setContentsMargins(0, 0, 0, 0);
  m_pathLayout->setSpacing(2);
  top->addWidget(m_pathBar, 1);
  auto* searchLabel = new QLabel(tr("&Search:"), this);
  m_search = new QLineEdit;
  m_search->setClearButtonEnabled(true);
  m_search->setAccessibleName(tr("Search designs"));
  m_search->setPlaceholderText(tr("Design name"));
  m_search->setToolTip(tr("Lists the designs whose name holds the text, as you type"));
  m_search->setMinimumWidth(180);
  searchLabel->setBuddy(m_search);
  auto* scopeLabel = new QLabel(tr("&In:"));
  m_scope = new QComboBox;
  m_scope->addItem(tr("All Projects"));
  m_scope->addItem(tr("This Project"));
  m_scope->setToolTip(tr("Where the search looks: every project, or the project shown"));
  scopeLabel->setBuddy(m_scope);
#ifdef Q_OS_MACOS
  searchLabel->hide();
#else
  top->addWidget(searchLabel);
#endif
  mac::styleSearchField(m_search);
  top->addWidget(m_search);
  top->addWidget(scopeLabel);
  top->addWidget(m_scope);
  layout->addLayout(top);

  // The folders left, the contents right.
  auto* splitter = new QSplitter(Qt::Horizontal);
  m_folders = new KeyedTree;
  panel::setCompactIndentation(m_folders, panel::Indent::Tree);
  m_folders->setHeaderHidden(true);
  m_folders->setColumnCount(1);
  m_folders->setIconSize(QSize(16, 16));
  m_folders->setUniformRowHeights(true);
  m_folders->setSelectionMode(QAbstractItemView::SingleSelection);
  m_folders->setEditTriggers(QAbstractItemView::NoEditTriggers);
  m_contents = new KeyedTree;
  panel::setCompactIndentation(m_contents, panel::Indent::Flat);
  m_contents->setColumnCount(kColumns);
  m_contents->setHeaderLabels({tr("Name"), tr("Version"), tr("Edit lock"), tr("Last change"), tr("By"),
                               tr("Project and folder")});
  m_contents->setRootIsDecorated(false);
  m_contents->setIconSize(QSize(16, 16));
  m_contents->setUniformRowHeights(true);
  m_contents->setSelectionMode(QAbstractItemView::SingleSelection);
  m_contents->setEditTriggers(QAbstractItemView::NoEditTriggers);
  m_contents->setAllColumnsShowFocus(true);
  m_contents->header()->setStretchLastSection(false);
  m_contents->header()->setSectionResizeMode(kName, QHeaderView::Stretch);
  for (int column = kVersion; column < kColumns; ++column) {
    m_contents->header()->setSectionResizeMode(column, QHeaderView::ResizeToContents);
  }
  m_contents->setColumnHidden(kWhere, true);
  splitter->addWidget(m_folders);
  splitter->addWidget(m_contents);
  splitter->setStretchFactor(0, 1);
  splitter->setStretchFactor(1, 3);
  splitter->setSizes({220, 600});
  layout->addWidget(splitter, 1);
  m_status = new QLabel(tr("Reading the projects…"));
  m_status->setWordWrap(true);
  m_status->setTextFormat(Qt::PlainText);
  layout->addWidget(m_status);

  // A helper at the left; platform-ordered Open/Cancel at the right.
  auto* bottom = new QHBoxLayout;
  auto* buttons = new QDialogButtonBox;
  if (!m_options.otherText.isEmpty()) {
    m_other = new QPushButton(m_options.otherText);
    m_other->setAutoDefault(false);
    bottom->addWidget(m_other);
  }
  bottom->addStretch(1);
  if (!m_options.readOnlyText.isEmpty()) {
    m_readOnly = new QCheckBox(m_options.readOnlyText);
    m_readOnly->setToolTip(tr("Opens the design without its edit lock: to look at it while someone else edits it"));
    bottom->addWidget(m_readOnly);
    connect(m_readOnly, &QCheckBox::toggled, this, [](bool on) {
      qInfo().noquote() << QStringLiteral("Project browser read-only: %1").arg(on ? QStringLiteral("on")
                                                                                  : QStringLiteral("off"));
    });
  }
  m_open = new QPushButton(m_options.acceptText.isEmpty() ? tr("&Open") : m_options.acceptText);
  m_open->setDefault(true);
  m_open->setEnabled(false);
  m_cancel = new QPushButton(tr("Cancel"));
  m_cancel->setAutoDefault(false);
  buttons->addButton(m_open, QDialogButtonBox::AcceptRole);
  buttons->addButton(m_cancel, QDialogButtonBox::RejectRole);
  bottom->addWidget(buttons);
  layout->addLayout(bottom);
  mac::styleDialogButtons(buttons, m_open, true);
  mac::styleListTree(m_folders);
  mac::styleListTree(m_contents);
  mac::styleDialog(this);

  // Enter on the folders' tree goes on to the contents; on the contents it
  // goes into a folder or opens a design; Backspace (and Alt+Up) goes up.
  m_folders->keys = [this](QKeyEvent* event) {
    if (event->key() == Qt::Key_Return || event->key() == Qt::Key_Enter) {
      m_contents->setFocus(Qt::OtherFocusReason);
      return true;
    }
    return false;
  };
  m_contents->keys = [this](QKeyEvent* event) {
    const bool up = event->key() == Qt::Key_Backspace ||
                    (event->key() == Qt::Key_Up && (event->modifiers() & Qt::AltModifier) != 0);
    if (event->key() == Qt::Key_Return || event->key() == Qt::Key_Enter) {
      activate(m_contents->currentItem());
      return true;
    }
    if (up) {
      if (!m_searching && m_hasPlace && !m_place.folder.isEmpty()) {
        showPlace(placeUp(m_place));
      }
      return true;
    }
    return false;
  };
  connect(m_folders, &QTreeWidget::currentItemChanged, this, [this](QTreeWidgetItem* item) {
    if (m_building || item == nullptr) {
      return;
    }
    showPlace(placeOf(item));
  });
  connect(m_contents, &QTreeWidget::currentItemChanged, this, [this](QTreeWidgetItem* item) {
    updateButtons();
    const BrowserEntry* entry = entryOf(item);
    if (m_building || entry == nullptr || entry->key() == m_loggedSelection) {
      return;
    }
    m_loggedSelection = entry->key();
    qInfo().noquote() << QStringLiteral("Project browser selected %1").arg(m_loggedSelection);
  });
  connect(m_contents, &QTreeWidget::itemDoubleClicked, this, [this](QTreeWidgetItem* item) { activate(item); });
  connect(m_search, &QLineEdit::textChanged, this, [this] { search(); });
  connect(m_scope, &QComboBox::currentIndexChanged, this, [this](int index) {
    qInfo().noquote() << QStringLiteral("Project browser search scope: %1")
                             .arg(index == 0 ? QStringLiteral("all projects") : QStringLiteral("this project"));
    search();
  });
  connect(m_open, &QPushButton::clicked, this, [this] { activate(m_contents->currentItem()); });
  connect(m_cancel, &QPushButton::clicked, this, [this] { reject(); });
  if (m_other != nullptr) {
    connect(m_other, &QPushButton::clicked, this, [this] {
      ProjectBrowserResult result;
      result.other = true;
      result.readOnly = m_readOnly != nullptr && m_readOnly->isChecked();
      finish(result, result.readOnly ? QStringLiteral("another file (read-only)") : QStringLiteral("another file"),
             m_place);
    });
  }
  // Folders folded or unfolded, the splitter's handle moved: the places
  // again.
  for (QTreeWidget* tree : std::initializer_list<QTreeWidget*>{m_folders, m_contents}) {
    connect(tree, &QTreeWidget::itemExpanded, this, [this] { logPlaces(); });
    connect(tree, &QTreeWidget::itemCollapsed, this, [this] { logPlaces(); });
  }
  connect(splitter, &QSplitter::splitterMoved, this, [this] { logPlaces(); });
}

std::optional<ProjectBrowserResult> BrowserDialog::run() {
  // The projects as the Projects panel lists them, read on a thread.
  const ProjectsSettings settings = ProjectsSettings::load();
  const QStringList known = settings.known;
  const QStringList hidden = settings.hidden;
  const QString folder = projectsDirectory();
  const QString session = m_options.session;
  RemoteTask* task = RemoteTask::custom(
      QStringLiteral("project_tree"),
      [known, folder, hidden, session](const SyncControl&) { return readProjects(known, folder, hidden, session); },
      this);
  connect(task, &RemoteTask::finished, this, [this, task] {
    const QJsonObject answer = task->answer();
    task->deleteLater();
    if (!m_closed) {
      fill(answer);
    }
  });
  task->start();
  qInfo().noquote() << QStringLiteral("Project browser %1: reading the projects").arg(m_options.title);
  m_contents->setFocus(Qt::OtherFocusReason);
  logPlaces();
  prepareModal(this);
  execDialog(this);
  if (!m_closed) {
    m_closed = true;
    qInfo().noquote() << QStringLiteral("Project browser %1: cancelled").arg(m_options.title);
    if (m_hasPlace) {
      ProjectsSettings remembered = ProjectsSettings::load();
      remembered.browseRoot = m_place.root;
      remembered.browseFolder = m_place.folder;
      remembered.save();
    }
    return std::nullopt;
  }
  return m_result;
}

void BrowserDialog::fill(const QJsonObject& answer) {
  m_projects = browsableProjects(projectRows(answer));
  m_keys = projectKeys(m_projects);
  m_building = true;
  m_folders->clear();
  QStringList listed;
  for (const ProjectRow& row : std::as_const(m_projects)) {
    const QString key = m_keys.value(row.root);
    listed << QStringLiteral("%1 (%2)").arg(row.name, kindText(row));
    auto* top = new QTreeWidgetItem(m_folders);
    top->setText(0, row.name);
    top->setIcon(0, themeIcon(row.kind == ProjectRow::Kind::Cloud ? QStringLiteral("remote-open")
                                                                  : QStringLiteral("home")));
    top->setData(0, kRootRole, row.root);
    top->setData(0, kFolderRole, QString());
    top->setData(0, kKeyRole, key);
    QStringList tip{row.name, row.kind == ProjectRow::Kind::Cloud ? tr("Cloud project") : tr("Local project")};
    if (!row.remoteUrl.isEmpty()) {
      tip << row.remoteUrl;
    }
    tip << QDir::toNativeSeparators(row.root);
    top->setToolTip(0, tip.join(QLatin1Char('\n')));
    QHash<QString, QTreeWidgetItem*> folders;
    folders.insert(QString(), top);
    // allFolders is sorted, so a folder's parent comes before it.
    for (const QString& path : allFolders(row)) {
      const qsizetype slash = path.lastIndexOf(QLatin1Char('/'));
      QTreeWidgetItem* parent = folders.value(slash < 0 ? QString() : path.left(slash), top);
      auto* item = new QTreeWidgetItem(parent);
      item->setText(0, path.mid(slash + 1));
      item->setIcon(0, themeIcon(QStringLiteral("folder")));
      item->setData(0, kRootRole, row.root);
      item->setData(0, kFolderRole, path);
      item->setData(0, kKeyRole, placeKey(key, path));
      item->setToolTip(0, QStringLiteral("%1\n%2").arg(path.mid(slash + 1),
                                                      QDir::toNativeSeparators(QDir(row.root).filePath(path))));
      folders.insert(path, item);
    }
  }
  // Projects' trees are small: every folder in sight.
  m_folders->expandAll();
  m_building = false;
  qInfo().noquote() << QStringLiteral("Project browser %1: %2 projects: %3")
                           .arg(m_options.title)
                           .arg(m_projects.size())
                           .arg(listed.join(QStringLiteral(" | ")));
  const QString error = answer.value(QStringLiteral("error")).toObject().value(QStringLiteral("message")).toString();
  if (!error.isEmpty()) {
    m_status->setText(tr("The projects could not be read: %1").arg(error));
  } else if (m_projects.isEmpty()) {
    m_status->setText(tr("No projects with versions are listed: Another File… opens any design."));
  }
  m_status->setVisible(!error.isEmpty() || m_projects.isEmpty());
  if (m_projects.isEmpty()) {
    logPlaces();
    return;
  }
  // The place shown last, else the given one, else the first project.
  const ProjectsSettings settings = ProjectsSettings::load();
  const BrowserPlace remembered{settings.browseRoot, settings.browseFolder};
  const BrowserPlace given{m_options.root, m_options.folder};
  BrowserPlace start{m_projects.constFirst().root, QString()};
  QString why = QStringLiteral("first project");
  if (!remembered.root.isEmpty() && placeExists(m_projects, remembered)) {
    start = remembered;
    why = QStringLiteral("remembered");
  } else if (!given.root.isEmpty() && placeExists(m_projects, given)) {
    start = given;
    why = QStringLiteral("current project");
  } else if (!given.root.isEmpty() && findProject(m_projects, given.root) != nullptr) {
    start = {given.root, QString()};
    why = QStringLiteral("current project");
  }
  // The listed root's spelling (a remembered one may differ in case).
  start.root = findProject(m_projects, start.root)->root;
  qInfo().noquote() << QStringLiteral("Project browser starts at %1 (%2)").arg(keyOfPlace(start), why);
  showPlace(start);
  m_contents->setFocus(Qt::OtherFocusReason);
}

QString BrowserDialog::keyOfPlace(const BrowserPlace& place) const {
  const ProjectRow* project = findProject(m_projects, place.root);
  return placeKey(project != nullptr ? m_keys.value(project->root) : place.root, place.folder);
}

BrowserPlace BrowserDialog::placeOf(const QTreeWidgetItem* item) const {
  if (item == nullptr) {
    return {};
  }
  return {item->data(0, kRootRole).toString(), item->data(0, kFolderRole).toString()};
}

const BrowserEntry* BrowserDialog::entryOf(const QTreeWidgetItem* item) const {
  if (item == nullptr) {
    return nullptr;
  }
  const int index = item->data(kName, kEntryRole).toInt();
  return index >= 0 && index < m_entries.size() ? &m_entries[index] : nullptr;
}

void BrowserDialog::showPlace(const BrowserPlace& place) {
  const ProjectRow* project = findProject(m_projects, place.root);
  if (project == nullptr) {
    return;
  }
  m_place = place;
  m_hasPlace = true;
  if (!m_search->text().isEmpty()) {
    // A place chosen ends the search.
    const QSignalBlocker blocker(m_search);
    m_search->clear();
  }
  // The place selected in the folders' tree, its folders shown.
  m_building = true;
  for (QTreeWidgetItemIterator it(m_folders); *it != nullptr; ++it) {
    if (placeOf(*it) == place) {
      for (QTreeWidgetItem* up = (*it)->parent(); up != nullptr; up = up->parent()) {
        up->setExpanded(true);
      }
      (*it)->setExpanded(true);
      m_folders->setCurrentItem(*it);
      m_folders->scrollToItem(*it);
      break;
    }
  }
  m_building = false;
  rebuildPathBar();
  const QVector<BrowserEntry> entries = folderContents(*project, m_keys.value(project->root), place.folder);
  showEntries(entries, false);
  QStringList words;
  for (const BrowserEntry& entry : entries) {
    words << entryWords(entry);
  }
  qInfo().noquote() << QStringLiteral("Project browser shows %1: %2")
                           .arg(keyOfPlace(place),
                                words.isEmpty() ? QStringLiteral("empty") : words.join(QStringLiteral(" | ")));
}

void BrowserDialog::showEntries(const QVector<BrowserEntry>& entries, bool searching) {
  m_searching = searching;
  m_building = true;
  m_contents->clear();
  m_entries = entries;
  m_contents->setColumnHidden(kWhere, !searching);
  for (int i = 0; i < m_entries.size(); ++i) {
    const BrowserEntry& entry = m_entries[i];
    auto* item = new QTreeWidgetItem(m_contents);
    item->setData(kName, kEntryRole, i);
    item->setText(kName, entry.name);
    const QString file = QDir::toNativeSeparators(QDir(entry.root).filePath(entry.path));
    if (entry.kind == BrowserEntry::Kind::Folder) {
      item->setIcon(kName, themeIcon(QStringLiteral("folder")));
      item->setToolTip(kName, QStringLiteral("%1\n%2").arg(entry.name, file));
    } else {
      const ProjectDesignRow& design = entry.design;
      const RowMarks marks = designMarks(design);
      item->setIcon(kName, themeIcon(QStringLiteral("project")));
      item->setToolTip(kName, QStringLiteral("%1\n%2\n%3").arg(entry.name, file, marks.full));
      item->setText(kVersion, marks.text);
      item->setToolTip(kVersion, marks.full);
      item->setText(kLock, lockText(design));
      if (!marks.icon.isEmpty()) {
        item->setIcon(kLock, themeIcon(marks.icon));
      }
      if (design.versionTime > 0) {
        item->setText(kChanged, QDateTime::fromSecsSinceEpoch(design.versionTime)
                                    .toLocalTime()
                                    .toString(QStringLiteral("yyyy-MM-dd HH:mm")));
      }
      item->setText(kBy, design.versionAuthor);
    }
    if (searching) {
      // "Team › bolts › hex": its project and folder.
      const QString separator = QStringLiteral(" ") + QChar(0x203A) + QStringLiteral(" ");
      QString where = entry.projectName;
      if (!entry.folder().isEmpty()) {
        where += separator + QString(entry.folder()).replace(QLatin1Char('/'), separator);
      }
      item->setText(kWhere, where);
    }
  }
  m_building = false;
  if (m_contents->topLevelItemCount() > 0) {
    m_contents->setCurrentItem(m_contents->topLevelItem(0));
  }
  updateButtons();
  logPlaces();
}

void BrowserDialog::rebuildPathBar() {
  for (QToolButton* button : std::as_const(m_pathButtons)) {
    button->deleteLater();
  }
  m_pathButtons.clear();
  m_pathKeys.clear();
  while (QLayoutItem* item = m_pathLayout->takeAt(0)) {
    if (QWidget* widget = item->widget()) {
      widget->deleteLater();
    }
    delete item;
  }
  const ProjectRow* project = findProject(m_projects, m_place.root);
  if (project == nullptr) {
    return;
  }
  const QVector<BrowserPathPart> parts = pathParts(*project, m_place.folder);
  for (int i = 0; i < parts.size(); ++i) {
    if (i > 0) {
      m_pathLayout->addWidget(new QLabel(QString(QChar(0x203A))));
    }
    auto* button = new QToolButton;
    button->setText(parts[i].text);
    button->setAutoRaise(true);
    button->setFocusPolicy(Qt::NoFocus);
    button->setToolTip(QDir::toNativeSeparators(QDir(project->root).filePath(parts[i].place.folder)));
    const BrowserPlace place = parts[i].place;
    connect(button, &QToolButton::clicked, this, [this, place] { showPlace(place); });
    m_pathLayout->addWidget(button);
    m_pathButtons << button;
    m_pathKeys << keyOfPlace(place);
  }
  m_pathLayout->addStretch(1);
}

void BrowserDialog::search() {
  const QString text = m_search->text().trimmed();
  if (text.isEmpty()) {
    if (m_searching && m_hasPlace) {
      showPlace(m_place);
    }
    return;
  }
  const bool here = m_scope->currentIndex() == 1 && m_hasPlace;
  const QVector<BrowserEntry> found = searchDesigns(m_projects, m_keys, text, here ? m_place.root : QString());
  showEntries(found, true);
  QStringList keys;
  for (const BrowserEntry& entry : found) {
    keys << entry.key();
  }
  m_status->setText(found.isEmpty() ? tr("No design's name holds “%1”.").arg(text) : QString());
  m_status->setVisible(found.isEmpty());
  const QString where = here ? keyOfPlace({m_place.root, QString()}) : QStringLiteral("all projects");
  qInfo().noquote() << QStringLiteral("Project browser search \"%1\" in %2: %3 found%4")
                           .arg(text, where)
                           .arg(found.size())
                           .arg(keys.isEmpty() ? QString() : QStringLiteral(": ") + keys.join(QStringLiteral(" | ")));
}

void BrowserDialog::activate(QTreeWidgetItem* item) {
  const BrowserEntry* entry = entryOf(item);
  if (entry == nullptr || m_closed) {
    return;
  }
  if (entry->kind == BrowserEntry::Kind::Folder) {
    showPlace({entry->root, entry->path});
    return;
  }
  ProjectBrowserResult result;
  result.root = entry->root;
  result.path = QDir::cleanPath(QDir(entry->root).filePath(entry->path));
  result.readOnly = m_readOnly != nullptr && m_readOnly->isChecked();
  const QString line =
      QStringLiteral("chose %1%2").arg(result.path, result.readOnly ? QStringLiteral(" (read-only)") : QString());
  const BrowserPlace place{entry->root, entry->folder()};
  finish(result, line, place);
}

void BrowserDialog::finish(std::optional<ProjectBrowserResult> result, const QString& line,
                           const BrowserPlace& place) {
  if (m_closed) {
    return;
  }
  m_closed = true;
  m_result = std::move(result);
  // The place to show the next time: the chosen design's folder, else the
  // place shown.
  if (!place.root.isEmpty()) {
    ProjectsSettings remembered = ProjectsSettings::load();
    remembered.browseRoot = place.root;
    remembered.browseFolder = place.folder;
    remembered.save();
  }
  qInfo().noquote() << QStringLiteral("Project browser %1: %2").arg(m_options.title, line);
  accept();
}

void BrowserDialog::updateButtons() { m_open->setEnabled(entryOf(m_contents->currentItem()) != nullptr); }

void BrowserDialog::logPlaces() {
  // Once the layout has placed what changed (many changes make one log).
  if (m_placesQueued) {
    return;
  }
  m_placesQueued = true;
  TestSync::singleShot(50, this, [this] {
    m_placesQueued = false;
    QCoreApplication::sendPostedEvents(nullptr, QEvent::LayoutRequest);
    QWidget* window = placesWindow(parentWidget());
    const QPoint origin = window != nullptr ? window->mapToGlobal(QPoint(0, 0)) : QPoint();
    const auto at = [&origin](const QWidget* widget, const QPoint& point) {
      const QPoint global = widget->mapToGlobal(point) - origin;
      return QStringLiteral("%1,%2").arg(global.x()).arg(global.y());
    };
    QStringList lines;
    const QRect folderSight = m_folders->viewport()->rect();
    for (QTreeWidgetItemIterator it(m_folders); *it != nullptr; ++it) {
      const QRect rect = m_folders->visualItemRect(*it);
      if (rect.isValid() && folderSight.contains(rect.center())) {
        lines << QStringLiteral("Project browser folder %1 at %2")
                     .arg((*it)->data(0, kKeyRole).toString(), at(m_folders->viewport(), rect.center()));
      }
    }
    const QRect contentSight = m_contents->viewport()->rect();
    for (int i = 0; i < m_contents->topLevelItemCount(); ++i) {
      QTreeWidgetItem* item = m_contents->topLevelItem(i);
      const QRect rect = m_contents->visualItemRect(item);
      const BrowserEntry* entry = entryOf(item);
      if (entry != nullptr && rect.isValid() && contentSight.contains(rect.center())) {
        // The name's middle, left in the row: a click there hits the name.
        const QPoint point(std::min(rect.left() + 40, rect.center().x()), rect.center().y());
        lines << QStringLiteral("Project browser item %1 at %2").arg(entry->key(), at(m_contents->viewport(), point));
      }
    }
    for (int i = 0; i < m_pathButtons.size() && i < m_pathKeys.size(); ++i) {
      lines << QStringLiteral("Project browser path %1 at %2")
                   .arg(m_pathKeys[i], at(m_pathButtons[i], m_pathButtons[i]->rect().center()));
    }
    lines << QStringLiteral("Project browser search at %1").arg(at(m_search, m_search->rect().center()));
    lines << QStringLiteral("Project browser scope at %1").arg(at(m_scope, m_scope->rect().center()));
    for (const QAbstractButton* button : std::initializer_list<const QAbstractButton*>{m_other, m_readOnly, m_open,
                                                                                       m_cancel}) {
      if (button != nullptr) {
        lines << QStringLiteral("Project browser button %1 at %2")
                     .arg(buttonName(button), at(button, button->rect().center()));
      }
    }
    const QString joined = lines.join(QLatin1Char('\n'));
    if (joined == m_loggedPlaces) {
      return;
    }
    m_loggedPlaces = joined;
    for (const QString& line : std::as_const(lines)) {
      qDebug().noquote() << line;
    }
  });
}

} // namespace

std::optional<ProjectBrowserResult> browseProjects(QWidget* parent, const ProjectBrowserOptions& options) {
  BrowserDialog dialog(parent, options);
  return dialog.run();
}

} // namespace mitcad
