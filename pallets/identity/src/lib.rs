//! # Identity module
//!
//! This module is used to manage identity concept.
//!
//!  - [Module](./struct.Module.html)
//!  - [Trait](./trait.Trait.html)
//!
//! ## Overview :
//!
//! Identity concept groups different account (keys) in one place, and it allows each key to
//! make operations based on the constraint that each account (permissions and key types).
//!
//! Any account can create and manage one and only one identity, using
//! [register_did](./struct.Module.html#method.register_did). Other accounts can be added to a
//! target identity as signing key, where we also define the type of account (`External`,
//! `MuliSign`, etc.) and/or its permission.
//!
//! Some operations at identity level are only allowed to its administrator account, like
//! [set_master_key](./struct.Module.html#method.set_master_key) or
//!
//! ## Identity information
//!
//! Identity contains the following data:
//!  - `master_key`. It is the administrator account of the identity.
//!  - `signing_keys`. List of keys and their capabilities (type of key and its permissions) .
//!
//! ## Freeze signing keys
//!
//! It is an *emergency action* to block all signing keys of an identity and it can only be performed
//! by its administrator.
//!
//! see [freeze_signing_keys](./struct.Module.html#method.freeze_signing_keys)
//! see [unfreeze_signing_keys](./struct.Module.html#method.unfreeze_signing_keys)
//!

#![cfg_attr(not(feature = "std"), no_std)]
#![recursion_limit = "256"]

use polymesh_primitives::{
    AccountKey, AuthIdentifier, Authorization, AuthorizationData, AuthorizationError,
    ClaimIdentifier, Identity as DidRecord, IdentityClaim, IdentityClaimData, IdentityId, Link,
    LinkData, Permission, PreAuthorizedKeyInfo, Signatory, SignatoryType, SigningItem, Ticker,
};
use polymesh_runtime_common::{
    constants::did::{SECURITY_TOKEN, USER},
    traits::{
        asset::AcceptTransfer,
        balances::BalancesTrait,
        group::GroupTrait,
        identity::{
            AuthorizationNonce, LinkedKeyInfo, RawEvent, SigningItemWithAuth, TargetIdAuthorization,
        },
        multisig::AddSignerMultiSig,
    },
    Context,
};

use codec::Encode;
use core::{convert::From, result::Result as StdResult};

use sp_core::sr25519::{Public, Signature};
use sp_io::hashing::blake2_256;
use sp_runtime::{
    traits::{Dispatchable, Hash, SaturatedConversion, Verify},
    AnySignature,
};
use sp_std::{convert::TryFrom, mem::swap, prelude::*, vec};

use frame_support::{
    decl_error, decl_module, decl_storage,
    dispatch::{DispatchError, DispatchResult},
    ensure,
    traits::{ExistenceRequirement, WithdrawReason},
    weights::SimpleDispatchInfo,
};
use frame_system::{self as system, ensure_signed};

use polymesh_runtime_identity_rpc_runtime_api::DidRecords as RpcDidRecords;

pub use polymesh_runtime_common::traits::identity::{IdentityTrait, Trait};
pub type Event<T> = polymesh_runtime_common::traits::identity::Event<T>;

decl_storage! {
    trait Store for Module<T: Trait> as identity {

        /// Module owner.
        Owner get(fn owner) config(): T::AccountId;

        /// DID -> identity info
        pub DidRecords get(fn did_records) config(): map IdentityId => DidRecord;

        /// DID -> bool that indicates if signing keys are frozen.
        pub IsDidFrozen get(fn is_did_frozen): map IdentityId => bool;

        /// It stores the current identity for current transaction.
        pub CurrentDid: Option<IdentityId>;

        /// (DID, claim_data, claim_issuer) -> Associated claims
        pub Claims: double_map hasher(blake2_256) IdentityId, blake2_256(ClaimIdentifier) => IdentityClaim;

        // Account => DID
        pub KeyToIdentityIds get(fn key_to_identity_ids) config(): map AccountKey => Option<LinkedKeyInfo>;

        /// How much does creating a DID cost
        pub DidCreationFee get(fn did_creation_fee) config(): T::Balance;

        /// Nonce to ensure unique actions. starts from 1.
        pub MultiPurposeNonce get(fn multi_purpose_nonce) build(|_| 1u64): u64;

        /// Pre-authorize join to Identity.
        pub PreAuthorizedJoinDid get(fn pre_authorized_join_did): map Signatory => Vec<PreAuthorizedKeyInfo>;

        /// Authorization nonce per Identity. Initially is 0.
        pub OffChainAuthorizationNonce get(fn offchain_authorization_nonce): map IdentityId => AuthorizationNonce;

        /// Inmediate revoke of any off-chain authorization.
        pub RevokeOffChainAuthorization get(fn is_offchain_authorization_revoked): map (Signatory, TargetIdAuthorization<T::Moment>) => bool;

        /// All authorizations that an identity/key has
        pub Authorizations: double_map hasher(blake2_256) Signatory, blake2_256(u64) => Authorization<T::Moment>;

        /// All links that an identity/key has
        pub Links: double_map hasher(blake2_256) Signatory, blake2_256(u64) => Link<T::Moment>;

        /// All authorizations that an identity/key has given. (Authorizer, auth_id -> authorized)
        pub AuthorizationsGiven: double_map hasher(blake2_256) Signatory, blake2_256(u64) => Signatory;
    }
}

