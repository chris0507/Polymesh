//! # General Transfer Manager Module
//!
//! The GTM module provides functionality for setting whitelisting rules for transfers
//!
//! ## Overview
//!
//! The GTM module provides functions for:
//!
//! - Adding rules for allowing transfers
//! - Removing rules that allow transfers
//! - Resetting all rules
//!
//! ### Use case
//!
//! This module is very versatile and offers infinite possibilities.
//! The rules can dictate various requirements like:
//!
//! - Only accredited investors should be able to trade
//! - Only valid CDD holders should be able to trade
//! - Only those with credit score of greater than 800 should be able to purchase this token
//! - People from Wakanda should only be able to trade with people from Wakanda
//! - People from Gryffindor should not be able to trade with people from Slytherin (But allowed to trade with anyone else)
//! - Only marvel supporters should be allowed to buy avengers token
//!
//! ### Terminology
//!
//! - **Active rules:** It is an array of Asset rules that are currently enforced for a ticker
//! - **Asset rule:** Every asset rule contains an array for sender rules and an array for receiver rules
//! - **sender rules:** These are rules that the sender of security tokens must follow
//! - **receiver rules:** These are rules that the receiver of security tokens must follow
//! - **Valid transfer:** For a transfer to be valid,
//!     All reciever and sender rules of any of the active asset rule must be followed.
//!
//! ## Interface
//!
//! ### Dispatchable Functions
//!
//! - `add_active_rule` - Adds a new asset rule to ticker's active rules
//! - `remove_active_rule` - Removes an asset rule from ticker's active rules
//! - `reset_active_rules` - Reset(remove) all active rules of a tikcer
//!
//! ### Public Functions
//!
//! - `verify_restriction` - Checks if a transfer is a valid transfer and returns the result

use crate::asset::{self, AssetTrait};

use polymesh_primitives::{AccountKey, IdentityClaimData, IdentityId, Signatory, Ticker};
use polymesh_runtime_common::{
    balances::Trait as BalancesTrait, constants::*, identity::Trait as IdentityTrait, Context,
};
use polymesh_runtime_identity as identity;

use codec::Encode;
use core::result::Result as StdResult;
use frame_support::{
    decl_error, decl_event, decl_module, decl_storage, dispatch::DispatchResult, ensure,
};
use frame_system::{self as system, ensure_signed};
use sp_std::{convert::TryFrom, prelude::*};

/// Type of claim requirements that a rule can have
#[derive(codec::Encode, codec::Decode, Clone, Copy, PartialEq, Eq, Debug, PartialOrd, Ord)]
pub enum RuleType {
    ClaimIsPresent,
    ClaimIsAbsent,
}

impl Default for RuleType {
    fn default() -> Self {
        RuleType::ClaimIsPresent
    }
}

/// The module's configuration trait.
pub trait Trait:
    pallet_timestamp::Trait + frame_system::Trait + BalancesTrait + IdentityTrait
{
    /// The overarching event type.
    type Event: From<Event> + Into<<Self as frame_system::Trait>::Event>;

    /// Asset module
    type Asset: asset::AssetTrait<Self::Balance, Self::AccountId>;
}

/// An asset rule.
/// All sender and receiver rules of the same asset rule must be true for tranfer to be valid
#[derive(codec::Encode, codec::Decode, Default, Clone, PartialEq, Eq, Debug)]
pub struct AssetRule {
    pub sender_rules: Vec<RuleData>,
    pub receiver_rules: Vec<RuleData>,
}

#[derive(codec::Encode, codec::Decode, Default, Clone, PartialEq, Eq, Debug)]
pub struct AssetRules {
    pub is_paused: bool,
    pub rules: Vec<AssetRule>,
}

/// Details about individual rules
#[derive(codec::Encode, codec::Decode, Default, Clone, PartialEq, Eq, Debug)]
pub struct RuleData {
    /// Claim key
    claim: IdentityClaimData,

    /// Array of trusted claim issuers
    trusted_issuers: Vec<IdentityId>,

    /// Defines if it is a whitelist based rule or a blacklist based rule
    rule_type: RuleType,
}

type Identity<T> = identity::Module<T>;

decl_storage! {
    trait Store for Module<T: Trait> as GeneralTM {
        /// List of active rules for a ticker (Ticker -> Array of AssetRules)
        pub AssetRulesMap get(fn asset_rules): map Ticker => AssetRules;
    }
}

