use crate::{
    committee_test::root,
    ext_builder::{ExtBuilder, MockProtocolBaseFees},
    pips_test::assert_balance,
    storage::{
        account_from, add_secondary_key, make_account_without_cdd,
        provide_scope_claim_to_multiple_parties, register_keyring_account, AccountId, TestStorage,
    },
};
use frame_support::IterableStorageMap;
use pallet_asset::ethereum;
use pallet_asset::{
    self as asset, AssetOwnershipRelation, AssetType, ClassicTickerImport,
    ClassicTickerRegistration, ClassicTickers, FundingRoundName, SecurityToken, TickerRegistration,
    TickerRegistrationConfig, Tickers,
};
use pallet_balances as balances;
use pallet_compliance_manager as compliance_manager;
use pallet_identity as identity;
use pallet_statistics as statistics;
use polymesh_common_utilities::{
    compliance_manager::Trait, constants::*, protocol_fee::ProtocolOp, traits::balances::Memo,
    traits::CddAndFeeDetails as _, SystematicIssuers,
};
use polymesh_primitives::{
    AssetIdentifier, AuthorizationData, Claim, Condition, ConditionType, Document, DocumentName,
    IdentityId, PortfolioId, Signatory, SmartExtension, SmartExtensionName, SmartExtensionType,
    Ticker,
};
use sp_io::hashing::keccak_256;

use chrono::prelude::Utc;
use frame_support::{
    assert_err, assert_noop, assert_ok, dispatch::DispatchResult, traits::Currency,
    StorageDoubleMap, StorageMap,
};
use hex_literal::hex;
use ink_primitives::hash as FunctionSelectorHasher;
use rand::Rng;
use sp_runtime::AnySignature;
use std::convert::{TryFrom, TryInto};
use test_client::AccountKeyring;

type Identity = identity::Module<TestStorage>;
type Balances = balances::Module<TestStorage>;
type Asset = asset::Module<TestStorage>;
type Timestamp = pallet_timestamp::Module<TestStorage>;
type ComplianceManager = compliance_manager::Module<TestStorage>;
type Portfolio = pallet_portfolio::Module<TestStorage>;
type AssetError = asset::Error<TestStorage>;
type OffChainSignature = AnySignature;
type Origin = <TestStorage as frame_system::Trait>::Origin;
type DidRecords = identity::DidRecords<TestStorage>;
type Statistics = statistics::Module<TestStorage>;
type AssetGenesis = asset::GenesisConfig<TestStorage>;
type System = frame_system::Module<TestStorage>;
type FeeError = pallet_protocol_fee::Error<TestStorage>;

macro_rules! assert_add_claim {
    ($signer:expr, $target:expr, $claim:expr) => {
        assert_ok!(Identity::add_claim($signer, $target, $claim, None,));
    };
}

macro_rules! assert_revoke_claim {
    ($signer:expr, $target:expr, $claim:expr) => {
        assert_ok!(Identity::revoke_claim($signer, $target, $claim,));
    };
}

fn smart_extension_addition(
    account_id: AccountId,
    extension_name: SmartExtensionName,
    signer: Origin,
    ticker: Ticker,
) -> DispatchResult {
    let extension_details = SmartExtension {
        extension_type: SmartExtensionType::TransferManager,
        extension_name: extension_name,
        extension_id: account_id,
        is_archive: false,
    };

    Asset::add_extension(signer, ticker, extension_details.clone())
}

#[test]
fn check_the_test_hex() {
    ExtBuilder::default().build().execute_with(|| {
        let selector: [u8; 4] = (FunctionSelectorHasher::keccak256("verify_transfer".as_bytes())
            [0..4])
            .try_into()
            .unwrap();
        println!("{:#X}", u32::from_be_bytes(selector));
        let data = hex!("D9386E41");
        println!("{:?}", data);
    });
}

#[test]
fn issuers_can_create_and_rename_tokens() {
    ExtBuilder::default().build().execute_with(|| {
        let owner_signed = Origin::signed(AccountKeyring::Dave.public());
        let owner_did = register_keyring_account(AccountKeyring::Dave).unwrap();
        let funding_round_name: FundingRoundName = b"round1".into();
        // Expected token entry
        let token = SecurityToken {
            name: vec![0x01].into(),
            owner_did,
            total_supply: 1_000_000,
            divisible: true,
            asset_type: AssetType::default(),
            primary_issuance_agent: Some(owner_did),
            ..Default::default()
        };
        let ticker = Ticker::try_from(token.name.as_slice()).unwrap();
        assert!(!<DidRecords>::contains_key(
            Identity::get_token_did(&ticker).unwrap()
        ));
        let identifiers = Vec::new();
        let ticker = Ticker::try_from(token.name.as_slice()).unwrap();
        assert_err!(
            Asset::create_asset(
                owner_signed.clone(),
                token.name.clone(),
                ticker,
                1_000_000_000_000_000_000_000_000, // Total supply over the limit
                true,
                token.asset_type.clone(),
                identifiers.clone(),
                Some(funding_round_name.clone()),
            ),
            AssetError::TotalSupplyAboveLimit
        );

        // Issuance is successful
        assert_ok!(Asset::create_asset(
            owner_signed.clone(),
            token.name.clone(),
            ticker,
            token.total_supply,
            true,
            token.asset_type.clone(),
            identifiers.clone(),
            Some(funding_round_name.clone()),
        ));

        // Check the update investor count for the newly created asset
        assert_eq!(Statistics::investor_count_per_asset(ticker), 1);

        // A correct entry is added
        assert_eq!(Asset::token_details(ticker), token);
        assert_eq!(
            Asset::asset_ownership_relation(token.owner_did, ticker),
            AssetOwnershipRelation::AssetOwned
        );
        assert!(<DidRecords>::contains_key(
            Identity::get_token_did(&ticker).unwrap()
        ));
        assert_eq!(Asset::funding_round(ticker), funding_round_name.clone());

        // Unauthorized identities cannot rename the token.
        let eve_signed = Origin::signed(AccountKeyring::Eve.public());
        let _eve_did = register_keyring_account(AccountKeyring::Eve).unwrap();
        assert_err!(
            Asset::rename_asset(eve_signed, ticker, vec![0xde, 0xad, 0xbe, 0xef].into()),
            AssetError::Unauthorized
        );
        // The token should remain unchanged in storage.
        assert_eq!(Asset::token_details(ticker), token);
        // Rename the token and check storage has been updated.
        let renamed_token = SecurityToken {
            name: vec![0x42].into(),
            owner_did: token.owner_did,
            total_supply: token.total_supply,
            divisible: token.divisible,
            asset_type: token.asset_type.clone(),
            primary_issuance_agent: Some(token.owner_did),
            ..Default::default()
        };
        assert_ok!(Asset::rename_asset(
            owner_signed.clone(),
            ticker,
            renamed_token.name.clone()
        ));
        assert_eq!(Asset::token_details(ticker), renamed_token);
        assert!(Asset::identifiers(ticker).is_empty());
    });
}

/// # TODO
/// It should be re-enable once issuer claim is re-enabled.
#[test]
#[ignore]
fn non_issuers_cant_create_tokens() {
    ExtBuilder::default().build().execute_with(|| {
        let _ = Origin::signed(AccountKeyring::Dave.public());
        let owner_did = register_keyring_account(AccountKeyring::Dave).unwrap();

        // Expected token entry
        let _ = SecurityToken {
            name: vec![0x01].into(),
            owner_did,
            total_supply: 1_000_000,
            divisible: true,
            asset_type: AssetType::default(),
            ..Default::default()
        };

        Balances::make_free_balance_be(&AccountKeyring::Bob.public(), 1_000_000);

        let wrong_did = IdentityId::try_from("did:poly:wrong");
        assert!(wrong_did.is_err());
    });
}

#[test]
fn valid_transfers_pass() {
    ExtBuilder::default()
        .cdd_providers(vec![AccountKeyring::Eve.public()])
        .build()
        .execute_with(|| {
            let now = Utc::now();
            Timestamp::set_timestamp(now.timestamp() as u64);

            let owner_signed = Origin::signed(AccountKeyring::Dave.public());
            let owner_did = register_keyring_account(AccountKeyring::Dave).unwrap();

            // Expected token entry
            let token = SecurityToken {
                name: vec![0x01].into(),
                owner_did,
                total_supply: 1_000_000,
                divisible: true,
                asset_type: AssetType::default(),
                ..Default::default()
            };
            let ticker = Ticker::try_from(token.name.as_slice()).unwrap();
            let alice_did = register_keyring_account(AccountKeyring::Alice).unwrap();
            let eve = AccountKeyring::Eve.public();

            // Provide scope claim to sender and receiver of the transaction.
            provide_scope_claim_to_multiple_parties(&[alice_did, owner_did], ticker, eve);

            // Issuance is successful
            assert_ok!(Asset::create_asset(
                owner_signed.clone(),
                token.name.clone(),
                ticker,
                token.total_supply,
                true,
                token.asset_type.clone(),
                vec![],
                None,
            ));

            assert_eq!(
                Asset::balance_of(&ticker, token.owner_did),
                token.total_supply
            );

            // Allow all transfers
            assert_ok!(ComplianceManager::add_compliance_requirement(
                owner_signed.clone(),
                ticker,
                vec![],
                vec![]
            ));

            // Should fail as sender matches receiver
            assert_noop!(
                Asset::base_transfer(
                    PortfolioId::default_portfolio(owner_did),
                    PortfolioId::default_portfolio(owner_did),
                    &ticker,
                    500
                ),
                AssetError::InvalidTransfer
            );

            assert_ok!(Asset::base_transfer(
                PortfolioId::default_portfolio(owner_did),
                PortfolioId::default_portfolio(alice_did),
                &ticker,
                500
            ));

            let balance_alice = <asset::BalanceOf<TestStorage>>::get(&ticker, &alice_did);
            let balance_owner = <asset::BalanceOf<TestStorage>>::get(&ticker, &owner_did);
            assert_eq!(balance_owner, 1_000_000 - 500);
            assert_eq!(balance_alice, 500);
        })
}

