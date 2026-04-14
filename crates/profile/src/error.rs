use tabula_core::{ColumnProfileId, EncodingProfileId, SchemeId, SchemeProfileId, TypeId};

/// Errors raised by canonical profile construction and validation.
#[derive(Debug, thiserror::Error)]
pub enum ProfileError {
    /// Failed to serialize a semantic payload for canonical hashing.
    #[error("failed to serialize canonical profile payload: {0}")]
    EncodeJson(#[source] serde_json::Error),

    /// A registered descriptor id was duplicated.
    #[error("duplicate {kind} id {raw}")]
    DuplicateId {
        /// Descriptor family name.
        kind: &'static str,
        /// Raw numeric identifier.
        raw: u32,
    },

    /// A descriptor vector is not stored in deterministic id order.
    #[error("{kind} catalog entries must be sorted by ascending id")]
    UnsortedCatalog {
        /// Descriptor family name.
        kind: &'static str,
    },

    /// A descriptor references another descriptor that is not present.
    #[error("missing referenced {kind} id {raw}")]
    MissingReference {
        /// Referenced descriptor family name.
        kind: &'static str,
        /// Raw numeric identifier.
        raw: u32,
    },

    /// A stored semantic hash does not match the canonical recomputation.
    #[error("{kind} id {raw} has non-canonical semantic hash")]
    HashMismatch {
        /// Descriptor family name.
        kind: &'static str,
        /// Raw numeric identifier.
        raw: u32,
    },

    /// A source-level semantic name was registered more than once.
    #[error("duplicate named {kind} semantic '{name}'")]
    DuplicateNamedSemantic {
        /// Semantic family name.
        kind: &'static str,
        /// Duplicate source-level name.
        name: String,
    },

    /// A default resolution rule was registered more than once.
    #[error("duplicate {kind} resolution rule")]
    DuplicateDefaultResolution {
        /// Resolution family name.
        kind: &'static str,
    },

    /// A source-level semantic name could not be resolved.
    #[error("unknown named {kind} semantic '{name}'")]
    UnknownNamedSemantic {
        /// Semantic family name.
        kind: &'static str,
        /// Unknown source-level name.
        name: String,
    },

    /// A type is missing its default encoding.
    #[error("type {0} has no registered default encoding profile")]
    MissingDefaultEncoding(TypeId),

    /// A type is missing its default key encoding.
    #[error("type {0} has no registered default key encoding profile")]
    MissingDefaultKeyEncoding(TypeId),

    /// A `(scheme family, encoding)` pair is missing its default scheme profile.
    #[error(
        "scheme family {scheme_id} with encoding profile {encoding_profile_id} has no registered default scheme profile"
    )]
    MissingDefaultSchemeProfile {
        /// Scheme family identifier.
        scheme_id: SchemeId,
        /// Encoding profile identifier.
        encoding_profile_id: EncodingProfileId,
    },

    /// A column profile references an encoding that belongs to another type.
    #[error(
        "column profile {column_profile_id} type {type_id} does not match encoding {encoding_profile_id} type"
    )]
    EncodingTypeMismatch {
        /// Column profile identifier.
        column_profile_id: ColumnProfileId,
        /// Column type identifier.
        type_id: TypeId,
        /// Encoding profile identifier.
        encoding_profile_id: EncodingProfileId,
    },

    /// A scheme profile rejects the chosen encoding profile.
    #[error(
        "column profile {column_profile_id} encoding {encoding_profile_id} is incompatible with scheme profile {scheme_profile_id}: {reason}"
    )]
    SchemeEncodingIncompatibility {
        /// Column profile identifier.
        column_profile_id: ColumnProfileId,
        /// Encoding profile identifier.
        encoding_profile_id: EncodingProfileId,
        /// Scheme profile identifier.
        scheme_profile_id: SchemeProfileId,
        /// Human-readable incompatibility reason.
        reason: String,
    },

    /// A requested column profile does not exist in the catalog.
    #[error("unknown column profile id {0}")]
    UnknownColumnProfile(ColumnProfileId),
}
