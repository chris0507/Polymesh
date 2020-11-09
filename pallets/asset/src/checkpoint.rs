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

//! # Checkpoint Module
//!
//! The Checkpoint module provides extrinsics and storage to take snapshots,
//! henceforth called *checkpoints*, of the supply of assets,
//! and how they were distributed at the time of checkpoint.
//!
//! Using the module, users can also schedule checkpoints in the future,
//! either at fixed points in time (e.g., "next friday"),
//! or at regular intervals (e.g., "every month").
//!
//! ## Interface
//!
//! ### Dispatchable Functions
//!
//! - `create_checkpoint` creates a checkpoint.
//! - `set_schedules_max_complexity` sets the max total complexity of a ticker's schedule set.
//! - `create_schedule` creates a checkpoint schedule.
//! - `remove_schedule` removes a checkpoint schedule.
//!
//! ### Public Functions
//!
//! - `balance_at(ticker, did, cp)` returns the balance of `did` for `ticker` at least `>= cp`.
//! - `advance_update_balances(ticker, updates)` advances schedules for `ticker`
//!    and applies new balances in `updates` for the last checkpoint.
//! - Other misc storage items as defined in `decl_storage!`.

use codec::{Decode, Encode};
use core::iter;
use frame_support::{
    decl_error, decl_event, decl_module, decl_storage,
    dispatch::{DispatchError, DispatchResult},
    ensure,
    traits::{Get, UnixTime},
};
use frame_system::ensure_root;
use polymesh_common_utilities::{
    protocol_fee::{ChargeProtocolFee, ProtocolOp},
    CommonTrait, GC_DID,
};
use polymesh_primitives::{
    calendar::{CalendarPeriod, CheckpointId, CheckpointSchedule},
    IdentityId, Moment, Ticker,
};
#[cfg(feature = "std")]
use sp_runtime::{Deserialize, Serialize};
use sp_std::prelude::*;

use crate as pallet_asset;
use pallet_asset::Trait;

type Asset<T> = pallet_asset::Module<T>;

/// ID of a `StoredSchedule`.
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
#[derive(Encode, Decode, Copy, Clone, Debug, PartialEq, Eq, Default)]
pub struct ScheduleId(pub u64);

/// One or more scheduled checkpoints in the future.
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
#[derive(Encode, Decode, Copy, Clone, Debug, PartialEq, Eq)]
pub struct StoredSchedule {
    /// A series of checkpoints in the future defined by the schedule.
    pub schedule: CheckpointSchedule,
    /// The ID of the schedule itself.
    /// Not to be confused for a checkpoint's ID.
    pub id: ScheduleId,
    /// When the next checkpoint is due to be created.
    /// Used as a cache for more efficient sorting.
    pub at: Moment,
}

/// Input specification for a checkpoint schedule.
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
#[derive(Encode, Decode, Copy, Clone, Debug, PartialEq, Eq)]
pub struct ScheduleSpec {
    /// Unix time in milli-seconds.
    /// When `None`, this is an instruction to use the current time.
    pub start: Option<Moment>,
    /// The period at which the checkpoint is set to recur after `start`.
    pub period: CalendarPeriod,
}

/// Create a schedule spec due exactly at the provided `start: Moment` time.
impl From<Moment> for ScheduleSpec {
    fn from(start: Moment) -> Self {
        let period = <_>::default();
        let start = start.into();
        Self { start, period }
    }
}