decl_module! {
    /// The module declaration.
    pub struct Module<T: Trait> for enum Call where origin: T::Origin {

        type Error = Error<T>;

        // Initializing events
        // this is needed only if you are using events in your module
        fn deposit_event() = default;
        pub fn register_did(origin, signing_items: Vec<SigningItem>) -> DispatchResult {
            let sender = ensure_signed(origin)?;
            // TODO: Subtract proper fee.
            let _imbalance = <T::Balances>::withdraw(
                &sender,
                Self::did_creation_fee(),
                WithdrawReason::Fee.into(),
                ExistenceRequirement::KeepAlive,
            )?;

            let _new_id = Self::_register_did(sender, signing_items)?;
            Ok(())
        }

        /// Register `target_account` with a new Identity.
        ///
        /// # Failure
        /// - `origin` has to be a trusted CDD provider.
        /// - `target_account` (master key of the new Identity) can be linked to just one and only
        /// one identity.
        /// - External signing keys can be linked to just one identity.
        ///
        /// # TODO
        /// - Imbalance: Since we are not handling the imbalance here, this will leave a hold in
        ///     the total supply. We are reducing someone's balance but not increasing anyone's
        ///     else balance or decreasing total supply. This will mean that the sum of all
        ///     balances will become less than the total supply.
        pub fn cdd_register_did(
            origin,
            target_account: T::AccountId,
            cdd_claim_expiry: T::Moment,
            signing_items: Vec<SigningItem>
        ) -> DispatchResult {
            // Sender has to be part of CDDProviders
            let cdd_sender = ensure_signed(origin)?;
            let cdd_key = AccountKey::try_from(cdd_sender.encode())?;
            let cdd_id = Context::current_identity_or::<Self>(&cdd_key)?;

            let cdd_providers = T::CddServiceProviders::get_members();
            ensure!(
                cdd_providers.into_iter().any(|kyc_id| kyc_id == cdd_id),
                Error::<T>::UnAuthorizedCddProvider
            );

            // Register Identity and add claim.
            let new_id = Self::_register_did(target_account, signing_items)?;
            Self::unsafe_add_claim(new_id, IdentityClaimData::CustomerDueDiligence, cdd_id, cdd_claim_expiry);
            Ok(())
        }

        /// Adds new signing keys for a DID. Only called by master key owner.
        ///
        /// # Failure
        ///  - It can only called by master key owner.
        ///  - If any signing key is already linked to any identity, it will fail.
        ///  - If any signing key is already
        pub fn add_signing_items(origin, signing_items: Vec<SigningItem>) -> DispatchResult {
            let sender_key = AccountKey::try_from(ensure_signed(origin)?.encode())?;
            let did = Context::current_identity_or::<Self>(&sender_key)?;
            let _grants_checked = Self::grant_check_only_master_key(&sender_key, did)?;

            // Check constraint 1-to-1 in relation key-identity.
            for s_item in &signing_items{
                if let Signatory::AccountKey(ref key) = s_item.signer {
                    ensure!(
                        Self::can_key_be_linked_to_did(key, s_item.signer_type),
                        Error::<T>::AlreadyLinked
                    );
                }
            }

            // Ignore any key which is already valid in that identity.
            let authorized_signing_items = Self::did_records( did).signing_items;
            signing_items.iter()
                .filter( |si| authorized_signing_items.contains(si) == false)
                .for_each( |si| Self::add_pre_join_identity( si, did));

            Self::deposit_event(RawEvent::NewSigningItems(did, signing_items));
            Ok(())
        }

        /// Removes specified signing keys of a DID if present.
        ///
        /// # Failure
        /// It can only called by master key owner.
        pub fn remove_signing_items(origin, signers_to_remove: Vec<Signatory>) -> DispatchResult {
            let sender_key = AccountKey::try_from(ensure_signed(origin)?.encode())?;
            let did = Context::current_identity_or::<Self>(&sender_key)?;
            let _grants_checked = Self::grant_check_only_master_key(&sender_key, did)?;

            // Remove any Pre-Authentication & link
            signers_to_remove.iter().for_each( |signer| {
                Self::remove_pre_join_identity( signer, did);
                if let Signatory::AccountKey(ref key) = signer {
                    Self::unlink_key_to_did(key, did);
                }
            });

            // Update signing keys at Identity.
            <DidRecords>::mutate(did, |record| {
                (*record).remove_signing_items( &signers_to_remove);
            });

            Self::deposit_event(RawEvent::RevokedSigningItems(did, signers_to_remove));
            Ok(())
        }

        /// Sets a new master key for a DID.
        ///
        /// # Failure
        /// Only called by master key owner.
        fn set_master_key(origin, new_key: AccountKey) -> DispatchResult {
            let sender = ensure_signed(origin)?;
            let sender_key = AccountKey::try_from( sender.encode())?;
            let did = Context::current_identity_or::<Self>(&sender_key)?;
            let _grants_checked = Self::grant_check_only_master_key(&sender_key, did)?;

            ensure!(
                Self::can_key_be_linked_to_did(&new_key, SignatoryType::External),
                Error::<T>::AlreadyLinked
            );

            <DidRecords>::mutate(did,
            |record| {
                (*record).master_key = new_key.clone();
            });

            Self::deposit_event(RawEvent::NewMasterKey(did, sender, new_key));
            Ok(())
        }

        /// Call this with the new master key. By invoking this method, caller accepts authorization
        /// with the new master key. If a CDD service provider approved this change, master key of
        /// the DID is updated.
        ///
        /// # Arguments
        /// * `owner_auth_id` Authorization from the owner who initiated the change
        /// * `cdd_auth_id` Authorization from a CDD service provider
        pub fn accept_master_key(origin, rotation_auth_id: u64, cdd_auth_id: u64) -> DispatchResult {
            let sender = ensure_signed(origin)?;
            let sender_key = AccountKey::try_from(sender.encode())?;
            let signer = Signatory::from(sender_key);

            // When both authorizations are present...
            ensure!(
                <Authorizations<T>>::exists(signer, rotation_auth_id),
                Error::<T>::InvalidAuthorizationFromOwner
            );
            ensure!(
                <Authorizations<T>>::exists(signer, cdd_auth_id),
                Error::<T>::InvalidAuthorizationFromCddProvider
            );

            // Accept authorization from the owner
            let rotation_auth = <Authorizations<T>>::get(signer, rotation_auth_id);

            if let AuthorizationData::RotateMasterKey(rotation_for_did) = rotation_auth.authorization_data {
                // Ensure the request was made by the owner of master key
                match rotation_auth.authorized_by {
                    Signatory::AccountKey(key) =>  {
                        let master_key = <DidRecords>::get(rotation_for_did).master_key;
                        ensure!(key == master_key, Error::<T>::KeyChangeUnauthorized);
                    },
                    _ => return Err(Error::<T>::UnknownAuthorization.into())
                };

                // Aceept authorization from CDD service provider

                let cdd_auth = <Authorizations<T>>::get(signer, cdd_auth_id);

                if let AuthorizationData::AttestMasterKeyRotation(attestation_for_did) = cdd_auth.authorization_data {
                    // Attestor must be a CDD service provider
                    let cdd_provider_did = match cdd_auth.authorized_by {
                        Signatory::AccountKey(ref key) =>  Self::get_identity(key),
                        Signatory::Identity(id)  => Some(id),
                    };

                    if let Some(id) = cdd_provider_did {
                        ensure!(
                            T::CddServiceProviders::is_member(&id),
                            Error::<T>::NotCddProviderAttestation
                        );
                    } else {
                        return Err(Error::<T>::NoDIDFound.into());
                    }

                    // Make sure authorizations are for the same DID
                    ensure!(
                        rotation_for_did == attestation_for_did,
                        Error::<T>::AuthorizationsNotForSameDids
                    );

                    // remove owner's authorization
                    Self::consume_auth(rotation_auth.authorized_by, signer, rotation_auth_id)?;

                    // remove CDD service provider's authorization
                    Self::consume_auth(cdd_auth.authorized_by, signer, cdd_auth_id)?;

                    // Replace master key of the owner that initiated key rotation
                    <DidRecords>::mutate(rotation_for_did, |record| {
                        (*record).master_key = sender_key.clone();
                    });

                    Self::deposit_event(RawEvent::MasterKeyChanged(rotation_for_did, sender_key));
                } else {
                    return Err(Error::<T>::UnknownAuthorization.into());
                }
            } else {
                return Err(Error::<T>::UnknownAuthorization.into());
            }

            Ok(())
        }

        /// Adds new claim record or edits an existing one. Only called by did_issuer's signing key
        #[weight = SimpleDispatchInfo::FixedNormal(10_000)]
        pub fn add_claim(
            origin,
            did: IdentityId,
            claim_data: IdentityClaimData,
            expiry: T::Moment,
        ) -> DispatchResult {
            let sender = ensure_signed(origin)?;
            let sender_key = AccountKey::try_from(sender.encode())?;
            let did_issuer = Context::current_identity_or::<Self>(&sender_key)?;

            ensure!(<DidRecords>::exists(did), Error::<T>::DidMustAlreadyExist);
            ensure!(<DidRecords>::exists(did_issuer), Error::<T>::ClaimIssuerDidMustAlreadyExist);

            // Verify that sender key is one of did_issuer's signing keys
            let sender_signer = Signatory::AccountKey(sender_key);
            ensure!(
                Self::is_signer_authorized(did_issuer, &sender_signer),
                Error::<T>::SenderMustHoldClaimIssuerKey
            );

            Self::unsafe_add_claim(did, claim_data, did_issuer, expiry);
            Ok(())
        }

        /// Adds a new batch of claim records or edits an existing one. Only called by
        /// `did_issuer`'s signing key.
        // TODO: fix #[weight = BatchDispatchInfo::new_normal(3_000, 10_000)]
        pub fn add_claims_batch(
            origin,
            // Vec(did_of_claim_receiver, claim_expiry, claim_data)
            claims: Vec<(IdentityId, T::Moment, IdentityClaimData)>
        ) -> DispatchResult {
            let sender = ensure_signed(origin)?;
            let sender_key = AccountKey::try_from(sender.encode())?;
            let did_issuer = Context::current_identity_or::<Self>(&sender_key)?;

            ensure!(<DidRecords>::exists(did_issuer), Error::<T>::ClaimIssuerDidMustAlreadyExist);

            // Verify that sender key is one of did_issuer's signing keys
            let sender_signer = Signatory::AccountKey(sender_key);
            ensure!(
                Self::is_signer_authorized(did_issuer, &sender_signer),
                Error::<T>::SenderMustHoldClaimIssuerKey
            );

            // Check input claims.
            for (did, _, _) in &claims {
                ensure!(<DidRecords>::exists(did), Error::<T>::DidMustAlreadyExist);
            }
            for (did, expiry, claim_data) in claims {
                Self::unsafe_add_claim(did, claim_data, did_issuer, expiry);
            }
            Ok(())
        }

        fn forwarded_call(origin, target_did: IdentityId, proposal: Box<T::Proposal>) -> DispatchResult {
            let sender = ensure_signed(origin)?;

            // 1. Constraints.
            // 1.1. A valid current identity.
            if let Some(current_did) = Context::current_identity::<Self>() {
                // 1.2. Check that current_did is a signing key of target_did
                ensure!(
                    Self::is_signer_authorized(current_did, &Signatory::Identity(target_did)),
                    Error::<T>::CurrentIdentityCannotBeForwarded
                );
            } else {
                return Err(Error::<T>::MissingCurrentIdentity.into());
            }

            // 1.3. Check that target_did has a CDD.
            // Please keep in mind that `current_did` is double-checked:
            //  - by `SignedExtension` (`update_did_signed_extension`) on 0 level nested call, or
            //  - by next code, as `target_did`, on N-level nested call, where N is equal or greater that 1.
            ensure!(Self::has_valid_cdd(target_did), Error::<T>::TargetHasNoCdd);

            // 2. Actions
            Context::set_current_identity::<Self>(Some(target_did));

            // Also set current_did roles when acting as a signing key for target_did
            // Re-dispatch call - e.g. to asset::doSomething...
            let new_origin = frame_system::RawOrigin::Signed(sender).into();

            let _res = match proposal.dispatch(new_origin) {
                Ok(_) => true,
                Err(e) => {
                    let e: DispatchError = e.into();
                    sp_runtime::print(e);
                    false
                }
            };

            Ok(())
        }

        /// Marks the specified claim as revoked
        pub fn revoke_claim(origin, did: IdentityId, claim_data: IdentityClaimData) -> DispatchResult {
            let sender_key = AccountKey::try_from( ensure_signed(origin)?.encode())?;
            let did_issuer = Context::current_identity_or::<Self>(&sender_key)?;
            let sender = Signatory::AccountKey(sender_key);

            ensure!(<DidRecords>::exists(&did_issuer), Error::<T>::ClaimIssuerDidMustAlreadyExist);
            // Verify that sender key is one of did_issuer's signing keys
            ensure!(
                Self::is_signer_authorized(did_issuer, &sender),
                Error::<T>::SenderMustHoldClaimIssuerKey
            );

            let claim_meta_data = ClaimIdentifier(claim_data, did_issuer);

            <Claims>::remove(&did, &claim_meta_data);

            Self::deposit_event(RawEvent::RevokedClaim(did, claim_meta_data));

            Ok(())
        }

        /// It sets permissions for an specific `target_key` key.
        /// Only the master key of an identity is able to set signing key permissions.
        pub fn set_permission_to_signer(origin, signer: Signatory, permissions: Vec<Permission>) -> DispatchResult {
            let sender_key = AccountKey::try_from( ensure_signed(origin)?.encode())?;
            let did = Context::current_identity_or::<Self>(&sender_key)?;
            let record = Self::grant_check_only_master_key( &sender_key, did)?;

            // You are trying to add a permission to did's master key. It is not needed.
            if let Signatory::AccountKey(ref key) = signer {
                if record.master_key == *key {
                    return Ok(());
                }
            }

            // Find key in `DidRecord::signing_keys`
            if record.signing_items.iter().find(|&si| si.signer == signer).is_some() {
                Self::update_signing_item_permissions(did, &signer, permissions)
            } else {
                Err(Error::<T>::InvalidSender.into())
            }
        }

        /// It disables all signing keys at `did` identity.
        ///
        /// # Errors
        ///
        pub fn freeze_signing_keys(origin) -> DispatchResult {
            Self::set_frozen_signing_key_flags(origin, true)
        }

        pub fn unfreeze_signing_keys(origin) -> DispatchResult {
            Self::set_frozen_signing_key_flags(origin, false)
        }

        pub fn get_my_did(origin) -> DispatchResult {
            let sender_key = AccountKey::try_from(ensure_signed(origin)?.encode())?;
            let did = Context::current_identity_or::<Self>(&sender_key)?;

            Self::deposit_event(RawEvent::DidQuery(sender_key, did));
            Ok(())
        }

        // Manage generic authorizations
        /// Adds an authorization
        pub fn add_authorization(
            origin,
            target: Signatory,
            authorization_data: AuthorizationData,
            expiry: Option<T::Moment>
        ) -> DispatchResult {
            let sender_key = AccountKey::try_from(ensure_signed(origin)?.encode())?;
            let from_did = Context::current_identity_or::<Self>(&sender_key)?;

            Self::add_auth(Signatory::from(from_did), target, authorization_data, expiry);

            Ok(())
        }

        /// Adds an authorization as a key.
        /// To be used by signing keys that don't have an identity
        pub fn add_authorization_as_key(
            origin,
            target: Signatory,
            authorization_data: AuthorizationData,
            expiry: Option<T::Moment>
        ) -> DispatchResult {
            let sender_key = AccountKey::try_from(ensure_signed(origin)?.encode())?;

            Self::add_auth(Signatory::from(sender_key), target, authorization_data, expiry);

            Ok(())
        }

        // Manage generic authorizations
        /// Adds an array of authorization
        pub fn batch_add_authorization(
            origin,
            // Vec<(target_did, auth_data, expiry)>
            auths: Vec<(Signatory, AuthorizationData, Option<T::Moment>)>
        ) -> DispatchResult {
            let sender_key = AccountKey::try_from(ensure_signed(origin)?.encode())?;
            let from_did = Context::current_identity_or::<Self>(&sender_key)?;

            for auth in auths {
                Self::add_auth(Signatory::from(from_did), auth.0, auth.1, auth.2);
            }

            Ok(())
        }

        /// Removes an authorization
        pub fn remove_authorization(
            origin,
            target: Signatory,
            auth_id: u64
        ) -> DispatchResult {
            let sender_key = AccountKey::try_from(ensure_signed(origin)?.encode())?;
            let from_did = Context::current_identity_or::<Self>(&sender_key)?;

            ensure!(
                <Authorizations<T>>::exists(target, auth_id),
                Error::<T>::AuthorizationDoesNotExist
            );
            let auth = <Authorizations<T>>::get(target, auth_id);
            ensure!(
                auth.authorized_by.eq_either(&from_did, &sender_key) ||
                    target.eq_either(&from_did, &sender_key),
                Error::<T>::Unauthorized
            );
            Self::remove_auth(target, auth_id, auth.authorized_by);

            Ok(())
        }

        /// Removes an array of authorizations
        pub fn batch_remove_authorization(
            origin,
            auth_identifiers: Vec<AuthIdentifier>
        ) -> DispatchResult {
            let sender_key = AccountKey::try_from(ensure_signed(origin)?.encode())?;
            let from_did = Context::current_identity_or::<Self>(&sender_key)?;
            let mut auths = Vec::with_capacity(auth_identifiers.len());
            for i in 0..auth_identifiers.len() {
                let auth_identifier = &auth_identifiers[i];
                ensure!(
                    <Authorizations<T>>::exists(&auth_identifier.0, &auth_identifier.1),
                    Error::<T>::AuthorizationDoesNotExist
                );
                auths.push(<Authorizations<T>>::get(&auth_identifier.0, &auth_identifier.1));
                ensure!(
                    auths[i].authorized_by.eq_either(&from_did, &sender_key) ||
                        auth_identifier.0.eq_either(&from_did, &sender_key),
                    Error::<T>::Unauthorized
                );
            }

            for i in 0..auth_identifiers.len() {
                let auth_identifier = &auth_identifiers[i];
                Self::remove_auth(auth_identifier.0, auth_identifier.1, auths[i].authorized_by);

            }

            Ok(())
        }

        /// Accepts an authorization
        pub fn accept_authorization(
            origin,
            auth_id: u64
        ) -> DispatchResult {
            let sender_key = AccountKey::try_from(ensure_signed(origin)?.encode())?;
            let signer = Context::current_identity_or::<Self>(&sender_key)
                .map_or_else(
                    |_error| Signatory::from(sender_key),
                    |did| Signatory::from(did));
            ensure!(
                <Authorizations<T>>::exists(signer, auth_id),
                Error::<T>::AuthorizationDoesNotExist
            );
            let auth = <Authorizations<T>>::get(signer, auth_id);
            match signer {
                Signatory::Identity(did) => {
                    match auth.authorization_data {
                        AuthorizationData::TransferTicker(_) =>
                            T::AcceptTransferTarget::accept_ticker_transfer(did, auth_id),
                        AuthorizationData::TransferTokenOwnership(_) =>
                            T::AcceptTransferTarget::accept_token_ownership_transfer(did, auth_id),
                        AuthorizationData::AddMultiSigSigner =>
                            T::AddSignerMultiSigTarget::accept_multisig_signer(Signatory::from(did), auth_id),
                        _ => return Err(Error::<T>::UnknownAuthorization.into())
                    }
                },
                Signatory::AccountKey(key) => {
                    match auth.authorization_data {
                        AuthorizationData::AddMultiSigSigner =>
                            T::AddSignerMultiSigTarget::accept_multisig_signer(Signatory::from(key), auth_id),
                        _ => return Err(Error::<T>::UnknownAuthorization.into())
                    }
                }
            }
        }

        /// Accepts an array of authorizations
        pub fn batch_accept_authorization(
            origin,
            auth_ids: Vec<u64>
        ) -> DispatchResult {
            let sender_key = AccountKey::try_from(ensure_signed(origin)?.encode())?;
            let signer = Context::current_identity_or::<Self>(&sender_key)
                .map_or_else(
                    |_error| Signatory::from(sender_key),
                    |did| Signatory::from(did));

            match signer {
                Signatory::Identity(did) => {
                    for auth_id in auth_ids {
                        // NB: Even if an auth is invalid (due to any reason), this batch function does NOT return an error.
                        // It will just skip that particular authorization.

                        if <Authorizations<T>>::exists(signer, auth_id) {
                            let auth = <Authorizations<T>>::get(signer, auth_id);

                            // NB: Result is not handled, invalid auths are just ignored to let the batch function continue.
                            let _result = match auth.authorization_data {
                                AuthorizationData::TransferTicker(_) =>
                                    T::AcceptTransferTarget::accept_ticker_transfer(did, auth_id),
                                AuthorizationData::TransferTokenOwnership(_) =>
                                    T::AcceptTransferTarget::accept_token_ownership_transfer(did, auth_id),
                                AuthorizationData::AddMultiSigSigner =>
                                    T::AddSignerMultiSigTarget::accept_multisig_signer(Signatory::from(did), auth_id),
                                _ => Err(Error::<T>::UnknownAuthorization.into())
                            };
                        }
                    }
                },
                Signatory::AccountKey(key) => {
                    for auth_id in auth_ids {
                        // NB: Even if an auth is invalid (due to any reason), this batch function does NOT return an error.
                        // It will just skip that particular authorization.

                        if <Authorizations<T>>::exists(signer, auth_id) {
                            let auth = <Authorizations<T>>::get(signer, auth_id);

                            //NB: Result is not handled, invalid auths are just ignored to let the batch function continue.
                            let _result = match auth.authorization_data {
                                AuthorizationData::AddMultiSigSigner =>
                                    T::AddSignerMultiSigTarget::accept_multisig_signer(Signatory::from(key), auth_id),
                                _ => Err(Error::<T>::UnknownAuthorization.into())
                            };
                        }
                    }
                }
            }

            Ok(())
        }

        // Manage Authorizations to join to an Identity
        // ================================================

        /// The key designated by `origin` accepts the authorization to join to `target_id`
        /// Identity.
        ///
        /// # Errors
        ///  - AccountKey should be authorized previously to join to that target identity.
        ///  - AccountKey is not linked to any other identity.
        pub fn authorize_join_to_identity(origin, target_id: IdentityId) -> DispatchResult {
            let sender_key = AccountKey::try_from( ensure_signed(origin)?.encode())?;
            let signer_from_key = Signatory::AccountKey( sender_key.clone());
            let signer_id_found = Self::key_to_identity_ids(sender_key);

            // Double check that `origin` (its key or identity) has been pre-authorize.
            let valid_signer = if <PreAuthorizedJoinDid>::exists(&signer_from_key) {
                // Sender key is valid.
                // Verify 1-to-1 relation between key and identity.
                ensure!(signer_id_found.is_none(), Error::<T>::AlreadyLinked);
                Some(signer_from_key)
            } else {
                // Otherwise, sender's identity (only master key) should be pre-authorize.
                match signer_id_found {
                    Some(LinkedKeyInfo::Unique(sender_id)) if Self::is_master_key(sender_id, &sender_key) => {
                        let signer_from_id = Signatory::Identity(sender_id);
                        if <PreAuthorizedJoinDid>::exists(&signer_from_id) {
                            Some(signer_from_id)
                        } else {
                            None
                        }
                    },
                    _ => None
                }
            };

            // Only works with a valid signer.
            if let Some(signer) = valid_signer {
                if let Some(pre_auth) = Self::pre_authorized_join_did( signer.clone())
                        .iter()
                        .find( |pre_auth_item| pre_auth_item.target_id == target_id) {
                    // Remove pre-auth, link key to identity and update identity record.
                    Self::remove_pre_join_identity(&signer, target_id);
                    if let Signatory::AccountKey(key) = signer {
                        Self::link_key_to_did( &key, pre_auth.signing_item.signer_type, target_id);
                    }
                    <DidRecords>::mutate( target_id, |identity| {
                        identity.add_signing_items( &[pre_auth.signing_item.clone()]);
                    });
                    Self::deposit_event( RawEvent::SignerJoinedToIdentityApproved( signer, target_id));
                    Ok(())
                } else {
                    Err(Error::<T>::Unauthorized.into())
                }
            } else {
                Err(Error::<T>::Unauthorized.into())
            }
        }

        /// Identity's master key or target key are allowed to reject a pre authorization to join.
        /// It only affects the authorization: if key accepted it previously, then this transaction
        /// shall have no effect.
        pub fn unauthorized_join_to_identity(origin, signer: Signatory, target_id: IdentityId) -> DispatchResult {
            let sender_key = AccountKey::try_from(ensure_signed(origin)?.encode())?;

            let mut is_remove_allowed = Self::is_master_key( target_id, &sender_key);

            if !is_remove_allowed {
                is_remove_allowed = match signer {
                    Signatory::AccountKey(ref key) => sender_key == *key,
                    Signatory::Identity(id) => Self::is_master_key(id, &sender_key)
                }
            }

            if is_remove_allowed {
                Self::remove_pre_join_identity( &signer, target_id);
                Ok(())
            } else {
                Err(Error::<T>::Unauthorized.into())
            }
        }


        /// It adds signing keys to target identity `id`.
        /// Keys are directly added to identity because each of them has an authorization.
        ///
        /// Arguments:
        ///     - `origin` Master key of `id` identity.
        ///     - `id` Identity where new signing keys will be added.
        ///     - `additional_keys` New signing items (and their authorization data) to add to target
        ///     identity.
        ///
        /// Failure
        ///     - It can only called by master key owner.
        ///     - Keys should be able to linked to any identity.
        pub fn add_signing_items_with_authorization( origin,
                expires_at: T::Moment,
                additional_keys: Vec<SigningItemWithAuth>) -> DispatchResult {
            let sender = ensure_signed(origin)?;
            let sender_key = AccountKey::try_from(sender.encode())?;
            let id = Context::current_identity_or::<Self>(&sender_key)?;
            let _grants_checked = Self::grant_check_only_master_key(&sender_key, id)?;

            // 0. Check expiration
            let now = <pallet_timestamp::Module<T>>::get();
            ensure!(now < expires_at, Error::<T>::AuthorizationExpired);
            let authorization = TargetIdAuthorization {
                target_id: id,
                nonce: Self::offchain_authorization_nonce(id),
                expires_at
            };
            let auth_encoded= authorization.encode();

            // 1. Verify signatures.
            for si_with_auth in additional_keys.iter() {
                let si = &si_with_auth.signing_item;

                // Get account_id from signer
                let account_id_found = match si.signer {
                    Signatory::AccountKey(ref key) =>  Public::try_from(key.as_slice()).ok(),
                    Signatory::Identity(ref id) if <DidRecords>::exists(id) => {
                        let master_key = <DidRecords>::get(id).master_key;
                        Public::try_from( master_key.as_slice()).ok()
                    },
                    _ => None
                };

                if let Some(account_id) = account_id_found {
                    if let Signatory::AccountKey(ref key) = si.signer {
                        // 1.1. Constraint 1-to-1 account to DID
                        ensure!(
                            Self::can_key_be_linked_to_did(key, si.signer_type),
                            Error::<T>::AlreadyLinked
                        );
                    }
                    // 1.2. Offchain authorization is not revoked explicitly.
                    let si_signer_authorization = &(si.signer, authorization.clone());
                    ensure!(
                        !Self::is_offchain_authorization_revoked(si_signer_authorization),
                        Error::<T>::AuthorizationHasBeenRevoked
                    );
                    // 1.3. Verify the signature.
                    let signature = AnySignature::from( Signature::from_h512(si_with_auth.auth_signature));
                    ensure!(
                        signature.verify(auth_encoded.as_slice(), &account_id),
                        Error::<T>::InvalidAuthorizationSignature
                    );
                } else {
                    return Err(Error::<T>::InvalidAccountKey.into());
                }
            }

            // 2.1. Link keys to identity
            additional_keys.iter().for_each( |si_with_auth| {
                let si = & si_with_auth.signing_item;
                if let Signatory::AccountKey(ref key) = si.signer {
                    Self::link_key_to_did( key, si.signer_type, id);
                }
            });

            // 2.2. Update that identity information and its offchain authorization nonce.
            <DidRecords>::mutate( id, |record| {
                let keys = additional_keys.iter().map( |si_with_auth| si_with_auth.signing_item.clone())
                    .collect::<Vec<_>>();
                (*record).add_signing_items( &keys[..]);
            });
            <OffChainAuthorizationNonce>::mutate( id, |offchain_nonce| {
                *offchain_nonce = authorization.nonce + 1;
            });

            Ok(())
        }

        /// It revokes the `auth` off-chain authorization of `signer`. It only takes effect if
        /// the authorized transaction is not yet executed.
        pub fn revoke_offchain_authorization(origin, signer: Signatory, auth: TargetIdAuthorization<T::Moment>) -> DispatchResult {
            let sender_key = AccountKey::try_from(ensure_signed(origin)?.encode())?;

            match signer {
                Signatory::AccountKey(ref key) => {
                    ensure!(sender_key == *key, Error::<T>::KeyNotAllowed);
                }
                Signatory::Identity(id) => {
                    ensure!(Self::is_master_key(id, &sender_key), Error::<T>::NotMasterKey);
                }
            }

            <RevokeOffChainAuthorization<T>>::insert((signer,auth), true);
            Ok(())
        }
    }
}

