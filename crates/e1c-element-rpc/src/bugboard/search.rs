// Copyright 2026 krylovim. Apache-2.0 WITH Commons Clause 1.0; see LICENSE.
// Project-scoped search additions to bugboard-mcp.
use super::*;

#[cfg(test)]
mod tests;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CatalogProject {
    pub reference: String,
    pub code: String,
    pub title: String,
    pub abbreviation: Option<String>,
    pub updated_at: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SearchBugRow {
    pub reference: String,
    pub number: String,
    pub title: String,
    pub project: CatalogProject,
    pub description: String,
    pub url: String,
}

/// Selectors are exact; query is a literal CONTAINS over code, title and abbreviation.
/// Callers must not treat a limited page as a complete catalog.
pub fn project_catalog_request(
    query: Option<&str>,
    code: Option<&str>,
    reference: Option<&str>,
    limit: u32,
) -> Result<HttpRequest, Error> {
    require_limit(limit)?;
    let mut filters = vec![filter_item(
        "ПометкаУдаления",
        json!({"type":"Std::Boolean","value":false}),
    )];
    if let Some(query) = query {
        require_argument("query", query)?;
        filters.push(contains_any(
            &["Код", "Наименование", "Аббревиатура"],
            query,
        ));
    }
    if let Some(code) = code {
        require_argument("project_code", code)?;
        filters.push(filter_item(
            "Код",
            json!({"type":"Std::String","value":code}),
        ));
    }
    if let Some(reference) = reference {
        require_reference(reference)?;
        filters.push(filter_item(
            "Ссылка",
            json!({"type":PROJECT_REFERENCE_TYPE,"value":reference}),
        ));
    }
    dynamic_list_request(DynamicListRequest::new(
        json!({
            "MainTable": table("e1c::bugboard::Багборд::Проекты"),
            "Fields": fields(&[("Ссылка","Ссылка"),("Код","Код"),("Наименование","Наименование"),
                ("Аббревиатура","Аббревиатура"),("ДатаПоследнегоОбновления","ДатаПоследнегоОбновления")]),
            "Filter": filter_group(filters),
            "Sorting": sorting(&["Наименование", "Код"], 0),
        }),
        limit,
    ))
}

pub fn decode_catalog_projects(
    response: &DynamicListResponse,
) -> Result<Vec<CatalogProject>, Error> {
    response
        .rows()
        .iter()
        .map(|row| {
            let f = required_fields(row, 5)?;
            Ok(CatalogProject {
                reference: required_reference(f, 0, PROJECT_REFERENCE_TYPE)?,
                code: required_text(f, 1)?,
                title: required_text(f, 2)?,
                abbreviation: string_value_at(f, 3),
                updated_at: date_value_at(f, 4),
            })
        })
        .collect()
}

/// The project predicate and text/number predicate are sent in one AND group,
/// before the server applies limit. No client-side global-page filtering.
pub fn bug_content_search_request(
    query: &str,
    project_ref: Option<&str>,
    exact_number: bool,
    limit: u32,
) -> Result<HttpRequest, Error> {
    require_argument("query", query)?;
    require_limit(limit)?;
    let mut filters = vec![filter_item(
        "КУдалению",
        json!({"type":"Std::Boolean","value":false}),
    )];
    if let Some(reference) = project_ref {
        require_reference(reference)?;
        filters.push(filter_item(
            "Проект",
            json!({"type":PROJECT_REFERENCE_TYPE,"value":reference}),
        ));
    }
    filters.push(if exact_number {
        filter_item("Наименование", json!({"type":"Std::String","value":query}))
    } else {
        contains_any(&["Заголовок", "Описание"], query)
    });
    dynamic_list_request(DynamicListRequest::new(
        json!({
            "MainTable": table("e1c::bugboard::Багборд::Ошибки"),
            "Fields": fields(&[("Ссылка","Ссылка"),("Наименование","Наименование"),("Заголовок","Заголовок"),
                ("Проект","Проект"),("Проект.Код","КодПроекта"),("Проект.Наименование","НазваниеПроекта"),
                ("Проект.Аббревиатура","СокращениеПроекта"),("Описание","Описание"),("ФрагментСсылки","ФрагментСсылки"),("ДатаПоследнегоОбновления","ДатаПоследнегоОбновления")]),
            "Filter": filter_group(filters),
            "Sorting": sorting(&["ДатаПоследнегоОбновления", "Ссылка"], 0),
        }),
        limit,
    ))
}

pub fn decode_search_bug_rows(response: &DynamicListResponse) -> Result<Vec<SearchBugRow>, Error> {
    response
        .rows()
        .iter()
        .map(|row| {
            let f = required_fields(row, 10)?;
            let fragment = required_text(f, 8)?;
            Ok(SearchBugRow {
                reference: required_reference(f, 0, BUG_REFERENCE_TYPE)?,
                number: required_text(f, 1)?,
                title: required_text(f, 2)?,
                project: CatalogProject {
                    reference: required_reference(f, 3, PROJECT_REFERENCE_TYPE)?,
                    code: required_text(f, 4)?,
                    title: required_text(f, 5)?,
                    abbreviation: string_value_at(f, 6),
                    updated_at: None,
                },
                description: string_value_at(f, 7)
                    .ok_or(Error::UnexpectedResponse("missing search description"))?,
                url: if fragment.starts_with("http://") || fragment.starts_with("https://") {
                    fragment
                } else {
                    format!("{BUGBOARD_BASE_URL}/#{fragment}")
                },
            })
        })
        .collect()
}

/// Internal reference for enriching legacy full-text results; never expose it to MCP.
pub fn bug_project_reference(response: &Value) -> Result<String, Error> {
    required_reference(
        &[bug_object(response)
            .get("Проект")
            .cloned()
            .unwrap_or(Value::Null)],
        0,
        PROJECT_REFERENCE_TYPE,
    )
}

fn required_text(fields: &[Value], index: usize) -> Result<String, Error> {
    string_value_at(fields, index)
        .filter(|s| !s.trim().is_empty())
        .ok_or(Error::UnexpectedResponse(
            "missing required search/catalog text",
        ))
}

fn contains_any(names: &[&str], query: &str) -> Value {
    // Values are JSON strings, never expressions or interpolated query language.
    let items: Vec<_> = names
        .iter()
        .map(|name| {
            let mut item = filter_item(name, json!({"type":"Std::String","value":query}));
            item["value"]["ComparisonKind"] = json!(0);
            item
        })
        .collect();
    let mut group = filter_group(items);
    group["value"]["Items"]["value"]["items"][0]["value"]["GroupKind"] = json!(1);
    group
}
