# Badge Wave — live SDK demo

Open `fold-badge-live.fold`. It contains a shared **Composition → Finish → Sequence** chain. The main Composition exposes Duplicator, badge (hidden source) with Text and Background children, and the full-canvas Background layer. Finish contains a Compositing layer reading the main Composition.

- **Badge:** expand it directly in Composition. Select Text or Background to open that child’s actual MoGraph network and properties. The background follows replacement text.
- **Duplicator:** select its layer to open its original network, then change Count, Spacing and Stagger. Its Source points to Badge. Position the copies with the Duplicator transform; the source stays independently editable.
- **Oscillator:** drives the Duplicator's Y offset. Minimum/Maximum change the height; Duration changes the period. Other waveforms and time modes remain available.
- **Color Ramp:** maps Copy Progress to color. Edit its stops independently of distribution and motion. Shapes Only keeps text readable.
- Enter each tool's group for its native nodes and published interface.

## Continue with an agent

Enable **Agent → Allow local connections**. Ask the agent to discover `catalog guide="live-tools"` and SDK version 2. This project uses namespace **`badge-demo`**. Reusing that namespace preserves identities after renaming, reconnecting and reopening.

For example: “Make the motion gentler, preserving my text, layout and palette. Show me an Agent preview before applying.” The agent changes Oscillator Minimum/Maximum explicitly. The preview has its own frame scrubber, Original comparison, Apply and Discard. Applying creates one undo entry. An intervening artist edit blocks stale application.

The script's constructor arguments initialize new work only. Repeating the script does not reset artist edits. Explicit setters are edits; wrap initialization setters in `.initialize(...)`.

## Original source-graph generator

```sh
python3 skills/fold/scripts/live_badge.py target/release/fold-cli /tmp/new-badge.fold
```

The checked-in files have been updated directly to the common Composition model. This script still builds the original provider-graph chain; it does not recreate the shared layer stack. Public SDK support for creating that stack is tracked in phase 51.

The script creates isolated sessions, renders five animation times, checks repetition and save/reopen, and tests artist-preserving updates. It saves the original artwork before running the follow-up edits.

This checked-in demo retains the original Badge project's legacy color policy for exact visual parity. Fresh projects use Fold's current managed-color default and therefore display these same authored colors differently. To compare against an original demo, pass `--baseline ORIGINAL.fold`; the script keeps that file untouched and reconstructs all artwork in a separate session under its color policy.
