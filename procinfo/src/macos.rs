#![cfg(target_os = "macos")]
use super::*;
use std::ffi::{OsStr, OsString};
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::sync::Mutex;
use std::time::Instant;

impl From<u32> for LocalProcessStatus {
    fn from(s: u32) -> Self {
        match s {
            1 => Self::Idle,
            2 => Self::Run,
            3 => Self::Sleep,
            4 => Self::Stop,
            5 => Self::Zombie,
            _ => Self::Unknown,
        }
    }
}

impl LocalProcessInfo {
    pub fn current_working_dir(pid: u32) -> Option<PathBuf> {
        let mut pathinfo: libc::proc_vnodepathinfo = unsafe { std::mem::zeroed() };
        let size = std::mem::size_of_val(&pathinfo) as libc::c_int;
        let ret = unsafe {
            libc::proc_pidinfo(
                pid as _,
                libc::PROC_PIDVNODEPATHINFO,
                0,
                &mut pathinfo as *mut _ as *mut _,
                size,
            )
        };
        if ret != size {
            return None;
        }

        // Workaround a workaround for an old rustc version supported by libc;
        // the type of vip_path should just be [c_char; MAXPATHLEN] but it
        // is defined as a horrible nested array by the libc crate:
        // `[[c_char; 32]; 32]`.
        // Urgh.  Let's re-cast it as the correct kind of slice.
        let vip_path = unsafe {
            std::slice::from_raw_parts(
                pathinfo.pvi_cdir.vip_path.as_ptr() as *const u8,
                libc::MAXPATHLEN as usize,
            )
        };
        let nul = vip_path.iter().position(|&c| c == 0)?;
        Some(OsStr::from_bytes(&vip_path[0..nul]).into())
    }

    pub fn executable_path(pid: u32) -> Option<PathBuf> {
        let mut buffer: Vec<u8> = Vec::with_capacity(libc::PROC_PIDPATHINFO_MAXSIZE as _);
        let x = unsafe {
            libc::proc_pidpath(
                pid as _,
                buffer.as_mut_ptr() as *mut _,
                libc::PROC_PIDPATHINFO_MAXSIZE as _,
            )
        };
        if x <= 0 {
            return None;
        }

        unsafe { buffer.set_len(x as usize) };
        Some(OsString::from_vec(buffer).into())
    }

    pub fn with_root_pid(pid: u32) -> Option<Self> {
        ProcTable::capture().tree_for(pid)
    }

    /// Resolve the tree rooted at `pid`, reusing a previously captured
    /// snapshot of the system process table if it is no more than `max_age`
    /// old.  See [`ProcTable`] for why that matters.
    ///
    /// Pass `Duration::ZERO` to force a fresh capture.
    pub(crate) fn with_root_pid_snapshot(pid: u32, max_age: Duration) -> Option<Self> {
        let mut table = PROC_TABLE.lock().unwrap();

        let expired = table
            .as_ref()
            .is_none_or(|table| table.captured.elapsed() > max_age);
        if expired {
            table.replace(ProcTable::capture());
        }

        table.as_ref().and_then(|table| table.tree_for(pid))
    }
}

/// A snapshot of every process on the system, plus a pid -> children index
/// so that resolving a tree out of it is linear in the size of that tree
/// rather than in the size of the whole table.
///
/// Capturing one costs a `proc_pidinfo` syscall per process on the system,
/// and each of those takes the kernel's global process list lock.  Callers
/// that need many trees at once — eg: the sidebar, which wants the
/// foreground process of every pane — would otherwise pay that cost once
/// per pane and contend with every fork/exec happening on the machine.
struct ProcTable {
    procs: Vec<libc::proc_bsdinfo>,
    /// Indices into `procs` of the children of a given pid
    children_of: HashMap<u32, Vec<usize>>,
    captured: Instant,
}

/// The most recently captured table, shared by all `with_root_pid_snapshot`
/// callers.  Holds at most one table; the TTL is supplied per call so that
/// different callers can demand different freshness.
static PROC_TABLE: Mutex<Option<ProcTable>> = Mutex::new(None);

