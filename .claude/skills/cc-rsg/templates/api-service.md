---
template_name: api-service
template_version: 0.1.0
last_updated: 2026-05-01
description: API service spec template. For microservices and public APIs that expose REST/GraphQL/gRPC.
---

# API service spec template

This template defines the chapter outline for the spec of a service whose endpoints are called by other systems.

Designed for API services, microservices, and public APIs over REST, GraphQL, gRPC, WebSocket, etc.

---

## Chapter outline

### Chapter 1: Overview

<!-- meta: purpose and scope of the API as a whole. -->

#### 1.1 API purpose
- The value the API provides
- Intended consumers (internal systems / partners / public)
- Position in the business

#### 1.2 Main use cases
- 3-5 representative scenarios

#### 1.3 Service composition diagram
- API Gateway / Load Balancer / Backend structure
- Dependencies on related services

---

### Chapter 2: Architecture overview

<!-- meta: technology choices and overall structure. -->

#### 2.1 Technology stack
- Language / framework (Spring Boot / Express / FastAPI / .NET, etc.)
- API style (REST / GraphQL / gRPC / WebSocket)
- API spec format (OpenAPI / GraphQL SDL / .proto)

#### 2.2 Internal architecture
- Layering (Controller / Service / Repository, etc.)
- Data stores (RDB / NoSQL / Cache)
- Messaging infrastructure

#### 2.3 Deployment topology
- Runtime (Kubernetes / ECS / Lambda, etc.)
- Scaling strategy

---

### Chapter 3: Endpoint catalogue

<!-- meta: inventory of all endpoints. The pillar of verification. -->

#### 3.1 Endpoint catalogue
| Endpoint ID | Method | Path | Summary | Auth | Version |
|---------------|---------|------|------|------|----------|
| EP-001 | GET | /v1/users/{id} | Get user | required | v1 |
| EP-002 | POST | /v1/users | Create user | required | v1 |
| ... | ... | ... | ... | ... | ... |

#### 3.2 Grouping by resource
- Organise endpoints by resource
- Relationships between resources

---

### Chapter 4: Request / response specifications

<!-- meta: per-endpoint details. If they can be generated from OpenAPI, reference only is acceptable. -->

For each endpoint, describe:

#### {Endpoint name}

##### Overview
- Purpose
- Use scenario

##### Request
- HTTP method + path
- Path parameters
- Query parameters
- Headers (required / optional)
- Request body (schema + example)

##### Response
- Success (2xx)
  - Status code
  - Response body (schema + example)
  - Response headers
- Error (4xx, 5xx)
  - Expected error codes
  - Error response body

##### Side effects
- Database updates
- Calls to external systems
- Events published

##### Idempotency
- Whether the endpoint is idempotent
- Idempotency-key mechanism (if supported)

---

### Chapter 5: Error codes / error responses

<!-- meta: full error-code list and semantics. -->

#### 5.1 Common error-response format
```json
{
  "error": {
    "code": "USER_NOT_FOUND",
    "message": "The specified user was not found",
    "details": {},
    "trace_id": "..."
  }
}
```

#### 5.2 Error-code list
| Code | HTTP status | Category | Meaning | Consumer action |
|-------|--------------|---------|------|----------|
| USER_NOT_FOUND | 404 | client error | User does not exist | Check the ID |
| RATE_LIMIT_EXCEEDED | 429 | client error | Rate-limited | Retry |
| INTERNAL_ERROR | 500 | server error | Internal failure | Contact support |
| ... | ... | ... | ... | ... |

#### 5.3 HTTP status-code policy
- When to use 200 vs 201 vs 204
- When to use 400 vs 401 vs 403 vs 404 vs 409 vs 422
- When to use 500 vs 502 vs 503 vs 504

---

### Chapter 6: Authentication

<!-- meta: authentication-method details. -->

#### 6.1 Authentication method
- API key / OAuth 2.0 / JWT / mTLS / Basic auth
- Reason for the choice

#### 6.2 Authentication flow
- Token-acquisition steps
- Token lifetime
- Refresh procedure

#### 6.3 Authorisation
- Scopes / permissions
- Role-based access control (RBAC)

#### 6.4 Credential management
- Where keys / secrets are stored
- Rotation procedure

---

### Chapter 7: Rate limiting / quotas

<!-- meta: usage caps and behaviour. -->

#### 7.1 Rate-limit policy
| Tier | Limit | Unit | Scope |
|------|-------|---------|---------|
| Free plan | 100 req/min | per minute | per API key |
| Paid plan | 10000 req/min | per minute | per API key |
| ... | ... | ... | ... |

#### 7.2 Behaviour on exceeding the limit
- HTTP status (429 Too Many Requests)
- Retry-After header
- When the limit resets

#### 7.3 Quotas
- Monthly / daily total-call ceilings
- Behaviour when exceeded

---

### Chapter 8: Versioning

<!-- meta: API evolution and compatibility. -->

#### 8.1 Versioning strategy
- URL-path style (/v1/, /v2/)
- Header style
- Media-type style

#### 8.2 Supported versions
| Version | Released | Sunset planned | Status |
|----------|----------|---------------|------|
| v1 | 2024-01 | 2026-12 | active |
| v2 | 2026-03 | - | active (recommended) |

#### 8.3 Breaking-change policy
- What counts as a breaking change
- Advance-notice period
- Migration-guide commitment

#### 8.4 Backward compatibility
- Change patterns that preserve compatibility
- Deprecation process

---

### Chapter 9: SLA / performance requirements

<!-- meta: the quality the service provides. -->

#### 9.1 Availability targets
- Availability SLA (e.g. 99.9%)
- Measurement method
- How planned downtime is announced

#### 9.2 Performance targets
| Metric | Target | Measurement |
|------|-------|---------|
| Mean response time | < 200ms | p50 |
| 95th percentile response time | < 500ms | p95 |
| Peak throughput | 10000 RPS | over 1-minute windows |

#### 9.3 Incident response
- Incident classification
- Communication flow
- Status page

---

### Chapter 10: Operations settings

<!-- meta: deployment / monitoring / logging. -->

#### 10.1 Environment variables / configuration values
| Variable | Required | Default | Purpose |
|-------|------|----------|------|
| DB_HOST | required | - | Database connection target |
| ... | ... | ... | ... |

#### 10.2 Deployment procedure
- Build / deploy pipeline
- Canary releases (if used)
- Rollback procedure

#### 10.3 Monitoring
- Monitored metrics
- Alert conditions
- Dashboards

#### 10.4 Logging
- Access-log spec
- Application-log spec
- Log retention period

---

### Chapter 11: Known constraints and unresolved items

<!-- meta: spec credibility safeguard. -->

#### 11.1 Known technical constraints
- Request-body size cap
- Concurrent-connection cap
- Known bugs / workarounds

#### 11.2 Unresolved items
- Place the `abandoned` entries from the Question Bank here

---

## Customisation guidance

### GraphQL
- Restructure Chapter 3 into "Schema", "Query", "Mutation", "Subscription".
- Change Chapter 4 to per-resolver descriptions.

### gRPC
- Restructure Chapter 3 into "Service" and "RPC Method".
- Change Chapter 4 to centre on `.proto` message definitions.

### WebSocket
- Restructure Chapter 3 around "message types".
- Change Chapter 4 to centre on client / server message flow.

### Public API (for external developers)
- Add "Quick start" and "SDK support" chapters.
- Add a "Changelog" chapter.

Customisation is finalised in dialogue with the user after Phase 1 template selection.
