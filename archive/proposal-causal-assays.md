**Proposal: causal assays with compiler-checked experimental controls**

Research and design analysis, 2026-10-16. This is a proposed extension, not implemented syntax or a change to the current specification. No experiments reported below have been run for this proposal. Example counts and horizons are design choices, not power calculations.

**Recommendation.** Give an Emergent program an experimental interface: named mechanisms can declare how to substitute a control, which immediate effects must remain identical, and which differences the experiment permits. An `assay` checks these obligations, executes isolated branches, and returns evidence whose interpretation and scope remain attached to the value.

The unusual part is that a programmer could get a compile error for an invalid experimental control. For example: “You claimed to remove only this signal's information, but your control also removes the gene's upkeep.”

This borrows from sham-controlled experiments and experimental design in medicine. It fits Emergent particularly well because costs, conservation, effect boundaries, and execution phases already have language semantics. The aim is to make a causal argument something the program can partially check and reproduce.

There is substantial prior art. Counterfactual execution, typed intervention points, causal effect handlers, relational cost types, statistical verification, and even placebo comparisons of program abstractions already exist. The specific proposal is the integration of **law-preserving control construction, relational checks on immediate effects, and evidence scoped to that checked contrast** in an evolving graph language. I did not find that complete feature in the sources examined. That is a bounded novelty finding, not proof that nobody has attempted it.

**What the existing language does especially well.**

I treated [core.md](../core.md) as the proposed general language, [language.md](../language.md) as the current syntax authority, and [spec-v0.7.8.md](../spec-v0.7.8.md) as the creatures application's runtime specification. They should not be conflated: the core syntax is provisional, and the implementation still contains application-specific machinery.

The generalization to graphs is convincing. Conserved quantities can live on nodes or edges; a regular grid becomes a representation choice. Laws and owned rules separate the environment's authority from evolving behavior. Template instances make mutation structurally valid without requiring arbitrary generated source code to type-check. The fixed schedule makes competition for resources explicit. These decisions give the language a recognizable identity: programs describe environments in which other programs can change.

There are limits worth acknowledging. Valid mutation is not necessarily behaviorally small: a small parameter change can cross a threshold or redirect a contested resource. Conservation prevents impossible accounting but does not establish ecological viability. Restricting evolution to a menu makes its possibilities understandable, but also bounds what it can discover. The proposed general graph and arbitrary templates put pressure on the original straight-line GPU strategy. These are tradeoffs, not reasons to discard the design.

Its biggest opportunity is explanatory. The birth log and attribution counters describe ancestry and gross production. The specification explicitly says these counters do not establish the origin of retained inventory. Even complete provenance would not establish that a mechanism helped survival: producing something, retaining it, and benefiting from it are different claims.

The [body-synthesis investigation](../proposals/problem-body-synthesis.md) shows why that matters. Increasing rot changed both corpse recycling and weather-derived food, and phase order made those changes asymmetric. Removing stealing did not by itself restore synthesis. The latest notes report that recycling, more light, and resident turnover let an introduced producer spread and resist reverse invasion; the [working plan](../proposals/plan-test9.md) records discovery testing as pending. That is meaningful progress. It establishes a reason to investigate discovery separately, rather than continue assuming that selection always rejects synthesis.

The proposed [budget check](../proposals/budget-check.md) remains useful for accounting bounds. A potential coefficient is not automatically an ecological exchange price, however, and maximum yield per attempt does not include encounter rates, contention, turnover, or reproduction timing. An assay would measure a declared intervention in the running ecology, complementing those static calculations.

**The borrowed idea: the control must preserve the surrounding procedure.**

