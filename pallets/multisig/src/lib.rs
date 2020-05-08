//! # Multisig Module
//!
//! The multisig module provides functionality for `n` out of `m` multisigs.
//!
//! ## Overview
//!
//! The multisig module provides functions for:
//!
//! - creating a new multisig,
//! - proposing a multisig transaction,
//! - approving a multisig transaction,
//! - adding new signers to the multisig,
//! - removing existing signers from multisig.
//!
//! ### Terminology
//!
//! - **multisig**: a special type of account that can do tranaction only if at least `n` of its `m`
//! signers approve.
//! - **proposal**: a general transaction that the multisig can vote on and accept.
//!
//! ## Interface
//!
//! ### Dispatchable Functions
//!
//! - `create_multisig` - Creates a new multisig.
//! - `create_or_approve_proposal_as_identity` - Creates or approves a multisig proposal given the
//! signer's identity.
//! - `create_or_approve_proposal_as_key` - Creates or approves a multisig proposal given the
//! signer's account key.
//! - `create_proposal_as_identity` - Creates a multisig proposal given the signer's identity.
//! - `create_proposal_as_key` - Creates a multisig proposal given the signer's account key.
//! - `approve_as_identity` - Approves a multisig proposal given the signer's identity.
//! - `approve_as_key` - Approves a multisig proposal given the signer's account key.
//! - `accept_multisig_signer_as_identity` - Accepts a multisig signer authorization given the
//! signer's identity.
//! - `accept_multisig_signer_as_key` - Accepts a multisig signer authorization given the signer's
//! account key.
//! - `add_multisig_signer` - Adds a signer to the multisig.
//! - `remove_multisig_signer` - Removes a signer from the multisig.
//! - `add_multisig_signers_via_creator` - Adds a signer to the multisig with the signed being the
//! creator of the multisig.
//! - `change_sigs_required` - Changes the number of signatures required to execute a transaction.
//! - `change_all_signers_and_sigs_required` - Replaces all existing signers of the given multisig
//! and changes the number of required signatures.
//! `make_multisig_signer` - Adds a multisig as a signer of the current DID if the current DID is
//! the creator of the multisig.
//! `make_multisig_master` - Adds a multisig as the master key of the current DID if the current did
//! is the creator of the multisig.

//!
//! ### Other Public Functions
//!
//! - `create_multisig_account` - Creates a multisig account without precondition checks or emitting
//! an event.
//! - `create_proposal` - Creates a proposal for a multisig transaction.
//! - `create_or_approve_proposal` - Creates or approves a multisig proposal.
//! - `approve_for` - Approves a multisig proposal and executes it if enough signatures have been
//! received.
//! - `_accept_multisig_signer` - Accepts and processes an addition of a signer to a multisig.
//! - `get_next_multisig_address` - Gets the next available multisig account ID.
//! - `get_multisig_address` - Constructs a multisig account given a nonce.
//! - `ms_signers` - Helper function that checks if someone is an authorized signer of a multisig or
//! not.
//! - `is_changing_signers_allowed` - Checks whether changing the list of signers is allowed in a
//! multisig.

#![cfg_attr(not(feature = "std"), no_std)]

use codec::{Decode, Encode, Error as CodecError};
use core::convert::{From, TryInto};
use frame_support::{
    decl_error, decl_event, decl_module, decl_storage,
    dispatch::{DispatchError, DispatchResult},
    ensure,
    weights::{DispatchClass, FunctionOf, GetDispatchInfo, SimpleDispatchInfo},
    StorageValue,
};
use frame_system::{self as system, ensure_signed};
use pallet_identity as identity;
use pallet_transaction_payment::{CddAndFeeDetails, ChargeTxFee};
use polymesh_common_utilities::{
    identity::{LinkedKeyInfo, Trait as IdentityTrait},
    multisig::AddSignerMultiSig,
    Context,
};
use polymesh_primitives::{
    AccountKey, AuthorizationData, AuthorizationError, IdentityId, Signatory,
};
use sp_runtime::traits::{Dispatchable, Hash};
use sp_std::{convert::TryFrom, prelude::*};

/// Either the ID of a successfully created multisig account or an error.
pub type CreateMultisigAccountResult<T> =
    sp_std::result::Result<<T as frame_system::Trait>::AccountId, DispatchError>;
/// Either the ID of a successfully created proposal or an error.
pub type CreateProposalResult = sp_std::result::Result<u64, DispatchError>;

/// The multisig trait.
pub trait Trait: frame_system::Trait + IdentityTrait {
    /// The overarching event type.
    type Event: From<Event<Self>> + Into<<Self as frame_system::Trait>::Event>;
}

