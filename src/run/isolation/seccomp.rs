//! Module for compiling and setting up seccomp filters.
//! Filters should be compiled by service process and
//! applied in a worker process.

use std::collections::HashMap;

use anyhow::Ok;
use seccompiler::{sock_filter, BpfProgram, SeccompAction, SeccompFilter, TargetArch};

use crate::error::prelude::*;

use super::{
    super::RunConfig,
    syscalls::{AARCH64_CALLS, X86_64_CALLS},
};

const fn get_arch() -> TargetArch {
    #[cfg(any(target_arch = "x86_64", target_arch = "x86"))] // Ellis: is target_arch = "x86" i686?
    {
        TargetArch::x86_64
    }
    #[cfg(target_arch = "aarch64")]
    {
        TargetArch::aarch64
    }
    #[cfg(not(any(target_arch = "x86_64", target_arch = "x86", target_arch = "aarch64")))]
    {
        compile_error!("Unsupported architecture");
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct SockFilter {
    /// Code of the instruction.
    pub code: u16,
    /// Jump if true offset.
    pub jt: usize,
    /// Jump if false offset.
    pub jf: usize,
    /// Immediate value.
    pub k: u32,
}

impl From<sock_filter> for SockFilter {
    fn from(filter: sock_filter) -> Self {
        Self {
            code: filter.code,
            jt: filter.jt as usize,
            jf: filter.jf as usize,
            k: filter.k,
        }
    }
}

impl From<SockFilter> for sock_filter {
    fn from(val: SockFilter) -> Self {
        sock_filter {
            code: val.code,
            jt: val.jt as u8,
            jf: val.jf as u8,
            k: val.k,
        }
    }
}

// Redeclared from seccompiler because they don't serialize well
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case", tag = "action")]
pub enum _SeccompAction {
    /// Allows syscall.
    Allow,
    /// Returns from syscall with specified error number.
    Errno { errno: u32 },
    /// Kills calling thread.
    KillThread,
    /// Kills calling process.
    KillProcess,
    /// Allows syscall after logging it.
    Log,
    /// Notifies tracing process of the caller with respective number.
    Trace { number: u32 },
    /// Sends `SIGSYS` to the calling process.
    Trap,
}

impl From<_SeccompAction> for SeccompAction {
    fn from(action: _SeccompAction) -> Self {
        match action {
            _SeccompAction::Allow => SeccompAction::Allow,
            _SeccompAction::Errno { errno } => SeccompAction::Errno(errno),
            _SeccompAction::KillThread => SeccompAction::KillThread,
            _SeccompAction::KillProcess => SeccompAction::KillProcess,
            _SeccompAction::Log => SeccompAction::Log,
            _SeccompAction::Trace { number } => SeccompAction::Trace(number),
            _SeccompAction::Trap => SeccompAction::Trap,
        }
    }
}

const fn default_mismatch_action() -> _SeccompAction {
    _SeccompAction::KillProcess
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(crate = "rocket::serde")]
pub struct BpfConfig {
    #[serde(default = "default_mismatch_action")]
    mismatch_action: _SeccompAction,
    #[serde(default)]
    allowed_calls: Vec<String>,
}

impl Default for BpfConfig {
    fn default() -> Self {
        Self {
            mismatch_action: default_mismatch_action(),
            allowed_calls: Vec::new(),
        }
    }
}

type SyscallNo = i32;

const BASE_ALLOWED_SYSCALLS: [&str; 98] = [
    "sched_yield",
    "statx",
    "clock_nanosleep",
    "faccessat2",
    "setsockopt",
    "dup",
    "getdents64",
    "madvise",
    "exit",
    "getgid",
    "getegid",
    "getppid",
    "getpgrp",
    "mkdir",
    "unlinkat",
    "mremap",
    "tgkill",
    "socketpair",
    "clone",
    "recvfrom",
    "vfork",
    "umask",
    "chmod",
    "unlink",
    "write",
    "openat",
    "close",
    "pipe2",
    "prlimit64",
    "mmap",
    "rt_sigprocmask",
    "clone3",
    "rt_sigaction",
    "dup2",
    "execve",
    "munmap",
    "ioctl",
    "poll",
    "brk",
    "access",
    "newfstatat",
    "read",
    "fstat",
    "pread64",
    "arch_prctl",
    "set_tid_address",
    "set_robust_list",
    "rseq",
    "mprotect",
    "getrandom",
    "getuid",
    "geteuid",
    "uname",
    "getcwd",
    "getpid",
    "socket",
    "connect",
    "lseek",
    "fcntl",
    "readlinkat",
    "futex",
    "sigaltstack",
    "sched_getaffinity",
    "readlink",
    "prctl",
    "rt_sigreturn",
    "exit_group",
    "wait4",
    "getrusage",
    "statfs",
    "sysinfo",
    "clock_getres",
    "gettid",
    "chdir",
    "listxattr",
    "ftruncate",
    "sched_getparam",
    "sched_getscheduler",
    "sched_get_priority_min",
    "sched_get_priority_max",
    "sched_setscheduler",
    "fadvise64",
    "clock_gettime",
    "capget",
    "timerfd_create",
    "timerfd_settime",
    "epoll_create",
    "eventfd2",
    "epoll_ctl",
    "epoll_wait",
    "rename",
    "fallocate",
    "rmdir",
    "epoll_create1",
    "io_uring_setup",
    "io_uring_enter",
    "epoll_pwait",
    "pkey_alloc",
];

pub fn compile_filter(run_config: &RunConfig) -> Result<Vec<SockFilter>> {
    let arch = get_arch();

    let call_table: HashMap<&str, SyscallNo> = match arch {
        TargetArch::x86_64 => X86_64_CALLS.into_iter().collect(),
        TargetArch::aarch64 => AARCH64_CALLS.into_iter().collect(),
    };

    let config = &run_config.seccomp;

    let rules = BASE_ALLOWED_SYSCALLS
        .into_iter()
        .chain(config.allowed_calls.iter().map(|s| s.as_str()))
        .map(|call| {
            call_table
                .get(call)
                .copied()
                .ok_or(call)
                .map(|call| (call as i64, vec![]))
        })
        .collect::<Result<_, _>>()
        .map_err(|call| anyhow!("Unknown syscall for seccomp: {}", call))?;

    let filter = SeccompFilter::new(
        rules,
        config.mismatch_action.into(),
        SeccompAction::Allow,
        arch,
    )
    .context("Failed to create seccomp filter")?;

    let compiled: BpfProgram = filter
        .try_into()
        .context("Failed to compile seccomp filter")?;

    Ok(compiled.into_iter().map(Into::into).collect())
}

pub fn install_filters(filters: &[SockFilter]) -> Result {
    debug!("Applying seccomp filters");
    let bpf_filter = filters.iter().map(|s| s.clone().into()).collect::<Vec<_>>();
    seccompiler::apply_filter(&bpf_filter).context("Couldn't apply seccomp filter")
}
