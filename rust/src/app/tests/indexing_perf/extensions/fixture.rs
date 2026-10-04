//! Owned, deterministic inputs. Expected membership is defined before indexing.
use super::*;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Shape {
    FlatMixed,
    FlatFiles,
    NestedEarly,
    NestedLate,
    Deep,
    Wide,
    InternalLinks,
}

#[derive(Clone, Debug)]
pub(super) struct Record {
    pub(super) path: PathBuf,
    pub(super) is_dir: bool,
    pub(super) modified: SystemTime,
    pub(super) size_bytes: u64,
    /// Potential ignore marker, independent of the case-sensitive filter setting.
    pub(super) ignored: bool,
}

pub(super) struct ExtendedFixture {
    pub(super) root: PathBuf,
    pub(super) records: Vec<Record>,
    pub(super) expected: Vec<Record>,
    pub(super) manifest: Vec<u8>,
    pub(super) shape: Shape,
    owns_root: bool,
    publication_backup: std::cell::RefCell<Option<PathBuf>>,
}
pub(super) struct EmptyRootPublication<'a> {
    fixture: &'a ExtendedFixture,
    holder: PathBuf,
    backup: PathBuf,
    moved: bool,
    published: Option<PublicationNodeIdentity>,
    empty_list: Option<PublicationNodeIdentity>,
    finished: bool,
}
impl<'a> EmptyRootPublication<'a> {
    /// Untimed preparation only. On Windows no worker/file handle may still
    /// reference this root during either rename; the driver owns quiescence.
    pub(super) fn new(fixture: &'a ExtendedFixture, source: Source) -> Self {
        Self::try_new_with_hook(fixture, source, || Ok(()))
            .expect("publish owned empty root for setup indexing")
    }
    /// The driver must stop all setup work before restoring, then dispatch the
    /// measured request without an intervening frame. No app is held here.
    pub(super) fn restore(mut self) -> std::io::Result<()> {
        let result = self.restore_inner();
        // A failed explicit restore reports its recovery path without another
        // automatic attempt on Drop. The fixture keeps the backup protected.
        self.finished = true;
        result.map_err(|error| self.recovery_error(error))
    }
    fn try_new_with_hook(
        fixture: &'a ExtendedFixture,
        source: Source,
        after_move: impl FnOnce() -> std::io::Result<()>,
    ) -> std::io::Result<Self> {
        if !fixture.owns_root || fixture.publication_backup.borrow().is_some() {
            return Err(std::io::Error::other(
                "empty publication requires an unretired owned fixture",
            ));
        }
        let root_type = fs::symlink_metadata(&fixture.root)?.file_type();
        if !root_type.is_dir() || root_type.is_symlink() {
            return Err(std::io::Error::other(
                "owned fixture root must be a physical directory",
            ));
        }
        let parent = fixture
            .root
            .parent()
            .ok_or_else(|| std::io::Error::other("owned fixture root has no parent"))?;
        let name = fixture
            .root
            .file_name()
            .ok_or_else(|| std::io::Error::other("owned fixture root has no name"))?
            .to_string_lossy();
        static NONCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        // Atomic create reserves a sibling container. Its destination contents
        // is absent, including on Windows where rename cannot replace a dir.
        let holder = loop {
            let nonce = NONCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let candidate = parent.join(format!(
                ".{name}-empty-backup-{}-{nonce}",
                std::process::id()
            ));
            match fs::create_dir(&candidate) {
                Ok(()) => break candidate,
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error),
            }
        };
        let backup = holder.join("contents");
        *fixture.publication_backup.borrow_mut() = Some(backup.clone());
        let mut guard = Self {
            fixture,
            holder,
            backup,
            moved: false,
            published: None,
            empty_list: None,
            finished: false,
        };
        fs::rename(&fixture.root, &guard.backup)?;
        guard.moved = true;
        after_move()?;
        fs::create_dir(&fixture.root)?;
        guard.published = Some(PublicationNodeIdentity::read(&fixture.root)?);
        if source == Source::FileList {
            let path = fixture.root.join("FileList.txt");
            // A fresh file leaves an actual empty FileList source, not a marker
            // entry that a Walker could mistake for input workload.
            let file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)?;
            drop(file);
            guard.empty_list = Some(PublicationNodeIdentity::read(&path)?);
        }
        Ok(guard)
    }
    fn restore_inner(&mut self) -> std::io::Result<()> {
        if self.moved {
            if let Some(identity) = &self.published {
                let root = &self.fixture.root;
                let root_type = fs::symlink_metadata(root)?.file_type();
                if PublicationNodeIdentity::read(root)? != *identity
                    || !root_type.is_dir()
                    || root_type.is_symlink()
                {
                    return Err(std::io::Error::other(
                        "publication root identity changed; refuse deletion",
                    ));
                }
                let contents = fs::read_dir(root)?
                    .map(|entry| entry.map(|entry| entry.path()))
                    .collect::<std::io::Result<Vec<_>>>()?;
                let list = root.join("FileList.txt");
                if self.empty_list.is_some() {
                    if contents.len() != 1 || contents[0] != list {
                        return Err(std::io::Error::other(
                            "unrelated publication contents; refuse deletion",
                        ));
                    }
                    let metadata = fs::symlink_metadata(&list)?;
                    if !metadata.file_type().is_file()
                        || metadata.len() != 0
                        || Some(PublicationNodeIdentity::read(&list)?) != self.empty_list
                    {
                        return Err(std::io::Error::other(
                            "empty FileList changed; refuse deletion",
                        ));
                    }
                    fs::remove_file(list)?;
                    self.empty_list = None;
                } else if !contents.is_empty() {
                    return Err(std::io::Error::other(
                        "unrelated publication contents; refuse deletion",
                    ));
                }
                // Never recursively remove a publication root. A concurrent
                // extra entry causes remove_dir to fail and keeps the backup.
                fs::remove_dir(root)?;
                self.published = None;
            } else if self.fixture.root.try_exists()? {
                return Err(std::io::Error::other(
                    "unowned replacement root; refuse deletion",
                ));
            }
            fs::rename(&self.backup, &self.fixture.root)?;
            self.moved = false;
        }
        fs::remove_dir(&self.holder)?;
        self.fixture.publication_backup.borrow_mut().take();
        self.finished = true;
        Ok(())
    }
    fn recovery_error(&self, error: std::io::Error) -> std::io::Error {
        std::io::Error::new(
            error.kind(),
            format!(
                "{error}; original contents retained at root {} or recovery backup {}",
                self.fixture.root.display(),
                self.backup.display()
            ),
        )
    }
}
impl Drop for EmptyRootPublication<'_> {
    fn drop(&mut self) {
        if !self.finished {
            if let Err(error) = self.restore_inner() {
                let error = self.recovery_error(error);
                self.finished = true;
                if std::thread::panicking() {
                    eprintln!("empty publication rollback: {error}");
                } else {
                    panic!("empty publication rollback: {error}");
                }
            }
        }
    }
}

