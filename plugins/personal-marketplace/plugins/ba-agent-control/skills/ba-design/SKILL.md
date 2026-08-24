---
name: ba-design
description: Design a BA Agent change when an approved requirement needs interfaces, data flow, safety boundaries, migration impact, and verification strategy.
---

# Design a change

Trace current code and contracts first. Describe the smallest architecture that meets the requirement, including affected components, public interfaces, state transitions, failure handling, and security boundaries.

Call out data migration, compatibility, and rollback risk explicitly. Prefer reversible choices and existing project conventions. Do not invent APIs or claim parity that the active runtime cannot provide.

Return a decision-ready design and a list of risks that the plan must cover; do not implement it.
