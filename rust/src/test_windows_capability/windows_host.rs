//! Owned process containment and compile-only SDK check; no tracing/privilege APIs.
use std::ffi::c_void;
use std::fs;
use std::io::Read;
use std::os::windows::io::AsRawHandle;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};
#[repr(C)]
#[derive(Default)]
struct Basic {
    process_time: i64,
    job_time: i64,
    flags: u32,
    min: usize,
    max: usize,
    active: u32,
    affinity: usize,
    priority: u32,
    scheduling: u32,
}
#[repr(C)]
#[derive(Default)]
struct Extended {
    basic: Basic,
    io: [u64; 6],
    process_memory: usize,
    job_memory: usize,
    peak_process: usize,
    peak_job: usize,
}
#[link(name = "kernel32")]
extern "system" {
    fn CreateJobObjectW(attributes: *const c_void, name: *const u16) -> *mut c_void;
    fn SetInformationJobObject(job: *mut c_void, class: i32, info: *const c_void, len: u32) -> i32;
    fn AssignProcessToJobObject(job: *mut c_void, process: *mut c_void) -> i32;
    fn QueryInformationJobObject(
        job: *mut c_void,
        class: i32,
        info: *mut c_void,
        len: u32,
        returned: *mut u32,
    ) -> i32;
    fn TerminateJobObject(job: *mut c_void, code: u32) -> i32;
    fn CloseHandle(handle: *mut c_void) -> i32;
    fn GetProcessTimes(
        process: *mut c_void,
        birth: *mut [u32; 2],
        exit: *mut [u32; 2],
        kernel: *mut [u32; 2],
        user: *mut [u32; 2],
    ) -> i32;
}
fn os_code() -> u32 {
    std::io::Error::last_os_error().raw_os_error().unwrap_or(0) as u32
}
pub(super) struct Job(*mut c_void);
impl Job {
    pub(super) fn attach(child: &Child) -> Result<(Self, u64), u32> {
        let h = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
        if h.is_null() {
            return Err(os_code());
        }
        let job = Self(h);
        let mut info = Extended::default();
        info.basic.flags = 0x2000; // JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE; unnamed, not inheritable.
        if unsafe {
            SetInformationJobObject(
                h,
                9,
                (&info as *const Extended).cast(),
                std::mem::size_of::<Extended>() as u32,
            )
        } == 0
        {
            return Err(os_code());
        }
        if unsafe { AssignProcessToJobObject(h, child.as_raw_handle()) } == 0 {
            return Err(os_code());
        }
        let (mut b, mut e, mut k, mut u) = ([0; 2], [0; 2], [0; 2], [0; 2]);
        if unsafe { GetProcessTimes(child.as_raw_handle(), &mut b, &mut e, &mut k, &mut u) } == 0 {
            return Err(os_code());
        }
        Ok((job, u64::from(b[0]) | (u64::from(b[1]) << 32)))
    }
    pub(super) fn empty(&self) -> Result<bool, u32> {
        let mut accounting = [0u64; 6];
        if unsafe {
            QueryInformationJobObject(
                self.0,
                1,
                accounting.as_mut_ptr().cast(),
                48,
                std::ptr::null_mut(),
            )
        } == 0
        {
            return Err(os_code());
        }
        Ok((accounting[5] & 0xffff_ffff) == 0)
    }
    pub(super) fn terminate(&self) -> Result<(), u32> {
        if unsafe { TerminateJobObject(self.0, 78) } == 0 {
            Err(os_code())
        } else {
            Ok(())
        }
    }
}
impl Drop for Job {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}

