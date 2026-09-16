// Copyright 2026 krylovim. Apache-2.0 WITH Commons Clause 1.0; see LICENSE.
use super::*;

fn body(request: HttpRequest) -> Value {
    serde_json::from_str(request.body().unwrap()).unwrap()
}
fn predicates(filter: &Value) -> &Vec<Value> {
    filter
        .pointer("/value/Items/value/items/0/value/Items/value/items")
        .unwrap()
        .as_array()
        .unwrap()
}

#[test]
fn content_query_is_project_and_title_or_description_with_typed_literal_values() {
    let query = "НДС % _ [x] ' \" \\ OR 1=1";
    let request = body(bug_content_search_request(query, Some("bp-ref"), false, 51).unwrap());
    assert_eq!(request["limit"], 51);
    // Regression for a live HTTP 500: sort keys must be selected list fields.
    let selected: Vec<_> = request["dynamicList"]["Fields"]["value"]["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["value"]["Alias"].as_str().unwrap())
        .collect();
    for sort in request["dynamicList"]["Sorting"]["value"]["items"]
        .as_array()
        .unwrap()
    {
        assert!(selected.contains(&sort["value"]["Field"].as_str().unwrap()));
    }
    let filter = &request["dynamicList"]["Filter"];
    assert_eq!(filter["value"]["GroupKind"], 0);
    let items = predicates(filter);
    assert_eq!(items.len(), 3);
    assert_eq!(items[1]["value"]["Field"], "Проект");
    assert_eq!(items[1]["value"]["ComparisonKind"], 1);
    assert_eq!(
        items[1]["value"]["Value"],
        json!({"type":PROJECT_REFERENCE_TYPE,"value":"bp-ref"})
    );
    assert_eq!(
        items[2]["value"]["Items"]["value"]["items"][0]["value"]["GroupKind"],
        1
    );
    let text = predicates(&items[2]);
    assert_eq!(text.len(), 2);
    for (item, field) in text.iter().zip(["Заголовок", "Описание"]) {
        assert_eq!(item["value"]["Field"], field);
        assert_eq!(item["value"]["ComparisonKind"], 0);
        assert_eq!(
            item["value"]["Value"],
            json!({"type":"Std::String","value":query})
        );
    }
}

#[test]
fn number_and_catalog_lookup_are_exact_and_allow_lookahead() {
    let r = body(bug_content_search_request("00-123", None, true, 2).unwrap());
    let filters = predicates(&r["dynamicList"]["Filter"]);
    assert_eq!(filters.len(), 2);
    assert_eq!(filters[1]["value"]["Field"], "Наименование");
    assert_eq!(filters[1]["value"]["ComparisonKind"], 1);
    assert_eq!(r["limit"], 2);
    let p = body(project_catalog_request(None, Some("bp3"), None, 2).unwrap());
    let filters = predicates(&p["dynamicList"]["Filter"]);
    assert_eq!(filters[1]["value"]["Field"], "Код");
    assert_eq!(filters[1]["value"]["ComparisonKind"], 1);
    let p = body(project_catalog_request(Some("Бухгалтерия"), None, None, 51).unwrap());
    let filters = predicates(&p["dynamicList"]["Filter"]);
    let text = predicates(&filters[1]);
    assert_eq!(
        text.iter()
            .map(|f| f["value"]["Field"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec!["Код", "Наименование", "Аббревиатура"]
    );
}

#[test]
fn catalog_and_search_decoders_fail_closed_on_missing_identity_or_wrong_reference_type() {
    let response = |rows: Value| {
        DynamicListResponse::from_slice(serde_json::to_vec(&json!({"rows":rows})).unwrap()).unwrap()
    };
    assert!(
        decode_catalog_projects(&response(json!([])))
            .unwrap()
            .is_empty()
    );
    assert!(
        decode_search_bug_rows(&response(json!([])))
            .unwrap()
            .is_empty()
    );
    for fields in [
        json!([]),
        json!([{"type":BUG_REFERENCE_TYPE,"value":"wrong"},"bp3","BP","BP",null]),
        json!([{"type":PROJECT_REFERENCE_TYPE,"value":"p"},null,"BP","BP",null]),
    ] {
        assert!(decode_catalog_projects(&response(json!([{"fieldValues":fields}]))).is_err());
    }
    assert!(
        decode_search_bug_rows(&response(
            json!([{"fieldValues":[null,"123","title",null,null,null,null,"desc","url"]}])
        ))
        .is_err()
    );
    assert!(project_catalog_request(Some(" "), None, None, 1).is_err());
    assert!(bug_content_search_request("x", Some(" "), false, 1).is_err());
    assert!(bug_content_search_request("x", None, false, 0).is_err());
}
