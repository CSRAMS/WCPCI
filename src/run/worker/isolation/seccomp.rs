//! Module for compiling and setting up seccomp filters.
//! Filters should be compiled by service process and
//! applied in a worker process.

use std::collections::HashMap;

use anyhow::Ok;
use seccompiler::{sock_filter, BpfProgram, SeccompAction, SeccompFilter, TargetArch};

use crate::error::prelude::*;

use super::syscalls::{AARCH64_CALLS, BASE_ALLOWED_SYSCALLS, SPECIAL_CASE_SYSCALLS, X86_64_CALLS};

const fn get_arch() -> TargetArch {
    #[cfg(target_arch = "x86_64")]
    {
        TargetArch::x86_64
    }
    #[cfg(target_arch = "aarch64")]
    {
        TargetArch::aarch64
    }
    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
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
#[derive(Serialize, Deserialize, Default, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case", tag = "action")]
pub enum _SeccompAction {
    /// Allows syscall.
    Allow,
    /// Returns from syscall with specified error number.
    Errno { errno: u32 },
    /// Kills calling thread.
    KillThread,
    /// Kills calling process.
    #[default]
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

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
#[serde(crate = "rocket::serde")]
pub struct BpfConfig {
    #[serde(default)]
    mismatch_action: _SeccompAction,
    #[serde(default)]
    allowed_calls: Vec<String>,
}

type SyscallNo = i32;

pub fn compile_filter(config: &BpfConfig) -> Result<Vec<SockFilter>> {
    let arch = get_arch();

    let call_table: HashMap<&str, SyscallNo> = match arch {
        TargetArch::x86_64 => X86_64_CALLS.into_iter().collect(),
        TargetArch::aarch64 => AARCH64_CALLS.into_iter().collect(),
        _ => unreachable!("Not reachable as get_arch fails compilation"),
    };

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
        .chain(
            SPECIAL_CASE_SYSCALLS
                .into_iter()
                .map(|call| Result::<_, &str>::Ok((call, vec![]))),
        )
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
