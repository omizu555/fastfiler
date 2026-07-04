# Question Categories Reference

Detailed definitions of the Question Bank's 7 standard categories, plus how to add custom categories.

---

## 7 standard categories

### 1. business_rule

#### Definition
Questions about business judgement criteria and rules. "Why does this branch exist?", "What business justification supports this threshold?" — questions whose answers cannot be derived from reading the code alone.

#### Typical examples
- Is this retry count driven by a technical constraint or by a business requirement?
- Why does sales aggregation exclude a particular status?
- What is the business basis of this 50,000-yen threshold?
- Why does this batch run on the last day of the month — is it a month-end accounting requirement or a system convenience?

#### How to resolve
- Direct interview with SME (subject-matter experts) is most effective.
- Consult business manuals and policy documents.
- Check past meeting minutes and ticket history.

#### Impact when unresolved
- "Business-rule correctness" is undermined. Developers who later read the spec may modify the code under wrong premises.

---

### 2. architecture_decision

#### Definition
Questions about why a particular design choice was made. "Why was this pattern adopted?", "Why was the boundary placed here?" — questions tied to the designer's intent.

#### Typical examples
- Why does the codebase use the repository pattern (DI? testability? legacy convention?)?
- Why was the system split into microservices?
- Why was CQRS adopted?
- What is the intent of this event-driven composition (loose coupling? scaling?)?

#### How to resolve
- Consult Architecture Decision Records (ADRs).
- Interview the original designer.
- Review past sprint-review materials.

#### Impact when unresolved
- During refactoring, "designs that can change" cannot be distinguished from "designs that must not change".

---

### 3. data_model_intent

#### Definition
Questions about the design intent behind database schemas or domain models. "What is the real purpose of this field?", "Why was this denormalisation chosen?", etc.

#### Typical examples
- What do the flags `user.flag_a` and `user.flag_b` actually mean?
- Why is this table denormalised (performance? business reason?)?
- Is this nullable column reserved for future use, or is it actively used today?
- What does the `LEGACY_001` enum value represent?

#### How to resolve
- Consult the database design document.
- Sample real data (read-only query against production DB).
- Look at ER diagrams / DDL comments.

#### Impact when unresolved
- Unintended destruction during data migration or schema changes.

---

### 4. external_integration

#### Definition
Questions about the specifications of integration with external systems / APIs.

#### Typical examples
- Which version of the payment gateway is being used (API v1? v2?)?
- Is this webhook's retry behaviour the external system's spec, or our implementation?
- Is the procedure for obtaining this authentication token documented?
- Why is only this field Base64-encoded — is that the external spec or a local choice?

#### How to resolve
- Consult the external system's official documentation (use WebFetch).
- Ask the integration counterpart's owner.
- Look at past integration-test logs.

#### Impact when unresolved
- Inability to keep up when the external system bumps a version or deprecates an API.

---

### 5. naming_history

#### Definition
Questions about historical context implied by naming and comments. "Why is it named this?", "What does this comment mean?", "Is this code still in use?", etc.

#### Typical examples
- What does the variable name `temp_fix_v3` suggest?
- What is the background of `// FIXME: 2018 temporary fix`?
- Is the class `OldUserService` still used, or scheduled for removal?
- What is the criterion for the `_deprecated` prefix?

#### How to resolve
- Use Git blame / history.
- Trace past Issues / PRs.
- Interview the original author.

#### Impact when unresolved
- Dead code cannot be reliably identified, generating unnecessary maintenance cost.

---

### 6. operational_requirement

#### Definition
Questions about operational requirements and constraints — SLAs, backups, monitoring, deployment procedures, etc., that are not written in the source.

#### Typical examples
- What is the expected runtime of this batch (does an SLA exist)?
- What is the backup-frequency and generation-management policy?
- What is the expected concurrent-connection load for this service?
- What are the RTO/RPO targets for recovery?

#### How to resolve
- Consult the operations design document.
- Check the monitoring dashboards.
- Interview the infrastructure team.

#### Impact when unresolved
- When operations hand-off happens, "what to monitor" and "where to draw the line" become unclear.

---

### 7. security_compliance

#### Definition
Questions about security requirements, regulatory compliance, and conformance to industry standards.

#### Typical examples
- Is this personal-information field GDPR-compliant?
- Does this log output contain PII (personally identifiable information)?
- Is this API-key storage compliant with the internal security policy?
- Is this encryption algorithm (AES-128) acceptable under current standards?

#### How to resolve
- Consult the security-policy documents.
- Ask the security team.
- Review audit records / vulnerability-assessment results.

#### Impact when unresolved
- Risk of regulatory violations or security incidents.

---

## Category-selection logic (instructions to Claude)

When a sub-agent raises a question, use this flow to choose the category:

```
1. Is the question about a "business decision"?
   YES → business_rule
2. Is the question about a "design choice's intent"?
   YES → architecture_decision
3. Is the question about "data structure / semantics"?
   YES → data_model_intent
4. Is the question about "the boundary with external systems"?
   YES → external_integration
5. Is the question about "naming / comments / history"?
   YES → naming_history
6. Is the question about "operations / monitoring / SLA"?
   YES → operational_requirement
7. Is the question about "security / compliance"?
   YES → security_compliance
8. None of the above
   → Hold as a custom-category candidate; surface to the user during Phase 4 deduplication.
```

When a question spans multiple categories, choose the dominant one and add the others to a `related_categories` field for auxiliary classification.

---

## Adding custom categories

### v1 support range
The initial release does not support adding custom categories through a UI. Users can add them by editing `.cc-rsg/questions.json` manually.

### Manual procedure

1. Create `.cc-rsg/custom-categories.json`:

   ```json
   {
     "categories": [
       {
         "id": "performance_tuning",
         "label_en": "Performance Tuning",
         "label_ja": "性能チューニング",
         "description": "Questions about performance-optimisation decisions and thresholds"
       }
     ]
   }
   ```

2. Set the `category` field of relevant entries in `questions.json` to the new `id`.

3. During Phase 5 dialogue, Claude treats custom categories on equal footing with the 7 standard categories.

### Future extensions

- Interactive custom-category addition via AskUserQuestion
- Custom-category templates (a library of commonly used categories)

---

## Per-category dialogue strategy

To reduce user burden in Phase 5, vary the dialogue strategy per category.

| Category | Recommended strategy |
|---------|-----------|
| business_rule | Focused interview with SME (consecutive questions) |
| architecture_decision | Consecutive questions if the original designer is available, otherwise inference + abandoned |
| data_model_intent | Often answerable by sampling real data |
| external_integration | Prefer external-doc lookup (WebFetch) |
| naming_history | Most are answered by Git blame |
| operational_requirement | Group questions and interview infra / ops in one pass |
| security_compliance | Formal inquiry to security; expect a delay |

---

## Category vs severity tendencies

Empirically, categories and severity correlate as follows (not strict).

| Category | critical frequency | Notes |
|---------|------------|------|
| business_rule | High | Directly affects business correctness |
| architecture_decision | Medium | Affects refactoring decisions |
| data_model_intent | High | Data-migration risk |
| external_integration | Medium | Integration-failure risk |
| naming_history | Low | Mostly nice-to-have |
| operational_requirement | Medium | Needed for operations hand-off |
| security_compliance | High | Regulatory-violation risk |

Sub-agents may use this as a hint when deciding severity.
