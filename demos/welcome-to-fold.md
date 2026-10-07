# Welcome to Fold

Open [welcome-to-fold.fold](welcome-to-fold.fold) and select **PLAY · Welcome to Fold · 18 seconds**. The project is 1920×1080, 30 fps, with three six-second Composition clips. Fonts are embedded; backgrounds are procedural. No external media or audio is required. Use a build containing Phase 57 GPU effects and transitions.

| Timeline | Text | Effect location |
| --- | --- | --- |
| 0–6 seconds | welcome to Fold! | First timeline clip → Effects → Glass |
| 6–12 seconds | A Video Editor, VFX, and Motion Graphics tool. | Second Composition → Glitch adjustment layer → Effects |
| 12–18 seconds | now available for free | Third Composition → text layer → Effects → Glow |

The cross dissolve spans 5.5–6.5 seconds. The circular radial wipe spans 11.5–12.5 seconds. Both are real timeline transitions referencing editable graph definitions. Each eight-second source Composition is used from source time 0.5 to 6.5 seconds, leaving valid transition handles.

Each timeline clip opens **one Composition**. Its named text layer owns the Motion network directly; select that layer to work on its Text, Align Content and Keyframes nodes. Background is a separate compositing layer. There is no intervening image-preparation wrapper. Clip 2 also has a Glitch adjustment layer; clip 3 has Glow attached directly to its text layer. Align Content keeps the phrase centered when its wording changes; longer replacements may need a smaller text size. The second phrase has an intentional line break after “VFX,”. The Keyframes node drives the whole phrase's vertical pop-in and overshoot. Its times are source seconds: 0.5 offscreen, 1.0 overshoot, 1.3 settled. Motion uses a 1280×720 logical canvas independently of its 1920×1080 raster.

Glass is a rounded frosted panel with mild animated refraction, blur, tint and edge highlights. Because it is a timeline effect, it processes the complete first Composition, including the text. It is a basic image effect, not a geometry material. Its definition exposes **Glass amount**. Glitch exposes **Glitch amount**, with RGB separation, scanlines and intermittent horizontal displacement. Both definitions contain editable WGSL Shader nodes. Their standalone Input nodes need a host image; preview through the clip or Composition.

In the third Composition, select the text layer to inspect its Glow stack. Edit Graph opens the reusable GPU glow definition; threshold, strength and radius remain editable. The separate halo layer and its unused graph were removed through a public transaction, preserving the current user edits. A fresh GPU render at 14 seconds passed and the project was saved. Selecting a layer opens its network while the Inspector stays on the layer container; selecting a node switches the Inspector to that node.

Verified through public Fold MCP tools on 7 October 2026: private construction and one-step commit, GPU renders of all titles and animation/transition samples, save, desktop reopen, retained effects/fonts/keys, and fresh GPU renders at 0, 2, 6, 8, 12, 14 and 17.9 seconds. A 1920×1080 first-clip render also succeeded; MCP displayed a diagnostic thumbnail. Continuous playback performance and manual UI interactions remain for review. This demo does not close the remaining Phase 57 implementation work.

Navigation correction: the original wrapper-based layout was replaced in the live session using Composition-owned graph layers and explicit Motion object bindings. GPU samples at 2, 6, 8, 12 and 14 seconds passed after restructuring; the corrected project was saved. Direct pointer navigation after this correction has not been independently exercised.
