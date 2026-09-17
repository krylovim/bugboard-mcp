// Added by krylovim, 2026. Local-only authorization helper (Apache-2.0 + Commons Clause).
use crate::{
    client::BugboardClient,
    config::{SessionConfig, parse_env_file},
    session_store::{SessionStore, StoreError, profile_from_env, root_from_env, validate_profile},
};
use serde_json::json;
use std::io::{self, IsTerminal, Read};
#[cfg(windows)]
use std::io::{BufRead, Write};

pub(crate) async fn run_if_requested() -> Option<i32> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) != Some("auth") {
        return None;
    }
    let result = run(&args[1..]).await;
    match result {
        Ok(value) => {
            println!("{value}");
            Some(0)
        }
        Err(error) => {
            eprintln!("{}", json!({"ok": false, "error": error.0}));
            Some(1)
        }
    }
}

struct Options {
    operation: String,
    profile: String,
    stdin: bool,
}
fn options(args: &[String]) -> Result<Options, StoreError> {
    let operation = args
        .first()
        .ok_or(StoreError("usage_auth_status_import_import-env_delete"))?
        .clone();
    if !["status", "import", "import-env", "delete"].contains(&operation.as_str()) {
        return Err(StoreError("unknown_auth_operation"));
    }
    let mut profile = profile_from_env()?;
    let mut stdin = false;
    let mut explicit_profile = false;
    let mut index = 1;
    while index < args.len() {
        match args[index].as_str() {
            "--stdin" if operation == "import" && !stdin => stdin = true,
            "--profile" if !explicit_profile => {
                index += 1;
                profile = args
                    .get(index)
                    .ok_or(StoreError("missing_profile"))?
                    .clone();
                explicit_profile = true;
            }
            _ => return Err(StoreError("invalid_auth_arguments")),
        }
        index += 1;
    }
    validate_profile(&profile)?;
    Ok(Options {
        operation,
        profile: profile.to_ascii_lowercase(),
        stdin,
    })
}

async fn run(args: &[String]) -> Result<serde_json::Value, StoreError> {
    let options = options(args)?;
    let store = SessionStore::new(root_from_env()?, options.profile.clone())?;
    match options.operation.as_str() {
        "delete" => {
            store.delete()?;
            Ok(
                json!({"ok": true, "profile": options.profile, "local_session_deleted": true, "remote_session_revoked": false}),
            )
        }
        "status" => {
            let cookie = store.load()?;
            verify_cookie(cookie).await?;
            Ok(
                json!({"ok": true, "profile": options.profile, "authenticated": true, "storage": "windows_current_user_dpapi"}),
            )
        }
        "import" | "import-env" => {
            let cookie = if options.operation == "import-env" {
                legacy_cookie()?
            } else {
                read_cookie(options.stdin)?
            };
            save_after_verification(&store, cookie, verify_cookie).await?;
            Ok(
                json!({"ok": true, "profile": options.profile, "authenticated": true, "saved": true, "storage": "windows_current_user_dpapi", "mcp_restart_required": true}),
            )
        }
        _ => unreachable!(),
    }
}

async fn save_after_verification<F, Fut>(
    store: &SessionStore,
    cookie: String,
    verify: F,
) -> Result<(), StoreError>
where
    F: FnOnce(String) -> Fut,
    Fut: std::future::Future<Output = Result<(), StoreError>>,
{
    struct Secret(String);
    impl Drop for Secret {
        fn drop(&mut self) {
            for byte in unsafe { self.0.as_mut_vec() } {
                unsafe {
                    std::ptr::write_volatile(byte, 0);
                }
            }
        }
    }
    let secret = Secret(cookie);
    verify(secret.0.clone()).await?;
    store.save(&secret.0)
}

/// A false status, unexpected reply or network failure must never replace stored credentials.
async fn verify_cookie(cookie: String) -> Result<(), StoreError> {
    let config = SessionConfig::from_cookie(Some(&cookie))
        .map_err(|_| StoreError("invalid_session_input"))?;
    let client = BugboardClient::new(config).map_err(|_| StoreError("invalid_session_input"))?;
    let request = e1c_element_rpc::bugboard::auth_status_request()
        .map_err(|_| StoreError("auth_request_failed"))?;
    let response = client
        .execute_unversioned("auth_import_verify", request)
        .await
        .map_err(|_| StoreError("auth_verification_failed"))?;
    if response
        .get("isAuthenticated")
        .and_then(serde_json::Value::as_bool)
        == Some(true)
    {
        Ok(())
    } else {
        Err(StoreError("not_authenticated"))
    }
}

