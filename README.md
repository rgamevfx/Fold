# Fold

Fold is a video editor, compositor, and application for motion graphics and 3D.
Connect nodes to build effects, transitions, presets, and templates of your own.
A node does one task. Its connections pass the result to other nodes.

Fold is in development. Features and file formats can change.

![Fold desktop with the viewer, project panel, timeline, and node graph](resources/readme/desktop.png)

## Work with images, clips, and nodes

| Edit | Composite | Animate |
| --- | --- | --- |
| Arrange video and audio on a timeline. | Connect nodes to combine images and apply effects. | Change values over time to create motion graphics and 3D scenes. |

![Node graph in Fold](resources/readme/nodes.png)

Connect nodes in the Network panel. Select a node to change its controls in the
Inspector. Use the Viewer to see the result.

![Node controls in Fold](resources/readme/inspector.png)

## Build and run

Use stable Rust. On Ubuntu, install the build tools and FFmpeg. The desktop also
needs a Vulkan graphics driver.

```sh
sudo apt-get install -y build-essential cmake ninja-build pkg-config libasound2-dev ffmpeg python3
cargo run -p fold-app --bin fold-desktop --features desktop --locked
```

Color-managed rendering needs Fold's private OpenColorIO runtime. Build it once,
then set `FOLD_COLOR_ROOT` when you run Fold:

```sh
python3 scripts/build_color_runtime.py /absolute/build-dir /absolute/color-package
FOLD_COLOR_ROOT=/absolute/color-package cargo run -p fold-app --bin fold-desktop --features desktop --locked
```

Use absolute paths and a package directory that does not exist. The runtime build
requires the exact compiler and CMake versions in
[toolchain.json](resources/color/toolchain.json).

To build a Linux package with the desktop, CLI, color runtime, and native video
helper:

```sh
python3 scripts/package_fold.py /absolute/build-dir /absolute/new-package-dir
/absolute/new-package-dir/bin/fold-desktop
```

The video helper build also needs CUDA development headers. Native video decoding
needs a compatible NVIDIA driver. See the [video runtime requirements and
notices](resources/licenses/video-runtime.md).

Fold uses Rust and Dear ImGui. Color processing uses OpenColorIO 2.4.2 and ACES 1.3.
Third-party notices are in [resources/licenses](resources/licenses),
[resources/color](resources/color), and [resources/fonts](resources/fonts).
