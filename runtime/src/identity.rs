use rstd::{convert::TryFrom, prelude::*};

pub static DID_PREFIX: &'static str = "did:poly:";
use crate::{
    balances,
    entity::{IdentityRole, Key, RoledKey},
};

use codec::Encode;
use sr_primitives::traits::{CheckedAdd, CheckedSub};
use srml_support::{
    decl_event, decl_module, decl_storage,
    dispatch::Result,
    ensure,
    traits::{Currency, ExistenceRequirement, WithdrawReason},
};
use system::{self, ensure_signed};

#[derive(codec::Encode, codec::Decode, Default, Clone, PartialEq, Debug)]
pub struct DidRecord<U> {
    pub master_key: Key,
    pub signing_keys: Vec<RoledKey>,
    pub balance: U,
}

impl<U> DidRecord<U> {
    pub fn has_role(&self, role: IdentityRole) -> bool {
        self.signing_keys
            .iter()
            .find(|&rk| rk.has_role(role))
            .is_some()
    }
}

#[derive(codec::Encode, codec::Decode, Default, Clone, PartialEq, Eq, Debug)]
pub struct Claim<U> {
    topic: u32,
    schema: u32,
    bytes: Vec<u8>,
    expiry: U,
}

#[derive(codec::Encode, codec::Decode, Default, Clone, PartialEq, Eq, Debug)]
pub struct ClaimRecord<U> {
    claim: Claim<U>,
    revoked: bool,
    /// issuer DID
    issued_by: Vec<u8>,
    attestation: Vec<u8>,
}

/// The module's configuration trait.
pub trait Trait: system::Trait + balances::Trait + timestamp::Trait {
    /// The overarching event type.
    type Event: From<Event<Self>> + Into<<Self as system::Trait>::Event>;
}

decl_storage! {
    trait Store for Module<T: Trait> as identity {

        Owner get(owner) config(): T::AccountId;

        /// DID -> identity info
        pub DidRecords get(did_records): map Vec<u8> => DidRecord<T::Balance>;

        /// DID -> Associated claims
        pub Claims get(claims): map Vec<u8> => Vec<ClaimRecord<T::Moment>>;

        // Signing key => DID
        pub SigningKeyDid get(signing_key_did): map Key => Vec<u8>;

        // Signing key => Charge Fee to did?. Default is false i.e. the fee will be charged from user balance
        pub ChargeDid get(charge_did): map Key => bool;

        /// How much does creating a DID cost
        pub DidCreationFee get(did_creation_fee) config(): T::Balance;
    }
}