In a sham-controlled experiment, removing the active component while preserving other aspects of treatment can distinguish effects that a treatment-versus-nothing comparison combines. Negative controls serve related diagnostic purposes but are not interchangeable with sham controls. Work on negative controls explicitly identifies assumptions needed for them to detect bias; their existence does not automatically validate a causal conclusion. [Lipsitch, Tchetgen Tchetgen, and Cohen, 2010](https://dash.harvard.edu/entities/publication/73120379-21f6-6bd4-e053-0100007fdf3b).

The computational analogue is immediate. Deleting a gene can remove its function, its upkeep, its action fees, its resource consumption, and its participation in arbitration. A comparison against the deleted gene measures that package. It cannot silently be interpreted as the benefit of the function with its burden held fixed.

Emergent should let the author specify that distinction, check the mechanically checkable parts, and retain the remaining qualifications in the result. “Placebo” is an analogy, not a promise that every useful mechanism has a perfect inert substitute.

**A concrete assay: retaining newly synthesized body.**

The current `build` reaction is:

```text
energy + carbon + nitrate -> heat + body
```

Suppose we want to know whether retaining its output improves a producer lineage's persistence. Compare two branches from each identical checkpoint:

| Branch | What happens to the selected build output | What remains in place |
| :-- | :-- | :-- |
| Retain | Body enters its usual inventory slot | Expression, upkeep, fees, admission, input consumption, heat production |
| Export control | The same admitted body output leaves through an assay-authorized sink | The same code and immediate accounting rules |

This answers a precise question: **What is the effect of retaining this product instead of making it and exporting it under the specified policy?** It does not answer the net advantage of possessing synthesis rather than lacking it. That requires another comparison, where the machinery and its costs may legitimately differ.

Gross production counters still record the synthesized output in both arms; routing it away does not undo production. The primary outcome must therefore measure persistence or another declared consequence, rather than treat production itself as proof of benefit. This first example is chosen for precise semantics. Establishing that an indispensable product helps its producer would not, by itself, be an interesting scientific discovery.

Illustrative syntax:

```text
assay RetainingBody for Dish {
    checkpoints: EstablishedResidents;
    replicate: whole_world;
    target: output(Producer, @build, body);

    arms {
        retain: normal;
        export_control: route_to(void) preserving original_admission;
    }

    match_same_state: [expression, upkeep, fees, input_debits, admission];
    allow_immediate_difference: [target_delivery, sink_ledger];
    mutation: off;
    horizon: 2_000 ticks;
    outcome: living_descendants(Founders);
    inference: paired_worlds;
}
```

`EstablishedResidents` would be an explicitly supplied checkpoint collection; `Producer` and `Founders` would be cohorts selected before intervention. This syntax introduces new constructs, selectors, and semantics. It is not accepted by the current parser. Replication count, checkpoint generation, stopping rule, and analysis method must also be recorded in a complete experiment manifest.

Here, the intervention follows the designated rule instance through reproduction under mutation off. A descendant retains its founder's cohort membership independently of whether it is alive at the endpoint. Extinct founder lineages contribute zero; they do not disappear from the sample. For multiple founders, the statistic must specify whether it counts their union or summarizes founder-level counts. A single world is still one experimental replicate.

The existing `Builder` is a useful positive control, but comparing the full `Builder` and `Drifter` genomes is a comparison of those genomes as wholes. It is not automatically a clean contrast of one mechanism.

**The language change beneath the syntax.**

Rules expose selected **intervention ports** at effect boundaries. The initial implementation needs only a few compiler-recognized ports, such as a free-value write and a reaction output. Arbitrary interception of arbitrary expressions can wait.

For a reaction port, the compiler/runtime conceptually prepares an effect containing the guard, admitted extent, input debits, output credits, costs, capacity checks, and destination. A control substitutes a permitted delivery operation after the original admission calculation. It cannot quietly change the guard or grant extra capacity.

That last restriction is crucial. An unlimited sink would otherwise let the export branch synthesize more whenever the ordinary body slot is full. The control must compute admission using the original destination's headroom, even though it exports the admitted output. It then commits the reaction and the authorized export atomically, before any subsequent transform or condition can read the intermediate output.

This is a real extension of the effect semantics. Adding a generic cleanup rule at the end of Transform would be wrong: another reaction could consume the body before cleanup. Likewise, an owned rule must not acquire general access to sinks. The assay installs a narrowly scoped law-authorized handler; only that handler can export this output to the declared sink.

Every branch has its own ordinary conservation ledger. Cloning a checkpoint creates another simulation instance, not extra matter inside the original universe. No conserved quantity can cross between branches. The control's exported body is counted by composition in its sink ledger. The build reaction still has its original potential drop; export is a boundary transfer, not an invented uphill reaction or an unaccounted deletion.

There is deliberately no universal `make_inert(effect)` primitive. Deleting a node requires a disposal route, redirecting a transfer affects contention, suppressing a split changes topology, and replacing a signal changes its information content. Each supported control has its own explicit contract. Unsupported controls should produce a useful diagnostic rather than an apparently neutral substitute.

**What can be proved, and what cannot.**

Let `P(S, u)` prepare an effect from local state `S` and the relevant random inputs `u`. Let `T` and `C` commit its treatment and control forms. Let `F` select the immediate footprint the author claims to match: for example, input debits, current fees, and admission decisions.

The intended local obligation is:

```text
for every admissible S and u:
    F(T(P(S, u))) = F(C(P(S, u)))
```

Both executions must also preserve the world's invariants, and their immediate state difference must lie within the declared difference set. The identity of the shared input state is essential. In the example, the treatment's local body credit and the control's sink credit differ; that difference is declared explicitly.

This is a relational property of two local transitions. Ordinary effect rows that merely list written slots are insufficient: they do not prove equal debit amounts or equal admission functions. The practical first version should certify restricted built-in transformations by construction. Later versions could use symbolic normalization, bounded verification, or explicit proof obligations. Testing sampled states alone must yield “tested,” not a universal certificate.

After the first intervention, the branches usually have different states. The contract does **not** imply:

```text
total_cost(treatment over 2_000 ticks)
    == total_cost(control over 2_000 ticks)
```

The treatment may reproduce sooner, carry more body, pay more carrying upkeep, reach capacity, or change what neighbors receive. Its later build admission may differ. These are downstream effects of the intervention. Forcing later costs to be equal could add a subsidy or prevent a real consequence, changing the question.

An author requesting exact cost equality along diverging histories would need a different, explicitly cross-world experiment, perhaps using a prescribed external schedule. Such a schedule must still handle affordability and conservation. It should be outside the initial feature.

The checker can certify the construction of a contrast. It cannot certify that the contrast captures the scientist's informal notion of “the benefit of body.”

**Evidence should retain the question it answers.**

An assay should return a structured value with a nominal identity for its experimental specification:

```text
Evidence<
    contrast = retain_vs_export(@build.body),
    matched = same_state(admission, debits, fees, upkeep),
    scope = experiment_manifest_hash
>
```

The manifest identifies the world and template revisions, cohorts, checkpoint protocol, random coupling, backend, intervention duration, mutation policy, outcomes, replication units, and stopping rule. Estimates, uncertainty intervals, local certificates, unresolved assumptions, and failed runs are separate fields. This need not require a general dependent type system: content-addressed manifests and a few opaque evidence types are enough to start.

Such a value cannot be passed to an API expecting evidence for `present_vs_absent(synthesis)` without an explicit new analysis. Two result objects derived from the same worlds cannot be declared independent replicates merely because they were computed by different functions. Dependency records can detect known shared histories; they cannot prove all real independence assumptions.

Statistical conclusions should distinguish support for a prespecified effect threshold, evidence against it, insufficient precision, and an invalid experiment. Exhausting a compute budget is not evidence against the mechanism. A type-correct experiment may be inconclusive. Its statistical assumptions remain assumptions.

A changed physical law invalidates reuse of an old result for the new world by default. A rename preserving identity need not. Broader reuse would require an explicit equivalence argument or another assay. Mutation does not inherit evidence of benefit merely because it inherits a valid template.

**How this differs from running two parameter sweeps.**

The existing `explore` construct already runs variants. It could host much of the assay scheduler, but variants alone do not say what changed or which causal interpretation is licensed.

A useful diagnostic would be:

```text
Control cannot satisfy match_same_state(upkeep).

Disabling @build removes one expressed gene.
That changes the per-gene upkeep charge.

Available interpretations:
  compare the whole expressed mechanism with its absence;
  use a supported output control while retaining expression.
```

Both choices can be scientifically useful. The language should reject only the unsupported matching claim, not forbid the broader experiment. Nor can it prevent someone from writing an overstated sentence outside the language. It can make the accurate interpretation available to reports, tooling, and an AI author.

In a language where programs mutate, this matters more than ordinary unit-test coverage. A program can remain legal, produce body, and fail to establish. The desired output is a tested explanation of that behavior with its scope attached.

**The replication model and random numbers need special care.**

For each experimental unit `i`, run both policies from checkpoint `S_i` and record outcomes `Y_i(T)` and `Y_i(C)`. The basic paired estimator is:

```text
estimated_effect = mean_i(Y_i(T) - Y_i(C))
```

If every pair starts from one checkpoint, this estimates a conditional effect for that state, with variation over the specified future randomness. To generalize to established populations, checkpoints must themselves come from a declared sampling protocol. Consecutive snapshots of one world are correlated, not independent worlds. Likewise, thousands of organisms in one interacting population do not supply thousands of independent replicates.

Starting from recorded simulation state is also different from inferring an unobserved past from observations. The first implementation needs controlled forward experiments; it does not need a general Bayesian abduction engine.

Shared random numbers can improve paired precision, but they are a coupling choice rather than a guarantee. Emergent's Philox naming already helps: unrelated draws need not shift because a sequential PRNG consumed one extra value. However, entity IDs, rule indices, candidate ordering, and draw indices can still change their correspondence after intervention.

For example, the next numeric child ID can refer to descendants of different parents in the two branches. Calling their draws “the same biological event” would be misleading. Begin by documenting exactly which environmental draws are shared, and keep unmatched endogenous draws independent or use a versioned semantic identity scheme. A future scheme might include parent identity, birth opportunity, rule identity, phase, and draw role; even that requires decisions about what a corresponding birth means. Do not change persistent IDs or arbitration order merely to align randomness.

A recent preprint directly discusses using event identities with counter-based generators for counterfactual agent simulations. Event-keyed randomness is therefore prior art, not a novelty claim here. It is also a design whose assumptions must be checked against this runtime. [Buffalo, Pearson, and Klein, 2026](https://arxiv.org/abs/2603.11084).

For an average intervention effect, valid marginal randomness in each arm is sufficient for the paired difference to target the difference of expectations; poor pairing can destroy variance reduction. Claims about the particular alternate fate of an individual require a meaningful cross-world correspondence and stronger modeling commitments. Those are different interpretations.

**Interference, adaptation, and scope.**

A producer changes its neighbors' environment. Treating some individuals and using their untreated neighbors as independent controls can mix direct effects with spillovers. The default should be whole-world assignment: every selected producer in one branch retains output, and its counterpart cohort in the other branch exports it. The measured effect then includes ecological responses. A within-world treatment design would need an explicit exposure model and an analysis appropriate to interference.

Use mutation off initially to study a fixed mechanism. With mutation on, distinguish a one-time intervention from a persistent policy and state whether it follows the original gene identity, all its descendants, or any future rule producing the same substance. These can yield different answers. A frozen selector may miss replacement pathways; a behavioral selector can inhibit discoveries that were never part of the original mechanism.

An assay cannot expose its treatment flag to owned rules. Otherwise evolution may learn a special response to the experiment label. Organisms may still sense the physical consequences of treatment; removing that would suppress the mechanism being studied. In small or rapidly mixing worlds, interference is inherently global, so a “local causal cone” optimization must not silently freeze the rest of the world.

Do not feed assay scores automatically into division or survival. That would introduce explicit fitness pressure into a system currently designed around resource-mediated selection. Reports remain in the observer/author context. An author can deliberately use results to design a new experiment or world revision, with that action journaled.

**What the feature would make possible.**

First, it would turn the synthesis investigation into several distinct executable questions. Does the complete machinery improve invasion under the new resource regime? Does retention of its output explain some of that advantage? Does corpse disposal change the result? Does a nitrate-uptake change help only in combination with a body-producing reaction? Does mutation find a viable combination often enough?

A four-arm factorial experiment could compare neither component, uptake only, synthesis only, and both. An interaction contrast is `Y11 - Y10 - Y01 + Y00`, using a declared outcome scale. A positive interaction is evidence of complementarity in that context, not a universal fitness value for either gene. The controls and burden changes must be specified for each arm.

Second, it could detect misleading accounts of emergent complexity. An extra module may persist without contributing; deleting it may help only because of upkeep. A signal network can look sophisticated while carrying useless information. An intervention report could separate these hypotheses without equating graph size with adaptive complexity.

A signal control illustrates why the matching contract must be limited. Replacing a sensor value with zero can preserve its gene's immediate upkeep while changing downstream activation rates. Shuffling readings might preserve a marginal distribution while destroying temporal structure. Neither operation should receive a general certificate saying that only useful information was removed. The author must state the specific transformation and the properties actually checked.

Third, the same infrastructure could help with supply networks, congestion, and distributed policies modeled in the core. A relay's benefit and its maintenance burden can be separated by an appropriate declared contrast. Whether a convincing control exists depends on the model; the language should make that design work visible.

The most ambitious outcome is a program that carries an executable argument for why some of its mechanisms matter. That would make Emergent more useful for scientific explanation and automated investigation even if it did not make evolution itself more open-ended.

**Failure modes and limits of the proposal.**

| Problem | Consequence | Design response |
| :-- | :-- | :-- |
| No physically admissible control | Removing one effect necessarily changes another | Require a broader named contrast or reject the requested matching contract |
| An unlimited sink bypasses capacity | The sham changes reaction throughput immediately | Preserve original admission before output routing |
| Delayed interception | Other rules consume the supposed suppressed output | Commit intervention atomically at the declared phase boundary |
| Feedback changes later costs | A “cost-matched” label overstates what was checked | State same-input local matching; report realized cost divergence separately |
| Export changes nutrient availability | Outcome includes resource removal and recycling changes | Name the retain-versus-export contrast; use additional controls for other hypotheses |
| Many interacting samples | False precision from counting entities or ticks as replicates | Track whole-world and checkpoint ancestry in the analysis manifest |
| Optional stopping or adaptive search | Attractive results are overselected | Separate exploration from confirmation; freeze the confirmatory design or use justified sequential inference |
| Short observation horizon | An early benefit hides later collapse | Declare the horizon; examine prespecified later outcomes and distributions |
| Altered RNG correspondence | Apparent individual counterfactuals become arbitrary | Version coupling; distinguish average effects from individual alternate histories |
| Evidence reused after mutation or hot-swap | Old support is attributed to a new mechanism or ecology | Bind results to exact revisions and intervention selectors |
| An incorrect simulator | Precise experiments repeat an implementation error | Maintain an independently checked semantic kernel and invariant audits |
| New semantic machinery overwhelms the core | The language becomes harder to implement and understand | Start with certified built-in ports; keep scheduling and statistics in tooling |

There is an additional compositional problem. Two locally valid controls need not remain matched when applied together: they can share a balance, a capacity, a destination, or an admission decision. Initial assays should reject overlapping ports, or require a joint contract. It would be unsound to infer compositional independence from disjoint source labels alone.

Nor is simulation evidence evidence that the modeled biology or economics is true. The effect is causal within the specified executable model. Mapping it to the outside world requires separate model validation. Confidence intervals address a declared sampling process, not every source of model error.

**Prior art and the actual novelty boundary.**

The search included counterfactual programming, Kappa causal analysis, typed causal abstractions, causal profiling, experimental-design languages, placebo program transformations, statistical verification, and event-coupled simulation. The following are the most relevant sources, rather than a claim of exhaustive coverage.

| Work | What it already contributes | Consequence for this proposal |
| :-- | :-- | :-- |
| [Laurent, Yang, and Fontana: Counterfactual Resimulation for Causal Analysis of Rule-Based Models, IJCAI 2018](https://fontana.hms.harvard.edu/sites/fontana.hms.harvard.edu/files/documents/ijcai18.pdf) | Semantics and a Kappa implementation for counterfactual traces, including enablement and prevention | The closest structural precedent; counterfactual explanation of graph rules is already established |
| [Tavares et al.: A Language for Counterfactual Generative Models, ICML 2021](https://proceedings.mlr.press/v139/tavares21a.html) | Omega's counterfactual inference operator and formal semantics | A language-level “what if” operator would not be new |
| [ChiRho project](https://github.com/BasisResearch/chirho) | Intervention and counterfactual effect handlers, world splitting, and dynamical-system support | Effect handlers and multiple simulated worlds are implementation foundations, not novelty |
| [Wang et al.: Typed Abstractions for Causal Probabilistic Programming, 2026 manuscript](https://theo.wang/files/works/2026_01_LAFI.pdf) | Typed intervention points and a Haskell presentation of causal multivalues | “Make interventions typed” is also too broad a novelty claim |
| [Çiçek et al.: Relational Cost Analysis, POPL 2017](https://people.mpi-sws.org/~dg/papers/popl17.pdf) | A refinement type and effect system for differences between execution costs of related programs | Relational checking of cost differences is an existing foundation; the new application would concern model-level intervention burdens and their experimental interpretation |
| [Kawamoto, Kobayashi, and Suenaga: StatWhy, CAV 2025](https://arxiv.org/abs/2405.17492) | Verifies statistical testing programs against specified requirements, including assumptions that cannot themselves be verified | Formal checks on statistical reasoning are prior art; assumptions must remain visible |
| [Curtsinger and Berger: Coz, SOSP 2015](https://github.com/plasma-umass/coz) | Runtime experiments estimate the performance impact of virtual speedups | Experimental causality is already useful in programming tools |
| [NetLogo BehaviorSpace](https://docs.netlogo.org/behaviorspace.html) | Integrated repeated experiments and parameter sweeps | Experiment orchestration alone does not distinguish the proposal |
| [From Compression to Forward Transfer: Evaluating Reusable Knowledge in Program Synthesis](https://corbensorenson.github.io/asi-stack-book/papers/forward_transfer_program_synthesis.html) | An author-published framework explicitly proposes matched placebo abstractions and full cost accounting | Even applying placebo comparisons to program components has been proposed; this source does not establish an implemented language feature or peer-reviewed validation |

These sources materially narrow the proposal. I would not claim to have invented counterfactual programming, typed causality, relational cost analysis, or computational placebos. The research candidate is a **relational effect contract that checks a control's specified immediate burden and legal resource flows, then carries that contract into the interpretation of an experiment**. The sources examined did not demonstrate that exact integration in an evolving, resource-conserving graph language.

Before making a publication-level priority claim, compare the formal rule directly against Kappa's intervention semantics, the ChiRho typed interface, and relational program logics. Proving that the feature is useful requires a prototype and user examples; an absence of a matching search result is not such proof.

**A deliberately small implementation path.**

The CPU reference `World` already derives `Clone` in [world.rs](../crates/emerge-ref/src/world.rs), which offers a plausible start for isolated experimental copies. This does not establish a persistent checkpoint format, causal random coupling, or an existing assay API. The birth log and attribution machinery are also not all implemented; the [runner notes](../crates/emerge-ref/DISCREPANCIES.md) identify remaining gaps.

1. Define the local relational contract and one reaction-output handler. Support normal delivery and export through an existing declared sink, preserving original admission. Begin with mutation off and nonoverlapping ports.
2. Add isolated CPU branches, preintervention cohort labels, exact manifest identities, and explicit failed-run records. Reuse existing effect and ledger semantics.
3. Return raw paired outcomes and the local contract certificate before adding automated significance labels. Make the contrast name and its qualifications unavoidable in generated reports.
4. Add a small inference layer with explicit replicate ancestry, fixed stopping rules, and effect thresholds. Reuse an existing statistics library rather than designing statistics as part of the simulation kernel.
5. Only after concrete investigations benefit, add syntax, other ports, mutation-on policies, and GPU batching. A sophisticated dependent type system is not a prerequisite.

Useful acceptance tests would target failure modes rather than mirror implementation: two identical arms must produce identical deterministic runs; a disabled gene must fail an upkeep-matching claim; original-capacity admission must hold when the sink has unlimited room; intercepted body must be unavailable to the next transform; all ledgers must balance; interventions on a blocked reaction must have no output effect; a stale manifest must not authorize reuse; and overlapping controls must require a joint contract.

The scientific evaluation should use small worlds with deliberately constructed mechanisms, including a costly but useless gene and two complementary genes. It should then test whether the assay reproduces a manually designed synthesis experiment and rejects the misleading control. No expected numerical outcome for the real Dish is assumed here.

Compute cost is significant. With `r` replicate pairs, `h` ticks, and a full-world tick cost `C`, a two-arm assay costs about `2*r*h*C`, plus state copies and analysis. Copy-on-write helps initially but frequent mutation of dense arrays can erase its advantage. Batched GPU worlds help throughput, not the number of causal questions that must be evaluated. Localized re-simulation is valid only with a defensible dependency boundary; contention and diffusion can quickly expand it to the full world.

The proposal earns its place if it catches real experimental mistakes and produces useful, precisely scoped explanations without burdening ordinary execution. If the same guarantees are easily obtained with a small library over `explore`, retain that library and avoid new syntax. The semantic contract is the contribution worth testing.
