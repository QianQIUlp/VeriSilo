//! Strict JSON router for the user-operated Remote Agent.
//!
//! The router has four deliberately disjoint wire shapes: unauthenticated
//! one-time pairing, authenticated TLS-pin-rotation authorization, the nine
//! fixed lifecycle operations, and the six typed interactive `AgentRequest`
//! commands. A bearer credential is never accepted from JSON, and remote input
//! cannot select a process, filesystem path, shell command, provider executable,
//! or URL.

use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;
use zeroize::Zeroize;

use verisilo_remote_backend::{
    agent::{
        deletion_resources_are_bound, AgentCommand, AgentCore, AgentError, AgentProvider,
        AgentRequest, AgentResponse, AgentStore, EnvironmentRecord, EnvironmentState, KeyCustody,
        NodeDisclosure, PrincipalKind,
    },
    AgentControlResponseBody, AgentResponseEnvelope, CapabilityAvailability, Clock, OperationBody,
    OperationRequestEnvelope, OperationResponseBody, OperationResponseEnvelope, OperationResult,
    PairingRejectionCode, PairingRequestEnvelope, PairingResponseBody, PairingResponseEnvelope,
    RejectionCode, RemoteCapability, RemoteLogEntry, RemoteLogLevel, RemoteOperation,
    RemoteResultState, TlsPinRotationAuthorizationRequestEnvelope,
    TlsPinRotationAuthorizationResponseBody, TlsPinRotationAuthorizationResponseEnvelope,
    MAX_CLOCK_SKEW_MS, MAX_LOG_ENTRIES, MAX_MESSAGE_BYTES, PROTOCOL_VERSION,
};

use crate::auth_store::{
    fresh_nonce, AuthStore, AuthStoreError, MAX_CONTROL_CREDENTIAL_LIFETIME_MS,
    MAX_PAIRING_TOKEN_LIFETIME_MS,
};

const MAX_SAFE_MESSAGE_BYTES: usize = 512;

/// Minimal boundary needed by the router. Production uses `AgentCore`; tests
/// can provide a deterministic in-memory implementation.
pub trait ServiceAgent {
    fn environment(&self, silo_id: Uuid) -> Option<EnvironmentRecord>;
    fn execute(&mut self, request: AgentRequest) -> Result<AgentResponse, AgentError>;
    /// Returns true when durable Agent state became uncertain and the service
    /// must reject future provider mutations until restart.
    fn maintenance_tick(&mut self) -> bool;
}

impl<P, S, C> ServiceAgent for AgentCore<P, S, C>
where
    P: AgentProvider,
    S: AgentStore,
    C: Clock,
{
    fn environment(&self, silo_id: Uuid) -> Option<EnvironmentRecord> {
        self.store().environment(silo_id)
    }

    fn execute(&mut self, request: AgentRequest) -> Result<AgentResponse, AgentError> {
        AgentCore::execute(self, request)
    }

    fn maintenance_tick(&mut self) -> bool {
        // Failed deletions remain non-deleted in the durable store and are
        // retried by the next listener-scheduled tick. Error details are not
        // logged because provider failures can contain local process context.
        self.sweep_expired()
            .iter()
            .any(|result| matches!(result, Err(AgentError::Store(_))))
    }
}

/// A bounded application response consumed by the HTTPS layer.
///
/// `Drop` clears the serialized bytes because a successful pairing response
/// contains the one plaintext control credential issued to the client.
pub struct ServiceResponse {
    status_code: u16,
    body: Vec<u8>,
}

impl ServiceResponse {
    pub fn status_code(&self) -> u16 {
        self.status_code
    }

    pub fn body(&self) -> &[u8] {
        &self.body
    }

    pub fn into_parts(mut self) -> (u16, Vec<u8>) {
        let body = std::mem::take(&mut self.body);
        (self.status_code, body)
    }

    fn protocol(body: Vec<u8>) -> Self {
        Self {
            status_code: 200,
            body,
        }
    }

    fn fixed_error(status_code: u16, code: &'static str) -> Self {
        let body = format!("{{\"error\":\"{code}\"}}").into_bytes();
        Self { status_code, body }
    }
}

impl Drop for ServiceResponse {
    fn drop(&mut self) {
        self.body.zeroize();
    }
}

/// Stateful, single-writer router. The blocking HTTPS listener intentionally
/// serializes access to this value so auth and Agent state mutations have one
/// in-process ordering in addition to their durable filesystem ordering.
pub struct AgentService<A, C> {
    auth: AuthStore,
    agent: A,
    node: NodeDisclosure,
    capabilities: Vec<RemoteCapability>,
    clock: C,
    credential_lifetime_ms: u64,
    agent_state_poisoned: bool,
}

impl<A: ServiceAgent, C: Clock> AgentService<A, C> {
    pub fn new(
        auth: AuthStore,
        agent: A,
        node: NodeDisclosure,
        capabilities: Vec<RemoteCapability>,
        clock: C,
        credential_lifetime_ms: u64,
    ) -> Result<Self, ServiceBuildError> {
        node.validate()
            .map_err(|_| ServiceBuildError::InvalidConfiguration)?;
        if node.node_id == Uuid::nil()
            || !(60_000..=MAX_CONTROL_CREDENTIAL_LIFETIME_MS).contains(&credential_lifetime_ms)
        {
            return Err(ServiceBuildError::InvalidConfiguration);
        }
        validate_capabilities(&capabilities)?;
        Ok(Self {
            auth,
            agent,
            node,
            capabilities,
            clock,
            credential_lifetime_ms,
            agent_state_poisoned: false,
        })
    }

    pub fn server_id(&self) -> Uuid {
        self.auth.server_id()
    }

    /// Executes bounded-frequency maintenance scheduled by the listener even
    /// when no client request arrives.
    pub fn maintenance_tick(&mut self) {
        if !self.agent_state_poisoned && self.agent.maintenance_tick() {
            self.agent_state_poisoned = true;
        }
    }

    #[cfg(test)]
    pub fn agent_for_test(&self) -> &A {
        &self.agent
    }

