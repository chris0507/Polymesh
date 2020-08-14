use super::storage::AccountId;
use crate::TestStorage;
use pallet_asset::{self as asset, TickerRegistrationConfig};
use pallet_balances as balances;
use pallet_committee as committee;
use pallet_group as group;
use pallet_identity as identity;
use pallet_pips as pips;
use polymesh_common_utilities::{protocol_fee::ProtocolOp, traits::identity::LinkedKeyInfo};
use polymesh_primitives::{Identity, IdentityId, PosRatio};
use sp_core::sr25519::Public;
use sp_io::TestExternalities;
use std::{cell::RefCell, convert::From, iter};
use test_client::AccountKeyring;

/// A prime number fee to test the split between multiple recipients.
pub const PROTOCOL_OP_BASE_FEE: u128 = 41;

pub const COOL_OFF_PERIOD: u64 = 100;

struct BuilderVoteThreshold {
    pub numerator: u32,
    pub denominator: u32,
}

impl Default for BuilderVoteThreshold {
    fn default() -> Self {
        BuilderVoteThreshold {
            numerator: 2,
            denominator: 3,
        }
    }
}

pub struct MockProtocolBaseFees(pub Vec<(ProtocolOp, u128)>);

impl Default for MockProtocolBaseFees {
    fn default() -> Self {
        let ops = vec![
            ProtocolOp::AssetRegisterTicker,
            ProtocolOp::AssetIssue,
            ProtocolOp::AssetAddDocument,
            ProtocolOp::AssetCreateAsset,
            ProtocolOp::DividendNew,
            ProtocolOp::ComplianceManagerAddActiveRule,
            ProtocolOp::IdentityRegisterDid,
            ProtocolOp::IdentityCddRegisterDid,
            ProtocolOp::IdentityAddClaim,
            ProtocolOp::IdentitySetPrimaryKey,
            ProtocolOp::IdentityAddSecondaryKeysWithAuthorization,
            ProtocolOp::PipsPropose,
            ProtocolOp::VotingAddBallot,
        ];
        let fees = ops
            .into_iter()
            .zip(iter::repeat(PROTOCOL_OP_BASE_FEE))
            .collect();
        MockProtocolBaseFees(fees)
    }
}

#[derive(Default)]
pub struct ExtBuilder {
    /// Minimum weight for the extrinsic (see `weight_to_fee` below).
    extrinsic_base_weight: u64,
    /// The transaction fee per byte.
    /// Transactions with bigger payloads will have a bigger `len_fee`.
    /// This is calculated as `transaction_byte_fee * tx.len()`.
    transaction_byte_fee: u128,
    /// Contributes to the `weight_fee`, indicating the compute requirements of a transaction.
    /// A more resource-intensive transaction will have a higher `weight_fee`.
    weight_to_fee: u128,
    /// Scaling factor for initial balances on genesis.
    balance_factor: u128,
    /// When `false`, no balances will be initialized on genesis.
    monied: bool,
    vesting: bool,
    cdd_providers: Vec<Public>,
    governance_committee_members: Vec<Public>,
    governance_committee_vote_threshold: BuilderVoteThreshold,
    protocol_base_fees: MockProtocolBaseFees,
    protocol_coefficient: PosRatio,
}

thread_local! {
    pub static EXTRINSIC_BASE_WEIGHT: RefCell<u64> = RefCell::new(0);
    pub static TRANSACTION_BYTE_FEE: RefCell<u128> = RefCell::new(0);
    pub static WEIGHT_TO_FEE: RefCell<u128> = RefCell::new(0);
}

impl ExtBuilder {
    /// Sets the minimum weight for the extrinsic (see also `weight_fee`).
    pub fn base_weight(mut self, extrinsic_base_weight: u64) -> Self {
        self.extrinsic_base_weight = extrinsic_base_weight;
        self
    }

    /// Sets the fee per each byte in a transaction.
    /// The full byte fee is defined as: `transaction_byte_fee * tx.len()`.
    pub fn byte_fee(mut self, transaction_byte_fee: u128) -> Self {
        self.transaction_byte_fee = transaction_byte_fee;
        self
    }

