#![cfg(unix)]

#[allow(dead_code)]
#[path = "../src/agent_service.rs"]
mod agent_service;
#[allow(dead_code)]
#[path = "../src/auth_store.rs"]
mod auth_store;

use std::{
    cell::RefCell,
    fs,
    path::PathBuf,
    rc::Rc,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
};

use agent_service::AgentService;
use auth_store::AuthStore;
use uuid::Uuid;
use verisilo_remote_backend::{
    agent::{
        AgentCore, AgentError, AgentProvider, AgentRequest, AgentResponse, AgentStore,
        CostDisclosure, DeletionResourceKind, DeletionResourceStatus, EnvironmentRecord,
        EnvironmentState, KeyCustody, LifecycleReceipt, MemoryAgentStore, NodeDisclosure,
        NodeOwnership, Principal, PrincipalKind, ProviderDeletionReceipt, ProvisionReceipt,
        ResourceDeletionItem, ScreenChannel, ScreenTransport, VolumeAttestation,
    },
    AgentControlResponseBody, AgentResponseEnvelope, CapabilityAvailability, Clock, DnsEvidence,
    EndpointOwnership, EvidenceCheckState, ExitEvidence, GuestEvidence, GuestEvidenceSource,
    GuestHealthEvidence, GuestHealthState, MemoryBindingStore, OperationBody,
    OperationRequestEnvelope, OperationResponseBody, OperationResponseEnvelope, PairingOperation,
    PairingRejectionCode, PairingRequestBody, PairingRequestEnvelope, PairingResponseBody,
    PairingResponseEnvelope, PairingSnapshot, ProxyEvidence, ProxyEvidenceState, RejectionCode,
    RemoteBackendError, RemoteCapability, RemoteEndpoint, RemoteEnvironmentBackend,
    RemoteNetworkPolicy, RemoteOperation, RemoteTransport, TlsPin, TlsPinKind,
    TlsPinRotationAuthorizationBody, TlsPinRotationAuthorizationRequestEnvelope,
    TlsPinRotationAuthorizationResponseBody, TlsPinRotationAuthorizationResponseEnvelope,
    TlsPinRotationOperation, TlsPinRotationPairingClaim, TransportRequest, TransportResponse,
    WebRtcEvidence, PROTOCOL_VERSION,
};

const NOW: u64 = 1_800_000_000_000;

#[derive(Clone)]
struct TestClock(Arc<AtomicU64>);

impl TestClock {
    fn new(now: u64) -> Self {
        Self(Arc::new(AtomicU64::new(now)))
    }

    fn set(&self, now: u64) {
        self.0.store(now, Ordering::SeqCst);
    }
}

impl Clock for TestClock {
    fn now_unix_ms(&self) -> u64 {
        self.0.load(Ordering::SeqCst)
    }
}

struct FakeProvider {
    capabilities: Vec<RemoteCapability>,
    evidence_sequence: u64,
}

impl FakeProvider {
    fn evidence(&mut self, record: &EnvironmentRecord) -> GuestEvidence {
        self.evidence_sequence += 1;
        GuestEvidence {
            protocol_version: PROTOCOL_VERSION,
            evidence_id: Uuid::new_v4(),
            binding_id: record.binding_id,
            remote_environment_id: record.remote_environment_id,
            source: GuestEvidenceSource::GuestAgent,
            sequence: self.evidence_sequence,
            observed_at_unix_ms: record.last_activity_at_unix_ms,
            proxy: ProxyEvidence {
                state: ProxyEvidenceState::NotRequired,
                policy_id: None,
            },
            exit: ExitEvidence {
                state: EvidenceCheckState::Verified,
                public_addresses: vec!["203.0.113.10".to_owned()],
            },
            dns: DnsEvidence {
                state: EvidenceCheckState::Verified,
                resolvers: vec!["9.9.9.9".to_owned()],
                leak_detected: false,
            },
            web_rtc: WebRtcEvidence {
                state: EvidenceCheckState::Verified,
                observed_candidates: Vec::new(),
                leak_detected: false,
            },
            health: GuestHealthEvidence {
                state: GuestHealthState::Healthy,
                agent_version: "test-agent-1".to_owned(),
                checks: vec!["guest_agent".to_owned()],
            },
        }
    }
}

