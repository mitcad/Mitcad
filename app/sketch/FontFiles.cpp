// SPDX-License-Identifier: MIT
#include "FontFiles.hpp"

#include <QDir>
#include <QDirIterator>
#include <QFileInfo>
#include <QFont>
#include <QFontDatabase>
#include <QHBoxLayout>
#include <QHash>
#include <QLabel>
#include <QListWidget>
#include <QPainter>
#include <QPushButton>
#include <QSettings>
#include <QTimer>
#include <QVBoxLayout>
#include <QtLogging>

#include "../framework/Dialogs.hpp"
#include "../platform/MacDialogs.hpp"

namespace mitcad::sketch {

namespace {

const QString kChoicePrefix = QStringLiteral("file:");

QString cleanFolder(const QString& typed) {
  return QDir::cleanPath(QFileInfo(QDir::fromNativeSeparators(typed.trimmed())).absoluteFilePath());
}

} // namespace

QStringList savedFontFolders() {
  QStringList folders;
  for (const QString& folder : QSettings().value(QStringLiteral("fonts/folders")).toStringList()) {
    const QString clean = QDir::cleanPath(QDir::fromNativeSeparators(folder.trimmed()));
    if (!folder.trimmed().isEmpty() && !folders.contains(clean)) {
      folders << clean;
    }
  }
  return folders;
}

void saveFontFolders(const QStringList& folders) {
  QSettings().setValue(QStringLiteral("fonts/folders"), folders);
  forgetFolderFonts();
}

QStringList fontFilesIn(const QString& folder) {
  QStringList files;
  QDirIterator it(folder,
                  {QStringLiteral("*.ttf"), QStringLiteral("*.otf"), QStringLiteral("*.ttc"), QStringLiteral("*.woff"),
                   QStringLiteral("*.woff2")},
                  QDir::Files, QDirIterator::Subdirectories);
  while (it.hasNext()) {
    files << QDir::cleanPath(it.next());
  }
  files.sort(Qt::CaseInsensitive);
  return files;
}

const QStringList& installedFontFamilies() {
  static const QStringList families = [] {
    QStringList all;
    for (const QString& family : QFontDatabase::families()) {
      if (!QFontDatabase::isPrivateFamily(family)) {
        all << family;
      }
    }
    return all;
  }();
  return families;
}

QString fontFileFamily(const QString& path) {
  installedFontFamilies(); // before the file's family joins them
  static QHash<QString, QString> families;
  const auto found = families.constFind(path);
  if (found != families.constEnd()) {
    return *found;
  }
  QString family;
  const int id = QFontDatabase::addApplicationFont(path);
  if (id >= 0) {
    const QStringList names = QFontDatabase::applicationFontFamilies(id);
    if (!names.isEmpty()) {
      family = names.first();
    }
  }
  families.insert(path, family);
  return family;
}

QVector<FolderFont> folderFonts(const QStringList& folders) {
  QVector<FolderFont> fonts;
  for (const QString& folder : folders) {
    for (const QString& path : fontFilesIn(folder)) {
      const QString family = fontFileFamily(path);
      fonts.append({path, family.isEmpty() ? QFileInfo(path).completeBaseName() : family});
    }
  }
  return fonts;
}

namespace {

struct FolderFontCache {
  bool found = false;
  QVector<FolderFont> fonts;
};

FolderFontCache& folderFontCache() {
  static FolderFontCache cache;
  return cache;
}

} // namespace

const QVector<FolderFont>& savedFolderFonts() {
  FolderFontCache& cache = folderFontCache();
  if (!cache.found) {
    cache.fonts = folderFonts(savedFontFolders());
    cache.found = true;
  }
  return cache.fonts;
}

void forgetFolderFonts() { folderFontCache() = FolderFontCache(); }

QString findFontFile(const QString& path, const QStringList& folders) {
  if (path.isEmpty() || QFileInfo(path).isFile()) {
    return path;
  }
  const QString name = QFileInfo(QDir::fromNativeSeparators(path)).fileName();
  for (const QString& folder : folders) {
    for (const QString& file : fontFilesIn(folder)) {
      if (QFileInfo(file).fileName() == name) {
        return file;
      }
    }
  }
  return path;
}

QString fontFileChoice(const QString& path) { return kChoicePrefix + path; }

QString fontFileOfChoice(const QString& choice) {
  return choice.startsWith(kChoicePrefix) ? choice.mid(kChoicePrefix.size()) : QString();
}

QString fontFileLabel(const QString& path) {
  const QString family = fontFileFamily(path);
  const QString name = QFileInfo(path).fileName();
  return family.isEmpty() ? name : QStringLiteral("%1 (%2)").arg(family, name);
}

QImage fontPreview(const QString& choice) {
  static QHash<QString, QImage> previews;
  const auto found = previews.constFind(choice);
  if (found != previews.constEnd()) {
    return *found;
  }
  const QString file = fontFileOfChoice(choice);
  const QString family = file.isEmpty() ? choice : fontFileFamily(file);
  QImage image;
  if (!family.isEmpty()) {
    image = QImage(96, 96, QImage::Format_ARGB32_Premultiplied);
    image.setDevicePixelRatio(2.0);
    image.fill(Qt::transparent);
    QPainter painter(&image);
    painter.setRenderHint(QPainter::TextAntialiasing);
    QFont font(family);
    font.setPixelSize(30);
    painter.setFont(font);
    painter.setPen(QColor(60, 60, 60));
    painter.drawText(QRectF(0, 0, 48, 48), Qt::AlignCenter, QStringLiteral("Ag"));
  }
  previews.insert(choice, image);
  return image;
}

FontPreferencesBox::FontPreferencesBox(QWidget* parent)
#ifdef MITCAD_MAC_UI
    : QWidget(parent) {
#else
    : QGroupBox(tr("Fonts"), parent) {
#endif
  auto* layout = new QVBoxLayout(this);
  // The list's name, as AI Assistants' Folders for designs; the font files
  // taken and what a design keeps are in the user guide (mitcad#732).
  auto* title = new QLabel(tr("Font folders"));
  layout->addWidget(title);
  m_folders = new QListWidget;
  m_folders->setObjectName(QStringLiteral("fontFolders"));
  // Named also while it is empty.
  m_folders->setAccessibleName(tr("Font folders"));
  title->setBuddy(m_folders);
  m_folders->setSelectionMode(QAbstractItemView::ExtendedSelection);
  m_folders->setMinimumHeight(90);
  for (const QString& folder : savedFontFolders()) {
    m_folders->addItem(QDir::toNativeSeparators(folder));
  }
  // The list takes the page's height.
  layout->addWidget(m_folders, 1);
  mac::stylePreview(m_folders);
  // Add… chooses a folder, Remove takes the selected ones off the list.
  m_add = new QPushButton(tr("Add…"));
  m_add->setObjectName(QStringLiteral("fontFolderAdd"));
  m_add->setToolTip(tr("Chooses a folder to add"));
  m_remove = new QPushButton(tr("Remove"));
  m_remove->setToolTip(tr("Removes the selected folders from the list (the files stay)"));
  m_remove->setEnabled(false);
  for (QPushButton* button : {m_add, m_remove}) {
    button->setAutoDefault(false);
  }
  auto* controls = new QHBoxLayout;
#ifdef MITCAD_MAC_UI
  controls->setSpacing(0);
#endif
  controls->addWidget(m_add);
  controls->addWidget(m_remove);
  controls->addStretch(1);
  mac::styleListButtons(m_add, m_remove);
  layout->addLayout(controls);

  connect(m_add, &QPushButton::clicked, this, [this] {
    const QString folder = dialogDirectory(this, tr("Font Folder"));
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

void FontPreferencesBox::showEvent(QShowEvent* event) {
  FontPreferencesBase::showEvent(event);
  // Where Add… is, in the main window's coordinates (UI tests).
  QTimer::singleShot(0, this, [this] {
    QWidget* main = window()->parentWidget() != nullptr ? window()->parentWidget()->window() : nullptr;
    if (main != nullptr && m_add->isVisible()) {
      const QPoint at = m_add->mapToGlobal(m_add->rect().center()) - main->mapToGlobal(QPoint(0, 0));
      qDebug().noquote() << QStringLiteral("Preferences font folder add at %1,%2").arg(at.x()).arg(at.y());
    }
  });
}

void FontPreferencesBox::addFolder(const QString& typed) {
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

QStringList FontPreferencesBox::chosen() const {
  QStringList folders;
  for (int i = 0; i < m_folders->count(); ++i) {
    folders << QDir::fromNativeSeparators(m_folders->item(i)->text());
  }
  return folders;
}

} // namespace mitcad::sketch
