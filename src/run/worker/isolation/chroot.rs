//! Module for performing a chroot into our new jail

use std::path::Path;

use crate::error::prelude::*;

pub fn chroot(new_root: &Path) -> Result {
    // cd and chroot to the new root directory
    std::env::set_current_dir(new_root).context("Couldn't set current directory to new root")?;
    nix::unistd::chroot(new_root).context("Couldn't chroot to new root")?;
    // Extra chdir to make sure PWD is 100% correct
    std::env::set_current_dir("/").context("Couldn't set current directory to /")
}