decl_error! {
    pub enum Error for Module<T: Trait> {
        /// The sender must be a signing key for the DID.
        SenderMustBeSigningKeyForDid,
        /// User is not authorized.
        Unauthorized,
    }
}

decl_module! {
    /// The module declaration.
    pub struct Module<T: Trait> for enum Call where origin: T::Origin {
        type Error = Error<T>;

        fn deposit_event() = default;

        /// Adds an asset rule to active rules for a ticker
        pub fn add_active_rule(origin, ticker: Ticker, asset_rule: AssetRule) -> DispatchResult {
            let sender_key = AccountKey::try_from(ensure_signed(origin)?.encode())?;
            let did = Context::current_identity_or::<Identity<T>>(&sender_key)?;
            let sender = Signatory::AccountKey(sender_key);

            // Check that sender is allowed to act on behalf of `did`
            ensure!(
                <identity::Module<T>>::is_signer_authorized(did, &sender),
                Error::<T>::SenderMustBeSigningKeyForDid
            );
            ensure!(Self::is_owner(&ticker, did), Error::<T>::Unauthorized);

            <AssetRulesMap>::mutate(ticker, |old_asset_rules| {
                if !old_asset_rules.rules.contains(&asset_rule) {
                    old_asset_rules.rules.push(asset_rule.clone());
                }
            });

            Self::deposit_event(Event::NewAssetRule(ticker, asset_rule));

            Ok(())
        }

        /// Removes a rule from active asset rules
        pub fn remove_active_rule(origin, ticker: Ticker, asset_rule: AssetRule) -> DispatchResult {
            let sender_key = AccountKey::try_from( ensure_signed(origin)?.encode())?;
            let did = Context::current_identity_or::<Identity<T>>(&sender_key)?;
            let sender = Signatory::AccountKey(sender_key);

            ensure!(
                <identity::Module<T>>::is_signer_authorized(did, &sender),
                Error::<T>::SenderMustBeSigningKeyForDid
            );
            ensure!(Self::is_owner(&ticker, did), Error::<T>::Unauthorized);

            <AssetRulesMap>::mutate(ticker, |old_asset_rules| {
                old_asset_rules.rules.retain( |rule| { *rule != asset_rule });
            });

            Self::deposit_event(Event::RemoveAssetRule(ticker, asset_rule));

            Ok(())
        }

        /// Removes all active rules of a ticker
        pub fn reset_active_rules(origin, ticker: Ticker) -> DispatchResult {
            let sender_key = AccountKey::try_from(ensure_signed(origin)?.encode())?;
            let did = Context::current_identity_or::<Identity<T>>(&sender_key)?;
            let sender = Signatory::AccountKey(sender_key);

            ensure!(
                <identity::Module<T>>::is_signer_authorized(did, &sender),
                Error::<T>::SenderMustBeSigningKeyForDid
            );
            ensure!(Self::is_owner(&ticker, did), Error::<T>::Unauthorized);

            <AssetRulesMap>::remove(ticker);

            Self::deposit_event(Event::ResetAssetRules(ticker));

            Ok(())
        }

        /// It pauses the verification of rules for `ticker` during transfers.
        pub fn pause_asset_rules(origin, ticker: Ticker) -> DispatchResult {
            Self::pause_resume_rules(origin, ticker, true)?;

            Self::deposit_event(Event::PauseAssetRules(ticker));
            Ok(())
        }

        /// It resumes the verification of rules for `ticker` during transfers.
        pub fn resume_asset_rules(origin, ticker: Ticker) -> DispatchResult {
            Self::pause_resume_rules(origin, ticker, false)?;

            Self::deposit_event(Event::ResumeAssetRules(ticker));
            Ok(())
        }
    }
}

decl_event!(
    pub enum Event {
        NewAssetRule(Ticker, AssetRule),
        RemoveAssetRule(Ticker, AssetRule),
        ResetAssetRules(Ticker),
        ResumeAssetRules(Ticker),
        PauseAssetRules(Ticker),
    }
);

impl<T: Trait> Module<T> {
    fn is_owner(ticker: &Ticker, sender_did: IdentityId) -> bool {
        T::Asset::is_owner(ticker, sender_did)
    }

