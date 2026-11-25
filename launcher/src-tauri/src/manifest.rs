use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestFile {
    pub path: String,
    pub source: String, // "modrinth" | "github"
    pub url: String,
    pub sha256: String,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    pub name: String,
    pub ip: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MinecraftConfig {
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoaderConfig {
    #[serde(rename = "type")]
    pub loader_type: String,
    pub version: String,
    pub installer_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ElybyConfig {
    #[serde(rename = "clientId")]
    pub client_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    #[serde(rename = "schemaVersion")]
    pub schema_version: u32,
    pub server: ServerConfig,
    pub minecraft: MinecraftConfig,
    pub loader: LoaderConfig,
    pub elyby: ElybyConfig,
    pub files: Vec<ManifestFile>,
}
