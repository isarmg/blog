//! Filesystem operations whose names encode their durability and link-safety guarantees.

#[cfg(test)]
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::{
    ffi::OsStr,
    fs::File,
    io,
    path::{Component, Path, PathBuf},
};
mod unix;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RelativePath(PathBuf);

/// Exactly one canonical directory entry, not a relative traversal path.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct EntryName(PathBuf);
impl EntryName {
    /// # Errors
    /// The name is not exactly one canonical directory entry.
    pub fn new(name: impl AsRef<OsStr>) -> Result<Self, Error> {
        let path = RelativePath::new(Path::new(name.as_ref()))?;
        if path.as_path().components().count() != 1 {
            return Err(Error::UnsafeRelativePath(path.0));
        }
        Ok(Self(path.0))
    }
    #[must_use]
    pub fn as_path(&self) -> &Path {
        &self.0
    }
    #[must_use]
    pub fn as_os_str(&self) -> &OsStr {
        self.0.as_os_str()
    }
    #[must_use]
    pub fn as_relative(&self) -> RelativePath {
        RelativePath(self.0.clone())
    }
}

#[derive(Clone, Debug)]
pub struct FileEntry {
    pub name: EntryName,
    pub bytes: u64,
}

impl RelativePath {
    /// # Errors
    /// The path is empty, absolute, or contains a noncanonical component.
    pub fn new(path: impl AsRef<Path>) -> Result<Self, Error> {
        let path = path.as_ref();
        if path.as_os_str().is_empty()
            || path.as_os_str().as_encoded_bytes().contains(&0)
            || path
                .components()
                .any(|part| !matches!(part, Component::Normal(_)))
        {
            return Err(Error::UnsafeRelativePath(path.to_path_buf()));
        }
        let canonical: PathBuf = path.components().collect();
        if canonical.as_os_str() != path.as_os_str() {
            return Err(Error::UnsafeRelativePath(path.to_path_buf()));
        }
        Ok(Self(path.to_path_buf()))
    }
    #[must_use]
    pub fn as_path(&self) -> &Path {
        &self.0
    }
}

#[derive(Debug)]
pub struct PrivateDirectory {
    path: PathBuf,
    directory: File,
    administration_owner: Option<(u32, u32)>,
}

impl PrivateDirectory {
    /// Open an existing service-owned private directory for authorized offline
    /// administration. Requires effective uid 0 or the actual owner and
    /// exact 0700 permissions. Newly created files keep that owner's uid/gid;
    /// existing files and directories are never repaired. Runtime callers must
    /// use [`Self::open_existing`].
    /// # Errors
    /// The directory cannot be opened or does not meet the administrative ownership and permission policy.
    pub fn open_for_administration(path: impl AsRef<Path>) -> Result<Self, Error> {
        unix::open_administrative_directory(path.as_ref())
    }
    /// Create or validate a direct child of this held private directory.
    /// # Errors
    /// The child cannot be created or does not meet the private-directory policy.
    pub fn create_child(&self, name: &EntryName) -> Result<Self, Error> {
        unix::create_private_child(self, name)
    }

    /// Open an existing private directory without creating or changing anything.
    /// # Errors
    /// The existing directory cannot be opened with the required identity and permissions.
    pub fn open_existing(path: impl AsRef<Path>) -> Result<Self, Error> {
        unix::open_private_directory(path.as_ref())
    }

    /// Enumerate only regular, single-linked children with hard entry/byte budgets.
    /// # Errors
    /// A child is unsafe, a budget is exceeded, or enumeration fails.
    pub fn files(&self, limits: InventoryLimits) -> Result<Vec<FileEntry>, Error> {
        unix::files(self, limits)
    }
    /// # Errors
    /// The file cannot be safely read within the specified byte limit.
    pub fn read_bounded(&self, name: &EntryName, max_bytes: usize) -> Result<Vec<u8>, Error> {
        unix::read_bounded(self, name, max_bytes)
    }
    /// # Errors
    /// The entry cannot be safely removed from the held directory.
    pub fn remove_file(&self, name: &EntryName) -> Result<(), Error> {
        unix::remove_file(self, name)
    }
    /// # Errors
    /// The private directory cannot be created or verified.
    pub fn create(path: impl AsRef<Path>) -> Result<Self, Error> {
        unix::create_private_directory(path.as_ref())
    }
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }
    /// Confirm that the named directory still refers to this held handle.
    /// # Errors
    /// The path's identity, type, ownership, or permissions no longer match the held directory.
    pub fn verify_identity(&self) -> Result<(), Error> {
        let named = std::fs::symlink_metadata(&self.path)?;
        let held = self.directory.metadata()?;
        if !named.is_dir() || named.file_type().is_symlink() {
            return Err(Error::UnsafeFileType(self.path.clone()));
        }
        if named.dev() != held.dev() || named.ino() != held.ino() {
            return Err(Error::UnsafeFileType(self.path.clone()));
        }
        let owner = self
            .administration_owner
            .map_or_else(|| rustix::process::geteuid().as_raw(), |(uid, _)| uid);
        if named.uid() != owner || named.mode() & 0o7777 != 0o700 {
            return Err(Error::UnsafePermissions(self.path.clone()));
        }
        Ok(())
    }
    #[must_use]
    pub fn resolve(&self, relative: &RelativePath) -> PathBuf {
        self.path.join(relative.as_path())
    }
    /// # Errors
    /// The operating system cannot synchronize the directory.
    pub fn sync(&self) -> Result<(), Error> {
        self.directory.sync_all()?;
        Ok(())
    }
}

