<!--
SYNC IMPACT REPORT
==================
Version change  : [TEMPLATE] → 1.0.0
Bump rationale  : MAJOR — initial ratification; all placeholder tokens resolved for the first time.

Principles resolved:
  [PRINCIPLE_1_NAME] / [PRINCIPLE_1_DESCRIPTION] → I. Non-Intrusion (NON-NEGOTIABLE)
  [PRINCIPLE_2_NAME] / [PRINCIPLE_2_DESCRIPTION] → II. Latency Over Throughput
  [PRINCIPLE_3_NAME] / [PRINCIPLE_3_DESCRIPTION] → III. Interface-First Abstractions
  [PRINCIPLE_4_NAME] / [PRINCIPLE_4_DESCRIPTION] → IV. Safe Code Standards
  [PRINCIPLE_5_NAME] / [PRINCIPLE_5_DESCRIPTION] → V. Test-Validated Quality Gates
  Added (beyond template)                         → VI. Observable by Design
  Added (beyond template)                         → VII. Minimum Privilege

Sections resolved:
  [SECTION_2_NAME] / [SECTION_2_CONTENT] → Configuration Management
  [SECTION_3_NAME] / [SECTION_3_CONTENT] → Contribution & Review Standards
  [GOVERNANCE_RULES]                      → Governance (fully populated)

Templates updated:
  ✅ .specify/templates/plan-template.md  — Constitution Check gates populated with all 7 principles
  ✅ .specify/templates/spec-template.md  — Performance success-criteria note added
  ✅ .specify/templates/tasks-template.md — Session integrity test task type added
  ✅ .specify/templates/checklist-template.md  — generic; no update required
  ✅ .specify/templates/agent-file-template.md — generic; no update required

Deferred TODOs: None. All placeholders resolved.
-->

# gameQA Constitution

## Core Principles

### I. Non-Intrusion (NON-NEGOTIABLE)

Non-intrusion into game processes is the absolute first principle and admits no exceptions.

- No implementation technique that intrudes into, injects into, or attaches to the game process is
  permitted, regardless of how much simpler or faster it would be.
- Code that circumvents this principle for convenience MUST NOT be written.
- This principle supersedes every other engineering trade-off.

**Rationale**: Game processes are externally-owned binaries. Intrusion creates legal liability,
instability, and anti-cheat conflicts that cannot be mitigated by any convenience gain.

### II. Latency Over Throughput

Latency minimization MUST take precedence over throughput in every design decision.

- Buffer sizes, queue depths, thread models, and all architectural decisions MUST minimize latency
  first.
- Trade-offs that sacrifice latency for throughput MUST be explicitly justified in writing before
  adoption; implicit trade-offs are not permitted.

**Rationale**: QA capture sessions require real-time temporal fidelity; high-latency captures
corrupt frame-interval and event-ordering analysis.

### III. Interface-First Abstractions

All components that may be replaced MUST be hidden behind explicit interfaces.

- Capture backends, serialization formats, and storage implementations MUST be accessed only through
  defined interfaces.
- Higher-layer code MUST NOT reference concrete implementations directly.
- When a plausible alternative implementation exists, default to defining an interface.

**Rationale**: Enables backend swaps (e.g., DXGI → BitBlt, file → database) without rewriting
upper layers, and keeps test doubles straightforward.

### IV. Safe Code Standards

All production code MUST follow safe-by-default practices across three areas:

**Safe defaults**

- All public APIs MUST validate inputs and return explicit errors.
- `unwrap()`, `expect()`, and any other panic-inducing calls are PROHIBITED in production code
  paths; they are permitted only in test code.
- Errors MUST propagate and be logged exactly once, at the system boundary. Duplicate logging in
  intermediate layers is PROHIBITED.

**Explicitness over implicitness**

- Timestamps MUST carry typed clock-source distinctions (monotonic vs. wall-clock); mixing them as
  raw numbers is PROHIBITED.
- All tunable values MUST be declared in profile files; hardcoding in source is PROHIBITED.

**Minimal shared state**

- Thread data sharing MUST occur only through explicit channels or locks.
- Global mutable state is PROHIBITED.

**Rationale**: Panic-prone code in a long-running capture service causes silent data loss; mixed
clock sources corrupt temporal analysis; global state creates non-deterministic failures.

### V. Test-Validated Quality Gates

All numeric success criteria MUST be covered by automated tests.

- Capture latency, event-queue delay, and frame intervals MUST each have a performance regression
  test in CI; CI MUST fail on any regression.
- Unit tests MUST target interfaces, not implementations; they MUST pass unchanged when a backend
  is swapped.
- External dependencies (OS APIs, filesystem) MUST be replaced with mocks in unit tests.
- Session integrity tests MUST cover failure scenarios — abnormal shutdown, disk-full, thread panic
  — verifying that no corrupt data reaches storage.

**Rationale**: Unverified performance claims are marketing, not engineering; interface-level tests
preserve correctness through backend evolution.

### VI. Observable by Design

All performance metrics MUST be measurable at runtime without requiring code changes.

- Capture latency, queue depth, orphan event count, and session completion rate MUST be
  instrumentable by design.
- Any performance claim without a corresponding measurement mechanism is considered invalid.
- Debug output (e.g., capture-preview windows) MUST run in an independent thread.
- Enabling debug mode MUST NOT affect capture performance metrics.

**Rationale**: Observability without production overhead enables confident tuning and rapid incident
diagnosis in deployed sessions.

### VII. Minimum Privilege

The capture module MUST operate with only the minimum OS permissions required.

- WRITE, DEBUG, and memory-map access to game processes MUST NOT exist anywhere in the codebase.
- Any required permission MUST be documented with written justification in code comments or
  reference documentation.

**Rationale**: Minimizing privileges contains the blast radius of security incidents, avoids
anti-cheat interference, and enforces the non-intrusion boundary at the OS level.

## Configuration Management

All configuration MUST follow a hierarchical profile model.

- `base.yaml` defines every default value; game-specific profile files override only the values
  that differ.
- Switching a game profile (without any code change) MUST be sufficient to support a new game.
- Any value absent from a game profile MUST fall back to `base.yaml`; in-code defaults MUST NOT
  conflict with profile-declared values.

## Contribution & Review Standards

- Code that violates any principle of this Constitution MUST NOT pass review, regardless of author
  or origin.
- Changing a numeric performance threshold requires amending this Constitution first, with
  documented justification for the change.
- AI-generated code is subject to identical review standards; "AI-generated" is not an exception
  or a mitigation.

## Governance

This Constitution supersedes all other development practices and guidelines.

- **Amendments** require: (1) written rationale, (2) an update to this document with a version
  bump, and (3) a migration plan if existing code is affected.
- **Versioning**: MAJOR for principle removal or incompatible redefinition; MINOR for new principle
  or materially expanded guidance; PATCH for clarifications, wording, or typo fixes.
- **Compliance**: All PRs MUST include a Constitution Check confirming no principles are violated.
  Architectural reviews occur quarterly.
- Runtime development guidance is maintained in `.specify/memory/` and agent context files.

**Version**: 1.0.0 | **Ratified**: 2026-02-26 | **Last Amended**: 2026-02-26
