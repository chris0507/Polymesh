// Copyright 2019-2020 Parity Technologies (UK) Ltd.
// This file is part of Substrate.

// Substrate is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Substrate is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Substrate.  If not, see <http://www.gnu.org/licenses/>.

// # Modified by Polymath Inc - 23rd March 2020
// This module is inspired by the `membership` module of the substrate framework
// https://github.com/PolymeshAssociation/substrate/tree/a439a7aa5a9a3df2a42d9b25ea04288d3a0866e8/frame/membership
// It get customize as per the Polymesh requirements
// - Change member type from `AccountId` to `IdentityId`.
// - Remove `change_key` function from the implementation in the favour of "User can hold only single identity on Polymesh blockchain".
// - Remove the logic of prime member logic that led the removal of `set_prime()` & `clear_prime()` dispatchables.
// - Add `abdicate_membership()` dispatchable to allows a caller member to unilaterally quit without this
// being subject to a GC vote.

//! # Group Module
//!
//! The Group module is used to manage a set of identities. A group of identities can be a
//! collection of CDD providers, council members for governance and so on. This is an instantiable
//! module.
//!
//! ## Overview
//!
//! Allows control of membership of a set of `IdentityId`s, useful for managing group
//! membership. This includes:
//!
//! - adding a new identity,
//! - removing an identity from a group,
//! - swapping members,
//! - reseting group members.
//!
//! ## Active and Inactive members
//!
//! There are two kinds of members:
//!  - *Active members*, who can *act* on behalf of this group. For instance, any active CDD providers can
//!  generate CDD claims.
//!  - *Inactive members*, Members who were active previously but at some point they were disabled. Each
//!  inactive member has two timestamps:
//!     - `deactivated_at`: It indicates the moment when this member was disabled. Any claim generated *after*
//!     this moment is considered as invalid.
//!     - `expiry`: It is the moment when it should be removed completely from this group. From
//!     that moment on, any claim is considered invalid (as a group claim).
//!
//! This mechanism has been designed to disable any _compromised_ member at specific moment without
//! disabling all claims generated by this member. It means that, claims generated before disabling
//! any member are still valid and anyone generated after that moment will be invalid.
//!
//! ## Interface
//!
//! ### Dispatchable Functions
//!
//! - `add_member` - Adds a new identity to the group, as an *active* member..
//! - `remove_member` - Removes an *active* member from the group if it exists.
//! - `swap_member` - Replaces one identity with another.
//! - `reset_members` - Re-initializes group members.
//! - `abdicate_membership` - Unilateral abdication without being subject to a GC vote.
//!
//! ### Other Public Methods
//!
//! - `get_valid_members` - Returns the current "active members" and any "valid member" whose
//! revocation time-stamp is in the future.

// Ensure we're `no_std` when compiling for Wasm.
#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(feature = "runtime-benchmarks")]
pub mod benchmarking;

use pallet_identity as identity;
pub use polymesh_common_utilities::{
    group::{Config, GroupTrait, InactiveMember, MemberCount, RawEvent, WeightInfo},
    Context, GC_DID,
};
use polymesh_primitives::{committee::COMMITTEE_MEMBERS_MAX, IdentityId};

use frame_support::{
    decl_error, decl_module, decl_storage,
    dispatch::DispatchResult,
    ensure,
    traits::{ChangeMembers, EnsureOrigin},
    StorageValue,
};
use sp_std::prelude::*;

pub type Event<T, I> = polymesh_common_utilities::group::Event<T, I>;
type Identity<T> = identity::Module<T>;

decl_storage! {
    trait Store for Module<T: Config<I>, I: Instance=DefaultInstance> as Group {
        /// The current "active" membership, stored as an ordered Vec.
        pub ActiveMembers get(fn active_members) config(): Vec<IdentityId>;
        /// The current "inactive" membership, stored as an ordered Vec.
        pub InactiveMembers get(fn inactive_members): Vec<InactiveMember<T::Moment>>;
        /// Limit of how many "active" members there can be.
        pub ActiveMembersLimit get(fn active_members_limit) config(): u32;
    }
    add_extra_genesis {
        config(phantom): sp_std::marker::PhantomData<(T, I)>;
        build(|config: &Self| {
            use frame_support::traits::InitializeMembers;

            let mut members = config.active_members.clone();
            assert!(members.len() as MemberCount <= config.active_members_limit);
            members.sort();
            T::MembershipInitialized::initialize_members(&members);
            <ActiveMembers<I>>::put(members);
        })
    }
}