decl_module! {
    /// The module declaration.
    pub struct Module<T: Trait> for enum Call where origin: T::Origin {
        // Initializing events
        // this is needed only if you are using events in your module
        fn deposit_event() = default;

        fn create_issuer(origin, issuer_did: Vec<u8>) -> Result {
            let sender = ensure_signed(origin)?;
            ensure!(Self::owner() == sender,"Sender must be the identity module owner");

            let sender_key = Key::try_from( sender.encode())?;
            ensure!( <SigningKeyDid>::exists( &sender_key), "Sender key does not associated DID");

            Self::do_create_issuer( &issuer_did, &sender_key)
        }

        fn create_simple_token_issuer(origin, issuer_did: Vec<u8>) -> Result {
            let sender = ensure_signed(origin)?;
            ensure!(Self::owner() == sender,"Sender must be the identity module owner");

            let sender_key = Key::try_from( sender.encode())?;
            ensure!( <SigningKeyDid>::exists( &sender_key), "Sender key does not associated DID");
            Self::do_create_simple_token_issuer(&issuer_did, &sender_key)
        }

        fn create_investor(origin, investor_did: Vec<u8>) -> Result {
            let sender = ensure_signed(origin)?;
            ensure!(Self::owner() == sender,"Sender must be the identity module owner");

            let sender_key = Key::try_from( sender.encode())?;
            ensure!( <SigningKeyDid>::exists( &sender_key), "Sender key does not associated DID");
            Self::do_create_investor(&investor_did, &sender_key)
        }

        fn set_charge_did(origin, charge_did: bool) -> Result {
            let sender = ensure_signed(origin)?;
            let sender_key = Key::try_from( sender.encode())?;
            <ChargeDid>::insert(sender_key, charge_did);
            Ok(())
        }

        /// Register signing keys for a new DID. Uses origin key as the master key
        pub fn register_did(origin, did: Vec<u8>, signing_keys: Vec<RoledKey>) -> Result {
            let sender = ensure_signed(origin)?;
            let master_key = Key::try_from( sender.encode())?;

            // Make sure caller specified a correct DID
            validate_did(did.as_slice())?;

            // Make sure there's no pre-existing entry for the DID
            ensure!(!<DidRecords<T>>::exists(&did), "DID must be unique");

            // TODO: Subtract the fee
            let _imbalance = <balances::Module<T> as Currency<_>>::withdraw(
                &sender,
                Self::did_creation_fee(),
                WithdrawReason::Fee,
                ExistenceRequirement::KeepAlive
                )?;

            for roled_key in &signing_keys {
                let key = &roled_key.key;
                if <SigningKeyDid>::exists(key) {
                    ensure!(<SigningKeyDid>::get(key) == did, "One signing key can only belong to one DID");
                }
            }

            for roled_key in &signing_keys {
                <SigningKeyDid>::insert( &roled_key.key, did.clone());
            }

            let record = DidRecord {
                signing_keys: signing_keys.clone(),
                master_key: master_key,
                ..Default::default()
            };

            <DidRecords<T>>::insert(&did, record);

            Self::deposit_event(RawEvent::NewDid(did, sender, signing_keys));

            Ok(())
        }

        /// Adds new signing keys for a DID. Only called by master key owner.
        fn add_signing_keys(origin, did: Vec<u8>, additional_keys: Vec<Key>) -> Result {
            let sender = ensure_signed(origin)?;

            ensure!(<DidRecords<T>>::exists(&did), "DID must already exist");

            // Verify that sender key is current master key
            let sender_key = sender.encode();
            let record = <DidRecords<T>>::get(&did);
            ensure!(record.master_key == sender_key, "Sender must hold the master key");

            for key in &additional_keys {
                if <SigningKeyDid>::exists(key) {
                    ensure!(<SigningKeyDid>::get(key) == did, "One signing key can only belong to one DID");
                }
            }

            for key in &additional_keys {
                <SigningKeyDid>::insert(key, did.clone());
            }

            <DidRecords<T>>::mutate(&did,
            |record| {
                // Concatenate new keys while making sure the key set is
                // unique
                let mut new_roled_keys = additional_keys.iter()
                    .filter( |&add_key| {
                        record.signing_keys.iter()
                        .find( |&rk| rk == add_key)
                        .is_none()
                    })
                    .map( |add_key| RoledKey::from(add_key.clone()))
                    .collect::<Vec<_>>();

                (*record).signing_keys.append( &mut new_roled_keys);
            });

            Self::deposit_event(RawEvent::SigningKeysAdded(did, additional_keys));

            Ok(())
        }

        /// Removes specified signing keys of a DID if present. Only called by master key owner.
        fn remove_signing_keys(origin, did: Vec<u8>, keys_to_remove: Vec<Key>) -> Result {
            let sender = ensure_signed(origin)?;

            // Verify that sender key is current master key
            let sender_key = sender.encode();
            let record = <DidRecords<T>>::get(&did);
            ensure!(record.master_key == sender_key, "Sender must hold the master key");

            ensure!(<DidRecords<T>>::exists(&did), "DID must already exist");

            for key in &keys_to_remove {
                if <SigningKeyDid>::exists(key) {
                    ensure!(<SigningKeyDid>::get(key) == did, "Signing key does not belong to this DID");
                }
            }

            for key in &keys_to_remove {
                <SigningKeyDid>::remove(key);
            }

            <DidRecords<T>>::mutate(&did,
            |record| {
                // Filter out keys meant for deletion
                let keys = record.signing_keys
                    .iter()
                    .filter(|&roled_key| keys_to_remove.iter()
                        .find(|&rk| roled_key == rk)
                        .is_none())
                    .cloned()
                    .collect::<Vec<_>>();

                (*record).signing_keys = keys;
            });

            Self::deposit_event(RawEvent::SigningKeysRemoved(did, keys_to_remove));

            Ok(())
        }

        pub fn set_roles(origin, did: Vec<u8>, roles: Vec<IdentityRole>) -> Result {
            let sender = ensure_signed(origin)?;
            let sender_key = Key::try_from( sender.encode())?;

            Self::do_set_roles( &did, &sender_key, roles)
        }

        /// Sets a new master key for a DID. Only called by master key owner.
        fn set_master_key(origin, did: Vec<u8>, new_key: Key) -> Result {
            let sender = ensure_signed(origin)?;

            // Verify that sender key is current master key
            let sender_key = sender.encode();
            let record = <DidRecords<T>>::get(&did);
            ensure!(record.master_key == sender_key, "Sender must hold the master key");

            ensure!(<DidRecords<T>>::exists(&did), "DID must already exist");

            <DidRecords<T>>::mutate(&did,
            |record| {
                (*record).master_key = new_key.clone();
            });

            Self::deposit_event(RawEvent::NewMasterKey(did, sender, new_key));

            Ok(())
        }

        /// Adds funds to a DID.
        pub fn fund_poly(origin, did: Vec<u8>, amount: <T as balances::Trait>::Balance) -> Result {
            let sender = ensure_signed(origin)?;

            ensure!(<DidRecords<T>>::exists(&did), "DID must already exist");

            let record = <DidRecords<T>>::get(&did);

            // We must know that new balance is valid without creating side effects
            let new_record_balance = record.balance.checked_add(&amount).ok_or("overflow occured when increasing DID balance")?;

            let _imbalance = <balances::Module<T> as Currency<_>>::withdraw(
                &sender,
                amount,
                WithdrawReason::Fee,
                ExistenceRequirement::KeepAlive
                )?;

            <DidRecords<T>>::mutate(&did, |record| {
                (*record).balance = new_record_balance;
            });

            Self::deposit_event(RawEvent::PolyDepositedInDid(did, sender, amount));

            Ok(())
        }

        /// Withdraws funds from a DID. Only called by master key owner.
        fn withdrawy_poly(origin, did: Vec<u8>, amount: <T as balances::Trait>::Balance) -> Result {
            let sender = ensure_signed(origin)?;

            // Verify that sender key is current master key
            let sender_key = sender.encode();
            let record = <DidRecords<T>>::get(&did);
            ensure!(record.master_key == sender_key, "Sender must hold the master key");

            ensure!(<DidRecords<T>>::exists(&did), "DID must already exist");

            let record = <DidRecords<T>>::get(&did);

            // We must know that new balance is valid without creating side effects
            let new_record_balance = record.balance.checked_sub(&amount).ok_or("underflow occured when decreasing DID balance")?;

            let _imbalance = <balances::Module<T> as Currency<_>>::deposit_into_existing(&sender, amount)?;

            <DidRecords<T>>::mutate(&did, |record| {
                (*record).balance = new_record_balance;
            });

            Self::deposit_event(RawEvent::PolyWithdrawnFromDid(did, sender, amount));

            Ok(())
        }

        /// Transfers funds between DIDs.
        fn transfer_poly(origin, did: Vec<u8>, to_did: Vec<u8>, amount: <T as balances::Trait>::Balance) -> Result {
            let sender = ensure_signed(origin)?;

            // Check that sender is allowed to act on behalf of `did`
            ensure!(Self::is_signing_key(&did, &Key::try_from(sender.encode())?), "sender must be a signing key for DID");

            let from_record = <DidRecords<T>>::get(did.clone());
            let to_record = <DidRecords<T>>::get(to_did.clone());

            // Same for `from`
            let new_from_balance = from_record.balance.checked_sub(&amount).ok_or("Sender must have sufficient funds")?;

            // Compute new `to_did` balance and check that beneficiary's balance can be increased
            let new_to_balance = to_record.balance.checked_add(&amount).ok_or("Failed to increase to_did balance")?;

            // Alter from record
            <DidRecords<T>>::mutate(did, |record| {
                record.balance = new_from_balance;
            });

            // Alter to record
            <DidRecords<T>>::mutate(to_did, |record| {
                record.balance = new_to_balance;
            });

            Ok(())
        }

        /// Adds new claim records. Only called by did_issuer's signing key
        fn add_claim(origin, did: Vec<u8>, did_issuer: Vec<u8>, claims: Vec<Claim<T::Moment>>) -> Result {
            let sender = ensure_signed(origin)?;

            ensure!(<DidRecords<T>>::exists(&did), "DID must already exist");
            ensure!(<DidRecords<T>>::exists(&did_issuer), "claim issuer DID must already exist");

            let sender_key = Key::try_from( sender.encode())?;
            ensure!(Self::is_claim_issuer(&did, &did_issuer) || Self::is_master_key(&did, &sender_key), "did_issuer must be a claim issuer or master key for DID");

            // Verify that sender key is one of did_issuer's signing keys
            ensure!(Self::is_signing_key(&did_issuer, &sender_key), "Sender must hold a claim issuer's signing key");

            <Claims<T>>::mutate(&did, |claim_records| {
                let mut new_records = claims
                    .iter()
                    .cloned()
                    .map(|claim| ClaimRecord {
                        claim,
                        revoked: false,
                        issued_by: did_issuer.clone(),
                        attestation: Vec::new(),
                    })
                    .collect();

                claim_records.append(&mut new_records);
            });

            Self::deposit_event(RawEvent::NewClaims(did, did_issuer, claims));

            Ok(())
        }

        /// Adds new claim records with an attestation. Only called by issuer signing keys
        fn add_claim_with_attestation(origin, did: Vec<u8>, did_issuer: Vec<u8>, claims: Vec<Claim<T::Moment>>, attestation: Vec<u8>) -> Result {
            let sender = ensure_signed(origin)?;

            ensure!(<DidRecords<T>>::exists(&did), "DID must already exist");
            ensure!(<DidRecords<T>>::exists(&did_issuer), "claim issuer DID must already exist");

            let sender_key = Key::try_from( sender.encode())?;
            ensure!(Self::is_claim_issuer(&did, &did_issuer) || Self::is_master_key(&did, &sender_key), "did_issuer must be a claim issuer or master key for DID");

            // Verify that sender key is one of did_issuer's signing keys
            ensure!(Self::is_signing_key(&did_issuer, &sender_key), "Sender must hold a claim issuer's signing key");

            <Claims<T>>::mutate(&did, |claim_records| {
                let mut new_records = claims
                    .iter()
                    .cloned()
                    .map(|claim| ClaimRecord {
                        claim,
                        revoked: false,
                        issued_by: did_issuer.clone(),
                        attestation: attestation.clone(),
                    })
                    .collect();

                claim_records.append(&mut new_records);
            });

            Self::deposit_event(RawEvent::NewClaimsWithAttestation(did, did_issuer, claims, attestation));

            Ok(())
        }

        /// Marks the specified claim as revoked
        fn revoke_claim(origin, did: Vec<u8>, did_issuer: Vec<u8>, claim: Claim<T::Moment>) -> Result {
            let sender = ensure_signed(origin)?;

            ensure!(<DidRecords<T>>::exists(&did), "DID must already exist");
            ensure!(<DidRecords<T>>::exists(&did_issuer), "claim issuer DID must already exist");
            ensure!(Self::is_claim_issuer(&did, &did_issuer), "did_issuer must be a claim issuer for DID");

            // Verify that sender key is one of did_issuer's signing keys
            let sender_key = Key::try_from( sender.encode())?;
            ensure!(Self::is_signing_key(&did_issuer, &sender_key), "Sender must hold a claim issuer's signing key");

            <Claims<T>>::mutate(&did, |claim_records| {
                claim_records
                    .iter_mut()
                    .for_each(|record| if record.issued_by == did_issuer && record.claim == claim {
                        (*record).revoked = true;
                })
            });

            Self::deposit_event(RawEvent::RevokedClaim(did, did_issuer, claim));

            Ok(())
        }

        /// Marks all claims of an issuer as revoked
        fn revoke_all(origin, did: Vec<u8>, did_issuer: Vec<u8>) -> Result {
            let sender = ensure_signed(origin)?;

            ensure!(<DidRecords<T>>::exists(did.clone()), "DID must already exist");
            ensure!(<DidRecords<T>>::exists(did_issuer.clone()), "claim issuer DID must already exist");
            ensure!(Self::is_claim_issuer(&did, &did_issuer), "did_issuer must be a claim issuer or master key for DID");

            // Verify that sender key is one of did_issuer's signing keys
            let sender_key = Key::try_from( sender.encode())?;
            ensure!(Self::is_signing_key(&did_issuer, &sender_key), "Sender must hold a claim issuer's signing key");

            <Claims<T>>::mutate(did.clone(), |claim_records| {

                claim_records
                    .iter_mut()
                    .for_each(|record| if record.issued_by == did_issuer {
                        (*record).revoked = true;
                })
            });

            Self::deposit_event(RawEvent::RevokedAllClaims(did, did_issuer));

            Ok(())
        }
    }
}

