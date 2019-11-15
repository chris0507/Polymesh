use crate::asset::{self, AssetTrait};
use crate::balances;
use crate::constants::*;
use crate::identity;
use crate::utils;
use codec::Encode;
use core::result::Result as StdResult;
use identity::ClaimValue;
use primitives::{IdentityId, Key};
use rstd::{convert::TryFrom, prelude::*};
use srml_support::{decl_event, decl_module, decl_storage, dispatch::Result, ensure};
use system::{self, ensure_signed};

#[derive(codec::Encode, codec::Decode, Clone, Copy, PartialEq, Eq, Debug, PartialOrd, Ord)]
pub enum Operators {
    EqualTo,
    NotEqualTo,
    LessThan,
    GreaterThan,
    LessOrEqualTo,
    GreaterOrEqualTo,
}

impl Default for Operators {
    fn default() -> Self {
        Operators::EqualTo
    }
}

/// The module's configuration trait.
pub trait Trait:
    timestamp::Trait + system::Trait + balances::Trait + utils::Trait + identity::Trait
{
    // TODO: Add other types and constants required configure this module.

    /// The overarching event type.
    type Event: From<Event> + Into<<Self as system::Trait>::Event>;
    type Asset: asset::AssetTrait<Self::TokenBalance>;
}

#[derive(codec::Encode, codec::Decode, Default, Clone, PartialEq, Eq, Debug)]
pub struct AssetRule {
    pub sender_rules: Vec<RuleData>,
    pub receiver_rules: Vec<RuleData>,
}

#[derive(codec::Encode, codec::Decode, Default, Clone, PartialEq, Eq, Debug)]
pub struct RuleData {
    key: Vec<u8>,
    value: Vec<u8>,
    trusted_issuers: Vec<IdentityId>,
    operator: Operators,
}

decl_storage! {
    trait Store for Module<T: Trait> as GeneralTM {
        // (Asset -> AssetRules)
        pub ActiveRules get(active_rules): map Vec<u8> => Vec<AssetRule>;
    }
}

decl_module! {
    /// The module declaration.
    pub struct Module<T: Trait> for enum Call where origin: T::Origin {
        fn deposit_event() = default;

        pub fn add_active_rule(origin, did: IdentityId, _ticker: Vec<u8>, asset_rule: AssetRule) -> Result {
            let ticker = utils::bytes_to_upper(_ticker.as_slice());
            let sender = ensure_signed(origin)?;

            // Check that sender is allowed to act on behalf of `did`
            ensure!(<identity::Module<T>>::is_signing_key(did, &Key::try_from(sender.encode())?), "sender must be a signing key for DID");

            ensure!(Self::is_owner(&ticker, did), "user is not authorized");

            <ActiveRules>::mutate(ticker.clone(), |old_asset_rules| {
                if !old_asset_rules.contains(&asset_rule) {
                    old_asset_rules.push(asset_rule.clone());
                }
            });

            Self::deposit_event(Event::NewAssetRule(ticker, asset_rule));

            Ok(())
        }

        pub fn remove_active_rule(origin, did: IdentityId, _ticker: Vec<u8>, asset_rule: AssetRule) -> Result {
            let ticker = utils::bytes_to_upper(_ticker.as_slice());
            let sender = ensure_signed(origin)?;

            ensure!(<identity::Module<T>>::is_signing_key(did, &Key::try_from(sender.encode())?), "sender must be a signing key for DID");

            ensure!(Self::is_owner(&ticker, did), "user is not authorized");

            <ActiveRules>::mutate(ticker.clone(), |old_asset_rules| {
                *old_asset_rules = old_asset_rules
                    .iter()
                    .cloned()
                    .filter(|an_asset_rule| *an_asset_rule != asset_rule)
                    .collect();
            });

            Self::deposit_event(Event::RemoveAssetRule(ticker, asset_rule));

            Ok(())
        }

        pub fn reset_active_rules(origin, did: IdentityId, _ticker: Vec<u8>) -> Result {
            let ticker = utils::bytes_to_upper(_ticker.as_slice());
            let sender = ensure_signed(origin)?;

            ensure!(<identity::Module<T>>::is_signing_key(did, &Key::try_from(sender.encode())?), "sender must be a signing key for DID");

            ensure!(Self::is_owner(&ticker, did), "user is not authorized");

            <ActiveRules>::remove(ticker.clone());

            Self::deposit_event(Event::ResetAssetRules(ticker));

            Ok(())
        }
    }
}