    #[cfg(test)]
    pub fn auth_for_test_mut(&mut self) -> &mut AuthStore {
        &mut self.auth
    }

    /// Routes exactly one already-bounded JSON request. Header syntax and body
    /// size are checked again here so direct callers cannot bypass HTTPS limits.
    pub fn route(&mut self, bearer: Option<&str>, payload: &[u8]) -> ServiceResponse {
        if payload.is_empty() || payload.len() > MAX_MESSAGE_BYTES {
            return ServiceResponse::fixed_error(400, "invalid_request");
        }

        match bearer {
            None => {
                if let Ok(request) = strict_json::<PairingRequestEnvelope>(payload) {
                    return self.route_pairing(request);
                }
                match strict_json::<AuthenticatedWireRequest>(payload) {
                    Ok(request) => self.route_authenticated(request, None),
                    Err(_) => ServiceResponse::fixed_error(400, "invalid_request"),
                }
            }
            Some(credential) => match strict_json::<AuthenticatedWireRequest>(payload) {
                Ok(request) => self.route_authenticated(request, Some(credential)),
                Err(_) => ServiceResponse::fixed_error(400, "invalid_request"),
            },
        }
    }

    fn route_pairing(&mut self, request: PairingRequestEnvelope) -> ServiceResponse {
        let now = self.clock.now_unix_ms();
        if request.protocol_version != PROTOCOL_VERSION
            || request.request_id == Uuid::nil()
            || !valid_nonce(&request.nonce)
            || request.sent_at_unix_ms.abs_diff(now) > MAX_CLOCK_SKEW_MS
            || request.body.pairing_token_id == Uuid::nil()
        {
            return ServiceResponse::fixed_error(400, "invalid_pairing_request");
        }

        if !request.body.approved_by_user {
            return self.pairing_rejection(
                request.request_id,
                PairingRejectionCode::ApprovalRequired,
                "Explicit local user approval is required.",
                now,
            );
        }
        if request.body.pairing_token_expires_at_unix_ms <= now {
            return self.pairing_rejection(
                request.request_id,
                PairingRejectionCode::TokenExpired,
                "The one-time pairing token has expired.",
                now,
            );
        }
        if request.body.pairing_token_expires_at_unix_ms - now > MAX_PAIRING_TOKEN_LIFETIME_MS
            || !valid_secret(&request.body.pairing_token)
        {
            return self.pairing_rejection(
                request.request_id,
                PairingRejectionCode::TokenInvalid,
                "The one-time pairing token is invalid.",
                now,
            );
        }

        let pairing = match request.body.tls_pin_rotation.as_ref() {
            Some(rotation) => self.auth.redeem_pairing_token_for_rotation(
                request.request_id,
                &request.nonce,
                request.body.pairing_token_id,
                &request.body.pairing_token,
                request.body.pairing_token_expires_at_unix_ms,
                now,
                self.credential_lifetime_ms,
                rotation,
            ),
            None => self.auth.redeem_pairing_token(
                request.request_id,
                &request.nonce,
                request.body.pairing_token_id,
                &request.body.pairing_token,
                request.body.pairing_token_expires_at_unix_ms,
                now,
                self.credential_lifetime_ms,
            ),
        };
        match pairing {
            Ok(grant) => {
                let envelope = PairingResponseEnvelope {
                    protocol_version: PROTOCOL_VERSION,
                    response_id: Uuid::new_v4(),
                    in_reply_to: request.request_id,
                    nonce: fresh_nonce(),
                    sent_at_unix_ms: now,
                    sequence: grant.response_sequence,
                    body: PairingResponseBody::Success {
                        server_id: self.auth.server_id(),
                        client_credential_id: grant.credential_id,
                        node: self.node.clone(),
                        client_credential: grant.credential.to_string(),
                        credential_expires_at_unix_ms: grant.credential_expires_at_unix_ms,
                        capabilities: self.capabilities.clone(),
                    },
                };
                serialize_bounded(&envelope)
                    .map(ServiceResponse::protocol)
                    .unwrap_or_else(internal_error)
            }
            Err(AuthStoreError::PairingTokenExpired) => self.pairing_rejection(
                request.request_id,
                PairingRejectionCode::TokenExpired,
                "The one-time pairing token has expired.",
                now,
            ),
            Err(AuthStoreError::Replay) => self.pairing_rejection(
                request.request_id,
                PairingRejectionCode::Replay,
                "The pairing request was already consumed.",
                now,
            ),
            Err(AuthStoreError::CredentialLimitReached) => self.pairing_rejection(
                request.request_id,
                PairingRejectionCode::LimitExceeded,
                "The local pairing state reached its configured limit.",
                now,
            ),
            Err(AuthStoreError::PinRotationAuthorizationInvalid) => self.pairing_rejection(
                request.request_id,
                PairingRejectionCode::RotationAuthorizationInvalid,
                "The TLS pin rotation authorization is invalid, expired, or already consumed.",
                now,
            ),
            Err(
                AuthStoreError::PairingTokenInvalid
                | AuthStoreError::CredentialInvalid
                | AuthStoreError::InvalidRequestIdentity
                | AuthStoreError::InvalidLifetime(_),
            ) => self.pairing_rejection(
                request.request_id,
                PairingRejectionCode::TokenInvalid,
                "The one-time pairing token is invalid.",
                now,
            ),
            Err(_) => internal_error(()),
        }
    }

    fn pairing_rejection(
        &mut self,
        in_reply_to: Uuid,
        code: PairingRejectionCode,
        message: &'static str,
        now: u64,
    ) -> ServiceResponse {
        let sequence = match self.auth.reserve_response_sequence() {
            Ok(sequence) => sequence,
            Err(_) => return internal_error(()),
        };
        let envelope = PairingResponseEnvelope {
            protocol_version: PROTOCOL_VERSION,
            response_id: Uuid::new_v4(),
            in_reply_to,
            nonce: fresh_nonce(),
            sent_at_unix_ms: now,
            sequence,
            body: PairingResponseBody::Rejected {
                code,
                message: message.to_owned(),
            },
        };
        serialize_bounded(&envelope)
            .map(ServiceResponse::protocol)
            .unwrap_or_else(internal_error)
    }