impl AgentProvider for FakeProvider {
    fn capabilities(&self) -> Vec<RemoteCapability> {
        self.capabilities.clone()
    }

    fn create(&mut self, record: &EnvironmentRecord) -> Result<ProvisionReceipt, AgentError> {
        Ok(ProvisionReceipt {
            volume: VolumeAttestation {
                encrypted: true,
                key_custody: KeyCustody::UserControlled,
                volume_id: Uuid::new_v4(),
                key_id: Uuid::new_v4(),
            },
            evidence: self.evidence(record),
        })
    }

    fn lifecycle(
        &mut self,
        operation: RemoteOperation,
        record: &EnvironmentRecord,
        _log_limit: Option<u16>,
    ) -> Result<LifecycleReceipt, AgentError> {
        if operation == RemoteOperation::Logs {
            return Ok(LifecycleReceipt {
                evidence: None,
                logs: vec!["provider log".to_owned()],
            });
        }
        Ok(LifecycleReceipt {
            evidence: Some(self.evidence(record)),
            logs: Vec::new(),
        })
    }

    fn destroy(
        &mut self,
        record: &EnvironmentRecord,
    ) -> Result<ProviderDeletionReceipt, AgentError> {
        Ok(ProviderDeletionReceipt {
            receipt_id: Uuid::new_v4(),
            remote_environment_id: record.remote_environment_id,
            volume_id: record.volume.volume_id,
            resource_deletions: vec![
                ResourceDeletionItem {
                    kind: DeletionResourceKind::ComputeInstance,
                    resource_id: Some(record.remote_environment_id),
                    status: DeletionResourceStatus::Deleted,
                },
                ResourceDeletionItem {
                    kind: DeletionResourceKind::PersistentVolume,
                    resource_id: Some(record.volume.volume_id),
                    status: DeletionResourceStatus::Deleted,
                },
                ResourceDeletionItem {
                    kind: DeletionResourceKind::Snapshot,
                    resource_id: None,
                    status: DeletionResourceStatus::NotApplicable,
                },
                ResourceDeletionItem {
                    kind: DeletionResourceKind::EphemeralKey,
                    resource_id: Some(record.volume.key_id),
                    status: DeletionResourceStatus::Deleted,
                },
            ],
        })
    }

    fn open_screen(
        &mut self,
        record: &EnvironmentRecord,
        authorization_id: Uuid,
        expires_at_unix_ms: u64,
    ) -> Result<ScreenChannel, AgentError> {
        Ok(ScreenChannel {
            channel_id: Uuid::new_v4(),
            remote_environment_id: record.remote_environment_id,
            authorization_id,
            expires_at_unix_ms,
            transport: ScreenTransport::AuthenticatedEncryptedStream,
        })
    }

    fn send_input(
        &mut self,
        _record: &EnvironmentRecord,
        _authorization_id: Uuid,
        _events: &[verisilo_remote_backend::agent::InputEvent],
    ) -> Result<(), AgentError> {
        Ok(())
    }
}

type TestService = AgentService<AgentCore<FakeProvider, MemoryAgentStore, TestClock>, TestClock>;

#[derive(Clone, Copy)]
enum DestroyResponseTamper {
    Server,
    Environment,
    ProofId,
    Reason,
    MissingResource,
    DuplicateResource,
    UnknownKind,
    UnknownStatus,
}

struct InProcessServiceTransport {
    service: Rc<RefCell<TestService>>,
    pin: TlsPin,
    tamper: Option<DestroyResponseTamper>,
}

