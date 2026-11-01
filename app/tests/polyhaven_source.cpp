// SPDX-License-Identifier: MIT
// Poly Haven as a texture source (mitcad#313), without the network: the
// list of textures and a texture's list of files read from small
// hand-written answers; the source searching, giving thumbnails, details
// and maps through a fake fetcher with those answers and generated images;
// the caches (MITCAD_TEXTURES_DIR) that save requests; errors as sentences;
// the fetchers' refusals (network off, no HTTPS) and the folder fetcher of
// the UI tests.
#include <cstdio>
#include <functional>
#include <memory>

#include <QBuffer>
#include <QByteArray>
#include <QColor>
#include <QCoreApplication>
#include <QCryptographicHash>
#include <QDateTime>
#include <QDir>
#include <QElapsedTimer>
#include <QFile>
#include <QFileInfo>
#include <QHash>
#include <QImage>
#include <QJsonArray>
#include <QJsonObject>
#include <QObject>
#include <QTemporaryDir>
#include <QThread>
#include <QUrl>

#include "materials/PolyHavenTextureSource.hpp"
#include "materials/TextureFetcher.hpp"
#include "materials/TextureSource.hpp"
#include "materials/TextureUse.hpp"

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "polyhaven_source.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

using namespace mitcad;

void waitFor(const std::function<bool()>& done) {
  QElapsedTimer timer;
  timer.start();
  while (!done() && timer.elapsed() < 10000) {
    QCoreApplication::processEvents(QEventLoop::AllEvents, 20);
    QThread::msleep(2);
  }
}

// Answers by address (the whole address first, then without its query);
// the others are HTTP 404. Every request is kept.
class FakeFetcher : public TextureFetcher {
public:
  QHash<QString, TextureReply> answers;
  QStringList requests;

  void answer(const QString& url, const QByteArray& data) {
    TextureReply reply;
    reply.data = data;
    reply.status = 200;
    answers.insert(url, reply);
  }
  void fail(const QString& url, TextureReply::Error error, int status = 0) {
    TextureReply reply;
    reply.error = error;
    reply.status = status;
    reply.detail = QStringLiteral("Host not found");
    answers.insert(url, reply);
  }

  void get(const QUrl& url, qint64 maxBytes, QObject* context, Progress progress, Done done) override {
    requests << url.toString();
    TextureReply reply;
    if (answers.contains(url.toString())) {
      reply = answers.value(url.toString());
    } else if (answers.contains(url.toString(QUrl::RemoveQuery))) {
      reply = answers.value(url.toString(QUrl::RemoveQuery));
    } else {
      reply.error = TextureReply::Error::Http;
      reply.status = 404;
    }
    if (reply.ok() && reply.data.size() > maxBytes) {
      reply = TextureReply();
      reply.error = TextureReply::Error::TooLarge;
    }
    later(context, [reply, progress, done] {
      if (reply.ok() && progress) {
        progress(reply.data.size(), reply.data.size());
      }
      done(reply);
    });
  }
};

QByteArray imageBytes(const QColor& color, int width, int height, const char* format) {
  QImage image(width, height, QImage::Format_RGB32);
  image.fill(color);
  QByteArray bytes;
  QBuffer buffer(&bytes);
  buffer.open(QIODevice::WriteOnly);
  CHECK(image.save(&buffer, format));
  return bytes;
}

QString md5(const QByteArray& data) {
  return QString::fromLatin1(QCryptographicHash::hash(data, QCryptographicHash::Md5).toHex());
}

// A hand-written list in the API's shape: two textures, an HDRI, an entry
// whose id is no plain one.
const char* const kAssets = R"({
  "red_tiles_01": {"type": 1, "name": "Red Tiles 01", "tags": ["tiles", "glazed"],
    "categories": ["floor", "man made"], "dimensions": [2000, 1000], "max_resolution": [4096, 2048],
    "files_hash": "aaaa1111bbbb2222", "download_count": 50,
    "thumbnail_url": "https://cdn.example.test/thumbs/red_tiles_01.png?width=256&height=256&format=webp&v=1"},
  "grey_stone_02": {"type": 1, "name": "Grey Stone 02", "tags": ["rock"], "categories": ["natural", "floor"],
    "dimensions": [3000, 3000], "max_resolution": [8192, 8192], "files_hash": "cccc3333", "download_count": 900,
    "thumbnail_url": "http://cdn.example.test/thumbs/grey_stone_02.png"},
  "sky_noon": {"type": 0, "name": "Sky Noon", "categories": ["skies"]},
  "../escape": {"type": 1, "name": "Escape", "categories": ["floor"]}
})";

