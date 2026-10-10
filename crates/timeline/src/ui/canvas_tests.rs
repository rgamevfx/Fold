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
    compositions: Vec<Vec<ObjectId>>,
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
            effects: vec![],
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
            transitions: vec![],
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
            compositions: vec![],
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
                        placement: None,
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
                Action::Pause | Action::Preview { .. } | Action::CancelPreview => {}
                Action::NewComposition(ids) => self.compositions.push(ids),
                Action::Open(_) | Action::AddTransition(_) => {}
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

#[test]
fn dragging_above_video_creates_one_track_and_escape_creates_none() {
    let mut h = Harness::new();
    h.tick([0., 0.], None);
    h.tick([0., 0.], None);
    let press = h.point(12., 1);
    h.tick(press, Some(true));
    let mut destination = h.point(12., 0);
    destination[1] = h.layout.unwrap().body.min[1] + 8.;
    h.tick(destination, None);
    assert_eq!(h.sequence().tracks.len(), 4);
    h.tick(destination, Some(false));
    assert_eq!(h.commits, 1);
    assert_eq!(h.sequence().tracks.len(), 5);
    h.project.undo().unwrap();
    h.tick(destination, None);
    h.tick(press, Some(true));
    h.tick(destination, None);
    h.context.io_mut().add_key_event(Key::Escape, true);
    h.tick(destination, None);
    h.tick(destination, Some(false));
    assert_eq!(h.sequence().tracks.len(), 4);
    assert_eq!(h.commits, 1);
}

#[test]
fn marquee_selects_linked_clips_without_history_and_track_resize_is_local() {
    let mut h = Harness::new();
    h.tick([0., 0.], None);
    h.tick([0., 0.], None);
    let start = h.point(220., 0);
    h.tick(start, Some(true));
    let end = h.point(0., 3);
    h.tick(end, None);
    h.tick(end, Some(false));
    assert_eq!(h.selected.len(), 3);
    assert_eq!(h.commits, 0);
    let layout = h.layout.unwrap();
    let row = layout.row_rect(0, 0.);
    let point = [layout.area.min[0] + 15., row.max[1] - 2.];
    h.tick(point, Some(true));
    let target = [point[0], point[1] + 25.];
    h.tick(target, None);
    h.tick(target, Some(false));
    assert_eq!(h.canvas.track_heights.len(), 1);
    assert_eq!(h.canvas.row_height, 58.);
    assert_eq!(h.commits, 0);
}

#[test]
fn snapping_uses_exact_clip_edges_and_excludes_the_moving_selection() {
    let h = Harness::new();
    let sequence = h.sequence();
    let edge = Time::new(143, 24).unwrap();
    assert_eq!(
        interaction::snap_offset(&sequence, &[], &[edge], Time::ZERO, 2.).unwrap(),
        Some(Time::new(1, 24).unwrap())
    );
    assert_eq!(
        interaction::snap_offset(&sequence, &[sequence.clips[2].id], &[edge], Time::ZERO, 2.)
            .unwrap(),
        None
    );
}

#[test]
fn native_drop_uses_scrolled_scaled_lanes_and_fractional_frame_time() {
    let track = Track::new(TrackKind::Video, "V1");
    let sequence = Sequence {
        transitions: vec![],
        dimensions: [64, 36],
        rate: [30000, 1001],
        tracks: vec![track.clone()],
        clips: vec![],
        extensions: Default::default(),
    };
    let mut canvas = Canvas::default();
    canvas.snapping = false;
    canvas.view.first = 10.;
    canvas.view.pixels_per_frame = 2.;
    canvas.view.scroll_y = 20.;
    let layout = Layout::new([30., 40.], [900., 600.], 1.5);
    canvas.drop_layout = Some(layout);
    let row = layout.row_rect(0, canvas.view.scroll_y);
    let position = [layout.body.min[0] + 40., row.min[1] + 25.];
    let target = DocumentId::new();
    let request = canvas
        .native_placement(target, &sequence, position, 0)
        .unwrap();
    assert_eq!(request.target, target);
    assert_eq!(request.track, Some(track.id));
    assert_eq!(request.at, Time::new(1001, 1000).unwrap());
    assert!(!request.new_track);
    assert!(!request.audio_only);
    assert!(
        canvas
            .native_placement(target, &sequence, [layout.area.min[0], position[1]], 0)
            .is_none()
    );
}