    fn is_any_rule_broken(did: IdentityId, rules: Vec<RuleData>) -> bool {
        for rule in rules {
            let is_valid_claim_present =
                <identity::Module<T>>::is_any_claim_valid(did, rule.claim, rule.trusted_issuers);
            if rule.rule_type == RuleType::ClaimIsPresent && !is_valid_claim_present
                || rule.rule_type == RuleType::ClaimIsAbsent && is_valid_claim_present
            {
                return true;
            }
        }
        return false;
    }

    ///  Sender restriction verification
    pub fn verify_restriction(
        ticker: &Ticker,
        from_did_opt: Option<IdentityId>,
        to_did_opt: Option<IdentityId>,
        _value: T::Balance,
    ) -> StdResult<u8, &'static str> {
        // Transfer is valid if ALL reciever AND sender rules of ANY asset rule are valid.
        let asset_rules = Self::asset_rules(ticker);
        if asset_rules.is_paused {
            return Ok(ERC1400_TRANSFER_SUCCESS);
        }

        for active_rule in asset_rules.rules {
            let mut rule_broken = false;

            if let Some(from_did) = from_did_opt {
                rule_broken = Self::is_any_rule_broken(from_did, active_rule.sender_rules);
                if rule_broken {
                    // Skips checking receiver rules because sender rules are not satisfied.
                    continue;
                }
            }

            if let Some(to_did) = to_did_opt {
                rule_broken = Self::is_any_rule_broken(to_did, active_rule.receiver_rules)
            }

            if !rule_broken {
                return Ok(ERC1400_TRANSFER_SUCCESS);
            }
        }

        sp_runtime::print("Identity TM restrictions not satisfied");
        Ok(ERC1400_TRANSFER_FAILURE)
    }

    pub fn pause_resume_rules(origin: T::Origin, ticker: Ticker, pause: bool) -> DispatchResult {
        let sender_key = AccountKey::try_from(ensure_signed(origin)?.encode())?;
        let did = Context::current_identity_or::<Identity<T>>(&sender_key)?;
        let sender = Signatory::AccountKey(sender_key);

        ensure!(
            <identity::Module<T>>::is_signer_authorized(did, &sender),
            Error::<T>::SenderMustBeSigningKeyForDid
        );
        ensure!(Self::is_owner(&ticker, did), Error::<T>::Unauthorized);

        <AssetRulesMap>::mutate(&ticker, |asset_rules| {
            asset_rules.is_paused = pause;
        });

        Ok(())
    }
}

/// tests for this module
#[cfg(test)]
mod tests {
    use super::*;
    use chrono::prelude::*;
    use frame_support::traits::Currency;
    use frame_support::{
        assert_err, assert_ok, dispatch::DispatchResult, impl_outer_dispatch, impl_outer_origin,
        parameter_types,
    };
    use frame_system::EnsureSignedBy;
    use sp_core::{crypto::key_types, H256};
    use sp_runtime::{
        testing::{Header, UintAuthorityId},
        traits::{BlakeTwo256, ConvertInto, IdentityLookup, OpaqueKeys, Verify},
        AnySignature, KeyTypeId, Perbill,
    };
    use sp_std::result::Result;
    use test_client::{self, AccountKeyring};

    use polymesh_primitives::IdentityId;
    use polymesh_runtime_balances as balances;
    use polymesh_runtime_common::traits::{
        asset::AcceptTransfer, group::GroupTrait, multisig::AddSignerMultiSig, CommonTrait,
    };
    use polymesh_runtime_group as group;
    use polymesh_runtime_identity as identity;

    use crate::{
        asset::{AssetType, Error as AssetError, SecurityToken, TickerRegistrationConfig},
        exemption, percentage_tm, statistics, utils,
    };

    impl_outer_origin! {
        pub enum Origin for Test {}
    }

    impl_outer_dispatch! {
        pub enum Call for Test where origin: Origin {
            pallet_contracts::Contracts,
            identity::Identity,
        }
    }

    // For testing the module, we construct most of a mock runtime. This means
    // first constructing a configuration type (`Test`) which `impl`s each of the
    // configuration traits of modules we want to use.
    #[derive(Clone, Eq, PartialEq)]
    pub struct Test;

    parameter_types! {
        pub const BlockHashCount: u32 = 250;
        pub const MaximumBlockWeight: u32 = 4096;
        pub const MaximumBlockLength: u32 = 4096;
        pub const AvailableBlockRatio: Perbill = Perbill::from_percent(75);
    }

