import unittest
from check_proposal_ownership import violations


class ProposalOwnershipTests(unittest.TestCase):
    def test_aliases_grouped_imports_and_whitespace_cannot_hide_dependency(self):
        for source in ('use fold_motion_canvas :: model::Canvas as Graph;',
                       'use fold_motion_context::proposals::MotionProvider;',
                       'use fold_three_d::model::Scene;',
                       'use {fold_compositor::{Composite as Graph}, serde::Serialize};',
                       'use fold_composition::Composition;',
                       'extern crate fold_timeline as creative;'):
            self.assertTrue(violations(source), source)

    def test_comments_literals_and_test_modules_do_not_create_dependencies(self):
        source = '''// fold_motion_canvas::model
        /* nested /* fold_compositor */ comment */
        const HELP: &str = r##"fold_motion_canvas:: and "Project""##;
        #[cfg(test)] mod tests { use fold_motion_canvas::model::Canvas; }
        use fold_platform::proposals::Prepared;
        '''
        self.assertEqual(violations(source), [])
        self.assertTrue(violations(source + 'use fold_motion_canvas::model;'))

    def test_provider_can_prepare_but_cannot_publish_or_own_receipts(self):
        self.assertEqual(violations('use fold_project::{Document, Snapshot, Mutation};', True), [])
        for source in ('use fold_project::{Project as Writer, Snapshot};',
                       'use fold_project::proposals::InspectionProof as Receipt;',
                       'let store = proposals :: Store::default();',
                       'use fold_project::{Snapshot, proposals::{Store as Candidates}};',
                       'use fold_project::{proposals as candidates};',
                       'writer . commit_named (name, batch);'):
            self.assertTrue(violations(source, True), source)

    def test_reconciliation_stays_with_provider(self):
        source = 'fold_graph :: reconcile :: merge(before, after, current)'
        self.assertTrue(violations(source))
        self.assertEqual(violations(source, True), [])
        self.assertTrue(violations('use fold_graph::{model::Node, reconcile::{merge as edit}};'))
