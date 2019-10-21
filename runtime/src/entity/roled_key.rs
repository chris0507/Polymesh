use rstd::{prelude::Vec, vec};

// use crate::entity::IgnoredCaseString;

/// Size of key, when it is u64
#[cfg(test)]
const KEY_SIZE: usize = 8;
#[cfg(not(test))]
const KEY_SIZE: usize = 32;

/// Identity roles.
/// # TODO
/// 2. Review documents:
///     - [MESH-235](https://polymath.atlassian.net/browse/MESH-235)
///     - [Polymesh: Roles/Permissions](https://docs.google.com/document/d/12u-rMavow4fvidsFlLcLe7DAXuqWk8XUHOBV9kw05Z8/)
///
#[derive(codec::Encode, codec::Decode, Clone, Copy, PartialEq, Eq, Debug)]
pub enum IdentityRole {
    Full,
    Admin,
    Operator,
    Issuer,
    Validator,
    // From MESH-235
    Investor,
    NodeRunner,
    PM,
    KYCAMLClaimIssuer,
    AccreditedInvestorClaimIssuer,
    VerifiedIdentityClaimIssuer,
    // Future or custom identities
    // Custom(IgnoredCaseString),
}

#[derive(codec::Encode, codec::Decode, Default, Clone, Eq, Debug)]
/// It is a key, and its associated roles.
pub struct RoledKey {
    pub key: [u8; KEY_SIZE],
    pub roles: Vec<IdentityRole>,
}

impl RoledKey {
    pub fn new(key: &[u8], roles: Vec<IdentityRole>) -> Self {
        let mut s = Self {
            key: [0u8; KEY_SIZE],
            roles,
        };
        s.key.copy_from_slice(key);

        s
    }

    /// It checks if this key has specified `role` role.
    /// Role `IdentityRole::Full` is special and denotates that this key can be used for any role.
    pub fn has_role(&self, role: IdentityRole) -> bool {
        self.roles
            .iter()
            .find(|&r| role == *r || *r == IdentityRole::Full)
            .is_some()
    }
}

impl From<&[u8]> for RoledKey {
    fn from(s: &[u8]) -> Self {
        Self::new(s, vec![])
    }
}

impl PartialEq for RoledKey {
    fn eq(&self, other: &Self) -> bool {
        self.key == other.key && self.roles == other.roles
    }
}

impl PartialEq<&[u8]> for RoledKey {
    fn eq(&self, other: &&[u8]) -> bool {
        self.key == *other
    }
}

impl PartialEq<Vec<u8>> for RoledKey {
    fn eq(&self, other: &Vec<u8>) -> bool {
        self.key == other.as_slice()
    }
}

#[cfg(test)]
mod tests {
    use super::{IdentityRole, RoledKey};

    #[test]
    fn build_test() {
        let key = "ABCDABCD".as_bytes();
        let rk1 = RoledKey::new(key, vec![]);
        let rk2 = RoledKey::from(key);
        assert_eq!(rk1, rk2);

        let rk3 = RoledKey::new(key, vec![IdentityRole::Operator, IdentityRole::Issuer]);
        assert_ne!(rk1, rk3);

        let mut rk4 = RoledKey::from(key);
        rk4.roles = vec![IdentityRole::Operator, IdentityRole::Issuer];
        assert_eq!(rk3, rk4);
    }

    #[test]
    fn full_role_test() {
        let key = "ABCDABCD".as_bytes();
        let full_key = RoledKey::new(key, vec![IdentityRole::Full]);
        let not_full_key = RoledKey::new(key, vec![IdentityRole::Issuer, IdentityRole::Operator]);

        assert_eq!(full_key.has_role(IdentityRole::Issuer), true);
        assert_eq!(full_key.has_role(IdentityRole::Operator), true);

        assert_eq!(not_full_key.has_role(IdentityRole::Issuer), true);
        assert_eq!(not_full_key.has_role(IdentityRole::Operator), true);
        assert_eq!(not_full_key.has_role(IdentityRole::Validator), false);
    }

    #[test]
    #[should_panic]
    fn panic_build_test() {
        let _rk_panic = RoledKey::from("ABCDABCDx".as_bytes());
    }
}
