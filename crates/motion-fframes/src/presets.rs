//! Bridge the curated upstream components into Fold's immutable script inputs.
//! A frame owns its SVG strings; no component state or borrowed config escapes.
use crate::Context;
use fframes::{Frame, Svgr};
use fframes_components::{Component, ComponentFrame, Fonts, RenderCtx, components::*};
use std::sync::OnceLock;

pub fn render(preset: &str, input: Context<'_>) -> Result<Svgr<'static>, String> {
    // Bound public inputs at evaluation too: agents and expressions need the same
    // protection from excessive geometry as the inspector's numeric controls.
    static SCHEMAS: OnceLock<Vec<fold_motion_canvas::model::Canvas>> = OnceLock::new();
    let index = fold_motion_canvas::presets::CATALOG
        .iter()
        .position(|p| p.0 == preset)
        .ok_or("Unknown component preset")?;
    let schema = &SCHEMAS.get_or_init(|| {
        fold_motion_canvas::presets::CATALOG
            .iter()
            .map(|p| fold_motion_canvas::presets::canvas(p.0).expect("bundled preset"))
            .collect()
    })[index];
    let fold_motion_canvas::model::Content::Script { controls, .. } =
        &schema.objects[&schema.roots[0]].content
    else {
        unreachable!()
    };
    let number = |name: &str| -> Result<f32, String> {
        let value = input.number(name)?;
        let definition = controls
            .values()
            .find(|c| c.name == name)
            .ok_or("Unknown preset input")?;
        let range = definition.extensions["fold.control"]["range"]
            .as_array()
            .ok_or("Missing preset range")?;
        if !value.is_finite() {
            return Err(format!("{name} must be finite"));
        }
        Ok(value.clamp(range[0].as_f64().unwrap(), range[1].as_f64().unwrap()) as f32)
    };
    let text = |name: &str| -> Result<String, String> {
        let value = input.text(name)?;
        if value.chars().count() > 512 {
            return Err(format!("{name} exceeds 512 characters"));
        }
        Ok(value.to_owned())
    };
    let lines = |name: &str| -> Result<Vec<String>, String> {
        let values: Vec<_> = text(name)?
            .lines()
            .filter(|s| !s.trim().is_empty())
            .map(str::to_owned)
            .collect();
        if values.is_empty() || values.len() > 16 {
            return Err(format!("{name} needs 1–16 nonempty lines"));
        }
        Ok(values)
    };
    let color = |name: &str| -> Result<String, String> {
        let [r, g, b, a] = input.color(name)?;
        Ok(format!(
            "rgba({},{},{},{a})",
            (r * 255.).round() as u8,
            (g * 255.).round() as u8,
            (b * 255.).round() as u8
        ))
    };
    let choice_error = || "Unknown preset choice".to_string();
    let component: Box<dyn Component> = match preset {
        "cinematic-text-swap" => {
            use showcase::text_swap::*;
            let transition = match input.text("Transition")? {
                "Fly through" => TextSwapTransition::FlyThrough,
                "Fade through" => TextSwapTransition::FadeThrough,
                "Crossfade" => TextSwapTransition::Crossfade,
                "Shared axis Y" => TextSwapTransition::SharedAxisY,
                "Shared axis Z" => TextSwapTransition::SharedAxisZ,
                "Cut" => TextSwapTransition::Cut,
                _ => return Err(choice_error()),
            };
            let unit = match input.text("Unit")? {
                "Block" => TextSwapUnit::Block,
                "Words" => TextSwapUnit::Word,
                _ => return Err(choice_error()),
            };
            Box::new(TextSwap::new(TextSwapConfig {
                from_text: text("From")?,
                to_text: text("To")?,
                transition,
                unit,
                font_size: number("Size")?,
                color: Some(color("Color")?),
                enter_duration: number("Enter frames")?,
                exit_duration: number("Exit frames")?,
                enter_stagger: number("Stagger")?,
                exit_stagger: number("Stagger")?,
                ..TextSwapConfig::for_transition(transition)
            }))
        }
        "soft-blur-in" => Box::new(soft_blur_in::SoftBlurIn::new(
            soft_blur_in::SoftBlurInConfig {
                text: text("Text")?,
                font_size: number("Size")?,
                color: color("Color")?,
                blur_from: number("Blur")?,
                stagger: number("Stagger")? as usize,
                char_duration_frames: number("Reveal frames")? as usize,
                ..Default::default()
            },
        )),
        "tracking-in" => Box::new(tracking_in::TrackingIn::new(
            tracking_in::TrackingInConfig {
                text: text("Text")?,
                font_size: number("Size")?,
                color: color("Color")?,
                start_tracking: number("Start tracking")?,
                end_tracking: number("End tracking")?,
                damping: number("Damping")?,
                stiffness: number("Stiffness")?,
                ..Default::default()
            },
        )),
        "word-gather" => Box::new(showcase::word_gather::WordGather::new(
            showcase::word_gather::WordGatherConfig {
                text: text("Text")?,
                max_width: number("Line width")?,
                accent: Some(color("Accent")?),
                mode: fframes_components::ui::Mode::Dark,
                ..Default::default()
            },
        )),
        "word-flip" => Box::new(word_flip::WordFlip::new(word_flip::WordFlipConfig {
            prefix: text("Prefix")?,
            words: lines("Words")?,
            suffix: text("Suffix")?,
            font_size: number("Size")?,
            color: color("Color")?,
            accent_color: color("Accent")?,
            cycle: number("Cycle frames")?,
            overlap: number("Overlap")?,
            ..Default::default()
        })),
        "lines-to-text" => Box::new(lines_to_text::LinesToText::new(
            lines_to_text::LinesToTextConfig {
                lines: lines("Lines")?,
                bars_per_line: number("Bars per line")? as usize,
                drift_frames: number("Drift frames")? as usize,
                converge_duration: number("Converge frames")? as usize,
                resolve_duration: number("Resolve frames")? as usize,
                hold_frames: number("Hold frames")? as usize,
                exit_duration: number("Exit frames")? as usize,
                ..Default::default()
            },
        )),
        "echo-stack" => {
            let direction = match input.text("Direction")? {
                "Up" => echo_stack::EchoDirection::Up,
                "Down" => echo_stack::EchoDirection::Down,
                _ => return Err(choice_error()),
            };
            Box::new(echo_stack::EchoStack::new(echo_stack::EchoStackConfig {
                text: text("Text")?,
                font_size: number("Size")?,
                color: color("Color")?,
                echo_color: color("Echo color")?,
                echoes: number("Echoes")? as usize,
                echo_opacity: number("Echo opacity")?,
                stroke_width: number("Outline")?,
                direction,
                cycle: number("Cycle frames")?,
                ..Default::default()
            }))
        }
        "glitch-cycle" => {
            let mut config = glitch_cycle::GlitchCycleConfig {
                texts: lines("Words")?,
                font_size: number("Size")?,
                color: color("Color")?,
                item_duration: number("Item frames")? as usize,
                delay: number("Delay frames")? as usize,
                ..Default::default()
            };
            for amount in &mut config.glitch {
                *amount *= number("Glitch amount")?;
            }
            Box::new(glitch_cycle::GlitchCycle::new(config))
        }
        "text-highlight" => {
            use showcase::text_highlight::*;
            let preset = match input.text("Style")? {
                "Logo wipe" => TextHighlightPreset::LogoWipe,
                "Marker" => TextHighlightPreset::Marker,
                "Color" => TextHighlightPreset::Color,
                "Underline" => TextHighlightPreset::Underline,
                "Strikethrough" => TextHighlightPreset::Strikethrough,
                "Shimmer" => TextHighlightPreset::Shimmer,
                _ => return Err(choice_error()),
            };
            Box::new(TextHighlight::new(TextHighlightConfig {
                before: text("Before")?,
                highlight: text("Highlight")?,
                after: text("After")?,
                replace_with: Some(text("Replacement")?),
                preset,
                font_size: number("Size")?,
                base_color: Some(color("Color")?),
                highlighted_text_color: Some(color("Color")?),
                accent_color: Some(color("Accent")?),
                cover_color: Some(color("Accent")?),
                draw_duration: number("Draw frames")?,
                thickness: Some(number("Thickness")?),
                ..Default::default()
            }))
        }
        "logo-assemble" => Box::new(showcase::logo_assemble::LogoAssemble::new(
            showcase::logo_assemble::LogoAssembleConfig {
                brand_name: text("Brand")?,
                logo_src: input.image("Logo")?,
                middle_text: text("Middle text")?,
                images: (1..=8)
                    .map(|i| input.image(&format!("Image {i}")))
                    .collect::<Result<_, _>>()?,
                count: number("Cards")?,
                background: Some(color("Background")?),
                ..Default::default()
            },
        )),
        _ => return Err("Unknown component preset".into()),
    };
    let local = (input.seconds() - number("Start")? as f64) * 30. * number("Speed")? as f64;
    if local < 0. {
        return Ok(Svgr::empty());
    }
    // The upstream choreography is authored at 30 fps. Sample exact elapsed time
    // in those units, independent of the containing document's output rate.
    let mut frame = Frame::new(local as usize, local as usize, 30);
    static METERS: OnceLock<fframes_components::TextMeters> = OnceLock::new();
    let mut ctx = RenderCtx::new(
        &mut frame,
        Fonts::default(),
        METERS.get_or_init(fframes_components::embedded_meters),
        ComponentFrame {
            local: local as f32,
            global: local as usize,
            fps: 30.,
            duration: component.duration(),
            width: input.canvas.width as f32,
            height: input.canvas.height as f32,
        },
    );
    Ok(owned(
        component.show(&mut ctx),
        &input.object.transform.x.id.to_string(),
    ))
}

fn owned(svg: Svgr<'_>, namespace: &str) -> Svgr<'static> {
    use fframes::usvgr::svgtree::{
        Attribute, NestedNodeData, NestedNodeKind as K, NestedSvgDocument, SvgAttributeValue as V,
        roxmltree::StringStorage as S,
    };
    fn string(s: S<'_>) -> S<'static> {
        match s {
            S::Owned(s) => S::Owned(s),
            S::Borrowed(s) => S::new_owned(s.to_owned()),
        }
    }
    fn node(n: NestedNodeData<'_>, namespace: &str) -> NestedNodeData<'static> {
        NestedNodeData {
            kind: match n.kind {
                K::Root => K::Root,
                K::Element { tag_name } => K::Element { tag_name },
                K::Text(s) => K::Text(string(s)),
            },
            attrs: n
                .attrs
                .into_vec()
                .into_iter()
                .map(|a| Attribute {
                    name: a.name,
                    value: match a.value {
                        V::Float(f, s) => V::Float(f, string(s)),
                        V::StringStorage(s) => {
                            // Clip paths, masks and gradients use local SVG IDs. Scope
                            // those IDs per placed object, including URL references.
                            let value: &str = s.as_ref();
                            let value = if a.name == fframes::usvgr::svgtree::AId::Id {
                                format!("{namespace}-{value}")
                            } else if value.starts_with("url(#") {
                                value.replace("url(#", &format!("url(#{namespace}-"))
                            } else if a.name == fframes::usvgr::svgtree::AId::Href
                                && value.starts_with('#')
                            {
                                format!("#{namespace}-{}", &value[1..])
                            } else {
                                return Attribute {
                                    name: a.name,
                                    value: V::StringStorage(string(s)),
                                };
                            };
                            V::StringStorage(S::new_owned(value))
                        }
                        V::Length(v) => V::Length(v),
                        V::Transform(v) => V::Transform(v),
                        V::Color(v) => V::Color(v),
                        V::ImageData(v) => V::ImageData(v),
                        V::PathData(v) => V::PathData(std::borrow::Cow::Owned(v.into_owned())),
                        V::StaticPath(v) => V::StaticPath(v),
                    },
                })
                .collect(),
            children: n
                .children
                .into_iter()
                .map(|n| n.map(|n| node(n, namespace)))
                .collect(),
            static_hash: None,
        }
    }
    Svgr {
        svg_tree: NestedSvgDocument::from_nodes(
            svg.svg_tree
                .nodes
                .into_iter()
                .map(|n| n.map(|n| node(n, namespace)))
                .collect(),
        ),
    }
}