void parseTests() {
  const PolyHavenList list = parsePolyHavenAssets(kAssets);
  CHECK(list.error.isEmpty());
  CHECK(list.assets.size() == 2);
  if (list.assets.size() != 2) {
    return;
  }
  // The most downloaded first.
  CHECK(list.assets.at(0).id == QLatin1String("grey_stone_02"));
  const TextureAsset& tiles = list.assets.at(1);
  CHECK(tiles.name == QLatin1String("Red Tiles 01"));
  CHECK(tiles.tags == QStringList({QStringLiteral("tiles"), QStringLiteral("glazed")}));
  CHECK(tiles.categories == QStringList({QStringLiteral("Floor"), QStringLiteral("Man made")}));
  CHECK(tiles.size == QSizeF(2000.0, 1000.0)); // millimetres
  CHECK(tiles.license == QLatin1String("CC0-1.0"));
  CHECK(tiles.url == QLatin1String("https://polyhaven.com/a/red_tiles_01"));
  CHECK(tiles.recorded() == QLatin1String("red_tiles_01"));
  CHECK(tiles.resolutions == QStringList({QStringLiteral("1k"), QStringLiteral("2k"), QStringLiteral("4k")}));
  CHECK(tiles.bytes.isEmpty());
  CHECK(tiles.thumbnail.startsWith(QLatin1String("https://cdn.example.test/thumbs/red_tiles_01.png")));
  CHECK(list.assets.at(0).thumbnail.isEmpty()); // no HTTPS
  CHECK(list.assets.at(0).resolutions.size() == 4);
  CHECK(list.categories ==
        QStringList({QStringLiteral("Floor"), QStringLiteral("Man made"), QStringLiteral("Natural")}));
  CHECK(list.filesHash.value(QStringLiteral("red_tiles_01")) == QLatin1String("aaaa1111bbbb2222"));

  CHECK(parsePolyHavenAssets("{}").assets.isEmpty() && parsePolyHavenAssets("{}").error.isEmpty());
  CHECK(parsePolyHavenAssets("<html>busy</html>").error.contains(QStringLiteral("unexpected answer")));
  CHECK(!parsePolyHavenAssets(R"(["a", "b"])").error.isEmpty());
  CHECK(!parsePolyHavenAssets(R"({"x": {"type": 0}})").error.isEmpty());
}

// The files of red_tiles_01 in the API's shape, the images generated.
struct Files {
  QByteArray json;
  QHash<QString, QByteArray> images; // address -> bytes
};

Files tileFiles(const QString& host) {
  Files files;
  const auto add = [&](const QString& name, const QByteArray& bytes) {
    const QString url = host + QStringLiteral("/textures/") + name;
    files.images.insert(url, bytes);
    return QStringLiteral(R"({"url": "%1", "size": %2, "md5": "%3"})").arg(url).arg(bytes.size()).arg(md5(bytes));
  };
  const QByteArray diff1 = imageBytes(QColor(200, 40, 30), 32, 16, "JPEG");
  const QByteArray diff2 = imageBytes(QColor(200, 40, 30), 64, 32, "JPEG");
  const QByteArray diff2png = imageBytes(QColor(200, 40, 30), 64, 32, "PNG");
  const QByteArray nor2 = imageBytes(QColor(128, 128, 255), 16, 16, "JPEG");
  const QByteArray rough2 = imageBytes(QColor(180, 180, 180), 16, 16, "JPEG");
  const QByteArray ao2 = imageBytes(QColor(240, 240, 240), 16, 16, "PNG");
  const QByteArray dx2 = imageBytes(QColor(128, 128, 0), 16, 16, "JPEG");
  const QByteArray nor4 = imageBytes(QColor(128, 128, 255), 16, 16, "JPEG");
  files.json = QStringLiteral(R"({
    "Diffuse": {"1k": {"jpg": %1}, "2k": {"jpg": %2, "png": %3}},
    "nor_gl": {"2k": {"jpg": %4}, "4k": {"jpg": %5}},
    "nor_dx": {"2k": {"jpg": %6}},
    "Rough": {"2k": {"jpg": %7, "exr": {"url": "https://x.test/r.exr", "size": 9}}},
    "AO": {"2k": {"png": %8}},
    "arm": {"2k": {"jpg": %6}},
    "blend": {"2k": {"blend": {"url": "https://x.test/a.blend", "size": 1}}}
  })")
                   .arg(add(QStringLiteral("red_tiles_01_diff_1k.jpg"), diff1),
                        add(QStringLiteral("red_tiles_01_diff_2k.jpg"), diff2),
                        add(QStringLiteral("red_tiles_01_diff_2k.png"), diff2png),
                        add(QStringLiteral("red_tiles_01_nor_gl_2k.jpg"), nor2),
                        add(QStringLiteral("red_tiles_01_nor_gl_4k.jpg"), nor4),
                        add(QStringLiteral("red_tiles_01_nor_dx_2k.jpg"), dx2),
                        add(QStringLiteral("red_tiles_01_rough_2k.jpg"), rough2),
                        add(QStringLiteral("red_tiles_01_ao_2k.png"), ao2))
                   .toUtf8();
  return files;
}

