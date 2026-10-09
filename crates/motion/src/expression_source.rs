//! Bounded textual projection of the common expression IR.
//! UUID annotations belong to native edges; source byte ranges are derived metadata.
use crate::{
    expression::{Binary, ClockScope, Element, Expr, Function, Unary},
    model::{Edge, Reference},
    types::{Literal, Unit},
};
use fold_foundation::ObjectId;
use fold_project::Metadata;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const MAX_SOURCE_BYTES: usize = 64 * 1024;
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Code {
    Syntax,
    Limit,
    Reference,
    Identity,
    Value,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diagnostic {
    pub code: Code,
    pub message: String,
    pub span: Span,
    pub native_id: Option<ObjectId>,
    /// Native validation cause, when parsing reached structural validation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub validation: Option<crate::validation::Diagnostic>,
}
pub struct Parsed {
    pub expression: Expr,
    pub edges: BTreeMap<ObjectId, Span>,
}
/// Reference names are resolved within the caller's native definition scope.
pub fn parse(
    source: &str,
    resolve: &mut impl FnMut(&str) -> Result<Reference, String>,
) -> Result<Parsed, Diagnostic> {
    let (parsed, consumed) = parse_prefix(source, resolve)?;
    if consumed != source.len() {
        return Err(Diagnostic {
            code: Code::Syntax,
            message: "Unexpected trailing source".into(),
            span: Span {
                start: consumed,
                end: consumed,
            },
            native_id: None,
            validation: None,
        });
    }
    Ok(parsed)
}
/// Parse one expression within a larger native declaration, stopping at its delimiter.
pub(crate) fn parse_prefix(
    source: &str,
    resolve: &mut impl FnMut(&str) -> Result<Reference, String>,
) -> Result<(Parsed, usize), Diagnostic> {
    if source.len() > MAX_SOURCE_BYTES {
        return Err(Diagnostic {
            code: Code::Limit,
            message: "Source exceeds 64 KiB".into(),
            span: Span {
                start: 0,
                end: source.len(),
            },
            native_id: None,
            validation: None,
        });
    }
    let mut parser = Parser {
        source,
        position: 0,
        terms: 0,
        edges: BTreeMap::new(),
        resolve,
    };
    let expression = parser.expression(0, 0)?;
    parser.whitespace();
    expression
        .references()
        .map_err(|message| parser.error(Code::Value, &message))?;
    Ok((
        Parsed {
            expression,
            edges: parser.edges,
        },
        parser.position,
    ))
}

/// Print the complete expression, including UUIDs and extension metadata.
/// Parentheses preserve the exact native tree, not just equivalent arithmetic.
pub fn canonical(
    expression: &Expr,
    name: &mut impl FnMut(Reference) -> Result<String, String>,
) -> Result<String, String> {
    expression.references()?;
    let result = print(expression, name)?;
    if result.len() > MAX_SOURCE_BYTES {
        return Err("Canonical expression exceeds 64 KiB".into());
    }
    Ok(result)
}
fn print(
    expression: &Expr,
    name: &mut impl FnMut(Reference) -> Result<String, String>,
) -> Result<String, String> {
    let result: Result<String, String> = match expression {
        Expr::Literal { value } => Ok(match value {
            Literal::Bool { value } => value.to_string(),
            Literal::Text { value } => serde_json::to_string(value).map_err(|e| e.to_string())?,
            Literal::Scalar { value } if !value.is_sign_negative() => value.to_string(),
            Literal::Integer { value } if *value >= 0 => format!("{value}i"),
            Literal::Distance { value, unit } if !value.is_sign_negative() => format!(
                "{value}{}",
                match unit {
                    Unit::Canvas => "px",
                    Unit::Meters => "m",
                    Unit::Unitless => return Err("Unitless distance".into()),
                }
            ),
            _ => format!(
                "literal({})",
                serde_json::to_string(value).map_err(|e| e.to_string())?
            ),
        }),
        Expr::Reference { reference } => {
            let mut metadata = reference.extensions.clone();
            metadata.insert(
                "id".into(),
                serde_json::to_value(reference.id).map_err(|e| e.to_string())?,
            );
            Ok(format!(
                "${} @{}",
                serde_json::to_string(&name(reference.target)?).map_err(|e| e.to_string())?,
                serde_json::to_string(&metadata).map_err(|e| e.to_string())?
            ))
        }
        Expr::Time { scope } => Ok(match scope {
            ClockScope::Scene => "time.scene",
            ClockScope::Asset => "time.asset",
        }
        .into()),
        Expr::Element { property } => Ok(match property {
            Element::Index => "element.index".into(),
            Element::Count => "element.count".into(),
            Element::Position { unit, space } => format!(
                "element.position({},{})",
                serde_json::to_string(unit).map_err(|e| e.to_string())?,
                serde_json::to_string(space).map_err(|e| e.to_string())?
            ),
        }),
        Expr::Random { version, stream } => Ok(format!(
            "random({version},{})",
            serde_json::to_string(stream).map_err(|e| e.to_string())?
        )),
        Expr::Unary { op, value } => Ok(format!(
            "({}{})",
            match op {
                Unary::Negate => "-",
                Unary::Not => "!",
            },
            print(value, name)?
        )),
        Expr::Binary { op, left, right } => Ok(format!(
            "({} {} {})",
            print(left, name)?,
            match op {
                Binary::Add => "+",
                Binary::Subtract => "-",
                Binary::Multiply => "*",
                Binary::Divide => "/",
                Binary::Equal => "==",
                Binary::Less => "<",
                Binary::And => "&&",
                Binary::Or => "||",
            },
            print(right, name)?
        )),
        Expr::Call {
            function,
            arguments,
        } => {
            let function = match function {
                Function::Sin => "sin",
                Function::Cos => "cos",
                Function::Abs => "abs",
                Function::Min => "min",
                Function::Max => "max",
                Function::Clamp => "clamp",
                Function::Lerp => "lerp",
                Function::Length => "length",
            };
            let arguments = arguments
                .iter()
                .map(|a| print(a, name))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(format!("{function}({})", arguments.join(", ")))
        }
        Expr::Vector {
            components,
            unit,
            space,
        } => {
            let mut args = vec![
                serde_json::to_string(unit).map_err(|e| e.to_string())?,
                serde_json::to_string(space).map_err(|e| e.to_string())?,
            ];
            args.extend(
                components
                    .iter()
                    .map(|a| print(a, name))
                    .collect::<Result<Vec<_>, _>>()?,
            );
            Ok(format!("vector({})", args.join(", ")))
        }
    };
    let result = result?;
    if result.len() > MAX_SOURCE_BYTES {
        return Err("Canonical expression exceeds 64 KiB".into());
    }
    Ok(result)
}
struct Parser<'a, F> {
    source: &'a str,
    position: usize,
    terms: usize,
    edges: BTreeMap<ObjectId, Span>,
    resolve: &'a mut F,
}
impl<F: FnMut(&str) -> Result<Reference, String>> Parser<'_, F> {
    fn error(&self, code: Code, message: &str) -> Diagnostic {
        Diagnostic {
            code,
            message: message.into(),
            span: Span {
                start: self.position,
                end: self.position
                    + self.source[self.position..]
                        .chars()
                        .next()
                        .map_or(0, char::len_utf8),
            },
            native_id: None,
            validation: None,
        }
    }
    fn whitespace(&mut self) {
        loop {
            while self
                .source
                .as_bytes()
                .get(self.position)
                .is_some_and(u8::is_ascii_whitespace)
            {
                self.position += 1;
            }
            if self.source[self.position..].starts_with("//") {
                self.position = self.source[self.position..]
                    .find('\n')
                    .map_or(self.source.len(), |n| self.position + n);
            } else {
                break;
            }
        }
    }
    fn take(&mut self, text: &str) -> bool {
        self.whitespace();
        if self.source[self.position..].starts_with(text) {
            self.position += text.len();
            true
        } else {
            false
        }
    }
    fn expect(&mut self, text: &str) -> Result<(), Diagnostic> {
        if self.take(text) {
            Ok(())
        } else {
            Err(self.error(Code::Syntax, &format!("Expected {text}")))
        }
    }
    fn json<T: serde::de::DeserializeOwned>(&mut self) -> Result<T, Diagnostic> {
        self.whitespace();
        let mut stream =
            serde_json::Deserializer::from_str(&self.source[self.position..]).into_iter::<T>();
        let value = stream
            .next()
            .ok_or_else(|| self.error(Code::Syntax, "Expected a value"))?
            .map_err(|e| self.error(Code::Syntax, &e.to_string()))?;
        self.position += stream.byte_offset();
        Ok(value)
    }
    fn word(&mut self) -> Result<String, Diagnostic> {
        self.whitespace();
        let start = self.position;
        while self
            .source
            .as_bytes()
            .get(self.position)
            .is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'_' || *b == b'.')
        {
            self.position += 1;
        }
        if start == self.position {
            return Err(self.error(Code::Syntax, "Expected a name"));
        }
        Ok(self.source[start..self.position].into())
    }
    fn expression(&mut self, minimum: u8, depth: usize) -> Result<Expr, Diagnostic> {
        self.terms += 1;
        if depth > 64 || self.terms > 4096 {
            return Err(self.error(Code::Limit, "Expression exceeds 4096 terms or depth 64"));
        }
        let mut left = self.atom(depth)?;
        loop {
            self.whitespace();
            let operator = [
                ("||", 1, Binary::Or),
                ("&&", 2, Binary::And),
                ("==", 3, Binary::Equal),
                ("<", 4, Binary::Less),
                ("+", 5, Binary::Add),
                ("-", 5, Binary::Subtract),
                ("*", 6, Binary::Multiply),
                ("/", 6, Binary::Divide),
            ]
            .into_iter()
            .find(|(text, precedence, _)| {
                *precedence >= minimum && self.source[self.position..].starts_with(text)
            });
            let Some((text, precedence, op)) = operator else {
                break;
            };
            self.position += text.len();
            let right = self.expression(precedence + 1, depth + 1)?;
            left = Expr::Binary {
                op,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        Ok(left)
    }
    fn atom(&mut self, depth: usize) -> Result<Expr, Diagnostic> {
        self.whitespace();
        let start = self.position;
        if self.take("(") {
            let expression = self.expression(0, depth + 1)?;
            self.expect(")")?;
            return Ok(expression);
        }
        for (token, op) in [("-", Unary::Negate), ("!", Unary::Not)] {
            if self.take(token) {
                return Ok(Expr::Unary {
                    op,
                    value: Box::new(self.expression(7, depth + 1)?),
                });
            }
        }
        if self.take("$") {
            let name = if self.source[self.position..].trim_start().starts_with('"') {
                self.json::<String>()?
            } else {
                self.word()?
            };
            return self.reference(start, name);
        }
        if self.source[self.position..].starts_with('"') {
            return Ok(Expr::Literal {
                value: Literal::Text {
                    value: self.json()?,
                },
            });
        }
        if self
            .source
            .as_bytes()
            .get(self.position)
            .is_some_and(u8::is_ascii_digit)
        {
            return self.number();
        }
        let word = self.word()?;
        match word.as_str() {
            "true" | "false" => Ok(Expr::Literal {
                value: Literal::Bool {
                    value: word == "true",
                },
            }),
            "time.scene" => Ok(Expr::Time {
                scope: ClockScope::Scene,
            }),
            "time.asset" => Ok(Expr::Time {
                scope: ClockScope::Asset,
            }),
            "element.index" => Ok(Expr::Element {
                property: Element::Index,
            }),
            "element.count" => Ok(Expr::Element {
                property: Element::Count,
            }),
            "element.position" => {
                self.expect("(")?;
                let unit = self.json()?;
                self.expect(",")?;
                let space = self.json()?;
                self.expect(")")?;
                Ok(Expr::Element {
                    property: Element::Position { unit, space },
                })
            }
            "literal" => {
                self.expect("(")?;
                let value = self.json()?;
                self.expect(")")?;
                Ok(Expr::Literal { value })
            }
            "random" => {
                self.expect("(")?;
                let version = self.json()?;
                self.expect(",")?;
                let stream = self.json()?;
                self.expect(")")?;
                Ok(Expr::Random { version, stream })
            }
            "vector" => {
                self.expect("(")?;
                let unit = self.json()?;
                self.expect(",")?;
                let space = self.json()?;
                self.expect(",")?;
                let components = self.arguments(depth)?;
                Ok(Expr::Vector {
                    components,
                    unit,
                    space,
                })
            }
            "sin" | "cos" | "abs" | "min" | "max" | "clamp" | "lerp" | "length" => {
                let function = match word.as_str() {
                    "sin" => Function::Sin,
                    "cos" => Function::Cos,
                    "abs" => Function::Abs,
                    "min" => Function::Min,
                    "max" => Function::Max,
                    "clamp" => Function::Clamp,
                    "lerp" => Function::Lerp,
                    _ => Function::Length,
                };
                self.expect("(")?;
                let arguments = self.arguments(depth)?;
                Ok(Expr::Call {
                    function,
                    arguments,
                })
            }
            _ => self.reference(start, word),
        }
    }
    fn arguments(&mut self, depth: usize) -> Result<Vec<Expr>, Diagnostic> {
        let mut arguments = Vec::new();
        if self.take(")") {
            return Ok(arguments);
        }
        loop {
            if arguments.len() == 3 {
                return Err(self.error(Code::Limit, "At most three expression arguments"));
            }
            arguments.push(self.expression(0, depth + 1)?);
            if self.take(")") {
                return Ok(arguments);
            }
            self.expect(",")?;
        }
    }
    fn reference(&mut self, start: usize, name: String) -> Result<Expr, Diagnostic> {
        let target =
            (self.resolve)(&name).map_err(|message| self.error(Code::Reference, &message))?;
        let mut metadata: Metadata = if self.take("@") {
            self.json()?
        } else {
            Metadata::new()
        };
        let id = match metadata.remove("id") {
            Some(value) => serde_json::from_value(value)
                .map_err(|e| self.error(Code::Identity, &e.to_string()))?,
            None => ObjectId::new(),
        };
        let reference = Edge {
            id,
            target,
            extensions: metadata,
        };
        reference
            .validate()
            .map_err(|message| self.error(Code::Identity, &message))?;
        let span = Span {
            start,
            end: self.position,
        };
        if self.edges.insert(id, span).is_some() {
            return Err(Diagnostic {
                code: Code::Identity,
                message: "Duplicate connection identity".into(),
                span,
                native_id: Some(id),
                validation: None,
            });
        }
        Ok(Expr::Reference { reference })
    }
    fn number(&mut self) -> Result<Expr, Diagnostic> {
        let start = self.position;
        while self
            .source
            .as_bytes()
            .get(self.position)
            .is_some_and(u8::is_ascii_digit)
        {
            self.position += 1;
        }
        if self.source.as_bytes().get(self.position) == Some(&b'.') {
            self.position += 1;
            while self
                .source
                .as_bytes()
                .get(self.position)
                .is_some_and(u8::is_ascii_digit)
            {
                self.position += 1;
            }
        }
        if matches!(self.source.as_bytes().get(self.position), Some(b'e' | b'E')) {
            self.position += 1;
            if matches!(self.source.as_bytes().get(self.position), Some(b'+' | b'-')) {
                self.position += 1;
            }
            while self
                .source
                .as_bytes()
                .get(self.position)
                .is_some_and(u8::is_ascii_digit)
            {
                self.position += 1;
            }
        }
        let number = &self.source[start..self.position];
        let suffix_start = self.position;
        while self
            .source
            .as_bytes()
            .get(self.position)
            .is_some_and(u8::is_ascii_alphabetic)
        {
            self.position += 1;
        }
        let suffix = &self.source[suffix_start..self.position];
        let value = if suffix == "i" {
            Literal::Integer {
                value: number
                    .parse()
                    .map_err(|_| self.error(Code::Value, "Invalid integer"))?,
            }
        } else {
            let value = number
                .parse()
                .map_err(|_| self.error(Code::Value, "Invalid number"))?;
            match suffix {
                "" => Literal::Scalar { value },
                "px" => Literal::Distance {
                    value,
                    unit: Unit::Canvas,
                },
                "m" => Literal::Distance {
                    value,
                    unit: Unit::Meters,
                },
                _ => return Err(self.error(Code::Value, "Unknown numeric unit")),
            }
        };
        value
            .validate()
            .map_err(|message| self.error(Code::Value, &message))?;
        Ok(Expr::Literal { value })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        execution::{Budget, Context},
        types::{Space, Type},
    };
    use fold_foundation::Time;
    fn no_reference(_: &str) -> Result<Reference, String> {
        Err("Unknown name".into())
    }
    #[test]
    fn precedence_units_and_integer_identity_round_trip() {
        let input = ObjectId::new();
        let parsed = parse(
            "// width in canvas units\n size * (2 + 3) - 4px",
            &mut |name| {
                if name == "size" {
                    Ok(Reference::Input { input })
                } else {
                    Err("Unknown name".into())
                }
            },
        )
        .unwrap();
        assert_eq!(
            parsed
                .expression
                .infer(&mut |_| Ok(Type::Distance { unit: Unit::Canvas }))
                .unwrap(),
            Type::Distance { unit: Unit::Canvas }
        );
        let context = Context {
            scene_time: Time::ZERO,
            asset_time: Time::ZERO,
            seed: 1,
            operation: [0; 16],
            element: None,
        };
        assert_eq!(
            parsed
                .expression
                .evaluate(&context, &mut Budget::new(100, &|| Ok(())), &mut |_| Ok(
                    Literal::Distance {
                        value: 10.,
                        unit: Unit::Canvas
                    }
                ))
                .unwrap(),
            Literal::Distance {
                value: 46.,
                unit: Unit::Canvas
            }
        );
        let canonical = canonical(&parsed.expression, &mut |_| Ok("renamed size".into())).unwrap();
        assert!(canonical.contains("renamed size"));
        assert_eq!(
            parse(&canonical, &mut |_| Ok(Reference::Input { input }))
                .unwrap()
                .expression,
            parsed.expression
        );
        assert_eq!(parsed.edges.len(), 1);
        let span = parsed.edges.values().next().unwrap();
        assert_eq!(
            &"// width in canvas units\n size * (2 + 3) - 4px"[span.start..span.end].trim(),
            &"size"
        );
        let fresh = parse("size", &mut |_| Ok(Reference::Input { input })).unwrap();
        assert_ne!(fresh.edges.keys().next(), parsed.edges.keys().next());
    }
    #[test]
    fn every_expression_form_and_literal_escape_has_a_canonical_projection() {
        for source in [
            "-3i + 4i * 2i",
            "!(true && false) || 2 < 3",
            "3 == 3",
            "min(1, 2)",
            "max(1, 2)",
            "clamp(abs(-2), 0, 1)",
            "lerp(0, 1, 0.5)",
            "sin(1) / cos(2)",
            "length(vector(\"canvas\", \"screen\", 2px, 3px))",
            "time.scene",
            "time.asset",
            "element.index",
            "element.count",
            "element.position(\"meters\",\"world\")",
            "random(1,\"jitter\")",
            "\"λ title \\nline\"",
            "literal({\"kind\":\"duration\",\"value\":{\"numerator\":1,\"denominator\":3}})",
        ] {
            let first = parse(source, &mut no_reference).unwrap();
            let printed =
                canonical(&first.expression, &mut |_| Err("No references".into())).unwrap();
            let second = parse(&printed, &mut no_reference).unwrap();
            assert_eq!(first.expression, second.expression, "{source}");
        }
        for literal in [
            Literal::Scalar { value: -0.0 },
            Literal::Integer { value: i64::MIN },
            Literal::Vector {
                value: vec![1., 2.],
                unit: Unit::Canvas,
                space: Space::Screen,
            },
            Literal::euler([0.1, 0.2, 0.3], crate::types::EulerOrder::Zyx, Space::World).unwrap(),
        ] {
            let expression = Expr::Literal { value: literal };
            let source = canonical(&expression, &mut |_| unreachable!()).unwrap();
            assert_eq!(
                parse(&source, &mut no_reference).unwrap().expression,
                expression
            );
        }
    }
    #[test]
    fn edge_extensions_survive_and_invalid_annotations_do_not_alias_objects() {
        let input = ObjectId::new();
        let mut edge = Edge::from(Reference::Input { input });
        edge.extensions
            .insert("artist_note".into(), serde_json::json!({"text":"keep λ"}));
        let expression = Expr::Reference {
            reference: edge.clone(),
        };
        let source = canonical(&expression, &mut |_| Ok("size".into())).unwrap();
        assert_eq!(
            parse(&source, &mut |_| Ok(edge.target)).unwrap().expression,
            expression
        );
        let repeated = format!("{source} + {source}");
        assert_eq!(
            parse(&repeated, &mut |_| Ok(edge.target))
                .err()
                .unwrap()
                .code,
            Code::Identity
        );
        assert_eq!(
            parse("missing", &mut no_reference).err().unwrap().code,
            Code::Reference
        );
        assert!(parse("size @{\"target\":{}}", &mut |_| Ok(edge.target)).is_err());
        assert!(parse("size @{\"id\":\"bad-uuid\"}", &mut |_| Ok(edge.target)).is_err());
    }
    #[test]
    fn source_limits_syntax_and_nonfinite_literals_fail_with_byte_ranges() {
        for source in [
            "1 +",
            "min(1,2,3,4)",
            "1e999",
            "3cm",
            "random(2,\"stream\")",
            "1; import filesystem",
            "vector(\"canvas\",\"screen\",1px)",
        ] {
            let error = parse(source, &mut no_reference).err().unwrap();
            assert!(error.span.start <= error.span.end && error.span.end <= source.len());
        }
        let deep = format!("{}1{}", "(".repeat(100), ")".repeat(100));
        assert_eq!(
            parse(&deep, &mut no_reference).err().unwrap().code,
            Code::Limit
        );
        assert_eq!(
            parse(&" ".repeat(MAX_SOURCE_BYTES + 1), &mut no_reference)
                .err()
                .unwrap()
                .code,
            Code::Limit
        );
    }
}
