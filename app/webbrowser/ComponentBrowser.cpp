// SPDX-License-Identifier: MIT
// Insert › Component from Web's window (mitcad#1579, app/COMMANDS.md
// "Component from Web"): a CEF browser (CefView: off-screen, painted in
// software) with Back, Forward, Reload, an address field and the user's
// Sites; its downloads go to a temporary folder of MitCad's own (no save
// dialog), are checked by the format filter (ComponentWebFiles.hpp) and come
// into the design through the window's import, a zip's supported files
// picked from a list first. A download's folder is removed once its import
// has read it; the rest when the window ends, and at the next start what a
// run left behind. Part of the web browser component's module
// mitcad-webbrowser (app/CMakeLists.txt), which links Qt and CEF only and
// asks the application for the rest (ComponentBrowserHost).
#include "ComponentBrowser.hpp"

#include <algorithm>
#include <utility>

#include <QAction>
#include <QCoreApplication>
#include <QDialogButtonBox>
#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QHBoxLayout>
#include <QHeaderView>
#include <QEvent>
#include <QJsonArray>
#include <QKeyEvent>
#include <QJsonDocument>
#include <QJsonObject>
#include <QLabel>
#include <QLineEdit>
#include <QLocale>
#include <QMenu>
#include <QPointer>
#include <QProgressBar>
#include <QPushButton>
#include <QStyle>
#include <QTimer>
#include <QToolButton>
#include <QTreeWidget>
#include <QUrl>
#include <QVBoxLayout>
#include <QtGlobal>
#include <QtLogging>

#include "cef/CefEngine.hpp"
#include "cef/CefView.hpp"
#include "framework/PanelTree.hpp"