decl_storage! {
    trait Store for Module<T: Trait> as MultiSig {
        /// Nonce to ensure unique MultiSig addresses are generated; starts from 1.
        pub MultiSigNonce get(fn ms_nonce) build(|_| 1u64): u64;
        /// Signers of a multisig. (multisig, signer) => signer.
        pub MultiSigSigners: double_map hasher(twox_64_concat) T::AccountId, hasher(blake2_128_concat) Signatory => Signatory;
        /// Number of approved/accepted signers of a multisig.
        pub NumberOfSigners get(fn number_of_signers): map hasher(twox_64_concat) T::AccountId => u64;
        /// Confirmations required before processing a multisig tx.
        pub MultiSigSignsRequired get(fn ms_signs_required): map hasher(twox_64_concat) T::AccountId => u64;
        /// Number of transactions proposed in a multisig. Used as tx id; starts from 0.
        pub MultiSigTxDone get(fn ms_tx_done): map hasher(twox_64_concat) T::AccountId => u64;
        /// Proposals presented for voting to a multisig (multisig, proposal id) => Option<proposal>.
        pub Proposals get(fn proposals): map hasher(twox_64_concat) (T::AccountId, u64) => Option<T::Proposal>;
        /// A mapping of proposals to their IDs.
        pub ProposalIds get(fn proposal_ids):
            double_map hasher(twox_64_concat) T::AccountId, hasher(blake2_256) T::Proposal => Option<u64>;
        /// Number of votes in favor of a tx. Mapping from (multisig, tx id) => no. of approvals.
        pub TxApprovals get(fn tx_approvals): map hasher(twox_64_concat) (T::AccountId, u64) => u64;
        /// Individual multisig signer votes. (multi sig, signer, proposal) => vote.
        pub Votes get(fn votes): map hasher(blake2_128_concat) (T::AccountId, Signatory, u64) => bool;
        /// Maps a multisig to its creator's identity.
        pub MultiSigCreator get(fn ms_creator): map hasher(twox_64_concat) T::AccountId => IdentityId;
        /// Maps a key to a multisig address.
        pub KeyToMultiSig get(fn key_to_ms): map hasher(blake2_128_concat) AccountKey => T::AccountId;
    }
}