type Failure = (&'static str, Option<u32>);
fn fail(stage: &'static str, e: std::io::Error) -> Failure {
    (stage, e.raw_os_error().map(|v| v as u32))
}
fn bounded_command(
    mut cmd: Command,
    root: &Path,
    start: Instant,
    stage: &'static str,
    stdout: bool,
) -> Result<(), Failure> {
    if start.elapsed() >= Duration::from_secs(10) {
        return Err(("time", None));
    }
    cmd.stdin(Stdio::null()).stderr(Stdio::null());
    if stdout {
        let out = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(root.join("tool-result"))
            .map_err(|e| fail(stage, e))?;
        cmd.stdout(Stdio::from(out));
    } else {
        cmd.stdout(Stdio::null());
    }
    let mut child = cmd.spawn().map_err(|e| fail(stage, e))?;
    loop {
        match child.try_wait() {
            Ok(Some(code)) => {
                return if code.success() {
                    Ok(())
                } else {
                    Err((stage, code.code().map(|c| c as u32)))
                }
            }
            Ok(None) => {}
            Err(e) => {
                let _ = child.kill();
                return Err(fail(stage, e));
            }
        }
        if start.elapsed() >= Duration::from_secs(10) {
            let _ = child.kill();
            // Parent job owns this descendant; parent confirms the entire tree's exit.
            return Err(("sdk-command-time", None));
        }
        if stdout && fs::metadata(root.join("tool-result")).is_ok_and(|m| m.len() > 4096) {
            let _ = child.kill();
            return Err(("sdk-discovery", None));
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}
fn version_dirs(root: &Path) -> Result<Vec<PathBuf>, Failure> {
    let mut dirs = Vec::new();
    for row in fs::read_dir(root)
        .map_err(|e| fail("sdk-discovery", e))?
        .take(65)
    {
        let row = row.map_err(|e| fail("sdk-discovery", e))?;
        let name = row.file_name();
        let Some(name) = name.to_str() else { continue };
        if name.starts_with("10.")
            && name
                .split('.')
                .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
        {
            dirs.push(row.path());
        }
    }
    if dirs.len() > 64 {
        return Err(("sdk-discovery", None));
    }
    dirs.sort_by(|a, b| b.file_name().cmp(&a.file_name()));
    Ok(dirs)
}
pub(super) fn compile_sdk(root: &Path, start: Instant) -> Result<(), Failure> {
    let pf = std::env::var_os("ProgramFiles(x86)")
        .map(PathBuf::from)
        .ok_or(("sdk-discovery", None))?;
    let vswhere = pf.join("Microsoft Visual Studio/Installer/vswhere.exe");
    let mut find = Command::new(vswhere);
    find.args([
        "-utf8",
        "-latest",
        "-products",
        "*",
        "-requires",
        "Microsoft.VisualStudio.Component.VC.Tools.x86.x64",
        "-find",
        "VC/Tools/MSVC/**/bin/Hostx64/x64/cl.exe",
    ]);
    bounded_command(find, root, start, "sdk-discovery", true)?;
    let mut bytes = Vec::new();
    fs::File::open(root.join("tool-result"))
        .map_err(|e| fail("sdk-discovery", e))?
        .take(4097)
        .read_to_end(&mut bytes)
        .map_err(|e| fail("sdk-discovery", e))?;
    if bytes.len() > 4096 {
        return Err(("sdk-discovery", None));
    }
    let paths = std::str::from_utf8(&bytes)
        .map_err(|_| ("sdk-discovery", None))?
        .lines()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>();
    if paths.len() != 1 {
        return Err(("sdk-discovery", None));
    }
    let compiler = PathBuf::from(paths[0]);
    if !compiler.is_absolute()
        || compiler.file_name().is_none_or(|s| s != "cl.exe")
        || !compiler.is_file()
    {
        return Err(("sdk-compiler", None));
    }
    let vc = compiler
        .ancestors()
        .nth(4)
        .ok_or(("sdk-compiler", None))?
        .join("include");
    let includes = version_dirs(&pf.join("Windows Kits/10/Include"))?;
    let sdk = includes
        .into_iter()
        .find(|p| {
            p.join("um/windows.h").is_file()
                && p.join("um/tdh.h").is_file()
                && p.join("ucrt/stddef.h").is_file()
        })
        .ok_or(("sdk-discovery", None))?;
    fs::write(root.join("sdk.c"), include_str!("sdk.c")).map_err(|e| fail("sdk-layout", e))?;
    let mut compile = Command::new(compiler);
    compile.current_dir(root).args([
        "/nologo",
        "/TC",
        "/std:c11",
        "/W4",
        "/WX",
        "/c",
        "/Folayout.obj",
    ]);
    for path in [vc, sdk.join("um"), sdk.join("shared"), sdk.join("ucrt")] {
        compile.arg("/I").arg(path);
    }
    compile.arg("sdk.c");
    bounded_command(compile, root, start, "sdk-layout", false)?;
    if !root.join("layout.obj").is_file() {
        return Err(("sdk-layout", None));
    }
    Ok(())
}
#[test]
fn job_abi_layouts() {
    assert_eq!(std::mem::size_of::<Extended>(), 144);
    assert_eq!(std::mem::offset_of!(Basic, flags), 16);
}
