use crate::{
    asset::{self, IdentifierType, SecurityToken},
    general_tm, statistics,
    test::{
        storage::{register_keyring_account, TestStorage},
        ExtBuilder,
    },
};
use polymesh_primitives::Ticker;

use frame_support::assert_ok;
use test_client::AccountKeyring;

type Origin = <TestStorage as frame_system::Trait>::Origin;
type Asset = asset::Module<TestStorage>;
type Statistic = statistics::Module<TestStorage>;
type GeneralTM = general_tm::Module<TestStorage>;

#[test]
fn investor_count_per_asset() {
    ExtBuilder::default()
        .build()
        .execute_with(|| investor_count_per_asset_with_ext);
}

fn investor_count_per_asset_with_ext() {
    let alice_did = register_keyring_account(AccountKeyring::Alice).unwrap();
    let alice_signed = Origin::signed(AccountKeyring::Alice.public());
    let bob_did = register_keyring_account(AccountKeyring::Bob).unwrap();
    let bob_signed = Origin::signed(AccountKeyring::Bob.public());
    let charlie_did = register_keyring_account(AccountKeyring::Charlie).unwrap();

    // 1. Alice create an asset.
    let token = SecurityToken {
        name: vec![0x01],
        owner_did: alice_did,
        total_supply: 1_000_000,
        divisible: true,
        ..Default::default()
    };

    let identifiers = vec![(IdentifierType::default(), b"undefined".to_vec())];
    let ticker = Ticker::from_slice(token.name.as_slice());
    assert_ok!(Asset::create_token(
        alice_signed.clone(),
        token.name.clone(),
        ticker,
        1_000_000, // Total supply over the limit
        true,
        token.asset_type.clone(),
        identifiers.clone(),
        None,
    ));

    // NOTE: TM needs at least one asset rule.
    let asset_rule = general_tm::AssetRule {
        sender_rules: vec![],
        receiver_rules: vec![],
    };

    let ticker = Ticker::from_slice(token.name.as_slice());
    assert_ok!(GeneralTM::add_active_rule(
        alice_signed.clone(),
        ticker,
        asset_rule
    ));

    // Alice sends some tokens to Bob. Token has only one investor.
    assert_ok!(Asset::transfer(alice_signed.clone(), ticker, bob_did, 500));
    assert_eq!(Statistic::investor_count_per_asset(&ticker), 1);

    // Alice sends some tokens to Charlie. Token has now two investors.
    assert_ok!(Asset::transfer(alice_signed, ticker, charlie_did, 5000));
    assert_eq!(Statistic::investor_count_per_asset(&ticker), 2);

    // Bob sends all his tokens to Charlie, so now we have one investor again.
    assert_ok!(Asset::transfer(bob_signed, ticker, charlie_did, 500));
    assert_eq!(Statistic::investor_count_per_asset(&ticker), 1);
}
