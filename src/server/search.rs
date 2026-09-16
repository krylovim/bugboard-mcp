// Copyright 2026 krylovim. Apache-2.0 WITH Commons Clause 1.0; see LICENSE.
// Explicit per-call product context; no persistent catalog or repository binding.
use super::*;
use bugboard::{CatalogProject, SearchBugRow};

#[cfg(test)]
mod tests;

#[derive(Default, Deserialize, JsonSchema)]
pub(super) struct ProjectListParams {
    /// Literal substring of the official title, abbreviation or stable code.
    query: Option<String>,
    limit: Option<u32>,
}

#[derive(Clone, Copy, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(super) enum SearchMode {
    Auto,
    Text,
    Number,
}

impl BugboardServer {
    pub(super) async fn project_catalog_value(
        &self,
        params: ProjectListParams,
    ) -> Result<Value, ToolFailure> {
        let limit = normalize_limit(params.limit)?;
        let query = optional_nonempty(params.query.as_deref(), "query")?;
        let client = self.client()?;
        let parsed = client
            .execute_dynamic_list(
                "project_list",
                bugboard::project_catalog_request(query, None, None, limit + 1)?,
            )
            .await?;
        let rows = bugboard::decode_catalog_projects(&parsed)
            .map_err(|e| ToolFailure::bugboard_changed("project_list", e))?;
        let has_more = rows.len() > limit as usize;
        let projects = rows
            .iter()
            .take(limit as usize)
            .map(|p| self.catalog_project_value(p))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(
            json!({"count":projects.len(), "projects":projects, "limit":limit,
            "query":query, "has_more":has_more,
            "coverage": page_coverage(has_more),
            "selection":"candidates_only"}),
        )
    }

    fn catalog_project_value(&self, p: &CatalogProject) -> Result<Value, ToolFailure> {
        normalize_project_row(self, p)
    }

    // This exact resolver is also the future catalog-cache boundary (#2).
    // Never resolve a code from the first page of an unfiltered catalog.
    async fn search_project(
        &self,
        code: Option<&str>,
        handle: Option<&str>,
    ) -> Result<Option<CatalogProject>, ToolFailure> {
        let code = optional_nonempty(code, "project_code")?;
        let handle = optional_nonempty(handle, "project_handle")?;
        let reference = handle.map(|h| self.resolve_project_ref(h)).transpose()?;
        if code.is_none() && reference.is_none() {
            return Ok(None);
        }
        let client = self.client()?;
        // Resolve code independently: adding a handle must not hide duplicate codes.
        let parsed = client
            .execute_dynamic_list(
                "project_resolve",
                bugboard::project_catalog_request(
                    None,
                    code,
                    if code.is_none() {
                        reference.as_deref()
                    } else {
                        None
                    },
                    2,
                )?,
            )
            .await?;
        let rows = bugboard::decode_catalog_projects(&parsed)
            .map_err(|e| ToolFailure::bugboard_changed("project_resolve", e))?;
        match rows.as_slice() {
            [] => Err(ToolFailure::new(
                "unknown_project",
                "No visible product matches this exact code or handle.",
                json!({"project_code":code}),
            )),
            [p] => {
                if code.is_some_and(|code| code != p.code) {
                    return Err(ToolFailure::new(
                        "unknown_project",
                        "Product codes are exact and case-sensitive. Use project_list candidates.",
                        json!({"project_code":code}),
                    ));
                }
                if reference
                    .as_ref()
                    .is_some_and(|reference| reference != &p.reference)
                {
                    return Err(ToolFailure::invalid_arguments(
                        "project_code and project_handle identify different products.",
                    ));
                }
                Ok(Some(p.clone()))
            }
            _ => Err(ToolFailure::new(
                "ambiguous_project",
                "Product selector is ambiguous; no product was selected.",
                json!({"candidates":rows.iter().map(|p| self.catalog_project_value(p)).collect::<Result<Vec<_>,_>>()?,
                    "candidates_complete":false}),
            )),
        }
    }

