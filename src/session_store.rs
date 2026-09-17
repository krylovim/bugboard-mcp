// Added by krylovim, 2026. Protected, current-user session profiles (Apache-2.0 + Commons Clause).
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

const MAX_SESSION_BYTES: u64 = 64 * 1024;
const STORE_MARKER: &str = "bugboard-mcp protected sessions v1\n";

/// Errors intentionally contain no OS messages, response bodies, cookies or plaintext.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct StoreError(pub(crate) &'static str);

pub(crate) fn validate_profile(profile: &str) -> Result<(), StoreError> {
    if profile.is_empty()
        || profile.len() > 64
        || !profile
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Err(StoreError("invalid_profile"));
    }
    let lower = profile.to_ascii_lowercase();
    if ["con", "prn", "aux", "nul"].contains(&lower.as_str())
        || (lower.len() == 4
            && (lower.starts_with("com") || lower.starts_with("lpt"))
            && matches!(lower.as_bytes()[3], b'1'..=b'9'))
    {
        return Err(StoreError("invalid_profile"));
    }
    Ok(())
}

pub(crate) fn profile_from_env() -> Result<String, StoreError> {
    let profile = std::env::var("BUGBOARD_PROFILE").unwrap_or_else(|_| "default".into());
    validate_profile(&profile)?;
    Ok(profile.to_ascii_lowercase())
}

pub(crate) fn root_from_env() -> Result<PathBuf, StoreError> {
    platform::ensure_supported()?;
    let root = if let Some(root) = std::env::var_os("BUGBOARD_SESSION_ROOT") {
        PathBuf::from(root)
    } else {
        PathBuf::from(std::env::var_os("LOCALAPPDATA").ok_or(StoreError("session_root_missing"))?)
            .join("bugboard-mcp")
            .join("protected-sessions")
    };
    if !root.is_absolute() {
        return Err(StoreError("session_root_must_be_absolute"));
    }
    Ok(root)
}

#[derive(Serialize, Deserialize)]
struct Record {
    schema: u32,
    profile: String,
    cookie: String,
    generation: String,
}
impl Drop for Record {
    fn drop(&mut self) {
        unsafe {
            erase(self.cookie.as_mut_vec());
        }
    }
}

pub(crate) struct SessionStore {
    root: PathBuf,
    profile: String,
}

impl SessionStore {
    pub(crate) fn new(root: PathBuf, profile: String) -> Result<Self, StoreError> {
        validate_profile(&profile)?;
        if !root.is_absolute() {
            return Err(StoreError("session_root_must_be_absolute"));
        }
        Ok(Self {
            root,
            profile: profile.to_ascii_lowercase(),
        })
    }

    pub(crate) fn from_env() -> Result<Self, StoreError> {
        Self::new(root_from_env()?, profile_from_env()?)
    }
    fn path(&self) -> PathBuf {
        self.root.join(format!("{}.dpapi", self.profile))
    }

