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

//! # Contracts Wrapper Module
//!
//! The Contracts Wrapper module wraps Contracts, allowing for DID integration and permissioning
//!
//! ## To Do
//!
//!   - Remove the ability to call the Contracts module, bypassing Contracts Wrapper
//!   - Integrate DID into all calls, and validate signing_key
//!   - Track ownership of code and instances via DIDs
//!
//! ## Possible Tokenomics
//!
//!   - Initially restrict list of accounts that can put_code
//!   - When code is instantiated enforce a POLYX fee to the DID owning the code (i.e. that executed put_code)

use pallet_identity as identity;
use polymesh_common_utilities::{identity::Trait as IdentityTrait, Context};
use polymesh_primitives::{AccountKey, IdentityId, Signatory};

use codec::Encode;
use frame_support::traits::Currency;
use frame_support::{decl_error, decl_module, decl_storage, dispatch::DispatchResult, ensure};
use frame_system::ensure_signed;
use pallet_contracts::{CodeHash, Gas, Schedule};
use sp_runtime::traits::StaticLookup;
use sp_std::{convert::TryFrom, prelude::*};

// pub type CodeHash<T> = <T as frame_system::Trait>::Hash;

pub type BalanceOf<T> = <<T as pallet_contracts::Trait>::Currency as Currency<
    <T as frame_system::Trait>::AccountId,
>>::Balance;

pub trait Trait: pallet_contracts::Trait + IdentityTrait {}

decl_storage! {
    trait Store for Module<T: Trait> as ContractsWrapper {
        pub CodeHashDid: map hasher(twox_64_concat) CodeHash<T> => Option<IdentityId>;
    }
}

decl_error! {
    pub enum Error for Module<T: Trait> {
        /// The sender must be a signing key for the DID.
        SenderMustBeSigningKeyForDid,
    }
}

type Identity<T> = identity::Module<T>;

decl_module! {
    // Wrap dispatchable functions for contracts so that we can add additional gating logic
    // TODO: Figure out how to remove dispatchable calls from the underlying contracts module
    pub struct Module<T: Trait> for enum Call where origin: T::Origin {
        type Error = Error<T>;

        // Simply forwards to the `update_schedule` function in the Contract module.
        pub fn update_schedule(origin, schedule: Schedule) -> DispatchResult {
            <pallet_contracts::Module<T>>::update_schedule(origin, schedule)
        }

        // Simply forwards to the `put_code` function in the Contract module.
        pub fn put_code(
            origin,
            #[compact] gas_limit: Gas,
            code: Vec<u8>
        ) -> DispatchResult {
            let sender = ensure_signed(origin)?;
            let sender_key = AccountKey::try_from(sender.encode())?;
            let did = Context::current_identity_or::<Identity<T>>(&sender_key)?;
            let signer = Signatory::AccountKey(sender_key);

            // Check that sender is allowed to act on behalf of `did`
            ensure!(
                <identity::Module<T>>::is_signer_authorized(did, &signer),
                Error::<T>::SenderMustBeSigningKeyForDid
            );

            // Call underlying function
            let new_origin = frame_system::RawOrigin::Signed(sender).into();
            <pallet_contracts::Module<T>>::put_code(new_origin, gas_limit, code)
        }

        // Simply forwards to the `call` function in the Contract module.
        pub fn call(
            origin,
            dest: <T::Lookup as StaticLookup>::Source,
            #[compact] value: BalanceOf<T>,
            #[compact] gas_limit: Gas,
            data: Vec<u8>
        ) -> DispatchResult {
            <pallet_contracts::Module<T>>::call(origin, dest, value, gas_limit, data)
        }

        // Simply forwards to the `instantiate` function in the Contract module.
        pub fn instantiate(
            origin,
            #[compact] endowment: BalanceOf<T>,
            #[compact] gas_limit: Gas,
            code_hash: CodeHash<T>,
            data: Vec<u8>
        ) -> DispatchResult {
            <pallet_contracts::Module<T>>::instantiate(origin, endowment, gas_limit, code_hash, data)
        }
    }
}

impl<T: Trait> Module<T> {}