decl_module! {
    /// A multisig module.
    pub struct Module<T: Trait> for enum Call where origin: T::Origin {
        type Error = Error<T>;

        fn deposit_event() = default;

        /// Creates a multisig
        ///
        /// # Arguments
        /// * `signers` - Signers of the multisig (They need to accept authorization before they are actually added).
        /// * `sigs_required` - Number of sigs required to process a multi-sig tx.
        #[weight = SimpleDispatchInfo::FixedNormal(250_000)]
        pub fn create_multisig(origin, signers: Vec<Signatory>, sigs_required: u64) -> DispatchResult {
            let sender = ensure_signed(origin)?;
            ensure!(!signers.is_empty(), Error::<T>::NoSigners);
            ensure!(u64::try_from(signers.len()).unwrap_or_default() >= sigs_required && sigs_required > 0,
                Error::<T>::RequiredSignaturesOutOfBounds
            );
            let account_id = Self::create_multisig_account(
                sender.clone(),
                signers.as_slice(),
                sigs_required
            )?;
            Self::deposit_event(RawEvent::MultiSigCreated(account_id, sender, signers, sigs_required));
            Ok(())
        }

        /// Creates a multisig proposal if it hasn't been created or approves it if it has.
        ///
        /// # Arguments
        /// * `multisig` - MultiSig address.
        /// * `proposal` - Proposal to be voted on.
        /// If this is 1 out of `m` multisig, the proposal will be immediately executed.
        #[weight = SimpleDispatchInfo::FixedNormal(750_000)]
        pub fn create_or_approve_proposal_as_identity(
            origin,
            multisig: T::AccountId,
            proposal: Box<T::Proposal>
        ) -> DispatchResult {
            let sender = ensure_signed(origin)?;
            let sender_key = AccountKey::try_from(sender.encode())?;
            let sender_did = Context::current_identity_or::<identity::Module<T>>(&sender_key)?;
            let sender_signer = Signatory::from(sender_did);
            Self::create_or_approve_proposal(multisig, sender_signer, proposal)
        }

        /// Creates a multisig proposal if it hasn't been created or approves it if it has.
        ///
        /// # Arguments
        /// * `multisig` - MultiSig address.
        /// * `proposal` - Proposal to be voted on.
        /// If this is 1 out of `m` multisig, the proposal will be immediately executed.
        #[weight = SimpleDispatchInfo::FixedNormal(750_000)]
        pub fn create_or_approve_proposal_as_key(
            origin,
            multisig: T::AccountId,
            proposal: Box<T::Proposal>
        ) -> DispatchResult {
            let sender = ensure_signed(origin)?;
            let sender_signer = Signatory::from(AccountKey::try_from(sender.encode())?);
            Self::create_or_approve_proposal(multisig, sender_signer, proposal)
        }

        /// Creates a multisig proposal
        ///
        /// # Arguments
        /// * `multisig` - MultiSig address.
        /// * `proposal` - Proposal to be voted on.
        /// If this is 1 out of `m` multisig, the proposal will be immediately executed.
        #[weight = SimpleDispatchInfo::FixedNormal(250_000)]
        pub fn create_proposal_as_identity(origin, multisig: T::AccountId, proposal: Box<T::Proposal>) -> DispatchResult {
            let sender = ensure_signed(origin)?;
            let sender_key = AccountKey::try_from(sender.encode())?;
            let sender_did = Context::current_identity_or::<identity::Module<T>>(&sender_key)?;

            let sender_signer = Signatory::from(sender_did);
            Self::create_proposal(multisig, sender_signer, proposal)?;
            Ok(())
        }

        /// Creates a multisig proposal
        ///
        /// # Arguments
        /// * `multisig` - MultiSig address.
        /// * `proposal` - Proposal to be voted on.
        /// If this is 1 out of `m` multisig, the proposal will be immediately executed.
        #[weight = SimpleDispatchInfo::FixedNormal(250_000)]
        pub fn create_proposal_as_key(origin, multisig: T::AccountId, proposal: Box<T::Proposal>) -> DispatchResult {
            let sender = ensure_signed(origin)?;
            let sender_signer = Signatory::from(AccountKey::try_from(sender.encode())?);
            Self::create_proposal(multisig, sender_signer, proposal)?;
            Ok(())
        }

        /// Approves a multisig proposal using the caller's identity.
        ///
        /// # Arguments
        /// * `multisig` - MultiSig address.
        /// * `proposal_id` - Proposal id to approve.
        /// If quorum is reached, the proposal will be immediately executed.
        #[weight = SimpleDispatchInfo::FixedNormal(750_000)]
        pub fn approve_as_identity(origin, multisig: T::AccountId, proposal_id: u64) -> DispatchResult {
            let sender = ensure_signed(origin)?;
            let sender_key = AccountKey::try_from(sender.encode())?;
            let sender_did = Context::current_identity_or::<identity::Module<T>>(&sender_key)?;
            let signer = Signatory::from(sender_did);
            Self::approve_for(multisig, signer, proposal_id)
        }

        /// Approves a multisig proposal using the caller's signing key (`AccountId`).
        ///
        /// # Arguments
        /// * `multisig` - MultiSig address.
        /// * `proposal_id` - Proposal id to approve.
        /// If quorum is reached, the proposal will be immediately executed.
        #[weight = SimpleDispatchInfo::FixedNormal(750_000)]
        pub fn approve_as_key(origin, multisig: T::AccountId, proposal_id: u64) -> DispatchResult {
            let sender = ensure_signed(origin)?;
            let signer = Signatory::from(AccountKey::try_from(sender.encode())?);
            Self::approve_for(multisig, signer, proposal_id)
        }

        /// Accepts a multisig signer authorization given to signer's identity.
        ///
        /// # Arguments
        /// * `proposal_id` - Auth id of the authorization.
        #[weight = SimpleDispatchInfo::FixedNormal(500_000)]
        pub fn accept_multisig_signer_as_identity(origin, auth_id: u64) -> DispatchResult {
            let sender = ensure_signed(origin)?;
            let sender_key = AccountKey::try_from(sender.encode())?;
            let sender_did = Context::current_identity_or::<identity::Module<T>>(&sender_key)?;

            let signer = Signatory::from(sender_did);
            Self::_accept_multisig_signer(signer, auth_id)
        }

        /// Accepts a multisig signer authorization given to signer's key (AccountId).
        ///
        /// # Arguments
        /// * `proposal_id` - Auth id of the authorization.
        #[weight = SimpleDispatchInfo::FixedNormal(500_000)]
        pub fn accept_multisig_signer_as_key(origin, auth_id: u64) -> DispatchResult {
            let sender = ensure_signed(origin)?;
            let signer = Signatory::from(AccountKey::try_from(sender.encode())?);
            Self::_accept_multisig_signer(signer, auth_id)
        }

        /// Adds a signer to the multisig. This must be called by the multisig itself.
        ///
        /// # Arguments
        /// * `signer` - Signatory to add.
        #[weight = SimpleDispatchInfo::FixedNormal(400_000)]
        pub fn add_multisig_signer(origin, signer: Signatory) -> DispatchResult {
            let sender = ensure_signed(origin)?;
            ensure!(<MultiSigSignsRequired<T>>::contains_key(&sender), Error::<T>::NoSuchMultisig);
            let sender_signer = Signatory::from(AccountKey::try_from(sender.encode())?);
            Self::unsafe_add_auth_for_signers(sender_signer, signer, sender);
            Ok(())
        }

        /// Removes a signer from the multisig. This must be called by the multisig itself.
        ///
        /// # Arguments
        /// * `signer` - Signatory to remove.
        #[weight = SimpleDispatchInfo::FixedNormal(250_000)]
        pub fn remove_multisig_signer(origin, signer: Signatory) -> DispatchResult {
            let sender = ensure_signed(origin)?;
            ensure!(<MultiSigSignsRequired<T>>::contains_key(&sender), Error::<T>::NoSuchMultisig);
            ensure!(<MultiSigSigners<T>>::contains_key(&sender, &signer), Error::<T>::NotASigner);
            ensure!(
                <NumberOfSigners<T>>::get(&sender) > <MultiSigSignsRequired<T>>::get(&sender),
                Error::<T>::NotEnoughSigners
            );
            ensure!(Self::is_changing_signers_allowed(&sender), Error::<T>::ChangeNotAllowed);
            <NumberOfSigners<T>>::mutate(&sender, |x| *x = *x - 1u64);
            Self::unsafe_signer_removal(sender, &signer);
            Ok(())
        }

        /// Adds a signer to the multisig. This must be called by the creator identity of the
        /// multisig.
        ///
        /// # Arguments
        /// * `multisig` - Address of the multi sig
        /// * `signers` - Signatories to add.
        ///
        /// # Weight
        /// `100_000 + 300_000 * signers.len()`
        #[weight = FunctionOf(
            |(_, signers): (
                &T::AccountId,
                &Vec<Signatory>,
            )| {
                100_000 + 300_000 * u32::try_from(signers.len()).unwrap_or_default()
            },
            DispatchClass::Normal,
            true
        )]
        pub fn add_multisig_signers_via_creator(origin, multisig: T::AccountId, signers: Vec<Signatory>) -> DispatchResult {
            let sender = ensure_signed(origin)?;
            ensure!(<MultiSigSignsRequired<T>>::contains_key(&multisig), Error::<T>::NoSuchMultisig);
            let sender_key = AccountKey::try_from(sender.encode())?;
            let sender_did = Context::current_identity_or::<identity::Module<T>>(&sender_key)?;
            ensure!(
                <MultiSigCreator<T>>::get(&multisig) == sender_did,
                Error::<T>::IdentityNotCreator
            );
            let multisig_signer = Signatory::from(AccountKey::try_from(multisig.encode())?);
            for signer in signers {
                Self::unsafe_add_auth_for_signers(multisig_signer, signer, multisig.clone());
            }
            Ok(())
        }

        /// Removes a signer from the multisig.
        /// This must be called by the creator identity of the multisig.
        ///
        /// # Arguments
        /// * `multisig` - Address of the multisig.
        /// * `signers` - Signatories to remove.
        ///
        /// # Weight
        /// `150_000 + 150_000 * signers.len()`
        #[weight = FunctionOf(
            |(_, signers): (
                &T::AccountId,
                &Vec<Signatory>,
            )| {
                150_000 + 150_000 * u32::try_from(signers.len()).unwrap_or_default()
            },
            DispatchClass::Normal,
            true
        )]
        pub fn remove_multisig_signers_via_creator(origin, multisig: T::AccountId, signers: Vec<Signatory>) -> DispatchResult {
            let sender = ensure_signed(origin)?;
            ensure!(<MultiSigSignsRequired<T>>::contains_key(&multisig), Error::<T>::NoSuchMultisig);
            let sender_key = AccountKey::try_from(sender.encode())?;
            let sender_did = Context::current_identity_or::<identity::Module<T>>(&sender_key)?;
            ensure!(
                <MultiSigCreator<T>>::get(&multisig) == sender_did,
                Error::<T>::IdentityNotCreator
            );
            ensure!(Self::is_changing_signers_allowed(&multisig), Error::<T>::ChangeNotAllowed);
            let signers_len:u64 = u64::try_from(signers.len()).unwrap_or_default();

            // NB: the below check can be underflowed but that doesnt matter
            // because the checks in the next loop will fail in that case.
            ensure!(
                <NumberOfSigners<T>>::get(&multisig) - signers_len >= <MultiSigSignsRequired<T>>::get(&multisig),
                Error::<T>::NotEnoughSigners
            );

            for signer in signers {
                ensure!(<MultiSigSigners<T>>::contains_key(&multisig, &signer), Error::<T>::NotASigner);
                Self::unsafe_signer_removal(multisig.clone(), &signer);
            }

            <NumberOfSigners<T>>::mutate(&multisig, |x| *x = *x - signers_len);

            Ok(())
        }

        /// Changes the number of signatures required by a multisig. This must be called by the
        /// multisig itself.
        ///
        /// # Arguments
        /// * `sigs_required` - New number of required signatures.
        #[weight = SimpleDispatchInfo::FixedNormal(150_000)]
        pub fn change_sigs_required(origin, sigs_required: u64) -> DispatchResult {
            let sender = ensure_signed(origin)?;
            ensure!(<MultiSigSignsRequired<T>>::contains_key(&sender), Error::<T>::NoSuchMultisig);
            ensure!(
                <NumberOfSigners<T>>::get(&sender) >= sigs_required,
                Error::<T>::NotEnoughSigners
            );
            ensure!(Self::is_changing_signers_allowed(&sender), Error::<T>::ChangeNotAllowed);
            Self::unsafe_change_sigs_required(sender, sigs_required);
            Ok(())
        }

        /// Replaces all existing signers of the given multisig and changes the number of required
        /// signatures.
        ///
        /// NOTE: Once this function get executed no other function of the multisig is allowed to
        /// execute until unless enough potential signers accept the authorization whose count is
        /// greater than or equal to the number of required signatures.
        ///
        /// # Arguments
        /// * signers - Vector of signers for a given multisig.
        /// * sigs_required - Number of signature required for a given multisig.
        ///
        /// # Weight
        /// `200_000 + 300_000 * signers.len()`
        #[weight = FunctionOf(
            |(signers, _): (
                &Vec<Signatory>,
                &u64
            )| {
                200_000 + 300_000 * u32::try_from(signers.len()).unwrap_or_default()
            },
            DispatchClass::Normal,
            true
        )]
        pub fn change_all_signers_and_sigs_required(origin, signers: Vec<Signatory>, sigs_required: u64) -> DispatchResult {
            let sender = ensure_signed(origin)?;
            let sender_signer = Signatory::from(AccountKey::try_from(sender.encode())?);
            ensure!(<MultiSigSignsRequired<T>>::contains_key(&sender), Error::<T>::NoSuchMultisig);
            ensure!(signers.len() > 0, Error::<T>::NoSigners);
            ensure!(u64::try_from(signers.len()).unwrap_or_default() >= sigs_required && sigs_required > 0,
                Error::<T>::RequiredSignaturesOutOfBounds
            );
            ensure!(Self::is_changing_signers_allowed(&sender), Error::<T>::ChangeNotAllowed);

            // Collect the list of all signers present for the given multisig
            let current_signers = <MultiSigSigners<T>>::iter_prefix(&sender).collect::<Vec<Signatory>>();
            // Collect all those signers who need to be removed. It means those signers that are not exist in the signers vector
            // but present in the current_signers vector
            let old_signers = current_signers.clone().into_iter().filter(|x| !signers.contains(x)).collect::<Vec<Signatory>>();
            // Collect all those signers who need to be added. It means those signers that are not exist in the current_signers vector
            // but present in the signers vector
            let new_signers = signers.into_iter().filter(|x| !current_signers.contains(x)).collect::<Vec<Signatory>>();
            // Removing the signers from the valid multi-signers list first
            old_signers.iter()
                .for_each(|signer| {
                    Self::unsafe_signer_removal(sender.clone(), signer);
                });

            // Add the new signers for the given multi-sig
            new_signers.into_iter()
                .for_each(|signer| {
                    Self::unsafe_add_auth_for_signers(sender_signer, signer, sender.clone())
                });
            // Change the no. of signers for a multisig
            <NumberOfSigners<T>>::mutate(&sender, |x| *x = *x - u64::try_from(old_signers.len()).unwrap_or_default());
            // Change the required signature count
            Self::unsafe_change_sigs_required(sender, sigs_required);

            Ok(())
        }

        /// Adds a multisig as a signer of current did if the current did is the creator of the
        /// multisig.
        ///
        /// # Arguments
        /// * `multi_sig` - multi sig address
        #[weight = SimpleDispatchInfo::FixedNormal(250_000)]
        pub fn make_multisig_signer(origin, multi_sig: T::AccountId) -> DispatchResult {
            let sender = ensure_signed(origin)?;
            ensure!(<MultiSigSignsRequired<T>>::contains_key(&multi_sig), Error::<T>::NoSuchMultisig);
            let sender_key = AccountKey::try_from(sender.encode())?;
            let sender_did = Context::current_identity_or::<identity::Module<T>>(&sender_key)?;
            ensure!(
                <MultiSigCreator<T>>::get(&multi_sig) == sender_did,
                Error::<T>::IdentityNotCreator
            );
            <identity::Module<T>>::unsafe_join_identity(
                sender_did,
                Signatory::from(AccountKey::try_from(multi_sig.encode())?)
            )
        }

        /// Adds a multisig as the master key of the current did if the current did is the creator
        /// of the multisig.
        ///
        /// # Arguments
        /// * `multi_sig` - multi sig address
        #[weight = SimpleDispatchInfo::FixedNormal(250_000)]
        pub fn make_multisig_master(origin, multi_sig: T::AccountId, optional_cdd_auth_id: Option<u64>) -> DispatchResult {
            let sender = ensure_signed(origin)?;
            ensure!(<MultiSigSignsRequired<T>>::contains_key(&multi_sig), Error::<T>::NoSuchMultisig);
            let sender_key = AccountKey::try_from(sender.encode())?;
            let sender_did = Context::current_identity_or::<identity::Module<T>>(&sender_key)?;
            ensure!(
                <MultiSigCreator<T>>::get(&multi_sig) == sender_did,
                Error::<T>::IdentityNotCreator
            );
            <identity::Module<T>>::unsafe_master_key_rotation(
                AccountKey::try_from(multi_sig.encode())?,
                sender_did,
                optional_cdd_auth_id
            )
        }
    }
}