decl_storage! {
    trait Store for Module<T: Trait> as Checkpoint {
        // --------------------- Supply / Balance storage ----------------------

        /// Total supply of the token at the checkpoint.
        ///
        /// (ticker, checkpointId) -> total supply at given checkpoint
        pub TotalSupply get(fn total_supply_at):
            map hasher(blake2_128_concat) (Ticker, CheckpointId) => T::Balance;

        /// Balance of a DID at a checkpoint.
        ///
        /// (ticker, did, checkpoint ID) -> Balance of a DID at a checkpoint
        pub Balance get(fn balance_at_checkpoint):
            map hasher(blake2_128_concat) (Ticker, IdentityId, CheckpointId) => T::Balance;

        // ------------------------ Checkpoint storage -------------------------

        /// Checkpoints ID generator sequence.
        /// ID of first checkpoint is 1 instead of 0.
        ///
        /// (ticker) -> no. of checkpoints
        pub CheckpointIdSequence get(fn checkpoint_id_sequence):
            map hasher(blake2_128_concat) Ticker => CheckpointId;

        /// Checkpoints where a DID's balance was updated.
        /// (ticker, did) -> [checkpoint ID where user balance changed]
        pub BalanceUpdates get(fn balance_updates):
            map hasher(blake2_128_concat) (Ticker, IdentityId) => Vec<CheckpointId>;

        /// Checkpoint timestamps.
        ///
        /// Every schedule-originated checkpoint maps its ID to its due time.
        /// Every checkpoint manually created maps its ID to the time of recording.
        ///
        /// (checkpoint ID) -> checkpoint timestamp
        pub Timestamps get(fn timestamps):
            map hasher(identity) CheckpointId => Moment;

        // -------------------- Checkpoint Schedule storage --------------------

        /// The maximum complexity allowed for an arbitrary ticker's schedule set
        /// (i.e. `Schedules` storage item below).
        pub SchedulesMaxComplexity get(fn schedules_max_complexity) config(): u64;

        /// Checkpoint schedule ID sequence for tickers.
        ///
        /// (ticker) -> schedule ID
        pub ScheduleIdSequence get(fn schedule_id_sequence):
            map hasher(blake2_128_concat) Ticker => ScheduleId;

        /// Checkpoint schedules for tickers.
        ///
        /// (ticker) -> [schedule]
        pub Schedules get(fn schedules):
            map hasher(blake2_128_concat) Ticker => Vec<StoredSchedule>;

        /// Is the schedule removable?
        ///
        /// (ticker, schedule ID) -> removable?
        pub ScheduleRemovable get(fn schedule_removable):
            map hasher(blake2_128_concat) (Ticker, ScheduleId) => bool;

        /// All the checkpoints a given schedule originated.
        ///
        /// (ticker, schedule ID) -> [checkpoint ID]
        pub SchedulePoints get(fn schedule_points):
            map hasher(blake2_128_concat) (Ticker, ScheduleId) => Vec<CheckpointId>;
    }
}

