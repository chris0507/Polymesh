//! Runtime API definition for Identity module.

use codec::{Decode, Encode};
pub use polymesh_primitives::{Authorization, AuthorizationType, IdentityId, Moment};
#[cfg(feature = "std")]
use serde::{Deserialize, Serialize};
use sp_std::{prelude::*, vec::Vec};

pub type Error = Vec<u8>;
pub type CddStatus = Result<IdentityId, Error>;
pub type AssetDidResult = Result<IdentityId, Error>;

/// A result of execution of get_votes.
#[derive(Eq, PartialEq, Encode, Decode)]
#[cfg_attr(feature = "std", derive(Debug, Serialize, Deserialize))]
#[cfg_attr(feature = "std", serde(rename_all = "camelCase"))]
pub enum DidRecords<AccountId, SecondaryKey> {
    /// Id was found and has the following primary key and secondary keys.
    Success {
        primary_key: AccountId,
        secondary_keys: Vec<SecondaryKey>,
    },
    /// Error.
    IdNotFound,
}

#[derive(Encode, Decode, PartialEq, Eq)]
#[cfg_attr(feature = "std", derive(Debug, Serialize, Deserialize))]
pub enum DidStatus {
    Unknown,
    Exists,
    CddVerified,
}
