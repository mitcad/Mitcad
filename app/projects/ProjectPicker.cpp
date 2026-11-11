// SPDX-License-Identifier: MIT
#include "ProjectPicker.hpp"

#include <algorithm>
#include <functional>
#include <utility>

#include <QCoreApplication>
#include <QDialog>
#include <QDialogButtonBox>
#include <QDir>
#include <QEvent>
#include <QFileInfo>
#include <QHBoxLayout>
#include <QHash>
#include <QIcon>
#include <QJsonObject>
#include <QKeyEvent>
#include <QLabel>
#include <QLineEdit>
#include <QPalette>
#include <QPixmap>
#include <QPushButton>
#include <QStringList>
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
#include "../framework/Theme.hpp"
#include "PickerNewFolder.hpp"
#include "ProjectsTree.hpp"
#include "../framework/UiSizes.hpp"

namespace mitcad {
namespace {

// The file's texts in the context ProjectPicker, where lupdate records them
// too (mitcad#1472).
struct Text {
  Q_DECLARE_TR_FUNCTIONS(ProjectPicker)
};

// What a row stands for.
constexpr int kTypeRole = Qt::UserRole + 1;
constexpr int kRootRole = Qt::UserRole + 2;
constexpr int kPathRole = Qt::UserRole + 3; // relative to the project, '/'
constexpr int kKeyRole = Qt::UserRole + 4;  // for the logs
constexpr int kProjectRow = 1;
constexpr int kFolderRow = 2;
constexpr int kDesignRow = 3;
constexpr int kNewFolderRow = 4; // a new folder's name being typed (mitcad#1649)
constexpr int kMessageRow = 5;   // its refusal, below it

// The design files' suffix.
QString suffix() { return QStringLiteral(".mitcad"); }

// Whether a project of this kind is offered.
bool offered(const ProjectRow& row, ProjectPickerOptions::Mode mode) {
  switch (row.kind) {
  case ProjectRow::Kind::Local:
  case ProjectRow::Kind::Cloud:
    return true;
  case ProjectRow::Kind::NoHistory:
    // Open Project starts its history.
    return mode == ProjectPickerOptions::Mode::Project;
  case ProjectRow::Kind::Missing:
  case ProjectRow::Kind::Unreadable:
    break;
  }
  return false;
}

QString kindText(const ProjectRow& row) {
  switch (row.kind) {
  case ProjectRow::Kind::Cloud:
    return QStringLiteral("cloud");
  case ProjectRow::Kind::NoHistory:
    return QStringLiteral("no versions");
  default:
    break;
  }
  return QStringLiteral("local");
}

// The button's text without its mnemonic and dots ("Another Folder").
QString buttonName(const QPushButton* button) {
  return button->text().remove(QLatin1Char('&')).remove(QStringLiteral("...")).remove(QChar(0x2026)).trimmed();
}

// The window the UI tests' places are relative to: the main window, also
// for a picker opened from another dialog (Version History).
QWidget* placesWindow(QWidget* parent) {
  QWidget* top = parent;
  while (top != nullptr && top->parentWidget() != nullptr) {
    top = top->parentWidget();
  }
  return top != nullptr ? top->window() : nullptr;
}

// The picker's list, where New Folder's name is typed in place
// (mitcad#1649). Return in the name's editor ends it with the name taken,
// and goes no further (the dialog's default button would save); every other
// end (Esc, the focus elsewhere, another row made current) takes nothing.
class PickerTree : public QTreeWidget {
public:
  // Every end of the name's editor, once the view is done with it.
  std::function<void(bool accepted)> editorClosed;

  bool editing() const { return state() == QAbstractItemView::EditingState; }

  // The name's editor on a row, if it is open.
  QLineEdit* nameEditor(const QTreeWidgetItem* item) const {
    return item == nullptr ? nullptr : qobject_cast<QLineEdit*>(indexWidget(indexFromItem(item, 0)));
  }

  // Ends the open editor without its name.
  void cancelEdit() {
    if (!editing()) {
      return;
    }
    if (QWidget* editor = indexWidget(currentIndex())) {
      closeEditor(editor, QAbstractItemDelegate::RevertModelCache);
    }
  }

protected:
  void closeEditor(QWidget* editor, QAbstractItemDelegate::EndEditHint hint) override {
    const bool accepted = m_returned && hint != QAbstractItemDelegate::RevertModelCache;
    m_returned = false;
    QTreeWidget::closeEditor(editor, hint);
    if (editorClosed) {
      editorClosed(accepted);
    }
  }

