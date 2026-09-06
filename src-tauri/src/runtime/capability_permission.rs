use std::collections::HashSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CapabilityPermissionDecision {
    Allowed,
    RequiresApproval,
    Denied,
}

/// GM-1 safety rule.
///
/// Generative Media execution always requires confirmation attached to the
/// current request. Persistent Trusted Automation cannot substitute for that
/// confirmation.
///
/// GM-4 may add explicit paid-cloud budget/authorization semantics later
/// without changing this Runtime-owned permission boundary.
pub(crate) const GENERATIVE_MEDIA_ALWAYS_CONFIRM_CAPABILITIES: &[&str] = &[
    "media.text-to-image",
    "media.image-edit",
    "media.text-to-video",
    "media.image-to-video",
    "media.reference.image.analyze",
    "media.reference.video.analyze",
    "media.reference.generate",
];

/// Executor-neutral Runtime permission policy.
///
/// The Runtime owns the authorization decision. Individual executors provide
/// their confirmable/always-confirm capability policy without owning the
/// persisted Trusted Automation allow-list.
pub(crate) struct ConfiguredCapabilityPermissionGate {
    allowed_capabilities: HashSet<String>,
}

impl ConfiguredCapabilityPermissionGate {
    pub(crate) fn new(
        allowed_capabilities: impl IntoIterator<Item = String>,
    ) -> Self {
        Self {
            allowed_capabilities: allowed_capabilities
                .into_iter()
                .map(|capability| capability.trim().to_owned())
                .filter(|capability| !capability.is_empty())
                .collect(),
        }
    }

    pub(crate) fn authorize_with_policy(
        &self,
        capability: &str,
        user_confirmed: bool,
        confirmable_capabilities: &[&str],
        always_confirm_capabilities: &[&str],
    ) -> CapabilityPermissionDecision {
        let capability = capability.trim();

        if capability.is_empty() {
            return CapabilityPermissionDecision::Denied;
        }

        if always_confirm_capabilities.contains(&capability) {
            return if user_confirmed {
                CapabilityPermissionDecision::Allowed
            } else {
                CapabilityPermissionDecision::RequiresApproval
            };
        }

        if self.allowed_capabilities.contains(capability)
            || (confirmable_capabilities.contains(&capability)
                && user_confirmed)
        {
            CapabilityPermissionDecision::Allowed
        } else {
            CapabilityPermissionDecision::Denied
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn existing_trusted_automation_behavior_is_preserved() {
        let gate = ConfiguredCapabilityPermissionGate::new([
            "filesystem.scan".to_owned(),
        ]);

        assert_eq!(
            gate.authorize_with_policy(
                "filesystem.scan",
                false,
                &["filesystem.scan", "filesystem.read"],
                &[],
            ),
            CapabilityPermissionDecision::Allowed
        );

        assert_eq!(
            gate.authorize_with_policy(
                "filesystem.read",
                false,
                &["filesystem.scan", "filesystem.read"],
                &[],
            ),
            CapabilityPermissionDecision::Denied
        );

        assert_eq!(
            gate.authorize_with_policy(
                "filesystem.read",
                true,
                &["filesystem.scan", "filesystem.read"],
                &[],
            ),
            CapabilityPermissionDecision::Allowed
        );
    }

    #[test]
    fn always_confirm_overrides_persistent_trust() {
        let gate = ConfiguredCapabilityPermissionGate::new([
            "system.power.shutdown".to_owned(),
        ]);

        assert_eq!(
            gate.authorize_with_policy(
                "system.power.shutdown",
                false,
                &["system.power.shutdown"],
                &["system.power.shutdown"],
            ),
            CapabilityPermissionDecision::RequiresApproval
        );

        assert_eq!(
            gate.authorize_with_policy(
                "system.power.shutdown",
                true,
                &["system.power.shutdown"],
                &["system.power.shutdown"],
            ),
            CapabilityPermissionDecision::Allowed
        );
    }

    #[test]
    fn every_media_capability_requires_current_confirmation() {
        let gate = ConfiguredCapabilityPermissionGate::new(
            GENERATIVE_MEDIA_ALWAYS_CONFIRM_CAPABILITIES
                .iter()
                .map(|capability| (*capability).to_owned()),
        );

        for capability in GENERATIVE_MEDIA_ALWAYS_CONFIRM_CAPABILITIES {
            assert_eq!(
                gate.authorize_with_policy(
                    capability,
                    false,
                    GENERATIVE_MEDIA_ALWAYS_CONFIRM_CAPABILITIES,
                    GENERATIVE_MEDIA_ALWAYS_CONFIRM_CAPABILITIES,
                ),
                CapabilityPermissionDecision::RequiresApproval,
                "{capability} must not inherit Trusted Automation approval"
            );

            assert_eq!(
                gate.authorize_with_policy(
                    capability,
                    true,
                    GENERATIVE_MEDIA_ALWAYS_CONFIRM_CAPABILITIES,
                    GENERATIVE_MEDIA_ALWAYS_CONFIRM_CAPABILITIES,
                ),
                CapabilityPermissionDecision::Allowed,
                "{capability} should run after current confirmation"
            );
        }
    }

    #[test]
    fn unknown_media_capability_fails_closed() {
        let gate = ConfiguredCapabilityPermissionGate::new(Vec::new());

        assert_eq!(
            gate.authorize_with_policy(
                "media.unknown",
                true,
                GENERATIVE_MEDIA_ALWAYS_CONFIRM_CAPABILITIES,
                GENERATIVE_MEDIA_ALWAYS_CONFIRM_CAPABILITIES,
            ),
            CapabilityPermissionDecision::Denied
        );
    }

    #[test]
    fn blank_capability_fails_closed() {
        let gate = ConfiguredCapabilityPermissionGate::new(Vec::new());

        assert_eq!(
            gate.authorize_with_policy(" ", true, &[], &[]),
            CapabilityPermissionDecision::Denied
        );
    }
}
