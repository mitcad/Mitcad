// SPDX-License-Identifier: MIT
// Texture sources (mitcad#315): the folder source's map detection in file
// names, its scan of synthetic texture folders, its asynchronous search,
// categories, thumbnails (kept in the cache under MITCAD_TEXTURES_DIR) and
// fetch, the registry, the cache's folder names, and Use's
// create_appearance command with embedded and shared images.
#include "UnitTests.hpp"
#include <cstdio>
#include <functional>

#include <QByteArray>
#include <QColor>
#include <QCoreApplication>
#include <QCryptographicHash>
#include <QDir>
#include <QElapsedTimer>
#include <QFile>
#include <QFileInfo>
#include <QImage>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QObject>
#include <QTemporaryDir>
#include <QThread>

#include "materials/FolderTextureSource.hpp"
#include "materials/TextureSource.hpp"
#include "materials/TextureUse.hpp"

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "texture_sources.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

using namespace mitcad;

bool parsed(const char* file, const char* map, const char* stem, const char* resolution) {
  const TextureFileName name = parseTextureFileName(QString::fromUtf8(file));
  const bool same = name.map == QLatin1String(map) && name.stem == QString::fromUtf8(stem) &&
                    name.resolution == QLatin1String(resolution);
  if (!same) {
    std::fprintf(stderr, "texture_sources.cpp: %s: map '%s', stem '%s', resolution '%s'\n", file,
                 qPrintable(name.map), qPrintable(name.stem), qPrintable(name.resolution));
  }
  return same;
}

void fileNameTests() {
  // Poly Haven style: stem, map, resolution.
  CHECK(parsed("oak_veneer_01_diff_2k.jpg", "base_color", "oak_veneer_01", "2k"));
  CHECK(parsed("oak_veneer_01_nor_gl_2k.png", "normal", "oak_veneer_01", "2k"));
  CHECK(parsed("oak_veneer_01_nor_dx_2k.png", "normal_directx", "oak_veneer_01", "2k"));
  CHECK(parsed("oak_veneer_01_rough_4K.png", "roughness", "oak_veneer_01", "4k"));
  CHECK(parsed("oak_veneer_01_metal_1k.png", "metalness", "oak_veneer_01", "1k"));
  CHECK(parsed("oak_veneer_01_ao_8k.jpg", "occlusion", "oak_veneer_01", "8k"));
  CHECK(parsed("rock_albedo.png", "base_color", "rock", ""));
  CHECK(parsed("rock_basecolor.jpeg", "base_color", "rock", ""));
  CHECK(parsed("rock-normal.png", "normal", "rock", ""));
  // The resolution before the map's suffix, words in capitals.
  CHECK(parsed("Bricks051_2K_Color.jpg", "base_color", "Bricks051", "2k"));
  CHECK(parsed("Bricks051_2K_NormalGL.png", "normal", "Bricks051", "2k"));
  CHECK(parsed("Bricks051_2K_NormalDX.png", "normal_directx", "Bricks051", "2k"));
  CHECK(parsed("Bricks051_2K_Roughness.jpg", "roughness", "Bricks051", "2k"));
  CHECK(parsed("Bricks051_2K_Metalness.jpg", "metalness", "Bricks051", "2k"));
  CHECK(parsed("Bricks051_2K_AmbientOcclusion.jpg", "occlusion", "Bricks051", "2k"));
  // Pixel counts as resolutions, separators kept inside the stem.
  CHECK(parsed("Metal plate-Metallic-1024.png", "metalness", "Metal plate", "1k"));
  CHECK(parsed("tile_ao.jpeg", "occlusion", "tile", ""));
  CHECK(parsed("Base Color.png", "base_color", "", ""));
  CHECK(parsed("roughness.png", "roughness", "", ""));
  // No map suffix: no texture map (a number is no resolution either).
  CHECK(parsed("wood.png", "", "", ""));
  CHECK(parsed("wood_049.png", "", "", ""));
  CHECK(parsed("wood_2k.png", "", "", ""));
}

// A PNG of a colour, `width` x `height`.
void writeImage(const QString& path, const QColor& color, int width = 16, int height = 16,
                const char* format = "PNG") {
  QDir().mkpath(QFileInfo(path).absolutePath());
  QImage image(width, height, QImage::Format_RGB32);
  image.fill(color);
  CHECK(image.save(path, format));
}

