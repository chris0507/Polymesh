/// Voting majority origin for `Instance`.
pub type VMO<Instance> =
    pallet_committee::EnsureThresholdMet<polymesh_primitives::AccountId, Instance>;

pub type GovernanceCommittee = pallet_committee::Instance1;

/// Provides miscellaneous and common pallet-`Config` implementations for a `Runtime`.
#[macro_export]
macro_rules! misc_pallet_impls {
    () => {
        /// Native version.
        #[cfg(any(feature = "std", test))]
        pub fn native_version() -> NativeVersion {
            NativeVersion {
                runtime_version: VERSION,
                can_author_with: Default::default(),
            }
        }

        use sp_runtime::{
            generic, impl_opaque_keys,
            traits::{SaturatedConversion as _, Saturating as _},
            ApplyExtrinsicResult, MultiSignature,
        };

        impl frame_system::Config for Runtime {
            /// The basic call filter to use in dispatchable.
            type BaseCallFilter = frame_support::traits::Everything;
            /// Block & extrinsics weights: base values and limits.
            type BlockWeights = polymesh_runtime_common::RuntimeBlockWeights;
            /// The maximum length of a block (in bytes).
            type BlockLength = polymesh_runtime_common::RuntimeBlockLength;
            /// The designated SS85 prefix of this chain.
            ///
            /// This replaces the "ss58Format" property declared in the chain spec. Reason is
            /// that the runtime should know about the prefix in order to make use of it as
            /// an identifier of the chain.
            type SS58Prefix = SS58Prefix;
            /// The identifier used to distinguish between accounts.
            type AccountId = polymesh_primitives::AccountId;
            /// The aggregated dispatch type that is available for extrinsics.
            type Call = Call;
            /// The lookup mechanism to get account ID from whatever is passed in dispatchers.
            type Lookup = Indices;
            /// The index type for storing how many extrinsics an account has signed.
            type Index = polymesh_primitives::Index;
            /// The index type for blocks.
            type BlockNumber = polymesh_primitives::BlockNumber;
            /// The type for hashing blocks and tries.
            type Hash = polymesh_primitives::Hash;
            /// The hashing algorithm used.
            type Hashing = sp_runtime::traits::BlakeTwo256;
            /// The header type.
            type Header =
                sp_runtime::generic::Header<polymesh_primitives::BlockNumber, BlakeTwo256>;
            /// The ubiquitous event type.
            type Event = Event;
            /// The ubiquitous origin type.
            type Origin = Origin;
            /// Maximum number of block number to block hash mappings to keep (oldest pruned first).
            type BlockHashCount = polymesh_runtime_common::BlockHashCount;
            /// The weight of database operations that the runtime can invoke.
            type DbWeight = polymesh_runtime_common::RocksDbWeight;
            /// Version of the runtime.
            type Version = Version;
            /// Converts a module to the index of the module in `construct_runtime!`.
            ///
            /// This type is being generated by `construct_runtime!`.
            type PalletInfo = PalletInfo;
            /// What to do if a new account is created.
            type OnNewAccount = ();
            /// What to do if an account is fully reaped from the system.
            type OnKilledAccount = ();
            /// The data to be stored in an account.
            type AccountData = polymesh_common_utilities::traits::balances::AccountData;
            type SystemWeightInfo = polymesh_weights::frame_system::WeightInfo;
            type OnSetCode = ();
        }

        impl pallet_base::Config for Runtime {
            type Event = Event;
            type MaxLen = MaxLen;
        }

        impl pallet_babe::Config for Runtime {
            type WeightInfo = polymesh_weights::pallet_babe::WeightInfo;
            type EpochDuration = EpochDuration;
            type ExpectedBlockTime = ExpectedBlockTime;
            type EpochChangeTrigger = pallet_babe::ExternalTrigger;
            type DisabledValidators = Session;

            type KeyOwnerProofSystem = Historical;

            type KeyOwnerProof = <Self::KeyOwnerProofSystem as KeyOwnerProofSystem<(
                sp_core::crypto::KeyTypeId,
                pallet_babe::AuthorityId,
            )>>::Proof;

            type KeyOwnerIdentification = <Self::KeyOwnerProofSystem as KeyOwnerProofSystem<(
                sp_core::crypto::KeyTypeId,
                pallet_babe::AuthorityId,
            )>>::IdentificationTuple;

            type HandleEquivocation = pallet_babe::EquivocationHandler<
                Self::KeyOwnerIdentification,
                Offences,
                ReportLongevity,
            >;
            type MaxAuthorities = MaxAuthorities;
        }

        impl pallet_indices::Config for Runtime {
            type AccountIndex = polymesh_primitives::AccountIndex;
            type Currency = Balances;
            type Deposit = IndexDeposit;
            type Event = Event;
            type WeightInfo = polymesh_weights::pallet_indices::WeightInfo;
        }

        impl pallet_transaction_payment::Config for Runtime {
            type Currency = Balances;
            type OnChargeTransaction =
                pallet_transaction_payment::CurrencyAdapter<Balances, DealWithFees>;
            type TransactionByteFee = polymesh_runtime_common::TransactionByteFee;
            type WeightToFee = polymesh_runtime_common::WeightToFee;
            type FeeMultiplierUpdate = ();
            type CddHandler = CddHandler;
            type Subsidiser = Relayer;
            type GovernanceCommittee = PolymeshCommittee;
            type CddProviders = CddServiceProviders;
            type Identity = Identity;
        }

        impl polymesh_common_utilities::traits::CommonConfig for Runtime {
            type AssetSubTraitTarget = Asset;
            type BlockRewardsReserve = pallet_balances::Pallet<Runtime>;
        }

        impl pallet_balances::Config for Runtime {
            type MaxLocks = MaxLocks;
            type DustRemoval = ();
            type Event = Event;
            type ExistentialDeposit = ExistentialDeposit;
            type AccountStore = frame_system::Pallet<Runtime>;
            type CddChecker = polymesh_runtime_common::cdd_check::CddChecker<Runtime>;
            type WeightInfo = polymesh_weights::pallet_balances::WeightInfo;
        }

        impl pallet_protocol_fee::Config for Runtime {
            type Event = Event;
            type Currency = Balances;
            type OnProtocolFeePayment = DealWithFees;
            type WeightInfo = polymesh_weights::pallet_protocol_fee::WeightInfo;
            type Subsidiser = Relayer;
        }

        impl pallet_timestamp::Config for Runtime {
            type Moment = polymesh_primitives::Moment;
            type OnTimestampSet = Babe;
            type MinimumPeriod = MinimumPeriod;
            type WeightInfo = polymesh_weights::pallet_timestamp::WeightInfo;
        }

        impl pallet_authorship::Config for Runtime {
            type FindAuthor = pallet_session::FindAccountFromAuthorIndex<Self, Babe>;
            type UncleGenerations = UncleGenerations;
            type FilterUncle = ();
            type EventHandler = (Staking, ImOnline);
        }

        impl_opaque_keys! {
            pub struct SessionKeys {
                pub grandpa: Grandpa,
                pub babe: Babe,
                pub im_online: ImOnline,
                pub authority_discovery: AuthorityDiscovery,
            }
        }

        impl pallet_session::Config for Runtime {
            type Event = Event;
            type ValidatorId = polymesh_primitives::AccountId;
            type ValidatorIdOf = pallet_staking::StashOf<Self>;
            type ShouldEndSession = Babe;
            type NextSessionRotation = Babe;
            type SessionManager = pallet_session::historical::NoteHistoricalRoot<Self, Staking>;
            type SessionHandler =
                <SessionKeys as sp_runtime::traits::OpaqueKeys>::KeyTypeIdProviders;
            type Keys = SessionKeys;
            type WeightInfo = polymesh_weights::pallet_session::WeightInfo;
        }

        impl pallet_session::historical::Config for Runtime {
            type FullIdentification = pallet_staking::Exposure<
                polymesh_primitives::AccountId,
                polymesh_primitives::Balance,
            >;
            type FullIdentificationOf = pallet_staking::ExposureOf<Runtime>;
        }

        impl pallet_staking::Config for Runtime {
            const MAX_NOMINATIONS: u32 = pallet_staking::MAX_NOMINATIONS;
            type Currency = Balances;
            type UnixTime = Timestamp;
            type CurrencyToVote = frame_support::traits::U128CurrencyToVote;
            type RewardRemainder = ();
            type Event = Event;
            type Slash = Treasury; // send the slashed funds to the treasury.
            type Reward = (); // rewards are minted from the void
            type SessionsPerEra = SessionsPerEra;
            type BondingDuration = BondingDuration;
            type SlashDeferDuration = SlashDeferDuration;
            type SlashCancelOrigin = polymesh_primitives::EnsureRoot;
            type SessionInterface = Self;
            type RewardCurve = RewardCurve;
            type NextNewSession = Session;
            type ElectionLookahead = ElectionLookahead;
            type Call = Call;
            type MaxIterations = MaxIterations;
            type MinSolutionScoreBump = MinSolutionScoreBump;
            type MaxNominatorRewardedPerValidator = MaxNominatorRewardedPerValidator;
            type UnsignedPriority = StakingUnsignedPriority;
            type RequiredAddOrigin = Self::SlashCancelOrigin;
            type RequiredRemoveOrigin = Self::SlashCancelOrigin;
            type RequiredCommissionOrigin = Self::SlashCancelOrigin;
            type RewardScheduler = Scheduler;
            type MaxValidatorPerIdentity = MaxValidatorPerIdentity;
            type MaxVariableInflationTotalIssuance = MaxVariableInflationTotalIssuance;
            type FixedYearlyReward = FixedYearlyReward;
            type PalletsOrigin = OriginCaller;
            type MinimumBond = MinimumBond;
            // The unsigned solution weight targeted by the OCW. We set it to the maximum possible value of
            // a single extrinsic.
            type OffchainSolutionWeightLimit = polymesh_runtime_common::OffchainSolutionWeightLimit;
            type WeightInfo = polymesh_weights::pallet_staking::WeightInfo;
        }

        impl pallet_authority_discovery::Config for Runtime {
            type MaxAuthorities = MaxAuthorities;
        }

        impl pallet_sudo::Config for Runtime {
            type Event = Event;
            type Call = Call;
        }

        impl pallet_multisig::Config for Runtime {
            type Event = Event;
            type Scheduler = Scheduler;
            type SchedulerCall = Call;
            type WeightInfo = polymesh_weights::pallet_multisig::WeightInfo;
        }

        impl pallet_bridge::Config for Runtime {
            type Event = Event;
            type Proposal = Call;
            type Scheduler = Scheduler;
        }

        impl pallet_portfolio::Config for Runtime {
            type Event = Event;
            type WeightInfo = polymesh_weights::pallet_portfolio::WeightInfo;
        }

        impl pallet_external_agents::Config for Runtime {
            type Event = Event;
            type WeightInfo = polymesh_weights::pallet_external_agents::WeightInfo;
        }

        impl pallet_relayer::Config for Runtime {
            type Event = Event;
            type WeightInfo = polymesh_weights::pallet_relayer::WeightInfo<Runtime>;
        }

        impl pallet_rewards::Config for Runtime {
            type Event = Event;
            type WeightInfo = polymesh_weights::pallet_rewards::WeightInfo<Runtime>;
        }

        impl pallet_asset::Config for Runtime {
            type Event = Event;
            type Currency = Balances;
            type ComplianceManager = pallet_compliance_manager::Module<Runtime>;
            type MaxNumberOfTMExtensionForAsset = MaxNumberOfTMExtensionForAsset;
            type UnixTime = pallet_timestamp::Pallet<Runtime>;
            type AssetNameMaxLength = AssetNameMaxLength;
            type FundingRoundNameMaxLength = FundingRoundNameMaxLength;
            type AssetFn = Asset;
            type WeightInfo = polymesh_weights::pallet_asset::WeightInfo;
            type CPWeightInfo = polymesh_weights::pallet_checkpoint::WeightInfo;
            //type ContractsFn = polymesh_contracts::Module<Runtime>;
        }

        /*
        impl polymesh_contracts::Config for Runtime {
            type Event = Event;
            type NetworkShareInFee = NetworkShareInFee;
            type WeightInfo = polymesh_weights::polymesh_contracts::WeightInfo;
        }
        impl pallet_contracts::Config for Runtime {
            type Time = Timestamp;
            type Randomness = RandomnessCollectiveFlip;
            type Currency = Balances;
            type Event = Event;
            type RentPayment = ();
            type SignedClaimHandicap = polymesh_runtime_common::SignedClaimHandicap;
            type TombstoneDeposit = TombstoneDeposit;
            type DepositPerContract = polymesh_runtime_common::DepositPerContract;
            type DepositPerStorageByte = polymesh_runtime_common::DepositPerStorageByte;
            type DepositPerStorageItem = polymesh_runtime_common::DepositPerStorageItem;
            type RentFraction = RentFraction;
            type SurchargeReward = SurchargeReward;
            type MaxDepth = polymesh_runtime_common::ContractsMaxDepth;
            type MaxValueSize = polymesh_runtime_common::ContractsMaxValueSize;
            type WeightPrice = pallet_transaction_payment::Module<Self>;
            type WeightInfo = pallet_contracts::weights::SubstrateWeight<Self>;
            type ChainExtension = ();
            type DeletionQueueDepth = DeletionQueueDepth;
            type DeletionWeightLimit = DeletionWeightLimit;
            type MaxCodeSize = polymesh_runtime_common::ContractsMaxCodeSize;
        }
        */

        impl pallet_compliance_manager::Config for Runtime {
            type Event = Event;
            type Asset = Asset;
            type WeightInfo = polymesh_weights::pallet_compliance_manager::WeightInfo;
            type MaxConditionComplexity = MaxConditionComplexity;
        }

        impl pallet_corporate_actions::Config for Runtime {
            type Event = Event;
            type MaxTargetIds = MaxTargetIds;
            type MaxDidWhts = MaxDidWhts;
            type WeightInfo = polymesh_weights::pallet_corporate_actions::WeightInfo;
            type BallotWeightInfo = polymesh_weights::pallet_corporate_ballot::WeightInfo;
            type DistWeightInfo = polymesh_weights::pallet_capital_distribution::WeightInfo;
        }

        impl pallet_statistics::Config for Runtime {
            type Event = Event;
            type Asset = Asset;
            type MaxTransferManagersPerAsset = MaxTransferManagersPerAsset;
            type WeightInfo = polymesh_weights::pallet_statistics::WeightInfo;
        }

        impl pallet_utility::Config for Runtime {
            type Event = Event;
            type Call = Call;
            type WeightInfo = polymesh_weights::pallet_utility::WeightInfo;
        }

        impl pallet_scheduler::Config for Runtime {
            type Event = Event;
            type Origin = Origin;
            type PalletsOrigin = OriginCaller;
            type Call = Call;
            type MaximumWeight = MaximumSchedulerWeight;
            type ScheduleOrigin = polymesh_primitives::EnsureRoot;
            type MaxScheduledPerBlock = MaxScheduledPerBlock;
            type WeightInfo = polymesh_weights::pallet_scheduler::WeightInfo;
            type OriginPrivilegeCmp = frame_support::traits::EqualPrivilegeOnly;
        }

        impl pallet_offences::Config for Runtime {
            type Event = Event;
            type IdentificationTuple = pallet_session::historical::IdentificationTuple<Self>;
            type OnOffenceHandler = Staking;
        }

        type GrandpaKey = (sp_core::crypto::KeyTypeId, pallet_grandpa::AuthorityId);

        impl pallet_im_online::Config for Runtime {
            type AuthorityId = pallet_im_online::sr25519::AuthorityId;
            type Event = Event;
            type NextSessionRotation = Babe;
            type ValidatorSet = Historical;
            type UnsignedPriority = ImOnlineUnsignedPriority;
            type ReportUnresponsiveness = Offences;
            type WeightInfo = polymesh_weights::pallet_im_online::WeightInfo;
            type MaxKeys = MaxKeys;
            type MaxPeerInHeartbeats = MaxPeerInHeartbeats;
            type MaxPeerDataEncodingSize = MaxPeerDataEncodingSize;
        }

        impl pallet_grandpa::Config for Runtime {
            type WeightInfo = polymesh_weights::pallet_grandpa::WeightInfo;
            type Event = Event;
            type Call = Call;

            type KeyOwnerProofSystem = Historical;

            type KeyOwnerProof =
                <Self::KeyOwnerProofSystem as KeyOwnerProofSystem<GrandpaKey>>::Proof;

            type KeyOwnerIdentification =
                <Self::KeyOwnerProofSystem as KeyOwnerProofSystem<GrandpaKey>>::IdentificationTuple;

            type HandleEquivocation = pallet_grandpa::EquivocationHandler<
                Self::KeyOwnerIdentification,
                Offences,
                ReportLongevity,
            >;
            type MaxAuthorities = MaxAuthorities;
        }

        impl pallet_randomness_collective_flip::Config for Runtime {}

        impl pallet_treasury::Config for Runtime {
            type Event = Event;
            type Currency = Balances;
            type WeightInfo = polymesh_weights::pallet_treasury::WeightInfo;
        }

        impl pallet_settlement::Config for Runtime {
            type Event = Event;
            type Scheduler = Scheduler;
            type WeightInfo = polymesh_weights::pallet_settlement::WeightInfo;
            type MaxLegsInInstruction = MaxLegsInInstruction;
        }

        impl pallet_sto::Config for Runtime {
            type Event = Event;
            type WeightInfo = polymesh_weights::pallet_sto::WeightInfo;
        }

        impl polymesh_common_utilities::traits::permissions::Config for Runtime {
            type Checker = Identity;
        }

        impl<LocalCall> frame_system::offchain::CreateSignedTransaction<LocalCall> for Runtime
        where
            Call: From<LocalCall>,
        {
            fn create_transaction<
                C: frame_system::offchain::AppCrypto<Self::Public, Self::Signature>,
            >(
                call: Call,
                public: <polymesh_primitives::Signature as Verify>::Signer,
                account: polymesh_primitives::AccountId,
                nonce: polymesh_primitives::Index,
            ) -> Option<(Call, <UncheckedExtrinsic as Extrinsic>::SignaturePayload)> {
                // take the biggest period possible.
                let period = polymesh_runtime_common::BlockHashCount::get()
                    .checked_next_power_of_two()
                    .map(|c| c / 2)
                    .unwrap_or(2) as u64;
                let current_block = System::block_number()
                    .saturated_into::<u64>()
                    // The `System::block_number` is initialized with `n+1`,
                    // so the actual block number is `n`.
                    .saturating_sub(1);
                let tip = 0;
                let extra: SignedExtra = (
                    frame_system::CheckSpecVersion::new(),
                    frame_system::CheckTxVersion::new(),
                    frame_system::CheckGenesis::new(),
                    frame_system::CheckEra::from(generic::Era::mortal(period, current_block)),
                    frame_system::CheckNonce::from(nonce),
                    polymesh_extensions::CheckWeight::new(),
                    pallet_transaction_payment::ChargeTransactionPayment::from(tip),
                    pallet_permissions::StoreCallMetadata::new(),
                );
                let raw_payload = SignedPayload::new(call, extra)
                    .map_err(|e| {
                        log::warn!("Unable to create signed payload: {:?}", e);
                    })
                    .ok()?;
                let signature = raw_payload.using_encoded(|payload| C::sign(payload, public))?;
                let address = Indices::unlookup(account);
                let (call, extra, _) = raw_payload.deconstruct();
                Some((call, (address, signature, extra)))
            }
        }

        impl frame_system::offchain::SigningTypes for Runtime {
            type Public = <polymesh_primitives::Signature as Verify>::Signer;
            type Signature = polymesh_primitives::Signature;
        }

        impl<C> frame_system::offchain::SendTransactionTypes<C> for Runtime
        where
            Call: From<C>,
        {
            type Extrinsic = UncheckedExtrinsic;
            type OverarchingCall = Call;
        }
    };
}

