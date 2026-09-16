// Copyright 2026 krylovim. Apache-2.0 WITH Commons Clause 1.0; see LICENSE.
use super::*;
use axum::{
    Router,
    routing::{get, post},
};

fn typed(t: &str, value: Value) -> Value {
    json!({"type":t,"value":value})
}
fn project(code: &str, title: &str) -> Value {
    json!({"Ссылка":typed(bugboard::PROJECT_REFERENCE_TYPE,json!(format!("ref-{code}"))),
        "Код":code,"Наименование":title,"Аббревиатура":format!("1С:{code}"),
        "ПометкаУдаления":false,"ДатаПоследнегоОбновления":"2026-09-17"})
}
fn bug(code: &str, id: usize, title: &str, description: &str) -> Value {
    json!({"Ссылка":typed(bugboard::BUG_REFERENCE_TYPE,json!(format!("bug-ref-{code}-{id}"))),
        "Наименование":"00-123", "Заголовок":title,"Описание":description,"КУдалению":false,
        "Проект":typed(bugboard::PROJECT_REFERENCE_TYPE,json!(format!("ref-{code}"))),
        "Проект.Код":code,"Проект.Наименование":format!("Product {code}"),"Проект.Аббревиатура":code,
        "ФрагментСсылки":format!("{code}/00-123"),
        "ИсторияЖизни":typed("Std::Collections::Array<e1c::bugboard::Багборд::Ошибки.ИсторияЖизни>",json!({"items":[]}))})
}
fn matches(filter: &Value, row: &Value) -> bool {
    let f = &filter["value"];
    if f["Use"] == false {
        return true;
    }
    if let Some(items) = f.pointer("/Items/value/items").and_then(Value::as_array) {
        return match f["GroupKind"].as_u64().unwrap() {
            0 => items.iter().all(|item| matches(item, row)),
            1 => items.iter().any(|item| matches(item, row)),
            _ => panic!("unverified filter group"),
        };
    }
    let actual = &row[f["Field"].as_str().unwrap()];
    let actual = actual.get("value").unwrap_or(actual);
    let wanted = &f["Value"]["value"];
    match f["ComparisonKind"].as_u64().unwrap() {
        1 => actual == wanted,
        0 => actual
            .as_str()
            .unwrap_or_default()
            .to_lowercase()
            .contains(&wanted.as_str().unwrap().to_lowercase()),
        _ => panic!("unverified comparison"),
    }
}

