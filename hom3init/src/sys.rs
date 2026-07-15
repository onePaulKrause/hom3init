// sys.rs - the syscall layer for HOM3 init.
//
// Minimal, audited FFI against the static musl libc we link. Each wrapper is a
// thin, safe-ish shell over one kernel call. NOTHING here reaches the network.
//
// TIGHTENING (minimization loop, later): replace these libc externs with raw
// aarch64 `svc #0` syscalls and build no_std, dropping libc entirely. The set
// of calls is deliberately tiny so that step is small.

use core::ffi::c_void;
use std::ffi::CString;

#[allow(non_camel_case_types)]
type c_int = i32;
#[allow(non_camel_case_types)]
type c_ulong = u64;

extern "C" {
    fn mount(src: *const u8, tgt: *const u8, fstype: *const u8,
             flags: c_ulong, data: *const c_void) -> c_int;
    fn umount2(tgt: *const u8, flags: c_int) -> c_int;
    fn sync();
    fn reboot(howto: c_int) -> c_int;
    fn fork() -> c_int;
    fn execv(path: *const u8, argv: *const *const u8) -> c_int;
    fn waitpid(pid: c_int, status: *mut c_int, options: c_int) -> c_int;
    fn setsid() -> c_int;
    fn _exit(code: c_int) -> !;
    fn __errno_location() -> *mut c_int;
}

// mount flags / reboot magics (Linux aarch64, stable ABI)
pub const MS_NOSUID: c_ulong = 2;
pub const MS_NODEV: c_ulong = 4;
pub const MS_NOEXEC: c_ulong = 8;
pub const MS_RDONLY: c_ulong = 1;
pub const RB_AUTOBOOT: c_int = 0x0123_4567u32 as c_int; // reboot
pub const RB_POWER_OFF: c_int = 0x4321_fedcu32 as c_int; // power off

pub fn errno() -> i32 {
    unsafe { *__errno_location() }
}

pub fn mount_vfs(src: &str, tgt: &str, fstype: &str, flags: c_ulong) -> Result<(), i32> {
    let s = CString::new(src).unwrap();
    let t = CString::new(tgt).unwrap();
    let f = CString::new(fstype).unwrap();
    let rc = unsafe {
        mount(s.as_ptr() as *const u8, t.as_ptr() as *const u8,
              f.as_ptr() as *const u8, flags, core::ptr::null())
    };
    if rc == 0 { Ok(()) } else { Err(errno()) }
}

pub fn mount_dev(dev: &str, tgt: &str, fstype: &str, flags: c_ulong) -> Result<(), i32> {
    mount_vfs(dev, tgt, fstype, flags)
}

pub fn umount(tgt: &str) -> Result<(), i32> {
    let t = CString::new(tgt).unwrap();
    let rc = unsafe { umount2(t.as_ptr() as *const u8, 0) };
    if rc == 0 { Ok(()) } else { Err(errno()) }
}

pub fn sync_all() { unsafe { sync() } }

/// Never returns on success.
pub fn power_off() -> ! {
    unsafe { sync(); reboot(RB_POWER_OFF); }
    loop { core::hint::spin_loop() }
}

pub fn reboot_now() -> ! {
    unsafe { sync(); reboot(RB_AUTOBOOT); }
    loop { core::hint::spin_loop() }
}

/// fork+exec a child, returning its pid in the parent. Child execs `path`.
pub fn spawn(path: &str, argv: &[&str]) -> Result<i32, i32> {
    let cpath = CString::new(path).unwrap();
    let cargs: Vec<CString> = argv.iter().map(|a| CString::new(*a).unwrap()).collect();
    let mut ptrs: Vec<*const u8> = cargs.iter().map(|c| c.as_ptr() as *const u8).collect();
    ptrs.push(core::ptr::null());
    let pid = unsafe { fork() };
    if pid < 0 {
        return Err(errno());
    }
    if pid == 0 {
        // child: new session, then exec. No allocation happens here (argv was
        // built before fork). If exec fails, _exit immediately (async-signal-safe).
        unsafe {
            setsid();
            execv(cpath.as_ptr() as *const u8, ptrs.as_ptr());
            _exit(127);
        }
    }
    Ok(pid)
}

/// Reap exactly one terminated child (any). Returns (pid, raw_status) or None.
pub fn reap_one() -> Option<(i32, i32)> {
    let mut status: c_int = 0;
    let pid = unsafe { waitpid(-1, &mut status as *mut c_int, 0) };
    if pid > 0 { Some((pid, status)) } else { None }
}