decl_module! {
    pub struct Module<T: Config<I>, I: Instance=DefaultInstance>
        for enum Call
        where origin: T::Origin
    {
        type Error = Error<T, I>;

        fn deposit_event() = default;

        /// Change this group's limit for how many concurrent active members they may be.
        ///
        /// # Arguments
        /// * `limit` - the number of active members there may be concurrently.
        #[weight = <T as Config<I>>::WeightInfo::set_active_members_limit()]
        pub fn set_active_members_limit(origin, limit: MemberCount) {
            T::LimitOrigin::ensure_origin(origin)?;
            ensure!(limit <= COMMITTEE_MEMBERS_MAX, Error::<T, I>::ActiveMembersLimitOverflow);
            let old = <ActiveMembersLimit<I>>::mutate(|slot| core::mem::replace(slot, limit));
            Self::deposit_event(RawEvent::ActiveLimitChanged(GC_DID, limit, old));
        }

        /// Disables a member at specific moment.
        ///
        /// Please note that if member is already revoked (a "valid member"), its revocation
        /// time-stamp will be updated.
        ///
        /// Any disabled member should NOT allow to act like an active member of the group. For
        /// instance, a disabled CDD member should NOT be able to generate a CDD claim. However any
        /// generated claim issued before `at` would be considered as a valid one.
        ///
        /// If you want to invalidate any generated claim, you should use `Self::remove_member`.
        ///
        /// # Arguments
        /// * `at` - Revocation time-stamp.
        /// * `who` - Target member of the group.
        /// * `expiry` - Time-stamp when `who` is removed from CDD. As soon as it is expired, the
        /// generated claims will be "invalid" as `who` is not considered a member of the group.
        #[weight = <T as Config<I>>::WeightInfo::disable_member()]
        pub fn disable_member( origin,
            who: IdentityId,
            expiry: Option<T::Moment>,
            at: Option<T::Moment>
        ) -> DispatchResult {
            T::RemoveOrigin::ensure_origin(origin)?;

            <Self as GroupTrait<T::Moment>>::disable_member(who, expiry, at)
        }

        /// Adds a member `who` to the group. May only be called from `AddOrigin` or root.
        ///
        /// # Arguments
        /// * `origin` - Origin representing `AddOrigin` or root
        /// * `who` - IdentityId to be added to the group.
        #[weight = <T as Config<I>>::WeightInfo::add_member()]
        pub fn add_member(origin, who: IdentityId) -> DispatchResult {
            T::AddOrigin::ensure_origin(origin)?;
            <Self as GroupTrait<T::Moment>>::add_member(who)
        }

        /// Removes a member `who` from the set. May only be called from `RemoveOrigin` or root.
        ///
        /// Any claim previously generated by this member is not valid as a group claim. For
        /// instance, if a CDD member group generated a claim for a target identity and then it is
        /// removed, that claim will be invalid.  In case you want to keep the validity of generated
        /// claims, you have to use `Self::disable_member` function
        ///
        /// # Arguments
        /// * `origin` - Origin representing `RemoveOrigin` or root
        /// * `who` - IdentityId to be removed from the group.
        #[weight = <T as Config<I>>::WeightInfo::remove_member()]
        pub fn remove_member(origin, who: IdentityId) -> DispatchResult {
            T::RemoveOrigin::ensure_origin(origin)?;
            Self::base_remove_member(who)
        }

        /// Swaps out one member `remove` for another member `add`.
        ///
        /// May only be called from `SwapOrigin` or root.
        ///
        /// # Arguments
        /// * `origin` - Origin representing `SwapOrigin` or root
        /// * `remove` - IdentityId to be removed from the group.
        /// * `add` - IdentityId to be added in place of `remove`.
        #[weight = <T as Config<I>>::WeightInfo::swap_member()]
        pub fn swap_member(origin, remove: IdentityId, add: IdentityId) {
            T::SwapOrigin::ensure_origin(origin)?;

            if remove == add { return Ok(()) }

            let mut members = <ActiveMembers<I>>::get();
            let remove_location = members.binary_search(&remove).ok().ok_or(Error::<T, I>::NoSuchMember)?;
            let _add_location = members.binary_search(&add).err().ok_or(Error::<T, I>::DuplicateMember)?;
            members[remove_location] = add;
            members.sort();
            <ActiveMembers<I>>::put(&members);

            T::MembershipChanged::change_members_sorted(
                &[add],
                &[remove],
                &members[..],
            );
            let current_did = Context::current_identity::<Identity<T>>().unwrap_or(GC_DID);
            Self::deposit_event(RawEvent::MembersSwapped(current_did, remove, add));
        }

        /// Changes the membership to a new set, disregarding the existing membership.
        /// May only be called from `ResetOrigin` or root.
        ///
        /// # Arguments
        /// * `origin` - Origin representing `ResetOrigin` or root
        /// * `members` - New set of identities
        #[weight = <T as Config<I>>::WeightInfo::reset_members( members.len() as u32)]
        pub fn reset_members(origin, members: Vec<IdentityId>) {
            T::ResetOrigin::ensure_origin(origin)?;

            Self::ensure_within_active_members_limit(&members)?;

            let mut new_members = members.clone();
            new_members.sort();
            <ActiveMembers<I>>::mutate(|m| {
                T::MembershipChanged::set_members_sorted(&new_members[..], m);
                *m = new_members;
            });
            let current_did = Context::current_identity::<Identity<T>>().unwrap_or(GC_DID);
            Self::deposit_event(RawEvent::MembersReset(current_did, members));
        }

        /// Allows the calling member to *unilaterally quit* without this being subject to a GC
        /// vote.
        ///
        /// # Arguments
        /// * `origin` - Member of committee who wants to quit.
        ///
        /// # Error
        ///
        /// * Only primary key can abdicate.
        /// * Last member of a group cannot abdicate.
        #[weight = <T as Config<I>>::WeightInfo::abdicate_membership()]
        pub fn abdicate_membership(origin) {
            let (who, remove_id) = Identity::<T>::ensure_did(origin)?;

            ensure!(
                <Identity<T>>::is_primary_key(&remove_id, &who),
                Error::<T,I>::OnlyPrimaryKeyAllowed
            );

            let mut members = Self::get_members();
            ensure!(members.len() > 1, Error::<T,I>::LastMemberCannotQuit);
            ensure!(members.binary_search(&remove_id).is_ok(), Error::<T,I>::NoSuchMember);

            members.retain(|id| *id != remove_id);
            <ActiveMembers<I>>::put(&members);

            T::MembershipChanged::change_members_sorted(
                &[],
                &[remove_id],
                &members[..],
            );
        }
    }
}

