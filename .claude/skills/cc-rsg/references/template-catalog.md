# Template Catalog Reference

Selection guide used in Phase 1 when presenting template candidates to the user. For each template, this document defines the intended target, chapter-outline summary, selection criteria, and the decision-tree logic.

---

## Initial set of 4

The skill ships with the following 4 templates by default. The user may also bring their own template (by specifying a path).

1. **Web application spec** (`templates/web-app.md`)
2. **Batch-system spec** (`templates/batch-system.md`)
3. **API service spec** (`templates/api-service.md`)
4. **Library / SDK spec** (`templates/library-sdk.md`)

---

## 1. Web application spec

### Target
- Systems the user operates through screens.
- PHP (Laravel/Symfony/CakePHP), Python (Django/Flask), Ruby (Rails), Node.js (Next.js/Nuxt/Express), Java (Spring MVC), etc.
- Authentication, session management, and screen transitions are present.

### Chapter outline
- Overview / system purpose
- Architecture overview
- Screen list and transitions
- Routes / endpoint list
- Data model (ER diagram, entity definitions)
- Authentication and authorisation
- External-system integration
- Operations settings / deployment
- Known constraints and unresolved items

### Selection criteria
- Evidence of HTML rendering and a templating engine.
- Session-management code (`session`, `cookie`).
- Routing definitions present (`routes/`, `urls.py`, etc.).
- Existence of `views/`, `templates/`, `pages/` directories.

---

## 2. Batch-system spec

### Target
- Scheduled or event-driven background processing.
- COBOL + JCL, cron / systemd timers, Spring Batch, Apache Airflow, Celery, Sidekiq, AWS Batch / Lambda scheduled runs.
- Includes data pipelines (ETL).

### Chapter outline
- Overview / business purpose
- Job catalogue
- Triggers and schedule
- Data flow (input → processing → output)
- Error handling and retry policy
- Recovery procedures
- Operations calendar / dependency graph
- Monitoring / alerts
- Known constraints and unresolved items

### Selection criteria
- Presence of scheduler configuration (crontab, Quartz, Airflow DAG, JCL).
- Presence of job-execution scripts.
- No persistent UI or API, or only an admin one.
- Evidence of large-data processing (chunked processing, bulk operations).

---

## 3. API service spec

### Target
- Endpoints called by other systems.
- REST, GraphQL, gRPC, WebSocket.
- Microservices, public APIs, internal APIs.

### Chapter outline
- Overview / API purpose
- Endpoint catalogue
- Request / response specs (per endpoint)
- Error codes / error responses
- Authentication (API key, OAuth, JWT)
- Rate limiting / quotas
- Versioning
- SLA / performance requirements
- Known constraints and unresolved items

### Selection criteria
- Presence of OpenAPI / Swagger / GraphQL schema.
- Routing definitions centred on endpoints (`/api/...`).
- No web UI (HTML rendering), or only as a secondary feature.
- Presence of API-Gateway configuration (Kong, AWS API Gateway, etc.).

---

## 4. Library / SDK spec

### Target
- Reusable code consumed by other applications.
- npm / pip / composer / gem / NuGet packages.
- Internal common libraries.

### Chapter outline
- Overview / library purpose
- Installation
- Public API catalogue
- Usage examples (quick start)
- Configuration options
- Compatibility (supported language versions, dependencies)
- Extension points / plugin system
- Migration guide (from older versions)
- Known constraints and unresolved items

### Selection criteria
- Package manifest (`package.json` / `setup.py` / `composer.json`, etc.) defines `name`, `version`, `main` / `module`.
- Directory structure consistent with distribution (`dist/`, `lib/`, `src/`).
- No application-entry code (a main function, entry-point script), or only samples.

---

## Decision tree (Claude's recommendation logic)

Based on the Phase 1 reconnaissance, Claude follows this procedure to recommend a template:

```
1. Does the package manifest define main/module/bin?
   YES → Is there application-startup code?
            NO  → Recommend Library / SDK spec
            YES → Continue

2. Do routing definitions exist?
   YES → Is there HTML rendering (views/templates)?
            YES → Recommend Web application spec
            NO  → Recommend API service spec

3. Are scheduler configuration / batch scripts the main subject?
   YES → Recommend Batch-system spec

4. None of the above / composite type
   → Present multiple candidates and ask the user.
   → Example: "Includes both web app and API; recommend a merged custom outline."
```

---

## Handling composite projects

Real projects often do not fit into a single template. Handle them as follows.

### When there is a primary / secondary relationship
- Pick the primary template and add a chapter from the secondary one.
- Example: web app primary, batch secondary → add a "background jobs" chapter to the web-app spec.

### Composite at equal scale
- Generate a custom template by merging the chapter outlines.
- Ask the user for the chapter-ordering preference.

### Monorepo with multiple services
- Recommend generating separate specs per service.
- Merge into a single spec only if the user explicitly wants one spec for the whole monorepo.

---

## When the user brings their own template

1. Get the path to the template file.
2. Parse the template and extract the chapter outline.
3. Check whether each chapter has a meta-comment describing what it covers.
   - When missing, Claude infers it from the chapter title and confirms with the user.
4. Use the extracted outline for Phase 2 skeleton generation.

---

## When the user adjusts Claude's recommendation

After the user accepts the recommendation, accept chapter additions, removals, or renames.

```
Claude: "I recommend the Web application spec. The outline is:
- Overview
- Architecture
- Screen list
- Routes
- Data model
- Authentication and authorisation
- External integration
- Operations settings

Any chapters to add, remove, or rename?"

User: "Add a 'non-functional requirements' chapter. Place it before 'Operations settings'."

Claude: "Got it. Finalising with:
- Overview
- Architecture
- Screen list
- Routes
- Data model
- Authentication and authorisation
- External integration
- Non-functional requirements   ← added
- Operations settings"
```

---

## Template version management

Each template file starts with version information.

```yaml
---
template_name: web-app
template_version: 0.1.0
last_updated: 2026-05-01
---
```

The consuming project's `wbs.json` records the template version, guaranteeing reproducibility.

---

## Future templates

After OSS release, the following templates may be added in response to user requests:

- Data warehouse / DWH spec
- Machine-learning pipeline spec
- Infrastructure spec (IaC, Terraform, Kubernetes)
- Mobile app spec (iOS / Android / React Native / Flutter)
- Blockchain / smart-contract spec
- Game-design spec

Requests are received via GitHub Issues.