decl_module! {
    pub struct Module<T: Trait> for enum Call where origin: T::Origin {
        type Error = Error<T>;

        fn deposit_event() = default;

        /// Creates a single checkpoint at the current time.
        ///
        /// # Arguments
        /// - `origin` is a signer that has permissions to act as owner of `ticker`.
        /// - `ticker` to create the checkpoint for.
        ///
        /// # Errors
        /// - `Unauthorized` if the DID of `origin` doesn't own `ticker`.
        /// - `CheckpointOverflow` if the total checkpoint counter would overflow.
        #[weight = T::DbWeight::get().reads_writes(3, 2) + 400_000_000]
        pub fn create_checkpoint(origin, ticker: Ticker) {
            let owner = <Asset<T>>::ensure_perms_owner(origin, &ticker)?;
            Self::create_at_by(owner, ticker, Self::now_unix())?;
        }

        /// Sets the max complexity of a schedule set for an arbitrary ticker to `max_complexity`.
        /// The new maximum is not enforced retroactively,
        /// and only applies once new schedules are made.
        ///
        /// Must be called as a PIP (requires "root").
        ///
        /// # Arguments
        /// - `origin` is the root origin.
        /// - `max_complexity` allowed for an arbitrary ticker's schedule set.
        #[weight = 1_000_000_000]
        pub fn set_schedules_max_complexity(origin, max_complexity: u64) {
            ensure_root(origin)?;
            SchedulesMaxComplexity::put(max_complexity);
            Self::deposit_event(RawEvent::MaximumSchedulesComplexityChanged(GC_DID, max_complexity));
        }

        /// Creates a schedule generating checkpoints
        /// in the future at either a fixed time or at intervals.
        ///
        /// # Arguments
        /// - `origin` is a signer that has permissions to act as owner of `ticker`.
        /// - `ticker` to create the schedule for.
        /// - `schedule` that will generate checkpoints.
        ///
        /// # Errors
        /// - `Unauthorized` if the DID of `origin` doesn't own `ticker`.
        /// - `ScheduleDurationTooShort` if the schedule duration is too short.
        /// - `InsufficientAccountBalance` if the protocol fee could not be charged.
        /// - `ScheduleOverflow` if the schedule ID counter would overflow.
        /// - `CheckpointOverflow` if the total checkpoint counter would overflow.
        /// - `FailedToComputeNextCheckpoint` if the next checkpoint for `schedule` is in the past.
        #[weight = T::DbWeight::get().reads_writes(6, 2) + 1_000_000_000]
        pub fn create_schedule(
            origin,
            ticker: Ticker,
            schedule: ScheduleSpec,
        ) {
            let owner = <Asset<T>>::ensure_perms_owner(origin, &ticker)?;
            Self::create_schedule_base(owner, ticker, schedule, true)?;
        }

        /// Removes the checkpoint schedule of an asset identified by `id`.
        ///
        /// # Arguments
        /// - `origin` is a signer that has permissions to act as owner of `ticker`.
        /// - `ticker` to remove the schedule from.
        /// - `id` of the schedule, when it was created by `created_schedule`.
        ///
        /// # Errors
        /// - `Unauthorized` if the caller doesn't own the asset.
        /// - `NoCheckpointSchedule` if `id` does not identify a schedule for this `ticker`.
        /// - `ScheduleNotRemovable` if `id` exists but is not removable.
        #[weight = T::DbWeight::get().reads_writes(5, 2) + 400_000_000]
        pub fn remove_schedule(
            origin,
            ticker: Ticker,
            id: ScheduleId,
        ) {
            let owner = <Asset<T>>::ensure_perms_owner(origin, &ticker)?;

            // If the ID matches and schedule is removable, it should be removed.
            let schedule_id = (ticker, id);
            let schedule = Schedules::try_mutate(&ticker, |ss| {
                ensure!(ScheduleRemovable::get(schedule_id), Error::<T>::ScheduleNotRemovable);
                ss.iter().position(|s| s.id == id)
                    // By definiton of `position` being `Some(pos), `.remove(pos)` won't panic.
                    .map(|pos| ss.remove(pos))
                    .ok_or(Error::<T>::NoSuchSchedule)
            })?;

            // Remove some additional data.
            // We don't remove historical points related to the schedule.
            ScheduleRemovable::remove(schedule_id);

            // Emit event.
            Self::deposit_event(RawEvent::ScheduleRemoved(owner, ticker, schedule));
        }
    }
}

decl_event! {
    pub enum Event<T>
    where
        Balance = <T as CommonTrait>::Balance,
    {
        /// A checkpoint was created.
        ///
        /// (caller DID, ticker, checkpoint ID, total supply, checkpoint timestamp)
        CheckpointCreated(Option<IdentityId>, Ticker, CheckpointId, Balance, Moment),

        /// The maximum complexity for an arbitrary ticker's schedule set was changed.
        ///
        /// (GC DID, the new maximum)
        MaximumSchedulesComplexityChanged(IdentityId, u64),

        /// A checkpoint schedule was created.
        ///
        /// (caller DID, ticker, schedule)
        ScheduleCreated(IdentityId, Ticker, StoredSchedule),

        /// A checkpoint schedule was removed.
        ///
        /// (caller DID, ticker, schedule)
        ScheduleRemoved(IdentityId, Ticker, StoredSchedule),
    }
}