    /// Sets the fee to charge per weight.
    /// A more demanding computation will have a higher fee for its weight.
    pub fn weight_fee(mut self, weight_to_fee: u128) -> Self {
        self.weight_to_fee = weight_to_fee;
        self
    }

    /// Sets parameters for transaction fees
    /// (`extrinsic_base_weight`, `transaction_byte_fee`, and `weight_to_fee`).
    /// See the corresponding methods for more details.
    pub fn transaction_fees(
        self,
        extrinsic_base_weight: u64,
        transaction_byte_fee: u128,
        weight_to_fee: u128,
    ) -> Self {
        self.base_weight(extrinsic_base_weight)
            .byte_fee(transaction_byte_fee)
            .weight_fee(weight_to_fee)
    }

    /// Set the scaling factor used for initial balances on genesis to `factor`.
    /// The default is `0`.
    pub fn balance_factor(mut self, factor: u128) -> Self {
        self.balance_factor = factor;
        self
    }

    /// Set whether balances should be initialized on genesis.
    /// This also does `.balance_factor(1)` when it is `0`.
    /// The default is `false`.
    pub fn monied(mut self, monied: bool) -> Self {
        self.monied = monied;
        if self.balance_factor == 0 {
            self.balance_factor = 1;
        }
        self
    }

    pub fn governance_committee(mut self, members: Vec<Public>) -> Self {
        self.governance_committee_members = members;
        self
    }

    pub fn governance_committee_vote_threshold(mut self, threshold: (u32, u32)) -> Self {
        self.governance_committee_vote_threshold = BuilderVoteThreshold {
            numerator: threshold.0,
            denominator: threshold.1,
        };
        self
    }

    /// It sets `providers` as CDD providers.
    pub fn cdd_providers(mut self, providers: Vec<Public>) -> Self {
        self.cdd_providers = providers;
        self
    }

    pub fn set_protocol_base_fees(mut self, fees: MockProtocolBaseFees) -> Self {
        self.protocol_base_fees = fees;
        self
    }

    pub fn set_protocol_coefficient(mut self, coefficient: (u32, u32)) -> Self {
        self.protocol_coefficient = PosRatio::from(coefficient);
        self
    }

    fn set_associated_consts(&self) {
        EXTRINSIC_BASE_WEIGHT.with(|v| *v.borrow_mut() = self.extrinsic_base_weight);
        TRANSACTION_BYTE_FEE.with(|v| *v.borrow_mut() = self.transaction_byte_fee);
        WEIGHT_TO_FEE.with(|v| *v.borrow_mut() = self.weight_to_fee);
    }

    fn make_balances(&self) -> Vec<(Public, u128)> {
        if self.monied {
            vec![
                (AccountKeyring::Alice.public(), 1_000 * self.balance_factor),
                (AccountKeyring::Bob.public(), 2_000 * self.balance_factor),
                (
                    AccountKeyring::Charlie.public(),
                    3_000 * self.balance_factor,
                ),
                (AccountKeyring::Dave.public(), 4_000 * self.balance_factor),
                // CDD Accounts
                (AccountKeyring::Eve.public(), 1_000_000),
                (AccountKeyring::Ferdie.public(), 1_000_000),
            ]
        } else {
            vec![]
        }
    }

    /// It generates, based on CDD providers, a pair of vectors whose contain:
    ///  - mapping between DID and Identity info.
    ///  - mapping between an account key and its DID.
    /// Please note that generated DIDs start from 1.
    fn make_identities(
        accounts: &[Public],
    ) -> (
        Vec<(IdentityId, Identity<AccountId>)>,
        Vec<(AccountId, LinkedKeyInfo)>,
    ) {
        let identities = accounts
            .iter()
            .enumerate()
            .map(|(idx, key)| (IdentityId::from((idx + 1) as u128), Identity::from(*key)))
            .collect::<Vec<_>>();
        let key_links = accounts
            .into_iter()
            .enumerate()
            .map(|(idx, key)| {
                (
                    *key,
                    LinkedKeyInfo::Unique(IdentityId::from((idx + 1) as u128)),
                )
            })
            .collect::<Vec<_>>();

        (identities, key_links)
    }