#[test]
fn checkpoints_fuzz_test() {
    println!("Starting");
    for _ in 0..10 {
        // When fuzzing in local, feel free to bump this number to add more fuzz runs.
        ExtBuilder::default().build().execute_with(|| {
            let now = Utc::now();
            Timestamp::set_timestamp(now.timestamp() as u64);

            let owner_signed = Origin::signed(AccountKeyring::Dave.public());
            let owner_did = register_keyring_account(AccountKeyring::Dave).unwrap();

            // Expected token entry
            let token = SecurityToken {
                name: vec![0x01].into(),
                owner_did,
                total_supply: 1_000_000,
                divisible: true,
                asset_type: AssetType::default(),
                ..Default::default()
            };
            let ticker = Ticker::try_from(token.name.as_slice()).unwrap();
            let bob_did = register_keyring_account(AccountKeyring::Bob).unwrap();

            // Issuance is successful
            assert_ok!(Asset::create_asset(
                owner_signed.clone(),
                token.name.clone(),
                ticker,
                token.total_supply,
                true,
                token.asset_type.clone(),
                vec![],
                None,
            ));

            // Allow all transfers
            assert_ok!(ComplianceManager::add_compliance_requirement(
                owner_signed.clone(),
                ticker,
                vec![],
                vec![]
            ));
            let mut owner_balance: [u128; 100] = [1_000_000; 100];
            let mut bob_balance: [u128; 100] = [0; 100];
            let mut rng = rand::thread_rng();
            for j in 1..100 {
                let transfers = rng.gen_range(0, 10);
                owner_balance[j] = owner_balance[j - 1];
                bob_balance[j] = bob_balance[j - 1];
                for _k in 0..transfers {
                    if j == 1 {
                        owner_balance[0] -= 1;
                        bob_balance[0] += 1;
                    }
                    owner_balance[j] -= 1;
                    bob_balance[j] += 1;
                    assert_ok!(Asset::unsafe_transfer(
                        PortfolioId::default_portfolio(owner_did),
                        PortfolioId::default_portfolio(bob_did),
                        &ticker,
                        1
                    ));
                }
                assert_ok!(Asset::create_checkpoint(owner_signed.clone(), ticker));
                let x: u64 = u64::try_from(j).unwrap();
                assert_eq!(
                    Asset::get_balance_at(ticker, owner_did, 0),
                    owner_balance[j]
                );
                assert_eq!(Asset::get_balance_at(ticker, bob_did, 0), bob_balance[j]);
                assert_eq!(
                    Asset::get_balance_at(ticker, owner_did, 1),
                    owner_balance[1]
                );
                assert_eq!(Asset::get_balance_at(ticker, bob_did, 1), bob_balance[1]);
                assert_eq!(
                    Asset::get_balance_at(ticker, owner_did, x - 1),
                    owner_balance[j - 1]
                );
                assert_eq!(
                    Asset::get_balance_at(ticker, bob_did, x - 1),
                    bob_balance[j - 1]
                );
                assert_eq!(
                    Asset::get_balance_at(ticker, owner_did, x),
                    owner_balance[j]
                );
                assert_eq!(Asset::get_balance_at(ticker, bob_did, x), bob_balance[j]);
                assert_eq!(
                    Asset::get_balance_at(ticker, owner_did, x + 1),
                    owner_balance[j]
                );
                assert_eq!(
                    Asset::get_balance_at(ticker, bob_did, x + 1),
                    bob_balance[j]
                );
                assert_eq!(
                    Asset::get_balance_at(ticker, owner_did, 1000),
                    owner_balance[j]
                );
                assert_eq!(Asset::get_balance_at(ticker, bob_did, 1000), bob_balance[j]);
            }
        });
    }
}

#[test]
fn register_ticker() {
    ExtBuilder::default().build().execute_with(|| {
        let now = Utc::now();
        Timestamp::set_timestamp(now.timestamp() as u64);

        let owner_signed = Origin::signed(AccountKeyring::Dave.public());
        let owner_did = register_keyring_account(AccountKeyring::Dave).unwrap();

        let token = SecurityToken {
            name: vec![0x01].into(),
            owner_did,
            total_supply: 1_000_000,
            divisible: true,
            asset_type: AssetType::default(),
            ..Default::default()
        };
        let identifiers = vec![AssetIdentifier::isin(*b"US0378331005").unwrap()];
        let ticker = Ticker::try_from(token.name.as_slice()).unwrap();
        // Issuance is successful
        assert_ok!(Asset::create_asset(
            owner_signed.clone(),
            token.name.clone(),
            ticker,
            token.total_supply,
            true,
            token.asset_type.clone(),
            identifiers.clone(),
            None,
        ));

        assert_eq!(Asset::is_ticker_registry_valid(&ticker, owner_did), true);
        assert_eq!(Asset::is_ticker_available(&ticker), false);
        let stored_token = Asset::token_details(&ticker);
        assert_eq!(stored_token.asset_type, token.asset_type);
        assert_eq!(Asset::identifiers(ticker), identifiers);
        assert_err!(
            Asset::register_ticker(owner_signed.clone(), Ticker::try_from(&[0x01][..]).unwrap()),
            AssetError::AssetAlreadyCreated
        );

        assert_err!(
            Asset::register_ticker(
                owner_signed.clone(),
                Ticker::try_from(&[0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01][..])
                    .unwrap()
            ),
            AssetError::TickerTooLong
        );

        let ticker = Ticker::try_from(&[0x01, 0x01][..]).unwrap();

        assert_eq!(Asset::is_ticker_available(&ticker), true);

        assert_ok!(Asset::register_ticker(owner_signed.clone(), ticker));

        assert_eq!(
            Asset::asset_ownership_relation(owner_did, ticker),
            AssetOwnershipRelation::TickerOwned
        );

        let alice_signed = Origin::signed(AccountKeyring::Alice.public());
        let _ = register_keyring_account(AccountKeyring::Alice).unwrap();

        assert_err!(
            Asset::register_ticker(alice_signed.clone(), ticker),
            AssetError::TickerAlreadyRegistered
        );

        assert_eq!(Asset::is_ticker_registry_valid(&ticker, owner_did), true);
        assert_eq!(Asset::is_ticker_available(&ticker), false);

        Timestamp::set_timestamp(now.timestamp() as u64 + 10001);

        assert_eq!(Asset::is_ticker_registry_valid(&ticker, owner_did), false);
        assert_eq!(Asset::is_ticker_available(&ticker), true);
    })
}

#[test]
fn transfer_ticker() {
    ExtBuilder::default().build().execute_with(|| {
        let now = Utc::now();
        Timestamp::set_timestamp(now.timestamp() as u64);

        let owner_signed = Origin::signed(AccountKeyring::Dave.public());
        let owner_did = register_keyring_account(AccountKeyring::Dave).unwrap();
        let alice_signed = Origin::signed(AccountKeyring::Alice.public());
        let alice_did = register_keyring_account(AccountKeyring::Alice).unwrap();
        let bob_signed = Origin::signed(AccountKeyring::Bob.public());
        let bob_did = register_keyring_account(AccountKeyring::Bob).unwrap();

        let ticker = Ticker::try_from(&[0x01, 0x01][..]).unwrap();

        assert_eq!(Asset::is_ticker_available(&ticker), true);
        assert_ok!(Asset::register_ticker(owner_signed.clone(), ticker));

        let auth_id_alice = Identity::add_auth(
            owner_did,
            Signatory::from(alice_did),
            AuthorizationData::TransferTicker(ticker),
            None,
        );

        let auth_id_bob = Identity::add_auth(
            owner_did,
            Signatory::from(bob_did),
            AuthorizationData::TransferTicker(ticker),
            None,
        );

        assert_eq!(Asset::is_ticker_registry_valid(&ticker, owner_did), true);
        assert_eq!(Asset::is_ticker_registry_valid(&ticker, alice_did), false);
        assert_eq!(Asset::is_ticker_available(&ticker), false);

        assert_err!(
            Asset::accept_ticker_transfer(alice_signed.clone(), auth_id_alice + 1),
            "Authorization does not exist"
        );

        assert_eq!(
            Asset::asset_ownership_relation(owner_did, ticker),
            AssetOwnershipRelation::TickerOwned
        );

        assert_ok!(Asset::accept_ticker_transfer(
            alice_signed.clone(),
            auth_id_alice
        ));

        assert_eq!(
            Asset::asset_ownership_relation(owner_did, ticker),
            AssetOwnershipRelation::NotOwned
        );
        assert_eq!(
            Asset::asset_ownership_relation(alice_did, ticker),
            AssetOwnershipRelation::TickerOwned
        );

        assert_eq!(
            Asset::asset_ownership_relation(alice_did, ticker),
            AssetOwnershipRelation::TickerOwned
        );

        assert_err!(
            Asset::accept_ticker_transfer(bob_signed.clone(), auth_id_bob),
            "Illegal use of Authorization"
        );

        let mut auth_id = Identity::add_auth(
            alice_did,
            Signatory::from(bob_did),
            AuthorizationData::TransferTicker(ticker),
            Some(now.timestamp() as u64 - 100),
        );

        assert_err!(
            Asset::accept_ticker_transfer(bob_signed.clone(), auth_id),
            "Authorization expired"
        );

        auth_id = Identity::add_auth(
            alice_did,
            Signatory::from(bob_did),
            AuthorizationData::Custom(ticker),
            Some(now.timestamp() as u64 + 100),
        );

        assert_err!(
            Asset::accept_ticker_transfer(bob_signed.clone(), auth_id),
            AssetError::NoTickerTransferAuth
        );

        auth_id = Identity::add_auth(
            alice_did,
            Signatory::from(bob_did),
            AuthorizationData::TransferTicker(ticker),
            Some(now.timestamp() as u64 + 100),
        );

        assert_ok!(Asset::accept_ticker_transfer(bob_signed.clone(), auth_id));

        assert_eq!(Asset::is_ticker_registry_valid(&ticker, owner_did), false);
        assert_eq!(Asset::is_ticker_registry_valid(&ticker, alice_did), false);
        assert_eq!(Asset::is_ticker_registry_valid(&ticker, bob_did), true);
        assert_eq!(Asset::is_ticker_available(&ticker), false);
    })
}