#[derive(Eq, PartialEq)]
struct PublicationNodeIdentity {
    #[cfg(unix)]
    device: u64,
    #[cfg(unix)]
    inode: u64,
    #[cfg(windows)]
    volume: u32,
    #[cfg(windows)]
    file_index: u64,
    #[cfg(not(any(unix, windows)))]
    created: SystemTime,
}
impl PublicationNodeIdentity {
    fn read(path: &Path) -> std::io::Result<Self> {
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            let metadata = fs::symlink_metadata(path)?;
            Ok(Self {
                device: metadata.dev(),
                inode: metadata.ino(),
            })
        }
        #[cfg(windows)]
        {
            Self::read_windows(path)
        }
        #[cfg(not(any(unix, windows)))]
        {
            let metadata = fs::symlink_metadata(path)?;
            Ok(Self {
                created: metadata.created()?,
            })
        }
    }
    #[cfg(windows)]
    fn read_windows(path: &Path) -> std::io::Result<Self> {
        use std::ffi::c_void;
        use std::os::windows::fs::OpenOptionsExt;
        use std::os::windows::io::AsRawHandle;
        // Same native identity contract as filelist_writer and paged_preview;
        // local test code also works in old tags without the paged module.
        #[repr(C)]
        struct FileTime {
            low: u32,
            high: u32,
        }
        #[repr(C)]
        struct FileInformation {
            attributes: u32,
            creation: FileTime,
            access: FileTime,
            write: FileTime,
            volume: u32,
            size_high: u32,
            size_low: u32,
            links: u32,
            index_high: u32,
            index_low: u32,
        }
        #[link(name = "kernel32")]
        extern "system" {
            fn GetFileInformationByHandle(handle: *mut c_void, info: *mut FileInformation) -> i32;
        }
        let file = fs::OpenOptions::new()
            .read(true)
            .custom_flags(0x02000000)
            .open(path)?;
        let mut info = std::mem::MaybeUninit::<FileInformation>::uninit();
        // The file owns a live handle; the successful call fully initializes
        // the fixed Win32 layout before it is read. File drops before rename.
        let success =
            unsafe { GetFileInformationByHandle(file.as_raw_handle(), info.as_mut_ptr()) };
        if success == 0 {
            return Err(std::io::Error::last_os_error());
        }
        let info = unsafe { info.assume_init() };
        Ok(Self {
            volume: info.volume,
            file_index: (u64::from(info.index_high) << 32) | u64::from(info.index_low),
        })
    }
}
pub(super) struct GenerationChange<'a> {
    original: &'a ExtendedFixture,
    view: ExtendedFixture,
    moved: Vec<(PathBuf, PathBuf)>,
    original_manifest: Option<(Vec<u8>, SystemTime)>,
}
impl GenerationChange<'_> {
    pub(super) fn fixture(&self) -> &ExtendedFixture {
        &self.view
    }
}
impl Drop for GenerationChange<'_> {
    fn drop(&mut self) {
        for (old, new) in self.moved.iter().rev() {
            fs::rename(new, old).expect("restore owned original generation");
        }
        let root_list = self.original.root.join("FileList.txt");
        if let Some((bytes, modified)) = &self.original_manifest {
            fs::write(&root_list, bytes).unwrap();
            set_modified(&root_list, *modified);
        } else if root_list.exists() {
            fs::remove_file(root_list).unwrap();
        }
    }
}
impl ExtendedFixture {
    pub(super) fn new(count: usize, shape: Shape) -> Self {
        assert!(count > 0);
        static ROOT_NONCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let lexical = create_fixture_root(std::iter::repeat_with(|| {
            let nonce = ROOT_NONCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            test_root(&format!(
                "indexing-perf-extended-{}-{nonce}",
                std::process::id()
            ))
        }));
        let root = fs::canonicalize(lexical).unwrap();
        let mut records = Vec::with_capacity(count);
        let expected = match shape {
            Shape::FlatMixed | Shape::FlatFiles => {
                for i in 0..count {
                    let dir = shape == Shape::FlatMixed && i % 5 == 0;
                    records.push(create_record(&root, root.join(item_name(i, dir)), i, dir));
                }
                records.clone()
            }
            Shape::Deep | Shape::Wide => {
                let directory_count = if shape == Shape::Deep { 12 } else { 64 }.min(count / 4);
                let mut directories: Vec<PathBuf> = Vec::new();
                for i in 0..directory_count {
                    let parent = if shape == Shape::Deep && i > 0 {
                        &directories[i - 1]
                    } else {
                        &root
                    };
                    let path = parent.join(item_name(i, true));
                    records.push(create_record(&root, path.clone(), i, true));
                    directories.push(path);
                }
                for i in directory_count..count {
                    let parent = if directories.is_empty() {
                        &root
                    } else {
                        &directories[(i - directory_count) % directories.len()]
                    };
                    records.push(create_record(
                        &root,
                        parent.join(item_name(i, false)),
                        i,
                        false,
                    ));
                }
                records.clone()
            }
            Shape::NestedEarly | Shape::NestedLate => {
                assert!(
                    count >= 2049,
                    "late hierarchy reference requires an actual first batch"
                );
                let child = root.join("item_child");
                let directory = create_record(&root, child.clone(), count + 1, true);
                let old_count = count / 4;
                let survivor_count = count - old_count - 2;
                for i in 0..survivor_count {
                    records.push(create_record(
                        &root,
                        root.join(item_name(i, false)),
                        i,
                        false,
                    ));
                }
                records.push(directory);
                for i in 0..old_count {
                    records.push(create_record(
                        &root,
                        child.join(format!("old_{}", item_name(count + i, false))),
                        count + i,
                        false,
                    ));
                }
                // The override removes the child directory, child reference and old
                // descendants. Declare exactly the same number of replacement rows.
                let replacements = (0..old_count + 2)
                    .map(|i| {
                        create_record(
                            &root,
                            child.join(format!("new_{}", item_name(count * 2 + i, false))),
                            count * 2 + i,
                            false,
                        )
                    })
                    .collect::<Vec<_>>();
                let child_manifest = manifest(&child, &replacements);
                fs::write(child.join("FileList.txt"), &child_manifest).unwrap();
                let child_modified = fixed_time(count * 4 + 100);
                set_modified(&child.join("FileList.txt"), child_modified);
                let reference = Record {
                    path: child.join("FileList.txt"),
                    is_dir: false,
                    modified: child_modified,
                    size_bytes: child_manifest.len() as u64,
                    ignored: false,
                };
                records.insert(
                    if shape == Shape::NestedEarly { 0 } else { 2048 },
                    reference,
                );
                let mut final_rows = records
                    .iter()
                    .filter(|r| !r.path.starts_with(&child))
                    .cloned()
                    .collect::<Vec<_>>();
                final_rows.extend(replacements);
                final_rows
            }
            Shape::InternalLinks => {
                assert!(count >= 3);
                let target = root.join("item_target");
                records.push(create_record(&root, target.clone(), 0, true));
                for i in 0..count - 2 {
                    records.push(create_record(
                        &root,
                        target.join(item_name(i, false)),
                        i + 1,
                        false,
                    ));
                }
                let alias = root.join("item_alias");
                create_directory_link(&target, &alias);
                records.push(Record {
                    path: alias.clone(),
                    ..records[0].clone()
                });
                let mut expanded = records.clone();
                expanded.extend(records[1..records.len() - 1].iter().map(|r| Record {
                    path: alias.join(r.path.strip_prefix(&target).unwrap()),
                    ..r.clone()
                }));
                expanded
            }
        };
        let mut fixture = Self {
            root,
            manifest: Vec::new(),
            records,
            expected,
            shape,
            owns_root: true,
            publication_backup: std::cell::RefCell::new(None),
        };
        // Creating children changes directory metadata. Freeze timestamps only
        // after the complete topology exists; directory lengths are OS-defined.
        for record in &mut fixture.records {
            if record.is_dir {
                set_modified(&record.path, record.modified);
                record.size_bytes = fs::metadata(&record.path).unwrap().len();
            }
        }
        let directory_metadata = fixture
            .records
            .iter()
            .filter(|r| r.is_dir)
            .map(|r| (r.path.clone(), (r.modified, r.size_bytes)))
            .collect::<HashMap<_, _>>();
        for record in &mut fixture.expected {
            if record.is_dir {
                if let Some((modified, size)) = directory_metadata.get(&record.path) {
                    record.modified = *modified;
                    record.size_bytes = *size;
                }
            }
        }
        fixture.manifest = manifest(&fixture.root, &fixture.records);
        fixture.prepare_source(Source::FileList);
        fixture
    }
    pub(super) fn prepare_source(&self, source: Source) {
        let path = self.root.join("FileList.txt");
        if source == Source::FileList {
            fs::write(&path, &self.manifest).unwrap();
            set_modified(&path, UNIX_EPOCH + Duration::from_secs(1_699_999_900));
        } else if path.exists() {
            fs::remove_file(path).unwrap();
        }
    }
    pub(super) fn signature(&self) -> String {
        let mut hash = 0xcbf29ce484222325u64;
        for rows in [&self.records, &self.expected] {
            for record in rows {
                let relative = record
                    .path
                    .strip_prefix(&self.root)
                    .unwrap()
                    .to_string_lossy();
                let description = format!(
                    "{}:{}:{}:{}:{}\n",
                    relative,
                    record.is_dir,
                    record
                        .modified
                        .duration_since(UNIX_EPOCH)
                        .unwrap()
                        .as_secs(),
                    record.size_bytes,
                    record.ignored
                );
                for byte in description.bytes() {
                    hash = (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3);
                }
            }
            hash = (hash ^ 0xff).wrapping_mul(0x100000001b3);
        }
        format!("{hash:016x}")
    }
    pub(super) fn paths(&self) -> Vec<PathBuf> {
        self.expected.iter().map(|r| r.path.clone()).collect()
    }
    pub(super) fn expected_for_follow_links(&self, follow: bool) -> Vec<Record> {
        if self.shape == Shape::InternalLinks && !follow {
            self.records.clone()
        } else {
            self.expected.clone()
        }
    }
    /// Call after setup workers settle; drop after measured workers quiesce.
    pub(super) fn next_generation(&self) -> GenerationChange<'_> {
        assert_eq!(
            self.shape,
            Shape::FlatFiles,
            "same-root generation is files-only"
        );
        let root_list = self.root.join("FileList.txt");
        let original_manifest = if root_list.exists() {
            Some((
                fs::read(&root_list).unwrap(),
                fs::metadata(&root_list).unwrap().modified().unwrap(),
            ))
        } else {
            None
        };
        let moved = self
            .records
            .iter()
            .map(|r| {
                let name = r.path.file_name().unwrap().to_str().unwrap();
                let next = r.path.with_file_name(format!("next_{name}"));
                assert!(!next.exists(), "owned generation target must be fresh");
                (r.path.clone(), next)
            })
            .collect::<Vec<_>>();
        let remap = moved.iter().cloned().collect::<HashMap<_, _>>();
        let remap_record = |r: &Record| Record {
            path: remap[&r.path].clone(),
            ..r.clone()
        };
        let records = self.records.iter().map(remap_record).collect::<Vec<_>>();
        let expected = self.expected.iter().map(remap_record).collect();
        let view = ExtendedFixture {
            root: self.root.clone(),
            manifest: manifest(&self.root, &records),
            records,
            expected,
            shape: self.shape,
            owns_root: false,
            publication_backup: std::cell::RefCell::new(None),
        };
        let mut change = GenerationChange {
            original: self,
            view,
            moved: Vec::with_capacity(moved.len()),
            original_manifest,
        };
        // Record each successful rename immediately so unwinding also restores
        // a partially transformed fixture.
        for (old, new) in moved {
            fs::rename(&old, &new).expect("owned same-root generation rename");
            change.moved.push((old, new));
        }
        if change.original_manifest.is_some() {
            change.view.prepare_source(Source::FileList);
        }
        change
    }
}

