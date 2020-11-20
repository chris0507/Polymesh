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

//! # Settlement Module
//!
//! Settlement module manages all kinds of transfers and settlements of assets
//!
//! ## Overview
//!
//! The settlement module provides functionality to settle onchain as well as offchain trades between multiple parties.
//! All trades are settled under venues. A token issuer can allow/block certain venues from settling trades that involve their tokens.
//! An atomic settlement is called an Instruction. An instruction can contain multiple legs. Legs are essentially simple one to one transfers.
//! When an instruction is settled, either all legs are executed successfully or none are. In other words, if one of the leg fails due to
//! compliance failure, all other legs will also fail.
//!
//! An instruction must be authorized by all the counter parties involved for it to be executed.
//! An instruction can be set to automatically execute when all authorizations are received or at a particular block number.
//!
//! Offchain settlements are represented via receipts. If a leg has a receipt attached to it, it will not be executed onchain.
//! All other legs will be executed onchain during settlement.
//!
//! ## Dispatchable Functions
//!
//! - `create_venue` - Registers a new venue.
//! - `add_instruction` - Adds a new instruction.
//! - `affirm_instruction` - Provides affirmation to an existing instruction.
//! - `withdraw_affirmation` - Withdraw an existing affirmation to given instruction.
//! - `reject_instruction` - Rejects an existing instruction.
//! - `claim_receipt` - Claims a signed receipt.
//! - `unclaim_receipt` - Unclaims a previously claimed receipt.
//! - `set_venue_filtering` - Enables or disabled venue filtering for a token.
//! - `allow_venues` - Allows additional venues to create instructions involving an asset.
//! - `disallow_venues` - Revokes permission given to venues for creating instructions involving a particular asset.
//!
#![cfg_attr(not(feature = "std"), no_std)]
#![recursion_limit = "256"]

#[cfg(feature = "runtime-benchmarks")]
pub mod benchmarking;

use codec::{Decode, Encode};
use frame_support::{
    decl_error, decl_event, decl_module, decl_storage,
    dispatch::{DispatchError, DispatchResult, DispatchResultWithPostInfo},
    ensure, storage,
    traits::{
        schedule::{Anon as ScheduleAnon, DispatchTime, LOWEST_PRIORITY},
        Get,
    },
    weights::{PostDispatchInfo, Weight},
    IterableStorageDoubleMap, Parameter, StorageHasher, Twox128,
};
use frame_system::{self as system, ensure_root, RawOrigin};
use pallet_asset as asset;
use pallet_identity::{self as identity, PermissionedCallOriginData};
use polymesh_common_utilities::{
    traits::{
        asset::GAS_LIMIT, identity::Trait as IdentityTrait, portfolio::PortfolioSubTrait,
        CommonTrait,
    },
    with_transaction,
    SystematicIssuers::Settlement as SettlementDID,
};
use polymesh_primitives::{IdentityId, PortfolioId, SecondaryKey, Ticker};
use polymesh_primitives_derive::VecU8StrongTyped;
use sp_runtime::{
    traits::{Dispatchable, Verify, Zero},
    DispatchErrorWithPostInfo,
};
use sp_std::{collections::btree_set::BTreeSet, convert::TryFrom, prelude::*};

type Identity<T> = identity::Module<T>;
type System<T> = frame_system::Module<T>;
type Asset<T> = asset::Module<T>;

pub trait Trait:
    frame_system::Trait + CommonTrait + IdentityTrait + pallet_timestamp::Trait + asset::Trait
{
    /// The overarching event type.
    type Event: From<Event<Self>> + Into<<Self as frame_system::Trait>::Event>;
    /// The maximum number of total legs allowed for a instruction can have.
    type MaxLegsInInstruction: Get<u32>;
    /// Scheduler of settlement instructions.
    type Scheduler: ScheduleAnon<Self::BlockNumber, Self::SchedulerCall, Self::SchedulerOrigin>;
    /// A type for identity-mapping the `Origin` type. Used by the scheduler.
    type SchedulerOrigin: From<RawOrigin<Self::AccountId>>;
    /// A call type for identity-mapping the `Call` enum type. Used by the scheduler.
    type SchedulerCall: Parameter
        + Dispatchable<Origin = <Self as frame_system::Trait>::Origin>
        + From<Call<Self>>;
    /// Weight information for extrinsics in the identity pallet.
    type WeightInfo: WeightInfo;
}

/// A wrapper for VenueDetails
#[derive(
    Decode, Encode, Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, VecU8StrongTyped,
)]
pub struct VenueDetails(Vec<u8>);

/// Status of an instruction
#[derive(Encode, Decode, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum InstructionStatus {
    /// Invalid instruction or details pruned
    Unknown,
    /// Instruction is pending execution
    Pending,
}

impl Default for InstructionStatus {
    fn default() -> Self {
        Self::Unknown
    }
}

/// Type of the venue. Used for offchain filtering.
#[derive(Encode, Decode, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum VenueType {
    /// Default type - used for mixed and unknown types
    Other,
    /// Represents a primary distribution
    Distribution,
    /// Represents an offering/fund raiser
    Sto,
    /// Represents a match making service
    Exchange,
}

impl Default for VenueType {
    fn default() -> Self {
        Self::Other
    }
}

/// Status of a leg
#[derive(Encode, Decode, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum LegStatus<AccountId> {
    /// It is waiting for affirmation
    PendingTokenLock,
    /// It is waiting execution (tokens currently locked)
    ExecutionPending,
    /// receipt used, (receipt signer, receipt uid)
    ExecutionToBeSkipped(AccountId, u64),
}

impl<AccountId> Default for LegStatus<AccountId> {
    fn default() -> Self {
        Self::PendingTokenLock
    }
}

/// Status of an affirmation
#[derive(Encode, Decode, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum AffirmationStatus {
    /// Invalid affirmation
    Unknown,
    /// Pending user's consent
    Pending,
    /// Affirmed by the user
    Affirmed,
    /// Rejected by the user
    Rejected,
}

impl Default for AffirmationStatus {
    fn default() -> Self {
        Self::Unknown
    }
}

/// Type of settlement
#[derive(Encode, Decode, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum SettlementType<BlockNumber> {
    /// Instruction should be settled as soon as all affirmations are received
    SettleOnAffirmation,
    /// Instruction should be settled on a particular block
    SettleOnBlock(BlockNumber),
}

impl<BlockNumber> Default for SettlementType<BlockNumber> {
    fn default() -> Self {
        Self::SettleOnAffirmation
    }
}

/// Details about an instruction
#[derive(Encode, Decode, Default, Clone, PartialEq, Eq, Debug, PartialOrd, Ord)]
pub struct Instruction<Moment, BlockNumber> {
    /// Unique instruction id. It is an auto incrementing number
    pub instruction_id: u64,
    /// Id of the venue this instruction belongs to
    pub venue_id: u64,
    /// Status of the instruction
    pub status: InstructionStatus,
    /// Type of settlement used for this instruction
    pub settlement_type: SettlementType<BlockNumber>,
    /// Date at which this instruction was created
    pub created_at: Option<Moment>,
    /// Date from which this instruction is valid
    pub valid_from: Option<Moment>,
}

/// Details of a leg including the leg id in the instruction
#[derive(Encode, Decode, Default, Clone, PartialEq, Eq, Debug, PartialOrd, Ord)]
pub struct Leg<Balance> {
    /// Portfolio of the sender
    pub from: PortfolioId,
    /// Portfolio of the receiver
    pub to: PortfolioId,
    /// Ticker of the asset being transferred
    pub asset: Ticker,
    /// Amount being transferred
    pub amount: Balance,
}