#[test]
fn transfer_primary_issuance_agent() {
    ExtBuilder::default().build().execute_with(|| {
        let now = Utc::now();
        Timestamp::set_timestamp(now.timestamp() as u64);

        let owner_signed = Origin::signed(AccountKeyring::Alice.public());
        let owner_did = register_keyring_account(AccountKeyring::Alice).unwrap();
        let primary_issuance_signed = Origin::signed(AccountKeyring::Bob.public());
        let primary_issuance_agent = register_keyring_account(AccountKeyring::Bob).unwrap();

        let ticker = Ticker::try_from(&[0x01, 0x01][..]).unwrap();
        let token = SecurityToken {
            name: ticker.as_slice().into(),
            total_supply: 1_000_000,
            owner_did,
            divisible: true,
            asset_type: Default::default(),
            primary_issuance_agent: Some(owner_did),
        };

        assert_ok!(Asset::create_asset(
            owner_signed,
            token.name.clone(),
            ticker.clone(),
            token.total_supply,
            token.divisible,
            token.asset_type.clone(),
            Default::default(),
            Default::default(),
        ));

        assert!(!Asset::is_ticker_available(&ticker));
        assert_eq!(Asset::token_details(&ticker), token);

        let auth_id = Identity::add_auth(
            owner_did,
            Signatory::from(primary_issuance_agent),
            AuthorizationData::TransferPrimaryIssuanceAgent(ticker),
            Some(now.timestamp() as u64 - 100),
        );

        assert_err!(
            Asset::accept_primary_issuance_agent_transfer(primary_issuance_signed.clone(), auth_id),
            "Authorization expired"
        );
        assert_eq!(Asset::token_details(&ticker), token);

        let auth_id = Identity::add_auth(
            owner_did,
            Signatory::from(owner_did),
            AuthorizationData::TransferPrimaryIssuanceAgent(ticker),
            None,
        );

        assert_err!(
            Asset::accept_primary_issuance_agent_transfer(primary_issuance_signed.clone(), auth_id),
            "Authorization does not exist"
        );
        assert_eq!(Asset::token_details(&ticker), token);

        let auth_id = Identity::add_auth(
            primary_issuance_agent,
            Signatory::from(primary_issuance_agent),
            AuthorizationData::TransferPrimaryIssuanceAgent(ticker),
            None,
        );

        assert_err!(
            Asset::accept_primary_issuance_agent_transfer(primary_issuance_signed.clone(), auth_id),
            "Illegal use of Authorization"
        );
        assert_eq!(Asset::token_details(&ticker), token);

        let auth_id = Identity::add_auth(
            owner_did,
            Signatory::from(primary_issuance_agent),
            AuthorizationData::TransferPrimaryIssuanceAgent(ticker),
            None,
        );

        assert_ok!(Asset::accept_primary_issuance_agent_transfer(
            primary_issuance_signed.clone(),
            auth_id
        ));
        let mut new_token = token.clone();
        new_token.primary_issuance_agent = Some(primary_issuance_agent);
        assert_eq!(Asset::token_details(&ticker), new_token);
    })
}

#[test]
fn transfer_token_ownership() {
    ExtBuilder::default().build().execute_with(|| {
        let now = Utc::now();
        Timestamp::set_timestamp(now.timestamp() as u64);

        let owner_signed = Origin::signed(AccountKeyring::Dave.public());
        let owner_did = register_keyring_account(AccountKeyring::Dave).unwrap();
        let alice_signed = Origin::signed(AccountKeyring::Alice.public());
        let alice_did = register_keyring_account(AccountKeyring::Alice).unwrap();
        let bob_signed = Origin::signed(AccountKeyring::Bob.public());
        let bob_did = register_keyring_account(AccountKeyring::Bob).unwrap();

        let token_name = vec![0x01, 0x01];
        let ticker = Ticker::try_from(token_name.as_slice()).unwrap();
        assert_ok!(Asset::create_asset(
            owner_signed.clone(),
            token_name.into(),
            ticker,
            1_000_000,
            true,
            AssetType::default(),
            vec![],
            None,
        ));

        let auth_id_alice = Identity::add_auth(
            owner_did,
            Signatory::from(alice_did),
            AuthorizationData::TransferAssetOwnership(ticker),
            None,
        );

        let auth_id_bob = Identity::add_auth(
            owner_did,
            Signatory::from(bob_did),
            AuthorizationData::TransferAssetOwnership(ticker),
            None,
        );

        assert_eq!(Asset::token_details(&ticker).owner_did, owner_did);

        assert_err!(
            Asset::accept_asset_ownership_transfer(alice_signed.clone(), auth_id_alice + 1),
            "Authorization does not exist"
        );

        assert_eq!(
            Asset::asset_ownership_relation(owner_did, ticker),
            AssetOwnershipRelation::AssetOwned
        );

        assert_ok!(Asset::accept_asset_ownership_transfer(
            alice_signed.clone(),
            auth_id_alice
        ));
        assert_eq!(Asset::token_details(&ticker).owner_did, alice_did);
        assert_eq!(
            Asset::asset_ownership_relation(owner_did, ticker),
            AssetOwnershipRelation::NotOwned
        );
        assert_eq!(
            Asset::asset_ownership_relation(alice_did, ticker),
            AssetOwnershipRelation::AssetOwned
        );

        assert_err!(
            Asset::accept_asset_ownership_transfer(bob_signed.clone(), auth_id_bob),
            "Illegal use of Authorization"
        );

        let mut auth_id = Identity::add_auth(
            alice_did,
            Signatory::from(bob_did),
            AuthorizationData::TransferAssetOwnership(ticker),
            Some(now.timestamp() as u64 - 100),
        );

        assert_err!(
            Asset::accept_asset_ownership_transfer(bob_signed.clone(), auth_id),
            "Authorization expired"
        );

        auth_id = Identity::add_auth(
            alice_did,
            Signatory::from(bob_did),
            AuthorizationData::Custom(ticker),
            Some(now.timestamp() as u64 + 100),
        );

        assert_err!(
            Asset::accept_asset_ownership_transfer(bob_signed.clone(), auth_id),
            AssetError::NotTickerOwnershipTransferAuth
        );

        auth_id = Identity::add_auth(
            alice_did,
            Signatory::from(bob_did),
            AuthorizationData::TransferAssetOwnership(Ticker::try_from(&[0x50][..]).unwrap()),
            Some(now.timestamp() as u64 + 100),
        );

        assert_err!(
            Asset::accept_asset_ownership_transfer(bob_signed.clone(), auth_id),
            AssetError::NoSuchAsset
        );

        auth_id = Identity::add_auth(
            alice_did,
            Signatory::from(bob_did),
            AuthorizationData::TransferAssetOwnership(ticker),
            Some(now.timestamp() as u64 + 100),
        );

        assert_ok!(Asset::accept_asset_ownership_transfer(
            bob_signed.clone(),
            auth_id
        ));
        assert_eq!(Asset::token_details(&ticker).owner_did, bob_did);
    })
}

#[test]
fn update_identifiers() {
    ExtBuilder::default().build().execute_with(|| {
        let owner_signed = Origin::signed(AccountKeyring::Dave.public());
        let owner_did = register_keyring_account(AccountKeyring::Dave).unwrap();

        // Expected token entry
        let token = SecurityToken {
            name: b"TEST".into(),
            owner_did,
            total_supply: 1_000_000,
            divisible: true,
            asset_type: AssetType::default(),
            primary_issuance_agent: Some(owner_did),
            ..Default::default()
        };
        let ticker = Ticker::try_from(token.name.as_slice()).unwrap();
        assert!(!<DidRecords>::contains_key(
            Identity::get_token_did(&ticker).unwrap()
        ));
        let identifier_value1 = b"037833100";
        let identifiers = vec![AssetIdentifier::cusip(*identifier_value1).unwrap()];
        assert_ok!(Asset::create_asset(
            owner_signed.clone(),
            token.name.clone(),
            ticker,
            token.total_supply,
            true,
            token.asset_type.clone(),
            identifiers.clone(),
            None,
        ));

        // A correct entry was added
        assert_eq!(Asset::token_details(ticker), token);
        assert_eq!(Asset::identifiers(ticker), identifiers);
        let identifier_value2 = b"US0378331005";
        let updated_identifiers = vec![
            AssetIdentifier::cusip(*b"17275R102").unwrap(),
            AssetIdentifier::isin(*identifier_value2).unwrap(),
        ];
        assert_ok!(Asset::update_identifiers(
            owner_signed.clone(),
            ticker,
            updated_identifiers.clone(),
        ));
        assert_eq!(Asset::identifiers(ticker), updated_identifiers);
    });
}