    impl frame_system::Trait for Test {
        type Origin = Origin;
        type Index = u64;
        type BlockNumber = BlockNumber;
        type Call = ();
        type Hash = H256;
        type Hashing = BlakeTwo256;
        type AccountId = AccountId;
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

    impl AcceptTransfer for Test {
        fn accept_ticker_transfer(_to_did: IdentityId, _auth_id: u64) -> DispatchResult {
            unimplemented!();
        }

        fn accept_token_ownership_transfer(_to_did: IdentityId, _auth_id: u64) -> DispatchResult {
            unimplemented!();
        }
    }

    impl GroupTrait for Test {
        fn get_members() -> Vec<IdentityId> {
            unimplemented!();
        }

        fn is_member(_member_id: &IdentityId) -> bool {
            unimplemented!();
        }
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

    parameter_types! {
        pub const MinimumPeriod: u64 = 3;
    }

    type SessionIndex = u32;
    type AuthorityId = <AnySignature as Verify>::Signer;
    type BlockNumber = u64;
    type AccountId = <AnySignature as Verify>::Signer;
    type OffChainSignature = AnySignature;

    impl pallet_timestamp::Trait for Test {
        type Moment = u64;
        type OnTimestampSet = ();
        type MinimumPeriod = MinimumPeriod;
    }

    impl utils::Trait for Test {
        type Public = AccountId;
        type OffChainSignature = OffChainSignature;
        fn validator_id_to_account_id(
            v: <Self as pallet_session::Trait>::ValidatorId,
        ) -> Self::AccountId {
            v
        }
    }

    pub struct TestOnSessionEnding;
    impl pallet_session::OnSessionEnding<AuthorityId> for TestOnSessionEnding {
        fn on_session_ending(_: SessionIndex, _: SessionIndex) -> Option<Vec<AuthorityId>> {
            None
        }
    }

    pub struct TestSessionHandler;
    impl pallet_session::SessionHandler<AuthorityId> for TestSessionHandler {
        const KEY_TYPE_IDS: &'static [KeyTypeId] = &[key_types::DUMMY];
        fn on_new_session<Ks: OpaqueKeys>(
            _changed: bool,
            _validators: &[(AuthorityId, Ks)],
            _queued_validators: &[(AuthorityId, Ks)],
        ) {
        }

        fn on_disabled(_validator_index: usize) {}

        fn on_genesis_session<Ks: OpaqueKeys>(_validators: &[(AuthorityId, Ks)]) {}

        fn on_before_session_ending() {}
    }

    parameter_types! {
        pub const Period: BlockNumber = 1;
        pub const Offset: BlockNumber = 0;
        pub const DisabledValidatorsThreshold: Perbill = Perbill::from_percent(33);
    }

    impl pallet_session::Trait for Test {
        type OnSessionEnding = TestOnSessionEnding;
        type Keys = UintAuthorityId;
        type ShouldEndSession = pallet_session::PeriodicSessions<Period, Offset>;
        type SessionHandler = TestSessionHandler;
        type Event = ();
        type ValidatorId = AuthorityId;
        type ValidatorIdOf = ConvertInto;
        type SelectInitialValidators = ();
        type DisabledValidatorsThreshold = DisabledValidatorsThreshold;
    }

    impl pallet_session::historical::Trait for Test {
        type FullIdentification = ();
        type FullIdentificationOf = ();
    }

    parameter_types! {
        pub const One: AccountId = AccountId::from(AccountKeyring::Dave);
        pub const Two: AccountId = AccountId::from(AccountKeyring::Dave);
        pub const Three: AccountId = AccountId::from(AccountKeyring::Dave);
        pub const Four: AccountId = AccountId::from(AccountKeyring::Dave);
        pub const Five: AccountId = AccountId::from(AccountKeyring::Dave);
    }

    impl group::Trait<group::Instance1> for Test {
        type Event = ();
        type AddOrigin = EnsureSignedBy<One, AccountId>;
        type RemoveOrigin = EnsureSignedBy<Two, AccountId>;
        type SwapOrigin = EnsureSignedBy<Three, AccountId>;
        type ResetOrigin = EnsureSignedBy<Four, AccountId>;
        type MembershipInitialized = ();
        type MembershipChanged = ();
    }

