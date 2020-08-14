use grandpa::AuthorityId as GrandpaId;
use im_online::sr25519::AuthorityId as ImOnlineId;
use pallet_asset::TickerRegistrationConfig;
use polymesh_common_utilities::{constants::currency::POLY, protocol_fee::ProtocolOp};
use polymesh_primitives::{AccountId, IdentityId, InvestorUid, PosRatio, Signatory, Signature};
use polymesh_runtime_develop::{
    self as general,
    config::{self as GeneralConfig},
    constants::time as generalTime,
};
use polymesh_runtime_testnet_v1::{
    self as aldebaran,
    config::{self as AldebaranConfig},
    constants::time as aldebaranTime,
};
use sc_chain_spec::ChainType;
use sc_service::Properties;
use sc_telemetry::TelemetryEndpoints;
use serde_json::json;
use sp_authority_discovery::AuthorityId as AuthorityDiscoveryId;
use sp_consensus_babe::AuthorityId as BabeId;
use sp_core::{sr25519, Pair, Public};
use sp_runtime::{
    traits::{IdentifyAccount, Verify},
    PerThing,
};
use std::iter;

const STAGING_TELEMETRY_URL: &str = "wss://telemetry.polymesh.live/submit/";

pub type AldebaranChainSpec = sc_service::GenericChainSpec<AldebaranConfig::GenesisConfig>;
pub type GeneralChainSpec = sc_service::GenericChainSpec<GeneralConfig::GenesisConfig>;

type AccountPublic = <Signature as Verify>::Signer;

fn aldebaran_session_keys(
    grandpa: GrandpaId,
    babe: BabeId,
    im_online: ImOnlineId,
    authority_discovery: AuthorityDiscoveryId,
) -> aldebaran::SessionKeys {
    aldebaran::SessionKeys {
        babe,
        grandpa,
        im_online,
        authority_discovery,
    }
}

fn general_session_keys(
    grandpa: GrandpaId,
    babe: BabeId,
    im_online: ImOnlineId,
    authority_discovery: AuthorityDiscoveryId,
) -> general::SessionKeys {
    general::SessionKeys {
        babe,
        grandpa,
        im_online,
        authority_discovery,
    }
}

/// Helper function to generate a crypto pair from seed
pub fn get_from_seed<TPublic: Public>(seed: &str) -> <TPublic::Pair as Pair>::Public {
    TPublic::Pair::from_string(&format!("//{}", seed), None)
        .expect("static values are valid; qed")
        .public()
}

/// Helper function to generate an account ID from seed
pub fn get_account_id_from_seed<TPublic: Public>(seed: &str) -> AccountId
where
    AccountPublic: From<<TPublic::Pair as Pair>::Public>,
{
    AccountPublic::from(get_from_seed::<TPublic>(seed)).into_account()
}

/// Helper function to generate stash, controller and session key from seed
pub fn get_authority_keys_from_seed(
    seed: &str,
    uniq: bool,
) -> (
    AccountId,
    AccountId,
    GrandpaId,
    BabeId,
    ImOnlineId,
    AuthorityDiscoveryId,
) {
    if uniq {
        (
            get_account_id_from_seed::<sr25519::Public>(&format!("{}//stash", seed)),
            get_account_id_from_seed::<sr25519::Public>(seed),
            get_from_seed::<GrandpaId>(&format!("{}//gran", seed)),
            get_from_seed::<BabeId>(&format!("{}//babe", seed)),
            get_from_seed::<ImOnlineId>(&format!("{}//imon", seed)),
            get_from_seed::<AuthorityDiscoveryId>(&format!("{}//auth", seed)),
        )
    } else {
        (
            get_account_id_from_seed::<sr25519::Public>(&format!("{}//stash", seed)),
            get_account_id_from_seed::<sr25519::Public>(seed),
            get_from_seed::<GrandpaId>(seed),
            get_from_seed::<BabeId>(seed),
            get_from_seed::<ImOnlineId>(seed),
            get_from_seed::<AuthorityDiscoveryId>(seed),
        )
    }
}