// Runs the event loop until `done` or ten seconds.
void waitFor(const std::function<bool()>& done) {
  QElapsedTimer timer;
  timer.start();
  while (!done() && timer.elapsed() < 10000) {
    QCoreApplication::processEvents(QEventLoop::AllEvents, 20);
    QThread::msleep(2);
  }
}

const FolderMaterial* materialNamed(const QVector<FolderMaterial>& materials, const QString& name) {
  for (const FolderMaterial& material : materials) {
    if (material.asset.name == name) {
      return &material;
    }
  }
  return nullptr;
}

void scanTests(const QString& root) {
  const QVector<FolderMaterial> materials = scanTextureFolders({root, root + QStringLiteral("/missing")});
  CHECK(materials.size() == 2);
  const FolderMaterial* bricks = materialNamed(materials, QStringLiteral("Red bricks"));
  const FolderMaterial* plate = materialNamed(materials, QStringLiteral("Plate"));
  CHECK(bricks != nullptr && plate != nullptr);
  if (bricks == nullptr || plate == nullptr) {
    return;
  }
  CHECK(materials.first().asset.name == QLatin1String("Plate")); // by name
  CHECK(bricks->asset.recorded() == QLatin1String("red_bricks"));
  CHECK(bricks->asset.resolutions == QStringList({QStringLiteral("1k"), QStringLiteral("2k")}));
  CHECK(bricks->asset.categories == QStringList{QFileInfo(root).fileName()});
  CHECK(bricks->asset.tags.contains(QStringLiteral("walls")));
  const QMap<QString, QString> large = folderMaterialFiles(*bricks, QStringLiteral("2k"));
  CHECK(large.keys() == QStringList({QStringLiteral("base_color"), QStringLiteral("normal"),
                                     QStringLiteral("occlusion"), QStringLiteral("roughness")}));
  CHECK(large.value(QStringLiteral("base_color")).endsWith(QLatin1String("walls/red_bricks_diff_2k.png")));
  // The occlusion map without a resolution goes with each resolution; the
  // DirectX normal map is left out.
  CHECK(large.value(QStringLiteral("occlusion")).endsWith(QLatin1String("walls/red_bricks_ao.png")));
  CHECK(large.value(QStringLiteral("normal")).endsWith(QLatin1String("red_bricks_nor_gl_2k.png")));
  const QMap<QString, QString> small = folderMaterialFiles(*bricks, QStringLiteral("1k"));
  CHECK(small.keys() == QStringList({QStringLiteral("base_color"), QStringLiteral("occlusion")}));
  CHECK(folderMaterialFiles(*bricks, QStringLiteral("4k")).isEmpty());
  qint64 bytes = 0;
  for (const QString& file : large) {
    bytes += QFileInfo(file).size();
  }
  CHECK(bricks->asset.bytes.value(QStringLiteral("2k")) == bytes);
  CHECK(bricks->asset.thumbnail.endsWith(QLatin1String("red_bricks_diff_1k.png")));
  // One material per folder: named by the folder, the JPEG base colour.
  CHECK(plate->asset.resolutions == QStringList{QString()});
  CHECK(plate->asset.recorded() == QLatin1String("Plate"));
  CHECK(plate->asset.tags == QStringList({QStringLiteral("metal"), QStringLiteral("plate")}));
  const QMap<QString, QString> plateFiles = folderMaterialFiles(*plate, QString());
  CHECK(plateFiles.keys() == QStringList({QStringLiteral("base_color"), QStringLiteral("metalness"),
                                          QStringLiteral("roughness")}));
  CHECK(plateFiles.value(QStringLiteral("base_color")).endsWith(QLatin1String("color.jpg")));
}

