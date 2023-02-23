// This file is part of Substrate.

// Copyright (C) 2021 Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Autogenerated weights for polymesh_contracts
//!
//! THIS FILE WAS AUTO-GENERATED USING THE SUBSTRATE BENCHMARK CLI VERSION 4.0.0-dev
//! DATE: 2023-02-20, STEPS: `100`, REPEAT: 5, LOW RANGE: `[]`, HIGH RANGE: `[]`
//! EXECUTION: Some(Wasm), WASM-EXECUTION: Compiled, CHAIN: None, DB CACHE: 512

// Executed Command:
// ./target/release/polymesh
// benchmark
// pallet
// -s
// 100
// -r
// 5
// -p=polymesh_contracts
// -e=*
// --heap-pages
// 4096
// --db-cache
// 512
// --execution
// wasm
// --wasm-execution
// compiled
// --output
// ./pallets/weights/src/
// --template
// ./.maintain/frame-weight-template.hbs

#![allow(unused_parens)]
#![allow(unused_imports)]

use polymesh_runtime_common::{RocksDbWeight as DbWeight, Weight};

/// Weights for polymesh_contracts using the Substrate node and recommended hardware.
pub struct WeightInfo;
impl polymesh_contracts::WeightInfo for WeightInfo {
    // Storage: Identity KeyRecords (r:2 w:0)
    // Storage: System Account (r:1 w:0)
    // Storage: Contracts ContractInfoOf (r:1 w:1)
    // Storage: Contracts CodeStorage (r:1 w:0)
    // Storage: Timestamp Now (r:1 w:0)
    // Storage: unknown [0x3a7472616e73616374696f6e5f6c6576656c3a] (r:1 w:1)
    // Storage: Identity IsDidFrozen (r:1 w:0)
    // Storage: Instance2Group ActiveMembers (r:1 w:0)
    // Storage: Instance2Group InactiveMembers (r:1 w:0)
    // Storage: Identity Claims (r:2 w:0)
    // Storage: unknown [0x00] (r:1 w:0)
    // Storage: unknown [0x000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f] (r:1 w:0)
    fn chain_extension_read_storage(k: u32, v: u32) -> Weight {
        (575_457_000 as Weight)
            // Standard Error: 3_000
            .saturating_add((5_000 as Weight).saturating_mul(k as Weight))
            // Standard Error: 3_000
            .saturating_add((6_000 as Weight).saturating_mul(v as Weight))
            .saturating_add(DbWeight::get().reads(13 as Weight))
            .saturating_add(DbWeight::get().writes(2 as Weight))
    }
    // Storage: Identity KeyRecords (r:2 w:0)
    // Storage: System Account (r:1 w:0)
    // Storage: Contracts ContractInfoOf (r:1 w:1)
    // Storage: Contracts CodeStorage (r:1 w:0)
    // Storage: Timestamp Now (r:1 w:0)
    // Storage: unknown [0x3a7472616e73616374696f6e5f6c6576656c3a] (r:1 w:1)
    // Storage: Identity IsDidFrozen (r:1 w:0)
    // Storage: Instance2Group ActiveMembers (r:1 w:0)
    // Storage: Instance2Group InactiveMembers (r:1 w:0)
    // Storage: Identity Claims (r:2 w:0)
    fn chain_extension_get_version(r: u32) -> Weight {
        (549_682_000 as Weight)
            // Standard Error: 9_429_000
            .saturating_add((169_925_000 as Weight).saturating_mul(r as Weight))
            .saturating_add(DbWeight::get().reads(12 as Weight))
            .saturating_add(DbWeight::get().writes(2 as Weight))
    }
    // Storage: Identity KeyRecords (r:3 w:0)
    // Storage: System Account (r:1 w:0)
    // Storage: Contracts ContractInfoOf (r:1 w:1)
    // Storage: Contracts CodeStorage (r:1 w:0)
    // Storage: Timestamp Now (r:1 w:0)
    // Storage: unknown [0x3a7472616e73616374696f6e5f6c6576656c3a] (r:1 w:1)
    // Storage: Identity IsDidFrozen (r:1 w:0)
    // Storage: Instance2Group ActiveMembers (r:1 w:0)
    // Storage: Instance2Group InactiveMembers (r:1 w:0)
    // Storage: Identity Claims (r:2 w:0)
    fn chain_extension_get_key_did(r: u32) -> Weight {
        (54_268_000 as Weight)
            // Standard Error: 31_131_000
            .saturating_add((495_788_000 as Weight).saturating_mul(r as Weight))
            .saturating_add(DbWeight::get().reads(13 as Weight))
            .saturating_add(DbWeight::get().writes(2 as Weight))
    }
    // Storage: Identity KeyRecords (r:2 w:0)
    // Storage: System Account (r:1 w:0)
    // Storage: Contracts ContractInfoOf (r:1 w:1)
    // Storage: Contracts CodeStorage (r:1 w:0)
    // Storage: Timestamp Now (r:1 w:0)
    // Storage: unknown [0x3a7472616e73616374696f6e5f6c6576656c3a] (r:1 w:1)
    // Storage: Identity IsDidFrozen (r:1 w:0)
    // Storage: Instance2Group ActiveMembers (r:1 w:0)
    // Storage: Instance2Group InactiveMembers (r:1 w:0)
    // Storage: Identity Claims (r:2 w:0)
    fn chain_extension_hash_twox_64(r: u32) -> Weight {
        (447_236_000 as Weight)
            // Standard Error: 6_822_000
            .saturating_add((199_156_000 as Weight).saturating_mul(r as Weight))
            .saturating_add(DbWeight::get().reads(12 as Weight))
            .saturating_add(DbWeight::get().writes(2 as Weight))
    }
    // Storage: Identity KeyRecords (r:2 w:0)
    // Storage: System Account (r:1 w:0)
    // Storage: Contracts ContractInfoOf (r:1 w:1)
    // Storage: Contracts CodeStorage (r:1 w:0)
    // Storage: Timestamp Now (r:1 w:0)
    // Storage: unknown [0x3a7472616e73616374696f6e5f6c6576656c3a] (r:1 w:1)
    // Storage: Identity IsDidFrozen (r:1 w:0)
    // Storage: Instance2Group ActiveMembers (r:1 w:0)
    // Storage: Instance2Group InactiveMembers (r:1 w:0)
    // Storage: Identity Claims (r:2 w:0)
    fn chain_extension_hash_twox_64_per_kb(n: u32) -> Weight {
        (802_237_000 as Weight)
            // Standard Error: 1_150_000
            .saturating_add((45_973_000 as Weight).saturating_mul(n as Weight))
            .saturating_add(DbWeight::get().reads(12 as Weight))
            .saturating_add(DbWeight::get().writes(2 as Weight))
    }
    // Storage: Identity KeyRecords (r:2 w:0)
    // Storage: System Account (r:1 w:0)
    // Storage: Contracts ContractInfoOf (r:1 w:1)
    // Storage: Contracts CodeStorage (r:1 w:0)
    // Storage: Timestamp Now (r:1 w:0)
    // Storage: unknown [0x3a7472616e73616374696f6e5f6c6576656c3a] (r:1 w:1)
    // Storage: Identity IsDidFrozen (r:1 w:0)
    // Storage: Instance2Group ActiveMembers (r:1 w:0)
    // Storage: Instance2Group InactiveMembers (r:1 w:0)
    // Storage: Identity Claims (r:2 w:0)
    fn chain_extension_hash_twox_128(r: u32) -> Weight {
        (487_844_000 as Weight)
            // Standard Error: 3_113_000
            .saturating_add((191_807_000 as Weight).saturating_mul(r as Weight))
            .saturating_add(DbWeight::get().reads(12 as Weight))
            .saturating_add(DbWeight::get().writes(2 as Weight))
    }
    // Storage: Identity KeyRecords (r:2 w:0)
    // Storage: System Account (r:1 w:0)
    // Storage: Contracts ContractInfoOf (r:1 w:1)
    // Storage: Contracts CodeStorage (r:1 w:0)
    // Storage: Timestamp Now (r:1 w:0)
    // Storage: unknown [0x3a7472616e73616374696f6e5f6c6576656c3a] (r:1 w:1)
    // Storage: Identity IsDidFrozen (r:1 w:0)
    // Storage: Instance2Group ActiveMembers (r:1 w:0)
    // Storage: Instance2Group InactiveMembers (r:1 w:0)
    // Storage: Identity Claims (r:2 w:0)
    fn chain_extension_hash_twox_128_per_kb(n: u32) -> Weight {
        (857_838_000 as Weight)
            // Standard Error: 1_631_000
            .saturating_add((56_482_000 as Weight).saturating_mul(n as Weight))
            .saturating_add(DbWeight::get().reads(12 as Weight))
            .saturating_add(DbWeight::get().writes(2 as Weight))
    }
    // Storage: Identity KeyRecords (r:2 w:0)
    // Storage: System Account (r:1 w:0)
    // Storage: Contracts ContractInfoOf (r:1 w:1)
    // Storage: Contracts CodeStorage (r:1 w:0)
    // Storage: Timestamp Now (r:1 w:0)
    // Storage: unknown [0x3a7472616e73616374696f6e5f6c6576656c3a] (r:1 w:1)
    // Storage: Identity IsDidFrozen (r:1 w:0)
    // Storage: Instance2Group ActiveMembers (r:1 w:0)
    // Storage: Instance2Group InactiveMembers (r:1 w:0)
    // Storage: Identity Claims (r:2 w:0)
    fn chain_extension_hash_twox_256(r: u32) -> Weight {
        (701_465_000 as Weight)
            // Standard Error: 6_087_000
            .saturating_add((192_876_000 as Weight).saturating_mul(r as Weight))
            .saturating_add(DbWeight::get().reads(12 as Weight))
            .saturating_add(DbWeight::get().writes(2 as Weight))
    }
    // Storage: Identity KeyRecords (r:2 w:0)
    // Storage: System Account (r:1 w:0)
    // Storage: Contracts ContractInfoOf (r:1 w:1)
    // Storage: Contracts CodeStorage (r:1 w:0)
    // Storage: Timestamp Now (r:1 w:0)
    // Storage: unknown [0x3a7472616e73616374696f6e5f6c6576656c3a] (r:1 w:1)
    // Storage: Identity IsDidFrozen (r:1 w:0)
    // Storage: Instance2Group ActiveMembers (r:1 w:0)
    // Storage: Instance2Group InactiveMembers (r:1 w:0)
    // Storage: Identity Claims (r:2 w:0)
    fn chain_extension_hash_twox_256_per_kb(n: u32) -> Weight {
        (704_307_000 as Weight)
            // Standard Error: 1_778_000
            .saturating_add((79_365_000 as Weight).saturating_mul(n as Weight))
            .saturating_add(DbWeight::get().reads(12 as Weight))
            .saturating_add(DbWeight::get().writes(2 as Weight))
    }
    // Storage: Identity KeyRecords (r:2 w:0)
    // Storage: System Account (r:1 w:0)
    // Storage: Contracts ContractInfoOf (r:1 w:1)
    // Storage: Contracts CodeStorage (r:1 w:0)
    // Storage: Timestamp Now (r:1 w:0)
    // Storage: unknown [0x3a7472616e73616374696f6e5f6c6576656c3a] (r:1 w:1)
    // Storage: Identity IsDidFrozen (r:1 w:0)
    // Storage: Instance2Group ActiveMembers (r:1 w:0)
    // Storage: Instance2Group InactiveMembers (r:1 w:0)
    // Storage: Identity Claims (r:2 w:0)
    // Storage: Contracts CallRuntimeWhitelist (r:1 w:0)
    // Storage: Identity CurrentPayer (r:1 w:1)
    // Storage: Identity CurrentDid (r:1 w:1)
    // Storage: Permissions CurrentPalletName (r:1 w:1)
    // Storage: Permissions CurrentDispatchableName (r:1 w:1)
    fn chain_extension_call_runtime(n: u32) -> Weight {
        (585_436_000 as Weight)
            // Standard Error: 3_000
            .saturating_add((4_000 as Weight).saturating_mul(n as Weight))
            .saturating_add(DbWeight::get().reads(17 as Weight))
            .saturating_add(DbWeight::get().writes(6 as Weight))
    }
    // Storage: Identity KeyRecords (r:2 w:0)
    // Storage: System Account (r:1 w:0)
    // Storage: Contracts ContractInfoOf (r:1 w:1)
    // Storage: Contracts CodeStorage (r:1 w:0)
    // Storage: Timestamp Now (r:1 w:0)
    // Storage: unknown [0x3a7472616e73616374696f6e5f6c6576656c3a] (r:1 w:1)
    // Storage: Identity IsDidFrozen (r:1 w:0)
    // Storage: Instance2Group ActiveMembers (r:1 w:0)
    // Storage: Instance2Group InactiveMembers (r:1 w:0)
    // Storage: Identity Claims (r:2 w:0)
    fn dummy_contract() -> Weight {
        (282_229_000 as Weight)
            .saturating_add(DbWeight::get().reads(12 as Weight))
            .saturating_add(DbWeight::get().writes(2 as Weight))
    }
    fn basic_runtime_call(_n: u32) -> Weight {
        (1_570_000 as Weight)
    }
    // Storage: Identity KeyRecords (r:2 w:1)
    // Storage: unknown [0x3a7472616e73616374696f6e5f6c6576656c3a] (r:1 w:1)
    // Storage: Contracts CodeStorage (r:1 w:1)
    // Storage: System Account (r:2 w:2)
    // Storage: Contracts Nonce (r:1 w:1)
    // Storage: Contracts ContractInfoOf (r:1 w:1)
    // Storage: Timestamp Now (r:1 w:0)
    // Storage: Identity IsDidFrozen (r:1 w:0)
    // Storage: Instance2Group ActiveMembers (r:1 w:0)
    // Storage: Instance2Group InactiveMembers (r:1 w:0)
    // Storage: Identity Claims (r:2 w:0)
    // Storage: Contracts OwnerInfoOf (r:1 w:1)
    // Storage: Identity DidKeys (r:0 w:1)
    fn instantiate_with_hash_perms(s: u32) -> Weight {
        (877_160_000 as Weight)
            // Standard Error: 0
            .saturating_add((5_000 as Weight).saturating_mul(s as Weight))
            .saturating_add(DbWeight::get().reads(15 as Weight))
            .saturating_add(DbWeight::get().writes(9 as Weight))
    }
    // Storage: Identity KeyRecords (r:2 w:1)
    // Storage: unknown [0x3a7472616e73616374696f6e5f6c6576656c3a] (r:1 w:1)
    // Storage: Contracts CodeStorage (r:1 w:1)
    // Storage: System Account (r:2 w:2)
    // Storage: Contracts Nonce (r:1 w:1)
    // Storage: Contracts ContractInfoOf (r:1 w:1)
    // Storage: Timestamp Now (r:1 w:0)
    // Storage: Identity IsDidFrozen (r:1 w:0)
    // Storage: Instance2Group ActiveMembers (r:1 w:0)
    // Storage: Instance2Group InactiveMembers (r:1 w:0)
    // Storage: Identity Claims (r:2 w:0)
    // Storage: Identity DidKeys (r:0 w:1)
    // Storage: Contracts PristineCode (r:0 w:1)
    // Storage: Contracts OwnerInfoOf (r:0 w:1)
    fn instantiate_with_code_perms(c: u32, s: u32) -> Weight {
        (359_632_000 as Weight)
            // Standard Error: 3_000
            .saturating_add((315_000 as Weight).saturating_mul(c as Weight))
            // Standard Error: 0
            .saturating_add((6_000 as Weight).saturating_mul(s as Weight))
            .saturating_add(DbWeight::get().reads(14 as Weight))
            .saturating_add(DbWeight::get().writes(10 as Weight))
    }
    // Storage: Contracts CallRuntimeWhitelist (r:0 w:20)
    fn update_call_runtime_whitelist(u: u32) -> Weight {
        (305_906_000 as Weight)
            // Standard Error: 76_000
            .saturating_add((1_756_000 as Weight).saturating_mul(u as Weight))
            .saturating_add(DbWeight::get().writes((1 as Weight).saturating_mul(u as Weight)))
    }
}
