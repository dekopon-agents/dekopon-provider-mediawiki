//! Real-component policy and secret authority checks for the fixed Wikipedia HTTP surface.
//! No test in this module makes a production network request.
use dekopon_broker::{
    AuthenticatedContext, Broker, BrokerLimits, CapabilityRoute, ConstraintCatalog, ConstraintSet,
    CredentialStore, IdentityDirectory, InMemoryAuditLog, InvocationRequest, PolicyEngine,
    PolicyWorld,
};
use dekopon_broker_host::{BrokerHostLimits, BrokerProviderRegistry, asset::AssetInputs};
use dekopon_broker_protocol::TraceParent;
use dekopon_capability::{EffectKind, ExecutionConstraints, HttpConstraints, InvocationOutcome};
use dekopon_core::{Actor, AgentId, CapabilityId, PrincipalId, SecretUseProposal};
use serde_json::{Value, json};
use std::{path::PathBuf, sync::Arc};

fn component() -> PathBuf {
    std::env::var_os("DEKOPON_PROVIDER_COMPONENT")
        .expect("DEKOPON_PROVIDER_COMPONENT must point to a fresh component")
        .into()
}
fn capability() -> CapabilityId {
    "mediawiki.search".parse().expect("fixed ID")
}
fn principal(value: &str) -> PrincipalId {
    value.parse().expect("fixture principal")
}
fn context(value: &str) -> AuthenticatedContext {
    AuthenticatedContext::attested(
        principal(value),
        Actor::Agent {
            agent: "mediawiki-test".parse::<AgentId>().expect("fixture agent"),
        },
        principal("gateway"),
        "slack.t0123abc.u9xyz".parse().expect("fixture route"),
    )
    .expect("attested context")
}
fn request(id: &str, input: Value, secret_use: Option<SecretUseProposal>) -> InvocationRequest {
    InvocationRequest {
        id: id.parse().expect("fixture invocation"),
        capability: capability(),
        trace_parent: TraceParent::new([7; 16], [3; 8], 1).expect("trace"),
        secret_use,
        input,
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn cedar_and_secret_policy_refuse_before_the_guest_can_call_http() {
    let registry = BrokerProviderRegistry::load([component()], BrokerHostLimits::default())
        .await
        .expect("load actual component");
    let world = PolicyWorld::new(
        [principal("allowed-caller"), principal("denied-caller")],
        [(capability(), "mediawiki".parse().expect("provider ID"))],
    )
    .expect("policy world");
    let policy = r#"@id("wikipedia-read") permit(
        principal == Dekopon::Principal::"allowed-caller",
        action == Dekopon::Action::"mediawiki.search",
        resource == Dekopon::Provider::"mediawiki"
    ) when { context.agent == "mediawiki-test" && context.via == "gateway" };"#;
    let constraints = ConstraintSet {
        route: CapabilityRoute::Generic,
        provider: "mediawiki".parse().expect("provider ID"),
        effect: EffectKind::ReadOnly,
        risk: dekopon_core::RiskLevel::Low,
        credential: None,
        constraints: ExecutionConstraints {
            timeout_ms: 10_000,
            http: Some(HttpConstraints {
                allowed_hosts: vec!["en.wikipedia.org".to_owned()],
                allowed_methods: vec!["GET".to_owned()],
                max_requests: 1,
                max_request_bytes: 16_384,
                max_response_bytes: 1_048_576,
                allow_plaintext_loopback: false,
                propagate_trace: false,
            }),
            storage: None,
            asset: None,
            secret_use: None,
        },
    };
    let audit = Arc::new(InMemoryAuditLog::new(16).expect("bounded audit"));
    let broker = Broker::new(
        registry,
        principal("broker-test"),
        "policy-test".to_owned(),
        PolicyEngine::new(policy, &world).expect("Cedar policy"),
        ConstraintCatalog::new([(capability(), constraints)]).expect("closed constraints"),
        CredentialStore::empty(),
        IdentityDirectory::empty(),
        Arc::clone(&audit),
        BrokerLimits::default(),
    )
    .expect("broker");
    let denied = broker
        .invoke(
            &context("denied-caller"),
            None,
            None,
            request("wiki-cedar-denied", json!({"query":"private-canary"}), None),
            AssetInputs::default(),
        )
        .await
        .expect("policy denial is an outcome");
    assert_eq!(denied.result.outcome, InvocationOutcome::Denied);
    assert_eq!(denied.result.error.as_deref(), Some("policy-denied"));
    let secret = "drn:com.xrl:secret:test:mediawiki/token";
    let unbound = broker
        .invoke(
            &context("allowed-caller"),
            None,
            None,
            request(
                "wiki-secret-denied",
                json!({"query":"private-canary"}),
                Some(SecretUseProposal::HttpBearer {
                    secret: secret.parse().expect("DRN"),
                }),
            ),
            AssetInputs::default(),
        )
        .await
        .expect("unbound secret denial is an outcome");
    assert_eq!(unbound.result.outcome, InvocationOutcome::Denied);
    assert_eq!(unbound.result.error.as_deref(), Some("secret-denied"));
    let records = serde_json::to_string(&audit.records()).expect("audit JSON");
    assert!(
        !records.contains("private-canary"),
        "input never enters the audit"
    );
    assert!(
        !records.contains("token-bytes-canary"),
        "no secret bytes in audit"
    );
}