fn polymath_props() -> Properties {
    json!({"tokenDecimals": 6, "tokenSymbol": "POLYX" })
        .as_object()
        .unwrap()
        .clone()
}

fn general_testnet_genesis(
    initial_authorities: Vec<(
        AccountId,
        AccountId,
        GrandpaId,
        BabeId,
        ImOnlineId,
        AuthorityDiscoveryId,
    )>,
    root_key: AccountId,
    endowed_accounts: Vec<AccountId>,
    enable_println: bool,
) -> GeneralConfig::GenesisConfig {
    const STASH: u128 = 5_000_000 * POLY;
    const ENDOWMENT: u128 = 100_000_000 * POLY;
    const BRIDGE_CREATOR_ID: u128 = 6;
    const BRIDGE_CREATOR_ID_BALANCE: u128 = 1_000 * POLY;

    GeneralConfig::GenesisConfig {
        frame_system: Some(GeneralConfig::SystemConfig {
            code: general::WASM_BINARY.to_vec(),
            changes_trie_config: Default::default(),
        }),
        asset: Some(GeneralConfig::AssetConfig {
            ticker_registration_config: TickerRegistrationConfig {
                max_ticker_length: 12,
                registration_length: Some(5_184_000_000),
            },
        }),
        identity: {
            let initial_identities = vec![
                // (primary_account_id, service provider did, target did, expiry time of CustomerDueDiligence claim i.e 10 days is ms)
                // Service providers
                (
                    get_account_id_from_seed::<sr25519::Public>("cdd_provider_1"),
                    IdentityId::from(1),
                    IdentityId::from(1),
                    InvestorUid::from(b"uid1".as_ref()),
                    None,
                ),
                (
                    get_account_id_from_seed::<sr25519::Public>("cdd_provider_2"),
                    IdentityId::from(2),
                    IdentityId::from(2),
                    InvestorUid::from(b"uid2".as_ref()),
                    None,
                ),
                // Governance committee members
                (
                    get_account_id_from_seed::<sr25519::Public>("governance_committee_1"),
                    IdentityId::from(1),
                    IdentityId::from(3),
                    InvestorUid::from(b"uid3".as_ref()),
                    None,
                ),
                (
                    get_account_id_from_seed::<sr25519::Public>("governance_committee_2"),
                    IdentityId::from(1),
                    IdentityId::from(4),
                    InvestorUid::from(b"uid4".as_ref()),
                    None,
                ),
                (
                    get_account_id_from_seed::<sr25519::Public>("governance_committee_3"),
                    IdentityId::from(1),
                    IdentityId::from(5),
                    InvestorUid::from(b"uid4".as_ref()),
                    None,
                ),
            ];
            let num_initial_identities = initial_identities.len() as u128;
            let mut identity_counter = num_initial_identities;
            let authority_identities = initial_authorities
                .iter()
                .map(|x| {
                    identity_counter = identity_counter + 1;
                    let did = IdentityId::from(identity_counter);
                    let investor_uid = InvestorUid::from(did.as_ref());
                    (x.1.clone(), IdentityId::from(1), did, investor_uid, None)
                })
                .collect::<Vec<_>>();

            let all_identities = initial_identities
                .iter()
                .cloned()
                .chain(authority_identities.iter().cloned())
                .collect::<Vec<_>>();
            identity_counter = num_initial_identities;
            let secondary_keys = initial_authorities
                .iter()
                .map(|x| {
                    identity_counter += 1;
                    (x.0.clone(), IdentityId::from(identity_counter))
                })
                .collect::<Vec<_>>();

            Some(GeneralConfig::IdentityConfig {
                identities: all_identities,
                secondary_keys,
                ..Default::default()
            })
        },
        balances: Some(GeneralConfig::BalancesConfig {
            balances: endowed_accounts
                .iter()
                .map(|k: &AccountId| (k.clone(), ENDOWMENT))
                .chain(initial_authorities.iter().map(|x| (x.1.clone(), ENDOWMENT)))
                .chain(initial_authorities.iter().map(|x| (x.0.clone(), STASH)))
                .collect(),
        }),
        bridge: Some(GeneralConfig::BridgeConfig {
            admin: initial_authorities[0].1.clone(),
            creator: initial_authorities[0].1.clone(),
            signatures_required: 1,
            signers: vec![
                Signatory::Account(AccountId::from(
                    get_from_seed::<sr25519::Public>("relay_1").0,
                )),
                Signatory::Account(AccountId::from(
                    get_from_seed::<sr25519::Public>("relay_2").0,
                )),
                Signatory::Account(AccountId::from(
                    get_from_seed::<sr25519::Public>("relay_3").0,
                )),
                Signatory::Account(AccountId::from(
                    get_from_seed::<sr25519::Public>("relay_4").0,
                )),
                Signatory::Account(AccountId::from(
                    get_from_seed::<sr25519::Public>("relay_5").0,
                )),
            ],
            timelock: 10,
            bridge_limit: (100_000_000 * POLY, 1000),
        }),
        pallet_indices: Some(GeneralConfig::IndicesConfig { indices: vec![] }),
        pallet_sudo: Some(GeneralConfig::SudoConfig { key: root_key }),
        pallet_session: Some(GeneralConfig::SessionConfig {
            keys: initial_authorities
                .iter()
                .map(|x| {
                    (
                        x.0.clone(),
                        x.0.clone(),
                        general_session_keys(x.2.clone(), x.3.clone(), x.4.clone(), x.5.clone()),
                    )
                })
                .collect::<Vec<_>>(),
        }),
        pallet_staking: Some(GeneralConfig::StakingConfig {
            minimum_validator_count: 1,
            validator_count: 2,
            validator_commission: aldebaran::Commission::Global(
                PerThing::from_rational_approximation(1u64, 4u64),
            ),
            stakers: initial_authorities
                .iter()
                .map(|x| {
                    (
                        x.0.clone(),
                        x.1.clone(),
                        STASH,
                        general::StakerStatus::Validator,
                    )
                })
                .collect(),
            invulnerables: initial_authorities.iter().map(|x| x.0.clone()).collect(),
            slash_reward_fraction: general::Perbill::from_percent(10),
            min_bond_threshold: 5_000_000_000_000,
            ..Default::default()
        }),
        pallet_pips: Some(GeneralConfig::PipsConfig {
            prune_historical_pips: false,
            min_proposal_deposit: 0,
            proposal_cool_off_period: generalTime::MINUTES,
            default_enactment_period: generalTime::MINUTES,
            max_pip_skip_count: 1,
            active_pip_limit: 25,
        }),
        pallet_im_online: Some(GeneralConfig::ImOnlineConfig {
            slashing_params: general::OfflineSlashingParams {
                max_offline_percent: 10u32,
                constant: 3u32,
                max_slash_percent: 7u32,
            },
            ..Default::default()
        }),
        pallet_authority_discovery: Some(Default::default()),
        pallet_babe: Some(Default::default()),
        pallet_grandpa: Some(Default::default()),
        pallet_contracts: Some(GeneralConfig::ContractsConfig {
            current_schedule: contracts::Schedule {
                enable_println, // this should only be enabled on development chains
                ..Default::default()
            },
        }),
        // Governance Council:
        group_Instance1: Some(general::runtime::CommitteeMembershipConfig {
            active_members: vec![
                IdentityId::from(3),
                IdentityId::from(4),
                IdentityId::from(5),
                IdentityId::from(6),
            ],
            phantom: Default::default(),
        }),
        committee_Instance1: Some(GeneralConfig::PolymeshCommitteeConfig {
            vote_threshold: (1, 2),
            members: vec![],
            release_coordinator: IdentityId::from(6),
            phantom: Default::default(),
        }),
        group_Instance2: Some(general::runtime::CddServiceProvidersConfig {
            // sp1, sp2, first authority
            active_members: vec![
                IdentityId::from(1),
                IdentityId::from(2),
                IdentityId::from(6),
            ],
            phantom: Default::default(),
        }),
        // Technical Committee:
        group_Instance3: Some(general::runtime::TechnicalCommitteeMembershipConfig {
            active_members: vec![IdentityId::from(3)],
            phantom: Default::default(),
        }),
        committee_Instance3: Some(GeneralConfig::TechnicalCommitteeConfig {
            vote_threshold: (1, 2),
            members: vec![],
            release_coordinator: IdentityId::from(3),
            phantom: Default::default(),
        }),
        // Upgrade Committee:
        group_Instance4: Some(general::runtime::UpgradeCommitteeMembershipConfig {
            active_members: vec![IdentityId::from(4)],
            phantom: Default::default(),
        }),
        committee_Instance4: Some(GeneralConfig::UpgradeCommitteeConfig {
            vote_threshold: (1, 2),
            members: vec![],
            release_coordinator: IdentityId::from(4),
            phantom: Default::default(),
        }),
        protocol_fee: Some(GeneralConfig::ProtocolFeeConfig {
            base_fees: vec![
                (ProtocolOp::AssetCreateAsset, 10_000 * 1_000_000),
                (ProtocolOp::AssetRegisterTicker, 2_500 * 1_000_000),
            ],
            coefficient: PosRatio(1, 1),
        }),
        settlement: Some(Default::default()),
    }
}

