// SPDX-License-Identifier: MIT
#include "FolderTextureSource.hpp"

#include <algorithm>
#include <utility>

#include <QCoreApplication>
#include <QDateTime>
#include <QDir>
#include <QFileInfo>
#include <QImageReader>
#include <QRegularExpression>
#include <QSaveFile>
#include <QThreadPool>

#include "platform/MacChrome.hpp"

namespace mitcad {
namespace {

// How deep below a folder materials are looked for, and how many files of
// one folder are looked at, at most.
constexpr int kMaxDepth = 6;
constexpr int kMaxFiles = 20000;

struct MapSuffix {
  QStringList tokens; // lower case, as the name's tokens
  const char* map;
};

// The map suffixes, longest first (`nor gl` before `nor`).
const QVector<MapSuffix>& mapSuffixes() {
  static const QVector<MapSuffix> suffixes = [] {
    const auto split = [](const char* text) { return QString::fromLatin1(text).split(QLatin1Char(' ')); };
    QVector<MapSuffix> list;
    const std::pair<const char*, const char*> entries[] = {
        {"base color", "base_color"},    {"base colour", "base_color"},     {"diff", "base_color"},
        {"diffuse", "base_color"},       {"albedo", "base_color"},          {"basecolor", "base_color"},
        {"color", "base_color"},         {"colour", "base_color"},          {"col", "base_color"},
        {"nor gl", "normal"},            {"normal gl", "normal"},           {"nrm gl", "normal"},
        {"normalgl", "normal"},          {"normal", "normal"},              {"nor", "normal"},
        {"nrm", "normal"},               {"norm", "normal"},                {"nor dx", "normal_directx"},
        {"normal dx", "normal_directx"}, {"nrm dx", "normal_directx"},      {"normaldx", "normal_directx"},
        {"rough", "roughness"},          {"roughness", "roughness"},        {"rgh", "roughness"},
        {"metal", "metalness"},          {"metallic", "metalness"},         {"metalness", "metalness"},
        {"ao", "occlusion"},             {"ambient occlusion", "occlusion"}, {"ambientocclusion", "occlusion"},
        {"occlusion", "occlusion"},      {"occ", "occlusion"},
    };
    for (const auto& [tokens, map] : entries) {
      list.append({split(tokens), map});
    }
    std::stable_sort(list.begin(), list.end(),
                     [](const MapSuffix& a, const MapSuffix& b) { return a.tokens.size() > b.tokens.size(); });
    return list;
  }();
  return suffixes;
}

// `2k`, `4K`, `1024`: the resolution as `2k` (`512` and `256` as they
// are); empty when the token is none.
QString resolutionOf(const QString& token) {
  static const QRegularExpression kilo(QStringLiteral("^([0-9]{1,2})k$"));
  const QRegularExpressionMatch match = kilo.match(token);
  if (match.hasMatch()) {
    return QString::number(match.captured(1).toInt()) + QLatin1Char('k');
  }
  for (const int pixels : {1024, 2048, 4096, 8192, 16384}) {
    if (token == QString::number(pixels)) {
      return QString::number(pixels / 1024) + QLatin1Char('k');
    }
  }
  if (token == QLatin1String("256") || token == QLatin1String("512")) {
    return token;
  }
  return QString();
}

// For sorting resolutions: pixels across (`` first).
int resolutionPixels(const QString& resolution) {
  if (resolution.endsWith(QLatin1Char('k'))) {
    return resolution.chopped(1).toInt() * 1024;
  }
  return resolution.toInt();
}

bool isImage(const QFileInfo& file) {
  const QString suffix = file.suffix().toLower();
  return suffix == QLatin1String("png") || suffix == QLatin1String("jpg") || suffix == QLatin1String("jpeg");
}

// `oak_veneer_01` as `Oak veneer 01`.
QString shownName(const QString& stem) {
  QString name = stem;
  name.replace(QLatin1Char('_'), QLatin1Char(' '));
  name.replace(QLatin1Char('-'), QLatin1Char(' '));
  name = name.simplified();
  if (!name.isEmpty() && name == name.toLower()) {
    name[0] = name[0].toUpper();
  }
  return name;
}

// The category of each folder: its name, with its parent's when two
// folders have the same name.
QStringList categoryNames(const QStringList& folders) {
  QStringList names;
  for (const QString& folder : folders) {
    names << QFileInfo(folder).fileName();
  }
  for (int i = 0; i < folders.size(); ++i) {
    if (names.count(names.at(i)) > 1 || names.at(i).isEmpty()) {
      const QFileInfo info(folders.at(i));
      names[i] = QStringLiteral("%1 (%2)").arg(info.fileName(), QDir::toNativeSeparators(info.absolutePath()));
    }
  }
  return names;
}

// The images of one folder grouped by stem: stem (lower case) -> the stem
// as written, and resolution -> map -> file.
struct Group {
  QString stem;
  QMap<QString, QMap<QString, QString>> files;
};

void scanFolder(const QString& root, const QString& category, QVector<FolderMaterial>& materials) {
  int seen = 0;
  struct Pending {
    QString path;
    int depth;
  };
  QVector<Pending> pending{{root, 0}};
  while (!pending.isEmpty() && seen < kMaxFiles) {
    const Pending folder = pending.takeFirst();
    const QDir dir(folder.path);
    QMap<QString, Group> groups;
    const QFileInfoList entries =
        dir.entryInfoList(QDir::Files | QDir::Dirs | QDir::NoDotAndDotDot | QDir::NoSymLinks, QDir::Name);
    for (const QFileInfo& entry : entries) {
      // QDir leaves out hidden entries, but on Windows only those with the
      // hidden attribute: skip dot names there too (`.git`, `.cache`).
      if (entry.fileName().startsWith(QLatin1Char('.'))) {
        continue;
      }
      if (entry.isDir()) {
        if (folder.depth < kMaxDepth) {
          pending.append({entry.absoluteFilePath(), folder.depth + 1});
        }
        continue;
      }
      if (++seen > kMaxFiles) {
        break;
      }
      if (!isImage(entry)) {
        continue;
      }
      const TextureFileName name = parseTextureFileName(entry.fileName());
      if (name.map.isEmpty() || name.map == QLatin1String("normal_directx")) {
        continue;
      }
      Group& group = groups[name.stem.toLower()];
      if (group.stem.isEmpty()) {
        group.stem = name.stem;
      }
      QString& file = group.files[name.resolution][name.map];
      // Two files for one map: PNG (lossless) before JPEG, else the first.
      if (file.isEmpty() || (entry.suffix().compare(QLatin1String("png"), Qt::CaseInsensitive) == 0 &&
                             !file.endsWith(QLatin1String(".png"), Qt::CaseInsensitive))) {
        file = entry.absoluteFilePath();
      }
    }
    const QString relative = QDir(root).relativeFilePath(folder.path);
    QStringList tags;
    if (relative != QLatin1String(".")) {
      for (const QString& part : relative.split(QLatin1Char('/'), Qt::SkipEmptyParts)) {
        tags << shownName(part).toLower();
      }
    }
    for (auto it = groups.cbegin(); it != groups.cend(); ++it) {
      const Group& group = it.value();
      FolderMaterial material;
      for (auto files = group.files.cbegin(); files != group.files.cend(); ++files) {
        if (files.value().contains(QStringLiteral("base_color"))) {
          material.asset.resolutions << files.key();
        }
      }
      if (material.asset.resolutions.isEmpty()) {
        continue; // no base colour image
      }
      std::sort(material.asset.resolutions.begin(), material.asset.resolutions.end(),
                [](const QString& a, const QString& b) { return resolutionPixels(a) < resolutionPixels(b); });
      material.files = group.files;
      const QString folderName = QFileInfo(folder.path).fileName();
      material.asset.id = folder.path + QLatin1Char('/') + group.stem;
      material.asset.name = shownName(group.stem.isEmpty() ? folderName : group.stem);
      material.asset.recordedId = group.stem.isEmpty() ? folderName : group.stem;
      material.asset.tags = tags;
      material.asset.categories = {category};
      for (const QString& resolution : std::as_const(material.asset.resolutions)) {
        qint64 bytes = 0;
        for (const QString& file : folderMaterialFiles(material, resolution)) {
          bytes += QFileInfo(file).size();
        }
        material.asset.bytes.insert(resolution, bytes);
      }
      material.asset.thumbnail =
          folderMaterialFiles(material, material.asset.resolutions.first()).value(QStringLiteral("base_color"));
      materials.append(material);
    }
  }
}

QThreadPool& thumbnailPool() {
  static QThreadPool pool;
  pool.setMaxThreadCount(4);
  return pool;
}

} // namespace

TextureFileName parseTextureFileName(const QString& fileName) {
  TextureFileName result;
  const QString base = QFileInfo(fileName).completeBaseName();
  struct Token {
    QString text;
    qsizetype start;
    qsizetype end;
  };
  QVector<Token> tokens;
  static const QRegularExpression word(QStringLiteral("[^_\\-\\s.]+"));
  QRegularExpressionMatchIterator it = word.globalMatch(base);
  while (it.hasNext()) {
    const QRegularExpressionMatch match = it.next();
    tokens.append({match.captured().toLower(), match.capturedStart(), match.capturedEnd()});
  }
  qsizetype end = tokens.size();
  if (end > 0) {
    result.resolution = resolutionOf(tokens.at(end - 1).text);
    if (!result.resolution.isEmpty()) {
      --end;
    }
  }
  for (const MapSuffix& suffix : mapSuffixes()) {
    const qsizetype n = suffix.tokens.size();
    if (end < n) {
      continue;
    }
    bool same = true;
    for (qsizetype i = 0; i < n && same; ++i) {
      same = tokens.at(end - n + i).text == suffix.tokens.at(i);
    }
    if (same) {
      result.map = QString::fromLatin1(suffix.map);
      end -= n;
      break;
    }
  }
  if (result.map.isEmpty()) {
    return {};
  }
  // `Bricks051_2K_Color`: the resolution before the map's suffix.
  if (result.resolution.isEmpty() && end > 0) {
    result.resolution = resolutionOf(tokens.at(end - 1).text);
    if (!result.resolution.isEmpty()) {
      --end;
    }
  }
  result.stem = end > 0 ? base.left(tokens.at(end - 1).end) : QString();
  return result;
}

QMap<QString, QString> folderMaterialFiles(const FolderMaterial& material, const QString& resolution) {
  QMap<QString, QString> files = material.files.value(resolution);
  if (!files.contains(QStringLiteral("base_color"))) {
    return {};
  }
  const QMap<QString, QString> plain = material.files.value(QString());
  for (auto it = plain.cbegin(); it != plain.cend(); ++it) {
    if (!files.contains(it.key())) {
      files.insert(it.key(), it.value());
    }
  }
  return files;
}

QVector<FolderMaterial> scanTextureFolders(const QStringList& folders) {
  QVector<FolderMaterial> materials;
  const QStringList categories = categoryNames(folders);
  for (int i = 0; i < folders.size(); ++i) {
    if (QFileInfo(folders.at(i)).isDir()) {
      scanFolder(QDir::cleanPath(QFileInfo(folders.at(i)).absoluteFilePath()), categories.at(i), materials);
    }
  }
  std::stable_sort(materials.begin(), materials.end(), [](const FolderMaterial& a, const FolderMaterial& b) {
    const int byName = a.asset.name.compare(b.asset.name, Qt::CaseInsensitive);
    return byName != 0 ? byName < 0 : a.asset.id < b.asset.id;
  });
  return materials;
}

QImage textureThumbnail(const QString& imageFile, int size, const QString& cacheFolder) {
  const QFileInfo image(imageFile);
  const QString cached =
      cacheFolder.isEmpty() ? QString() : cacheFolder + QStringLiteral("/thumbnail-%1.png").arg(size);
  if (!cached.isEmpty()) {
    const QFileInfo kept(cached);
    if (kept.isFile() && kept.lastModified() >= image.lastModified()) {
      const QImage thumbnail(cached);
      if (!thumbnail.isNull()) {
        return thumbnail;
      }
    }
  }
  QImageReader reader(imageFile);
  reader.setAutoTransform(true);
  const QSize full = reader.size();
  if (full.isValid() && !full.isEmpty()) {
    // The middle square, scaled down while it is read.
    const int side = std::min(full.width(), full.height());
    reader.setClipRect(QRect((full.width() - side) / 2, (full.height() - side) / 2, side, side));
    const int scaled = std::min(size, side);
    reader.setScaledSize(QSize(scaled, scaled));
  }
  QImage thumbnail = reader.read();
  if (thumbnail.isNull()) {
    return QImage();
  }
  if (thumbnail.width() > size || thumbnail.height() > size) {
    thumbnail = thumbnail.scaled(size, size, Qt::KeepAspectRatio, Qt::SmoothTransformation);
  }
  thumbnail = thumbnail.convertToFormat(QImage::Format_RGB32);
  if (!cached.isEmpty() && QDir().mkpath(cacheFolder)) {
    QSaveFile file(cached);
    if (file.open(QIODevice::WriteOnly) && thumbnail.save(&file, "PNG")) {
      file.commit();
    }
  }
  return thumbnail;
}

FolderTextureSource::FolderTextureSource(QStringList folders, QObject* parent)
    : TextureSource(parent), m_folders(std::move(folders)) {}

QString FolderTextureSource::emptyHint() const {
  if (m_folders.isEmpty()) {
    return tr("No texture folders yet: add folders of texture images on the Textures page of Tools › Preferences.");
  }
  return tr("No materials in the texture folders. A material is a set of PNG or JPEG images named by their "
            "maps, such as oak_diff.png, oak_nor_gl.png, oak_rough.png.");
}

void FolderTextureSource::rescan() {
  m_scanned = false;
  ++m_generation;
}

void FolderTextureSource::whenScanned(std::function<void()> then) {
  if (m_scanned) {
    then();
    return;
  }
  m_waiting.push_back(std::move(then));
  if (m_scanning) {
    return;
  }
  m_scanning = true;
  const int generation = m_generation;
  const QStringList folders = m_folders;
  const QPointer<QObject> self(this);
  QThreadPool::globalInstance()->start([self, folders, generation] {
    const mac::ThreadQualityScope quality(mac::ThreadQuality::Utility);
    QVector<FolderMaterial> materials = scanTextureFolders(folders);
    deliver(self, [self, generation, materials = std::move(materials)] {
      auto* source = static_cast<FolderTextureSource*>(self.data());
      source->m_scanning = false;
      if (generation != source->m_generation) {
        // Rescanned meanwhile: once more.
        std::vector<std::function<void()>> waiting = std::move(source->m_waiting);
        source->m_waiting.clear();
        for (auto& call : waiting) {
          source->whenScanned(std::move(call));
        }
        return;
      }
      source->m_materials = materials;
      source->m_byId.clear();
      for (int i = 0; i < materials.size(); ++i) {
        source->m_byId.insert(materials.at(i).asset.id, i);
      }
      source->m_scanned = true;
      std::vector<std::function<void()>> waiting = std::move(source->m_waiting);
      source->m_waiting.clear();
      for (auto& call : waiting) {
        call();
      }
    });
  });
}

void FolderTextureSource::categories(QObject* context, CategoriesDone done) {
  const QStringList names = categoryNames(m_folders);
  deliver(context, [names, done = std::move(done)] { done(names, QString()); });
}

void FolderTextureSource::search(const QString& text, const QString& category, QObject* context, SearchDone done) {
  const QPointer<QObject> guard(context);
  whenScanned([this, guard, text, category, done = std::move(done)] {
    const QStringList words = text.toLower().split(QRegularExpression(QStringLiteral("\\s+")), Qt::SkipEmptyParts);
    QVector<TextureAsset> found;
    for (const FolderMaterial& material : std::as_const(m_materials)) {
      const TextureAsset& asset = material.asset;
      if (!category.isEmpty() && !asset.categories.contains(category)) {
        continue;
      }
      const QString haystack = (QStringList{asset.name, asset.recorded()} + asset.tags).join(QLatin1Char(' ')).toLower();
      if (std::all_of(words.cbegin(), words.cend(), [&haystack](const QString& w) { return haystack.contains(w); })) {
        found.append(asset);
      }
    }
    deliver(guard, [found, done] { done(found, QString()); });
  });
}

void FolderTextureSource::thumbnail(const TextureAsset& asset, int size, QObject* context, ThumbnailDone done) {
  const QPointer<QObject> guard(context);
  const QString file = asset.thumbnail;
  const QString cache = textureAssetFolder(id(), asset.id);
  thumbnailPool().start([guard, file, size, cache, done = std::move(done)] {
    const QImage image = textureThumbnail(file, size, cache);
    deliver(guard, [image, done] { done(image); });
  });
}

void FolderTextureSource::fetch(const TextureAsset& asset, const QString& resolution, QObject* context,
                                FetchProgress progress, FetchDone done) {
  Q_UNUSED(progress);
  const QPointer<QObject> guard(context);
  const QString assetId = asset.id;
  whenScanned([this, guard, assetId, resolution, done = std::move(done)] {
    TextureFiles files;
    files.resolution = resolution;
    QString error;
    const auto found = m_byId.constFind(assetId);
    if (found == m_byId.cend()) {
      error = tr("The material is no longer in the texture folders.");
    } else {
      files.maps = folderMaterialFiles(m_materials.at(found.value()), resolution);
      if (files.maps.isEmpty()) {
        error = tr("The material has no base color image in %1.")
                    .arg(resolution.isEmpty() ? tr("its own size") : resolution);
      }
      for (auto it = files.maps.cbegin(); error.isEmpty() && it != files.maps.cend(); ++it) {
        if (!QFileInfo(it.value()).isFile()) {
          error = tr("The image %1 is missing.").arg(QDir::toNativeSeparators(it.value()));
        }
      }
    }
    if (!error.isEmpty()) {
      files = TextureFiles();
    }
    deliver(guard, [files, error, done] { done(files, error); });
  });
}

} // namespace mitcad
