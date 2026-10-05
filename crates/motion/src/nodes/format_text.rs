//! Bounded number formatting that remains a field until a consumer selects a domain.
use super::*;
use crate::{evaluation::field, fields::Field};

enum Part {
    Literal(String),
    Number(usize),
}

fn parse(template: &str) -> Result<Vec<Part>, String> {
    let mut rest = template;
    let mut parts = Vec::new();
    let mut literal = String::new();
    while !rest.is_empty() {
        if rest.starts_with("{{") || rest.starts_with("}}") {
            literal.push(rest.as_bytes()[0] as char);
            rest = &rest[2..];
        } else if rest.starts_with('{') {
            let end = rest.find('}').ok_or("unclosed text placeholder")?;
            let index = match &rest[1..end] {
                "value" => 0,
                "x" => 1,
                "y" => 2,
                _ => return Err("text placeholders are {value}, {x}, and {y}".into()),
            };
            parts.push(Part::Literal(std::mem::take(&mut literal)));
            parts.push(Part::Number(index));
            rest = &rest[end + 1..];
        } else if rest.starts_with('}') {
            return Err("unmatched text brace; use }} for a literal brace".into());
        } else {
            let ch = rest.chars().next().unwrap();
            literal.push(ch);
            rest = &rest[ch.len_utf8()..];
        }
    }
    parts.push(Part::Literal(literal));
    Ok(parts)
}

pub fn definition_format() -> Definition {
    let mut def = definition(
        "fold.motion.format_text",
        "Format Text",
        "Values",
        vec![
            Socket::value("template", Datum::Text("{value}".into()), ""),
            scalar("value", 0., ""),
            scalar("x", 0., ""),
            scalar("y", 0., ""),
            scalar("decimals", 0., "integer, 0–6"),
        ],
        vec![("value", Kind::Text)],
        |e, n| {
            let template = e.uniform(n, "template")?;
            let parts = parse(template.text()?)?;
            let decimals = e.scalar(n, "decimals")?;
            if !(0. ..=6.).contains(&decimals) || decimals.fract() != 0. {
                return Err("decimal places must be an integer in 0..=6".into());
            }
            let values = [e.field(n, "value")?, e.field(n, "x")?, e.field(n, "y")?];
            let varying = parts
                .iter()
                .any(|part| matches!(part, Part::Number(index) if values[*index].varying));
            Ok(field(Field::new(Kind::Text, varying, move |s, budget| {
                let mut result = String::new();
                for part in &parts {
                    budget.spend()?;
                    match part {
                        Part::Literal(text) => result.push_str(text),
                        Part::Number(index) => {
                            let value = values[*index].sample(s, budget)?.scalar()?;
                            let text =
                                format!("{value:.precision$}", precision = decimals as usize);
                            // Rounding a small negative coordinate should display zero.
                            let text = if text.starts_with('-') && text.parse::<f64>() == Ok(0.) {
                                &text[1..]
                            } else {
                                &text
                            };
                            result.push_str(text);
                        }
                    }
                    if result.len() > 65536 {
                        return Err("formatted text exceeds 65536 bytes".into());
                    }
                }
                Ok(Datum::Text(result))
            })))
        },
    );
    def.description = "Format scalar fields with {value}, {x}, {y}; escape braces with {{ and }}. Decimal places: integer 0–6. Use Text on Points to consume per-copy fields.";
    def
}
