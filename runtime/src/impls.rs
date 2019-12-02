//! Auxillary struct/enums

use crate::constants::fee::TARGET_BLOCK_FULLNESS;
use crate::{Authorship, Balances, MaximumBlockWeight, NegativeImbalance};
use primitives::Balance;
use sr_primitives::traits::{Convert, Saturating};
use sr_primitives::weights::{Weight, WeightMultiplier};
use sr_primitives::Fixed64;
use srml_support::traits::{Currency, OnUnbalanced};

/// Logic for the author to get a portion of fees.
pub struct ToAuthor;

impl OnUnbalanced<NegativeImbalance> for ToAuthor {
    fn on_unbalanced(amount: NegativeImbalance) {
        Balances::resolve_creating(&Authorship::author(), amount);
    }
}

/// Converter for currencies to votes.
pub struct CurrencyToVoteHandler;

impl CurrencyToVoteHandler {
    fn factor() -> u128 {
        (Balances::total_issuance() / u64::max_value() as u128).max(1)
    }
}

impl Convert<u128, u64> for CurrencyToVoteHandler {
    fn convert(x: u128) -> u64 {
        (x / Self::factor()) as u64
    }
}

impl Convert<u128, u128> for CurrencyToVoteHandler {
    fn convert(x: u128) -> u128 {
        x * Self::factor()
    }
}

/// Handles converting a weight scalar to a fee value, based on the scale and granularity of the
/// node's balance type.
///
/// This should typically create a mapping between the following ranges:
///   - [0, system::MaximumBlockWeight]
///   - [Balance::min, Balance::max]
///
/// Yet, it can be used for any other sort of change to weight-fee. Some examples being:
///   - Setting it to `0` will essentially disable the weight fee.
///   - Setting it to `1` will cause the literal `#[weight = x]` values to be charged.
pub struct WeightToFee;
impl Convert<Weight, Balance> for WeightToFee {
    fn convert(x: Weight) -> Balance {
        // in Polkadot a weight of 10_000 (smallest non-zero weight) to be mapped to 10^7 units of
        // fees (1/10 CENT), hence:
        Balance::from(x).saturating_mul(1)
    }
}

/// A struct that updates the weight multiplier based on the saturation level of the previous block.
/// This should typically be called once per-block.
///
/// This assumes that weight is a numeric value in the u32 range.
///
/// Given `TARGET_BLOCK_FULLNESS = 1/2`, a block saturation greater than 1/2 will cause the system
/// fees to slightly grow and the opposite for block saturations less than 1/2.
///
/// Formula:
///   diff = (target_weight - current_block_weight)
///   v = 0.00004
///   next_weight = weight * (1 + (v . diff) + (v . diff)^2 / 2)
///
/// https://research.web3.foundation/en/latest/polkadot/Token%20Economics/#relay-chain-transaction-fees
pub struct WeightMultiplierUpdateHandler;

impl Convert<(Weight, WeightMultiplier), WeightMultiplier> for WeightMultiplierUpdateHandler {
    fn convert(previous_state: (Weight, WeightMultiplier)) -> WeightMultiplier {
        let (block_weight, multiplier) = previous_state;
        let max_weight = MaximumBlockWeight::get();
        let target_weight = (TARGET_BLOCK_FULLNESS * max_weight) as u128;
        let block_weight = block_weight as u128;

        // determines if the first_term is positive
        let positive = block_weight >= target_weight;
        let diff_abs = block_weight.max(target_weight) - block_weight.min(target_weight);
        // diff is within u32, safe.
        let diff = Fixed64::from_rational(diff_abs as i64, max_weight as u64);
        let diff_squared = diff.saturating_mul(diff);

        // 0.00004 = 4/100_000 = 40_000/10^9
        let v = Fixed64::from_rational(4, 100_000);
        // 0.00004^2 = 16/10^10 ~= 2/10^9. Taking the future /2 into account, then it is just 1 parts
        // from a billionth.
        let v_squared_2 = Fixed64::from_rational(1, 1_000_000_000);

        let first_term = v.saturating_mul(diff);
        // It is very unlikely that this will exist (in our poor perbill estimate) but we are giving
        // it a shot.
        let second_term = v_squared_2.saturating_mul(diff_squared);

        if positive {
            // Note: this is merely bounded by how big the multiplier and the inner value can go,
            // not by any economical reasoning.
            let excess = first_term.saturating_add(second_term);
            multiplier.saturating_add(WeightMultiplier::from_fixed(excess))
        } else {
            // first_term > second_term
            let negative = first_term - second_term;
            multiplier
                .saturating_sub(WeightMultiplier::from_fixed(negative))
                // despite the fact that apply_to saturates weight (final fee cannot go below 0)
                // it is crucially important to stop here and don't further reduce the weight fee
                // multiplier. While at -1, it means that the network is so un-congested that all
                // transactions have no weight fee. We stop here and only increase if the network
                // became more busy.
                .max(WeightMultiplier::from_rational(-1, 1))
        }
    }
}
