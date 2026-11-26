//! Administrator-controlled runtime configuration, read without source writes.

use super::{ConfigCause, ConfigError, ConfigSource, MAX_CONFIG_BYTES, Reason};
use rustix::fs::{AtFlags, FileType, Mode, OFlags, Stat, fstat, open, openat, statat};
use std::{
    ffi::OsStr,
    fs::File,
    io::Read,
    os::fd::AsRawFd,
    path::{Component, Path},
};

fn error(reason: Reason) -> ConfigError {
    ConfigError::new(reason, "", ConfigSource::File)
}

fn io_error(cause: rustix::io::Errno) -> ConfigError {
    error(match cause {
        rustix::io::Errno::NOENT => Reason::MissingFile,
        rustix::io::Errno::LOOP | rustix::io::Errno::NOTDIR => Reason::UnsafeFile,
        _ => Reason::ReadFailed,
    })
    .with_cause(ConfigCause::OpenFile(cause.into()))
}

fn inspect_error(cause: rustix::io::Errno) -> ConfigError {
    error(Reason::ReadFailed).with_cause(ConfigCause::InspectFile(cause.into()))
}

struct Directory {
    handle: File,
    before: Stat,
}

impl Directory {
    fn capture(
        handle: File,
        immediate: bool,
        development_owner: Option<u32>,
    ) -> Result<Self, ConfigError> {
        let before = fstat(&handle).map_err(inspect_error)?;
        let mode = before.st_mode & 0o7777;
        let development = development_owner == Some(before.st_uid);
        let root_owned = before.st_uid == 0;
        let sticky_root = root_owned && mode & 0o1000 != 0;
        if FileType::from_raw_mode(before.st_mode) != FileType::Directory
            || (!root_owned && !development)
            || (mode & 0o022 != 0 && !sticky_root)
        {
            return Err(error(Reason::UnsafeFile));
        }
        if immediate {
            if !(root_owned && matches!(mode, 0o700 | 0o750) || development && mode == 0o700) {
                return Err(error(Reason::UnsafeFile));
            }
            no_additional_acl(&handle, true)?;
        }
        Ok(Self { handle, before })
    }

    fn verify(&self, parent: Option<(&File, &OsStr)>) -> Result<(), ConfigError> {
        let now = fstat(&self.handle).map_err(inspect_error)?;
        let named = match parent {
            Some((directory, name)) => statat(directory, name, AtFlags::SYMLINK_NOFOLLOW),
            None => rustix::fs::stat("/"),
        }
        .map_err(inspect_error)?;
        if !same_identity(&self.before, &now) || !same_identity(&self.before, &named) {
            return Err(error(Reason::SourceChanged));
        }
        Ok(())
    }
}

fn same_identity(before: &Stat, after: &Stat) -> bool {
    before.st_dev == after.st_dev
        && before.st_ino == after.st_ino
        && before.st_uid == after.st_uid
        && before.st_gid == after.st_gid
        && before.st_mode == after.st_mode
}

fn same_file(before: &Stat, after: &Stat) -> bool {
    same_identity(before, after)
        && before.st_size == after.st_size
        && before.st_nlink == after.st_nlink
        && before.st_mtime == after.st_mtime
        && before.st_mtime_nsec == after.st_mtime_nsec
        && before.st_ctime == after.st_ctime
        && before.st_ctime_nsec == after.st_ctime_nsec
}

fn no_additional_acl(file: &File, directory: bool) -> Result<(), ConfigError> {
    // O_PATH keeps each physical directory without requiring directory listing
    // permission. /proc/self/fd selects that held inode, never a caller path.
    let physical = format!("/proc/self/fd/{}", file.as_raw_fd());
    for attribute in ["system.posix_acl_access", "system.posix_acl_default"] {
        if attribute.ends_with("default") && !directory {
            continue;
        }
        match rustix::fs::getxattr_query(&physical, attribute) {
            Err(rustix::io::Errno::NODATA | rustix::io::Errno::NOTSUP) => (),
            Ok(_) => return Err(error(Reason::UnsafeFile)),
            Err(cause) => {
                return Err(error(Reason::ReadFailed)
                    .with_cause(ConfigCause::ReadAccessControl(cause.into())));
            }
        }
    }
    Ok(())
}

