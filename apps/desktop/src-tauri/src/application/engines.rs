use super::identity::managed_browser_package_root;
use super::DesktopCore;
use crate::domain::{discover_browsers as discover_installed_browsers, BrowserCandidate};
use crate::engine;
use crate::engine::{
    EngineAdapter, EngineAdapterId, EngineCapabilityId, EngineDescriptor, EngineHealth,
    EngineMaintenanceReceipt, EngineNegotiation, EnginePackageRequest,
    ExternalPackageEngineAdapter,
};
use chrono::Utc;
use serde::Serialize;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct EngineAdapterStatus {
    pub(crate) descriptor: EngineDescriptor,
    pub(crate) negotiation: EngineNegotiation,
    pub(crate) health: EngineHealth,
}

pub(crate) fn summarize_engine(adapter: &dyn EngineAdapter) -> EngineAdapterStatus {
    EngineAdapterStatus {
        descriptor: adapter.descriptor(),
        negotiation: adapter.negotiate(&EngineCapabilityId::ALL),
        health: adapter.health(),
    }
}

pub(crate) fn list_engine_adapters(
    state: &DesktopCore,
) -> Result<Vec<EngineAdapterStatus>, String> {
    let mut statuses = Vec::new();
    for id in [
        engine::EngineAdapterId::ControlledChromium,
        engine::EngineAdapterId::Camoufox,
    ] {
        let mut adapter = ExternalPackageEngineAdapter::production_prototype(id)
            .map_err(|error| error.to_string())?;
        if id == EngineAdapterId::Camoufox {
            if let Err(error) =
                adapter.ensure_builtin_package(&managed_browser_package_root(&state))
            {
                let mut status = summarize_engine(&adapter);
                status.health.state = engine::EngineHealthState::Unavailable;
                status.health.message =
                    format!("The bundled Camoufox RC1 package is unavailable: {error}");
                status.health.checked_at = Utc::now();
                statuses.push(status);
                continue;
            }
        }
        statuses.push(summarize_engine(&adapter));
    }
    Ok(statuses)
}

pub(crate) fn production_external_engine(
    adapter_id: EngineAdapterId,
) -> Result<ExternalPackageEngineAdapter, String> {
    ExternalPackageEngineAdapter::production_prototype(adapter_id)
        .map_err(|error| error.to_string())
}

pub(crate) fn install_engine_package(
    state: &DesktopCore,
    adapter_id: EngineAdapterId,
    request: EnginePackageRequest,
) -> Result<EngineMaintenanceReceipt, String> {
    if adapter_id == EngineAdapterId::Camoufox {
        let mut adapter = production_external_engine(adapter_id)?;
        return adapter
            .ensure_builtin_package(&managed_browser_package_root(&state))
            .map_err(|error| error.to_string());
    }
    production_external_engine(adapter_id)?
        .install(&request)
        .map_err(|error| error.to_string())
}

pub(crate) fn update_engine_package(
    state: &DesktopCore,
    adapter_id: EngineAdapterId,
    request: EnginePackageRequest,
) -> Result<EngineMaintenanceReceipt, String> {
    if adapter_id == EngineAdapterId::Camoufox {
        let mut adapter = production_external_engine(adapter_id)?;
        return adapter
            .ensure_builtin_package(&managed_browser_package_root(&state))
            .map_err(|error| error.to_string());
    }
    production_external_engine(adapter_id)?
        .update(&request)
        .map_err(|error| error.to_string())
}

pub(crate) fn rollback_engine_package(
    adapter_id: EngineAdapterId,
) -> Result<EngineMaintenanceReceipt, String> {
    production_external_engine(adapter_id)?
        .rollback()
        .map_err(|error| error.to_string())
}

pub(crate) fn set_engine_emergency_disabled(
    adapter_id: EngineAdapterId,
    disabled: bool,
    reason: Option<String>,
) -> Result<EngineAdapterStatus, String> {
    let mut adapter = production_external_engine(adapter_id)?;
    adapter
        .set_emergency_disabled(disabled, reason)
        .map_err(|error| error.to_string())?;
    Ok(summarize_engine(&adapter))
}

pub(crate) fn discover_browsers() -> Vec<BrowserCandidate> {
    discover_installed_browsers()
}
