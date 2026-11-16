// SPDX-License-Identifier: MIT
// Component from Web's browser in the feedback diagnostics (mitcad#1906):
// see BrowserDiagnostics.hpp.
#include "BrowserDiagnostics.hpp"

#include <QChar>
#include <QDateTime>
#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QJsonDocument>
#include <QJsonObject>
#include <QLatin1String>
#include <QObject>
#include <QSaveFile>
#include <QUrl>
#include <QVariantMap>

namespace mitcad::browserdiag {

const char* const kWebglProbeScript =
    "(function(){"
    "if(!window.__mitcadWebglLost){window.__mitcadWebglLost=true;"
    "window.addEventListener('webglcontextlost',function(e){"
    "console.log('mitcad:webgl-lost '+((e&&e.statusMessage)||''));},true);}"
    "var c=document.createElement('canvas'),why='',v='2',gl=null;"
    "c.addEventListener('webglcontextcreationerror',function(e){why=e.statusMessage||why;});"
    "try{gl=c.getContext('webgl2');}catch(x){}"
    "if(!gl){v='1';try{gl=c.getContext('webgl');}catch(x){}}"
    "if(!gl){console.log('mitcad:webgl off '+(why||'no WebGL context'));return;}"
    "var i=gl.getExtension('WEBGL_debug_renderer_info');"
    "var r=i?gl.getParameter(i.UNMASKED_RENDERER_WEBGL):gl.getParameter(gl.RENDERER);"
    "var l=gl.getExtension('WEBGL_lose_context');if(l){l.loseContext();}"
    "console.log('mitcad:webgl on '+v+' '+r);})();";

namespace {

QString oneLine(const QString& text) {
  QString line = text;
  line.replace(QLatin1String("\r\n"), QLatin1String(" | "));
  line.replace(QLatin1Char('\n'), QLatin1String(" | "));
  line.replace(QLatin1Char('\r'), QLatin1String(" | "));
  return line.trimmed();
}

QString cut(const QString& text) {
  return text.size() > kLineLength ? text.left(kLineLength - 1) + QChar(0x2026) : text;
}

bool softwareRenderer(const QString& renderer) {
  const QString lower = renderer.toLower();
  return lower.contains(QLatin1String("swiftshader")) || lower.contains(QLatin1String("llvmpipe")) ||
         lower.contains(QLatin1String("softpipe")) || lower.contains(QLatin1String("software rasterizer")) ||
         lower.contains(QLatin1String("microsoft basic render"));
}

void keepNewest(QStringList& lines, const QString& line, int count) {
  lines << line;
  while (lines.size() > count) {
    lines.removeFirst();
  }
}

} // namespace

QVariant toVariant(const State& state) {
  QVariantMap map;
  map.insert(QStringLiteral("cef"), state.cef);
  map.insert(QStringLiteral("softwareOnly"), state.softwareOnly);
  map.insert(QStringLiteral("softwareReason"), state.softwareReason);
  map.insert(QStringLiteral("webgl"), state.webgl);
  map.insert(QStringLiteral("webglDetail"), state.webglDetail);
  map.insert(QStringLiteral("gpuFailures"), state.gpuFailures);
  map.insert(QStringLiteral("gpuLines"), state.gpuLines);
  map.insert(QStringLiteral("softwareNext"), state.softwareNext);
  map.insert(QStringLiteral("contextsLost"), state.contextsLost);
  map.insert(QStringLiteral("pageErrors"), state.pageErrors);
  return map;
}

State fromVariant(const QVariant& value) {
  const QVariantMap map = value.toMap();
  State state;
  state.cef = map.value(QStringLiteral("cef")).toString();
  state.softwareOnly = map.value(QStringLiteral("softwareOnly")).toBool();
  state.softwareReason = map.value(QStringLiteral("softwareReason")).toString();
  state.webgl = map.value(QStringLiteral("webgl")).toString();
  state.webglDetail = map.value(QStringLiteral("webglDetail")).toString();
  state.gpuFailures = map.value(QStringLiteral("gpuFailures")).toInt();
  state.gpuLines = map.value(QStringLiteral("gpuLines")).toStringList();
  state.softwareNext = map.value(QStringLiteral("softwareNext")).toBool();
  state.contextsLost = map.value(QStringLiteral("contextsLost")).toInt();
  state.pageErrors = map.value(QStringLiteral("pageErrors")).toStringList();
  return state;
}

bool keepPageMessage(bool error, const QString& message, const QString& source) {
  if (source == QLatin1String(kScriptSource) || message.startsWith(QLatin1String("mitcad:"))) {
    return false;
  }
  if (error) {
    return true;
  }
  const QString lower = message.toLower();
  return lower.contains(QLatin1String("webgl")) || lower.contains(QLatin1String("context lost")) ||
         lower.contains(QLatin1String("context_lost")) || lower.contains(QLatin1String("contextlost"));
}

QString pageErrorLine(const QString& pageUrl, const QString& message, const QString& source, int line) {
  const QUrl page(pageUrl);
  QString where = page.host();
  if (where.isEmpty()) {
    where = page.scheme().isEmpty() ? QStringLiteral("page") : page.scheme();
  }
  QString text = cut(where + QStringLiteral(": ") + oneLine(message));
  const QString file = QUrl(source).fileName();
  if (!file.isEmpty()) {
    text += line > 0 ? QStringLiteral(" (%1:%2)").arg(file).arg(line) : QStringLiteral(" (%1)").arg(file);
  }
  return text;
}

void addPageError(State& state, const QString& line) {
  if (!state.pageErrors.isEmpty() && state.pageErrors.last() == line) {
    return;
  }
  keepNewest(state.pageErrors, line, kPageErrors);
}

GpuLogLine classifyGpuLogLine(const QString& line) {
  const QString lower = line.toLower();
  // Chromium's own words: "GPU process exited unexpectedly: exit_code=…",
  // "The GPU process exited with code …", "Exiting GPU process because some
  // drivers can't recover from errors", "GPU process launch failed", "The
  // GPU process failed to start!", "GPU process isn't usable. Goodbye.",
  // "The GPU process crashed!", "GPU state invalid after
  // WaitForGetOffsetInRange." (a renderer's context lost with its GPU
  // channel), "Lost UI shared context."
  static const char* const failures[] = {
      "gpu process exited",        "exiting gpu process",       "gpu process launch failed",
      "gpu process failed to start", "gpu process isn't usable", "gpu process is not usable",
      "gpu process has crashed",   "gpu process crashed",       "killed the gpu process",
      "gpu state invalid",         "lost ui shared context",
  };
  for (const char* failure : failures) {
    if (lower.contains(QLatin1String(failure))) {
      return GpuLogLine::Failure;
    }
  }
  static const char* const related[] = {"gpu", "webgl", "swiftshader", "vulkan", "egl", "/gl/", "gl_", "/angle/"};
  for (const char* word : related) {
    if (lower.contains(QLatin1String(word))) {
      return GpuLogLine::Gpu;
    }
  }
  return line.contains(QLatin1String("ANGLE")) ? GpuLogLine::Gpu : GpuLogLine::Other;
}

QString gpuLogText(const QString& line) {
  QString text = line.trimmed();
  if (text.startsWith(QLatin1Char('['))) {
    const qsizetype end = text.indexOf(QLatin1String("] "));
    if (end > 0) {
      text = text.mid(end + 2);
    }
  }
  return cut(oneLine(text));
}

void addGpuLine(State& state, const QString& text) { keepNewest(state.gpuLines, text, kGpuLines); }

bool parseWebglProbe(const QString& message, QString* webgl, QString* detail) {
  const QLatin1String prefix("mitcad:webgl ");
  if (!message.startsWith(prefix)) {
    return false;
  }
  const QString rest = message.mid(prefix.size()).trimmed();
  if (rest == QLatin1String("off") || rest.startsWith(QLatin1String("off "))) {
    const QString why = rest.mid(3).trimmed();
    *webgl = QStringLiteral("off");
    *detail = cut(why.isEmpty() ? QStringLiteral("no WebGL context") : oneLine(why));
    return true;
  }
  if (rest.startsWith(QLatin1String("on "))) {
    const QString body = rest.mid(3).trimmed();
    const QString version = body.section(QLatin1Char(' '), 0, 0);
    const QString renderer = body.section(QLatin1Char(' '), 1).trimmed();
    *webgl = softwareRenderer(renderer) ? QStringLiteral("software") : QStringLiteral("gpu");
    *detail = cut(QStringLiteral("WebGL %1, %2")
                      .arg(version, renderer.isEmpty() ? QStringLiteral("renderer not told") : oneLine(renderer)));
    return true;
  }
  return false;
}

QStringList diagnosticsLines(const State& state) {
  QStringList lines;
  if (state.cef.isEmpty()) {
    return lines;
  }
  lines << QStringLiteral("Component browser: CEF %1, WebGL %2")
               .arg(state.cef, state.softwareOnly
                                   ? QStringLiteral("in software only (%1)").arg(state.softwareReason)
                                   : QStringLiteral("on the GPU where Chromium accepts it, else in software"));
  QString webgl;
  if (state.webgl == QLatin1String("gpu")) {
    webgl = QStringLiteral("on the GPU, ") + state.webglDetail;
  } else if (state.webgl == QLatin1String("software")) {
    webgl = QStringLiteral("in software, ") + state.webglDetail;
  } else if (state.webgl == QLatin1String("off")) {
    webgl = QStringLiteral("off, ") + state.webglDetail;
  } else {
    webgl = QStringLiteral("not checked yet (no page loaded)");
  }
  lines << QStringLiteral("Component browser WebGL: ") + webgl;
  QString process = state.gpuFailures == 0
                        ? QStringLiteral("no failure in CEF's log")
                        : QStringLiteral("failures in CEF's log: %1").arg(state.gpuFailures);
  if (state.softwareNext) {
    process += QStringLiteral("; WebGL in software from the next start");
  }
  lines << QStringLiteral("Component browser GPU process: ") + process;
  for (const QString& line : state.gpuLines) {
    lines << QStringLiteral("Component browser GPU log: ") + line;
  }
  if (state.contextsLost > 0) {
    lines << QStringLiteral("Component browser WebGL contexts lost: %1").arg(state.contextsLost);
  }
  if (state.pageErrors.isEmpty()) {
    lines << QStringLiteral("Component browser page errors: none");
  }
  for (const QString& line : state.pageErrors) {
    lines << QStringLiteral("Component browser page error: ") + line;
  }
  return lines;
}

QStringList componentBrowserDiagnostics(const QObject* application) {
  if (application == nullptr) {
    return {};
  }
  const QVariant value = application->property(kStateProperty);
  return value.isValid() ? diagnosticsLines(fromVariant(value)) : QStringList();
}

GpuChoice chooseGpu(const QString& variable, const QString& markerPath, const QString& cef) {
  GpuChoice choice;
  const QString chosen = variable.trimmed().toLower();
  if (chosen == QLatin1String("software")) {
    choice.software = true;
    choice.reason = QStringLiteral("MITCAD_WEB_GPU=software");
    return choice;
  }
  if (chosen == QLatin1String("gpu") || cef.isEmpty()) {
    return choice;
  }
  QFile file(markerPath);
  if (!file.open(QIODevice::ReadOnly)) {
    return choice;
  }
  const QJsonObject marker = QJsonDocument::fromJson(file.readAll()).object();
  if (marker.value(QStringLiteral("cef")).toString() != cef) {
    return choice;
  }
  const QString when = marker.value(QStringLiteral("written")).toString().left(10);
  const QString why = marker.value(QStringLiteral("reason")).toString();
  choice.software = true;
  choice.reason = QStringLiteral("the GPU failed in an earlier session%1%2")
                      .arg(when.isEmpty() ? QString() : QStringLiteral(" (%1)").arg(when),
                           why.isEmpty() ? QString() : QStringLiteral(": ") + why);
  return choice;
}

bool writeSoftwareMarker(const QString& path, const QString& cef, const QString& reason) {
  QDir().mkpath(QFileInfo(path).absolutePath());
  QJsonObject marker;
  marker.insert(QStringLiteral("cef"), cef);
  marker.insert(QStringLiteral("reason"), reason);
  marker.insert(QStringLiteral("written"), QDateTime::currentDateTimeUtc().toString(Qt::ISODate));
  QSaveFile file(path);
  if (!file.open(QIODevice::WriteOnly)) {
    return false;
  }
  file.write(QJsonDocument(marker).toJson(QJsonDocument::Indented));
  return file.commit();
}

bool softwareNextWanted(const State& state, bool gpuWebglSeen, bool gpuFailed) {
  if (state.softwareNext || state.softwareOnly) {
    return false;
  }
  const bool off = state.webgl == QLatin1String("off");
  return off || (gpuFailed && gpuWebglSeen);
}

} // namespace mitcad::browserdiag