fn legacy_cookie() -> Result<String, StoreError> {
    if let Ok(cookie) = std::env::var("BUGBOARD_COOKIE") {
        return checked_cookie(cookie);
    }
    let path = std::env::var_os("BUGBOARD_SESSION_ENV")
        .ok_or(StoreError("legacy_session_source_missing"))?;
    let metadata =
        std::fs::metadata(&path).map_err(|_| StoreError("legacy_session_read_failed"))?;
    if metadata.len() > 64 * 1024 {
        return Err(StoreError("legacy_session_too_large"));
    }
    let contents =
        std::fs::read_to_string(path).map_err(|_| StoreError("legacy_session_read_failed"))?;
    let mut values = parse_env_file(&contents);
    checked_cookie(
        values
            .remove("BUGBOARD_COOKIE")
            .ok_or(StoreError("legacy_session_cookie_missing"))?,
    )
}

fn checked_cookie(value: String) -> Result<String, StoreError> {
    let cookie = value.trim_end_matches(['\r', '\n']).to_owned();
    if cookie.trim().is_empty() {
        return Err(StoreError("session_input_cancelled_or_empty"));
    }
    if cookie.len() > 32 * 1024 || cookie.contains(['\r', '\n', '\0']) {
        return Err(StoreError("invalid_session_input"));
    }
    Ok(cookie)
}

fn read_cookie(from_stdin: bool) -> Result<String, StoreError> {
    if from_stdin {
        if io::stdin().is_terminal() {
            return Err(StoreError("stdin_requires_pipe_use_hidden_prompt"));
        }
        let mut value = String::new();
        io::stdin()
            .take(32 * 1024 + 3)
            .read_to_string(&mut value)
            .map_err(|_| StoreError("session_input_failed"))?;
        return checked_cookie(value);
    }
    hidden_cookie()
}

#[cfg(windows)]
fn hidden_cookie() -> Result<String, StoreError> {
    use std::ffi::c_void;
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetStdHandle(handle: u32) -> *mut c_void;
        fn GetConsoleMode(handle: *mut c_void, mode: *mut u32) -> i32;
        fn SetConsoleMode(handle: *mut c_void, mode: u32) -> i32;
    }
    struct Restore(*mut c_void, u32);
    impl Drop for Restore {
        fn drop(&mut self) {
            unsafe {
                SetConsoleMode(self.0, self.1);
            }
        }
    }
    let handle = unsafe { GetStdHandle((-10i32) as u32) };
    let mut mode = 0;
    if unsafe { GetConsoleMode(handle, &mut mode) } == 0 {
        return Err(StoreError("hidden_prompt_requires_console"));
    }
    // Disable echo and processed input: Ctrl+C becomes input, so the RAII guard restores echo.
    if unsafe { SetConsoleMode(handle, mode & !0x4 & !0x1) } == 0 {
        return Err(StoreError("hidden_prompt_unavailable"));
    }
    let _restore = Restore(handle, mode);
    eprint!("Cookie (hidden; empty input cancels): ");
    io::stderr()
        .flush()
        .map_err(|_| StoreError("session_input_failed"))?;
    let mut value = String::new();
    io::BufReader::new(io::stdin().take(32 * 1024 + 3))
        .read_line(&mut value)
        .map_err(|_| StoreError("session_input_failed"))?;
    eprintln!();
    if value.contains('\u{3}') {
        return Err(StoreError("session_input_cancelled_or_empty"));
    }
    checked_cookie(value)
}
#[cfg(not(windows))]
fn hidden_cookie() -> Result<String, StoreError> {
    Err(StoreError("protected_sessions_unsupported_os"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(windows)]
    #[tokio::test]
    async fn rejected_import_never_changes_existing_record() {
        let root = tempfile::tempdir().unwrap();
        let store = SessionStore::new(root.path().to_owned(), "default".into()).unwrap();
        store.save("s=synthetic-existing").unwrap();
        let before = store.load_with_namespace().unwrap();
        let failure = save_after_verification(&store, "s=synthetic-rejected".into(), |_| async {
            Err(StoreError("not_authenticated"))
        })
        .await;
        assert_eq!(failure.unwrap_err().0, "not_authenticated");
        assert_eq!(store.load_with_namespace().unwrap(), before);
    }
    #[test]
    fn secret_arguments_are_never_accepted_or_reflected() {
        let args = vec![
            "import".into(),
            "--cookie".into(),
            "session=SENSITIVE".into(),
        ];
        let error = options(&args).err().unwrap();
        assert_eq!(error.0, "invalid_auth_arguments");
        assert!(!format!("{error:?}").contains("SENSITIVE"));
    }
    #[test]
    fn cancelled_or_bad_input_is_rejected() {
        assert_eq!(
            checked_cookie("\r\n".into()).unwrap_err().0,
            "session_input_cancelled_or_empty"
        );
        assert!(checked_cookie("session=a\nInjected=b".into()).is_err());
        assert!(checked_cookie("s=".repeat(20000)).is_err());
        assert_eq!(
            checked_cookie("session=ok\r\n".into()).unwrap(),
            "session=ok"
        );
    }
}