impl ProcTable {
    fn capture() -> Self {
        let procs: Vec<_> = all_pids().into_iter().filter_map(info_for_pid).collect();

        let mut children_of: HashMap<u32, Vec<usize>> = HashMap::new();
        for (idx, proc) in procs.iter().enumerate() {
            // pid 0 is its own parent; keep it from becoming its own child
            // and sending the tree walk into infinite recursion.
            if proc.pbi_pid != proc.pbi_ppid {
                children_of.entry(proc.pbi_ppid).or_default().push(idx);
            }
        }

        Self {
            procs,
            children_of,
            captured: Instant::now(),
        }
    }

    fn tree_for(&self, pid: u32) -> Option<LocalProcessInfo> {
        let root = self.procs.iter().position(|info| info.pbi_pid == pid)?;
        Some(self.build_proc(root))
    }

    fn build_proc(&self, idx: usize) -> LocalProcessInfo {
        let info = &self.procs[idx];

        let mut children = HashMap::new();
        if let Some(kids) = self.children_of.get(&info.pbi_pid) {
            for &kid in kids {
                children.insert(self.procs[kid].pbi_pid, self.build_proc(kid));
            }
        }

        let (executable, argv) = exe_and_args_for_pid_sysctl(info.pbi_pid as _)
            .unwrap_or_else(|| (exe_for_pid(info.pbi_pid as _), vec![]));

        let name = unsafe { std::ffi::CStr::from_ptr(info.pbi_comm.as_ptr() as _) };
        let name = name.to_str().unwrap_or("").to_string();

        LocalProcessInfo {
            pid: info.pbi_pid,
            ppid: info.pbi_ppid,
            name,
            executable,
            cwd: cwd_for_pid(info.pbi_pid as _),
            argv,
            start_time: info.pbi_start_tvsec,
            status: LocalProcessStatus::from(info.pbi_status),
            children,
        }
    }
}

/// Enumerate all current process identifiers
fn all_pids() -> Vec<libc::pid_t> {
    let num_pids = unsafe { libc::proc_listallpids(std::ptr::null_mut(), 0) };
    if num_pids < 1 {
        return vec![];
    }

    // Give a bit of padding to avoid looping if processes are spawning
    // rapidly while we're trying to collect this info
    const PADDING: usize = 32;
    let mut pids: Vec<libc::pid_t> = Vec::with_capacity(num_pids as usize + PADDING);
    loop {
        let n = unsafe {
            libc::proc_listallpids(
                pids.as_mut_ptr() as *mut _,
                (pids.capacity() * std::mem::size_of::<libc::pid_t>()) as _,
            )
        };

        if n < 1 {
            return vec![];
        }

        let n = n as usize;

        if n > pids.capacity() {
            pids.reserve(n + PADDING);
            continue;
        }

        unsafe { pids.set_len(n) };
        return pids;
    }
}

/// Obtain info block for a pid.
/// Note that the process could have gone away since we first
/// observed the pid and the time we call this, so we must
/// be able to tolerate this failing.
fn info_for_pid(pid: libc::pid_t) -> Option<libc::proc_bsdinfo> {
    let mut info: libc::proc_bsdinfo = unsafe { std::mem::zeroed() };
    let wanted_size = std::mem::size_of::<libc::proc_bsdinfo>() as _;
    let res = unsafe {
        libc::proc_pidinfo(
            pid,
            libc::PROC_PIDTBSDINFO,
            0,
            &mut info as *mut _ as *mut _,
            wanted_size,
        )
    };

    if res == wanted_size {
        Some(info)
    } else {
        None
    }
}

fn cwd_for_pid(pid: libc::pid_t) -> PathBuf {
    LocalProcessInfo::current_working_dir(pid as _).unwrap_or_else(PathBuf::new)
}

