// This file is part of the Polymesh distribution (https://github.com/PolymathNetwork/Polymesh).
// Copyright (c) 2020 Polymath

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3.

// This program is distributed in the hope that it will be useful, but
// WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU
// General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <http://www.gnu.org/licenses/>.

//! # Protocol Fee Module
//!
//! This module stores the fee of each protocol operation, and a common coefficient which is applied on
//! fee computation.
//!
//! It also provides helper functions to calculate and charge fees on each protocol operation.
//!
//! ## Interface
//!
//! ### Dispatchable Functions
//!
//! - [change_coefficient](Module::change_coefficient) - It changes the fee coefficient.
//! - [change_base_fee](Module::change_base_fee) - It changes the base fee.
//!
//! ### Public Functions
//!
//! - [compute_fee](Module::compute_fee) - It computes the fee of the operation.
//! - [charge_fee](Module::charge_fee) - It calculates the fee and charges it.
//! - [batch_charge_fee](Module::batch_charge_fee) - It calculates the fee and charges it on a batch operation.
//!
#![cfg_attr(not(feature = "std"), no_std)]

use frame_support::{
    decl_error, decl_event, decl_module, decl_storage,
    dispatch::{DispatchError, DispatchResult},
    traits::{Currency, ExistenceRequirement, OnUnbalanced, WithdrawReason},
    weights::{DispatchClass, Pays},
};
use frame_system::{self as system, ensure_root};
use pallet_identity as identity;
use polymesh_common_utilities::{
    identity::Trait as IdentityTrait,
    protocol_fee::{ChargeProtocolFee, ProtocolOp},
    transaction_payment::CddAndFeeDetails,
    Context, SystematicIssuers,
};
use polymesh_primitives::{IdentityId, PosRatio, Signatory};
use sp_runtime::{
    traits::{Saturating, Zero},
    Perbill,
};

type BalanceOf<T> =
    <<T as Trait>::Currency as Currency<<T as frame_system::Trait>::AccountId>>::Balance;
type NegativeImbalanceOf<T> =
    <<T as Trait>::Currency as Currency<<T as frame_system::Trait>::AccountId>>::NegativeImbalance;
/// Either an imbalance or an error.
type WithdrawFeeResult<T> = sp_std::result::Result<NegativeImbalanceOf<T>, DispatchError>;
type Identity<T> = identity::Module<T>;

pub trait Trait: frame_system::Trait + IdentityTrait {
    type Event: From<Event<Self>> + Into<<Self as frame_system::Trait>::Event>;
    /// The currency type in which fees will be paid.
    type Currency: Currency<Self::AccountId> + Send + Sync;
    /// Handler for the unbalanced reduction when taking protocol fees.
    type OnProtocolFeePayment: OnUnbalanced<NegativeImbalanceOf<Self>>;
}

decl_error! {
    pub enum Error for Module<T: Trait> {
        /// Insufficient identity balance to pay the fee.
        InsufficientIdentityBalance,
        /// Insufficient account balance to pay the fee.
        InsufficientAccountBalance,
        /// Account ID decoding failed.
        AccountIdDecode,
        /// Missing the current identity.
        MissingCurrentIdentity,
    }
}

decl_storage! {
    trait Store for Module<T: Trait> as ProtocolFee {
        /// The mapping of operation names to the base fees of those operations.
        pub BaseFees get(fn base_fees) config(): map hasher(twox_64_concat) ProtocolOp => BalanceOf<T>;
        /// The fee coefficient as a positive rational (numerator, denominator).
        pub Coefficient get(fn coefficient) config() build(|config: &GenesisConfig<T>| {
            if config.coefficient.1 == 0 {
                PosRatio(1, 1)
            } else {
                config.coefficient
            }
        }): PosRatio;
    }
}

decl_event! {
    pub enum Event<T> where
        AccountId = <T as frame_system::Trait>::AccountId,
        Balance = BalanceOf<T>,
    {
        /// The protocol fee of an operation.
        FeeSet(IdentityId, Balance),
        /// The fee coefficient.
        CoefficientSet(IdentityId, PosRatio),
        /// Fee charged.
        FeeCharged(AccountId, Balance),
    }
}

