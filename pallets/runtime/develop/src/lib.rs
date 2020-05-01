#![cfg_attr(not(feature = "std"), no_std)]
// `construct_runtime!` does a lot of recursion and requires us to increase the limit to 256.
#![recursion_limit = "256"]

pub mod fee_details;
pub mod runtime;
pub use fee_details::CddHandler;
pub mod constants;

#[cfg(feature = "std")]
pub use pallet_staking::{Commission, StakerStatus};

pub use pallet_im_online::OfflineSlashingParams;
#[cfg(feature = "std")]
pub use runtime::{native_version, WASM_BINARY};

pub use runtime::{
    api, Asset, Authorship, Balances, Bridge, Contracts, ProtocolFee, Runtime, RuntimeApi,
    SessionKeys, System, TargetBlockFullness, TransactionPayment, UncheckedExtrinsic,
};

#[cfg(feature = "std")]
pub mod config {

    use pallet_asset as asset;
    use pallet_balances as balances;
    use pallet_committee as committee;
    use pallet_identity as identity;
    use pallet_protocol_fee as protocol_fee;

    pub type AssetConfig = asset::GenesisConfig<crate::Runtime>;
    pub type BalancesConfig = balances::GenesisConfig<crate::Runtime>;
    pub type BridgeConfig = polymesh_runtime_common::bridge::GenesisConfig<crate::Runtime>;
    pub type IdentityConfig = identity::GenesisConfig<crate::Runtime>;
    pub type StakingConfig = pallet_staking::GenesisConfig<crate::Runtime>;
    pub type PolymeshCommitteeConfig =
        committee::GenesisConfig<crate::Runtime, committee::Instance1>;
    pub type PipsConfig = pallet_pips::GenesisConfig<crate::Runtime>;
    pub type ContractsConfig = pallet_contracts::GenesisConfig<crate::Runtime>;
    pub type IndicesConfig = pallet_indices::GenesisConfig<crate::Runtime>;
    pub type ImOnlineConfig = pallet_im_online::GenesisConfig<crate::Runtime>;
    pub type SudoConfig = pallet_sudo::GenesisConfig<crate::Runtime>;
    pub type SystemConfig = frame_system::GenesisConfig;
    pub type GenesisConfig = crate::runtime::GenesisConfig;
    pub type SessionConfig = pallet_session::GenesisConfig<crate::Runtime>;
    pub type ProtocolFeeConfig = protocol_fee::GenesisConfig<crate::Runtime>;
}

pub use sp_runtime::{Perbill, Permill};