namespace mitcad {
namespace {

// A file of a zip: its path inside the archive and its size.
struct ZipFile {
  QString name;
  qint64 size = 0;
};

QString fileList(const QList<ZipFile>& files) {
  QStringList shown;
  for (const ZipFile& file : files) {
    shown << QStringLiteral("%1 (%2 bytes)").arg(file.name).arg(file.size);
  }
  return shown.isEmpty() ? QStringLiteral("none") : shown.join(QStringLiteral(" | "));
}

// What a page tells the window (CefView::consoleMessage): its links counted,
// and a link followed for the tests. Links that open another window open in
// this one: an off-screen browser has no other window to show.
const char* const kOnPageLoaded =
    "(function(){var fix=function(){Array.prototype.forEach.call(document.querySelectorAll('a[target]'),"
    "function(a){a.target='_self';});};fix();"
    "if(window.MutationObserver&&document.body){new MutationObserver(fix).observe(document.body,"
    "{childList:true,subtree:true});}"
    "console.log('mitcad:links:'+document.links.length);})();";

} // namespace

ComponentBrowser::ComponentBrowser(ComponentBrowserHost* host, QWidget* parent) : QDialog(parent), m_host(host) {
  setObjectName(QStringLiteral("componentBrowser"));
  setWindowTitle(tr("Component from Web"));
  setWindowFlags(windowFlags() | Qt::Window | Qt::WindowMinMaxButtonsHint);
  // The window does not keep MitCad running once the designs' windows are
  // closed.
  setAttribute(Qt::WA_QuitOnClose, false);

  auto* layout = new QVBoxLayout(this);
  auto* bar = new QHBoxLayout;
  const auto toolButton = [this, bar](const QString& name, QStyle::StandardPixmap icon, const QString& tip) {
    auto* button = new QToolButton(this);
    button->setObjectName(name);
    button->setIcon(style()->standardIcon(icon));
    button->setToolTip(tip);
    button->setAccessibleName(tip);
    button->setAutoRaise(true);
    bar->addWidget(button);
    return button;
  };
  m_back = toolButton(QStringLiteral("componentBrowserBack"), QStyle::SP_ArrowBack, tr("Back"));
  m_forward = toolButton(QStringLiteral("componentBrowserForward"), QStyle::SP_ArrowForward, tr("Forward"));
  m_reload = toolButton(QStringLiteral("componentBrowserReload"), QStyle::SP_BrowserReload, tr("Reload"));
  m_address = new QLineEdit(this);
  m_address->setObjectName(QStringLiteral("componentBrowserAddress"));
  m_address->setPlaceholderText(tr("Address"));
  m_address->setToolTip(tr("The page's address"));
  m_address->setClearButtonEnabled(true);
  bar->addWidget(m_address, 1);
  m_sites = new QToolButton(this);
  m_sites->setObjectName(QStringLiteral("componentBrowserSites"));
  m_sites->setText(tr("Sites"));
  m_sites->setToolTip(tr("Your sites, and adding or editing them"));
  m_sites->setToolButtonStyle(Qt::ToolButtonTextOnly);
  bar->addWidget(m_sites);
  layout->addLayout(bar);

  m_view = new CefView(this);
  m_view->setObjectName(QStringLiteral("componentBrowserView"));
  m_view->beforeDownload = [this](const QString& name, const QUrl& url) { return downloadStarting(name, url); };
  layout->addWidget(m_view, 1);

  auto* status = new QHBoxLayout;
  m_note = new QLabel(this);
  m_note->setObjectName(QStringLiteral("componentBrowserNote"));
  m_note->setWordWrap(true);
  m_note->setTextInteractionFlags(Qt::TextSelectableByMouse);
  m_note->hide();
  status->addWidget(m_note, 1);
  m_progress = new QProgressBar(this);
  m_progress->setObjectName(QStringLiteral("componentBrowserProgress"));
  m_progress->setMaximumWidth(240);
  m_progress->setTextVisible(false);
  m_progress->hide();
  status->addWidget(m_progress);
  layout->addLayout(status);

  // Back, Forward and Reload are the user's navigations too.
  const auto navigate = [this](void (CefView::*step)()) {
    return [this, step] {
      m_addressEdited = false;
      m_followPage = true;
      (m_view->*step)();
    };
  };
  connect(m_back, &QToolButton::clicked, this, navigate(&CefView::back));
  connect(m_forward, &QToolButton::clicked, this, navigate(&CefView::forward));
  connect(m_reload, &QToolButton::clicked, this, navigate(&CefView::reload));
  connect(m_sites, &QToolButton::clicked, this, &ComponentBrowser::showSitesMenu);
  connect(m_address, &QLineEdit::returnPressed, this, [this] {
    // The user's navigation: the field follows the page again.
    m_addressEdited = false;
    m_followPage = true;
    openAddress(m_address->text());
  });
  // What the user types is never overwritten by the page (as in a browser):
  // the field follows the page's address only while it is not being edited,
  // and again after the user's next navigation, Escape, or leaving it
  // unedited (eventFilter).
  connect(m_address, &QLineEdit::textEdited, this, [this] {
    m_addressEdited = true;
    m_followPage = false;
  });
  m_address->installEventFilter(this);
  connect(m_view, &CefView::urlChanged, this, [this](const QUrl& url) {
    m_pageAddress = shownAddress(url);
    if (!m_addressEdited && (!m_address->hasFocus() || m_followPage)) {
      m_address->setText(m_pageAddress);
    }
    updateNavigation();
  });
  connect(m_view, &CefView::navigationChanged, this, &ComponentBrowser::updateNavigation);
  connect(m_view, &CefView::loadFinished, this, [this](bool ok) {
    updateNavigation();
    if (!ok) {
      qDebug().noquote() << QStringLiteral("Component browser page failed: %1").arg(m_view->url().toDisplayString());
      return;
    }
    m_view->runJavaScript(QString::fromLatin1(kOnPageLoaded));
  });
  connect(m_view, &CefView::consoleMessage, this, [this](const QString& message) {
    const QString url = m_view->url().toDisplayString();
    if (message.startsWith(QStringLiteral("mitcad:links:"))) {
      // (Not of MitCad's own start page.)
      if (m_view->url().scheme() != QStringLiteral("data")) {
        qDebug().noquote() << QStringLiteral("Component browser page: %1 (%2 links)")
                                  .arg(url, message.mid(int(qstrlen("mitcad:links:"))));
      }
    } else if (message.startsWith(QStringLiteral("mitcad:linkat:"))) {
      // The link's middle: clicked there as real input, which the page takes
      // as the user's gesture (a script's click is none, and a site's second
      // download without one is blocked).
      const QStringList parts = message.mid(int(qstrlen("mitcad:linkat:"))).split(QLatin1Char(','));
      if (parts.size() >= 3) {
        const QString href = parts.mid(2).join(QLatin1Char(','));
        qDebug().noquote() << QStringLiteral("Component browser followed %1").arg(href);
        m_view->clickAt(QPoint(parts.at(0).toInt(), parts.at(1).toInt()));
      }
    } else if (message.startsWith(QStringLiteral("mitcad:nolink:"))) {
      qDebug().noquote() << QStringLiteral("Component browser: no link with %1")
                                .arg(message.mid(int(qstrlen("mitcad:nolink:"))));
    }
  });
  connect(m_view, &CefView::downloadUpdated, this, &ComponentBrowser::downloadUpdated);
  connect(this, &QDialog::finished, this, [] { qDebug().noquote() << QStringLiteral("Component browser closed"); });
  // The temporary files go as MitCad ends, even when its windows are not
  // taken down one by one.
  connect(QCoreApplication::instance(), &QCoreApplication::aboutToQuit, this, [this] { m_folders.removeAll(); });

  m_host->fitWindow(this, QSize(1100, 780));
  showStartPage();
  qDebug().noquote() << QStringLiteral("Component browser opened: %1 site(s)").arg(loadWebSites().size());
}

ComponentBrowser::~ComponentBrowser() = default;

QString ComponentBrowser::shownAddress(const QUrl& url) {
  // MitCad's own start page (a data: address) and about:blank show as an
  // empty field.
  const bool start = url.isEmpty() || url.scheme() == QStringLiteral("about") || url.scheme() == QStringLiteral("data");
  return start ? QString() : url.toDisplayString();
}

bool ComponentBrowser::eventFilter(QObject* watched, QEvent* event) {
  if (watched == m_address) {
    if (event->type() == QEvent::KeyPress && static_cast<QKeyEvent*>(event)->key() == Qt::Key_Escape &&
        m_addressEdited) {
      // Escape gives back the page's address (a second one closes the
      // window, as a dialog's does).
      m_addressEdited = false;
      m_address->setText(m_pageAddress);
      m_address->selectAll();
      return true;
    }
    if (event->type() == QEvent::FocusOut) {
      m_followPage = false;
      if (!m_addressEdited) {
        m_address->setText(m_pageAddress);
      }
    }
    if (event->type() == QEvent::FocusIn) {
      m_followPage = false;
    }
  }
  return QDialog::eventFilter(watched, event);
}

void ComponentBrowser::showStartPage() {
  const ComponentBrowserColors colors = m_host->colors(palette());
  const QString background = colors.window.name();
  const QString text = colors.text.name();
  const QString secondary = colors.secondary.name();
  const QString link = colors.link.name();
  const auto paragraph = [](const QString& words) {
    return QStringLiteral("<p>%1</p>").arg(words.toHtmlEscaped());
  };
  QString body = QStringLiteral("<h1>%1</h1>").arg(tr("Components from the web").toHtmlEscaped());
  const QList<WebSite> sites = loadWebSites();
  if (sites.isEmpty()) {
    body += paragraph(tr("Add the sites you take parts and models from: a supplier, a model library, your own "
                         "server. Type a site's address above, then choose Sites › Add This Page…."));
  } else {
    body += QStringLiteral("<h2>%1</h2><ul>").arg(tr("Your sites").toHtmlEscaped());
    for (const WebSite& site : sites) {
      body += QStringLiteral("<li><a href=\"%1\">%2</a></li>")
                  .arg(site.url.toHtmlEscaped(), site.name.toHtmlEscaped());
    }
    body += QStringLiteral("</ul>");
  }
  body += paragraph(tr("When you download a STEP, IGES, BRep, STL, OBJ or MitCad file, or a zip of them, MitCad "
                       "brings it into the open design as a component and starts Move so that you can place it. "
                       "The component is named after the file, and the name's first part is its part number."));
  body += paragraph(tr("Other files are refused. Sign-ins and cookies stay in MitCad's own browsing data, which "
                       "Sites › Clear Browsing Data… removes."));
  const QString html =
      QStringLiteral("<!doctype html><html><head><meta charset=\"utf-8\"><style>"
                     "body{font-family:sans-serif;background:%1;color:%2;margin:40px;max-width:680px}"
                     "h1{font-size:20px;font-weight:600}h2{font-size:15px;font-weight:600}"
                     "p{line-height:1.5;color:%3}a{color:%4}</style></head><body>%5</body></html>")
          .arg(background, text, secondary, link, body);
  m_view->setHtml(html);
}

void ComponentBrowser::openAddress(const QString& typed) {
  const QString address = normalWebAddress(typed);
  if (address.isEmpty()) {
    showStartPage();
    return;
  }
  const QUrl url = QUrl::fromUserInput(address);
  if (!url.isValid()) {
    showNote(tr("%1 is not a web address.").arg(typed), true);
    return;
  }
  qDebug().noquote() << QStringLiteral("Component browser open: %1").arg(url.toDisplayString());
  m_addressEdited = false;
  m_followPage = true;
  m_address->setText(shownAddress(url));
  m_view->load(url);
}

void ComponentBrowser::updateNavigation() {
  m_back->setEnabled(m_view->canGoBack());
  m_forward->setEnabled(m_view->canGoForward());
}

void ComponentBrowser::showNote(const QString& text, bool warning) {
  QPalette colors = m_note->palette();
  colors.setColor(QPalette::WindowText,
                  warning ? m_host->colors(palette()).warning : palette().color(QPalette::WindowText));
  m_note->setPalette(colors);
  m_note->setText(text);
  m_note->setVisible(!text.isEmpty());
}

void ComponentBrowser::clickLink(const QString& text) {
  // The text as a JSON string inside an array, so that any character in
  // it stays a string; the answer comes back through the page's log.
  const QString quoted = QString::fromUtf8(QJsonDocument(QJsonArray{text}).toJson(QJsonDocument::Compact));
  const QString script =
      QStringLiteral("(function(t){var a=Array.prototype.find.call(document.links,function(l){"
                     "return l.textContent.indexOf(t)>=0||l.href.indexOf(t)>=0;});"
                     "if(!a){console.log('mitcad:nolink:'+t);return;}"
                     "a.scrollIntoView({block:'center'});var r=a.getBoundingClientRect();"
                     "console.log('mitcad:linkat:'+Math.round(r.left+r.width/2)+','+"
                     "Math.round(r.top+r.height/2)+','+a.href);})(%1[0])")
          .arg(quoted);
  m_view->runJavaScript(script);
}


// ---------------------------------------------------------------------------
// Sites

void ComponentBrowser::showSitesMenu() {
  QMenu menu(this);
  const QList<WebSite> sites = loadWebSites();
  QList<QPair<QAction*, QString>> siteActions;
  for (const WebSite& site : sites) {
    QAction* action = menu.addAction(site.name);
    action->setToolTip(site.url);
    siteActions.append({action, site.url});
  }
  if (sites.isEmpty()) {
    menu.addAction(tr("No sites yet"))->setEnabled(false);
  }
  menu.addSeparator();
  QAction* add = menu.addAction(tr("Add This Page…"));
  add->setEnabled(!m_address->text().isEmpty());
  QAction* edit = menu.addAction(tr("Edit Sites…"));
  menu.addSeparator();
  QAction* clear = menu.addAction(tr("Clear Browsing Data…"));
  QAction* chosen = m_host->execMenu(&menu, m_sites->mapToGlobal(QPoint(0, m_sites->height())));
  if (chosen == nullptr) {
    return;
  }
  if (chosen == add) {
    editSites(true);
  } else if (chosen == edit) {
    editSites(false);
  } else if (chosen == clear) {
    clearBrowsingData();
  } else {
    for (const auto& [action, url] : siteActions) {
      if (action == chosen) {
        openAddress(url);
      }
    }
  }
}

void ComponentBrowser::editSites(bool addThisPage) {
  QDialog dialog(this);
  dialog.setWindowTitle(tr("Sites"));
  auto* layout = new QVBoxLayout(&dialog);
  auto* label = new QLabel(tr("The sites the Sites menu opens. Double-click a name or an address to change it."),
                           &dialog);
  label->setWordWrap(true);
  layout->addWidget(label);
  auto* list = new QTreeWidget(&dialog);
  list->setObjectName(QStringLiteral("componentBrowserSiteList"));
  list->setColumnCount(2);
  list->setHeaderLabels({tr("Name"), tr("Address")});
  list->setRootIsDecorated(false);
  panel::setCompactIndentation(list, panel::Indent::Flat);
  list->header()->setStretchLastSection(true);
  const auto addRow = [list](const WebSite& site) {
    auto* item = new QTreeWidgetItem(list, {site.name, site.url});
    item->setFlags(item->flags() | Qt::ItemIsEditable);
    return item;
  };
  for (const WebSite& site : loadWebSites()) {
    addRow(site);
  }
  if (addThisPage) {
    const QString title = m_view->title().trimmed();
    const QString url = m_view->url().toString();
    QTreeWidgetItem* item = addRow({title.isEmpty() ? m_view->url().host() : title, url});
    list->setCurrentItem(item);
  }
  list->resizeColumnToContents(0);
  layout->addWidget(list, 1);
  auto* rowButtons = new QHBoxLayout;
  auto* addButton = new QPushButton(tr("&Add"), &dialog);
  auto* removeButton = new QPushButton(tr("&Remove"), &dialog);
  rowButtons->addWidget(addButton);
  rowButtons->addWidget(removeButton);
  rowButtons->addStretch(1);
  layout->addLayout(rowButtons);
  auto* buttons = new QDialogButtonBox(QDialogButtonBox::Ok | QDialogButtonBox::Cancel, &dialog);
  layout->addWidget(buttons);
  connect(buttons, &QDialogButtonBox::accepted, &dialog, &QDialog::accept);
  connect(buttons, &QDialogButtonBox::rejected, &dialog, &QDialog::reject);
  connect(addButton, &QPushButton::clicked, &dialog, [list, addRow] {
    QTreeWidgetItem* item = addRow({ComponentBrowser::tr("New site"), QStringLiteral("https://")});
    list->setCurrentItem(item);
    list->editItem(item, 0);
  });
  connect(removeButton, &QPushButton::clicked, &dialog, [list] { delete list->currentItem(); });
  m_host->fitWindow(&dialog, QSize(560, 360));
  if (m_host->execDialog(&dialog) != QDialog::Accepted) {
    return;
  }
  QList<WebSite> sites;
  QStringList logged;
  for (int row = 0; row < list->topLevelItemCount(); ++row) {
    const QTreeWidgetItem* item = list->topLevelItem(row);
    const QString url = normalWebAddress(item->text(1));
    if (url.isEmpty() || url == QStringLiteral("https://")) {
      continue;
    }
    const QString name = item->text(0).trimmed().isEmpty() ? url : item->text(0).trimmed();
    sites.append({name, url});
    logged << QStringLiteral("%1 <%2>").arg(name, url);
  }
  saveWebSites(sites);
  qDebug().noquote() << QStringLiteral("Component browser sites: %1")
                            .arg(logged.isEmpty() ? QStringLiteral("none") : logged.join(QStringLiteral(" | ")));
  if (m_address->text().isEmpty()) {
    showStartPage();
  }
}


void ComponentBrowser::clearBrowsingData() {
  if (!m_host->ask(this, tr("Clear Browsing Data"),
                   tr("Remove the cookies, site sign-ins, cache and history of MitCad's browser? Your sites stay."))) {
    return;
  }
  m_view->clearBrowsingData();
  showNote(tr("Browsing data cleared."), false);
  qDebug().noquote() << QStringLiteral("Component browser: browsing data cleared");
}

// ---------------------------------------------------------------------------
// Downloads

QString ComponentBrowser::downloadStarting(const QString& suggestedName, const QUrl& url) {
  QString name = QFileInfo(suggestedName).fileName();
  if (name.isEmpty()) {
    name = QFileInfo(url.path()).fileName();
  }
  const QString from = url.toDisplayString();
  const WebFileCheck check = checkWebFileName(name);
  if (check.kind == WebFileKind::Refused) {
    qInfo().noquote() << QStringLiteral("Component browser refused %1 from %2: %3").arg(name, from, check.note);
    showNote(check.note, true);
    return QString();
  }
  const QString folder = m_folders.newDownloadFolder();
  if (folder.isEmpty()) {
    showNote(tr("MitCad could not make a temporary folder for %1.").arg(name), true);
    return QString();
  }
  ++m_downloading;
  m_progress->setRange(0, 0);
  m_progress->show();
  showNote(tr("Downloading %1…").arg(name), false);
  qInfo().noquote() << QStringLiteral("Component browser download: %1 from %2").arg(name, from);
  return QDir(folder).filePath(name);
}

void ComponentBrowser::downloadUpdated(quint32 /*id*/, qint64 received, qint64 total, int state, const QString& path,
                                       const QString& reason) {
  const QString name = QFileInfo(path).fileName();
  const QString folder = QFileInfo(path).absolutePath();
  if (state == static_cast<int>(CefView::DownloadState::InProgress)) {
    if (total > 0) {
      m_progress->setRange(0, 1000);
      m_progress->setValue(static_cast<int>(received * 1000 / total));
      showNote(tr("Downloading %1: %2 of %3")
                   .arg(name, QLocale().formattedDataSize(received), QLocale().formattedDataSize(total)),
               false);
    }
    return;
  }
  m_downloading = std::max(0, m_downloading - 1);
  if (m_downloading == 0) {
    m_progress->hide();
  }
  if (state != static_cast<int>(CefView::DownloadState::Complete)) {
    const bool cancelled = state == static_cast<int>(CefView::DownloadState::Canceled);
    qInfo().noquote() << QStringLiteral("Component browser download of %1 %2%3")
                             .arg(name, cancelled ? QStringLiteral("canceled") : QStringLiteral("failed"),
                                  cancelled ? QString() : QStringLiteral(": ") + reason);
    showNote(cancelled ? tr("The download of %1 was canceled.").arg(name)
                       : tr("The download of %1 failed: %2").arg(name, reason),
             !cancelled);
    m_folders.remove(folder);
    return;
  }
  qInfo().noquote() << QStringLiteral("Component browser downloaded %1 (%2 bytes)")
                           .arg(name)
                           .arg(QFileInfo(path).size());
  showNote(QString(), false);
  // After CEF's own handler: the import runs the model and may open a
  // dialog.
  QTimer::singleShot(0, this, [this, path, folder] {
    bool keep = false;
    if (checkWebFileName(path).kind == WebFileKind::Zip) {
      importZip(path, folder, &keep);
    } else {
      importFile(path, &keep);
    }
    if (!keep) {
      m_folders.remove(folder);
      qDebug().noquote() << QStringLiteral("Component browser removed the temporary files of %1")
                                .arg(QFileInfo(path).fileName());
    }
  });
}

bool ComponentBrowser::importFile(const QString& path, bool* keep) {
  const QString name = QFileInfo(path).fileName();
  QByteArray head;
  QFile file(path);
  if (file.open(QIODevice::ReadOnly)) {
    head = file.read(kWebFileHeadSize);
  }
  file.close();
  const WebFileCheck check = checkWebFileContent(path, head);
  if (check.kind == WebFileKind::Refused || check.kind == WebFileKind::Zip) {
    const QString note = check.kind == WebFileKind::Zip ? tr("%1 is a zip inside a zip: unpack it first.").arg(name)
                                                        : check.note;
    qInfo().noquote() << QStringLiteral("Component browser refused %1: %2").arg(name, note);
    showNote(note, true);
    return false;
  }
  QString error;
  const bool ok = m_host->importDownload(path, check.kind, &error);
  if (!ok) {
    qInfo().noquote() << QStringLiteral("Component browser could not import %1: %2").arg(name, error);
    showNote(tr("Could not import %1: %2").arg(name, error), true);
    return false;
  }
  // A design of its own is imported by a process that still reads the file.
  if (check.kind == WebFileKind::Document) {
    *keep = true;
  }
  showNote(tr("Imported %1.").arg(name), false);
  return true;
}

void ComponentBrowser::importZip(const QString& path, const QString& folder, bool* keep) {
  const QString zipName = QFileInfo(path).fileName();
  QByteArray head;
  QFile file(path);
  if (file.open(QIODevice::ReadOnly)) {
    head = file.read(kWebFileHeadSize);
  }
  file.close();
  const WebFileCheck check = checkWebFileContent(path, head);
  if (check.kind != WebFileKind::Zip) {
    qInfo().noquote() << QStringLiteral("Component browser refused %1: %2").arg(zipName, check.note);
    showNote(check.note, true);
    return;
  }
  QString readError;
  const QString listed = m_host->zipList(path, &readError);
  if (!readError.isEmpty()) {
    qInfo().noquote() << QStringLiteral("Component browser could not read %1: %2").arg(zipName, readError);
    showNote(tr("Could not read %1: %2").arg(zipName, readError), true);
    return;
  }
  const QJsonArray entries = QJsonDocument::fromJson(listed.toUtf8()).array();
  QList<ZipFile> supported;
  int others = 0;
  for (const QJsonValue& value : entries) {
    const QJsonObject entry = value.toObject();
    const ZipFile zipped{entry.value(QStringLiteral("name")).toString(),
                         static_cast<qint64>(entry.value(QStringLiteral("size")).toDouble())};
    const WebFileKind kind = checkWebFileName(zipped.name).kind;
    if (kind == WebFileKind::Component || kind == WebFileKind::Document) {
      supported.append(zipped);
    } else {
      ++others;
    }
  }
  qInfo().noquote() << QStringLiteral("Component browser zip %1: %2 file(s) MitCad imports: %3; %4 other(s)")
                           .arg(zipName)
                           .arg(supported.size())
                           .arg(fileList(supported))
                           .arg(others);
  if (supported.isEmpty()) {
    showNote(tr("%1 holds no file MitCad imports.").arg(zipName), true);
    return;
  }
  // Unpacked next to the zip, in its download's folder.
  const QString unpacked = QDir(folder).filePath(QStringLiteral("unpacked"));
  QList<ZipFile> files;
  for (const ZipFile& zipped : std::as_const(supported)) {
    QString unpackError;
    if (m_host->zipExtract(path, zipped.name, QDir(unpacked).filePath(zipped.name), &unpackError)) {
      files.append(zipped);
    } else {
      qInfo().noquote() << QStringLiteral("Component browser could not unpack %1 from %2: %3")
                               .arg(zipped.name, zipName, unpackError);
    }
  }
  if (files.isEmpty()) {
    showNote(tr("Could not unpack the files of %1.").arg(zipName), true);
    return;
  }
  QStringList chosen;
  if (files.size() == 1) {
    chosen << files.first().name;
  } else {
    QDialog dialog(this);
    dialog.setWindowTitle(tr("Files in %1").arg(zipName));
    auto* layout = new QVBoxLayout(&dialog);
    auto* label = new QLabel(tr("Choose the files to bring in as components:"), &dialog);
    layout->addWidget(label);
    auto* list = new QTreeWidget(&dialog);
    list->setObjectName(QStringLiteral("componentBrowserZipFiles"));
    list->setColumnCount(2);
    list->setHeaderLabels({tr("File"), tr("Size")});
    list->setRootIsDecorated(false);
    panel::setCompactIndentation(list, panel::Indent::Flat);
    for (const ZipFile& zipped : std::as_const(files)) {
      auto* item = new QTreeWidgetItem(list, {zipped.name, QLocale().formattedDataSize(zipped.size)});
      item->setFlags(item->flags() | Qt::ItemIsUserCheckable);
      item->setCheckState(0, Qt::Unchecked);
      item->setTextAlignment(1, Qt::AlignRight | Qt::AlignVCenter);
    }
    list->setCurrentItem(list->topLevelItem(0));
    list->resizeColumnToContents(0);
    layout->addWidget(list, 1);
    auto* buttons = new QDialogButtonBox(QDialogButtonBox::Cancel, &dialog);
    QPushButton* importButton = buttons->addButton(tr("&Import"), QDialogButtonBox::AcceptRole);
    importButton->setEnabled(false);
    layout->addWidget(buttons);
    connect(buttons, &QDialogButtonBox::accepted, &dialog, &QDialog::accept);
    connect(buttons, &QDialogButtonBox::rejected, &dialog, &QDialog::reject);
    connect(list, &QTreeWidget::itemChanged, &dialog, [list, importButton] {
      QStringList checked;
      for (int row = 0; row < list->topLevelItemCount(); ++row) {
        if (list->topLevelItem(row)->checkState(0) == Qt::Checked) {
          checked << list->topLevelItem(row)->text(0);
        }
      }
      importButton->setEnabled(!checked.isEmpty());
      qDebug().noquote() << QStringLiteral("Component browser zip picked: %1")
                                .arg(checked.isEmpty() ? QStringLiteral("none") : checked.join(QStringLiteral(" | ")));
    });
    m_host->fitWindow(&dialog, QSize(520, 320));
    if (m_host->execDialog(&dialog) != QDialog::Accepted) {
      qDebug().noquote() << QStringLiteral("Component browser zip %1: nothing imported").arg(zipName);
      return;
    }
    for (int row = 0; row < list->topLevelItemCount(); ++row) {
      if (list->topLevelItem(row)->checkState(0) == Qt::Checked) {
        chosen << list->topLevelItem(row)->text(0);
      }
    }
  }
  for (const QString& name : std::as_const(chosen)) {
    bool kept = false;
    importFile(QDir(unpacked).filePath(name), &kept);
    *keep = *keep || kept;
  }
}

} // namespace mitcad

// The module's entry points (ComponentBrowserHost.hpp), resolved by name by
// the application (webbrowser/MainWindowComponentWeb.cpp).
extern "C" Q_DECL_EXPORT int mitcad_component_browser_abi() { return mitcad::kComponentBrowserAbi; }

extern "C" Q_DECL_EXPORT QDialog* mitcad_create_component_browser(mitcad::ComponentBrowserHost* host,
                                                                  QWidget* parent) {
  if (host == nullptr) {
    return nullptr;
  }
  // CEF once for the process, from the component's folder.
  QString error;
  if (!mitcad::cef::startEngine(host->componentFolder(), host->profileFolder(), &error)) {
    qWarning().noquote() << QStringLiteral("Component browser: %1").arg(error);
    return nullptr;
  }
  return new mitcad::ComponentBrowser(host, parent);
}