    impl identity::Trait for Test {
        type Event = ();
        type Proposal = Call;
        type AddSignerMultiSigTarget = Test;
        type CddServiceProviders = Test;
        type Balances = balances::Module<Test>;
    }

    impl AddSignerMultiSig for Test {
        fn accept_multisig_signer(_: Signatory, _: u64) -> DispatchResult {
            unimplemented!()
        }
    }

    impl asset::Trait for Test {
        type Event = ();
        type Currency = balances::Module<Test>;
    }

    impl statistics::Trait for Test {}

    impl percentage_tm::Trait for Test {
        type Event = ();
    }

    impl exemption::Trait for Test {
        type Event = ();
        type Asset = asset::Module<Test>;
    }

    impl Trait for Test {
        type Event = ();
        type Asset = asset::Module<Test>;
    }

    parameter_types! {
        pub const SignedClaimHandicap: u64 = 2;
        pub const TombstoneDeposit: u64 = 16;
        pub const StorageSizeOffset: u32 = 8;
        pub const RentByteFee: u64 = 4;
        pub const RentDepositOffset: u64 = 10_000;
        pub const SurchargeReward: u64 = 150;
        pub const ContractTransactionBaseFee: u64 = 2;
        pub const ContractTransactionByteFee: u64 = 6;
        pub const ContractFee: u64 = 21;
        pub const CallBaseFee: u64 = 135;
        pub const InstantiateBaseFee: u64 = 175;
        pub const MaxDepth: u32 = 100;
        pub const MaxValueSize: u32 = 16_384;
        pub const ContractTransferFee: u64 = 50000;
        pub const ContractCreationFee: u64 = 50;
        pub const BlockGasLimit: u64 = 10000000;
    }

    impl pallet_contracts::Trait for Test {
        type Currency = Balances;
        type Time = Timestamp;
        type Randomness = Randomness;
        type Call = Call;
        type Event = ();
        type DetermineContractAddress = pallet_contracts::SimpleAddressDeterminator<Test>;
        type ComputeDispatchFee = pallet_contracts::DefaultDispatchFeeComputor<Test>;
        type TrieIdGenerator = pallet_contracts::TrieIdFromParentCounter<Test>;
        type GasPayment = ();
        type RentPayment = ();
        type SignedClaimHandicap = SignedClaimHandicap;
        type TombstoneDeposit = TombstoneDeposit;
        type StorageSizeOffset = StorageSizeOffset;
        type RentByteFee = RentByteFee;
        type RentDepositOffset = RentDepositOffset;
        type SurchargeReward = SurchargeReward;
        type TransferFee = ContractTransferFee;
        type CreationFee = ContractCreationFee;
        type TransactionBaseFee = ContractTransactionBaseFee;
        type TransactionByteFee = ContractTransactionByteFee;
        type ContractFee = ContractFee;
        type CallBaseFee = CallBaseFee;
        type InstantiateBaseFee = InstantiateBaseFee;
        type MaxDepth = MaxDepth;
        type MaxValueSize = MaxValueSize;
        type BlockGasLimit = BlockGasLimit;
    }

    type Identity = identity::Module<Test>;
    type GeneralTM = Module<Test>;
    type Balances = balances::Module<Test>;
    type Asset = asset::Module<Test>;
    type Timestamp = pallet_timestamp::Module<Test>;
    type Randomness = pallet_randomness_collective_flip::Module<Test>;
    type Contracts = pallet_contracts::Module<Test>;

    /// Build a genesis identity instance owned by the specified account
    fn identity_owned_by_alice() -> sp_io::TestExternalities {
        let mut t = frame_system::GenesisConfig::default()
            .build_storage::<Test>()
            .unwrap();
        identity::GenesisConfig::<Test> {
            owner: AccountKeyring::Alice.public().into(),
            did_creation_fee: 250,
            ..Default::default()
        }
        .assimilate_storage(&mut t)
        .unwrap();
        asset::GenesisConfig::<Test> {
            asset_creation_fee: 0,
            ticker_registration_fee: 0,
            ticker_registration_config: TickerRegistrationConfig {
                max_ticker_length: 12,
                registration_length: Some(10000),
            },
            fee_collector: AccountKeyring::Dave.public().into(),
        }
        .assimilate_storage(&mut t)
        .unwrap();
        sp_io::TestExternalities::new(t)
    }