    fn prepare(&self) -> Result<(), StoreError> {
        platform::ensure_supported()?;
        if self.root.parent().is_none()
            || self
                .root
                .components()
                .any(|part| matches!(part, std::path::Component::ParentDir))
        {
            return Err(StoreError("session_root_not_dedicated"));
        }
        for ancestor in self.root.ancestors() {
            if ancestor.join(".git").exists() {
                return Err(StoreError("session_store_inside_repository"));
            }
            platform::reject_link(ancestor)?;
        }
        // Never rewrite the ACL of an unrelated nonempty directory.
        let marker = self.root.join(".bugboard-session-store");
        platform::reject_link(&marker)?;
        if self.root.exists() {
            if marker.exists() {
                if fs::metadata(&marker)
                    .map_err(|_| StoreError("session_store_io"))?
                    .len()
                    != STORE_MARKER.len() as u64
                    || fs::read_to_string(&marker).map_err(|_| StoreError("session_store_io"))?
                        != STORE_MARKER
                {
                    return Err(StoreError("session_root_not_dedicated"));
                }
            } else if fs::read_dir(&self.root)
                .map_err(|_| StoreError("session_store_io"))?
                .next()
                .is_some()
            {
                return Err(StoreError("session_root_not_dedicated"));
            }
        }
        fs::create_dir_all(&self.root).map_err(|_| StoreError("session_store_io"))?;
        platform::restrict_acl(&self.root)?;
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&marker)
        {
            Ok(mut file) => file
                .write_all(STORE_MARKER.as_bytes())
                .map_err(|_| StoreError("session_store_io"))?,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(_) => return Err(StoreError("session_store_io")),
        }
        Ok(())
    }

    pub(crate) fn load(&self) -> Result<String, StoreError> {
        self.load_with_namespace().map(|(cookie, _)| cookie)
    }

    pub(crate) fn load_with_namespace(&self) -> Result<(String, String), StoreError> {
        let mut record = self.load_record()?;
        Ok((
            std::mem::take(&mut record.cookie),
            record.generation.clone(),
        ))
    }

    fn load_record(&self) -> Result<Record, StoreError> {
        self.prepare()?;
        let path = self.path();
        platform::reject_link(&path)?;
        let metadata =
            fs::metadata(&path).map_err(|_| StoreError("session_missing_or_unreadable"))?;
        if !metadata.is_file() || metadata.len() > MAX_SESSION_BYTES {
            return Err(StoreError("session_corrupt"));
        }
        platform::restrict_acl(&path)?;
        let encrypted = fs::read(path).map_err(|_| StoreError("session_store_io"))?;
        let mut plaintext = platform::unprotect(&encrypted)?;
        let decoded = serde_json::from_slice::<Record>(&plaintext);
        erase(&mut plaintext);
        let record = decoded.map_err(|_| StoreError("session_corrupt"))?;
        if record.schema != 1
            || record.profile != self.profile
            || record.cookie.trim().is_empty()
            || record.generation.len() != 32
            || !record.generation.bytes().all(|b| b.is_ascii_hexdigit())
        {
            return Err(StoreError("session_profile_mismatch_or_corrupt"));
        }
        Ok(record)
    }

    fn lock(&self) -> Result<FileGuard, StoreError> {
        let path = self.root.join(format!("{}.lock", self.profile));
        platform::reject_link(&path)?;
        let file = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)
            .map_err(|_| StoreError("session_store_io"))?;
        fs2::FileExt::try_lock_exclusive(&file).map_err(|_| StoreError("session_store_busy"))?;
        // Keep the inode: deleting an OS lock file can allow concurrent locks on different files.
        Ok(FileGuard {
            path,
            file: Some(file),
            remove: false,
        })
    }

    pub(crate) fn save(&self, cookie: &str) -> Result<(), StoreError> {
        self.prepare()?;
        if cookie.is_empty() || cookie.len() > 32 * 1024 || cookie.contains(['\r', '\n', '\0']) {
            return Err(StoreError("invalid_session_input"));
        }
        let _guard = self.lock()?;
        let mut plaintext = serde_json::to_vec(&Record {
            schema: 1,
            profile: self.profile.clone(),
            cookie: cookie.into(),
            generation: platform::generation()?,
        })
        .map_err(|_| StoreError("session_encode_failed"))?;
        let encrypted = platform::protect(&plaintext);
        erase(&mut plaintext);
        let encrypted = encrypted?;
        let temporary = self
            .root
            .join(format!("{}.{}.tmp", self.profile, std::process::id()));
        let file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|_| StoreError("session_store_io"))?;
        let mut guard = FileGuard {
            path: temporary.clone(),
            file: Some(file),
            remove: true,
        };
        guard
            .file
            .as_mut()
            .unwrap()
            .write_all(&encrypted)
            .map_err(|_| StoreError("session_store_io"))?;
        guard
            .file
            .as_mut()
            .unwrap()
            .sync_all()
            .map_err(|_| StoreError("session_store_io"))?;
        drop(guard.file.take());
        platform::restrict_acl(&temporary)?;
        platform::reject_link(&self.path())?;
        platform::replace(&temporary, &self.path())?;
        Ok(())
    }

    pub(crate) fn delete(&self) -> Result<(), StoreError> {
        self.prepare()?;
        let _guard = self.lock()?;
        platform::reject_link(&self.path())?;
        match fs::remove_file(self.path()) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(_) => Err(StoreError("session_store_io")),
        }
    }
}

fn erase(bytes: &mut [u8]) {
    for byte in bytes {
        unsafe {
            std::ptr::write_volatile(byte, 0);
        }
    }
}
struct FileGuard {
    path: PathBuf,
    file: Option<fs::File>,
    remove: bool,
}
impl Drop for FileGuard {
    fn drop(&mut self) {
        drop(self.file.take());
        if self.remove {
            let _ = fs::remove_file(&self.path);
        }
    }
}

