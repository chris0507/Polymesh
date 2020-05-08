#![cfg_attr(not(feature = "std"), no_std)]
#![recursion_limit = "256"]

use pallet_identity as identity;
use polymesh_common_utilities::{
    constants::TREASURY_MODULE_ID,
    traits::{balances::Trait as BalancesTrait, identity::Trait as IdentityTrait, CommonTrait},
    Context,
};
use polymesh_primitives::{traits::IdentityCurrency, AccountKey, Beneficiary, IdentityId};

use codec::Encode;
use frame_support::{
    decl_error, decl_event, decl_module,
    dispatch::DispatchResult,
    ensure,
    traits::{Currency, ExistenceRequirement, Imbalance, OnUnbalanced, WithdrawReason},
};
use frame_system::{self as system, ensure_root, ensure_signed};
use sp_runtime::traits::{AccountIdConversion, Saturating};
use sp_std::{convert::TryFrom, prelude::*};

pub type ProposalIndex = u32;

type Identity<T> = identity::Module<T>;
type BalanceOf<T> =
    <<T as Trait>::Currency as Currency<<T as frame_system::Trait>::AccountId>>::Balance;
type NegativeImbalanceOf<T> =
    <<T as Trait>::Currency as Currency<<T as frame_system::Trait>::AccountId>>::NegativeImbalance;

pub trait Trait: frame_system::Trait + CommonTrait + BalancesTrait + IdentityTrait {
    // The overarching event type.
    type Event: From<Event<Self>> + Into<<Self as frame_system::Trait>::Event>;
    /// The native currency.
    type Currency: Currency<Self::AccountId> + IdentityCurrency<Self::AccountId>;
}

pub trait TreasuryTrait<Balance> {
    fn disbursement(target: IdentityId, amount: Balance);
    fn balance() -> Balance;
}

decl_event!(
    pub enum Event<T>
    where
        Balance = BalanceOf<T>,
    {
        /// Disbursement to a target Identity.
        /// (target identity, amount)
        TreasuryDisbursement(IdentityId, Balance),

        /// Treasury reimbursement.
        TreasuryReimbursement(Balance),
    }
);

decl_error! {
    /// Error for the treasury module.
    pub enum Error for Module<T: Trait> {
        /// Proposer's balance is too low.
        InsufficientBalance,
    }
}

decl_module! {
    pub struct Module<T: Trait> for enum Call where origin: T::Origin {
        type Error = Error<T>;

        fn deposit_event() = default;

        /// It transfers balances from treasury to each of beneficiaries and the specific amount
        /// for each of them.
        ///
        /// # Error
        /// * `BadOrigin`: Only root can execute transaction.
        /// * `InsufficientBalance`: If treasury balances is not enough to cover all beneficiaries.
        pub fn disbursement(origin, beneficiaries: Vec< Beneficiary<BalanceOf<T>>>) -> DispatchResult
        {
            ensure_root(origin)?;

            // Ensure treasury has enough balance.
            let total_amount = beneficiaries.iter().fold( 0.into(), |acc,b| b.amount.saturating_add(acc));
            ensure!(
                Self::balance() >= total_amount,
                Error::<T>::InsufficientBalance
            );

            beneficiaries.into_iter().for_each( |b| {
                Self::unsafe_disbursement(b.id, b.amount);
                Self::deposit_event(RawEvent::TreasuryDisbursement(b.id, b.amount));
            });
            Ok(())
        }

        /// It transfers the specific `amount` from `origin` account into treasury.
        ///
        /// Only accounts which are associated to an identity can make a donation to treasury.
        pub fn reimbursement(origin, amount: BalanceOf<T>) -> DispatchResult {
            let sender = ensure_signed(origin)?;
            let sender_key = AccountKey::try_from(sender.encode())?;
            let _did = Context::current_identity_or::<Identity<T>>(&sender_key)?;

            let _ = T::Currency::transfer(
                &sender,
                &Self::account_id(),
                amount,
                ExistenceRequirement::AllowDeath,
            )?;

            Self::deposit_event(RawEvent::TreasuryReimbursement(amount));
            Ok(())
        }
    }
}

impl<T: Trait> Module<T> {
    /// The account ID of the treasury pot.
    ///
    /// This actually does computation. If you need to keep using it, then make sure you cache the
    /// value and only call this once.
    pub fn account_id() -> T::AccountId {
        TREASURY_MODULE_ID.into_account()
    }

    pub fn unsafe_disbursement(target: IdentityId, amount: BalanceOf<T>) {
        let _ = T::Currency::withdraw(
            &Self::account_id(),
            amount,
            WithdrawReason::Transfer.into(),
            ExistenceRequirement::AllowDeath,
        );
        let _ = T::Currency::deposit_into_existing_identity(&target, amount);
        Self::deposit_event(RawEvent::TreasuryDisbursement(target, amount));
    }

    fn balance() -> BalanceOf<T> {
        T::Currency::free_balance(&Self::account_id())
    }
}

impl<T: Trait> TreasuryTrait<BalanceOf<T>> for Module<T> {
    #[inline]
    fn disbursement(target: IdentityId, amount: BalanceOf<T>) {
        Self::unsafe_disbursement(target, amount);
    }

    #[inline]
    fn balance() -> BalanceOf<T> {
        Self::balance()
    }
}

impl<T: Trait> OnUnbalanced<NegativeImbalanceOf<T>> for Module<T> {
    fn on_nonzero_unbalanced(amount: NegativeImbalanceOf<T>) {
        let numeric_amount = amount.peek();

        let _ = T::Currency::resolve_creating(&Self::account_id(), amount);

        Self::deposit_event(RawEvent::TreasuryReimbursement(numeric_amount));
    }
}