decl_event!(
    pub enum Event<T>
    where
        AccountId = <T as system::Trait>::AccountId,
        Balance = <T as balances::Trait>::Balance,
        Moment = <T as timestamp::Trait>::Moment,
    {
        /// DID, master key account ID, signing keys
        NewDid(Vec<u8>, AccountId, Vec<RoledKey>),

        /// DID, new keys
        SigningKeysAdded(Vec<u8>, Vec<Key>),

        /// DID, the keys that got removed
        SigningKeysRemoved(Vec<u8>, Vec<Key>),

        /// DID, old master key account ID, new key
        NewMasterKey(Vec<u8>, AccountId, Key),

        /// beneficiary DID, sender, amount
        PolyDepositedInDid(Vec<u8>, AccountId, Balance),

        /// DID, beneficiary, amount
        PolyWithdrawnFromDid(Vec<u8>, AccountId, Balance),

        /// DID, amount
        PolyChargedFromDid(Vec<u8>, Balance),

        /// DID from, DID to, amount
        PolyTransfer(Vec<u8>, Vec<u8>, Balance),

        /// DID, claim issuer DID
        NewClaimIssuer(Vec<u8>, Vec<u8>),

        /// DID, removed claim issuer DID
        RemovedClaimIssuer(Vec<u8>, Vec<u8>),

        /// DID, claim issuer DID, claims
        NewClaims(Vec<u8>, Vec<u8>, Vec<Claim<Moment>>),

        /// DID, claim issuer DID, claims, attestation
        NewClaimsWithAttestation(Vec<u8>, Vec<u8>, Vec<Claim<Moment>>, Vec<u8>),

        /// DID, claim issuer DID, claim
        RevokedClaim(Vec<u8>, Vec<u8>, Claim<Moment>),

        /// DID, claim issuer DID
        RevokedAllClaims(Vec<u8>, Vec<u8>),

        /// DID
        NewIssuer(Vec<u8>),
    }
);