decl_error! {
    pub enum Error for Module<T: Trait> {
        /// One signing key can only belong to one DID
        AlreadyLinked,
        /// Missing current identity on the transaction
        MissingCurrentIdentity,
        /// Sender is not part of did's signing keys
        InvalidSender,
        /// No did linked to the user
        NoDIDFound,
        /// Signatory is not pre authorized by the identity
        Unauthorized,
        /// Given authorization is not pre-known
        UnknownAuthorization,
        /// Account Id cannot be extracted from signer
        InvalidAccountKey,
        /// Only CDD service providers are allowed.
        UnAuthorizedCddProvider,
        /// An invalid authorization from the owner.
        InvalidAuthorizationFromOwner,
        /// An invalid authorization from the CDD provider.
        InvalidAuthorizationFromCddProvider,
        /// The authorization to change the key was not from the owner of the master key.
        KeyChangeUnauthorized,
        /// Attestation was not by a CDD service provider.
        NotCddProviderAttestation,
        /// Authorizations are not for the same DID.
        AuthorizationsNotForSameDids,
        /// The DID must already exist.
        DidMustAlreadyExist,
        /// The Claim issuer DID must already exist.
        ClaimIssuerDidMustAlreadyExist,
        /// Sender must hold a claim issuer's signing key.
        SenderMustHoldClaimIssuerKey,
        /// Current identity cannot be forwarded, it is not a signing key of target identity.
        CurrentIdentityCannotBeForwarded,
        /// The authorization does not exist.
        AuthorizationDoesNotExist,
        /// The offchain authorization has expired.
        AuthorizationExpired,
        /// The master key is not linked to an identity.
        MasterKeyNotLinked,
        /// The target DID has no valid CDD.
        TargetHasNoCdd,
        /// Authorization has been explicitly revoked.
        AuthorizationHasBeenRevoked,
        /// An invalid authorization signature.
        InvalidAuthorizationSignature,
        /// This key is not allowed to execute a given operation.
        KeyNotAllowed,
        /// Only the master key is allowed to revoke an Identity Signatory off-chain authorization.
        NotMasterKey,
        /// The DID does not exist.
        DidDoesNotExist,
        /// The DID already exists.
        DidAlreadyExists,
        /// The signing keys contain the master key.
        SigningKeysContainMasterKey,
    }
}

