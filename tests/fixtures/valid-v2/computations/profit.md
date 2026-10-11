---
type: Attested Computation
title: Gross profit for fiscal year
description: Gross profit by segment, via a shared computation file.
status: stable
runtime: dbt
computation: references/computations/profit.sql
parameters:
  - { name: year, type: integer, required: true }
  - { name: segment, type: string, required: true }
executor:
  resource: references/skills/run-dbt.md
generated: { by: reference_agent/gemini-2.5-pro, at: 2026-06-14T14:00:00Z }
verified:
  - { by: process:finance-nightly, at: 2026-06-12T08:00:00Z }
sources:
  - id: cost-alloc
    resource: https://wiki.acme/finance/cost-allocation
    title: Cost allocation standard
---

# Definition

Gross profit per the standard.[^cost-alloc] The computation lives in the
file named by `computation:`, so no inline fence is needed.

[^cost-alloc]: Cost allocation standard