    fn route_authenticated(
        &mut self,
        request: AuthenticatedWireRequest,
        bearer: Option<&str>,
    ) -> ServiceResponse {
        match request {
            AuthenticatedWireRequest::Operation(request) => self.route_operation(request, bearer),
            AuthenticatedWireRequest::Agent(request) => self.route_agent(request, bearer),
            AuthenticatedWireRequest::TlsPinRotationAuthorization(request) => {
                self.route_tls_pin_rotation_authorization(request, bearer)
            }
        }
    }

    fn route_tls_pin_rotation_authorization(
        &mut self,
        request: TlsPinRotationAuthorizationRequestEnvelope,
        bearer: Option<&str>,
    ) -> ServiceResponse {
        let now = self.clock.now_unix_ms();
        if let Some((code, message)) = validate_authenticated_metadata(
            request.protocol_version,
            request.request_id,
            &request.nonce,
            request.sequence,
            request.sent_at_unix_ms,
            now,
        ) {
            return self.pin_rotation_rejection(request.request_id, code, message, now, None);
        }
        if request.body.client_credential_id == Uuid::nil()
            || request.body.pairing_token_id == Uuid::nil()
            || request.body.new_pin.validate().is_err()
        {
            return self.pin_rotation_rejection(
                request.request_id,
                RejectionCode::InvalidRequest,
                "The TLS pin rotation authorization request is invalid.",
                now,
                None,
            );
        }
        let authorization = match bearer {
            Some(bearer) => self.auth.authorize_pin_rotation(
                bearer,
                request.request_id,
                &request.nonce,
                request.sequence,
                request.body.client_credential_id,
                request.body.pairing_token_id,
                &request.body.new_pin,
                now,
            ),
            None => Err(AuthStoreError::CredentialInvalid),
        };
        match authorization {
            Ok(grant) => self.pin_rotation_body(
                request.request_id,
                now,
                grant.response_sequence,
                TlsPinRotationAuthorizationResponseBody::Success {
                    server_id: self.auth.server_id(),
                    client_credential_id: grant.credential_id,
                    pairing_token_id: request.body.pairing_token_id,
                    new_pin: request.body.new_pin,
                    challenge: grant.challenge,
                    authorization_expires_at_unix_ms: grant.expires_at_unix_ms,
                },
            ),
            Err(AuthStoreError::Replay) => self.pin_rotation_rejection(
                request.request_id,
                RejectionCode::Replay,
                "The TLS pin rotation authorization request was already consumed.",
                now,
                None,
            ),
            Err(
                AuthStoreError::CredentialInvalid
                | AuthStoreError::CredentialExpired
                | AuthStoreError::CredentialRevoked,
            ) => self.pin_rotation_rejection(
                request.request_id,
                RejectionCode::Unauthorized,
                "The old control credential is invalid, expired, or revoked.",
                now,
                None,
            ),
            Err(AuthStoreError::PairingTokenInvalid | AuthStoreError::PairingTokenExpired) => self
                .pin_rotation_rejection(
                    request.request_id,
                    RejectionCode::InvalidRequest,
                    "The referenced one-time pairing token is unavailable or expired.",
                    now,
                    None,
                ),
            Err(_) => self.pin_rotation_rejection(
                request.request_id,
                RejectionCode::InvalidState,
                "The local authentication state could not authorize TLS pin rotation.",
                now,
                None,
            ),
        }
    }

    fn route_operation(
        &mut self,
        request: OperationRequestEnvelope,
        bearer: Option<&str>,
    ) -> ServiceResponse {
        let now = self.clock.now_unix_ms();
        if let Some((code, message)) = validate_authenticated_metadata(
            request.protocol_version,
            request.request_id,
            &request.nonce,
            request.sequence,
            request.sent_at_unix_ms,
            now,
        ) {
            return self.operation_rejection(request.request_id, code, message, now, None);
        }
        if request.silo_id == Uuid::nil() {
            return self.operation_rejection(
                request.request_id,
                RejectionCode::InvalidRequest,
                "The operation request is invalid.",
                now,
                None,
            );
        }

        let authenticated = match self.authenticate_request(
            bearer,
            request.request_id,
            &request.nonce,
            request.sequence,
            now,
        ) {
            Ok(authenticated) => authenticated,
            Err((code, message)) => {
                return self.operation_rejection(request.request_id, code, message, now, None)
            }
        };

        if self.agent_state_poisoned {
            return self.operation_rejection(
                request.request_id,
                RejectionCode::InvalidState,
                "The durable Agent state is uncertain; operator restart is required.",
                now,
                Some(authenticated.response_sequence),
            );
        }

        let operation = operation_of(&request.body);
        let mapped = match self.map_lifecycle_command(request.silo_id, request.body) {
            Ok(command) => command,
            Err((code, message)) => {
                return self.operation_rejection(
                    request.request_id,
                    code,
                    message,
                    now,
                    Some(authenticated.response_sequence),
                )
            }
        };
        if operation != RemoteOperation::Destroy {
            if let Some(reason) = unavailable_reason(&self.capabilities, operation) {
                return self.operation_body(
                    request.request_id,
                    now,
                    authenticated.response_sequence,
                    OperationResponseBody::Unavailable { operation, reason },
                );
            }
        }

        let agent_request = AgentRequest {
            protocol_version: request.protocol_version,
            request_id: request.request_id,
            nonce: request.nonce,
            sequence: request.sequence,
            sent_at_unix_ms: request.sent_at_unix_ms,
            principal: verisilo_remote_backend::agent::Principal {
                kind: PrincipalKind::ControlPlane,
                credential_id: authenticated.credential_id,
                authorization_id: None,
            },
            command: mapped.command,
        };
        let body = match self.agent.execute(agent_request) {
            Ok(response) => {
                let stored_record = self.agent.environment(request.silo_id);
                match lifecycle_result(
                    operation,
                    request.silo_id,
                    (mapped.binding_id, mapped.remote_environment_id),
                    self.auth.server_id(),
                    stored_record.as_ref(),
                    response,
                    now,
                ) {
                    Ok(result) => OperationResponseBody::Success { result },
                    Err(error) => operation_error_body(operation, error),
                }
            }
            Err(error) => {
                if matches!(&error, AgentError::Store(_)) {
                    self.agent_state_poisoned = true;
                }
                operation_error_body(operation, error)
            }
        };
        self.operation_body(
            request.request_id,
            now,
            authenticated.response_sequence,
            body,
        )
    }