    /// Create externalities.
    ///
    /// For each `cdd_providers`:
    ///     1. A new `IdentityId` is generated (from 1 to n),
    ///     2. CDD provider's account key is linked to its new Identity ID.
    ///     3. That Identity ID is added as member of CDD provider group.
    pub fn build(self) -> TestExternalities {
        self.set_associated_consts();

        let mut storage = frame_system::GenesisConfig::default()
            .build_storage::<TestStorage>()
            .unwrap();

        let _root = AccountKeyring::Alice.public();

        // Create Identitys.
        let mut system_accounts = self
            .cdd_providers
            .iter()
            .chain(self.governance_committee_members.iter())
            .cloned()
            .collect::<Vec<_>>();
        system_accounts.sort();
        system_accounts.dedup();

        let (system_identities, system_links) = Self::make_identities(system_accounts.as_slice());

        // Identity genesis.
        identity::GenesisConfig::<TestStorage> {
            did_records: system_identities.clone(),
            key_to_identity_ids: system_links,
            identities: vec![],
            ..Default::default()
        }
        .assimilate_storage(&mut storage)
        .unwrap();

        // Balances genesis.
        balances::GenesisConfig::<TestStorage> {
            balances: self.make_balances(),
        }
        .assimilate_storage(&mut storage)
        .unwrap();

        // Asset genesis.
        asset::GenesisConfig::<TestStorage> {
            ticker_registration_config: TickerRegistrationConfig {
                max_ticker_length: 8,
                registration_length: Some(10000),
            },
        }
        .assimilate_storage(&mut storage)
        .unwrap();

        // CDD Service providers.
        let cdd_ids = self
            .cdd_providers
            .iter()
            .map(|key| {
                let (id, _) = system_identities
                    .iter()
                    .find(|(_id, info)| info.primary_key == *key)
                    .unwrap();
                id
            })
            .cloned()
            .collect::<Vec<_>>();

        group::GenesisConfig::<TestStorage, group::Instance2> {
            active_members: cdd_ids,
            ..Default::default()
        }
        .assimilate_storage(&mut storage)
        .unwrap();

        // Committee
        let gc_ids = self
            .governance_committee_members
            .iter()
            .map(|key| {
                let (id, _) = system_identities
                    .iter()
                    .find(|(_id, info)| info.primary_key == *key)
                    .unwrap();
                id
            })
            .cloned()
            .collect::<Vec<_>>();

        group::GenesisConfig::<TestStorage, group::Instance1> {
            active_members: gc_ids.clone(),
            ..Default::default()
        }
        .assimilate_storage(&mut storage)
        .unwrap();

        committee::GenesisConfig::<TestStorage, committee::Instance1> {
            members: gc_ids,
            vote_threshold: (
                self.governance_committee_vote_threshold.numerator,
                self.governance_committee_vote_threshold.denominator,
            ),
            release_coordinator: IdentityId::from(999),
            ..Default::default()
        }
        .assimilate_storage(&mut storage)
        .unwrap();

        pallet_protocol_fee::GenesisConfig::<TestStorage> {
            base_fees: self.protocol_base_fees.0,
            coefficient: self.protocol_coefficient,
        }
        .assimilate_storage(&mut storage)
        .unwrap();

        pips::GenesisConfig::<TestStorage> {
            prune_historical_pips: false,
            min_proposal_deposit: 50,
            proposal_cool_off_period: COOL_OFF_PERIOD,
            default_enactment_period: 100,
            max_pip_skip_count: 1,
            active_pip_limit: 5,
        }
        .assimilate_storage(&mut storage)
        .unwrap();

        sp_io::TestExternalities::new(storage)
    }
}
