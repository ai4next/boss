use serde::{Deserialize, Serialize};

use crate::Resource;

/// Lease object used for leader election (Phase 5). Skeleton stores it as a
/// plain resource.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LeaseSpec {
    #[serde(
        default,
        rename = "holderIdentity",
        skip_serializing_if = "Option::is_none"
    )]
    pub holder_identity: Option<String>,
    #[serde(
        default,
        rename = "leaseDurationSeconds",
        skip_serializing_if = "Option::is_none"
    )]
    pub lease_duration_seconds: Option<i64>,
    #[serde(
        default,
        rename = "acquireTime",
        skip_serializing_if = "Option::is_none"
    )]
    pub acquire_time: Option<String>,
    #[serde(default, rename = "renewTime", skip_serializing_if = "Option::is_none")]
    pub renew_time: Option<String>,
    #[serde(
        default,
        rename = "leaseTransitions",
        skip_serializing_if = "Option::is_none"
    )]
    pub lease_transitions: Option<i64>,
}

pub type Lease = crate::Object<LeaseSpec>;

impl Resource for LeaseSpec {
    type Status = LeaseStatus;
    const KIND: &'static str = "Lease";
    const API_VERSION: &'static str = "boss.io/v1";
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LeaseStatus {}