    fn map_lifecycle_command(
        &self,
        silo_id: Uuid,
        body: OperationBody,
    ) -> Result<MappedLifecycleCommand, (RejectionCode, &'static str)> {
        let bound = |binding_id: Uuid, remote_environment_id: Uuid| {
            if binding_id == Uuid::nil() || remote_environment_id == Uuid::nil() {
                return Err((
                    RejectionCode::InvalidRequest,
                    "The operation request is invalid.",
                ));
            }
            let record = self.agent.environment(silo_id).ok_or((
                RejectionCode::InvalidState,
                "No bound remote environment exists for this Silo.",
            ))?;
            if record.binding_id != binding_id
                || record.remote_environment_id != remote_environment_id
            {
                return Err((
                    RejectionCode::InvalidRequest,
                    "The operation binding does not match the stored environment.",
                ));
            }
            Ok(())
        };

        match body {
            OperationBody::Create {
                network,
                ttl_seconds,
                cost_acknowledged,
            } => {
                if !(60..=verisilo_remote_backend::agent::MAX_ENVIRONMENT_TTL_SECONDS)
                    .contains(&ttl_seconds)
                    || !cost_acknowledged
                {
                    return Err((
                        RejectionCode::InvalidRequest,
                        "Create requires bounded TTL and explicit cost acknowledgement.",
                    ));
                }
                let binding_id = Uuid::new_v4();
                let remote_environment_id = Uuid::new_v4();
                Ok(MappedLifecycleCommand {
                    command: AgentCommand::Create {
                        silo_id,
                        binding_id,
                        remote_environment_id,
                        ttl_seconds,
                        network,
                        cost_acknowledged,
                    },
                    binding_id,
                    remote_environment_id,
                })
            }
            OperationBody::Start {
                binding_id,
                remote_environment_id,
            } => {
                bound(binding_id, remote_environment_id)?;
                Ok(MappedLifecycleCommand {
                    command: AgentCommand::Start { silo_id },
                    binding_id,
                    remote_environment_id,
                })
            }
            OperationBody::Stop {
                binding_id,
                remote_environment_id,
            } => {
                bound(binding_id, remote_environment_id)?;
                Ok(MappedLifecycleCommand {
                    command: AgentCommand::Stop { silo_id },
                    binding_id,
                    remote_environment_id,
                })
            }
            OperationBody::Pause {
                binding_id,
                remote_environment_id,
            } => {
                bound(binding_id, remote_environment_id)?;
                Ok(MappedLifecycleCommand {
                    command: AgentCommand::Pause { silo_id },
                    binding_id,
                    remote_environment_id,
                })
            }
            OperationBody::Snapshot {
                binding_id,
                remote_environment_id,
            } => {
                bound(binding_id, remote_environment_id)?;
                Ok(MappedLifecycleCommand {
                    command: AgentCommand::Snapshot { silo_id },
                    binding_id,
                    remote_environment_id,
                })
            }
            OperationBody::Destroy {
                binding_id,
                remote_environment_id,
                confirm_destroy,
            } => {
                bound(binding_id, remote_environment_id)?;
                Ok(MappedLifecycleCommand {
                    command: AgentCommand::Destroy {
                        silo_id,
                        confirm_destroy,
                    },
                    binding_id,
                    remote_environment_id,
                })
            }
            OperationBody::ConfigureNetwork {
                binding_id,
                remote_environment_id,
                network,
            } => {
                bound(binding_id, remote_environment_id)?;
                Ok(MappedLifecycleCommand {
                    command: AgentCommand::ConfigureNetwork { silo_id, network },
                    binding_id,
                    remote_environment_id,
                })
            }
            OperationBody::Health {
                binding_id,
                remote_environment_id,
            } => {
                bound(binding_id, remote_environment_id)?;
                Ok(MappedLifecycleCommand {
                    command: AgentCommand::Health { silo_id },
                    binding_id,
                    remote_environment_id,
                })
            }
            OperationBody::Logs {
                binding_id,
                remote_environment_id,
                cursor,
                limit,
            } => {
                bound(binding_id, remote_environment_id)?;
                if cursor.is_some() || limit == 0 || limit > MAX_LOG_ENTRIES {
                    return Err((
                        RejectionCode::InvalidRequest,
                        "Logs require a 1-200 limit; provider pagination is not available in V0.9.",
                    ));
                }
                Ok(MappedLifecycleCommand {
                    command: AgentCommand::Logs { silo_id, limit },
                    binding_id,
                    remote_environment_id,
                })
            }
        }
    }

