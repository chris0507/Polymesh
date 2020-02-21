//! # Mips Module
//!
//! MESH Improvement Proposals (MIPs) are proposals (ballots) that can then be proposed and voted on
//! by all MESH token holders. If a ballot passes this community vote it is then passed to the
//! governance council to ratify (or reject).
//! - minimum of 5,000 MESH needs to be staked by the proposer of the ballot
//! in order to create a new ballot.
//! - minimum of 100,000 MESH (quorum) needs to vote in favour of the ballot in order for the
//! ballot to be considered by the governing committee.
//! - ballots run for 1 week
//! - a simple majority is needed to pass the ballot so that it heads for the
//! next stage (governing committee)
//!
//! ## Overview
//!
//! The Mips module provides functions for:
//!
//! - Creating Mesh Improvement Proposals
//! - Voting on Mesh Improvement Proposals
//! - Governance committee to ratify or reject proposals
//!
//! ## Interface
//!
//! ### Dispatchable Functions
//!
//! - `set_min_proposal_deposit` change min deposit to create a proposal
//! - `set_quorum_threshold` change stake required to make a proposal into a referendum
//! - `set_proposal_duration` change duration in blocks for which proposal stays active
//! - `propose` - Token holders can propose a new ballot.
//! - `vote` - Token holders can vote on a ballot.
//! - `kill_proposal` - close a proposal and refund all deposits
//! - `enact_referendum` committee calls to execute a referendum
//!
//! ### Public Functions
//!
//! - `end_block` - Returns details of the token

use codec::{Decode, Encode};
use frame_support::{
    decl_error, decl_event, decl_module, decl_storage,
    dispatch::DispatchResult,
    ensure,
    traits::{Currency, LockableCurrency, ReservableCurrency},
    weights::SimpleDispatchInfo,
    Parameter,
};
use frame_system::{self as system, ensure_root, ensure_signed};
use sp_runtime::{
    traits::{Dispatchable, EnsureOrigin, Hash, Zero},
    DispatchError,
};
use sp_std::{prelude::*, vec};

/// Mesh Improvement Proposal index. Used offchain.
pub type MipsIndex = u32;

/// Balance
type BalanceOf<T> =
    <<T as Trait>::Currency as Currency<<T as frame_system::Trait>::AccountId>>::Balance;

/// Represents a proposal
#[derive(Encode, Decode, Clone, PartialEq, Eq)]
pub struct MIP<Proposal> {
    /// The proposal's unique index.
    index: MipsIndex,
    /// The proposal being voted on.
    proposal: Proposal,
}

/// A wrapper for a proposal url.
#[derive(Decode, Encode, Clone, Debug, Default, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct Url(pub Vec<u8>);

impl<T: AsRef<[u8]>> From<T> for Url {
    fn from(s: T) -> Self {
        let s = s.as_ref();
        let mut v = Vec::with_capacity(s.len());
        v.extend_from_slice(s);
        Url(v)
    }
}

/// Represents a proposal metadata
#[derive(Encode, Decode, Clone, PartialEq, Eq)]
pub struct MipsMetadata<BlockNumber: Parameter, Hash: Parameter> {
    /// The proposal's unique index.
    index: MipsIndex,
    /// When voting will end.
    end: BlockNumber,
    /// The proposal being voted on.
    proposal_hash: Hash,
    /// The proposal url for proposal discussion.
    url: Option<Url>,
}

/// For keeping track of proposal being voted on.
#[derive(PartialEq, Eq, Clone, Encode, Decode)]
#[cfg_attr(feature = "std", derive(Debug))]
pub struct PolymeshVotes<AccountId, Balance> {
    /// The proposal's unique index.
    index: MipsIndex,
    /// The current set of voters that approved with their stake.
    ayes: Vec<(AccountId, Balance)>,
    /// The current set of voters that rejected with their stake.
    nays: Vec<(AccountId, Balance)>,
}

#[derive(Encode, Decode, Copy, Clone, Eq, PartialEq)]
#[cfg_attr(feature = "std", derive(Debug))]
pub enum MipsPriority {
    /// A proposal made by the committee for e.g.,
    High,
    /// By default all proposals have a normal priority
    Normal,
}

