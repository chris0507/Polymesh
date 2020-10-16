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

//! # Corporate Actions module.
//!
//! TODO
//!
//! ## Overview
//!
//! TODO
//!
//! ## Interface
//!
//! TODO
//!
//! ### Dispatchable Functions
//!
//! TODO
//!
//! ### Public Functions
//!
//! TODO
//!

#![cfg_attr(not(feature = "std"), no_std)]
#![feature(bool_to_option)]

use codec::{Decode, Encode};
use frame_support::{
    decl_error, decl_event, decl_module, decl_storage,
    dispatch::{DispatchError, DispatchResult},
    ensure,
    weights::Weight,
};
use pallet_asset as asset;
use pallet_identity as identity;
use polymesh_common_utilities::{
    balances::Trait as BalancesTrait,
    identity::{IdentityToCorporateAction, Trait as IdentityTrait},
};
use polymesh_primitives::{AuthorizationData, IdentityId, Ticker};
use sp_arithmetic::Permill;
#[cfg(feature = "std")]
use sp_runtime::{Deserialize, Serialize};
use sp_std::prelude::*;

/// How should `identities` in `TargetIdentities` be used?
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
#[derive(Clone, Copy, PartialEq, Eq, Encode, Decode, Debug)]
pub enum TargetTreatment {
    /// Only those identities should be included.
    Include,
    /// All identities *but* those should be included.
    Exclude,
}

impl Default for TargetTreatment {
    fn default() -> Self {
        // By default, an empty list of identities to exclude means all identities are included.
        Self::Exclude
    }
}

/// A description of which identities that a CA will apply to.
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
#[derive(Clone, PartialEq, Eq, Encode, Decode, Default, Debug)]
pub struct TargetIdentities {
    /// The specified identities either relevant or irrelevant, depending on `treatment`, for CAs.
    pub identities: Vec<IdentityId>,
    /// How should `identities` be treated?
    pub treatment: TargetTreatment,
}

/// Weight abstraction for the corporate actions module.
pub trait WeightInfo {
    fn reset_caa() -> Weight;
    fn set_default_targets(num_targets: u32) -> Weight;
    fn set_default_withholding_tax() -> Weight;
    fn set_did_withholding_tax() -> Weight;
}

/// The module's configuration trait.
pub trait Trait: frame_system::Trait + BalancesTrait + IdentityTrait + asset::Trait {
    /// The overarching event type.
    type Event: From<Event> + Into<<Self as frame_system::Trait>::Event>;

    /// Weight information for extrinsics in the corporate actions pallet.
    type WeightInfo: WeightInfo;
}

type Identity<T> = identity::Module<T>;
type Asset<T> = asset::Module<T>;

decl_storage! {
    trait Store for Module<T: Trait> as CorporateAction {
        /// A corporate action agent (CAA) of a ticker, if specified,
        /// that may be different from the asset owner (AO).
        /// If `None`, the AO is the CAA.
        ///
        /// The CAA may be distict from the AO because the CAA may require a money services license,
        /// and the assets would need to originate from the CAA's identity, not the AO's identity.
        ///
        /// (ticker => did?)
        pub Agent get(fn agent): map hasher(blake2_128_concat) Ticker => Option<IdentityId>;

        /// The identities targeted by default for CAs for this ticker,
        /// either to be excluded or included.
        ///
        /// (ticker => target identities)
        pub DefaultTargetIdentities get(fn default_target_identities): map hasher(blake2_128_concat) Ticker => TargetIdentities;

        /// The default amount of tax to withhold ("withholding tax", WT) for this ticker when distributing dividends.
        ///
        /// To understand withholding tax, e.g., let's assume that you hold ACME shares.
        /// ACME now decides to distribute 100 SEK to Alice.
        /// Alice lives in Sweden, so Skatteverket (the Swedish tax authority) wants 30% of that.
        /// Then those 100 * 30% are withheld from Alice, and ACME will send them to Skatteverket.
        ///
        /// (ticker => % to withhold)
        pub DefaultWitholdingTax get(fn default_withholding_tax): map hasher(blake2_128_concat) Ticker => Permill;

        /// The amount of tax to withhold ("withholding tax", WT) for a certain ticker x DID.
        /// If an entry exists for a certain DID, it overrides the default in `DefaultWithholdingTax`.
        ///
        /// (ticker => DID => % to withhold)
        pub DidWitholdingTax get(fn did_withholding_tax):
            double_map hasher(blake2_128_concat) Ticker, hasher(blake2_128_concat) IdentityId => Option<Permill>;
    }
}

