# Media in Motion

Open [motion-image-inputs.fold](motion-image-inputs.fold). Four editable documents demonstrate the new image inputs:

- **Warped texture:** the linked test image stretches with an animated subdivided grid. Select the object for size and grid density; **Edit image source** replaces the image. The graph's Wave controls deformation amplitude and timing.
- **Media carousel:** six moving cards alternate an image and video. The source collection chooses the content; Index selects per copy, Grid Points controls the layout, and Wave animates the layout origin. Card fit preserves the source aspect ratio by default.
- **Video ASCII:** an embedded font atlas follows the video's luminance. Edit Characters (dark to light), Columns, Rows, Gain, Bias and Color on the object. **Edit image source** replaces the video.
- **Nested image output:** a card consumes the live Warped texture document without flattening it.

The study is 640×360, 30 fps, three seconds. Companion PNGs show 0, 0.5 and 1.5 seconds. The `.ppm` and `.mp4` files beside the project are linked fixtures, so keep them available; relink them through Project if the repository moves.

Rebuild the study through the public interface using [image_inputs.py](../skills/fold/scripts/image_inputs.py), supplying a live session and a new output path. [Phase 49](../docs/phase-49-motion-images.md) records checks and performance evidence. The supplied test pattern demonstrates mechanics; it does not recreate the inaccessible Instagram reference.
