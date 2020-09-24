//! THIS FILE WAS AUTO-GENERATED USING THE SUBSTRATE BENCHMARK CLI VERSION 2.0.0-rc6

#![allow(unused_parens)]
#![allow(unused_imports)]

use frame_support::weights::{constants::RocksDbWeight as DbWeight, Weight};

pub struct WeightInfo;
impl pallet_timestamp::WeightInfo for WeightInfo {
    fn set() -> Weight {
        (5191000 as Weight)
            .saturating_add(DbWeight::get().reads(2 as Weight))
            .saturating_add(DbWeight::get().writes(1 as Weight))
    }
    // WARNING! Some components were not used: ["t"]
    fn on_finalize() -> Weight {
        (3693000 as Weight)
    }
}
