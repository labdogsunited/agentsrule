# Questions to go through together (written 2026-10-20 for 2026-10-21)

**Closed 2026-10-21:** the user accepted every proposed answer, so all answers below are decisions. They are recorded in `proposals/vex-language.md`, `visual-language.md` §11, `engine-api.md`, `field-rules.md` and `plan-next.md`.

Each question says why it matters and, where I have one, my recommendation. Background: [language](../proposals/vex-language.md), [visuals](../proposals/visual-language.md), [engine](../proposals/engine-api.md), [next steps](../proposals/plan-next.md), and [field rules](../proposals/field-rules.md).

Edited 2026-10-21. **Answer** preserves and clarifies my stated preference. **Proposed answer** supplies a recommendation where I left the choice open or did not answer; it is a draft, not a decision I have already approved. These answers describe intended direction, not necessarily implemented features. The process answers do not authorize a push or history rewrite.

## 1. A VEX-like language (`vex-language.md`)

1. **How far should it go?** Views only, views then world rules, or the whole language (rules, genesis, genes)? *Recommendation: views first, then world rules; genes last, if at all.*

**Answer:** The whole language: views, world rules, genesis, and genes. I want a consistent VEX-like way of working throughout, rather than limiting it to visualization. Implementation can happen in stages, but the design should cover the whole language from the start.

2. **What does `+=` on a substance mean?** Substances are conserved, so `@nitrate += …` cannot create nitrate. Is "move from a named store" (`@nitrate += take(world.rock, 10u);`, arbitrated like any transfer) the meaning you want? Or did you mean something else, such as a non-conserved quantity?

**Answer:** Yes. For a conserved substance, `+=` should mean transferring from an explicitly named store, subject to availability, capacity, and the normal arbitration rules. It must never create matter. Keep direct assignment available for non-conserved attributes where the context permits it.

3. **What is an agent's "type"?** Its founding lineage (`@lineage == "Builder"`), something measured (`@made_body > 0` is test 9's "producer"), or a label the program defines (`@producer = …;`)? Should species (clusters of similar genomes) become a type too?

**Answer:** “Type” was just an example. What I really want is a consistent way to access an agent’s properties: identity, lineage, genome, inventory, age, measured behavior, and any classifications we define. I like a VEX-like function interface for queries, alongside attributes for the current element. Keep lineage, measured traits, and species distinct rather than overloading one `type` value.

4. **Experimenter code vs evolved behaviour.** A `context(agent)` block that you write acts on agents from outside (a feeding regime, a cull, a rescue). Behaviour itself comes only from evolved genomes. Should the language keep these two strictly apart, for example by naming the block `rule … on agents` rather than calling it agent code?

**Proposed answer:** Keep experimenter interventions and evolved behavior explicitly separate, while giving both the same underlying syntax and query functions. The enclosing declaration should identify whether a block is a world rule, genome code, or a view; `context(agent)` should only select which agents it operates on. Record interventions in the run journal so externally supplied behavior cannot be mistaken for something evolution discovered.

5. **Syntax style:** C-like as in VEX (`if (…) …;` and braces), or the `if …:` style of your example? `.em` has one spelling per meaning, so only one of them can be chosen.

**Answer:** Use C-like syntax throughout: braces, semicolons, and `if (…) { … }`. Keep one canonical spelling for each construct.

6. **One effect kind per block?** Today a rule holds one kind of effect (transfers, reactions or signal writes), which fixes when it runs in a tick. Keep that rule, or let a VEX body mix effects and split it into phases automatically (shown by `emerge order`)?

**Proposed answer:** Aim to allow mixed effects in one body, provided the compiler can assign them unambiguously to the existing tick phases. Reads of simulation state should use the defined snapshot, not imply that an earlier transfer has already happened. Show the schedule through `emerge order` and reject dependencies that require unavailable intermediate state. Until those semantics are specified and checked, retain one effect kind per block.

7. **Nested contexts:** `context(world) { context(agent) { … } }` as in your example, or flat blocks per element kind (`context(agent) { … }`, `context(cells) { … }`)? Nesting reads well, but needs rules for what an inner block sees of the outer one.

**Answer:** Yes, support nested contexts. That feels natural for moving from a world to its agents or cells. Define scope clearly: inner blocks should see outer constants and explicit references to outer elements, while unqualified attributes refer to the current element. Nesting must not imply sequential execution or bypass snapshot and conservation rules.