impl Default for MipsPriority {
    fn default() -> Self {
        MipsPriority::Normal
    }
}

/// Properties of a referendum
#[derive(Encode, Decode, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "std", derive(Debug))]
pub struct PolymeshReferendumInfo<Hash: Parameter> {
    /// The proposal's unique index.
    index: MipsIndex,
    /// Priority.
    priority: MipsPriority,
    /// The proposal being voted on.
    proposal_hash: Hash,
}

/// The module's configuration trait.
pub trait Trait: frame_system::Trait {
    /// Currency type for this module.
    type Currency: ReservableCurrency<Self::AccountId>
        + LockableCurrency<Self::AccountId, Moment = Self::BlockNumber>;

    /// A proposal is a dispatchable call
    type Proposal: Parameter + Dispatchable<Origin = Self::Origin>;

    /// Required origin for enacting a referundum.
    type CommitteeOrigin: EnsureOrigin<Self::Origin>;

    /// The overarching event type.
    type Event: From<Event<Self>> + Into<<Self as system::Trait>::Event>;
}

// This module's storage items.
decl_storage! {
    trait Store for Module<T: Trait> as Mips {
        /// The minimum amount to be used as a deposit for a public referendum proposal.
        pub MinimumProposalDeposit get(fn min_proposal_deposit) config(): BalanceOf<T>;

        /// Minimum stake a proposal must gather in order to be considered by the committee.
        pub QuorumThreshold get(fn quorum_threshold) config(): BalanceOf<T>;

        /// How long (in blocks) a ballot runs
        pub ProposalDuration get(fn proposal_duration) config(): T::BlockNumber;

        /// Proposals so far. Index can be used to keep track of MIPs off-chain.
        pub ProposalCount get(fn proposal_count): u32;

        /// The hashes of the active proposals.
        pub ProposalMetadata get(fn proposal_meta): Vec<MipsMetadata<T::BlockNumber, T::Hash>>;

        /// Those who have locked a deposit.
        /// proposal hash -> (deposit, proposer)
        pub Deposits get(fn deposit_of): map T::Hash => Vec<(T::AccountId, BalanceOf<T>)>;

        /// Actual proposal for a given hash, if it's current.
        /// proposal hash -> proposal
        pub Proposals get(fn proposals): map T::Hash => Option<MIP<T::Proposal>>;

        /// PolymeshVotes on a given proposal, if it is ongoing.
        /// proposal hash -> voting info
        pub Voting get(fn voting): map T::Hash => Option<PolymeshVotes<T::AccountId, BalanceOf<T>>>;

        /// Active referendums.
        pub ReferendumMetadata get(fn referendum_meta): Vec<PolymeshReferendumInfo<T::Hash>>;

        /// Proposals that have met the quorum threshold to be put forward to a governance committee
        /// proposal hash -> proposal
        pub Referendums get(fn referendums): map T::Hash => Option<T::Proposal>;
    }
}

decl_event!(
    pub enum Event<T>
    where
        Balance = BalanceOf<T>,
        <T as frame_system::Trait>::Hash,
        <T as frame_system::Trait>::AccountId,
    {
        /// A Mesh Improvement Proposal was made with a `Balance` stake
        Proposed(AccountId, Balance, MipsIndex, Hash),
        /// `AccountId` voted `bool` on the proposal referenced by `Hash`
        Voted(AccountId, MipsIndex, Hash, bool),
        /// Proposal referenced by `Hash` has been closed
        ProposalClosed(MipsIndex, Hash),
        /// Proposal referenced by `Hash` has been closed
        ProposalFastTracked(MipsIndex, Hash),
        /// Referendum created for proposal referenced by `Hash`
        ReferendumCreated(MipsIndex, MipsPriority, Hash),
        /// Proposal referenced by `Hash` was dispatched with the result `bool`
        ReferendumEnacted(Hash, bool),
    }
);

decl_error! {
    pub enum Error for Module<T: Trait> {
        /// Incorrect origin
        BadOrigin,
        /// Proposer can't afford to lock minimum deposit
        InsufficientDeposit,
        /// when voter vote gain
        DuplicateVote,
    }
}

