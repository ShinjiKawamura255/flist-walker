use serde::{Deserialize, Serialize};
#[cfg(any(not(target_os = "macos"), test))]
use std::path::Path;

pub(super) const MARKER_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(in crate::updater) enum TargetRole {
    Readme,
    License,
    Notices,
    Binary,
}

impl TargetRole {
    pub(super) const ORDER: [Self; 4] = [Self::Readme, Self::License, Self::Notices, Self::Binary];

    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Readme => "readme",
            Self::License => "license",
            Self::Notices => "notices",
            Self::Binary => "binary",
        }
    }

    pub(super) fn target_name(self, binary_name: &str, sidecar_prefix: Option<&str>) -> String {
        let sidecar_prefix = sidecar_prefix.unwrap_or_default();
        match self {
            Self::Readme => format!("{sidecar_prefix}README.txt"),
            Self::License => format!("{sidecar_prefix}LICENSE.txt"),
            Self::Notices => format!("{sidecar_prefix}THIRD_PARTY_NOTICES.txt"),
            Self::Binary => binary_name.to_string(),
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Phase {
    PreparedParentOwned,
    HelperRegistered,
    ApplyingSidecars,
    BinaryIntent,
    BinaryCommitted,
    RollingBack,
    RolledBack,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum TargetState {
    Prepared,
    Intent,
    Applied,
    RolledBack,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(super) struct TargetRecord {
    pub(super) role: TargetRole,
    pub(super) originally_present: bool,
    pub(super) old_hash: Option<String>,
    pub(super) new_hash: String,
    pub(super) state: TargetState,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(in crate::updater) struct TransactionMarker {
    pub(super) version: u32,
    pub(super) transaction_id: String,
    pub(super) binary_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) sidecar_prefix: Option<String>,
    pub(super) parent_pid: u32,
    pub(super) helper_pid: Option<u32>,
    pub(super) helper_start_token: Option<String>,
    pub(super) helper_hash: String,
    pub(super) phase: Phase,
    pub(super) targets: Vec<TargetRecord>,
}

#[cfg(any(not(target_os = "macos"), test))]
pub(in crate::updater) struct TransactionSources<'a> {
    pub(in crate::updater) binary: &'a Path,
    pub(in crate::updater) readme: &'a Path,
    pub(in crate::updater) license: &'a Path,
    pub(in crate::updater) notices: &'a Path,
}

#[cfg(any(not(target_os = "macos"), test))]
impl TransactionSources<'_> {
    pub(super) fn for_role(&self, role: TargetRole) -> &Path {
        match role {
            TargetRole::Readme => self.readme,
            TargetRole::License => self.license,
            TargetRole::Notices => self.notices,
            TargetRole::Binary => self.binary,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::updater) enum RecoveryOutcome {
    Deferred,
    RolledBack,
    Committed,
    Ambiguous,
}