    fn route_agent(&mut self, request: AgentRequest, bearer: Option<&str>) -> ServiceResponse {
        let now = self.clock.now_unix_ms();
        if let Some((code, message)) = validate_authenticated_metadata(
            request.protocol_version,
            request.request_id,
            &request.nonce,
            request.sequence,
            request.sent_at_unix_ms,
            now,
        ) {
            return self.agent_rejection(request.request_id, code, message, now, None);
        }
        let authenticated = match self.authenticate_request(
            bearer,
            request.request_id,
            &request.nonce,
            request.sequence,
            now,
        ) {
            Ok(authenticated) => authenticated,
            Err((code, message)) => {
                return self.agent_rejection(request.request_id, code, message, now, None)
            }
        };
        if self.agent_state_poisoned {
            return self.agent_rejection(
                request.request_id,
                RejectionCode::InvalidState,
                "The durable Agent state is uncertain; operator restart is required.",
                now,
                Some(authenticated.response_sequence),
            );
        }
        if !is_interactive_command(&request.command) {
            return self.agent_rejection(
                request.request_id,
                RejectionCode::InvalidRequest,
                "This endpoint accepts only the fixed interactive Agent commands.",
                now,
                Some(authenticated.response_sequence),
            );
        }
        if request.principal.credential_id != authenticated.credential_id {
            return self.agent_rejection(
                request.request_id,
                RejectionCode::Unauthorized,
                "The authenticated credential does not match the request principal.",
                now,
                Some(authenticated.response_sequence),
            );
        }

        let request_id = request.request_id;
        let expected = ExpectedAgentResponse::from_command(&request.command);
        let body = match self.agent.execute(request) {
            Ok(response) if expected.matches(&response) => {
                AgentControlResponseBody::Success { response }
            }
            Ok(_) => AgentControlResponseBody::Rejected {
                code: RejectionCode::InvalidState,
                message: "The local Agent returned an unexpected response type.".to_owned(),
            },
            Err(AgentError::Unavailable(_)) => AgentControlResponseBody::Unavailable {
                reason: "The interactive operation is unavailable in local provider configuration."
                    .to_owned(),
            },
            Err(error) => {
                if matches!(&error, AgentError::Store(_)) {
                    self.agent_state_poisoned = true;
                }
                let (code, message) = map_agent_error(error);
                AgentControlResponseBody::Rejected {
                    code,
                    message: message.to_owned(),
                }
            }
        };
        self.agent_body(request_id, now, authenticated.response_sequence, body)
    }

    fn authenticate_request(
        &mut self,
        bearer: Option<&str>,
        request_id: Uuid,
        nonce: &str,
        sequence: u64,
        now: u64,
    ) -> Result<crate::auth_store::AuthenticatedOperation, (RejectionCode, &'static str)> {
        let Some(bearer) = bearer else {
            return Err((
                RejectionCode::NotPaired,
                "A paired control credential is required.",
            ));
        };
        match self
            .auth
            .authenticate_operation(bearer, request_id, nonce, sequence, now)
        {
            Ok(authenticated) => Ok(authenticated),
            Err(AuthStoreError::Replay) => Err((
                RejectionCode::Replay,
                "The authenticated request was already consumed.",
            )),
            Err(
                AuthStoreError::CredentialInvalid
                | AuthStoreError::CredentialExpired
                | AuthStoreError::CredentialRevoked
                | AuthStoreError::InvalidRequestIdentity,
            ) => {
                if self.auth.has_active_credentials(now) {
                    Err((
                        RejectionCode::Unauthorized,
                        "The bearer credential is invalid, expired, or revoked.",
                    ))
                } else {
                    Err((
                        RejectionCode::NotPaired,
                        "A paired control credential is required.",
                    ))
                }
            }
            Err(_) => Err((
                RejectionCode::InvalidState,
                "The local authentication state could not accept the request.",
            )),
        }
    }

    fn operation_rejection(
        &mut self,
        in_reply_to: Uuid,
        code: RejectionCode,
        message: &'static str,
        now: u64,
        reserved_sequence: Option<u64>,
    ) -> ServiceResponse {
        let sequence = match reserved_sequence {
            Some(sequence) => sequence,
            None => match self.auth.reserve_response_sequence() {
                Ok(sequence) => sequence,
                Err(_) => return internal_error(()),
            },
        };
        self.operation_body(
            in_reply_to,
            now,
            sequence,
            OperationResponseBody::Rejected {
                code,
                message: message.to_owned(),
            },
        )
    }

    fn operation_body(
        &self,
        in_reply_to: Uuid,
        now: u64,
        sequence: u64,
        body: OperationResponseBody,
    ) -> ServiceResponse {
        let metadata = ResponseMetadata::new(in_reply_to, now, sequence);
        let envelope = metadata.operation(body);
        match serialize_bounded(&envelope) {
            Ok(raw) => ServiceResponse::protocol(raw),
            Err(_) => {
                let fallback = metadata.operation(OperationResponseBody::Rejected {
                    code: RejectionCode::LimitExceeded,
                    message: "The local response exceeded the protocol size limit.".to_owned(),
                });
                serialize_bounded(&fallback)
                    .map(ServiceResponse::protocol)
                    .unwrap_or_else(internal_error)
            }
        }
    }

    fn agent_rejection(
        &mut self,
        in_reply_to: Uuid,
        code: RejectionCode,
        message: &'static str,
        now: u64,
        reserved_sequence: Option<u64>,
    ) -> ServiceResponse {
        let sequence = match reserved_sequence {
            Some(sequence) => sequence,
            None => match self.auth.reserve_response_sequence() {
                Ok(sequence) => sequence,
                Err(_) => return internal_error(()),
            },
        };
        self.agent_body(
            in_reply_to,
            now,
            sequence,
            AgentControlResponseBody::Rejected {
                code,
                message: message.to_owned(),
            },
        )
    }

    fn agent_body(
        &self,
        in_reply_to: Uuid,
        now: u64,
        sequence: u64,
        body: AgentControlResponseBody,
    ) -> ServiceResponse {
        let metadata = ResponseMetadata::new(in_reply_to, now, sequence);
        let envelope = metadata.agent(body);
        match serialize_bounded(&envelope) {
            Ok(raw) => ServiceResponse::protocol(raw),
            Err(_) => {
                let fallback = metadata.agent(AgentControlResponseBody::Rejected {
                    code: RejectionCode::LimitExceeded,
                    message: "The local response exceeded the protocol size limit.".to_owned(),
                });
                serialize_bounded(&fallback)
                    .map(ServiceResponse::protocol)
                    .unwrap_or_else(internal_error)
            }
        }
    }