decl_error! {
    pub enum Error for Module<T: Trait> {
        /// An overflow while calculating the checkpoint ID.
        CheckpointOverflow,
        /// An overflow while calculating the checkpoint schedule ID.
        ScheduleOverflow,
        /// A checkpoint schedule does not exist for the asset.
        NoSuchSchedule,
        /// A checkpoint schedule is not removable.
        ScheduleNotRemovable,
        /// Failed to compute the next checkpoint.
        /// The schedule does not have any upcoming checkpoints.
        FailedToComputeNextCheckpoint,
        /// The duration of a schedule period is too short.
        ScheduleDurationTooShort,
        /// The set of schedules taken together are too complex.
        /// For example, they are too many, or they occurs too frequently.
        SchedulesTooComplex,
    }
}

impl<T: Trait> Module<T> {
    /// Does checkpoint with ID `cp_id` exist for `ticker`?
    pub fn checkpoint_exists(ticker: &Ticker, cp: CheckpointId) -> bool {
        (CheckpointId(1)..=CheckpointIdSequence::get(ticker)).contains(&cp)
    }

    /// Returns the balance of `did` for `ticker` at first checkpoint ID `>= cp`, if any.
    ///
    /// Reasons for returning `None` include:
    /// - `cp` is not a valid checkpoint ID.
    /// - `did` hasn't made transfers in all of `ticker`'s checkpoints.
    /// - `did`'s last transaction was strictly before `cp`, so their balance is the current one.
    ///
    /// N.B. in case of `None`, you likely want the current balance instead.
    /// To compute that, use `Asset::get_balance_at(ticker, did, cp)`, which calls into here.
    pub fn balance_at(ticker: Ticker, did: IdentityId, cp: CheckpointId) -> Option<T::Balance> {
        let ticker_did = (ticker, did);
        if Self::checkpoint_exists(&ticker, cp) && BalanceUpdates::contains_key(&ticker_did) {
            // Checkpoint exists and user has some part in that.
            let balance_updates = BalanceUpdates::get(&ticker_did);
            if cp <= balance_updates.last().copied().unwrap_or(CheckpointId(0)) {
                // Use first checkpoint created after target checkpoint.
                // The user has data for that checkpoint.
                let id = *find_ceiling(&balance_updates, &cp);
                return Some(Self::balance_at_checkpoint((ticker, did, id)));
            }
            // User has not transacted after checkpoint creation.
            // This means their current balance = their balance at that cp.
        }
        None
    }

    /// Advances checkpoints for `ticker`,
    /// and for each DID in `updates`, sets their balance to the one provided.
    pub fn advance_update_balances(
        ticker: &Ticker,
        updates: &[(IdentityId, T::Balance)],
    ) -> DispatchResult {
        Self::advance_schedules(ticker)?;
        Self::update_balances(ticker, updates);
        Ok(())
    }

    /// Updates manual and scheduled checkpoints if those are defined.
    ///
    /// # Assumption
    ///
    /// * When minting, the total supply of `ticker` is updated **after** this function is called.
    fn update_balances(ticker: &Ticker, updates: &[(IdentityId, T::Balance)]) {
        let last_cp = CheckpointIdSequence::get(ticker);
        if last_cp < CheckpointId(1) {
            return;
        }
        for (did, balance) in updates {
            let bal_key = (*ticker, did, last_cp);
            if !<Balance<T>>::contains_key(bal_key) {
                <Balance<T>>::insert(bal_key, balance);
                BalanceUpdates::append((*ticker, did), last_cp);
            }
        }
    }

