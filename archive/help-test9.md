# Offer of help: finding what prevents producers from establishing

Response to [briefing-test9.md](briefing-test9.md), written 2026-10-17. I also read [plan-test9.md](plan-test9.md), [theft-consequences.md](../proposals/theft-consequences.md), and [problem-body-synthesis.md](problem-body-synthesis.md), and checked the current dish and relevant runner code. This is an experimental proposal based on that evidence, not a literature review. I have not run new simulations. The plan still records the `delta` experiment as pending.

I can help most by separating **discovery, establishment, and persistence** before adding another mechanism. Your strongest result is the passing seed with priced theft: producers establish before theft spreads, but remain at 60–67% after thieves reach 98%. That shows synthesis and theft can coexist for the observed interval. It does not yet show that a rare producer can invade an established thief population, or that the resulting community is stable over longer runs.

My recommendation is to investigate that asymmetry first, alongside one inexpensive change to contact range. I would keep the existing exit test unchanged during this investigation.

**What the passing seed tells us.** It is evidence for a possible order-of-arrival effect, not sufficient evidence that raising the price will reliably solve the problem. Both explanations remain plausible: the price permits coexistence once production is common, or this particular lineage and its ecological surroundings happen to support synthesis. Also, `test9n` and `test9p` both report seed 9 under the same configuration; deterministic repetition of that success is not an independent success.

The most useful follow-up is a pair of invasion experiments using an evolved producer from the passing run:

- Introduce it at low frequency into an established thief population from a failed run, with mutation off initially. Compare it with a matched variant whose body-producing reaction is disabled.
- Introduce that synthesis-disabled variant at low frequency into an established population of the successful producer. Measure whether losing synthesis spreads.

Preserve the reaction gene's upkeep in the disabled control if the assay permits it; otherwise report the upkeep difference. Use several resident snapshots, and account for any material introduced with the invaders. These are diagnostic assays, separate from the unmodified exit test. The checkpoint-and-branch tooling already proposed in the plan would make them much easier.

If producers fail only when rare, establishment is the immediate target. If synthesis-disabled variants invade and replace established producers, persistence is also unresolved. A favourable result in both directions would justify studying why mutation rarely assembles the successful phenotype. Compare surviving descendants over several generations, not just reaction flux or immediate births.

**Check resource dependence before attributing each collapse to theft.** Seed 9's burst producers mostly take sugar made elsewhere and almost never photosynthesise. Their decline could involve the disappearance of their suppliers, alongside theft. The no-theft control implicates the theft regime, but does not identify the immediate cause of every producer decline.

I can help add an optional accounting report for body production, body theft, energy income and expenditure, sugar and nitrate uptake, and deaths. Track successful transfers separately from active `steal` genes: the runner's `thieves` column measures gene presence, not successful theft. Compare local input availability around producers as well as world averages. A sugar shortage near producers can be hidden by abundant sugar elsewhere. First finish the already-proposed sugar trace; expand instrumentation only where it leaves the explanation unresolved.

**The cheapest dish change I would try is a shorter interaction radius.** The checked-in dish exposes `max_interaction_radius: 4.0`, so this does not require a new language feature. Copy the actual recalibrated experimental dish: the checked-in example still has different sunlight and age settings.

As an illustrative calculation, 33,000 uniformly and independently placed entities in a 512 × 512 world give about 6.3 neighbours within radius 4, but only 0.40 within radius 1. The corresponding Poisson estimates for having any neighbour are about 99.8% and 33%. These are geometric estimates, not predictions for your clustered, moving populations. Measure actual neighbour availability and repeated victim contacts.

Try radii 4, 2, and 1 with all other parameters fixed. Check which senses and contact actions share this radius, because reducing it can affect information and giving as well as theft. A useful result would be improved producer establishment while successful theft remains present. If theft effectively disappears, this is another no-theft control, not evidence of coexistence under meaningful exposure.

Charging every fired contact attempt is also worth testing as a general semantic correction, consistently for `give` and `steal`. Its additional effect is limited to attempts without a target, so it may change little in dense populations. Keep that change separate initially; combining it with a smaller radius would obscure which mechanism helped. A small factorial comparison can follow if either result is promising.

**Other mechanisms to consider are private returns, limited exploitation throughput, and complementary metabolism.** These are hypotheses for this world, not claims that any one is already present:

- A producer can benefit if it converts its product into reproduction before others can remove it. Audit the existing action, reaction, and division order to establish how much opportunity this permits. Do not reorder phases merely to obtain a pass.
- A thief may be limited by encounters or transfer capacity even when victims remain accessible. Compare actual extraction per victim with production and reproductive use. Several thieves targeting one victim could overwhelm a per-action cap.
- Synthesis might become advantageous as part of a reaction that also supplies something the producer needs immediately. The generated multi-output reactions make this worth examining in successful genomes. Do not add a hidden reward for carrying synthesis genes.

None guarantees persistence under unrestricted access. If non-producers can obtain all the same benefits while avoiding synthesis costs, another source of individual advantage must be demonstrated. Repulsion alone does not demonstrate it: displaced neighbours can remain within stealing range. Before implementing space changes, specify and measure the expected reduction in contact duration or repeated extraction.

**The test can distinguish capability from ecological robustness without lowering its standard.** Keep the current 2%/10,000-tick criterion as the existing benchmark. Alongside it, report time to first production, time to establishment, subsequent persistence, and extinction separately. A run that discovers synthesis near its deadline has a different failure from one that repeatedly loses established producers.

If you decide to revise the test, one defensible alternative is a fixed discovery deadline followed by a full 10,000-tick retention period after a predefined establishment threshold. Specify both before running; this changes the time allowance and should be reported as a new test version. Retain a separate test for persistence under theft.

The producer metric also deserves an explicit interpretation. The current runner counts entities alive at the window boundary that produced during that window; newborns start with no production credit, and dead producers are absent. That is a legitimate measure, but not the fraction of all entities that produced during the interval. Add production per entity-time and lineage reproduction as diagnostics before changing the pass criterion. Positive gross synthesis alone does not establish a reproductive benefit.

I can help turn this into a reproducible experiment package: frozen dish configurations and manifests, a small radius comparison, accounting that does not alter simulation behaviour, and the two invasion assays. Start with existing successful and failed runs for diagnosis, then evaluate the chosen intervention on a fresh, predefined seed set. The first deliverable should explain which stage fails and why; that evidence can justify a price adjustment, a spatial mechanism, or a revised test.