// The module's dispatchable functions.
decl_module! {
    /// The module declaration.
    pub struct Module<T: Trait> for enum Call where origin: T::Origin {

        type Error = Error<T>;

        fn deposit_event() = default;

        /// Change the minimum proposal deposit amount required to start a proposal. Only Governance
        /// committee is allowed to change this value.
        ///
        /// # Arguments
        /// * `deposit` the new min deposit required to start a proposal
        #[weight = SimpleDispatchInfo::FixedOperational(100_000)]
        fn set_min_proposal_deposit(origin, deposit: BalanceOf<T>) {
            T::CommitteeOrigin::try_origin(origin)
                .map(|_| ())
                .or_else(ensure_root)
                .map_err(|_| Error::<T>::BadOrigin)?;
            <MinimumProposalDeposit<T>>::put(deposit);
        }

        /// Change the quorum threshold amount. This is the amount which a proposal must gather so
        /// as to be considered by a committee. Only Governance committee is allowed to change
        /// this value.
        ///
        /// # Arguments
        /// * `threshold` the new quorum threshold amount value
        #[weight = SimpleDispatchInfo::FixedOperational(100_000)]
        fn set_quorum_threshold(origin, threshold: BalanceOf<T>) {
            T::CommitteeOrigin::try_origin(origin)
                .map(|_| ())
                .or_else(ensure_root)
                .map_err(|_| Error::<T>::BadOrigin)?;
            <QuorumThreshold<T>>::put(threshold);
        }

        /// Change the proposal duration value. This is the number of blocks for which votes are
        /// accepted on a proposal. Only Governance committee is allowed to change this value.
        ///
        /// # Arguments
        /// * `duration` proposal duration in blocks
        #[weight = SimpleDispatchInfo::FixedOperational(100_000)]
        fn set_proposal_duration(origin, duration: T::BlockNumber) {
            T::CommitteeOrigin::try_origin(origin)
                .map(|_| ())
                .or_else(ensure_root)
                .map_err(|_| Error::<T>::BadOrigin)?;
            <ProposalDuration<T>>::put(duration);
        }

        /// A network member creates a Mesh Improvement Proposal by submitting a dispatchable which
        /// changes the network in someway. A minimum deposit is required to open a new proposal.
        ///
        /// # Arguments
        /// * `proposal` a dispatchable call
        /// * `deposit` minimum deposit value
        /// * `url` a link to a website for proposal discussion
        #[weight = SimpleDispatchInfo::FixedNormal(5_000_000)]
        pub fn propose(origin, proposal: Box<T::Proposal>, deposit: BalanceOf<T>, url: Option<Url>) -> DispatchResult {
            let proposer = ensure_signed(origin)?;
            let proposal_hash = T::Hashing::hash_of(&proposal);

            // Pre conditions: caller must have min balance
            ensure!(deposit >= Self::min_proposal_deposit(), "deposit is less than minimum required to start a proposal");
            // Proposal must be new
            ensure!(!<Proposals<T>>::exists(proposal_hash), "duplicate proposals are not allowed");

            // Reserve the minimum deposit
            T::Currency::reserve(&proposer, deposit).map_err(|_| Error::<T>::InsufficientDeposit)?;

            let index = Self::proposal_count();
            <ProposalCount>::mutate(|i| *i += 1);

            let proposal_meta = MipsMetadata {
                index,
                end: <system::Module<T>>::block_number() + Self::proposal_duration(),
                proposal_hash,
                url
            };
            <ProposalMetadata<T>>::mutate(|metadata| metadata.push(proposal_meta));

            <Deposits<T>>::insert(proposal_hash, vec![(proposer.clone(), deposit)]);

            let mip = MIP {
                index,
                proposal: *proposal
            };
            <Proposals<T>>::insert(proposal_hash, mip);

            let vote = PolymeshVotes {
                index,
                ayes: vec![(proposer.clone(), deposit)],
                nays: vec![],
            };
            <Voting<T>>::insert(proposal_hash, vote);

            Self::deposit_event(RawEvent::Proposed(proposer, deposit, index, proposal_hash));
            Ok(())
        }

        /// A network member can vote on any Mesh Improvement Proposal by selecting the hash that
        /// corresponds ot the dispatchable action and vote with some balance.
        ///
        /// # Arguments
        /// * `proposal` a dispatchable call
        /// * `index` proposal index
        /// * `aye_or_nay` a bool representing for or against vote
        /// * `deposit` minimum deposit value
        #[weight = SimpleDispatchInfo::FixedNormal(200_000)]
        pub fn vote(origin, proposal_hash: T::Hash, index: MipsIndex, aye_or_nay: bool, deposit: BalanceOf<T>) {
            let proposer = ensure_signed(origin)?;

            let mut voting = Self::voting(&proposal_hash).ok_or("proposal does not exist")?;
            ensure!(voting.index == index, "mismatched proposal index");

            let position_yes = voting.ayes.iter().position(|(a, _)| a == &proposer);
            let position_no = voting.nays.iter().position(|(a, _)| a == &proposer);

            if position_yes.is_none() && position_no.is_none()  {
                if aye_or_nay {
                    voting.ayes.push((proposer.clone(), deposit));
                } else {
                    voting.nays.push((proposer.clone(), deposit));
                }

                // Reserve the deposit
                T::Currency::reserve(&proposer, deposit).map_err(|_| Error::<T>::InsufficientDeposit)?;

                <Deposits<T>>::mutate(proposal_hash, |deposits| deposits.push((proposer.clone(), deposit)));

                <Voting<T>>::remove(&proposal_hash);
                <Voting<T>>::insert(&proposal_hash, voting);
                Self::deposit_event(RawEvent::Voted(proposer, index, proposal_hash, aye_or_nay));
            } else {
                return Err(Error::<T>::DuplicateVote.into())
            }
        }

        /// An emergency stop measure to kill a proposal. Governance committee can kill
        /// a proposal at any time.
        #[weight = SimpleDispatchInfo::FixedOperational(100_000)]
        pub fn kill_proposal(origin, index: MipsIndex, proposal_hash: T::Hash) {
            T::CommitteeOrigin::try_origin(origin)
                .map(|_| ())
                .or_else(ensure_root)
                .map_err(|_| Error::<T>::BadOrigin)?;

            let mip = Self::proposals(&proposal_hash).ok_or("proposal does not exist")?;
            ensure!(mip.index == index, "mismatched proposal index");

            Self::close_proposal(index, proposal_hash);
        }

        /// An emergency stop measure to kill a proposal. Governance committee can kill
        /// a proposal at any time.
        #[weight = SimpleDispatchInfo::FixedOperational(200_000)]
        pub fn fast_track_proposal(origin, index: MipsIndex, proposal_hash: T::Hash) {
            T::CommitteeOrigin::try_origin(origin)
                .map(|_| ())
                .or_else(ensure_root)
                .map_err(|_| "bad origin")?;

            let mip = Self::proposals(&proposal_hash).ok_or("proposal does not exist")?;
            ensure!(mip.index == index, "mismatched proposal index");

            Self::create_referendum(
                index,
                MipsPriority::High,
                proposal_hash,
                mip.proposal,
            );

            Self::deposit_event(RawEvent::ProposalFastTracked(
                index,
                proposal_hash,
            ));

            Self::close_proposal(index, proposal_hash.clone());
        }

        /// An emergency proposal that bypasses network voting process. Governance committee can make
        /// a proposal that automatically becomes a referendum on which the committee can vote on.
        #[weight = SimpleDispatchInfo::FixedOperational(200_000)]
        pub fn emergency_referendum(origin, proposal: Box<T::Proposal>) {
            // Proposal must originate from the committee
            T::CommitteeOrigin::try_origin(origin)
                .map(|_| ())
                .or_else(ensure_root)
                .map_err(|_| Error::<T>::BadOrigin)?;

            let proposal_hash = T::Hashing::hash_of(&proposal);

            // Proposal must be new
            ensure!(!<Proposals<T>>::exists(proposal_hash), "proposal from committee already exists");

            let index = Self::proposal_count();
            <ProposalCount>::mutate(|i| *i += 1);

            Self::create_referendum(
                index,
                MipsPriority::High,
                proposal_hash.clone(),
                *proposal
            );
        }

        /// Moves a referendum instance into dispatch queue.
        #[weight = SimpleDispatchInfo::FixedOperational(100_000)]
        pub fn enact_referendum(origin, proposal_hash: T::Hash) {
            T::CommitteeOrigin::try_origin(origin)
                .map(|_| ())
                .or_else(ensure_root)
                .map_err(|_| Error::<T>::BadOrigin)?;

            Self::prepare_to_dispatch(proposal_hash);
        }

        /// When constructing a block check if it's time for a ballot to end. If ballot ends,
        /// proceed to ratification process.
        fn on_initialize(n: T::BlockNumber) {
            if let Err(e) = Self::end_block(n) {
                sp_runtime::print(e);
            }
        }

    }
}

