// Copyright 2026 krylovim. Apache-2.0 WITH Commons Clause 1.0; see LICENSE.
use super::*;

fn product(code: &str) -> CatalogProject {
    CatalogProject {
        reference: format!("internal-reference-{code}"),
        code: code.into(),
        title: format!("Product {code}"),
        abbreviation: Some(code.into()),
        updated_at: None,
    }
}
fn cache(root: &std::path::Path, profile: &str) -> CatalogCache {
    CatalogCache::new(
        "https://example.test",
        Some(profile.into()),
        Some(root.into()),
    )
}
fn outage() -> Result<Vec<CatalogProject>, ToolFailure> {
    Err(ToolFailure::transport("fixture outage"))
}

#[tokio::test]
async fn repeat_ttl_force_and_unknown_code_refresh() {
    let temp = tempfile::tempdir().unwrap();
    let c = cache(temp.path(), "account_a");
    c.get("code:bp3", false, true, || async {
        Ok(vec![product("bp3")])
    })
    .await
    .unwrap();
    let cached = c
        .get("code:bp3", false, true, || async {
            panic!("unexpected network")
        })
        .await
        .unwrap();
    assert_eq!(cached.status["source"], "memory");
    let stale = c
        .get("code:bp3", true, false, || async { outage() })
        .await
        .unwrap();
    assert!(stale.rows[0].reference.is_empty());
    assert_eq!(stale.status["references_available"], false);
    c.get("code:bp3", true, true, || async {
        Ok(vec![product("changed")])
    })
    .await
    .unwrap();
    assert_eq!(
        c.get("code:bp3", false, true, || async { panic!() })
            .await
            .unwrap()
            .rows[0]
            .code,
        "changed"
    );
    c.pages
        .lock()
        .await
        .get_mut("code:bp3")
        .unwrap()
        .updated_at_unix = unix_now() - TTL_SECONDS;
    assert_eq!(
        c.get("code:bp3", false, true, || async {
            Ok(vec![product("refreshed")])
        })
        .await
        .unwrap()
        .rows[0]
            .code,
        "refreshed"
    );
    c.get("code:new", false, true, || async { Ok(vec![]) })
        .await
        .unwrap();
    assert_eq!(
        c.get("code:new", false, true, || async {
            Ok(vec![product("new")])
        })
        .await
        .unwrap()
        .rows[0]
            .code,
        "new"
    );
}

#[tokio::test]
async fn disk_metadata_outage_age_and_failed_refresh_preserve_good_copy() {
    let temp = tempfile::tempdir().unwrap();
    let c = cache(temp.path(), "a");
    c.get("all", false, false, || async { Ok(vec![product("bp3")]) })
        .await
        .unwrap();
    let fresh_disk = cache(temp.path(), "a");
    let fresh = fresh_disk
        .get("all", false, false, || async {
            panic!("fresh metadata should survive restart")
        })
        .await
        .unwrap();
    assert_eq!(fresh.status["source"], "disk");
    assert_eq!(fresh.status["references_available"], false);
    let mut expired = c.pages.lock().await["all"].clone();
    expired.updated_at_unix = unix_now() - TTL_SECONDS - 10;
    // Install the expired fixture directly so persistence's newer-wins policy remains tested.
    let path = c.path.as_ref().unwrap();
    let disk = DiskCatalog {
        format_version: FORMAT_VERSION,
        server: c.server.clone(),
        profile: "a".into(),
        namespace: None,
        pages: BTreeMap::from([("all".into(), expired)]),
    };
    fs::write(path, serde_json::to_vec(&disk).unwrap()).unwrap();
    let bytes = fs::read(path).unwrap();
    let text = String::from_utf8(bytes.clone()).unwrap();
    assert!(!text.contains("internal-reference"));
    assert!(!text.contains("project_handle"));
    assert!(!text.contains("cookie"));
    let restarted = cache(temp.path(), "a");
    let stale = restarted
        .get("all", false, false, || async { outage() })
        .await
        .unwrap();
    assert_eq!(stale.status["stale"], true);
    assert_eq!(stale.status["references_available"], false);
    assert!(stale.status["age_seconds"].is_u64());
    assert_eq!(stale.status["complete"], false);
    assert!(stale.rows[0].reference.is_empty());
    assert_eq!(fs::read(path).unwrap(), bytes);
    assert!(
        restarted
            .get("all", true, true, || async { outage() })
            .await
            .is_err()
    );
    let fresh = restarted
        .get("all", false, false, || async { Ok(vec![product("bp3")]) })
        .await
        .unwrap();
    assert_eq!(fresh.status["references_available"], true);
    assert_eq!(fresh.status["stale"], false);
}