pub struct AtomicFile;

impl AtomicFile {
    /// # Errors
    /// The destination already exists or writing, synchronizing, or publishing the new file fails.
    pub fn create(
        directory: &PrivateDirectory,
        destination: &EntryName,
        bytes: &[u8],
    ) -> Result<(), Error> {
        unix::atomic_create(directory, destination, bytes)
    }

    /// # Errors
    /// The destination is unsafe or writing, synchronizing, or publishing its replacement fails.
    pub fn replace(
        directory: &PrivateDirectory,
        destination: &RelativePath,
        bytes: &[u8],
    ) -> Result<(), Error> {
        unix::atomic_replace(directory, destination, bytes)
    }
}

pub struct NoClobberPublish;
impl NoClobberPublish {
    /// Publish a single-linked regular staging file within one held private directory.
    /// The staging file is synchronized before the no-clobber rename; the
    /// containing directory is synchronized before success is returned.
    /// # Errors
    /// The staging file or destination is unsafe, the destination exists, or durable publication fails.
    pub fn publish(
        directory: &PrivateDirectory,
        source: &RelativePath,
        destination: &RelativePath,
    ) -> Result<(), Error> {
        unix::no_clobber_publish(directory, source, destination)
    }
}

/// A process-scoped exclusive advisory lock. The lock file remains in place
/// after release so a second process cannot lock a replacement inode while the
/// original holder is still alive.
pub struct AdvisoryLock {
    _file: File,
}

impl AdvisoryLock {
    /// # Errors
    /// The lock file is unsafe or the exclusive lock cannot be acquired.
    pub fn acquire(directory: &PrivateDirectory, name: &RelativePath) -> Result<Self, Error> {
        unix::advisory_lock(directory, name)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InventoryLimits {
    pub max_entries: usize,
    pub max_total_bytes: u64,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DirectoryInventory {
    pub entries: usize,
    pub total_bytes: u64,
}

/// # Errors
/// Directory contents are unsafe, exceed the configured inventory limits, or cannot be inspected.
pub fn bounded_directory_inventory(
    path: &Path,
    limits: InventoryLimits,
) -> Result<DirectoryInventory, Error> {
    unix::bounded_inventory(path, limits)
}

/// # Errors
/// Inspecting or synchronizing the file or its parent directory fails.
pub fn sync_file_and_parent(path: &Path) -> Result<(), Error> {
    unix::sync_file_and_parent(path)
}
fn temporary_name() -> Result<String, Error> {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).map_err(Error::Randomness)?;
    let nonce: String = crate::hexadecimal(&bytes);
    Ok(format!(".state-atomic-{nonce}.tmp"))
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("secure randomness unavailable")]
    Randomness(#[source] getrandom::Error),
    #[error("an absolute path is required: {0:?}")]
    AbsolutePathRequired(PathBuf),
    #[error("unsafe relative path: {0:?}")]
    UnsafeRelativePath(PathBuf),
    #[error("path is not a regular unlinked object: {0:?}")]
    UnsafeFileType(PathBuf),
    #[error("path has multiple links: {0:?}")]
    MultipleLinks(PathBuf),
    #[error("path is not privately owned with exact permissions: {0:?}")]
    UnsafePermissions(PathBuf),
    #[error("directory entry identity changed: {0:?}")]
    IdentityChanged(PathBuf),
    #[error("publication happened but durability could not be confirmed")]
    PublishedDurabilityUnknown(#[source] io::Error),
    #[error("advisory lock is already held: {0:?}")]
    AlreadyLocked(PathBuf),
    #[error("destination already exists: {0:?}")]
    DestinationExists(PathBuf),
    #[error("directory inventory budget exceeded")]
    BudgetExceeded,
    #[error("file length is negative: {path:?}")]
    InvalidFileLength {
        path: PathBuf,
        #[source]
        source: std::num::TryFromIntError,
    },
    #[error("filesystem operation failed; temporary-file cleanup also failed: {cleanup}")]
    Cleanup {
        #[source]
        operation: Box<Error>,
        cleanup: io::Error,
    },
    #[error(transparent)]
    Io(#[from] io::Error),
}

#[path = "../../../../test/application/unit/fs_safety/tests.rs"]
#[cfg(test)]
mod tests;
