//! # Group Module
//!
//! The Group module is used to manage a set of identities. A group of identities can be a
//! collection of KYC providers, council members for governance and so on. This is an instantiable
//! module.
//!
//! ## Overview
//! Allows control of membership of a set of `IdentityId`s, useful for managing membership of a
//! collective.
//!
//! - Add a new identity
//! - Remove identity from the group
//! - Swam members
//! - Reset group members
//!
//! ### Dispatchable Functions
//!
//! - `add_member` - Adds a new identity to the group.
//! - `remove_member` - Remove identity from the group if it exists.
//! - `swap_member` - Replace one identity with the other.
//! - `reset_members` - Re-initialize group members.

#![cfg_attr(not(feature = "std"), no_std)]

use polymesh_primitives::IdentityId;
pub use polymesh_runtime_common::group::{GroupTrait, RawEvent, Trait};

use frame_support::{
    decl_module, decl_storage,
    traits::{ChangeMembers, InitializeMembers},
    weights::SimpleDispatchInfo,
    StorageValue,
};
use frame_system::{self as system, ensure_root};
use sp_runtime::traits::EnsureOrigin;
use sp_std::prelude::*;

pub type Event<T, I> = polymesh_runtime_common::group::Event<T, I>;

decl_storage! {
    trait Store for Module<T: Trait<I>, I: Instance=DefaultInstance> as Group {
        /// Identities that are part of this group
        pub Members get(fn members) config(): Vec<IdentityId>;
    }
    add_extra_genesis {
        config(phantom): sp_std::marker::PhantomData<(T, I)>;
        build(|config: &Self| {
            let mut members = config.members.clone();
            members.sort();
            T::MembershipInitialized::initialize_members(&members);
            <Members<I>>::put(members);
        })
    }
}

decl_module! {
    pub struct Module<T: Trait<I>, I: Instance=DefaultInstance>
        for enum Call
        where origin: T::Origin
    {
        fn deposit_event() = default;

        /// Add a member `who` to the set. May only be called from `AddOrigin` or root.
        ///
        /// # Arguments
        /// * `origin` Origin representing `AddOrigin` or root
        /// * `who` IdentityId to be added to the group.
        #[weight = SimpleDispatchInfo::FixedNormal(50_000)]
        pub fn add_member(origin, who: IdentityId) {
            T::AddOrigin::try_origin(origin)
                .map(|_| ())
                .or_else(ensure_root)
                .map_err(|_| "bad origin")?;

            let mut members = <Members<I>>::get();
            let location = members.binary_search(&who).err().ok_or("already a member")?;
            members.insert(location, who.clone());
            <Members<I>>::put(&members);

            T::MembershipChanged::change_members_sorted(&[who], &[], &members[..]);

            Self::deposit_event(RawEvent::MemberAdded(who));
        }

        /// Remove a member `who` from the set. May only be called from `RemoveOrigin` or root.
        ///
        /// # Arguments
        /// * `origin` Origin representing `RemoveOrigin` or root
        /// * `who` IdentityId to be removed from the group.
        #[weight = SimpleDispatchInfo::FixedNormal(50_000)]
        fn remove_member(origin, who: IdentityId) {
            T::RemoveOrigin::try_origin(origin)
                .map(|_| ())
                .or_else(ensure_root)
                .map_err(|_| "bad origin")?;

            let mut members = <Members<I>>::get();
            let location = members.binary_search(&who).ok().ok_or("not a member")?;
            members.remove(location);
            <Members<I>>::put(&members);

            T::MembershipChanged::change_members_sorted(&[], &[who], &members[..]);

            Self::deposit_event(RawEvent::MemberRemoved(who));
        }

        /// Swap out one member `remove` for another `add`.
        /// May only be called from `SwapOrigin` or root.
        ///
        /// # Arguments
        /// * `origin` Origin representing `SwapOrigin` or root
        /// * `remove` IdentityId to be removed from the group.
        /// * `add` IdentityId to be added in place of `remove`.
        #[weight = SimpleDispatchInfo::FixedNormal(50_000)]
        fn swap_member(origin, remove: IdentityId, add: IdentityId) {
            T::SwapOrigin::try_origin(origin)
                .map(|_| ())
                .or_else(ensure_root)
                .map_err(|_| "bad origin")?;

            if remove == add { return Ok(()) }

            let mut members = <Members<I>>::get();

            let location = members.binary_search(&remove).ok().ok_or("not a member")?;
            members[location] = add.clone();

            let _location = members.binary_search(&add).err().ok_or("already a member")?;
            members.sort();
            <Members<I>>::put(&members);

            T::MembershipChanged::change_members_sorted(
                &[add],
                &[remove],
                &members[..],
            );

            Self::deposit_event(RawEvent::MembersSwapped(remove, add));
        }

        /// Change the membership to a new set, disregarding the existing membership.
        /// May only be called from `ResetOrigin` or root.
        ///
        /// # Arguments
        /// * `origin` Origin representing `ResetOrigin` or root
        /// * `members` New set of identities
        #[weight = SimpleDispatchInfo::FixedNormal(50_000)]
        fn reset_members(origin, members: Vec<IdentityId>) {
            T::ResetOrigin::try_origin(origin)
                .map(|_| ())
                .or_else(ensure_root)
                .map_err(|_| "bad origin")?;

            let mut new_members = members.clone();
            new_members.sort();
            <Members<I>>::mutate(|m| {
                T::MembershipChanged::set_members_sorted(&members[..], m);
                *m = new_members;
            });

            Self::deposit_event(RawEvent::MembersReset(members));
        }
    }
}