/// Details about a venue
#[derive(Encode, Decode, Clone, Default, PartialEq, Eq, Debug, PartialOrd, Ord)]
pub struct Venue {
    /// Identity of the venue's creator
    pub creator: IdentityId,
    /// instructions under this venue (Only needed for the UI)
    pub instructions: Vec<u64>,
    /// Additional details about this venue (Only needed for the UI)
    pub details: VenueDetails,
    /// Specifies type of the venue (Only needed for the UI)
    pub venue_type: VenueType,
}

/// Old venue details format. Used only for storage migration.
#[derive(Encode, Decode, Clone, Default, PartialEq, Eq, Debug, PartialOrd, Ord)]
pub struct OldVenue {
    /// Identity of the venue's creator
    pub creator: IdentityId,
    /// instructions under this venue (Only needed for the UI)
    pub instructions: Vec<u64>,
    /// Additional details about this venue (Only needed for the UI)
    pub details: VenueDetails,
}

impl Venue {
    pub fn new(creator: IdentityId, details: VenueDetails, venue_type: VenueType) -> Self {
        Self {
            creator,
            instructions: Vec::new(),
            details,
            venue_type,
        }
    }
}

/// Details about an offchain transaction receipt
#[derive(Encode, Decode, Clone, PartialEq, Eq, Debug, PartialOrd, Ord)]
pub struct Receipt<Balance> {
    /// Unique receipt number set by the signer for their receipts
    pub receipt_uid: u64,
    /// Identity of the sender
    pub from: PortfolioId,
    /// Identity of the receiver
    pub to: PortfolioId,
    /// Ticker of the asset being transferred
    pub asset: Ticker,
    /// Amount being transferred
    pub amount: Balance,
}

/// A wrapper for VenueDetails
#[derive(
    Decode, Encode, Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, VecU8StrongTyped,
)]
pub struct ReceiptMetadata(Vec<u8>);

/// Details about an offchain transaction receipt that a user must input
#[derive(Encode, Decode, Clone, PartialEq, Eq, Debug, PartialOrd, Ord)]
pub struct ReceiptDetails<AccountId, OffChainSignature> {
    /// Unique receipt number set by the signer for their receipts
    pub receipt_uid: u64,
    /// Target leg id
    pub leg_id: u64,
    /// Signer for this receipt
    pub signer: AccountId,
    /// signature confirming the receipt details
    pub signature: OffChainSignature,
    /// Generic text that can be used to attach messages to receipts
    pub metadata: ReceiptMetadata,
}

pub trait WeightInfo {
    fn create_venue(d: u32, u: u32) -> Weight;
    fn update_venue(d: u32) -> Weight;
    fn add_instruction(u: u32) -> Weight;
    fn add_and_affirm_instruction(u: u32) -> Weight;
    fn affirm_instruction() -> Weight;
    fn withdraw_affirmation(u: u32) -> Weight;
    fn reject_instruction() -> Weight;
    fn affirm_with_receipts() -> Weight;
    fn claim_receipt() -> Weight;
    fn unclaim_receipt() -> Weight;
    fn set_venue_filtering() -> Weight;
    fn allow_venues(u: u32) -> Weight;
    fn disallow_venues(u: u32) -> Weight;

    // Some multiple paths based extrinsic.
    // TODO: Will be removed once we get the worst case weight.
    fn set_venue_filtering_disallow() -> Weight;
    fn withdraw_affirmation_with_receipt(u: u32) -> Weight;
    fn add_instruction_with_settle_on_block_type(u: u32) -> Weight;
    fn add_and_affirm_instruction_with_settle_on_block_type(u: u32) -> Weight;
}

impl WeightInfo for () {
    fn create_venue(_d: u32, _u: u32) -> Weight {
        1_000_000_000
    }
    fn update_venue(_d: u32) -> Weight {
        1_000_000_000
    }
    fn add_instruction(_u: u32) -> Weight {
        1_000_000_000
    }
    fn add_and_affirm_instruction(_u: u32) -> Weight {
        1_000_000_000
    }
    fn affirm_instruction() -> Weight {
        1_000_000_000
    }
    fn withdraw_affirmation(_u: u32) -> Weight {
        1_000_000_000
    }
    fn reject_instruction() -> Weight {
        1_000_000_000
    }
    fn affirm_with_receipts() -> Weight {
        1_000_000_000
    }
    fn claim_receipt() -> Weight {
        1_000_000_000
    }
    fn unclaim_receipt() -> Weight {
        1_000_000_000
    }
    fn set_venue_filtering() -> Weight {
        1_000_000_000
    }
    fn allow_venues(_u: u32) -> Weight {
        1_000_000_000
    }
    fn disallow_venues(_u: u32) -> Weight {
        1_000_000_000
    }
    fn set_venue_filtering_disallow() -> Weight {
        1_000_000_000
    }
    fn withdraw_affirmation_with_receipt(_u: u32) -> Weight {
        1_000_000_000
    }
    fn add_instruction_with_settle_on_block_type(_u: u32) -> Weight {
        1_000_000_000
    }
    fn add_and_affirm_instruction_with_settle_on_block_type(_u: u32) -> Weight {
        1_000_000_000
    }
}

pub mod weight_for {
    use super::*;

    pub fn weight_for_execute_instruction_if_no_pending_affirm<T: Trait>(
        weight_for_custodian_transfer: Weight,
    ) -> Weight {
        T::DbWeight::get()
            .reads(4) // Weight for read
            .saturating_add(150_000_000) // General weight
            .saturating_add(weight_for_custodian_transfer) // Weight for custodian transfer
    }

    pub fn weight_for_execute_instruction_if_pending_affirm<T: Trait>() -> Weight {
        T::DbWeight::get()
            .reads_writes(2, 1) // For read and write
            .saturating_add(900_000_000) // Mocking unchecked_release_locks() function weight.
    }

    pub fn weight_for_affirmation_with_receipts<T: Trait>(no_of_receipts: u32) -> Weight {
        T::DbWeight::get()
            .reads_writes(6, 3) // weight for read and write
            .saturating_add((no_of_receipts * 80_000_000).into()) // Weight for receipts.
            .saturating_add(
                T::DbWeight::get()
                    .reads_writes(3, 1)
                    .saturating_mul(no_of_receipts.into()),
            ) // weight for read and write related to receipts.
    }

    pub fn weight_for_affirmation_instruction<T: Trait>() -> Weight {
        T::DbWeight::get()
            .reads_writes(5, 3) // weight for read and writes
            .saturating_add(600_000_000)
    }

    pub fn weight_for_reject_instruction<T: Trait>() -> Weight {
        T::DbWeight::get()
            .reads_writes(3, 2) // weight for read and writes
            .saturating_add(500_000_000) // Lump-sum weight for `unsafe_withdraw_instruction_affirmation()`
    }

    pub fn weight_for_transfer<T: Trait>() -> Weight {
        GAS_LIMIT
            .saturating_mul(
                (<Asset<T>>::max_number_of_tm_extension() * T::MaxLegsInInstruction::get()).into(),
            )
            .saturating_add(70_000_000) // Weight for compliance manager
            .saturating_add(T::DbWeight::get().reads_writes(4, 5)) // Weight for read
            .saturating_add(150_000_000)
    }

    pub fn weight_for_instruction_creation<T: Trait>(no_of_legs: usize) -> Weight {
        T::DbWeight::get()
            .reads_writes(2, 5)
            .saturating_add(u64::try_from(no_of_legs * 50_000_000).unwrap_or_default())
    }
}