impl<T: Trait> Module<T> {
    /// Private and not sanitized function. It is designed to be used internally by
    /// others sanitezed functions.
    fn update_roles(target_did: &Vec<u8>, key: &Key, roles: Vec<IdentityRole>) -> Result {
        <DidRecords<T>>::mutate(target_did, |record| {
            // First filter avoids duplication of key.
            let mut signing_keys = record
                .signing_keys
                .iter()
                .filter(|&rk| rk != key)
                .cloned()
                .collect::<Vec<_>>();

            signing_keys.push(RoledKey::new(key.clone(), roles));
            (*record).signing_keys = signing_keys;
        });
        Ok(())
    }

    pub fn do_set_roles(target_did: &Vec<u8>, key: &Key, roles: Vec<IdentityRole>) -> Result {
        // Target did existence
        ensure!(
            <DidRecords<T>>::exists(target_did),
            "DID must already exist"
        );
        let record = <DidRecords<T>>::get(target_did);

        // Target did has sender's master key in its signing keys.
        ensure!(
            record.signing_keys.iter().find(|&rk| rk == key).is_some(),
            "Sender is not part of did's signing keys"
        );

        Self::update_roles(target_did, key, roles)
    }

    pub fn add_role_to_key(did: &Vec<u8>, key: &Key, role: IdentityRole) -> Result {
        ensure!(<DidRecords<T>>::exists(did), "Investor DID does not exist");
        let record = <DidRecords<T>>::get(did);

        // Target did has sender's master key in its signing keys.
        ensure!(
            record.signing_keys.iter().find(|&rk| rk == key).is_some(),
            "Sender is not part of did's signing keys"
        );

        // Get current roles of `key` at `investor_did`.
        let mut new_roles = match record.signing_keys.iter().find(|&rk| rk == key) {
            Some(ref rk) => rk.roles.clone(),
            None => Vec::with_capacity(1),
        };

        // Add new role, sort result and remove duplicates.
        new_roles.push(role);
        new_roles.sort();
        new_roles.dedup();

        Self::update_roles(&did, key, new_roles)
    }