decl_event!(
    pub enum Event {
        NewAssetRule(Vec<u8>, AssetRule),
        RemoveAssetRule(Vec<u8>, AssetRule),
        ResetAssetRules(Vec<u8>),
    }
);

impl<T: Trait> Module<T> {
    pub fn is_owner(ticker: &Vec<u8>, sender_did: IdentityId) -> bool {
        let upper_ticker = utils::bytes_to_upper(ticker);
        T::Asset::is_owner(&upper_ticker, sender_did)
    }

    pub fn fetch_value(
        did: IdentityId,
        key: Vec<u8>,
        trusted_issuers: Vec<IdentityId>,
    ) -> Option<ClaimValue> {
        <identity::Module<T>>::fetch_claim_value_multiple_issuers(did, key, trusted_issuers)
    }

    ///  Sender restriction verification
    pub fn verify_restriction(
        ticker: &Vec<u8>,
        from_did_opt: Option<IdentityId>,
        to_did_opt: Option<IdentityId>,
        _value: T::TokenBalance,
    ) -> StdResult<u8, &'static str> {
        // Transfer is valid if All reciever and sender rules of any asset rule are valid.
        let ticker = utils::bytes_to_upper(ticker.as_slice());
        let active_rules = Self::active_rules(ticker.clone());
        for active_rule in active_rules {
            let mut rule_broken = false;

            if let Some(from_did) = from_did_opt {
                for sender_rule in active_rule.sender_rules {
                    let identity_value = Self::fetch_value(
                        from_did.clone(),
                        sender_rule.key,
                        sender_rule.trusted_issuers,
                    );
                    rule_broken = match identity_value {
                        None => true,
                        Some(x) => utils::is_rule_broken(
                            sender_rule.value,
                            x.value,
                            x.data_type,
                            sender_rule.operator,
                        ),
                    };
                    if rule_broken {
                        break;
                    }
                }
                if rule_broken {
                    continue;
                }
            }

            if let Some(to_did) = to_did_opt {
                for receiver_rule in active_rule.receiver_rules {
                    let identity_value = Self::fetch_value(
                        to_did.clone(),
                        receiver_rule.key,
                        receiver_rule.trusted_issuers,
                    );
                    rule_broken = match identity_value {
                        None => true,
                        Some(x) => utils::is_rule_broken(
                            receiver_rule.value,
                            x.value,
                            x.data_type,
                            receiver_rule.operator,
                        ),
                    };
                    if rule_broken {
                        break;
                    }
                }
            }

            if !rule_broken {
                sr_primitives::print("Satisfied Identity TM restrictions");
                return Ok(ERC1400_TRANSFER_SUCCESS);
            }
        }

        sr_primitives::print("Identity TM restrictions not satisfied");
        Ok(ERC1400_TRANSFER_FAILURE)
    }
}

/// tests for this module
#[cfg(test)]
mod tests {
    use super::*;
    use chrono::prelude::*;
    use sr_io::with_externalities;
    use sr_primitives::{
        testing::{Header, UintAuthorityId},
        traits::{BlakeTwo256, ConvertInto, IdentityLookup, OpaqueKeys, Verify},
        AnySignature, Perbill,
    };
    use srml_support::traits::Currency;
    use srml_support::{assert_ok, impl_outer_origin, parameter_types};
    use std::result::Result;
    use substrate_primitives::{Blake2Hasher, H256};
    use test_client::{self, AccountKeyring};

    use crate::{
        asset::SecurityToken, balances, exemption, identity, identity::DataTypes, percentage_tm,
        registry,
    };

