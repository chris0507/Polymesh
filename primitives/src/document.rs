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

//! Document type
use crate::{self as polymesh_primitives, Moment};
use codec::{Decode, Encode};
use polymesh_primitives_derive::{Migrate, VecU8StrongTyped};
#[cfg(feature = "std")]
use serde::{Deserialize, Serialize};
use sp_std::prelude::Vec;

/// The local, per-ticker, ID of an asset documentation.
#[derive(Decode, Encode, Copy, Clone, Debug, Default, Hash, PartialEq, Eq, PartialOrd, Ord)]
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
pub struct DocumentId(pub u32);

/// A wrapper for a document name.
#[derive(
    Decode, Encode, Clone, Debug, Default, Hash, PartialEq, Eq, PartialOrd, Ord, VecU8StrongTyped,
)]
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
pub struct DocumentName(pub Vec<u8>);

/// A wrapper for a document URI.
#[derive(
    Decode, Encode, Clone, Debug, Default, Hash, PartialEq, Eq, PartialOrd, Ord, VecU8StrongTyped,
)]
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
pub struct DocumentUri(pub Vec<u8>);

/// A wrapper for a document hash.
#[derive(
    Decode, Encode, Clone, Debug, Default, Hash, PartialEq, Eq, PartialOrd, Ord, VecU8StrongTyped,
)]
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
pub struct DocumentHash(pub Vec<u8>);

/// A wrapper for a document's type.
#[derive(
    Decode, Encode, Clone, Debug, Default, Hash, PartialEq, Eq, PartialOrd, Ord, VecU8StrongTyped,
)]
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
pub struct DocumentType(pub Vec<u8>);

/// Represents a document associated with an asset
#[derive(Decode, Encode, Clone, Debug, Default, Hash, PartialEq, Eq, PartialOrd, Ord, Migrate)]
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
#[migrate_context(DocumentName)]
pub struct Document {
    /// Document URI
    pub uri: DocumentUri,
    /// Document hash
    pub content_hash: DocumentHash,
    /// The document's name.
    /// Need not be unique among a ticker's documents.
    #[migrate_from(())]
    #[migrate_with(context)]
    pub name: DocumentName,
    /// The document's type.
    /// This is free form text with no uniqueness requirements.
    #[migrate_from(())]
    #[migrate_with(None)]
    pub doc_type: Option<DocumentType>,
    /// When, legally speaking, the document was filed.
    /// Need not be when added to chain.
    #[migrate_from(())]
    #[migrate_with(None)]
    pub filing_date: Option<Moment>,
}