    /// Add a new investor. Warning: No identity module ownership checks are performed
    pub fn do_create_investor(investor_did: &Vec<u8>, key: &Key) -> Result {
        Self::add_role_to_key(investor_did, key, IdentityRole::Investor)
    }

    /// Add a new issuer. Warning: No identity module ownership checks are performed
    pub fn do_create_issuer(issuer_did: &Vec<u8>, key: &Key) -> Result {
        Self::add_role_to_key(issuer_did, key, IdentityRole::Issuer)?;
        Self::deposit_event(RawEvent::NewIssuer(issuer_did.clone()));
        Ok(())
    }

    /// Add a new SimpleToken issuer. Warning: No identity module ownership checks are performed
    pub fn do_create_simple_token_issuer(issuer_did: &Vec<u8>, key: &Key) -> Result {
        Self::add_role_to_key(issuer_did, key, IdentityRole::SimpleTokenIssuer)
    }

    pub fn is_issuer(did: &Vec<u8>) -> bool {
        <DidRecords<T>>::get(did).has_role(IdentityRole::Issuer)
    }

    pub fn is_issuer_of(issuer_did: &Vec<u8>, master_did: &Vec<u8>) -> bool {
        Self::has_role_for_did(issuer_did, master_did, IdentityRole::Issuer)
    }

