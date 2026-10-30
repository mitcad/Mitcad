// SPDX-License-Identifier: MIT
#include "TexturePreferences.hpp"

#include <algorithm>

#include <QCheckBox>
#include <QDir>
#include <QFileInfo>
#include <QFormLayout>
#include <QHBoxLayout>
#include <QLabel>
#include <QListWidget>
#include <QPushButton>
#include <QTimer>
#include <QVBoxLayout>
#include <QtLogging>

#include "../framework/Dialogs.hpp"
#include "../platform/MacDialogs.hpp"

namespace mitcad {

TexturePreferencesBox::TexturePreferencesBox(QWidget* parent)
#ifdef Q_OS_MACOS
    : QWidget(parent),
#else
    : QGroupBox(tr("Textures"), parent),
#endif
      m_settings(TextureSettings::load()) {
  auto* layout = new QVBoxLayout(this);
  // The list's name, as AI Assistants' Folders for designs; how the maps
  // are named is in the user guide (mitcad#732).
  auto* title = new QLabel(tr("Texture folders"));
  layout->addWidget(title);
  m_folders = new QListWidget;
  m_folders->setObjectName(QStringLiteral("textureFolders"));
  // Named also while it is empty.
  m_folders->setAccessibleName(tr("Texture folders"));
  title->setBuddy(m_folders);
  m_folders->setSelectionMode(QAbstractItemView::ExtendedSelection);
  m_folders->setTextElideMode(Qt::ElideMiddle);
  m_folders->setHorizontalScrollBarPolicy(Qt::ScrollBarAlwaysOff);
  m_folders->setMinimumHeight(90);
  for (const QString& folder : std::as_const(m_settings.folders)) {
    m_folders->addItem(QDir::toNativeSeparators(folder));
  }
  // The list takes the page's height.
  layout->addWidget(m_folders, 1);
  mac::stylePreview(m_folders);
  // Add… chooses a folder, Remove takes the selected ones off the list.
  m_add = new QPushButton(tr("Add…"));
  m_add->setObjectName(QStringLiteral("textureFolderAdd"));
  m_add->setToolTip(tr("Chooses a folder to add"));
  m_remove = new QPushButton(tr("Remove"));
  m_remove->setObjectName(QStringLiteral("textureFolderRemove"));
  m_remove->setToolTip(tr("Removes the selected folders from the list (the files stay)"));
  m_remove->setEnabled(false);
  for (QPushButton* button : {m_add, m_remove}) {
    button->setAutoDefault(false);
  }
  auto* controls = new QHBoxLayout;
#ifdef Q_OS_MACOS
  controls->setSpacing(0);
#endif
  controls->addWidget(m_add);
  controls->addWidget(m_remove);
  controls->addStretch(1);
  mac::styleListButtons(m_add, m_remove);
  layout->addLayout(controls);
  const bool network = std::any_of(textureSourceRegistry().cbegin(), textureSourceRegistry().cend(),
                                   [](const TextureSourceEntry& entry) { return entry.usesNetwork; });
  if (network) {
    m_network = new QCheckBox(tr("Allow texture sources on the &internet"));
    m_network->setChecked(m_settings.allowNetwork);
    m_network->setToolTip(tr("Online texture libraries are searched only when this is on"));
    layout->addWidget(m_network);
    connect(m_network, &QCheckBox::toggled, this, &TexturePreferencesBox::changed);
  }

  connect(m_add, &QPushButton::clicked, this, [this] {
    const QString folder = dialogDirectory(this, tr("Texture Folder"));
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

void TexturePreferencesBox::showEvent(QShowEvent* event) {
  TexturePreferencesBase::showEvent(event);
  // Where Add… is, in the main window's coordinates (UI tests).
  QTimer::singleShot(0, this, [this] {
    QWidget* main = window()->parentWidget() != nullptr ? window()->parentWidget()->window() : nullptr;
    if (main != nullptr && m_add->isVisible()) {
      const QPoint at = m_add->mapToGlobal(m_add->rect().center()) - main->mapToGlobal(QPoint(0, 0));
      qDebug().noquote() << QStringLiteral("Preferences texture folder add at %1,%2").arg(at.x()).arg(at.y());
    }
  });
}

void TexturePreferencesBox::addFolder(const QString& typed) {
  if (typed.trimmed().isEmpty()) {
    return;
  }
  const QString folder = QDir::cleanPath(QFileInfo(QDir::fromNativeSeparators(typed.trimmed())).absoluteFilePath());
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

TextureSettings TexturePreferencesBox::chosen() const {
  // The browser's resolution as it is now (Edit Appearances keeps it).
  TextureSettings settings = TextureSettings::load();
  settings.folders.clear();
  for (int i = 0; i < m_folders->count(); ++i) {
    settings.folders << QDir::fromNativeSeparators(m_folders->item(i)->text());
  }
  settings.allowNetwork = m_network != nullptr ? m_network->isChecked() : m_settings.allowNetwork;
  return settings;
}

} // namespace mitcad
