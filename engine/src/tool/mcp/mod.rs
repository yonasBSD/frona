pub mod client;
pub mod manager;
pub mod metadata;
pub mod models;
pub mod registry;
pub mod service;
pub mod repository;

pub use client::{McpClient, McpClientHandler, default_client_info};
pub use manager::{McpConnection, McpManager};
pub use models::{
    CachedMcpTool, McpPackage, McpRuntime, McpServer, McpServerInstall, McpServerStatus,
    McpServerInfo, sanitize_slug,
};
pub use metadata::{
    Enrichment, PrebuiltMetadata, RegistryEnvVar, RegistryPackage, RegistryServerEntry,
    RegistryStatus,
};
pub use registry::{
    McpRegistryClient, PREBUILT_METADATA_URL, PREBUILT_SERVERS_URL, PrebuiltMcpRegistryClient,
};
pub use service::{McpServerService, NoopPackageInstaller, PackageInstaller, StartResult, UpdateResult};
pub use repository::McpServerRepository;