    pub fn is_simple_token_issuer_of(st_issuer_did: &Vec<u8>, master_did: &Vec<u8>) -> bool {
        Self::has_role_for_did(st_issuer_did, master_did, IdentityRole::SimpleTokenIssuer)
    }

    pub fn is_simple_token_issuer(did: &Vec<u8>) -> bool {
        <DidRecords<T>>::get(did).has_role(IdentityRole::SimpleTokenIssuer)
    }

    pub fn is_investor(investor_did: &Vec<u8>, master_did: &Vec<u8>) -> bool {
        Self::has_role_for_did(investor_did, master_did, IdentityRole::Investor)
    }

    pub fn is_claim_issuer(issuer_did: &Vec<u8>, master_did: &Vec<u8>) -> bool {
        Self::has_role_for_did(issuer_did, master_did, IdentityRole::ClaimIssuer)
    }

    /// It checks if `did` has role `role` for master key of `key_did`.
    fn has_role_for_did(did: &Vec<u8>, key_did: &Vec<u8>, role: IdentityRole) -> bool {
        let key = <DidRecords<T>>::get(key_did).master_key;
        Self::has_role_for_key(did, &key, role)
    }

    /// It checks if `did` has role `role` for signing key `key`.
    fn has_role_for_key(did: &Vec<u8>, key: &Key, role: IdentityRole) -> bool {
        let did_record = <DidRecords<T>>::get(did);
        did_record
            .signing_keys
            .iter()
            .find(|&rk| rk == key)
            .map_or(false, |rk| rk.has_role(role))
    }

    pub fn is_signing_key(did: &Vec<u8>, key: &Key) -> bool {
        let record = <DidRecords<T>>::get(did);
        record.signing_keys.iter().find(|&rk| rk == key).is_some() || record.master_key == *key
    }

    /// Use `did` as reference.
    pub fn is_master_key(did: &Vec<u8>, key: &Key) -> bool {
        key == &<DidRecords<T>>::get(did).master_key
    }

    /// Withdraws funds from a DID balance
    pub fn charge_poly(did: Vec<u8>, amount: T::Balance) -> bool {
        if !<DidRecords<T>>::exists(did.clone()) {
            return false;
        }

        let record = <DidRecords<T>>::get(did.clone());

        if record.balance < amount {
            return false;
        }

        <DidRecords<T>>::mutate(did.clone(), |record| {
            (*record).balance = record.balance - amount;
        });

        Self::deposit_event(RawEvent::PolyChargedFromDid(did, amount));

        return true;
    }

    /// By default, all functions require `IdentityRole::Full` role.
    pub fn check_default_grants(did: &Vec<u8>) -> Result {
        Self::check_grants_with_roles(did, &[IdentityRole::Full])
    }

    /// It double-checks that `did` has all roles of `roles` list.
    pub fn check_grants_with_roles(_did: &Vec<u8>, _roles: &[IdentityRole]) -> Result {
        unimplemented!();
        /*let record = Self::did_records( did);
        for role in roles.into_iter() {
            if !record.master_key.has_role(*role) {
                return Err( "This function requires {} role");
            }
        }
        Ok(())
        */
    }
}

/// Make sure the supplied slice is a valid Polymesh DID
pub fn validate_did(did: &[u8]) -> Result {
    // TODO: Also check length after prefix,
    if did.starts_with(DID_PREFIX.as_bytes()) {
        Ok(())
    } else {
        Err("DID has no valid prefix")
    }
}

pub trait IdentityTrait<T> {
    fn signing_key_charge_did(signing_key: &Key) -> bool;
    fn charge_poly(signing_key: &Key, amount: T) -> bool;
}

impl<T: Trait> IdentityTrait<T::Balance> for Module<T> {
    fn charge_poly(signing_key: &Key, amount: T::Balance) -> bool {
        Self::charge_poly(<SigningKeyDid>::get(signing_key), amount)
    }

    fn signing_key_charge_did(signing_key: &Key) -> bool {
        if <SigningKeyDid>::exists(signing_key) {
            if Self::is_signing_key(&<SigningKeyDid>::get(signing_key), signing_key) {
                if <ChargeDid>::exists(signing_key) {
                    return <ChargeDid>::get(signing_key);
                }
            }
        }
        return false;
    }
}