## 2. Visuals (`visual-language.md`, plan item 12)

8. **What does "amazing" look like to you?** Microscope (crisp, flat), bioluminescent dish (dark, glowing centres, trails), landscape (a field as terrain)? Is there a reference image or video you like?

**Answer:** I want a modern visual system with enough control to develop different styles. Prioritize data-driven attributes, editable color palettes and ramps, antialiasing, glow, layers, transparency, and compositing modes such as add, multiply, and subtract. Typography should expose font family and size. These controls should be easy to use from the language and the viewer, with attractive defaults.

9. **Live or recorded first?** A live browser view while a world runs, or excellent stills and short clips of recorded runs (journals, batches)? *Recommendation: recorded first (it works for every experiment we already have), then live.*

**Answer:** Live first, with recording and export built into the workflow. I want to explore a running world, record it, and export PNG or EXR images and image sequences, plus MOV video. The same view definition should drive the live display and exports. The existing still renderer can be reused as the foundation.

10. **Which experiments should be drawn first?** For example test 9 (producers appearing among Drifters), the bloat head-to-head, the catalyst dish, the contact runs. Each needs its own attributes; test 9 needs a `made` (producer) attribute in views.

**Proposed answer:** Start with test 9: show the resource field, agents, and measured body production over an explicit window. It directly supports the current science and gives the proposed `made` binding a concrete use. Next, visualize the bloat head-to-head to exercise lineage comparisons. Add the catalyst and contact runs after those, and Game of Life alongside the first field-rule implementation.

11. **Species:** the reference view uses `species_id`, which is not defined. Should we define species (for example, genomes within a distance of each other), or use lineage plus measured traits only?

**Proposed answer:** Expose stable entity IDs and founding-lineage IDs now, together with measured traits. Defer `species_id` until we define a reproducible genome-clustering method, its parameters, and how cluster identities persist across time. IDs are useful, but lineage should not silently stand in for species. A view requesting unavailable species data should say so explicitly.

12. **Cellular automata:** they need field rules to run (`field-rules.md`) before there is anything to draw. Start with a discrete Game of Life (exact fixtures), or go straight to continuous fields (Lenia)?

**Proposed answer:** Start with discrete Game of Life on the CPU, using exact blinker and glider fixtures and cell-exact rendering. This gives field updates, neighborhood reads, boundary handling, and visualization a small, testable first target. Move to continuous fields such as Lenia after that foundation works; revise the older field proposal to match the chosen numeric contract before implementing it.

13. **Charts beside the picture:** which numbers belong next to every image (population by lineage, producers, flux)?

**Proposed answer:** Use a compact default panel: tick and revision, total population, population by lineage, births and deaths, and conservation error. Add experiment-specific charts: producer share and body-production flux for test 9; lineage share and upkeep for the bloat comparison. Show units and measurement windows, and let the view choose which charts are visible.

14. **Where should pictures be kept?** Generated on demand only, or a small gallery of chosen images in the repository?

**Proposed answer:** Keep a small curated gallery in the repository, with a reproducible command and the source, seed, tick, backend, mode, and view settings for each image. Generate other images on demand. Store large recordings and EXR sequences outside Git, with a manifest or link when they support a result.

## 3. Engine, hot-swap and MCP (`engine-api.md`)

15. **Read the proposal and its results.** It was written and built without your review. Anything to change?

**Proposed answer:** Keep the current engine architecture as the starting point: staged swaps, revision checks, preflight, and replayable journals fit the intended workflow. Before extending it, reconcile the proposal with the implemented limits and make those limits visible to clients. In particular, the browser viewer needs an explicit connection design because the server currently rejects WebSocket requests with an `Origin` header; GPU genome inspection is still incomplete. This is a direction recommendation, not a claim that the implementation has received a fresh code review.

16. **The swappable subset:** substances, reactions, menus and action fees cannot be swapped yet, and a lowered cap needs `move_to`. Which of these matter for the experiments you plan?

**Proposed answer:** Prioritize action-fee changes and explicit `move_to` handling for lowered caps: they would let us explore costs and resource constraints without restarting. Implement each only with defined migration semantics, atomic rejection, conservation checks, and replay coverage on both backends. Defer swapping substances, reaction definitions, and menus until an experiment needs it; start a new world for those changes in the meantime.

17. **AI in the loop:** should a model (Haiku by default) act as a live director of experiments, adjusting a running world under a stop rule? Or should MCP stay a convenience for building and inspecting worlds?