decl_event!(
    pub enum Event<T>
    where
        AccountId = <T as frame_system::Trait>::AccountId,
    {
        /// Event emitted after creation of a multisig. Arguments: multisig address, creator
        /// address, signers (pending approval), signatures required.
        MultiSigCreated(AccountId, AccountId, Vec<Signatory>, u64),
        /// Event emitted after adding a proposal. Argments: multisig, proposal ID.
        ProposalAdded(AccountId, u64),
        /// Event emitted when a proposal is executed. Arguments: multisig, proposal ID, result.
        ProposalExecuted(AccountId, u64, bool),
        /// Event emitted when a signatory is added. Arguments: multisig, added signer.
        MultiSigSignerAdded(AccountId, Signatory),
        /// Event emitted when a multisig signatory is authorized to be added. Arguments: multisig,
        /// authorized signer.
        MultiSigSignerAuthorized(AccountId, Signatory),
        /// Event emitted when a multisig signatory is removed. Arguments: multisig, removed signer.
        MultiSigSignerRemoved(AccountId, Signatory),
        /// Event emitted when the number of required signatures is changed. Arguments: multisig,
        /// new required signatures.
        MultiSigSignaturesRequiredChanged(AccountId, u64),
    }
);

decl_error! {
    /// Multisig module errors.
    pub enum Error for Module<T: Trait> {
        /// The multisig is not attached to a CDD'd identity.
        CddMissing,
        /// The proposal does not exist.
        ProposalMissing,
        /// Multisig address.
        DecodingError,
        /// No signers.
        NoSigners,
        /// Too few or too many required signatures.
        RequiredSignaturesOutOfBounds,
        /// Not a signer.
        NotASigner,
        /// No such multisig.
        NoSuchMultisig,
        /// Not a multisig authorization.
        NotAMultisigAuth,
        /// Not enough signers.
        NotEnoughSigners,
        /// A nonce overflow.
        NonceOverflow,
        /// Already approved.
        AlreadyApproved,
        /// Already a signer.
        AlreadyASigner,
        /// Couldn't charge fee for the transaction.
        FailedToChargeFee,
        /// Identity provided is not the multisig's creator.
        IdentityNotCreator,
        /// Changing multisig parameters not allowed since multisig is a master key.
        ChangeNotAllowed,
        /// Signer is an account key that is already associated with a multisig.
        SignerAlreadyLinked
    }
}