#[tokio::test]
async fn profiles_servers_corruption_versions_and_partial_pages_are_isolated() {
    let temp = tempfile::tempdir().unwrap();
    let a = cache(temp.path(), "a");
    a.get("all", false, false, || async { Ok(vec![product("bp3")]) })
        .await
        .unwrap();
    assert!(
        cache(temp.path(), "b")
            .get("all", false, false, || async { outage() })
            .await
            .is_err()
    );
    let other = CatalogCache::new(
        "https://other.test",
        Some("a".into()),
        Some(temp.path().into()),
    );
    assert!(
        other
            .get("all", false, false, || async { outage() })
            .await
            .is_err()
    );
    // A known title does not prove the exact code is unique or present beyond a partial page.
    assert!(
        a.get("code:bp3", false, true, || async { outage() })
            .await
            .is_err()
    );
    let path = a.path.as_ref().unwrap();
    fs::write(path, "not JSON").unwrap();
    assert!(
        cache(temp.path(), "a")
            .get("all", false, false, || async { outage() })
            .await
            .is_err()
    );
    let recovered = cache(temp.path(), "a");
    recovered
        .get("all", false, false, || async { Ok(vec![product("erp")]) })
        .await
        .unwrap();
    let mut disk: Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
    disk["format_version"] = json!(999);
    fs::write(path, disk.to_string()).unwrap();
    assert!(
        cache(temp.path(), "a")
            .get("all", false, false, || async { outage() })
            .await
            .is_err()
    );
    disk["format_version"] = json!(FORMAT_VERSION);
    disk["pages"]["all"]["complete"] = json!(true);
    fs::write(path, disk.to_string()).unwrap();
    assert!(
        cache(temp.path(), "a")
            .get("all", false, false, || async { outage() })
            .await
            .is_err()
    );
}

#[tokio::test]
async fn concurrent_same_process_fetches_once_and_writer_lock_preserves_file() {
    let temp = tempfile::tempdir().unwrap();
    let c = cache(temp.path(), "a");
    let calls = std::sync::atomic::AtomicU32::new(0);
    let fetch = || async {
        calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        tokio::task::yield_now().await;
        Ok(vec![product("bp3")])
    };
    let (a, b) = tokio::join!(
        c.get("all", false, false, fetch),
        c.get("all", false, false, fetch)
    );
    a.unwrap();
    b.unwrap();
    assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 1);
    let path = c.path.as_ref().unwrap();
    let old = fs::read(path).unwrap();
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .open(path.parent().unwrap().join("catalog.lock"))
        .unwrap();
    lock.lock_exclusive().unwrap();
    let result = c
        .get("all", true, false, || async { Ok(vec![product("erp")]) })
        .await
        .unwrap();
    assert_eq!(result.status["warning"], "cache_write_failed");
    assert_eq!(fs::read(path).unwrap(), old);
    FileExt::unlock(&lock).unwrap();
    c.get("all", true, false, || async { Ok(vec![product("erp")]) })
        .await
        .unwrap();
    assert_ne!(fs::read(path).unwrap(), old);
}

#[test]
fn independent_writers_publish_valid_atomic_snapshots() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().to_owned();
    let initial = cache(&root, "a");
    initial
        .persist(
            "all",
            &Page {
                updated_at_unix: unix_now(),
                complete: false,
                products: vec![],
            },
        )
        .unwrap();
    let jobs: Vec<_> = (0..8)
        .map(|i| {
            let root = root.clone();
            std::thread::spawn(move || {
                let c = cache(&root, "a");
                let page = Page {
                    updated_at_unix: unix_now(),
                    complete: false,
                    products: vec![product(&i.to_string()).into()],
                };
                for _ in 0..30 {
                    let _ = c.persist(&format!("code:{i}"), &page);
                    let loaded = c.read_disk();
                    if c.path.as_ref().unwrap().exists() {
                        assert!(loaded.is_ok());
                    }
                }
            })
        })
        .collect();
    for job in jobs {
        job.join().unwrap();
    }
    let c = cache(&root, "a");
    assert!(!c.read_disk().unwrap().is_empty());
}

#[test]
fn profile_validation_and_clock_rollback() {
    for bad in ["", "../account", "a/b", "a\\b", "счет"] {
        assert!(!valid_profile(bad));
    }
    assert!(valid_profile("account_A-1"));
    let page = Page {
        updated_at_unix: 100,
        complete: false,
        products: vec![],
    };
    assert!(!page.fresh_at(99));
    assert!(page.fresh_at(100));
    assert!(!page.fresh_at(100 + TTL_SECONDS));
}

#[tokio::test]
async fn protected_generation_isolates_account_replacement_and_old_process_writes() {
    let temp = tempfile::tempdir().unwrap();
    let make = |generation: &str| {
        CatalogCache::new_namespaced(
            "https://example.test",
            Some("same_profile".into()),
            Some(temp.path().into()),
            Some(generation.into()),
        )
    };
    let old_process = make("11111111111111111111111111111111");
    old_process
        .get("all", false, false, || async {
            Ok(vec![product("old-account")])
        })
        .await
        .unwrap();
    let new_process = make("22222222222222222222222222222222");
    assert!(
        new_process
            .get("all", false, false, || async { outage() })
            .await
            .is_err()
    );
    new_process
        .get("all", false, false, || async {
            Ok(vec![product("new-account")])
        })
        .await
        .unwrap();
    old_process
        .get("all", true, false, || async {
            Ok(vec![product("still-old")])
        })
        .await
        .unwrap();
    let restarted_new = make("22222222222222222222222222222222");
    assert_eq!(
        restarted_new
            .get("all", false, false, || async { panic!("fresh cache") })
            .await
            .unwrap()
            .rows[0]
            .code,
        "new-account"
    );
}
