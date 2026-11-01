# BRIEFING — 2026-10-31T03:34:30Z

## Mission
Execute the approved Master Architectural Gap Matrix & Comprehensive Engineering Plan (v2.8) for RapidRAW to resolve stability issues, fix dark TIFF exports and large folder crashes, restore navigation, and build the release installer.

## 🔒 My Identity
- Archetype: orchestrator
- Roles: orchestrator, user_liaison, human_reporter, successor
- Working directory: d:\RapidRAW_V.03\.agents\teamwork\orchestrator
- Original parent: parent (8e725be5-2992-4197-8466-544565a7f9b2)
- Original parent conversation ID: 8e725be5-2992-4197-8466-544565a7f9b2

## 🔒 My Workflow
- **Pattern**: Project
- **Scope document**: d:\RapidRAW_V.03\.agents\teamwork\orchestrator\PROJECT.md
1. **Decompose**: Survey with 3 parallel Explorers, decompose into modules/milestones, enforce Feature Inventory
2. **Dispatch & Execute**:
   - **Direct (iteration loop)**: Explorer(s) -> Worker -> Reviewer(s) -> Challenger(s) -> Forensic Auditor -> Gate
3. **On failure** (in this order):
   - Retry: nudge stuck agent or re-send task
   - Replace: spawn fresh agent with partial progress
   - Skip: proceed without (only if non-critical)
   - Redistribute: split stuck agent's remaining work
   - Redesign: re-partition decomposition
   - Escalate: report to parent (sub-orchestrators only, last resort)
4. **Succession**: At 16 spawns, write handoff.md, cancel crons, spawn successor
- **Work items**:
  1. Phase 0: Survey & Map scope (R1-R8) [done]
  2. M1: Dark TIFF Export Tone-Mapping Parity (R1) & Progress Broadcasting (R3) [done]
  3. M2: Large Folder Crash Prevention & Linear DNG Support (R2) [verifying gate]
  4. M3: Window Transparency, Background & Dual-Tab Left Sidebar Navigation (R4, R5) [pending]
  5. M4: White Balance Dropper Fallback & Auto-Crop Pixel Scaling (R6) [pending]
  6. M5: Brand Scrubbing & Shader Synchronization (R7) [pending]
  7. M6: Verification, Test Suite & Release Build Packaging (R8) [pending]
- **Current phase**: 3 (M2 Verification Gate)
- **Current focus**: Milestone 2 - Reviewers, Challengers, and Forensic Auditor running quality gate verification

## 🔒 Key Constraints
- NEVER write, modify, or create source code files directly.
- NEVER run build/test commands yourself — require workers to do so.
- NEVER investigate or explore the problem at the code level — dispatch Explorers for technical investigation.
- You MAY use file-editing tools ONLY for metadata/state files (.md) in your .agents/teamwork/ folder.
- Always include path to ORIGINAL_REQUEST.md in every subagent dispatch.
- Mandatory integrity warning in Worker dispatch prompts.
- Forensic Auditor verdict is a hard binary veto.
- Never reuse a subagent after it has delivered its handoff — always spawn fresh.

## Current Parent
- Conversation ID: 8e725be5-2992-4197-8466-544565a7f9b2
- Updated: 2026-10-30T22:45:00Z

## Key Decisions Made
- Milestone 1 passed all quality gates.
- Worker M2 completed Milestone 2 implementation and tests.
- Dispatched Reviewer 1, Reviewer 2, Challenger 1, Challenger 2, and Forensic Auditor in parallel for Milestone 2 Quality Gate.

## Team Roster
| Agent | Type | Work Item | Status | Conv ID |
|-------|------|-----------|--------|---------|
| teamwork_explorer_survey_v28_1 | teamwork_explorer | Survey R1 & R3 | completed | 9cad7fd2-9928-4248-b748-6e69a5746c9a |
| teamwork_explorer_survey_v28_2 | teamwork_explorer | Survey R2 | completed | fa889be1-701e-4946-84d8-5ba93b1e23a4 |
| teamwork_explorer_survey_v28_3 | teamwork_explorer | Survey R4-R8 | completed | ad5d48c8-6ea3-4f19-a80f-9dad227dcfed |
| worker_m1 | teamwork_worker | Milestone 1 Implementation (R1, R3) | completed | 8f58f58e-134b-4729-ad1e-c24817375bfe |
| reviewer_m1_1 | teamwork_reviewer | M1 Review & Verification 1 | completed | c3e02d4a-e3fa-4b66-b9ad-d9e588c7973e |
| reviewer_m1_2 | teamwork_reviewer | M1 Review & Verification 2 | completed | f317a4a6-a9ba-48d8-9a00-14d7ef8752a2 |
| challenger_m1_1 | teamwork_challenger | M1 Empirical Stress & Parity | completed | 35504116-3fb8-4fea-a5e2-a3e39c143247 |
| challenger_m1_2 | teamwork_challenger | M1 Regressions & DNG Verification | completed | a1364985-2dbd-46c6-a79e-1135916695cd |
| auditor_m1_1 | teamwork_auditor | M1 Forensic Integrity Audit | completed | 46ec43df-de94-4b21-bee0-52e0824a67a5 |
| worker_m2 | teamwork_worker | Milestone 2 Implementation (R2) | completed | 9d04570f-7325-480d-9c50-e0dc80d50084 |
| reviewer_m2_1 | teamwork_reviewer | M2 Review & Verification 1 | in-progress | de0c60b3-aae0-4682-9d50-7cddde951921 |
| reviewer_m2_2 | teamwork_reviewer | M2 Review & Verification 2 | in-progress | 2e2a026f-bfa3-446f-9b53-90e14f33d840 |
| challenger_m2_1 | teamwork_challenger | M2 Safe IO & Memory Stress | in-progress | 45e0b5f9-6f0c-4ef2-beff-782e29c9e64a |
| challenger_m2_2 | teamwork_challenger | M2 Linear DNG & EXIF Stress | in-progress | a1a2e0ba-765d-4489-80e4-495bad1541d9 |
| auditor_m2_1 | teamwork_auditor | M2 Forensic Integrity Audit | in-progress | ed6d3952-a9be-468a-9b47-34ed658d79cf |

## Succession Status
- Succession required: no
- Spawn count: 15 / 16
- Pending subagents: de0c60b3-aae0-4682-9d50-7cddde951921, 2e2a026f-bfa3-446f-9b53-90e14f33d840, 45e0b5f9-6f0c-4ef2-beff-782e29c9e64a, a1a2e0ba-765d-4489-80e4-495bad1541d9, ed6d3952-a9be-468a-9b47-34ed658d79cf
- Predecessor: none
- Successor: not yet spawned

## Active Timers
- Heartbeat cron: 5a72144f-8be1-4295-9cb2-a3907b4e00a6/task-45 (every 10 min)
- Safety timer: 5a72144f-8be1-4295-9cb2-a3907b4e00a6/task-487 (600s)
- On succession: kill all timers before spawning successor
- On context truncation: handle timers as step 4 of the State Recovery section of your prompt says

## Artifact Index
- d:\RapidRAW_V.03\.agents\teamwork\ORIGINAL_REQUEST.md — Authoritative User Request
- d:\RapidRAW_V.03\.agents\teamwork\orchestrator\PROJECT.md — Global Project Specification & Milestones
- d:\RapidRAW_V.03\.agents\teamwork\orchestrator\GATE_STATUS.md — Quality Gate Status (M1 PASSED)
- d:\RapidRAW_V.03\.agents\teamwork\worker_m2\handoff.md — Worker M2 Completion Report
