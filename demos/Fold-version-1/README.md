# Fold version 1 — clip 01

## Updated reveal

The latest file is [Fold-version-1-noise-reveal.fold](../Fold-version-1-noise-reveal.fold).
The text stays in place while a sweep flickers through its dots, then settles.
**Sweep Time** (0.9 seconds) and **Noise Tail** (0.65 seconds) are Motion container
inputs referenced directly in the color parameter. The graph still has ten nodes.
The 250 × 55 sampling image and later purple wave are retained.
See the [animated preview](noise-reveal.gif) and [revision 45 evidence](noise-reveal-evidence.json).
Twelve rendered frames cover 0–2.25 seconds. Current artist changes present at
revision 41 were preserved. The original version below remains available.

## Original reveal

Open [Fold-version-1.fold](../Fold-version-1.fold). Select **01 · FOLD / VERSION 1**, then the **Dot display · Motion** layer. The six-second Composition is 1920 × 1080 at 30 fps.

The display reveals **FOLD V1**, holds the lettering, then changes to a purple wave. The headline reads **FOLD / VERSION 1**. All artwork is native and editable; there is no external video dependency.

## Controls

- Composition: **Artwork Position**, **Artwork Opacity**.
- Motion object **Dot display**: **Dot Radius**, **Columns**, **Rows**, **Spacing**, **Highlight**, **Shadow**, **Speed**, **Accent** on Channel References. Speed is an integer multiplier for the image and color clocks.
- Motion document **Dot display · Image source**: **Text**. Its Message node controls text size; Message position contains the reveal expression.
- Separate scene objects hold the headline, subtitle, footer, panel, background, camera, and light.

The main Motion graph has ten nodes. Grid and a single disc prototype feed Copy to Points. Two linear fields map positions to image UVs. Sample Image reads the text mask; Color Animator applies the palette. Dot size reads the Dot Radius channel. The small Wrangle creates only the shared 16-sided disc, once per evaluation. There are 840 instances and no primitive control nodes.

## Preview and evidence

- [Hero screenshot](hero.png), 1920 × 1080, at two seconds.
- [Contact sheet](contact-sheet.png): opening, reveal, held lettering, color transition.
- [MCP evidence](evidence.json): inspected native records and Source, parameter samples, saved revision, and limitations.
- Screenshot provenance is saved beside each PNG. Files were saved through MCP at revision 16; an identical save retry returned the same successful result.

## Current limits

This is the first Composition, ready for later sequence assembly. The running MCP discovery exposes no Timeline authoring provider. Composition controls cannot bind to Motion inputs, so the creative controls remain in the Motion graph. Native UI interaction and reopening the saved file have not been verified.

The verified density is **60 × 14**, with a **250 × 55** sampled text image. Larger animated grids, larger sampled text images, and dense per-point Wrangle edits reached the runtime budgets during authoring. These are observed limits of this setup, not established universal maximums. Text meshes use small extrusions at detail 0.1; the initial planar text-mesh setup exceeded the element budget. Runtime fixes are outside this artwork task.

The rewrite-plan documents referenced by AGENTS.md are absent from this checkout and HEAD; this demo does not claim rewrite-phase completion.