fn general_development_genesis() -> GeneralConfig::GenesisConfig {
    general_testnet_genesis(
        vec![get_authority_keys_from_seed("Alice", false)],
        get_account_id_from_seed::<sr25519::Public>("Alice"),
        vec![
            get_account_id_from_seed::<sr25519::Public>("Bob"),
            get_account_id_from_seed::<sr25519::Public>("relay_1"),
            get_account_id_from_seed::<sr25519::Public>("relay_2"),
            get_account_id_from_seed::<sr25519::Public>("relay_3"),
            get_account_id_from_seed::<sr25519::Public>("relay_4"),
            get_account_id_from_seed::<sr25519::Public>("relay_5"),
        ],
        true,
    )
}

pub fn general_development_testnet_config() -> GeneralChainSpec {
    GeneralChainSpec::from_genesis(
        "Development",
        "dev",
        ChainType::Development,
        general_development_genesis,
        vec![],
        None,
        None,
        Some(polymath_props()),
        None,
    )
}

fn general_local_genesis() -> GeneralConfig::GenesisConfig {
    general_testnet_genesis(
        vec![
            get_authority_keys_from_seed("Alice", false),
            get_authority_keys_from_seed("Bob", false),
        ],
        get_account_id_from_seed::<sr25519::Public>("Alice"),
        vec![
            get_account_id_from_seed::<sr25519::Public>("Charlie"),
            get_account_id_from_seed::<sr25519::Public>("Dave"),
            get_account_id_from_seed::<sr25519::Public>("Charlie//stash"),
            get_account_id_from_seed::<sr25519::Public>("relay_1"),
            get_account_id_from_seed::<sr25519::Public>("relay_2"),
            get_account_id_from_seed::<sr25519::Public>("relay_3"),
            get_account_id_from_seed::<sr25519::Public>("relay_4"),
            get_account_id_from_seed::<sr25519::Public>("relay_5"),
        ],
        true,
    )
}

