// SPDX-License-Identifier: MIT
#ifdef _WIN32
#include <windows.h>
#else
#include <cerrno>
#include <unistd.h>
#endif

#include "F3dImport.hpp"

#include <algorithm>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <functional>
#include <memory>
#include <string>
#include <thread>
#include <utility>

#include <QCloseEvent>
#include <QCommandLineParser>
#include <QCoreApplication>
#include <QDialog>
#include <QDialogButtonBox>
#include <QFile>
#include <QFileInfo>
#include <QJsonArray>
#include <QJsonDocument>
#include <QLabel>
#include <QProcess>
#include <QProcessEnvironment>
#include <QProgressBar>
#include <QPushButton>
#include <QRegularExpression>
#include <QSaveFile>
#include <QTimer>
#include <QVBoxLayout>
#include <QtLogging>

#include "../framework/Diagnostics.hpp"
#include "../framework/Json.hpp"
#include "../framework/ResultCache.hpp"
#include "mitcad_bridge/lib.h"

namespace mitcad {
namespace {

// Seconds without progress after which the importer takes a geometry
// kernel call to hang (the model's hang_limit).
constexpr int kHangLimit = 90;

const QRegularExpression kItemStart(QStringLiteral(R"(^import: \[[0-9.]+\] item ([0-9]+): (.*)$)"));
const QRegularExpression kItemDone(
    QStringLiteral(R"(^import: \[[0-9.]+\] (.+): (parametric|partial|fallback|skipped) in [0-9.]+ s$)"));

bool writeFile(const QString& path, const QByteArray& data) {
  QSaveFile file(path);
  if (!file.open(QIODevice::WriteOnly)) {
    return false;
  }
  file.write(data);
  return file.commit();
}

// The application's requests on the worker's standard input, a line each,
// until it closes: "stop" (T1e) cancels the job of the import's document,
// which stops the import early. Read without the C library's streams,
// whose locks the process's exit takes while this thread waits.
void readRequests(const std::shared_ptr<rust::Box<JobControl>>& control) {
  std::string pending;
  char buffer[256];
  for (;;) {
#ifdef _WIN32
    DWORD count = 0;
    if (ReadFile(GetStdHandle(STD_INPUT_HANDLE), buffer, static_cast<DWORD>(sizeof buffer), &count, nullptr) == 0 ||
        count == 0) {
      return;
    }
#else
    const ssize_t count = ::read(STDIN_FILENO, buffer, sizeof buffer);
    if (count < 0 && errno == EINTR) {
      continue;
    }
    if (count <= 0) {
      return;
    }
#endif
    pending.append(buffer, static_cast<std::size_t>(count));
    std::size_t end = 0;
    while ((end = pending.find('\n')) != std::string::npos) {
      // (Without the line's end and spaces around: cmd's echo leaves some.)
      const QByteArray request = QByteArray::fromStdString(pending.substr(0, end)).trimmed();
      pending.erase(0, end + 1);
      if (request == "stop" && !(*control)->is_cancelled()) {
        (*control)->cancel();
        std::printf("import-stopping\n");
        std::fflush(stdout);
      }
    }
  }
}

} // namespace

// ---------------------------------------------------------------------------
// The progress dialog

// The import's progress (T1e), modal to the window: the file, the timeline
// item being replayed of how many, the time so far, and two ways to end the
// import early. Esc and closing the dialog cancel the import; Enter and
// Space do nothing, as they were meant for the window.
class ImportProgress : public QDialog {
public:
  ImportProgress(QWidget* parent, std::function<void()> stop, std::function<void()> cancel)
      : QDialog(parent), m_stop(std::move(stop)), m_cancel(std::move(cancel)), m_file(new QLabel(this)),
        m_item(new QLabel(this)), m_bar(new QProgressBar(this)), m_time(new QLabel(this)) {
    setObjectName(QStringLiteral("importProgress"));
    setWindowTitle(F3dImport::tr("Import"));
    setWindowModality(Qt::WindowModal);
    setMinimumWidth(460);
    auto* layout = new QVBoxLayout(this);
    layout->addWidget(m_file);
    layout->addWidget(m_item);
    m_bar->setTextVisible(false);
    m_bar->setRange(0, 0); // busy until the worker counts the items
    layout->addWidget(m_bar);
    layout->addWidget(m_time);
    auto* buttons = new QDialogButtonBox(this);
    m_stopButton =
        buttons->addButton(F3dImport::tr("&Stop and Keep What Is Imported"), QDialogButtonBox::ActionRole);
    m_stopButton->setToolTip(F3dImport::tr("End the import now: what is imported stays, and the remaining "
                                              "timeline items come in as the file's bodies"));
    QPushButton* cancelButton = buttons->addButton(F3dImport::tr("&Cancel Import"), QDialogButtonBox::RejectRole);
    cancelButton->setToolTip(F3dImport::tr("Discard the import; the open document stays as it is"));
    for (QPushButton* button : {m_stopButton, cancelButton}) {
      button->setAutoDefault(false);
      button->setDefault(false);
      button->setFocusPolicy(Qt::NoFocus);
    }
    connect(m_stopButton, &QPushButton::clicked, this, [this] { m_stop(); });
    connect(cancelButton, &QPushButton::clicked, this, [this] { m_cancel(); });
    layout->addWidget(buttons);
  }