fn exe_and_args_for_pid_sysctl(pid: libc::pid_t) -> Option<(PathBuf, Vec<String>)> {
    use libc::c_int;
    let mut size = 64 * 1024;
    let mut buf: Vec<u8> = Vec::with_capacity(size);
    let mut mib = [libc::CTL_KERN, libc::KERN_PROCARGS2, pid as c_int];

    let res = unsafe {
        libc::sysctl(
            mib.as_mut_ptr(),
            mib.len() as _,
            buf.as_mut_ptr() as *mut _,
            &mut size,
            std::ptr::null_mut(),
            0,
        )
    };
    if res == -1 {
        return None;
    }
    if size < (std::mem::size_of::<c_int>() * 2) {
        // Not big enough
        return None;
    }
    unsafe { buf.set_len(size) };

    parse_exe_and_argv_sysctl(buf)
}

fn exe_for_pid(pid: libc::pid_t) -> PathBuf {
    LocalProcessInfo::executable_path(pid as _).unwrap_or_else(PathBuf::new)
}

fn parse_exe_and_argv_sysctl(buf: Vec<u8>) -> Option<(PathBuf, Vec<String>)> {
    use libc::c_int;

    // The data in our buffer is laid out like this:
    // argc - c_int
    // exe_path - NUL terminated string
    // argv[0] - NUL terminated string
    // argv[1] - NUL terminated string
    // ...
    // argv[n] - NUL terminated string
    // envp[0] - NUL terminated string
    // ...

    let mut ptr = &buf[0..buf.len()];

    let argc: c_int = unsafe { std::ptr::read(ptr.as_ptr() as *const c_int) };
    ptr = &ptr[std::mem::size_of::<c_int>()..];

    fn consume_cstr(ptr: &mut &[u8]) -> Option<String> {
        // Parse to the end of a null terminated string
        let nul = ptr.iter().position(|&c| c == 0)?;
        let s = String::from_utf8_lossy(&ptr[0..nul]).to_owned().to_string();
        *ptr = ptr.get(nul + 1..)?;

        // Find the position of the first non null byte. `.position()`
        // will return None if we run off the end.
        if let Some(not_nul) = ptr.iter().position(|&c| c != 0) {
            // If there are no trailing nulls, not_nul will be 0
            // and this call will be a noop
            *ptr = ptr.get(not_nul..)?;
        }

        Some(s)
    }

    let exe_path = consume_cstr(&mut ptr)?.into();

    let mut args = vec![];
    for _ in 0..argc {
        args.push(consume_cstr(&mut ptr)?);
    }

    Some((exe_path, args))
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::parse_exe_and_argv_sysctl;
    use super::LocalProcessInfo;

    #[test]
    fn resolves_own_pid() {
        let me = std::process::id();
        let info = LocalProcessInfo::with_root_pid(me).expect("should resolve own pid");
        assert_eq!(info.pid, me);
        assert_ne!(info.ppid, 0, "test process should have a real parent");
        assert!(!info.name.is_empty());
    }

    #[test]
    fn resolves_children_of_own_pid() {
        let mut kids: Vec<_> = (0..2)
            .map(|_| {
                std::process::Command::new("/bin/sleep")
                    .arg("30")
                    .spawn()
                    .expect("spawn sleep")
            })
            .collect();

        let me = std::process::id();
        let info = LocalProcessInfo::with_root_pid(me).expect("should resolve own pid");

        for kid in &kids {
            let found = info
                .children
                .get(&kid.id())
                .unwrap_or_else(|| panic!("child {} missing from tree", kid.id()));
            assert_eq!(found.ppid, me);
            assert_eq!(found.name, "sleep");
        }

        for kid in &mut kids {
            let _ = kid.kill();
            let _ = kid.wait();
        }
    }

    #[test]
    fn unknown_pid_resolves_to_none() {
        // pid_t is 32-bit but macOS caps pids well below this, so this pid
        // cannot exist.
        assert!(LocalProcessInfo::with_root_pid(u32::MAX - 1).is_none());
    }

    #[test]
    fn cached_read_agrees_with_fresh_read() {
        let me = std::process::id();
        let fresh = LocalProcessInfo::with_root_pid(me).expect("fresh");
        // Duration::ZERO forces a capture, so this must agree exactly rather
        // than merely being a plausible tree.
        let cached =
            LocalProcessInfo::with_root_pid_cached(me, std::time::Duration::ZERO).expect("cached");

        assert_eq!(fresh.pid, cached.pid);
        assert_eq!(fresh.ppid, cached.ppid);
        assert_eq!(fresh.name, cached.name);
        assert_eq!(fresh.executable, cached.executable);
        assert_eq!(fresh.argv, cached.argv);
    }

    #[test]
    fn test_trailing_zeros() {
        // Example data generated from running 'sleep 5' on the commit author's local machine,
        let buf = vec![
            2, 0, 0, 0, 47, 98, 105, 110, 47, 115, 108, 101, 101, 112, 0, 0, 0, 0, 0, 0, 115, 108,
            101, 101, 112, 0, 53, 0,
        ];

        let (exe_path, argv) = parse_exe_and_argv_sysctl(buf).unwrap();

        assert_eq!(exe_path, Path::new("/bin/sleep").to_path_buf());
        assert_eq!(argv, vec!["sleep".to_string(), "5".to_string()]);
    }

    #[test]
    fn test_no_trailing_zeros() {
        // Example data generated from running 'sleep 5' on the commit author's local machine,
        // then modified to remove the trailing 0s between the exe_path and the argv
        let buf = vec![
            2, 0, 0, 0, 47, 98, 105, 110, 47, 115, 108, 101, 101, 112, 0, 115, 108, 101, 101, 112,
            0, 53, 0,
        ];

        let (exe_path, argv) = parse_exe_and_argv_sysctl(buf).unwrap();

        assert_eq!(exe_path, Path::new("/bin/sleep").to_path_buf());
        assert_eq!(argv, vec!["sleep".to_string(), "5".to_string()]);
    }

    #[test]
    fn test_multiple_trailing_zeros() {
        // Example data generated from running 'sleep 5' on the commit author's local machine,
        // then modified to add trailing 0s between argv items
        let buf = vec![
            2, 0, 0, 0, 47, 98, 105, 110, 47, 115, 108, 101, 101, 112, 0, 0, 0, 115, 108, 101, 101,
            112, 0, 0, 0, 53, 0,
        ];

        let (exe_path, argv) = parse_exe_and_argv_sysctl(buf).unwrap();

        assert_eq!(exe_path, Path::new("/bin/sleep").to_path_buf());
        assert_eq!(argv, vec!["sleep".to_string(), "5".to_string()]);
    }

    #[test]
    fn test_trailing_zeros_at_end() {
        // Example data generated from running 'sleep 5' on the commit author's local machine,
        // then modified to add zeroes to the end of the buffer
        let buf = vec![
            2, 0, 0, 0, 47, 98, 105, 110, 47, 115, 108, 101, 101, 112, 0, 0, 0, 115, 108, 101, 101,
            112, 0, 0, 0, 53, 0, 0, 0, 0, 0,
        ];

        let (exe_path, argv) = parse_exe_and_argv_sysctl(buf).unwrap();

        assert_eq!(exe_path, Path::new("/bin/sleep").to_path_buf());
        assert_eq!(argv, vec!["sleep".to_string(), "5".to_string()]);
    }

    #[test]
    fn test_malformed() {
        // Example data generated from running 'sleep 5' on the commit author's local machine,
        // then modified to remove the last 0, making a malformed null-terminated string
        let buf = vec![
            2, 0, 0, 0, 47, 98, 105, 110, 47, 115, 108, 101, 101, 112, 0, 0, 0, 115, 108, 101, 101,
            112, 0, 0, 0, 53,
        ];

        assert!(parse_exe_and_argv_sysctl(buf).is_none());
    }
}
