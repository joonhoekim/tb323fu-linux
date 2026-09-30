// SPDX-License-Identifier: MIT
//! Device model of the Lenovo Legion Tab Gen 5 / Y700 5th Gen (TB323FU) for
//! `tb323fu-helperd`: where each feature lives in sysfs, how to read and write
//! it safely, and the persistent configuration. No D-Bus here, so everything
//! can be tested against a fake sysfs tree (`TB323FU_SYSFS_ROOT`).

pub mod config;
pub mod features;
pub mod sys;

pub use config::Config;