impl<T: Trait> Module<T> {
    /// Retrieve all proposals that need to be closed as of block `n`.
    pub fn proposals_maturing_at(n: T::BlockNumber) -> Vec<(MipsIndex, T::Hash)> {
        Self::proposal_meta()
            .into_iter()
            .filter(|meta| meta.end == n)
            .map(|meta| (meta.index, meta.proposal_hash))
            .collect()
    }

    // Private functions

    /// Runs the following procedure:
    /// 1. Find all proposals that need to end as of this block and close voting
    /// 2. Tally votes
    /// 3. Submit any proposals that meet the quorum threshold, to the governance committee
    fn end_block(block_number: T::BlockNumber) -> DispatchResult {
        // Find all matured proposals...
        for (index, hash) in Self::proposals_maturing_at(block_number).into_iter() {
            // Tally votes and create referendums
            Self::tally_votes(index, hash.clone());

            // And close proposals
            Self::close_proposal(index, hash);
        }

        Ok(())
    }

    /// Summarize voting and create referendums if proposals meet or exceed quorum threshold
    fn tally_votes(index: MipsIndex, proposal_hash: T::Hash) {
        if let Some(voting) = <Voting<T>>::get(proposal_hash) {
            let aye_stake = voting
                .ayes
                .iter()
                .fold(<BalanceOf<T>>::zero(), |acc, ayes| acc + ayes.1);

            let nay_stake = voting
                .nays
                .iter()
                .fold(<BalanceOf<T>>::zero(), |acc, nays| acc + nays.1);

            // 1. Ayes staked must be more than nays staked (simple majority)
            // 2. Ayes staked are more than the minimum quorum threshold
            if aye_stake > nay_stake && aye_stake >= Self::quorum_threshold() {
                if let Some(mip) = <Proposals<T>>::get(&proposal_hash) {
                    Self::create_referendum(
                        index,
                        MipsPriority::Normal,
                        proposal_hash.clone(),
                        mip.proposal,
                    );
                }
            }
        }
    }