void sourceTests(const QString& root, const QString& cache) {
  CHECK(!textureSourceRegistry().empty());
  CHECK(QLatin1String(textureSourceRegistry().front().id) == QLatin1String("folder"));
  const auto sources = makeTextureSources(TextureSettings{{root}, QStringLiteral("2k"), false});
  CHECK(sources.size() == textureSourceRegistry().size());
  TextureSource* source = sources.front().get();
  CHECK(source->id() == QLatin1String("folder"));
  CHECK(!source->usesNetwork());
  CHECK(source->attribution().isEmpty());
  QObject context;

  QStringList categories;
  bool answered = false;
  source->categories(&context, [&](const QStringList& names, const QString& error) {
    categories = names;
    answered = error.isEmpty();
  });
  CHECK(!answered); // never within the call
  waitFor([&] { return answered; });
  CHECK(categories == QStringList{QFileInfo(root).fileName()});

  const auto search = [&](const QString& text, const QString& category) {
    QVector<TextureAsset> found;
    bool done = false;
    source->search(text, category, &context, [&](const QVector<TextureAsset>& assets, const QString& error) {
      CHECK(error.isEmpty());
      found = assets;
      done = true;
    });
    CHECK(!done);
    waitFor([&] { return done; });
    CHECK(done);
    return found;
  };
  CHECK(search(QString(), QString()).size() == 2);
  const QVector<TextureAsset> bricks = search(QStringLiteral("BRICK"), QString());
  CHECK(bricks.size() == 1 && bricks.first().name == QLatin1String("Red bricks"));
  CHECK(search(QStringLiteral("red brick"), QString()).size() == 1);
  CHECK(search(QStringLiteral("metal"), QString()).size() == 1); // the folder as a tag
  CHECK(search(QStringLiteral("nothing"), QString()).isEmpty());
  CHECK(search(QString(), categories.value(0)).size() == 2);
  CHECK(search(QString(), QStringLiteral("another")).isEmpty());
  if (bricks.isEmpty()) {
    return;
  }

  // The thumbnail: the base colour's middle, kept in the cache.
  QImage thumbnail;
  bool shown = false;
  source->thumbnail(bricks.first(), 48, &context, [&](const QImage& image) {
    thumbnail = image;
    shown = true;
  });
  waitFor([&] { return shown; });
  CHECK(!thumbnail.isNull() && thumbnail.width() <= 48 && thumbnail.height() <= 48);
  CHECK(thumbnail.width() == thumbnail.height());
  const QString kept = textureAssetFolder(QStringLiteral("folder"), bricks.first().id) + QStringLiteral("/thumbnail-48.png");
  CHECK(kept.startsWith(cache + QLatin1Char('/')));
  CHECK(QFileInfo(kept).isFile());
  CHECK(QColor(thumbnail.pixel(4, 4)).red() > 200);

  // Fetch: the source's own files.
  TextureFiles fetched;
  QString fetchError;
  bool fetchedDone = false;
  source->fetch(bricks.first(), QStringLiteral("2k"), &context, {}, [&](const TextureFiles& files, const QString& error) {
    fetched = files;
    fetchError = error;
    fetchedDone = true;
  });
  waitFor([&] { return fetchedDone; });
  CHECK(fetchError.isEmpty());
  CHECK(fetched.resolution == QLatin1String("2k"));
  CHECK(fetched.maps.size() == 4);
  TextureAsset gone = bricks.first();
  gone.id += QStringLiteral("-gone");
  fetchedDone = false;
  source->fetch(gone, QStringLiteral("2k"), &context, {}, [&](const TextureFiles& files, const QString& error) {
    fetched = files;
    fetchError = error;
    fetchedDone = true;
  });
  waitFor([&] { return fetchedDone; });
  CHECK(!fetchError.isEmpty() && fetched.maps.isEmpty());

  // An answer to a context that is gone is dropped.
  bool late = false;
  {
    QObject shortLived;
    source->search(QString(), QString(), &shortLived, [&](const QVector<TextureAsset>&, const QString&) { late = true; });
  }
  QCoreApplication::processEvents();
  QThread::msleep(50);
  QCoreApplication::processEvents();
  CHECK(!late);

  // No folders: nothing, and a hint.
  FolderTextureSource none({});
  CHECK(!none.emptyHint().isEmpty());
  bool emptyDone = false;
  none.search(QString(), QString(), &context, [&](const QVector<TextureAsset>& assets, const QString& error) {
    CHECK(assets.isEmpty() && error.isEmpty());
    emptyDone = true;
  });
  waitFor([&] { return emptyDone; });
  CHECK(emptyDone);
}

QString sha256Of(const QString& path) {
  QFile file(path);
  CHECK(file.open(QIODevice::ReadOnly));
  return QString::fromLatin1(QCryptographicHash::hash(file.readAll(), QCryptographicHash::Sha256).toHex());
}