/// tests for this module
#[cfg(test)]
mod tests {
    use super::*;

    use sr_io::{with_externalities, TestExternalities};
    use sr_primitives::{
        testing::Header,
        traits::{BlakeTwo256, ConvertInto, IdentityLookup},
        Perbill,
    };
    use srml_support::{assert_err, assert_ok, impl_outer_origin, parameter_types};
    use std::result::Result;
    use substrate_primitives::{Blake2Hasher, H256};

    impl_outer_origin! {
        pub enum Origin for IdentityTest {}
    }

    // For testing the module, we construct most of a mock runtime. This means
    // first constructing a configuration type (`Test`) which `impl`s each of the
    // configuration traits of modules we want to use.
    #[derive(Clone, Eq, PartialEq)]
    pub struct IdentityTest;

    parameter_types! {
        pub const BlockHashCount: u32 = 250;
        pub const MaximumBlockWeight: u32 = 4096;
        pub const MaximumBlockLength: u32 = 4096;
        pub const AvailableBlockRatio: Perbill = Perbill::from_percent(75);
    }

    impl system::Trait for IdentityTest {
        type Origin = Origin;
        type Index = u64;
        type BlockNumber = u64;
        type Hash = H256;
        type Hashing = BlakeTwo256;
        type AccountId = u64;
        type Lookup = IdentityLookup<Self::AccountId>;
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

    impl balances::Trait for IdentityTest {
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
        type Identity = super::Module<IdentityTest>;
    }

    parameter_types! {
        pub const MinimumPeriod: u64 = 3;
    }

    impl timestamp::Trait for IdentityTest {
        type Moment = u64;
        type OnTimestampSet = ();
        type MinimumPeriod = MinimumPeriod;
    }

    impl super::Trait for IdentityTest {
        type Event = ();
    }

    type Identity = super::Module<IdentityTest>;

    /// Create externalities
    fn build_ext() -> TestExternalities<Blake2Hasher> {
        system::GenesisConfig::default()
            .build_storage::<IdentityTest>()
            .unwrap()
            .into()
    }

    /// It creates an Account and registers its DID.
    fn make_account(
        id: u64,
    ) -> Result<(<IdentityTest as system::Trait>::Origin, Vec<u8>), &'static str> {
        let signed_id = Origin::signed(id);
        let did = format!("did:poly:{}", id).as_bytes().to_vec();

        Identity::register_did(signed_id.clone(), did.clone(), vec![])?;
        Ok((signed_id, did))
    }

    #[test]
    fn dids_are_unique() {
        with_externalities(&mut build_ext(), || {
            let did_1 = "did:poly:1".as_bytes().to_vec();

            assert_ok!(Identity::register_did(
                Origin::signed(1),
                did_1.clone(),
                vec![]
            ));

            assert_ok!(Identity::register_did(
                Origin::signed(2),
                "did:poly:2".as_bytes().to_vec(),
                vec![]
            ));

            assert_err!(
                Identity::register_did(Origin::signed(3), did_1, vec![]),
                "DID must be unique"
            );
        });
    }

    #[test]
    fn only_claim_issuers_can_add_claims() {
        with_externalities(&mut build_ext(), || {
            let owner_id = Identity::owner();
            let (owner, owner_did) = make_account(owner_id).unwrap();
            let (_issuer, issuer_did) = make_account(2).unwrap();
            let (claim_issuer, claim_issuer_did) = make_account(3).unwrap();

            assert_ok!(Identity::add_signing_keys(
                claim_issuer.clone(),
                claim_issuer_did.clone(),
                vec![Key::try_from(owner_id.encode()).unwrap()]
            ));

            // Create issuer and claim issuer
            assert_ok!(Identity::create_issuer(owner.clone(), issuer_did.clone()));
            assert_ok!(Identity::set_roles(
                owner.clone(),
                claim_issuer_did.clone(),
                vec![IdentityRole::ClaimIssuer]
            ));
            /*assert_ok!(Identity::add_claim_issuer(
                owner.clone(),
                owner_did.clone(),
                claim_issuer_did.clone()
            ));*/

            // Add Claims by master & claim_issuer
            let claims = vec![Claim {
                topic: 1,
                schema: 1,
                bytes: vec![],
                expiry: 10,
            }];

            assert_ok!(Identity::add_claim(
                owner.clone(),
                owner_did.clone(),
                claim_issuer_did.clone(),
                claims.clone()
            ));
            assert_ok!(Identity::add_claim(
                claim_issuer.clone(),
                owner_did.clone(),
                claim_issuer_did.clone(),
                claims.clone()
            ));

            assert_err!(
                Identity::add_claim(
                    claim_issuer.clone(),
                    owner_did.clone(),
                    issuer_did.clone(),
                    claims.clone()
                ),
                "did_issuer must be a claim issuer or master key for DID"
            );
            assert_err!(
                Identity::add_claim(owner.clone(), owner_did.clone(), issuer_did, claims),
                "Sender must hold a claim issuer\'s signing key"
            );
        });
    }