    pub(super) async fn bug_search_value(
        &self,
        params: BugSearchParams,
    ) -> Result<Value, ToolFailure> {
        let query = params.query.trim();
        if query.is_empty() {
            return Err(ToolFailure::invalid_arguments("Set a non-empty query."));
        }
        let limit = normalize_limit(params.limit)?;
        let number = match params.mode.unwrap_or(SearchMode::Auto) {
            SearchMode::Number => {
                if !is_bug_number(query) {
                    return Err(ToolFailure::invalid_arguments(
                        "A bug number must contain ASCII digit groups separated by single hyphens.",
                    ));
                }
                true
            }
            SearchMode::Text => false,
            SearchMode::Auto => is_bug_number(query),
        };
        let project = self
            .search_project(
                params.project_code.as_deref(),
                params.project_handle.as_deref(),
            )
            .await?;
        let legacy = params.mode.is_none() && project.is_none() && !number;
        if legacy {
            return self.legacy_text_search(query, limit).await;
        }
        let client = self.client()?;
        let parsed = client
            .execute_dynamic_list(
                "bug_search",
                bugboard::bug_content_search_request(
                    query,
                    project.as_ref().map(|p| p.reference.as_str()),
                    number,
                    limit + 1,
                )?,
            )
            .await?;
        let rows = bugboard::decode_search_bug_rows(&parsed)
            .map_err(|e| ToolFailure::bugboard_changed("bug_search", e))?;
        // Fail closed on scope/number drift. Never quietly filter a global page locally.
        validate_search_rows(&rows, project.as_ref(), number.then_some(query))?;
        let has_more = rows.len() > limit as usize;
        let ambiguous = number && rows.len() > 1;
        // Keep existing card fields in search responses. Enrichment is bounded by limit.
        let refs = rows
            .iter()
            .take(limit as usize)
            .map(|r| r.reference.clone())
            .collect();
        let filter = json!({"query":query, "mode":if number {"bug_lookup"} else {"text"},
            "effective_mode":if number {"number"} else {"text"}, "full_text":false,
            "project":project.as_ref().map(|p| self.catalog_project_value(p)).transpose()?,
            "scope":if project.is_some() {"project"} else {"all_visible_projects"},
            "fields":if number {vec!["number"]} else {vec!["title","description"]},
            "operator":if number {"equals"} else {"contains_literal"},
            "fields_join":"OR", "project_join":"AND", "server_filtered":true});
        let mut result = self
            .bug_references_response(client, refs, limit, filter)
            .await?;
        for (bug, row) in result["bugs"].as_array_mut().unwrap().iter_mut().zip(&rows) {
            bug["project"] = self.catalog_project_value(&row.project)?;
            // Search snapshot owns identity and link even if a subsequent card read changed.
            bug["number"] = json!(row.number);
            bug["url"] = json!(row.url);
            if !number {
                let q = query.to_lowercase();
                let mut matched = Vec::new();
                if row.title.to_lowercase().contains(&q) {
                    matched.push("title");
                }
                if row.description.to_lowercase().contains(&q) {
                    matched.push("description");
                }
                bug["matched_fields"] = json!(matched);
                bug["match_evidence"] =
                    json!("client_unicode_lowercase_check; server collation may differ");
            }
        }
        result["has_more"] = json!(has_more);
        result["ambiguous"] = json!(ambiguous);
        result["coverage"] = page_coverage(has_more);
        result["limitations"] = json!([
            "Only currently visible, non-deleted bugs are searched.",
            "Text is a literal substring, not morphology, synonyms or semantic search; server collation applies.",
            "No cursor pagination or total count is claimed. Data may change between list and card reads.",
            "No match does not prove that the problem is absent; affected versions require card analysis."
        ]);
        Ok(result)
    }

    async fn legacy_text_search(&self, query: &str, limit: u32) -> Result<Value, ToolFailure> {
        let client = self.client()?;
        let value = client
            .execute_module_call("bug_search", bugboard::bug_full_text_search_request(query)?)
            .await?;
        let refs = bugboard::decode_full_text_bug_references(&value)
            .map_err(|e| ToolFailure::bugboard_changed("bug_search", e))?;
        let has_more = refs.len() > limit as usize;
        let server = self.clone();
        let mut result = self.bug_references_response_with(refs, limit,
            json!({"query":query,"mode":"full_text","effective_mode":"legacy_full_text","full_text":true,
                "scope":"all_visible_projects","project":null}), move |reference| {
                let client = Arc::clone(&client);
                let server = server.clone();
                async move {
                    let raw = client.execute("bug_get", bugboard::bug_get_request(&reference)?).await?;
                    let project_ref = bugboard::bug_project_reference(&raw)
                        .map_err(|e| ToolFailure::bugboard_changed("bug_search",e))?;
                    let handle = server.remember_ref(HandleKind::Project, &project_ref)?;
                    let project = server.search_project(None, Some(&handle)).await?
                        .ok_or_else(|| ToolFailure::internal("missing project"))?;
                    let mut bug = normalize_bug_details(raw)?;
                    bug["project"] = server.catalog_project_value(&project)?;
                    Ok(bug)
                }
            }).await?;
        result["has_more"] = json!(has_more);
        result["ambiguous"] = json!(false);
        result["coverage"] = json!({"source":"legacy_rpc","complete":false,"truncated":has_more,
            "pagination_supported":false,"total":null});
        result["limitations"] = json!([
            "Legacy RPC matching rules and index coverage are unverified. Use explicit mode=text for title/description matching.",
            "No match does not prove that the problem is absent."
        ]);
        Ok(result)
    }
}

fn optional_nonempty<'a>(
    value: Option<&'a str>,
    field: &str,
) -> Result<Option<&'a str>, ToolFailure> {
    value
        .map(|v| {
            let v = v.trim();
            if v.is_empty() {
                Err(ToolFailure::invalid_arguments(format!(
                    "{field} must not be empty."
                )))
            } else {
                Ok(v)
            }
        })
        .transpose()
}

fn page_coverage(has_more: bool) -> Value {
    // Even a short page is not proof of a complete server result: cursor protocol
    // and server-side caps have not been verified. has_more only reflects lookahead.
    json!({"source":"dynamic_list_first_page","complete":false,"truncated":has_more,
        "more_results":if has_more {"yes"} else {"unknown"},
        "pagination_supported":false,"total":null})
}

fn validate_search_rows(
    rows: &[SearchBugRow],
    project: Option<&CatalogProject>,
    number: Option<&str>,
) -> Result<(), ToolFailure> {
    if rows.iter().any(|row| {
        project.is_some_and(|p| p.reference != row.project.reference || p.code != row.project.code)
            || number.is_some_and(|n| n != row.number)
    }) {
        return Err(ToolFailure::bugboard_changed(
            "bug_search",
            "server returned a row outside the requested project or number",
        ));
    }
    Ok(())
}
