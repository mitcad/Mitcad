// SPDX-License-Identifier: MIT
#include "TextureBrowser.hpp"

#include <QComboBox>
#include <QHBoxLayout>
#include <QLabel>
#include <QListView>
#include <QPainter>
#include <QPushButton>
#include <QScrollBar>
#include <QSignalBlocker>
#include <QVBoxLayout>
#include <QtLogging>

#include "../framework/Theme.hpp"

namespace mitcad {

TextureAssetModel::TextureAssetModel(QObject* parent) : QAbstractListModel(parent) {
  // An empty tile while the thumbnail comes.
  m_placeholder = QPixmap(kThumbnailSize, kThumbnailSize);
  m_placeholder.fill(QColor(128, 128, 128, 60));
}

void TextureAssetModel::setAssets(TextureSource* source, const QVector<TextureAsset>& assets) {
  beginResetModel();
  if (source != m_source) {
    m_thumbnails.clear();
    m_asked.clear();
    ++m_generation;
  }
  m_source = source;
  m_assets = assets;
  endResetModel();
}

bool TextureAssetModel::updateAsset(const TextureAsset& asset) {
  for (int row = 0; row < m_assets.size(); ++row) {
    TextureAsset& listed = m_assets[row];
    if (listed.id != asset.id) {
      continue;
    }
    if (listed.resolutions == asset.resolutions && listed.bytes == asset.bytes && listed.size == asset.size) {
      return false;
    }
    listed = asset;
    const QModelIndex changed = index(row);
    emit dataChanged(changed, changed, {Qt::ToolTipRole});
    return true;
  }
  return false;
}

const TextureAsset* TextureAssetModel::asset(int row) const {
  return row >= 0 && row < m_assets.size() ? &m_assets.at(row) : nullptr;
}

int TextureAssetModel::rowCount(const QModelIndex& parent) const {
  return parent.isValid() ? 0 : static_cast<int>(m_assets.size());
}

QVariant TextureAssetModel::data(const QModelIndex& index, int role) const {
  const TextureAsset* a = asset(index.row());
  if (a == nullptr) {
    return {};
  }
  switch (role) {
  case Qt::DisplayRole:
    return a->name;
  case kIdRole:
    return a->id;
  case Qt::ToolTipRole: {
    QStringList lines{a->name};
    if (!a->tags.isEmpty()) {
      lines << tr("Tags: %1").arg(a->tags.join(QStringLiteral(", ")));
    }
    QStringList resolutions;
    for (const QString& resolution : a->resolutions) {
      resolutions << (resolution.isEmpty() ? tr("its own size") : resolution);
    }
    lines << tr("Resolutions: %1").arg(resolutions.join(QStringLiteral(", ")));
    if (!a->license.isEmpty()) {
      lines << tr("License: %1").arg(a->license);
    }
    return lines.join(QLatin1Char('\n'));
  }
  case Qt::DecorationRole: {
    const auto found = m_thumbnails.constFind(a->id);
    if (found != m_thumbnails.cend()) {
      return found.value();
    }
    // Asked the first time a view wants it: only the tiles in sight.
    if (!m_asked.contains(a->id) && m_source != nullptr) {
      m_asked.insert(a->id);
      auto* self = const_cast<TextureAssetModel*>(this);
      const QString id = a->id;
      const QString name = a->name;
      const int generation = m_generation;
      m_source->thumbnail(*a, kThumbnailSize, self, [self, id, name, generation](const QImage& image) {
        if (generation != self->m_generation || image.isNull()) {
          return;
        }
        // Square tiles: a narrower image in the middle of one.
        QPixmap tile(kThumbnailSize, kThumbnailSize);
        tile.fill(Qt::transparent);
        QPainter painter(&tile);
        const QImage scaled = image.scaled(kThumbnailSize, kThumbnailSize, Qt::KeepAspectRatio, Qt::SmoothTransformation);
        painter.drawImage((kThumbnailSize - scaled.width()) / 2, (kThumbnailSize - scaled.height()) / 2, scaled);
        painter.end();
        self->m_thumbnails.insert(id, tile);
        for (int row = 0; row < self->m_assets.size(); ++row) {
          if (self->m_assets.at(row).id == id) {
            const QModelIndex changed = self->index(row);
            emit self->dataChanged(changed, changed, {Qt::DecorationRole});
          }
        }
        emit self->thumbnailShown(name);
      });
    }
    return m_placeholder;
  }
  default:
    return {};
  }
}

TextureBrowser::TextureBrowser(QWidget* parent) : QWidget(parent) {
  auto* layout = new QVBoxLayout(this);
  layout->setContentsMargins(0, 0, 0, 0);
  m_model = new TextureAssetModel(this);
  m_view = new QListView;
  m_view->setModel(m_model);
  m_view->setViewMode(QListView::IconMode);
  m_view->setIconSize(QSize(TextureAssetModel::kThumbnailSize, TextureAssetModel::kThumbnailSize));
  m_view->setGridSize(QSize(TextureAssetModel::kThumbnailSize + 24, TextureAssetModel::kThumbnailSize + 40));
  m_view->setResizeMode(QListView::Adjust);
  m_view->setMovement(QListView::Static);
  m_view->setUniformItemSizes(true);
  m_view->setWordWrap(true);
  m_view->setSelectionMode(QAbstractItemView::SingleSelection);
  layout->addWidget(m_view, 1);
  m_attribution = new QLabel;
  m_attribution->setWordWrap(true);
  m_attribution->setOpenExternalLinks(true);
  m_attribution->hide();
  layout->addWidget(m_attribution);
  auto* row = new QHBoxLayout;
  m_resolution = new QComboBox;
  m_resolution->setToolTip(tr("The resolution of the images: larger ones show more detail and make the design "
                              "file larger"));
  m_size = new QLabel;
  m_use = new QPushButton(tr("Use"));
  m_use->setAutoDefault(false);
  m_use->setToolTip(tr("A new appearance of this design with the texture's maps embedded in it"));
  row->addWidget(m_resolution);
  row->addWidget(m_size, 1);
  row->addWidget(m_use);
  layout->addLayout(row);
  m_message = new QLabel;
  m_message->setWordWrap(true);
  layout->addWidget(m_message);

  connect(m_view->selectionModel(), &QItemSelectionModel::currentChanged, this, [this] { showAsset(); });
  connect(m_model, &QAbstractItemModel::modelReset, this, [this] { showAsset(); });
  connect(m_view, &QListView::doubleClicked, this, [this] {
    if (m_use->isEnabled()) {
      m_use->click();
    }
  });
  connect(m_resolution, &QComboBox::currentIndexChanged, this, [this] {
    showSize();
    const QString chosen = m_resolution->currentData().toString();
    if (!chosen.isEmpty() && m_resolution->isEnabled()) {
      TextureSettings settings = TextureSettings::load();
      if (settings.resolution != chosen) {
        settings.resolution = chosen;
        settings.save();
      }
    }
  });
  connect(m_use, &QPushButton::clicked, this, [this] {
    if (const TextureAsset* asset = currentAsset()) {
      emit useRequested(*asset, resolution());
    }
  });
  connect(m_model, &TextureAssetModel::thumbnailShown, this,
          [](const QString& name) { qDebug().noquote() << QStringLiteral("Texture thumbnail %1 shown").arg(name); });
  connect(m_view->verticalScrollBar(), &QScrollBar::valueChanged, this, &TextureBrowser::placesChanged);
  showAsset();
}

void TextureBrowser::search(TextureSource* source, const QString& text, const QString& category) {
  const int ticket = ++m_searches;
  if (source != m_source) {
    m_source = source;
    m_model->setAssets(source, {});
    const QString attribution = source != nullptr ? source->attribution() : QString();
    m_attribution->setText(attribution);
    m_attribution->setVisible(!attribution.isEmpty());
    m_detailed.clear();
    if (!attribution.isEmpty()) {
      qDebug().noquote() << QStringLiteral("Texture source %1 attribution: %2").arg(source->id(), attribution);
    }
  }
  if (source == nullptr) {
    return;
  }
  setMessage(tr("Searching…"), false);
  const QString sourceId = source->id();
  source->search(text, category, this, [this, ticket, sourceId, text, category](const QVector<TextureAsset>& assets,
                                                                                const QString& error) {
    if (ticket != m_searches || m_source == nullptr) {
      return; // a later search replaced it
    }
    const QString previous = currentAsset() != nullptr ? currentAsset()->id : QString();
    m_detailed.clear(); // the new results come without their details
    m_model->setAssets(m_source, assets);
    for (int row = 0; row < assets.size(); ++row) {
      if (assets.at(row).id == previous) {
        m_view->setCurrentIndex(m_model->index(row));
      }
    }
    if (!error.isEmpty()) {
      setMessage(error, true);
    } else if (assets.isEmpty()) {
      const QString hint = text.isEmpty() && category.isEmpty() ? m_source->emptyHint() : QString();
      setMessage(hint.isEmpty() ? tr("Nothing found.") : hint, false);
    } else {
      setMessage(tr("%n texture(s).", nullptr, static_cast<int>(assets.size())), false);
    }
    qDebug().noquote() << QStringLiteral("Texture search %1 \"%2\" (%3): %4 found%5")
                              .arg(sourceId, text,
                                   category.isEmpty() ? QStringLiteral("all categories") : category)
                              .arg(assets.size())
                              .arg(error.isEmpty() ? QString() : QStringLiteral(": ") + error);
    emit placesChanged();
  });
}

const TextureAsset* TextureBrowser::currentAsset() const { return m_model->asset(m_view->currentIndex().row()); }

QString TextureBrowser::resolution() const { return m_resolution->currentData().toString(); }

void TextureBrowser::setBusy(const QString& message) {
  m_busy = message;
  if (!message.isEmpty()) {
    setMessage(message, false);
  }
  showAsset();
}

void TextureBrowser::setMessage(const QString& message, bool error) {
  m_message->setText(message);
  if (error) {
    setErrorStyleSheet(m_message);
  } else {
    m_message->setStyleSheet(QString());
  }
}

void TextureBrowser::showAsset() {
  const TextureAsset* asset = currentAsset();
  const QSignalBlocker block(m_resolution);
  m_resolution->clear();
  if (asset != nullptr) {
    for (const QString& resolution : asset->resolutions) {
      m_resolution->addItem(resolution.isEmpty() ? tr("Original") : resolution, resolution);
    }
    // The last choice, else the largest below it, else the smallest.
    const QString wanted = TextureSettings::load().resolution;
    int index = m_resolution->findData(wanted);
    if (index < 0) {
      const auto pixels = [](const QString& r) {
        return r.endsWith(QLatin1Char('k')) ? r.chopped(1).toInt() * 1024 : r.toInt();
      };
      index = 0;
      for (int i = 0; i < m_resolution->count(); ++i) {
        if (pixels(m_resolution->itemData(i).toString()) <= pixels(wanted)) {
          index = i;
        }
      }
    }
    m_resolution->setCurrentIndex(index);
  }
  m_resolution->setEnabled(asset != nullptr && m_busy.isEmpty());
  m_use->setEnabled(asset != nullptr && m_busy.isEmpty());
  showSize();
  if (asset != nullptr) {
    qDebug().noquote() << QStringLiteral("Texture selected: %1 (%2)")
                              .arg(asset->name, resolution().isEmpty() ? QStringLiteral("original") : resolution());
    // What the source knows of it only now (an online source's files),
    // asked once per asset (mitcad#313).
    if (m_source != nullptr && !m_detailed.contains(asset->id)) {
      m_detailed.insert(asset->id);
      const QPointer<TextureSource> source = m_source;
      m_source->details(*asset, this, [this, source](const TextureAsset& detailed, const QString& error) {
        if (source != m_source) {
          return;
        }
        if (!error.isEmpty()) {
          m_detailed.remove(detailed.id); // asked again when chosen again
          if (m_busy.isEmpty()) {
            setMessage(error, true);
          }
          return;
        }
        if (!m_model->updateAsset(detailed)) {
          return;
        }
        QStringList sizes;
        for (const QString& r : detailed.resolutions) {
          sizes << QStringLiteral("%1 %2").arg(r.isEmpty() ? QStringLiteral("original") : r,
                                               textureBytesText(detailed.bytes.value(r, -1)));
        }
        qDebug().noquote() << QStringLiteral("Texture details: %1: %2")
                                  .arg(detailed.name, sizes.join(QStringLiteral(", ")));
        const TextureAsset* current = currentAsset();
        if (current != nullptr && current->id == detailed.id) {
          showAsset();
        }
      });
    }
  }
}

void TextureBrowser::showSize() {
  const TextureAsset* asset = currentAsset();
  if (asset == nullptr) {
    m_size->clear();
    return;
  }
  const qint64 bytes = asset->bytes.value(resolution(), -1);
  const bool network = m_source != nullptr && m_source->usesNetwork();
  m_size->setText(bytes < 0      ? QString()
                  : network ? tr("%1 to download, embedded in the design").arg(textureBytesText(bytes))
                            : tr("%1, embedded in the design").arg(textureBytesText(bytes)));
}

QVector<QPair<QString, QPoint>> TextureBrowser::assetsInSight() const {
  QVector<QPair<QString, QPoint>> places;
  // The grid's places now, not after its delayed layout.
  m_view->doItemsLayout();
  const QRect sight = m_view->viewport()->rect();
  for (int row = 0; row < m_model->rowCount(); ++row) {
    const QRect rect = m_view->visualRect(m_model->index(row));
    if (sight.contains(rect.center())) {
      places.append({m_model->asset(row)->name, rect.center()});
    }
  }
  return places;
}

} // namespace mitcad
