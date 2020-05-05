use super::{
    storage::{register_keyring_account, TestStorage},
    ExtBuilder,
};

use pallet_balances as balances;
use pallet_identity as identity;
use pallet_treasury::{self as treasury, TreasuryTrait};
use polymesh_primitives::Beneficiary;

use frame_support::{assert_err, assert_ok};
use sp_runtime::DispatchError;
use test_client::AccountKeyring;

pub type Balances = balances::Module<TestStorage>;
pub type Treasury = treasury::Module<TestStorage>;
type Identity = identity::Module<TestStorage>;

type Origin = <TestStorage as frame_system::Trait>::Origin;

#[test]
fn reimbursement_and_disbursement() {
    ExtBuilder::default()
        .existential_deposit(10)
        .build()
        .execute_with(reimbursement_and_disbursement_we);
}

fn reimbursement_and_disbursement_we() {
    let root = Origin::system(frame_system::RawOrigin::Root);
    let alice = register_keyring_account(AccountKeyring::Alice).unwrap();
    let alice_acc = Origin::signed(AccountKeyring::Alice.public());
    let bob = register_keyring_account(AccountKeyring::Bob).unwrap();

    let total_issuance = Balances::total_issuance();

    // Verify reimburstement.
    assert_eq!(Treasury::balance(), 0);
    assert_ok!(Treasury::reimbursement(alice_acc.clone(), 1_000));
    assert_eq!(Treasury::balance(), 1_000);
    assert_eq!(total_issuance, Balances::total_issuance());

    // Disbursement: Only root can do that.
    let beneficiaries = vec![
        Beneficiary {
            id: alice,
            amount: 100,
        },
        Beneficiary {
            id: bob,
            amount: 500,
        },
    ];

    assert_ok!(Treasury::disbursement(root.clone(), beneficiaries));
    assert_eq!(Treasury::balance(), 400);
    assert_eq!(Balances::identity_balance(alice), 100);
    assert_eq!(Balances::identity_balance(bob), 500);
    assert_eq!(total_issuance, Balances::total_issuance());

    // Alice cannot make a disbursement to herself.
    assert_err!(
        Treasury::disbursement(
            alice_acc,
            vec![Beneficiary {
                id: alice,
                amount: 500
            }]
        ),
        DispatchError::BadOrigin
    );
    assert_eq!(total_issuance, Balances::total_issuance());
}