/// Retrieve all members of this group
/// Is the given `IdentityId` a valid member?
impl<T: Trait<I>, I: Instance> GroupTrait for Module<T, I> {
    fn get_members() -> Vec<IdentityId> {
        return Self::members();
    }

    fn is_member(did: &IdentityId) -> bool {
        Self::members().iter().any(|id| id == did)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use frame_support::{
        assert_noop, assert_ok, impl_outer_origin, parameter_types, traits::InitializeMembers,
    };
    use frame_system::{self as system, EnsureSignedBy};
    use sp_core::H256;
    use sp_std::cell::RefCell;

    use sp_runtime::{
        testing::Header,
        traits::{BlakeTwo256, IdentityLookup},
        Perbill,
    };

    impl_outer_origin! {
        pub enum Origin for Test {}
    }

    #[derive(Clone, Eq, PartialEq)]
    pub struct Test;
    parameter_types! {
        pub const BlockHashCount: u64 = 250;
        pub const MaximumBlockWeight: u32 = 1024;
        pub const MaximumBlockLength: u32 = 2 * 1024;
        pub const AvailableBlockRatio: Perbill = Perbill::one();
    }

    impl frame_system::Trait for Test {
        type Origin = Origin;
        type Index = u64;
        type BlockNumber = u64;
        type Call = ();
        type Hash = H256;
        type Hashing = BlakeTwo256;
        type AccountId = u64;
        type Lookup = IdentityLookup<Self::AccountId>;
        type Header = Header;
        type Event = ();
        type BlockHashCount = BlockHashCount;
        type MaximumBlockWeight = MaximumBlockWeight;
        type MaximumBlockLength = MaximumBlockLength;
        type AvailableBlockRatio = AvailableBlockRatio;
        type Version = ();
        type ModuleToIndex = ();
    }
    parameter_types! {
        pub const One: u64 = 1;
        pub const Two: u64 = 2;
        pub const Three: u64 = 3;
        pub const Four: u64 = 4;
        pub const Five: u64 = 5;
    }

    thread_local! {
        static MEMBERS: RefCell<Vec<IdentityId>> = RefCell::new(vec![]);
    }

    pub struct TestChangeMembers;
    impl ChangeMembers<IdentityId> for TestChangeMembers {
        fn change_members_sorted(
            incoming: &[IdentityId],
            outgoing: &[IdentityId],
            new: &[IdentityId],
        ) {
            let mut old_plus_incoming = MEMBERS.with(|m| m.borrow().to_vec());
            old_plus_incoming.extend_from_slice(incoming);
            old_plus_incoming.sort();
            let mut new_plus_outgoing = new.to_vec();
            new_plus_outgoing.extend_from_slice(outgoing);
            new_plus_outgoing.sort();
            assert_eq!(old_plus_incoming, new_plus_outgoing);

            MEMBERS.with(|m| *m.borrow_mut() = new.to_vec());
        }
    }
    impl InitializeMembers<IdentityId> for TestChangeMembers {
        fn initialize_members(members: &[IdentityId]) {
            MEMBERS.with(|m| *m.borrow_mut() = members.to_vec());
        }
    }

    impl Trait<DefaultInstance> for Test {
        type Event = ();
        type AddOrigin = EnsureSignedBy<One, u64>;
        type RemoveOrigin = EnsureSignedBy<Two, u64>;
        type SwapOrigin = EnsureSignedBy<Three, u64>;
        type ResetOrigin = EnsureSignedBy<Four, u64>;
        type MembershipInitialized = TestChangeMembers;
        type MembershipChanged = TestChangeMembers;
    }

    type Group = Module<Test>;

    // This function basically just builds a genesis storage key/value store according to
    // our desired mockup.
    fn new_test_ext() -> sp_io::TestExternalities {
        let mut t = system::GenesisConfig::default()
            .build_storage::<Test>()
            .unwrap();

        // We use default for brevity, but you can configure as desired if needed.
        GenesisConfig::<Test> {
            members: vec![
                IdentityId::from(1),
                IdentityId::from(2),
                IdentityId::from(3),
            ],
            ..Default::default()
        }
        .assimilate_storage(&mut t)
        .unwrap();
        t.into()
    }

    #[test]
    fn query_membership_works() {
        new_test_ext().execute_with(|| {
            assert_eq!(
                Group::members(),
                vec![
                    IdentityId::from(1),
                    IdentityId::from(2),
                    IdentityId::from(3)
                ]
            );
            assert_eq!(
                MEMBERS.with(|m| m.borrow().clone()),
                vec![
                    IdentityId::from(1),
                    IdentityId::from(2),
                    IdentityId::from(3)
                ]
            );
        });
    }

    #[test]
    fn add_member_works() {
        new_test_ext().execute_with(|| {
            assert_noop!(
                Group::add_member(Origin::signed(5), IdentityId::from(3)),
                "bad origin"
            );
            assert_noop!(
                Group::add_member(Origin::signed(1), IdentityId::from(3)),
                "already a member"
            );
            assert_ok!(Group::add_member(Origin::signed(1), IdentityId::from(4)));
            assert_eq!(
                Group::members(),
                vec![
                    IdentityId::from(1),
                    IdentityId::from(2),
                    IdentityId::from(3),
                    IdentityId::from(4)
                ]
            );
            assert_eq!(MEMBERS.with(|m| m.borrow().clone()), Group::members());
        });
    }

    #[test]
    fn remove_member_works() {
        new_test_ext().execute_with(|| {
            assert_noop!(
                Group::remove_member(Origin::signed(5), IdentityId::from(3)),
                "bad origin"
            );
            assert_noop!(
                Group::remove_member(Origin::signed(2), IdentityId::from(5)),
                "not a member"
            );
            assert_ok!(Group::remove_member(Origin::signed(2), IdentityId::from(3)));
            assert_eq!(
                Group::members(),
                vec![IdentityId::from(1), IdentityId::from(2),]
            );
            assert_eq!(MEMBERS.with(|m| m.borrow().clone()), Group::members());
        });
    }

    #[test]
    fn swap_member_works() {
        new_test_ext().execute_with(|| {
            assert_noop!(
                Group::swap_member(Origin::signed(5), IdentityId::from(1), IdentityId::from(5)),
                "bad origin"
            );
            assert_noop!(
                Group::swap_member(Origin::signed(3), IdentityId::from(5), IdentityId::from(6)),
                "not a member"
            );
            assert_noop!(
                Group::swap_member(Origin::signed(3), IdentityId::from(1), IdentityId::from(3)),
                "already a member"
            );
            assert_ok!(Group::swap_member(
                Origin::signed(3),
                IdentityId::from(2),
                IdentityId::from(2)
            ));
            assert_eq!(
                Group::members(),
                vec![
                    IdentityId::from(1),
                    IdentityId::from(2),
                    IdentityId::from(3)
                ]
            );
            assert_ok!(Group::swap_member(
                Origin::signed(3),
                IdentityId::from(1),
                IdentityId::from(6)
            ));
            assert_eq!(
                Group::members(),
                vec![
                    IdentityId::from(2),
                    IdentityId::from(3),
                    IdentityId::from(6),
                ]
            );
            assert_eq!(MEMBERS.with(|m| m.borrow().clone()), Group::members());
        });
    }

    #[test]
    fn reset_members_works() {
        new_test_ext().execute_with(|| {
            assert_noop!(
                Group::reset_members(
                    Origin::signed(1),
                    vec![
                        IdentityId::from(4),
                        IdentityId::from(5),
                        IdentityId::from(6),
                    ]
                ),
                "bad origin"
            );
            assert_ok!(Group::reset_members(
                Origin::signed(4),
                vec![
                    IdentityId::from(4),
                    IdentityId::from(5),
                    IdentityId::from(6),
                ]
            ));
            assert_eq!(
                Group::members(),
                vec![
                    IdentityId::from(4),
                    IdentityId::from(5),
                    IdentityId::from(6),
                ]
            );
            assert_eq!(MEMBERS.with(|m| m.borrow().clone()), Group::members());
        });
    }
}
