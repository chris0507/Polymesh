use crate::traits::identity::IdentityTrait;

use polymesh_primitives::{AccountKey, IdentityId, Signatory};
use sp_runtime::DispatchError;

/// Helper class to access to some context information.
/// Currently it allows to access to
///     - `current_identity` throught an `IdentityTrait`, because it is stored using extrinsics.
///     .
#[derive(Default)]
pub struct Context {}

impl Context {
    #[inline]
    #[cfg(not(feature = "default_identity"))]
    pub fn current_identity<I: IdentityTrait>() -> Option<IdentityId> {
        I::current_identity()
    }

    #[inline]
    #[cfg(feature = "default_identity")]
    pub fn current_identity<I: IdentityTrait>() -> Option<IdentityId> {
        I::current_identity().or_else(|| Some(IdentityId::default()))
    }

    #[inline]
    pub fn set_current_identity<I: IdentityTrait>(id: Option<IdentityId>) {
        I::set_current_identity(id)
    }

    #[inline]
    pub fn current_payer<I: IdentityTrait>() -> Option<Signatory> {
        I::current_payer()
    }

    #[inline]
    pub fn set_current_payer<I: IdentityTrait>(payer: Option<Signatory>) {
        I::set_current_payer(payer)
    }

    /// It gets the current identity and if it is none, it will use the identity from `key`.
    /// This function is a helper tool for testing where SignedExtension is not used and
    /// `current_identity` is always none.
    #[cfg(not(feature = "default_identity"))]
    pub fn current_identity_or<I: IdentityTrait>(
        key: &AccountKey,
    ) -> Result<IdentityId, DispatchError> {
        Self::current_identity::<I>()
            .or_else(|| I::get_identity(key))
            .ok_or_else(|| {
                DispatchError::Other(
                    "Current identity is none and key is not linked to any identity",
                )
            })
    }

    #[cfg(feature = "default_identity")]
    pub fn current_identity_or<I: IdentityTrait>(
        key: &AccountKey,
    ) -> Result<IdentityId, DispatchError> {
        I::current_identity()
            .or_else(|| I::get_identity(key))
            .or_else(|| Some(IdentityId::default()))
            .ok_or_else(|| DispatchError::Other("Unreachable code"))
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use polymesh_primitives::{Permission, Signatory};

    use lazy_static::lazy_static;
    use std::{collections::BTreeMap, convert::From, sync::RwLock, thread};
    use test_client::AccountKeyring;

    lazy_static! {
        pub static ref CURR_IDENTITY: RwLock<Option<IdentityId>> = RwLock::new(None);
    }

    struct IdentityTest {}

    impl IdentityTrait for IdentityTest {
        fn get_identity(key: &AccountKey) -> Option<IdentityId> {
            let keys: BTreeMap<AccountKey, u128> = vec![
                (AccountKey::from(AccountKeyring::Alice.public().0), 1u128),
                (AccountKey::from(AccountKeyring::Bob.public().0), 2u128),
                (AccountKey::from(AccountKeyring::Charlie.public().0), 3u128),
            ]
            .into_iter()
            .collect();

            if let Some(id) = keys.get(key) {
                Some(IdentityId::from(*id))
            } else {
                None
            }
        }

        fn current_identity() -> Option<IdentityId> {
            let r = CURR_IDENTITY.read().unwrap();
            r.clone()
        }

        fn set_current_identity(id: Option<IdentityId>) {
            let mut w = CURR_IDENTITY.write().unwrap();
            *w = id;
        }

        fn is_signer_authorized(_did: IdentityId, _signer: &Signatory) -> bool {
            false
        }

        fn is_master_key(_did: IdentityId, _key: &AccountKey) -> bool {
            false
        }

        fn is_signer_authorized_with_permissions(
            _did: IdentityId,
            _signer: &Signatory,
            _permissions: Vec<Permission>,
        ) -> bool {
            false
        }

        fn unsafe_add_systematic_cdd_claims(_targets: &[IdentityId]) {}
        fn unsafe_revoke_systematic_cdd_claims(_targets: &[IdentityId]) {}
    }

    #[test]
    fn context_functions() -> Result<(), &'static str> {
        assert_eq!(Context::current_identity::<IdentityTest>(), None);
        Context::set_current_identity::<IdentityTest>(Some(IdentityId::from(42)));

        let _ = thread::spawn(|| {
            let id = Context::current_identity::<IdentityTest>();
            assert_eq!(id, Some(IdentityId::from(42u128)));
            Context::set_current_identity::<IdentityTest>(None);
        })
        .join()
        .map_err(|_| "Poison error")?;

        assert_eq!(Context::current_identity::<IdentityTest>(), None);

        let _ = thread::spawn(|| {
            let id = Context::current_identity::<IdentityTest>();
            assert_eq!(id, None);
            Context::set_current_identity::<IdentityTest>(Some(IdentityId::from(15)));
        })
        .join()
        .map_err(|_| "Poison error")?;

        assert_eq!(
            Context::current_identity::<IdentityTest>(),
            Some(IdentityId::from(15))
        );

        // Check "or" option.
        let alice = AccountKey::from(AccountKeyring::Alice.public().0);
        assert_eq!(
            Context::current_identity_or::<IdentityTest>(&alice),
            Ok(IdentityId::from(15))
        );
        Context::set_current_identity::<IdentityTest>(None);
        assert_eq!(
            Context::current_identity_or::<IdentityTest>(&alice),
            Ok(IdentityId::from(1))
        );

        let eve = AccountKey::from(AccountKeyring::Eve.public().0);
        assert_eq!(
            Context::current_identity_or::<IdentityTest>(&eve),
            Err(DispatchError::Other(
                "Current identity is none and key is not linked to any identity"
            ))
        );

        Ok(())
    }
}