    /// Create a referendum object from a proposal.
    /// Committee votes on this referendum instance
    fn create_referendum(
        index: MipsIndex,
        priority: MipsPriority,
        proposal_hash: T::Hash,
        proposal: T::Proposal,
    ) {
        let ri = PolymeshReferendumInfo {
            index,
            priority,
            proposal_hash,
        };

        <ReferendumMetadata<T>>::mutate(|metadata| metadata.push(ri));
        <Referendums<T>>::insert(proposal_hash.clone(), proposal);

        Self::deposit_event(RawEvent::ReferendumCreated(
            index,
            priority.clone(),
            proposal_hash.clone(),
        ));
    }

    /// Close a proposal. Voting ceases and proposal is removed from storage.
    /// All deposits are unlocked and returned to respective stakers.
    fn close_proposal(index: MipsIndex, proposal_hash: T::Hash) {
        if <Voting<T>>::get(proposal_hash).is_some() {
            if <Deposits<T>>::exists(&proposal_hash) {
                let deposits: Vec<(T::AccountId, BalanceOf<T>)> =
                    <Deposits<T>>::take(&proposal_hash);

                for (depositor, deposit) in deposits.iter() {
                    T::Currency::unreserve(depositor, *deposit);
                }
            }

            if <Proposals<T>>::take(&proposal_hash).is_some() {
                <Voting<T>>::remove(&proposal_hash);
                let hash = proposal_hash.clone();
                <ProposalMetadata<T>>::mutate(|metadata| {
                    metadata.retain(|m| m.proposal_hash != hash)
                });

                Self::deposit_event(RawEvent::ProposalClosed(index, hash));
            }
        }
    }

