//! Declarative extension bundle parsing.

use anyhow::{Context as _, bail};
use tabula_profile::{TYPE_BOOL_ID, TYPE_BYTES32_ID, TYPE_I64_ID, TYPE_U64_ID};
use tabula_sdk::interop::{
    CapabilityProofVisibility, CapabilityQueryPolicy, CapabilityTotality, HashFamily,
    SourceCapabilityDescriptor, TypeRef,
};

use super::status::ExtensionBundleStatus;

#[derive(Debug, Clone)]
pub(crate) struct ParsedBundle {
    pub(crate) capabilities: Vec<SourceCapabilityDescriptor>,
    pub(crate) status: ExtensionBundleStatus,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct ExtensionBundleManifest {
    version: u32,
    name: String,
    #[serde(default)]
    capabilities: Vec<CapabilityManifestEntry>,
    #[serde(default)]
    types: Vec<toml::Table>,
    #[serde(default)]
    encodings: Vec<toml::Table>,
    #[serde(default)]
    schemes: Vec<toml::Table>,
    #[serde(default)]
    root_backends: Vec<toml::Table>,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct CapabilityManifestEntry {
    path: String,
    inputs: Vec<String>,
    outputs: Vec<String>,
    totality: ManifestCapabilityTotality,
    query_policy: ManifestCapabilityQueryPolicy,
    proof_visibility: ManifestCapabilityProofVisibility,
    hash_family: Option<ManifestHashFamily>,
}

#[derive(Debug, Clone, Copy, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
enum ManifestCapabilityTotality {
    Total,
    Checked,
}

#[derive(Debug, Clone, Copy, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
enum ManifestCapabilityQueryPolicy {
    QuerySafe,
    TxOnly,
}

#[derive(Debug, Clone, Copy, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
enum ManifestCapabilityProofVisibility {
    OpaqueRuntimeOnly,
    Journaled,
}

#[derive(Debug, Clone, Copy, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
enum ManifestHashFamily {
    Poseidon,
}

pub(crate) fn load_bundle(path: &std::path::Path) -> anyhow::Result<ParsedBundle> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read extension bundle {}", path.display()))?;
    let manifest: ExtensionBundleManifest = toml::from_str(&content)
        .with_context(|| format!("failed to parse extension bundle {}", path.display()))?;
    if manifest.version != 1 {
        bail!(
            "extension bundle {} uses unsupported version {}",
            path.display(),
            manifest.version
        );
    }

    let mut unsupported_entries = Vec::new();
    if !manifest.types.is_empty() {
        unsupported_entries.push("types".to_string());
    }
    if !manifest.encodings.is_empty() {
        unsupported_entries.push("encodings".to_string());
    }
    if !manifest.schemes.is_empty() {
        unsupported_entries.push("schemes".to_string());
    }
    if !manifest.root_backends.is_empty() {
        unsupported_entries.push("root_backends".to_string());
    }

    let mut capabilities = Vec::with_capacity(manifest.capabilities.len());
    let mut capability_paths = Vec::with_capacity(manifest.capabilities.len());
    for capability in manifest.capabilities {
        capability_paths.push(capability.path.clone());
        capabilities.push(SourceCapabilityDescriptor {
            path: capability.path,
            inputs: capability
                .inputs
                .iter()
                .map(|ty| resolve_builtin_type(ty))
                .collect::<anyhow::Result<Vec<_>>>()?,
            outputs: capability
                .outputs
                .iter()
                .map(|ty| resolve_builtin_type(ty))
                .collect::<anyhow::Result<Vec<_>>>()?,
            totality: match capability.totality {
                ManifestCapabilityTotality::Total => CapabilityTotality::Total,
                ManifestCapabilityTotality::Checked => CapabilityTotality::Checked,
            },
            query_policy: match capability.query_policy {
                ManifestCapabilityQueryPolicy::QuerySafe => CapabilityQueryPolicy::QuerySafe,
                ManifestCapabilityQueryPolicy::TxOnly => CapabilityQueryPolicy::TxOnly,
            },
            proof_visibility: match capability.proof_visibility {
                ManifestCapabilityProofVisibility::OpaqueRuntimeOnly => {
                    CapabilityProofVisibility::OpaqueRuntimeOnly
                }
                ManifestCapabilityProofVisibility::Journaled => {
                    CapabilityProofVisibility::Journaled
                }
            },
            hash_family: capability.hash_family.map(|family| match family {
                ManifestHashFamily::Poseidon => HashFamily::Poseidon,
            }),
        });
    }

    Ok(ParsedBundle {
        capabilities,
        status: ExtensionBundleStatus {
            path: path.display().to_string(),
            name: manifest.name,
            capability_paths,
            unsupported_entries,
        },
    })
}

fn resolve_builtin_type(name: &str) -> anyhow::Result<TypeRef> {
    match name {
        "u64" => Ok(TYPE_U64_ID),
        "i64" => Ok(TYPE_I64_ID),
        "bool" => Ok(TYPE_BOOL_ID),
        "bytes32" => Ok(TYPE_BYTES32_ID),
        other => bail!(
            "unsupported declarative type `{other}`; only built-in scalar capability types are supported in this beta"
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::load_bundle;

    #[test]
    fn unsupported_sections_are_reported() {
        let dir = temp_path("bundle");
        let path = dir.join("bundle.toml");
        std::fs::write(
            &path,
            r#"
version = 1
name = "demo"

[[capabilities]]
path = "demo_hash"
inputs = ["u64"]
outputs = ["bytes32"]
totality = "total"
query_policy = "query_safe"
proof_visibility = "opaque_runtime_only"
hash_family = "poseidon"

[[types]]
name = "custom"
"#,
        )
        .unwrap();

        let bundle = load_bundle(&path).unwrap();
        assert_eq!(bundle.status.unsupported_entries, vec!["types"]);
        assert_eq!(bundle.status.capability_paths, vec!["demo_hash"]);
    }

    fn temp_path(label: &str) -> std::path::PathBuf {
        let base = std::env::temp_dir().join(format!(
            "tabula-cli-env-tests-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        if base.exists() {
            std::fs::remove_dir_all(&base).unwrap();
        }
        std::fs::create_dir_all(&base).unwrap();
        base
    }
}