impl<T: Trait> Module<T> {
    pub fn add_auth(
        from: Signatory,
        target: Signatory,
        authorization_data: AuthorizationData,
        expiry: Option<T::Moment>,
    ) -> u64 {
        let new_nonce = Self::multi_purpose_nonce() + 1u64;
        <MultiPurposeNonce>::put(&new_nonce);

        let auth = Authorization {
            authorization_data: authorization_data.clone(),
            authorized_by: from,
            expiry: expiry,
            auth_id: new_nonce,
        };

        <Authorizations<T>>::insert(target, new_nonce, auth);
        <AuthorizationsGiven>::insert(from, new_nonce, target);

        Self::deposit_event(RawEvent::NewAuthorization(
            new_nonce,
            from,
            target,
            authorization_data,
            expiry,
        ));

        new_nonce
    }

    /// Remove any authorization. No questions asked.
    /// NB: Please do all the required checks before calling this function.

    pub fn remove_auth(target: Signatory, auth_id: u64, authorizer: Signatory) {
        <Authorizations<T>>::remove(target, auth_id);
        <AuthorizationsGiven>::remove(authorizer, auth_id);

        Self::deposit_event(RawEvent::AuthorizationRemoved(auth_id, target));
    }

    /// Consumes an authorization.
    /// Checks if the auth has not expired and the caller is authorized to consume this auth.
    pub fn consume_auth(from: Signatory, target: Signatory, auth_id: u64) -> DispatchResult {
        ensure!(
            <Authorizations<T>>::exists(target, auth_id),
            AuthorizationError::Invalid
        );
        let auth = <Authorizations<T>>::get(target, auth_id);
        ensure!(auth.authorized_by == from, AuthorizationError::Unauthorized);
        if let Some(expiry) = auth.expiry {
            let now = <pallet_timestamp::Module<T>>::get();
            ensure!(expiry > now, AuthorizationError::Expired);
        }

        Self::remove_auth(target, auth_id, auth.authorized_by);
        Ok(())
    }