    fn make_account(
        account_id: &AccountId,
    ) -> Result<(<Test as frame_system::Trait>::Origin, IdentityId), &'static str> {
        let signed_id = Origin::signed(account_id.clone());
        Balances::make_free_balance_be(&account_id, 1_000_000);
        let _ = Identity::register_did(signed_id.clone(), vec![]);
        let did = Identity::get_identity(&AccountKey::try_from(account_id.encode())?).unwrap();
        Ok((signed_id, did))
    }

    #[test]
    fn should_add_and_verify_assetrule() {
        identity_owned_by_alice().execute_with(|| {
            let token_owner_acc = AccountId::from(AccountKeyring::Alice);
            let (token_owner_signed, token_owner_did) = make_account(&token_owner_acc).unwrap();

            // A token representing 1M shares
            let token = SecurityToken {
                name: vec![0x01].into(),
                owner_did: token_owner_did.clone(),
                total_supply: 1_000_000,
                divisible: true,
                asset_type: AssetType::default(),
                ..Default::default()
            };
            let ticker = Ticker::from(token.name.0.as_slice());
            Balances::make_free_balance_be(&token_owner_acc, 1_000_000);

            // Share issuance is successful
            assert_ok!(Asset::create_token(
                token_owner_signed.clone(),
                token.name.clone(),
                ticker,
                token.total_supply,
                true,
                token.asset_type.clone(),
                vec![],
                None
            ));
            let claim_issuer_acc = AccountId::from(AccountKeyring::Bob);
            Balances::make_free_balance_be(&claim_issuer_acc, 1_000_000);
            let (claim_issuer_signed, claim_issuer_did) =
                make_account(&claim_issuer_acc.clone()).unwrap();

            assert_ok!(Identity::add_claim(
                claim_issuer_signed.clone(),
                token_owner_did,
                IdentityClaimData::NoData,
                99999999999999999u64,
            ));

            let now = Utc::now();
            <pallet_timestamp::Module<Test>>::set_timestamp(now.timestamp() as u64);

            let sender_rule = RuleData {
                claim: IdentityClaimData::NoData,
                trusted_issuers: vec![claim_issuer_did],
                rule_type: RuleType::ClaimIsPresent,
            };

            let receiver_rule1 = RuleData {
                claim: IdentityClaimData::CustomerDueDiligence,
                trusted_issuers: vec![claim_issuer_did],
                rule_type: RuleType::ClaimIsAbsent,
            };

            let receiver_rule2 = RuleData {
                claim: IdentityClaimData::Accredited(token_owner_did),
                trusted_issuers: vec![claim_issuer_did],
                rule_type: RuleType::ClaimIsPresent,
            };

            let x = vec![sender_rule];
            let y = vec![receiver_rule1, receiver_rule2];

            let asset_rule = AssetRule {
                sender_rules: x,
                receiver_rules: y,
            };

            assert_ok!(GeneralTM::add_active_rule(
                token_owner_signed.clone(),
                ticker,
                asset_rule
            ));

            assert_ok!(Identity::add_claim(
                claim_issuer_signed.clone(),
                token_owner_did,
                IdentityClaimData::Accredited(claim_issuer_did),
                99999999999999999u64,
            ));

            //Transfer tokens to investor
            assert_err!(
                Asset::transfer(
                    token_owner_signed.clone(),
                    ticker,
                    token_owner_did.clone(),
                    token.total_supply
                ),
                AssetError::<Test>::InvalidTransfer
            );

            assert_ok!(Identity::add_claim(
                claim_issuer_signed.clone(),
                token_owner_did,
                IdentityClaimData::Accredited(token_owner_did),
                99999999999999999u64,
            ));

            assert_ok!(Asset::transfer(
                token_owner_signed.clone(),
                ticker,
                token_owner_did.clone(),
                token.total_supply
            ));

            assert_ok!(Identity::add_claim(
                claim_issuer_signed.clone(),
                token_owner_did,
                IdentityClaimData::CustomerDueDiligence,
                99999999999999999u64,
            ));

            assert_err!(
                Asset::transfer(
                    token_owner_signed.clone(),
                    ticker,
                    token_owner_did.clone(),
                    token.total_supply
                ),
                AssetError::<Test>::InvalidTransfer
            );
        });
    }