#[test]
fn adding_removing_documents() {
    ExtBuilder::default().build().execute_with(|| {
        let owner_signed = Origin::signed(AccountKeyring::Dave.public());
        let owner_did = register_keyring_account(AccountKeyring::Dave).unwrap();

        let token = SecurityToken {
            name: vec![0x01].into(),
            owner_did,
            total_supply: 1_000_000,
            divisible: true,
            asset_type: AssetType::default(),
            ..Default::default()
        };

        let ticker = Ticker::try_from(token.name.as_slice()).unwrap();

        assert!(!<DidRecords>::contains_key(
            Identity::get_token_did(&ticker).unwrap()
        ));

        let identifiers = Vec::new();
        let _ticker_did = Identity::get_token_did(&ticker).unwrap();

        // Issuance is successful
        assert_ok!(Asset::create_asset(
            owner_signed.clone(),
            token.name.clone(),
            ticker,
            token.total_supply,
            true,
            token.asset_type.clone(),
            identifiers.clone(),
            None,
        ));

        let documents = vec![
            (
                b"A".into(),
                Document {
                    uri: b"www.a.com".into(),
                    content_hash: b"0x1".into(),
                },
            ),
            (
                b"B".into(),
                Document {
                    uri: b"www.b.com".into(),
                    content_hash: b"0x2".into(),
                },
            ),
        ];

        assert_ok!(Asset::add_documents(
            owner_signed.clone(),
            documents.clone(),
            ticker
        ));

        assert_eq!(
            Asset::asset_documents(ticker, DocumentName::from(b"A")),
            documents[0].1
        );
        assert_eq!(
            Asset::asset_documents(ticker, DocumentName::from(b"B")),
            documents[1].1
        );

        assert_ok!(Asset::remove_documents(
            owner_signed.clone(),
            vec![b"A".into(), b"B".into()],
            ticker
        ));

        assert_eq!(
            <asset::AssetDocuments>::iter_prefix_values(ticker).count(),
            0
        );
    });
}

#[test]
fn add_extension_successfully() {
    ExtBuilder::default()
        .set_max_tms_allowed(2)
        .build()
        .execute_with(|| {
            let owner_signed = Origin::signed(AccountKeyring::Dave.public());
            let _ = register_keyring_account(AccountKeyring::Dave).unwrap();

            // Expected token entry
            let token = SecurityToken {
                name: b"TEST".into(),
                total_supply: 1_000_000,
                divisible: true,
                asset_type: AssetType::default(),
                ..Default::default()
            };

            let ticker = Ticker::try_from(token.name.as_slice()).unwrap();
            assert!(!<DidRecords>::contains_key(
                Identity::get_token_did(&ticker).unwrap()
            ));
            let identifier_value1 = b"037833100";
            let identifiers = vec![AssetIdentifier::cusip(*identifier_value1).unwrap()];
            assert_ok!(Asset::create_asset(
                owner_signed.clone(),
                token.name.clone(),
                ticker,
                token.total_supply,
                true,
                token.asset_type.clone(),
                identifiers.clone(),
                None,
            ));

            // Add smart extension
            let extension_name_1: SmartExtensionName = b"PTM1".into();
            let extension_name_2 = b"PTM2".into();
            let extension_name_3 = b"PTM3".into();
            let extension_id_1 = account_from(1);
            let extension_id_2 = account_from(2);
            let extension_id_3 = account_from(3);

            assert_ok!(smart_extension_addition(
                extension_id_1,
                extension_name_1.clone(),
                owner_signed.clone(),
                ticker
            ));
            assert_ok!(smart_extension_addition(
                extension_id_2,
                extension_name_2,
                owner_signed.clone(),
                ticker
            ));

            // verify the data within the runtime
            assert_eq!(
                Asset::extension_details((ticker, extension_id_1)).extension_name,
                extension_name_1
            );
            assert_eq!(
                (Asset::extensions((ticker, SmartExtensionType::TransferManager))).len(),
                2
            );
            assert_eq!(
                (Asset::extensions((ticker, SmartExtensionType::TransferManager)))[0],
                extension_id_1
            );

            assert_err!(
                smart_extension_addition(
                    extension_id_3,
                    extension_name_3,
                    owner_signed.clone(),
                    ticker
                ),
                AssetError::MaximumTMExtensionLimitReached
            );
        });
}

#[test]
fn add_same_extension_should_fail() {
    ExtBuilder::default()
        .set_max_tms_allowed(10)
        .build()
        .execute_with(|| {
            let owner_signed = Origin::signed(AccountKeyring::Dave.public());
            let owner_did = register_keyring_account(AccountKeyring::Dave).unwrap();

            // Expected token entry
            let token = SecurityToken {
                name: b"TEST".into(),
                owner_did,
                total_supply: 1_000_000,
                divisible: true,
                asset_type: AssetType::default(),
                ..Default::default()
            };

            let ticker = Ticker::try_from(token.name.as_slice()).unwrap();
            assert!(!<DidRecords>::contains_key(
                Identity::get_token_did(&ticker).unwrap()
            ));
            let identifier_value1 = b"037833100";
            let identifiers = vec![AssetIdentifier::cusip(*identifier_value1).unwrap()];
            assert_ok!(Asset::create_asset(
                owner_signed.clone(),
                token.name.clone(),
                ticker,
                token.total_supply,
                true,
                token.asset_type.clone(),
                identifiers.clone(),
                None,
            ));

            // Add smart extension
            let extension_name = b"PTM".into();
            let extension_id = AccountKeyring::Bob.public();

            let extension_details = SmartExtension {
                extension_type: SmartExtensionType::TransferManager,
                extension_name,
                extension_id: extension_id.clone(),
                is_archive: false,
            };

            assert_ok!(Asset::add_extension(
                owner_signed.clone(),
                ticker,
                extension_details.clone()
            ));

            // verify the data within the runtime
            assert_eq!(
                Asset::extension_details((ticker, extension_id)),
                extension_details.clone()
            );
            assert_eq!(
                (Asset::extensions((ticker, SmartExtensionType::TransferManager))).len(),
                1
            );
            assert_eq!(
                (Asset::extensions((ticker, SmartExtensionType::TransferManager)))[0],
                extension_id
            );

            assert_err!(
                Asset::add_extension(owner_signed.clone(), ticker, extension_details),
                AssetError::ExtensionAlreadyPresent
            );
        });
}

#[test]
fn should_successfully_archive_extension() {
    ExtBuilder::default().build().execute_with(|| {
        let owner_signed = Origin::signed(AccountKeyring::Dave.public());
        let owner_did = register_keyring_account(AccountKeyring::Dave).unwrap();

        // Expected token entry
        let token = SecurityToken {
            name: b"TEST".into(),
            owner_did,
            total_supply: 1_000_000,
            divisible: true,
            asset_type: AssetType::default(),
            ..Default::default()
        };

        let ticker = Ticker::try_from(token.name.as_slice()).unwrap();
        assert!(!<DidRecords>::contains_key(
            Identity::get_token_did(&ticker).unwrap()
        ));
        let identifier_value1 = b"037833100";
        let identifiers = vec![AssetIdentifier::cusip(*identifier_value1).unwrap()];
        assert_ok!(Asset::create_asset(
            owner_signed.clone(),
            token.name.clone(),
            ticker,
            token.total_supply,
            true,
            token.asset_type.clone(),
            identifiers.clone(),
            None,
        ));
        // Add smart extension
        let extension_name = b"STO".into();
        let extension_id = AccountKeyring::Bob.public();

        let extension_details = SmartExtension {
            extension_type: SmartExtensionType::Offerings,
            extension_name,
            extension_id: extension_id.clone(),
            is_archive: false,
        };

        assert_ok!(Asset::add_extension(
            owner_signed.clone(),
            ticker,
            extension_details.clone()
        ));

        // verify the data within the runtime
        assert_eq!(
            Asset::extension_details((ticker, extension_id)),
            extension_details
        );
        assert_eq!(
            (Asset::extensions((ticker, SmartExtensionType::Offerings))).len(),
            1
        );
        assert_eq!(
            (Asset::extensions((ticker, SmartExtensionType::Offerings)))[0],
            extension_id
        );

        assert_ok!(Asset::archive_extension(
            owner_signed.clone(),
            ticker,
            extension_id
        ));

        assert_eq!(
            (Asset::extension_details((ticker, extension_id))).is_archive,
            true
        );
    });
}

#[test]
fn should_fail_to_archive_an_already_archived_extension() {
    ExtBuilder::default().build().execute_with(|| {
        let owner_signed = Origin::signed(AccountKeyring::Dave.public());
        let owner_did = register_keyring_account(AccountKeyring::Dave).unwrap();

        // Expected token entry
        let token = SecurityToken {
            name: b"TEST".into(),
            owner_did,
            total_supply: 1_000_000,
            divisible: true,
            asset_type: AssetType::default(),
            ..Default::default()
        };

        let ticker = Ticker::try_from(token.name.as_slice()).unwrap();
        assert!(!<DidRecords>::contains_key(
            Identity::get_token_did(&ticker).unwrap()
        ));
        let identifier_value1 = b"037833100";
        let identifiers = vec![AssetIdentifier::cusip(*identifier_value1).unwrap()];
        assert_ok!(Asset::create_asset(
            owner_signed.clone(),
            token.name.clone(),
            ticker,
            token.total_supply,
            true,
            token.asset_type.clone(),
            identifiers.clone(),
            None,
        ));
        // Add smart extension
        let extension_name = b"STO".into();
        let extension_id = AccountKeyring::Bob.public();

        let extension_details = SmartExtension {
            extension_type: SmartExtensionType::Offerings,
            extension_name,
            extension_id: extension_id.clone(),
            is_archive: false,
        };

        assert_ok!(Asset::add_extension(
            owner_signed.clone(),
            ticker,
            extension_details.clone()
        ));

        // verify the data within the runtime
        assert_eq!(
            Asset::extension_details((ticker, extension_id)),
            extension_details
        );
        assert_eq!(
            (Asset::extensions((ticker, SmartExtensionType::Offerings))).len(),
            1
        );
        assert_eq!(
            (Asset::extensions((ticker, SmartExtensionType::Offerings)))[0],
            extension_id
        );

        assert_ok!(Asset::archive_extension(
            owner_signed.clone(),
            ticker,
            extension_id
        ));

        assert_eq!(
            (Asset::extension_details((ticker, extension_id))).is_archive,
            true
        );

        assert_err!(
            Asset::archive_extension(owner_signed.clone(), ticker, extension_id),
            AssetError::AlreadyArchived
        );
    });
}