decl_event!(
    pub enum Event<T>
    where
        Balance = <T as CommonTrait>::Balance,
        Moment = <T as pallet_timestamp::Trait>::Moment,
        BlockNumber = <T as frame_system::Trait>::BlockNumber,
        AccountId = <T as frame_system::Trait>::AccountId,
    {
        /// A new venue has been created (did, venue_id, details, type)
        VenueCreated(IdentityId, u64, VenueDetails, VenueType),
        /// An existing venue has been updated (did, venue_id, details, type)
        VenueUpdated(IdentityId, u64, VenueDetails, VenueType),
        /// A new instruction has been created
        /// (did, venue_id, instruction_id, settlement_type, valid_from, legs)
        InstructionCreated(
            IdentityId,
            u64,
            u64,
            SettlementType<BlockNumber>,
            Option<Moment>,
            Vec<Leg<Balance>>,
        ),
        /// An instruction has been affirmed (did, portfolio, instruction_id)
        InstructionAffirmed(IdentityId, PortfolioId, u64),
        /// An affirmation has been withdrawn (did, portfolio, instruction_id)
        AffirmationWithdrawn(IdentityId, PortfolioId, u64),
        /// An instruction has been rejected (did, instruction_id)
        InstructionRejected(IdentityId, u64),
        /// A receipt has been claimed (did, instruction_id, leg_id, receipt_uid, signer, receipt metadata)
        ReceiptClaimed(IdentityId, u64, u64, u64, AccountId, ReceiptMetadata),
        /// A receipt has been unclaimed (did, instruction_id, leg_id, receipt_uid, signer)
        ReceiptUnclaimed(IdentityId, u64, u64, u64, AccountId),
        /// Venue filtering has been enabled or disabled for a ticker (did, ticker, filtering_enabled)
        VenueFiltering(IdentityId, Ticker, bool),
        /// Venues added to allow list (did, ticker, vec<venue_id>)
        VenuesAllowed(IdentityId, Ticker, Vec<u64>),
        /// Venues added to block list (did, ticker, vec<venue_id>)
        VenuesBlocked(IdentityId, Ticker, Vec<u64>),
        /// Execution of a leg failed (did, instruction_id, leg_id)
        LegFailedExecution(IdentityId, u64, u64),
        /// Instruction failed execution (did, instruction_id)
        InstructionFailed(IdentityId, u64),
        /// Instruction executed successfully(did, instruction_id)
        InstructionExecuted(IdentityId, u64),
        /// Venue unauthorized by ticker owner (did, Ticker, venue_id)
        VenueUnauthorized(IdentityId, Ticker, u64),
    }
);

decl_error! {
    /// Errors for the Settlement module.
    pub enum Error for Module<T: Trait> {
        /// Venue does not exist.
        InvalidVenue,
        /// Sender does not have required permissions.
        Unauthorized,
        /// No pending affirmation for the provided instruction.
        NoPendingAffirm,
        /// Instruction has not been affirmed.
        InstructionNotAffirmed,
        /// Provided instruction is not pending execution.
        InstructionNotPending,
        /// Provided leg is not pending execution.
        LegNotPending,
        /// Signer is not authorized by the venue.
        UnauthorizedSigner,
        /// Receipt already used.
        ReceiptAlreadyClaimed,
        /// Receipt not used yet.
        ReceiptNotClaimed,
        /// Venue does not have required permissions.
        UnauthorizedVenue,
        /// While affirming the transfer, system failed to lock the assets involved.
        FailedToLockTokens,
        /// Instruction failed to execute.
        InstructionFailed,
        /// Instruction validity has not started yet.
        InstructionWaitingValidity,
        /// Instruction's target settle block reached.
        InstructionSettleBlockPassed,
        /// Instruction waiting for settle block.
        InstructionWaitingSettleBlock,
        /// Offchain signature is invalid.
        InvalidSignature,
        /// Sender and receiver are the same.
        SameSenderReceiver,
        /// Maximum numbers of legs in a instruction > `MaxLegsInInstruction`.
        LegsCountExceededMaxLimit,
        /// Portfolio in receipt does not match with portfolios provided by the user.
        PortfolioMismatch,
        /// The provided settlement block number is in the past and cannot be used by the scheduler.
        SettleOnPastBlock,
        /// Portfolio based actions require at least one portfolio to be provided as input.
        NoPortfolioProvided,
        /// The current instruction affirmation status does not support the requested action.
        UnexpectedAffirmationStatus
    }
}

decl_storage! {
    trait Store for Module<T: Trait> as Settlement {
        /// Info about a venue. venue_id -> venue_details
        pub VenueInfo get(fn venue_info): map hasher(twox_64_concat) u64 => Option<Venue>;
        /// Signers allowed by the venue. (venue_id, signer) -> bool
        VenueSigners get(fn venue_signers): double_map hasher(twox_64_concat) u64, hasher(twox_64_concat) T::AccountId => bool;
        /// Array of venues created by an identity. Only needed for the UI. IdentityId -> Vec<venue_id>
        UserVenues get(fn user_venues): map hasher(twox_64_concat) IdentityId => Vec<u64>;
        /// Details about an instruction. instruction_id -> instruction_details
        InstructionDetails get(fn instruction_details): map hasher(twox_64_concat) u64 => Instruction<T::Moment, T::BlockNumber>;
        /// Legs under an instruction. (instruction_id, leg_id) -> Leg
        InstructionLegs get(fn instruction_legs): double_map hasher(twox_64_concat) u64, hasher(twox_64_concat) u64 => Leg<T::Balance>;
        /// Status of a leg under an instruction. (instruction_id, leg_id) -> LegStatus
        InstructionLegStatus get(fn instruction_leg_status): double_map hasher(twox_64_concat) u64, hasher(twox_64_concat) u64 => LegStatus<T::AccountId>;
        /// Number of affirmations pending before instruction is executed. instruction_id -> affirm_pending
        InstructionAffirmsPending get(fn instruction_affirms_pending): map hasher(twox_64_concat) u64 => u64;
        /// Tracks affirmations received for an instruction. (instruction_id, counter_party) -> AffirmationStatus
        AffirmsReceived get(fn affirms_received): double_map hasher(twox_64_concat) u64, hasher(twox_64_concat) PortfolioId => AffirmationStatus;
        /// Helps a user track their pending instructions and affirmations (only needed for UI).
        /// (counter_party, instruction_id) -> AffirmationStatus
        UserAffirmations get(fn user_affirmations): double_map hasher(twox_64_concat) PortfolioId, hasher(twox_64_concat) u64 => AffirmationStatus;
        /// Tracks redemption of receipts. (signer, receipt_uid) -> receipt_used
        ReceiptsUsed get(fn receipts_used): double_map hasher(twox_64_concat) T::AccountId, hasher(blake2_128_concat) u64 => bool;
        /// Tracks if a token has enabled filtering venues that can create instructions involving their token. Ticker -> filtering_enabled
        VenueFiltering get(fn venue_filtering): map hasher(blake2_128_concat) Ticker => bool;
        /// Venues that are allowed to create instructions involving a particular ticker. Oly used if filtering is enabled.
        /// (ticker, venue_id) -> allowed
        VenueAllowList get(fn venue_allow_list): double_map hasher(blake2_128_concat) Ticker, hasher(twox_64_concat) u64 => bool;
        /// Number of venues in the system (It's one more than the actual number)
        VenueCounter get(fn venue_counter) build(|_| 1u64): u64;
        /// Number of instructions in the system (It's one more than the actual number)
        InstructionCounter get(fn instruction_counter) build(|_| 1u64): u64;
    }
}