    pub fn get_authorization(target: Signatory, auth_id: u64) -> Authorization<T::Moment> {
        <Authorizations<T>>::get(target, auth_id)
    }

    pub fn get_link(target: Signatory, link_id: u64) -> Link<T::Moment> {
        <Links<T>>::get(target, link_id)
    }

    /// Adds a link to a key or an identity
    /// NB: Please do all the required checks before calling this function.
    pub fn add_link(target: Signatory, link_data: LinkData, expiry: Option<T::Moment>) -> u64 {
        let new_nonce = Self::multi_purpose_nonce() + 1u64;
        <MultiPurposeNonce>::put(&new_nonce);

        let link = Link {
            link_data: link_data.clone(),
            expiry: expiry,
            link_id: new_nonce,
        };

        <Links<T>>::insert(target, new_nonce, link);

        Self::deposit_event(RawEvent::NewLink(new_nonce, target, link_data, expiry));
        new_nonce
    }

    /// Remove a link (if it exists) from a key or identity
    /// NB: Please do all the required checks before calling this function.
    pub fn remove_link(target: Signatory, link_id: u64) {
        if <Links<T>>::exists(target, link_id) {
            <Links<T>>::remove(target, link_id);
            Self::deposit_event(RawEvent::LinkRemoved(link_id, target));
        }
    }