#[cfg(windows)]
mod platform {
    use super::*;
    use std::{
        ffi::c_void,
        os::windows::{ffi::OsStrExt, fs::MetadataExt, process::CommandExt},
        ptr,
    };
    #[repr(C)]
    struct Blob {
        len: u32,
        data: *mut u8,
    }
    #[link(name = "crypt32")]
    unsafe extern "system" {
        fn CryptProtectData(
            input: *const Blob,
            description: *const u16,
            entropy: *const Blob,
            reserved: *mut c_void,
            prompt: *mut c_void,
            flags: u32,
            output: *mut Blob,
        ) -> i32;
        fn CryptUnprotectData(
            input: *const Blob,
            description: *mut *mut u16,
            entropy: *const Blob,
            reserved: *mut c_void,
            prompt: *mut c_void,
            flags: u32,
            output: *mut Blob,
        ) -> i32;
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn LocalFree(memory: *mut c_void) -> *mut c_void;
        fn MoveFileExW(from: *const u16, to: *const u16, flags: u32) -> i32;
    }
    #[link(name = "bcrypt")]
    unsafe extern "system" {
        fn BCryptGenRandom(algorithm: *mut c_void, buffer: *mut u8, len: u32, flags: u32) -> i32;
    }
    pub(super) fn generation() -> Result<String, StoreError> {
        let mut bytes = [0u8; 16];
        if unsafe { BCryptGenRandom(ptr::null_mut(), bytes.as_mut_ptr(), 16, 2) } != 0 {
            return Err(StoreError("session_random_failed"));
        }
        Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
    }
    pub(super) fn ensure_supported() -> Result<(), StoreError> {
        Ok(())
    }
    pub(super) fn reject_link(path: &Path) -> Result<(), StoreError> {
        match fs::symlink_metadata(path) {
            Ok(m) if m.file_attributes() & 0x400 != 0 => {
                Err(StoreError("session_store_reparse_point"))
            }
            Ok(_) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(_) => Err(StoreError("session_store_io")),
        }
    }
    pub(super) fn restrict_acl(path: &Path) -> Result<(), StoreError> {
        // Fixed script, path is data in the child environment. No secret is passed to the shell.
        let script = "$ErrorActionPreference='Stop'; $p=$env:BUGBOARD_ACL_TARGET; $sid=[Security.Principal.WindowsIdentity]::GetCurrent().User; $dir=(Get-Item -LiteralPath $p).PSIsContainer; $acl=if($dir){[Security.AccessControl.DirectorySecurity]::new()}else{[Security.AccessControl.FileSecurity]::new()}; $acl.SetAccessRuleProtection($true,$false); $acl.SetOwner($sid); $inherit=if($dir){[Security.AccessControl.InheritanceFlags]'ContainerInherit,ObjectInherit'}else{[Security.AccessControl.InheritanceFlags]::None}; foreach($id in @($sid,[Security.Principal.SecurityIdentifier]'S-1-5-18')) { $acl.AddAccessRule([Security.AccessControl.FileSystemAccessRule]::new($id,'FullControl',$inherit,'None','Allow')) }; if($dir){[IO.Directory]::SetAccessControl($p,$acl)}else{[IO.File]::SetAccessControl($p,$acl)}";
        let powershell =
            PathBuf::from(std::env::var_os("SystemRoot").ok_or(StoreError("session_acl_failed"))?)
                .join("System32/WindowsPowerShell/v1.0/powershell.exe");
        let result = std::process::Command::new(powershell)
            .args(["-NoProfile", "-NonInteractive", "-Command", script])
            .env("BUGBOARD_ACL_TARGET", path)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .creation_flags(0x08000000)
            .status()
            .map_err(|_| StoreError("session_acl_failed"))?;
        if result.success() {
            Ok(())
        } else {
            Err(StoreError("session_acl_failed"))
        }
    }
    fn transform(bytes: &[u8], encrypt: bool) -> Result<Vec<u8>, StoreError> {
        let input = Blob {
            len: bytes
                .len()
                .try_into()
                .map_err(|_| StoreError("session_too_large"))?,
            data: bytes.as_ptr().cast_mut(),
        };
        let mut output = Blob {
            len: 0,
            data: ptr::null_mut(),
        };
        // No CRYPTPROTECT_LOCAL_MACHINE: encryption is bound to this Windows user.
        let success = unsafe {
            if encrypt {
                CryptProtectData(
                    &input,
                    ptr::null(),
                    ptr::null(),
                    ptr::null_mut(),
                    ptr::null_mut(),
                    1,
                    &mut output,
                )
            } else {
                CryptUnprotectData(
                    &input,
                    ptr::null_mut(),
                    ptr::null(),
                    ptr::null_mut(),
                    ptr::null_mut(),
                    1,
                    &mut output,
                )
            }
        };
        if success == 0 {
            return Err(StoreError("session_protection_failed"));
        }
        let slice = unsafe { std::slice::from_raw_parts_mut(output.data, output.len as usize) };
        let result = slice.to_vec();
        erase(slice);
        unsafe {
            LocalFree(output.data.cast());
        }
        Ok(result)
    }
    pub(super) fn protect(bytes: &[u8]) -> Result<Vec<u8>, StoreError> {
        transform(bytes, true)
    }
    pub(super) fn unprotect(bytes: &[u8]) -> Result<Vec<u8>, StoreError> {
        transform(bytes, false)
    }
    pub(super) fn replace(from: &Path, to: &Path) -> Result<(), StoreError> {
        let from: Vec<u16> = from.as_os_str().encode_wide().chain(Some(0)).collect();
        let to: Vec<u16> = to.as_os_str().encode_wide().chain(Some(0)).collect();
        if unsafe { MoveFileExW(from.as_ptr(), to.as_ptr(), 1 | 8) } != 0 {
            Ok(())
        } else {
            Err(StoreError("session_store_io"))
        }
    }
}