impl<T: Trait> Module<T> {
    /// Adds an authorization for the accountKey to become a signer of multisig.
    fn unsafe_add_auth_for_signers(from: Signatory, target: Signatory, authorizer: T::AccountId) {
        <identity::Module<T>>::add_auth(from, target, AuthorizationData::AddMultiSigSigner, None);
        Self::deposit_event(RawEvent::MultiSigSignerAuthorized(authorizer, target));
    }

    /// Removes a signer from the valid signer list for a given multisig.
    fn unsafe_signer_removal(multisig: T::AccountId, signer: &Signatory) {
        if let Signatory::AccountKey(key) = signer {
            <KeyToMultiSig<T>>::remove(key);
        }
        <MultiSigSigners<T>>::remove(&multisig, signer);
        Self::deposit_event(RawEvent::MultiSigSignerRemoved(multisig, *signer));
    }

    /// Changes the required signature count for a given multisig.
    fn unsafe_change_sigs_required(multisig: T::AccountId, sigs_required: u64) {
        <MultiSigSignsRequired<T>>::insert(&multisig, &sigs_required);
        Self::deposit_event(RawEvent::MultiSigSignaturesRequiredChanged(
            multisig,
            sigs_required,
        ));
    }

    /// Creates a multisig account without precondition checks or emitting an event.
    pub fn create_multisig_account(
        sender: T::AccountId,
        signers: &[Signatory],
        sigs_required: u64,
    ) -> CreateMultisigAccountResult<T> {
        let sender_key = AccountKey::try_from(sender.encode())?;
        let sender_did = Context::current_identity_or::<identity::Module<T>>(&sender_key)?;
        let new_nonce = Self::ms_nonce()
            .checked_add(1)
            .ok_or(Error::<T>::NonceOverflow)?;
        <MultiSigNonce>::put(new_nonce);
        let account_id =
            Self::get_multisig_address(sender, new_nonce).map_err(|_| Error::<T>::DecodingError)?;
        for signer in signers {
            <identity::Module<T>>::add_auth(
                Signatory::from(AccountKey::try_from(account_id.encode())?),
                *signer,
                AuthorizationData::AddMultiSigSigner,
                None,
            );
        }
        <MultiSigSignsRequired<T>>::insert(&account_id, &sigs_required);
        <MultiSigCreator<T>>::insert(&account_id, &sender_did);
        <identity::KeyToIdentityIds>::insert(
            AccountKey::try_from(account_id.encode())?,
            LinkedKeyInfo::Unique(sender_did),
        );
        Ok(account_id)
    }