decl_module! {
    pub struct Module<T: Trait> for enum Call where origin: <T as frame_system::Trait>::Origin {
        type Error = Error<T>;

        fn deposit_event() = default;

        const MaxLegsInInstruction: u32 = T::MaxLegsInInstruction::get();

        fn on_runtime_upgrade() -> Weight {
            // Delete all settlement data that were stored at a wrong prefix.
            let prefix = Twox128::hash(b"StoCapped");
            storage::unhashed::kill_prefix(&prefix);

            // Set venue counter and instruction counter to 1 so that the id(s) start from 1 instead of 0
            <VenueCounter>::put(1);
            <InstructionCounter>::put(1);

            1_000
        }

        /// Registers a new venue.
        ///
        /// * `details` - Extra details about a venue
        /// * `signers` - Array of signers that are allowed to sign receipts for this venue
        /// * `venue_type` - Type of venue being created
        ///
        /// # Weight
        /// `200_000_000 + 5_000_000 * signers.len()`
        #[weight = 200_000_000 + 5_000_000 * u64::try_from(signers.len()).unwrap_or_default()]
        pub fn create_venue(origin, details: VenueDetails, signers: Vec<T::AccountId>, venue_type: VenueType) -> DispatchResult {
            let did = Identity::<T>::ensure_origin_call_permissions(origin)?.primary_did;
            let venue = Venue::new(did, details, venue_type);
            // NB: Venue counter starts with 1.
            let venue_counter = Self::venue_counter();
            <VenueInfo>::insert(venue_counter, venue.clone());
            for signer in signers {
                <VenueSigners<T>>::insert(venue_counter, signer, true);
            }
            <VenueCounter>::put(venue_counter + 1);
            <UserVenues>::append(did, venue_counter);
            Self::deposit_event(RawEvent::VenueCreated(did, venue_counter, venue.details, venue.venue_type));
            Ok(())
        }

        /// Edit venue details and types.
        /// Both parameters are optional, they will be updated only if Some(value) is provided
        ///
        /// * `venue_id` - ID of the venue to edit
        /// * `details` - Extra details about a venue
        /// * `type` - Type of venue being created
        ///
        /// # Weight
        /// `200_000_000
        #[weight = 200_000_000]
        pub fn update_venue(origin, venue_id: u64, details: Option<VenueDetails>, venue_type: Option<VenueType>) -> DispatchResult {
            let did = Identity::<T>::ensure_origin_call_permissions(origin)?.primary_did;
            // Check if a venue exists and the sender is the creator of the venue
            let mut venue = Self::venue_info(venue_id).ok_or(Error::<T>::InvalidVenue)?;
            ensure!(venue.creator == did, Error::<T>::Unauthorized);
            if let Some(venue_details) = details {
                venue.details = venue_details;
            }
            if let Some(v_type) = venue_type {
                venue.venue_type = v_type;
            }
            <VenueInfo>::insert(&venue_id, venue.clone());
            Self::deposit_event(RawEvent::VenueUpdated(did, venue_id, venue.details, venue.venue_type));
            Ok(())
        }

        /// Adds a new instruction.
        ///
        /// # Arguments
        /// * `venue_id` - ID of the venue this instruction belongs to.
        /// * `settlement_type` - Defines if the instruction should be settled
        ///    immediately after receiving all affirmations or waiting till a specific block.
        /// * `valid_from` - Optional date from which people can interact with this instruction.
        /// * `legs` - Legs included in this instruction.
        ///
        /// # Weight
        /// `950_000_000 + 1_000_000 * legs.len()`
        #[weight = weight_for::weight_for_instruction_creation::<T>(legs.len())]
        pub fn add_instruction(
            origin,
            venue_id: u64,
            settlement_type: SettlementType<T::BlockNumber>,
            valid_from: Option<T::Moment>,
            legs: Vec<Leg<T::Balance>>
        ) -> DispatchResult {
            let did = Identity::<T>::ensure_origin_call_permissions(origin)?.primary_did;
            Self::base_add_instruction(did, venue_id, settlement_type, valid_from, legs)?;
            Ok(())
        }

        /// Adds and affirms a new instruction.
        ///
        /// # Arguments
        /// * `venue_id` - ID of the venue this instruction belongs to.
        /// * `settlement_type` - Defines if the instruction should be settled
        ///    immediately after receiving all affirmations or waiting till a specific block.
        /// * `valid_from` - Optional date from which people can interact with this instruction.
        /// * `legs` - Legs included in this instruction.
        /// * `portfolios` - Portfolios that the sender controls and wants to use in this affirmations.
        #[weight = weight_for::weight_for_instruction_creation::<T>(legs.len())
            + weight_for::weight_for_affirmation_instruction::<T>()
            + weight_for::weight_for_transfer::<T>()
        ]
        pub fn add_and_affirm_instruction(
            origin,
            venue_id: u64,
            settlement_type: SettlementType<T::BlockNumber>,
            valid_from: Option<T::Moment>,
            legs: Vec<Leg<T::Balance>>,
            portfolios: Vec<PortfolioId>
        ) -> DispatchResultWithPostInfo {
            let did = Identity::<T>::ensure_origin_call_permissions(origin.clone())?.primary_did;
            let portfolios_set = portfolios.into_iter().collect::<BTreeSet<_>>();
            let legs_count = legs.len();
            let affirm_instruction_weight = with_transaction(|| {
                let instruction_id = Self::base_add_instruction(did, venue_id, settlement_type, valid_from, legs)?;
                Self::affirm_instruction(origin, instruction_id, portfolios_set.into_iter().collect::<Vec<_>>())
            })?;
            Ok(
                Some(
                    weight_for::weight_for_instruction_creation::<T>(legs_count)
                        .saturating_add(affirm_instruction_weight.actual_weight.unwrap_or_default())
                ).into()
            )
        }

        /// Provide affirmation to an existing instruction.
        ///
        /// # Arguments
        /// * `instruction_id` - Instruction id to affirm.
        /// * `portfolios` - Portfolios that the sender controls and wants to affirm this instruction
        #[weight = weight_for::weight_for_affirmation_instruction::<T>()
            + weight_for::weight_for_transfer::<T>() // Maximum weight for `execute_instruction()`
        ]
        pub fn affirm_instruction(origin, instruction_id: u64, portfolios: Vec<PortfolioId>) -> DispatchResultWithPostInfo {
            match Self::base_affirm_instruction(origin, instruction_id, portfolios) {
                Ok(post_info) => Ok(post_info),
                Err(e) => if e.error == Error::<T>::InstructionFailed.into() || e.error == Error::<T>::UnauthorizedVenue.into() {
                    Ok(e.post_info)
                } else {
                    Err(e)
                }
            }
        }

        /// Withdraw an affirmation for a given instruction.
        ///
        /// # Arguments
        /// * `instruction_id` - Instruction id for that affirmation get withdrawn.
        /// * `portfolios` - Portfolios that the sender controls and wants to withdraw affirmation.
        #[weight = 25_000_000_000]
        pub fn withdraw_affirmation(origin, instruction_id: u64, portfolios: Vec<PortfolioId>) -> DispatchResult {
            let (did, secondary_key) = Self::ensure_origin_perm_and_instruction_validity(origin, instruction_id)?;
            let portfolios_set = portfolios.into_iter().collect::<BTreeSet<_>>();

            // Withdraw an affirmation.
            Self::unsafe_withdraw_instruction_affirmation(did, instruction_id, portfolios_set, secondary_key.as_ref())
        }

        /// Rejects an existing instruction.
        ///
        /// # Arguments
        /// * `instruction_id` - Instruction id to reject.
        /// * `portfolios` - Portfolios that the sender controls and wants them to reject this instruction
        #[weight = weight_for::weight_for_reject_instruction::<T>()
            + weight_for::weight_for_transfer::<T>() // Maximum weight for `execute_instruction()`
        ]
        pub fn reject_instruction(origin, instruction_id: u64, portfolios: Vec<PortfolioId>) -> DispatchResultWithPostInfo {
            let (did, secondary_key) = Self::ensure_origin_perm_and_instruction_validity(origin, instruction_id)?;
            let portfolios_set = portfolios.into_iter().collect::<BTreeSet<_>>();
            ensure!(portfolios_set.len() > 0, Error::<T>::NoPortfolioProvided);

            // If the instruction was affirmed by the portfolio, the affirmation must be withdrawn.
            // The sender must have custodian permission over the portfolio.
            let mut affirmed_portfolios = BTreeSet::new();
            for portfolio in &portfolios_set {
                let user_affirmation_status = Self::user_affirmations(portfolio, instruction_id);
                match user_affirmation_status {
                    AffirmationStatus::Affirmed => { affirmed_portfolios.insert(*portfolio); },
                    AffirmationStatus::Pending => T::Portfolio::ensure_portfolio_custody_and_permission(*portfolio, did, secondary_key.as_ref())?,
                    _ => return Err(Error::<T>::NoPendingAffirm.into())
                };
            }
            Self::unsafe_withdraw_instruction_affirmation(did, instruction_id, affirmed_portfolios, secondary_key.as_ref())?;

            // Updates storage to mark the instruction as rejected.
            for portfolio in portfolios_set {
                <UserAffirmations>::insert(portfolio, instruction_id, AffirmationStatus::Rejected);
                <AffirmsReceived>::insert(instruction_id, portfolio, AffirmationStatus::Rejected);
            }

            // Execute the instruction if it was meant to be executed on affirmation
            let weight_for_instruction_execution = Self::is_instruction_executed(
                Zero::zero(),
                Self::instruction_details(instruction_id).settlement_type,
                instruction_id
            );

            Self::deposit_event(RawEvent::InstructionRejected(did, instruction_id));
            match weight_for_instruction_execution {
                Ok(post_info) => Ok(post_info.actual_weight.map(|w| w.saturating_add(weight_for::weight_for_reject_instruction::<T>())).into()),
                Err(e) => Ok(e.post_info)
            }
        }

        /// Accepts an instruction and claims a signed receipt.
        ///
        /// # Arguments
        /// * `instruction_id` - Target instruction id.
        /// * `leg_id` - Target leg id for the receipt
        /// * `receipt_uid` - Receipt ID generated by the signer.
        /// * `signer` - Signer of the receipt.
        /// * `signed_data` - Signed receipt.
        /// * `portfolios` - Portfolios that the sender controls and wants to accept this instruction with
        #[weight = weight_for::weight_for_affirmation_with_receipts::<T>(u32::try_from(receipt_details.len()).unwrap_or_default())
            + weight_for::weight_for_transfer::<T>() // Maximum weight for `execute_instruction()`
            ]
        pub fn affirm_with_receipts(origin, instruction_id: u64, receipt_details: Vec<ReceiptDetails<T::AccountId, T::OffChainSignature>>, portfolios: Vec<PortfolioId>) -> DispatchResultWithPostInfo {
            match Self::base_affirm_with_receipts(origin, instruction_id, receipt_details, portfolios) {
                Ok(post_info) => Ok(post_info),
                Err(e) => if e == Error::<T>::InstructionFailed.into() || e == Error::<T>::UnauthorizedVenue.into() {
                    Ok(e.post_info)
                }else {
                    Err(e)
                }
            }
        }

        /// Claims a signed receipt.
        ///
        /// # Arguments
        /// * `instruction_id` - Target instruction id for the receipt.
        /// * `leg_id` - Target leg id for the receipt
        /// * `receipt_uid` - Receipt ID generated by the signer.
        /// * `signer` - Signer of the receipt.
        /// * `signed_data` - Signed receipt.
        #[weight = 10_000_000_000]
        pub fn claim_receipt(origin, instruction_id: u64, receipt_details: ReceiptDetails<T::AccountId, T::OffChainSignature>) -> DispatchResult {
            let (primary_did, secondary_key) = Self::ensure_origin_perm_and_instruction_validity(origin, instruction_id)?;
            Self::unsafe_claim_receipt(
                primary_did,
                instruction_id,
                receipt_details,
                secondary_key.as_ref()
            )
        }

        /// Unclaims a previously claimed receipt.
        ///
        /// # Arguments
        /// * `instruction_id` - Target instruction id for the receipt.
        /// * `leg_id` - Target leg id for the receipt
        #[weight = 5_000_000_000]
        pub fn unclaim_receipt(origin, instruction_id: u64, leg_id: u64) -> DispatchResult {
            let (did, secondary_key) = Self::ensure_origin_perm_and_instruction_validity(origin, instruction_id)?;

            if let LegStatus::ExecutionToBeSkipped(signer, receipt_uid) = Self::instruction_leg_status(instruction_id, leg_id) {
                let leg = Self::instruction_legs(instruction_id, leg_id);
                T::Portfolio::ensure_portfolio_custody_and_permission(leg.from, did, secondary_key.as_ref())?;
                // Lock tokens that are part of the leg
                T::Portfolio::lock_tokens(&leg.from, &leg.asset, &leg.amount)?;
                <ReceiptsUsed<T>>::insert(&signer, receipt_uid, false);
                <InstructionLegStatus<T>>::insert(instruction_id, leg_id, LegStatus::ExecutionPending);
                Self::deposit_event(RawEvent::ReceiptUnclaimed(did, instruction_id, leg_id, receipt_uid, signer));
                Ok(())
            } else {
                Err(Error::<T>::ReceiptNotClaimed.into())
            }
        }

        /// Enables or disabled venue filtering for a token.
        ///
        /// # Arguments
        /// * `ticker` - Ticker of the token in question.
        /// * `enabled` - Boolean that decides if the filtering should be enabled.
        #[weight = 200_000_000]
        pub fn set_venue_filtering(origin, ticker: Ticker, enabled: bool) -> DispatchResult {
            let did = <Asset<T>>::ensure_perms_owner_asset(origin, &ticker)?;
            if enabled {
                <VenueFiltering>::insert(ticker, enabled);
            } else {
                <VenueFiltering>::remove(ticker);
            }
            Self::deposit_event(RawEvent::VenueFiltering(did, ticker, enabled));
            Ok(())
        }

        /// Allows additional venues to create instructions involving an asset.
        ///
        /// * `ticker` - Ticker of the token in question.
        /// * `venues` - Array of venues that are allowed to create instructions for the token in question.
        ///
        /// # Weight
        /// `200_000_000 + 500_000 * venues.len()`
        #[weight = 200_000_000 + 500_000 * u64::try_from(venues.len()).unwrap_or_default()]
        pub fn allow_venues(origin, ticker: Ticker, venues: Vec<u64>) -> DispatchResult {
            let did = <Asset<T>>::ensure_perms_owner_asset(origin, &ticker)?;
            for venue in &venues {
                <VenueAllowList>::insert(&ticker, venue, true);
            }
            Self::deposit_event(RawEvent::VenuesAllowed(did, ticker, venues));
            Ok(())
        }

        /// Revokes permission given to venues for creating instructions involving a particular asset.
        ///
        /// * `ticker` - Ticker of the token in question.
        /// * `venues` - Array of venues that are no longer allowed to create instructions for the token in question.
        ///
        /// # Weight
        /// `200_000_000 + 500_000 * venues.len()`
        #[weight = 200_000_000 + 500_000 * u64::try_from(venues.len()).unwrap_or_default()]
        pub fn disallow_venues(origin, ticker: Ticker, venues: Vec<u64>) -> DispatchResult {
            let did = <Asset<T>>::ensure_perms_owner_asset(origin, &ticker)?;
            for venue in &venues {
                <VenueAllowList>::remove(&ticker, venue);
            }
            Self::deposit_event(RawEvent::VenuesBlocked(did, ticker, venues));
            Ok(())
        }

        /// An internal call to execute a scheduled settlement instruction.
        #[weight = 500_000_000]
        fn execute_scheduled_instruction(origin, instruction_id: u64) -> DispatchResultWithPostInfo {
            ensure_root(origin)?;
            Self::execute_instruction(instruction_id).1
        }
    }
}

