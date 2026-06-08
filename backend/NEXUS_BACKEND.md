# Nexus B2B Automation Backend

Nexus is an additive multi-tenant B2B SaaS domain inside Astra. Existing assistant, research,
HTTPA, guardian, render, connector, and ASC-II capabilities remain available.

## Product Model

The two Nexus blueprints describe a shared enterprise data layer rather than isolated point
solutions. The backend implements that model as:

- organizations, memberships, tenant roles, and plan limits
- a shared business-event stream with idempotent ingestion
- 18 agent types: the eight departmental operating systems and ten new specialist agents
- event-triggered workflows and cross-department coordination
- evidence-backed findings, ROI estimates, and approval-gated proposed actions
- ASC-II strategic reasoning attached to every API-triggered agent run; sensitive departments stay local
- integration records that store credential fingerprints, never plaintext credentials
- WAL-mode SQLite persistence, usage metering, dashboards, and secret redaction

Every action remains a proposal until an approver authorizes it. The current backend does not
pretend that external systems changed merely because an action was approved. Provider-specific
delivery workers can consume approved actions when those connectors are implemented.

## Agents And Event Contracts

| Agent | Primary event type | Required data examples |
|---|---|---|
| Sales OS | `sales.*`, `crm.*` | `company_fit`, `engagement`, `intent`, `authority`, `potential_deal_value` |
| Support OS | `support.*` | `sentiment`, `answer_confidence`, `human_handle_cost` |
| Operations OS | `operations.*`, `inventory.*` | `expected_quantity`, `actual_quantity`, `unit_value`, `manual_entry` |
| HR OS | `hr.*`, `candidate.*` | `skills_match`, `experience_match`, `role_fit` |
| Marketing OS | `marketing.*`, `campaign.*` | `spend`, `attributed_revenue` |
| Compliance OS | `compliance.*`, `transaction.*` | `risk_score`, `avoided_penalty` |
| Legal OS | `legal.*`, `contract.*` | `risky_clauses`, `risk_exposure` |
| Finance OS | `finance.*`, `transaction.*` | `anomaly_score`, `amount` |
| Meeting Destruction | `calendar.meeting` | `attendees`, `duration_hours`, `average_annual_salary`, `annual_occurrences`, `meeting_type` |
| Shadow Work Eliminator | `work.shadow_pattern` | `pattern`, `repeat_count`, `minutes_per_occurrence`, `hourly_rate` |
| Organizational Debt Auditor | `process.observation` | `approval_steps`, `transaction_amount`, `monthly_occurrences`, `minutes_per_occurrence` |
| Revenue Leak Detector | `finance.delivery_billing` | `delivered_value`, `invoiced_value` |
| Scope Creep Enforcer | `project.scope_request` | `contracted_scope`, `requested_scope`, `change_order_value` |
| Vendor Intelligence | `vendor.usage` | `paid_seats`, `active_seats`, `annual_price_per_seat`, `market_price_per_seat` |
| Employee Churn Radar | `hr.retention_signals` | boolean retention signals and `replacement_cost` |
| Decision Memory | `decision.recorded` | `decision`, `rationale`, `estimated_value` |
| Regulatory Horizon Scanner | `regulation.upcoming` | `business_impact`, `deadline`, `avoided_penalty` |
| Cash Flow Sentinel | `finance.cash_snapshot` | `cash`, `receivables_60d`, `payables_60d`, `monthly_burn` |

Employee retention findings are explicitly advisory and require confidential human review. They
must never be used as the sole basis for an adverse employment decision.

## API Surface

All endpoints are under `/api/v1`. In production, provide `x-nexus-principal-id` and the
configured `x-astra-admin-token`; organization roles are `viewer`, `operator`, `approver`,
`admin`, and `owner`.

- `GET /nexus/agents/catalog`
- `POST /nexus/organizations`
- `GET /nexus/organizations`
- `GET /nexus/organizations/{organization_id}`
- `GET /nexus/organizations/{organization_id}/dashboard`
- `PUT /nexus/organizations/{organization_id}/plan`
- `POST|GET /nexus/organizations/{organization_id}/members`
- `GET /nexus/organizations/{organization_id}/agents`
- `PUT /nexus/organizations/{organization_id}/agents/{agent_key}`
- `POST /nexus/organizations/{organization_id}/agents/{agent_key}/runs`
- `GET /nexus/organizations/{organization_id}/runs/{run_id}`
- `POST|GET /nexus/organizations/{organization_id}/integrations`
- `DELETE /nexus/organizations/{organization_id}/integrations/{integration_id}`
- `POST|GET /nexus/organizations/{organization_id}/events`
- `POST|GET /nexus/organizations/{organization_id}/workflows`
- `GET /nexus/organizations/{organization_id}/actions`
- `POST /nexus/organizations/{organization_id}/actions/{action_id}/approve`
- `POST /nexus/organizations/{organization_id}/actions/{action_id}/reject`

## Cross-Department Coordination

The shared event layer currently implements these built-in coordination rules:

- frustrated support customer -> propose pausing sales upsell outreach
- high-risk legal contract -> propose holding finance payment
- won sales deal -> propose drafting a finance invoice
- newly hired employee -> propose provisioning approved systems

Additional coordination is created by defining workflows that map an exact event type to any
enabled agent.

## Example

Create an enterprise organization, enable the revenue leak agent, ingest ERP evidence, then run it:

```http
POST /api/v1/nexus/organizations
x-nexus-principal-id: founder

{"name":"Acme","plan":"enterprise","currency":"USD","timezone":"UTC"}
```

```http
POST /api/v1/nexus/organizations/{org}/events
x-nexus-principal-id: founder

{
  "source":"erp",
  "event_type":"finance.delivery_billing",
  "subject_type":"customer",
  "subject_id":"customer-42",
  "idempotency_key":"delivery-billing-42",
  "data":{"delivered_value":125000,"invoiced_value":100000}
}
```

The Revenue Leak Detector returns a `25000` recoverable-value finding and a pending,
rollback-described billing-correction action for approval.