decl_module! {
    pub struct Module<T: Trait> for enum Call where origin: T::Origin {
        type Error = Error<T>;

        fn deposit_event() = default;

        /// Changes the fee coefficient for the root origin.
        ///
        /// # Errors
        /// * `BadOrigin` - Only root allowed.
        #[weight = (200_000_000, DispatchClass::Operational, Pays::Yes)]
        pub fn change_coefficient(origin, coefficient: PosRatio) -> DispatchResult {
            ensure_root(origin)?;
            let id = Context::current_identity::<Identity<T>>().unwrap_or_else(|| SystematicIssuers::Committee.as_id());

            <Coefficient>::put(&coefficient);
            Self::deposit_event(RawEvent::CoefficientSet(id, coefficient));
            Ok(())
        }

        /// Changes the a base fee for the root origin.
        ///
        /// # Errors
        /// * `BadOrigin` - Only root allowed.
        #[weight = (200_000_000, DispatchClass::Operational, Pays::Yes)]
        pub fn change_base_fee(origin, op: ProtocolOp, base_fee: BalanceOf<T>) ->
            DispatchResult
        {
            ensure_root(origin)?;
            let id = Context::current_identity::<Identity<T>>().unwrap_or_else(|| SystematicIssuers::Committee.as_id());

            <BaseFees<T>>::insert(op, &base_fee);
            Self::deposit_event(RawEvent::FeeSet(id, base_fee));
            Ok(())
        }

    }
}

impl<T: Trait> Module<T> {
    /// Computes the fee of the operation as `(base_fee * coefficient.0) / coefficient.1`.
    pub fn compute_fee(op: ProtocolOp) -> BalanceOf<T> {
        let coefficient = Self::coefficient();
        let ratio = Perbill::from_rational_approximation(coefficient.0, coefficient.1);
        ratio * Self::base_fees(op)
    }

    /// Computes the fee of the operation and charges it to the current payer. The fee is then
    /// credited to the intended recipients according to the implementation of
    /// `OnProtocolFeePayment`.
    pub fn charge_fee(op: ProtocolOp) -> DispatchResult {
        let fee = Self::compute_fee(op);
        if fee.is_zero() {
            return Ok(());
        }
        if let Some(payer) = T::CddHandler::get_payer_from_context() {
            let imbalance = Self::withdraw_fee(payer, fee)?;
            T::OnProtocolFeePayment::on_unbalanced(imbalance);
        }
        Ok(())
    }

    /// Computes the fee for `count` similar operations, and charges that fee to the current payer.
    pub fn batch_charge_fee(op: ProtocolOp, count: usize) -> DispatchResult {
        let fee = Self::compute_fee(op).saturating_mul(<BalanceOf<T>>::from(count as u32));
        if fee.is_zero() {
            return Ok(());
        }
        if let Some(payer) = T::CddHandler::get_payer_from_context() {
            let imbalance = Self::withdraw_fee(payer, fee)?;
            T::OnProtocolFeePayment::on_unbalanced(imbalance);
        }
        Ok(())
    }

    /// Withdraws a precomputed fee from the current payer if it is defined or from the current
    /// identity otherwise.
    fn withdraw_fee(account: T::AccountId, fee: BalanceOf<T>) -> WithdrawFeeResult<T> {
        let result = T::Currency::withdraw(
            &account,
            fee,
            WithdrawReason::Fee.into(),
            ExistenceRequirement::KeepAlive,
        )
        .map_err(|_| Error::<T>::InsufficientAccountBalance.into());
        Self::deposit_event(RawEvent::FeeCharged(account, fee));
        result
    }
}

impl<T: Trait> ChargeProtocolFee<T::AccountId> for Module<T> {
    fn charge_fee(op: ProtocolOp) -> DispatchResult {
        Self::charge_fee(op)
    }

    fn batch_charge_fee(op: ProtocolOp, count: usize) -> DispatchResult {
        Self::batch_charge_fee(op, count)
    }
}