/// Read administrator-controlled runtime configuration, at most 1 MiB.
///
/// The physical parent is root-owned mode 0700 or 0750; the regular, single-link
/// file is root-owned mode 0600 or 0640 and has the same group as its parent.
/// Ancestors are root-owned and cannot be written by other identities, except
/// root-owned sticky system directories. Links, extended access/default ACLs,
/// unsafe modes, and identity or content changes during inspection are rejected.
///
/// `allow_development_owner` additionally accepts a process-owned 0700 parent
/// and 0600 file. A consumer may set it only for an explicitly unbound debug
/// build. This function never creates files, locks, directories, or permissions.
///
/// # Errors
/// Rejects unsafe, oversized or changing sources, and reports failed reads.
pub fn read_managed_file(
    path: &Path,
    allow_development_owner: bool,
) -> Result<Vec<u8>, ConfigError> {
    if !path.is_absolute() || path.as_os_str().len() > 16 * 1024 {
        return Err(error(Reason::UnsafeFile));
    }
    let names = path
        .components()
        .skip(1) // The absolute root is opened separately.
        .map(|component| match component {
            Component::Normal(name) => Ok(name),
            Component::Prefix(_)
            | Component::RootDir
            | Component::CurDir
            | Component::ParentDir => Err(error(Reason::UnsafeFile)),
        })
        .collect::<Result<Vec<_>, _>>()?;
    let (&name, ancestors) = names
        .split_last()
        .filter(|_| names.len() < 256)
        .ok_or_else(|| error(Reason::UnsafeFile))?;
    let development_owner = allow_development_owner.then(|| rustix::process::geteuid().as_raw());
    let flags = OFlags::PATH | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC;
    let root = Directory::capture(
        File::from(open("/", flags, Mode::empty()).map_err(io_error)?),
        ancestors.is_empty(),
        development_owner,
    )?;
    let mut directories = Vec::with_capacity(ancestors.len());
    for (index, &part) in ancestors.iter().enumerate() {
        let parent: &Directory = directories.last().unwrap_or(&root);
        let directory =
            File::from(openat(&parent.handle, part, flags, Mode::empty()).map_err(io_error)?);
        directories.push(Directory::capture(
            directory,
            index + 1 == ancestors.len(),
            development_owner,
        )?);
    }
    let parent = directories.last().unwrap_or(&root);
    let bytes = read_file(parent, name, development_owner)?;
    no_additional_acl(&parent.handle, true)?;
    root.verify(None)?;
    let mut parent = &root;
    for (directory, &part) in directories.iter().zip(ancestors) {
        directory.verify(Some((&parent.handle, part)))?;
        parent = directory;
    }
    Ok(bytes)
}

fn read_file(
    parent: &Directory,
    name: &OsStr,
    development_owner: Option<u32>,
) -> Result<Vec<u8>, ConfigError> {
    let directory = &parent.handle;
    let mut file = File::from(
        openat(
            directory,
            name,
            OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
            Mode::empty(),
        )
        .map_err(io_error)?,
    );
    let before = fstat(&file).map_err(inspect_error)?;
    let mode = before.st_mode & 0o7777;
    let managed = parent.before.st_uid == 0 && before.st_uid == 0 && matches!(mode, 0o600 | 0o640);
    let development = development_owner == Some(parent.before.st_uid)
        && before.st_uid == parent.before.st_uid
        && mode == 0o600;
    if FileType::from_raw_mode(before.st_mode) != FileType::RegularFile
        || before.st_nlink != 1
        || before.st_gid != parent.before.st_gid
        || (!managed && !development)
    {
        return Err(error(Reason::UnsafeFile));
    }
    no_additional_acl(&file, false)?;
    let length = u64::try_from(before.st_size).map_err(|_| error(Reason::LimitExceeded))?;
    if length > MAX_CONFIG_BYTES as u64 {
        return Err(error(Reason::LimitExceeded));
    }
    let mut bytes = Vec::new();
    (&mut file)
        .take(MAX_CONFIG_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|cause| error(Reason::ReadFailed).with_cause(ConfigCause::ReadFile(cause)))?;
    if bytes.len() > MAX_CONFIG_BYTES {
        return Err(error(Reason::LimitExceeded));
    }
    let after = fstat(&file).map_err(inspect_error)?;
    let named = statat(directory, name, AtFlags::SYMLINK_NOFOLLOW).map_err(inspect_error)?;
    if !same_file(&before, &after) || !same_file(&before, &named) || bytes.len() as u64 != length {
        return Err(error(Reason::SourceChanged));
    }
    no_additional_acl(&file, false)?;
    Ok(bytes)
}