    /// Update link data (if it exists) from a key or identity
    /// NB: Please do all the required checks before calling this function.
    pub fn update_link(target: Signatory, link_id: u64, link_data: LinkData) {
        if <Links<T>>::exists(target, link_id) {
            <Links<T>>::mutate(target, link_id, |link| link.link_data = link_data);
            Self::deposit_event(RawEvent::LinkUpdated(link_id, target));
        }
    }

    /// Private and not sanitized function. It is designed to be used internally by
    /// others sanitezed functions.
    fn update_signing_item_permissions(
        target_did: IdentityId,
        signer: &Signatory,
        mut permissions: Vec<Permission>,
    ) -> DispatchResult {
        // Remove duplicates.
        permissions.sort();
        permissions.dedup();

        let mut new_s_item: Option<SigningItem> = None;

        <DidRecords>::mutate(target_did, |record| {
            if let Some(mut signing_item) = (*record)
                .signing_items
                .iter()
                .find(|si| si.signer == *signer)
                .cloned()
            {
                swap(&mut signing_item.permissions, &mut permissions);
                (*record).signing_items.retain(|si| si.signer != *signer);
                (*record).signing_items.push(signing_item.clone());
                new_s_item = Some(signing_item);
            }
        });

        if let Some(s) = new_s_item {
            Self::deposit_event(RawEvent::SigningPermissionsUpdated(
                target_did,
                s,
                permissions,
            ));
        }
        Ok(())
    }

    /// It checks if `key` is a signing key of `did` identity.
    /// # IMPORTANT
    /// If signing keys are frozen this function always returns false.
    /// Master key cannot be frozen.
    pub fn is_signer_authorized(did: IdentityId, signer: &Signatory) -> bool {
        let record = <DidRecords>::get(did);

        // Check master id or key
        match signer {
            Signatory::AccountKey(ref signer_key) if record.master_key == *signer_key => true,
            Signatory::Identity(ref signer_id) if did == *signer_id => true,
            _ => {
                // Check signing items if DID is not frozen.
                !Self::is_did_frozen(did)
                    && record.signing_items.iter().any(|si| si.signer == *signer)
            }
        }
    }

    fn is_signer_authorized_with_permissions(
        did: IdentityId,
        signer: &Signatory,
        permissions: Vec<Permission>,
    ) -> bool {
        let record = <DidRecords>::get(did);

        match signer {
            Signatory::AccountKey(ref signer_key) if record.master_key == *signer_key => true,
            Signatory::Identity(ref signer_id) if did == *signer_id => true,
            _ => {
                if !Self::is_did_frozen(did) {
                    if let Some(signing_item) =
                        record.signing_items.iter().find(|&si| &si.signer == signer)
                    {
                        // It retruns true if all requested permission are in this signing item.
                        return permissions.iter().all(|required_permission| {
                            signing_item.has_permission(*required_permission)
                        });
                    }
                }
                // Signatory is not part of signing items of `did`, or
                // Did is frozen.
                false
            }
        }
    }

    /// Use `did` as reference.
    pub fn is_master_key(did: IdentityId, key: &AccountKey) -> bool {
        key == &<DidRecords>::get(did).master_key
    }

