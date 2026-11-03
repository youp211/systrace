//! macOS sysctl, Mach VM statistics and per-process task info.
//! Read-only.

use std::ffi::{CStr, c_int};
use std::io;
use std::mem;
use std::sync::OnceLock;

// Declared here because libc deprecates its binding in favour of the mach2 crate.
unsafe extern "C" {
    fn mach_host_self() -> libc::mach_port_t;
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

/// System-wide VM state. Sizes are bytes; event counts are monotonic totals.
#[derive(Clone, Copy, Debug)]
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
    pub compressions: u64,
    pub decompressions: u64,
}

/// One `host_statistics64(HOST_VM_INFO64)` call; the source of `vm_stat`.
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

    // SAFETY: vm_page_size is set by libSystem before main and never written again.
    let page = unsafe { libc::vm_page_size } as u64;
    Ok(VmStats {
        wired: u64::from(info.wire_count) * page,
        compressed: u64::from(info.compressor_page_count) * page,
        compressor_input: info.total_uncompressed_pages_in_compressor * page,
        pageins: info.pageins * page,
        pageouts: info.pageouts * page,
        swapins: info.swapins * page,
        swapouts: info.swapouts * page,
        faults: info.faults,
        compressions: info.compressions,
        decompressions: info.decompressions,
    })
}

/// Every PID on the system, including ones this user can't inspect.
pub fn pids() -> io::Result<Vec<libc::pid_t>> {
    // SAFETY: a null buffer asks only for the current count.
    let n = unsafe { libc::proc_listallpids(std::ptr::null_mut(), 0) };
    if n <= 0 {
        return Err(io::Error::last_os_error());
    }
    // Headroom for processes started between the two calls.
    let mut pids: Vec<libc::pid_t> = vec![0; n as usize + 64];
    let bytes =
        c_int::try_from(pids.len() * mem::size_of::<libc::pid_t>()).map_err(io::Error::other)?;
    // SAFETY: `pids` has room for `bytes` bytes; the call returns how many PIDs it wrote.
    let n = unsafe { libc::proc_listallpids(pids.as_mut_ptr().cast(), bytes) };
    if n <= 0 {
        return Err(io::Error::last_os_error());
    }
    pids.truncate(n as usize);
    Ok(pids)
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

/// True when running as root, which widens what `task_info` can read.
pub fn is_root() -> bool {
    // SAFETY: geteuid has no preconditions.
    unsafe { libc::geteuid() == 0 }
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
}