    /// Creates a new proposal.
    pub fn create_proposal(
        multisig: T::AccountId,
        sender_signer: Signatory,
        proposal: Box<T::Proposal>,
    ) -> CreateProposalResult {
        ensure!(
            <MultiSigSigners<T>>::contains_key(&multisig, &sender_signer),
            Error::<T>::NotASigner
        );
        let proposal_id = Self::ms_tx_done(multisig.clone());
        <Proposals<T>>::insert((multisig.clone(), proposal_id), proposal.clone());
        <ProposalIds<T>>::insert(multisig.clone(), *proposal, proposal_id);
        // Since proposal_ids are always only incremented by 1, they can not overflow.
        let next_proposal_id: u64 = proposal_id + 1u64;
        <MultiSigTxDone<T>>::insert(multisig.clone(), next_proposal_id);
        Self::deposit_event(RawEvent::ProposalAdded(multisig.clone(), proposal_id));
        Self::approve_for(multisig, sender_signer, proposal_id)?;
        Ok(proposal_id)
    }

    /// Creates or approves a multisig proposal.
    pub fn create_or_approve_proposal(
        multisig: T::AccountId,
        sender_signer: Signatory,
        proposal: Box<T::Proposal>,
    ) -> DispatchResult {
        if let Some(proposal_id) = Self::proposal_ids(&multisig, &*proposal) {
            // This is an existing proposal.
            Self::approve_for(multisig, sender_signer, proposal_id)?;
        } else {
            // The proposal is new.
            Self::create_proposal(multisig, sender_signer, proposal)?;
        }
        Ok(())
    }