impl<T: Trait> Module<T> {
    /// Ensure origin call permission and the given instruction validity.
    fn ensure_origin_perm_and_instruction_validity(
        origin: <T as frame_system::Trait>::Origin,
        instruction_id: u64,
    ) -> Result<(IdentityId, Option<SecondaryKey<T::AccountId>>), DispatchError> {
        let PermissionedCallOriginData {
            primary_did,
            secondary_key,
            ..
        } = Identity::<T>::ensure_origin_call_permissions(origin)?;
        Self::ensure_instruction_validity(instruction_id)?;
        Ok((primary_did, secondary_key))
    }

    pub fn base_add_instruction(
        did: IdentityId,
        venue_id: u64,
        settlement_type: SettlementType<T::BlockNumber>,
        valid_from: Option<T::Moment>,
        legs: Vec<Leg<T::Balance>>,
    ) -> Result<u64, DispatchError> {
        // Check whether the no. of legs within the limit or not.
        ensure!(
            u32::try_from(legs.len()).unwrap_or_default() <= T::MaxLegsInInstruction::get(),
            Error::<T>::LegsCountExceededMaxLimit
        );

        // Ensure that the scheduled block number is in the future so that T::Scheduler::schedule
        // doesn't fail.
        if let SettlementType::SettleOnBlock(block_number) = &settlement_type {
            ensure!(
                *block_number > System::<T>::block_number(),
                Error::<T>::SettleOnPastBlock
            );
        }

        // Check if a venue exists and the sender is the creator of the venue
        let mut venue = Self::venue_info(venue_id).ok_or(Error::<T>::InvalidVenue)?;
        ensure!(venue.creator == did, Error::<T>::Unauthorized);

        // Prepare data to store in storage
        // NB Instruction counter starts from 1
        let instruction_counter = Self::instruction_counter();
        let mut counter_parties = BTreeSet::new();
        let mut tickers = BTreeSet::new();
        // This is done to create a list of unique CP and tickers involved in the instruction.
        for leg in &legs {
            ensure!(leg.from != leg.to, Error::<T>::SameSenderReceiver);
            counter_parties.insert(leg.from);
            counter_parties.insert(leg.to);
            tickers.insert(leg.asset);
        }

        // Check if the venue has required permissions from token owners
        for ticker in &tickers {
            if Self::venue_filtering(ticker) {
                ensure!(
                    Self::venue_allow_list(ticker, venue_id),
                    Error::<T>::UnauthorizedVenue
                );
            }
        }

        let instruction = Instruction {
            instruction_id: instruction_counter,
            venue_id,
            status: InstructionStatus::Pending,
            settlement_type,
            created_at: Some(<pallet_timestamp::Module<T>>::get()),
            valid_from,
        };

        // write data to storage
        for counter_party in &counter_parties {
            <UserAffirmations>::insert(
                counter_party,
                instruction_counter,
                AffirmationStatus::Pending,
            );
        }

        for (i, leg) in legs.iter().enumerate() {
            <InstructionLegs<T>>::insert(
                instruction_counter,
                u64::try_from(i).unwrap_or_default(),
                leg.clone(),
            );
        }

        if let SettlementType::SettleOnBlock(block_number) = settlement_type {
            let call = Call::<T>::execute_scheduled_instruction(instruction_counter).into();
            T::Scheduler::schedule(
                DispatchTime::At(block_number),
                None,
                LOWEST_PRIORITY,
                RawOrigin::Root.into(),
                call,
            )?;
        }

        <InstructionDetails<T>>::insert(instruction_counter, instruction);
        <InstructionAffirmsPending>::insert(
            instruction_counter,
            u64::try_from(counter_parties.len()).unwrap_or_default(),
        );
        venue.instructions.push(instruction_counter);
        <VenueInfo>::insert(venue_id, venue);
        <InstructionCounter>::put(instruction_counter + 1);
        Self::deposit_event(RawEvent::InstructionCreated(
            did,
            venue_id,
            instruction_counter,
            settlement_type,
            valid_from,
            legs,
        ));
        Ok(instruction_counter)
    }