#[cfg(not(windows))]
mod platform {
    use super::*;
    pub(super) fn ensure_supported() -> Result<(), StoreError> {
        Err(StoreError("protected_sessions_unsupported_os"))
    }
    pub(super) fn generation() -> Result<String, StoreError> {
        Err(StoreError("protected_sessions_unsupported_os"))
    }
    pub(super) fn reject_link(_: &Path) -> Result<(), StoreError> {
        ensure_supported()
    }
    pub(super) fn restrict_acl(_: &Path) -> Result<(), StoreError> {
        ensure_supported()
    }
    pub(super) fn protect(_: &[u8]) -> Result<Vec<u8>, StoreError> {
        Err(StoreError("protected_sessions_unsupported_os"))
    }
    pub(super) fn unprotect(_: &[u8]) -> Result<Vec<u8>, StoreError> {
        Err(StoreError("protected_sessions_unsupported_os"))
    }
    pub(super) fn replace(_: &Path, _: &Path) -> Result<(), StoreError> {
        ensure_supported()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn profiles_are_filename_safe() {
        for invalid in [
            "", "..", "../a", "a/b", "a\\b", "a.b", "a b", "é", "CON", "nul", "LPT1", "com9",
        ] {
            assert!(validate_profile(invalid).is_err());
        }
        assert!(validate_profile("Work_2-test").is_ok());
    }
    #[cfg(windows)]
    #[test]
    fn dpapi_corruption_and_profile_isolation() {
        let record = serde_json::to_vec(&Record {
            schema: 1,
            profile: "one".into(),
            cookie: "session=unit-test-not-real".into(),
            generation: platform::generation().unwrap(),
        })
        .unwrap();
        let mut encrypted = platform::protect(&record).unwrap();
        assert!(!encrypted.windows(9).any(|w| w == b"unit-test"));
        assert_eq!(platform::unprotect(&encrypted).unwrap(), record);
        let last = encrypted.len() - 1;
        encrypted[last] ^= 1;
        assert!(platform::unprotect(&encrypted).is_err());
    }
    #[cfg(windows)]
    #[test]
    fn store_roundtrip_swap_delete_and_acl() {
        let root =
            std::env::temp_dir().join(format!("bugboard-protected-unit-{}", std::process::id()));
        let first = SessionStore::new(root.clone(), "one".into()).unwrap();
        let second = SessionStore::new(root.clone(), "two".into()).unwrap();
        assert_eq!(
            SessionStore::new(root.clone(), "ONE".into())
                .unwrap()
                .path(),
            first.path()
        );
        first.save("session=unit-test-not-real").unwrap();
        assert_eq!(first.load().unwrap(), "session=unit-test-not-real");
        let old_generation = first.load_record().unwrap().generation.clone();
        fs::copy(first.path(), second.path()).unwrap();
        assert_eq!(
            second.load().unwrap_err(),
            StoreError("session_profile_mismatch_or_corrupt")
        );
        first.save("session=refreshed-unit-test").unwrap();
        assert_ne!(first.load_record().unwrap().generation, old_generation);
        assert_eq!(first.load().unwrap(), "session=refreshed-unit-test");
        first.delete().unwrap();
        second.delete().unwrap();
        assert!(first.load().is_err());
        fs::remove_file(root.join("one.lock")).unwrap();
        fs::remove_file(root.join("two.lock")).unwrap();
        fs::remove_file(root.join(".bugboard-session-store")).unwrap();
        fs::remove_dir(root).unwrap();
    }
    #[cfg(windows)]
    #[test]
    fn unrelated_directory_is_never_adopted() {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("unrelated.txt"), "untouched").unwrap();
        let store = SessionStore::new(root.path().to_owned(), "default".into()).unwrap();
        assert_eq!(
            store.save("s=synthetic").unwrap_err(),
            StoreError("session_root_not_dedicated")
        );
        assert_eq!(
            fs::read_to_string(root.path().join("unrelated.txt")).unwrap(),
            "untouched"
        );
    }
    #[cfg(not(windows))]
    #[test]
    fn unsupported_os_is_explicit() {
        assert_eq!(
            root_from_env().unwrap_err(),
            StoreError("protected_sessions_unsupported_os")
        );
    }
}
