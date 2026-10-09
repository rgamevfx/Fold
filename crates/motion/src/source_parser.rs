//! Two-pass native source parsing: declarations allocate identities before names resolve.
use crate::{
    binding::Binding,
    expression::Expr,
    expression_source::{self, Code, Diagnostic, Span},
    model::{Construction, Definition, Input, Instance, Node, Output, Parameter, Reference},
    registry,
};
use fold_foundation::ObjectId;
use fold_project::Metadata;
use std::collections::BTreeMap;

pub struct Parsed {
    pub construction: Construction,
    pub spans: BTreeMap<ObjectId, Span>,
    /// Unresolved operations remain native records but cannot execute.
    pub unresolved: Vec<ObjectId>,
}
struct PendingReference {
    owner: Option<String>,
    scope: Option<ObjectId>,
    name: String,
    span: Span,
}
struct Parser<'a> {
    owner: Option<String>,
    source: &'a str,
    position: usize,
    spans: BTreeMap<ObjectId, Span>,
    references: BTreeMap<ObjectId, PendingReference>,
    definitions: BTreeMap<ObjectId, String>,
    override_names: BTreeMap<ObjectId, String>,
    unresolved: Vec<ObjectId>,
}
pub struct Fragment {
    pub definition: Definition,
    pub spans: BTreeMap<ObjectId, Span>,
    pub unresolved: Vec<ObjectId>,
}
/// Parse one asset declaration in a retained native context. A missing definition
/// UUID stays fresh; only explicit reconciliation can map it onto the selected asset.
pub fn parse_fragment(
    source: &str,
    context: &Construction,
    target: ObjectId,
) -> Result<Fragment, Diagnostic> {
    let mut parser = Parser::new(source)?;
    context
        .validate_structure_detailed()
        .map_err(|diagnostic| parser.validation_error(diagnostic))?;
    parser.expect("asset")?;
    let definition = parser.asset()?;
    parser.whitespace();
    if parser.position != source.len() {
        return Err(parser.error(Code::Syntax, "Expected exactly one asset declaration"));
    }
    let mut working = context.clone();
    let slot = working
        .definitions
        .iter()
        .position(|d| d.id == target)
        .ok_or_else(|| parser.error(Code::Reference, "Unknown fragment target"))?;
    working.definitions[slot] = definition;
    parser.resolve(&mut working)?;
    let definition = working.definitions.remove(slot);
    crate::reconcile::identities(&definition)
        .map_err(|message| parser.error(Code::Identity, &message))?;
    Ok(Fragment {
        definition,
        spans: parser.spans,
        unresolved: parser.unresolved,
    })
}
/// Parse losslessly, resolve all names, and structurally validate the document.
/// Execution/type qualification is `Construction::validate_semantics`; unresolved
/// native records deliberately survive this persistence/source boundary.
pub fn parse(source: &str) -> Result<Parsed, Diagnostic> {
    let mut parser = Parser::new(source)?;
    parser.expect("motion")?;
    let version: u32 = parser.number()?;
    if version != 1 {
        return Err(parser.error(Code::Value, "Unsupported construction source version"));
    }
    parser.expect(";")?;
    parser.expect("seed")?;
    let seed = parser.number()?;
    parser.expect(";")?;
    parser.expect("canvas")?;
    let canvas = parser.json()?;
    parser.expect(";")?;
    let mut definitions = vec![];
    while parser.take("asset") {
        definitions.push(parser.asset()?);
    }
    parser.expect("root")?;
    let root = parser.instance(None)?;
    parser.expect("presentation")?;
    let presentation_start = parser.position;
    let presentation: crate::model::Presentation = parser.json()?;
    parser.expect(";")?;
    for group in &presentation.groups {
        parser.span(group.id, presentation_start)?;
    }
    parser.expect("extensions")?;
    let extensions = parser.json()?;
    parser.expect(";")?;
    parser.whitespace();
    if parser.position != source.len() {
        return Err(parser.error(Code::Syntax, "Unexpected trailing source"));
    }
    let mut construction = Construction {
        seed,
        canvas,
        definitions,
        root,
        presentation,
        extensions,
    };
    parser.resolve(&mut construction)?;
    construction
        .validate_structure_detailed()
        .map_err(|diagnostic| parser.validation_error(diagnostic))?;
    Ok(Parsed {
        construction,
        spans: parser.spans,
        unresolved: parser.unresolved,
    })
}
impl Parser<'_> {
    fn new(source: &str) -> Result<Parser<'_>, Diagnostic> {
        if source.len() > expression_source::MAX_SOURCE_BYTES {
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
        Ok(Parser {
            owner: None,
            source,
            position: 0,
            spans: BTreeMap::new(),
            references: BTreeMap::new(),
            definitions: BTreeMap::new(),
            override_names: BTreeMap::new(),
            unresolved: vec![],
        })
    }
    fn error(&self, code: Code, message: &str) -> Diagnostic {
        Diagnostic {
            code,
            message: message.into(),
            span: Span {
                start: self.position,
                end: self.position,
            },
            native_id: None,
            validation: None,
        }
    }
    fn validation_error(&self, validation: crate::validation::Diagnostic) -> Diagnostic {
        let mut diagnostic = self.error(Code::Value, &validation.message);
        if let Some((id, span)) = validation
            .native_ids
            .iter()
            .find_map(|id| self.spans.get(id).map(|span| (*id, *span)))
        {
            diagnostic.native_id = Some(id);
            diagnostic.span = span;
        }
        diagnostic.validation = Some(validation);
        diagnostic
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
    fn take(&mut self, token: &str) -> bool {
        self.whitespace();
        let remaining = &self.source[self.position..];
        if !remaining.starts_with(token) {
            return false;
        }
        if token.as_bytes().last().is_some_and(u8::is_ascii_alphabetic)
            && remaining
                .as_bytes()
                .get(token.len())
                .is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'_')
        {
            return false;
        }
        self.position += token.len();
        true
    }
    fn expect(&mut self, token: &str) -> Result<(), Diagnostic> {
        if self.take(token) {
            Ok(())
        } else {
            Err(self.error(Code::Syntax, &format!("Expected {token}")))
        }
    }
    fn number<T: std::str::FromStr>(&mut self) -> Result<T, Diagnostic> {
        self.whitespace();
        let start = self.position;
        while self
            .source
            .as_bytes()
            .get(self.position)
            .is_some_and(u8::is_ascii_digit)
        {
            self.position += 1;
        }
        self.source[start..self.position]
            .parse()
            .map_err(|_| self.error(Code::Value, "Expected an unsigned integer within range"))
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
    fn annotation(&mut self) -> Result<(ObjectId, Metadata), Diagnostic> {
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
        Ok((id, metadata))
    }
    fn protected(&self, metadata: &mut Metadata) -> Result<bool, Diagnostic> {
        metadata
            .remove("protected")
            .map(|value| {
                serde_json::from_value(value).map_err(|e| self.error(Code::Value, &e.to_string()))
            })
            .transpose()
            .map(|v| v.unwrap_or(false))
    }
    fn span(&mut self, id: ObjectId, start: usize) -> Result<(), Diagnostic> {
        let span = Span {
            start,
            end: self.position,
        };
        if self.spans.insert(id, span).is_some() {
            return Err(Diagnostic {
                code: Code::Identity,
                message: "Duplicate native source identity".into(),
                span,
                native_id: Some(id),
                validation: None,
            });
        }
        if self.spans.len() > crate::model::MAX_OBJECTS {
            return Err(self.error(Code::Limit, "Native source object budget exceeded"));
        }
        Ok(())
    }
    fn expression(&mut self, scope: Option<ObjectId>) -> Result<Expr, Diagnostic> {
        self.whitespace();
        let offset = self.position;
        let mut pending = vec![];
        let (parsed, consumed) =
            expression_source::parse_prefix(&self.source[offset..], &mut |name| {
                let id = ObjectId::new();
                pending.push((id, name.to_string()));
                Ok(Reference::Node { node: id })
            })
            .map_err(|mut error| {
                error.span.start += offset;
                error.span.end += offset;
                error
            })?;
        self.position += consumed;
        for (id, name) in pending {
            self.references.insert(
                id,
                PendingReference {
                    owner: self.owner.clone(),
                    scope,
                    name,
                    span: Span {
                        start: offset,
                        end: self.position,
                    },
                },
            );
        }
        for (id, mut span) in parsed.edges {
            span.start += offset;
            span.end += offset;
            if self.spans.insert(id, span).is_some() {
                return Err(Diagnostic {
                    code: Code::Identity,
                    message: "Duplicate connection identity".into(),
                    span,
                    native_id: Some(id),
                    validation: None,
                });
            }
        }
        Ok(parsed.expression)
    }
    fn binding(&mut self, scope: Option<ObjectId>, depth: usize) -> Result<Binding, Diagnostic> {
        if depth > 64 {
            return Err(self.error(Code::Limit, "Binding depth exceeds 64"));
        }
        if self.take("list") {
            self.expect("[")?;
            let mut values = vec![];
            if !self.take("]") {
                loop {
                    if values.len() >= 4096 {
                        return Err(self.error(Code::Limit, "List exceeds 4096 entries"));
                    }
                    values.push(self.binding(scope, depth + 1)?);
                    if self.take("]") {
                        break;
                    }
                    self.expect(",")?;
                }
            }
            return Ok(Binding::List { values });
        }
        if self.take("ramp") {
            self.expect("(")?;
            let start = self.position;
            let ramp: crate::ramp::Ramp = self.json()?;
            self.expect(")")?;
            ramp.validate()
                .map_err(|message| self.error(Code::Value, &message))?;
            self.span(ramp.id, start)?;
            for stop in &ramp.stops {
                self.span(stop.id, start)?;
            }
            return Ok(Binding::Ramp { ramp });
        }
        if self.take("track") {
            self.expect("(")?;
            let start = self.position;
            let track: crate::binding::Track = self.json()?;
            self.expect(")")?;
            track
                .validate()
                .map_err(|message| self.error(Code::Value, &message))?;
            self.span(track.id, start)?;
            for key in &track.keys {
                self.span(key.id, start)?;
            }
            return Ok(Binding::Track { track });
        }
        let form = if self.take("value") {
            0
        } else if self.take("link") {
            1
        } else if self.take("expr") {
            2
        } else {
            return Err(self.error(
                Code::Syntax,
                "Expected value, link, expr, track, ramp or list binding",
            ));
        };
        self.expect("(")?;
        let expression = self.expression(scope)?;
        self.expect(")")?;
        match (form, expression) {
            (0, Expr::Literal { value }) => Ok(Binding::Literal { value }),
            (1, Expr::Reference { reference }) => Ok(Binding::Link { reference }),
            (2, expression) => Ok(Binding::Expression { expression }),
            _ => Err(self.error(Code::Value, "Binding wrapper does not match its expression")),
        }
    }
    fn parameter(&mut self, scope: Option<ObjectId>) -> Result<(String, Parameter), Diagnostic> {
        self.whitespace();
        let start = self.position;
        let name = self.json()?;
        let (id, mut extensions) = self.annotation()?;
        let protected = self.protected(&mut extensions)?;
        self.expect("=")?;
        let binding = self.binding(scope, 0)?;
        self.expect(";")?;
        let value =
            serde_json::to_value(binding).map_err(|e| self.error(Code::Value, &e.to_string()))?;
        self.span(id, start)?;
        Ok((
            name,
            Parameter {
                id,
                value,
                protected,
                extensions,
            },
        ))
    }
    fn instance(&mut self, scope: Option<ObjectId>) -> Result<Instance, Diagnostic> {
        self.whitespace();
        let start = self.position;
        let name: String = self.json()?;
        let (id, extensions) = self.annotation()?;
        self.expect("=")?;
        let definition_name = self.json()?;
        self.expect("clock")?;
        let clock = self.json()?;
        self.expect("{")?;
        let mut overrides = BTreeMap::new();
        let mut names = std::collections::BTreeSet::new();
        while !self.take("}") {
            self.owner = Some(name.clone());
            let (name, parameter) = self.parameter(scope)?;
            self.owner = None;
            if !names.insert(name.clone()) {
                return Err(self.error(Code::Value, "Duplicate override name"));
            }
            let input = ObjectId::new();
            self.override_names.insert(input, name);
            overrides.insert(input, parameter);
        }
        self.expect(";")?;
        self.span(id, start)?;
        self.definitions.insert(id, definition_name);
        Ok(Instance {
            id,
            name,
            definition: ObjectId::new(),
            clock,
            overrides,
            extensions,
        })
    }
    fn asset(&mut self) -> Result<Definition, Diagnostic> {
        self.whitespace();
        let start = self.position;
        let name = self.json()?;
        let (id, extensions) = self.annotation()?;
        self.expect("{")?;
        let mut definition = Definition {
            id,
            name,
            inputs: vec![],
            nodes: vec![],
            instances: vec![],
            outputs: vec![],
            extensions,
        };
        while !self.take("}") {
            if self.take("input") {
                self.whitespace();
                let start = self.position;
                let name = self.json()?;
                let (id, mut extensions) = self.annotation()?;
                let protected = self.protected(&mut extensions)?;
                self.expect(":")?;
                let ty = self.json()?;
                self.expect("=")?;
                let Expr::Literal { value: default } = self.expression(Some(definition.id))? else {
                    return Err(self.error(Code::Value, "Input default must be a literal"));
                };
                self.expect(";")?;
                self.span(id, start)?;
                definition.inputs.push(Input {
                    id,
                    name,
                    ty,
                    default,
                    protected,
                    extensions,
                });
            } else if self.take("let") {
                self.whitespace();
                let start = self.position;
                let name: String = self.json()?;
                let (id, extensions) = self.annotation()?;
                self.expect("=")?;
                let operation: String = self.json()?;
                self.expect(".")?;
                let mode: String = self.json()?;
                self.expect("@")?;
                let version = self.number()?;
                self.expect("(")?;
                let contract = registry::resolve(&operation, version, &mode)
                    .map_err(|message| self.error(Code::Value, &message))?;
                let mut parameters = BTreeMap::new();
                while !self.take(")") {
                    self.owner = Some(name.clone());
                    let (name, parameter) = self.parameter(Some(definition.id))?;
                    self.owner = None;
                    if parameters.insert(name, parameter).is_some() {
                        return Err(self.error(Code::Value, "Duplicate parameter name"));
                    }
                }
                self.expect(";")?;
                let bindings = parameters
                    .iter()
                    .map(|(name, p)| Ok((name.clone(), p.binding()?)))
                    .collect::<Result<_, String>>()
                    .map_err(|message| self.error(Code::Value, &message))?;
                let mut node = contract
                    .node(name, bindings)
                    .map_err(|message| self.error(Code::Value, &message))?;
                for (name, parameter) in parameters {
                    node.parameters.insert(name, parameter);
                }
                for parameter in node.parameters.values() {
                    if !self.spans.contains_key(&parameter.id) {
                        self.span(parameter.id, start)?;
                    }
                }
                node.id = id;
                node.extensions = extensions;
                self.span(id, start)?;
                definition.nodes.push(node);
            } else if self.take("instance") {
                definition
                    .instances
                    .push(self.instance(Some(definition.id))?);
            } else if self.take("output") {
                self.whitespace();
                let start = self.position;
                let name = self.json()?;
                let (id, extensions) = self.annotation()?;
                self.expect(":")?;
                let ty = self.json()?;
                self.expect("=")?;
                let Expr::Reference { reference: value } = self.expression(Some(definition.id))?
                else {
                    return Err(self.error(Code::Value, "Output requires a named reference"));
                };
                self.expect(";")?;
                self.span(id, start)?;
                definition.outputs.push(Output {
                    id,
                    name,
                    ty,
                    value,
                    extensions,
                });
            } else if self.take("unresolved") {
                let start = self.position;
                let node: Node = self.json()?;
                self.expect(";")?;
                if registry::resolve(&node.operation, node.version, &node.mode).is_ok() {
                    return Err(self.error(
                        Code::Value,
                        "Known operation must use a native operation declaration",
                    ));
                }
                self.span(node.id, start)?;
                for parameter in node.parameters.values() {
                    self.span(parameter.id, start)?;
                }
                self.unresolved.push(node.id);
                definition.nodes.push(node);
            } else {
                return Err(self.error(
                    Code::Syntax,
                    "Expected input, let, instance, output or unresolved declaration",
                ));
            }
        }
        self.span(id, start)?;
        Ok(definition)
    }
    fn resolve(&self, construction: &mut Construction) -> Result<(), Diagnostic> {
        let mut definitions = BTreeMap::new();
        for definition in &construction.definitions {
            if definitions
                .insert(
                    definition.name.clone(),
                    (
                        definition.id,
                        definition
                            .inputs
                            .iter()
                            .map(|i| (i.name.clone(), i.id))
                            .collect::<BTreeMap<_, _>>(),
                    ),
                )
                .is_some()
            {
                return Err(self.error(Code::Value, "Duplicate definition name"));
            }
        }
        for instance in std::iter::once(&mut construction.root).chain(
            construction
                .definitions
                .iter_mut()
                .flat_map(|d| &mut d.instances),
        ) {
            let Some(name) = self.definitions.get(&instance.id) else {
                continue;
            };
            let (id, inputs) = definitions.get(name).ok_or_else(|| {
                self.error(Code::Reference, &format!("Unknown definition {name}"))
            })?;
            instance.definition = *id;
            let mut overrides = BTreeMap::new();
            for (temporary, parameter) in std::mem::take(&mut instance.overrides) {
                let name = &self.override_names[&temporary];
                let input = inputs.get(name).ok_or_else(|| {
                    self.error(Code::Reference, &format!("Unknown override input {name}"))
                })?;
                overrides.insert(*input, parameter);
            }
            instance.overrides = overrides;
        }
        let mut targets = BTreeMap::new();
        for (temporary, pending) in &self.references {
            let resolved = (|| {
                let scope = pending
                    .scope
                    .ok_or("Root overrides cannot reference a parent definition")?;
                let definition = construction.definition(scope)?;
                if pending.name.starts_with("channel:") {
                    fold_graph::channel::resolve_definition(
                        definition,
                        pending.owner.as_deref(),
                        &pending.name,
                    )
                } else {
                    crate::source::resolve_reference_name(construction, definition, &pending.name)
                }
            })()
            .map_err(|message| Diagnostic {
                code: Code::Reference,
                message,
                span: pending.span,
                native_id: pending.scope,
                validation: None,
            })?;
            targets.insert(*temporary, resolved);
        }
        for definition in &mut construction.definitions {
            for node in &mut definition.nodes {
                if !self.spans.contains_key(&node.id) || self.unresolved.contains(&node.id) {
                    continue;
                }
                for parameter in node.parameters.values_mut() {
                    rewrite_parameter(parameter, &targets)
                        .map_err(|message| self.error(Code::Value, &message))?;
                }
            }
            for output in &mut definition.outputs {
                if self.spans.contains_key(&output.id) {
                    rewrite_reference(&mut output.value.target, &targets);
                }
            }
        }
        for instance in std::iter::once(&mut construction.root).chain(
            construction
                .definitions
                .iter_mut()
                .flat_map(|d| &mut d.instances),
        ) {
            if !self.definitions.contains_key(&instance.id) {
                continue;
            }
            for parameter in instance.overrides.values_mut() {
                rewrite_parameter(parameter, &targets)
                    .map_err(|message| self.error(Code::Value, &message))?;
            }
        }
        Ok(())
    }
}
fn rewrite_reference(reference: &mut Reference, targets: &BTreeMap<ObjectId, Reference>) {
    if let Reference::Node { node } = reference
        && let Some(target) = targets.get(node)
    {
        *reference = *target;
    }
}
fn rewrite_expression(expression: &mut Expr, targets: &BTreeMap<ObjectId, Reference>) {
    match expression {
        Expr::Reference { reference } => rewrite_reference(&mut reference.target, targets),
        Expr::Unary { value, .. } => rewrite_expression(value, targets),
        Expr::Binary { left, right, .. } => {
            rewrite_expression(left, targets);
            rewrite_expression(right, targets);
        }
        Expr::Call { arguments, .. } => {
            for argument in arguments {
                rewrite_expression(argument, targets);
            }
        }
        Expr::Vector { components, .. } => {
            for component in components {
                rewrite_expression(component, targets);
            }
        }
        _ => {}
    }
}
fn rewrite_parameter(
    parameter: &mut Parameter,
    targets: &BTreeMap<ObjectId, Reference>,
) -> Result<(), String> {
    fn rewrite(binding: &mut Binding, targets: &BTreeMap<ObjectId, Reference>) {
        match binding {
            Binding::Link { reference } => rewrite_reference(&mut reference.target, targets),
            Binding::Expression { expression } => rewrite_expression(expression, targets),
            Binding::List { values } => {
                for value in values {
                    rewrite(value, targets);
                }
            }
            _ => {}
        }
    }
    let mut binding = parameter.binding()?;
    rewrite(&mut binding, targets);
    parameter.value = serde_json::to_value(binding).map_err(|e| e.to_string())?;
    Ok(())
}