fn create_fixture_root(candidates: impl IntoIterator<Item = PathBuf>) -> PathBuf {
    // Creating exclusively is the ownership boundary: a colliding candidate
    // can belong to another fixture and must never be reused or cleaned up.
    for candidate in candidates {
        match fs::create_dir(&candidate) {
            Ok(()) => return candidate,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => panic!("create fixture root {}: {error}", candidate.display()),
        }
    }
    panic!("fixture root candidates exhausted")
}

fn item_name(i: usize, directory: bool) -> String {
    format!(
        "item_{i:08}_{}_{}_{}{}",
        if i.is_multiple_of(100) {
            "needle"
        } else {
            "bulk"
        },
        if i.is_multiple_of(10) {
            "日本語"
        } else {
            "ascii"
        },
        match i % 17 {
            0 => "SKIP",
            1 => "skip",
            _ => "keep",
        },
        if directory { "" } else { ".txt" }
    )
}
fn fixed_time(i: usize) -> SystemTime {
    UNIX_EPOCH + Duration::from_secs(1_700_000_000 + i as u64)
}
fn create_record(root: &Path, path: PathBuf, i: usize, is_dir: bool) -> Record {
    let size_bytes = if is_dir {
        fs::create_dir(&path).unwrap();
        0
    } else {
        let size = 64 + (i % 97);
        let mut content = format!("preview item {i:08}\n").into_bytes();
        content.resize(size, b'x');
        fs::write(&path, content).unwrap();
        size as u64
    };
    let modified = fixed_time(i);
    if !is_dir {
        set_modified(&path, modified);
    }
    let relative = path.strip_prefix(root).unwrap().to_string_lossy();
    let ignored = relative.contains("SKIP") || relative.contains("skip");
    Record {
        path,
        is_dir,
        modified,
        size_bytes,
        ignored,
    }
}
fn manifest(base: &Path, rows: &[Record]) -> Vec<u8> {
    let mut out = Vec::new();
    for row in rows {
        out.extend_from_slice(
            row.path
                .strip_prefix(base)
                .unwrap()
                .to_string_lossy()
                .as_bytes(),
        );
        out.push(b'\n');
    }
    out
}
fn set_modified(path: &Path, modified: SystemTime) {
    #[cfg(windows)]
    let file = {
        use std::os::windows::fs::OpenOptionsExt;
        fs::OpenOptions::new()
            .read(true)
            .custom_flags(0x02000000)
            .open(path)
            .unwrap()
    };
    #[cfg(not(windows))]
    let file = fs::File::open(path).unwrap();
    file.set_times(fs::FileTimes::new().set_modified(modified))
        .unwrap();
}
fn create_directory_link(target: &Path, alias: &Path) {
    #[cfg(unix)]
    std::os::unix::fs::symlink(target, alias).expect("owned internal directory link");
    #[cfg(windows)]
    {
        // Native Path arguments match the owned junction fixtures used by the
        // walker tests; forward slashes can be parsed as mklink switches.
        let output = std::process::Command::new("cmd.exe")
            .args(["/C", "mklink", "/J"])
            .arg(alias)
            .arg(target)
            .output()
            .expect("create owned internal directory junction");
        assert!(
            output.status.success(),
            "owned junction fixture: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    #[cfg(not(any(unix, windows)))]
    panic!(
        "internal directory links unsupported on this platform: {} -> {}",
        alias.display(),
        target.display()
    );
}
impl Drop for ExtendedFixture {
    fn drop(&mut self) {
        if self.owns_root && self.publication_backup.borrow().is_none() {
            let _ = fs::remove_dir_all(&self.root);
        }
    }
}

#[test]
fn tc_229_extended_fixture_nested_expected_replaces_old_subtree() {
    for shape in [Shape::NestedEarly, Shape::NestedLate] {
        let f = ExtendedFixture::new(4096, shape);
        assert_eq!(f.records.len(), 4096);
        assert_eq!(f.expected.len(), 4096);
        let root_lines = String::from_utf8(f.manifest.clone())
            .unwrap()
            .lines()
            .map(str::to_owned)
            .collect::<Vec<_>>();
        let reference = root_lines
            .iter()
            .position(|p| p.ends_with("/FileList.txt"))
            .unwrap();
        if shape == Shape::NestedEarly {
            assert_eq!(reference, 0);
        } else {
            assert!(reference > 1024);
        }
        assert!(f
            .records
            .iter()
            .any(|r| r.path.to_string_lossy().contains("old_item")));
        assert!(!f
            .expected
            .iter()
            .any(|r| r.path.to_string_lossy().contains("old_item")));
        assert!(f
            .expected
            .iter()
            .any(|r| r.path.to_string_lossy().contains("new_item")));
    }
}

#[test]
fn tc_229_extended_fixture_fixed_file_metadata_and_markers() {
    let f = ExtendedFixture::new(301, Shape::FlatFiles);
    assert_eq!(f.records.len(), 301);
    assert_eq!(
        f.records
            .iter()
            .filter(|r| r.path.to_string_lossy().contains("needle"))
            .count(),
        4
    );
    assert!(f
        .records
        .iter()
        .any(|r| r.path.to_string_lossy().contains("SKIP")));
    assert!(f
        .records
        .iter()
        .any(|r| r.path.to_string_lossy().contains("skip")));
    for r in &f.records {
        let m = fs::metadata(&r.path).unwrap();
        assert_eq!(m.len(), r.size_bytes);
        assert_eq!(m.modified().unwrap(), r.modified);
    }
}

#[cfg(unix)]
#[test]
fn tc_229_extended_fixture_link_alias_membership_is_explicit() {
    let f = ExtendedFixture::new(16, Shape::InternalLinks);
    assert_eq!(f.expected_for_follow_links(false).len(), 16);
    assert_eq!(f.expected_for_follow_links(true).len(), 30);
    assert_eq!(f.expected.len(), 30);
    let link = f
        .records
        .iter()
        .find(|r| {
            fs::symlink_metadata(&r.path)
                .unwrap()
                .file_type()
                .is_symlink()
        })
        .unwrap();
    assert!(fs::read_link(&link.path).unwrap().starts_with(&f.root));
}

#[test]
fn tc_229_extended_fixture_generation_restores_same_root_and_source() {
    for source in [Source::FileList, Source::Walker] {
        let f = ExtendedFixture::new(32, Shape::FlatFiles);
        f.prepare_source(source);
        let original_signature = f.signature();
        let change = f.next_generation();
        assert_eq!(change.fixture().root, f.root);
        assert_eq!(change.fixture().expected.len(), f.expected.len());
        assert_ne!(change.fixture().paths(), f.paths());
        assert!(change.fixture().expected.iter().all(|r| r.path.exists()));
        assert!(f.expected.iter().all(|r| !r.path.exists()));
        assert_eq!(
            f.root.join("FileList.txt").exists(),
            source == Source::FileList
        );
        drop(change);
        assert_eq!(f.signature(), original_signature);
        assert!(f.root.exists());
        assert!(f.expected.iter().all(|r| r.path.exists()));
        assert_eq!(
            f.root.join("FileList.txt").exists(),
            source == Source::FileList
        );
        if source == Source::FileList {
            assert_eq!(fs::read(f.root.join("FileList.txt")).unwrap(), f.manifest);
        }
    }
}

#[test]
fn tc_229_empty_publication_preserves_root_source_and_original_files() {
    for source in [Source::FileList, Source::Walker] {
        let fixture = ExtendedFixture::new(32, Shape::FlatFiles);
        fixture.prepare_source(source);
        let original_manifest = source
            .eq(&Source::FileList)
            .then(|| fs::read(fixture.root.join("FileList.txt")).unwrap());
        let publication = EmptyRootPublication::new(&fixture, source);
        assert!(fixture.root.is_dir());
        assert!(fixture.expected.iter().all(|r| !r.path.exists()));
        let paths = fs::read_dir(&fixture.root)
            .unwrap()
            .map(|e| e.unwrap().path())
            .collect::<Vec<_>>();
        if source == Source::FileList {
            assert_eq!(paths, vec![fixture.root.join("FileList.txt")]);
            assert_eq!(fs::read(&paths[0]).unwrap(), b"");
        } else {
            assert!(paths.is_empty());
        }
        publication.restore().unwrap();
        for record in &fixture.records {
            let metadata = fs::metadata(&record.path).unwrap();
            assert_eq!(metadata.len(), record.size_bytes);
            assert_eq!(metadata.modified().unwrap(), record.modified);
        }
        assert_eq!(fixture.publication_backup.borrow().as_ref(), None);
        assert_eq!(
            fixture.root.join("FileList.txt").exists(),
            source == Source::FileList
        );
        if let Some(bytes) = original_manifest {
            assert_eq!(fs::read(fixture.root.join("FileList.txt")).unwrap(), bytes);
        }
    }
}

#[test]
fn tc_229_empty_publication_unwind_and_partial_error_restore_owned_contents() {
    let fixture = ExtendedFixture::new(16, Shape::FlatFiles);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _publication = EmptyRootPublication::new(&fixture, Source::FileList);
        panic!("abort after empty publication");
    }));
    assert!(result.is_err());
    assert!(fixture.records.iter().all(|r| r.path.exists()));
    assert_eq!(
        fs::read(fixture.root.join("FileList.txt")).unwrap(),
        fixture.manifest
    );
    let result = EmptyRootPublication::try_new_with_hook(&fixture, Source::Walker, || {
        Err(std::io::Error::other("injected post-rename failure"))
    });
    assert!(result.is_err());
    assert!(fixture.records.iter().all(|r| r.path.exists()));
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = EmptyRootPublication::try_new_with_hook(&fixture, Source::Walker, || {
            panic!("abort after moving root")
        });
    }));
    assert!(result.is_err());
    assert!(fixture.records.iter().all(|r| r.path.exists()));
    assert!(fixture.publication_backup.borrow().is_none());
}

