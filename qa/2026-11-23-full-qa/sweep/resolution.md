# Host sweep: how every suspect was resolved

Host run: `qa/tools/module_sweep.py`, REAPER VST3, 4 hidden instances, 2 s renders, 28.6 min, 1964 own parameters swept (the 17 chain positions are covered by the unit test `position_decides_the_order_and_any_values_are_valid`)
(REAPER lists 1985 with its Bypass, Wet and Delta). One module on at a time; min and max against that module's default render;
suspects retried in 5 random states of their neighbourhood. Result counts: {'ok': 245, 'CONDITIONAL': 993, 'DEAD': 723, 'non-audio': 3}.

Rule 7: a DEAD from the host is a hypothesis. Second route, in-process (`recheck_dead_parameters`, fresh instance per render,
renders aligned by latency as a host does), every parameter of the module random, 40 states:

| Step | Remaining DEAD | How |
|---|---|---|
| host sweep | 723 | min/max vs default, 5 neighbourhood states |
| recheck 1 | 192 | 531 act in a module-wide random state: CONDITIONAL (`recheck-1-random.tsv`) |
| recheck 2 | 63 | solo switches held at default (a soloed band hides the others), 129 more act (`recheck-2-solo-held.tsv`) |
| last 63 | 0 | below |

The last 63:
- `delayUnit` (1): non-audio. It picks how the channel delay is shown (ms or frames); the delay is stored in ms and the
  engine (`settings_from`) never reads it. Recorded with that reason in the sweep's non-audio list.
- SimplEQ `b2/b4 Width/Depth`, `b6 Width/Depth Ch2` (6): act on a band reject (depth only exists there, reject width is
  0.02 to 1 octave). Proven by `conditional_parameters_act_once_their_condition_holds`, Ch2 with the sets unlinked.
- Loudness MAX `learnTarget`, `learnLoudness` (2): a learn action. At its ends learnLoudness sets nothing (idle, or louder
  than any target); between them it sets the drive to target minus loudness. Proven by the same test.
- DynEQ (54, scattered over bands): CONDITIONAL by construction. All 32 bands go through one mapping
  (`band_param(i, f)`, layout checked uniform over all 1088 band parameters), and each of these fields acts in 23 to 31
  other bands in the recheck. They need band on, dynamics on and the detector crossing the threshold at once, which random
  states rarely line up for a given band.

No parameter is dead. The visual leg was not part of this sweep (moving any module parameter also switches its rack chip,
which hides the answer); it rests on `every_parameter_can_be_reached` and the screenshot review.