    /// Approves a multisig proposal and executes it if enough signatures have been received.
    pub fn approve_for(
        multisig: T::AccountId,
        signer: Signatory,
        proposal_id: u64,
    ) -> DispatchResult {
        ensure!(
            <MultiSigSigners<T>>::contains_key(&multisig, &signer),
            Error::<T>::NotASigner
        );
        let multisig_signer_proposal = (multisig.clone(), signer, proposal_id);
        let multisig_proposal = (multisig.clone(), proposal_id);
        ensure!(
            !Self::votes(&multisig_signer_proposal),
            Error::<T>::AlreadyApproved
        );
        if let Some(proposal) = Self::proposals(&multisig_proposal) {
            <Votes<T>>::insert(&multisig_signer_proposal, true);
            // Since approvals are always only incremented by 1, they can not overflow.
            let approvals: u64 = Self::tx_approvals(&multisig_proposal) + 1u64;
            <TxApprovals<T>>::insert(&multisig_proposal, approvals);
            let approvals_needed = Self::ms_signs_required(multisig.clone());
            if approvals >= approvals_needed {
                let ms_key = AccountKey::try_from(multisig.clone().encode())?;
                if let Some(did) = <identity::Module<T>>::get_identity(&ms_key) {
                    ensure!(
                        <identity::Module<T>>::has_valid_cdd(did),
                        Error::<T>::CddMissing
                    );
                    T::CddHandler::set_current_identity(&did);
                } else {
                    let creator_identity = Self::ms_creator(&multisig);
                    ensure!(
                        <identity::Module<T>>::has_valid_cdd(creator_identity),
                        Error::<T>::CddMissing
                    );
                    T::CddHandler::set_current_identity(&creator_identity);
                }
                ensure!(
                    T::ChargeTxFeeTarget::charge_fee(
                        proposal.encode().len().try_into().unwrap_or_default(),
                        proposal.get_dispatch_info(),
                    )
                    .is_ok(),
                    Error::<T>::FailedToChargeFee
                );
                let res = match proposal
                    .dispatch(frame_system::RawOrigin::Signed(multisig.clone()).into())
                {
                    Ok(_) => true,
                    Err(e) => {
                        let e: DispatchError = e;
                        sp_runtime::print(e);
                        false
                    }
                };
                Self::deposit_event(RawEvent::ProposalExecuted(multisig, proposal_id, res));
                Ok(())
            } else {
                Ok(())
            }
        } else {
            Err(Error::<T>::ProposalMissing.into())
        }
    }