void useTests(const QString& root) {
  const QVector<FolderMaterial> materials = scanTextureFolders({root});
  const FolderMaterial* bricks = materialNamed(materials, QStringLiteral("Red bricks"));
  CHECK(bricks != nullptr);
  if (bricks == nullptr) {
    return;
  }
  TextureFiles files{QStringLiteral("2k"), folderMaterialFiles(*bricks, QStringLiteral("2k"))};
  TextureUse use = textureUseCommand(QStringLiteral("folder"), bricks->asset, files, QStringLiteral("Red bricks"), {});
  CHECK(use.error.isEmpty());
  CHECK(use.sent == 4 && use.shared == 0);
  const QJsonObject command = use.command;
  CHECK(command.value(QStringLiteral("cmd")).toString() == QLatin1String("create_appearance"));
  CHECK(command.value(QStringLiteral("name")).toString() == QLatin1String("Red bricks"));
  const QJsonObject texture = command.value(QStringLiteral("texture")).toObject();
  CHECK(texture.value(QStringLiteral("path")).toString() == QLatin1String("red_bricks_diff_2k.png"));
  QFile base(files.maps.value(QStringLiteral("base_color")));
  CHECK(base.open(QIODevice::ReadOnly));
  CHECK(QByteArray::fromBase64(texture.value(QStringLiteral("data")).toString().toLatin1()) == base.readAll());
  for (const char* map : {"normal", "roughness", "occlusion"}) {
    CHECK(texture.value(QLatin1String(map)).toObject().contains(QStringLiteral("data")));
  }
  CHECK(!texture.contains(QStringLiteral("metalness")));
  // The base colour image is 64 x 32: 100 mm wide, 50 mm high.
  CHECK(texture.value(QStringLiteral("size")).toArray() == QJsonArray({100.0, 50.0}));
  CHECK(texture.value(QStringLiteral("source")).toObject() ==
        QJsonObject({{QStringLiteral("id"), QStringLiteral("folder")},
                     {QStringLiteral("asset"), QStringLiteral("red_bricks")},
                     {QStringLiteral("resolution"), QStringLiteral("2k")}}));
  // The base colour: the image's average colour (a red one).
  const QJsonArray color = command.value(QStringLiteral("base_color")).toArray();
  CHECK(color.size() == 3 && color.at(0).toDouble() > 0.7 && color.at(1).toDouble() < 0.2);

  // An image the design has already is referred to by its digest.
  const QString normalDigest = sha256Of(files.maps.value(QStringLiteral("normal")));
  use = textureUseCommand(QStringLiteral("folder"), bricks->asset, files, QStringLiteral("Red bricks 2"),
                          {normalDigest});
  CHECK(use.sent == 3 && use.shared == 1);
  const QJsonObject normal = use.command.value(QStringLiteral("texture")).toObject().value(QStringLiteral("normal")).toObject();
  CHECK(normal == QJsonObject({{QStringLiteral("path"), QStringLiteral("red_bricks_nor_gl_2k.png")},
                               {QStringLiteral("embedded"), true},
                               {QStringLiteral("image_sha256"), normalDigest}}));

  // A known size, licence and page; a page that is no web address is left out.
  TextureAsset known = bricks->asset;
  known.size = QSizeF(300.0, 250.0);
  known.license = QStringLiteral("CC0-1.0");
  known.url = QStringLiteral("https://example.org/a/red_bricks");
  use = textureUseCommand(QStringLiteral("folder"), known, files, QStringLiteral("x"), {});
  const QJsonObject knownTexture = use.command.value(QStringLiteral("texture")).toObject();
  CHECK(knownTexture.value(QStringLiteral("size")).toArray() == QJsonArray({300.0, 250.0}));
  CHECK(knownTexture.value(QStringLiteral("source")).toObject().value(QStringLiteral("license")).toString() ==
        QLatin1String("CC0-1.0"));
  CHECK(knownTexture.value(QStringLiteral("source")).toObject().value(QStringLiteral("url")).toString() ==
        QLatin1String("https://example.org/a/red_bricks"));
  known.url = QStringLiteral("file:///somewhere");
  use = textureUseCommand(QStringLiteral("folder"), known, files, QStringLiteral("x"), {});
  CHECK(!use.command.value(QStringLiteral("texture")).toObject().value(QStringLiteral("source")).toObject().contains(
      QStringLiteral("url")));

  // Too large, not an image, no base colour: refused with a reason.
  use = textureUseCommand(QStringLiteral("folder"), bricks->asset, files, QStringLiteral("x"), {}, 10);
  CHECK(!use.error.isEmpty() && use.command.isEmpty());
  const QString text = root + QStringLiteral("/../not_an_image_diff.png");
  QFile fake(text);
  CHECK(fake.open(QIODevice::WriteOnly) && fake.write("hello") == 5);
  fake.close();
  TextureFiles wrong{QString(), {{QStringLiteral("base_color"), text}}};
  use = textureUseCommand(QStringLiteral("folder"), bricks->asset, wrong, QStringLiteral("x"), {});
  CHECK(use.error.contains(QStringLiteral("no PNG or JPEG")));
  use = textureUseCommand(QStringLiteral("folder"), bricks->asset, TextureFiles(), QStringLiteral("x"), {});
  CHECK(!use.error.isEmpty());
}

