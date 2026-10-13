# Documentation

Read the document for your task, not the whole directory.

| Task | Source of truth |
| --- | --- |
| Understand ownership and edit/render flow | [Architecture](architecture.md) |
| Continue or qualify the Motion/3D replacement | [Current work](current-work.md) |
| Build, run, package or choose validation commands | [Development](development.md) |
| Change native UI | [UI skill](../.agents/skills/fold-ui/SKILL.md) |
| Author artwork through public tools | [Fold skill](../skills/fold/SKILL.md) |
| Work on pinned native color or Skia inputs | [Color resources](../resources/color/README.md), [Skia build](../resources/motion-context/Skia_Precision_Build.md) |

## Keep this small

- One owner per fact: architecture holds contracts; current work holds open tasks
  and acceptance; skills hold workflows; source, schemas and `--help` hold API details.
- Update existing sections in place. Completed work leaves the checklist unless
  its evidence or limitation affects a remaining decision.
- Put detailed measurements beside fixtures, not in chronological prose logs.
  Link only the evidence needed for a current decision. Record build/fixture
  identity and measurement scope; a past result is not a current test run.
- Use Git for history. Keep session paths, PIDs, launch logs and temporary handoffs
  outside the repository. Add a document only for a distinct task that needs it;
  link it from its nearest relevant entry point.

## Audit and retained material

The October 2026 cleanup replaced the 4,605-line Motion replacement log, two
handoffs and duplicate status reports with the system map and current checklist.
It removed missing-plan links, retired Motion UI guidance and broken README
images; build examples now include required SDK inputs. This is a documentation
reorganization, not a new product completion or runtime qualification claim.

Historical tracked prose is available at commit `8051a93`:
`git show 8051a93:docs/Motion_Context_Replacement.md` (and the old handoff/report paths).
Pre-cleanup worktree copies, including uncommitted updates and the ignored HTML
explainer, were separately backed up on the local machine before removal.

Retained, on demand:

- [Motion evidence](../resources/motion-context): machine-readable results and
  render fixtures. Former `docs/Motion_Replacement_Report_assets/` images now
  live under `review/`; they are renderer output, not UI screenshots.
- [Compositor prototypes](../crates/compositor/prototypes): design studies, not
  implemented UI or product contracts.
- [Fold version 1](../demos/Fold-version-1/README.md): historical artwork, not a
  current-provider tutorial.
- [OCIO probe](../scripts/phase11-ocio/README.md): isolated historical experiment.
- Resource provenance and license notices remain with their resources.