    fn prepare_to_dispatch(hash: T::Hash) {
        if let Some(referendum) = <Referendums<T>>::get(&hash) {
            let result = match referendum.dispatch(system::RawOrigin::Root.into()) {
                Ok(_) => true,
                Err(e) => {
                    let e: DispatchError = e.into();
                    sp_runtime::print(e);
                    false
                }
            };
            Self::deposit_event(RawEvent::ReferendumEnacted(hash, result));
        }
    }
}

// tests for this module
#[cfg(test)]
mod tests {
    use super::*;

    use polymesh_primitives::{IdentityId, Signatory};
    use polymesh_runtime_balances as balances;
    use polymesh_runtime_common::traits::{
        asset::AcceptTransfer, multisig::AddSignerMultiSig, CommonTrait,
    };
    use polymesh_runtime_group as group;
    use polymesh_runtime_identity as identity;

    use frame_support::{
        assert_err, assert_ok, dispatch::DispatchResult, impl_outer_dispatch, impl_outer_origin,
        parameter_types,
    };
    use frame_system::EnsureSignedBy;
    use sp_core::H256;
    use sp_runtime::{
        testing::Header,
        traits::{BlakeTwo256, IdentityLookup},
        Perbill,
    };

    impl_outer_origin! {
        pub enum Origin for Test {}
    }

    impl_outer_dispatch! {
        pub enum Call for Test where origin: Origin {
            balances::Balances,
            system::System,
            mips::Mips,
        }
    }