  void keyPressEvent(QKeyEvent* event) override {
    // Return in the editor (passed on by it): the name is committed after
    // this, and taken.
    if (editing() && (event->key() == Qt::Key_Return || event->key() == Qt::Key_Enter)) {
      m_returned = true;
      event->accept();
      return;
    }
    QTreeWidget::keyPressEvent(event);
  }

private:
  bool m_returned = false;
};

} // namespace

QString designNameOf(const QString& typed) {
  QString name = typed.trimmed();
  if (name.endsWith(suffix(), Qt::CaseInsensitive)) {
    name.chop(suffix().size());
    name = name.trimmed();
  }
  if (name.isEmpty() || name == QLatin1String(".") || name == QLatin1String("..")) {
    return {};
  }
  // A name, not a path; nothing a file name cannot hold on any platform.
  for (const QChar c : std::as_const(name)) {
    if (c == QLatin1Char('/') || c == QLatin1Char('\\') || c == QLatin1Char(':') || c == QLatin1Char('*') ||
        c == QLatin1Char('?') || c == QLatin1Char('"') || c == QLatin1Char('<') || c == QLatin1Char('>') ||
        c == QLatin1Char('|') || c.unicode() < 0x20) {
      return {};
    }
  }
  return name;
}

QString folderInProject(const QString& root, const QString& folder) {
  if (root.isEmpty() || folder.isEmpty()) {
    return {};
  }
  const QString relative = QDir::cleanPath(QDir(QDir::cleanPath(root)).relativeFilePath(QDir::cleanPath(folder)));
  if (relative.isEmpty() || relative == QLatin1String(".") || relative == QLatin1String("..") ||
      relative.startsWith(QLatin1String("../")) || QDir::isAbsolutePath(relative)) {
    return {};
  }
  return relative;
}

std::optional<ProjectPickerResult> pickInProjects(QWidget* parent, const ProjectPickerOptions& options) {
  using Mode = ProjectPickerOptions::Mode;
  const Mode mode = options.mode;
  const QString title = options.title;
  QDialog dialog(parent);
  dialog.setWindowTitle(title);
  dialog.resize(460, 440);
  auto* layout = new QVBoxLayout(&dialog);
  if (!options.text.isEmpty()) {
    auto* text = new QLabel(options.text);
    text->setWordWrap(true);
    text->setTextFormat(Qt::PlainText);
    layout->addWidget(text);
  }
  auto* tree = new PickerTree;
  panel::setCompactIndentation(tree, panel::Indent::Tree);
  tree->setHeaderHidden(true);
  tree->setColumnCount(1);
  tree->setIconSize(uiIconSize(16, 16));
  tree->setUniformRowHeights(true);
  tree->setSelectionMode(QAbstractItemView::SingleSelection);
  tree->setEditTriggers(QAbstractItemView::NoEditTriggers);
  tree->setMinimumSize(360, 220);
  layout->addWidget(tree, 1);
  auto* status = new QLabel(Text::tr("Reading the projects…"));
  status->setWordWrap(true);
  status->setTextFormat(Qt::PlainText);
  layout->addWidget(status);
  // Place: the design's name, a clash shown below it.
  QLineEdit* name = nullptr;
  QLabel* clash = nullptr;
  if (mode == Mode::Place) {
    auto* row = new QHBoxLayout;
    auto* label = new QLabel(Text::tr("&Name:"));
    name = new QLineEdit(options.name);
    label->setBuddy(name);
    row->addWidget(label);
    row->addWidget(name, 1);
    row->addWidget(new QLabel(suffix()));
    layout->addLayout(row);
    clash = new QLabel;
    clash->setWordWrap(true);
    clash->setTextFormat(Qt::PlainText);
    QPalette palette = clash->palette();
    palette.setColor(QPalette::WindowText, errorColor(palette));
    clash->setPalette(palette);
    clash->hide();
    layout->addWidget(clash);
  }
  auto* buttonsRow = new QHBoxLayout;
  QPushButton* other = nullptr;
  QPushButton* newProject = nullptr;
  QPushButton* newFolder = nullptr;
  if (mode == Mode::Place && options.makeFolder) {
    // A folder in the selected project or folder, named in the list
    // (mitcad#1649).
    newFolder = new QPushButton(Text::tr("New &Folder"));
    newFolder->setObjectName(QStringLiteral("projectPickerNewFolder"));
    newFolder->setAutoDefault(false);
    newFolder->setEnabled(false);
    buttonsRow->addWidget(newFolder);
  }
  if (mode == Mode::Place && !options.newProjectText.isEmpty()) {
    newProject = new QPushButton(options.newProjectText);
    newProject->setAutoDefault(false);
    buttonsRow->addWidget(newProject);
  }
  if (!options.otherText.isEmpty()) {
    other = new QPushButton(options.otherText);
    other->setAutoDefault(false);
    buttonsRow->addWidget(other);
  }
  buttonsRow->addStretch(1);
  auto* buttons = new QDialogButtonBox(QDialogButtonBox::Ok | QDialogButtonBox::Cancel);
  QPushButton* ok = buttons->button(QDialogButtonBox::Ok);
  QPushButton* cancel = buttons->button(QDialogButtonBox::Cancel);
  if (!options.acceptText.isEmpty()) {
    ok->setText(options.acceptText);
  }
  ok->setDefault(true);
  ok->setEnabled(false);
  cancel->setAutoDefault(false);
  buttonsRow->addWidget(buttons);
  layout->addLayout(buttonsRow);

  std::optional<ProjectPickerResult> chosen;
  bool closed = false;
  // The project names by root (shown), and the keys of the logs: the
  // name, or the folder when another listed project has the same name.
  QHash<QString, QString> names;
  QHash<QString, QString> keys;
  QString loggedPlaces;
  QString loggedSelection;
  QString loggedTarget;
  // New Folder (mitcad#1649): whether the projects are read, the row whose
  // name is being typed and the row it is in, a refusal's row below it, and
  // the editor's turn (a later one makes an older end void).
  bool projectsRead = false;
  QTreeWidgetItem* naming = nullptr;
  QTreeWidgetItem* namingHost = nullptr;
  QTreeWidgetItem* refusal = nullptr;
  int namingSerial = 0;
  QString loggedNewFolder;

  const auto keyOf = [](const QTreeWidgetItem* item) {
    return item == nullptr ? QString() : item->data(0, kKeyRole).toString();
  };
  const auto typeOf = [](const QTreeWidgetItem* item) { return item == nullptr ? 0 : item->data(0, kTypeRole).toInt(); };

  // Where the rows in sight, the name field and the buttons are (UI tests).
  const std::function<void()> logPlaces = [&] {
    TestSync::singleShot(50, &dialog, [&] {
      QCoreApplication::sendPostedEvents(nullptr, QEvent::LayoutRequest);
      QWidget* window = placesWindow(parent);
      const QPoint origin = window != nullptr ? window->mapToGlobal(QPoint(0, 0)) : QPoint();
      const auto at = [&origin](const QWidget* widget, const QPoint& point) {
        const QPoint global = widget->mapToGlobal(point) - origin;
        return QStringLiteral("%1,%2").arg(global.x()).arg(global.y());
      };
      QStringList lines;
      const QRect sight = tree->viewport()->rect();
      for (QTreeWidgetItemIterator it(tree); *it != nullptr; ++it) {
        const QRect rect = tree->visualItemRect(*it);
        if (rect.isValid() && sight.contains(rect.center())) {
          lines << QStringLiteral("Project picker row %1 at %2").arg(keyOf(*it), at(tree->viewport(), rect.center()));
        }
      }
      if (name != nullptr) {
        lines << QStringLiteral("Project picker name at %1").arg(at(name, name->rect().center()));
      }
      for (const QPushButton* button : {newFolder, newProject, other, ok, cancel}) {
        if (button != nullptr) {
          lines << QStringLiteral("Project picker button %1 at %2")
                       .arg(buttonName(button), at(button, button->rect().center()));
        }
      }
      const QString joined = lines.join(QLatin1Char('\n'));
      if (joined == loggedPlaces) {
        return;
      }
      loggedPlaces = joined;
      for (const QString& line : std::as_const(lines)) {
        qDebug().noquote() << line;
      }
    });
  };

  // The accept button, the clash and the logs follow the selection and the
  // name.
  const std::function<void()> update = [&] {
    QTreeWidgetItem* item = tree->currentItem();
    const int type = typeOf(item);
    bool acceptable = false;
    if (mode == Mode::Place) {
      const QString designName = designNameOf(name->text());
      const bool place = type == kProjectRow || type == kFolderRow;
      QString state;
      QString target;
      if (place) {
        const QString root = item->data(0, kRootRole).toString();
        const QString folder = item->data(0, kPathRole).toString();
        target = joinPath(folder, designName + suffix());
        const QString file = QDir::cleanPath(QDir(root).filePath(target));
        if (designName.isEmpty()) {
          state = QStringLiteral("invalid");
          clash->setText(name->text().trimmed().isEmpty()
                             ? Text::tr("Type the design's name.")
                             : Text::tr("A design's name cannot hold / \\ : * ? \" < > |, nor be . or .."));
        } else if (QFileInfo::exists(file) && !(!options.ownFile.isEmpty() && sameRoot(file, options.ownFile))) {
          state = QStringLiteral("exists");
          clash->setText(Text::tr("%1 is in %2 already: choose another name or folder.")
                             .arg(designName + suffix(),
                                  folder.isEmpty() ? names.value(root) : joinPath(names.value(root), folder)));
        } else {
          state = QStringLiteral("free");
          acceptable = true;
        }
        clash->setVisible(state != QLatin1String("free"));
        const QString line =
            QStringLiteral("Project picker target %1/%2: %3").arg(keys.value(root), target, state);
        if (line != loggedTarget) {
          loggedTarget = line;
          qInfo().noquote() << line;
        }
      } else {
        clash->hide();
      }
    } else if (mode == Mode::Design) {
      acceptable = type == kDesignRow;
    } else {
      acceptable = type == kProjectRow;
    }
    ok->setEnabled(acceptable);
    if (newFolder != nullptr) {
      // New Folder: in a project or a folder of one that can be written,
      // once the projects are read and while no name is being typed; the
      // tooltip says why it waits.
      FolderHost host = FolderHost::None;
      bool writable = true;
      if (!projectsRead) {
        host = FolderHost::Reading;
      } else if (naming != nullptr) {
        host = FolderHost::Naming;
      } else if (type == kProjectRow || type == kFolderRow) {
        host = FolderHost::Place;
        const QString root = item->data(0, kRootRole).toString();
        const QString folder = item->data(0, kPathRole).toString();
        writable = QFileInfo(folder.isEmpty() ? root : QDir(root).filePath(folder)).isWritable();
      }
      const QString why = newFolderBlocked(host, writable);
      newFolder->setEnabled(why.isEmpty());
      newFolder->setToolTip(why.isEmpty() ? Text::tr("A new folder in the selected project or folder, named here")
                                          : why);
      const QString line = why.isEmpty() ? QStringLiteral("Project picker New Folder: enabled")
                                         : QStringLiteral("Project picker New Folder: disabled: %1").arg(why);
      if (line != loggedNewFolder) {
        loggedNewFolder = line;
        qInfo().noquote() << line;
      }
    }
  };

  const auto addRow = [&](QTreeWidgetItem* parentItem, int type, const ProjectRow& row, const QString& path,
                          const QString& text) {
    auto* item = new QTreeWidgetItem(parentItem);
    item->setText(0, text);
    item->setData(0, kTypeRole, type);
    item->setData(0, kRootRole, row.root);
    item->setData(0, kPathRole, path);
    item->setData(0, kKeyRole,
                  joinPath(keys.value(row.root), type == kFolderRow ? path + QLatin1Char('/') : path));
    item->setToolTip(0, QDir::toNativeSeparators(QDir(row.root).filePath(path)));
    item->setIcon(0, themeIcon(type == kFolderRow ? QStringLiteral("folder") : QStringLiteral("project")));
    item->setFlags(Qt::ItemIsEnabled | Qt::ItemIsSelectable);
    return item;
  };

  // The projects read: their rows, the preselection.
  const auto fill = [&](const QJsonObject& answer) {
    QVector<ProjectRow> rows;
    for (const ProjectRow& row : projectRows(answer)) {
      if (offered(row, mode)) {
        rows << row;
      }
    }
    // The preselected (or current) project first, the others by name.
    std::stable_sort(rows.begin(), rows.end(), [&options](const ProjectRow& a, const ProjectRow& b) {
      const bool aFirst = !options.root.isEmpty() && sameRoot(a.root, options.root);
      const bool bFirst = !options.root.isEmpty() && sameRoot(b.root, options.root);
      if (aFirst != bFirst) {
        return aFirst;
      }
      return a.name.compare(b.name, Qt::CaseInsensitive) < 0;
    });
    QTreeWidgetItem* select = nullptr;
    QStringList listed;
    for (const ProjectRow& row : std::as_const(rows)) {
      int same = 0;
      for (const ProjectRow& other : std::as_const(rows)) {
        same += other.name == row.name ? 1 : 0;
      }
      const QString key = same > 1 ? row.root : row.name;
      names.insert(row.root, row.name);
      keys.insert(row.root, key);
      listed << QStringLiteral("%1 (%2)").arg(row.name, kindText(row));
      qInfo().noquote() << QStringLiteral("Project picker project %1: %2, %3").arg(key, kindText(row), row.root);
      auto* top = new QTreeWidgetItem(tree);
      top->setText(0, row.name);
      top->setData(0, kTypeRole, kProjectRow);
      top->setData(0, kRootRole, row.root);
      top->setData(0, kPathRole, QString());
      top->setData(0, kKeyRole, key);
      top->setFlags(Qt::ItemIsEnabled | Qt::ItemIsSelectable);
      top->setIcon(0, themeIcon(row.kind == ProjectRow::Kind::Cloud ? QStringLiteral("web") : QStringLiteral("home")));
      QStringList tip{row.kind == ProjectRow::Kind::Cloud ? Text::tr("Cloud project") : Text::tr("Local project")};
      if (!row.remoteUrl.isEmpty()) {
        tip << row.remoteUrl;
      }
      tip << QDir::toNativeSeparators(row.root);
      if (row.kind == ProjectRow::Kind::NoHistory) {
        tip << Text::tr("It keeps no versions yet: Open starts its history.");
      }
      top->setToolTip(0, tip.join(QLatin1Char('\n')));
      const bool preferred = !options.root.isEmpty() && sameRoot(row.root, options.root);
      if (mode != Mode::Project) {
        QHash<QString, QTreeWidgetItem*> folders;
        folders.insert(QString(), top);
        std::function<QTreeWidgetItem*(const QString&)> ensure = [&](const QString& path) -> QTreeWidgetItem* {
          if (QTreeWidgetItem* found = folders.value(path)) {
            return found;
          }
          QTreeWidgetItem* folder = addRow(ensure(dirOf(path)), kFolderRow, row, path, lastPart(path));
          folders.insert(path, folder);
          return folder;
        };
        if (mode == Mode::Place) {
          // The project's real folders, as the panel shows them.
          for (const QString& folder : row.folders) {
            ensure(folder);
          }
        } else {
          // Its designs (the open design not offered), in their folders.
          for (const ProjectDesignRow& design : row.designs) {
            const QString file = QDir::cleanPath(QDir(row.root).filePath(design.path));
            if (!options.ownFile.isEmpty() && sameRoot(file, options.ownFile)) {
              continue;
            }
            QTreeWidgetItem* item = addRow(ensure(dirOf(design.path)), kDesignRow, row, design.path,
                                           lastPart(design.path));
            if (design.version > 0) {
              item->setToolTip(0, QStringLiteral("%1\nv%2").arg(item->toolTip(0)).arg(design.version));
            }
          }
        }
        if (preferred) {
          for (QTreeWidgetItem* folder : std::as_const(folders)) {
            folder->setExpanded(true);
          }
          if (mode == Mode::Place) {
            select = folders.value(options.folder, top);
          }
        }
      }
      if (preferred && select == nullptr) {
        select = top;
      }
    }
    if (select == nullptr && mode != Mode::Design && tree->topLevelItemCount() > 0) {
      select = tree->topLevelItem(0);
    }
    if (select != nullptr) {
      for (QTreeWidgetItem* up = select->parent(); up != nullptr; up = up->parent()) {
        up->setExpanded(true);
      }
      tree->setCurrentItem(select);
      tree->scrollToItem(select);
    }
    const QString error =
        answer.value(QStringLiteral("error")).toObject().value(QStringLiteral("message")).toString();
    if (!error.isEmpty()) {
      status->setText(Text::tr("The projects could not be read: %1").arg(error));
    } else if (rows.isEmpty()) {
      status->setText(mode == Mode::Project ? Text::tr("No projects are listed.")
                                            : Text::tr("No projects with versions are listed."));
    } else if (mode == Mode::Design) {
      status->setText(Text::tr("Choose a design."));
    }
    status->setVisible(!error.isEmpty() || rows.isEmpty() || mode == Mode::Design);
    projectsRead = true;
    qInfo().noquote() << QStringLiteral("Project picker %1: %2 projects: %3")
                             .arg(title)
                             .arg(rows.size())
                             .arg(listed.join(QStringLiteral(" | ")));
    update();
    logPlaces();
  };

  const auto finish = [&](std::optional<ProjectPickerResult> result, const QString& line) {
    if (closed) {
      return;
    }
    closed = true;
    chosen = std::move(result);
    qInfo().noquote() << QStringLiteral("Project picker %1: %2").arg(title, line);
    dialog.accept();
  };
  const auto take = [&] {
    QTreeWidgetItem* item = tree->currentItem();
    if (!ok->isEnabled() || item == nullptr) {
      return;
    }
    ProjectPickerResult result;
    result.root = item->data(0, kRootRole).toString();
    const QString path = item->data(0, kPathRole).toString();
    switch (mode) {
    case Mode::Place:
      result.folder = path;
      result.path = QDir::cleanPath(
          QDir(result.root).filePath(joinPath(path, designNameOf(name->text()) + suffix())));
      break;
    case Mode::Design:
      result.folder = dirOf(path);
      result.path = QDir::cleanPath(QDir(result.root).filePath(path));
      break;
    case Mode::Project:
      result.path = result.root;
      break;
    }
    finish(result, QStringLiteral("chose %1").arg(result.path));
  };

  // New Folder (mitcad#1649): the name typed in place in the list, as the
  // Projects panel's New Folder has it.
  const auto clearRefusal = [&] {
    delete refusal;
    refusal = nullptr;
  };
  // The editor on the new row, after the button's click has ended and
  // given the focus back, so that the editor keeps it; its name selected,
  // so that typing replaces it.
  const auto openNameEditor = [&] {
    const int serial = ++namingSerial;
    TestSync::singleShot(0, &dialog, [&, serial] {
      if (closed || naming == nullptr || serial != namingSerial) {
        return;
      }
      tree->setCurrentItem(naming);
      tree->scrollToItem(naming);
      tree->setFocus(Qt::OtherFocusReason);
      tree->editItem(naming, 0);
      if (QLineEdit* editor = tree->nameEditor(naming)) {
        editor->setFocus(Qt::OtherFocusReason);
        editor->selectAll();
      }
      qInfo().noquote() << QStringLiteral("Project picker: naming a new folder in %1: %2")
                               .arg(keyOf(namingHost), naming->text(0));
      logPlaces();
    });
  };
  // The new row taken away; the row it was in selected again.
  const auto dropNaming = [&] {
    QTreeWidgetItem* host = namingHost;
    clearRefusal();
    QTreeWidgetItem* row = naming;
    naming = nullptr;
    namingHost = nullptr;
    ++namingSerial;
    delete row;
    if (host != nullptr) {
      tree->setCurrentItem(host);
    }
    update();
    logPlaces();
  };
  // A refusal in words in a row of its own below the name, its editor open
  // again with what was typed.
  const auto refuseName = [&](const QString& message) {
    clearRefusal();
    QTreeWidgetItem* host = naming->parent();
    auto* row = new QTreeWidgetItem;
    host->insertChild(host->indexOfChild(naming) + 1, row);
    row->setText(0, message);
    row->setToolTip(0, message);
    const QPixmap mark = themeIcon(QStringLiteral("warning")).pixmap(tree->iconSize());
    QIcon icon;
    icon.addPixmap(mark, QIcon::Normal);
    icon.addPixmap(mark, QIcon::Disabled);
    row->setIcon(0, icon);
    // Not greyed although it takes no clicks.
    row->setForeground(0, tree->palette().brush(QPalette::Active, QPalette::Text));
    row->setData(0, kTypeRole, kMessageRow);
    row->setData(0, kKeyRole, QStringLiteral("%1 (message)").arg(keyOf(naming)));
    row->setFlags(Qt::NoItemFlags);
    refusal = row;
    openNameEditor();
  };
  // The name's editor ended: a name taken makes the folder with the
  // caller's create_folder; else the row goes.
  const auto namingEnded = [&](bool accepted) {
    if (naming == nullptr) {
      return;
    }
    if (!accepted) {
      qInfo().noquote() << QStringLiteral("Project picker: New Folder cancelled");
      dropNaming();
      return;
    }
    const QString typed = naming->text(0).trimmed();
    const QString folderName = folderNameOf(typed);
    if (folderName.isEmpty()) {
      qInfo().noquote() << QStringLiteral("Project picker: New Folder refused (name): \"%1\"").arg(typed);
      naming->setText(0, typed);
      refuseName(Text::tr("A folder's name cannot be empty, start or end with a dot, or hold / \\ < > : \" | ? *."));
      return;
    }
    naming->setText(0, folderName);
    const QString root = naming->data(0, kRootRole).toString();
    const QString path = joinPath(naming->data(0, kPathRole).toString(), folderName);
    const ProjectFolderMade made = options.makeFolder(root, path);
    if (closed || naming == nullptr) {
      return;
    }
    if (made.made) {
      // The row is the folder's now, among the others by name, and
      // selected: Save puts the design there.
      clearRefusal();
      QTreeWidgetItem* row = naming;
      QTreeWidgetItem* host = namingHost;
      naming = nullptr;
      namingHost = nullptr;
      ++namingSerial;
      host->takeChild(host->indexOfChild(row));
      int at = 0;
      while (at < host->childCount() &&
             !(typeOf(host->child(at)) == kFolderRow &&
               host->child(at)->text(0).compare(folderName, Qt::CaseInsensitive) > 0)) {
        ++at;
      }
      host->insertChild(at, row);
      row->setData(0, kTypeRole, kFolderRow);
      row->setData(0, kPathRole, path);
      row->setData(0, kKeyRole, joinPath(keys.value(root), path + QLatin1Char('/')));
      row->setToolTip(0, QDir::toNativeSeparators(QDir(root).filePath(path)));
      row->setFlags(Qt::ItemIsEnabled | Qt::ItemIsSelectable);
      qInfo().noquote() << QStringLiteral("Project picker: New Folder %1").arg(joinPath(keys.value(root), path));
      tree->setCurrentItem(row);
      tree->scrollToItem(row);
      tree->setFocus(Qt::OtherFocusReason);
      update();
      logPlaces();
      return;
    }
    if (made.errorClass.isEmpty()) {
      // Failed or cancelled, and told already.
      qInfo().noquote() << QStringLiteral("Project picker: New Folder not made");
      dropNaming();
      return;
    }
    qInfo().noquote()
        << QStringLiteral("Project picker: New Folder refused (%1): %2").arg(made.errorClass, made.message);
    refuseName(made.message.isEmpty() ? Text::tr("The folder could not be made there.") : made.message);
  };
  // New Folder's click: a row in the selected project or folder, with a
  // name of its own there (New Folder, New Folder 2, ...).
  const auto startNaming = [&] {
    QTreeWidgetItem* host = tree->currentItem();
    const int type = typeOf(host);
    if (naming != nullptr || (type != kProjectRow && type != kFolderRow)) {
      return;
    }
    QStringList taken;
    for (int i = 0; i < host->childCount(); ++i) {
      taken << host->child(i)->text(0);
    }
    // And on disk: a folder or file there that the list does not show.
    const QString root = host->data(0, kRootRole).toString();
    const QString parentPath = host->data(0, kPathRole).toString();
    const QDir dir(parentPath.isEmpty() ? root : QDir(root).filePath(parentPath));
    taken << dir.entryList(QDir::AllEntries | QDir::Hidden | QDir::System | QDir::NoDotAndDotDot);
    host->setExpanded(true);
    auto* row = new QTreeWidgetItem;
    host->insertChild(0, row);
    row->setText(0, freeFolderName(Text::tr("New Folder"), taken));
    row->setIcon(0, themeIcon(QStringLiteral("folder")));
    row->setData(0, kTypeRole, kNewFolderRow);
    row->setData(0, kRootRole, root);
    row->setData(0, kPathRole, parentPath);
    row->setData(0, kKeyRole, QStringLiteral("%1 (new)").arg(keyOf(host)));
    row->setFlags(Qt::ItemIsEnabled | Qt::ItemIsSelectable | Qt::ItemIsEditable);
    naming = row;
    namingHost = host;
    tree->setCurrentItem(row);
    openNameEditor();
  };
  tree->editorClosed = [&](bool accepted) {
    // Once the view is done with the editor.
    const int serial = namingSerial;
    QMetaObject::invokeMethod(
        &dialog,
        [&, serial, accepted] {
          if (!closed && naming != nullptr && serial == namingSerial) {
            namingEnded(accepted);
          }
        },
        Qt::QueuedConnection);
  };

  QObject::connect(tree, &QTreeWidget::currentItemChanged, &dialog, [&](QTreeWidgetItem* item) {
    const QString key = keyOf(item);
    if (!key.isEmpty() && key != loggedSelection) {
      loggedSelection = key;
      qInfo().noquote() << QStringLiteral("Project picker selected %1").arg(key);
    }
    update();
  });
  QObject::connect(tree, &QTreeWidget::itemDoubleClicked, &dialog, [&](QTreeWidgetItem* item) {
    const int type = typeOf(item);
    if ((mode == Mode::Design && type == kDesignRow) || (mode == Mode::Project && type == kProjectRow)) {
      take();
    }
  });
  QObject::connect(tree, &QTreeWidget::itemExpanded, &dialog, [&] { logPlaces(); });
  QObject::connect(tree, &QTreeWidget::itemCollapsed, &dialog, [&] { logPlaces(); });
  if (name != nullptr) {
    QObject::connect(name, &QLineEdit::textChanged, &dialog, [&] { update(); });
  }
  QObject::connect(ok, &QPushButton::clicked, &dialog, [&] { take(); });
  QObject::connect(cancel, &QPushButton::clicked, &dialog, [&] { dialog.reject(); });
  if (newFolder != nullptr) {
    QObject::connect(newFolder, &QPushButton::clicked, &dialog, [&] { startNaming(); });
  }
  if (newProject != nullptr) {
    QObject::connect(newProject, &QPushButton::clicked, &dialog, [&] {
      ProjectPickerResult result;
      result.newProject = true;
      finish(result, QStringLiteral("new project"));
    });
  }
  if (other != nullptr) {
    QObject::connect(other, &QPushButton::clicked, &dialog, [&] {
      ProjectPickerResult result;
      result.other = true;
      finish(result, mode == Mode::Design ? QStringLiteral("another file") : QStringLiteral("another folder"));
    });
  }

  // The projects as the Projects panel lists them, read on a thread.
  const ProjectsSettings settings = ProjectsSettings::load();
  const QStringList known = settings.known;
  const QStringList hidden = settings.hidden;
  const QString folder = projectsDirectory();
  RemoteTask* task = RemoteTask::custom(
      QStringLiteral("project_tree"),
      [known, folder, hidden](const SyncControl&) { return readProjects(known, folder, hidden, QString()); },
      &dialog);
  QObject::connect(task, &RemoteTask::finished, &dialog, [&, task] {
    const QJsonObject answer = task->answer();
    task->deleteLater();
    if (!closed) {
      fill(answer);
    }
  });
  task->start();
  qInfo().noquote() << QStringLiteral("Project picker %1: %2, reading the projects")
                           .arg(title, mode == Mode::Place    ? QStringLiteral("a place")
                                       : mode == Mode::Design ? QStringLiteral("a design")
                                                              : QStringLiteral("a project"));
  if (name != nullptr) {
    // The name typed first; the folder chosen in the list.
    name->setFocus();
    name->selectAll();
  } else {
    tree->setFocus();
  }
  // The buttons' state while the projects are read (New Folder's tooltip).
  update();
  logPlaces();
  prepareModal(&dialog);
  execDialog(&dialog);
  // The editor's end calls into this function's state no more.
  tree->editorClosed = nullptr;
  if (!closed) {
    closed = true;
    qInfo().noquote() << QStringLiteral("Project picker %1: cancelled").arg(title);
    return std::nullopt;
  }
  return chosen;
}

} // namespace mitcad
