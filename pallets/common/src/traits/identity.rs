// This file is part of the Polymesh distribution (https://github.com/PolymathNetwork/Polymesh).
// Copyright (c) 2020 Polymath

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3.

// This program is distributed in the hope that it will be useful, but
// WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU
// General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <http://www.gnu.org/licenses/>.

use crate::{
    traits::{
        balances,
        group::GroupTrait,
        multisig::MultiSigSubTrait,
        portfolio::PortfolioSubTrait,
        transaction_payment::{CddAndFeeDetails, ChargeTxFee},
        CommonTrait,
    },
    ChargeProtocolFee, SystematicIssuers,
};
use polymesh_primitives::{
    AuthorizationData, IdentityClaim, IdentityId, Permission, SecondaryKey, Signatory, Ticker,
};

use codec::{Decode, Encode};
use frame_support::{
    decl_event, dispatch::PostDispatchInfo, traits::Currency, weights::GetDispatchInfo, Parameter,
};
use sp_core::H512;
use sp_runtime::traits::{Dispatchable, IdentifyAccount, Member, Verify};
#[cfg(feature = "std")]
use sp_runtime::{Deserialize, Serialize};
use sp_std::vec::Vec;

/// Keys could be linked to several identities (`IdentityId`) as primary key or secondary key.
/// Primary key or external type secondary key are restricted to be linked to just one identity.
/// Other types of secondary key could be associated with more than one identity.
/// # TODO
/// * Use of `Primary` and `Signer` (instead of `Unique`) will optimize the access.
#[derive(codec::Encode, codec::Decode, Clone, PartialEq, Eq, Debug)]
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
pub enum LinkedKeyInfo {
    Unique(IdentityId),
    Group(Vec<IdentityId>),
}

pub type AuthorizationNonce = u64;

/// It represents an authorization that any account could sign to allow operations related with a
/// target identity.
///
/// # Safety
///
/// Please note, that `nonce` has been added to avoid **replay attack** and it should be the current
/// value of nonce of primary key of `target_id`. See `System::account_nonce`.
/// In this way, the authorization is delimited to an specific transaction (usually the next one)
/// of primary key of target identity.
#[derive(codec::Encode, codec::Decode, Clone, PartialEq, Eq, Debug)]
pub struct TargetIdAuthorization<Moment> {
    /// Target identity which is authorized to make an operation.
    pub target_id: IdentityId,
    /// It HAS TO be `target_id` authorization nonce: See `Identity::offchain_authorization_nonce`
    pub nonce: AuthorizationNonce,
    pub expires_at: Moment,
}

/// It is a secondary item with authorization of that secondary key (off-chain operation) to be added
/// to an identity.
/// `auth_signature` is the signature, generated by secondary item, of `TargetIdAuthorization`.
///
/// # TODO
///  - Replace `H512` type by a template type which represents explicitly the relation with
///  `TargetIdAuthorization`.
#[derive(codec::Encode, codec::Decode, Clone, PartialEq, Eq, Debug)]
pub struct SecondaryKeyWithAuth<AccountId> {
    /// Secondary key to be added.
    pub secondary_key: SecondaryKey<AccountId>,
    /// Off-chain authorization signature.
    pub auth_signature: H512,
}

/// The module's configuration trait.
pub trait Trait: CommonTrait + pallet_timestamp::Trait + balances::Trait {
    /// The overarching event type.
    type Event: From<Event<Self>> + Into<<Self as frame_system::Trait>::Event>;
    /// An extrinsic call.
    type Proposal: Parameter
        + Dispatchable<Origin = <Self as frame_system::Trait>::Origin, PostInfo = PostDispatchInfo>
        + GetDispatchInfo;
    /// MultiSig module
    type MultiSig: MultiSigSubTrait<Self::AccountId>;
    /// Portfolio module. Required to accept portfolio custody transfers.
    type Portfolio: PortfolioSubTrait<Self::Balance>;
    /// Group module
    type CddServiceProviders: GroupTrait<Self::Moment>;
    /// Balances module
    type Balances: Currency<Self::AccountId>;
    /// Charges fee for forwarded call
    type ChargeTxFeeTarget: ChargeTxFee;
    /// Used to check and update CDD
    type CddHandler: CddAndFeeDetails<Self::AccountId, Self::Call>;

    type Public: IdentifyAccount<AccountId = Self::AccountId>;
    type OffChainSignature: Verify<Signer = Self::Public> + Member + Decode + Encode;
    type ProtocolFee: ChargeProtocolFee<Self::AccountId>;
}

