# Fold authoring concepts

Read this when planning a construction or resolving terminology. Use the live catalog for exact node IDs, sockets, defaults, settings, and current capability support.

| Concept | Meaning and authoring consequence |
| --- | --- |
| Document / output | A Motion, Composite, or Sequence document owns editable work. A document/output reference lets another document consume its result without rendering an intermediate file. Resolve both ID and output name. |
| Scene object / graph node | An object participates in the Motion scene. A raw node computes something; it needs a path to an authored output to become visible. `object` can establish scene wiring and resources that `add` alone does not. |
| Content / points | Content carries drawable geometry and hierarchy. Points describe a distribution used by consumers such as Instance on Points and Text on Points. Reuse the same distribution and ordering when overlays must correspond to copies. |
| Uniform value / field | A uniform value is shared; a field is sampled by its consumer using context such as position, index, identity, and count. Linking a field does not bake one value per frame. The consumer determines where sampling occurs. |
| Index / identity | Index is an element's position in an ordered domain; identity identifies the element. An even-index selection stays aligned only while branches retain the same ordering. Changing count can legitimately rearrange an index-derived pattern. |
| Domain | Which elements receive a response: for example Points or Instances. Moving label points uses Points; modifying copied circles uses Instances. Inspect supported domains instead of assuming all responses accept all domains. |
| Driver / influence / response | A driver produces change (Time, Wave, Keyframes); influence selects elements or supplies weights; a response applies it (position, scale, opacity). Sharing drivers preserves relationships between branches. |
| Selection / weight | Selection is Bool; weight is a scalar strength. A comparison cannot be wired directly to a scalar Mix weight. Use selection sockets for filtering, or a scalar mapping for blending. |
| Combine mode | Replace supplies an absolute value in the current space; Multiply scales an existing value. Match the mode to the intended relationship: set centers with Replace, resize existing instances with Multiply. |
| Local / downstream coordinates | Source geometry has an origin; later transforms change placement. Center a source before copying it. A Position field sees its consumer's upstream context, so labels need an explicit later center offset when reporting composition coordinates. |
| Content Group / reusable group | A Content Group combines back/front content. A reusable group defines an internal graph with typed public ports; a Group Instance references that definition. They are different operations despite the shared word. |
| Definition / instance override | Editing a definition changes its shared construction and defaults. Setting an instance input changes that instance. Discover its `settings.group`, inspect the definition's ports, and edit by stable port key. |
| Published control | A typed public port plus internal wiring. Its display name can change while its stable key stays the same. A listed control is only useful if changing it affects the intended output. |
| Embedded font | A resource belonging to the document. A raw text node with no font cannot render. Reuse an inspected same-document font or create a text object through public authoring to establish one. |
| Working / display color | Authored color values are interpreted in the working space, then transformed for display. Values above 1 may be intentional. A screenshot's RGB triplet is not automatically a valid working-space match. |
| Exact time | Use rational seconds, e.g. frame 45 at 30 fps is `{"numerator":45,"denominator":30}`. A period, document duration, and drift speed are separate controls unless explicitly wired together. |
| Viewer / delivery output | A viewer presents a selected document/output at its own time. Navigation does not establish the project's delivery choice. Displayed and requested identities can differ while a frame is pending. |
| Revision / project epoch | Revision guards authored state; epoch also detects project replacement/save context changes. Refresh both from `state` before mutations and after save/reopen. |
| Job / committed result | A returned job ID acknowledges queued work. Poll `jobs` and inspect `outcome.ok` before using inspection data, claiming an edit, or reporting a save. |

To plan new work, name its content, distribution, driver, influence, and response; then identify which relationships must remain shared when the artist changes a control. The [Motion reference](motion.md) provides typed wiring conventions, and the [dot-study workflow](procedural-dot-study.md) combines them into a complete construction.