#[test]
fn should_fail_to_archive_a_non_existent_extension() {
    ExtBuilder::default().build().execute_with(|| {
        let owner_signed = Origin::signed(AccountKeyring::Dave.public());
        let owner_did = register_keyring_account(AccountKeyring::Dave).unwrap();

        // Expected token entry
        let token = SecurityToken {
            name: b"TEST".into(),
            owner_did,
            total_supply: 1_000_000,
            divisible: true,
            asset_type: AssetType::default(),
            ..Default::default()
        };

        let ticker = Ticker::try_from(token.name.as_slice()).unwrap();
        assert!(!<DidRecords>::contains_key(
            Identity::get_token_did(&ticker).unwrap()
        ));
        let identifier_value1 = b"037833100";
        let identifiers = vec![AssetIdentifier::cusip(*identifier_value1).unwrap()];
        assert_ok!(Asset::create_asset(
            owner_signed.clone(),
            token.name.clone(),
            ticker,
            token.total_supply,
            true,
            token.asset_type.clone(),
            identifiers.clone(),
            None,
        ));
        // Add smart extension
        let extension_id = AccountKeyring::Bob.public();

        assert_err!(
            Asset::archive_extension(owner_signed.clone(), ticker, extension_id),
            AssetError::NoSuchSmartExtension
        );
    });
}

#[test]
fn should_successfuly_unarchive_an_extension() {
    ExtBuilder::default().build().execute_with(|| {
        let owner_signed = Origin::signed(AccountKeyring::Dave.public());
        let owner_did = register_keyring_account(AccountKeyring::Dave).unwrap();

        // Expected token entry
        let token = SecurityToken {
            name: b"TEST".into(),
            owner_did,
            total_supply: 1_000_000,
            divisible: true,
            asset_type: AssetType::default(),
            ..Default::default()
        };

        let ticker = Ticker::try_from(token.name.as_slice()).unwrap();
        assert!(!<DidRecords>::contains_key(
            Identity::get_token_did(&ticker).unwrap()
        ));
        let identifier_value1 = b"037833100";
        let identifiers = vec![AssetIdentifier::cusip(*identifier_value1).unwrap()];
        assert_ok!(Asset::create_asset(
            owner_signed.clone(),
            token.name.clone(),
            ticker,
            token.total_supply,
            true,
            token.asset_type.clone(),
            identifiers.clone(),
            None,
        ));
        // Add smart extension
        let extension_name = b"STO".into();
        let extension_id = AccountKeyring::Bob.public();

        let extension_details = SmartExtension {
            extension_type: SmartExtensionType::Offerings,
            extension_name,
            extension_id: extension_id.clone(),
            is_archive: false,
        };

        assert_ok!(Asset::add_extension(
            owner_signed.clone(),
            ticker,
            extension_details.clone()
        ));

        // verify the data within the runtime
        assert_eq!(
            Asset::extension_details((ticker, extension_id)),
            extension_details
        );
        assert_eq!(
            (Asset::extensions((ticker, SmartExtensionType::Offerings))).len(),
            1
        );
        assert_eq!(
            (Asset::extensions((ticker, SmartExtensionType::Offerings)))[0],
            extension_id
        );

        assert_ok!(Asset::archive_extension(
            owner_signed.clone(),
            ticker,
            extension_id
        ));

        assert_eq!(
            (Asset::extension_details((ticker, extension_id))).is_archive,
            true
        );

        assert_ok!(Asset::unarchive_extension(
            owner_signed.clone(),
            ticker,
            extension_id
        ));
        assert_eq!(
            (Asset::extension_details((ticker, extension_id))).is_archive,
            false
        );
    });
}

#[test]
fn should_fail_to_unarchive_an_already_unarchived_extension() {
    ExtBuilder::default().build().execute_with(|| {
        let owner_signed = Origin::signed(AccountKeyring::Dave.public());
        let owner_did = register_keyring_account(AccountKeyring::Dave).unwrap();

        // Expected token entry
        let token = SecurityToken {
            name: b"TEST".into(),
            owner_did,
            total_supply: 1_000_000,
            divisible: true,
            asset_type: AssetType::default(),
            ..Default::default()
        };

        let ticker = Ticker::try_from(token.name.as_slice()).unwrap();
        assert!(!<DidRecords>::contains_key(
            Identity::get_token_did(&ticker).unwrap()
        ));
        let identifier_value1 = b"037833100";
        let identifiers = vec![AssetIdentifier::cusip(*identifier_value1).unwrap()];
        assert_ok!(Asset::create_asset(
            owner_signed.clone(),
            token.name.clone(),
            ticker,
            token.total_supply,
            true,
            token.asset_type.clone(),
            identifiers.clone(),
            None,
        ));
        // Add smart extension
        let extension_name = b"STO".into();
        let extension_id = AccountKeyring::Bob.public();

        let extension_details = SmartExtension {
            extension_type: SmartExtensionType::Offerings,
            extension_name,
            extension_id: extension_id.clone(),
            is_archive: false,
        };

        assert_ok!(Asset::add_extension(
            owner_signed.clone(),
            ticker,
            extension_details.clone(),
        ));

        // verify the data within the runtime
        assert_eq!(
            Asset::extension_details((ticker, extension_id)),
            extension_details
        );
        assert_eq!(
            (Asset::extensions((ticker, SmartExtensionType::Offerings))).len(),
            1
        );
        assert_eq!(
            (Asset::extensions((ticker, SmartExtensionType::Offerings)))[0],
            extension_id
        );

        assert_ok!(Asset::archive_extension(
            owner_signed.clone(),
            ticker,
            extension_id
        ));

        assert_eq!(
            (Asset::extension_details((ticker, extension_id))).is_archive,
            true
        );

        assert_ok!(Asset::unarchive_extension(
            owner_signed.clone(),
            ticker,
            extension_id
        ));
        assert_eq!(
            (Asset::extension_details((ticker, extension_id))).is_archive,
            false
        );

        assert_err!(
            Asset::unarchive_extension(owner_signed.clone(), ticker, extension_id),
            AssetError::AlreadyUnArchived
        );
    });
}

#[test]
fn freeze_unfreeze_asset() {
    ExtBuilder::default().build().execute_with(|| {
        let now = Utc::now();
        Timestamp::set_timestamp(now.timestamp() as u64);
        let alice_signed = Origin::signed(AccountKeyring::Alice.public());
        let alice_did = register_keyring_account(AccountKeyring::Alice).unwrap();
        let bob_signed = Origin::signed(AccountKeyring::Bob.public());
        let bob_did = register_keyring_account(AccountKeyring::Bob).unwrap();
        let token_name = b"COOL";
        let ticker = Ticker::try_from(&token_name[..]).unwrap();
        assert_ok!(Asset::create_asset(
            alice_signed.clone(),
            token_name.into(),
            ticker,
            1_000_000,
            true,
            AssetType::default(),
            vec![],
            None,
        ));

        // Allow all transfers.
        assert_ok!(ComplianceManager::add_compliance_requirement(
            alice_signed.clone(),
            ticker,
            vec![],
            vec![]
        ));
        assert_err!(
            Asset::freeze(bob_signed.clone(), ticker),
            AssetError::Unauthorized
        );
        assert_err!(
            Asset::unfreeze(alice_signed.clone(), ticker),
            AssetError::NotFrozen
        );
        assert_ok!(Asset::freeze(alice_signed.clone(), ticker));
        assert_err!(
            Asset::freeze(alice_signed.clone(), ticker),
            AssetError::AlreadyFrozen
        );

        // Attempt to transfer token ownership.
        let auth_id = Identity::add_auth(
            alice_did,
            Signatory::from(bob_did),
            AuthorizationData::TransferAssetOwnership(ticker),
            None,
        );

        assert_ok!(Asset::accept_asset_ownership_transfer(
            bob_signed.clone(),
            auth_id
        ));

        assert_ok!(Asset::unfreeze(bob_signed.clone(), ticker));
        assert_err!(
            Asset::unfreeze(bob_signed.clone(), ticker),
            AssetError::NotFrozen
        );
    });
}
#[test]
fn frozen_secondary_keys_create_asset() {
    ExtBuilder::default()
        .build()
        .execute_with(frozen_secondary_keys_create_asset_we);
}

fn frozen_secondary_keys_create_asset_we() {
    // 0. Create identities.
    let alice = AccountKeyring::Alice.public();
    let alice_id = register_keyring_account(AccountKeyring::Alice).unwrap();
    let _charlie_id = register_keyring_account(AccountKeyring::Charlie).unwrap();
    let bob = AccountKeyring::Bob.public();

    // 1. Add Bob as signatory to Alice ID.
    let bob_signatory = Signatory::Account(AccountKeyring::Bob.public());
    add_secondary_key(alice_id, bob_signatory);
    assert_ok!(Balances::transfer_with_memo(
        Origin::signed(alice),
        bob,
        1_000,
        Some(Memo::from("Bob funding"))
    ));

    // 2. Bob can create token
    let token_1 = SecurityToken {
        name: vec![0x01].into(),
        owner_did: alice_id,
        total_supply: 1_000_000,
        divisible: true,
        asset_type: AssetType::default(),
        primary_issuance_agent: Some(alice_id),
        ..Default::default()
    };
    let ticker_1 = Ticker::try_from(token_1.name.as_slice()).unwrap();
    assert_ok!(Asset::create_asset(
        Origin::signed(bob),
        token_1.name.clone(),
        ticker_1,
        token_1.total_supply,
        true,
        token_1.asset_type.clone(),
        vec![],
        None,
    ));
    assert_eq!(Asset::token_details(ticker_1), token_1);

    // 3. Alice freezes her secondary keys.
    assert_ok!(Identity::freeze_secondary_keys(Origin::signed(alice)));

    // 4. Bob cannot create a token.
    let token_2 = SecurityToken {
        name: vec![0x01].into(),
        owner_did: alice_id,
        total_supply: 1_000_000,
        divisible: true,
        asset_type: AssetType::default(),
        ..Default::default()
    };
    let _ticker_2 = Ticker::try_from(token_2.name.as_slice()).unwrap();
    // commenting this because `default_identity` feature is not allowing to access None identity.
    // let create_token_result = Asset::create_asset(
    //     Origin::signed(bob),
    //     token_2.name.clone(),
    //     ticker_2,
    //     token_2.total_supply,
    //     true,
    //     token_2.asset_type.clone(),
    //     vec![],
    //     None,
    // );
    // assert_err!(
    //     create_token_result,
    //     DispatchError::Other("Current identity is none and key is not linked to any identity")
    // );
}

