//! macOS sysctl, Mach VM statistics and per-process task info.
//! Read-only.

use std::ffi::{CStr, c_int};
use std::io;
use std::mem;
use std::sync::OnceLock;

// Declared here: libc deprecates `mach_host_self` in favour of the mach2 crate
// and doesn't bind `vm_kernel_page_size`.
unsafe extern "C" {
    fn mach_host_self() -> libc::mach_port_t;
    /// The kernel's page size, which VM statistics count in. Unlike `vm_page_size`
    /// it stays 16 KiB for an x86_64 process running under Rosetta.
    static vm_kernel_page_size: libc::vm_size_t;
}

/// Bytes per page in kernel VM statistics.
pub fn page_size() -> u64 {
    // SAFETY: set by the kernel before main and never written again.
    unsafe { vm_kernel_page_size as u64 }
}

/// Integer sysctl, e.g. `kern.memorystatus_level`.
pub fn sysctl_i32(name: &CStr) -> io::Result<i32> {
    let mut value: i32 = 0;
    let mut len = mem::size_of::<i32>();
    // SAFETY: `value` is an i32 and `len` is its size; the kernel writes at most `len` bytes.
    let rc = unsafe {
        libc::sysctlbyname(
            name.as_ptr(),
            (&raw mut value).cast(),
            &mut len,
            std::ptr::null_mut(),
            0,
        )
    };
    if rc != 0 {
        return Err(io::Error::last_os_error());
    }
    if len != mem::size_of::<i32>() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "not an int sysctl",
        ));
    }
    Ok(value)
}

/// System-wide VM state. Page quantities are converted to bytes; `faults` counts
/// events. Totals only grow, so two equal answers mean the kernel served a cache.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VmStats {
    pub wired: u64,
    pub compressed: u64,
    /// Bytes of app memory held in compressed form.
    pub compressor_input: u64,
    pub pageins: u64,
    pub pageouts: u64,
    pub swapins: u64,
    pub swapouts: u64,
    pub faults: u64,
    /// Bytes of pages compressed and decompressed.
    pub compressions: u64,
    pub decompressions: u64,
}

/// Integers in the oldest `vm_statistics64` layout that has every field we read
/// (`HOST_VM_INFO64_REV1`); an older kernel answers with fewer.
const VM_INFO_FIELDS: usize =
    mem::offset_of!(libc::vm_statistics64, swapped_count) / mem::size_of::<libc::integer_t>();

/// One `host_statistics64(HOST_VM_INFO64)` call; the source of `vm_stat`.
///
/// For apps outside the OS, the kernel answers this call a few times per second
/// (2 to 10, shared by every such process) and repeats a cached answer otherwise.
/// Callers compare answers to spot the repeats.
pub fn vm_stats() -> io::Result<VmStats> {
    // mach_host_self adds a port reference per call, so take it once.
    static HOST: OnceLock<libc::mach_port_t> = OnceLock::new();
    // SAFETY: no arguments; returns this task's host port name.
    let host = *HOST.get_or_init(|| unsafe { mach_host_self() });

    // SAFETY: vm_statistics64 is plain integers, so all-zero is valid.
    let mut info: libc::vm_statistics64 = unsafe { mem::zeroed() };
    let mut count = libc::HOST_VM_INFO64_COUNT;
    // SAFETY: `info` holds HOST_VM_INFO64_COUNT integers, which `count` tells the kernel.
    let rc = unsafe {
        libc::host_statistics64(
            host,
            libc::HOST_VM_INFO64,
            (&raw mut info).cast(),
            &mut count,
        )
    };
    if rc != libc::KERN_SUCCESS {
        return Err(io::Error::other(format!("host_statistics64 returned {rc}")));
    }
    if (count as usize) < VM_INFO_FIELDS {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            format!("host_statistics64 returned {count} fields, need {VM_INFO_FIELDS}"),
        ));
    }

    let page = page_size();
    Ok(VmStats {
        wired: u64::from(info.wire_count) * page,
        compressed: u64::from(info.compressor_page_count) * page,
        compressor_input: info.total_uncompressed_pages_in_compressor * page,
        pageins: info.pageins * page,
        pageouts: info.pageouts * page,
        swapins: info.swapins * page,
        swapouts: info.swapouts * page,
        faults: info.faults,
        compressions: info.compressions * page,
        decompressions: info.decompressions * page,
    })
}