    fn unsafe_withdraw_instruction_affirmation(
        did: IdentityId,
        instruction_id: u64,
        portfolios: BTreeSet<PortfolioId>,
        secondary_key: Option<&SecondaryKey<T::AccountId>>,
    ) -> DispatchResult {
        // checks custodianship of portfolios and affirmation status
        Self::ensure_portfolios_and_affirmation_status(
            instruction_id,
            &portfolios,
            did,
            secondary_key,
            &[AffirmationStatus::Affirmed],
        )?;
        // Unlock tokens that were previously locked during the affirmation
        let legs = <InstructionLegs<T>>::iter_prefix(instruction_id);
        for (leg_id, leg_details) in
            legs.filter(|(_leg_id, leg_details)| portfolios.contains(&leg_details.from))
        {
            match Self::instruction_leg_status(instruction_id, leg_id) {
                LegStatus::ExecutionToBeSkipped(signer, receipt_uid) => {
                    // Receipt was claimed for this instruction. Therefore, no token unlocking is required, we just unclaim the receipt.
                    <ReceiptsUsed<T>>::insert(&signer, receipt_uid, false);
                    Self::deposit_event(RawEvent::ReceiptUnclaimed(
                        did,
                        instruction_id,
                        leg_id,
                        receipt_uid,
                        signer,
                    ));
                }
                LegStatus::ExecutionPending => {
                    // Tokens are unlocked, need to be unlocked
                    T::Portfolio::unlock_tokens(
                        &leg_details.from,
                        &leg_details.asset,
                        &leg_details.amount,
                    )?;
                }
                LegStatus::PendingTokenLock => {
                    return Err(Error::<T>::InstructionNotAffirmed.into())
                }
            };
            <InstructionLegStatus<T>>::insert(instruction_id, leg_id, LegStatus::PendingTokenLock);
        }

        // Updates storage
        for portfolio in &portfolios {
            <UserAffirmations>::insert(portfolio, instruction_id, AffirmationStatus::Pending);
            <AffirmsReceived>::remove(instruction_id, portfolio);
            Self::deposit_event(RawEvent::AffirmationWithdrawn(
                did,
                *portfolio,
                instruction_id,
            ));
        }

        <InstructionAffirmsPending>::mutate(instruction_id, |affirms_pending| {
            *affirms_pending += u64::try_from(portfolios.len()).unwrap_or_default()
        });

        Ok(())
    }

    fn ensure_instruction_validity(instruction_id: u64) -> DispatchResult {
        let instruction_details = Self::instruction_details(instruction_id);
        ensure!(
            instruction_details.status == InstructionStatus::Pending,
            Error::<T>::InstructionNotPending
        );
        if let Some(valid_from) = instruction_details.valid_from {
            #[cfg(not(feature = "runtime-benchmarks"))]
            ensure!(
                <pallet_timestamp::Module<T>>::get() >= valid_from,
                Error::<T>::InstructionWaitingValidity
            );
        }
        if let SettlementType::SettleOnBlock(block_number) = instruction_details.settlement_type {
            ensure!(
                block_number > system::Module::<T>::block_number(),
                Error::<T>::InstructionSettleBlockPassed
            );
        }
        Ok(())
    }