#[test]
fn test_can_transfer_rpc() {
    ExtBuilder::default()
        .cdd_providers(vec![AccountKeyring::Eve.public()])
        .monied(true)
        .balance_factor(1)
        .build()
        .execute_with(|| {
            let alice_signed = Origin::signed(AccountKeyring::Alice.public());
            let alice_did = register_keyring_account(AccountKeyring::Alice).unwrap();
            let _bob_signed = Origin::signed(AccountKeyring::Bob.public());
            let bob_did = register_keyring_account(AccountKeyring::Bob).unwrap();

            let eve = AccountKeyring::Eve.public();

            let token_name = b"COOL";
            let ticker = Ticker::try_from(&token_name[..]).unwrap();
            assert_ok!(Asset::create_asset(
                alice_signed.clone(),
                token_name.into(),
                ticker,
                1_000 * currency::ONE_UNIT,
                false, // Asset divisibility is false
                AssetType::default(),
                vec![],
                None,
            ));

            // check the balance of the alice Identity
            assert_eq!(
                Asset::balance_of(&ticker, alice_did),
                1_000 * currency::ONE_UNIT
            );

            // Provide scope claim for sender and receiver.
            provide_scope_claim_to_multiple_parties(&[alice_did, bob_did], ticker, eve);

            let unsafe_can_transfer_result = |sender_account, from_did, to_did, amount| {
                Asset::unsafe_can_transfer(
                    sender_account,
                    None,
                    PortfolioId::default_portfolio(from_did),
                    None,
                    PortfolioId::default_portfolio(to_did),
                    &ticker,
                    amount, // It only passed when it should be the multiple of currency::ONE_UNIT
                )
                .unwrap()
            };

            // case 1: When passed invalid granularity
            assert_eq!(
                unsafe_can_transfer_result(AccountKeyring::Alice.public(), alice_did, bob_did, 100),
                INVALID_GRANULARITY
            );

            // Case 2: when from_did balance is 0
            assert_eq!(
                unsafe_can_transfer_result(
                    AccountKeyring::Bob.public(),
                    bob_did,
                    alice_did,
                    100 * currency::ONE_UNIT
                ),
                ERC1400_INSUFFICIENT_BALANCE
            );

            // Comment below test case, These will be un-commented when we improved the DID check.

            // // Case 4: When sender doesn't posses a valid cdd
            // // 4.1: Create Identity who doesn't posses cdd
            // let (from_without_cdd_signed, from_without_cdd_did) = make_account(AccountKeyring::Ferdie.public()).unwrap();
            // // Execute can_transfer
            // assert_eq!(
            //     Asset::unsafe_can_transfer(
            //         AccountKeyring::Ferdie.public(),
            //         ticker,
            //         Some(from_without_cdd_did),
            //         Some(bob_did),
            //         5 * currency::ONE_UNIT
            //     ).unwrap(),
            //     INVALID_SENDER_DID
            // );

            // // Case 5: When receiver doesn't posses a valid cdd
            // assert_eq!(
            //     Asset::unsafe_can_transfer(
            //         AccountKeyring::Alice.public(),
            //         ticker,
            //         Some(alice_did),
            //         Some(from_without_cdd_did),
            //         5 * currency::ONE_UNIT
            //     ).unwrap(),
            //     INVALID_RECEIVER_DID
            // );

            // Case 6: When Asset transfer is frozen
            // 6.1: pause the transfer
            assert_ok!(Asset::freeze(alice_signed.clone(), ticker));
            assert_eq!(
                unsafe_can_transfer_result(
                    AccountKeyring::Alice.public(),
                    alice_did,
                    bob_did,
                    20 * currency::ONE_UNIT
                ),
                ERC1400_TRANSFERS_HALTED
            );
            assert_ok!(Asset::unfreeze(alice_signed.clone(), ticker));

            // Case 7: when transaction get success by the compliance_manager
            // Allow all transfers.
            assert_ok!(ComplianceManager::add_compliance_requirement(
                alice_signed.clone(),
                ticker,
                vec![],
                vec![]
            ));

            assert_eq!(
                unsafe_can_transfer_result(
                    AccountKeyring::Bob.public(),
                    alice_did,
                    bob_did,
                    20 * currency::ONE_UNIT
                ),
                ERC1400_TRANSFER_SUCCESS
            );
        })
}

#[test]
fn can_set_primary_issuance_agent() {
    ExtBuilder::default()
        .build()
        .execute_with(can_set_primary_issuance_agent_we);
}

fn can_set_primary_issuance_agent_we() {
    let alice = AccountKeyring::Alice.public();
    let alice_id = register_keyring_account(AccountKeyring::Alice).unwrap();
    let bob = AccountKeyring::Bob.public();
    let bob_id = register_keyring_account(AccountKeyring::Bob).unwrap();
    assert_ok!(Balances::transfer_with_memo(
        Origin::signed(alice),
        bob,
        1_000,
        Some(Memo::from("Bob funding"))
    ));
    let mut token = SecurityToken {
        name: vec![0x01].into(),
        owner_did: alice_id,
        total_supply: 1_000_000,
        divisible: true,
        asset_type: AssetType::default(),
        primary_issuance_agent: Some(bob_id),
        ..Default::default()
    };
    let ticker = Ticker::try_from(token.name.as_slice()).unwrap();

    assert_ok!(Asset::create_asset(
        Origin::signed(alice),
        token.name.clone(),
        ticker,
        token.total_supply,
        true,
        token.asset_type.clone(),
        vec![],
        None,
    ));
    let auth_id = Identity::add_auth(
        token.owner_did,
        Signatory::from(bob_id),
        AuthorizationData::TransferPrimaryIssuanceAgent(ticker),
        None,
    );
    assert_ok!(Asset::accept_primary_issuance_agent_transfer(
        Origin::signed(bob),
        auth_id
    ));
    assert_eq!(Asset::token_details(ticker), token);
    assert_ok!(Asset::remove_primary_issuance_agent(
        Origin::signed(alice),
        ticker
    ));
    token.primary_issuance_agent = None;
    assert_eq!(Asset::token_details(ticker), token);
}

#[test]
fn test_weights_for_is_valid_transfer() {
    ExtBuilder::default()
        .cdd_providers(vec![AccountKeyring::Dave.public()])
        .set_max_tms_allowed(4)
        .build()
        .execute_with(|| {
            let alice = AccountKeyring::Alice.public();
            let (alice_signed, alice_did) = make_account_without_cdd(alice).unwrap();

            let bob = AccountKeyring::Bob.public();
            let (bob_signed, bob_did) = make_account_without_cdd(bob).unwrap();

            let charlie = AccountKeyring::Charlie.public();
            let (charlie_signed, charlie_did) = make_account_without_cdd(charlie).unwrap();

            let eve = AccountKeyring::Eve.public();
            let (eve_signed, eve_did) = make_account_without_cdd(eve).unwrap();

            let dave = AccountKeyring::Dave.public();

            let token = SecurityToken {
                name: vec![0x01].into(),
                owner_did: alice_did,
                total_supply: 1_000_000_000,
                divisible: true,
                asset_type: AssetType::default(),
                primary_issuance_agent: Some(alice_did),
                ..Default::default()
            };
            let ticker = Ticker::try_from(token.name.as_slice()).unwrap();

            assert_ok!(Asset::create_asset(
                Origin::signed(alice),
                token.name.clone(),
                ticker,
                token.total_supply,
                true,
                token.asset_type.clone(),
                vec![],
                None,
            ));

            // Provide scope claim to sender and receiver.
            provide_scope_claim_to_multiple_parties(&[alice_did, bob_did], ticker, dave);
            // Get token Id.
            let ticker_id = Identity::get_token_did(&ticker).unwrap();

            // Adding different compliance requirements
            assert_ok!(ComplianceManager::add_compliance_requirement(
                alice_signed.clone(),
                ticker,
                vec![
                    Condition {
                        condition_type: ConditionType::IsPresent(Claim::Accredited(
                            ticker_id.into()
                        )),
                        issuers: vec![eve_did]
                    },
                    Condition {
                        condition_type: ConditionType::IsAbsent(Claim::BuyLockup(ticker_id.into())),
                        issuers: vec![eve_did]
                    }
                ],
                vec![
                    Condition {
                        condition_type: ConditionType::IsPresent(Claim::Accredited(
                            ticker_id.into()
                        )),
                        issuers: vec![eve_did]
                    },
                    Condition {
                        condition_type: ConditionType::IsAnyOf(vec![
                            Claim::BuyLockup(ticker_id.into()),
                            Claim::KnowYourCustomer(ticker_id.into())
                        ]),
                        issuers: vec![eve_did]
                    }
                ]
            ));

            // Providing claim to sender and receiver
            // For Alice
            assert_add_claim!(
                eve_signed.clone(),
                alice_did,
                Claim::Accredited(alice_did.into())
            );
            assert_add_claim!(
                eve_signed.clone(),
                alice_did,
                Claim::BuyLockup(ticker_id.into())
            );
            // For Bob
            assert_add_claim!(
                eve_signed.clone(),
                bob_did,
                Claim::Accredited(ticker_id.into())
            );
            assert_add_claim!(
                eve_signed.clone(),
                bob_did,
                Claim::KnowYourCustomer(ticker_id.into())
            );

            // Add Tms
            assert_ok!(Asset::add_extension(
                alice_signed.clone(),
                ticker,
                SmartExtension {
                    extension_type: SmartExtensionType::TransferManager,
                    extension_name: b"ABC".into(),
                    extension_id: account_from(1),
                    is_archive: false
                }
            ));

            assert_ok!(Asset::add_extension(
                alice_signed.clone(),
                ticker,
                SmartExtension {
                    extension_type: SmartExtensionType::TransferManager,
                    extension_name: b"ABC".into(),
                    extension_id: account_from(2),
                    is_archive: false
                }
            ));

            let is_valid_transfer_result = || {
                Asset::_is_valid_transfer(
                    &ticker,
                    alice,
                    PortfolioId::default_portfolio(alice_did),
                    PortfolioId::default_portfolio(bob_did),
                    100,
                )
                .unwrap()
                .1
            };

            let verify_restriction_weight = || {
                ComplianceManager::verify_restriction(
                    &ticker,
                    Some(alice_did),
                    Some(bob_did),
                    100,
                    Some(alice_did),
                )
                .unwrap()
                .1
            };

            // call is_valid_transfer()
            let result = is_valid_transfer_result();
            let weight_from_verify_transfer = verify_restriction_weight();
            assert!(matches!(result, weight_from_verify_transfer)); // Only sender rules are processed.

            assert_revoke_claim!(
                eve_signed.clone(),
                alice_did,
                Claim::BuyLockup(ticker_id.into())
            );
            assert_add_claim!(
                eve_signed.clone(),
                alice_did,
                Claim::Accredited(ticker_id.into())
            );

            let result = is_valid_transfer_result();
            let weight_from_verify_transfer = verify_restriction_weight();
            let computed_weight =
                Asset::compute_transfer_result(false, 2, weight_from_verify_transfer).1;
            assert!(matches!(result, computed_weight)); // Sender & receiver rules are processed.

            // Adding different claim rules
            assert_ok!(ComplianceManager::add_compliance_requirement(
                alice_signed.clone(),
                ticker,
                vec![Condition {
                    condition_type: ConditionType::IsPresent(Claim::Exempted(ticker_id.into())),
                    issuers: vec![eve_did]
                }],
                vec![Condition {
                    condition_type: ConditionType::IsPresent(Claim::Blocked(ticker_id.into())),
                    issuers: vec![eve_did]
                }]
            ));
            let result = is_valid_transfer_result();
            let weight_from_verify_transfer = verify_restriction_weight();
            let computed_weight =
                Asset::compute_transfer_result(false, 2, weight_from_verify_transfer).1;
            assert!(matches!(result, computed_weight)); // Sender & receiver rules are processed.

            // pause transfer rules
            assert_ok!(ComplianceManager::pause_asset_compliance(
                alice_signed,
                ticker
            ));

            let result = is_valid_transfer_result();
            let weight_from_verify_transfer = verify_restriction_weight();
            let computed_weight =
                Asset::compute_transfer_result(false, 2, weight_from_verify_transfer).1;
            assert!(matches!(result, computed_weight));
        });
}

