# Native expression language — implemented M2 subset

This describes `fold_motion::expression_source` and the native construction
language in `fold_motion::source`. Both have automated round-trip tests. They are
not yet an available CLI/MCP authoring protocol.
The authoritative expression is the native common IR. Parsing and printing require
only the language service, not a Rust build for each edit.

An expression is bounded to 64 KiB of UTF-8 source, 4,096 terms and depth 64. Source
ranges use UTF-8 byte offsets. Whitespace and `//` comments are accepted. Canonical
printing adds parentheses to preserve the exact native tree; it does not retain
source formatting or comments. Artist annotations belong to native metadata.

## Values and references

- `2` and `2.5` are Scalar; `2i` is Integer. Numeric tokens begin with a digit and
  support decimal exponents. `100px` is Distance<Canvas>; `0.2m` is Distance<Meters>.
- `true`, `false`, and JSON-escaped quoted strings are Bool and Text.
- `literal(JSON)` represents any validated native literal exactly, including
  rational Duration, Color, Vector, Rotation and AssetRef. Its JSON is the native
  tagged literal schema. Negative literal values use this form when canonical
  printing must distinguish a literal from a unary-negation expression.
- `size` or `$"size"` resolves a named reference in the caller's definition scope.
  Canonical printing uses the quoted form. Names can contain spaces or Unicode;
  scope membership and type checking use native target IDs.
- A reference can carry `@{"id":"UUID","artist_note":"keep"}`. The UUID identifies
  this connection, not its target. Other metadata is retained, except reserved
  authored fields are rejected. Missing UUIDs allocate fresh connections. Duplicate
  connection UUIDs fail validation. Canonical source always includes UUID metadata.

Example tested by the parser/evaluator suite:

```text
// width in canvas units
size * (2 + 3) - 4px
```

For a size input of 10 Canvas units, the result is 46 Canvas units. Renaming the
input changes its projected name while preserving its target and connection IDs.
The parser returns edge source ranges separately from the native expression.

## Operators and functions

Precedence, low to high: `||`, `&&`, `==`, `<`, `+ -`, `* /`, unary `- !`.
Binary operators associate left. Parentheses override precedence. Evaluation uses
the existing typed IR rules; parsing does not introduce numeric, unit or space
coercions. Boolean operands are evaluated eagerly under the shared work budget.

Functions are `sin`, `cos`, `abs`, `min`, `max`, `clamp`, `lerp`, and `length`.
The native type checker owns arity and type validation. Argument lists allow at
most three terms. Other common-IR forms are:

```text
time.scene
time.asset
element.index
element.count
element.position("meters", "world")
random(1, "jitter")
vector("canvas", "screen", 2px, 3px)
```

Vectors require two or three components with matching units. Element expressions
require an explicit element context during execution. Random accepts only the
qualified version and named-stream policy in `contracts.md`. No source construct
imports code, accesses files/network, creates host closures, or runs imperative
programs.

Diagnostics distinguish syntax, source limits, unresolved names, invalid values
and invalid identities, with a byte span and native ID where applicable. Source-buffer transactions now use the provider edit service described in
[editing.md](editing.md). Asset fragments and explicit identity mapping are also implemented there.


## Construction language — parser and printer implemented

`fold_motion::source::canonical` prints the complete native construction and
`fold_motion::source::parse` reads it. Full native equality, canonical stability,
evaluation after reparse, asset overrides, graph presentation and unresolved-record
recovery are tested. This is language/reference evidence; native artist and public
agent acceptance are still pending.

The declaration envelope is `motion 1;`, followed by `seed`, `canvas`, `asset`
definitions, a `root` instance, `presentation` and `extensions`. Assets contain
`input`, `let`, `instance`, `output` and `unresolved` declarations. Authored names
are JSON-escaped strings. Identity and native metadata use `@{...}` annotations;
types and rational clocks use their explicit native JSON schemas. Operation calls
name the registry operation key, mode and version. Parameters have their own UUID,
protection flag and extension metadata.

Parameter bindings print as `value(expression)`, `link(reference)`,
`expr(expression)`, `track(native-track-JSON)` or `list[binding, ...]`. The wrappers
preserve the native binding form rather than guessing it from an expression's
shape. Full tracks retain key IDs, interpolation, combination, base and extensions.
Unresolved nodes retain their complete native JSON and are never substituted with
known operations. Layout is an independent native presentation record.

Ordinary input/node references use their local names. Instance output references
use an `instance:` prefix followed by a JSON pair of instance and output names.
A local name beginning with `instance:` or `binding:` is escaped as `binding:` plus
its JSON string, making these namespaces unambiguous. The expression layer quotes
these complete reference names; IDs continue to own semantic identity.


Parsing allocates native IDs for declarations and connections lacking annotations,
then resolves names across the completed definition set. Forward node references
and forward asset definitions are supported. Required parameters come from the
operation registry; omitted optional parameters receive its defaults and fresh
parameter IDs. Repeated parameter/override names are errors. Root overrides have
no parent scope to reference.

`parse` returns the native construction, a map of native IDs to source byte ranges,
and the IDs of explicitly unresolved operations. It validates syntax, references
and structural persistence. `Construction::validate_semantics` performs typed
operation/graph qualification, and `Prepared::new` requires that qualification
before execution. This separation preserves unresolved records without treating
them as executable. Complete-document source proposals now pass through scoped, protected project
transactions. One-asset source projection/parsing and explicit identity reconciliation are also
implemented; see [editing.md](editing.md).
