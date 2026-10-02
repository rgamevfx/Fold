use super::*;
use crate::{SourceMedia, Track};
use fold_foundation::DocumentId;
use fold_project::{EditBatch, Mutation, Project};
use fold_ui::sdk::imgui::{Condition, Context, WindowKey};
fn t(n: i64) -> Time {
    Time::new(n, 1).unwrap()
}
struct Harness {
    context: Context,
    canvas: Canvas,
    project: Project,
    document: DocumentId,
    selected: Vec<ObjectId>,
    playhead: u32,
    layout: Option<Layout>,
    commits: usize,
}
impl Harness {
    fn new() -> Self {
        let mut context = Context::create();
        context.set_ini_filename(None::<String>).unwrap();
        context.io_mut().set_display_size([1200.0, 700.0]);
        context.io_mut().set_delta_time(1.0 / 60.0);
        context
            .font_atlas()
            .try_claim_legacy_renderer()
            .unwrap()
            .build();
        let mut project = Project::new(8);
        let document = DocumentId::new();
        let tracks = vec![
            Track::new(TrackKind::Video, "V1"),
            Track::new(TrackKind::Video, "V2"),
            Track::new(TrackKind::Audio, "A1"),
            Track::new(TrackKind::Audio, "A2"),
        ];
        let asset = AssetId::new();
        let link = Some(ObjectId::new());
        let make = |index: usize, start, duration, link| Clip {
            id: ObjectId::new(),
            track: tracks[index].id,
            link,
            asset,
            info: SourceMedia::Video(fold_media::VideoInfo {
                width: 2,
                height: 2,
                rate: [24, 1],
                frames: 240,
            }),
            start: t(start),
            duration: t(duration),
            source_start: Time::ZERO,
            level: 1.0,
            extensions: Default::default(),
        };
        let clips = vec![
            make(0, 0, 2, link),
            make(2, 0, 2, link),
            make(1, 6, 2, None),
        ];
        let sequence = Sequence {
            tracks,
            clips,
            dimensions: [2, 2],
            rate: [24, 1],
            extensions: Default::default(),
        };
        project
            .commit(EditBatch {
                base: project.snapshot().revision(),
                mutations: vec![
                    Mutation::PutAsset(fold_project::Asset {
                        id: asset,
                        location: "test.mp4".into(),
                        fingerprint: "test".into(),
                        extensions: Default::default(),
                    }),
                    Mutation::PutDocument(sequence.document(document).unwrap()),
                ],
            })
            .unwrap();
        let canvas = Canvas {
            fit: false,
            snapping: false,
            view: View {
                pixels_per_frame: 3.0,
                ..View::default()
            },
            ..Canvas::default()
        };
        Self {
            context,
            canvas,
            project,
            document,
            selected: vec![],
            playhead: 0,
            layout: None,
            commits: 0,
        }
    }
    fn sequence(&self) -> Sequence {
        Sequence::from_document(&self.project.snapshot().state().documents[&self.document]).unwrap()
    }
    fn point(&self, frame: f64, row: usize) -> [f32; 2] {
        let layout = self.layout.unwrap();
        let rect = layout.row_rect(row, self.canvas.view.scroll_y);
        [
            self.canvas.view.x(frame, layout.body.min[0]),
            (rect.min[1] + rect.max[1]) / 2.0,
        ]
    }
    fn tick(&mut self, point: [f32; 2], button: Option<bool>) {
        self.context.io_mut().add_mouse_pos_event(point);
        if let Some(down) = button {
            self.context
                .io_mut()
                .add_mouse_button_event(MouseButton::Left, down);
        }
        let snapshot = self.project.snapshot();
        let sequence = self.sequence();
        let labels = BTreeMap::new();
        let ui = self.context.frame();
        let mut actions = Vec::new();
        let key = WindowKey::new("test.timeline.canvas", "Timeline test").unwrap();
        ui.window(&key)
            .position([0.0, 0.0], Condition::Always)
            .size([1100.0, 400.0], Condition::Always)
            .build(|| {
                self.layout = Some(Layout::new(
                    ui.cursor_screen_pos(),
                    ui.content_region_avail(),
                    (ui.text_line_height() / 13.0).max(0.75),
                ));
                actions = self.canvas.draw(
                    ui,
                    CanvasModel {
                        sequence: &sequence,
                        revision: snapshot.revision(),
                        playhead: self.playhead,
                        selected: &self.selected,
                        labels: &labels,
                    },
                );
            });
        self.context.end_frame();
        for action in actions {
            match action {
                Action::Commit { base, edit } => {
                    assert_eq!(base, snapshot.revision());
                    self.project
                        .commit(crate::edit_sequence(&snapshot, self.document, &edit).unwrap())
                        .unwrap();
                    self.commits += 1;
                }
                Action::Select(ids) => self.selected = ids,
                Action::Seek(frame) => self.playhead = frame,
                Action::Pause => {}
                Action::Place { .. } => panic!("unexpected Project placement in clip gesture test"),
            }
        }
    }
}
#[test]
fn actual_imgui_canvas_moves_trims_seeks_and_cancels_without_intermediate_history() {
    let mut h = Harness::new();
    h.tick([0.0, 0.0], None);
    h.tick([0.0, 0.0], None);
    let initial = h.project.snapshot();
    let press = h.point(12.0, 1);
    h.tick(press, Some(true));
    assert!(h.canvas.drag.is_some());
    assert_eq!(h.selected.len(), 2);
    let target = h.point(108.0, 0);
    h.tick(target, None);
    assert_eq!(h.project.snapshot().revision(), initial.revision());
    h.tick(target, Some(false));
    assert_eq!(h.commits, 1);
    let moved = h.sequence();
    assert_eq!(moved.clips[0].start, t(4));
    assert_eq!(moved.clips[1].start, t(4));
    assert_eq!(moved.clips[0].track, moved.tracks[1].id);
    assert_eq!(moved.clips[1].track, moved.tracks[2].id);
    h.project.undo().unwrap();
    h.tick(target, None);
    // A drop overlapping the cutaway is rejected without a transaction.
    let revision = h.project.snapshot().revision();
    let press = h.point(12.0, 1);
    h.tick(press, Some(true));
    let invalid = h.point(132.0, 0);
    h.tick(invalid, None);
    assert!(h.canvas.message.contains("overlap"), "{}", h.canvas.message);
    h.tick(invalid, Some(false));
    assert_eq!(h.project.snapshot().revision(), revision);
    // Trim by the visible left handle. Both linked ranges follow.
    let left = h.point(0.0, 1);
    let left = [left[0] + 2.0, left[1]];
    h.tick(left, Some(true));
    let trim = [left[0] + 24.0 * 3.0, left[1]];
    h.tick(trim, None);
    h.tick(trim, Some(false));
    let trimmed = h.sequence();
    for clip in &trimmed.clips[..2] {
        assert_eq!(clip.start, t(1));
        assert_eq!(clip.source_start, t(1));
        assert_eq!(clip.duration, t(1));
    }
    assert_eq!(h.commits, 2);
    h.project.undo().unwrap();
    h.tick(trim, None);
    // Escape discards the local preview, including on the subsequent release.
    let before = h.project.snapshot().revision();
    let press = h.point(12.0, 1);
    h.tick(press, Some(true));
    let target = h.point(60.0, 1);
    h.tick(target, None);
    h.context.io_mut().add_key_event(Key::Escape, true);
    h.tick(target, None);
    assert!(h.canvas.drag.is_none());
    h.tick(target, Some(false));
    assert_eq!(h.project.snapshot().revision(), before);
    h.context.io_mut().add_key_event(Key::Escape, false);
    h.tick(target, None);
    let layout = h.layout.unwrap();
    let ruler = [
        h.canvas.view.x(100.0, layout.body.min[0]),
        layout.ruler.min[1] + 10.0,
    ];
    h.tick(ruler, Some(true));
    assert_eq!(h.playhead, 100);
    h.tick(ruler, Some(false));
    assert_eq!(h.commits, 2);
}