#[test]
fn check_functionality_of_remove_extension() {
    ExtBuilder::default()
        .set_max_tms_allowed(5)
        .build()
        .execute_with(|| {
            let alice = AccountKeyring::Alice.public();
            let (alice_signed, alice_did) = make_account_without_cdd(alice).unwrap();

            let token = SecurityToken {
                name: vec![0x01].into(),
                owner_did: alice_did,
                total_supply: 1_000_000_000,
                divisible: true,
                asset_type: AssetType::default(),
                primary_issuance_agent: Some(alice_did),
                ..Default::default()
            };
            let ticker = Ticker::try_from(token.name.as_slice()).unwrap();

            assert_ok!(Asset::create_asset(
                alice_signed.clone(),
                token.name.clone(),
                ticker,
                token.total_supply,
                true,
                token.asset_type.clone(),
                vec![],
                None,
            ));

            let extension_id = account_from(1);

            // Add Tms
            assert_ok!(Asset::add_extension(
                alice_signed.clone(),
                ticker,
                SmartExtension {
                    extension_type: SmartExtensionType::TransferManager,
                    extension_name: b"ABC".into(),
                    extension_id: extension_id,
                    is_archive: false
                }
            ));

            // verify storage
            assert_eq!(
                Asset::extensions((ticker, SmartExtensionType::TransferManager)),
                vec![extension_id]
            );
            // Remove the extension
            assert_ok!(Asset::remove_smart_extension(
                alice_signed.clone(),
                ticker,
                extension_id
            ));

            // verify storage
            assert_eq!(
                Asset::extensions((ticker, SmartExtensionType::TransferManager)),
                vec![]
            );

            // Removing the same extension gives the error.
            assert_err!(
                Asset::remove_smart_extension(alice_signed.clone(), ticker, extension_id),
                AssetError::NoSuchSmartExtension
            );
        });
}

// Classic token tests:

fn ticker(name: &str) -> Ticker {
    name.as_bytes().try_into().unwrap()
}

fn default_classic() -> ClassicTickerImport {
    ClassicTickerImport {
        eth_owner: <_>::default(),
        ticker: <_>::default(),
        is_created: false,
        is_contract: false,
    }
}

fn default_reg_config() -> TickerRegistrationConfig<u64> {
    TickerRegistrationConfig {
        max_ticker_length: 8,
        registration_length: Some(10000),
    }
}

fn alice_secret_key() -> secp256k1::SecretKey {
    secp256k1::SecretKey::parse(&keccak_256(b"Alice")).unwrap()
}

fn bob_secret_key() -> secp256k1::SecretKey {
    secp256k1::SecretKey::parse(&keccak_256(b"Bob")).unwrap()
}

fn sorted<K: Ord + Clone, V>(iter: impl IntoIterator<Item = (K, V)>) -> Vec<(K, V)> {
    let mut vec: Vec<_> = iter.into_iter().collect();
    vec.sort_by_key(|x| x.0.clone());
    vec
}

fn with_asset_genesis(genesis: AssetGenesis) -> ExtBuilder {
    ExtBuilder::default().adjust(Box::new(move |storage| {
        genesis.assimilate_storage(storage).unwrap();
    }))
}

fn test_asset_genesis(genesis: AssetGenesis) {
    with_asset_genesis(genesis).build().execute_with(|| {});
}

#[test]
#[should_panic = "lowercase ticker"]
fn classic_ticker_genesis_lowercase() {
    test_asset_genesis(AssetGenesis {
        classic_migration_tickers: vec![ClassicTickerImport {
            ticker: ticker("lower"),
            ..default_classic()
        }],
        ..<_>::default()
    });
}

#[test]
#[should_panic = "TickerTooLong"]
fn classic_ticker_genesis_too_long() {
    test_asset_genesis(AssetGenesis {
        classic_migration_tconfig: TickerRegistrationConfig {
            max_ticker_length: 3,
            registration_length: None,
        },
        classic_migration_tickers: vec![ClassicTickerImport {
            ticker: ticker("ACME"),
            ..default_classic()
        }],
        ..<_>::default()
    });
}

#[test]
#[should_panic = "TickerAlreadyRegistered"]
fn classic_ticker_genesis_already_registered_sys_did() {
    let import = ClassicTickerImport {
        ticker: ticker("ACME"),
        ..default_classic()
    };
    test_asset_genesis(AssetGenesis {
        classic_migration_tconfig: default_reg_config(),
        classic_migration_tickers: vec![import.clone(), import],
        ..<_>::default()
    });
}

#[test]
#[should_panic = "TickerAlreadyRegistered"]
fn classic_ticker_genesis_already_registered_other_did() {
    let import_a = ClassicTickerImport {
        ticker: ticker("ACME"),
        ..default_classic()
    };
    let import_b = ClassicTickerImport {
        is_contract: true,
        ..import_a.clone()
    };
    test_asset_genesis(AssetGenesis {
        classic_migration_contract_did: 1.into(),
        classic_migration_tconfig: default_reg_config(),
        classic_migration_tickers: vec![import_a, import_b],
        ..<_>::default()
    });
}

#[test]
fn classic_ticker_genesis_works() {
    let alice_eth = ethereum::EthereumAddress(*b"0x012345678987654321");
    let bob_eth = ethereum::EthereumAddress(*b"0x212345678987654321");
    let charlie_eth = ethereum::EthereumAddress(*b"0x512345678987654321");

    // Define actual on-genesis asset config.
    let classic_migration_tickers = vec![
        ClassicTickerImport {
            eth_owner: alice_eth,
            ticker: ticker("ALPHA"),
            is_created: false,
            is_contract: false,
        },
        ClassicTickerImport {
            eth_owner: alice_eth,
            ticker: ticker("BETA"),
            is_created: true,
            is_contract: false,
        },
        ClassicTickerImport {
            eth_owner: bob_eth,
            ticker: ticker("GAMMA"),
            is_created: false,
            is_contract: true,
        },
        ClassicTickerImport {
            eth_owner: charlie_eth,
            ticker: ticker("DELTA"),
            is_created: true,
            is_contract: true,
        },
    ];
    let contract_did = IdentityId::from(1337);
    let registration_length = Some(42);
    let standard_config = default_reg_config();
    let genesis = AssetGenesis {
        classic_migration_tickers,
        ticker_registration_config: standard_config.clone(),
        classic_migration_contract_did: contract_did,
        classic_migration_tconfig: TickerRegistrationConfig {
            registration_length,
            ..standard_config
        },
        reserved_country_currency_codes: vec![],
    };

    // Define expected ticker data after genesis.
    let reg = |owner, expiry| TickerRegistration { expiry, owner };
    let cm_did = SystematicIssuers::ClassicMigration.as_id();
    let mut tickers = vec![
        (ticker("ALPHA"), reg(cm_did, registration_length)),
        (ticker("BETA"), reg(cm_did, registration_length)),
        (ticker("GAMMA"), reg(contract_did, registration_length)),
        (ticker("DELTA"), reg(contract_did, registration_length)),
    ];

    // Define expected classic ticker data after genesis.
    let classic_reg = |eth_owner, is_created| ClassicTickerRegistration {
        eth_owner,
        is_created,
    };
    let classic_tickers = vec![
        (ticker("ALPHA"), classic_reg(alice_eth, false)),
        (ticker("BETA"), classic_reg(alice_eth, true)),
        (ticker("GAMMA"), classic_reg(bob_eth, false)),
        (ticker("DELTA"), classic_reg(charlie_eth, true)),
    ];

    with_asset_genesis(genesis).build().execute_with(move || {
        // Dave enters the room.
        let rt_signer = Origin::signed(AccountKeyring::Dave.public());
        let rt_did = register_keyring_account(AccountKeyring::Dave).unwrap();

        // Ensure we have cm_did != contract_did != rt_did.
        assert_ne!(cm_did, contract_did);
        assert_ne!(rt_did, cm_did);
        assert_ne!(rt_did, contract_did);

        // Add another ticker to contrast expiry and DID and expect it.
        let expiry = standard_config.registration_length;
        assert_ok!(Asset::register_ticker(rt_signer, ticker("EPSILON")));
        tickers.push((ticker("EPSILON"), reg(rt_did, expiry)));

        // Ensure actual permutes expected.
        assert_eq!(sorted(Tickers::<TestStorage>::iter()), sorted(tickers));
        assert_eq!(sorted(ClassicTickers::iter()), sorted(classic_tickers));
    });
}

