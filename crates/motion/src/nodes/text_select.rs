//! Select an editable sequence entry without baking text into frame-specific graphs.
use super::*;
use crate::{evaluation::field, fields::Field};
pub fn definition_select() -> Definition {
    let mut d = definition(
        "fold.motion.text_select",
        "Text Select",
        "Values",
        vec![
            Socket::value("text", Datum::Text("One\nTwo\nThree".into()), ""),
            scalar("index", 0., "entry; floor"),
            boolean("characters", false),
            boolean("loop", true),
        ],
        vec![("value", Kind::Text)],
        |e, n| {
            let text = e.uniform(n, "text")?.text()?.to_owned();
            let characters = e.uniform(n, "characters")?.boolean()?;
            let looping = e.uniform(n, "loop")?.boolean()?;
            let entries: Vec<String> = if characters {
                text.chars().take(16385).map(|c| c.to_string()).collect()
            } else {
                text.split('\n').take(16385).map(str::to_owned).collect()
            };
            if entries.len() > 16384 {
                return Err("Text Select exceeds 16384 entries".into());
            }
            let index = e.field(n, "index")?;
            Ok(field(Field::new(
                Kind::Text,
                index.varying,
                move |sample, budget| {
                    let index = index.sample(sample, budget)?.scalar()?;
                    if !index.is_finite() || index.abs() > 1e12 {
                        return Err("Text Select index must be finite and within +/-1e12".into());
                    }
                    if entries.is_empty() {
                        return Ok(Datum::Text(String::new()));
                    }
                    let index = if looping {
                        (index.floor() as i64).rem_euclid(entries.len() as i64)
                    } else {
                        (index.floor() as i64).clamp(0, entries.len() as i64 - 1)
                    };
                    Ok(Datum::Text(entries[index as usize].clone()))
                },
            )))
        },
    );
    d.description = "Select a newline-delimited text entry using a numeric field. characters=true instead selects Unicode scalar characters (not shaped glyphs or grapheme clusters). Index is floored, then wrapped when loop=true or clamped otherwise. Connect Time/Expression/Keyframes to index and a published Text control to text; Text shapes the selected entry with its embedded font. Empty entries produce empty text.";
    d
}