    fn pin_rotation_rejection(
        &mut self,
        in_reply_to: Uuid,
        code: RejectionCode,
        message: &'static str,
        now: u64,
        reserved_sequence: Option<u64>,
    ) -> ServiceResponse {
        let sequence = match reserved_sequence {
            Some(sequence) => sequence,
            None => match self.auth.reserve_response_sequence() {
                Ok(sequence) => sequence,
                Err(_) => return internal_error(()),
            },
        };
        self.pin_rotation_body(
            in_reply_to,
            now,
            sequence,
            TlsPinRotationAuthorizationResponseBody::Rejected {
                code,
                message: message.to_owned(),
            },
        )
    }

    fn pin_rotation_body(
        &self,
        in_reply_to: Uuid,
        now: u64,
        sequence: u64,
        body: TlsPinRotationAuthorizationResponseBody,
    ) -> ServiceResponse {
        let metadata = ResponseMetadata::new(in_reply_to, now, sequence);
        let envelope = metadata.pin_rotation(body);
        serialize_bounded(&envelope)
            .map(ServiceResponse::protocol)
            .unwrap_or_else(internal_error)
    }
}

#[derive(Debug, Error)]
pub enum ServiceBuildError {
    #[error("Remote Agent service configuration is invalid")]
    InvalidConfiguration,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum AuthenticatedWireRequest {
    Operation(OperationRequestEnvelope),
    Agent(AgentRequest),
    TlsPinRotationAuthorization(TlsPinRotationAuthorizationRequestEnvelope),
}

struct MappedLifecycleCommand {
    command: AgentCommand,
    binding_id: Uuid,
    remote_environment_id: Uuid,
}

#[derive(Clone, Copy)]
struct ResponseMetadata {
    response_id: Uuid,
    in_reply_to: Uuid,
    nonce: Uuid,
    sent_at_unix_ms: u64,
    sequence: u64,
}

impl ResponseMetadata {
    fn new(in_reply_to: Uuid, sent_at_unix_ms: u64, sequence: u64) -> Self {
        Self {
            response_id: Uuid::new_v4(),
            in_reply_to,
            nonce: Uuid::new_v4(),
            sent_at_unix_ms,
            sequence,
        }
    }

    fn operation(self, body: OperationResponseBody) -> OperationResponseEnvelope {
        OperationResponseEnvelope {
            protocol_version: PROTOCOL_VERSION,
            response_id: self.response_id,
            in_reply_to: self.in_reply_to,
            nonce: self.nonce.simple().to_string(),
            sent_at_unix_ms: self.sent_at_unix_ms,
            sequence: self.sequence,
            body,
        }
    }

    fn agent(self, body: AgentControlResponseBody) -> AgentResponseEnvelope {
        AgentResponseEnvelope {
            protocol_version: PROTOCOL_VERSION,
            response_id: self.response_id,
            in_reply_to: self.in_reply_to,
            nonce: self.nonce.simple().to_string(),
            sent_at_unix_ms: self.sent_at_unix_ms,
            sequence: self.sequence,
            body,
        }
    }

