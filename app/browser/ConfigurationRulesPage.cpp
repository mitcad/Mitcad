// SPDX-License-Identifier: MIT
// The Rules page of the Configurations window (ConfigurationsDialog.hpp):
// the rule block editor of mitcad#808 (app/configurations/RulesPage.hpp)
// on the window's design. The page reads and writes through the JSON
// queries and commands of the model (commands.md, "Configurations:
// options, rows and rules"); the table itself goes through the
// configurations' JSON adapter (ConfigurationsData.hpp), so that what the
// page does not edit (the options, the rows) goes back as it was.
#include <exception>
#include "framework/ModelMessages.hpp"
#include <utility>

#include <QJsonArray>
#include <QJsonObject>
#include <QPointer>
#include <QVBoxLayout>

#include "../configurations/RulesPage.hpp"
#include "../framework/ParameterField.hpp"
#include "ConfigurationsData.hpp"
#include "ConfigurationsDialog.hpp"
#include "DocumentHost.hpp"

namespace mitcad {
namespace {

QJsonObject queryOrError(DocumentHost& host, const QJsonObject& query) {
  try {
    return host.model().queryObject(query);
  } catch (const std::exception& e) {
    return {{QStringLiteral("error"), mitcad::modelMessage(e.what())}};
  } catch (...) {
    return {{QStringLiteral("error"), QStringLiteral("the design cannot answer now")}};
  }
}

rules::RulesSource sourceFor(DocumentHost& host, QWidget* page) {
  rules::RulesSource source;
  source.configurations = [&host] { return queryOrError(host, configurations::configurationsQuery()); };
  source.configure = [&host] { return queryOrError(host, configurations::configureQuery()); };
  source.setRules = [&host](const QJsonArray& rules, QString* error) {
    if (!host.canChangeModel()) {
      if (error != nullptr) {
        *error = host.changeBlockedReason();
      }
      return false;
    }
    QJsonObject answer;
    try {
      answer = host.model().queryObject(configurations::configurationsQuery());
    } catch (const std::exception& e) {
      if (error != nullptr) {
        *error = mitcad::modelMessage(e.what());
      }
      return false;
    }
    configurations::Table table = configurations::readTable(answer);
    table.other.insert(QStringLiteral("rules"), rules);
    if (host.runModelCommand(configurations::setTableCommand(table))) {
      return true;
    }
    if (error != nullptr) {
      *error = host.lastCancelled() ? QStringLiteral("The computation was canceled.") : mitcad::modelMessage(host.lastError());
    }
    return false;
  };
  source.tryRules = [&host](const QJsonArray& rules) {
    QJsonObject query = configurations::configurationsQuery();
    query.insert(QStringLiteral("rules"), rules);
    return queryOrError(host, query);
  };
  source.checkRules = [&host] { return queryOrError(host, {{QStringLiteral("query"), QStringLiteral("check_rules")}}); };
  source.tables = [&host] {
    return queryOrError(host, {{QStringLiteral("query"), QStringLiteral("formula_help")}})
        .value(QStringLiteral("tables"))
        .toArray();
  };
  source.undo = [&host] {
    if (host.canChangeModel()) {
      host.runModelCommand({{QStringLiteral("cmd"), QStringLiteral("undo")}});
    }
  };
  source.redo = [&host] {
    if (host.canChangeModel()) {
      host.runModelCommand({{QStringLiteral("cmd"), QStringLiteral("redo")}});
    }
  };
  // The Formula Editor's: the main window's parameter source, which the
  // window this page is in belongs to, asked at each call (the page is
  // made before the window takes it).
  const QPointer<QWidget> at(page);
  const auto of = [at]() -> const ParameterSource* { return at != nullptr ? parameterSourceOf(at) : nullptr; };
  source.parameters.parameters = [of] {
    const ParameterSource* parameters = of();
    return parameters != nullptr && parameters->parameters ? parameters->parameters() : QJsonArray();
  };
  source.parameters.addParameter = [of](const QJsonObject& command, QString* error) {
    const ParameterSource* parameters = of();
    return parameters != nullptr && parameters->addParameter ? parameters->addParameter(command, error) : false;
  };
  source.parameters.lengthUnit = [of] {
    const ParameterSource* parameters = of();
    return parameters != nullptr && parameters->lengthUnit ? parameters->lengthUnit() : QStringLiteral("mm");
  };
  source.parameters.query = [of](const QJsonObject& query) {
    const ParameterSource* parameters = of();
    return parameters != nullptr && parameters->query ? parameters->query(query) : QJsonObject();
  };
  source.parameters.limitProblem = [of](const QJsonObject& parameter) {
    const ParameterSource* parameters = of();
    return parameters != nullptr && parameters->limitProblem ? parameters->limitProblem(parameter) : QString();
  };
  return source;
}

class RulesTab : public ConfigurationPage {
public:
  RulesTab(DocumentHost& host, QWidget* parent) : ConfigurationPage(parent) {
    setObjectName(QStringLiteral("configurationsRulesPage"));
    auto* layout = new QVBoxLayout(this);
    layout->setContentsMargins(0, 0, 0, 0);
    m_page = new rules::RulesPage(sourceFor(host, this), this);
    layout->addWidget(m_page);
  }

  void refresh() override { m_page->refresh(); }

private:
  rules::RulesPage* m_page = nullptr;
};

} // namespace

ConfigurationPage* makeConfigurationRulesPage(DocumentHost& host, QWidget* parent) {
  return new RulesTab(host, parent);
}

} // namespace mitcad