impl RemoteTransport for InProcessServiceTransport {
    fn exchange(
        &mut self,
        request: TransportRequest<'_>,
    ) -> Result<TransportResponse, RemoteBackendError> {
        let response = self
            .service
            .borrow_mut()
            .route(request.credential, request.payload);
        if response.status_code() != 200 {
            return Err(RemoteBackendError::Protocol(
                "In-process Agent returned an HTTP error.".to_owned(),
            ));
        }
        let mut payload = response.body().to_vec();
        if let Some(tamper) = self.tamper {
            let mut value: serde_json::Value = serde_json::from_slice(&payload)?;
            if value["body"]["status"] == "success"
                && value["body"]["result"]["operation"] == "destroy"
            {
                let result = &mut value["body"]["result"];
                match tamper {
                    DestroyResponseTamper::Server => {
                        result["serverId"] = serde_json::json!(Uuid::new_v4());
                    }
                    DestroyResponseTamper::Environment => {
                        result["remoteEnvironmentId"] = serde_json::json!(Uuid::new_v4());
                    }
                    DestroyResponseTamper::ProofId => {
                        result["deletionProof"]["proofId"] = serde_json::json!(Uuid::nil());
                    }
                    DestroyResponseTamper::Reason => {
                        result["deletionProof"]["reason"] = serde_json::json!("forged_reason");
                    }
                    DestroyResponseTamper::MissingResource => {
                        result["deletionProof"]["resourceDeletions"]
                            .as_array_mut()
                            .unwrap()
                            .pop();
                    }
                    DestroyResponseTamper::DuplicateResource => {
                        let resources = result["deletionProof"]["resourceDeletions"]
                            .as_array_mut()
                            .unwrap();
                        resources[2] = resources[0].clone();
                    }
                    DestroyResponseTamper::UnknownKind => {
                        result["deletionProof"]["resourceDeletions"][2]["kind"] =
                            serde_json::json!("unknown_kind");
                    }
                    DestroyResponseTamper::UnknownStatus => {
                        result["deletionProof"]["resourceDeletions"][2]["status"] =
                            serde_json::json!("unknown_status");
                    }
                }
                payload = serde_json::to_vec(&value)?;
            }
        }
        Ok(TransportResponse {
            tls_validated: true,
            peer_pin: self.pin.clone(),
            payload,
        })
    }
}

fn root(label: &str) -> PathBuf {
    let root =
        std::env::temp_dir().join(format!("verisilo-service-test-{label}-{}", Uuid::new_v4()));
    fs::create_dir(&root).unwrap();
    root
}

fn nonce() -> String {
    Uuid::new_v4().simple().to_string()
}

fn capabilities(available: bool) -> Vec<RemoteCapability> {
    RemoteOperation::ALL
        .into_iter()
        .map(|operation| RemoteCapability {
            operation,
            availability: if available {
                CapabilityAvailability::Available
            } else {
                CapabilityAvailability::Unavailable {
                    reason: "No fixed provider artifact is configured.".to_owned(),
                }
            },
        })
        .collect()
}

fn node() -> NodeDisclosure {
    NodeDisclosure {
        node_id: Uuid::new_v4(),
        ownership: NodeOwnership::UserSelfHosted,
        operator_label: "Test operator".to_owned(),
        data_region: "test-region".to_owned(),
        key_custody: KeyCustody::UserControlled,
        cost: CostDisclosure {
            currency: "USD".to_owned(),
            estimated_micros_per_hour: 1,
            notice: "Test-only provider cost.".to_owned(),
        },
    }
}

fn paired_service(
    label: &str,
    provider_capabilities: Vec<RemoteCapability>,
) -> (TestService, String, Uuid, TestClock, PathBuf) {
    let root = root(label);
    let mut auth = AuthStore::open(root.join("auth.json")).unwrap();
    let token = auth.issue_pairing_token(NOW, 300_000).unwrap();
    let provider = FakeProvider {
        capabilities: provider_capabilities.clone(),
        evidence_sequence: 0,
    };
    let clock = TestClock::new(NOW);
    let core =
        AgentCore::new(node(), provider, MemoryAgentStore::default(), clock.clone()).unwrap();
    let service_node = core.node().clone();
    let mut service = AgentService::new(
        auth,
        core,
        service_node,
        provider_capabilities,
        clock.clone(),
        3_600_000,
    )
    .unwrap();
    let request = PairingRequestEnvelope {
        protocol_version: PROTOCOL_VERSION,
        request_id: Uuid::new_v4(),
        nonce: nonce(),
        sent_at_unix_ms: NOW,
        body: PairingRequestBody {
            operation: PairingOperation::Pair,
            approved_by_user: true,
            pairing_token_id: token.token_id(),
            pairing_token: token.secret().to_owned(),
            pairing_token_expires_at_unix_ms: token.expires_at_unix_ms(),
            tls_pin_rotation: None,
        },
    };
    let response = service.route(None, &serde_json::to_vec(&request).unwrap());
    assert_eq!(response.status_code(), 200);
    let envelope: PairingResponseEnvelope = serde_json::from_slice(response.body()).unwrap();
    let (credential, credential_id) = match &envelope.body {
        PairingResponseBody::Success {
            client_credential,
            client_credential_id,
            ..
        } => (client_credential.clone(), *client_credential_id),
        PairingResponseBody::Rejected { .. } => panic!("pairing unexpectedly rejected"),
    };
    (service, credential, credential_id, clock, root)
}

