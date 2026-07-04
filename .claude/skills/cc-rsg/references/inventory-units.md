# Inventory Units Reference

A catalogue of typical inventory units per language/framework, used during inventory extraction in Phase 2.

This document is the reference Claude consults to decide "what to enumerate exhaustively" from the target codebase. It becomes the basis for the "everything covered" check in Phase 4 inventory-based verification.

<!--
Extension guide:
- Add a language → add a `## {language name}` section.
- Framework-specific (L2) → add a `### {framework name}` subsection inside the relevant language section.
- Trigger threshold: if this file exceeds 800 lines, consider migration to pattern B (split into language / framework two-level structure).
-->

## Table of contents

- [Common concepts](#common-concepts)
- [PHP](#php)
- [COBOL](#cobol)
- [Python](#python)
  - [Flask specifics](#flask-specifics)
  - [FastAPI specifics](#fastapi-specifics)
- [Java](#java)
- [JavaScript / TypeScript](#javascript--typescript)
  - [Next.js specifics (App Router / Pages Router)](#nextjs-specifics-app-router--pages-router)
  - [Expo / React Native specifics](#expo--react-native-specifics)
- [C#](#c)
- [SQL / database schema](#sql--database-schema)
- [When language choice is ambiguous](#when-language-choice-is-ambiguous)
- [Customisation and extension](#customisation-and-extension)
- [Instruction summary for Claude during extraction](#instruction-summary-for-claude-during-extraction)

---

## Common concepts

Inventory units exist in 3 tiers:

1. **Macro units**: modules, packages, services
2. **Middle units**: classes, functions, endpoints, jobs
3. **Micro units**: methods, fields, configuration values

The tier you target depends on the granularity preference fixed in Phase 0.

- **High-level overview**: macro units only
- **Medium**: macro + middle
- **Detailed**: all tiers

---

## PHP

### Macro units
- Composer packages (`name` in `composer.json`)
- Namespaces (PSR-4)
- Framework-specific modules (Laravel's `app/Modules/`, Symfony's `src/Bundle/`, etc.)

### Middle units
- Classes (`class`), traits (`trait`), interfaces (`interface`)
- Route definitions (`routes/web.php`, `routes/api.php`, Symfony attribute routes, Slim `app->get`, etc.)
- Artisan commands (Laravel `app/Console/Commands/`)
- Event listeners, jobs, middleware

### Micro units
- Public methods
- Eloquent / Doctrine entity properties
- Configuration-file keys (`config/*.php`)

### Extraction examples
```bash
# Enumerate classes
grep -rEn "^(abstract |final )?class [A-Z]" src/ --include="*.php"

# Enumerate routes (Laravel)
grep -rEn "Route::(get|post|put|patch|delete|any)" routes/ --include="*.php"

# Enumerate Artisan commands
grep -rEn "protected \\\$signature" app/Console/Commands/ --include="*.php"
```

---

## COBOL

### Macro units
- COPYBOOK
- Jobs (JCL steps)

### Middle units
- PROGRAM-ID
- SECTION
- PARAGRAPH
- CALL targets (dynamic and static)

### Micro units
- 01-level items
- File definitions (SELECT / FD)
- DB invocations (EXEC SQL / EXEC CICS)

### Extraction examples
```bash
# Enumerate PROGRAM-ID
grep -rEn "^[ ]*PROGRAM-ID\\." src/ --include="*.cob" --include="*.cbl"

# Enumerate SECTION
grep -rEn "^[ 0-9]+[A-Z0-9-]+ +SECTION\\." src/ --include="*.cob"

# Enumerate CALL statements
grep -rEn "^[ ]*CALL +'" src/ --include="*.cob"
```

### COBOL-specific cautions
- Column position matters (columns 7+ are the effective area; columns 1-6 are sequence numbers).
- The mapping between the logical structure after COPYBOOK expansion and physical files must be recorded separately.
- JCL (Job Control Language) is a separate language from COBOL but is required in the spec because it drives job-trigger conditions.

---

## Python

### Macro units
- Packages (directories that contain `__init__.py`)
- Modules (`.py` files)
- Installable packages (`pyproject.toml` / `setup.py`)

### Middle units
- Classes (`class`)
- Top-level functions (`def`)
- FastAPI / Flask / Django endpoints
- Celery tasks (`@app.task`)
- Click / argparse commands

### Micro units
- Public methods
- Pydantic model / dataclass fields
- Configuration keys (`settings.py`, environment variables)

### Extraction examples
```bash
# Enumerate classes
grep -rEn "^class " --include="*.py" src/

# FastAPI endpoints
grep -rEn "@(app|router)\\.(get|post|put|patch|delete)" --include="*.py"

# Django models
grep -rEn "^class .*\\(.*models\\.Model.*\\):" --include="*.py"
```

### Flask specifics

Flask is a WSGI application defined by Blueprint-based module decomposition and decorator-based route definitions. Per-Blueprint chapter splits work well in the spec.

#### Macro units
- Application-factory function (`def create_app(): ...`)
- Blueprints (per-module feature groupings)
- Flask extensions (Flask-SQLAlchemy, Flask-Login, Flask-Migrate, etc.)

#### Middle units
- View functions (`@app.route` / `@bp.route` / `@app.get`, etc.)
- Class-based views (subclasses of `MethodView`)
- Hooks (`@app.before_request`, `@app.after_request`, `@app.errorhandler`)
- Jinja2 templates (`templates/*.html`)
- Flask-WTF forms (subclasses of `FlaskForm`)
- Flask-SQLAlchemy models (subclasses of `db.Model`)
- CLI commands (`@app.cli.command()` / `@bp.cli.command()`)

#### Micro units
- URL parameters and query parameters per route
- `app.config[...]` configuration keys
- Jinja2 macros and filters

#### Extraction examples
```bash
# Enumerate Blueprints
grep -rEn "Blueprint\\(['\"]" --include="*.py" src/

# Route definitions (app / bp / *_bp)
grep -rEn "@([a-zA-Z_]+)\\.(route|get|post|put|patch|delete)\\(" --include="*.py" src/

# Enumerate hooks
grep -rEn "@([a-zA-Z_]+)\\.(before_request|after_request|teardown_request|errorhandler|context_processor)" --include="*.py" src/

# Jinja2 templates
find templates/ -name "*.html" 2>/dev/null

# CLI commands
grep -rEn "@([a-zA-Z_]+)\\.cli\\.command\\(" --include="*.py" src/
```

#### Flask-specific cautions
- Blueprint names are easy to reuse as chapter IDs in the spec.
- `before_request` / `after_request` often contain hidden business logic — always include them in the chapter.
- In the application-factory pattern, configuration variants (`config.from_envvar`, etc.) should be called out in the operations chapter.

---

### FastAPI specifics

FastAPI is a type-hint-driven ASGI framework. Pydantic schemas and dependency injection (DI) are central concepts; a spec that omits them is incomplete.

#### Macro units
- FastAPI app (`FastAPI()` instances)
- APIRouter (per-feature routers)
- Lifespan (`@asynccontextmanager` / `lifespan` parameter)

#### Middle units
- Endpoints (`@app.get`, `@router.post`, etc., including WebSocket)
- Pydantic schemas (subclasses of `BaseModel`, both request and response)
- Dependencies (functions / classes injected via `Depends(...)`)
- Background tasks (functions invoked via `BackgroundTasks`)
- Middleware (`@app.middleware("http")` / `app.add_middleware(...)`)
- Exception handlers (`@app.exception_handler(...)`)
- Security schemes (`OAuth2PasswordBearer`, `APIKeyHeader`, etc.)

#### Micro units
- Path / query parameter types and constraints (`Query(...)`, `Path(...)`)
- Pydantic field validations (`Field(...)`, `field_validator`)
- Response models (`response_model=...`) and status codes (`status_code=...`)

#### Extraction examples
```bash
# APIRouter definitions
grep -rEn "APIRouter\\(" --include="*.py" src/

# Endpoints (REST + WebSocket)
grep -rEn "@([a-zA-Z_]+)\\.(get|post|put|patch|delete|head|options|websocket)\\(" --include="*.py" src/

# Pydantic schemas
grep -rEn "^class .*\\((BaseModel|RootModel)\\b" --include="*.py" src/

# Dependency functions
grep -rEn "Depends\\(" --include="*.py" src/

# Exception handlers
grep -rEn "@([a-zA-Z_]+)\\.exception_handler\\(" --include="*.py" src/

# Middleware registration
grep -rEn "@([a-zA-Z_]+)\\.middleware\\(|add_middleware\\(" --include="*.py" src/
```

#### FastAPI-specific cautions
- Response Pydantic schemas are the core of the API spec. When the chosen template is `api-service`, always promote them to a dedicated chapter.
- Functions injected via `Depends(...)` often contain crucial logic (authorisation, DB session acquisition, external API client construction, etc.). Consider raising questions in the `architecture_decision` category of the Question Bank.
- OpenAPI schema (`/openapi.json`) is generated automatically — when possible, fetch it at startup and attach to `recon-report.md`.

---

## Java

### Macro units
- Packages (`com.example.foo`)
- Maven modules (`pom.xml`)
- Spring profiles / Bundle / OSGi modules

### Middle units
- Classes (`class`, `interface`, `enum`, `record`)
- Spring `@Controller`, `@Service`, `@Repository`, `@Component`
- Endpoints (`@RequestMapping`, `@GetMapping`, etc.)
- Batch jobs (Spring Batch `Job`, `Step`)
- Scheduled tasks (`@Scheduled`)

### Micro units
- Public methods
- JPA entity fields
- Configuration properties (`application.yml` / `application.properties`)

### Extraction examples
```bash
# Enumerate classes
grep -rEn "^(public |abstract |final )*(class|interface|enum|record) " src/ --include="*.java"

# Spring endpoints
grep -rEn "@(Get|Post|Put|Patch|Delete|Request)Mapping" src/ --include="*.java"

# JPA entities
grep -rEn "@Entity" src/ --include="*.java"
```

---

## JavaScript / TypeScript

### Macro units
- npm packages (`name` in `package.json`)
- Workspaces (monorepo: pnpm workspace, turborepo)
- Frontend: pages, routes (Next.js `app/`, `pages/`)
- Backend: modules (NestJS)

### Middle units
- Exported functions / classes
- React components, Vue components
- Route handlers for Express / Fastify / Hono
- NestJS Controller / Service / Module
- Background jobs (BullMQ, Agenda)

### Micro units
- Public methods
- Zod / Yup / TypeScript type definitions
- Environment variables

### Extraction examples
```bash
# Exported functions / classes
grep -rEn "^export (default )?(async )?(function|class|const)" --include="*.ts" --include="*.tsx" --include="*.js" src/

# Express routes
grep -rEn "(app|router)\\.(get|post|put|patch|delete)\\(" --include="*.ts" --include="*.js" src/

# React components (function components)
grep -rEn "^export (default )?function [A-Z]" --include="*.tsx" --include="*.jsx" src/
```

### Next.js specifics (App Router / Pages Router)

Next.js is a convention-over-configuration framework: file names themselves become inventory units. App Router (13+) and Pages Router have different structures — keep them separate.

#### Macro units
- `app/` directory (App Router projects)
- `pages/` directory (Pages Router projects; `pages/api/` is API endpoints)
- Route Group (directories of the form `(group_name)` — logical groupings that do not appear in the URL)
- Parallel Routes (`@slot_name`), Intercepting Routes (`(.)`, `(..)`, `(...)`)

#### Middle units (App Router)
- Pages (`app/**/page.tsx` / `page.ts`)
- Layouts (`app/**/layout.tsx`)
- Loading UI (`app/**/loading.tsx`)
- Error boundaries (`app/**/error.tsx` / `global-error.tsx`)
- Not Found (`app/**/not-found.tsx`)
- API routes (`app/**/route.ts` — Route Handler)
- Server Actions (files / functions that include `'use server'`)
- Server Component / Client Component boundary (`'use client'` directive)
- Middleware (`middleware.ts` at the project root)
- Instrumentation (`instrumentation.ts`)

#### Middle units (Pages Router)
- Pages (`pages/**/*.tsx`)
- API endpoints (`pages/api/**/*.ts`)
- Special files: `_app.tsx`, `_document.tsx`, `_error.tsx`
- Pages containing `getStaticProps` / `getStaticPaths` / `getServerSideProps`

#### Micro units
- Dynamic route segments (`[id]`, `[...slug]`, `[[...slug]]`)
- Route metadata (`generateMetadata`, `metadata` exports)
- Config files (`next.config.js` / `next.config.mjs`)
- Environment variables (those starting with `NEXT_PUBLIC_` are exposed to the client)

#### Extraction examples
```bash
# App Router pages
find app -name "page.tsx" -o -name "page.ts" -o -name "page.jsx" -o -name "page.js" 2>/dev/null

# App Router API routes
find app -name "route.ts" -o -name "route.js" 2>/dev/null

# Pages Router pages (excluding _ files)
find pages -name "*.tsx" -o -name "*.ts" 2>/dev/null | grep -vE "/(_app|_document|_error)\\."

# Pages Router API
find pages/api -type f 2>/dev/null

# Files containing a Server Action
grep -rEn "^[\"']use server[\"']" --include="*.ts" --include="*.tsx" .

# Client Component declarations
grep -rEn "^[\"']use client[\"']" --include="*.ts" --include="*.tsx" app/

# Middleware
ls middleware.ts middleware.js 2>/dev/null
```

#### Next.js-specific cautions
- **Mixed App Router and Pages Router** is common in real projects. When both are detected, split chapters in the spec.
- Server Actions look like ordinary functions in source but cross the network boundary. **Enumerate them as API endpoints.**
- The presence/absence of `'use client'` changes the execution environment. Reference it in the security / performance chapters.
- The `experimental` section of `next.config.js` is a strong basis for future instability — always state it in the spec.

---

### Expo / React Native specifics

Expo / React Native targets **mobile apps**. Different units (screens, navigation, native modules, build config) dominate compared to a web app. Choose a future `mobile-app.md` template (or a mobile customisation of `web-app.md`) instead of using `web-app.md` directly.

#### Macro units
- App entry (`App.tsx` / `app/_layout.tsx` for Expo Router)
- Navigator hierarchy (nested Stack / Tab / Drawer)
- Platform directories (`android/`, `ios/` in the Bare Workflow)

#### Middle units
- Screens
  - `screens/*.tsx` / `screens/**/*Screen.tsx` (traditional structure)
  - `app/**/*.tsx` (Expo Router; similar to Next.js App Router)
- Navigator definitions (`createNativeStackNavigator`, `createBottomTabNavigator`, `createDrawerNavigator`)
- Custom hooks (`hooks/use*.ts`)
- Native module references (`NativeModules.XXX`, Expo Modules such as `expo-camera`)
- Background tasks (`expo-background-fetch`, `expo-task-manager`)
- Storage / persistence (`AsyncStorage`, `SecureStore`, MMKV, Realm)

#### Micro units
- Screen navigation parameter types (`RootStackParamList`)
- Permission declarations (`permissions` / `infoPlist` / `androidManifest` in `app.json`)
- Environment variables (`EXPO_PUBLIC_*`, `Constants.expoConfig`)

#### Build / distribution configuration
- `app.json` / `app.config.ts` / `app.config.js` (app metadata, build configuration)
- `eas.json` (EAS Build / Submit configuration; development / preview / production profiles)
- `package.json` `scripts` (`expo start`, `expo run:ios`, `expo prebuild`, etc.)
- `metro.config.js` (bundler configuration)
- `babel.config.js` (plugin composition, especially `react-native-reanimated/plugin`, etc.)

#### Extraction examples
```bash
# Screen files (traditional structure)
find . -path ./node_modules -prune -o -name "*Screen.tsx" -print 2>/dev/null

# Expo Router screens (under app/)
find app -name "*.tsx" -not -name "_*" 2>/dev/null

# Navigator creation
grep -rEn "create(NativeStack|BottomTab|Drawer|Material(Top|Bottom)Tab)Navigator" --include="*.tsx" --include="*.ts" .

# Expo Module usage
grep -rEn "from ['\"]expo-[a-z-]+['\"]" --include="*.ts" --include="*.tsx" .

# Permission declarations (app.json / app.config.*)
grep -nE "(permissions|infoPlist|androidManifest)" app.json app.config.* 2>/dev/null

# EAS build profiles
cat eas.json 2>/dev/null | grep -E '"(development|preview|production)"'
```

#### Expo / React Native-specific cautions
- **Decide Managed vs Bare Workflow first** (presence of `android/` `ios/` directories, or traces of `expo prebuild`). The spec structure changes significantly.
- Native modules (`expo-camera`, `expo-location`, etc.) require **OS-specific permissions**. Always cover them in the security / operations chapters.
- `runtimeVersion` in `app.json` and `channel` in `eas.json` decide **OTA (Over-the-Air) update** behaviour. Mention this in the versioning chapter.
- **Behaviour differences between iOS and Android** (push notifications, background execution, file access) should be split into explicit chapters or sections.
- Check whether `react-native-web` output is configured (if yes, also add a web-app chapter).

---

## C#

### Macro units
- Assemblies (`.csproj`)
- Namespaces (`namespace`)
- Solutions (`.sln`)

### Middle units
- Classes (`class`, `interface`, `record`, `struct`)
- ASP.NET Core Controller / Minimal API endpoints
- Hosted services (`IHostedService`)
- Background workers

### Micro units
- Public methods
- EF Core entity properties
- Configuration keys in `appsettings.json`

### Extraction examples
```bash
# Enumerate classes
grep -rEn "^[[:space:]]*(public |internal )?(abstract |sealed )?(class|interface|record|struct) " src/ --include="*.cs"

# ASP.NET endpoints
grep -rEn "\\[Http(Get|Post|Put|Patch|Delete)\\]" src/ --include="*.cs"
```

---

## SQL / database schema

Database schemas are part of the spec target alongside the source code itself.

### Inventory units
- Tables
- Views
- Stored procedures / functions
- Triggers
- Indexes
- Foreign-key constraints

### Extraction methods
- Parse migration files (Rails, Laravel, Django, Flyway, Liquibase)
- Read `information_schema` from a production DB (when possible)
- Read ER diagrams / DDL files directly

---

## When language choice is ambiguous

### Multi-language repository
- Produce a separate inventory per language and tag each entry with the language.
- Example: add `"language": "php"` to each entry in `inventory.json`.

### DSL / configuration files are essential
- For projects centred on a DSL — Terraform `.tf`, Kubernetes `.yaml`, Ansible playbooks — treat those DSLs as middle units.
- Resource definitions (`resource "aws_instance" ...`), Pods / Services / Deployments, and playbook tasks become the units.

### Microservices
- Treat each service as a macro unit, and develop the per-language inventory inside each service as middle units and below.

---

## Customisation and extension

When the user wants to add inventory units for a new language / framework, append to this file. The v1 release does not provide a UI-based addition mechanism.

A future version may split this into `references/inventory-units-{custom}.md`-style files.

---

## Instruction summary for Claude during extraction

1. **First, run `scripts/source-map.py --target <root>`**. This auto-extracts file-level source units (SRC-NNNN) and saves them to `.cc-rsg/source-map.json`.
2. Identify the target codebase's primary language(s) from `recon-report.md`.
3. Consult the matching section and plan an extraction strategy.
4. **Group `source-map.json` units into conceptual units**. Many-to-one (multiple SRC → 1 INV) is acceptable, subject to the granularity rules below.
5. Save the result to `inventory.json`. Schema lives in SKILL.md's Phase 2 section. For each inventory item, record the corresponding multiple SRC-NNNN in `related_source_ids`; Phase 4 verification uses this.
6. Tag commented-out / deprecated / test code that you encounter (e.g. `"deprecated": true`).

---

## Granularity rules (mandatory)

### Minimum count

```
inventory.json minimum count = max(50, files_scanned // 20)
```

- Example: 1,000-file Rails project → at least 50 entries
- Example: 5,000-file large JS project → at least 250 entries

`scripts/coverage-check.py` fails on this rule. The value is **derived automatically from `source-map.json`'s `stats.files_scanned`**.

### Macro-unit prohibition

❌ **Grouping-style INVs like the following are forbidden**:
- `controller_group` "Account-family controllers" ← bundling Account, Sessions, Twofa together
- `model_group` "User-family models" ← bundling User, Group, Member together
- `module_group` "Wiki/Document family" ← bundling multiple independent responsibilities

These are too coarse: maintainers cannot tell which file to modify.

✅ **Correct granularity**:
- 1 controller class = 1 INV
- 1 model class = 1 INV
- 1 service class = 1 INV
- 1 job class = 1 INV
- 1 concern module = 1 INV
- 1 mailer class = 1 INV
- For large controllers (300+ lines), **per-action additional INVs are allowed**

`scripts/coverage-check.py` treats INVs whose `type` field contains `group` / `module` / `domain` / `category` / `bundle` / `section` as "macro type" and **fails if they exceed 20% of all INVs**.

---

## Ruby on Rails catalogue (detailed)

For Rails applications, always extract by the following units. **An `inventory.json` that does not satisfy this catalogue is considered non-compliant.**

### 1. Controllers (`app/controllers/**/*.rb`)
- Every file ending in `_controller.rb` × 1 INV
- type: `controller`
- name: class name (e.g. `IssuesController`)
- file: the file
- Large controllers (300+ lines) may also add **per-action INVs** (type: `controller_action`)

### 2. Models (`app/models/**/*.rb`)
- Every model class × 1 INV (including direct ApplicationRecord descendants and indirect descendants like Principal/User)
- type: `model` / `model_subclass`
- name: class name (e.g. `Issue`, `Project`)

### 3. Concerns (`app/controllers/concerns/`, `app/models/concerns/`)
- 1 module = 1 INV
- type: `concern`

### 4. Services / use cases (`app/services/`, `app/use_cases/`, `lib/services/`)
- 1 class = 1 INV
- type: `service`

### 5. Jobs (`app/jobs/**/*.rb`)
- 1 class = 1 INV
- type: `job`

### 6. Mailers (`app/mailers/**/*.rb`)
- 1 class = 1 INV
- type: `mailer`

### 7. Helpers (`app/helpers/**/*.rb`)
- 1 module = 1 INV
- type: `helper`

### 8. Lib (`lib/**/*.rb`)
- 1 class or 1 module = 1 INV
- type: `lib_class` / `lib_module`

### 9. Migrations (`db/migrate/**/*.rb`)
- 1 migration file = 1 INV (treated per table)
- type: `migration`
- name: migration class name (e.g. `CreateIssues`)

### 10. Route groups (`config/routes.rb`)
- One `resources :foo do … end` block × 1 INV
- One `namespace :api do … end` block × 1 INV
- type: `rails_route`
- name: prefixed name like `resources:issues` / `namespace:api/v1`

### 11. View groups (`app/views/**`)
- Group per resource (e.g. `app/views/issues/`) × 1 INV
- type: `view_group`
- name: directory name

### 12. JavaScript modules (`app/javascript/**`)
- 1 export = 1 INV (per file if file count is small)
- type: `js_export`

### 13. Configuration files (`config/*.yml`, `config/initializers/**/*.rb`)
- Key initializers: 1 file = 1 INV
- type: `config`

### 14. Mailer templates (`app/views/mailer/**/*.erb`)
- 1 file = 1 INV
- type: `mailer_view`

### Rails granularity guideline

| Rails app size | Minimum INV | Approximate breakdown |
|----------------|---------|---------|
| Small (100 .rb) | 50 | controllers 10, models 15, jobs 3, lib 5, migrations 10, routes 5, views 2 |
| Medium (500 .rb) | 50 | controllers 30, models 50, services 20, jobs 10, migrations 30, routes 15, helpers 10 |
| Large (1,000+ .rb) | 90+ | controllers 80, models 80, concerns 20, services 30, jobs 20, migrations 100+, routes 30 |

Example: a medium Rails codebase with ~1,000 .rb files → at least ~90 INVs are required (an agent that stops at 30 is under-granular).