decl_error! {
    pub enum Error for Module<T: Config<I>, I: Instance> {
        /// Only primary key of the identity is allowed.
        OnlyPrimaryKeyAllowed,
        /// Group member was added already.
        DuplicateMember,
        /// Can't remove a member that doesn't exist.
        NoSuchMember,
        /// Last member of the committee can not quit.
        LastMemberCannotQuit,
        /// Missing current DID
        MissingCurrentIdentity,
        /// The limit for the number of concurrent active members for this group has been exceeded.
        ActiveMembersLimitExceeded,
        /// Active member limit was greater than maximum committee members limit.
        ActiveMembersLimitOverflow,
    }
}

impl<T: Config<I>, I: Instance> Module<T, I> {
    /// Ensure that updating the active set to `members` will not exceed the set limit.
    fn ensure_within_active_members_limit(members: &[IdentityId]) -> DispatchResult {
        ensure!(
            members.len() as MemberCount <= Self::active_members_limit(),
            Error::<T, I>::ActiveMembersLimitExceeded
        );
        Ok(())
    }

    /// Returns the current "active members" and any "valid member" whose revocation time-stamp is
    /// in the future.
    pub fn get_valid_members() -> Vec<IdentityId> {
        let now = <pallet_timestamp::Pallet<T>>::get();
        Self::get_valid_members_at(now)
    }

    /// Removes a member `who` as "active" or "inactive" member.
    ///
    /// # Arguments
    /// * `who` IdentityId to be removed from the group.
    fn base_remove_member(who: IdentityId) -> DispatchResult {
        Self::base_remove_active_member(who).or_else(|_| Self::base_remove_inactive_member(who))
    }