// rustfmt adds a comma after Option<Moment> in NewAuthorization and it breaks compilation
#[rustfmt::skip]
decl_event!(
    pub enum Event<T>
    where
        AccountId = <T as frame_system::Trait>::AccountId,
        Moment = <T as pallet_timestamp::Trait>::Moment,
    {
        /// DID, primary key account ID, secondary keys
        DidCreated(IdentityId, AccountId, Vec<SecondaryKey<AccountId>>),

        /// DID, new keys
        SecondaryKeysAdded(IdentityId, Vec<SecondaryKey<AccountId>>),

        /// DID, the keys that got removed
        SecondaryKeysRemoved(IdentityId, Vec<Signatory<AccountId>>),

        /// A signer left their identity. (did, signer)
        SignerLeft(IdentityId, Signatory<AccountId>),

        /// DID, updated secondary key, previous permissions
        SecondaryPermissionsUpdated(IdentityId, SecondaryKey<AccountId>, Vec<Permission>),


        /// DID, old primary key account ID, new ID
        PrimaryKeyUpdated(IdentityId, AccountId, AccountId),

        /// DID, claims
        ClaimAdded(IdentityId, IdentityClaim),

        /// DID, ClaimType, Claim Issuer
        ClaimRevoked(IdentityId, IdentityClaim),

        /// DID queried
        DidStatus(IdentityId, AccountId),

        /// CDD queried
        CddStatus(Option<IdentityId>, AccountId, bool),

        /// Asset DID
        AssetDidRegistered(IdentityId, Ticker),

        /// New authorization added.
        /// (authorised_by, target_did, target_key, auth_id, authorization_data, expiry)
        AuthorizationAdded(
            IdentityId,
            Option<IdentityId>,
            Option<AccountId>,
            u64,
            AuthorizationData<AccountId>,
            Option<Moment>
        ),

        /// Authorization revoked by the authorizer.
        /// (authorized_identity, authorized_key, auth_id)
        AuthorizationRevoked(Option<IdentityId>, Option<AccountId>, u64),

        /// Authorization rejected by the user who was authorized.
        /// (authorized_identity, authorized_key, auth_id)
        AuthorizationRejected(Option<IdentityId>, Option<AccountId>, u64),

        /// Authorization consumed.
        /// (authorized_identity, authorized_key, auth_id)
        AuthorizationConsumed(Option<IdentityId>, Option<AccountId>, u64),

        /// Off-chain Authorization has been revoked.
        /// (Target Identity, Signatory)
        OffChainAuthorizationRevoked(IdentityId, Signatory<AccountId>),

        /// CDD requirement for updating primary key changed. (new_requirement)
        CddRequirementForPrimaryKeyUpdated(bool),

        /// CDD claims generated by `IdentityId` (a CDD Provider) have been invalidated from
        /// `Moment`.
        CddClaimsInvalidated(IdentityId, Moment),

        /// All Secondary keys of the identity ID are frozen.
        SecondaryKeysFrozen(IdentityId),

        /// All Secondary keys of the identity ID are unfrozen.
        SecondaryKeysUnfrozen(IdentityId),
    }
);

pub trait IdentityTrait<AccountId> {
    fn get_identity(key: &AccountId) -> Option<IdentityId>;
    fn current_identity() -> Option<IdentityId>;
    fn set_current_identity(id: Option<IdentityId>);
    fn current_payer() -> Option<AccountId>;
    fn set_current_payer(payer: Option<AccountId>);

    fn is_signer_authorized(did: IdentityId, signer: &Signatory<AccountId>) -> bool;
    fn is_signer_authorized_with_permissions(
        did: IdentityId,
        signer: &Signatory<AccountId>,
        permissions: Vec<Permission>,
    ) -> bool;
    fn is_primary_key(did: IdentityId, key: &AccountId) -> bool;

    /// It adds a systematic CDD claim for each `target` identity.
    ///
    /// It is used when we add a new member to CDD providers or Governance Committee.
    fn add_systematic_cdd_claims(targets: &[IdentityId], issuer: SystematicIssuers);

    /// It removes the systematic CDD claim for each `target` identity.
    ///
    /// It is used when we remove a member from CDD providers or Governance Committee.
    fn revoke_systematic_cdd_claims(targets: &[IdentityId], issuer: SystematicIssuers);

    // Provides the DID status for the given DID
    fn has_valid_cdd(target_did: IdentityId) -> bool;

    fn create_did_with_cdd(target: AccountId) -> IdentityId;
}