fn paired_backend(
    label: &str,
    tamper: Option<DestroyResponseTamper>,
) -> (
    RemoteEnvironmentBackend<InProcessServiceTransport, MemoryBindingStore, TestClock>,
    Rc<RefCell<TestService>>,
    TestClock,
    PathBuf,
) {
    let provider_capabilities = capabilities(true);
    let (service, credential, credential_id, clock, root) =
        paired_service(label, provider_capabilities.clone());
    let server_id = service.server_id();
    let service = Rc::new(RefCell::new(service));
    let pin = TlsPin {
        kind: TlsPinKind::SpkiSha256,
        sha256: "a".repeat(64),
    };
    let endpoint = RemoteEndpoint {
        ownership: EndpointOwnership::UserSelfHosted,
        origin: "https://agent.example.test".to_owned(),
        pin: pin.clone(),
    };
    let transport = InProcessServiceTransport {
        service: service.clone(),
        pin,
        tamper,
    };
    let mut backend = RemoteEnvironmentBackend::new(
        endpoint,
        transport,
        MemoryBindingStore::default(),
        clock.clone(),
    )
    .unwrap();
    backend
        .restore_pairing(PairingSnapshot {
            server_id,
            client_credential_id: credential_id,
            node: node(),
            client_credential: credential,
            credential_expires_at_unix_ms: NOW + 3_600_000,
            capabilities: provider_capabilities,
            last_client_sequence: 0,
            last_server_sequence: 1,
        })
        .unwrap();
    (backend, service, clock, root)
}

fn create_request(silo_id: Uuid, sequence: u64, ttl_seconds: u64) -> OperationRequestEnvelope {
    OperationRequestEnvelope {
        protocol_version: PROTOCOL_VERSION,
        request_id: Uuid::new_v4(),
        nonce: nonce(),
        sequence,
        sent_at_unix_ms: NOW,
        silo_id,
        body: OperationBody::Create {
            network: RemoteNetworkPolicy::Direct,
            ttl_seconds,
            cost_acknowledged: true,
        },
    }
}

fn operation_response(response: &agent_service::ServiceResponse) -> OperationResponseEnvelope {
    assert_eq!(response.status_code(), 200);
    serde_json::from_slice(response.body()).unwrap()
}

