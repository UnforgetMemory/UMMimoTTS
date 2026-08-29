//! UM-MimoTTS v4 HTTP layer: REST v3 + SSE + bearer auth + static UI.
//!
//! Contract source of truth: `packages/contract/openapi.yaml` (ADR-003).

pub mod auth;
pub mod routes;
pub mod state;

#[cfg(feature = "embed-ui")]
pub mod embedded {
    //! Compile-time embedded WebUI (apps/web/dist).
    use rust_embed::RustEmbed;

    #[derive(RustEmbed)]
    #[folder = "../../apps/web/dist/"]
    pub struct UiAssets;
}

pub use state::AppState;