    /// Removes `who` as "inactive member"
    ///
    /// # Errors
    /// * `NoSuchMember` if `who` is not part of *inactive members*.
    fn base_remove_inactive_member(who: IdentityId) -> DispatchResult {
        let inactive_who = InactiveMember::<T::Moment>::from(who);
        let mut members = <InactiveMembers<T, I>>::get();
        let position = members
            .binary_search(&inactive_who)
            .ok()
            .ok_or(Error::<T, I>::NoSuchMember)?;

        members.swap_remove(position);

        <InactiveMembers<T, I>>::put(&members);
        let current_did = Context::current_identity::<Identity<T>>()
            .ok_or_else(|| Error::<T, I>::MissingCurrentIdentity)?;
        Self::deposit_event(RawEvent::MemberRemoved(current_did, who));
        Ok(())
    }

    /// Removes `who` as "active member"
    ///
    /// # Errors
    /// * `NoSuchMember` if `who` is not part of *active members*.
    fn base_remove_active_member(who: IdentityId) -> DispatchResult {
        let mut members = <ActiveMembers<I>>::get();
        let location = members
            .binary_search(&who)
            .ok()
            .ok_or(Error::<T, I>::NoSuchMember)?;

        members.remove(location);
        <ActiveMembers<I>>::put(&members);

        T::MembershipChanged::change_members_sorted(&[], &[who], &members[..]);
        let current_did = Context::current_identity::<Identity<T>>().unwrap_or(GC_DID);
        Self::deposit_event(RawEvent::MemberRemoved(current_did, who));
        Ok(())
    }
}

/// Retrieve all members of this group
/// Is the given `IdentityId` a valid member?
impl<T: Config<I>, I: Instance> GroupTrait<T::Moment> for Module<T, I> {
    /// Returns the "active members".
    #[inline]
    fn get_members() -> Vec<IdentityId> {
        Self::active_members()
    }

    /// Returns inactive members who are not expired yet.
    #[inline]
    fn get_inactive_members() -> Vec<InactiveMember<T::Moment>> {
        let now = <pallet_timestamp::Pallet<T>>::get();
        Self::inactive_members()
            .into_iter()
            .filter(|member| !Self::is_member_expired(member, now))
            .collect::<Vec<_>>()
    }

    /// Transforms an *active* membership into a *inactive* one.
    ///
    /// # Arguments
    /// * `who`: An *active* member.
    /// * `expiry`: When that *inactive* member will be completely removed from this group. If
    /// `None` that member will keep as *inactive* forever.
    /// * `at`: A past moment when `who` was considered as *inactive*. Any generated claim from
    /// that moment is considered as invalid. If `None`, the current block is used.
    fn disable_member(
        who: IdentityId,
        expiry: Option<T::Moment>,
        at: Option<T::Moment>,
    ) -> DispatchResult {
        Self::base_remove_active_member(who)?;
        let current_did = Context::current_identity::<Identity<T>>().unwrap_or(GC_DID);

        let deactivated_at = at.unwrap_or_else(<pallet_timestamp::Pallet<T>>::get);
        let inactive_member = InactiveMember {
            id: who,
            expiry,
            deactivated_at,
        };

        <InactiveMembers<T, I>>::mutate(|members| {
            // Remove expired members.
            let now = <pallet_timestamp::Pallet<T>>::get();
            members.retain(|m| {
                if !Self::is_member_expired(m, now) {
                    true
                } else {
                    Self::deposit_event(RawEvent::MemberRemoved(current_did, who));
                    false
                }
            });

            // Update inactive member
            if let Ok(idx) = members.binary_search(&inactive_member) {
                members[idx] = inactive_member;
            } else {
                members.push(inactive_member);
                members.sort();
            }
        });

        Self::deposit_event(RawEvent::MemberRevoked(current_did, who));
        Ok(())
    }

    /// Adds a new member to the group
    fn add_member(who: IdentityId) -> DispatchResult {
        let mut members = <ActiveMembers<I>>::get();
        let location = members
            .binary_search(&who)
            .err()
            .ok_or(Error::<T, I>::DuplicateMember)?;
        members.insert(location, who);
        Self::ensure_within_active_members_limit(&members)?;
        <ActiveMembers<I>>::put(&members);

        T::MembershipChanged::change_members_sorted(&[who], &[], &members[..]);
        let current_did = Context::current_identity::<Identity<T>>().unwrap_or(GC_DID);
        Self::deposit_event(RawEvent::MemberAdded(current_did, who));
        Ok(())
    }
}
