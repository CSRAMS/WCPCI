//! Misc hardening of the process
//!

use crate::error::prelude::*;

pub fn harden_process() -> Result {
    // TODO: Set more secure bits?

    debug!("Applying misc. hardening to process");

    nix::sys::prctl::set_dumpable(false).context("Couldn't set dumpable to false")?;
    nix::sys::prctl::set_no_new_privs().context("Couldn't set no new privs")
}
