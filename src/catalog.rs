// Copyright 2026 krylovim. Apache-2.0 WITH Commons Clause 1.0; see LICENSE.
// Catalog metadata persists; session references exist only in this process.
use std::{
    collections::BTreeMap,
    fs::{self, OpenOptions},
    future::Future,
    io::Write,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

use e1c_element_rpc::bugboard::CatalogProject;
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::sync::Mutex;

use crate::errors::ToolFailure;

const FORMAT_VERSION: u32 = 1;
pub(crate) const TTL_SECONDS: u64 = 24 * 60 * 60;
pub(crate) const PAGE_SIZE: u32 = 51;
const MAX_DISK_BYTES: u64 = 8 * 1024 * 1024;
const MAX_PAGES: usize = 256;

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Product {
    code: String,
    title: String,
    abbreviation: Option<String>,
    updated_at: Option<String>,
    #[serde(skip)]
    reference: String,
}

impl From<CatalogProject> for Product {
    fn from(p: CatalogProject) -> Self {
        Self {
            code: p.code,
            title: p.title,
            abbreviation: p.abbreviation,
            updated_at: p.updated_at,
            reference: p.reference,
        }
    }
}
impl From<&Product> for CatalogProject {
    fn from(p: &Product) -> Self {
        Self {
            code: p.code.clone(),
            title: p.title.clone(),
            abbreviation: p.abbreviation.clone(),
            updated_at: p.updated_at.clone(),
            reference: p.reference.clone(),
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Page {
    updated_at_unix: u64,
    // Dynamic-list cursor/cap semantics are unverified: never assert a full catalog.
    complete: bool,
    products: Vec<Product>,
}
impl Page {
    fn has_references(&self) -> bool {
        self.products.iter().all(|p| !p.reference.is_empty())
    }
    fn fresh_at(&self, now: u64) -> bool {
        self.updated_at_unix <= now && now - self.updated_at_unix < TTL_SECONDS
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct DiskCatalog {
    format_version: u32,
    server: String,
    profile: String,
    namespace: Option<String>,
    pages: BTreeMap<String, Page>,
}

pub(crate) struct CatalogCache {
    server: String,
    profile: Option<String>,
    namespace: Option<String>,
    path: Option<PathBuf>,
    pages: Mutex<BTreeMap<String, Page>>,
}

pub(crate) struct CatalogSnapshot {
    pub(crate) rows: Vec<CatalogProject>,
    pub(crate) status: Value,
}

impl CatalogCache {
    pub(crate) fn from_env(server: &str, namespace: Option<String>) -> Self {
        // Only a generation captured together with protected credentials provides
        // account isolation. A legacy caller may change cookies under one profile.
        let profile = if namespace.is_some() {
            std::env::var("BUGBOARD_PROFILE")
                .ok()
                .or_else(|| Some("default".into()))
        } else {
            None
        }
        .filter(|p| valid_profile(p))
        .map(|p| p.to_ascii_lowercase());
        let root = std::env::var_os("BUGBOARD_CACHE_DIR")
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("LOCALAPPDATA")
                    .map(|p| PathBuf::from(p).join("bugboard-mcp/cache"))
            })
            .or_else(|| {
                std::env::var_os("XDG_CACHE_HOME").map(|p| PathBuf::from(p).join("bugboard-mcp"))
            })
            .or_else(|| {
                std::env::var_os("HOME").map(|p| PathBuf::from(p).join(".cache/bugboard-mcp"))
            });
        Self::new_namespaced(server, profile, root, namespace)
    }

    #[cfg(test)]
    pub(crate) fn new(server: &str, profile: Option<String>, root: Option<PathBuf>) -> Self {
        Self::new_namespaced(server, profile, root, None)
    }

    fn new_namespaced(
        server: &str,
        profile: Option<String>,
        root: Option<PathBuf>,
        namespace: Option<String>,
    ) -> Self {
        // Hex encoding is collision-free and uses only safe filename characters.
        let server_key = server
            .as_bytes()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        let path = root.zip(profile.as_ref()).map(|(root, profile)| {
            let directory = root.join(server_key).join(profile);
            namespace
                .as_ref()
                .map_or_else(|| directory.join("legacy"), |n| directory.join(n))
                .join("catalog-v1.json")
        });
        let cache = Self {
            server: server.into(),
            profile,
            namespace,
            path,
            pages: Mutex::new(BTreeMap::new()),
        };
        let pages = cache.read_disk().unwrap_or_default();
        Self {
            pages: Mutex::new(pages),
            ..cache
        }
    }

    /// Caches a specific server selector, never claims missing entries in a partial
    /// page are absent from the catalog. Unknown exact codes refresh on every use.
    pub(crate) async fn get<F, Fut>(
        &self,
        key: &str,
        force: bool,
        needs_references: bool,
        fetch: F,
    ) -> Result<CatalogSnapshot, ToolFailure>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<Vec<CatalogProject>, ToolFailure>>,
    {
        // Lock covers fetch: concurrent requests in one process share a refresh.
        let mut pages = self.pages.lock().await;
        let now = unix_now();
        let old = pages.get(key).cloned();
        if let Some(page) = &old {
            let known = !key.starts_with("code:") || !page.products.is_empty();
            if !force && known && page.fresh_at(now) && (!needs_references || page.has_references())
            {
                return Ok(self.snapshot(
                    page,
                    if page.has_references() {
                        "memory"
                    } else {
                        "disk"
                    },
                    false,
                    None,
                ));
            }
        }
        match fetch().await {
            Ok(rows) => {
                let page = Page {
                    updated_at_unix: unix_now(),
                    complete: false,
                    products: rows.into_iter().map(Product::from).collect(),
                };
                pages.insert(key.into(), page.clone());
                trim_pages(&mut pages);
                // Storage failure must not turn a successful remote query into an error.
                let persistence_error =
                    self.persist(key, &page).err().map(|_| "cache_write_failed");
                Ok(self.snapshot(&page, "server", false, persistence_error))
            }
            Err(error) => {
                // Metadata fallback is useful for product discovery, never to invent a
                // session handle after restart or let cached authorization bypass a check.
                if !needs_references && let Some(mut page) = old {
                    for product in &mut page.products {
                        product.reference.clear();
                    }
                    return Ok(self.snapshot(&page, "stale_cache", true, Some("refresh_failed")));
                }
                Err(error)
            }
        }
    }

    fn snapshot(
        &self,
        page: &Page,
        source: &str,
        stale: bool,
        warning: Option<&str>,
    ) -> CatalogSnapshot {
        let now = unix_now();
        CatalogSnapshot {
            rows: page.products.iter().map(CatalogProject::from).collect(),
            status: json!({"source":source,"stale":stale,"age_seconds":now.saturating_sub(page.updated_at_unix),
                "updated_at_unix":page.updated_at_unix,"ttl_seconds":TTL_SECONDS,
                "format_version":FORMAT_VERSION,"persistent":self.path.is_some(),
                "references_available":page.has_references(),"warning":warning,
                "complete":page.complete,"scope":"cached_selector_first_page"}),
        }
    }

    fn read_disk(&self) -> std::io::Result<BTreeMap<String, Page>> {
        let Some(path) = &self.path else {
            return Ok(BTreeMap::new());
        };
        if fs::metadata(path)?.len() > MAX_DISK_BYTES {
            return Err(invalid_data());
        }
        let catalog: DiskCatalog =
            serde_json::from_slice(&fs::read(path)?).map_err(|_| invalid_data())?;
        if catalog.format_version != FORMAT_VERSION
            || catalog.server != self.server
            || Some(&catalog.profile) != self.profile.as_ref()
            || catalog.namespace != self.namespace
            || catalog.pages.len() > MAX_PAGES
            || catalog.pages.values().any(|p| {
                p.complete
                    || p.products.len() > PAGE_SIZE as usize
                    || p.products
                        .iter()
                        .any(|p| p.code.is_empty() || p.title.is_empty())
            })
        {
            return Err(invalid_data());
        }
        Ok(catalog.pages)
    }

    fn persist(&self, key: &str, page: &Page) -> std::io::Result<()> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        let parent = path.parent().ok_or_else(invalid_data)?;
        fs::create_dir_all(parent)?;
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(parent.join("catalog.lock"))?;
        // Nonblocking OS lock: a crashed writer cannot leave a stale lock. A busy
        // writer only skips this best-effort persistence; readers see the old file.
        lock.try_lock_exclusive()?;
        let mut pages = self.read_disk().unwrap_or_default();
        if pages
            .get(key)
            .is_none_or(|p| p.updated_at_unix <= page.updated_at_unix)
        {
            pages.insert(key.into(), page.clone());
        }
        trim_pages(&mut pages);
        let disk = DiskCatalog {
            format_version: FORMAT_VERSION,
            server: self.server.clone(),
            profile: self.profile.clone().unwrap(),
            namespace: self.namespace.clone(),
            pages,
        };
        let bytes = serde_json::to_vec(&disk)?;
        if bytes.len() as u64 > MAX_DISK_BYTES {
            return Err(invalid_data());
        }
        let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
        temporary.write_all(&bytes)?;
        temporary.as_file().sync_all()?;
        temporary.persist(path).map_err(|e| e.error)?;
        Ok(())
    }
}

fn invalid_data() -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidData, "Invalid catalog cache")
}
fn valid_profile(profile: &str) -> bool {
    !profile.is_empty()
        && profile.len() <= 64
        && profile
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}
fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
fn trim_pages(pages: &mut BTreeMap<String, Page>) {
    while pages.len() > MAX_PAGES {
        let oldest = pages
            .iter()
            .min_by_key(|(_, p)| p.updated_at_unix)
            .map(|(k, _)| k.clone())
            .unwrap();
        pages.remove(&oldest);
    }
}

#[cfg(test)]
mod tests;