    #[test]
    fn only_master_or_signing_keys_can_authenticate_as_an_identity() {
        with_externalities(&mut build_ext(), || {
            let owner_id = Identity::owner();
            let owner_key = Key::try_from(owner_id.encode()).unwrap();
            let (_owner, owner_did) = make_account(owner_id).unwrap();
            let (a, a_did) = make_account(2).unwrap();
            let (_b, b_did) = make_account(3).unwrap();

            assert_ok!(Identity::add_signing_keys(
                a.clone(),
                a_did.clone(),
                vec![owner_key.clone()]
            ));

            // Check master key on master and signing_keys.
            assert!(Identity::is_signing_key(&owner_did, &owner_key));
            assert!(Identity::is_signing_key(&a_did, &owner_key));

            assert!(Identity::is_signing_key(&b_did, &owner_key) == false);

            // ... and remove that key.
            assert_ok!(Identity::remove_signing_keys(
                a.clone(),
                a_did.clone(),
                vec![owner_key.clone()]
            ));
            assert!(Identity::is_signing_key(&a_did, &owner_key) == false);
        });
    }

    #[test]
    fn revoking_claims() {
        with_externalities(&mut build_ext(), || {
            let owner_id = Identity::owner();
            let owner_key = Key::try_from(owner_id.encode()).unwrap();
            let (owner, owner_did) = make_account(Identity::owner()).unwrap();
            let (issuer, issuer_did) = make_account(2).unwrap();

            let (claim_issuer, claim_issuer_did) = make_account(3).unwrap();
            assert_ok!(Identity::add_signing_keys(
                claim_issuer.clone(),
                claim_issuer_did.clone(),
                vec![owner_key]
            ));
            assert_ok!(Identity::set_roles(
                owner.clone(),
                claim_issuer_did.clone(),
                vec![IdentityRole::ClaimIssuer]
            ));

            // Add Claims by master & claim_issuer
            let claim = Claim {
                topic: 1,
                schema: 1,
                bytes: vec![],
                expiry: 10,
            };

            assert_ok!(Identity::add_claim(
                owner.clone(),
                owner_did.clone(),
                claim_issuer_did.clone(),
                vec![claim.clone()]
            ));

            assert_err!(
                Identity::revoke_claim(
                    issuer.clone(),
                    issuer_did.clone(),
                    claim_issuer_did.clone(),
                    claim.clone()
                ),
                "did_issuer must be a claim issuer for DID"
            );
            assert_err!(
                Identity::revoke_claim(
                    claim_issuer.clone(),
                    claim_issuer_did.clone(),
                    claim_issuer_did.clone(),
                    claim.clone()
                ),
                "did_issuer must be a claim issuer for DID"
            );

            assert_ok!(Identity::revoke_claim(
                owner.clone(),
                owner_did.clone(),
                claim_issuer_did.clone(),
                claim.clone()
            ));
            // TODO Revoke claim twice??
            assert_ok!(Identity::revoke_claim(
                owner,
                owner_did,
                claim_issuer_did,
                claim
            ));
        });
    }

    #[test]
    fn check_grants_test() {
        with_externalities(&mut build_ext(), || {
            let owner_id = Identity::owner();
            let owner_key = Key::try_from(owner_id.encode()).unwrap();
            let (owner, _owner_did) = make_account(Identity::owner()).unwrap();
            let owner2_id = 2;
            let owner2_key = Key::try_from(owner2_id.encode()).unwrap();
            let (_owner2, _owner2_did) = make_account(owner2_id).unwrap();
            let (issuer, issuer_did) = make_account(3).unwrap();

            assert_ok!(Identity::add_signing_keys(
                issuer.clone(),
                issuer_did.clone(),
                vec![owner_key, owner2_key]
            ));

            assert_ok!(Identity::set_roles(
                owner,
                issuer_did.clone(),
                vec![IdentityRole::Issuer]
            ));

            // TODO Check changes in API.
        });
    }
}