pub fn general_local_testnet_config() -> GeneralChainSpec {
    GeneralChainSpec::from_genesis(
        "Local Development",
        "local_dev",
        ChainType::Local,
        general_local_genesis,
        vec![],
        None,
        None,
        Some(polymath_props()),
        None,
    )
}

fn general_live_genesis() -> GeneralConfig::GenesisConfig {
    general_testnet_genesis(
        vec![
            get_authority_keys_from_seed("Alice", false),
            get_authority_keys_from_seed("Bob", false),
            get_authority_keys_from_seed("Charlie", false),
        ],
        get_account_id_from_seed::<sr25519::Public>("Alice"),
        vec![
            get_account_id_from_seed::<sr25519::Public>("Dave"),
            get_account_id_from_seed::<sr25519::Public>("Eve"),
            get_account_id_from_seed::<sr25519::Public>("Ferdie"),
            get_account_id_from_seed::<sr25519::Public>("Dave//stash"),
            get_account_id_from_seed::<sr25519::Public>("Eve//stash"),
            get_account_id_from_seed::<sr25519::Public>("Ferdie//stash"),
            get_account_id_from_seed::<sr25519::Public>("relay_1"),
            get_account_id_from_seed::<sr25519::Public>("relay_2"),
            get_account_id_from_seed::<sr25519::Public>("relay_3"),
            get_account_id_from_seed::<sr25519::Public>("relay_4"),
            get_account_id_from_seed::<sr25519::Public>("relay_5"),
        ],
        false,
    )
}