    /// Advance all checkpoint schedules for `ticker`.
    ///
    /// Complexity: O(max(s, r log s)) where:
    ///  - `s` is the number of schedule for `ticker`.
    ///  - `r`, with `r <= s` is the subset of `s` to reschedule.
    fn advance_schedules(ticker: &Ticker) -> DispatchResult {
        let mut schedules = Schedules::get(ticker);

        // No schedules? => We want to avoid `now_unix()` below for efficiency.
        if schedules.is_empty() {
            return Ok(());
        }

        // Find the first schedule not due. All the schedule before `end` are due.
        let now = Self::now_unix();
        let end = schedules
            .iter()
            .position(|s| s.at > now) // Complexity `O(s)`.
            .unwrap_or(schedules.len());

        if end == 0 {
            // Nothing found means no storage changes, so bail.
            return Ok(());
        };

        // Plan for CP creation for due schedules and rescheduling.
        let mut reschedule = Vec::new();
        let mut create = Vec::with_capacity(end); // Lower bound; might add more.
        for store in schedules.drain(..end) {
            let schedule = store.schedule;
            // If the schedule is recurring, we'll need to reschedule.
            if let Some(at) = schedule.next_checkpoint(now) {
                reschedule.push(StoredSchedule { at, ..store });
            }

            // Plan for all checkpoints for this schedule.
            // There might be more than one.
            // As an example, consider a checkpoint due every day,
            // and then there's a week without any transactions.
            create.extend(
                iter::successors(Some(store.at), |&at| {
                    schedule.next_checkpoint(at).filter(|&at| at <= now)
                })
                .map(|at| (at, store.id)),
            );
        }

        // Ensure that ID count won't overflow.
        // After this we're safe and can commit to storage.
        let (id_last, id_seq) = Self::next_cp_ids(ticker, create.len() as u64)?;

        // Create all checkpoints we planned for.
        for ((at, id), cp_id) in create.into_iter().zip(id_seq) {
            Self::create_at(None, *ticker, cp_id, at);
            SchedulePoints::append((*ticker, id), cp_id);
        }

        // Reschedule schedules we need to.
        // Complexity: `O(r log s)`.
        reschedule
            .into_iter()
            .for_each(|s| add_schedule(&mut schedules, s));

        // Commit changes to `schedules`.
        CheckpointIdSequence::insert(ticker, id_last);
        Schedules::insert(ticker, schedules);
        Ok(())
    }

    /// Creates a schedule generating checkpoints
    /// in the future at either a fixed time or at intervals.
    pub fn create_schedule_base(
        did: IdentityId,
        ticker: Ticker,
        schedule: ScheduleSpec,
        removable: bool,
    ) -> Result<StoredSchedule, DispatchError> {
        let ScheduleSpec { period, start } = schedule;

        // Ensure the total complexity for all schedules is not too great.
        let schedules = Schedules::get(ticker);
        schedules
            .iter()
            .map(|s| s.schedule.period.complexity())
            .try_fold(period.complexity(), |a, c| a.checked_add(c))
            .filter(|&c| c <= SchedulesMaxComplexity::get())
            .ok_or(Error::<T>::SchedulesTooComplex)?;

        // Compute next schedule ID.
        let id = Self::next_schedule_id(&ticker)?;

        // If start is now, we'll create the first checkpoint immediately later at (1).
        let now = Self::now_unix();
        let start = start.unwrap_or(now);
        let cp_id = (start == now)
            .then(|| Self::next_cp_ids(&ticker, 1).map(|(cp_id, _)| cp_id))
            .transpose()?;

        // Compute the next timestamp, if needed.
        // If the start isn't now or this schedule recurs, we'll need to schedule as done in (2).
        let schedule = CheckpointSchedule { start, period };
        let future_at = (cp_id.is_none() || period.amount.is_some())
            .then(|| {
                schedule
                    .next_checkpoint(now)
                    .ok_or(Error::<T>::FailedToComputeNextCheckpoint)
            })
            .transpose()?;

        // Charge the fee for checkpoint creation.
        // N.B. this operation bundles verification + a storage change.
        // Thus, it must be last, and only storage changes follow.
        T::ProtocolFee::charge_fee(ProtocolOp::AssetCreateCheckpointSchedule)?;

        // (1) Start is now, so create the checkpoint.
        if let Some(cp_id) = cp_id {
            CheckpointIdSequence::insert(ticker, cp_id);
            SchedulePoints::append((ticker, id), cp_id);
            Self::create_at(Some(did), ticker, cp_id, now);
        }

        // (2) There will be some future checkpoint, so schedule it.
        let at = future_at.unwrap_or(now);
        let schedule = StoredSchedule { at, id, schedule };
        if let Some(_) = future_at {
            // Store removability + Sort schedule into the queue.
            ScheduleRemovable::insert((ticker, id), removable);
            Schedules::insert(&ticker, {
                let mut schedules = schedules;
                add_schedule(&mut schedules, schedule);
                schedules
            })
        }

        ScheduleIdSequence::insert(ticker, id);
        Self::deposit_event(RawEvent::ScheduleCreated(did, ticker, schedule));
        Ok(schedule)
    }

