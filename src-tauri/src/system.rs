//! Computer Control: deterministic, system-level, directly callable.
//!
//! The boundary the owner set, and the one this module is written against:
//!
//! - Computer Use owns the screen -- looking at it, clicking, typing.
//! - OpenClaw owns agency -- planning, deciding what to do next.
//! - Computer Control owns neither. It is a set of operations that each take a
//!   defined input, do one thing, and return a defined result.
//!
//! In practice that draws a sharp line. "Set the output volume to 30" belongs
//! here. "Put the machine into a state suitable for a meeting" does not: that
//! is a plan. Listing the running applications belongs here; pressing a button
//! inside one of them does not, whatever it would accomplish.
//!
//! Nothing in this module reads the screen, and nothing in it loops.

pub(crate) mod apps;
pub(crate) mod audio;
pub(crate) mod clipboard;
pub(crate) mod inspect;
pub(crate) mod permissions;
pub(crate) mod power;
pub(crate) mod process_control;

/// Every capability this module answers.
///
/// It is a list rather than a chain of comparisons on purpose: the permission
/// gate has to be able to WALK it. Twelve Office capabilities were once
/// routable and unauthorisable at the same time, because the two facts lived in
/// places nothing compared. This one cannot drift for that reason.
pub(crate) const CAPABILITIES: &[&str] = &[
    "system.storage",
    "system.cpu",
    "system.memory",
    "system.network",
    "system.process.list",
    "system.process.info",
    "system.process.terminate",
    "system.app.list",
    "system.app.running",
    "system.app.launch",
    "system.app.quit",
    "system.clipboard.read",
    "system.clipboard.write",
    "system.audio.volume.get",
    "system.audio.volume.set",
    "system.audio.mute.get",
    "system.audio.mute.set",
    "system.permission.list",
    "system.permission.open_settings",
    "system.power.sleep",
    "system.power.restart",
    "system.power.shutdown",
];

#[derive(Debug)]
pub(crate) struct SystemError {
    pub(crate) invalid_request: bool,
    pub(crate) message: String,
}

impl SystemError {
    pub(crate) fn invalid(message: impl Into<String>) -> Self {
        Self {
            invalid_request: true,
            message: message.into(),
        }
    }

    pub(crate) fn execution(message: impl Into<String>) -> Self {
        Self {
            invalid_request: false,
            message: message.into(),
        }
    }
}