pub fn general_live_testnet_config() -> GeneralChainSpec {
    GeneralChainSpec::from_genesis(
        "Live Development",
        "live_dev",
        ChainType::Live,
        general_live_genesis,
        vec![],
        None,
        None,
        Some(polymath_props()),
        None,
    )
}

fn aldebaran_live_testnet_genesis() -> AldebaranConfig::GenesisConfig {
    aldebaran_testnet_genesis(
        vec![
            get_authority_keys_from_seed("operator_1", true),
            get_authority_keys_from_seed("operator_2", true),
            get_authority_keys_from_seed("operator_3", true),
            get_authority_keys_from_seed("operator_4", true),
            get_authority_keys_from_seed("operator_5", true),
        ],
        get_account_id_from_seed::<sr25519::Public>("polymath_1"),
        vec![
            get_account_id_from_seed::<sr25519::Public>("polymath_1"),
            get_account_id_from_seed::<sr25519::Public>("polymath_2"),
            get_account_id_from_seed::<sr25519::Public>("polymath_3"),
            get_account_id_from_seed::<sr25519::Public>("relay_1"),
            get_account_id_from_seed::<sr25519::Public>("relay_2"),
            get_account_id_from_seed::<sr25519::Public>("relay_3"),
            get_account_id_from_seed::<sr25519::Public>("relay_4"),
            get_account_id_from_seed::<sr25519::Public>("relay_5"),
        ],
        false,
    )
}

pub fn aldebaran_live_testnet_config() -> AldebaranChainSpec {
    // provide boot nodes
    let boot_nodes = vec![];
    AldebaranChainSpec::from_genesis(
        "Polymesh Aldebaran Testnet",
        "aldebaran",
        ChainType::Live,
        aldebaran_live_testnet_genesis,
        boot_nodes,
        Some(
            TelemetryEndpoints::new(vec![(STAGING_TELEMETRY_URL.to_string(), 0)])
                .expect("Aldebaran live telemetry url is valid; qed"),
        ),
        Some(&*"/polymath/aldebaran/1"),
        Some(polymath_props()),
        Default::default(),
    )
}

fn aldebaran_develop_testnet_genesis() -> AldebaranConfig::GenesisConfig {
    aldebaran_testnet_genesis(
        vec![get_authority_keys_from_seed("Alice", false)],
        get_account_id_from_seed::<sr25519::Public>("Alice"),
        vec![
            get_account_id_from_seed::<sr25519::Public>("Bob"),
            get_account_id_from_seed::<sr25519::Public>("Bob//stash"),
            get_account_id_from_seed::<sr25519::Public>("relay_1"),
            get_account_id_from_seed::<sr25519::Public>("relay_2"),
            get_account_id_from_seed::<sr25519::Public>("relay_3"),
            get_account_id_from_seed::<sr25519::Public>("relay_4"),
            get_account_id_from_seed::<sr25519::Public>("relay_5"),
        ],
        true,
    )
}