    fn pin_rotation(
        self,
        body: TlsPinRotationAuthorizationResponseBody,
    ) -> TlsPinRotationAuthorizationResponseEnvelope {
        TlsPinRotationAuthorizationResponseEnvelope {
            protocol_version: PROTOCOL_VERSION,
            response_id: self.response_id,
            in_reply_to: self.in_reply_to,
            nonce: self.nonce.simple().to_string(),
            sent_at_unix_ms: self.sent_at_unix_ms,
            sequence: self.sequence,
            body,
        }
    }
}

fn strict_json<T: for<'de> Deserialize<'de>>(payload: &[u8]) -> Result<T, serde_json::Error> {
    let mut deserializer = serde_json::Deserializer::from_slice(payload);
    let value = T::deserialize(&mut deserializer)?;
    deserializer.end()?;
    Ok(value)
}

fn serialize_bounded(value: &impl Serialize) -> Result<Vec<u8>, ()> {
    let raw = serde_json::to_vec(value).map_err(|_| ())?;
    if raw.len() > MAX_MESSAGE_BYTES {
        return Err(());
    }
    Ok(raw)
}

fn valid_secret(value: &str) -> bool {
    (32..=512).contains(&value.len())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
}

fn valid_nonce(value: &str) -> bool {
    (32..=128).contains(&value.len())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
}

fn valid_safe_message(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_SAFE_MESSAGE_BYTES
        && value.trim() == value
        && !value
            .chars()
            .any(|character| character.is_control() && character != '\t')
}

fn validate_capabilities(capabilities: &[RemoteCapability]) -> Result<(), ServiceBuildError> {
    if capabilities.len() != RemoteOperation::ALL.len()
        || RemoteOperation::ALL.iter().any(|operation| {
            capabilities
                .iter()
                .filter(|item| item.operation == *operation)
                .count()
                != 1
        })
        || capabilities.iter().any(|capability| {
            matches!(
                &capability.availability,
                CapabilityAvailability::Unavailable { reason } if !valid_safe_message(reason)
            )
        })
    {
        return Err(ServiceBuildError::InvalidConfiguration);
    }
    if capability_is_available(capabilities, RemoteOperation::Create)
        && !capability_is_available(capabilities, RemoteOperation::Destroy)
    {
        // Every created environment carries a TTL. Advertising create without
        // a working destroy path would make that lifecycle promise impossible.
        return Err(ServiceBuildError::InvalidConfiguration);
    }
    Ok(())
}

fn capability_is_available(capabilities: &[RemoteCapability], operation: RemoteOperation) -> bool {
    capabilities.iter().any(|capability| {
        capability.operation == operation
            && matches!(&capability.availability, CapabilityAvailability::Available)
    })
}

fn validate_authenticated_metadata(
    protocol_version: u16,
    request_id: Uuid,
    nonce: &str,
    sequence: u64,
    sent_at_unix_ms: u64,
    now: u64,
) -> Option<(RejectionCode, &'static str)> {
    if protocol_version != PROTOCOL_VERSION
        || request_id == Uuid::nil()
        || sequence == 0
        || !valid_nonce(nonce)
    {
        return Some((
            RejectionCode::InvalidRequest,
            "The authenticated request metadata is invalid.",
        ));
    }
    if sent_at_unix_ms.abs_diff(now) > MAX_CLOCK_SKEW_MS {
        return Some((
            RejectionCode::StaleRequest,
            "The authenticated request timestamp is outside the allowed window.",
        ));
    }
    None
}

fn operation_of(body: &OperationBody) -> RemoteOperation {
    match body {
        OperationBody::Create { .. } => RemoteOperation::Create,
        OperationBody::Start { .. } => RemoteOperation::Start,
        OperationBody::Stop { .. } => RemoteOperation::Stop,
        OperationBody::Pause { .. } => RemoteOperation::Pause,
        OperationBody::Snapshot { .. } => RemoteOperation::Snapshot,
        OperationBody::Destroy { .. } => RemoteOperation::Destroy,
        OperationBody::ConfigureNetwork { .. } => RemoteOperation::ConfigureNetwork,
        OperationBody::Health { .. } => RemoteOperation::Health,
        OperationBody::Logs { .. } => RemoteOperation::Logs,
    }
}

fn unavailable_reason(
    capabilities: &[RemoteCapability],
    operation: RemoteOperation,
) -> Option<String> {
    capabilities
        .iter()
        .find(|capability| capability.operation == operation)
        .and_then(|capability| match &capability.availability {
            CapabilityAvailability::Available => None,
            CapabilityAvailability::Unavailable { reason } => Some(reason.clone()),
        })
}

fn lifecycle_result(
    operation: RemoteOperation,
    silo_id: Uuid,
    expected_binding: (Uuid, Uuid),
    server_id: Uuid,
    stored_record: Option<&EnvironmentRecord>,
    response: AgentResponse,
    now: u64,
) -> Result<OperationResult, AgentError> {
    let (expected_binding_id, expected_remote_environment_id) = expected_binding;
    if server_id == Uuid::nil() {
        return Err(AgentError::InvalidState(
            "Agent server identity is unavailable.".to_owned(),
        ));
    }
    match (operation, response) {
        (
            operation @ (RemoteOperation::Create
            | RemoteOperation::Start
            | RemoteOperation::Stop
            | RemoteOperation::Pause
            | RemoteOperation::Snapshot
            | RemoteOperation::ConfigureNetwork
            | RemoteOperation::Health),
            AgentResponse::Environment { record, evidence },
        ) => {
            if record.silo_id != silo_id
                || record.binding_id != expected_binding_id
                || record.remote_environment_id != expected_remote_environment_id
                || record.last_activity_at_unix_ms == 0
                || record.last_activity_at_unix_ms > now.saturating_add(MAX_CLOCK_SKEW_MS)
                || stored_record != Some(&record)
                || (matches!(
                    operation,
                    RemoteOperation::Create
                        | RemoteOperation::Start
                        | RemoteOperation::ConfigureNetwork
                        | RemoteOperation::Health
                ) && evidence.is_none())
                || evidence.as_ref().is_some_and(|item| {
                    item.binding_id != record.binding_id
                        || item.remote_environment_id != record.remote_environment_id
                })
            {
                return Err(AgentError::InvalidState(
                    "Agent response binding mismatch.".to_owned(),
                ));
            }
            let volume = if operation == RemoteOperation::Create {
                if !record.volume.encrypted
                    || record.volume.key_custody != KeyCustody::UserControlled
                    || record.volume.volume_id == Uuid::nil()
                    || record.volume.key_id == Uuid::nil()
                {
                    return Err(AgentError::InvalidState(
                        "Create omitted encrypted volume attestation.".to_owned(),
                    ));
                }
                Some(record.volume.clone())
            } else {
                None
            };
            Ok(OperationResult {
                operation,
                silo_id,
                binding_id: record.binding_id,
                remote_environment_id: record.remote_environment_id,
                server_id,
                last_activity_at_unix_ms: record.last_activity_at_unix_ms,
                state: result_state(operation),
                volume,
                evidence,
                logs: None,
                next_cursor: None,
                deletion_proof: None,
            })
        }
        (RemoteOperation::Destroy, AgentResponse::Deleted { proof }) => {
            let record = stored_record.ok_or_else(|| {
                AgentError::InvalidState(
                    "Deleted environment is missing its durable record.".to_owned(),
                )
            })?;
            if record.state != EnvironmentState::Deleted
                || record.silo_id != silo_id
                || record.binding_id != expected_binding_id
                || record.remote_environment_id != expected_remote_environment_id
                || record.deletion_proof_id != Some(proof.proof_id)
                || record.last_activity_at_unix_ms != proof.deleted_at_unix_ms
                || proof.deleted_at_unix_ms == 0
                || proof.deleted_at_unix_ms > now.saturating_add(MAX_CLOCK_SKEW_MS)
                || proof.silo_id != silo_id
                || proof.binding_id != expected_binding_id
                || proof.remote_environment_id != expected_remote_environment_id
                || proof.volume_id != record.volume.volume_id
                || proof.provider_receipt_id == Uuid::nil()
                || !deletion_resources_are_bound(
                    &proof.resource_deletions,
                    record.remote_environment_id,
                    record.volume.volume_id,
                    record.volume.key_id,
                )
            {
                return Err(AgentError::InvalidState(
                    "Deletion proof does not match the durable deleted environment record."
                        .to_owned(),
                ));
            }
            Ok(OperationResult {
                operation,
                silo_id,
                binding_id: proof.binding_id,
                remote_environment_id: proof.remote_environment_id,
                server_id,
                last_activity_at_unix_ms: record.last_activity_at_unix_ms,
                state: RemoteResultState::Destroyed,
                volume: None,
                evidence: None,
                logs: None,
                next_cursor: None,
                deletion_proof: Some(proof),
            })
        }
        (
            RemoteOperation::Logs,
            AgentResponse::Logs {
                entries,
                last_activity_at_unix_ms,
            },
        ) => {
            let record = stored_record.ok_or_else(|| {
                AgentError::InvalidState(
                    "Log response is missing its environment record.".to_owned(),
                )
            })?;
            if entries.len() > usize::from(MAX_LOG_ENTRIES)
                || entries.iter().any(|entry| !valid_safe_message(entry))
                || record.silo_id != silo_id
                || record.binding_id != expected_binding_id
                || record.remote_environment_id != expected_remote_environment_id
                || record.last_activity_at_unix_ms != last_activity_at_unix_ms
                || last_activity_at_unix_ms == 0
                || last_activity_at_unix_ms > now.saturating_add(MAX_CLOCK_SKEW_MS)
            {
                return Err(AgentError::LimitExceeded(
                    "Provider log output exceeded protocol limits.".to_owned(),
                ));
            }
            let logs = entries
                .into_iter()
                .enumerate()
                .map(|(index, message)| RemoteLogEntry {
                    // Provider V0.9 returns strings only. These are explicitly
                    // batch-local ordinals and service observation timestamps,
                    // not fabricated provider timestamps or severity claims.
                    sequence: (index as u64) + 1,
                    observed_at_unix_ms: now,
                    level: RemoteLogLevel::Info,
                    message,
                })
                .collect();
            Ok(OperationResult {
                operation,
                silo_id,
                binding_id: expected_binding_id,
                remote_environment_id: expected_remote_environment_id,
                server_id,
                last_activity_at_unix_ms,
                state: RemoteResultState::LogsReturned,
                volume: None,
                evidence: None,
                logs: Some(logs),
                next_cursor: None,
                deletion_proof: None,
            })
        }
        _ => Err(AgentError::InvalidState(
            "Agent returned the wrong response variant.".to_owned(),
        )),
    }
}

fn result_state(operation: RemoteOperation) -> RemoteResultState {
    match operation {
        RemoteOperation::Create => RemoteResultState::Created,
        RemoteOperation::Start => RemoteResultState::Started,
        RemoteOperation::Stop => RemoteResultState::Stopped,
        RemoteOperation::Pause => RemoteResultState::Paused,
        RemoteOperation::Snapshot => RemoteResultState::SnapshotCreated,
        RemoteOperation::Destroy => RemoteResultState::Destroyed,
        RemoteOperation::ConfigureNetwork => RemoteResultState::NetworkConfigured,
        RemoteOperation::Health => RemoteResultState::Healthy,
        RemoteOperation::Logs => RemoteResultState::LogsReturned,
    }
}

fn operation_error_body(operation: RemoteOperation, error: AgentError) -> OperationResponseBody {
    if matches!(&error, AgentError::Unavailable(_)) {
        return OperationResponseBody::Unavailable {
            operation,
            reason: "The operation is unavailable in local provider configuration.".to_owned(),
        };
    }
    let (code, message) = map_agent_error(error);
    OperationResponseBody::Rejected {
        code,
        message: message.to_owned(),
    }
}

fn map_agent_error(error: AgentError) -> (RejectionCode, &'static str) {
    match error {
        AgentError::VersionMismatch | AgentError::InvalidRequest(_) | AgentError::Json(_) => (
            RejectionCode::InvalidRequest,
            "The typed Agent request is invalid.",
        ),
        AgentError::Stale => (
            RejectionCode::StaleRequest,
            "The typed Agent request timestamp is stale.",
        ),
        AgentError::Replay => (
            RejectionCode::Replay,
            "The typed Agent request was already consumed.",
        ),
        AgentError::Unauthorized(_) => (
            RejectionCode::Unauthorized,
            "The typed Agent request is not authorized.",
        ),
        AgentError::Provider(message) if message == "Required proxy evidence failed closed." => (
            RejectionCode::ProxyUnverified,
            "The required proxy evidence did not verify.",
        ),
        AgentError::Conflict(_)
        | AgentError::NotFound
        | AgentError::Expired
        | AgentError::InvalidState(_)
        | AgentError::Provider(_)
        | AgentError::Store(_) => (
            RejectionCode::InvalidState,
            "The local Agent cannot perform this operation in its current state.",
        ),
        AgentError::LimitExceeded(_) => (
            RejectionCode::LimitExceeded,
            "The operation exceeded a fixed local limit.",
        ),
        AgentError::Unavailable(_) => (
            RejectionCode::InvalidState,
            "The operation is unavailable in local provider configuration.",
        ),
    }
}

fn is_interactive_command(command: &AgentCommand) -> bool {
    matches!(
        command,
        AgentCommand::OpenHumanSession { .. }
            | AgentCommand::CloseHumanSession { .. }
            | AgentCommand::GrantAutomation { .. }
            | AgentCommand::RevokeAutomation { .. }
            | AgentCommand::OpenScreen { .. }
            | AgentCommand::SendInput { .. }
    )
}

#[derive(Clone, Copy)]
enum ExpectedAgentResponse {
    HumanSession,
    Automation,
    Screen,
    InputAccepted(usize),
}

impl ExpectedAgentResponse {
    fn from_command(command: &AgentCommand) -> Self {
        match command {
            AgentCommand::OpenHumanSession { .. } | AgentCommand::CloseHumanSession { .. } => {
                Self::HumanSession
            }
            AgentCommand::GrantAutomation { .. } | AgentCommand::RevokeAutomation { .. } => {
                Self::Automation
            }
            AgentCommand::OpenScreen { .. } => Self::Screen,
            AgentCommand::SendInput { events, .. } => Self::InputAccepted(events.len()),
            _ => unreachable!("caller filters interactive Agent commands"),
        }
    }

    fn matches(self, response: &AgentResponse) -> bool {
        match (self, response) {
            (Self::HumanSession, AgentResponse::HumanSession { .. })
            | (Self::Automation, AgentResponse::Automation { .. })
            | (Self::Screen, AgentResponse::Screen { .. }) => true,
            (Self::InputAccepted(expected), AgentResponse::InputAccepted { event_count }) => {
                expected == *event_count
            }
            _ => false,
        }
    }
}

fn internal_error<T>(_error: T) -> ServiceResponse {
    ServiceResponse::fixed_error(500, "internal_error")
}