    #[derive(Clone, Eq, PartialEq, Debug)]
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
        pub const ExistentialDeposit: u64 = 0;
        pub const TransferFee: u64 = 0;
        pub const CreationFee: u64 = 0;
        pub const TransactionBaseFee: u64 = 0;
        pub const TransactionByteFee: u64 = 0;
    }

    impl CommonTrait for Test {
        type Balance = u128;
        type CreationFee = CreationFee;
        type AcceptTransferTarget = Test;
        type BlockRewardsReserve = balances::Module<Test>;
    }

    impl balances::Trait for Test {
        type OnFreeBalanceZero = ();
        type OnNewAccount = ();
        type Event = ();
        type DustRemoval = ();
        type TransferPayment = ();
        type ExistentialDeposit = ExistentialDeposit;
        type TransferFee = TransferFee;
        type Identity = identity::Module<Test>;
    }

    impl identity::Trait for Test {
        type Event = ();
        type Proposal = Call;
        type AddSignerMultiSigTarget = Test;
        type KycServiceProviders = Test;
        type Balances = balances::Module<Test>;
    }

    impl group::GroupTrait for Test {
        fn get_members() -> Vec<IdentityId> {
            unimplemented!()
        }
        fn is_member(_did: &IdentityId) -> bool {
            unimplemented!()
        }
    }

    impl AddSignerMultiSig for Test {
        fn accept_multisig_signer(_: Signatory, _: u64) -> DispatchResult {
            unimplemented!()
        }
    }

    impl AcceptTransfer for Test {
        fn accept_ticker_transfer(_: IdentityId, _: u64) -> DispatchResult {
            unimplemented!()
        }
        fn accept_token_ownership_transfer(_: IdentityId, _: u64) -> DispatchResult {
            unimplemented!()
        }
    }

    parameter_types! {
        pub const MinimumPeriod: u64 = 3;
    }

    impl pallet_timestamp::Trait for Test {
        type Moment = u64;
        type OnTimestampSet = ();
        type MinimumPeriod = MinimumPeriod;
    }

    parameter_types! {
        pub const MinimumProposalDeposit: u128 = 50;
        pub const QuorumThreshold: u128 = 70;
        pub const ProposalDuration: u32 = 10;
        pub const One: u64 = 1;
        pub const Two: u64 = 2;
        pub const Three: u64 = 3;
        pub const Four: u64 = 4;
        pub const Five: u64 = 5;
    }

    impl Trait for Test {
        type Currency = balances::Module<Self>;
        type Proposal = Call;
        type CommitteeOrigin = EnsureSignedBy<One, u64>;
        type Event = ();
    }

    impl group::Trait<group::Instance2> for Test {
        type Event = ();
        type AddOrigin = EnsureSignedBy<One, u64>;
        type RemoveOrigin = EnsureSignedBy<Two, u64>;
        type SwapOrigin = EnsureSignedBy<Three, u64>;
        type ResetOrigin = EnsureSignedBy<Four, u64>;
        type MembershipInitialized = ();
        type MembershipChanged = ();
    }

    type System = system::Module<Test>;
    type Balances = balances::Module<Test>;
    type Mips = Module<Test>;

    fn new_test_ext() -> sp_io::TestExternalities {
        let mut t = system::GenesisConfig::default()
            .build_storage::<Test>()
            .unwrap();

        balances::GenesisConfig::<Test> {
            balances: vec![(1, 10), (2, 20), (3, 30), (4, 40), (5, 50), (6, 60)],
            vesting: vec![],
        }
        .assimilate_storage(&mut t)
        .unwrap();

        GenesisConfig::<Test> {
            min_proposal_deposit: 50,
            quorum_threshold: 70,
            proposal_duration: 10,
        }
        .assimilate_storage(&mut t)
        .unwrap();
        t.into()
    }

    fn next_block() {
        assert_eq!(Mips::end_block(System::block_number()), Ok(()));
        System::set_block_number(System::block_number() + 1);
    }

    fn fast_forward_to(n: u64) {
        while System::block_number() < n {
            next_block();
        }
    }

    fn make_proposal(value: u64) -> Call {
        Call::System(system::Call::remark(value.encode()))
    }

    #[test]
    fn should_start_a_proposal() {
        new_test_ext().execute_with(|| {
            System::set_block_number(1);
            let proposal = make_proposal(42);
            let hash = BlakeTwo256::hash_of(&proposal);
            let proposal_url: Url = b"www.abc.com".into();

            // Error when min deposit requirements are not met
            assert_err!(
                Mips::propose(
                    Origin::signed(6),
                    Box::new(proposal.clone()),
                    40,
                    Some(proposal_url.clone())
                ),
                "deposit is less than minimum required to start a proposal"
            );

            // Account 6 starts a proposal with min deposit
            assert_ok!(Mips::propose(
                Origin::signed(6),
                Box::new(proposal.clone()),
                50,
                Some(proposal_url.clone())
            ));

            assert_eq!(Balances::free_balance(&6), 10);

            assert_eq!(
                Mips::voting(&hash),
                Some(PolymeshVotes {
                    index: 0,
                    ayes: vec![(6, 50)],
                    nays: vec![],
                })
            );
        });
    }

    #[test]
    fn should_close_a_proposal() {
        new_test_ext().execute_with(|| {
            System::set_block_number(1);
            let proposal = make_proposal(42);
            let index = 0;
            let hash = BlakeTwo256::hash_of(&proposal);
            let proposal_url: Url = b"www.abc.com".into();

            // Account 6 starts a proposal with min deposit
            assert_ok!(Mips::propose(
                Origin::signed(6),
                Box::new(proposal.clone()),
                50,
                Some(proposal_url.clone())
            ));

            assert_eq!(Balances::free_balance(&6), 10);

            assert_eq!(
                Mips::voting(&hash),
                Some(PolymeshVotes {
                    index,
                    ayes: vec![(6, 50)],
                    nays: vec![],
                })
            );

            assert_ok!(Mips::kill_proposal(Origin::signed(1), index, hash));

            assert_eq!(Balances::free_balance(&6), 60);

            assert_eq!(Mips::voting(&hash), None);
        });
    }

    #[test]
    fn should_create_a_referendum() {
        new_test_ext().execute_with(|| {
            System::set_block_number(1);
            let proposal = make_proposal(42);
            let hash = BlakeTwo256::hash_of(&proposal);
            let proposal_url: Url = b"www.abc.com".into();

            assert_ok!(Mips::propose(
                Origin::signed(6),
                Box::new(proposal.clone()),
                50,
                Some(proposal_url.clone())
            ));

            assert_ok!(Mips::vote(Origin::signed(5), hash, 0, true, 50));

            assert_eq!(
                Mips::voting(&hash),
                Some(PolymeshVotes {
                    index: 0,
                    ayes: vec![(6, 50), (5, 50)],
                    nays: vec![]
                })
            );

            assert_eq!(Balances::free_balance(&5), 0);
            assert_eq!(Balances::free_balance(&6), 10);

            fast_forward_to(20);

            assert_eq!(Mips::referendums(&hash), Some(proposal));

            assert_eq!(Balances::free_balance(&5), 50);
            assert_eq!(Balances::free_balance(&6), 60);
        });
    }

    #[test]
    fn should_enact_a_referendum() {
        new_test_ext().execute_with(|| {
            System::set_block_number(1);
            let proposal = make_proposal(42);
            let hash = BlakeTwo256::hash_of(&proposal);
            let proposal_url: Url = b"www.abc.com".into();

            assert_ok!(Mips::propose(
                Origin::signed(6),
                Box::new(proposal.clone()),
                50,
                Some(proposal_url.clone())
            ));

            assert_ok!(Mips::vote(Origin::signed(5), hash, 0, true, 50));

            assert_eq!(
                Mips::voting(&hash),
                Some(PolymeshVotes {
                    index: 0,
                    ayes: vec![(6, 50), (5, 50)],
                    nays: vec![]
                })
            );

            fast_forward_to(20);

            assert_eq!(Mips::referendums(&hash), Some(proposal));

            assert_err!(
                Mips::enact_referendum(Origin::signed(5), hash),
                Error::<Test>::BadOrigin
            );

            assert_ok!(Mips::enact_referendum(Origin::signed(1), hash));
        });
    }

    #[test]
    fn should_fast_track_a_proposal() {
        new_test_ext().execute_with(|| {
            System::set_block_number(1);
            let proposal = make_proposal(42);
            let index = 0;
            let hash = BlakeTwo256::hash_of(&proposal);
            let proposal_url: Url = b"www.abc.com".into();

            assert_ok!(Mips::propose(
                Origin::signed(6),
                Box::new(proposal.clone()),
                50,
                Some(proposal_url.clone())
            ));

            assert_ok!(Mips::vote(Origin::signed(5), hash, index, true, 50));

            assert_ok!(Mips::fast_track_proposal(Origin::signed(1), index, hash));

            fast_forward_to(20);

            assert_eq!(Mips::referendums(&hash), Some(proposal));

            assert_err!(
                Mips::enact_referendum(Origin::signed(5), hash),
                Error::<Test>::BadOrigin
            );

            assert_ok!(Mips::enact_referendum(Origin::signed(1), hash));
        });
    }

    #[test]
    fn should_enact_an_emergency_referendum() {
        new_test_ext().execute_with(|| {
            System::set_block_number(1);
            let proposal = make_proposal(42);
            let index = 0;
            let hash = BlakeTwo256::hash_of(&proposal);

            assert_err!(
                Mips::emergency_referendum(Origin::signed(6), Box::new(proposal.clone())),
                Error::<Test>::BadOrigin
            );

            assert_ok!(Mips::emergency_referendum(
                Origin::signed(1),
                Box::new(proposal.clone())
            ));

            fast_forward_to(20);

            assert_eq!(Mips::referendums(&hash), Some(proposal));

            assert_eq!(
                Mips::referendum_meta(),
                vec![PolymeshReferendumInfo {
                    index,
                    priority: MipsPriority::High,
                    proposal_hash: hash
                }]
            );

            assert_err!(
                Mips::enact_referendum(Origin::signed(5), hash),
                Error::<Test>::BadOrigin
            );

            assert_ok!(Mips::enact_referendum(Origin::signed(1), hash));
        });
    }

    #[test]
    fn should_update_mips_variables() {
        new_test_ext().execute_with(|| {
            assert_eq!(Mips::min_proposal_deposit(), 50);
            assert_ok!(Mips::set_min_proposal_deposit(Origin::signed(1), 10));
            assert_eq!(Mips::min_proposal_deposit(), 10);

            assert_eq!(Mips::quorum_threshold(), 70);
            assert_ok!(Mips::set_quorum_threshold(Origin::signed(1), 100));
            assert_eq!(Mips::quorum_threshold(), 100);

            assert_eq!(Mips::proposal_duration(), 10);
            assert_ok!(Mips::set_proposal_duration(Origin::signed(1), 100));
            assert_eq!(Mips::proposal_duration(), 100);
        });
    }
}