pub fn aldebaran_develop_testnet_config() -> AldebaranChainSpec {
    // provide boot nodes
    let boot_nodes = vec![];
    AldebaranChainSpec::from_genesis(
        "Polymesh Aldebaran Develop",
        "dev_aldebaran",
        ChainType::Development,
        aldebaran_develop_testnet_genesis,
        boot_nodes,
        None,
        None,
        Some(polymath_props()),
        Default::default(),
    )
}

fn aldebaran_local_testnet_genesis() -> AldebaranConfig::GenesisConfig {
    aldebaran_testnet_genesis(
        vec![
            get_authority_keys_from_seed("Alice", false),
            get_authority_keys_from_seed("Bob", false),
        ],
        get_account_id_from_seed::<sr25519::Public>("Alice"),
        vec![
            get_account_id_from_seed::<sr25519::Public>("Charlie"),
            get_account_id_from_seed::<sr25519::Public>("Dave"),
            get_account_id_from_seed::<sr25519::Public>("Charlie//stash"),
            get_account_id_from_seed::<sr25519::Public>("relay_1"),
            get_account_id_from_seed::<sr25519::Public>("relay_2"),
            get_account_id_from_seed::<sr25519::Public>("relay_3"),
            get_account_id_from_seed::<sr25519::Public>("relay_4"),
            get_account_id_from_seed::<sr25519::Public>("relay_5"),
        ],
        true,
    )
}

pub fn aldebaran_local_testnet_config() -> AldebaranChainSpec {
    // provide boot nodes
    let boot_nodes = vec![];
    AldebaranChainSpec::from_genesis(
        "Polymesh Aldebaran Local",
        "local_aldebaran",
        ChainType::Local,
        aldebaran_local_testnet_genesis,
        boot_nodes,
        None,
        None,
        Some(polymath_props()),
        Default::default(),
    )
}