#[test]
fn classic_ticker_no_such_classic_ticker() {
    with_asset_genesis(AssetGenesis {
        classic_migration_tconfig: default_reg_config(),
        // There is a classic ticker, but not the one we're claiming.
        classic_migration_tickers: vec![ClassicTickerImport {
            ticker: ticker("ACME"),
            ..default_classic()
        }],
        ..<_>::default()
    })
    .build()
    .execute_with(|| {
        assert_noop!(
            Asset::claim_classic_ticker(root(), ticker("EMCA"), ethereum::EcdsaSignature([0; 65])),
            AssetError::NoSuchClassicTicker
        );
    });
}

#[test]
fn classic_ticker_registered_by_other() {
    let ticker = ticker("ACME");
    with_asset_genesis(AssetGenesis {
        classic_migration_tconfig: default_reg_config(),
        // There is a classic ticker, but its not owned by sys DID.
        classic_migration_tickers: vec![ClassicTickerImport {
            ticker,
            is_contract: true,
            ..default_classic()
        }],
        ..<_>::default()
    })
    .build()
    .execute_with(|| {
        assert_noop!(
            Asset::claim_classic_ticker(root(), ticker, ethereum::EcdsaSignature([0; 65])),
            AssetError::TickerAlreadyRegistered
        );
    });
}

#[test]
fn classic_ticker_expired_thus_available() {
    let ticker = ticker("ACME");
    with_asset_genesis(AssetGenesis {
        classic_migration_tconfig: TickerRegistrationConfig {
            registration_length: Some(0),
            ..default_reg_config()
        },
        classic_migration_tickers: vec![ClassicTickerImport {
            ticker,
            ..default_classic()
        }],
        ..<_>::default()
    })
    .build()
    .execute_with(|| {
        let rt_signer = Origin::signed(AccountKeyring::Dave.public());
        Timestamp::set_timestamp(1);
        assert_noop!(
            Asset::claim_classic_ticker(rt_signer, ticker, ethereum::EcdsaSignature([0; 65])),
            AssetError::TickerRegistrationExpired
        );
    });
}

#[test]
fn classic_ticker_garbage_signature() {
    let ticker = ticker("ACME");
    with_asset_genesis(AssetGenesis {
        classic_migration_tconfig: default_reg_config(),
        classic_migration_tickers: vec![ClassicTickerImport {
            ticker,
            ..default_classic()
        }],
        ..<_>::default()
    })
    .build()
    .execute_with(|| {
        let rt_signer = Origin::signed(AccountKeyring::Dave.public());
        assert_noop!(
            Asset::claim_classic_ticker(rt_signer, ticker, ethereum::EcdsaSignature([0; 65])),
            pallet_permissions::Error::<TestStorage>::UnauthorizedCaller
        );
    });
}

#[test]
fn classic_ticker_not_owner() {
    let ticker = ticker("ACME");
    with_asset_genesis(AssetGenesis {
        classic_migration_tconfig: default_reg_config(),
        classic_migration_tickers: vec![ClassicTickerImport {
            ticker,
            eth_owner: ethereum::address(&alice_secret_key()),
            ..default_classic()
        }],
        ..<_>::default()
    })
    .build()
    .execute_with(|| {
        let signer = Origin::signed(AccountKeyring::Bob.public());
        let did = register_keyring_account(AccountKeyring::Bob).unwrap();
        let eth_sig = ethereum::eth_msg(did, b"classic_claim", &bob_secret_key());
        assert_noop!(
            Asset::claim_classic_ticker(signer, ticker, eth_sig),
            AssetError::NotAnOwner
        );
    });
}

#[test]
fn classic_ticker_claim_works() {
    let eth_owner = ethereum::address(&alice_secret_key());

    // Define all the classic ticker imports.
    let import = |name, is_created| ClassicTickerImport {
        eth_owner,
        ticker: ticker(name),
        is_created,
        is_contract: false,
    };
    let tickers = vec![
        import("ALPHA", false),
        import("BETA", true),
        import("GAMMA", true),
        import("DELTA", true),
        import("EPSILON", true),
        import("ZETA", true),
    ];

    // Complete the genesis definition.
    let expire_after = 42;
    let genesis = AssetGenesis {
        classic_migration_tickers: tickers.clone(),
        ticker_registration_config: default_reg_config(),
        classic_migration_tconfig: TickerRegistrationConfig {
            registration_length: Some(expire_after),
            ..default_reg_config()
        },
        classic_migration_contract_did: 0.into(),
        reserved_country_currency_codes: vec![],
    };

    // Define the fees and initial balance.
    let init_bal = 150;
    let fee = 50;
    let fees = MockProtocolBaseFees(vec![(ProtocolOp::AssetCreateAsset, fee)]);

    let ext = with_asset_genesis(genesis).set_protocol_base_fees(fees);
    ext.build().execute_with(move || {
        System::set_block_number(1);

        let focus_user = |key: AccountKeyring, bal| {
            let acc = key.public();
            let did = crate::register_keyring_account_with_balance(key, bal).unwrap();
            TestStorage::set_payer_context(Some(acc));
            (acc, did)
        };

        // Initialize Alice and let them claim classic tickers successfully.
        let (alice_acc, alice_did) = focus_user(AccountKeyring::Alice, init_bal);
        let eth_sig = ethereum::eth_msg(alice_did, b"classic_claim", &alice_secret_key());
        for ClassicTickerImport { ticker, .. } in tickers {
            let signer = Origin::signed(alice_acc);
            assert_ok!(Asset::claim_classic_ticker(signer, ticker, eth_sig.clone()));
            assert_eq!(alice_did, Tickers::<TestStorage>::get(ticker).owner);
            assert!(matches!(
                &*System::events(),
                [.., frame_system::EventRecord {
                    event: super::storage::EventTest::asset(pallet_asset::RawEvent::ClassicTickerClaimed(..)),
                    ..
                }]
            ));
        }

        // Create `ALPHA` asset; this will cost.
        let create = |acc, name: &str, bal_after| {
            let asset = name.try_into().unwrap();
            let ticker = ticker(name);
            let signer = Origin::signed(acc);
            let ret = Asset::create_asset(signer, asset, ticker, 1, true, <_>::default(), vec![], None);
            assert_balance(acc, bal_after, 0);
            ret
        };
        assert_ok!(create(alice_acc, "ALPHA", init_bal - fee));

        // Create `BETA`; fee is waived as `is_created: true`.
        assert_ok!(create(alice_acc, "BETA", init_bal - fee));

        // Fast forward, thereby expiring `GAMMA` for which `is_created: true` holds.
        // Thus, fee isn't waived and is charged.
        Timestamp::set_timestamp(expire_after + 1);
        assert_ok!(create(alice_acc, "GAMMA", init_bal - fee - fee));

        // Now `DELTA` has expired as well. Bob registers it, so its not classic anymore and fee is charged.
        let (bob_acc, _) = focus_user(AccountKeyring::Bob, 0);
        assert!(ClassicTickers::get(&ticker("DELTA")).is_some());
        assert_ok!(Asset::register_ticker(Origin::signed(bob_acc), ticker("DELTA")));
        assert_eq!(ClassicTickers::get(&ticker("DELTA")), None);
        assert_noop!(create(bob_acc, "DELTA", 0), FeeError::InsufficientAccountBalance);

        // Repeat for `EPSILON`, but directly `create_asset` instead.
        let (charlie_acc, charlie_did) = focus_user(AccountKeyring::Charlie, 2 * fee);
        assert!(ClassicTickers::get(&ticker("EPSILON")).is_some());
        assert_ok!(create(charlie_acc, "EPSILON", 1 * fee));
        assert_eq!(ClassicTickers::get(&ticker("EPSILON")), None);

        // Travel back in time to unexpire `ZETA`,
        // transfer it to Charlie, and ensure its not classic anymore.
        Timestamp::set_timestamp(0);
        let zeta = ticker("ZETA");
        assert!(ClassicTickers::get(&zeta).is_some());
        let auth_id_alice = Identity::add_auth(
            alice_did,
            Signatory::from(charlie_did),
            AuthorizationData::TransferTicker(zeta),
            None,
        );
        assert_ok!(Asset::accept_ticker_transfer(Origin::signed(charlie_acc), auth_id_alice));
        assert_eq!(ClassicTickers::get(&zeta), None);
        assert_ok!(create(charlie_acc, "ZETA", 0 * fee));
        assert_eq!(ClassicTickers::get(&zeta), None);
    });
}
