//! IR-layer identifier newtypes shared across the Tabula stack.
//!
//! These IDs are consumed by the wire-type contract (`tabula-contract`) as
//! well as by the IR itself; owning them in `tabula-core` keeps the
//! contract → core dependency direction canonical and prevents the
//! contract from reaching back into the IR crate for vocabulary.

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

/// Identifies a program registered in the Tabula registry.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    BorshSerialize,
    BorshDeserialize,
)]
pub struct ProgramId(pub u32);

/// Identifies a callable entry (function, query, or transaction) within a program.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    BorshSerialize,
    BorshDeserialize,
)]
pub struct EntryId(pub u32);

/// Identifies a field in the program's public context.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    BorshSerialize,
    BorshDeserialize,
)]
pub struct ContextFieldId(pub u32);

/// Identifies an event type.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    BorshSerialize,
    BorshDeserialize,
)]
pub struct EventId(pub u32);
