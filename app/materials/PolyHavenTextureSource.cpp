// SPDX-License-Identifier: MIT
#include "PolyHavenTextureSource.hpp"

#include <algorithm>
#include <utility>

#include <QCryptographicHash>
#include <QDateTime>
#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QImage>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonParseError>
#include <QRegularExpression>
#include <QSaveFile>
#include <QSet>
#include <QUrl>
#include <QUrlQuery>
#include <QtLogging>

#include "TextureUse.hpp"

namespace mitcad {
namespace {

// The largest answers taken: the list of textures (under 1 MB in 2026),
// one texture's list of files, a thumbnail.
constexpr qint64 kMaxListBytes = qint64(64) << 20;
constexpr qint64 kMaxFilesBytes = qint64(8) << 20;
constexpr qint64 kMaxThumbnailBytes = qint64(4) << 20;
// A download whose size the list does not give.
constexpr qint64 kMaxDownloadBytes = kMaxEmbeddedImage;
// Thumbnails are asked for again after this long.
constexpr qint64 kThumbnailMaxAgeSeconds = 30 * 24 * 60 * 60;

QString unexpected(const QString& what) {
  return PolyHavenTextureSource::tr("Poly Haven sent an unexpected answer: %1.").arg(what);
}

// Ids as the API has them (`aerial_asphalt_01`): anything else is not
// put into an address or a file name.
bool plainId(const QString& id) {
  static const QRegularExpression plain(QStringLiteral("^[A-Za-z0-9_-]{1,128}$"));
  return plain.match(id).hasMatch();
}

bool httpsUrl(const QString& text) {
  const QUrl url(text);
  return url.isValid() && url.scheme() == QLatin1String("https") && !url.host().isEmpty();
}

// `man made` as `Man made`.
QString shownCategory(const QString& category) {
  QString shown = category.simplified();
  if (!shown.isEmpty()) {
    shown[0] = shown[0].toUpper();
  }
  return shown;
}

int resolutionPixels(const QString& resolution) {
  return resolution.endsWith(QLatin1Char('k')) ? resolution.chopped(1).toInt() * 1024 : resolution.toInt();
}

// The resolutions up to the largest side in pixels: 1k, 2k, 4k, ...
QStringList resolutionsUpTo(int pixels) {
  QStringList resolutions;
  for (int k = 1; k <= 64 && k * 1024 <= pixels; k *= 2) {
    resolutions << QString::number(k) + QLatin1Char('k');
  }
  return resolutions;
}

// The file keys of `/files/<id>` as map names; empty for the others
// (displacement, the packed `arm`, DirectX normals, blend, gltf, ...).
QString mapOfKey(const QString& key) {
  const QString k = key.toLower();
  if (k == QLatin1String("diffuse") || k == QLatin1String("diff") || k == QLatin1String("albedo") ||
      k == QLatin1String("color") || k == QLatin1String("col") || k == QLatin1String("basecolor")) {
    return QStringLiteral("base_color");
  }
  if (k == QLatin1String("nor_gl")) {
    return QStringLiteral("normal");
  }
  if (k == QLatin1String("rough") || k == QLatin1String("roughness")) {
    return QStringLiteral("roughness");
  }
  if (k == QLatin1String("metal") || k == QLatin1String("metalness") || k == QLatin1String("metallic")) {
    return QStringLiteral("metalness");
  }
  if (k == QLatin1String("ao")) {
    return QStringLiteral("occlusion");
  }
  return QString();
}

bool fresh(const QString& file, qint64 maxAgeSeconds) {
  const QFileInfo info(file);
  return info.isFile() && info.lastModified().secsTo(QDateTime::currentDateTime()) < maxAgeSeconds;
}

QByteArray readFile(const QString& path) {
  QFile file(path);
  return file.open(QIODevice::ReadOnly) ? file.readAll() : QByteArray();
}

// Writes `data` to `path` whole or not at all; an error as a sentence.
QString writeFile(const QString& path, const QByteArray& data) {
  if (!QDir().mkpath(QFileInfo(path).absolutePath())) {
    return PolyHavenTextureSource::tr("Could not make the folder %1.")
        .arg(QDir::toNativeSeparators(QFileInfo(path).absolutePath()));
  }
  QSaveFile file(path);
  if (!file.open(QIODevice::WriteOnly) || file.write(data) != data.size() || !file.commit()) {
    return PolyHavenTextureSource::tr("Could not write %1: %2").arg(QDir::toNativeSeparators(path), file.errorString());
  }
  return QString();
}

QString mapShownName(const QString& map) {
  if (map == QLatin1String("base_color")) {
    return PolyHavenTextureSource::tr("base color");
  }
  return map;
}

} // namespace

PolyHavenList parsePolyHavenAssets(const QByteArray& json) {
  PolyHavenList list;
  QJsonParseError parseError;
  const QJsonDocument document = QJsonDocument::fromJson(json, &parseError);
  if (parseError.error != QJsonParseError::NoError || !document.isObject()) {
    list.error = unexpected(PolyHavenTextureSource::tr("no list of textures"));
    return list;
  }
  const QJsonObject all = document.object();
  QHash<QString, double> popularity;
  QSet<QString> categories;
  for (auto it = all.constBegin(); it != all.constEnd(); ++it) {
    const QJsonObject o = it.value().toObject();
    // Type 1 is a texture (0 an HDRI, 2 a model).
    if (o.isEmpty() || !plainId(it.key()) || (o.contains(QStringLiteral("type")) && o.value(QStringLiteral("type")).toInt() != 1)) {
      continue;
    }
    TextureAsset asset;
    asset.id = it.key();
    asset.name = o.value(QStringLiteral("name")).toString().simplified();
    if (asset.name.isEmpty()) {
      asset.name = asset.id;
    }
    for (const QJsonValue& tag : o.value(QStringLiteral("tags")).toArray()) {
      if (tag.isString() && !tag.toString().trimmed().isEmpty()) {
        asset.tags << tag.toString().trimmed();
      }
    }
    for (const QJsonValue& category : o.value(QStringLiteral("categories")).toArray()) {
      const QString shown = shownCategory(category.toString());
      if (!shown.isEmpty() && !asset.categories.contains(shown)) {
        asset.categories << shown;
        categories.insert(shown);
      }
    }
    const QString thumbnail = o.value(QStringLiteral("thumbnail_url")).toString();
    if (httpsUrl(thumbnail)) {
      asset.thumbnail = thumbnail;
    }
    // The physical size of the texture, in millimetres.
    const QJsonArray dimensions = o.value(QStringLiteral("dimensions")).toArray();
    if (dimensions.size() == 2 && dimensions.at(0).toDouble() > 0.0 && dimensions.at(1).toDouble() > 0.0) {
      asset.size = QSizeF(dimensions.at(0).toDouble(), dimensions.at(1).toDouble());
    }
    asset.license = QStringLiteral("CC0-1.0");
    asset.url = QStringLiteral("https://polyhaven.com/a/") + asset.id;
    // The resolutions until the files are listed (details()).
    const QJsonArray largest = o.value(QStringLiteral("max_resolution")).toArray();
    const int pixels = largest.size() == 2 ? std::max(largest.at(0).toInt(), largest.at(1).toInt()) : 0;
    asset.resolutions = pixels >= 1024 ? resolutionsUpTo(pixels)
                                       : QStringList{QStringLiteral("1k"), QStringLiteral("2k"), QStringLiteral("4k")};
    const QString hash = o.value(QStringLiteral("files_hash")).toString();
    if (plainId(hash)) {
      list.filesHash.insert(asset.id, hash);
    }
    popularity.insert(asset.id, o.value(QStringLiteral("download_count")).toDouble());
    list.assets.append(asset);
  }
  if (list.assets.isEmpty() && !all.isEmpty()) {
    list.error = unexpected(PolyHavenTextureSource::tr("no textures in the list"));
    return list;
  }
  // The most used first, as on Poly Haven's site.
  std::stable_sort(list.assets.begin(), list.assets.end(), [&popularity](const TextureAsset& a, const TextureAsset& b) {
    const double pa = popularity.value(a.id);
    const double pb = popularity.value(b.id);
    if (pa != pb) {
      return pa > pb;
    }
    return a.name.compare(b.name, Qt::CaseInsensitive) < 0;
  });
  list.categories = QStringList(categories.cbegin(), categories.cend());
  std::sort(list.categories.begin(), list.categories.end(),
            [](const QString& a, const QString& b) { return a.compare(b, Qt::CaseInsensitive) < 0; });
  return list;
}

PolyHavenFiles parsePolyHavenFiles(const QByteArray& json) {
  PolyHavenFiles files;
  QJsonParseError parseError;
  const QJsonDocument document = QJsonDocument::fromJson(json, &parseError);
  if (parseError.error != QJsonParseError::NoError || !document.isObject()) {
    files.error = unexpected(PolyHavenTextureSource::tr("no list of files"));
    return files;
  }
  const QJsonObject all = document.object();
  QMap<QString, QMap<QString, PolyHavenFile>> found; // resolution -> map -> file
  for (auto it = all.constBegin(); it != all.constEnd(); ++it) {
    const QString map = mapOfKey(it.key());
    if (map.isEmpty()) {
      continue;
    }
    const QJsonObject byResolution = it.value().toObject();
    for (auto res = byResolution.constBegin(); res != byResolution.constEnd(); ++res) {
      if (!QRegularExpression(QStringLiteral("^[0-9]{1,2}k$")).match(res.key()).hasMatch() ||
          found.value(res.key()).contains(map)) {
        continue;
      }
      const QJsonObject formats = res.value().toObject();
      for (const char* format : {"jpg", "png"}) {
        const QJsonObject entry = formats.value(QLatin1String(format)).toObject();
        const QString url = entry.value(QStringLiteral("url")).toString();
        if (!httpsUrl(url)) {
          continue;
        }
        PolyHavenFile file;
        file.map = map;
        file.url = url;
        file.size = entry.contains(QStringLiteral("size")) ? static_cast<qint64>(entry.value(QStringLiteral("size")).toDouble())
                                                          : -1;
        file.md5 = entry.value(QStringLiteral("md5")).toString().toLower();
        found[res.key()].insert(map, file);
        break;
      }
    }
  }
  for (auto it = found.cbegin(); it != found.cend(); ++it) {
    if (!it.value().contains(QStringLiteral("base_color"))) {
      continue;
    }
    QVector<PolyHavenFile> maps;
    for (const QString& map : textureSetMapNames()) {
      if (it.value().contains(map)) {
        maps.append(it.value().value(map));
      }
    }
    files.byResolution.insert(it.key(), maps);
    files.resolutions << it.key();
  }
  std::sort(files.resolutions.begin(), files.resolutions.end(),
            [](const QString& a, const QString& b) { return resolutionPixels(a) < resolutionPixels(b); });
  if (files.resolutions.isEmpty()) {
    files.error = unexpected(PolyHavenTextureSource::tr("no base color images in the list of files"));
  }
  return files;
}

struct PolyHavenTextureSource::Download {
  QPointer<QObject> context;
  TextureAsset asset;
  QString folder;
  QVector<PolyHavenFile> files;
  int index = 0;
  qint64 total = 0;
  qint64 done = 0;
  FetchProgress progress;
  FetchDone finish;
  TextureFiles result;
};

PolyHavenTextureSource::PolyHavenTextureSource(std::unique_ptr<TextureFetcher> fetcher, QObject* parent)
    : TextureSource(parent), m_fetcher(fetcher != nullptr ? std::move(fetcher) : makeTextureFetcher()) {}

PolyHavenTextureSource::~PolyHavenTextureSource() = default;

QString PolyHavenTextureSource::attribution() const {
  return tr("Powered by <a href=\"https://polyhaven.com\">Poly Haven</a>: CC0 textures, free for any use.");
}

QString PolyHavenTextureSource::errorText(const TextureReply& reply) {
  switch (reply.error) {
  case TextureReply::Error::None:
    return QString();
  case TextureReply::Error::Refused:
    return reply.detail;
  case TextureReply::Error::Offline:
    return tr("Poly Haven could not be reached (%1). Check the internet connection.")
        .arg(reply.detail.isEmpty() ? tr("no answer") : reply.detail);
  case TextureReply::Error::Timeout:
    return tr("Poly Haven did not answer in time. Try again later.");
  case TextureReply::Error::Http:
    return tr("Poly Haven answered with an error (HTTP %1). Try again later.").arg(reply.status);
  case TextureReply::Error::TooLarge:
    return tr("Poly Haven sent more data than expected.");
  }
  return QString();
}

QString PolyHavenTextureSource::listCacheFile() const {
  const QString cache = textureCacheFolder();
  return cache.isEmpty() ? QString()
                         : cache + QLatin1Char('/') + textureFolderName(id()) + QStringLiteral("/textures.json");
}

QString PolyHavenTextureSource::filesCacheFile(const QString& asset) const {
  const QString folder = textureAssetFolder(id(), asset);
  if (folder.isEmpty()) {
    return QString();
  }
  // Named by the list's files_hash: a texture whose files changed is asked
  // for again.
  const QString hash = m_list.filesHash.value(asset);
  return folder + (hash.isEmpty() ? QStringLiteral("/files.json") : QStringLiteral("/files-%1.json").arg(hash.left(16)));
}

void PolyHavenTextureSource::listed(const PolyHavenList& list) {
  m_list = list;
  m_byId.clear();
  for (int i = 0; i < m_list.assets.size(); ++i) {
    m_byId.insert(m_list.assets.at(i).id, i);
  }
  m_listed = true;
}

void PolyHavenTextureSource::whenListed(Then then) {
  if (m_listed) {
    then(QString());
    return;
  }
  m_waiting.push_back(std::move(then));
  if (m_listing) {
    return;
  }
  const auto answer = [this](const QString& error) {
    m_listing = false;
    std::vector<Then> waiting = std::move(m_waiting);
    m_waiting.clear();
    for (Then& call : waiting) {
      call(error);
    }
  };
  // The cached list while it is new enough: no request.
  const QString cached = listCacheFile();
  if (!cached.isEmpty() && fresh(cached, kListMaxAgeSeconds)) {
    const PolyHavenList list = parsePolyHavenAssets(readFile(cached));
    if (list.error.isEmpty()) {
      listed(list);
      qDebug().noquote() << QStringLiteral("Poly Haven: %1 textures from the cache").arg(list.assets.size());
      // Later, as an answer from the API would come.
      m_listing = true;
      TextureFetcher::later(this, [answer] { answer(QString()); });
      return;
    }
  }
  m_listing = true;
  m_fetcher->get(QUrl(apiBase() + QStringLiteral("/assets?t=textures")), kMaxListBytes, this, {},
                 [this, cached, answer](const TextureReply& reply) {
                   QString error = errorText(reply);
                   PolyHavenList list;
                   if (error.isEmpty()) {
                     list = parsePolyHavenAssets(reply.data);
                     error = list.error;
                   }
                   if (error.isEmpty()) {
                     if (!cached.isEmpty()) {
                       const QString written = writeFile(cached, reply.data);
                       if (!written.isEmpty()) {
                         qWarning().noquote() << written;
                       }
                     }
                   } else if (!cached.isEmpty() && QFileInfo(cached).isFile()) {
                     // The service may change or stop: an older list still works.
                     const PolyHavenList old = parsePolyHavenAssets(readFile(cached));
                     if (old.error.isEmpty()) {
                       qDebug().noquote() << QStringLiteral("Poly Haven: the cached list of textures, as %1").arg(error);
                       list = old;
                       error.clear();
                     }
                   }
                   if (error.isEmpty()) {
                     listed(list);
                     qDebug().noquote() << QStringLiteral("Poly Haven: %1 textures").arg(list.assets.size());
                   }
                   answer(error);
                 });
}

void PolyHavenTextureSource::categories(QObject* context, CategoriesDone done) {
  const QPointer<QObject> guard(context);
  whenListed([this, guard, done = std::move(done)](const QString& error) {
    const QStringList names = error.isEmpty() ? m_list.categories : QStringList();
    deliver(guard, [names, error, done] { done(names, error); });
  });
}

void PolyHavenTextureSource::search(const QString& text, const QString& category, QObject* context,
                                    SearchDone done) {
  const QPointer<QObject> guard(context);
  whenListed([this, guard, text, category, done = std::move(done)](const QString& error) {
    QVector<TextureAsset> found;
    if (error.isEmpty()) {
      const QStringList words = text.toLower().split(QRegularExpression(QStringLiteral("\\s+")), Qt::SkipEmptyParts);
      for (const TextureAsset& asset : std::as_const(m_list.assets)) {
        if (!category.isEmpty() && !asset.categories.contains(category)) {
          continue;
        }
        const QString haystack =
            (QStringList{asset.name, asset.id} + asset.tags + asset.categories).join(QLatin1Char(' ')).toLower();
        if (std::all_of(words.cbegin(), words.cend(), [&haystack](const QString& w) { return haystack.contains(w); })) {
          found.append(asset);
        }
      }
    }
    deliver(guard, [found, error, done] { done(found, error); });
  });
}

void PolyHavenTextureSource::thumbnail(const TextureAsset& asset, int size, QObject* context, ThumbnailDone done) {
  const QPointer<QObject> guard(context);
  const QString folder = plainId(asset.id) ? textureAssetFolder(id(), asset.id) : QString();
  const QString cached = folder.isEmpty() ? QString() : folder + QStringLiteral("/thumbnail-%1.png").arg(size);
  if (!cached.isEmpty() && fresh(cached, kThumbnailMaxAgeSeconds)) {
    const QImage image(cached);
    if (!image.isNull()) {
      deliver(guard, [image, done = std::move(done)] { done(image); });
      return;
    }
  }
  if (asset.thumbnail.isEmpty()) {
    deliver(guard, [done = std::move(done)] { done(QImage()); });
    return;
  }
  // The thumbnail in the size shown, as PNG (Qt reads no WebP without a
  // plugin).
  QUrl url(asset.thumbnail);
  QUrlQuery query(url);
  for (const char* key : {"width", "height", "format"}) {
    query.removeAllQueryItems(QLatin1String(key));
  }
  query.addQueryItem(QStringLiteral("width"), QString::number(size));
  query.addQueryItem(QStringLiteral("height"), QString::number(size));
  query.addQueryItem(QStringLiteral("format"), QStringLiteral("png"));
  url.setQuery(query);
  m_fetcher->get(url, kMaxThumbnailBytes, context, {}, [guard, size, cached, done](const TextureReply& reply) {
    QImage image;
    if (reply.ok()) {
      image = QImage::fromData(reply.data);
    }
    if (!image.isNull()) {
      if (image.width() > size || image.height() > size) {
        image = image.scaled(size, size, Qt::KeepAspectRatio, Qt::SmoothTransformation);
      }
      image = image.convertToFormat(QImage::Format_RGB32);
      if (!cached.isEmpty() && QDir().mkpath(QFileInfo(cached).absolutePath())) {
        QSaveFile file(cached);
        if (file.open(QIODevice::WriteOnly) && image.save(&file, "PNG")) {
          file.commit();
        }
      }
    }
    deliver(guard, [image, done] { done(image); });
  });
}

void PolyHavenTextureSource::whenFiles(const QString& asset, FilesThen then) {
  const auto known = m_files.constFind(asset);
  if (known != m_files.cend()) {
    then(known.value());
    return;
  }
  if (!plainId(asset)) {
    PolyHavenFiles files;
    files.error = tr("Poly Haven has no texture %1.").arg(asset);
    then(files);
    return;
  }
  std::vector<FilesThen>& waiting = m_filesWaiting[asset];
  waiting.push_back(std::move(then));
  if (waiting.size() > 1) {
    return;
  }
  const auto answer = [this, asset](const PolyHavenFiles& files) {
    if (files.error.isEmpty()) {
      m_files.insert(asset, files);
    }
    std::vector<FilesThen> calls = std::move(m_filesWaiting[asset]);
    m_filesWaiting.remove(asset);
    for (FilesThen& call : calls) {
      call(files);
    }
  };
  const QString cached = filesCacheFile(asset);
  const bool byHash = !m_list.filesHash.value(asset).isEmpty();
  if (!cached.isEmpty() && (byHash ? QFileInfo(cached).isFile() : fresh(cached, kListMaxAgeSeconds))) {
    const PolyHavenFiles files = parsePolyHavenFiles(readFile(cached));
    if (files.error.isEmpty()) {
      TextureFetcher::later(this, [answer, files] { answer(files); });
      return;
    }
  }
  m_fetcher->get(QUrl(apiBase() + QStringLiteral("/files/") + asset), kMaxFilesBytes, this, {},
                 [cached, answer](const TextureReply& reply) {
                   PolyHavenFiles files;
                   files.error = errorText(reply);
                   if (files.error.isEmpty()) {
                     files = parsePolyHavenFiles(reply.data);
                   }
                   if (files.error.isEmpty() && !cached.isEmpty()) {
                     const QString written = writeFile(cached, reply.data);
                     if (!written.isEmpty()) {
                       qWarning().noquote() << written;
                     }
                   } else if (!files.error.isEmpty() && !cached.isEmpty() && QFileInfo(cached).isFile()) {
                     const PolyHavenFiles old = parsePolyHavenFiles(readFile(cached));
                     if (old.error.isEmpty()) {
                       files = old;
                     }
                   }
                   answer(files);
                 });
}

void PolyHavenTextureSource::details(const TextureAsset& asset, QObject* context, DetailsDone done) {
  const QPointer<QObject> guard(context);
  whenFiles(asset.id, [guard, asset, done = std::move(done)](const PolyHavenFiles& files) {
    TextureAsset detailed = asset;
    if (files.error.isEmpty()) {
      detailed.resolutions = files.resolutions;
      detailed.bytes.clear();
      for (auto it = files.byResolution.cbegin(); it != files.byResolution.cend(); ++it) {
        qint64 bytes = 0;
        for (const PolyHavenFile& file : it.value()) {
          bytes = bytes < 0 || file.size < 0 ? -1 : bytes + file.size;
        }
        detailed.bytes.insert(it.key(), bytes);
      }
    }
    const QString error = files.error;
    deliver(guard, [detailed, error, done] { done(detailed, error); });
  });
}

void PolyHavenTextureSource::fetch(const TextureAsset& asset, const QString& resolution, QObject* context,
                                   FetchProgress progress, FetchDone done) {
  auto download = std::make_shared<Download>();
  download->context = context;
  download->asset = asset;
  download->progress = std::move(progress);
  download->finish = std::move(done);
  download->result.resolution = resolution;
  whenFiles(asset.id, [this, download, resolution](const PolyHavenFiles& files) {
    QString error = files.error;
    if (error.isEmpty() && !files.byResolution.contains(resolution)) {
      error = tr("Poly Haven has no %1 images of %2.").arg(resolution, download->asset.name);
    }
    download->folder = textureAssetFolder(id(), download->asset.id, resolution);
    if (error.isEmpty() && download->folder.isEmpty()) {
      error = tr("There is no folder for the texture cache.");
    }
    if (error.isEmpty()) {
      download->files = files.byResolution.value(resolution);
      for (const PolyHavenFile& file : std::as_const(download->files)) {
        if (file.size > kMaxEmbeddedImage) {
          error = tr("The %1 image of %2 in %3 is larger than a design embeds (%4): choose a smaller resolution.")
                      .arg(mapShownName(file.map), download->asset.name, resolution,
                           textureBytesText(kMaxEmbeddedImage));
          break;
        }
        download->total += std::max<qint64>(file.size, 0);
      }
    }
    if (!error.isEmpty()) {
      deliver(download->context, [download, error] { download->finish(TextureFiles(), error); });
      return;
    }
    downloadNext(download);
  });
}

void PolyHavenTextureSource::downloadNext(const std::shared_ptr<Download>& download) {
  if (download->context.isNull()) {
    return; // the asker is gone
  }
  if (download->index >= download->files.size()) {
    qDebug().noquote() << QStringLiteral("Poly Haven: %1 %2 in the cache (%3 maps)")
                              .arg(download->asset.id, download->result.resolution)
                              .arg(download->result.maps.size());
    deliver(download->context, [download] { download->finish(download->result, QString()); });
    return;
  }
  const PolyHavenFile file = download->files.at(download->index);
  QString name = QFileInfo(QUrl(file.url).path()).fileName();
  if (!plainId(QFileInfo(name).completeBaseName()) ||
      !QRegularExpression(QStringLiteral("^(jpg|jpeg|png)$"), QRegularExpression::CaseInsensitiveOption)
           .match(QFileInfo(name).suffix())
           .hasMatch()) {
    name = file.map + (file.url.endsWith(QLatin1String(".png"), Qt::CaseInsensitive) ? QStringLiteral(".png")
                                                                                       : QStringLiteral(".jpg"));
  }
  const QString local = download->folder + QLatin1Char('/') + name;
  // Downloaded before: kept while its size is the list's.
  const QFileInfo kept(local);
  if (kept.isFile() && (file.size < 0 ? kept.size() > 0 : kept.size() == file.size)) {
    download->result.maps.insert(file.map, local);
    download->done += std::max<qint64>(file.size, 0);
    ++download->index;
    downloadNext(download);
    return;
  }
  const qint64 limit = file.size > 0 ? file.size : kMaxDownloadBytes;
  m_fetcher->get(
      QUrl(file.url), limit, download->context.data(),
      [download](qint64 received, qint64) {
        if (download->progress) {
          download->progress(download->done + received, download->total);
        }
      },
      [self = QPointer<PolyHavenTextureSource>(this), download, file, local](const TextureReply& reply) {
        if (self.isNull()) {
          return; // the source is gone (the dialog made new ones)
        }
        QString error = errorText(reply);
        if (error.isEmpty() && file.size >= 0 && reply.data.size() != file.size) {
          error = tr("The %1 image from Poly Haven has %2 bytes instead of %3.")
                      .arg(mapShownName(file.map))
                      .arg(reply.data.size())
                      .arg(file.size);
        }
        if (error.isEmpty() && !file.md5.isEmpty() &&
            QString::fromLatin1(QCryptographicHash::hash(reply.data, QCryptographicHash::Md5).toHex()) != file.md5) {
          error = tr("The %1 image from Poly Haven is damaged (its checksum differs).").arg(mapShownName(file.map));
        }
        if (error.isEmpty()) {
          error = writeFile(local, reply.data);
        }
        if (!error.isEmpty()) {
          deliver(download->context, [download, error] { download->finish(TextureFiles(), error); });
          return;
        }
        download->result.maps.insert(file.map, local);
        download->done += reply.data.size();
        ++download->index;
        self->downloadNext(download);
      });
}

} // namespace mitcad