    #[test]
    fn should_reset_assetrules() {
        identity_owned_by_alice().execute_with(|| {
            let token_owner_acc = AccountId::from(AccountKeyring::Alice);
            let (token_owner_signed, token_owner_did) = make_account(&token_owner_acc).unwrap();

            // A token representing 1M shares
            let token = SecurityToken {
                name: vec![0x01].into(),
                owner_did: token_owner_did,
                total_supply: 1_000_000,
                divisible: true,
                asset_type: AssetType::default(),
                ..Default::default()
            };
            let ticker = Ticker::from(token.name.0.as_slice());
            Balances::make_free_balance_be(&token_owner_acc, 1_000_000);

            // Share issuance is successful
            assert_ok!(Asset::create_token(
                token_owner_signed.clone(),
                token.name.clone(),
                ticker,
                token.total_supply,
                true,
                token.asset_type.clone(),
                vec![],
                None
            ));

            let asset_rule = AssetRule {
                sender_rules: vec![],
                receiver_rules: vec![],
            };

            assert_ok!(GeneralTM::add_active_rule(
                token_owner_signed.clone(),
                ticker,
                asset_rule
            ));

            let asset_rules = GeneralTM::asset_rules(ticker);
            assert_eq!(asset_rules.rules.len(), 1);

            assert_ok!(GeneralTM::reset_active_rules(
                token_owner_signed.clone(),
                ticker
            ));

            let asset_rules_new = GeneralTM::asset_rules(ticker);
            assert_eq!(asset_rules_new.rules.len(), 0);
        });
    }

    #[test]
    fn pause_resume_asset_rules() {
        identity_owned_by_alice().execute_with(pause_resume_asset_rules_we);
    }

    fn pause_resume_asset_rules_we() {
        // 0. Create accounts
        let token_owner_acc = AccountId::from(AccountKeyring::Alice);
        let (token_owner_signed, token_owner_did) = make_account(&token_owner_acc).unwrap();
        let receiver_acc = AccountId::from(AccountKeyring::Charlie);
        let (receiver_signed, receiver_did) = make_account(&receiver_acc.clone()).unwrap();

        Balances::make_free_balance_be(&receiver_acc, 1_000_000);

        // 1. A token representing 1M shares
        let token = SecurityToken {
            name: vec![0x01].into(),
            owner_did: token_owner_did.clone(),
            total_supply: 1_000_000,
            divisible: true,
            asset_type: AssetType::default(),
            ..Default::default()
        };
        let ticker = Ticker::from(token.name.0.as_slice());
        Balances::make_free_balance_be(&token_owner_acc, 1_000_000);

        // 2. Share issuance is successful
        assert_ok!(Asset::create_token(
            token_owner_signed.clone(),
            token.name.clone(),
            ticker,
            token.total_supply,
            true,
            token.asset_type.clone(),
            vec![],
            None
        ));

        assert_ok!(Identity::add_claim(
            receiver_signed.clone(),
            receiver_did.clone(),
            IdentityClaimData::NoData,
            99999999999999999u64,
        ));

        let now = Utc::now();
        <pallet_timestamp::Module<Test>>::set_timestamp(now.timestamp() as u64);

        // 4. Define rules
        let receiver_rules = vec![RuleData {
            claim: IdentityClaimData::NoData,
            trusted_issuers: vec![receiver_did],
            rule_type: RuleType::ClaimIsAbsent,
        }];

        let asset_rule = AssetRule {
            sender_rules: vec![],
            receiver_rules,
        };

        assert_ok!(GeneralTM::add_active_rule(
            token_owner_signed.clone(),
            ticker,
            asset_rule
        ));

        // 5. Verify pause/resume mechanism.
        // 5.1. Transfer should be cancelled.
        assert_err!(
            Asset::transfer(token_owner_signed.clone(), ticker, receiver_did, 10),
            AssetError::<Test>::InvalidTransfer
        );

        // 5.2. Pause asset rules, and run the transaction.
        assert_ok!(GeneralTM::pause_asset_rules(
            token_owner_signed.clone(),
            ticker
        ));
        assert_ok!(Asset::transfer(
            token_owner_signed.clone(),
            ticker,
            receiver_did,
            10
        ));

        // 5.3. Resume asset rules, and new transfer should fail again.
        assert_ok!(GeneralTM::resume_asset_rules(
            token_owner_signed.clone(),
            ticker
        ));
        assert_err!(
            Asset::transfer(token_owner_signed.clone(), ticker, receiver_did, 10),
            AssetError::<Test>::InvalidTransfer
        );
    }
}