    impl_outer_origin! {
        pub enum Origin for Test {}
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

    impl system::Trait for Test {
        type Origin = Origin;
        type Index = u64;
        type BlockNumber = u64;
        type Hash = H256;
        type Hashing = BlakeTwo256;
        type AccountId = AccountId;
        type Lookup = IdentityLookup<AccountId>;
        type Header = Header;
        type Event = ();
        type Call = ();
        type WeightMultiplierUpdate = ();
        type BlockHashCount = BlockHashCount;
        type MaximumBlockWeight = MaximumBlockWeight;
        type MaximumBlockLength = MaximumBlockLength;
        type AvailableBlockRatio = AvailableBlockRatio;
        type Version = ();
    }

    parameter_types! {
        pub const ExistentialDeposit: u64 = 0;
        pub const TransferFee: u64 = 0;
        pub const CreationFee: u64 = 0;
        pub const TransactionBaseFee: u64 = 0;
        pub const TransactionByteFee: u64 = 0;
    }

    impl balances::Trait for Test {
        type Balance = u128;
        type OnFreeBalanceZero = ();
        type OnNewAccount = ();
        type Event = ();
        type TransactionPayment = ();
        type DustRemoval = ();
        type TransferPayment = ();
        type ExistentialDeposit = ExistentialDeposit;
        type TransferFee = TransferFee;
        type CreationFee = CreationFee;
        type TransactionBaseFee = TransactionBaseFee;
        type TransactionByteFee = TransactionByteFee;
        type WeightToFee = ConvertInto;
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

    impl timestamp::Trait for Test {
        type Moment = u64;
        type OnTimestampSet = ();
        type MinimumPeriod = MinimumPeriod;
    }

    impl utils::Trait for Test {
        type TokenBalance = u128;
        type OffChainSignature = OffChainSignature;
        fn as_u128(v: Self::TokenBalance) -> u128 {
            v
        }
        fn as_tb(v: u128) -> Self::TokenBalance {
            v
        }
        fn token_balance_to_balance(v: Self::TokenBalance) -> <Self as balances::Trait>::Balance {
            v
        }
        fn balance_to_token_balance(v: <Self as balances::Trait>::Balance) -> Self::TokenBalance {
            v
        }
        fn validator_id_to_account_id(v: <Self as session::Trait>::ValidatorId) -> Self::AccountId {
            v
        }
    }

    pub struct TestOnSessionEnding;
    impl session::OnSessionEnding<AuthorityId> for TestOnSessionEnding {
        fn on_session_ending(_: SessionIndex, _: SessionIndex) -> Option<Vec<AuthorityId>> {
            None
        }
    }

    pub struct TestSessionHandler;
    impl session::SessionHandler<AuthorityId> for TestSessionHandler {
        fn on_new_session<Ks: OpaqueKeys>(
            _changed: bool,
            _validators: &[(AuthorityId, Ks)],
            _queued_validators: &[(AuthorityId, Ks)],
        ) {
        }

        fn on_disabled(_validator_index: usize) {}

        fn on_genesis_session<Ks: OpaqueKeys>(_validators: &[(AuthorityId, Ks)]) {}
    }

    parameter_types! {
        pub const Period: BlockNumber = 1;
        pub const Offset: BlockNumber = 0;
        pub const DisabledValidatorsThreshold: Perbill = Perbill::from_percent(33);
    }

    impl session::Trait for Test {
        type OnSessionEnding = TestOnSessionEnding;
        type Keys = UintAuthorityId;
        type ShouldEndSession = session::PeriodicSessions<Period, Offset>;
        type SessionHandler = TestSessionHandler;
        type Event = ();
        type ValidatorId = AuthorityId;
        type ValidatorIdOf = ConvertInto;
        type SelectInitialValidators = ();
        type DisabledValidatorsThreshold = DisabledValidatorsThreshold;
    }

    impl session::historical::Trait for Test {
        type FullIdentification = ();
        type FullIdentificationOf = ();
    }

    impl identity::Trait for Test {
        type Event = ();
    }

    impl asset::Trait for Test {
        type Event = ();
        type Currency = balances::Module<Test>;
    }

    impl percentage_tm::Trait for Test {
        type Event = ();
    }

    impl registry::Trait for Test {}

    impl exemption::Trait for Test {
        type Event = ();
        type Asset = asset::Module<Test>;
    }

    impl Trait for Test {
        type Event = ();
        type Asset = asset::Module<Test>;
    }

    type Identity = identity::Module<Test>;
    type GeneralTM = Module<Test>;
    type Balances = balances::Module<Test>;
    type Asset = asset::Module<Test>;

    /// Build a genesis identity instance owned by the specified account
    fn identity_owned_by_alice() -> sr_io::TestExternalities<Blake2Hasher> {
        let mut t = system::GenesisConfig::default()
            .build_storage::<Test>()
            .unwrap();
        identity::GenesisConfig::<Test> {
            owner: AccountKeyring::Alice.public().into(),
            did_creation_fee: 250,
        }
        .assimilate_storage(&mut t)
        .unwrap();
        sr_io::TestExternalities::new(t)
    }

    fn make_account(
        id: u64,
        account_id: AccountId,
    ) -> Result<(<Test as system::Trait>::Origin, IdentityId), &'static str> {
        let signed_id = Origin::signed(account_id);
        let did = IdentityId::from(id as u128);

        Identity::register_did(signed_id.clone(), did, vec![])?;
        Ok((signed_id, did))
    }

    #[test]
    fn should_add_and_verify_assetrule() {
        with_externalities(&mut identity_owned_by_alice(), || {
            let token_owner_acc = AccountId::from(AccountKeyring::Dave);
            let token_owner_did = IdentityId::from(1u128);

            // A token representing 1M shares
            let token = SecurityToken {
                name: vec![0x01],
                owner_did: token_owner_did.clone(),
                total_supply: 1_000_000,
                granularity: 1,
                decimals: 18,
            };

            Balances::make_free_balance_be(&token_owner_acc, 1_000_000);
            Identity::register_did(
                Origin::signed(token_owner_acc.clone()),
                token_owner_did,
                vec![],
            )
            .expect("Could not create token_owner_did");

            // Share issuance is successful
            assert_ok!(Asset::create_token(
                Origin::signed(token_owner_acc.clone()),
                token_owner_did,
                token.name.clone(),
                token.name.clone(),
                token.total_supply,
                true
            ));
            let claim_issuer_acc = AccountId::from(AccountKeyring::Bob);
            Balances::make_free_balance_be(&claim_issuer_acc, 1_000_000);
            let (_claim_issuer, claim_issuer_did) =
                make_account(3, claim_issuer_acc.clone()).unwrap();

            assert_ok!(Identity::add_claim_issuer(
                Origin::signed(token_owner_acc.clone()),
                token_owner_did,
                claim_issuer_did
            ));

            let claim_value = ClaimValue {
                data_type: DataTypes::VecU8,
                value: "some_value".as_bytes().to_vec(),
            };

            assert_ok!(Identity::add_claim(
                Origin::signed(claim_issuer_acc.clone()),
                token_owner_did,
                "some_key".as_bytes().to_vec(),
                claim_issuer_did,
                99999999999999999u64,
                claim_value.clone()
            ));

            let now = Utc::now();
            <timestamp::Module<Test>>::set_timestamp(now.timestamp() as u64);

            let sender_rule = RuleData {
                key: "some_key".as_bytes().to_vec(),
                value: "some_value".as_bytes().to_vec(),
                trusted_issuers: vec![claim_issuer_did],
                operator: Operators::EqualTo,
            };

            let x = vec![sender_rule];

            let asset_rule = AssetRule {
                sender_rules: x,
                receiver_rules: vec![],
            };

            // Allow all transfers
            assert_ok!(GeneralTM::add_active_rule(
                Origin::signed(token_owner_acc.clone()),
                token_owner_did,
                token.name.clone(),
                asset_rule
            ));

            //Transfer tokens to investor
            assert_ok!(Asset::transfer(
                Origin::signed(token_owner_acc.clone()),
                token_owner_did,
                token.name.clone(),
                token_owner_did,
                token.total_supply
            ));
        });
    }

    #[test]
    fn should_add_and_verify_complex_assetrule() {
        with_externalities(&mut identity_owned_by_alice(), || {
            let token_owner_acc = AccountId::from(AccountKeyring::Dave);
            let token_owner_did = IdentityId::from(1u128);

            // A token representing 1M shares
            let token = SecurityToken {
                name: vec![0x01],
                owner_did: token_owner_did.clone(),
                total_supply: 1_000_000,
                granularity: 1,
                decimals: 18,
            };

            Balances::make_free_balance_be(&token_owner_acc, 1_000_000);
            Identity::register_did(
                Origin::signed(token_owner_acc.clone()),
                token_owner_did,
                vec![],
            )
            .expect("Could not create token_owner_did");

            // Share issuance is successful
            assert_ok!(Asset::create_token(
                Origin::signed(token_owner_acc.clone()),
                token_owner_did,
                token.name.clone(),
                token.name.clone(),
                token.total_supply,
                true
            ));
            let claim_issuer_acc = AccountId::from(AccountKeyring::Bob);
            Balances::make_free_balance_be(&claim_issuer_acc, 1_000_000);
            let (_claim_issuer, claim_issuer_did) =
                make_account(3, claim_issuer_acc.clone()).unwrap();

            assert_ok!(Identity::add_claim_issuer(
                Origin::signed(token_owner_acc.clone()),
                token_owner_did.clone(),
                claim_issuer_did
            ));

            let claim_value = ClaimValue {
                data_type: DataTypes::U8,
                value: 10u8.encode(),
            };

            assert_ok!(Identity::add_claim(
                Origin::signed(claim_issuer_acc.clone()),
                token_owner_did,
                "some_key".as_bytes().to_vec(),
                claim_issuer_did,
                99999999999999999u64,
                claim_value.clone()
            ));

            let now = Utc::now();
            <timestamp::Module<Test>>::set_timestamp(now.timestamp() as u64);

            let sender_rule = RuleData {
                key: "some_key".as_bytes().to_vec(),
                value: 5u8.encode(),
                trusted_issuers: vec![claim_issuer_did],
                operator: Operators::GreaterThan,
            };

            let receiver_rule = RuleData {
                key: "some_key".as_bytes().to_vec(),
                value: 15u8.encode(),
                trusted_issuers: vec![claim_issuer_did],
                operator: Operators::LessThan,
            };

            let x = vec![sender_rule];
            let y = vec![receiver_rule];

            let asset_rule = AssetRule {
                sender_rules: x,
                receiver_rules: y,
            };

            assert_ok!(GeneralTM::add_active_rule(
                Origin::signed(token_owner_acc.clone()),
                token_owner_did,
                token.name.clone(),
                asset_rule
            ));

            //Transfer tokens to investor
            assert_ok!(Asset::transfer(
                Origin::signed(token_owner_acc.clone()),
                token_owner_did,
                token.name.clone(),
                token_owner_did.clone(),
                token.total_supply
            ));
        });
    }

    #[test]
    fn should_reset_assetrules() {
        with_externalities(&mut identity_owned_by_alice(), || {
            let token_owner_acc = AccountId::from(AccountKeyring::Dave);
            let token_owner_did = IdentityId::from(1u128);

            // A token representing 1M shares
            let token = SecurityToken {
                name: vec![0x01],
                owner_did: token_owner_did,
                total_supply: 1_000_000,
                granularity: 1,
                decimals: 18,
            };

            Balances::make_free_balance_be(&token_owner_acc, 1_000_000);
            Identity::register_did(
                Origin::signed(token_owner_acc.clone()),
                token_owner_did,
                vec![],
            )
            .expect("Could not create token_owner_did");

            // Share issuance is successful
            assert_ok!(Asset::create_token(
                Origin::signed(token_owner_acc.clone()),
                token_owner_did,
                token.name.clone(),
                token.name.clone(),
                token.total_supply,
                true
            ));

            let asset_rule = AssetRule {
                sender_rules: vec![],
                receiver_rules: vec![],
            };

            assert_ok!(GeneralTM::add_active_rule(
                Origin::signed(token_owner_acc.clone()),
                token_owner_did,
                token.name.clone(),
                asset_rule
            ));

            let asset_rules = GeneralTM::active_rules(token.name.clone());
            assert_eq!(asset_rules.len(), 1);

            assert_ok!(GeneralTM::reset_active_rules(
                Origin::signed(token_owner_acc.clone()),
                token_owner_did,
                token.name.clone()
            ));

            let asset_rules_new = GeneralTM::active_rules(token.name.clone());
            assert_eq!(asset_rules_new.len(), 0);
        });
    }
}