/// Defines API implementations, e.g., for RPCs, and type aliases, for a `Runtime`.
#[macro_export]
macro_rules! runtime_apis {
    ($($extra:item)*) => {
        use node_rpc_runtime_api::asset as rpc_api_asset;
        use sp_inherents::{CheckInherentsResult, InherentData};
        //use pallet_contracts_primitives::ContractExecResult;
        use pallet_identity::types::{AssetDidResult, CddStatus, DidRecords, DidStatus, KeyIdentityData};
        use pallet_pips::{Vote, VoteCount};
        use pallet_protocol_fee_rpc_runtime_api::CappedFee;
        use polymesh_primitives::{calendar::CheckpointId, compliance_manager::AssetComplianceResult, IdentityId, Index, PortfolioId, SecondaryKey, Signatory, Ticker};

        /// The address format for describing accounts.
        pub type Address = <Indices as StaticLookup>::Source;
        /// Block header type as expected by this runtime.
        pub type Header = generic::Header<BlockNumber, BlakeTwo256>;
        /// Block type as expected by this runtime.
        pub type Block = generic::Block<Header, UncheckedExtrinsic>;
        /// A Block signed with a Justification
        pub type SignedBlock = generic::SignedBlock<Block>;
        /// BlockId type as expected by this runtime.
        pub type BlockId = generic::BlockId<Block>;
        /// The SignedExtension to the basic transaction logic.
        pub type SignedExtra = (
            frame_system::CheckSpecVersion<Runtime>,
            frame_system::CheckTxVersion<Runtime>,
            frame_system::CheckGenesis<Runtime>,
            frame_system::CheckEra<Runtime>,
            frame_system::CheckNonce<Runtime>,
            polymesh_extensions::CheckWeight<Runtime>,
            pallet_transaction_payment::ChargeTransactionPayment<Runtime>,
            pallet_permissions::StoreCallMetadata<Runtime>,
        );
        /// Unchecked extrinsic type as expected by this runtime.
        pub type UncheckedExtrinsic = generic::UncheckedExtrinsic<Address, Call, polymesh_primitives::Signature, SignedExtra>;
        /// The payload being signed in transactions.
        pub type SignedPayload = generic::SignedPayload<Call, SignedExtra>;
        /// Extrinsic type that has already been checked.
        pub type CheckedExtrinsic = generic::CheckedExtrinsic<polymesh_primitives::AccountId, Call, SignedExtra>;
        /// Executive: handles dispatch to the various modules.
        pub type Executive = pallet_executive::Executive<
            Runtime,
            Block,
            frame_system::ChainContext<Runtime>,
            Runtime,
            AllPallets,
        >;

        sp_api::impl_runtime_apis! {
            impl sp_api::Core<Block> for Runtime {
                fn version() -> RuntimeVersion {
                    VERSION
                }

                fn execute_block(block: Block) {
                    Executive::execute_block(block)
                }

                fn initialize_block(header: &<Block as BlockT>::Header) {
                    Executive::initialize_block(header)
                }
            }

            impl sp_api::Metadata<Block> for Runtime {
                fn metadata() -> sp_core::OpaqueMetadata {
                    sp_core::OpaqueMetadata::new(Runtime::metadata().into())
                }
            }

            impl sp_block_builder::BlockBuilder<Block> for Runtime {
                fn apply_extrinsic(extrinsic: <Block as BlockT>::Extrinsic) -> ApplyExtrinsicResult {
                    Executive::apply_extrinsic(extrinsic)
                }

                fn finalize_block() -> <Block as BlockT>::Header {
                    Executive::finalize_block()
                }

                fn inherent_extrinsics(data: InherentData) -> Vec<<Block as BlockT>::Extrinsic> {
                    data.create_extrinsics()
                }

                fn check_inherents(block: Block, data: InherentData) -> CheckInherentsResult {
                    data.check_extrinsics(&block)
                }
            }

            impl sp_transaction_pool::runtime_api::TaggedTransactionQueue<Block> for Runtime {
                fn validate_transaction(
                    source: sp_runtime::transaction_validity::TransactionSource,
                    tx: <Block as BlockT>::Extrinsic,
                    block_hash: <Block as BlockT>::Hash,
                ) -> sp_runtime::transaction_validity::TransactionValidity {
                    Executive::validate_transaction(source, tx, block_hash)
                }
            }

            impl sp_offchain::OffchainWorkerApi<Block> for Runtime {
                fn offchain_worker(header: &<Block as BlockT>::Header) {
                    Executive::offchain_worker(header)
                }
            }

            impl pallet_grandpa::fg_primitives::GrandpaApi<Block> for Runtime {
                fn grandpa_authorities() -> pallet_grandpa::fg_primitives::AuthorityList {
                    Grandpa::grandpa_authorities()
                }

                fn submit_report_equivocation_unsigned_extrinsic(
                    equivocation_proof: pallet_grandpa::fg_primitives::EquivocationProof<
                        <Block as BlockT>::Hash,
                        NumberFor<Block>,
                    >,
                    key_owner_proof: pallet_grandpa::fg_primitives::OpaqueKeyOwnershipProof,
                ) -> Option<()> {
                    let key_owner_proof = key_owner_proof.decode()?;

                    Grandpa::submit_unsigned_equivocation_report(
                        equivocation_proof,
                        key_owner_proof,
                    )
                }

                fn generate_key_ownership_proof(
                    _set_id: pallet_grandpa::fg_primitives::SetId,
                    authority_id: pallet_grandpa::AuthorityId,
                ) -> Option<pallet_grandpa::fg_primitives::OpaqueKeyOwnershipProof> {
                    use codec::Encode;

                    Historical::prove((pallet_grandpa::fg_primitives::KEY_TYPE, authority_id))
                        .map(|p| p.encode())
                        .map(pallet_grandpa::fg_primitives::OpaqueKeyOwnershipProof::new)
                }

                fn current_set_id() -> pallet_grandpa::fg_primitives::SetId {
                    Grandpa::current_set_id()
                }
            }

            impl sp_consensus_babe::BabeApi<Block> for Runtime {
                fn configuration() -> sp_consensus_babe::BabeGenesisConfiguration {
                    // The choice of `c` parameter (where `1 - c` represents the
                    // probability of a slot being empty), is done in accordance to the
                    // slot duration and expected target block time, for safely
                    // resisting network delays of maximum two seconds.
                    // <https://research.web3.foundation/en/latest/polkadot/BABE/Babe/#6-practical-results>
                    sp_consensus_babe::BabeGenesisConfiguration {
                        slot_duration: Babe::slot_duration(),
                        epoch_length: EpochDuration::get(),
                        c: PRIMARY_PROBABILITY,
                        genesis_authorities: Babe::authorities().to_vec(),
                        randomness: Babe::randomness(),
                        allowed_slots: sp_consensus_babe::AllowedSlots::PrimaryAndSecondaryPlainSlots,
                    }
                }

                fn current_epoch_start() -> sp_consensus_babe::Slot{
                    Babe::current_epoch_start()
                }

                fn current_epoch() -> sp_consensus_babe::Epoch {
                    Babe::current_epoch()
                }

                fn next_epoch() -> sp_consensus_babe::Epoch {
                    Babe::next_epoch()
                }

                fn generate_key_ownership_proof(
                    _slot: sp_consensus_babe::Slot,
                    authority_id: sp_consensus_babe::AuthorityId,
                ) -> Option<sp_consensus_babe::OpaqueKeyOwnershipProof> {
                    use codec::Encode;

                    Historical::prove((sp_consensus_babe::KEY_TYPE, authority_id))
                        .map(|p| p.encode())
                        .map(sp_consensus_babe::OpaqueKeyOwnershipProof::new)
                }

                fn submit_report_equivocation_unsigned_extrinsic(
                    equivocation_proof: sp_consensus_babe::EquivocationProof<<Block as BlockT>::Header>,
                    key_owner_proof: sp_consensus_babe::OpaqueKeyOwnershipProof,
                ) -> Option<()> {
                    let key_owner_proof = key_owner_proof.decode()?;

                    Babe::submit_unsigned_equivocation_report(
                        equivocation_proof,
                        key_owner_proof,
                    )
                }
            }

            impl sp_authority_discovery::AuthorityDiscoveryApi<Block> for Runtime {
                fn authorities() -> Vec<sp_authority_discovery::AuthorityId> {
                    AuthorityDiscovery::authorities()
                }
            }

            impl frame_system_rpc_runtime_api::AccountNonceApi<Block, polymesh_primitives::AccountId, Index> for Runtime {
                fn account_nonce(account: polymesh_primitives::AccountId) -> Index {
                    System::account_nonce(account)
                }
            }

            /*
            impl pallet_contracts_rpc_runtime_api::ContractsApi<Block, polymesh_primitives::AccountId, Balance, BlockNumber>
                for Runtime
            {
                fn call(
                    origin: polymesh_primitives::AccountId,
                    dest: polymesh_primitives::AccountId,
                    value: Balance,
                    gas_limit: u64,
                    input_data: Vec<u8>,
                ) -> ContractExecResult {
                    BaseContracts::bare_call(origin, dest.into(), value, gas_limit, input_data)
                }

                fn get_storage(
                    address: polymesh_primitives::AccountId,
                    key: [u8; 32],
                ) -> pallet_contracts_primitives::GetStorageResult {
                    BaseContracts::get_storage(address, key)
                }

                fn rent_projection(
                    address: polymesh_primitives::AccountId,
                ) -> pallet_contracts_primitives::RentProjectionResult<BlockNumber> {
                    BaseContracts::rent_projection(address)
                }
            }
            */

            impl node_rpc_runtime_api::transaction_payment::TransactionPaymentApi<
                Block,
                UncheckedExtrinsic,
            > for Runtime {
                fn query_info(uxt: UncheckedExtrinsic, len: u32) -> RuntimeDispatchInfo<Balance> {
                    TransactionPayment::query_info(uxt, len)
                }
            }

            impl sp_session::SessionKeys<Block> for Runtime {
                fn generate_session_keys(seed: Option<Vec<u8>>) -> Vec<u8> {
                    SessionKeys::generate(seed)
                }

                fn decode_session_keys(
                    encoded: Vec<u8>,
                ) -> Option<Vec<(Vec<u8>, sp_core::crypto::KeyTypeId)>> {
                    SessionKeys::decode_into_raw_public_keys(&encoded)
                }
            }

            impl pallet_staking_rpc_runtime_api::StakingApi<Block> for Runtime {
                fn get_curve() -> Vec<(Perbill, Perbill)> {
                    RewardCurve::get().points.to_vec()
                }
            }

            impl node_rpc_runtime_api::pips::PipsApi<Block, polymesh_primitives::AccountId>
            for Runtime
            {
                /// Vote count for the PIP identified by `id`.
                fn get_votes(id: pallet_pips::PipId) -> VoteCount {
                    Pips::get_votes(id)
                }

                /// PIPs voted on by `address`.
                fn proposed_by(address: polymesh_primitives::AccountId) -> Vec<pallet_pips::PipId> {
                    Pips::proposed_by(pallet_pips::Proposer::Community(address))
                }

                /// PIPs `address` voted on.
                fn voted_on(address: polymesh_primitives::AccountId) -> Vec<pallet_pips::PipId> {
                    Pips::voted_on(address)
                }
            }

            impl pallet_protocol_fee_rpc_runtime_api::ProtocolFeeApi<
                Block,
            > for Runtime {
                fn compute_fee(op: ProtocolOp) -> CappedFee {
                    ProtocolFee::compute_fee(&[op]).into()
                }
            }

            impl
                node_rpc_runtime_api::identity::IdentityApi<
                    Block,
                    IdentityId,
                    Ticker,
                    polymesh_primitives::AccountId,
                    SecondaryKey<polymesh_primitives::AccountId>,
                    Signatory<polymesh_primitives::AccountId>,
                    Moment
                > for Runtime
            {
                /// RPC call to know whether the given did has valid cdd claim or not
                fn is_identity_has_valid_cdd(did: IdentityId, leeway: Option<u64>) -> CddStatus {
                    Identity::fetch_cdd(did, leeway.unwrap_or_default())
                        .ok_or_else(|| "Either cdd claim is expired or not yet provided to give identity".into())
                }

                /// RPC call to query the given ticker did
                fn get_asset_did(ticker: Ticker) -> AssetDidResult {
                    Identity::get_token_did(&ticker)
                        .map_err(|_| "Error in computing the given ticker error".into())
                }

                /// Retrieve primary key and secondary keys for a given IdentityId
                fn get_did_records(did: IdentityId) -> DidRecords<polymesh_primitives::AccountId, SecondaryKey<polymesh_primitives::AccountId>> {
                    Identity::get_did_records(did)
                }

                /// Retrieve the status of the DIDs
                fn get_did_status(dids: Vec<IdentityId>) -> Vec<DidStatus> {
                    Identity::get_did_status(dids)
                }

                fn get_key_identity_data(acc: polymesh_primitives::AccountId) -> Option<KeyIdentityData<IdentityId>> {
                    Identity::get_key_identity_data(acc)
                }

                /// Retrieve list of a authorization for a given signatory
                fn get_filtered_authorizations(
                    signatory: Signatory<polymesh_primitives::AccountId>,
                    allow_expired: bool,
                    auth_type: Option<polymesh_primitives::AuthorizationType>
                ) -> Vec<polymesh_primitives::Authorization<polymesh_primitives::AccountId, Moment>> {
                    Identity::get_filtered_authorizations(signatory, allow_expired, auth_type)
                }
            }

            impl rpc_api_asset::AssetApi<Block, polymesh_primitives::AccountId> for Runtime {
                #[inline]
                fn can_transfer(
                    _sender: polymesh_primitives::AccountId,
                    from_custodian: Option<IdentityId>,
                    from_portfolio: PortfolioId,
                    to_custodian: Option<IdentityId>,
                    to_portfolio: PortfolioId,
                    ticker: &Ticker,
                    value: Balance) -> rpc_api_asset::CanTransferResult
                {
                    Asset::unsafe_can_transfer(from_custodian, from_portfolio, to_custodian, to_portfolio, ticker, value)
                        .map_err(|msg| msg.as_bytes().to_vec())
                }

                #[inline]
                fn can_transfer_granular(
                    from_custodian: Option<IdentityId>,
                    from_portfolio: PortfolioId,
                    to_custodian: Option<IdentityId>,
                    to_portfolio: PortfolioId,
                    ticker: &Ticker,
                    value: Balance
                ) -> polymesh_primitives::asset::GranularCanTransferResult
                {
                    Asset::unsafe_can_transfer_granular(from_custodian, from_portfolio, to_custodian, to_portfolio, ticker, value)
                }
            }

            impl node_rpc_runtime_api::compliance_manager::ComplianceManagerApi<Block, polymesh_primitives::AccountId>
                for Runtime
            {
                #[inline]
                fn can_transfer(
                    ticker: Ticker,
                    from_did: Option<IdentityId>,
                    to_did: Option<IdentityId>,
                ) -> AssetComplianceResult
                {
                    use polymesh_common_utilities::compliance_manager::Config;
                    ComplianceManager::verify_restriction_granular(&ticker, from_did, to_did)
                }
            }

            impl pallet_group_rpc_runtime_api::GroupApi<Block> for Runtime {
                fn get_cdd_valid_members() -> Vec<pallet_group_rpc_runtime_api::Member> {
                    merge_active_and_inactive::<Block>(
                        CddServiceProviders::active_members(),
                        CddServiceProviders::inactive_members())
                }

                fn get_gc_valid_members() -> Vec<pallet_group_rpc_runtime_api::Member> {
                    merge_active_and_inactive::<Block>(
                        CommitteeMembership::active_members(),
                        CommitteeMembership::inactive_members())
                }
            }

            $($extra)*
        }
    }
}
