"""C4 seam regression check; complements the existing dependency inventory."""
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
errors = []
for path in [ROOT / "crates/app/src/agent/wire.rs", * (ROOT / "crates/app/src/agent").glob("dispatch*.rs"), *(ROOT / "crates/app/src/agent/dispatch").glob("*.rs")]:
    production = path.read_text().split("#[cfg(test)]\nmod tests", 1)[0]
    for forbidden in ("fold_motion::", "fold_compositor::", "fold_graph::reconcile", "Construction::", "Composite::"):
        if forbidden in production:
            errors.append(f"{path.relative_to(ROOT)} imports creative policy: {forbidden}")
for crate in ("motion", "compositor", "composition"):
    path = ROOT / f"crates/{crate}/src/proposals.rs"
    for forbidden in ("&mut Project", "Project::", ".commit(", ".commit_named(", "proposals::Store", "InspectionProof"):
        if forbidden in path.read_text():
            errors.append(f"{path.relative_to(ROOT)} crosses project authority: {forbidden}")
if errors:
    raise SystemExit("\n".join(errors))
print("Scoped proposal ownership passed: generic App dispatch has no creative models; providers have no project/candidate publication authority.")
