"""Check the C4 source seam that Cargo's crate-level graph cannot express.

Only generic wire/dispatch and provider proposal modules are constrained. Cross-
feature App commands remain legal. This is a lexical dependency check, not a Rust
parser or proof against macro-generated/re-exported coupling; behavioral contract
and publication tests supply the other half of the gate.
"""
from pathlib import Path
import re

ROOT = Path(__file__).resolve().parents[1]


def tokens(source):
    # Ignore comments and literals, including nested block comments and raw strings.
    pattern = re.compile(r'//[^\n]*|/\*|r(#+)?"|"(?:\\.|[^"\\])*"|\b[A-Za-z_][A-Za-z_0-9]*\b|::|[^\s]', re.S)
    result = []
    position = 0
    while match := pattern.search(source, position):
        value = match.group()
        position = match.end()
        if value.startswith('//') or value.startswith('"'):
            continue
        if value == '/*':
            depth = 1
            while depth and (comment := re.search(r'/\*|\*/', source[position:])):
                depth += 1 if comment.group() == '/*' else -1
                position += comment.end()
            if depth:
                raise ValueError('unterminated block comment')
            continue
        if value.startswith('r') and value.endswith('"'):
            end = source.find('"' + (match.group(1) or ''), position)
            if end < 0:
                raise ValueError('unterminated raw string')
            position = end + 1 + len(match.group(1) or '')
            continue
        result.append(value)
    return result


def production_tokens(source):
    values = tokens(source)
    # Exclude only an explicitly cfg(test) module body, not everything after it.
    marker = ['#', '[', 'cfg', '(', 'test', ')', ']', 'mod']
    result = []
    i = 0
    while i < len(values):
        if values[i:i + len(marker)] == marker and values[i + 9:i + 10] == ['{']:
            i += 10
            depth = 1
            while i < len(values) and depth:
                depth += (values[i] == '{') - (values[i] == '}')
                i += 1
        else:
            result.append(values[i])
            i += 1
    return result


def imported_paths(values):
    """Expand Rust use trees so aliases/grouping cannot conceal an owned service."""
    def tree(items, prefix=()):
        path = prefix
        while items:
            value = items.pop(0)
            if value == '{':
                yield from tree(items, path)
                path = prefix
            elif value == '}':
                if path != prefix:
                    yield path
                return
            elif value == ',':
                if path != prefix:
                    yield path
                path = prefix
            elif value == 'as':
                if items:
                    items.pop(0)
            elif value != '::':
                path += (value,)
        if path != prefix:
            yield path

    for index, value in enumerate(values):
        if value == 'use':
            end = values.index(';', index)
            yield from tree(values[index + 1:end])


def violations(source, provider=False):
    values = production_tokens(source)
    # Import aliases and grouped imports still contain their original identifiers.
    forbidden = {'Project', 'InspectionProof'} if provider else {'fold_motion', 'fold_compositor', 'fold_composition', 'fold_timeline'}
    errors = [f'forbidden dependency {name}' for name in sorted(forbidden & set(values))]
    paths = [('fold_project', '::', 'proposals'), ('proposals', '::', 'Store')] if provider else [('fold_graph', '::', 'reconcile')]
    for path in paths:
        if any(tuple(values[i:i + len(path)]) == path for i in range(len(values))):
            errors.append('forbidden authority/path ' + ''.join(path))
    for path in imported_paths(values):
        if (provider and (path[:2] == ('fold_project', 'proposals') or path[-2:] == ('proposals', 'Store'))) or (
            not provider and path[:2] == ('fold_graph', 'reconcile')
        ):
            errors.append('forbidden imported authority/path ' + '::'.join(path))
    if provider:
        for i in range(len(values) - 2):
            if values[i] == '.' and values[i + 1] in {'commit', 'commit_named'} and values[i + 2] == '(':
                errors.append('provider publishes a project transaction')
    return errors


def main():
    agent = ROOT / 'crates/app/src/agent'
    dispatch = [agent / 'wire.rs', *agent.glob('dispatch*.rs'), *(agent / 'dispatch').rglob('*.rs')]
    providers = [path for crate in ('motion', 'compositor', 'composition')
                 for path in (ROOT / f'crates/{crate}/src').rglob('*.rs')
                 if path.stem == 'proposals' or 'proposals' in path.parts]
    errors = [f'{path.relative_to(ROOT)}: {error}'
              for paths, provider in ((dispatch, False), (providers, True))
              for path in paths for error in violations(path.read_text(), provider)]
    if errors:
        raise SystemExit('\n'.join(errors))
    print('Scoped proposal ownership passed: generic dispatch has no creative dependencies; providers have no publication authority.')


if __name__ == '__main__':
    main()