#[test]
fn transition_strip_selects_its_own_id_and_drags_one_undo_step() {
    let mut h = Harness::new();
    let mut sequence = h.sequence();
    sequence.clips.truncate(1);
    sequence.clips[0].link = None;
    sequence.clips[0].source_start = t(1);
    let mut incoming = sequence.clips[0].clone();
    incoming.id = ObjectId::new();
    incoming.start = t(2);
    let transition = crate::Transition {
        id: ObjectId::new(),
        outgoing: sequence.clips[0].id,
        incoming: incoming.id,
        start: Time::new(3, 2).unwrap(),
        duration: t(1),
        effect: fold_platform::effects::Instance {
            id: ObjectId::new(),
            definition: fold_project::DocumentRef {
                document: DocumentId::new(),
                output: "video".into(),
                extensions: Default::default(),
            },
            version: 1,
            enabled: true,
            values: Default::default(),
            animation: Default::default(),
            extensions: Default::default(),
        },
    };
    let id = transition.id;
    let definition_id = transition.effect.definition.document;
    let mut definition = sequence.clone();
    definition.clips.clear();
    definition.transitions.clear();
    sequence.clips.push(incoming);
    sequence.transitions = vec![transition];
    h.project
        .commit(EditBatch {
            base: h.project.snapshot().revision(),
            mutations: vec![
                Mutation::PutDocument(definition.document(definition_id).unwrap()),
                Mutation::PutDocument(sequence.document(h.document).unwrap()),
            ],
        })
        .unwrap();
    h.tick([0., 0.], None);
    h.tick([0., 0.], None);
    let row = h.layout.unwrap().row_rect(1, h.canvas.view.scroll_y);
    let point = [h.point(48., 1)[0], row.max[1] - 10.];
    h.tick(point, Some(true));
    assert_eq!(h.selected, vec![id]);
    assert!(h.canvas.transition_drag.is_some());
    assert!(h.canvas.drag.is_none());
    h.tick([point[0] + 18., point[1]], None);
    assert_eq!(h.commits, 0);
    h.tick([point[0] + 18., point[1]], Some(false));
    assert_eq!(h.commits, 1);
    assert_eq!(h.sequence().transitions[0].start, Time::new(7, 4).unwrap());
    assert_eq!(h.sequence().clips[0].start, Time::ZERO);
    h.project.undo().unwrap();
    assert_eq!(h.sequence().transitions[0].start, Time::new(3, 2).unwrap());
    let drag = interaction::TransitionDrag {
        base: h.project.snapshot().revision(),
        original: h.sequence(),
        id,
        handle: Handle::Left,
        grab_frame: 36.,
    };
    let edit = drag.proposal(-10000.).unwrap();
    let clamped = h.sequence().edited(&edit).unwrap();
    assert_eq!(clamped.transitions[0].start, t(1)); // incoming source handle
    assert_eq!(
        clamped.transitions[0].end().unwrap(),
        Time::new(5, 2).unwrap()
    );
    let drag = interaction::TransitionDrag {
        handle: Handle::Right,
        ..drag
    };
    let extended = h
        .sequence()
        .edited(&drag.proposal(10000.).unwrap())
        .unwrap();
    assert_eq!(extended.transitions[0].end().unwrap(), t(4));
}

#[test]
fn clip_context_menu_creates_composition_from_the_clicked_video() {
    let mut h = Harness::new();
    h.tick([0., 0.], None);
    h.tick([0., 0.], None);
    let point = h.point(12., 1);
    h.tick(point, None);
    h.context
        .io_mut()
        .add_mouse_button_event(MouseButton::Right, true);
    h.tick(point, None);
    h.context
        .io_mut()
        .add_mouse_button_event(MouseButton::Right, false);
    h.tick(point, None);
    assert_eq!(h.canvas.context_clip, Some(h.sequence().clips[0].id));
    // New composition is the first action for a footage clip.
    let item = [point[0] + 55., point[1] + 12.];
    h.tick(item, None);
    h.tick(item, Some(true));
    h.tick(item, Some(false));
    assert_eq!(h.compositions.len(), 1);
    assert!(h.compositions[0].contains(&h.sequence().clips[0].id));
    assert_eq!(h.commits, 0);
}