#[test]
fn tc_229_empty_publication_refuses_unrelated_contents_and_borrowed_root() {
    let fixture = ExtendedFixture::new(16, Shape::FlatFiles);
    fixture.prepare_source(Source::Walker);
    let publication = EmptyRootPublication::new(&fixture, Source::Walker);
    let backup = fixture
        .publication_backup
        .borrow()
        .as_ref()
        .unwrap()
        .clone();
    let unrelated = fixture.root.join("unrelated.txt");
    fs::write(&unrelated, b"preserve unrelated content").unwrap();
    assert!(publication.restore().is_err());
    assert_eq!(fs::read(&unrelated).unwrap(), b"preserve unrelated content");
    assert!(backup
        .join(fixture.records[0].path.file_name().unwrap())
        .exists());
    // Explicit recovery belongs to this test, which created the extra file.
    fs::remove_file(unrelated).unwrap();
    fs::remove_dir(&fixture.root).unwrap();
    fs::rename(&backup, &fixture.root).unwrap();
    fs::remove_dir(backup.parent().unwrap()).unwrap();
    fixture.publication_backup.borrow_mut().take();
    let generation = fixture.next_generation();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _publication = EmptyRootPublication::new(generation.fixture(), Source::Walker);
    }));
    assert!(
        result.is_err(),
        "a borrowed view cannot own directory retirement"
    );
    assert!(generation.fixture().records.iter().all(|r| r.path.exists()));
}

#[test]
fn tc_229_extended_fixture_root_never_claims_an_existing_candidate() {
    let existing = ExtendedFixture::new(4, Shape::FlatFiles);
    let fresh = existing.root.with_extension("exclusive-candidate");
    assert!(!fresh.exists());
    let selected = create_fixture_root([existing.root.clone(), fresh.clone()]);
    assert_eq!(
        selected, fresh,
        "existing fixture cannot acquire another owner"
    );
    assert_eq!(
        fs::read(existing.root.join("FileList.txt")).unwrap(),
        existing.manifest
    );
    assert!(existing.records.iter().all(|r| r.path.exists()));
    fs::remove_dir(selected).unwrap();
}