// Public interface for this runtime module.
decl_module! {
    pub struct Module<T: Trait> for enum Call where origin: T::Origin {
        type Error = Error<T>;

        /// initialize the default event for this module
        fn deposit_event() = default;

        /// Reset the CAA of `ticker` to its owner.
        ///
        /// ## Arguments
        /// - `ticker` for which the CAA is reset.
        ///
        /// ## Errors
        /// - `Unauthorized` if `origin` isn't `ticker`'s owner.
        #[weight = <T as Trait>::WeightInfo::reset_caa()]
        pub fn reset_caa(origin, ticker: Ticker) {
            let did = <Asset<T>>::ensure_perms_owner(origin, &ticker)?;
            Self::change_ca_agent(did, ticker, None);
        }

        /// Set the default CA `TargetIdentities` to `targets`.
        ///
        /// ## Arguments
        /// - `ticker` for which the default identities are changing.
        /// - `targets` the default target identities for a CA.
        ///
        /// ## Errors
        /// - `UnauthorizedAsAgent` if `origin` is not `ticker`'s sole CAA (owner is not necessarily the CAA).
        #[weight = <T as Trait>::WeightInfo::set_default_targets(targets.identities.len() as u32)]
        pub fn set_default_targets(origin, ticker: Ticker, targets: TargetIdentities) {
            let caa = Self::ensure_ca_agent(origin, ticker)?;

            // Dedup any DIDs in `targets` to optimize iteration later.
            let new = {
                let mut ts = targets;
                ts.identities.sort_unstable();
                ts.identities.dedup();
                ts
            };

            // Commit + emit event.
            DefaultTargetIdentities::mutate(ticker, |slot| *slot = new.clone());
            Self::deposit_event(Event::DefaultTargetIdentitiesChanged(caa, ticker, new));
        }

        /// Set the default withholding tax for all DIDs and CAs relevant to this `ticker`.
        ///
        /// ## Arguments
        /// - `ticker` that the withholding tax will apply to.
        /// - `tax` that should be withheld when distributing dividends, etc.
        ///
        /// ## Errors
        /// - `UnauthorizedAsAgent` if `origin` is not `ticker`'s sole CAA (owner is not necessarily the CAA).
        #[weight = <T as Trait>::WeightInfo::set_default_withholding_tax()]
        pub fn set_default_withholding_tax(origin, ticker: Ticker, tax: Permill) {
            let caa = Self::ensure_ca_agent(origin, ticker)?;
            DefaultWitholdingTax::mutate(ticker, |slot| *slot = tax);
            Self::deposit_event(Event::DefaultWithholdingTaxChanged(caa, ticker, tax));
        }

        /// Set the withholding tax of `ticker` for `taxed_did` to `tax`.
        /// If `Some(tax)`, this overrides the default withholding tax of `ticker` to `tax` for `taxed_did`.
        /// Otherwise, if `None`, the default withholding tax will be used.
        ///
        /// ## Arguments
        /// - `ticker` that the withholding tax will apply to.
        /// - `taxed_did` that will have its withholding tax updated.
        /// - `tax` that should be withheld when distributing dividends, etc.
        ///
        /// ## Errors
        /// - `UnauthorizedAsAgent` if `origin` is not `ticker`'s sole CAA (owner is not necessarily the CAA).
        #[weight = <T as Trait>::WeightInfo::set_did_withholding_tax()]
        pub fn set_did_withholding_tax(origin, ticker: Ticker, taxed_did: IdentityId, tax: Option<Permill>) {
            let caa = Self::ensure_ca_agent(origin, ticker)?;
            DidWitholdingTax::mutate(ticker, taxed_did, |slot| *slot = tax);
            Self::deposit_event(Event::DidWithholdingTaxChanged(caa, ticker, taxed_did, tax));
        }
    }
}

decl_event! {
    pub enum Event {
        /// The set of default `TargetIdentities` for a ticker changed.
        /// (CAA DID, Ticker, New TargetIdentities)
        DefaultTargetIdentitiesChanged(IdentityId, Ticker, TargetIdentities),
        /// The default withholding tax for a ticker changed.
        /// (CAA DID, Ticker, New Tax).
        DefaultWithholdingTaxChanged(IdentityId, Ticker, Permill),
        /// The withholding tax specific to a DID for a ticker changed.
        /// (CAA DID, Ticker, Taxed DID, New Tax).
        DidWithholdingTaxChanged(IdentityId, Ticker, IdentityId, Option<Permill>),
        /// A new DID was made the CAA.
        /// (New CAA DID, Ticker, New CAA DID).
        CAATransferred(IdentityId, Ticker, IdentityId),
    }
}

decl_error! {
    pub enum Error for Module<T: Trait> {
        /// The signer is not authorized to act as a CAA for this asset.
        UnauthorizedAsAgent,
        /// The authorization type is not to transfer the CAA to another DID.
        AuthNotCAATransfer
    }
}

impl<T: Trait> IdentityToCorporateAction for Module<T> {
    fn accept_corporate_action_agent_transfer(did: IdentityId, auth_id: u64) -> DispatchResult {
        // Ensure we have authorization to transfer to `did`...
        let auth = <Identity<T>>::ensure_authorization(&did.into(), auth_id)?;
        let ticker = match auth.authorization_data {
            AuthorizationData::TransferCorporateActionAgent(ticker) => ticker,
            _ => return Err(Error::<T>::AuthNotCAATransfer.into()),
        };
        <Asset<T>>::consume_auth_by_owner(&ticker, did, auth_id)?;
        // ..and then transfer.
        Self::change_ca_agent(did, ticker, Some(did));
        Ok(())
    }
}

impl<T: Trait> Module<T> {
    /// Change `ticker`'s CAA to `new_caa` and emit an event.
    fn change_ca_agent(did: IdentityId, ticker: Ticker, new_caa: Option<IdentityId>) {
        // Transfer CAA status to `did`.
        Agent::mutate(ticker, |caa| *caa = new_caa);

        // Emit event.
        let new_caa = new_caa.unwrap_or_else(|| <Asset<T>>::token_details(ticker).owner_did);
        Self::deposit_event(Event::CAATransferred(did, ticker, new_caa));
    }

    /// Ensure that `origin` is authorized as a CA agent of the asset `ticker`.
    /// When `origin` is unsigned, `BadOrigin` occurs.
    /// Otherwise, should the DID not be the CAA of `ticker`, `UnauthorizedAsAgent` occurs.
    fn ensure_ca_agent(origin: T::Origin, ticker: Ticker) -> Result<IdentityId, DispatchError> {
        let did = <Identity<T>>::ensure_perms(origin)?;
        ensure!(
            Self::agent(ticker)
                .map_or_else(|| <Asset<T>>::is_owner(&ticker, did), |caa| caa == did),
            Error::<T>::UnauthorizedAsAgent
        );
        Ok(did)
    }
}