void filesTests() {
  const Files given = tileFiles(QStringLiteral("https://dl.example.test"));
  const PolyHavenFiles files = parsePolyHavenFiles(given.json);
  CHECK(files.error.isEmpty());
  // 4k has no base colour image: left out.
  CHECK(files.resolutions == QStringList({QStringLiteral("1k"), QStringLiteral("2k")}));
  const QVector<PolyHavenFile> two = files.byResolution.value(QStringLiteral("2k"));
  QStringList maps;
  for (const PolyHavenFile& file : two) {
    maps << file.map;
  }
  CHECK(maps == QStringList({QStringLiteral("base_color"), QStringLiteral("normal"), QStringLiteral("roughness"),
                             QStringLiteral("occlusion")}));
  // JPEG where there is one, else PNG; never EXR or DirectX normals.
  CHECK(two.value(0).url.endsWith(QLatin1String("diff_2k.jpg")));
  CHECK(two.value(3).url.endsWith(QLatin1String("ao_2k.png")));
  CHECK(two.value(0).size == given.images.value(two.value(0).url).size());
  CHECK(two.value(0).md5 == md5(given.images.value(two.value(0).url)));
  CHECK(files.byResolution.value(QStringLiteral("1k")).size() == 1);

  CHECK(!parsePolyHavenFiles("not json").error.isEmpty());
  CHECK(!parsePolyHavenFiles(R"({"Rough": {"1k": {"jpg": {"url": "https://x.test/r.jpg", "size": 3}}}})")
             .error.isEmpty());
  // Only HTTPS addresses.
  CHECK(!parsePolyHavenFiles(R"({"Diffuse": {"1k": {"jpg": {"url": "http://x.test/d.jpg", "size": 3}}}})")
             .error.isEmpty());
}

const QString kApi = PolyHavenTextureSource::apiBase();

// The source with the fake fetcher answering the list, the files and the
// images.
struct Fixture {
  FakeFetcher* fetcher = nullptr;
  std::unique_ptr<PolyHavenTextureSource> source;
  Files files;
};

Fixture makeFixture() {
  Fixture fixture;
  auto fetcher = std::make_unique<FakeFetcher>();
  fixture.fetcher = fetcher.get();
  fixture.files = tileFiles(QStringLiteral("https://dl.example.test"));
  fetcher->answer(kApi + QStringLiteral("/assets?t=textures"), kAssets);
  fetcher->answer(kApi + QStringLiteral("/files/red_tiles_01"), fixture.files.json);
  for (auto it = fixture.files.images.cbegin(); it != fixture.files.images.cend(); ++it) {
    fetcher->answer(it.key(), it.value());
  }
  fetcher->answer(QStringLiteral("https://cdn.example.test/thumbs/red_tiles_01.png"),
                  imageBytes(QColor(200, 40, 30), 128, 128, "PNG"));
  fixture.source = std::make_unique<PolyHavenTextureSource>(std::move(fetcher));
  return fixture;
}

QVector<TextureAsset> searchOf(TextureSource& source, const QString& text, const QString& category,
                               QString* error = nullptr) {
  QObject context;
  QVector<TextureAsset> found;
  bool done = false;
  source.search(text, category, &context, [&](const QVector<TextureAsset>& assets, const QString& e) {
    found = assets;
    if (error != nullptr) {
      *error = e;
    }
    done = true;
  });
  CHECK(!done); // never within the call
  waitFor([&] { return done; });
  CHECK(done);
  return found;
}

void sourceTests(const QString& cache) {
  Fixture fixture = makeFixture();
  PolyHavenTextureSource& source = *fixture.source;
  CHECK(source.id() == QLatin1String("polyhaven"));
  CHECK(source.name() == QLatin1String("Poly Haven"));
  CHECK(source.usesNetwork());
  CHECK(source.attribution().contains(QStringLiteral("Powered by")) &&
        source.attribution().contains(QStringLiteral("Poly Haven")));
  // Made, nothing asked.
  CHECK(fixture.fetcher->requests.isEmpty());

  // The list is asked for once and kept, on disk too.
  CHECK(searchOf(source, QString(), QString()).size() == 2);
  CHECK(searchOf(source, QStringLiteral("TILES glazed"), QString()).size() == 1);
  CHECK(searchOf(source, QStringLiteral("rock"), QString()).size() == 1);
  CHECK(searchOf(source, QString(), QStringLiteral("Natural")).size() == 1);
  CHECK(searchOf(source, QStringLiteral("nothing"), QString()).isEmpty());
  CHECK(fixture.fetcher->requests == QStringList{kApi + QStringLiteral("/assets?t=textures")});
  CHECK(QFileInfo(cache + QStringLiteral("/polyhaven/textures.json")).isFile());
  QObject context;
  QStringList categories;
  bool answered = false;
  source.categories(&context, [&](const QStringList& names, const QString& error) {
    categories = names;
    answered = error.isEmpty();
  });
  waitFor([&] { return answered; });
  CHECK(categories.contains(QStringLiteral("Man made")));
  const QVector<TextureAsset> tiles = searchOf(source, QStringLiteral("red"), QString());
  CHECK(tiles.size() == 1);
  if (tiles.isEmpty()) {
    return;
  }
  const TextureAsset& asset = tiles.first();

  // The thumbnail: the asset's address asked as PNG in the size shown,
  // kept in the cache.
  QImage thumbnail;
  bool shown = false;
  source.thumbnail(asset, 96, &context, [&](const QImage& image) {
    thumbnail = image;
    shown = true;
  });
  waitFor([&] { return shown; });
  CHECK(!thumbnail.isNull() && thumbnail.width() <= 96);
  const QUrl asked(fixture.fetcher->requests.last());
  CHECK(asked.host() == QLatin1String("cdn.example.test"));
  CHECK(asked.query().contains(QStringLiteral("format=png")) && asked.query().contains(QStringLiteral("width=96")) &&
        asked.query().contains(QStringLiteral("v=1")) && !asked.query().contains(QStringLiteral("webp")));
  CHECK(QFileInfo(cache + QStringLiteral("/polyhaven/red_tiles_01/thumbnail-96.png")).isFile());
  const int before = static_cast<int>(fixture.fetcher->requests.size());
  shown = false;
  source.thumbnail(asset, 96, &context, [&](const QImage& image) {
    thumbnail = image;
    shown = true;
  });
  waitFor([&] { return shown; });
  CHECK(!thumbnail.isNull());
  CHECK(fixture.fetcher->requests.size() == before); // from the cache
  // No thumbnail: a null image, nothing asked.
  shown = false;
  source.thumbnail(searchOf(source, QStringLiteral("stone"), QString()).value(0), 96, &context,
                   [&](const QImage& image) {
                     thumbnail = image;
                     shown = true;
                   });
  waitFor([&] { return shown; });
  CHECK(thumbnail.isNull());

  // Details: the resolutions with base colour images and their bytes.
  TextureAsset detailed;
  QString detailsError;
  bool detailsDone = false;
  source.details(asset, &context, [&](const TextureAsset& a, const QString& error) {
    detailed = a;
    detailsError = error;
    detailsDone = true;
  });
  CHECK(!detailsDone);
  waitFor([&] { return detailsDone; });
  CHECK(detailsError.isEmpty());
  CHECK(detailed.resolutions == QStringList({QStringLiteral("1k"), QStringLiteral("2k")}));
  qint64 twoK = 0;
  for (const PolyHavenFile& file : parsePolyHavenFiles(fixture.files.json).byResolution.value(QStringLiteral("2k"))) {
    twoK += file.size;
  }
  CHECK(detailed.bytes.value(QStringLiteral("2k")) == twoK);
  CHECK(detailed.size == asset.size && detailed.license == asset.license);
  CHECK(QFileInfo(cache + QStringLiteral("/polyhaven/red_tiles_01/files-aaaa1111bbbb2222.json")).isFile());

  // Fetch: the 2k maps into the cache, with progress.
  TextureFiles fetched;
  QString fetchError;
  bool fetchedDone = false;
  qint64 lastDone = 0;
  qint64 lastTotal = 0;
  const auto fetch = [&](const QString& resolution) {
    fetchedDone = false;
    source.fetch(
        detailed, resolution, &context,
        [&](qint64 done, qint64 total) {
          lastDone = done;
          lastTotal = total;
        },
        [&](const TextureFiles& files, const QString& error) {
          fetched = files;
          fetchError = error;
          fetchedDone = true;
        });
    CHECK(!fetchedDone);
    waitFor([&] { return fetchedDone; });
  };
  int requests = static_cast<int>(fixture.fetcher->requests.size());
  fetch(QStringLiteral("2k"));
  CHECK(fetchError.isEmpty());
  CHECK(fetched.resolution == QLatin1String("2k"));
  CHECK(fetched.maps.keys() == QStringList({QStringLiteral("base_color"), QStringLiteral("normal"),
                                            QStringLiteral("occlusion"), QStringLiteral("roughness")}));
  const QString folder = cache + QStringLiteral("/polyhaven/red_tiles_01/2k/");
  CHECK(fetched.maps.value(QStringLiteral("base_color")) == folder + QStringLiteral("red_tiles_01_diff_2k.jpg"));
  CHECK(fetched.maps.value(QStringLiteral("occlusion")) == folder + QStringLiteral("red_tiles_01_ao_2k.png"));
  CHECK(lastTotal == twoK && lastDone == twoK);
  CHECK(fixture.fetcher->requests.size() == requests + 4); // the files were known: the four maps
  for (const QString& file : fetched.maps) {
    QFile written(file);
    CHECK(written.open(QIODevice::ReadOnly));
    CHECK(fixture.files.images.values().contains(written.readAll()));
  }
  // Again: from the cache, nothing asked.
  requests = static_cast<int>(fixture.fetcher->requests.size());
  fetch(QStringLiteral("2k"));
  CHECK(fetchError.isEmpty() && fetched.maps.size() == 4);
  CHECK(fixture.fetcher->requests.size() == requests);

  // Use: the maps embedded, the size the asset's, the source recorded.
  const TextureUse use = textureUseCommand(source.id(), detailed, fetched, detailed.name, {});
  CHECK(use.error.isEmpty() && use.sent == 4);
  const QJsonObject texture = use.command.value(QStringLiteral("texture")).toObject();
  CHECK(texture.value(QStringLiteral("size")).toArray() == QJsonArray({2000.0, 1000.0}));
  CHECK(texture.value(QStringLiteral("source")).toObject() ==
        QJsonObject({{QStringLiteral("id"), QStringLiteral("polyhaven")},
                     {QStringLiteral("asset"), QStringLiteral("red_tiles_01")},
                     {QStringLiteral("resolution"), QStringLiteral("2k")},
                     {QStringLiteral("license"), QStringLiteral("CC0-1.0")},
                     {QStringLiteral("url"), QStringLiteral("https://polyhaven.com/a/red_tiles_01")}}));
  for (const char* map : {"normal", "roughness", "occlusion"}) {
    CHECK(texture.value(QLatin1String(map)).toObject().contains(QStringLiteral("data")));
  }

  // A resolution it does not have; a damaged download.
  fetch(QStringLiteral("8k"));
  CHECK(fetchError.contains(QStringLiteral("no 8k images")) && fetched.maps.isEmpty());
  const QString oneK = fixture.files.images.keys().filter(QStringLiteral("diff_1k")).value(0);
  fixture.fetcher->answer(oneK, fixture.files.images.value(oneK).left(10));
  fetch(QStringLiteral("1k"));
  CHECK(fetchError.contains(QStringLiteral("bytes instead of")));
  QByteArray changed = fixture.files.images.value(oneK);
  changed[changed.size() - 3] = static_cast<char>(changed.at(changed.size() - 3) ^ 0x55);
  fixture.fetcher->answer(oneK, changed);
  fetch(QStringLiteral("1k"));
  CHECK(fetchError.contains(QStringLiteral("checksum")));
  CHECK(!QFileInfo(cache + QStringLiteral("/polyhaven/red_tiles_01/1k/red_tiles_01_diff_1k.jpg")).exists());

  // A second source (the dialog opened again) takes the list from the
  // cache: no request.
  Fixture second = makeFixture();
  CHECK(searchOf(*second.source, QStringLiteral("tiles"), QString()).size() == 1);
  CHECK(second.fetcher->requests.isEmpty());
  // Its files from the cache too (named by the list's files_hash).
  detailsDone = false;
  second.source->details(asset, &context, [&](const TextureAsset& a, const QString& error) {
    detailed = a;
    detailsError = error;
    detailsDone = true;
  });
  waitFor([&] { return detailsDone; });
  CHECK(detailsError.isEmpty() && detailed.resolutions.size() == 2);
  CHECK(second.fetcher->requests.isEmpty());
}

void errorTests(const QString& cache) {
  // An empty cache folder: every answer is asked for.
  qputenv("MITCAD_TEXTURES_DIR", (cache + QStringLiteral("-errors")).toUtf8());
  const auto failing = [](TextureReply::Error error, int status) {
    auto fetcher = std::make_unique<FakeFetcher>();
    fetcher->fail(kApi + QStringLiteral("/assets?t=textures"), error, status);
    return std::make_unique<PolyHavenTextureSource>(std::move(fetcher));
  };
  QString error;
  CHECK(searchOf(*failing(TextureReply::Error::Offline, 0), QString(), QString(), &error).isEmpty());
  CHECK(error.contains(QStringLiteral("could not be reached")) && error.contains(QStringLiteral("Host not found")));
  searchOf(*failing(TextureReply::Error::Timeout, 0), QString(), QString(), &error);
  CHECK(error.contains(QStringLiteral("did not answer in time")));
  searchOf(*failing(TextureReply::Error::Http, 503), QString(), QString(), &error);
  CHECK(error.contains(QStringLiteral("HTTP 503")));
  searchOf(*failing(TextureReply::Error::TooLarge, 0), QString(), QString(), &error);
  CHECK(error.contains(QStringLiteral("more data")));
  {
    auto fetcher = std::make_unique<FakeFetcher>();
    fetcher->answer(kApi + QStringLiteral("/assets?t=textures"), "<html>maintenance</html>");
    PolyHavenTextureSource source(std::move(fetcher));
    searchOf(source, QString(), QString(), &error);
    CHECK(error.contains(QStringLiteral("unexpected answer")));
  }
  // After a failure the next search asks again (and may succeed).
  {
    auto fetcher = std::make_unique<FakeFetcher>();
    FakeFetcher* fake = fetcher.get();
    fake->fail(kApi + QStringLiteral("/assets?t=textures"), TextureReply::Error::Offline);
    PolyHavenTextureSource source(std::move(fetcher));
    searchOf(source, QString(), QString(), &error);
    CHECK(!error.isEmpty());
    fake->answer(kApi + QStringLiteral("/assets?t=textures"), kAssets);
    CHECK(searchOf(source, QString(), QString(), &error).size() == 2);
    CHECK(error.isEmpty());
    CHECK(fake->requests.size() == 2);
    // The service gone: the cached list still answers, however old.
    QFile list(cache + QStringLiteral("-errors/polyhaven/textures.json"));
    CHECK(list.open(QIODevice::ReadWrite));
    CHECK(list.setFileTime(QDateTime::currentDateTime().addDays(-30), QFileDevice::FileModificationTime));
    list.close();
    auto offline = std::make_unique<FakeFetcher>();
    FakeFetcher* gone = offline.get();
    gone->fail(kApi + QStringLiteral("/assets?t=textures"), TextureReply::Error::Offline);
    PolyHavenTextureSource later(std::move(offline));
    CHECK(searchOf(later, QString(), QString(), &error).size() == 2);
    CHECK(error.isEmpty() && gone->requests.size() == 1);
    // A texture whose files cannot be listed: details give the error.
    TextureAsset detailed;
    bool done = false;
    QObject context;
    later.details(searchOf(later, QStringLiteral("tiles"), QString()).value(0), &context,
                  [&](const TextureAsset&, const QString& e) {
                    error = e;
                    done = true;
                  });
    waitFor([&] { return done; });
    CHECK(error.contains(QStringLiteral("HTTP 404")));
  }
  qputenv("MITCAD_TEXTURES_DIR", cache.toUtf8());
}

void fetcherTests(const QString& root) {
  QObject context;
  const auto ask = [&](TextureFetcher& fetcher, const QString& url, qint64 maxBytes = 1 << 20) {
    TextureReply reply;
    bool done = false;
    fetcher.get(QUrl(url), maxBytes, &context, {}, [&](const TextureReply& r) {
      reply = r;
      done = true;
    });
    CHECK(!done);
    waitFor([&] { return done; });
    return reply;
  };
  bool allowed = false;
  // The network off: refused before anything is asked.
  NetworkTextureFetcher network([&allowed] { return allowed; });
  TextureReply reply = ask(network, kApi + QStringLiteral("/assets?t=textures"));
  CHECK(reply.error == TextureReply::Error::Refused && reply.detail.contains(QStringLiteral("Preferences")));
  allowed = true;
  reply = ask(network, QStringLiteral("http://api.example.test/assets"));
  CHECK(reply.error == TextureReply::Error::Refused && reply.detail.contains(QStringLiteral("HTTPS")));
  CHECK(NetworkTextureFetcher::userAgent().startsWith(QLatin1String("MitCad/")));

  // The UI tests' answers: files under <folder>/<host>/<path>.
  QDir().mkpath(root + QStringLiteral("/api.example.test/files"));
  QFile file(root + QStringLiteral("/api.example.test/files/one"));
  CHECK(file.open(QIODevice::WriteOnly) && file.write("{\"a\": 1}") == 8);
  file.close();
  FileTextureFetcher files(root, [&allowed] { return allowed; });
  reply = ask(files, QStringLiteral("https://api.example.test/files/one?x=1"));
  CHECK(reply.ok() && reply.data == "{\"a\": 1}");
  reply = ask(files, QStringLiteral("https://api.example.test/files/two"));
  CHECK(reply.error == TextureReply::Error::Http && reply.status == 404);
  reply = ask(files, QStringLiteral("https://api.example.test/files/one"), 4);
  CHECK(reply.error == TextureReply::Error::TooLarge);
  reply = ask(files, QStringLiteral("https://api.example.test/../../outside"));
  CHECK(!reply.ok());
  allowed = false;
  reply = ask(files, QStringLiteral("https://api.example.test/files/one"));
  CHECK(reply.error == TextureReply::Error::Refused);

  // The registry: Poly Haven uses the network; made, it asks nothing.
  bool listed = false;
  for (const TextureSourceEntry& entry : textureSourceRegistry()) {
    if (QLatin1String(entry.id) == QLatin1String("polyhaven")) {
      listed = entry.usesNetwork;
    }
  }
  CHECK(listed);
}

} // namespace

int polyHavenSourceTests() {
  QTemporaryDir dir;
  CHECK(dir.isValid());
  const QString cache = dir.filePath(QStringLiteral("cache"));
  const QByteArray previous = qgetenv("MITCAD_TEXTURES_DIR");
  qputenv("MITCAD_TEXTURES_DIR", cache.toUtf8());
  parseTests();
  filesTests();
  sourceTests(cache);
  errorTests(cache);
  fetcherTests(dir.filePath(QStringLiteral("fixtures")));
  if (previous.isNull()) {
    qunsetenv("MITCAD_TEXTURES_DIR");
  } else {
    qputenv("MITCAD_TEXTURES_DIR", previous);
  }
  return failures;
}
