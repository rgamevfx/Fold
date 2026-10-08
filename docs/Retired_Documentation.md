# Documentation retirement register

**Effective:** 8 October 2026. This register changes documentation authority; it does not claim that the old implementation has already been removed.

The [Motion System Rewrite Plan](Motion_System_Rewrite_Plan.md) is the active roadmap. Its [architecture and node-set proposal](Fold_Motion_Architecture_and_Node_Set_Proposal.md) defines target semantics, subject to the plan's explicit complete-replacement policy.

Retirement keeps original paths and historical bodies intact so links, source investigations and evidence remain usable. Historical instructions do not authorize retaining code, resuming old phases or implementing compatibility. Old “current,” “active,” and “next” labels must be read as statements at the record's original date.

| Documents | Disposition |
| --- | --- |
| [dev-plan.md](../dev-plan.md) | Entire roadmap retired and frozen. Motion/agent obligations replaced; unrelated unfinished work deferred, not completed |
| `docs/phase-*.md`, `docs/evidence/**`, `docs/archive/**` | Historical phase scope and evidence only. Old motion/protocol gates do not apply to the replacement; observations retain their original limitations |
| [Unified Motion proposal](Fold_Unified_Motion_Proposal.md) | Superseded by the new motion proposal and rewrite plan |
| [Motion Graphics Reference](Fold_Motion_Graphics_Reference.md) and its HTML edition | Historical old-engine capabilities, not a target catalog |
| [MoGraph authoring overhaul](mograph-authoring-overhaul.md) | Retired authoring direction, including its subsequent Rust-asset interpretation |
| [Agent interface research](agent-interface-research.md) | Historical protocol design, not a required compatibility surface |
| [fframes authoring research](fframes-authoring-research.md), [agent-first research](research/fframes-agent-first-unified-motion.md), [programmable nodes research](research/programmable-motion-nodes.md) | Research retained; old implementation/retention recommendations retired |
| `docs/consolidation*.md`, historical motion performance/UI plans and other earlier research | Background only wherever they prescribe motion or agent architecture. Revalidate useful observations before adopting them |
| [Product specification](Fold_Application_Product_Architecture.md) | Shared non-motion rules remain active. Conflicting motion/agent/render integration and roadmap sections are superseded by the new plan/proposal |
| [Architecture inventory](architecture.md) | Pre-rewrite source map and shared ownership reference. Motion/protocol inventory is removal context until updated with new evidence |
| [Fold skill](../skills/fold/SKILL.md), `skills/fold/references/**`, existing motion demo guides and protocol scripts | Old-build instructions/examples; no authority over the rewrite. Replace and execute new examples during M3–M7; delete obsolete shipped artifacts under M1/M9 |

The Fold UI skill remains applicable to new UI work. Historical visual references may inform new artwork, but executable demos and protocol examples must be rebuilt and qualified through the new public surface. The eight later geometry candidates remain separate from initial rewrite acceptance.

After each rewrite phase, update the active plan and affected current reference material. Keep historical results dated; do not rewrite them to imply new-engine acceptance. At M9, the current product/architecture documents must describe the accepted replacement, and shipped skills/catalogs must contain only supported new examples.