    pub fn is_claim_valid(
        did: IdentityId,
        claim_data: IdentityClaimData,
        claim_issuer: IdentityId,
    ) -> bool {
        let claim_meta_data = ClaimIdentifier(claim_data, claim_issuer);
        if <Claims>::exists(&did, &claim_meta_data) {
            let now = <pallet_timestamp::Module<T>>::get();
            let claim = <Claims>::get(&did, &claim_meta_data);
            if claim.expiry > now.saturated_into::<u64>() {
                return true;
            }
        }
        false
    }

    pub fn is_any_claim_valid(
        did: IdentityId,
        claim_data: IdentityClaimData,
        claim_issuers: Vec<IdentityId>,
    ) -> bool {
        for claim_issuer in claim_issuers {
            if Self::is_claim_valid(did, claim_data.clone(), claim_issuer) {
                return true;
            }
        }
        false
    }

    pub fn fetch_valid_claim(
        did: IdentityId,
        claim_data: IdentityClaimData,
        claim_issuer: IdentityId,
    ) -> Option<IdentityClaim> {
        let claim_meta_data = ClaimIdentifier(claim_data, claim_issuer);
        if <Claims>::exists(&did, &claim_meta_data) {
            let now = <pallet_timestamp::Module<T>>::get();
            let claim = <Claims>::get(&did, &claim_meta_data);
            if claim.expiry > now.saturated_into::<u64>() {
                return Some(claim);
            }
        }
        None
    }

    pub fn fetch_any_valid_claim(
        did: IdentityId,
        claim_data: IdentityClaimData,
        claim_issuers: Vec<IdentityId>,
    ) -> Option<IdentityClaim> {
        for claim_issuer in claim_issuers {
            if let Some(claim) = Self::fetch_valid_claim(did, claim_data.clone(), claim_issuer) {
                return Some(claim);
            }
        }
        None
    }

    pub fn has_valid_cdd(claim_for: IdentityId) -> bool {
        let trusted_cdd_providers = T::CddServiceProviders::get_members();
        Self::is_any_claim_valid(
            claim_for,
            IdentityClaimData::CustomerDueDiligence,
            trusted_cdd_providers,
        )
    }

    /// IMPORTANT: No state change is allowed in this function
    /// because this function is used within the RPC calls
    pub fn is_identity_has_valid_kyc(
        claim_for: IdentityId,
        buffer: u64,
    ) -> (bool, Option<IdentityId>) {
        let trusted_cdd_providers = T::CddServiceProviders::get_members();
        if let Some(threshold) = <pallet_timestamp::Module<T>>::get()
            .saturated_into::<u64>()
            .checked_add(buffer)
        {
            for trusted_cdd_provider in trusted_cdd_providers {
                if let Some(claim) = Self::fetch_valid_claim(
                    claim_for,
                    IdentityClaimData::CustomerDueDiligence,
                    trusted_cdd_provider,
                ) {
                    if claim.expiry > threshold {
                        return (true, Some(trusted_cdd_provider));
                    }
                }
            }
        }
        return (false, None);
    }

    /// It checks that `sender_key` is the master key of `did` Identifier and that
    /// did exists.
    /// # Return
    /// A result object containing the `DidRecord` of `did`.
    pub fn grant_check_only_master_key(
        sender_key: &AccountKey,
        did: IdentityId,
    ) -> sp_std::result::Result<DidRecord, Error<T>> {
        ensure!(<DidRecords>::exists(did), Error::<T>::DidDoesNotExist);
        let record = <DidRecords>::get(did);
        ensure!(*sender_key == record.master_key, Error::<T>::KeyNotAllowed);
        Ok(record)
    }

    /// It checks if `key` is the master key or signing key of any did
    /// # Return
    /// An Option object containing the `did` that belongs to the key.
    pub fn get_identity(key: &AccountKey) -> Option<IdentityId> {
        if let Some(linked_key_info) = <KeyToIdentityIds>::get(key) {
            if let LinkedKeyInfo::Unique(linked_id) = linked_key_info {
                return Some(linked_id);
            }
        }
        return None;
    }

    /// It freezes/unfreezes the target `did` identity.
    ///
    /// # Errors
    /// Only master key can freeze/unfreeze an identity.
    fn set_frozen_signing_key_flags(origin: T::Origin, freeze: bool) -> DispatchResult {
        let sender_key = AccountKey::try_from(ensure_signed(origin)?.encode())?;
        let did = Context::current_identity_or::<Self>(&sender_key)?;
        let _grants_checked = Self::grant_check_only_master_key(&sender_key, did)?;

        if freeze {
            <IsDidFrozen>::insert(did, true);
        } else {
            <IsDidFrozen>::remove(did);
        }
        Ok(())
    }

    /// It checks that any sternal account can only be associated with at most one.
    /// Master keys are considered as external accounts.
    pub fn can_key_be_linked_to_did(key: &AccountKey, signer_type: SignatoryType) -> bool {
        if let Some(linked_key_info) = <KeyToIdentityIds>::get(key) {
            match linked_key_info {
                LinkedKeyInfo::Unique(..) => false,
                LinkedKeyInfo::Group(..) => signer_type != SignatoryType::External,
            }
        } else {
            true
        }
    }

    /// It links `key` key to `did` identity as a `key_type` type.
    /// # Errors
    /// This function can be used if `can_key_be_linked_to_did` returns true. Otherwise, it will do
    /// nothing.
    fn link_key_to_did(key: &AccountKey, key_type: SignatoryType, did: IdentityId) {
        if let Some(linked_key_info) = <KeyToIdentityIds>::get(key) {
            match linked_key_info {
                LinkedKeyInfo::Group(mut dids) => {
                    if !dids.contains(&did) && key_type != SignatoryType::External {
                        dids.push(did);
                        dids.sort();

                        <KeyToIdentityIds>::insert(key, LinkedKeyInfo::Group(dids));
                    }
                }
                _ => {
                    // This case is protected by `can_key_be_linked_to_did`.
                }
            }
        } else {
            // AccountKey is not yet linked to any identity, so no constraints.
            let linked_key_info = match key_type {
                SignatoryType::External => LinkedKeyInfo::Unique(did),
                _ => LinkedKeyInfo::Group(vec![did]),
            };
            <KeyToIdentityIds>::insert(key, linked_key_info);
        }
    }

    /// It unlinks the `key` key from `did`.
    /// If there is no more associated identities, its full entry is removed.
    fn unlink_key_to_did(key: &AccountKey, did: IdentityId) {
        if let Some(linked_key_info) = <KeyToIdentityIds>::get(key) {
            match linked_key_info {
                LinkedKeyInfo::Unique(..) => <KeyToIdentityIds>::remove(key),
                LinkedKeyInfo::Group(mut dids) => {
                    dids.retain(|ref_did| *ref_did != did);
                    if dids.is_empty() {
                        <KeyToIdentityIds>::remove(key);
                    } else {
                        <KeyToIdentityIds>::insert(key, LinkedKeyInfo::Group(dids));
                    }
                }
            }
        }
    }