#[test]
fn pin_rotation_requires_old_bearer_and_consumes_bound_challenge_with_same_token() {
    let (mut service, credential, credential_id, _clock, root) =
        paired_service("pin-rotation", capabilities(true));
    let server_id = service.server_id();
    let token = service
        .auth_for_test_mut()
        .issue_pairing_token(NOW, 300_000)
        .unwrap();
    let new_pin = TlsPin {
        kind: TlsPinKind::SpkiSha256,
        sha256: "b".repeat(64),
    };
    let authorization = TlsPinRotationAuthorizationRequestEnvelope {
        protocol_version: PROTOCOL_VERSION,
        request_id: Uuid::new_v4(),
        nonce: nonce(),
        sequence: 1,
        sent_at_unix_ms: NOW,
        body: TlsPinRotationAuthorizationBody {
            operation: TlsPinRotationOperation::AuthorizeTlsPinRotation,
            client_credential_id: credential_id,
            pairing_token_id: token.token_id(),
            new_pin: new_pin.clone(),
        },
    };
    let authorization_raw = serde_json::to_vec(&authorization).unwrap();
    assert!(!String::from_utf8_lossy(&authorization_raw).contains(token.secret()));

    let mut unknown: serde_json::Value = serde_json::from_slice(&authorization_raw).unwrap();
    unknown["body"]["pairingToken"] = serde_json::json!(token.secret());
    assert_eq!(
        service
            .route(Some(&credential), &serde_json::to_vec(&unknown).unwrap())
            .status_code(),
        400
    );

    let response = service.route(Some(&credential), &authorization_raw);
    let response: TlsPinRotationAuthorizationResponseEnvelope =
        serde_json::from_slice(response.body()).unwrap();
    let authorization_response_sequence = response.sequence;
    let claim = match response.body {
        TlsPinRotationAuthorizationResponseBody::Success {
            server_id: response_server_id,
            client_credential_id,
            pairing_token_id,
            new_pin: response_pin,
            challenge,
            authorization_expires_at_unix_ms,
        } => {
            assert_eq!(response_server_id, server_id);
            assert_eq!(client_credential_id, credential_id);
            assert_eq!(pairing_token_id, token.token_id());
            assert_eq!(response_pin, new_pin);
            assert!(authorization_expires_at_unix_ms > NOW);
            TlsPinRotationPairingClaim {
                challenge,
                server_id,
                old_client_credential_id: credential_id,
                authorization_request_id: authorization.request_id,
                authorization_request_nonce: authorization.nonce.clone(),
                authorization_request_sequence: authorization.sequence,
                authorization_response_sequence,
                authorization_expires_at_unix_ms,
                pairing_token_id,
                new_pin: response_pin,
            }
        }
        TlsPinRotationAuthorizationResponseBody::Rejected { .. } => {
            panic!("old credential authorization unexpectedly rejected")
        }
    };
    let replayed_authorization = service.route(Some(&credential), &authorization_raw);
    let replayed_authorization: TlsPinRotationAuthorizationResponseEnvelope =
        serde_json::from_slice(replayed_authorization.body()).unwrap();
    assert!(matches!(
        replayed_authorization.body,
        TlsPinRotationAuthorizationResponseBody::Rejected {
            code: RejectionCode::Replay,
            ..
        }
    ));

    let mut wrong_claim = claim.clone();
    wrong_claim.challenge = nonce();
    let wrong_pairing = PairingRequestEnvelope {
        protocol_version: PROTOCOL_VERSION,
        request_id: Uuid::new_v4(),
        nonce: nonce(),
        sent_at_unix_ms: NOW,
        body: PairingRequestBody {
            operation: PairingOperation::Pair,
            approved_by_user: true,
            pairing_token_id: token.token_id(),
            pairing_token: token.secret().to_owned(),
            pairing_token_expires_at_unix_ms: token.expires_at_unix_ms(),
            tls_pin_rotation: Some(wrong_claim),
        },
    };
    let rejected = service.route(None, &serde_json::to_vec(&wrong_pairing).unwrap());
    let rejected: PairingResponseEnvelope = serde_json::from_slice(rejected.body()).unwrap();
    assert!(matches!(
        rejected.body,
        PairingResponseBody::Rejected {
            code: PairingRejectionCode::RotationAuthorizationInvalid,
            ..
        }
    ));

    let pairing = PairingRequestEnvelope {
        protocol_version: PROTOCOL_VERSION,
        request_id: Uuid::new_v4(),
        nonce: nonce(),
        sent_at_unix_ms: NOW,
        body: PairingRequestBody {
            operation: PairingOperation::Pair,
            approved_by_user: true,
            pairing_token_id: token.token_id(),
            pairing_token: token.secret().to_owned(),
            pairing_token_expires_at_unix_ms: token.expires_at_unix_ms(),
            tls_pin_rotation: Some(claim),
        },
    };
    let pairing_raw = serde_json::to_vec(&pairing).unwrap();
    let paired = service.route(None, &pairing_raw);
    let paired: PairingResponseEnvelope = serde_json::from_slice(paired.body()).unwrap();
    assert!(matches!(
        paired.body,
        PairingResponseBody::Success {
            server_id: response_server_id,
            ..
        } if response_server_id == server_id
    ));

    let replay = service.route(None, &pairing_raw);
    let replay: PairingResponseEnvelope = serde_json::from_slice(replay.body()).unwrap();
    assert!(matches!(
        replay.body,
        PairingResponseBody::Rejected {
            code: PairingRejectionCode::Replay,
            ..
        }
    ));

    drop(service);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn lifecycle_routes_preserve_client_sequence_binding_and_replay() {
    let (mut service, credential, _credential_id, _clock, root) =
        paired_service("lifecycle", capabilities(true));
    let silo_id = Uuid::new_v4();
    let create = create_request(silo_id, 1, 3_600);
    let create_response = operation_response(
        &service.route(Some(&credential), &serde_json::to_vec(&create).unwrap()),
    );
    let (binding_id, remote_environment_id) = match create_response.body {
        OperationResponseBody::Success { result } => {
            assert!(result
                .volume
                .as_ref()
                .is_some_and(|volume| volume.encrypted));
            (result.binding_id, result.remote_environment_id)
        }
        _ => panic!("create unexpectedly failed"),
    };
    assert_eq!(
        service
            .agent_for_test()
            .store()
            .environment(silo_id)
            .unwrap()
            .binding_id,
        binding_id
    );

    let start = OperationRequestEnvelope {
        protocol_version: PROTOCOL_VERSION,
        request_id: Uuid::new_v4(),
        nonce: nonce(),
        sequence: 2,
        sent_at_unix_ms: NOW,
        silo_id,
        body: OperationBody::Start {
            binding_id,
            remote_environment_id,
        },
    };
    assert!(matches!(
        operation_response(&service.route(Some(&credential), &serde_json::to_vec(&start).unwrap()))
            .body,
        OperationResponseBody::Success { .. }
    ));
    let replay =
        operation_response(&service.route(Some(&credential), &serde_json::to_vec(&start).unwrap()));
    assert!(matches!(
        replay.body,
        OperationResponseBody::Rejected {
            code: RejectionCode::Replay,
            ..
        }
    ));

    let wrong_binding = OperationRequestEnvelope {
        protocol_version: PROTOCOL_VERSION,
        request_id: Uuid::new_v4(),
        nonce: nonce(),
        sequence: 3,
        sent_at_unix_ms: NOW,
        silo_id,
        body: OperationBody::Health {
            binding_id: Uuid::new_v4(),
            remote_environment_id,
        },
    };
    assert!(matches!(
        operation_response(&service.route(
            Some(&credential),
            &serde_json::to_vec(&wrong_binding).unwrap()
        ))
        .body,
        OperationResponseBody::Rejected {
            code: RejectionCode::InvalidRequest,
            ..
        }
    ));
    let mut corrected = wrong_binding.clone();
    corrected.body = OperationBody::Health {
        binding_id,
        remote_environment_id,
    };
    assert!(matches!(
        operation_response(
            &service.route(Some(&credential), &serde_json::to_vec(&corrected).unwrap())
        )
        .body,
        OperationResponseBody::Rejected {
            code: RejectionCode::Replay,
            ..
        }
    ));
    drop(service);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn agent_channel_binds_bearer_credential_id_and_returns_typed_envelope() {
    let (mut service, credential, credential_id, _clock, root) =
        paired_service("agent", capabilities(true));
    let silo_id = Uuid::new_v4();
    let create = create_request(silo_id, 1, 3_600);
    assert!(matches!(
        operation_response(
            &service.route(Some(&credential), &serde_json::to_vec(&create).unwrap())
        )
        .body,
        OperationResponseBody::Success { .. }
    ));

    let mismatched = AgentRequest {
        protocol_version: PROTOCOL_VERSION,
        request_id: Uuid::new_v4(),
        nonce: nonce(),
        sequence: 2,
        sent_at_unix_ms: NOW,
        principal: Principal {
            kind: PrincipalKind::ControlPlane,
            credential_id: Uuid::new_v4(),
            authorization_id: None,
        },
        command: verisilo_remote_backend::agent::AgentCommand::OpenHumanSession {
            silo_id,
            lifetime_seconds: 300,
        },
    };
    let rejected: AgentResponseEnvelope = serde_json::from_slice(
        service
            .route(Some(&credential), &serde_json::to_vec(&mismatched).unwrap())
            .body(),
    )
    .unwrap();
    assert!(matches!(
        rejected.body,
        AgentControlResponseBody::Rejected {
            code: RejectionCode::Unauthorized,
            ..
        }
    ));

    let mut accepted = mismatched;
    accepted.request_id = Uuid::new_v4();
    accepted.nonce = nonce();
    accepted.sequence = 3;
    accepted.principal.credential_id = credential_id;
    let response = service.route(Some(&credential), &serde_json::to_vec(&accepted).unwrap());
    let envelope: AgentResponseEnvelope = serde_json::from_slice(response.body()).unwrap();
    assert!(matches!(
        envelope.body,
        AgentControlResponseBody::Success {
            response: AgentResponse::HumanSession { .. }
        }
    ));
    drop(service);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn unavailable_provider_is_reported_honestly_without_execution() {
    let (mut service, credential, _credential_id, _clock, root) =
        paired_service("unavailable", capabilities(false));
    let request = create_request(Uuid::new_v4(), 1, 3_600);
    let response = operation_response(
        &service.route(Some(&credential), &serde_json::to_vec(&request).unwrap()),
    );
    assert!(matches!(
        response.body,
        OperationResponseBody::Unavailable {
            operation: RemoteOperation::Create,
            ..
        }
    ));
    drop(service);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn logs_and_destroy_map_to_bounded_logs_and_bound_deletion_proof() {
    let (mut service, credential, _credential_id, _clock, root) =
        paired_service("result-mapping", capabilities(true));
    let silo_id = Uuid::new_v4();
    let create = operation_response(&service.route(
        Some(&credential),
        &serde_json::to_vec(&create_request(silo_id, 1, 3_600)).unwrap(),
    ));
    let (binding_id, remote_environment_id) = match create.body {
        OperationResponseBody::Success { result } => {
            (result.binding_id, result.remote_environment_id)
        }
        _ => panic!("create unexpectedly failed"),
    };

    let logs = OperationRequestEnvelope {
        protocol_version: PROTOCOL_VERSION,
        request_id: Uuid::new_v4(),
        nonce: nonce(),
        sequence: 2,
        sent_at_unix_ms: NOW,
        silo_id,
        body: OperationBody::Logs {
            binding_id,
            remote_environment_id,
            cursor: None,
            limit: 10,
        },
    };
    let logs =
        operation_response(&service.route(Some(&credential), &serde_json::to_vec(&logs).unwrap()));
    match logs.body {
        OperationResponseBody::Success { result } => {
            let entries = result.logs.expect("logs result must contain entries");
            assert_eq!(entries.len(), 1);
            assert_eq!(entries[0].sequence, 1);
            assert_eq!(entries[0].observed_at_unix_ms, NOW);
            assert!(result.next_cursor.is_none());
        }
        _ => panic!("logs unexpectedly failed"),
    }

    let destroy = OperationRequestEnvelope {
        protocol_version: PROTOCOL_VERSION,
        request_id: Uuid::new_v4(),
        nonce: nonce(),
        sequence: 3,
        sent_at_unix_ms: NOW,
        silo_id,
        body: OperationBody::Destroy {
            binding_id,
            remote_environment_id,
            confirm_destroy: true,
        },
    };
    let destroy = operation_response(
        &service.route(Some(&credential), &serde_json::to_vec(&destroy).unwrap()),
    );
    match destroy.body {
        OperationResponseBody::Success { result } => {
            let proof = result
                .deletion_proof
                .expect("destroy result must contain proof");
            assert_eq!(proof.silo_id, silo_id);
            assert_eq!(proof.binding_id, binding_id);
            assert_eq!(proof.remote_environment_id, remote_environment_id);
            assert_eq!(proof.resource_deletions.len(), 4);
        }
        _ => panic!("destroy unexpectedly failed"),
    }
    drop(service);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn maintenance_tick_deletes_expired_environment_without_client_traffic() {
    let (mut service, credential, _credential_id, clock, root) =
        paired_service("maintenance", capabilities(true));
    let silo_id = Uuid::new_v4();
    let request = create_request(silo_id, 1, 60);
    assert!(matches!(
        operation_response(
            &service.route(Some(&credential), &serde_json::to_vec(&request).unwrap())
        )
        .body,
        OperationResponseBody::Success { .. }
    ));
    clock.set(NOW + 61_000);
    service.maintenance_tick();
    let record = service
        .agent_for_test()
        .store()
        .environment(silo_id)
        .unwrap();
    assert_eq!(record.state, EnvironmentState::Deleted);
    assert!(record.deletion_proof_id.is_some());
    drop(service);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn ttl_sweep_proof_is_recovered_by_client_without_new_delete_confirmation() {
    let (mut backend, service, clock, root) = paired_backend("ttl-recovery", None);
    let silo_id = Uuid::new_v4();
    let created = backend
        .create(silo_id, RemoteNetworkPolicy::Direct, 60, true)
        .unwrap();
    assert_eq!(created.last_activity_at_unix_ms, NOW);
    assert_eq!(
        backend
            .binding(silo_id)
            .unwrap()
            .unwrap()
            .last_activity_at_unix_ms,
        NOW
    );

    clock.set(NOW + 61_000);
    service.borrow_mut().maintenance_tick();
    let recovered = backend
        .destroy(silo_id, false)
        .expect("already-deleted environment returns its durable proof");
    let proof = recovered.deletion_proof.as_ref().unwrap();
    assert_eq!(
        proof.reason,
        verisilo_remote_backend::agent::DeletionReason::TtlExpired
    );
    assert_eq!(proof.deleted_at_unix_ms, NOW + 61_000);
    assert_eq!(recovered.last_activity_at_unix_ms, proof.deleted_at_unix_ms);
    assert_eq!(recovered.server_id, service.borrow().server_id());
    assert_eq!(proof.resource_deletions.len(), 4);
    assert!(backend.binding(silo_id).unwrap().is_none());

    drop(backend);
    drop(service);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn live_destroy_still_requires_confirmation_before_provider_mutation() {
    let (mut backend, service, _clock, root) = paired_backend("live-confirmation", None);
    let silo_id = Uuid::new_v4();
    backend
        .create(silo_id, RemoteNetworkPolicy::Direct, 60, true)
        .unwrap();
    assert!(matches!(
        backend.destroy(silo_id, false),
        Err(RemoteBackendError::Rejected {
            code: RejectionCode::InvalidRequest,
            ..
        })
    ));
    assert!(backend.binding(silo_id).unwrap().is_some());
    let destroyed = backend.destroy(silo_id, true).unwrap();
    assert_eq!(
        destroyed.deletion_proof.unwrap().reason,
        verisilo_remote_backend::agent::DeletionReason::UserConfirmed
    );
    assert!(backend.binding(silo_id).unwrap().is_none());

    drop(backend);
    drop(service);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn client_rejects_forged_or_incomplete_recovered_deletion_receipts() {
    let cases = [
        ("server", DestroyResponseTamper::Server),
        ("environment", DestroyResponseTamper::Environment),
        ("proof-id", DestroyResponseTamper::ProofId),
        ("reason", DestroyResponseTamper::Reason),
        ("missing-resource", DestroyResponseTamper::MissingResource),
        (
            "duplicate-resource",
            DestroyResponseTamper::DuplicateResource,
        ),
        ("unknown-kind", DestroyResponseTamper::UnknownKind),
        ("unknown-status", DestroyResponseTamper::UnknownStatus),
    ];
    for (label, tamper) in cases {
        let (mut backend, service, clock, root) =
            paired_backend(&format!("tamper-{label}"), Some(tamper));
        let silo_id = Uuid::new_v4();
        backend
            .create(silo_id, RemoteNetworkPolicy::Direct, 60, true)
            .unwrap();
        clock.set(NOW + 61_000);
        service.borrow_mut().maintenance_tick();
        assert!(backend.destroy(silo_id, false).is_err(), "accepted {label}");
        assert!(
            backend.binding(silo_id).unwrap().is_some(),
            "removed binding after accepting {label}"
        );
        drop(backend);
        drop(service);
        fs::remove_dir_all(root).unwrap();
    }
}

#[test]
fn service_rejects_create_without_destroy_capability() {
    let root = root("capability-invariant");
    let auth = AuthStore::open(root.join("auth.json")).unwrap();
    let mut advertised = capabilities(true);
    advertised
        .iter_mut()
        .find(|capability| capability.operation == RemoteOperation::Destroy)
        .unwrap()
        .availability = CapabilityAvailability::Unavailable {
        reason: "Destroy adapter is not configured.".to_owned(),
    };
    let clock = TestClock::new(NOW);
    let core = AgentCore::new(
        node(),
        FakeProvider {
            capabilities: advertised.clone(),
            evidence_sequence: 0,
        },
        MemoryAgentStore::default(),
        clock.clone(),
    )
    .unwrap();
    let service_node = core.node().clone();
    assert!(AgentService::new(auth, core, service_node, advertised, clock, 3_600_000,).is_err());
    fs::remove_dir_all(root).unwrap();
}