struct Fixture {
    server: BugboardServer,
    task: tokio::task::JoinHandle<()>,
    requests: Arc<Mutex<Vec<Value>>>,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.task.abort();
    }
}
impl Fixture {
    async fn new() -> Self {
        let projects = Arc::new(vec![
            project("bp3", "Бухгалтерия предприятия, редакция 3.0"),
            project("bgu", "Бухгалтерия государственного учреждения"),
            project("erp", "ERP"),
            project("duplicate", "Duplicate one"),
            project("duplicate", "Duplicate two"),
        ]);
        // Many unrelated results precede BP, detecting a global-page then local-filter bug.
        let mut bugs: Vec<_> = (0..60)
            .map(|i| bug("erp", i, "НДС", "ERP description"))
            .collect();
        bugs.extend([
            bug(
                "bp3",
                0,
                "Расчет налога",
                r#"Только описание: НДС и 50% _ [x] \"цитата\""#,
            ),
            bug("bp3", 1, "НДС", "Other description"),
            bug("bp3", 2, "НДС", "Third description"),
        ]);
        let bugs = Arc::new(bugs);
        let requests = Arc::new(Mutex::new(Vec::new()));
        let app = Router::new()
            .route("/",get(|| async { "var __gSrv_APP_HASH='test'; var __gSrv_SRV_VERSION='1';" }))
            .route("/ui/dynamic-list",post({
                let bugs = Arc::clone(&bugs); let requests = Arc::clone(&requests);
                move |body: String| {
 let body: Value = serde_json::from_str(&body).unwrap();
                    let bugs = Arc::clone(&bugs); let projects = Arc::clone(&projects); let requests = Arc::clone(&requests);
                    async move {
                        requests.lock().unwrap().push(body.clone());
                        let spec = &body["dynamicList"];
                        let data = if spec["MainTable"]["value"]["Table"].as_str().unwrap().ends_with("Проекты") { projects } else { bugs };
                        let rows: Vec<_> = data.iter().filter(|r| matches(&spec["Filter"],r))
                            .take(body["limit"].as_u64().unwrap() as usize).map(|r| {
                                let values: Vec<_> = spec["Fields"]["value"]["items"].as_array().unwrap().iter()
                                    .map(|f| r[f["value"]["Expression"].as_str().unwrap()].clone()).collect();
                                json!({"fieldValues":values})
                            }).collect();
                        json_response(json!({"rows":rows}))
                    }
                }
            }))
            .route("/ui/entity/read",post({ let bugs = Arc::clone(&bugs); move |body: String| {
 let body: Value = serde_json::from_str(&body).unwrap();
                let bugs = Arc::clone(&bugs);
                async move {
                    // Entity request contains the exact typed reference somewhere in the envelope.
                    let raw = body.to_string();
                    let row = bugs.iter().find(|r| raw.contains(r["Ссылка"]["value"].as_str().unwrap())).unwrap();
                    json_response(json!({"object":row}))
                }
            }}))
            .route("/ui/module/call",post(|body: String| async move {
 let body: Value = serde_json::from_str(&body).unwrap();
                assert_eq!(body["moduleName"],bugboard::FULL_TEXT_SEARCH_MODULE);
                json_response(json!({"result":{"type":"e1c::bugboard::Компоненты::ПоискДанных::РезультатГлобальногоПоиска",
                    "value":{"НайденныеОшибки":typed("Std::Collections::Array<e1c::bugboard::Багборд::Ошибки.Reference>",json!({"items":[]}))}},
                    "debugExitReason":"NONE"}))
            }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let task = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        let server = BugboardServer {
            client: Ok(Arc::new(BugboardClient::for_test(&base))),
            tool_router: BugboardServer::tool_router(),
            handles: Arc::new(Mutex::new(HandleStore::default())),
        };
        Self {
            server,
            task,
            requests,
        }
    }
    async fn search(&self, args: Value) -> Result<Value, ToolFailure> {
        self.server
            .bug_search_value(serde_json::from_value(args).unwrap())
            .await
    }
}
fn error_code(error: ToolFailure) -> Value {
    error.into_result().structured_content.unwrap()["error"]["code"].clone()
}

#[tokio::test]
async fn project_filter_precedes_limit_and_description_only_match_survives() {
    let f = Fixture::new().await;
    let r = f
        .search(json!({"query":"ндс","project_code":"bp3","mode":"text","limit":1}))
        .await
        .unwrap();
    assert_eq!(r["count"], 1);
    assert_eq!(r["has_more"], true);
    assert_eq!(r["bugs"][0]["project"]["project_code"], "bp3");
    assert_eq!(r["bugs"][0]["matched_fields"], json!(["description"]));
    assert_eq!(r["filter"]["project"]["project_code"], "bp3");
    assert!(!r.to_string().contains("ref-bp3"));
    let requests = f.requests.lock().unwrap();
    assert_eq!(requests[1]["limit"], 2);
}

#[tokio::test]
async fn catalog_candidates_unknown_duplicate_and_conflicting_selectors() {
    let f = Fixture::new().await;
    let r = f
        .server
        .project_catalog_value(serde_json::from_value(json!({"query":"Бухгалтерия"})).unwrap())
        .await
        .unwrap();
    assert_eq!(r["count"], 2);
    assert_eq!(r["selection"], "candidates_only");
    assert_eq!(r["coverage"]["complete"], false);
    for (code, expected) in [
        ("unknown", "unknown_project"),
        ("duplicate", "ambiguous_project"),
        ("Бухгалтерия", "unknown_project"),
    ] {
        assert_eq!(
            error_code(
                f.search(json!({"query":"НДС","project_code":code}))
                    .await
                    .unwrap_err()
            ),
            expected
        );
    }
    let handle = r["projects"][0]["project_handle"].as_str().unwrap();
    assert!(
        f.search(json!({"query":"НДС","project_code":"bp3","project_handle":handle,"limit":1}))
            .await
            .is_ok()
    );
    assert_eq!(
        error_code(
            f.search(json!({"query":"НДС","project_code":"erp","project_handle":handle}))
                .await
                .unwrap_err()
        ),
        "invalid_arguments"
    );
}

#[tokio::test]
async fn duplicate_numbers_are_candidates_and_get_never_selects_first() {
    let f = Fixture::new().await;
    let r = f.search(json!({"query":"00-123","limit":1})).await.unwrap();
    assert_eq!(r["ambiguous"], true);
    assert_eq!(r["has_more"], true);
    assert_eq!(r["filter"]["effective_mode"], "number");
    assert_eq!(
        error_code(f.server.resolve_bug_number("00-123").await.unwrap_err()),
        "ambiguous_bug_number"
    );
    let handle = r["bugs"][0]["bug_handle"].as_str().unwrap().to_owned();
    let card = f
        .server
        .bug_get_value(BugGetParams {
            bug_handle: Some(handle),
            bug_number: None,
        })
        .await
        .unwrap();
    assert_eq!(card["number"], "00-123");
}

#[tokio::test]
async fn literal_special_characters_empty_pages_limits_and_mode_validation() {
    let f = Fixture::new().await;
    let digits_as_text = f
        .search(json!({"query":"00-123","mode":"text"}))
        .await
        .unwrap();
    assert_eq!(digits_as_text["count"], 0);
    assert_eq!(digits_as_text["filter"]["effective_mode"], "text");
    let automatic_text = f
        .search(json!({"query":"НДС","mode":"auto","limit":1}))
        .await
        .unwrap();
    assert_eq!(automatic_text["filter"]["effective_mode"], "text");
    let no_number = f
        .search(json!({"query":"999999999","mode":"number"}))
        .await
        .unwrap();
    assert_eq!(no_number["count"], 0);
    assert_eq!(no_number["ambiguous"], false);
    for query in ["50%", "_ [x]", "\\\"цитата\\\""] {
        let r = f
            .search(json!({"query":query,"project_code":"bp3","mode":"text"}))
            .await
            .unwrap();
        assert_eq!(r["count"], 1, "{query}");
        assert_eq!(r["has_more"], false);
        assert_eq!(r["coverage"]["complete"], false);
    }
    assert_eq!(
        f.search(json!({"query":"no-such-text","mode":"text"}))
            .await
            .unwrap()["count"],
        0
    );
    for args in [
        json!({"query":" "}),
        json!({"query":"x","limit":0}),
        json!({"query":"x","limit":51}),
        json!({"query":"x","project_code":" "}),
        json!({"query":"x","mode":"number"}),
    ] {
        assert_eq!(
            error_code(f.search(args).await.unwrap_err()),
            "invalid_arguments"
        );
    }
    assert!(
        serde_json::from_value::<BugSearchParams>(json!({"query":"x","mode":"semantic"})).is_err()
    );
}

#[tokio::test]
async fn concurrent_scopes_and_separate_sessions_do_not_share_context() {
    let f = Fixture::new().await;
    let (bp, erp) = tokio::join!(
        f.search(json!({"query":"НДС","project_code":"bp3","limit":1})),
        f.search(json!({"query":"НДС","project_code":"erp","limit":1}))
    );
    assert_eq!(bp.unwrap()["bugs"][0]["project"]["project_code"], "bp3");
    assert_eq!(erp.unwrap()["bugs"][0]["project"]["project_code"], "erp");
    let other = Fixture::new().await;
    assert_eq!(
        error_code(
            other
                .search(json!({"query":"НДС","project_handle":"project-1"}))
                .await
                .unwrap_err()
        ),
        "invalid_reference"
    );
    let legacy = f.search(json!({"query":"legacy query"})).await.unwrap();
    assert_eq!(legacy["filter"]["mode"], "full_text");
    assert_eq!(legacy["filter"]["scope"], "all_visible_projects");
    assert_eq!(legacy["count"], 0);
    assert!(
        f.server
            .project_catalog_value(ProjectListParams::default())
            .await
            .is_ok()
    );
}

#[test]
fn number_grammar_and_scope_drift_fail_closed() {
    for n in ["60021238", "00-00843958", " 00-123 "] {
        assert!(is_bug_number(n));
    }
    for n in ["", "-123", "123-", "1--2", "１２３", "1 2", "abc-123"] {
        assert!(!is_bug_number(n));
    }
    let p = CatalogProject {
        reference: "bp-ref".into(),
        code: "bp3".into(),
        title: "BP".into(),
        abbreviation: None,
        updated_at: None,
    };
    let mut row = SearchBugRow {
        reference: "b".into(),
        number: "123".into(),
        title: "t".into(),
        project: p.clone(),
        description: "d".into(),
        url: "u".into(),
    };
    assert!(validate_search_rows(&[row.clone()], Some(&p), Some("123")).is_ok());
    assert!(validate_search_rows(&[row.clone()], Some(&p), Some("124")).is_err());
    row.project.code = "erp".into();
    assert!(validate_search_rows(&[row], Some(&p), None).is_err());
}

fn json_response(value: Value) -> impl axum::response::IntoResponse {
    ([("content-type", "application/json")], value.to_string())
}
