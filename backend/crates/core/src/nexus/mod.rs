pub mod safe_fetch;

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::Arc;

use parking_lot::Mutex;
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};

use crate::common::{AppError, new_id, now_ms, sha3_hex};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanTier {
    Starter,
    Growth,
    Enterprise,
}

impl PlanTier {
    fn agent_limit(self) -> usize {
        match self {
            Self::Starter => 2,
            Self::Growth => 5,
            Self::Enterprise => usize::MAX,
        }
    }

    fn monthly_action_limit(self) -> u64 {
        match self {
            Self::Starter => 1_000,
            Self::Growth => 10_000,
            Self::Enterprise => u64::MAX,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NexusRole {
    Viewer,
    Operator,
    Approver,
    Admin,
    Owner,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentKind {
    SalesOs,
    SupportOs,
    OperationsOs,
    HrOs,
    MarketingOs,
    ComplianceOs,
    LegalOs,
    FinanceOs,
    MeetingDestruction,
    ShadowWorkEliminator,
    OrganizationalDebtAuditor,
    RevenueLeakDetector,
    ScopeCreepEnforcer,
    VendorIntelligenceNegotiator,
    EmployeeChurnRadar,
    DecisionMemory,
    RegulatoryHorizonScanner,
    CashFlowSentinel,
    BusinessBrain,
    DormantAssetMonetization,
    CustomerWalletShareMaximizer,
    DataProductCreator,
    NewRevenueStreamArchitect,
    PricingPowerAgent,
    PartnershipRevenueGenerator,
    WhiteLabelRevenueMultiplier,
    MarketTimingOracle,
    CompetitiveWeaknessExploiter,
    MarketCategoryCreator,
    SubscriptionEconomyConverter,
    NetworkEffectBuilder,
    ExitValueMaximizer,
    StrategicAcquirerIntelligence,
    BrandAuthorityCompound,
}

impl AgentKind {
    pub const ALL: [Self; 34] = [
        Self::SalesOs,
        Self::SupportOs,
        Self::OperationsOs,
        Self::HrOs,
        Self::MarketingOs,
        Self::ComplianceOs,
        Self::LegalOs,
        Self::FinanceOs,
        Self::MeetingDestruction,
        Self::ShadowWorkEliminator,
        Self::OrganizationalDebtAuditor,
        Self::RevenueLeakDetector,
        Self::ScopeCreepEnforcer,
        Self::VendorIntelligenceNegotiator,
        Self::EmployeeChurnRadar,
        Self::DecisionMemory,
        Self::RegulatoryHorizonScanner,
        Self::CashFlowSentinel,
        Self::BusinessBrain,
        Self::DormantAssetMonetization,
        Self::CustomerWalletShareMaximizer,
        Self::DataProductCreator,
        Self::NewRevenueStreamArchitect,
        Self::PricingPowerAgent,
        Self::PartnershipRevenueGenerator,
        Self::WhiteLabelRevenueMultiplier,
        Self::MarketTimingOracle,
        Self::CompetitiveWeaknessExploiter,
        Self::MarketCategoryCreator,
        Self::SubscriptionEconomyConverter,
        Self::NetworkEffectBuilder,
        Self::ExitValueMaximizer,
        Self::StrategicAcquirerIntelligence,
        Self::BrandAuthorityCompound,
    ];

    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::SalesOs => "sales_os",
            Self::SupportOs => "support_os",
            Self::OperationsOs => "operations_os",
            Self::HrOs => "hr_os",
            Self::MarketingOs => "marketing_os",
            Self::ComplianceOs => "compliance_os",
            Self::LegalOs => "legal_os",
            Self::FinanceOs => "finance_os",
            Self::MeetingDestruction => "meeting_destruction",
            Self::ShadowWorkEliminator => "shadow_work_eliminator",
            Self::OrganizationalDebtAuditor => "organizational_debt_auditor",
            Self::RevenueLeakDetector => "revenue_leak_detector",
            Self::ScopeCreepEnforcer => "scope_creep_enforcer",
            Self::VendorIntelligenceNegotiator => "vendor_intelligence_negotiator",
            Self::EmployeeChurnRadar => "employee_churn_radar",
            Self::DecisionMemory => "decision_memory",
            Self::RegulatoryHorizonScanner => "regulatory_horizon_scanner",
            Self::CashFlowSentinel => "cash_flow_sentinel",
            Self::BusinessBrain => "business_brain",
            Self::DormantAssetMonetization => "dormant_asset_monetization",
            Self::CustomerWalletShareMaximizer => "customer_wallet_share_maximizer",
            Self::DataProductCreator => "data_product_creator",
            Self::NewRevenueStreamArchitect => "new_revenue_stream_architect",
            Self::PricingPowerAgent => "pricing_power_agent",
            Self::PartnershipRevenueGenerator => "partnership_revenue_generator",
            Self::WhiteLabelRevenueMultiplier => "white_label_revenue_multiplier",
            Self::MarketTimingOracle => "market_timing_oracle",
            Self::CompetitiveWeaknessExploiter => "competitive_weakness_exploiter",
            Self::MarketCategoryCreator => "market_category_creator",
            Self::SubscriptionEconomyConverter => "subscription_economy_converter",
            Self::NetworkEffectBuilder => "network_effect_builder",
            Self::ExitValueMaximizer => "exit_value_maximizer",
            Self::StrategicAcquirerIntelligence => "strategic_acquirer_intelligence",
            Self::BrandAuthorityCompound => "brand_authority_compound",
        }
    }

    pub fn from_key(value: &str) -> Result<Self, AppError> {
        Self::ALL
            .into_iter()
            .find(|kind| kind.key() == value.to_ascii_lowercase())
            .ok_or_else(|| AppError::Validation(format!("unknown Nexus agent {value}")))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentDefinition {
    pub kind: AgentKind,
    pub key: String,
    pub name: String,
    pub department: String,
    pub purpose: String,
    pub accepted_event_types: Vec<String>,
    pub default_action_types: Vec<String>,
    pub risk_class: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Organization {
    pub organization_id: String,
    pub name: String,
    pub plan: PlanTier,
    pub currency: String,
    pub timezone: String,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateOrganizationRequest {
    pub name: String,
    #[serde(default = "default_plan")]
    pub plan: PlanTier,
    #[serde(default = "default_currency")]
    pub currency: String,
    #[serde(default = "default_timezone")]
    pub timezone: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateOrganizationPlanRequest {
    pub plan: PlanTier,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NexusPlanDefinition {
    pub plan: PlanTier,
    pub name: String,
    pub target_customer: String,
    pub monthly_price_usd: Option<u64>,
    pub annual_contract_range_usd: Option<(u64, u64)>,
    pub included_agent_limit: usize,
    pub included_action_limit: u64,
    pub integration_limit: Option<usize>,
    pub features: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NexusIntegrationDefinition {
    pub provider: String,
    pub category: String,
    pub supported_agent_keys: Vec<String>,
    pub required_scopes: Vec<String>,
    pub httpa_required: bool,
    pub sandbox_required: bool,
    pub semantic_render_required_for_intake: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Membership {
    pub membership_id: String,
    pub organization_id: String,
    pub principal_id: String,
    pub role: NexusRole,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AddMemberRequest {
    pub principal_id: String,
    pub role: NexusRole,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfiguration {
    pub organization_id: String,
    pub kind: AgentKind,
    pub enabled: bool,
    pub auto_execute_low_risk: bool,
    pub settings: serde_json::Value,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateAgentConfigurationRequest {
    pub enabled: bool,
    #[serde(default)]
    pub auto_execute_low_risk: bool,
    #[serde(default)]
    pub settings: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntegrationConnection {
    pub integration_id: String,
    pub organization_id: String,
    pub provider: String,
    pub display_name: String,
    pub scopes: Vec<String>,
    pub status: String,
    pub credential_fingerprint: Option<String>,
    pub configuration: serde_json::Value,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ConnectIntegrationRequest {
    pub provider: String,
    pub display_name: Option<String>,
    #[serde(default)]
    pub scopes: Vec<String>,
    pub credential: Option<String>,
    #[serde(default)]
    pub configuration: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BusinessEvent {
    pub event_id: String,
    pub organization_id: String,
    pub source: String,
    pub event_type: String,
    pub subject_type: String,
    pub subject_id: String,
    pub data: serde_json::Value,
    pub idempotency_key: Option<String>,
    pub occurred_at_ms: i64,
    pub ingested_at_ms: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct IngestBusinessEventRequest {
    pub source: String,
    pub event_type: String,
    pub subject_type: String,
    pub subject_id: String,
    #[serde(default)]
    pub data: serde_json::Value,
    pub idempotency_key: Option<String>,
    pub occurred_at_ms: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RevenueExperiment {
    pub experiment_id: String,
    pub hypothesis: String,
    pub target_segment: String,
    pub action_steps: Vec<String>,
    pub kpi: String,
    pub expected_value: f64,
    pub rollback: serde_json::Value,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RevenueOpportunity {
    pub opportunity_id: String,
    pub organization_id: String,
    pub run_id: String,
    pub agent: AgentKind,
    pub opportunity_type: String,
    pub title: String,
    pub evidence: serde_json::Value,
    pub estimated_annual_revenue: f64,
    pub confidence: f64,
    pub required_inputs: Vec<String>,
    pub risk: String,
    #[serde(default)]
    pub experiment: Option<RevenueExperiment>,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Finding {
    pub finding_id: String,
    pub severity: String,
    pub title: String,
    pub explanation: String,
    pub evidence: serde_json::Value,
    pub confidence: f64,
    pub estimated_annual_value: f64,
    #[serde(default)]
    pub required_inputs: Vec<String>,
    #[serde(default)]
    pub revenue_opportunity: Option<RevenueOpportunity>,
    #[serde(default)]
    pub revenue_experiment: Option<RevenueExperiment>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProposedAction {
    pub action_id: String,
    pub organization_id: String,
    pub run_id: String,
    pub action_type: String,
    pub title: String,
    pub payload: serde_json::Value,
    pub rollback: serde_json::Value,
    pub risk: String,
    pub status: String,
    pub approval_required: bool,
    pub approved_by: Option<String>,
    pub executed_at_ms: Option<i64>,
    #[serde(default)]
    pub sandbox: Option<crate::sandbox::SandboxReceipt>,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentRun {
    pub run_id: String,
    pub organization_id: String,
    pub agent: AgentKind,
    pub objective: String,
    pub event_ids: Vec<String>,
    pub status: String,
    pub findings: Vec<Finding>,
    pub actions: Vec<ProposedAction>,
    pub estimated_annual_value: f64,
    pub confidence: f64,
    #[serde(default)]
    pub asc2: Option<crate::asc2::Asc2Diagnostics>,
    #[serde(default)]
    pub reasoned_answer: Option<String>,
    #[serde(default)]
    pub revenue_opportunities: Vec<RevenueOpportunity>,
    pub created_at_ms: i64,
    pub completed_at_ms: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RunAgentRequest {
    pub objective: Option<String>,
    #[serde(default)]
    pub event_ids: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ApproveActionRequest {
    #[serde(default)]
    pub execute_after_approval: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RejectActionRequest {
    pub reason: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DueDiligenceAuditRequest {
    pub target_company_name: String,
    pub urls: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DueDiligenceAuditResult {
    pub audit_id: String,
    pub target_company_name: String,
    pub urls_audited: usize,
    pub generated_event_ids: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MonitorUrlRequest {
    pub url: String,
    pub event_type_to_trigger: String,
    pub subject_type: String,
    pub subject_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitoredUrl {
    pub monitor_id: String,
    pub organization_id: String,
    pub url: String,
    pub event_type_to_trigger: String,
    pub subject_type: String,
    pub subject_id: String,
    pub last_content_hash: Option<String>,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BusinessBrainQuestion {
    pub question_id: String,
    pub category: String,
    pub prompt: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateBusinessBrainInterviewRequest {
    pub founder_name: Option<String>,
    pub target_revenue: Option<f64>,
    #[serde(default)]
    pub context: serde_json::Value,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SubmitBusinessBrainAnswersRequest {
    pub answers: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BusinessBrainProfile {
    pub profile_id: String,
    pub organization_id: String,
    pub interview_id: String,
    pub founder_name: Option<String>,
    pub target_revenue: f64,
    pub skills: Vec<String>,
    pub assets: Vec<String>,
    pub network: Vec<String>,
    pub constraints: Vec<String>,
    pub founder_dna_scores: BTreeMap<String, f64>,
    pub opportunities: Vec<RevenueOpportunity>,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BusinessBrainMilestone {
    pub day: u32,
    pub title: String,
    pub deliverable: String,
    pub revenue_target: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BusinessBrainScript {
    pub script_id: String,
    pub audience: String,
    pub purpose: String,
    pub body: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BusinessBrainPlan {
    pub plan_id: String,
    pub organization_id: String,
    pub interview_id: String,
    pub selected_opportunity_id: String,
    pub revenue_target: f64,
    pub ninety_day_roadmap: Vec<BusinessBrainMilestone>,
    pub daily_tasks: Vec<String>,
    pub scripts: Vec<BusinessBrainScript>,
    pub milestones: Vec<String>,
    pub review_cadence: String,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BusinessBrainInterview {
    pub interview_id: String,
    pub organization_id: String,
    pub founder_name: Option<String>,
    pub target_revenue: f64,
    pub context: serde_json::Value,
    pub questions: Vec<BusinessBrainQuestion>,
    pub answers: BTreeMap<String, String>,
    pub status: String,
    #[serde(default)]
    pub profile: Option<BusinessBrainProfile>,
    #[serde(default)]
    pub plan: Option<BusinessBrainPlan>,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutomationWorkflow {
    pub workflow_id: String,
    pub organization_id: String,
    pub name: String,
    pub trigger_event_type: String,
    pub agent: AgentKind,
    pub enabled: bool,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateWorkflowRequest {
    pub name: String,
    pub trigger_event_type: String,
    pub agent: AgentKind,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventIngestionResult {
    pub event: BusinessEvent,
    pub triggered_runs: Vec<AgentRun>,
    pub coordination_actions: Vec<ProposedAction>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NexusDashboard {
    pub organization: Organization,
    pub enabled_agents: usize,
    pub integrations: usize,
    pub events_ingested: usize,
    pub completed_runs: usize,
    pub pending_approvals: usize,
    pub estimated_annual_value: f64,
    pub actions_used_this_month: u64,
    pub monthly_action_limit: u64,
    pub recent_runs: Vec<AgentRun>,
}

#[derive(Clone)]
pub struct NexusService {
    store: Arc<Mutex<Connection>>,
    semantic_render: Option<crate::Shared<crate::semantic_render::SemanticRenderEngine>>,
}

impl NexusService {
    pub fn new(data_dir: impl AsRef<Path>, semantic_render: Option<crate::Shared<crate::semantic_render::SemanticRenderEngine>>) -> Result<Self, AppError> {
        std::fs::create_dir_all(data_dir.as_ref()).map_err(|error| {
            AppError::Internal(format!("failed to create Nexus data directory: {error}"))
        })?;
        let connection = Connection::open(data_dir.as_ref().join("nexus.sqlite"))
            .map_err(|error| AppError::Internal(format!("failed to open Nexus store: {error}")))?;
        initialize_store(&connection)?;
        Ok(Self {
            store: Arc::new(Mutex::new(connection)),
            semantic_render,
        })
    }

    #[must_use]
    pub fn agent_catalog(&self) -> Vec<AgentDefinition> {
        AgentKind::ALL.into_iter().map(agent_definition).collect()
    }

    #[must_use]
    pub fn plan_catalog(&self) -> Vec<NexusPlanDefinition> {
        vec![
            NexusPlanDefinition {
                plan: PlanTier::Starter,
                name: "Starter".into(),
                target_customer: "SMB 50-200 employees".into(),
                monthly_price_usd: Some(499),
                annual_contract_range_usd: None,
                included_agent_limit: PlanTier::Starter.agent_limit(),
                included_action_limit: PlanTier::Starter.monthly_action_limit(),
                integration_limit: Some(5),
                features: vec![
                    "2 AI agents: Support OS and Sales OS by default".into(),
                    "Up to 1,000 governed actions/month".into(),
                    "5 integrations".into(),
                    "Email support".into(),
                ],
            },
            NexusPlanDefinition {
                plan: PlanTier::Growth,
                name: "Growth".into(),
                target_customer: "Mid-market 200-1,000 employees".into(),
                monthly_price_usd: Some(1_999),
                annual_contract_range_usd: None,
                included_agent_limit: PlanTier::Growth.agent_limit(),
                included_action_limit: PlanTier::Growth.monthly_action_limit(),
                integration_limit: None,
                features: vec![
                    "5 AI agents of the customer's choice".into(),
                    "10,000 governed actions/month".into(),
                    "Unlimited integrations".into(),
                    "Priority support and onboarding".into(),
                    "Custom agent training metadata".into(),
                ],
            },
            NexusPlanDefinition {
                plan: PlanTier::Enterprise,
                name: "Enterprise".into(),
                target_customer: "Large 1,000+ employees".into(),
                monthly_price_usd: None,
                annual_contract_range_usd: Some((50_000, 500_000)),
                included_agent_limit: PlanTier::Enterprise.agent_limit(),
                included_action_limit: PlanTier::Enterprise.monthly_action_limit(),
                integration_limit: None,
                features: vec![
                    "All 34 backend agents available".into(),
                    "Unlimited governed actions".into(),
                    "On-premise deployment option".into(),
                    "Dedicated success manager".into(),
                    "Custom SLA and compliance controls".into(),
                ],
            },
        ]
    }

    #[must_use]
    pub fn integration_catalog(&self) -> Vec<NexusIntegrationDefinition> {
        supported_integrations()
            .iter()
            .map(|provider| NexusIntegrationDefinition {
                provider: (*provider).into(),
                category: integration_category(provider).into(),
                supported_agent_keys: integration_agent_keys(provider),
                required_scopes: integration_scopes(provider),
                httpa_required: true,
                sandbox_required: true,
                semantic_render_required_for_intake: matches!(
                    integration_category(provider),
                    "sales" | "support" | "marketing" | "documents" | "collaboration" | "storage"
                ),
            })
            .collect()
    }

    pub fn create_organization(
        &self,
        owner_principal_id: &str,
        request: CreateOrganizationRequest,
    ) -> Result<Organization, AppError> {
        if owner_principal_id.trim().is_empty() || request.name.trim().is_empty() {
            return Err(AppError::Validation(
                "organization name and owner principal are required".into(),
            ));
        }
        let now = now_ms();
        let organization = Organization {
            organization_id: new_id("nexus_org"),
            name: request.name.trim().into(),
            plan: request.plan,
            currency: request.currency.trim().to_ascii_uppercase(),
            timezone: request.timezone.trim().into(),
            created_at_ms: now,
            updated_at_ms: now,
        };
        let membership = Membership {
            membership_id: new_id("nexus_member"),
            organization_id: organization.organization_id.clone(),
            principal_id: owner_principal_id.into(),
            role: NexusRole::Owner,
            created_at_ms: now,
        };
        let starter_agents = [AgentKind::SupportOs, AgentKind::SalesOs];
        let store = self.store.lock();
        insert_json(
            &store,
            "nexus_organizations",
            "organization_id",
            &organization.organization_id,
            Some(&organization.organization_id),
            &organization,
        )?;
        insert_json(
            &store,
            "nexus_memberships",
            "membership_id",
            &membership.membership_id,
            Some(&organization.organization_id),
            &membership,
        )?;
        for kind in starter_agents {
            let config = AgentConfiguration {
                organization_id: organization.organization_id.clone(),
                kind,
                enabled: true,
                auto_execute_low_risk: false,
                settings: serde_json::json!({}),
                updated_at_ms: now,
            };
            upsert_agent_config(&store, &config)?;
        }
        Ok(organization)
    }

    pub fn get_organization(
        &self,
        principal_id: &str,
        organization_id: &str,
    ) -> Result<Organization, AppError> {
        self.require_role(principal_id, organization_id, NexusRole::Viewer)?;
        read_json(
            &self.store.lock(),
            "nexus_organizations",
            "organization_id",
            organization_id,
        )
    }

    pub fn list_organizations(&self, principal_id: &str) -> Result<Vec<Organization>, AppError> {
        let store = self.store.lock();
        let mut statement = store
            .prepare(
                "SELECT o.payload FROM nexus_organizations o
                 INNER JOIN nexus_memberships m ON m.organization_id=o.organization_id
                 WHERE m.principal_id=?1 ORDER BY o.created_at_ms DESC",
            )
            .map_err(sql_error)?;
        statement
            .query_map([principal_id], |row| row.get::<_, String>(0))
            .map_err(sql_error)?
            .map(|row| row.map_err(sql_error).and_then(|payload| decode(&payload)))
            .collect()
    }

    pub fn update_plan(
        &self,
        principal_id: &str,
        organization_id: &str,
        request: UpdateOrganizationPlanRequest,
    ) -> Result<Organization, AppError> {
        self.require_role(principal_id, organization_id, NexusRole::Owner)?;
        let mut organization: Organization = read_json(
            &self.store.lock(),
            "nexus_organizations",
            "organization_id",
            organization_id,
        )?;
        let enabled = self
            .list_agent_configs(principal_id, organization_id)?
            .into_iter()
            .filter(|config| config.enabled)
            .count();
        if enabled > request.plan.agent_limit() {
            return Err(AppError::Conflict(format!(
                "disable agents before moving to {:?}; {} agents are enabled",
                request.plan, enabled
            )));
        }
        organization.plan = request.plan;
        organization.updated_at_ms = now_ms();
        insert_json(
            &self.store.lock(),
            "nexus_organizations",
            "organization_id",
            organization_id,
            Some(organization_id),
            &organization,
        )?;
        Ok(organization)
    }

    pub fn add_member(
        &self,
        actor: &str,
        organization_id: &str,
        request: AddMemberRequest,
    ) -> Result<Membership, AppError> {
        self.require_role(
            actor,
            organization_id,
            if request.role == NexusRole::Owner {
                NexusRole::Owner
            } else {
                NexusRole::Admin
            },
        )?;
        if request.principal_id.trim().is_empty() {
            return Err(AppError::Validation("member principal is required".into()));
        }
        let membership = Membership {
            membership_id: new_id("nexus_member"),
            organization_id: organization_id.into(),
            principal_id: request.principal_id,
            role: request.role,
            created_at_ms: now_ms(),
        };
        let store = self.store.lock();
        store
            .execute(
                "DELETE FROM nexus_memberships WHERE organization_id=?1 AND principal_id=?2",
                params![organization_id, membership.principal_id],
            )
            .map_err(sql_error)?;
        insert_json(
            &store,
            "nexus_memberships",
            "membership_id",
            &membership.membership_id,
            Some(organization_id),
            &membership,
        )?;
        Ok(membership)
    }

    pub fn list_members(
        &self,
        principal_id: &str,
        organization_id: &str,
    ) -> Result<Vec<Membership>, AppError> {
        self.require_role(principal_id, organization_id, NexusRole::Viewer)?;
        list_json(
            &self.store.lock(),
            "nexus_memberships",
            organization_id,
            500,
        )
    }

    pub fn list_agent_configs(
        &self,
        principal_id: &str,
        organization_id: &str,
    ) -> Result<Vec<AgentConfiguration>, AppError> {
        self.require_role(principal_id, organization_id, NexusRole::Viewer)?;
        let configured: Vec<AgentConfiguration> = list_json(
            &self.store.lock(),
            "nexus_agent_configs",
            organization_id,
            500,
        )?;
        let map = configured
            .into_iter()
            .map(|config| (config.kind, config))
            .collect::<BTreeMap<_, _>>();
        Ok(AgentKind::ALL
            .into_iter()
            .map(|kind| {
                map.get(&kind).cloned().unwrap_or(AgentConfiguration {
                    organization_id: organization_id.into(),
                    kind,
                    enabled: false,
                    auto_execute_low_risk: false,
                    settings: serde_json::json!({}),
                    updated_at_ms: 0,
                })
            })
            .collect())
    }

    pub fn update_agent_config(
        &self,
        principal_id: &str,
        organization_id: &str,
        kind: AgentKind,
        request: UpdateAgentConfigurationRequest,
    ) -> Result<AgentConfiguration, AppError> {
        self.require_role(principal_id, organization_id, NexusRole::Admin)?;
        let organization: Organization = read_json(
            &self.store.lock(),
            "nexus_organizations",
            "organization_id",
            organization_id,
        )?;
        let configs = self.list_agent_configs(principal_id, organization_id)?;
        let enabled_count = configs
            .iter()
            .filter(|config| config.enabled && config.kind != kind)
            .count();
        if request.enabled && enabled_count >= organization.plan.agent_limit() {
            return Err(AppError::Forbidden(format!(
                "{:?} plan allows {} enabled agents",
                organization.plan,
                organization.plan.agent_limit()
            )));
        }
        let config = AgentConfiguration {
            organization_id: organization_id.into(),
            kind,
            enabled: request.enabled,
            auto_execute_low_risk: request.auto_execute_low_risk,
            settings: request.settings,
            updated_at_ms: now_ms(),
        };
        upsert_agent_config(&self.store.lock(), &config)?;
        Ok(config)
    }

    pub fn connect_integration(
        &self,
        principal_id: &str,
        organization_id: &str,
        request: ConnectIntegrationRequest,
    ) -> Result<IntegrationConnection, AppError> {
        self.require_role(principal_id, organization_id, NexusRole::Admin)?;
        let provider = request.provider.trim().to_ascii_lowercase();
        if !supported_integrations().contains(&provider.as_str()) {
            return Err(AppError::Validation(format!(
                "unsupported Nexus integration {provider}"
            )));
        }
        let now = now_ms();
        let integration = IntegrationConnection {
            integration_id: new_id("nexus_integration"),
            organization_id: organization_id.into(),
            display_name: request
                .display_name
                .unwrap_or_else(|| provider.replace('_', " ")),
            provider,
            scopes: request.scopes,
            status: "connected".into(),
            credential_fingerprint: request
                .credential
                .map(|secret| sha3_hex(secret.as_bytes())[..20].to_string()),
            configuration: redact_json(request.configuration),
            created_at_ms: now,
            updated_at_ms: now,
        };
        insert_json(
            &self.store.lock(),
            "nexus_integrations",
            "integration_id",
            &integration.integration_id,
            Some(organization_id),
            &integration,
        )?;
        Ok(integration)
    }

    pub fn list_integrations(
        &self,
        principal_id: &str,
        organization_id: &str,
    ) -> Result<Vec<IntegrationConnection>, AppError> {
        self.require_role(principal_id, organization_id, NexusRole::Viewer)?;
        list_json(
            &self.store.lock(),
            "nexus_integrations",
            organization_id,
            500,
        )
    }

    pub fn disconnect_integration(
        &self,
        principal_id: &str,
        organization_id: &str,
        integration_id: &str,
    ) -> Result<IntegrationConnection, AppError> {
        self.require_role(principal_id, organization_id, NexusRole::Admin)?;
        let mut integration: IntegrationConnection = read_json(
            &self.store.lock(),
            "nexus_integrations",
            "integration_id",
            integration_id,
        )?;
        ensure_org(&integration.organization_id, organization_id)?;
        integration.status = "disconnected".into();
        integration.updated_at_ms = now_ms();
        insert_json(
            &self.store.lock(),
            "nexus_integrations",
            "integration_id",
            integration_id,
            Some(organization_id),
            &integration,
        )?;
        Ok(integration)
    }

    pub fn create_workflow(
        &self,
        principal_id: &str,
        organization_id: &str,
        request: CreateWorkflowRequest,
    ) -> Result<AutomationWorkflow, AppError> {
        self.require_role(principal_id, organization_id, NexusRole::Admin)?;
        self.ensure_agent_enabled(organization_id, request.agent)?;
        if request.name.trim().is_empty() || request.trigger_event_type.trim().is_empty() {
            return Err(AppError::Validation(
                "workflow name and trigger event type are required".into(),
            ));
        }
        let now = now_ms();
        let workflow = AutomationWorkflow {
            workflow_id: new_id("nexus_workflow"),
            organization_id: organization_id.into(),
            name: request.name.trim().into(),
            trigger_event_type: request.trigger_event_type.trim().into(),
            agent: request.agent,
            enabled: request.enabled,
            created_at_ms: now,
            updated_at_ms: now,
        };
        insert_json(
            &self.store.lock(),
            "nexus_workflows",
            "workflow_id",
            &workflow.workflow_id,
            Some(organization_id),
            &workflow,
        )?;
        Ok(workflow)
    }

    pub fn list_workflows(
        &self,
        principal_id: &str,
        organization_id: &str,
    ) -> Result<Vec<AutomationWorkflow>, AppError> {
        self.require_role(principal_id, organization_id, NexusRole::Viewer)?;
        list_json(&self.store.lock(), "nexus_workflows", organization_id, 500)
    }

    pub fn ingest_event(
        &self,
        principal_id: &str,
        organization_id: &str,
        request: IngestBusinessEventRequest,
    ) -> Result<EventIngestionResult, AppError> {
        self.require_role(principal_id, organization_id, NexusRole::Operator)?;
        if request.event_type.trim().is_empty()
            || request.source.trim().is_empty()
            || request.subject_id.trim().is_empty()
        {
            return Err(AppError::Validation(
                "event source, type, and subject id are required".into(),
            ));
        }
        if let Some(key) = request.idempotency_key.as_deref() {
            if let Some(event) = self.find_event_by_idempotency(organization_id, key)? {
                return Ok(EventIngestionResult {
                    event,
                    triggered_runs: Vec::new(),
                    coordination_actions: Vec::new(),
                });
            }
        }
        let mut processed_data = redact_json(request.data);

        if let Some(semantic) = &self.semantic_render {
            if let Some(url) = processed_data.get("website_url").and_then(|v| v.as_str()) {
                let mut engine = semantic.write();
                if let Ok(html) = safe_fetch::safe_fetch_html(url) {
                    if let Ok(report) = engine.render(crate::semantic_render::SemanticRenderRequest {
                        url: url.into(),
                        html,
                        content_type: "text/html".into(),
                        notarize_to_chain: false,
                    }) {
                        if let Some(obj) = processed_data.as_object_mut() {
                            obj.insert("semantic_enrichment".into(), serde_json::to_value(report).unwrap_or_default());
                        }
                    }
                }
            }
        }

        let event = BusinessEvent {
            event_id: new_id("nexus_event"),
            organization_id: organization_id.into(),
            source: request.source.trim().into(),
            event_type: request.event_type.trim().into(),
            subject_type: request.subject_type.trim().into(),
            subject_id: request.subject_id.trim().into(),
            data: processed_data,
            idempotency_key: request.idempotency_key,
            occurred_at_ms: request.occurred_at_ms.unwrap_or_else(now_ms),
            ingested_at_ms: now_ms(),
        };
        insert_event(&self.store.lock(), &event)?;

        let workflows: Vec<AutomationWorkflow> =
            list_json(&self.store.lock(), "nexus_workflows", organization_id, 500)?;
        let mut triggered_runs = Vec::new();
        for workflow in workflows
            .into_iter()
            .filter(|workflow| workflow.enabled && workflow.trigger_event_type == event.event_type)
        {
            triggered_runs.push(self.run_agent_internal(
                organization_id,
                workflow.agent,
                RunAgentRequest {
                    objective: Some(format!("Workflow: {}", workflow.name)),
                    event_ids: vec![event.event_id.clone()],
                },
            )?);
        }
        let coordination_actions = self.cross_department_coordination(&event)?;
        Ok(EventIngestionResult {
            event,
            triggered_runs,
            coordination_actions,
        })
    }

    pub fn list_events(
        &self,
        principal_id: &str,
        organization_id: &str,
    ) -> Result<Vec<BusinessEvent>, AppError> {
        self.require_role(principal_id, organization_id, NexusRole::Viewer)?;
        list_json(&self.store.lock(), "nexus_events", organization_id, 500)
    }

    pub fn run_agent(
        &self,
        principal_id: &str,
        organization_id: &str,
        kind: AgentKind,
        request: RunAgentRequest,
    ) -> Result<AgentRun, AppError> {
        self.require_role(principal_id, organization_id, NexusRole::Operator)?;
        self.run_agent_internal(organization_id, kind, request)
    }

    fn run_agent_internal(
        &self,
        organization_id: &str,
        kind: AgentKind,
        request: RunAgentRequest,
    ) -> Result<AgentRun, AppError> {
        self.ensure_agent_enabled(organization_id, kind)?;
        self.enforce_usage_limit(organization_id)?;
        let events = if request.event_ids.is_empty() {
            let all: Vec<BusinessEvent> =
                list_json(&self.store.lock(), "nexus_events", organization_id, 200)?;
            all.into_iter()
                .filter(|event| accepts_event(kind, &event.event_type))
                .take(100)
                .collect::<Vec<_>>()
        } else {
            request
                .event_ids
                .iter()
                .map(|event_id| {
                    let event: BusinessEvent =
                        read_json(&self.store.lock(), "nexus_events", "event_id", event_id)?;
                    if event.organization_id != organization_id {
                        return Err(AppError::Forbidden(
                            "event belongs to another organization".into(),
                        ));
                    }
                    Ok(event)
                })
                .collect::<Result<Vec<_>, AppError>>()?
        };
        let run_id = new_id("nexus_run");
        let findings = analyze(kind, &events, organization_id, &run_id);
        let revenue_opportunities = findings
            .iter()
            .filter_map(|finding| finding.revenue_opportunity.clone())
            .collect::<Vec<_>>();
        let actions = actions_for_findings(organization_id, &run_id, kind, &findings);
        let estimated_annual_value = findings
            .iter()
            .map(|finding| finding.estimated_annual_value)
            .sum();
        let confidence = if findings.is_empty() {
            0.0
        } else {
            findings
                .iter()
                .map(|finding| finding.confidence)
                .sum::<f64>()
                / findings.len() as f64
        };
        let now = now_ms();
        let run = AgentRun {
            run_id: run_id.clone(),
            organization_id: organization_id.into(),
            agent: kind,
            objective: request
                .objective
                .unwrap_or_else(|| format!("Analyze organization events with {}", kind.key())),
            event_ids: events.iter().map(|event| event.event_id.clone()).collect(),
            status: if findings.is_empty() {
                "completed_no_findings".into()
            } else {
                "completed".into()
            },
            findings,
            actions: actions.clone(),
            estimated_annual_value,
            confidence,
            asc2: None,
            reasoned_answer: None,
            revenue_opportunities,
            created_at_ms: now,
            completed_at_ms: now_ms(),
        };
        let store = self.store.lock();
        insert_json(
            &store,
            "nexus_agent_runs",
            "run_id",
            &run.run_id,
            Some(organization_id),
            &run,
        )?;
        for action in &actions {
            insert_json(
                &store,
                "nexus_actions",
                "action_id",
                &action.action_id,
                Some(organization_id),
                action,
            )?;
        }
        increment_usage(&store, organization_id, 1)?;
        Ok(run)
    }

    pub fn attach_asc2_reasoning(
        &self,
        organization_id: &str,
        run_id: &str,
        diagnostics: crate::asc2::Asc2Diagnostics,
        reasoned_answer: String,
    ) -> Result<AgentRun, AppError> {
        let mut run: AgentRun =
            read_json(&self.store.lock(), "nexus_agent_runs", "run_id", run_id)?;
        ensure_org(&run.organization_id, organization_id)?;
        run.asc2 = Some(diagnostics);
        run.reasoned_answer = Some(reasoned_answer);
        insert_json(
            &self.store.lock(),
            "nexus_agent_runs",
            "run_id",
            &run.run_id,
            Some(organization_id),
            &run,
        )?;
        Ok(run)
    }

    pub fn get_run(
        &self,
        principal_id: &str,
        organization_id: &str,
        run_id: &str,
    ) -> Result<AgentRun, AppError> {
        self.require_role(principal_id, organization_id, NexusRole::Viewer)?;
        let run: AgentRun = read_json(&self.store.lock(), "nexus_agent_runs", "run_id", run_id)?;
        ensure_org(&run.organization_id, organization_id)?;
        Ok(run)
    }

    pub fn list_actions(
        &self,
        principal_id: &str,
        organization_id: &str,
    ) -> Result<Vec<ProposedAction>, AppError> {
        self.require_role(principal_id, organization_id, NexusRole::Viewer)?;
        list_json(&self.store.lock(), "nexus_actions", organization_id, 500)
    }

    pub fn start_due_diligence_audit(
        &self,
        principal_id: &str,
        organization_id: &str,
        request: DueDiligenceAuditRequest,
    ) -> Result<DueDiligenceAuditResult, AppError> {
        self.require_role(principal_id, organization_id, NexusRole::Operator)?;

        let mut generated_event_ids = Vec::new();
        let audit_id = new_id("audit");

        for url in &request.urls {
            // First, trigger semantic render
            let mut semantic_data = serde_json::Value::Null;
            if let Some(semantic) = &self.semantic_render {
                let mut engine = semantic.write();
                if let Ok(html) = safe_fetch::safe_fetch_html(url) {
                    if let Ok(report) = engine.render(crate::semantic_render::SemanticRenderRequest {
                        url: url.clone(),
                        html,
                        content_type: "text/html".into(),
                        notarize_to_chain: true,
                    }) {
                        semantic_data = serde_json::to_value(report).unwrap_or_default();
                    }
                }
            }

            // Fan out into different departmental events for agents to pick up
            let departments = ["audit.finance", "audit.legal", "audit.hr", "audit.compliance"];
            for dept in departments {
                let event = BusinessEvent {
                    event_id: new_id("nexus_event"),
                    organization_id: organization_id.into(),
                    source: "due_diligence_auditor".into(),
                    event_type: dept.into(),
                    subject_type: "company".into(),
                    subject_id: request.target_company_name.clone(),
                    data: serde_json::json!({
                        "url": url,
                        "semantic_enrichment": semantic_data
                    }),
                    idempotency_key: Some(format!("{}_{}_{}", audit_id, dept, url)),
                    occurred_at_ms: now_ms(),
                    ingested_at_ms: now_ms(),
                };
                insert_event(&self.store.lock(), &event)?;
                generated_event_ids.push(event.event_id.clone());

                // Note: we can either manually trigger the agents here, or let them be triggered
                // by workflows matching the "audit.*" events. For now, we ingest them to the event stream.
            }
        }

        Ok(DueDiligenceAuditResult {
            audit_id,
            target_company_name: request.target_company_name,
            urls_audited: request.urls.len(),
            generated_event_ids,
        })
    }

    pub fn monitor_url(
        &self,
        principal_id: &str,
        organization_id: &str,
        request: MonitorUrlRequest,
    ) -> Result<MonitoredUrl, AppError> {
        self.require_role(principal_id, organization_id, NexusRole::Operator)?;
        let monitor = MonitoredUrl {
            monitor_id: new_id("monitor"),
            organization_id: organization_id.into(),
            url: request.url,
            event_type_to_trigger: request.event_type_to_trigger,
            subject_type: request.subject_type,
            subject_id: request.subject_id,
            last_content_hash: None,
            created_at_ms: now_ms(),
        };
        insert_json(
            &self.store.lock(),
            "nexus_url_monitors",
            "monitor_id",
            &monitor.monitor_id,
            Some(organization_id),
            &monitor,
        )?;
        Ok(monitor)
    }

    pub fn check_monitored_urls(&self) -> Result<usize, AppError> {
        let monitors: Vec<MonitoredUrl> = list_all_json(&self.store.lock(), "nexus_url_monitors")?;
        let mut triggered_count = 0;

        for mut monitor in monitors {
            if let Some(semantic) = &self.semantic_render {
                let mut engine = semantic.write();
                if let Ok(html) = safe_fetch::safe_fetch_html(&monitor.url) {
                    if let Ok(report) = engine.render(crate::semantic_render::SemanticRenderRequest {
                        url: monitor.url.clone(),
                        html,
                        content_type: "text/html".into(),
                        notarize_to_chain: false,
                    }) {
                        let new_hash = report.content_hash.clone();
                        if monitor.last_content_hash.as_deref() != Some(new_hash.as_str()) {
                            monitor.last_content_hash = Some(new_hash);
                            insert_json(
                                &self.store.lock(),
                                "nexus_url_monitors",
                                "monitor_id",
                                &monitor.monitor_id,
                                Some(&monitor.organization_id),
                                &monitor,
                            )?;

                            let event = BusinessEvent {
                                event_id: new_id("nexus_event"),
                                organization_id: monitor.organization_id.clone(),
                                source: "url_monitor".into(),
                                event_type: monitor.event_type_to_trigger.clone(),
                                subject_type: monitor.subject_type.clone(),
                                subject_id: monitor.subject_id.clone(),
                                data: serde_json::json!({
                                    "url": monitor.url,
                                    "semantic_enrichment": report
                                }),
                                idempotency_key: Some(format!("{}_{}", monitor.monitor_id, report.content_hash)),
                                occurred_at_ms: now_ms(),
                                ingested_at_ms: now_ms(),
                            };
                            insert_event(&self.store.lock(), &event)?;
                            triggered_count += 1;
                        }
                    }
                }
            }
        }
        Ok(triggered_count)
    }

    pub fn start_business_brain_interview(
        &self,
        principal_id: &str,
        organization_id: &str,
        request: CreateBusinessBrainInterviewRequest,
    ) -> Result<BusinessBrainInterview, AppError> {
        self.require_role(principal_id, organization_id, NexusRole::Operator)?;
        let now = now_ms();
        let interview = BusinessBrainInterview {
            interview_id: new_id("business_brain_interview"),
            organization_id: organization_id.into(),
            founder_name: request
                .founder_name
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty()),
            target_revenue: request.target_revenue.unwrap_or(10_000_000.0).max(1.0),
            context: redact_json(request.context),
            questions: business_brain_questions(),
            answers: BTreeMap::new(),
            status: "collecting_answers".into(),
            profile: None,
            plan: None,
            created_at_ms: now,
            updated_at_ms: now,
        };
        insert_json(
            &self.store.lock(),
            "nexus_business_brain_interviews",
            "interview_id",
            &interview.interview_id,
            Some(organization_id),
            &interview,
        )?;
        Ok(interview)
    }

    pub fn submit_business_brain_answers(
        &self,
        principal_id: &str,
        organization_id: &str,
        interview_id: &str,
        request: SubmitBusinessBrainAnswersRequest,
    ) -> Result<BusinessBrainInterview, AppError> {
        self.require_role(principal_id, organization_id, NexusRole::Operator)?;
        let mut interview: BusinessBrainInterview = read_json(
            &self.store.lock(),
            "nexus_business_brain_interviews",
            "interview_id",
            interview_id,
        )?;
        ensure_org(&interview.organization_id, organization_id)?;
        let question_ids = business_brain_questions()
            .into_iter()
            .map(|question| question.question_id)
            .collect::<BTreeSet<_>>();
        for (question_id, answer) in request.answers {
            if !question_ids.contains(&question_id) {
                return Err(AppError::Validation(format!(
                    "unknown Business Brain question {question_id}"
                )));
            }
            let answer = answer.trim();
            if !answer.is_empty() {
                interview.answers.insert(question_id, answer.to_string());
            }
        }
        interview.status = if interview.answers.len() >= 40 {
            "ready_for_plan".into()
        } else {
            "collecting_answers".into()
        };
        interview.updated_at_ms = now_ms();
        insert_json(
            &self.store.lock(),
            "nexus_business_brain_interviews",
            "interview_id",
            &interview.interview_id,
            Some(organization_id),
            &interview,
        )?;
        Ok(interview)
    }

    pub fn generate_business_brain_plan(
        &self,
        principal_id: &str,
        organization_id: &str,
        interview_id: &str,
    ) -> Result<BusinessBrainInterview, AppError> {
        self.require_role(principal_id, organization_id, NexusRole::Operator)?;
        let mut interview: BusinessBrainInterview = read_json(
            &self.store.lock(),
            "nexus_business_brain_interviews",
            "interview_id",
            interview_id,
        )?;
        ensure_org(&interview.organization_id, organization_id)?;
        if interview.answers.len() < 40 {
            return Err(AppError::Validation(format!(
                "Business Brain requires 40 answers before planning; received {}",
                interview.answers.len()
            )));
        }
        let profile = build_business_brain_profile(&interview);
        let plan = build_business_brain_plan(&interview, &profile);
        interview.profile = Some(profile);
        interview.plan = Some(plan);
        interview.status = "planned".into();
        interview.updated_at_ms = now_ms();
        insert_json(
            &self.store.lock(),
            "nexus_business_brain_interviews",
            "interview_id",
            &interview.interview_id,
            Some(organization_id),
            &interview,
        )?;
        Ok(interview)
    }

    pub fn list_revenue_opportunities(
        &self,
        principal_id: &str,
        organization_id: &str,
    ) -> Result<Vec<RevenueOpportunity>, AppError> {
        self.require_role(principal_id, organization_id, NexusRole::Viewer)?;
        let runs: Vec<AgentRun> =
            list_json(&self.store.lock(), "nexus_agent_runs", organization_id, 500)?;
        let interviews: Vec<BusinessBrainInterview> = list_json(
            &self.store.lock(),
            "nexus_business_brain_interviews",
            organization_id,
            500,
        )?;
        let mut opportunities = runs
            .into_iter()
            .flat_map(|run| run.revenue_opportunities)
            .collect::<Vec<_>>();
        opportunities.extend(
            interviews
                .into_iter()
                .filter_map(|interview| interview.profile)
                .flat_map(|profile| profile.opportunities),
        );
        opportunities.sort_by(|left, right| {
            right
                .estimated_annual_revenue
                .total_cmp(&left.estimated_annual_revenue)
        });
        Ok(opportunities)
    }

    pub fn approve_action(
        &self,
        principal_id: &str,
        organization_id: &str,
        action_id: &str,
        request: ApproveActionRequest,
    ) -> Result<ProposedAction, AppError> {
        self.require_role(principal_id, organization_id, NexusRole::Approver)?;
        let mut action: ProposedAction =
            read_json(&self.store.lock(), "nexus_actions", "action_id", action_id)?;
        ensure_org(&action.organization_id, organization_id)?;
        if action.status != "pending_approval" {
            return Err(AppError::Conflict(format!(
                "action {} is not pending approval",
                action.action_id
            )));
        }
        action.status = if request.execute_after_approval {
            "approved_for_delivery"
        } else {
            "approved"
        }
        .into();
        action.approved_by = Some(principal_id.into());
        action.executed_at_ms = None;
        insert_json(
            &self.store.lock(),
            "nexus_actions",
            "action_id",
            &action.action_id,
            Some(organization_id),
            &action,
        )?;
        Ok(action)
    }

    pub fn attach_sandbox_to_action(
        &self,
        organization_id: &str,
        action_id: &str,
        sandbox: crate::sandbox::SandboxReceipt,
    ) -> Result<ProposedAction, AppError> {
        let mut action: ProposedAction =
            read_json(&self.store.lock(), "nexus_actions", "action_id", action_id)?;
        ensure_org(&action.organization_id, organization_id)?;
        if matches!(
            sandbox.decision.outcome,
            crate::sandbox::SandboxDecisionOutcome::Deny
                | crate::sandbox::SandboxDecisionOutcome::ApprovalRequired
                | crate::sandbox::SandboxDecisionOutcome::SandboxUnavailable
        ) {
            action.status = "sandbox_blocked".into();
            action.executed_at_ms = None;
        } else if action.status == "approved_for_delivery" {
            action.status = "sandbox_certified_for_delivery".into();
        }
        action.sandbox = Some(sandbox);
        insert_json(
            &self.store.lock(),
            "nexus_actions",
            "action_id",
            &action.action_id,
            Some(organization_id),
            &action,
        )?;
        Ok(action)
    }

    pub fn reject_action(
        &self,
        principal_id: &str,
        organization_id: &str,
        action_id: &str,
        request: RejectActionRequest,
    ) -> Result<ProposedAction, AppError> {
        self.require_role(principal_id, organization_id, NexusRole::Approver)?;
        if request.reason.trim().is_empty() {
            return Err(AppError::Validation("rejection reason is required".into()));
        }
        let mut action: ProposedAction =
            read_json(&self.store.lock(), "nexus_actions", "action_id", action_id)?;
        ensure_org(&action.organization_id, organization_id)?;
        if action.status != "pending_approval" {
            return Err(AppError::Conflict(format!(
                "action {} is not pending approval",
                action.action_id
            )));
        }
        action.status = "rejected".into();
        action.approved_by = Some(principal_id.into());
        action.payload["rejection_reason"] = serde_json::Value::String(request.reason);
        insert_json(
            &self.store.lock(),
            "nexus_actions",
            "action_id",
            &action.action_id,
            Some(organization_id),
            &action,
        )?;
        Ok(action)
    }

    pub fn dashboard(
        &self,
        principal_id: &str,
        organization_id: &str,
    ) -> Result<NexusDashboard, AppError> {
        self.require_role(principal_id, organization_id, NexusRole::Viewer)?;
        let organization: Organization = read_json(
            &self.store.lock(),
            "nexus_organizations",
            "organization_id",
            organization_id,
        )?;
        let configs = self.list_agent_configs(principal_id, organization_id)?;
        let integrations: Vec<IntegrationConnection> = list_json(
            &self.store.lock(),
            "nexus_integrations",
            organization_id,
            500,
        )?;
        let events: Vec<BusinessEvent> =
            list_json(&self.store.lock(), "nexus_events", organization_id, 500)?;
        let runs: Vec<AgentRun> =
            list_json(&self.store.lock(), "nexus_agent_runs", organization_id, 500)?;
        let actions: Vec<ProposedAction> =
            list_json(&self.store.lock(), "nexus_actions", organization_id, 500)?;
        let used = usage_for_month(&self.store.lock(), organization_id)?;
        let monthly_action_limit = organization.plan.monthly_action_limit();
        Ok(NexusDashboard {
            organization,
            enabled_agents: configs.iter().filter(|config| config.enabled).count(),
            integrations: integrations.len(),
            events_ingested: events.len(),
            completed_runs: runs.len(),
            pending_approvals: actions
                .iter()
                .filter(|action| action.status == "pending_approval")
                .count(),
            estimated_annual_value: runs.iter().map(|run| run.estimated_annual_value).sum(),
            actions_used_this_month: used,
            monthly_action_limit,
            recent_runs: runs.into_iter().take(20).collect(),
        })
    }

    fn cross_department_coordination(
        &self,
        event: &BusinessEvent,
    ) -> Result<Vec<ProposedAction>, AppError> {
        let rule = match event.event_type.as_str() {
            "support.sentiment" if number(&event.data, "sentiment") < -0.5 => Some((
                "pause_sales_outreach",
                "Pause upsell outreach for frustrated customer",
                "moderate",
            )),
            "legal.contract_risk" if number(&event.data, "risk_score") >= 0.7 => Some((
                "hold_finance_payment",
                "Hold payment until legal risk is resolved",
                "high",
            )),
            "sales.deal_won" => Some((
                "draft_finance_invoice",
                "Draft invoice from newly won deal",
                "moderate",
            )),
            "hr.employee_hired" => Some((
                "provision_employee_access",
                "Provision approved systems for new employee",
                "high",
            )),
            _ => None,
        };
        let Some((action_type, title, risk)) = rule else {
            return Ok(Vec::new());
        };
        let action = ProposedAction {
            action_id: new_id("nexus_action"),
            organization_id: event.organization_id.clone(),
            run_id: format!("coordination:{}", event.event_id),
            action_type: action_type.into(),
            title: title.into(),
            payload: serde_json::json!({
                "source_event_id": event.event_id,
                "subject_id": event.subject_id,
            }),
            rollback: serde_json::json!({"action":"restore_previous_state"}),
            risk: risk.into(),
            status: "pending_approval".into(),
            approval_required: true,
            approved_by: None,
            executed_at_ms: None,
            sandbox: None,
            created_at_ms: now_ms(),
        };
        insert_json(
            &self.store.lock(),
            "nexus_actions",
            "action_id",
            &action.action_id,
            Some(&event.organization_id),
            &action,
        )?;
        Ok(vec![action])
    }

    fn require_role(
        &self,
        principal_id: &str,
        organization_id: &str,
        minimum: NexusRole,
    ) -> Result<NexusRole, AppError> {
        let payload = self
            .store
            .lock()
            .query_row(
                "SELECT payload FROM nexus_memberships WHERE organization_id=?1 AND principal_id=?2 LIMIT 1",
                params![organization_id, principal_id],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(sql_error)?
            .ok_or(AppError::Unauthorized)?;
        let membership: Membership = decode(&payload)?;
        if membership.role < minimum {
            return Err(AppError::Forbidden(format!("{minimum:?} role is required")));
        }
        Ok(membership.role)
    }

    fn ensure_agent_enabled(&self, organization_id: &str, kind: AgentKind) -> Result<(), AppError> {
        let config = self
            .store
            .lock()
            .query_row(
                "SELECT payload FROM nexus_agent_configs WHERE organization_id=?1 AND agent_key=?2",
                params![organization_id, kind.key()],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(sql_error)?
            .and_then(|payload| serde_json::from_str::<AgentConfiguration>(&payload).ok());
        if !config.is_some_and(|config| config.enabled) {
            return Err(AppError::Forbidden(format!(
                "Nexus agent {} is not enabled for this organization",
                kind.key()
            )));
        }
        Ok(())
    }

    fn enforce_usage_limit(&self, organization_id: &str) -> Result<(), AppError> {
        let organization: Organization = read_json(
            &self.store.lock(),
            "nexus_organizations",
            "organization_id",
            organization_id,
        )?;
        if usage_for_month(&self.store.lock(), organization_id)?
            >= organization.plan.monthly_action_limit()
        {
            return Err(AppError::Forbidden(
                "monthly Nexus action limit reached".into(),
            ));
        }
        Ok(())
    }

    fn find_event_by_idempotency(
        &self,
        organization_id: &str,
        idempotency_key: &str,
    ) -> Result<Option<BusinessEvent>, AppError> {
        self.store
            .lock()
            .query_row(
                "SELECT payload FROM nexus_events WHERE organization_id=?1 AND idempotency_key=?2 LIMIT 1",
                params![organization_id, idempotency_key],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(sql_error)?
            .map(|payload| decode(&payload))
            .transpose()
    }
}

fn initialize_store(connection: &Connection) -> Result<(), AppError> {
    connection
        .execute_batch(
            "
            PRAGMA journal_mode=WAL;
            PRAGMA synchronous=NORMAL;
            PRAGMA foreign_keys=ON;
            CREATE TABLE IF NOT EXISTS nexus_organizations (
                organization_id TEXT PRIMARY KEY, organization_id_scope TEXT NOT NULL,
                payload TEXT NOT NULL, created_at_ms INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS nexus_memberships (
                membership_id TEXT PRIMARY KEY, organization_id TEXT NOT NULL,
                principal_id TEXT GENERATED ALWAYS AS (json_extract(payload, '$.principal_id')) VIRTUAL,
                payload TEXT NOT NULL, created_at_ms INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_nexus_memberships_org_principal
                ON nexus_memberships(organization_id, principal_id);
            CREATE UNIQUE INDEX IF NOT EXISTS idx_nexus_memberships_unique_principal
                ON nexus_memberships(organization_id, principal_id);
            CREATE TABLE IF NOT EXISTS nexus_agent_configs (
                agent_key TEXT NOT NULL, organization_id TEXT NOT NULL, payload TEXT NOT NULL,
                created_at_ms INTEGER NOT NULL, PRIMARY KEY(organization_id, agent_key)
            );
            CREATE TABLE IF NOT EXISTS nexus_integrations (
                integration_id TEXT PRIMARY KEY, organization_id TEXT NOT NULL,
                payload TEXT NOT NULL, created_at_ms INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS nexus_events (
                event_id TEXT PRIMARY KEY, organization_id TEXT NOT NULL,
                idempotency_key TEXT, payload TEXT NOT NULL, created_at_ms INTEGER NOT NULL,
                UNIQUE(organization_id, idempotency_key)
            );
            CREATE TABLE IF NOT EXISTS nexus_agent_runs (
                run_id TEXT PRIMARY KEY, organization_id TEXT NOT NULL,
                payload TEXT NOT NULL, created_at_ms INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS nexus_actions (
                action_id TEXT PRIMARY KEY, organization_id TEXT NOT NULL,
                payload TEXT NOT NULL, created_at_ms INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS nexus_workflows (
                workflow_id TEXT PRIMARY KEY, organization_id TEXT NOT NULL,
                payload TEXT NOT NULL, created_at_ms INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS nexus_business_brain_interviews (
                interview_id TEXT PRIMARY KEY, organization_id TEXT NOT NULL,
                payload TEXT NOT NULL, created_at_ms INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS nexus_usage (
                organization_id TEXT NOT NULL, period TEXT NOT NULL, actions INTEGER NOT NULL,
                PRIMARY KEY(organization_id, period)
            );
            CREATE TABLE IF NOT EXISTS nexus_url_monitors (
                monitor_id TEXT PRIMARY KEY, organization_id TEXT NOT NULL,
                payload TEXT NOT NULL, created_at_ms INTEGER NOT NULL
            );
            ",
        )
        .map_err(sql_error)
}

fn insert_json<T: Serialize>(
    connection: &Connection,
    table: &str,
    id_column: &str,
    id: &str,
    organization_id: Option<&str>,
    value: &T,
) -> Result<(), AppError> {
    let payload = serde_json::to_string(value)
        .map_err(|error| AppError::Internal(format!("Nexus serialization failed: {error}")))?;
    if table == "nexus_organizations" {
        connection
            .execute(
                &format!("INSERT OR REPLACE INTO {table} ({id_column}, organization_id_scope, payload, created_at_ms) VALUES (?1, ?2, ?3, ?4)"),
                params![id, id, payload, now_ms()],
            )
            .map_err(sql_error)?;
    } else {
        connection
            .execute(
                &format!("INSERT OR REPLACE INTO {table} ({id_column}, organization_id, payload, created_at_ms) VALUES (?1, ?2, ?3, ?4)"),
                params![id, organization_id.unwrap_or_default(), payload, now_ms()],
            )
            .map_err(sql_error)?;
    }
    Ok(())
}

fn insert_event(connection: &Connection, event: &BusinessEvent) -> Result<(), AppError> {
    let payload = serde_json::to_string(event)
        .map_err(|error| AppError::Internal(format!("Nexus serialization failed: {error}")))?;
    connection
        .execute(
            "INSERT INTO nexus_events (event_id, organization_id, idempotency_key, payload, created_at_ms) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![event.event_id, event.organization_id, event.idempotency_key, payload, now_ms()],
        )
        .map_err(|error| {
            if error.to_string().contains("UNIQUE") {
                AppError::Conflict("duplicate Nexus event idempotency key".into())
            } else {
                sql_error(error)
            }
        })?;
    Ok(())
}

fn upsert_agent_config(
    connection: &Connection,
    config: &AgentConfiguration,
) -> Result<(), AppError> {
    let payload = serde_json::to_string(config)
        .map_err(|error| AppError::Internal(format!("Nexus serialization failed: {error}")))?;
    connection
        .execute(
            "INSERT OR REPLACE INTO nexus_agent_configs (agent_key, organization_id, payload, created_at_ms) VALUES (?1, ?2, ?3, ?4)",
            params![config.kind.key(), config.organization_id, payload, now_ms()],
        )
        .map_err(sql_error)?;
    Ok(())
}

fn read_json<T: for<'de> Deserialize<'de>>(
    connection: &Connection,
    table: &str,
    id_column: &str,
    id: &str,
) -> Result<T, AppError> {
    let payload = connection
        .query_row(
            &format!("SELECT payload FROM {table} WHERE {id_column}=?1"),
            [id],
            |row| row.get::<_, String>(0),
        )
        .map_err(|_| AppError::NotFound(format!("Nexus record {id}")))?;
    decode(&payload)
}

fn list_all_json<T: for<'de> Deserialize<'de>>(
    connection: &Connection,
    table: &str,
) -> Result<Vec<T>, AppError> {
    let mut statement = connection
        .prepare(&format!(
            "SELECT payload FROM {table} ORDER BY created_at_ms DESC"
        ))
        .map_err(sql_error)?;
    statement
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(sql_error)?
        .map(|row| row.map_err(sql_error).and_then(|payload| decode(&payload)))
        .collect()
}

fn list_json<T: for<'de> Deserialize<'de>>(
    connection: &Connection,
    table: &str,
    organization_id: &str,
    limit: usize,
) -> Result<Vec<T>, AppError> {
    let mut statement = connection
        .prepare(&format!(
            "SELECT payload FROM {table} WHERE organization_id=?1 ORDER BY created_at_ms DESC LIMIT ?2"
        ))
        .map_err(sql_error)?;
    statement
        .query_map(params![organization_id, limit], |row| {
            row.get::<_, String>(0)
        })
        .map_err(sql_error)?
        .map(|row| row.map_err(sql_error).and_then(|payload| decode(&payload)))
        .collect()
}

fn decode<T: for<'de> Deserialize<'de>>(payload: &str) -> Result<T, AppError> {
    serde_json::from_str(payload)
        .map_err(|error| AppError::Internal(format!("Nexus record decode failed: {error}")))
}

fn increment_usage(
    connection: &Connection,
    organization_id: &str,
    actions: u64,
) -> Result<(), AppError> {
    connection
        .execute(
            "INSERT INTO nexus_usage (organization_id, period, actions) VALUES (?1, ?2, ?3)
             ON CONFLICT(organization_id, period) DO UPDATE SET actions=actions+excluded.actions",
            params![organization_id, current_period(), actions],
        )
        .map_err(sql_error)?;
    Ok(())
}

fn usage_for_month(connection: &Connection, organization_id: &str) -> Result<u64, AppError> {
    connection
        .query_row(
            "SELECT actions FROM nexus_usage WHERE organization_id=?1 AND period=?2",
            params![organization_id, current_period()],
            |row| row.get::<_, u64>(0),
        )
        .optional()
        .map(|value| value.unwrap_or_default())
        .map_err(sql_error)
}

fn current_period() -> String {
    let days = now_ms() / 86_400_000;
    format!("month_{}", days / 30)
}

fn ensure_org(actual: &str, expected: &str) -> Result<(), AppError> {
    if actual == expected {
        Ok(())
    } else {
        Err(AppError::Forbidden(
            "resource belongs to another organization".into(),
        ))
    }
}

fn accepts_event(kind: AgentKind, event_type: &str) -> bool {
    let definition = agent_definition(kind);
    definition.accepted_event_types.iter().any(|accepted| {
        event_type == accepted
            || event_type
                .split_once('.')
                .is_some_and(|(prefix, _)| accepted == &format!("{prefix}.*"))
    })
}

fn analyze(
    kind: AgentKind,
    events: &[BusinessEvent],
    organization_id: &str,
    run_id: &str,
) -> Vec<Finding> {
    events
        .iter()
        .filter_map(|event| analyze_event(kind, event, organization_id, run_id))
        .collect()
}

fn analyze_event(
    kind: AgentKind,
    event: &BusinessEvent,
    organization_id: &str,
    run_id: &str,
) -> Option<Finding> {
    let data = &event.data;
    let finding = match kind {
        AgentKind::MeetingDestruction => {
            let attendees = number(data, "attendees").max(1.0);
            let duration = number(data, "duration_hours").max(0.0);
            let salary = number(data, "average_annual_salary").max(0.0);
            let occurrences = number(data, "annual_occurrences").max(1.0);
            let annual_cost = attendees * duration * salary / 2_080.0 * occurrences;
            let replaceable = string(data, "meeting_type") == "information_share"
                || bool_value(data, "decision_required") == Some(false);
            replaceable.then(|| make_finding(
                "high",
                "Replaceable meeting detected",
                format!(
                    "This recurring meeting costs approximately {:.2} annually and can be replaced with an asynchronous brief.",
                    annual_cost
                ),
                event,
                0.88,
                annual_cost * 0.7,
            ))
        }
        AgentKind::ShadowWorkEliminator => {
            let repeats = number(data, "repeat_count");
            (repeats >= 3.0).then(|| make_finding(
                "moderate",
                "Repeated shadow-work pattern",
                format!(
                    "The same {} task occurred {repeats:.0} times and should become a reusable automation or knowledge card.",
                    string(data, "pattern")
                ),
                event,
                0.84,
                number(data, "minutes_per_occurrence") * repeats * hourly_rate(data) * 12.0 / 60.0,
            ))
        }
        AgentKind::OrganizationalDebtAuditor => {
            let approval_steps = number(data, "approval_steps");
            let amount = number(data, "transaction_amount");
            (approval_steps >= 3.0 || bool_value(data, "manual_reentry") == Some(true)).then(|| {
                let monthly_cost =
                    number(data, "monthly_occurrences") * number(data, "minutes_per_occurrence")
                        / 60.0
                        * hourly_rate(data);
                make_finding(
                    "high",
                    "Organizational debt detected",
                    format!(
                        "Process uses {approval_steps:.0} approval steps for a {amount:.2} transaction or includes manual re-entry."
                    ),
                    event,
                    0.86,
                    monthly_cost * 12.0,
                )
            })
        }
        AgentKind::RevenueLeakDetector => {
            let delivered = number(data, "delivered_value");
            let invoiced = number(data, "invoiced_value");
            let leak = (delivered - invoiced).max(0.0);
            (leak > 0.0).then(|| {
                make_finding(
                    "critical",
                    "Delivered work has not been invoiced",
                    format!("{leak:.2} of delivered value has no matching invoice."),
                    event,
                    0.94,
                    leak,
                )
            })
        }
        AgentKind::ScopeCreepEnforcer => {
            let contracted = string_set(data, "contracted_scope");
            let requested = string_set(data, "requested_scope");
            let extra = requested
                .difference(&contracted)
                .cloned()
                .collect::<Vec<_>>();
            (!extra.is_empty()).then(|| {
                make_finding(
                    "high",
                    "Out-of-scope request detected",
                    format!(
                        "New request includes uncontracted scope: {}.",
                        extra.join(", ")
                    ),
                    event,
                    0.91,
                    number(data, "change_order_value"),
                )
            })
        }
        AgentKind::VendorIntelligenceNegotiator => {
            let paid = number(data, "paid_seats");
            let active = number(data, "active_seats");
            let annual_price = number(data, "annual_price_per_seat");
            let market_price = number(data, "market_price_per_seat");
            let unused = (paid - active).max(0.0) * annual_price;
            let pricing_gap = active * (annual_price - market_price).max(0.0);
            let savings = unused + pricing_gap;
            (savings > 0.0).then(|| make_finding(
                "high",
                "Vendor spend optimization opportunity",
                format!(
                    "{:.0} paid seats are inactive and/or current pricing exceeds market pricing.",
                    (paid - active).max(0.0)
                ),
                event,
                0.9,
                savings,
            ))
        }
        AgentKind::EmployeeChurnRadar => {
            let signals = [
                "response_time_slowdown",
                "meeting_disengagement",
                "linkedin_updated",
                "vacation_usage_surge",
                "after_hours_work_disappeared",
                "messages_declined",
                "manager_ones_on_ones_declined",
                "salary_review_overdue",
            ]
            .into_iter()
            .filter(|key| bool_value(data, key) == Some(true))
            .count();
            (signals >= 3).then(|| make_finding(
                "high",
                "Employee retention risk detected",
                format!("{signals} behavioral retention-risk signals are active. Human review is required; do not take adverse employment action from this score."),
                event,
                (0.55 + signals as f64 * 0.04).min(0.87),
                number(data, "replacement_cost"),
            ))
        }
        AgentKind::DecisionMemory => Some(make_finding(
            "informational",
            "Business decision recorded",
            format!(
                "Decision '{}' was added to institutional memory with its rationale and review date.",
                string(data, "decision")
            ),
            event,
            0.98,
            number(data, "estimated_value"),
        )),
        AgentKind::RegulatoryHorizonScanner => {
            let impact = string(data, "business_impact");
            (!impact.is_empty()).then(|| {
                make_finding(
                    "critical",
                    "Upcoming regulation affects the business",
                    format!(
                        "{}. Compliance deadline: {}.",
                        impact,
                        string(data, "deadline")
                    ),
                    event,
                    0.9,
                    number(data, "avoided_penalty"),
                )
            })
        }
        AgentKind::CashFlowSentinel => {
            let cash = number(data, "cash");
            let receivables = number(data, "receivables_60d");
            let payables = number(data, "payables_60d");
            let burn = number(data, "monthly_burn");
            let projected = cash + receivables - payables - burn * 2.0;
            (projected < 0.0).then(|| make_finding(
                "critical",
                "Projected 60-day cash shortfall",
                format!(
                    "Base-case cash projection reaches {:.2}; immediate collections and payment-timing review are required.",
                    projected
                ),
                event,
                0.92,
                projected.abs(),
            ))
        }
        AgentKind::SalesOs => {
            if event_type_contains(event, "outreach") {
                let engagement = weighted_score(data, &["opened_email", "visited_pricing", "downloaded_resource", "reply_intent"]);
                Some(make_finding(
                    "moderate",
                    "Personalized outreach sequence ready",
                    format!(
                        "Behavior-triggered outreach can continue until reply or disqualification with engagement score {:.0}/100.",
                        engagement * 100.0
                    ),
                    event,
                    0.84,
                    number(data, "potential_deal_value") * engagement * 0.12,
                ))
            } else if event_type_contains(event, "calendar") || event_type_contains(event, "schedule") {
                Some(make_finding(
                    "informational",
                    "Sales meeting scheduling automation prepared",
                    format!(
                        "Meeting intent '{}' can be scheduled with counteroffer handling and a pre-meeting brief.",
                        string(data, "meeting_intent")
                    ),
                    event,
                    0.82,
                    number_or(data, "minutes_saved", 8.0) / 60.0 * hourly_rate(data) * number_or(data, "annual_meetings", 1.0),
                ))
            } else if event_type_contains(event, "proposal") || event_type_contains(event, "quote") {
                let value = number(data, "deal_value");
                Some(make_finding(
                    "high",
                    "AI proposal package ready",
                    "CRM, meeting notes, product catalog, ROI model, pricing options, and follow-up tracking can be assembled into a proposal.".into(),
                    event,
                    0.86,
                    value * number_or(data, "win_rate_lift", 0.08),
                ))
            } else if event_type_contains(event, "deal_risk") || event_type_contains(event, "pipeline") {
                let risk = weighted_score(data, &["days_quiet", "competitor_mentioned", "champion_left", "price_objection"]);
                Some(make_finding(
                    if risk >= 0.55 { "high" } else { "moderate" },
                    "Deal risk detected before loss",
                    format!("Pipeline risk score is {:.0}/100; suggested recovery action should be reviewed.", risk * 100.0),
                    event,
                    0.87,
                    number(data, "deal_value") * risk * 0.12,
                ))
            } else {
                let score = weighted_score(data, &["company_fit", "engagement", "intent", "authority"]);
                Some(make_finding(
                    if score >= 0.75 {
                        "high"
                    } else {
                        "informational"
                    },
                    "Lead qualification completed",
                    format!("Lead score is {:.0}/100 across fit, engagement, intent, and authority signals.", score * 100.0),
                    event,
                    0.85,
                    number(data, "potential_deal_value") * score,
                ))
            }
        }
        AgentKind::SupportOs => {
            let confidence = number(data, "answer_confidence").clamp(0.0, 1.0);
            let sentiment = number(data, "sentiment");
            if event_type_contains(event, "knowledge") || event_type_contains(event, "kb") {
                return Some(make_finding(
                    "informational",
                    "Knowledge base improvement captured",
                    format!(
                        "New resolution '{}' should update the self-improving support knowledge base.",
                        string(data, "resolution")
                    ),
                    event,
                    0.88,
                    number(data, "future_ticket_deflection_value"),
                ));
            }
            Some(make_finding(
                if sentiment < -0.5 || confidence < 0.65 {
                    "high"
                } else {
                    "informational"
                },
                "Support case triaged",
                if sentiment < -0.5 || confidence < 0.65 {
                    "Case requires human escalation with full context.".into()
                } else {
                    "Case is suitable for an automated knowledge-backed response.".into()
                },
                event,
                confidence.max(0.5),
                number(data, "human_handle_cost"),
            ))
        }
        AgentKind::OperationsOs => {
            if event_type_contains(event, "document") {
                return Some(make_finding(
                    "moderate",
                    "Document fields extracted for system update",
                    format!(
                        "Document '{}' can be extracted and pushed to {} with human approval.",
                        string(data, "document_type"),
                        string_or(data, "target_system", "the configured system")
                    ),
                    event,
                    number_or(data, "extraction_confidence", 0.82),
                    number(data, "manual_processing_cost"),
                ));
            }
            if event_type_contains(event, "report") {
                return Some(make_finding(
                    "informational",
                    "Scheduled report automation ready",
                    format!(
                        "{} report can be generated with plain-English commentary and stakeholder distribution.",
                        string_or(data, "report_period", "Recurring")
                    ),
                    event,
                    0.86,
                    number(data, "hours_saved") * hourly_rate(data) * number_or(data, "annual_runs", 1.0),
                ));
            }
            if event_type_contains(event, "purchase_order") || event_type_contains(event, "reorder") {
                return Some(make_finding(
                    "high",
                    "Predictive reorder point reached",
                    format!(
                        "SKU {} reached reorder threshold based on demand, seasonality, and supplier lead time.",
                        string(data, "sku")
                    ),
                    event,
                    0.88,
                    number(data, "stockout_cost_avoided"),
                ));
            }
            let expected = number(data, "expected_quantity");
            let actual = number(data, "actual_quantity");
            let difference = (expected - actual).abs();
            (difference > 0.0 || bool_value(data, "manual_entry") == Some(true)).then(|| {
                make_finding(
                    "moderate",
                    "Operations discrepancy or automation opportunity",
                    format!(
                        "Detected quantity difference of {difference:.2} or a manual-entry process."
                    ),
                    event,
                    0.89,
                    difference * number(data, "unit_value"),
                )
            })
        }
        AgentKind::HrOs => {
            if event_type_contains(event, "interview") {
                return Some(make_finding(
                    "informational",
                    "Async AI first-interview summary ready",
                    format!(
                        "Candidate response depth score is {:.0}/100; hiring manager review remains required.",
                        weighted_score(data, &["clarity", "depth", "communication", "culture_fit"]) * 100.0
                    ),
                    event,
                    0.78,
                    number(data, "interview_time_saved"),
                ));
            }
            if event_type_contains(event, "offer") || event_type_contains(event, "onboarding") {
                return Some(make_finding(
                    "moderate",
                    "Hiring coordination workflow ready",
                    "Offer, reference-check, onboarding, or panel-feedback coordination can be drafted for approval.".into(),
                    event,
                    0.82,
                    number(data, "coordination_cost_saved"),
                ));
            }
            let score = weighted_score(data, &["skills_match", "experience_match", "role_fit"]);
            Some(make_finding(
                "informational",
                "Candidate screening completed",
                format!(
                    "Evidence-based role match score: {:.0}/100. Human hiring review remains required.",
                    score * 100.0
                ),
                event,
                0.8,
                number(data, "screening_cost_saved"),
            ))
        }
        AgentKind::MarketingOs => {
            if event_type_contains(event, "content") {
                return Some(make_finding(
                    "moderate",
                    "Brand-voice content package ready",
                    format!(
                        "Content for '{}' can be drafted with SEO keywords and scheduled after approval.",
                        string(data, "topic")
                    ),
                    event,
                    0.84,
                    number(data, "agency_cost_avoided"),
                ));
            }
            if event_type_contains(event, "nurture") || event_type_contains(event, "lead_score") {
                let score = weighted_score(data, &["email_engagement", "page_visits", "content_downloads", "fit"]);
                return Some(make_finding(
                    if score >= 0.7 { "high" } else { "informational" },
                    "Marketing lead nurture score updated",
                    format!("Lead nurture score is {:.0}/100 and can notify Sales when threshold is reached.", score * 100.0),
                    event,
                    0.83,
                    number(data, "potential_deal_value") * score * 0.08,
                ));
            }
            let spend = number(data, "spend");
            let revenue = number(data, "attributed_revenue");
            let roas = if spend > 0.0 { revenue / spend } else { 0.0 };
            Some(make_finding(
                if roas < 1.0 { "high" } else { "informational" },
                "Campaign performance analyzed",
                format!("Campaign return on ad spend is {roas:.2}x."),
                event,
                0.87,
                (revenue - spend).max(0.0),
            ))
        }
        AgentKind::ComplianceOs => {
            if event_type_contains(event, "filing") || event_type_contains(event, "report") {
                return Some(make_finding(
                    "high",
                    "Regulatory filing workflow prepared",
                    format!(
                        "Upcoming filing '{}' can be compiled from connected systems and submitted only after human approval.",
                        string(data, "filing_name")
                    ),
                    event,
                    0.88,
                    number(data, "missed_deadline_penalty"),
                ));
            }
            if event_type_contains(event, "policy") || event_type_contains(event, "training") {
                return Some(make_finding(
                    "moderate",
                    "Compliance policy change requires rollout",
                    format!(
                        "Policy change '{}' requires alerts, audit trail update, and employee training assignment.",
                        string(data, "policy_name")
                    ),
                    event,
                    0.86,
                    number(data, "avoided_penalty"),
                ));
            }
            let risk = number(data, "risk_score").clamp(0.0, 1.0);
            (risk >= 0.4).then(|| {
                make_finding(
                    if risk >= 0.8 { "critical" } else { "high" },
                    "Compliance risk detected",
                    format!(
                        "Control evaluation produced a {:.0}/100 risk score.",
                        risk * 100.0
                    ),
                    event,
                    0.9,
                    number(data, "avoided_penalty"),
                )
            })
        }
        AgentKind::LegalOs => {
            if event_type_contains(event, "generate") || event_type_contains(event, "template") {
                return Some(make_finding(
                    "moderate",
                    "Standard contract draft ready",
                    format!(
                        "{} template can be generated with approved clauses and routed for legal review.",
                        string_or(data, "contract_type", "Contract")
                    ),
                    event,
                    0.82,
                    number(data, "legal_hours_saved") * hourly_rate(data),
                ));
            }
            if event_type_contains(event, "deadline") || event_type_contains(event, "renewal") {
                return Some(make_finding(
                    "high",
                    "Legal deadline or renewal requires action",
                    format!(
                        "{} is due on {}; review workflow should be opened.",
                        string_or(data, "matter", "Legal matter"),
                        string(data, "deadline")
                    ),
                    event,
                    0.88,
                    number(data, "risk_exposure"),
                ));
            }
            let clauses = string_set(data, "risky_clauses")
                .into_iter()
                .collect::<Vec<_>>();
            (!clauses.is_empty()).then(|| {
                make_finding(
                    "critical",
                    "Contract clauses require legal review",
                    format!("Risky clauses: {}.", clauses.join(", ")),
                    event,
                    0.91,
                    number(data, "risk_exposure"),
                )
            })
        }
        AgentKind::FinanceOs => {
            if event_type_contains(event, "bookkeeping") || event_type_contains(event, "reconciliation") {
                return Some(make_finding(
                    "moderate",
                    "Bookkeeping automation opportunity",
                    format!(
                        "Transaction category '{}' can be reconciled daily with anomaly review.",
                        string(data, "category")
                    ),
                    event,
                    0.84,
                    number(data, "accounting_hours_saved") * hourly_rate(data),
                ));
            }
            if event_type_contains(event, "cash") {
                let projected = number(data, "cash") + number(data, "receivables_60d")
                    - number(data, "payables_60d")
                    - number(data, "monthly_burn") * 2.0;
                return (projected < 0.0).then(|| {
                    make_finding(
                        "critical",
                        "Finance OS cash-flow risk detected",
                        format!("Projected 60-day cash position is {:.2}.", projected),
                        event,
                        0.9,
                        projected.abs(),
                    )
                });
            }
            if event_type_contains(event, "expense") || event_type_contains(event, "tax") {
                return Some(make_finding(
                    "moderate",
                    "Expense or tax workflow prepared",
                    "Expense report or tax computation can be drafted from connected finance records for approval.".into(),
                    event,
                    0.82,
                    number(data, "manual_cost_saved"),
                ));
            }
            let anomaly = number(data, "anomaly_score").clamp(0.0, 1.0);
            (anomaly >= 0.5).then(|| {
                make_finding(
                    if anomaly >= 0.8 { "critical" } else { "high" },
                    "Financial anomaly detected",
                    format!("Transaction anomaly score is {:.0}/100.", anomaly * 100.0),
                    event,
                    0.9,
                    number(data, "amount"),
                )
            })
        }
        AgentKind::BusinessBrain => business_brain_event_finding(event, organization_id, run_id),
        AgentKind::DormantAssetMonetization => revenue_formula_finding(
            kind,
            event,
            organization_id,
            run_id,
            RevenueFormula {
                required: &["idle_capacity_hours", "capacity_rate"],
                opportunity_type: "asset_monetization",
                title: "Dormant asset monetization opportunity",
                risk: "moderate",
                confidence: 0.84,
                estimate: number(data, "idle_capacity_hours") * number(data, "capacity_rate")
                    + number(data, "licenseable_ip_count") * number(data, "ip_license_value")
                    + number(data, "unused_space_sqft") * number(data, "space_rate") * 12.0,
                explanation: format!(
                    "Idle capacity, unused space, data, expertise, or IP can generate {:.2} annual revenue if monetized through the safest matching channel.",
                    number(data, "idle_capacity_hours") * number(data, "capacity_rate")
                        + number(data, "licenseable_ip_count") * number(data, "ip_license_value")
                        + number(data, "unused_space_sqft") * number(data, "space_rate") * 12.0
                ),
                experiment: Some(experiment(
                    "Sell one dormant asset package to a validated buyer segment",
                    string_or(data, "buyer_segment", "adjacent customer segment"),
                    vec![
                        "Create asset prospectus with compliance review",
                        "Contact 10 validated buyers",
                        "Run one paid pilot before scaling",
                    ],
                    "pilot revenue booked",
                    number(data, "idle_capacity_hours") * number(data, "capacity_rate"),
                )),
            },
        ),
        AgentKind::CustomerWalletShareMaximizer => revenue_formula_finding(
            kind,
            event,
            organization_id,
            run_id,
            RevenueFormula {
                required: &["current_spend", "estimated_category_spend"],
                opportunity_type: "wallet_share_expansion",
                title: "Customer wallet-share expansion opportunity",
                risk: "moderate",
                confidence: 0.82,
                estimate: (number(data, "estimated_category_spend") - number(data, "current_spend"))
                    .max(0.0)
                    * number_or(data, "capture_rate", 0.25),
                explanation: format!(
                    "Customer spend gap indicates {:.2} annual expansion potential from products they likely buy elsewhere.",
                    (number(data, "estimated_category_spend") - number(data, "current_spend"))
                        .max(0.0)
                        * number_or(data, "capture_rate", 0.25)
                ),
                experiment: Some(experiment(
                    "Offer the highest-fit adjacent product to the customer segment",
                    string_or(data, "customer_segment", "existing customers"),
                    vec![
                        "Rank accounts by spend gap",
                        "Generate personalized expansion offer",
                        "Measure conversion and margin by segment",
                    ],
                    "expansion ARR",
                    (number(data, "estimated_category_spend") - number(data, "current_spend"))
                        .max(0.0)
                        * number_or(data, "capture_rate", 0.25),
                )),
            },
        ),
        AgentKind::DataProductCreator => {
            if data.get("semantic_render_proof_hash").is_none()
                && number(data, "compliance_score") < 0.7
            {
                Some(required_inputs_finding(
                    kind,
                    event,
                    organization_id,
                    run_id,
                    &["compliance_score", "semantic_render_proof_hash"],
                ))
            } else {
                revenue_formula_finding(
                    kind,
                    event,
                    organization_id,
                    run_id,
                    RevenueFormula {
                        required: &["buyer_count", "price_per_buyer"],
                        opportunity_type: "data_product",
                        title: "Compliant data product opportunity",
                        risk: "high",
                        confidence: 0.78,
                        estimate: number(data, "buyer_count") * number(data, "price_per_buyer") * 12.0,
                        explanation: format!(
                            "Compliant aggregated data can be packaged for {:.0} buyers at {:.2}/month.",
                            number(data, "buyer_count"),
                            number(data, "price_per_buyer")
                        ),
                        experiment: Some(experiment(
                            "Validate one aggregated data product prospectus",
                            string_or(data, "buyer_segment", "industry benchmark buyers"),
                            vec![
                                "Remove personal and tenant-identifying data",
                                "Publish data prospectus for legal review",
                                "Run paid design-partner calls",
                            ],
                            "signed data-product LOIs",
                            number(data, "buyer_count") * number(data, "price_per_buyer") * 3.0,
                        )),
                    },
                )
            }
        }
        AgentKind::NewRevenueStreamArchitect => revenue_formula_finding(
            kind,
            event,
            organization_id,
            run_id,
            RevenueFormula {
                required: &["customer_count", "new_stream_arpu"],
                opportunity_type: "new_revenue_stream",
                title: "New revenue stream candidate",
                risk: "moderate",
                confidence: 0.8,
                estimate: number(data, "customer_count")
                    * number(data, "new_stream_arpu")
                    * number_or(data, "attach_rate", 0.2),
                explanation: "Existing customer base and capabilities support a ranked new revenue stream test.".into(),
                experiment: Some(experiment(
                    "Launch a low-investment revenue-stream pilot",
                    string_or(data, "customer_segment", "highest-fit customers"),
                    vec![
                        "Score 20 revenue model archetypes",
                        "Pre-sell the top concept",
                        "Kill or scale after first paid signal",
                    ],
                    "paid pilots",
                    number(data, "customer_count")
                        * number(data, "new_stream_arpu")
                        * number_or(data, "attach_rate", 0.2),
                )),
            },
        ),
        AgentKind::PricingPowerAgent => revenue_formula_finding(
            kind,
            event,
            organization_id,
            run_id,
            RevenueFormula {
                required: &["current_revenue", "price_increase_pct"],
                opportunity_type: "pricing_power",
                title: "Evidence-backed pricing power",
                risk: "high",
                confidence: number_or(data, "segment_wtp_score", 0.75).clamp(0.55, 0.95),
                estimate: number(data, "current_revenue")
                    * number(data, "price_increase_pct")
                    * (1.0 - number_or(data, "churn_risk", 0.05).clamp(0.0, 0.9)),
                explanation: "Willingness-to-pay and churn-risk signals support a controlled repricing test.".into(),
                experiment: Some(experiment(
                    "Run segmented repricing with rollback guardrails",
                    string_or(data, "segment", "low-risk customer segment"),
                    vec![
                        "Exclude high-churn and contract-sensitive accounts",
                        "Test price lift on next renewal cohort",
                        "Rollback if churn exceeds guardrail",
                    ],
                    "net revenue retention",
                    number(data, "current_revenue") * number(data, "price_increase_pct"),
                )),
            },
        ),
        AgentKind::PartnershipRevenueGenerator => revenue_formula_finding(
            kind,
            event,
            organization_id,
            run_id,
            RevenueFormula {
                required: &["partner_customer_overlap", "average_deal_value"],
                opportunity_type: "partnership_revenue",
                title: "Partner revenue-share opportunity",
                risk: "moderate",
                confidence: 0.79,
                estimate: number(data, "partner_customer_overlap")
                    * number_or(data, "conversion_rate", 0.05)
                    * number(data, "average_deal_value")
                    * number_or(data, "partners", 1.0),
                explanation: "Overlapping partner audiences can be converted with a revenue-share offer.".into(),
                experiment: Some(experiment(
                    "Pilot a revenue-share partner motion",
                    string_or(data, "partner_segment", "overlapping partner accounts"),
                    vec![
                        "Rank partners by overlap and trust",
                        "Send co-sell offer with clear economics",
                        "Track sourced pipeline and closed revenue",
                    ],
                    "partner-sourced revenue",
                    number(data, "partner_customer_overlap")
                        * number_or(data, "conversion_rate", 0.05)
                        * number(data, "average_deal_value"),
                )),
            },
        ),
        AgentKind::WhiteLabelRevenueMultiplier => revenue_formula_finding(
            kind,
            event,
            organization_id,
            run_id,
            RevenueFormula {
                required: &["licensee_count", "annual_license_fee"],
                opportunity_type: "white_label_licensing",
                title: "White-label licensing package",
                risk: "moderate",
                confidence: 0.77,
                estimate: (number(data, "licensee_count") * number(data, "annual_license_fee")
                    - number(data, "enablement_cost"))
                .max(0.0),
                explanation: "Internal process, software, brand, or training systems can be licensed safely with enablement costs accounted for.".into(),
                experiment: Some(experiment(
                    "Pre-sell one white-label package",
                    string_or(data, "licensee_segment", "adjacent operators"),
                    vec![
                        "Package SOPs, training, and support boundaries",
                        "Price annual license plus onboarding",
                        "Sign one non-competing pilot licensee",
                    ],
                    "licensed revenue",
                    number(data, "annual_license_fee"),
                )),
            },
        ),
        AgentKind::MarketTimingOracle => revenue_formula_finding(
            kind,
            event,
            organization_id,
            run_id,
            RevenueFormula {
                required: &["market_size", "timing_score"],
                opportunity_type: "market_timing",
                title: "Market timing window detected",
                risk: "high",
                confidence: number(data, "timing_score").clamp(0.0, 1.0),
                estimate: number(data, "market_size")
                    * number_or(data, "entry_capture_rate", 0.01)
                    * number(data, "timing_score").clamp(0.0, 1.0),
                explanation: "Market, regulatory, competitor, investor, or talent signals indicate a launch/entry window.".into(),
                experiment: Some(experiment(
                    "Validate launch timing with demand signals",
                    string_or(data, "market_segment", "target market"),
                    vec![
                        "Collect semantic-rendered public signals",
                        "Run demand test in the launch segment",
                        "Proceed only if timing score stays above threshold",
                    ],
                    "qualified demand signals",
                    number(data, "market_size") * number_or(data, "entry_capture_rate", 0.01),
                )),
            },
        ),
        AgentKind::CompetitiveWeaknessExploiter => {
            if data.get("semantic_render_proof_hash").is_none() {
                Some(required_inputs_finding(
                    kind,
                    event,
                    organization_id,
                    run_id,
                    &["semantic_render_proof_hash"],
                ))
            } else {
                revenue_formula_finding(
                    kind,
                    event,
                    organization_id,
                    run_id,
                    RevenueFormula {
                        required: &["affected_customers", "annual_contract_value"],
                        opportunity_type: "competitive_displacement",
                        title: "Competitor weakness displacement campaign",
                        risk: "high",
                        confidence: 0.76,
                        estimate: number(data, "affected_customers")
                            * number_or(data, "expected_capture_rate", 0.08)
                            * number(data, "annual_contract_value"),
                        explanation: "Semantic-rendered competitor weakness evidence can support a compliant displacement campaign.".into(),
                        experiment: Some(experiment(
                            "Run a targeted competitor-displacement campaign",
                            string_or(data, "weakness_type", "affected competitor customers"),
                            vec![
                                "Verify public evidence and avoid false claims",
                                "Target accounts with the affected pain",
                                "Track displacement pipeline",
                            ],
                            "displacement ARR",
                            number(data, "annual_contract_value")
                                * number_or(data, "expected_capture_rate", 0.08),
                        )),
                    },
                )
            }
        }
        AgentKind::MarketCategoryCreator => revenue_formula_finding(
            kind,
            event,
            organization_id,
            run_id,
            RevenueFormula {
                required: &["addressable_market", "differentiation_score"],
                opportunity_type: "category_creation",
                title: "Market category creation opportunity",
                risk: "high",
                confidence: number(data, "differentiation_score").clamp(0.0, 1.0),
                estimate: number(data, "addressable_market")
                    * number_or(data, "category_capture_rate", 0.005)
                    * number(data, "differentiation_score").clamp(0.0, 1.0),
                explanation: "Differentiation and market whitespace support a category-definition playbook.".into(),
                experiment: Some(experiment(
                    "Test category narrative with ICP buyers",
                    string_or(data, "category_name", "new market category"),
                    vec![
                        "Define enemy, new frame, and proof pillars",
                        "Publish semantic-rendered category manifesto",
                        "Measure buyer resonance and inbound intent",
                    ],
                    "category-qualified pipeline",
                    number(data, "addressable_market")
                        * number_or(data, "category_capture_rate", 0.005),
                )),
            },
        ),
        AgentKind::SubscriptionEconomyConverter => revenue_formula_finding(
            kind,
            event,
            organization_id,
            run_id,
            RevenueFormula {
                required: &["customer_count", "monthly_subscription_price"],
                opportunity_type: "subscription_conversion",
                title: "One-time revenue to subscription conversion",
                risk: "moderate",
                confidence: 0.83,
                estimate: number(data, "customer_count")
                    * number(data, "monthly_subscription_price")
                    * 12.0
                    * number_or(data, "migration_rate", 0.4),
                explanation: "One-time services or products can be migrated into recurring tiers with measurable LTV expansion.".into(),
                experiment: Some(experiment(
                    "Migrate one cohort to subscription tiers",
                    string_or(data, "customer_segment", "existing service customers"),
                    vec![
                        "Design three recurring tiers",
                        "Offer migration credit to existing customers",
                        "Track retention and expansion",
                    ],
                    "monthly recurring revenue",
                    number(data, "customer_count") * number(data, "monthly_subscription_price"),
                )),
            },
        ),
        AgentKind::NetworkEffectBuilder => revenue_formula_finding(
            kind,
            event,
            organization_id,
            run_id,
            RevenueFormula {
                required: &["customer_count", "arpu", "network_feature_readiness"],
                opportunity_type: "network_effect",
                title: "Network effect activation opportunity",
                risk: "moderate",
                confidence: number(data, "network_feature_readiness").clamp(0.0, 1.0),
                estimate: number(data, "customer_count")
                    * number(data, "arpu")
                    * number_or(data, "viral_coefficient", 0.1)
                    * number(data, "network_feature_readiness").clamp(0.0, 1.0),
                explanation: "A data, marketplace, collaboration, or benchmark loop can increase product value as adoption grows.".into(),
                experiment: Some(experiment(
                    "Ship one network-effect feature pilot",
                    string_or(data, "network_effect_type", "benchmark/data network"),
                    vec![
                        "Choose the lowest-friction network loop",
                        "Launch to an opt-in cohort",
                        "Measure invites, shared data, and retention lift",
                    ],
                    "network-driven expansion revenue",
                    number(data, "customer_count") * number(data, "arpu") * 0.1,
                )),
            },
        ),
        AgentKind::ExitValueMaximizer => revenue_formula_finding(
            kind,
            event,
            organization_id,
            run_id,
            RevenueFormula {
                required: &["target_valuation", "current_arr", "current_multiple"],
                opportunity_type: "exit_value",
                title: "Exit value gap roadmap",
                risk: "high",
                confidence: 0.74,
                estimate: (number(data, "target_valuation")
                    - number(data, "current_arr") * number(data, "current_multiple"))
                .max(0.0),
                explanation: "Target exit value can be reverse-engineered into ARR, margin, growth, retention, and category metrics.".into(),
                experiment: Some(experiment(
                    "Move one valuation multiple driver",
                    string_or(data, "primary_metric", "net revenue retention"),
                    vec![
                        "Calculate gap to target valuation",
                        "Prioritize the highest-leverage metric",
                        "Create monthly milestone roadmap",
                    ],
                    "valuation gap closed",
                    (number(data, "target_valuation")
                        - number(data, "current_arr") * number(data, "current_multiple"))
                    .max(0.0),
                )),
            },
        ),
        AgentKind::StrategicAcquirerIntelligence => {
            if data.get("semantic_render_proof_hash").is_none() {
                Some(required_inputs_finding(
                    kind,
                    event,
                    organization_id,
                    run_id,
                    &["semantic_render_proof_hash"],
                ))
            } else {
                revenue_formula_finding(
                    kind,
                    event,
                    organization_id,
                    run_id,
                    RevenueFormula {
                        required: &["acquirer_count", "current_arr", "strategic_fit_score"],
                        opportunity_type: "acquirer_positioning",
                        title: "Strategic acquirer positioning opportunity",
                        risk: "high",
                        confidence: number(data, "strategic_fit_score").clamp(0.0, 1.0),
                        estimate: number(data, "current_arr")
                            * number_or(data, "target_multiple", 8.0)
                            * number(data, "strategic_fit_score").clamp(0.0, 1.0),
                        explanation: "Semantic-rendered M&A signals identify likely acquirers and positioning work to appear on their radar.".into(),
                        experiment: Some(experiment(
                            "Build one acquirer relationship loop",
                            string_or(data, "acquirer_segment", "strategic acquirers"),
                            vec![
                                "Map acquirer theses and gaps",
                                "Create proof assets they care about",
                                "Start warm corporate-development touchpoints",
                            ],
                            "qualified acquirer conversations",
                            number(data, "current_arr") * number_or(data, "target_multiple", 8.0),
                        )),
                    },
                )
            }
        }
        AgentKind::BrandAuthorityCompound => {
            if data.get("semantic_render_proof_hash").is_none()
                && number(data, "external_claims") > 0.0
            {
                Some(required_inputs_finding(
                    kind,
                    event,
                    organization_id,
                    run_id,
                    &["semantic_render_proof_hash"],
                ))
            } else {
                revenue_formula_finding(
                    kind,
                    event,
                    organization_id,
                    run_id,
                    RevenueFormula {
                        required: &["audience_size", "average_deal_value"],
                        opportunity_type: "brand_authority",
                        title: "Brand authority compounding opportunity",
                        risk: "moderate",
                        confidence: 0.75,
                        estimate: number(data, "audience_size")
                            * number_or(data, "conversion_rate", 0.01)
                            * number(data, "average_deal_value")
                            * number_or(data, "authority_gap_score", 0.5),
                        explanation: "Thought leadership, analyst proof, media, awards, and category assets can compound inbound revenue.".into(),
                        experiment: Some(experiment(
                            "Publish one authority-building proof asset",
                            string_or(data, "market", "target category"),
                            vec![
                                "Create annual research report outline",
                                "Pitch relevant publications and conferences",
                                "Measure qualified inbound and authority mentions",
                            ],
                            "authority-sourced pipeline",
                            number(data, "audience_size")
                                * number_or(data, "conversion_rate", 0.01)
                                * number(data, "average_deal_value"),
                        )),
                    },
                )
            }
        }
    }?;
    Some(finding)
}

fn actions_for_findings(
    organization_id: &str,
    run_id: &str,
    kind: AgentKind,
    findings: &[Finding],
) -> Vec<ProposedAction> {
    findings
        .iter()
        .filter(|finding| {
            finding.required_inputs.is_empty() && finding.estimated_annual_value > 0.0
        })
        .map(|finding| {
            let (action_type, title, risk) = action_template_for_finding(kind, finding);
            ProposedAction {
                action_id: new_id("nexus_action"),
                organization_id: organization_id.into(),
                run_id: run_id.into(),
                action_type: action_type.into(),
                title: format!("{title}: {}", finding.title),
                payload: serde_json::json!({
                    "finding_id": finding.finding_id,
                    "evidence": finding.evidence,
                }),
                rollback: serde_json::json!({
                    "required": true,
                    "action": "restore_previous_state_and_notify_owner",
                }),
                risk: risk.into(),
                status: "pending_approval".into(),
                approval_required: true,
                approved_by: None,
                executed_at_ms: None,
                sandbox: None,
                created_at_ms: now_ms(),
            }
        })
        .collect()
}

struct RevenueFormula {
    required: &'static [&'static str],
    opportunity_type: &'static str,
    title: &'static str,
    risk: &'static str,
    confidence: f64,
    estimate: f64,
    explanation: String,
    experiment: Option<RevenueExperiment>,
}

fn revenue_formula_finding(
    kind: AgentKind,
    event: &BusinessEvent,
    organization_id: &str,
    run_id: &str,
    formula: RevenueFormula,
) -> Option<Finding> {
    let missing = missing_inputs(&event.data, formula.required);
    if !missing.is_empty() {
        return Some(required_inputs_finding(
            kind,
            event,
            organization_id,
            run_id,
            &missing.iter().map(String::as_str).collect::<Vec<_>>(),
        ));
    }
    if formula.estimate <= 0.0 || !formula.estimate.is_finite() {
        return None;
    }
    Some(revenue_finding(
        kind,
        event,
        organization_id,
        run_id,
        formula.opportunity_type,
        formula.title,
        formula.explanation,
        formula.estimate,
        formula.confidence,
        formula.risk,
        Vec::new(),
        formula.experiment,
    ))
}

#[allow(clippy::too_many_arguments)]
fn revenue_finding(
    kind: AgentKind,
    event: &BusinessEvent,
    organization_id: &str,
    run_id: &str,
    opportunity_type: &str,
    title: &str,
    explanation: String,
    estimated_annual_revenue: f64,
    confidence: f64,
    risk: &str,
    required_inputs: Vec<String>,
    experiment: Option<RevenueExperiment>,
) -> Finding {
    let finding_id = new_id("nexus_finding");
    let evidence = serde_json::json!({
        "event_id": event.event_id,
        "source": event.source,
        "subject_type": event.subject_type,
        "subject_id": event.subject_id,
    });
    let opportunity = RevenueOpportunity {
        opportunity_id: new_id("nexus_revenue_opportunity"),
        organization_id: organization_id.into(),
        run_id: run_id.into(),
        agent: kind,
        opportunity_type: opportunity_type.into(),
        title: title.into(),
        evidence: evidence.clone(),
        estimated_annual_revenue: estimated_annual_revenue.max(0.0),
        confidence: confidence.clamp(0.0, 1.0),
        required_inputs: required_inputs.clone(),
        risk: risk.into(),
        experiment: experiment.clone(),
        created_at_ms: now_ms(),
    };
    Finding {
        finding_id,
        severity: if required_inputs.is_empty() {
            risk.into()
        } else {
            "informational".into()
        },
        title: title.into(),
        explanation,
        evidence,
        confidence: confidence.clamp(0.0, 1.0),
        estimated_annual_value: estimated_annual_revenue.max(0.0),
        required_inputs,
        revenue_opportunity: Some(opportunity),
        revenue_experiment: experiment,
    }
}

fn required_inputs_finding(
    kind: AgentKind,
    event: &BusinessEvent,
    organization_id: &str,
    run_id: &str,
    required_inputs: &[&str],
) -> Finding {
    revenue_finding(
        kind,
        event,
        organization_id,
        run_id,
        "required_inputs",
        "More data required before estimating revenue",
        format!(
            "{} needs these fields before it can compute a revenue estimate: {}.",
            kind.key(),
            required_inputs.join(", ")
        ),
        0.0,
        0.0,
        "informational",
        required_inputs
            .iter()
            .map(|value| (*value).to_string())
            .collect(),
        None,
    )
}

fn business_brain_event_finding(
    event: &BusinessEvent,
    organization_id: &str,
    run_id: &str,
) -> Option<Finding> {
    let missing = missing_inputs(
        &event.data,
        &[
            "skills_score",
            "assets_score",
            "network_score",
            "problem_score",
            "target_revenue",
        ],
    );
    if !missing.is_empty() {
        return Some(required_inputs_finding(
            AgentKind::BusinessBrain,
            event,
            organization_id,
            run_id,
            &missing.iter().map(String::as_str).collect::<Vec<_>>(),
        ));
    }
    let dna = weighted_score(
        &event.data,
        &[
            "skills_score",
            "assets_score",
            "network_score",
            "problem_score",
            "execution_score",
        ],
    );
    let estimate = number(&event.data, "target_revenue") * dna.clamp(0.0, 1.0);
    Some(revenue_finding(
        AgentKind::BusinessBrain,
        event,
        organization_id,
        run_id,
        "founder_strategy",
        "Business Brain founder strategy opportunity",
        format!(
            "Founder DNA score {:.0}/100 supports a personalized business plan toward {:.2} annual revenue.",
            dna * 100.0,
            number(&event.data, "target_revenue")
        ),
        estimate,
        dna,
        "moderate",
        Vec::new(),
        Some(experiment(
            "Validate the highest-fit founder opportunity",
            string_or(
                &event.data,
                "target_customer",
                "first reachable customer segment",
            ),
            vec![
                "Interview 10 reachable buyers",
                "Pre-sell the minimum valuable offer",
                "Update the 90-day roadmap weekly",
            ],
            "validated paid demand",
            estimate.min(number(&event.data, "target_revenue") / 4.0),
        )),
    ))
}

fn make_finding(
    severity: &str,
    title: &str,
    explanation: String,
    event: &BusinessEvent,
    confidence: f64,
    estimated_annual_value: f64,
) -> Finding {
    Finding {
        finding_id: new_id("nexus_finding"),
        severity: severity.into(),
        title: title.into(),
        explanation,
        evidence: serde_json::json!({
            "event_id": event.event_id,
            "source": event.source,
            "subject_type": event.subject_type,
            "subject_id": event.subject_id,
        }),
        confidence: confidence.clamp(0.0, 1.0),
        estimated_annual_value: estimated_annual_value.max(0.0),
        required_inputs: Vec::new(),
        revenue_opportunity: None,
        revenue_experiment: None,
    }
}

fn action_template(kind: AgentKind) -> (&'static str, &'static str, &'static str) {
    match kind {
        AgentKind::MeetingDestruction => {
            ("publish_async_brief", "Draft async replacement", "moderate")
        }
        AgentKind::ShadowWorkEliminator => (
            "create_automation",
            "Draft shadow-work automation",
            "moderate",
        ),
        AgentKind::OrganizationalDebtAuditor => {
            ("simplify_process", "Draft process simplification", "high")
        }
        AgentKind::RevenueLeakDetector => (
            "draft_invoice_correction",
            "Draft billing correction",
            "high",
        ),
        AgentKind::ScopeCreepEnforcer => ("draft_change_order", "Draft change order", "high"),
        AgentKind::VendorIntelligenceNegotiator => (
            "draft_vendor_negotiation",
            "Draft vendor negotiation",
            "high",
        ),
        AgentKind::EmployeeChurnRadar => (
            "create_retention_review",
            "Create confidential retention review",
            "high",
        ),
        AgentKind::DecisionMemory => (
            "schedule_decision_review",
            "Schedule decision outcome review",
            "moderate",
        ),
        AgentKind::RegulatoryHorizonScanner => (
            "create_compliance_roadmap",
            "Draft compliance roadmap",
            "high",
        ),
        AgentKind::CashFlowSentinel => (
            "create_cash_intervention",
            "Draft cash-flow intervention",
            "high",
        ),
        AgentKind::SalesOs => ("route_lead", "Route qualified lead", "moderate"),
        AgentKind::SupportOs => (
            "draft_support_response",
            "Draft or escalate support response",
            "moderate",
        ),
        AgentKind::OperationsOs => (
            "draft_operations_correction",
            "Draft operations correction",
            "high",
        ),
        AgentKind::HrOs => (
            "route_candidate_review",
            "Route candidate for human review",
            "high",
        ),
        AgentKind::MarketingOs => (
            "draft_campaign_adjustment",
            "Draft campaign adjustment",
            "moderate",
        ),
        AgentKind::ComplianceOs => (
            "open_compliance_case",
            "Open compliance investigation",
            "high",
        ),
        AgentKind::LegalOs => ("open_legal_review", "Open legal review", "high"),
        AgentKind::FinanceOs => (
            "hold_and_review_transaction",
            "Hold transaction for review",
            "high",
        ),
        AgentKind::BusinessBrain => (
            "create_business_brain_plan",
            "Create founder revenue roadmap",
            "moderate",
        ),
        AgentKind::DormantAssetMonetization => (
            "draft_asset_monetization_offer",
            "Draft asset monetization offer",
            "moderate",
        ),
        AgentKind::CustomerWalletShareMaximizer => (
            "draft_wallet_share_expansion",
            "Draft wallet-share expansion offer",
            "moderate",
        ),
        AgentKind::DataProductCreator => (
            "draft_data_product_prospectus",
            "Draft compliant data product prospectus",
            "high",
        ),
        AgentKind::NewRevenueStreamArchitect => (
            "draft_revenue_stream_pilot",
            "Draft new revenue stream pilot",
            "moderate",
        ),
        AgentKind::PricingPowerAgent => (
            "draft_repricing_experiment",
            "Draft repricing experiment",
            "high",
        ),
        AgentKind::PartnershipRevenueGenerator => (
            "draft_partnership_outreach",
            "Draft partnership revenue outreach",
            "moderate",
        ),
        AgentKind::WhiteLabelRevenueMultiplier => (
            "draft_white_label_package",
            "Draft white-label licensing package",
            "moderate",
        ),
        AgentKind::MarketTimingOracle => (
            "draft_market_timing_plan",
            "Draft market timing plan",
            "high",
        ),
        AgentKind::CompetitiveWeaknessExploiter => (
            "draft_competitor_displacement_campaign",
            "Draft competitor displacement campaign",
            "high",
        ),
        AgentKind::MarketCategoryCreator => (
            "draft_category_creation_playbook",
            "Draft category creation playbook",
            "high",
        ),
        AgentKind::SubscriptionEconomyConverter => (
            "draft_subscription_migration",
            "Draft subscription migration plan",
            "moderate",
        ),
        AgentKind::NetworkEffectBuilder => (
            "draft_network_effect_pilot",
            "Draft network effect pilot",
            "moderate",
        ),
        AgentKind::ExitValueMaximizer => (
            "draft_exit_value_roadmap",
            "Draft exit value roadmap",
            "high",
        ),
        AgentKind::StrategicAcquirerIntelligence => (
            "draft_acquirer_positioning_plan",
            "Draft acquirer positioning plan",
            "high",
        ),
        AgentKind::BrandAuthorityCompound => (
            "draft_authority_calendar",
            "Draft brand authority calendar",
            "moderate",
        ),
    }
}

fn action_template_for_finding(
    kind: AgentKind,
    finding: &Finding,
) -> (&'static str, &'static str, &'static str) {
    let title = finding.title.to_ascii_lowercase();
    match kind {
        AgentKind::SalesOs if title.contains("outreach") => {
            ("draft_outreach", "Draft personalized outreach", "moderate")
        }
        AgentKind::SalesOs if title.contains("scheduling") => (
            "schedule_sales_meeting",
            "Draft meeting scheduling workflow",
            "moderate",
        ),
        AgentKind::SalesOs if title.contains("proposal") => {
            ("draft_proposal", "Draft sales proposal", "moderate")
        }
        AgentKind::SalesOs if title.contains("deal risk") => {
            ("open_deal_rescue", "Draft deal rescue action", "high")
        }
        AgentKind::SupportOs if title.contains("knowledge base") => (
            "update_knowledge_base",
            "Draft knowledge base update",
            "moderate",
        ),
        AgentKind::SupportOs if title.contains("triaged") => (
            "draft_support_response",
            "Draft or escalate support response",
            "moderate",
        ),
        AgentKind::OperationsOs if title.contains("document") => {
            ("extract_document_fields", "Extract document fields", "high")
        }
        AgentKind::OperationsOs if title.contains("report") => {
            ("generate_report", "Draft scheduled report", "moderate")
        }
        AgentKind::OperationsOs if title.contains("reorder") => {
            ("draft_purchase_order", "Draft purchase order", "high")
        }
        AgentKind::OperationsOs if title.contains("discrepancy") => (
            "open_reconciliation",
            "Open discrepancy reconciliation",
            "high",
        ),
        AgentKind::HrOs if title.contains("interview") => {
            ("draft_interview_summary", "Draft interview summary", "high")
        }
        AgentKind::HrOs if title.contains("coordination") => (
            "draft_offer_workflow",
            "Draft hiring coordination workflow",
            "high",
        ),
        AgentKind::MarketingOs if title.contains("content") => {
            ("draft_content_package", "Draft content package", "moderate")
        }
        AgentKind::MarketingOs if title.contains("nurture") => (
            "update_nurture_sequence",
            "Update nurture sequence",
            "moderate",
        ),
        AgentKind::ComplianceOs if title.contains("filing") => {
            ("draft_regulatory_filing", "Draft regulatory filing", "high")
        }
        AgentKind::ComplianceOs if title.contains("policy") => (
            "assign_training",
            "Draft policy rollout and training",
            "high",
        ),
        AgentKind::LegalOs if title.contains("standard contract") => {
            ("draft_standard_contract", "Draft standard contract", "high")
        }
        AgentKind::LegalOs if title.contains("deadline") || title.contains("renewal") => (
            "schedule_legal_deadline",
            "Schedule legal deadline review",
            "high",
        ),
        AgentKind::FinanceOs if title.contains("bookkeeping") => {
            ("draft_bookkeeping_entry", "Draft bookkeeping entry", "high")
        }
        AgentKind::FinanceOs if title.contains("cash") => {
            ("open_cash_intervention", "Draft cash intervention", "high")
        }
        AgentKind::FinanceOs if title.contains("expense") || title.contains("tax") => (
            "draft_expense_or_tax_review",
            "Draft expense or tax review",
            "high",
        ),
        _ => action_template(kind),
    }
}

fn agent_definition(kind: AgentKind) -> AgentDefinition {
    let (name, department, purpose, events, actions, risk) = match kind {
        AgentKind::SalesOs => (
            "Sales OS",
            "sales",
            "Qualify leads, run follow-up, schedule meetings, draft proposals, and detect deal risk.",
            vec![
                "sales.*",
                "crm.*",
                "sales.lead",
                "sales.outreach",
                "sales.calendar",
                "sales.proposal",
                "sales.deal_risk",
            ],
            vec![
                "route_lead",
                "draft_outreach",
                "schedule_sales_meeting",
                "draft_proposal",
                "open_deal_rescue",
            ],
            "moderate",
        ),
        AgentKind::SupportOs => (
            "Support OS",
            "support",
            "Triage customer cases, draft answers, escalate frustration, and improve the knowledge base.",
            vec!["support.*", "support.ticket", "support.knowledge_update"],
            vec![
                "draft_support_response",
                "escalate_case",
                "update_knowledge_base",
            ],
            "moderate",
        ),
        AgentKind::OperationsOs => (
            "Operations OS",
            "operations",
            "Extract documents, generate reports, monitor inventory, draft purchase orders, and reconcile discrepancies.",
            vec![
                "operations.*",
                "inventory.*",
                "document.*",
                "operations.report",
                "inventory.reorder",
                "operations.purchase_order",
                "operations.reconciliation",
            ],
            vec![
                "draft_operations_correction",
                "extract_document_fields",
                "generate_report",
                "draft_purchase_order",
                "open_reconciliation",
            ],
            "high",
        ),
        AgentKind::HrOs => (
            "HR OS",
            "hr",
            "Screen candidates, run async first interviews, coordinate panels, offers, and onboarding.",
            vec![
                "hr.*",
                "candidate.*",
                "hr.interview",
                "hr.offer",
                "hr.onboarding",
            ],
            vec![
                "route_candidate_review",
                "draft_interview_summary",
                "draft_offer_workflow",
                "draft_onboarding_plan",
            ],
            "high",
        ),
        AgentKind::MarketingOs => (
            "Marketing OS",
            "marketing",
            "Generate content, nurture leads, score inbound demand, monitor SEO, and optimize campaigns.",
            vec![
                "marketing.*",
                "campaign.*",
                "marketing.content",
                "marketing.nurture",
                "marketing.lead_score",
                "marketing.seo",
            ],
            vec![
                "draft_campaign_adjustment",
                "draft_content_package",
                "update_nurture_sequence",
                "notify_sales_on_mql",
            ],
            "moderate",
        ),
        AgentKind::ComplianceOs => (
            "Compliance OS",
            "compliance",
            "Monitor violations, prepare filings, maintain audit trails, alert policy changes, and assign training.",
            vec![
                "compliance.*",
                "transaction.*",
                "compliance.filing",
                "compliance.policy_change",
                "compliance.training",
            ],
            vec![
                "open_compliance_case",
                "draft_regulatory_filing",
                "update_audit_trail",
                "assign_training",
            ],
            "high",
        ),
        AgentKind::LegalOs => (
            "Legal OS",
            "legal",
            "Review contracts, generate standard drafts, track renewals, analyze clauses, and assist legal research.",
            vec![
                "legal.*",
                "contract.*",
                "legal.generate",
                "legal.deadline",
                "legal.renewal",
            ],
            vec![
                "open_legal_review",
                "draft_standard_contract",
                "schedule_legal_deadline",
            ],
            "high",
        ),
        AgentKind::FinanceOs => (
            "Finance OS",
            "finance",
            "Automate bookkeeping, fraud review, AP/AR, cash forecasting, expense reports, and tax preparation.",
            vec![
                "finance.*",
                "transaction.*",
                "finance.bookkeeping",
                "finance.reconciliation",
                "finance.cash",
                "finance.expense",
                "finance.tax",
            ],
            vec![
                "hold_and_review_transaction",
                "draft_bookkeeping_entry",
                "open_cash_intervention",
                "draft_expense_or_tax_review",
            ],
            "high",
        ),
        AgentKind::MeetingDestruction => (
            "Meeting Destruction Engine",
            "productivity",
            "Price recurring meetings and replace information-share meetings with async briefs.",
            vec!["calendar.meeting"],
            vec!["publish_async_brief"],
            "moderate",
        ),
        AgentKind::ShadowWorkEliminator => (
            "Shadow Work Eliminator",
            "productivity",
            "Detect repeated questions, approval chasing, and reformatting work.",
            vec!["work.shadow_pattern"],
            vec!["create_automation"],
            "moderate",
        ),
        AgentKind::OrganizationalDebtAuditor => (
            "Organizational Debt Auditor",
            "operations",
            "Quantify obsolete, over-approved, and manually duplicated processes.",
            vec!["process.observation"],
            vec!["simplify_process"],
            "high",
        ),
        AgentKind::RevenueLeakDetector => (
            "Revenue Leak Detector",
            "finance",
            "Cross-check delivered and invoiced value to find uncaptured revenue.",
            vec!["finance.delivery_billing"],
            vec!["draft_invoice_correction"],
            "high",
        ),
        AgentKind::ScopeCreepEnforcer => (
            "Scope Creep Enforcer",
            "delivery",
            "Compare client requests with contracted scope before work begins.",
            vec!["project.scope_request"],
            vec!["draft_change_order"],
            "high",
        ),
        AgentKind::VendorIntelligenceNegotiator => (
            "Vendor Intelligence & Auto-Negotiator",
            "procurement",
            "Find unused seats and pricing gaps, then draft evidence-backed negotiations.",
            vec!["vendor.usage"],
            vec!["draft_vendor_negotiation"],
            "high",
        ),
        AgentKind::EmployeeChurnRadar => (
            "Employee Churn Radar",
            "hr",
            "Surface retention risk for confidential human review without adverse automated decisions.",
            vec!["hr.retention_signals"],
            vec!["create_retention_review"],
            "high",
        ),
        AgentKind::DecisionMemory => (
            "Decision Memory Agent",
            "leadership",
            "Record decisions, rationale, and outcome-review dates.",
            vec!["decision.recorded"],
            vec!["schedule_decision_review"],
            "moderate",
        ),
        AgentKind::RegulatoryHorizonScanner => (
            "Regulatory Horizon Scanner",
            "compliance",
            "Convert upcoming regulatory changes into business impact and action plans.",
            vec!["regulation.upcoming"],
            vec!["create_compliance_roadmap"],
            "high",
        ),
        AgentKind::CashFlowSentinel => (
            "Cash Flow Sentinel",
            "finance",
            "Forecast cash and surface shortfalls while there is time to respond.",
            vec!["finance.cash_snapshot"],
            vec!["create_cash_intervention"],
            "high",
        ),
        AgentKind::BusinessBrain => (
            "Business Brain",
            "strategy",
            "Interview founders, map skills/assets/network DNA, and generate a 90-day revenue roadmap.",
            vec!["business_brain.profile"],
            vec!["create_business_brain_plan"],
            "moderate",
        ),
        AgentKind::DormantAssetMonetization => (
            "Dormant Asset Monetization Agent",
            "revenue_discovery",
            "Find idle IP, capacity, data, expertise, space, and relationships that can become revenue.",
            vec!["revenue.asset_inventory"],
            vec!["draft_asset_monetization_offer"],
            "moderate",
        ),
        AgentKind::CustomerWalletShareMaximizer => (
            "Customer Wallet Share Maximizer",
            "revenue_discovery",
            "Compare current customer spend with likely category spend and draft expansion offers.",
            vec!["revenue.customer_spend", "crm.customer_wallet"],
            vec!["draft_wallet_share_expansion"],
            "moderate",
        ),
        AgentKind::DataProductCreator => (
            "Data Product Creator",
            "revenue_discovery",
            "Identify compliant aggregated data products and buyer segments.",
            vec!["revenue.data_inventory"],
            vec!["draft_data_product_prospectus"],
            "high",
        ),
        AgentKind::NewRevenueStreamArchitect => (
            "New Revenue Stream Architect",
            "revenue_creation",
            "Score new revenue model archetypes against capabilities and customers.",
            vec!["revenue.capability_inventory"],
            vec!["draft_revenue_stream_pilot"],
            "moderate",
        ),
        AgentKind::PricingPowerAgent => (
            "Pricing Power Agent",
            "revenue_creation",
            "Find willingness-to-pay-backed price increases with churn guardrails.",
            vec!["revenue.pricing"],
            vec!["draft_repricing_experiment"],
            "high",
        ),
        AgentKind::PartnershipRevenueGenerator => (
            "Partnership Revenue Generator",
            "revenue_creation",
            "Find overlapping audiences and draft revenue-share partnerships.",
            vec!["revenue.partner_signal"],
            vec!["draft_partnership_outreach"],
            "moderate",
        ),
        AgentKind::WhiteLabelRevenueMultiplier => (
            "White-Label Revenue Multiplier",
            "revenue_creation",
            "Package internal software, process, brand, or training assets for licensing.",
            vec!["revenue.licensing_asset"],
            vec!["draft_white_label_package"],
            "moderate",
        ),
        AgentKind::MarketTimingOracle => (
            "Market Timing Oracle",
            "strategic_intelligence",
            "Score market-entry, launch, acquisition, or raise timing from evidence-backed signals.",
            vec!["market.timing_signal"],
            vec!["draft_market_timing_plan"],
            "high",
        ),
        AgentKind::CompetitiveWeaknessExploiter => (
            "Competitive Weakness Exploiter",
            "strategic_intelligence",
            "Turn verified competitor weakness signals into compliant displacement campaigns.",
            vec!["market.competitor_weakness"],
            vec!["draft_competitor_displacement_campaign"],
            "high",
        ),
        AgentKind::MarketCategoryCreator => (
            "Market Category Creator",
            "strategic_intelligence",
            "Find category gaps and create a category-definition playbook.",
            vec!["market.category_gap"],
            vec!["draft_category_creation_playbook"],
            "high",
        ),
        AgentKind::SubscriptionEconomyConverter => (
            "Subscription Economy Converter",
            "business_model",
            "Convert one-time revenue into recurring subscription tiers and migration plans.",
            vec!["business_model.subscription"],
            vec!["draft_subscription_migration"],
            "moderate",
        ),
        AgentKind::NetworkEffectBuilder => (
            "Network Effect Builder",
            "business_model",
            "Design data, marketplace, collaboration, or benchmark loops that compound value.",
            vec!["business_model.network_effect"],
            vec!["draft_network_effect_pilot"],
            "moderate",
        ),
        AgentKind::ExitValueMaximizer => (
            "Exit Value Maximizer",
            "exit_and_scale",
            "Reverse-engineer target valuation into metric and execution roadmaps.",
            vec!["exit.valuation_goal"],
            vec!["draft_exit_value_roadmap"],
            "high",
        ),
        AgentKind::StrategicAcquirerIntelligence => (
            "Strategic Acquirer Intelligence Agent",
            "exit_and_scale",
            "Map likely acquirers and build evidence-backed positioning plans.",
            vec!["exit.acquirer_signal"],
            vec!["draft_acquirer_positioning_plan"],
            "high",
        ),
        AgentKind::BrandAuthorityCompound => (
            "Brand Authority Compound Agent",
            "exit_and_scale",
            "Build founder and company authority through proof assets, media, analysts, and events.",
            vec!["brand.authority_signal"],
            vec!["draft_authority_calendar"],
            "moderate",
        ),
    };
    AgentDefinition {
        kind,
        key: kind.key().into(),
        name: name.into(),
        department: department.into(),
        purpose: purpose.into(),
        accepted_event_types: events.into_iter().map(str::to_string).collect(),
        default_action_types: actions.into_iter().map(str::to_string).collect(),
        risk_class: risk.into(),
    }
}

fn business_brain_questions() -> Vec<BusinessBrainQuestion> {
    let rows = [
        (
            "q01",
            "skills",
            "What work can you do better than most people you know?",
        ),
        (
            "q02",
            "skills",
            "What industry knowledge have you built through lived experience?",
        ),
        (
            "q03",
            "skills",
            "Which problems do people already ask you to help solve?",
        ),
        (
            "q04",
            "skills",
            "What technical, creative, sales, operational, or domain skills do you have?",
        ),
        (
            "q05",
            "skills",
            "What credentials, proof, portfolio, or past results can customers trust?",
        ),
        (
            "q06",
            "skills",
            "Which tasks energize you enough to repeat every week?",
        ),
        (
            "q07",
            "skills",
            "Which tasks should the business avoid because you are weak at them?",
        ),
        (
            "q08",
            "skills",
            "What can you teach, package, automate, advise on, or deliver for money?",
        ),
        (
            "q09",
            "assets",
            "What savings, equipment, software, IP, space, audience, or inventory do you control?",
        ),
        (
            "q10",
            "assets",
            "What data, templates, processes, or proprietary know-how have you accumulated?",
        ),
        (
            "q11",
            "assets",
            "What unused capacity do you have each week?",
        ),
        (
            "q12",
            "assets",
            "What content, curriculum, community, or brand assets already exist?",
        ),
        (
            "q13",
            "assets",
            "What supplier, distribution, or operational advantages can you access?",
        ),
        (
            "q14",
            "assets",
            "What proof assets can be shown to early buyers?",
        ),
        (
            "q15",
            "assets",
            "How much capital can be risked without threatening survival?",
        ),
        (
            "q16",
            "assets",
            "What assets must remain protected or private?",
        ),
        (
            "q17",
            "network",
            "Who already trusts you enough to take a meeting?",
        ),
        (
            "q18",
            "network",
            "Which customer groups can you reach fastest?",
        ),
        (
            "q19",
            "network",
            "Which experts, partners, suppliers, or influencers can help?",
        ),
        (
            "q20",
            "network",
            "How many warm prospects can you contact in the next 14 days?",
        ),
        (
            "q21",
            "network",
            "Which communities, schools, companies, or associations do you know?",
        ),
        (
            "q22",
            "network",
            "Who could refer your first ten customers?",
        ),
        (
            "q23",
            "network",
            "Which buyers have urgent budget and trust your judgment?",
        ),
        (
            "q24",
            "network",
            "Which relationships are sensitive and require careful boundaries?",
        ),
        (
            "q25",
            "problem",
            "What painful problem have you personally experienced?",
        ),
        (
            "q26",
            "problem",
            "Who has this problem repeatedly and can pay to solve it?",
        ),
        (
            "q27",
            "problem",
            "What do people currently use as a workaround?",
        ),
        (
            "q28",
            "problem",
            "What outcome would make the buyer say this was worth paying for?",
        ),
        ("q29", "problem", "How often does the problem occur?"),
        ("q30", "problem", "How expensive is the problem if ignored?"),
        ("q31", "problem", "What makes the problem urgent right now?"),
        (
            "q32",
            "problem",
            "What proof would convince a skeptical buyer?",
        ),
        (
            "q33",
            "execution",
            "How many hours per week can you commit for 90 days?",
        ),
        (
            "q34",
            "execution",
            "What monthly revenue target are you aiming for?",
        ),
        (
            "q35",
            "execution",
            "What price would feel realistic for the first offer?",
        ),
        (
            "q36",
            "execution",
            "What delivery format is easiest: service, product, course, subscription, or marketplace?",
        ),
        (
            "q37",
            "execution",
            "What legal, compliance, family, job, or capital constraints matter?",
        ),
        (
            "q38",
            "execution",
            "What is the smallest offer you can deliver in seven days?",
        ),
        ("q39", "execution", "What will you measure every week?"),
        (
            "q40",
            "execution",
            "What would make you stop, pivot, or double down?",
        ),
    ];
    rows.into_iter()
        .map(|(question_id, category, prompt)| BusinessBrainQuestion {
            question_id: question_id.into(),
            category: category.into(),
            prompt: prompt.into(),
        })
        .collect()
}

fn build_business_brain_profile(interview: &BusinessBrainInterview) -> BusinessBrainProfile {
    let skills = terms_for_category(interview, "skills", "domain expertise");
    let assets = terms_for_category(interview, "assets", "existing assets");
    let network = terms_for_category(interview, "network", "warm network");
    let constraints = terms_for_category(interview, "execution", "limited time");
    let skill_score = score_category(interview, "skills");
    let asset_score = score_category(interview, "assets");
    let network_score = score_category(interview, "network");
    let problem_score = score_category(interview, "problem");
    let execution_score = score_category(interview, "execution");
    let mut founder_dna_scores = BTreeMap::new();
    founder_dna_scores.insert("skills".into(), skill_score);
    founder_dna_scores.insert("assets".into(), asset_score);
    founder_dna_scores.insert("network".into(), network_score);
    founder_dna_scores.insert("problem".into(), problem_score);
    founder_dna_scores.insert("execution".into(), execution_score);
    let average =
        (skill_score + asset_score + network_score + problem_score + execution_score) / 5.0;
    let profile_id = new_id("business_brain_profile");
    let run_id = format!("business_brain:{}", interview.interview_id);
    let opportunities = vec![
        business_brain_opportunity(
            interview,
            &run_id,
            "skill_to_service",
            "Turn strongest skill into a paid specialist service",
            interview.target_revenue
                * (skill_score * 0.35 + network_score * 0.25).clamp(0.05, 0.85),
            average,
        ),
        business_brain_opportunity(
            interview,
            &run_id,
            "asset_to_product",
            "Package existing assets into a repeatable product",
            interview.target_revenue * (asset_score * 0.4 + problem_score * 0.2).clamp(0.05, 0.8),
            average * 0.95,
        ),
        business_brain_opportunity(
            interview,
            &run_id,
            "network_to_distribution",
            "Use warm network to pre-sell the first offer",
            interview.target_revenue
                * (network_score * 0.45 + execution_score * 0.2).clamp(0.05, 0.75),
            average * 0.9,
        ),
    ];
    BusinessBrainProfile {
        profile_id,
        organization_id: interview.organization_id.clone(),
        interview_id: interview.interview_id.clone(),
        founder_name: interview.founder_name.clone(),
        target_revenue: interview.target_revenue,
        skills,
        assets,
        network,
        constraints,
        founder_dna_scores,
        opportunities,
        created_at_ms: now_ms(),
    }
}

fn business_brain_opportunity(
    interview: &BusinessBrainInterview,
    run_id: &str,
    opportunity_type: &str,
    title: &str,
    estimated: f64,
    confidence: f64,
) -> RevenueOpportunity {
    RevenueOpportunity {
        opportunity_id: new_id("nexus_revenue_opportunity"),
        organization_id: interview.organization_id.clone(),
        run_id: run_id.into(),
        agent: AgentKind::BusinessBrain,
        opportunity_type: opportunity_type.into(),
        title: title.into(),
        evidence: serde_json::json!({
            "interview_id": interview.interview_id,
            "answered_questions": interview.answers.len(),
        }),
        estimated_annual_revenue: estimated.max(0.0),
        confidence: confidence.clamp(0.0, 1.0),
        required_inputs: Vec::new(),
        risk: "moderate".into(),
        experiment: Some(experiment(
            format!("Validate {title}"),
            "warm reachable buyers",
            vec![
                "Contact 10 warm prospects",
                "Ask for paid commitment before building",
                "Deliver a small paid pilot",
            ],
            "paid validation conversations",
            estimated / 4.0,
        )),
        created_at_ms: now_ms(),
    }
}

fn build_business_brain_plan(
    interview: &BusinessBrainInterview,
    profile: &BusinessBrainProfile,
) -> BusinessBrainPlan {
    let selected = profile
        .opportunities
        .iter()
        .max_by(|left, right| {
            left.estimated_annual_revenue
                .total_cmp(&right.estimated_annual_revenue)
        })
        .expect("profile always has opportunities");
    let monthly_target = interview.target_revenue / 12.0;
    BusinessBrainPlan {
        plan_id: new_id("business_brain_plan"),
        organization_id: interview.organization_id.clone(),
        interview_id: interview.interview_id.clone(),
        selected_opportunity_id: selected.opportunity_id.clone(),
        revenue_target: interview.target_revenue,
        ninety_day_roadmap: vec![
            BusinessBrainMilestone {
                day: 7,
                title: "Validate the buyer pain".into(),
                deliverable: "10 buyer interviews and one refined paid offer".into(),
                revenue_target: 0.0,
            },
            BusinessBrainMilestone {
                day: 30,
                title: "Close first paid pilots".into(),
                deliverable: "3 paid pilots with clear success criteria".into(),
                revenue_target: monthly_target * 0.15,
            },
            BusinessBrainMilestone {
                day: 60,
                title: "Standardize delivery".into(),
                deliverable: "Repeatable fulfillment checklist, proof, and referral loop".into(),
                revenue_target: monthly_target * 0.45,
            },
            BusinessBrainMilestone {
                day: 90,
                title: "Scale the winning motion".into(),
                deliverable: "Documented acquisition channel and monthly revenue dashboard".into(),
                revenue_target: monthly_target,
            },
        ],
        daily_tasks: vec![
            "Contact one warm prospect or partner every day.".into(),
            "Record one buyer objection and update the offer.".into(),
            "Ship one proof asset, script, demo, or delivery improvement.".into(),
            "Track calls, paid commitments, delivery time, revenue, and referrals.".into(),
        ],
        scripts: vec![
            BusinessBrainScript {
                script_id: new_id("business_brain_script"),
                audience: "warm prospect".into(),
                purpose: "validation".into(),
                body: format!(
                    "I am testing a focused offer around '{}'. Can I ask what this problem costs you today and whether a small paid pilot would be useful?",
                    selected.title
                ),
            },
            BusinessBrainScript {
                script_id: new_id("business_brain_script"),
                audience: "referral partner".into(),
                purpose: "distribution".into(),
                body: "I am looking for 3 businesses with this problem. If someone comes to mind, I can share a short diagnostic and give them a concrete next step.".into(),
            },
        ],
        milestones: vec![
            "10 interviews completed".into(),
            "3 paid pilots sold".into(),
            "1 repeatable offer page created".into(),
            "First monthly revenue target reached or pivot trigger documented".into(),
        ],
        review_cadence: "weekly plan review with revenue, objections, proof, and next actions".into(),
        created_at_ms: now_ms(),
    }
}

fn terms_for_category(
    interview: &BusinessBrainInterview,
    category: &str,
    fallback: &str,
) -> Vec<String> {
    let mut terms = answers_for_category(interview, category)
        .split(|character: char| character == ',' || character == ';' || character == '\n')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .take(8)
        .map(str::to_string)
        .collect::<Vec<_>>();
    if terms.is_empty() {
        terms.push(fallback.into());
    }
    terms
}

fn score_category(interview: &BusinessBrainInterview, category: &str) -> f64 {
    let text = answers_for_category(interview, category);
    let words = text.split_whitespace().count() as f64;
    (0.25 + words / 120.0).clamp(0.25, 0.95)
}

fn answers_for_category(interview: &BusinessBrainInterview, category: &str) -> String {
    let question_ids = interview
        .questions
        .iter()
        .filter(|question| question.category == category)
        .map(|question| question.question_id.as_str())
        .collect::<BTreeSet<_>>();
    interview
        .answers
        .iter()
        .filter(|(question_id, _)| question_ids.contains(question_id.as_str()))
        .map(|(_, answer)| answer.as_str())
        .collect::<Vec<_>>()
        .join("\n")
}

fn supported_integrations() -> &'static [&'static str] {
    &[
        "salesforce",
        "hubspot",
        "pipedrive",
        "gmail",
        "outlook",
        "email",
        "google_calendar",
        "microsoft_calendar",
        "slack",
        "teams",
        "zendesk",
        "intercom",
        "freshdesk",
        "whatsapp",
        "sap",
        "oracle",
        "tally",
        "quickbooks",
        "zoho_books",
        "stripe",
        "razorpay",
        "banks",
        "google_sheets",
        "workday",
        "greenhouse",
        "lever",
        "naukri",
        "indeed",
        "docusign",
        "pandadoc",
        "google_drive",
        "sharepoint",
        "mailchimp",
        "google_analytics",
        "meta_ads",
        "linkedin_ads",
        "linkedin",
        "calendly",
        "government_portals",
        "generic_webhook",
    ]
}

fn integration_category(provider: &str) -> &'static str {
    match provider {
        "salesforce" | "hubspot" | "pipedrive" | "linkedin" | "calendly" => "sales",
        "gmail" | "outlook" | "google_calendar" | "microsoft_calendar" => "productivity",
        "slack" | "teams" | "whatsapp" | "email" => "collaboration",
        "zendesk" | "intercom" | "freshdesk" => "support",
        "sap" | "oracle" | "tally" | "quickbooks" | "zoho_books" => "finance_operations",
        "stripe" | "razorpay" | "banks" => "payments_finance",
        "google_sheets" => "documents",
        "workday" | "greenhouse" | "lever" | "naukri" | "indeed" => "hr",
        "docusign" | "pandadoc" => "legal",
        "google_drive" | "sharepoint" => "storage",
        "mailchimp" | "google_analytics" | "meta_ads" | "linkedin_ads" => "marketing",
        "government_portals" => "compliance",
        _ => "generic",
    }
}

fn integration_agent_keys(provider: &str) -> Vec<String> {
    let agents: &[AgentKind] = match integration_category(provider) {
        "sales" => &[AgentKind::SalesOs, AgentKind::MarketingOs],
        "productivity" => &[
            AgentKind::SalesOs,
            AgentKind::MeetingDestruction,
            AgentKind::DecisionMemory,
        ],
        "collaboration" => &[
            AgentKind::SupportOs,
            AgentKind::ShadowWorkEliminator,
            AgentKind::DecisionMemory,
        ],
        "support" => &[AgentKind::SupportOs],
        "finance_operations" => &[
            AgentKind::OperationsOs,
            AgentKind::FinanceOs,
            AgentKind::RevenueLeakDetector,
            AgentKind::CashFlowSentinel,
        ],
        "payments_finance" => &[
            AgentKind::FinanceOs,
            AgentKind::RevenueLeakDetector,
            AgentKind::CashFlowSentinel,
        ],
        "documents" | "storage" => &[
            AgentKind::OperationsOs,
            AgentKind::LegalOs,
            AgentKind::DecisionMemory,
        ],
        "hr" => &[AgentKind::HrOs, AgentKind::EmployeeChurnRadar],
        "legal" => &[AgentKind::LegalOs],
        "marketing" => &[AgentKind::MarketingOs, AgentKind::BrandAuthorityCompound],
        "compliance" => &[AgentKind::ComplianceOs, AgentKind::RegulatoryHorizonScanner],
        _ => &[AgentKind::BusinessBrain],
    };
    agents.iter().map(|agent| agent.key().to_string()).collect()
}

fn integration_scopes(provider: &str) -> Vec<String> {
    match integration_category(provider) {
        "sales" => vec!["read_crm".into(), "write_tasks_after_approval".into()],
        "productivity" => vec!["read_calendar_email".into(), "send_after_approval".into()],
        "collaboration" => vec!["read_threads".into(), "post_after_approval".into()],
        "support" => vec!["read_tickets".into(), "draft_replies".into()],
        "finance_operations" => vec!["read_finance_ops".into(), "draft_records".into()],
        "payments_finance" => vec![
            "read_transactions".into(),
            "no_payment_without_approval".into(),
        ],
        "documents" | "storage" => vec![
            "read_documents".into(),
            "write_drafts_after_approval".into(),
        ],
        "hr" => vec!["read_candidates".into(), "no_adverse_action".into()],
        "legal" => vec!["read_contracts".into(), "draft_legal_docs".into()],
        "marketing" => vec!["read_campaigns".into(), "draft_content".into()],
        "compliance" => vec![
            "read_controls".into(),
            "draft_filings_after_approval".into(),
        ],
        _ => vec!["read_events".into()],
    }
}

fn experiment(
    hypothesis: impl Into<String>,
    target_segment: impl Into<String>,
    action_steps: Vec<&str>,
    kpi: impl Into<String>,
    expected_value: f64,
) -> RevenueExperiment {
    RevenueExperiment {
        experiment_id: new_id("nexus_revenue_experiment"),
        hypothesis: hypothesis.into(),
        target_segment: target_segment.into(),
        action_steps: action_steps.into_iter().map(str::to_string).collect(),
        kpi: kpi.into(),
        expected_value: expected_value.max(0.0),
        rollback: serde_json::json!({
            "action": "stop_experiment_and_restore_previous_offer",
            "required": true
        }),
        created_at_ms: now_ms(),
    }
}

fn missing_inputs(value: &serde_json::Value, required: &[&str]) -> Vec<String> {
    required
        .iter()
        .filter(|key| {
            value
                .get(**key)
                .is_none_or(|value| value.is_null() || value.as_str() == Some(""))
        })
        .map(|key| (*key).to_string())
        .collect()
}

fn number(value: &serde_json::Value, key: &str) -> f64 {
    value
        .get(key)
        .and_then(serde_json::Value::as_f64)
        .unwrap_or_default()
}

fn number_or(value: &serde_json::Value, key: &str, default: f64) -> f64 {
    value
        .get(key)
        .and_then(serde_json::Value::as_f64)
        .unwrap_or(default)
}

fn string(value: &serde_json::Value, key: &str) -> String {
    value
        .get(key)
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .to_ascii_lowercase()
}

fn string_or(value: &serde_json::Value, key: &str, default: &str) -> String {
    value
        .get(key)
        .and_then(serde_json::Value::as_str)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| default.to_string())
}

fn event_type_contains(event: &BusinessEvent, needle: &str) -> bool {
    event
        .event_type
        .to_ascii_lowercase()
        .contains(&needle.to_ascii_lowercase())
}

fn bool_value(value: &serde_json::Value, key: &str) -> Option<bool> {
    value.get(key).and_then(serde_json::Value::as_bool)
}

fn string_set(value: &serde_json::Value, key: &str) -> BTreeSet<String> {
    value
        .get(key)
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(serde_json::Value::as_str)
        .map(|value| value.trim().to_ascii_lowercase())
        .collect()
}

fn hourly_rate(value: &serde_json::Value) -> f64 {
    number(value, "hourly_rate").max(25.0)
}

fn weighted_score(value: &serde_json::Value, keys: &[&str]) -> f64 {
    if keys.is_empty() {
        return 0.0;
    }
    keys.iter()
        .map(|key| number(value, key).clamp(0.0, 1.0))
        .sum::<f64>()
        / keys.len() as f64
}

fn redact_json(value: serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(map) => serde_json::Value::Object(
            map.into_iter()
                .map(|(key, value)| {
                    let lower = key.to_ascii_lowercase();
                    if ["password", "secret", "token", "api_key", "private_key"]
                        .iter()
                        .any(|needle| lower.contains(needle))
                    {
                        (
                            key,
                            serde_json::Value::String(format!(
                                "redacted:sha3:{}",
                                &sha3_hex(value.to_string().as_bytes())[..20]
                            )),
                        )
                    } else {
                        (key, redact_json(value))
                    }
                })
                .collect(),
        ),
        serde_json::Value::Array(values) => {
            serde_json::Value::Array(values.into_iter().map(redact_json).collect())
        }
        other => other,
    }
}

fn default_plan() -> PlanTier {
    PlanTier::Starter
}

fn default_currency() -> String {
    "USD".into()
}

fn default_timezone() -> String {
    "UTC".into()
}

fn default_true() -> bool {
    true
}

fn sql_error(error: rusqlite::Error) -> AppError {
    AppError::Internal(format!("Nexus persistence failed: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn service(label: &str) -> NexusService {
        NexusService::new(std::env::temp_dir().join(format!("nexus-test-{label}-{}", now_ms())), None)
            .expect("service")
    }

    fn organization(service: &NexusService) -> Organization {
        service
            .create_organization(
                "owner",
                CreateOrganizationRequest {
                    name: "Acme".into(),
                    plan: PlanTier::Enterprise,
                    currency: "USD".into(),
                    timezone: "UTC".into(),
                },
            )
            .expect("organization")
    }

    fn blueprint_event(event_type: &str, data: serde_json::Value) -> BusinessEvent {
        BusinessEvent {
            event_id: new_id("evt"),
            organization_id: "org_blueprint".into(),
            source: "test".into(),
            event_type: event_type.into(),
            subject_type: "subject".into(),
            subject_id: "subject_1".into(),
            data,
            idempotency_key: None,
            occurred_at_ms: now_ms(),
            ingested_at_ms: now_ms(),
        }
    }

    #[test]
    fn blueprint_saas_workflows_generate_specific_actions() {
        let cases = vec![
            (
                AgentKind::SalesOs,
                "sales.outreach",
                serde_json::json!({
                    "opened_email":0.9,
                    "visited_pricing":0.8,
                    "downloaded_resource":0.7,
                    "reply_intent":0.6,
                    "potential_deal_value":100_000.0
                }),
                "draft_outreach",
            ),
            (
                AgentKind::SalesOs,
                "sales.calendar",
                serde_json::json!({
                    "meeting_intent":"demo",
                    "minutes_saved":8.0,
                    "hourly_rate":75.0,
                    "annual_meetings":120.0
                }),
                "schedule_sales_meeting",
            ),
            (
                AgentKind::SalesOs,
                "sales.proposal",
                serde_json::json!({
                    "deal_value":250_000.0,
                    "win_rate_lift":0.1
                }),
                "draft_proposal",
            ),
            (
                AgentKind::SalesOs,
                "sales.deal_risk",
                serde_json::json!({
                    "days_quiet":1.0,
                    "competitor_mentioned":1.0,
                    "champion_left":0.0,
                    "price_objection":1.0,
                    "deal_value":200_000.0
                }),
                "open_deal_rescue",
            ),
            (
                AgentKind::SupportOs,
                "support.knowledge_update",
                serde_json::json!({
                    "resolution":"reset workspace token",
                    "future_ticket_deflection_value":5000.0
                }),
                "update_knowledge_base",
            ),
            (
                AgentKind::OperationsOs,
                "document.invoice",
                serde_json::json!({
                    "document_type":"invoice",
                    "target_system":"ERP",
                    "extraction_confidence":0.93,
                    "manual_processing_cost":1200.0
                }),
                "extract_document_fields",
            ),
            (
                AgentKind::OperationsOs,
                "operations.report",
                serde_json::json!({
                    "report_period":"weekly",
                    "hours_saved":8.0,
                    "hourly_rate":60.0,
                    "annual_runs":52.0
                }),
                "generate_report",
            ),
            (
                AgentKind::OperationsOs,
                "inventory.reorder",
                serde_json::json!({
                    "sku":"SKU-1",
                    "stockout_cost_avoided":18000.0
                }),
                "draft_purchase_order",
            ),
            (
                AgentKind::HrOs,
                "hr.interview",
                serde_json::json!({
                    "clarity":0.8,
                    "depth":0.7,
                    "communication":0.9,
                    "culture_fit":0.75,
                    "interview_time_saved":1000.0
                }),
                "draft_interview_summary",
            ),
            (
                AgentKind::HrOs,
                "hr.offer",
                serde_json::json!({
                    "coordination_cost_saved":1500.0
                }),
                "draft_offer_workflow",
            ),
            (
                AgentKind::MarketingOs,
                "marketing.content",
                serde_json::json!({
                    "topic":"workflow automation",
                    "agency_cost_avoided":20_000.0
                }),
                "draft_content_package",
            ),
            (
                AgentKind::MarketingOs,
                "marketing.nurture",
                serde_json::json!({
                    "email_engagement":0.8,
                    "page_visits":0.7,
                    "content_downloads":0.6,
                    "fit":0.9,
                    "potential_deal_value":80_000.0
                }),
                "update_nurture_sequence",
            ),
            (
                AgentKind::ComplianceOs,
                "compliance.filing",
                serde_json::json!({
                    "filing_name":"DPDP readiness report",
                    "missed_deadline_penalty":250_000.0
                }),
                "draft_regulatory_filing",
            ),
            (
                AgentKind::ComplianceOs,
                "compliance.policy_change",
                serde_json::json!({
                    "policy_name":"data retention",
                    "avoided_penalty":100_000.0
                }),
                "assign_training",
            ),
            (
                AgentKind::LegalOs,
                "legal.generate",
                serde_json::json!({
                    "contract_type":"NDA",
                    "legal_hours_saved":4.0,
                    "hourly_rate":200.0
                }),
                "draft_standard_contract",
            ),
            (
                AgentKind::LegalOs,
                "legal.renewal",
                serde_json::json!({
                    "matter":"vendor renewal",
                    "deadline":"2026-08-01",
                    "risk_exposure":50_000.0
                }),
                "schedule_legal_deadline",
            ),
            (
                AgentKind::FinanceOs,
                "finance.bookkeeping",
                serde_json::json!({
                    "category":"software",
                    "accounting_hours_saved":20.0,
                    "hourly_rate":80.0
                }),
                "draft_bookkeeping_entry",
            ),
            (
                AgentKind::FinanceOs,
                "finance.cash",
                serde_json::json!({
                    "cash":10_000.0,
                    "receivables_60d":5_000.0,
                    "payables_60d":20_000.0,
                    "monthly_burn":10_000.0
                }),
                "open_cash_intervention",
            ),
            (
                AgentKind::FinanceOs,
                "finance.expense",
                serde_json::json!({
                    "manual_cost_saved":1200.0
                }),
                "draft_expense_or_tax_review",
            ),
        ];

        for (kind, event_type, data, expected_action) in cases {
            let event = blueprint_event(event_type, data);
            let finding = analyze_event(kind, &event, "org_blueprint", "run_blueprint")
                .unwrap_or_else(|| panic!("{event_type} should produce a finding"));
            assert!(
                finding.estimated_annual_value > 0.0,
                "{event_type} should produce value"
            );
            let actions = actions_for_findings("org_blueprint", "run_blueprint", kind, &[finding]);
            assert_eq!(
                actions.first().map(|action| action.action_type.as_str()),
                Some(expected_action),
                "{event_type} should map to the listed workflow action"
            );
        }
    }

    #[test]
    fn ten_new_agent_blueprint_catalog_is_complete() {
        let keys = AgentKind::ALL
            .iter()
            .map(|kind| kind.key())
            .collect::<BTreeSet<_>>();
        for key in [
            "meeting_destruction",
            "shadow_work_eliminator",
            "organizational_debt_auditor",
            "revenue_leak_detector",
            "scope_creep_enforcer",
            "vendor_intelligence_negotiator",
            "employee_churn_radar",
            "decision_memory",
            "regulatory_horizon_scanner",
            "cash_flow_sentinel",
        ] {
            assert!(keys.contains(key), "{key} missing from catalog");
        }
    }

    fn event_for(kind: AgentKind) -> BusinessEvent {
        let data = match kind {
            AgentKind::BusinessBrain => serde_json::json!({
                "skills_score":0.8,
                "assets_score":0.7,
                "network_score":0.75,
                "problem_score":0.85,
                "execution_score":0.8,
                "target_revenue":1_200_000.0,
                "target_customer":"school owners"
            }),
            AgentKind::DormantAssetMonetization => serde_json::json!({
                "idle_capacity_hours":100.0,
                "capacity_rate":500.0,
                "buyer_segment":"nearby ecommerce sellers"
            }),
            AgentKind::CustomerWalletShareMaximizer => serde_json::json!({
                "current_spend":100_000.0,
                "estimated_category_spend":240_000.0,
                "capture_rate":0.3,
                "customer_segment":"enterprise accounts"
            }),
            AgentKind::DataProductCreator => serde_json::json!({
                "buyer_count":4.0,
                "price_per_buyer":20_000.0,
                "compliance_score":0.9,
                "buyer_segment":"benchmark subscribers"
            }),
            AgentKind::NewRevenueStreamArchitect => serde_json::json!({
                "customer_count":500.0,
                "new_stream_arpu":1000.0,
                "attach_rate":0.2
            }),
            AgentKind::PricingPowerAgent => serde_json::json!({
                "current_revenue":1_000_000.0,
                "price_increase_pct":0.12,
                "churn_risk":0.03,
                "segment_wtp_score":0.8
            }),
            AgentKind::PartnershipRevenueGenerator => serde_json::json!({
                "partner_customer_overlap":1000.0,
                "average_deal_value":25_000.0,
                "conversion_rate":0.04,
                "partners":2.0
            }),
            AgentKind::WhiteLabelRevenueMultiplier => serde_json::json!({
                "licensee_count":5.0,
                "annual_license_fee":200_000.0,
                "enablement_cost":100_000.0
            }),
            AgentKind::MarketTimingOracle => serde_json::json!({
                "market_size":50_000_000.0,
                "timing_score":0.7,
                "entry_capture_rate":0.01,
                "market_segment":"edtech buyers"
            }),
            AgentKind::CompetitiveWeaknessExploiter => serde_json::json!({
                "semantic_render_proof_hash":"proof",
                "affected_customers":120.0,
                "annual_contract_value":50_000.0,
                "expected_capture_rate":0.1
            }),
            AgentKind::MarketCategoryCreator => serde_json::json!({
                "addressable_market":100_000_000.0,
                "differentiation_score":0.8,
                "category_capture_rate":0.005
            }),
            AgentKind::SubscriptionEconomyConverter => serde_json::json!({
                "customer_count":50.0,
                "monthly_subscription_price":1500.0,
                "migration_rate":0.6
            }),
            AgentKind::NetworkEffectBuilder => serde_json::json!({
                "customer_count":500.0,
                "arpu":2000.0,
                "network_feature_readiness":0.75,
                "viral_coefficient":0.15
            }),
            AgentKind::ExitValueMaximizer => serde_json::json!({
                "target_valuation":200_000_000.0,
                "current_arr":5_000_000.0,
                "current_multiple":8.0
            }),
            AgentKind::StrategicAcquirerIntelligence => serde_json::json!({
                "semantic_render_proof_hash":"proof",
                "acquirer_count":10.0,
                "current_arr":5_000_000.0,
                "strategic_fit_score":0.8,
                "target_multiple":10.0
            }),
            AgentKind::BrandAuthorityCompound => serde_json::json!({
                "semantic_render_proof_hash":"proof",
                "audience_size":10000.0,
                "average_deal_value":40_000.0,
                "conversion_rate":0.01,
                "authority_gap_score":0.7,
                "external_claims":1.0
            }),
            _ => serde_json::json!({}),
        };
        BusinessEvent {
            event_id: new_id("test_event"),
            organization_id: "org_test".into(),
            source: "test".into(),
            event_type: agent_definition(kind)
                .accepted_event_types
                .first()
                .cloned()
                .unwrap_or_else(|| "test.event".into())
                .replace(".*", ".sample"),
            subject_type: "account".into(),
            subject_id: format!("subject_{}", kind.key()),
            data,
            idempotency_key: None,
            occurred_at_ms: now_ms(),
            ingested_at_ms: now_ms(),
        }
    }

    fn enable(service: &NexusService, organization_id: &str, kind: AgentKind) {
        service
            .update_agent_config(
                "owner",
                organization_id,
                kind,
                UpdateAgentConfigurationRequest {
                    enabled: true,
                    auto_execute_low_risk: false,
                    settings: serde_json::json!({}),
                },
            )
            .expect("enable agent");
    }

    #[test]
    fn tenant_roles_prevent_cross_organization_reads() {
        let service = service("tenant");
        let org = organization(&service);
        let error = service
            .get_organization("stranger", &org.organization_id)
            .expect_err("stranger must not read tenant");
        assert!(matches!(error, AppError::Unauthorized));
    }

    #[test]
    fn catalog_contains_all_34_agents_and_preserves_existing_keys() {
        let service = service("catalog-34");
        let catalog = service.agent_catalog();
        assert_eq!(catalog.len(), 34);
        let keys = catalog
            .iter()
            .map(|agent| agent.key.as_str())
            .collect::<BTreeSet<_>>();
        assert!(keys.contains("revenue_leak_detector"));
        assert!(keys.contains("business_brain"));
        assert!(keys.contains("brand_authority_compound"));
        assert_eq!(
            AgentKind::from_key("business_brain").unwrap(),
            AgentKind::BusinessBrain
        );
    }

    #[test]
    fn every_new_revenue_agent_produces_typed_opportunity() {
        let new_agents = [
            AgentKind::BusinessBrain,
            AgentKind::DormantAssetMonetization,
            AgentKind::CustomerWalletShareMaximizer,
            AgentKind::DataProductCreator,
            AgentKind::NewRevenueStreamArchitect,
            AgentKind::PricingPowerAgent,
            AgentKind::PartnershipRevenueGenerator,
            AgentKind::WhiteLabelRevenueMultiplier,
            AgentKind::MarketTimingOracle,
            AgentKind::CompetitiveWeaknessExploiter,
            AgentKind::MarketCategoryCreator,
            AgentKind::SubscriptionEconomyConverter,
            AgentKind::NetworkEffectBuilder,
            AgentKind::ExitValueMaximizer,
            AgentKind::StrategicAcquirerIntelligence,
            AgentKind::BrandAuthorityCompound,
        ];
        for kind in new_agents {
            let event = event_for(kind);
            let findings = analyze(kind, &[event], "org_test", "run_test");
            assert_eq!(
                findings.len(),
                1,
                "{} should produce one finding",
                kind.key()
            );
            let finding = &findings[0];
            assert!(
                finding.required_inputs.is_empty(),
                "{} missing inputs",
                kind.key()
            );
            assert!(
                finding.estimated_annual_value > 0.0,
                "{} estimate",
                kind.key()
            );
            assert!(
                finding.revenue_opportunity.is_some(),
                "{} typed opportunity",
                kind.key()
            );
            assert!(
                finding.revenue_experiment.is_some(),
                "{} experiment",
                kind.key()
            );
        }
    }

    #[test]
    fn missing_revenue_inputs_return_required_fields_without_actions() {
        let service = service("missing-inputs");
        let org = organization(&service);
        enable(&service, &org.organization_id, AgentKind::PricingPowerAgent);
        let result = service
            .ingest_event(
                "owner",
                &org.organization_id,
                IngestBusinessEventRequest {
                    source: "billing".into(),
                    event_type: "revenue.pricing".into(),
                    subject_type: "segment".into(),
                    subject_id: "starter".into(),
                    data: serde_json::json!({"current_revenue": 100000.0}),
                    idempotency_key: None,
                    occurred_at_ms: None,
                },
            )
            .expect("event");
        let run = service
            .run_agent(
                "owner",
                &org.organization_id,
                AgentKind::PricingPowerAgent,
                RunAgentRequest {
                    objective: None,
                    event_ids: vec![result.event.event_id],
                },
            )
            .expect("run");
        assert_eq!(run.estimated_annual_value, 0.0);
        assert!(run.actions.is_empty());
        assert_eq!(run.findings[0].required_inputs, vec!["price_increase_pct"]);
    }

    #[test]
    fn business_brain_requires_40_answers_before_plan_generation() {
        let service = service("business-brain");
        let org = organization(&service);
        let interview = service
            .start_business_brain_interview(
                "owner",
                &org.organization_id,
                CreateBusinessBrainInterviewRequest {
                    founder_name: Some("Asha".into()),
                    target_revenue: Some(1_200_000.0),
                    context: serde_json::json!({"market":"education"}),
                },
            )
            .expect("interview");
        assert_eq!(interview.questions.len(), 40);
        let error = service
            .generate_business_brain_plan("owner", &org.organization_id, &interview.interview_id)
            .expect_err("plan should require answers");
        assert!(error.to_string().contains("40 answers"));

        let answers = interview
            .questions
            .iter()
            .map(|question| {
                (
                    question.question_id.clone(),
                    format!("Detailed answer for {}", question.category),
                )
            })
            .collect();
        let interview = service
            .submit_business_brain_answers(
                "owner",
                &org.organization_id,
                &interview.interview_id,
                SubmitBusinessBrainAnswersRequest { answers },
            )
            .expect("answers");
        assert_eq!(interview.status, "ready_for_plan");
        let planned = service
            .generate_business_brain_plan("owner", &org.organization_id, &interview.interview_id)
            .expect("plan");
        assert_eq!(planned.status, "planned");
        assert_eq!(planned.profile.as_ref().unwrap().opportunities.len(), 3);
        assert_eq!(planned.plan.as_ref().unwrap().ninety_day_roadmap.len(), 4);
    }

    #[test]
    fn revenue_leak_agent_calculates_real_recoverable_value() {
        let service = service("revenue");
        let org = organization(&service);
        service
            .update_agent_config(
                "owner",
                &org.organization_id,
                AgentKind::RevenueLeakDetector,
                UpdateAgentConfigurationRequest {
                    enabled: true,
                    auto_execute_low_risk: false,
                    settings: serde_json::json!({}),
                },
            )
            .expect("enable");
        let event = service
            .ingest_event(
                "owner",
                &org.organization_id,
                IngestBusinessEventRequest {
                    source: "erp".into(),
                    event_type: "finance.delivery_billing".into(),
                    subject_type: "customer".into(),
                    subject_id: "customer-1".into(),
                    data: serde_json::json!({"delivered_value": 125000.0, "invoiced_value": 100000.0}),
                    idempotency_key: Some("billing-1".into()),
                    occurred_at_ms: None,
                },
            )
            .expect("event")
            .event;
        let run = service
            .run_agent(
                "owner",
                &org.organization_id,
                AgentKind::RevenueLeakDetector,
                RunAgentRequest {
                    objective: None,
                    event_ids: vec![event.event_id],
                },
            )
            .expect("run");
        assert_eq!(run.estimated_annual_value, 25_000.0);
        assert_eq!(run.actions[0].status, "pending_approval");
    }

    #[test]
    fn workflow_and_coordination_trigger_on_ingestion() {
        let service = service("workflow");
        let org = organization(&service);
        service
            .create_workflow(
                "owner",
                &org.organization_id,
                CreateWorkflowRequest {
                    name: "Triage support".into(),
                    trigger_event_type: "support.sentiment".into(),
                    agent: AgentKind::SupportOs,
                    enabled: true,
                },
            )
            .expect("workflow");
        let result = service
            .ingest_event(
                "owner",
                &org.organization_id,
                IngestBusinessEventRequest {
                    source: "zendesk".into(),
                    event_type: "support.sentiment".into(),
                    subject_type: "customer".into(),
                    subject_id: "customer-2".into(),
                    data: serde_json::json!({"sentiment": -0.8, "answer_confidence": 0.4}),
                    idempotency_key: None,
                    occurred_at_ms: None,
                },
            )
            .expect("event");
        assert_eq!(result.triggered_runs.len(), 1);
        assert_eq!(result.coordination_actions.len(), 1);
    }

    #[test]
    fn credentials_and_event_secrets_are_redacted() {
        let service = service("secrets");
        let org = organization(&service);
        let integration = service
            .connect_integration(
                "owner",
                &org.organization_id,
                ConnectIntegrationRequest {
                    provider: "slack".into(),
                    display_name: None,
                    scopes: Vec::new(),
                    credential: Some("real-secret".into()),
                    configuration: serde_json::json!({"api_key":"never-store-this"}),
                },
            )
            .expect("integration");
        assert!(integration.credential_fingerprint.is_some());
        assert!(
            integration.configuration["api_key"]
                .as_str()
                .expect("redacted")
                .starts_with("redacted:sha3:")
        );
    }

    #[test]
    fn starter_plan_enforces_two_agent_limit() {
        let service = service("plan-limit");
        let org = service
            .create_organization(
                "owner",
                CreateOrganizationRequest {
                    name: "Starter".into(),
                    plan: PlanTier::Starter,
                    currency: "USD".into(),
                    timezone: "UTC".into(),
                },
            )
            .expect("organization");
        let error = service
            .update_agent_config(
                "owner",
                &org.organization_id,
                AgentKind::RevenueLeakDetector,
                UpdateAgentConfigurationRequest {
                    enabled: true,
                    auto_execute_low_risk: false,
                    settings: serde_json::json!({}),
                },
            )
            .expect_err("third agent must exceed starter limit");
        assert!(matches!(error, AppError::Forbidden(_)));
    }

    #[test]
    fn integration_can_be_disconnected_without_losing_audit_record() {
        let service = service("disconnect");
        let org = organization(&service);
        let integration = service
            .connect_integration(
                "owner",
                &org.organization_id,
                ConnectIntegrationRequest {
                    provider: "hubspot".into(),
                    display_name: None,
                    scopes: vec!["crm.read".into()],
                    credential: Some("secret".into()),
                    configuration: serde_json::json!({}),
                },
            )
            .expect("integration");
        let disconnected = service
            .disconnect_integration("owner", &org.organization_id, &integration.integration_id)
            .expect("disconnect");
        assert_eq!(disconnected.status, "disconnected");
    }
}