    /// The `actor` creates a checkpoint at `at` for `ticker`.
    /// The ID of the new checkpoint is returned.
    // TODO(Centril): privatize when dividend module is nixed.
    pub fn create_at_by(
        actor: IdentityId,
        ticker: Ticker,
        at: Moment,
    ) -> Result<CheckpointId, DispatchError> {
        let (id, _) = Self::next_cp_ids(&ticker, 1)?;
        CheckpointIdSequence::insert(ticker, id);
        Self::create_at(Some(actor), ticker, id, at);
        Ok(id)
    }

    /// Creates a checkpoint at `at` for `ticker`, with the given, in advanced reserved, `id`.
    /// The `actor` is the DID creating the checkpoint,
    /// or `None` scheduling created the checkpoint.
    ///
    /// Creating a checkpoint entails:
    /// - recording the total supply,
    /// - mapping the the ID to the `time`.
    fn create_at(actor: Option<IdentityId>, ticker: Ticker, id: CheckpointId, at: Moment) {
        // Record total supply at checkpoint ID.
        let supply = <Asset<T>>::token_details(ticker).total_supply;
        <TotalSupply<T>>::insert(&(ticker, id), supply);

        // Relate ID -> time.
        Timestamps::insert(id, at);

        // Emit event & we're done.
        Self::deposit_event(RawEvent::CheckpointCreated(actor, ticker, id, supply, at));
    }

    /// Verify that `needed` amount of `CheckpointId`s can be reserved,
    /// returning the last ID and an iterator over all IDs to reserve.
    fn next_cp_ids(
        ticker: &Ticker,
        needed: u64,
    ) -> Result<(CheckpointId, impl Iterator<Item = CheckpointId>), DispatchError> {
        let CheckpointId(id) = CheckpointIdSequence::get(ticker);
        id.checked_add(needed)
            .ok_or(Error::<T>::CheckpointOverflow)?;
        let end = CheckpointId(id + needed);
        let seq = (0..needed).map(move |offset| CheckpointId(id + 1 + offset));
        Ok((end, seq))
    }

    /// Compute the next checkpoint schedule ID without changing storage.
    /// ID of first schedule is 1 rather than 0, which means that no schedules have been made yet.
    fn next_schedule_id(ticker: &Ticker) -> Result<ScheduleId, DispatchError> {
        let ScheduleId(id) = ScheduleIdSequence::get(ticker);
        let id = id.checked_add(1).ok_or(Error::<T>::ScheduleOverflow)?;
        Ok(ScheduleId(id))
    }

    /// Returns the current UNIX time, i.e. milli-seconds since UNIX epoch, 1970-01-01 00:00:00 UTC.
    pub fn now_unix() -> Moment {
        // We're OK with truncation here because we'll all be dead before it actually happens.
        T::UnixTime::now().as_millis() as u64
    }
}

/// Add `schedule` to `ss` in its sorted place, assuming `ss` is already sorted.
fn add_schedule(ss: &mut Vec<StoredSchedule>, schedule: StoredSchedule) {
    let Ok(i) | Err(i) = ss.binary_search_by_key(&schedule.at, |s| s.at);
    ss.insert(i, schedule);
}

/// Find the least element `>= key` in `arr`.
///
/// Assumes that key <= last element of the array,
/// the array consists of unique sorted elements,
/// and that array len > 0.
fn find_ceiling<'a, E: Ord>(arr: &'a [E], key: &E) -> &'a E {
    &arr[arr.binary_search(key).map_or_else(|i| i, |i| i)]
}