  void showProgress(const QString& file, const QString& item, int done, int items, const QString& time) {
    m_file->setText(file);
    m_item->setText(item);
    if (items > 0) {
      m_bar->setRange(0, items);
      m_bar->setValue(std::min(done, items));
    }
    m_time->setText(time);
  }

  // A stop was asked for: once is enough.
  void setStopping() { m_stopButton->setEnabled(false); }

  // An import that cannot stop early (a FreeCAD document's bodies, an
  // .ipt part's) has only Cancel.
  void setStoppable(bool stoppable) { m_stopButton->setVisible(stoppable); }

protected:
  void reject() override { m_cancel(); }

  void closeEvent(QCloseEvent* event) override {
    event->ignore();
    m_cancel();
  }

private:
  std::function<void()> m_stop;
  std::function<void()> m_cancel;
  QLabel* m_file;
  QLabel* m_item;
  QProgressBar* m_bar;
  QLabel* m_time;
  QPushButton* m_stopButton = nullptr;
};

// ---------------------------------------------------------------------------
// The worker process

bool isImportWorker(int argc, char* argv[]) {
  for (int i = 1; i < argc; ++i) {
    if (std::strcmp(argv[i], "--import-worker") == 0) {
      return true;
    }
  }
  return false;
}

namespace {

// A FreeCAD document (.FCStd) rather than an .f3d design.
bool isFreeCadFile(const QString& path) {
  return QFileInfo(path).suffix().compare(QStringLiteral("fcstd"), Qt::CaseInsensitive) == 0;
}

// An .ipt part file (mitcad#60): its stored bodies, without a timeline.
bool isIptFile(const QString& path) {
  return QFileInfo(path).suffix().compare(QStringLiteral("ipt"), Qt::CaseInsensitive) == 0;
}

// The worker's exit status. While import threads that the hang watchdog
// gave up still run (their geometry kernel call has not returned), the
// worker ends here, without the static destructors that would tear down
// OCCT's state under them and crash the worker after it wrote its result
// (mitcad#82).
int endWorker(int code) {
  if (abandoned_imports() > 0) {
    std::fflush(nullptr);
    std::_Exit(code);
  }
  return code;
}

} // namespace

int runImportWorker(int argc, char* argv[]) {
  QCoreApplication app(argc, argv);
  QCommandLineParser parser;
  const QCommandLineOption worker(QStringLiteral("import-worker"),
                                  QStringLiteral(".f3d, .f3z, .FCStd or .ipt file"), QStringLiteral("file"));
  const QCommandLineOption output(QStringLiteral("output"), QStringLiteral("Project file to write"),
                                  QStringLiteral("file"));
  const QCommandLineOption result(QStringLiteral("result"), QStringLiteral("Result (JSON) to write"),
                                  QStringLiteral("file"));
  // The result store (P7d): the design's costly results go there, so that
  // the application does not compute them again when it reads the project.
  const QCommandLineOption store(QStringLiteral("result-store"), QStringLiteral("Result store folder"),
                                 QStringLiteral("folder"));
  const QCommandLineOption minMs(QStringLiteral("persist-min-ms"), QStringLiteral("Milliseconds"),
                                 QStringLiteral("ms"));
  parser.addOptions({worker, output, result, store, minMs});
  parser.process(app);
  const QByteArray path = parser.value(worker).toUtf8();

  rust::Box<Document> document = new_document();
  // Stop (T1e): import_f3d takes the cancel of the document's job as a
  // request to stop early. For tests, the job's delay slows down the
  // definitions it tries (MITCAD_TEST_RECOMPUTE_DELAY_MS=import_f3d=<ms>).
  const auto control = std::make_shared<rust::Box<JobControl>>(new_job_control());
  if (const int delay = testRecomputeDelay(QStringLiteral("import_f3d")); delay > 0) {
    (*control)->set_test_delay(static_cast<std::uint32_t>(delay));
  }
  std::thread([control] { readRequests(control); }).detach();
  // A FreeCAD document: its structure, sketches and history in one
  // command, which has no stop yet (and no item count for the progress);
  // an .ipt part's bodies likewise.
  const bool freecad = isFreeCadFile(QString::fromUtf8(path));
  const bool ipt = isIptFile(QString::fromUtf8(path));
  // The timeline's length, for the progress dialog.
  try {
    if (!freecad && !ipt) {
      const QJsonObject designs = parseObject(document->import_f3d_timeline(rustStr(path), "{\"list\": true}"));
      int items = 0;
      for (const QJsonValue& design : designs.value(QStringLiteral("designs")).toArray()) {
        items = std::max(items, design.toObject().value(QStringLiteral("items")).toInt());
      }
      std::printf("import-items %d\n", items);
      std::fflush(stdout);
    }
  } catch (const std::exception&) {
    // The import itself says what is wrong with the file.
  }
  const QJsonObject command =
      freecad ? QJsonObject{{QStringLiteral("cmd"), QStringLiteral("import_fcstd")},
                            {QStringLiteral("path"), QString::fromUtf8(path)},
                            {QStringLiteral("bodies_only"), false}}
      : ipt   ? QJsonObject{{QStringLiteral("cmd"), QStringLiteral("import_ipt")},
                            {QStringLiteral("path"), QString::fromUtf8(path)}}
              : QJsonObject{{QStringLiteral("cmd"), QStringLiteral("import_f3d")},
                            {QStringLiteral("path"), QString::fromUtf8(path)},
                            {QStringLiteral("hang_limit"), kHangLimit}};
  QJsonObject answer;
  document->attach_job(**control);
  try {
    answer = parseObject(document->command(rustStr(compactJson(command))));
  } catch (const std::exception& e) {
    std::fprintf(stderr, "import-failed %s\n", e.what());
    return endWorker(3);
  }
  // A stop from now on has nothing to stop.
  document->detach_job();
  if (parser.isSet(store)) {
    const QByteArray folder = parser.value(store).toUtf8();
    const QByteArray label = QFileInfo(QString::fromUtf8(path)).fileName().toUtf8();
    document->set_result_store(rustStr(folder), rustStr(resultStoreBuildId().toUtf8()), rustStr(label));
    const QJsonObject persisted = parseObject(document->persist_results(parser.value(minMs).toDouble()));
    std::printf("import-stored %d\n", persisted.value(QStringLiteral("results")).toInt());
    std::fflush(stdout);
  }
  const rust::String project = document->to_json();
  if (!writeFile(parser.value(output), QByteArray(project.data(), static_cast<qsizetype>(project.size()))) ||
      !writeFile(parser.value(result), QJsonDocument(answer).toJson(QJsonDocument::Compact))) {
    std::fprintf(stderr, "import-failed cannot write the result\n");
    return endWorker(3);
  }
  std::printf("import-done\n");
  std::fflush(stdout);
  return endWorker(0);
}

// ---------------------------------------------------------------------------
// The application's side

F3dImport::F3dImport(const QString& path, QWidget* window)
    : QObject(window), m_path(path), m_window(window) {}

F3dImport::~F3dImport() {
  if (m_process != nullptr && m_process->state() != QProcess::NotRunning) {
    m_process->kill();
    m_process->waitForFinished(2000);
  }
  delete m_dialog;
}

void F3dImport::start() {
  if (!m_dir.isValid()) {
    done(false, tr("No temporary directory for the import."));
    return;
  }
  m_process = new QProcess(this);
  QProcessEnvironment environment = QProcessEnvironment::systemEnvironment();
  // The importer's trace gives a line per timeline item: the progress.
  environment.insert(QStringLiteral("MITCAD_IMPORT_TRACE"), QStringLiteral("1"));
  m_process->setProcessEnvironment(environment);
  connect(m_process, &QProcess::readyReadStandardOutput, this, &F3dImport::readProgress);
  connect(m_process, &QProcess::readyReadStandardError, this, &F3dImport::readProgress);
  connect(m_process, &QProcess::finished, this, [this](int exitCode) { processFinished(exitCode); });
  connect(m_process, &QProcess::errorOccurred, this, [this](QProcess::ProcessError error) {
    if (error == QProcess::FailedToStart) {
      done(false, tr("The import could not start: %1").arg(m_process->errorString()));
    }
  });

  const QString name = QFileInfo(m_path).fileName();
  m_dialog = new ImportProgress(
      m_window, [this] { stop(); }, [this] { cancel(); });
  m_dialog->setStoppable(!isFreeCadFile(m_path) && !isIptFile(m_path));
  m_ticker = new QTimer(this);
  m_ticker->setInterval(500);
  connect(m_ticker, &QTimer::timeout, this, &F3dImport::tick);

  m_clock.start();
  m_ticker->start();
  tick();
  m_dialog->show();
  qInfo().noquote() << QStringLiteral("Import of %1 started").arg(name);
  QStringList arguments{QStringLiteral("--import-worker"), m_path, QStringLiteral("--output"),
                        m_dir.filePath(QStringLiteral("import.mitcad")), QStringLiteral("--result"),
                        m_dir.filePath(QStringLiteral("result.json"))};
  // The worker has no application name for the user's folders: it is
  // told where the result store is (P7d).
  if (const QString store = resultStoreDirectory(); !store.isEmpty()) {
    arguments << QStringLiteral("--result-store") << store << QStringLiteral("--persist-min-ms")
              << QString::number(resultStoreMinMs());
  }
  m_process->start(QCoreApplication::applicationFilePath(), arguments);
}

void F3dImport::readProgress() {
  if (m_process == nullptr) {
    return;
  }
  m_errors += m_process->readAllStandardError();
  m_errors += m_process->readAllStandardOutput();
  const QString name = QFileInfo(m_path).fileName();
  qsizetype end = 0;
  while ((end = m_errors.indexOf('\n')) >= 0) {
    const QString line = QString::fromUtf8(m_errors.left(end)).trimmed();
    m_errors.remove(0, end + 1);
    if (line.startsWith(QStringLiteral("import-items "))) {
      m_items = line.mid(13).toInt();
      continue;
    }
    if (line.startsWith(QStringLiteral("import-stored "))) {
      qInfo().noquote() << QStringLiteral("Import stored %1 results").arg(line.mid(14).toInt());
      continue;
    }
    if (line == QStringLiteral("import-stopping")) {
      qInfo().noquote() << QStringLiteral("Import of %1 stopping: the remaining items take the file's bodies")
                               .arg(name);
      continue;
    }
    const QRegularExpressionMatch start = kItemStart.match(line);
    if (start.hasMatch()) {
      m_position = start.captured(1).toInt();
      m_current = start.captured(2);
      continue;
    }
    const QRegularExpressionMatch item = kItemDone.match(line);
    if (item.hasMatch()) {
      ++m_done;
      qDebug().noquote() << QStringLiteral("Import item %1: %2 %3").arg(m_done).arg(item.captured(1), item.captured(2));
      continue;
    }
    if (line.startsWith(QStringLiteral("import-failed ")) ||
        (!line.startsWith(QStringLiteral("import:")) && !line.isEmpty())) {
      m_lastLine = line.startsWith(QStringLiteral("import-failed ")) ? line.mid(14) : line;
    }
  }
  tick();
}

void F3dImport::tick() {
  if (m_finished || m_dialog == nullptr) {
    return;
  }
  const qint64 seconds = m_clock.elapsed() / 1000;
  QString item;
  if (m_items > 0) {
    const int position = std::clamp(m_position, 1, m_items);
    item = m_current.isEmpty() ? tr("Timeline item %1 of %2").arg(position).arg(m_items)
                               : tr("Timeline item %1 of %2: %3").arg(position).arg(m_items).arg(m_current);
  }
  const QString time = m_stopping ? tr("Stopping: the remaining items come in as the file's bodies (%1 s)").arg(seconds)
                                  : tr("%1 s elapsed").arg(seconds);
  m_dialog->showProgress(tr("Importing %1").arg(QFileInfo(m_path).fileName()), item, m_done, m_items, time);
}

void F3dImport::stop() {
  if (m_finished || m_stopping || m_process == nullptr) {
    return;
  }
  m_stopping = true;
  qInfo().noquote() << QStringLiteral("Import of %1: stop requested after %2 s")
                           .arg(QFileInfo(m_path).fileName())
                           .arg(m_clock.elapsed() / 1000);
  m_process->write("stop\n");
  m_dialog->setStopping();
  tick();
}

void F3dImport::cancel() {
  if (m_finished) {
    return;
  }
  qInfo().noquote() << QStringLiteral("Import of %1 cancelled").arg(QFileInfo(m_path).fileName());
  if (m_process != nullptr) {
    m_process->kill();
  }
  done(false, QString());
}

void F3dImport::processFinished(int exitCode) {
  if (m_finished) {
    return;
  }
  readProgress();
  if (exitCode != 0 || m_process->exitStatus() != QProcess::NormalExit) {
    const QString why = m_lastLine.isEmpty() ? tr("the import process stopped (exit code %1)").arg(exitCode)
                                             : m_lastLine;
    done(false, why);
    return;
  }
  QFile project(m_dir.filePath(QStringLiteral("import.mitcad")));
  QFile result(m_dir.filePath(QStringLiteral("result.json")));
  if (!project.open(QIODevice::ReadOnly) || !result.open(QIODevice::ReadOnly)) {
    done(false, tr("The import wrote no result."));
    return;
  }
  m_finished = true;
  m_ticker->stop();
  m_dialog->hide();
  emit finished(true, project.readAll(), QJsonDocument::fromJson(result.readAll()).object(), QString());
  deleteLater();
}

void F3dImport::done(bool ok, const QString& error) {
  if (m_finished) {
    return;
  }
  m_finished = true;
  if (m_ticker != nullptr) {
    m_ticker->stop();
  }
  if (m_dialog != nullptr) {
    m_dialog->hide();
  }
  emit finished(ok, QByteArray(), QJsonObject(), error);
  deleteLater();
}

} // namespace mitcad