fn aldebaran_testnet_genesis(
    initial_authorities: Vec<(
        AccountId,
        AccountId,
        GrandpaId,
        BabeId,
        ImOnlineId,
        AuthorityDiscoveryId,
    )>,
    root_key: AccountId,
    endowed_accounts: Vec<AccountId>,
    enable_println: bool,
) -> AldebaranConfig::GenesisConfig {
    const STASH: u128 = 5_000_000 * POLY;
    const ENDOWMENT: u128 = 100_000_000 * POLY;

    AldebaranConfig::GenesisConfig {
        frame_system: Some(AldebaranConfig::SystemConfig {
            code: aldebaran::WASM_BINARY.to_vec(),
            changes_trie_config: Default::default(),
        }),
        asset: Some(AldebaranConfig::AssetConfig {
            ticker_registration_config: TickerRegistrationConfig {
                max_ticker_length: 12,
                registration_length: Some(5_184_000_000),
            },
        }),
        identity: {
            let initial_identities = vec![
                // (primary_account_id, service provider did, target did, expiry time of CustomerDueDiligence claim i.e 10 days is ms)
                // Service providers
                (
                    get_account_id_from_seed::<sr25519::Public>("cdd_provider_1"),
                    IdentityId::from(1),
                    IdentityId::from(1),
                    InvestorUid::from(b"uid1".as_ref()),
                    None,
                ),
                (
                    get_account_id_from_seed::<sr25519::Public>("cdd_provider_2"),
                    IdentityId::from(2),
                    IdentityId::from(2),
                    InvestorUid::from(b"uid2".as_ref()),
                    None,
                ),
                (
                    get_account_id_from_seed::<sr25519::Public>("cdd_provider_3"),
                    IdentityId::from(3),
                    IdentityId::from(3),
                    InvestorUid::from(b"uid3".as_ref()),
                    None,
                ),
                // Governance committee members
                (
                    get_account_id_from_seed::<sr25519::Public>("polymath_1"),
                    IdentityId::from(1),
                    IdentityId::from(4),
                    InvestorUid::from(b"uid4".as_ref()),
                    None,
                ),
                (
                    get_account_id_from_seed::<sr25519::Public>("polymath_2"),
                    IdentityId::from(2),
                    IdentityId::from(5),
                    InvestorUid::from(b"uid5".as_ref()),
                    None,
                ),
                (
                    get_account_id_from_seed::<sr25519::Public>("polymath_3"),
                    IdentityId::from(3),
                    IdentityId::from(6),
                    InvestorUid::from(b"uid6".as_ref()),
                    None,
                ),
            ];
            let num_initial_identities = initial_identities.len() as u128;
            let mut identity_counter = num_initial_identities;
            let authority_identities = initial_authorities
                .iter()
                .map(|x| {
                    identity_counter = identity_counter + 1;
                    let did = IdentityId::from(identity_counter);
                    let investor_uid = InvestorUid::from(did.as_ref());
                    (x.1.clone(), IdentityId::from(1), did, investor_uid, None)
                })
                .collect::<Vec<_>>();

            let all_identities = initial_identities
                .iter()
                .cloned()
                .chain(authority_identities.iter().cloned())
                .collect::<Vec<_>>();
            identity_counter = num_initial_identities;
            let secondary_keys = initial_authorities
                .iter()
                .map(|x| {
                    identity_counter += 1;
                    (x.0.clone(), IdentityId::from(identity_counter))
                })
                .collect::<Vec<_>>();

            Some(AldebaranConfig::IdentityConfig {
                identities: all_identities,
                secondary_keys,
                ..Default::default()
            })
        },
        balances: Some(AldebaranConfig::BalancesConfig {
            balances: endowed_accounts
                .iter()
                .map(|k: &AccountId| (k.clone(), ENDOWMENT))
                .chain(initial_authorities.iter().map(|x| (x.1.clone(), ENDOWMENT)))
                .chain(initial_authorities.iter().map(|x| (x.0.clone(), STASH)))
                .collect(),
        }),
        bridge: Some(AldebaranConfig::BridgeConfig {
            admin: get_account_id_from_seed::<sr25519::Public>("polymath_1"),
            creator: get_account_id_from_seed::<sr25519::Public>("polymath_1"),
            signatures_required: 3,
            signers: vec![
                Signatory::Account(AccountId::from(
                    get_from_seed::<sr25519::Public>("relay_1").0,
                )),
                Signatory::Account(AccountId::from(
                    get_from_seed::<sr25519::Public>("relay_2").0,
                )),
                Signatory::Account(AccountId::from(
                    get_from_seed::<sr25519::Public>("relay_3").0,
                )),
                Signatory::Account(AccountId::from(
                    get_from_seed::<sr25519::Public>("relay_4").0,
                )),
                Signatory::Account(AccountId::from(
                    get_from_seed::<sr25519::Public>("relay_5").0,
                )),
            ],
            timelock: aldebaranTime::MINUTES * 15,
            bridge_limit: (30_000_000_000, aldebaranTime::DAYS),
        }),
        pallet_indices: Some(AldebaranConfig::IndicesConfig { indices: vec![] }),
        pallet_sudo: Some(AldebaranConfig::SudoConfig { key: root_key }),
        pallet_session: Some(AldebaranConfig::SessionConfig {
            keys: initial_authorities
                .iter()
                .map(|x| {
                    (
                        x.0.clone(),
                        x.0.clone(),
                        aldebaran_session_keys(x.2.clone(), x.3.clone(), x.4.clone(), x.5.clone()),
                    )
                })
                .collect::<Vec<_>>(),
        }),
        pallet_staking: Some(AldebaranConfig::StakingConfig {
            minimum_validator_count: 1,
            validator_count: initial_authorities.len() as u32,
            validator_commission: aldebaran::Commission::Global(PerThing::zero()),
            stakers: initial_authorities
                .iter()
                .map(|x| {
                    (
                        x.0.clone(),
                        x.1.clone(),
                        STASH,
                        aldebaran::StakerStatus::Validator,
                    )
                })
                .collect(),
            invulnerables: initial_authorities.iter().map(|x| x.0.clone()).collect(),
            slash_reward_fraction: aldebaran::Perbill::from_percent(10),
            min_bond_threshold: 5_000_000_000_000,
            ..Default::default()
        }),
        pallet_pips: Some(AldebaranConfig::PipsConfig {
            prune_historical_pips: false,
            min_proposal_deposit: 0,
            proposal_cool_off_period: aldebaranTime::HOURS * 6,
            default_enactment_period: aldebaranTime::DAYS * 7,
            max_pip_skip_count: 1,
            active_pip_limit: 1000,
        }),
        pallet_im_online: Some(AldebaranConfig::ImOnlineConfig {
            slashing_params: aldebaran::OfflineSlashingParams {
                max_offline_percent: 10u32,
                constant: 3u32,
                max_slash_percent: 7u32,
            },
            ..Default::default()
        }),
        pallet_authority_discovery: Some(Default::default()),
        pallet_babe: Some(Default::default()),
        pallet_grandpa: Some(Default::default()),
        pallet_contracts: Some(AldebaranConfig::ContractsConfig {
            current_schedule: contracts::Schedule {
                enable_println, // this should only be enabled on development chains
                ..Default::default()
            },
        }),
        group_Instance1: Some(aldebaran::runtime::CommitteeMembershipConfig {
            active_members: vec![
                IdentityId::from(4),
                IdentityId::from(5),
                IdentityId::from(6),
            ],
            phantom: Default::default(),
        }),
        committee_Instance1: Some(aldebaran::runtime::PolymeshCommitteeConfig {
            vote_threshold: (2, 3),
            members: vec![],
            release_coordinator: IdentityId::from(6),
            phantom: Default::default(),
        }),
        group_Instance2: Some(aldebaran::runtime::CddServiceProvidersConfig {
            // sp1, sp2, sp3
            active_members: vec![
                IdentityId::from(1),
                IdentityId::from(2),
                IdentityId::from(3),
            ],
            phantom: Default::default(),
        }),
        // Technical Committee:
        group_Instance3: Some(aldebaran::runtime::TechnicalCommitteeMembershipConfig {
            active_members: vec![IdentityId::from(4)],
            phantom: Default::default(),
        }),
        committee_Instance3: Some(aldebaran::runtime::TechnicalCommitteeConfig {
            vote_threshold: (1, 2),
            members: vec![],
            release_coordinator: IdentityId::from(4),
            phantom: Default::default(),
        }),
        // Upgrade Committee:
        group_Instance4: Some(aldebaran::runtime::UpgradeCommitteeMembershipConfig {
            active_members: vec![IdentityId::from(5)],
            phantom: Default::default(),
        }),
        committee_Instance4: Some(aldebaran::runtime::UpgradeCommitteeConfig {
            vote_threshold: (1, 2),
            members: vec![],
            release_coordinator: IdentityId::from(5),
            phantom: Default::default(),
        }),
        protocol_fee: Some(AldebaranConfig::ProtocolFeeConfig {
            base_fees: vec![
                (ProtocolOp::AssetCreateAsset, 10_000 * 1_000_000),
                (ProtocolOp::AssetRegisterTicker, 2_500 * 1_000_000),
            ],
            coefficient: PosRatio(1, 1),
        }),
        settlement: Some(Default::default()),
    }
}