    /// It adds `signing_item` to pre authorized items for `id` identity.
    fn add_pre_join_identity(signing_item: &SigningItem, id: IdentityId) {
        let signer = &signing_item.signer;
        let new_pre_auth = PreAuthorizedKeyInfo::new(signing_item.clone(), id);

        if !<PreAuthorizedJoinDid>::exists(signer) {
            <PreAuthorizedJoinDid>::insert(signer, vec![new_pre_auth]);
        } else {
            <PreAuthorizedJoinDid>::mutate(signer, |pre_auth_list| {
                pre_auth_list.retain(|pre_auth| *pre_auth != id);
                pre_auth_list.push(new_pre_auth);
            });
        }
    }

    /// It removes `signing_item` to pre authorized items for `id` identity.
    fn remove_pre_join_identity(signer: &Signatory, id: IdentityId) {
        let mut is_pre_auth_list_empty = false;
        <PreAuthorizedJoinDid>::mutate(signer, |pre_auth_list| {
            pre_auth_list.retain(|pre_auth| pre_auth.target_id != id);
            is_pre_auth_list_empty = pre_auth_list.is_empty();
        });

        if is_pre_auth_list_empty {
            <PreAuthorizedJoinDid>::remove(signer);
        }
    }

    /// It registers a did for a new asset. Only called by create_token function.
    pub fn register_asset_did(ticker: &Ticker) -> DispatchResult {
        let did = Self::get_token_did(ticker)?;
        Self::deposit_event(RawEvent::AssetDid(*ticker, did));
        // Making sure there's no pre-existing entry for the DID
        // This should never happen but just being defensive here
        ensure!(!<DidRecords>::exists(did), Error::<T>::DidAlreadyExists);
        <DidRecords>::insert(did, DidRecord::default());
        Ok(())
    }

    /// IMPORTANT: No state change is allowed in this function
    /// because this function is used within the RPC calls
    /// It is a helper function that can be used to get did for any asset
    pub fn get_token_did(ticker: &Ticker) -> StdResult<IdentityId, &'static str> {
        let mut buf = Vec::new();
        buf.extend_from_slice(&SECURITY_TOKEN.encode());
        buf.extend_from_slice(&ticker.encode());
        IdentityId::try_from(T::Hashing::hash(&buf[..]).as_ref())
    }

    pub fn _register_did(
        sender: T::AccountId,
        signing_items: Vec<SigningItem>,
    ) -> Result<IdentityId, DispatchError> {
        // Adding extrensic count to did nonce for some unpredictability
        // NB: this does not guarantee randomness
        let new_nonce =
            Self::multi_purpose_nonce() + u64::from(<system::Module<T>>::extrinsic_count()) + 7u64;
        // Even if this transaction fails, nonce should be increased for added unpredictability of dids
        <MultiPurposeNonce>::put(&new_nonce);

        let master_key = AccountKey::try_from(sender.encode())?;

        // 1 Check constraints.
        // 1.1. Master key is not linked to any identity.
        ensure!(
            Self::can_key_be_linked_to_did(&master_key, SignatoryType::External),
            Error::<T>::MasterKeyNotLinked
        );
        // 1.2. Master key is not part of signing keys.
        ensure!(
            signing_items.iter().find(|sk| **sk == master_key).is_none(),
            Error::<T>::SigningKeysContainMasterKey
        );

        let block_hash = <system::Module<T>>::block_hash(<system::Module<T>>::block_number());

        let did = IdentityId::from(blake2_256(&(USER, block_hash, new_nonce).encode()));

        // 1.3. Make sure there's no pre-existing entry for the DID
        // This should never happen but just being defensive here
        ensure!(!<DidRecords>::exists(did), Error::<T>::DidAlreadyExists);
        // 1.4. Signing keys can be linked to the new identity.
        for s_item in &signing_items {
            if let Signatory::AccountKey(ref key) = s_item.signer {
                ensure!(
                    Self::can_key_be_linked_to_did(key, s_item.signer_type),
                    Error::<T>::AlreadyLinked
                );
            }
        }

        // 2. Apply changes to our extrinsics.
        // 2.1. Link  master key and add pre-authorized signing keys
        Self::link_key_to_did(&master_key, SignatoryType::External, did);
        signing_items
            .iter()
            .for_each(|s_item| Self::add_pre_join_identity(s_item, did));

        // 2.2. Create a new identity record.
        let record = DidRecord {
            master_key,
            ..Default::default()
        };
        <DidRecords>::insert(&did, record);

        Self::deposit_event(RawEvent::NewDid(did.clone(), sender, signing_items));
        Ok(did)
    }

    /// It adds a new claim without any previous security check.
    fn unsafe_add_claim(
        target_did: IdentityId,
        claim_data: IdentityClaimData,
        did_issuer: IdentityId,
        expiry: T::Moment,
    ) {
        let claim_meta_data = ClaimIdentifier(claim_data.clone(), did_issuer);

        let last_update_date = <pallet_timestamp::Module<T>>::get().saturated_into::<u64>();

        let issuance_date = if <Claims>::exists(&target_did, &claim_meta_data) {
            <Claims>::get(&target_did, &claim_meta_data).issuance_date
        } else {
            last_update_date
        };

        let claim = IdentityClaim {
            claim_issuer: did_issuer,
            issuance_date: issuance_date,
            last_update_date: last_update_date,
            expiry: expiry.saturated_into::<u64>(),
            claim: claim_data,
        };

        <Claims>::insert(&target_did, &claim_meta_data, claim.clone());

        Self::deposit_event(RawEvent::NewClaims(target_did, claim_meta_data, claim));
    }
}

impl<T: Trait> Module<T> {
    /// RPC call to know whether the given did has valid cdd claim or not
    pub fn is_identity_has_valid_cdd(
        did: IdentityId,
        buffer_time: Option<u64>,
    ) -> Option<IdentityId> {
        let buffer = match buffer_time {
            Some(time) => time,
            None => 0u64,
        };
        let (status, provider) = Self::is_identity_has_valid_kyc(did, buffer);
        if status {
            return provider;
        }
        None
    }

    /// RPC call to query the given ticker did
    pub fn get_asset_did(ticker: Ticker) -> Result<IdentityId, &'static str> {
        Self::get_token_did(&ticker)
    }

    /// Retrieve DidRecords for `did`
    pub fn get_did_records(did: IdentityId) -> RpcDidRecords<AccountKey, SigningItem> {
        if <DidRecords>::exists(did) {
            let record = <DidRecords>::get(did);
            RpcDidRecords::Success {
                master_key: record.master_key,
                signing_items: record.signing_items,
            }
        } else {
            RpcDidRecords::IdNotFound
        }
    }
}

impl<T: Trait> IdentityTrait for Module<T> {
    fn get_identity(key: &AccountKey) -> Option<IdentityId> {
        Self::get_identity(&key)
    }

    fn current_identity() -> Option<IdentityId> {
        <CurrentDid>::get()
    }

    fn set_current_identity(id: Option<IdentityId>) {
        if let Some(id) = id {
            <CurrentDid>::put(id);
        } else {
            <CurrentDid>::kill();
        }
    }

    fn is_signer_authorized(did: IdentityId, signer: &Signatory) -> bool {
        Self::is_signer_authorized(did, signer)
    }

    fn is_master_key(did: IdentityId, key: &AccountKey) -> bool {
        Self::is_master_key(did, &key)
    }

    fn is_signer_authorized_with_permissions(
        did: IdentityId,
        signer: &Signatory,
        permissions: Vec<Permission>,
    ) -> bool {
        Self::is_signer_authorized_with_permissions(did, signer, permissions)
    }
}
