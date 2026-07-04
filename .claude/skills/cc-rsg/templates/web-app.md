---
template_name: web-app
template_version: 0.1.0
last_updated: 2026-05-01
description: Web application spec template. For interactive systems that render HTML.
---

# Web application spec template

This template defines the chapter outline for the spec of a web system that the user operates through screens.

Designed for typical web applications: PHP (Laravel/Symfony/CakePHP), Python (Django/Flask), Ruby (Rails), Node.js (Next.js/Nuxt/Express), Java (Spring MVC), ASP.NET MVC, etc.

---

## Chapter outline

### Chapter 1: Overview

<!-- meta: bird's-eye view of the whole system. A 3-minute "what is this" for the reader. -->

#### 1.1 System purpose
- The business problem this system solves
- Primary users / stakeholders
- Position in the business

#### 1.2 Main use cases
- Use case 1: ...
- Use case 2: ...
- 3 to 5 use cases

#### 1.3 High-level architecture diagram
- High-level component diagram of the system
- Use Mermaid notation when appropriate

---

### Chapter 2: Architecture overview

<!-- meta: design decisions and overall structure. Capture WHY this shape. -->

#### 2.1 Adopted framework / libraries
- Language, framework, and major libraries
- Version information

#### 2.2 Architecture pattern
- MVC / Clean architecture / Hexagonal, etc.
- Reason for adoption (to the extent it can be inferred)

#### 2.3 Directory structure
- Responsibility of each major directory
- Conventions (naming rules, placement rules)

#### 2.4 Dependencies
- External systems / APIs
- Database / cache / message queue

---

### Chapter 3: Screens and screen transitions

<!-- meta: UI structure from the user's perspective. -->

#### 3.1 Screen list
| Screen ID | Screen name | URL | Auth required | Required role |
|-------|-------|-----|---------|---------|
| SC-001 | Login | /login | no | - |
| SC-002 | Dashboard | /dashboard | yes | regular user or higher |
| ... | ... | ... | ... | ... |

#### 3.2 Screen-transition diagram
- Major transition paths (Mermaid notation, etc.)
- Exceptional transitions (errors, session timeout)

#### 3.3 Details of each screen
For each screen, describe:
- Displayed elements
- Input fields and their validation
- Actions (behaviour when buttons are pressed)
- Display conditions (role, data state)

---

### Chapter 4: Routes / endpoints

<!-- meta: full list of HTTP routes. The pillar of inventory-based verification. -->

#### 4.1 Web screen routes
| Method | Path | Controller::Action | Auth | Summary |
|---------|------|-----------------------|------|------|
| GET | / | HomeController::index | optional | Top page |
| GET | /users/{id} | UserController::show | required | User details |
| ... | ... | ... | ... | ... |

#### 4.2 Internal API / Ajax endpoints
- Ajax / Fetch APIs called from the screens
- Response format

#### 4.3 Per-route middleware
- Applied middleware and the order of processing

---

### Chapter 5: Data model

<!-- meta: structure and semantics of persisted data. -->

#### 5.1 ER diagram
- Relations between key entities
- Use Mermaid notation, etc.

#### 5.2 Entity list
Per entity:
- Table / class name
- Field list (type, nullability, default, business meaning)
- Indexes
- Foreign keys
- Relations (1:1, 1:N, N:N)

#### 5.3 Key domain rules
- Invariants
- State transitions (state machines)
- Business rules (e.g. "withdrawn users are excluded from search results")

---

### Chapter 6: Authentication and authorisation

<!-- meta: security core. Omissions here are critical. -->

#### 6.1 Authentication method
- Session / token / OAuth / SSO
- Password-hash algorithm
- Session timeout

#### 6.2 Authorisation model
- Roles and permissions
- Role hierarchy
- Where authorisation checks are implemented

#### 6.3 Authorisation flow
- Request → authorisation decision → execute / deny flow
- Behaviour on authorisation failure

#### 6.4 Session management
- Session store
- Conditions for session invalidation
- Concurrent-login control

---

### Chapter 7: External-system integration

<!-- meta: boundaries and failure propagation. -->

#### 7.1 Integration partners
| Partner | Protocol | Purpose | Behaviour on failure |
|-------|----------|------|----------|
| Payment gateway | HTTPS REST | Payment processing | Retry 3 times; notify on failure |
| ... | ... | ... | ... |

#### 7.2 Details per integration
- Authentication method (API key, OAuth, etc.)
- Request / response example
- Timeout / retry policy
- Idempotency (or lack thereof)
- Fallback behaviour on failure

---

### Chapter 8: Operations settings

<!-- meta: deployment, environment variables, monitoring. -->

#### 8.1 Environment composition
- Environment list (dev, staging, prod)
- Differences between environments

#### 8.2 Environment variables / configuration values
| Variable | Required | Default | Purpose |
|-------|------|----------|------|
| DB_HOST | required | - | Database connection target |
| ... | ... | ... | ... |

#### 8.3 Deployment procedure
- Build procedure
- Deploy command
- Rollback procedure

#### 8.4 Monitoring / logging
- Monitoring targets (liveness, performance, errors)
- Log destination and retention period
- Alert conditions

#### 8.5 Backup / restore
- Backup target
- Frequency and generation management
- Restore procedure

---

### Chapter 9: Known constraints and unresolved items

<!-- meta: spec credibility safeguard. -->

#### 9.1 Known technical constraints
- Performance ceilings (concurrent connections, response time)
- Known bugs / workarounds

#### 9.2 Unresolved items
- Place the `abandoned` entries from the Question Bank here
- For each item, record "why it could not be resolved", "current inference", "what is needed to resolve it in the future"

---

## Customisation guidance

This template assumes a standard web application. Customise as the actual project requires.

### Multi-tenant / SaaS
- Add a "tenant isolation" section to Chapter 6.

### Many background jobs
- Insert a "background jobs" chapter between Chapter 7 and Chapter 8 (see `templates/batch-system.md` for the outline).

### Multi-language support
- Add an "internationalisation (i18n)" section to Chapter 3.

### A mobile app is also offered
- Split Chapter 4 into "Web routes" and "Mobile API".

Customisation is finalised in dialogue with the user after Phase 1 template selection.