/// Every PID on the system, including ones this user can't inspect.
pub fn pids() -> io::Result<Vec<libc::pid_t>> {
    // SAFETY: a null buffer asks only for the current count.
    let n = unsafe { libc::proc_listallpids(std::ptr::null_mut(), 0) };
    if n <= 0 {
        return Err(io::Error::last_os_error());
    }
    // Headroom for processes started between calls; a full buffer may have been
    // cut short, so grow it and ask again.
    let mut capacity = n as usize + 64;
    for _ in 0..4 {
        let mut pids: Vec<libc::pid_t> = vec![0; capacity];
        let bytes = c_int::try_from(pids.len() * mem::size_of::<libc::pid_t>())
            .map_err(io::Error::other)?;
        // SAFETY: `pids` has room for `bytes` bytes; the call returns how many PIDs it wrote.
        let n = unsafe { libc::proc_listallpids(pids.as_mut_ptr().cast(), bytes) };
        if n <= 0 {
            return Err(io::Error::last_os_error());
        }
        if (n as usize) < capacity {
            pids.truncate(n as usize);
            return Ok(pids);
        }
        capacity *= 2;
    }
    Err(io::Error::other("process list kept growing"))
}

#[derive(Clone, Copy, Debug)]
pub struct TaskInfo {
    pub threads: u32,
    pub running: u32,
}

/// Thread counts for one process; `None` if it exited or belongs to another user.
pub fn task_info(pid: libc::pid_t) -> Option<TaskInfo> {
    // SAFETY: proc_taskinfo is plain integers, so all-zero is valid.
    let mut info: libc::proc_taskinfo = unsafe { mem::zeroed() };
    let size = mem::size_of::<libc::proc_taskinfo>() as c_int;
    // SAFETY: `info` is `size` bytes; the call returns the bytes written.
    let n =
        unsafe { libc::proc_pidinfo(pid, libc::PROC_PIDTASKINFO, 0, (&raw mut info).cast(), size) };
    (n == size).then(|| TaskInfo {
        threads: info.pti_threadnum.max(0) as u32,
        running: info.pti_numrunning.max(0) as u32,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_live_values() {
        assert!(vm_stats().is_ok_and(|v| v.wired > 0));
        let pids = pids().unwrap_or_default();
        assert!(pids.contains(&(std::process::id() as libc::pid_t)));
        let me = task_info(std::process::id() as libc::pid_t);
        assert!(me.is_some_and(|t| t.threads >= 1));
    }

    #[test]
    fn sysctl_reads_only_ints() {
        assert!(sysctl_i32(c"hw.ncpu").is_ok_and(|n| n > 0));
        // hw.memsize is 8 bytes, so it must be refused rather than truncated.
        assert!(sysctl_i32(c"hw.memsize").is_err());
        let missing = sysctl_i32(c"systrace.does.not.exist");
        assert_eq!(missing.map_err(|e| e.kind()), Err(io::ErrorKind::NotFound));
    }

    #[test]
    fn page_size_is_the_kernels() {
        let page = page_size();
        assert!(page >= 4096 && page.is_power_of_two(), "{page}");
        #[cfg(target_arch = "aarch64")]
        assert_eq!(page, 16384);
    }

    #[test]
    fn pids_are_unique() {
        let mut pids = pids().unwrap_or_default();
        let listed = pids.len();
        pids.sort_unstable();
        pids.dedup();
        assert_eq!(pids.len(), listed);
    }

    #[test]
    fn task_info_counts_this_process_threads() {
        let me = std::process::id() as libc::pid_t;
        let (tx, rx) = std::sync::mpsc::channel::<()>();
        let rx = std::sync::Arc::new(std::sync::Mutex::new(rx));
        let parked: Vec<_> = (0..8)
            .map(|_| {
                let rx = rx.clone();
                std::thread::spawn(move || {
                    let _ = rx.lock().map(|r| r.recv());
                })
            })
            .collect();
        let threads = task_info(me).map_or(0, |t| t.threads);
        drop(tx);
        for t in parked {
            let _ = t.join();
        }
        assert!(threads >= 9, "{threads}");
    }
}