    fn execute_instruction(instruction_id: u64) -> (u32, DispatchResultWithPostInfo) {
        let legs = <InstructionLegs<T>>::iter_prefix(instruction_id).collect::<Vec<_>>();
        let instructions_processed: u32 = u32::try_from(legs.len()).unwrap_or_default();
        Self::unchecked_release_locks(instruction_id, &legs);
        let mut result = DispatchResult::Ok(());
        let weight_for_execution = if Self::instruction_affirms_pending(instruction_id) > 0 {
            // Instruction rejected. Unlock any locked tokens and mark receipts as unused.
            // NB: Leg status is not updated because Instruction related details are deleted after settlement in any case.
            Self::deposit_event(RawEvent::InstructionRejected(
                SettlementDID.as_id(),
                instruction_id,
            ));
            result = DispatchResult::Err(Error::<T>::InstructionFailed.into());
            weight_for::weight_for_execute_instruction_if_pending_affirm::<T>()
        } else {
            let mut transaction_weight = 0;
            // Verify that the venue still has the required permissions for the tokens involved.
            let tickers: BTreeSet<Ticker> = legs.iter().map(|leg| leg.1.asset).collect();
            let venue_id = Self::instruction_details(instruction_id).venue_id;
            for ticker in &tickers {
                if Self::venue_filtering(ticker) && !Self::venue_allow_list(ticker, venue_id) {
                    Self::deposit_event(RawEvent::VenueUnauthorized(
                        SettlementDID.as_id(),
                        *ticker,
                        venue_id,
                    ));
                    result = DispatchResult::Err(Error::<T>::UnauthorizedVenue.into());
                }
            }

            if result.is_ok() {
                match with_transaction(|| {
                    for (leg_id, leg_details) in legs.iter().filter(|(leg_id, _)| {
                        let status = Self::instruction_leg_status(instruction_id, leg_id);
                        status == LegStatus::ExecutionPending
                    }) {
                        let result = <Asset<T>>::base_transfer(
                            leg_details.from,
                            leg_details.to,
                            &leg_details.asset,
                            leg_details.amount,
                        );
                        if let Ok(post_info) = result {
                            transaction_weight += post_info.actual_weight.unwrap_or_default();
                        } else {
                            return Err(leg_id);
                        }
                    }
                    Ok(())
                }) {
                    Ok(_) => {
                        Self::deposit_event(RawEvent::InstructionExecuted(
                            SettlementDID.as_id(),
                            instruction_id,
                        ));
                    }
                    Err(leg_id) => {
                        Self::deposit_event(RawEvent::LegFailedExecution(
                            SettlementDID.as_id(),
                            instruction_id,
                            leg_id.clone(),
                        ));
                        Self::deposit_event(RawEvent::InstructionFailed(
                            SettlementDID.as_id(),
                            instruction_id,
                        ));
                        result = DispatchResult::Err(Error::<T>::InstructionFailed.into());
                    }
                }
            }
            weight_for::weight_for_execute_instruction_if_no_pending_affirm::<T>(transaction_weight)
        };

        // Clean up instruction details to reduce chain bloat
        <InstructionLegs<T>>::remove_prefix(instruction_id);
        <InstructionDetails<T>>::remove(instruction_id);
        <InstructionLegStatus<T>>::remove_prefix(instruction_id);
        InstructionAffirmsPending::remove(instruction_id);
        AffirmsReceived::remove_prefix(instruction_id);
        Self::prune_user_affirmations(&legs, instruction_id);

        let post_info = PostDispatchInfo {
            actual_weight: Some(weight_for_execution.saturating_add(T::DbWeight::get().writes(5))),
            pays_fee: Default::default(),
        };
        (
            instructions_processed,
            result
                .map(|_| post_info)
                .map_err(|error| DispatchErrorWithPostInfo { post_info, error }),
        )
    }

    fn prune_user_affirmations(legs: &Vec<(u64, Leg<T::Balance>)>, instruction_id: u64) {
        // We remove duplicates in memory before triggering storage actions
        let mut counter_parties = Vec::with_capacity(legs.len() * 2);
        for (_, leg) in legs {
            counter_parties.push(leg.from);
            counter_parties.push(leg.to);
        }
        counter_parties.sort();
        counter_parties.dedup();
        for counter_party in counter_parties {
            UserAffirmations::remove(counter_party, instruction_id);
        }
    }

    pub fn unsafe_affirm_instruction(
        did: IdentityId,
        instruction_id: u64,
        portfolios: BTreeSet<PortfolioId>,
        secondary_key: Option<&SecondaryKey<T::AccountId>>,
    ) -> DispatchResult {
        // checks portfolio's custodian and if it is a counter party with a pending or rejected affirmation
        Self::ensure_portfolios_and_affirmation_status(
            instruction_id,
            &portfolios,
            did,
            secondary_key,
            &[AffirmationStatus::Pending, AffirmationStatus::Rejected],
        )?;

        with_transaction(|| {
            let legs = <InstructionLegs<T>>::iter_prefix(instruction_id);
            for (leg_id, leg_details) in
                legs.filter(|(_leg_id, leg_details)| portfolios.contains(&leg_details.from))
            {
                if T::Portfolio::lock_tokens(
                    &leg_details.from,
                    &leg_details.asset,
                    &leg_details.amount,
                )
                .is_err()
                {
                    // rustc fails to infer return type of `with_transaction` if you use ?/map_err here
                    return Err(DispatchError::from(Error::<T>::FailedToLockTokens));
                }
                <InstructionLegStatus<T>>::insert(
                    instruction_id,
                    leg_id,
                    LegStatus::ExecutionPending,
                );
            }
            Ok(())
        })?;

        let affirms_pending = Self::instruction_affirms_pending(instruction_id);

        // Updates storage
        for portfolio in &portfolios {
            <UserAffirmations>::insert(portfolio, instruction_id, AffirmationStatus::Affirmed);
            <AffirmsReceived>::insert(instruction_id, portfolio, AffirmationStatus::Affirmed);
            Self::deposit_event(RawEvent::InstructionAffirmed(
                did,
                *portfolio,
                instruction_id,
            ));
        }
        <InstructionAffirmsPending>::insert(
            instruction_id,
            affirms_pending.saturating_sub(u64::try_from(portfolios.len()).unwrap_or_default()),
        );

        Ok(())
    }

    fn unsafe_claim_receipt(
        did: IdentityId,
        instruction_id: u64,
        receipt_details: ReceiptDetails<T::AccountId, T::OffChainSignature>,
        secondary_key: Option<&SecondaryKey<T::AccountId>>,
    ) -> DispatchResult {
        Self::ensure_instruction_validity(instruction_id)?;

        ensure!(
            Self::instruction_leg_status(instruction_id, receipt_details.leg_id)
                == LegStatus::ExecutionPending,
            Error::<T>::LegNotPending
        );
        let venue_id = Self::instruction_details(instruction_id).venue_id;
        ensure!(
            Self::venue_signers(venue_id, &receipt_details.signer),
            Error::<T>::UnauthorizedSigner
        );
        ensure!(
            !Self::receipts_used(&receipt_details.signer, receipt_details.receipt_uid),
            Error::<T>::ReceiptAlreadyClaimed
        );

        let leg = Self::instruction_legs(instruction_id, receipt_details.leg_id);

        T::Portfolio::ensure_portfolio_custody_and_permission(leg.from, did, secondary_key)?;

        let msg = Receipt {
            receipt_uid: receipt_details.receipt_uid,
            from: leg.from,
            to: leg.to,
            asset: leg.asset,
            amount: leg.amount,
        };

        ensure!(
            receipt_details
                .signature
                .verify(&msg.encode()[..], &receipt_details.signer),
            Error::<T>::InvalidSignature
        );

        T::Portfolio::unlock_tokens(&leg.from, &leg.asset, &leg.amount)?;

        <ReceiptsUsed<T>>::insert(&receipt_details.signer, receipt_details.receipt_uid, true);

        <InstructionLegStatus<T>>::insert(
            instruction_id,
            receipt_details.leg_id,
            LegStatus::ExecutionToBeSkipped(
                receipt_details.signer.clone(),
                receipt_details.receipt_uid,
            ),
        );
        Self::deposit_event(RawEvent::ReceiptClaimed(
            did,
            instruction_id,
            receipt_details.leg_id,
            receipt_details.receipt_uid,
            receipt_details.signer,
            receipt_details.metadata,
        ));
        Ok(())
    }

