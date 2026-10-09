# Make it move — Composition and Outliner

Open `Make-it-move.fold` in Composition. Expand its three scene layers to select
the backdrop, title and colored-sphere objects; Network shows each object's own
graph. Camera/light graphs remain separate. The title's reveal keys are in the
Composition timeline. No parameters are exposed.

To regenerate through public tools, start an empty desktop session with
`--protocol-dir <directory>` and the installed `FOLD_COLOR_ROOT`/`FOLD_VIDEO_ROOT`.
Then run `python3 demos/make-it-move.py <fold-cli> <socket> <new.fold> <new-report.json>`.
The script inspects candidates before Apply and saves frames and a project copy.
See the [qualification record](../docs/rewrite/evidence/m5a-public-scene-result.md),
and the later [observation evidence correction](../docs/rewrite/evidence/m5a-compositor-cutover/result.md).

## Mixed scene/compositor acceptance fixture

`m5a-mixed-scene.fold` has a background scene with independent sphere and plate
objects, its own camera/light, an explicitly connected Read → Grade → Output
compositing layer, and a transparent title scene with separate camera/light.
Expand scene rows in Composition, select objects in Outliner, use Network's
Graph/Source tabs, and edit keys in the shared Track/Curve view. The original
three-scene `Make-it-move.fold` remains a separate, unchanged deliverable.

Regenerate from an empty public session:

```sh
python3 demos/m5a-mixed-scene.py <fold-cli> <socket> <new.fold> <new-report.json>
```

The script uses discovery, inspected candidates, guarded Apply, frame observation
and public save only. See [cutover evidence](../docs/rewrite/evidence/m5a-compositor-cutover/result.md)
for source/conflict/undo/reopen and native visual qualification.

The older integrated fixture below is retained as earlier rewrite evidence.

# Try the Motion rewrite

From the repository root:

```sh
./demos/run-m5-demo.sh
```

This builds the current Linux desktop with spatial rendering, copies
[`m5-integrated.fold`](m5-integrated.fold) into a fresh temporary directory, and
opens that copy. The printed path is your editable project. Use **Save As** to
keep your work in your own project folder. A graphical desktop and Vulkan adapter
are required; run this in your desktop terminal.

The scene is **MAKE IT MOVE**: staggered orange typography, extruded text,
O/B/8 holes, and a 20×12 grid of shared boxes driven by a moving radial field,
a palette ramp, and deterministic rotation. It is 1920×1080 at 24 fps for four
seconds. The final playable frame is 95/24 seconds.

## New scene workflow

In Composition, choose **Composition → Add Scene**. Expand the scene track or
select its objects in Outliner, edit the selected object's graph in Network, and
animate its properties in Composition's Track/Curve view. Scene object scale has
independent X/Y/Z controls; parenting retains nonuniform scale and shear.

The existing **MAKE IT MOVE** project below still uses the earlier flat
Construction fixture. The separate M5a fixture above qualifies the two-scene
workflow. New scene creation no longer offers a competing Motion layer.

## Try these edits

1. Open the Motion document in the Project panel if it is not selected. Use the
   linked Viewer and play or scrub the first second to see the word entrance.
2. In **Graph**, select the wording input and edit it in the Inspector. Compare
   the Screen title and its extruded counterpart.
3. Change the entrance duration or word delay, then scrub again. Edit the Palette
   ramp and grid count. Undo and redo each change.
4. Move a graph node, save, close, and reopen your saved copy. Check that your
   values and graph position survived.
5. Use the graph creation menu to try **Transform · Screen**, **Transform · Mesh**,
   **Transform · Instances**, **Tessellate**, **Extrude Profile**, and the Stroke
   cap/join choices. Each starter includes the geometry it needs; connect its
   result into the appropriate placement/Scene branch to see it in the Viewer.

Transform · Screen accepts contours and Outline Text; Transform · Mesh accepts
metric meshes; Transform · Instances applies uniform scale and Z rotation about
an explicit World pivot while sharing the prototype. Tessellate and Extrude
Profile accept closed planar contours or Outline Text, with an explicit fill rule
and canvas-to-meter conversion. Outlined glyphs remain separate profiles, with no
implicit union between glyphs. A spatial mesh needs Material and World Placement.

## Reproduce the public authoring checks

The demo was authored with [`m5-integrated.py`](m5-integrated.py) through Fold's
public protocol. The script discovers the running catalog, proposes/inspects/
applies Source, samples the scene, changes controls, restores them and saves.
Connection and regeneration steps are in the
[public workflow](../skills/fold/references/public-workflow.md).

The demo is available for hands-on testing. This is not a claim that all M0–M5
gates have passed: the [completion audit](../docs/rewrite/m0-m5-completion-audit.md)
tracks remaining rendering/resource and physical artist acceptance work.
