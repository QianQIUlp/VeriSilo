use super::DesktopCore;
use crate::domain::{SiloExecutionTarget, VaultLockState};
use crate::vault::RemoteVaultState;
use chrono::Utc;
use serde::Serialize;
use uuid::Uuid;
use verisilo_remote_backend::agent::{
    AutomationScope as RemoteAutomationScope, InputEvent as RemoteInputEvent,
};
use verisilo_remote_backend::transport::PinnedHttpsTransport;
use verisilo_remote_backend::{
    AgentInteractionReceipt as RemoteInteractionReceipt,
    CapabilityAvailability as RemoteCapabilityAvailability, InteractivePrincipal,
    MemoryBindingStore, OperationResult as RemoteOperationResult, PairingApproval,
    RemoteCapability, RemoteEndpoint, RemoteEnvironmentBackend, RemoteNetworkPolicy,
    RemoteOperation, RemoteOrphanReceipt, SiloBinding as RemoteSiloBinding, SystemClock,
    PROTOCOL_VERSION as REMOTE_PROTOCOL_VERSION,
};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RemoteEnvironmentStatus {
    pub(crate) protocol_version: u16,
    pub(crate) state: &'static str,
    pub(crate) transport_available: bool,
    pub(crate) durable_binding_store_available: bool,
    pub(crate) self_hosted_agent_available: bool,
    pub(crate) capabilities: Vec<RemoteCapability>,
    pub(crate) message: String,
    pub(crate) endpoint: Option<RemoteEndpoint>,
    pub(crate) pairing: Option<RemotePairingStatus>,
    pub(crate) bindings: Vec<RemoteSiloBinding>,
    pub(crate) last_results: Vec<RemoteOperationResult>,
    pub(crate) pairing_revoked_at: Option<chrono::DateTime<Utc>>,
    pub(crate) orphan_receipts: Vec<RemoteOrphanReceipt>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RemotePairingStatus {
    pub(crate) server_id: Uuid,
    pub(crate) client_credential_id: Uuid,
    pub(crate) node: verisilo_remote_backend::agent::NodeDisclosure,
    pub(crate) credential_expires_at_unix_ms: u64,
    pub(crate) expired: bool,
}

pub(crate) fn unavailable_remote_capabilities(reason: &str) -> Vec<RemoteCapability> {
    RemoteOperation::ALL
        .into_iter()
        .map(|operation| RemoteCapability {
            operation,
            availability: RemoteCapabilityAvailability::Unavailable {
                reason: reason.to_owned(),
            },
        })
        .collect()
}

pub(crate) fn remote_environment_status_from(
    vault_state: VaultLockState,
    remote: Option<&RemoteVaultState>,
) -> RemoteEnvironmentStatus {
    let now_ms = u64::try_from(Utc::now().timestamp_millis()).unwrap_or(0);
    let Some(remote) = remote else {
        let (state, reason, message) = match vault_state {
            VaultLockState::Unlocked => (
                "not_configured",
                "No self-hosted endpoint is paired.",
                "Enter a user-operated HTTPS origin, verify its pin and explicitly approve pairing. No default endpoint exists.",
            ),
            VaultLockState::Locked => (
                "vault_locked",
                "Unlock the Vault to use encrypted remote credentials.",
                "Remote credentials and bindings are unavailable while the Vault is locked; lifecycle requests are refused.",
            ),
            VaultLockState::Uninitialized => (
                "vault_uninitialized",
                "Initialize and unlock the Vault before pairing.",
                "Remote endpoint and pairing material are stored only inside an initialized encrypted Vault.",
            ),
        };
        return RemoteEnvironmentStatus {
            protocol_version: REMOTE_PROTOCOL_VERSION,
            state,
            transport_available: true,
            durable_binding_store_available: true,
            self_hosted_agent_available: false,
            capabilities: unavailable_remote_capabilities(reason),
            message: message.to_owned(),
            endpoint: None,
            pairing: None,
            bindings: Vec::new(),
            last_results: Vec::new(),
            pairing_revoked_at: None,
            orphan_receipts: Vec::new(),
        };
    };

    let pairing = remote.backend.pairing.as_ref();
    let expired = pairing.is_some_and(|pairing| pairing.credential_expires_at_unix_ms <= now_ms);
    let (state, capabilities, message) = match pairing {
        Some(pairing) if !expired => (
            "paired",
            pairing.capabilities.clone(),
            "A pinned self-hosted endpoint and unexpired application credential are stored. Availability still depends on the user-operated Agent at request time.".to_owned(),
        ),
        Some(_) => (
            "credential_expired",
            unavailable_remote_capabilities(
                "The stored remote credential expired; revoke and explicitly pair again.",
            ),
            "The endpoint and stable bindings remain encrypted, but lifecycle requests are refused because the pairing credential expired.".to_owned(),
        ),
        None if remote.pairing_revoked_at.is_some() => (
            "revoked",
            unavailable_remote_capabilities(
                "The local pairing credential was explicitly revoked.",
            ),
            "The local credential was erased. Existing binding metadata is retained to prevent accidental recreation or endpoint substitution.".to_owned(),
        ),
        None => (
            "not_paired",
            unavailable_remote_capabilities("The configured endpoint is not paired."),
            "The endpoint was entered during an explicit pairing attempt, but no usable credential is stored.".to_owned(),
        ),
    };
    let mut bindings = remote.backend.bindings.clone();
    bindings.sort_by_key(|binding| binding.silo_id);
    let mut last_results = remote.last_results.values().cloned().collect::<Vec<_>>();
    last_results.sort_by_key(|result| result.silo_id);
    let mut orphan_receipts = remote.orphan_receipts.clone();
    orphan_receipts.sort_by_key(|receipt| std::cmp::Reverse(receipt.detached_at_unix_ms));
    RemoteEnvironmentStatus {
        protocol_version: REMOTE_PROTOCOL_VERSION,
        state,
        transport_available: true,
        durable_binding_store_available: true,
        self_hosted_agent_available: pairing.is_some() && !expired,
        capabilities,
        message,
        endpoint: remote.endpoint.clone(),
        pairing: pairing.map(|pairing| RemotePairingStatus {
            server_id: pairing.server_id,
            client_credential_id: pairing.client_credential_id,
            node: pairing.node.clone(),
            credential_expires_at_unix_ms: pairing.credential_expires_at_unix_ms,
            expired,
        }),
        bindings,
        last_results,
        pairing_revoked_at: remote.pairing_revoked_at,
        orphan_receipts,
    }
}

pub(crate) fn remote_environment_status(
    state: &DesktopCore,
) -> Result<RemoteEnvironmentStatus, String> {
    let mut vault = state
        .vault
        .lock()
        .map_err(|_| "VeriSilo vault state is unavailable.".to_owned())?;
    let vault_status = vault.status(&state.root);
    let remote = if matches!(vault_status.state, VaultLockState::Unlocked) {
        Some(
            vault
                .remote_control_plane()
                .map_err(|error| error.to_string())?,
        )
    } else {
        None
    };
    Ok(remote_environment_status_from(
        vault_status.state,
        remote.as_ref(),
    ))
}

pub(crate) fn validate_remote_environment_endpoint(
    endpoint: RemoteEndpoint,
) -> Result<RemoteEndpoint, String> {
    endpoint.validate().map_err(|error| error.to_string())?;
    Ok(endpoint)
}

pub(crate) type ProductionRemoteBackend =
    RemoteEnvironmentBackend<PinnedHttpsTransport, MemoryBindingStore, SystemClock>;

pub(crate) fn production_remote_backend(
    remote: &RemoteVaultState,
) -> Result<ProductionRemoteBackend, String> {
    let endpoint = remote
        .endpoint
        .clone()
        .ok_or_else(|| "No self-hosted remote endpoint is configured.".to_owned())?;
    let transport = PinnedHttpsTransport::new().map_err(|error| error.to_string())?;
    RemoteEnvironmentBackend::from_snapshot(
        endpoint,
        transport,
        SystemClock,
        remote.backend.clone(),
    )
    .map_err(|error| error.to_string())
}

pub(crate) fn pair_remote_environment(
    state: &DesktopCore,
    endpoint: RemoteEndpoint,
    approval: PairingApproval,
) -> Result<RemoteEnvironmentStatus, String> {
    // Pairing is the first production network action. It is reached only from
    // this explicit command, never from startup, status, endpoint validation or
    // Silo selection.
    let _remote_guard = state
        .remote_control
        .lock()
        .map_err(|_| "VeriSilo remote control state is unavailable.".to_owned())?;
    endpoint.validate().map_err(|error| error.to_string())?;
    let mut vault = state
        .vault
        .lock()
        .map_err(|_| "VeriSilo vault state is unavailable.".to_owned())?;
    vault.record_activity().map_err(|error| error.to_string())?;
    let mut remote = vault
        .remote_control_plane()
        .map_err(|error| error.to_string())?;
    if remote.backend.pairing.is_some() {
        return Err(
            "A remote credential is already stored. Explicitly revoke it before pairing again."
                .to_owned(),
        );
    }
    if remote
        .backend
        .bindings
        .iter()
        .any(|binding| binding.endpoint != endpoint)
    {
        return Err(
            "Existing Silo bindings belong to a different endpoint. Re-pair that exact endpoint; endpoint substitution is refused."
                .to_owned(),
        );
    }
    remote.endpoint = Some(endpoint);
    let mut backend = production_remote_backend(&remote)?;
    let pairing_result = backend.pair(approval);
    remote.backend = backend.export_snapshot();
    if pairing_result.is_ok() {
        remote.pairing_revoked_at = None;
    }
    // A token is single-use locally even when TLS, pinning or the Agent rejects
    // the attempt. Commit that replay ledger before surfacing the network error.
    vault
        .persist_remote_control_plane(&state.root, remote.clone())
        .map_err(|error| error.to_string())?;
    pairing_result.map_err(|error| error.to_string())?;
    Ok(remote_environment_status_from(
        VaultLockState::Unlocked,
        Some(&remote),
    ))
}

pub(crate) fn rotate_remote_environment_tls_pin(
    state: &DesktopCore,
    endpoint: RemoteEndpoint,
    approval: PairingApproval,
    confirm_rotation: bool,
) -> Result<RemoteEnvironmentStatus, String> {
    if !confirm_rotation {
        return Err("TLS pin rotation requires explicit user confirmation.".to_owned());
    }
    let _remote_guard = state
        .remote_control
        .lock()
        .map_err(|_| "VeriSilo remote control state is unavailable.".to_owned())?;
    let mut vault = state
        .vault
        .lock()
        .map_err(|_| "VeriSilo vault state is unavailable.".to_owned())?;
    vault.record_activity().map_err(|error| error.to_string())?;
    let original = vault
        .remote_control_plane()
        .map_err(|error| error.to_string())?;
    if original.backend.pairing.is_none() {
        return Err(
            "TLS pin rotation requires an existing unrevoked pairing credential.".to_owned(),
        );
    }

    // Phase 1 is intentionally a replay-ledger-only commit. The old endpoint,
    // pairing and every binding remain current while the candidate pin is
    // contacted. This also makes the token single-use after transport errors,
    // wrong-server responses, rejected candidates, or a failed final commit.
    let mut reservation_backend = production_remote_backend(&original)?;
    reservation_backend
        .reserve_pairing_token(&approval)
        .map_err(|error| error.to_string())?;
    let mut ledger_state = original.clone();
    ledger_state.backend.used_pairing_token_ids = reservation_backend.used_pairing_token_ids();
    vault
        .persist_remote_control_plane(&state.root, ledger_state.clone())
        .map_err(|error| error.to_string())?;

    // Build the network attempt from the pre-reservation snapshot so the
    // backend itself consumes exactly the same ID while producing a complete
    // candidate snapshot. Nothing from this candidate is observable until the
    // single final Vault replacement succeeds.
    let mut candidate_backend = production_remote_backend(&original)?;
    let rotation_claim = match candidate_backend.begin_tls_pin_rotation(&endpoint, &approval) {
        Ok(rotation_claim) => rotation_claim,
        Err(error) => {
            // The endpoint, credential identity and bindings are still the
            // old ones, but an authenticated old-pin request may have
            // consumed client and server sequences. Persist those monotonic
            // counters alongside the already-reserved token so the old
            // credential remains usable.
            let mut failed = ledger_state;
            failed.backend = candidate_backend.export_snapshot();
            vault
                .persist_remote_control_plane(&state.root, failed)
                .map_err(|persist_error| persist_error.to_string())?;
            return Err(error.to_string());
        }
    };

    // Durably checkpoint the consumed old-credential sequences before the new
    // pin is contacted. If the new pairing or final commit fails, this remains
    // a complete old-endpoint state with the burned token and usable counters.
    let mut authorized = ledger_state;
    authorized.backend = candidate_backend.export_snapshot();
    vault
        .persist_remote_control_plane(&state.root, authorized.clone())
        .map_err(|error| error.to_string())?;

    if let Err(error) =
        candidate_backend.finish_tls_pin_rotation(endpoint.clone(), &approval, rotation_claim)
    {
        return Err(error.to_string());
    }
    let mut rotated = authorized;
    rotated.endpoint = Some(endpoint);
    rotated.backend = candidate_backend.export_snapshot();
    rotated.pairing_revoked_at = None;
    vault
        .persist_remote_control_plane(&state.root, rotated.clone())
        .map_err(|error| error.to_string())?;

    Ok(remote_environment_status_from(
        VaultLockState::Unlocked,
        Some(&rotated),
    ))
}

pub(crate) fn revoke_remote_pairing(
    state: &DesktopCore,
    confirm_revoke: bool,
) -> Result<RemoteEnvironmentStatus, String> {
    if !confirm_revoke {
        return Err("Remote credential revocation requires explicit confirmation.".to_owned());
    }
    let _remote_guard = state
        .remote_control
        .lock()
        .map_err(|_| "VeriSilo remote control state is unavailable.".to_owned())?;
    let mut vault = state
        .vault
        .lock()
        .map_err(|_| "VeriSilo vault state is unavailable.".to_owned())?;
    vault.record_activity().map_err(|error| error.to_string())?;
    let mut remote = vault
        .remote_control_plane()
        .map_err(|error| error.to_string())?;
    if remote.backend.pairing.take().is_none() {
        return Err("No remote pairing credential is stored.".to_owned());
    }
    // Authorization IDs are capabilities too. Erase every locally usable
    // interactive grant/channel alongside the bearer credential so a later
    // re-pair cannot accidentally reuse an authorization issued to the old
    // application credential. Stable environment bindings and audit receipts
    // remain for recovery and deletion.
    for binding in &mut remote.backend.bindings {
        binding.human_session = None;
        binding.automation_authorizations.clear();
        binding.last_screen_channel = None;
    }
    remote.pairing_revoked_at = Some(Utc::now());
    vault
        .persist_remote_control_plane(&state.root, remote.clone())
        .map_err(|error| error.to_string())?;
    Ok(remote_environment_status_from(
        VaultLockState::Unlocked,
        Some(&remote),
    ))
}

pub(crate) fn force_detach_remote_environment(
    state: &DesktopCore,
    silo_id: Uuid,
    confirm_local_detach: bool,
    acknowledge_remote_orphan_risk: bool,
) -> Result<RemoteEnvironmentStatus, String> {
    if !confirm_local_detach || !acknowledge_remote_orphan_risk {
        return Err(
            "Force detach requires both local removal and continuing remote cost confirmations."
                .to_owned(),
        );
    }
    let _remote_guard = state
        .remote_control
        .lock()
        .map_err(|_| "VeriSilo remote control state is unavailable.".to_owned())?;
    let mut vault = state
        .vault
        .lock()
        .map_err(|_| "VeriSilo vault state is unavailable.".to_owned())?;
    vault.record_activity().map_err(|error| error.to_string())?;
    vault.get_silo(silo_id).map_err(|error| error.to_string())?;
    let mut remote = vault
        .remote_control_plane()
        .map_err(|error| error.to_string())?;
    let mut backend = production_remote_backend(&remote)?;
    let receipt = backend
        .force_detach_binding(
            silo_id,
            confirm_local_detach,
            acknowledge_remote_orphan_risk,
        )
        .map_err(|error| error.to_string())?;
    remote.backend = backend.export_snapshot();
    remote.last_results.remove(&silo_id);
    remote.orphan_receipts.push(receipt);
    vault
        .persist_remote_control_plane(&state.root, remote.clone())
        .map_err(|error| error.to_string())?;

    Ok(remote_environment_status_from(
        VaultLockState::Unlocked,
        Some(&remote),
    ))
}

pub(crate) fn execute_remote_operation(
    state: &DesktopCore,
    silo_id: Uuid,
    operation: impl FnOnce(
        &mut ProductionRemoteBackend,
    )
        -> Result<RemoteOperationResult, verisilo_remote_backend::RemoteBackendError>,
) -> Result<RemoteOperationResult, String> {
    let _remote_guard = state
        .remote_control
        .lock()
        .map_err(|_| "VeriSilo remote control state is unavailable.".to_owned())?;
    let mut vault = state
        .vault
        .lock()
        .map_err(|_| "VeriSilo vault state is unavailable.".to_owned())?;
    vault.record_activity().map_err(|error| error.to_string())?;
    // This both validates the Silo identity and refuses every request while the
    // Vault is locked. The Vault guard remains held through the bounded HTTPS
    // exchange, so a concurrent lock linearizes strictly after this request.
    vault.get_silo(silo_id).map_err(|error| error.to_string())?;
    let mut remote = vault
        .remote_control_plane()
        .map_err(|error| error.to_string())?;
    let mut backend = production_remote_backend(&remote)?;
    let result = operation(&mut backend);
    remote.backend = backend.export_snapshot();
    if let Ok(result) = &result {
        remote.last_results.insert(silo_id, result.clone());
    }
    // Persist advancing response sequences and partial binding/evidence state
    // even when a required-proxy check intentionally fails closed.
    vault
        .persist_remote_control_plane(&state.root, remote)
        .map_err(|error| error.to_string())?;
    result.map_err(|error| error.to_string())
}

pub(crate) fn reject_unavailable_remote_runtime<T>(
    state: &DesktopCore,
    silo_id: Uuid,
) -> Result<T, String> {
    let mut vault = state
        .vault
        .lock()
        .map_err(|_| "VeriSilo vault state is unavailable.".to_owned())?;
    vault.record_activity().map_err(|error| error.to_string())?;
    let silo = vault.get_silo(silo_id).map_err(|error| error.to_string())?;
    let location = match silo.execution_target {
        SiloExecutionTarget::Local => "this Windows computer".to_owned(),
        SiloExecutionTarget::Wsl { distribution } => {
            format!("the Linux environment {distribution}")
        }
        SiloExecutionTarget::Remote { endpoint_origin } => endpoint_origin,
    };
    Err(format!(
        "This Silo is bound to {location}. Remote activation and interaction are unavailable until a remote-bound browser identity, global lifecycle lock, and durable identity journal are verified end to end."
    ))
}

pub(crate) fn remote_environment_create(
    state: &DesktopCore,
    silo_id: Uuid,
    network: RemoteNetworkPolicy,
    ttl_seconds: u64,
    cost_acknowledged: bool,
) -> Result<RemoteOperationResult, String> {
    let _ = (network, ttl_seconds, cost_acknowledged);
    reject_unavailable_remote_runtime(&state, silo_id)
}

pub(crate) fn remote_environment_start(
    state: &DesktopCore,
    silo_id: Uuid,
) -> Result<RemoteOperationResult, String> {
    reject_unavailable_remote_runtime(&state, silo_id)
}

pub(crate) fn remote_environment_stop(
    state: &DesktopCore,
    silo_id: Uuid,
) -> Result<RemoteOperationResult, String> {
    execute_remote_operation(&state, silo_id, |backend| backend.stop(silo_id))
}

pub(crate) fn remote_environment_pause(
    state: &DesktopCore,
    silo_id: Uuid,
) -> Result<RemoteOperationResult, String> {
    reject_unavailable_remote_runtime(&state, silo_id)
}

pub(crate) fn remote_environment_snapshot(
    state: &DesktopCore,
    silo_id: Uuid,
) -> Result<RemoteOperationResult, String> {
    reject_unavailable_remote_runtime(&state, silo_id)
}

pub(crate) fn remote_environment_destroy(
    state: &DesktopCore,
    silo_id: Uuid,
    confirm_destroy: bool,
) -> Result<RemoteOperationResult, String> {
    execute_remote_operation(&state, silo_id, |backend| {
        backend.destroy(silo_id, confirm_destroy)
    })
}

pub(crate) fn remote_environment_configure_network(
    state: &DesktopCore,
    silo_id: Uuid,
    network: RemoteNetworkPolicy,
) -> Result<RemoteOperationResult, String> {
    let _ = network;
    reject_unavailable_remote_runtime(&state, silo_id)
}

pub(crate) fn remote_environment_health(
    state: &DesktopCore,
    silo_id: Uuid,
) -> Result<RemoteOperationResult, String> {
    execute_remote_operation(&state, silo_id, |backend| backend.health(silo_id))
}

pub(crate) fn remote_environment_logs(
    state: &DesktopCore,
    silo_id: Uuid,
    cursor: Option<Uuid>,
    limit: u16,
) -> Result<RemoteOperationResult, String> {
    execute_remote_operation(&state, silo_id, |backend| {
        backend.logs(silo_id, cursor, limit)
    })
}

pub(crate) fn remote_environment_open_human_session(
    state: &DesktopCore,
    silo_id: Uuid,
    lifetime_seconds: u64,
) -> Result<RemoteInteractionReceipt, String> {
    let _ = lifetime_seconds;
    reject_unavailable_remote_runtime(&state, silo_id)
}

pub(crate) fn remote_environment_close_human_session(
    state: &DesktopCore,
    silo_id: Uuid,
) -> Result<RemoteInteractionReceipt, String> {
    reject_unavailable_remote_runtime(&state, silo_id)
}

pub(crate) fn remote_environment_grant_automation(
    state: &DesktopCore,
    silo_id: Uuid,
    lifetime_seconds: u64,
    scopes: Vec<RemoteAutomationScope>,
    approved_by_user: bool,
) -> Result<RemoteInteractionReceipt, String> {
    let _ = (lifetime_seconds, scopes, approved_by_user);
    reject_unavailable_remote_runtime(&state, silo_id)
}

pub(crate) fn remote_environment_revoke_automation(
    state: &DesktopCore,
    silo_id: Uuid,
    authorization_id: Uuid,
) -> Result<RemoteInteractionReceipt, String> {
    let _ = authorization_id;
    reject_unavailable_remote_runtime(&state, silo_id)
}

pub(crate) fn remote_environment_open_screen(
    state: &DesktopCore,
    silo_id: Uuid,
    principal: InteractivePrincipal,
) -> Result<RemoteInteractionReceipt, String> {
    let _ = principal;
    reject_unavailable_remote_runtime(&state, silo_id)
}

pub(crate) fn remote_environment_send_input(
    state: &DesktopCore,
    silo_id: Uuid,
    principal: InteractivePrincipal,
    events: Vec<RemoteInputEvent>,
) -> Result<RemoteInteractionReceipt, String> {
    let _ = (principal, events);
    reject_unavailable_remote_runtime(&state, silo_id)
}