void digestTests() {
  const QJsonArray listed = QJsonDocument::fromJson(R"([
    {"id": "steel_satin", "library": true},
    {"id": "custom1", "texture": {"path": "a.png", "embedded": true, "image_sha256": "aaa",
      "normal": {"path": "n.png", "embedded": true, "image_sha256": "bbb"},
      "roughness": {"path": "r.png"}}},
    {"id": "custom2", "texture": {"path": "c.png"}}
  ])").array();
  CHECK(embeddedImageDigests(listed) == QSet<QString>({QStringLiteral("aaa"), QStringLiteral("bbb")}));
}

void cacheTests(const QString& cache) {
  CHECK(textureCacheFolder() == QDir::cleanPath(cache));
  CHECK(textureFolderName(QStringLiteral("polyhaven")) == QLatin1String("polyhaven"));
  CHECK(textureFolderName(QStringLiteral("2k")) == QLatin1String("2k"));
  const QString odd = textureFolderName(QStringLiteral("/home/a b/x"));
  CHECK(odd.startsWith(QLatin1String("_home_a_b_x-")) && odd.size() == 11 + 1 + 12);
  CHECK(textureFolderName(QStringLiteral("a/b")) != textureFolderName(QStringLiteral("a_b-")));
  CHECK(textureFolderName(QStringLiteral("..")).startsWith(QLatin1String("__-")));
  CHECK(textureAssetFolder(QStringLiteral("polyhaven"), QStringLiteral("oak_veneer_01"), QStringLiteral("2k")) ==
        cache + QStringLiteral("/polyhaven/oak_veneer_01/2k"));
  CHECK(textureBytesText(1536) == QLatin1String("1.5 kB"));
  CHECK(textureBytesText(5 * 1024 * 1024) == QLatin1String("5.0 MB"));
}

} // namespace

int textureSourceTests() {
  QTemporaryDir dir;
  CHECK(dir.isValid());
  const QString root = dir.filePath(QStringLiteral("My Textures"));
  const QString cache = dir.filePath(QStringLiteral("cache"));
  const QByteArray previous = qgetenv("MITCAD_TEXTURES_DIR");
  qputenv("MITCAD_TEXTURES_DIR", cache.toUtf8());
  // A material named by stem in two resolutions, with a map without one
  // and a DirectX normal map; one per folder; images without a base
  // colour; a hidden folder; a file that is no image.
  writeImage(root + QStringLiteral("/walls/red_bricks_diff_1k.png"), QColor(220, 30, 20), 64, 32);
  writeImage(root + QStringLiteral("/walls/red_bricks_diff_2k.png"), QColor(220, 30, 20), 64, 32);
  writeImage(root + QStringLiteral("/walls/red_bricks_nor_gl_2k.png"), QColor(128, 128, 255));
  writeImage(root + QStringLiteral("/walls/red_bricks_nor_dx_2k.png"), QColor(128, 128, 255));
  writeImage(root + QStringLiteral("/walls/red_bricks_rough_2k.png"), QColor(200, 200, 200));
  writeImage(root + QStringLiteral("/walls/red_bricks_ao.png"), QColor(250, 250, 250));
  writeImage(root + QStringLiteral("/metal/Plate/color.jpg"), QColor(150, 150, 160), 16, 16, "JPEG");
  writeImage(root + QStringLiteral("/metal/Plate/metalness.png"), QColor(255, 255, 255));
  writeImage(root + QStringLiteral("/metal/Plate/roughness.png"), QColor(80, 80, 80));
  writeImage(root + QStringLiteral("/loose/stone_rough.png"), QColor(80, 80, 80));
  writeImage(root + QStringLiteral("/.hidden/ghost_diff.png"), QColor(80, 80, 80));
  QFile notes(root + QStringLiteral("/walls/notes_diff.txt"));
  CHECK(notes.open(QIODevice::WriteOnly) && notes.write("text") == 4);
  notes.close();

  fileNameTests();
  scanTests(root);
  sourceTests(root, cache);
  useTests(root);
  digestTests();
  cacheTests(cache);
  if (previous.isNull()) {
    qunsetenv("MITCAD_TEXTURES_DIR");
  } else {
    qputenv("MITCAD_TEXTURES_DIR", previous);
  }
  return failures;
}
MITCAD_UNIT_TEST(textureSourceTests);
