import copy
import unittest
from check_shared_probe import summarize


def report():
    phases = [dict(phase=p, seconds=30., admissions=[900, 90],
                   skipped_between_admissions=[0, 0], audio_errors=[None, None],
                   max_observed_underrun_callbacks=[0, None]) for p in (1, 2)]
    events = [dict(kind='phase', phase=1, at_ms=0),
              dict(kind='phase', phase=2, at_ms=30000),
              dict(kind='export', frames=150, through_completion_observed_seconds=30.)]
    events += [dict(kind='seek', warm_repeat=True, through_present_call_ms=5.) for _ in range(3)]
    return dict(success=True, shared=dict(phases=phases, events=events),
                samples=[dict(start_ms=t, draw_ms=10., acquire_ms=8.) for t in (1, 30001)])


class SharedProbeTests(unittest.TestCase):
    def test_native_completion_is_not_performance_acceptance(self):
        good = report()
        self.assertTrue(summarize(good)['passed'])
        for phase, count in ((0, 890), (1, 199)):
            bad = copy.deepcopy(good)
            bad['shared']['phases'][phase]['admissions'][0] = count
            self.assertFalse(summarize(bad)['passed'])

    def test_draw_interval_is_inclusive_and_missing_samples_fail(self):
        good = report()
        self.assertEqual(summarize(good)['phases'][0]['ui_p95_ms'], 10.)
        good['samples'] = []
        self.assertFalse(summarize(good)['passed'])

    def test_audio_gaps_seek_and_export_are_independent_gates(self):
        for change in (
            lambda r: r['shared']['phases'][1]['skipped_between_admissions'].__setitem__(1, 1),
            lambda r: r['shared']['phases'][0]['max_observed_underrun_callbacks'].__setitem__(0, 1),
            lambda r: r['shared']['events'][-1].__setitem__('through_present_call_ms', 101.),
            lambda r: r['shared']['events'][2].__setitem__('through_completion_observed_seconds', 61.),
        ):
            bad = report()
            change(bad)
            self.assertFalse(summarize(bad)['passed'])


if __name__ == '__main__':
    unittest.main()