    /// Accepts and processed an addition of a signer to a multisig.
    pub fn _accept_multisig_signer(signer: Signatory, auth_id: u64) -> DispatchResult {
        ensure!(
            <identity::Authorizations<T>>::contains_key(signer, auth_id),
            AuthorizationError::Invalid
        );

        let auth = <identity::Authorizations<T>>::get(signer, auth_id);

        ensure!(
            auth.authorization_data == AuthorizationData::AddMultiSigSigner,
            Error::<T>::NotAMultisigAuth
        );

        let wallet_id = {
            if let Signatory::AccountKey(multisig_key) = auth.authorized_by {
                T::AccountId::decode(&mut &multisig_key.as_slice()[..])
                    .map_err(|_| Error::<T>::DecodingError)
            } else {
                Err(Error::<T>::DecodingError)
            }
        }?;

        ensure!(
            <MultiSigSignsRequired<T>>::contains_key(&wallet_id),
            Error::<T>::NoSuchMultisig
        );
        ensure!(
            Self::is_changing_signers_allowed(&wallet_id),
            Error::<T>::ChangeNotAllowed
        );
        ensure!(
            !<MultiSigSigners<T>>::contains_key(&wallet_id, &signer),
            Error::<T>::AlreadyASigner
        );

        if let Signatory::AccountKey(key) = signer {
            ensure!(
                !<KeyToMultiSig<T>>::contains_key(&key),
                Error::<T>::SignerAlreadyLinked
            );
            <KeyToMultiSig<T>>::insert(key, wallet_id.clone())
        }

        let wallet_signer = Signatory::from(AccountKey::try_from(wallet_id.encode())?);
        <identity::Module<T>>::consume_auth(wallet_signer, signer, auth_id)?;

        <MultiSigSigners<T>>::insert(wallet_id.clone(), signer, signer);
        <NumberOfSigners<T>>::mutate(wallet_id.clone(), |x| *x += 1u64);

        Self::deposit_event(RawEvent::MultiSigSignerAdded(wallet_id, signer));

        Ok(())
    }

    /// Gets the next available multisig account ID.
    pub fn get_next_multisig_address(sender: T::AccountId) -> T::AccountId {
        // Nonce is always only incremented by small numbers and hence can never overflow 64 bits.
        // Also, this is just a helper function that does not modify state.
        let new_nonce = Self::ms_nonce() + 1;
        Self::get_multisig_address(sender, new_nonce).unwrap_or_default()
    }

    /// Constructs a multisig account given a nonce.
    pub fn get_multisig_address(
        sender: T::AccountId,
        nonce: u64,
    ) -> Result<T::AccountId, CodecError> {
        let h: T::Hash = T::Hashing::hash(&(b"MULTI_SIG", nonce, sender).encode());
        T::AccountId::decode(&mut &h.encode()[..])
    }

    /// Helper function that checks if someone is an authorized signer of a multisig or not.
    pub fn ms_signers(multi_sig: T::AccountId, signer: Signatory) -> bool {
        <MultiSigSigners<T>>::contains_key(multi_sig, signer)
    }

    /// Checks whether changing the list of signers is allowed in a multisig.
    pub fn is_changing_signers_allowed(multi_sig: &T::AccountId) -> bool {
        if <identity::Module<T>>::cdd_auth_for_master_key_rotation() {
            if let Ok(ms_key) = AccountKey::try_from(multi_sig.clone().encode()) {
                if let Some(did) = <identity::Module<T>>::get_identity(&ms_key) {
                    if ms_key == <identity::Module<T>>::did_records(&did).master_key {
                        return false;
                    }
                }
            } else {
                return false;
            }
        }
        true
    }
}

impl<T: Trait> AddSignerMultiSig for Module<T> {
    fn accept_multisig_signer(signer: Signatory, auth_id: u64) -> DispatchResult {
        Self::_accept_multisig_signer(signer, auth_id)
    }
}