**Proposed answer:** Support an optional AI director through MCP for bounded exploratory runs, using Haiku as the initial default from the existing trial. Give each session an objective, allowed interventions, a tick limit, a call or cost budget, and a stop rule; journal its changes and reasons. Keep fixed scientific baselines free of adaptive intervention unless the controller itself is the treatment. MCP should remain useful for manual creation and inspection too.

18. **The MCP tool names use dots** (`world.create`). They work in Claude Code. Keep them, or switch to underscores for clients that are strict about names?

**Proposed answer:** Use underscore names for the public MCP tools, such as `world_create`, mapped to the existing dotted JSON-RPC methods. This avoids making the MCP interface depend on clients accepting dots and does not require renaming the engine API. Update discovery, examples, and tests together, and document the tool-name change for existing clients.

## 4. The science (`plan-next.md` item 1)

19. **The joining problem:** producers need two gene pieces at once (a body reaction and `take(nitrate)`). Contact-based gene sharing and cheaper genes both failed. Which idea next: substance effects, recombination, or a world where each piece is useful alone?

**Proposed answer:** First design a small world where each piece can provide a measurable benefit on its own, using existing mechanics if possible. Compare neither piece, each piece alone, and both together under fixed conditions before running an evolution batch. Record the hypothesis, measurements, success threshold, and stop rule in a new proposal before running it. Treat this as a new experiment, not another attempt to tune the completed test 9 result. Revisit recombination or substance effects only if the diagnostic identifies a need for them; the failed contact and cheap-gene screens do not establish a universal cause.

20. **Use the GPU for batches?** GPU runs are 20 to 65 times faster, but they run in contract mode, so they need their own baselines (for example test 9 re-run on the GPU) before any comparison. Worth one baseline batch?

**Proposed answer:** Yes: propose one fixed GPU contract-mode baseline batch using the archived test-9 world, seeds 1–20, and its existing measurement windows and stop rules. Record the source revision, mode, device, and driver. Use this as the baseline for later GPU treatments; keep the historical CPU default-mode results separate. Establish the baseline without retuning the world or rerunning seeds selectively.

21. **The spec revision:** the catalyst integer form, the 10_000bp cap, view checking and the engine all need recording in the spec (v0.7.9?). When?

**Proposed answer:** Make a focused v0.7.9 documentation update before the next feature batch. Record the implemented catalyst arithmetic, the `10_000bp` cap, view checking and rendering, and the engine, swap, and journal contracts, including backend limits. Reconcile the spec, language guide, and support matrix. Keep the full VEX-like language and live visuals clearly marked as proposed until implemented; documenting a semantic change should not substitute for deciding whether to retain it.

## 5. Process

22. **Review of today's work:** about 30 local commits, none pushed. Push them, review them first, or squash them?

**Proposed answer:** Review the local commits first, then push the reviewed branch when authorized. Summarize the changes, check for unrelated files or generated artifacts, and run checks appropriate to the changes. Keep useful commit history; do not squash simply because there are many commits. Any history rewrite should be a separate, explicit choice.

23. **Approval gates:** I changed contract mode (the catalyst factor) and the default runner (the 10_000bp cap) on my own judgement. Which kinds of change should always wait for you?

**Proposed answer:** Proceed autonomously with routine fixes, refactors, documentation, tests, and implementation choices within the agreed scope. Bring back changes that alter the scientific contract or agreed scope: conservation, arithmetic and determinism, tick ordering, default evolutionary behavior, success criteria, or stop rules. Experimental variants can be developed behind explicit options without replacing the baseline. Previously approved choices do not need approval again; for a new decision, present the concrete change, evidence, and consequences together.

24. **Concurrent agents:** another agent wrote `visual-language.md` in this repository while I worked. Should parallel agents work in separate worktrees or branches?

**Proposed answer:** When parallel agents are used, give each one a separate worktree and branch with a clear scope. Assign ownership for shared spec and plan files, then integrate reviewed changes through one coordinating agent. Use a shared working tree only for deliberately non-overlapping edits with explicit coordination, and keep experimental outputs separate from source changes.

## Status while you think

- **Exit test 2** passed on the CPU too (100_000 ticks, 40 swaps, 0 ticks with element totals off), so it is met on both backends (`engine-api.md`).
- Phase 0 exit tests met: 1 to 11 (3 for worlds with a space).