    fn unchecked_release_locks(instruction_id: u64, legs: &Vec<(u64, Leg<T::Balance>)>) {
        for (leg_id, leg_details) in legs.iter() {
            match Self::instruction_leg_status(instruction_id, leg_id) {
                LegStatus::ExecutionToBeSkipped(signer, receipt_uid) => {
                    <ReceiptsUsed<T>>::insert(&signer, receipt_uid, false);
                    Self::deposit_event(RawEvent::ReceiptUnclaimed(
                        SettlementDID.as_id(),
                        instruction_id,
                        *leg_id,
                        receipt_uid,
                        signer,
                    ));
                }
                LegStatus::ExecutionPending => {
                    // This can never return an error since the settlement module
                    // must've locked these tokens when instruction was affirmed
                    T::Portfolio::unlock_tokens(
                        &leg_details.from,
                        &leg_details.asset,
                        &leg_details.amount,
                    )
                    .ok();
                }
                LegStatus::PendingTokenLock => {}
            }
        }
    }

    fn is_instruction_executed(
        affirms_pending: u64,
        settlement_type: SettlementType<T::BlockNumber>,
        id: u64,
    ) -> DispatchResultWithPostInfo {
        let execute_instruction_result =
            if affirms_pending == 0 && settlement_type == SettlementType::SettleOnAffirmation {
                Self::execute_instruction(id).1
            } else {
                Ok(PostDispatchInfo {
                    actual_weight: Some(Zero::zero()),
                    pays_fee: Default::default(),
                })
            };
        execute_instruction_result
    }

    pub fn base_affirm_with_receipts(
        origin: <T as frame_system::Trait>::Origin,
        instruction_id: u64,
        receipt_details: Vec<ReceiptDetails<T::AccountId, T::OffChainSignature>>,
        portfolios: Vec<PortfolioId>,
    ) -> DispatchResultWithPostInfo {
        let (did, secondary_key) =
            Self::ensure_origin_perm_and_instruction_validity(origin, instruction_id)?;
        let portfolios_set = portfolios.into_iter().collect::<BTreeSet<_>>();

        // Verify that the receipts provided are unique
        let receipt_ids = receipt_details
            .iter()
            .map(|receipt| (receipt.signer.clone(), receipt.receipt_uid))
            .collect::<BTreeSet<_>>();

        ensure!(
            receipt_ids.len() == receipt_details.len(),
            Error::<T>::ReceiptAlreadyClaimed
        );

        let instruction_details = Self::instruction_details(instruction_id);

        // verify portfolio custodianship and check if it is a counter party with a pending or rejected affirmation
        Self::ensure_portfolios_and_affirmation_status(
            instruction_id,
            &portfolios_set,
            did,
            secondary_key.as_ref(),
            &[AffirmationStatus::Pending, AffirmationStatus::Rejected],
        )?;

        // Verify that the receipts are valid
        for receipt in &receipt_details {
            ensure!(
                Self::venue_signers(&instruction_details.venue_id, &receipt.signer),
                Error::<T>::UnauthorizedSigner
            );
            ensure!(
                !Self::receipts_used(&receipt.signer, &receipt.receipt_uid),
                Error::<T>::ReceiptAlreadyClaimed
            );

            let leg = Self::instruction_legs(&instruction_id, &receipt.leg_id);
            ensure!(
                portfolios_set.contains(&leg.from),
                Error::<T>::PortfolioMismatch
            );

            let msg = Receipt {
                receipt_uid: receipt.receipt_uid,
                from: leg.from,
                to: leg.to,
                asset: leg.asset,
                amount: leg.amount,
            };

            ensure!(
                receipt.signature.verify(&msg.encode()[..], &receipt.signer),
                Error::<T>::InvalidSignature
            );
        }

        // Lock tokens that do not have a receipt attached to their leg.
        with_transaction(|| {
            let legs = <InstructionLegs<T>>::iter_prefix(instruction_id);
            for (leg_id, leg_details) in
                legs.filter(|(_leg_id, leg_details)| portfolios_set.contains(&leg_details.from))
            {
                // Receipt for the leg was provided
                if let Some(receipt) = receipt_details
                    .iter()
                    .find(|receipt| receipt.leg_id == leg_id)
                {
                    <InstructionLegStatus<T>>::insert(
                        instruction_id,
                        leg_id,
                        LegStatus::ExecutionToBeSkipped(
                            receipt.signer.clone(),
                            receipt.receipt_uid,
                        ),
                    );
                } else {
                    if T::Portfolio::lock_tokens(
                        &leg_details.from,
                        &leg_details.asset,
                        &leg_details.amount,
                    )
                    .is_err()
                    {
                        // rustc fails to infer return type of `with_transaction` if you use ?/map_err here
                        return Err(DispatchError::from(Error::<T>::FailedToLockTokens));
                    }
                    <InstructionLegStatus<T>>::insert(
                        instruction_id,
                        leg_id,
                        LegStatus::ExecutionPending,
                    );
                }
            }
            Ok(())
        })?;

        // Update storage
        let affirms_pending = Self::instruction_affirms_pending(instruction_id)
            .saturating_sub(u64::try_from(portfolios_set.len()).unwrap_or_default());
        for portfolio in portfolios_set {
            <UserAffirmations>::insert(portfolio, instruction_id, AffirmationStatus::Affirmed);
            <AffirmsReceived>::insert(instruction_id, portfolio, AffirmationStatus::Affirmed);
            Self::deposit_event(RawEvent::InstructionAffirmed(
                did,
                portfolio,
                instruction_id,
            ));
        }

        <InstructionAffirmsPending>::insert(instruction_id, affirms_pending);
        for receipt in &receipt_details {
            <ReceiptsUsed<T>>::insert(&receipt.signer, receipt.receipt_uid, true);
            Self::deposit_event(RawEvent::ReceiptClaimed(
                did,
                instruction_id,
                receipt.leg_id,
                receipt.receipt_uid,
                receipt.signer.clone(),
                receipt.metadata.clone(),
            ));
        }

        // Execute instruction if conditions are met.
        let execute_instruction_weight = Self::is_instruction_executed(
            affirms_pending,
            instruction_details.settlement_type,
            instruction_id,
        );

        execute_instruction_weight.map(|info| {
            info.actual_weight
                .map(|w| {
                    w.saturating_add(weight_for::weight_for_affirmation_with_receipts::<T>(
                        u32::try_from(receipt_details.len()).unwrap_or_default(),
                    ))
                })
                .into()
        })
    }

    pub fn base_affirm_instruction(
        origin: <T as frame_system::Trait>::Origin,
        instruction_id: u64,
        portfolios: Vec<PortfolioId>,
    ) -> DispatchResultWithPostInfo {
        let (did, secondary_key) =
            Self::ensure_origin_perm_and_instruction_validity(origin, instruction_id)?;
        let portfolios_set = portfolios.into_iter().collect::<BTreeSet<_>>();

        // Provide affirmation to the instruction
        Self::unsafe_affirm_instruction(
            did,
            instruction_id,
            portfolios_set,
            secondary_key.as_ref(),
        )?;

        // Execute the instruction if conditions are met
        let affirms_pending = Self::instruction_affirms_pending(instruction_id);
        let weight_for_instruction_execution = Self::is_instruction_executed(
            affirms_pending,
            Self::instruction_details(instruction_id).settlement_type,
            instruction_id,
        );

        weight_for_instruction_execution.map(|info| {
            info.actual_weight
                .map(|w| w.saturating_add(weight_for::weight_for_affirmation_instruction::<T>()))
                .into()
        })
    }

    fn ensure_portfolios_and_affirmation_status(
        instruction_id: u64,
        portfolios: &BTreeSet<PortfolioId>,
        custodian: IdentityId,
        secondary_key: Option<&SecondaryKey<T::AccountId>>,
        expected_statuses: &[AffirmationStatus],
    ) -> DispatchResult {
        for portfolio in portfolios {
            T::Portfolio::ensure_portfolio_custody_and_permission(
                *portfolio,
                custodian,
                secondary_key,
            )?;
            let user_affirmation = Self::user_affirmations(portfolio, instruction_id);
            ensure!(
                expected_statuses.contains(&user_affirmation),
                Error::<T>::UnexpectedAffirmationStatus
            );
        }
        Ok(())
    }
}
