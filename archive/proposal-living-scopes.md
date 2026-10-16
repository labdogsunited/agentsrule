**Proposal: living scopes — make the unit of ownership evolvable**

Design and prior-art research, 2026-10-16. Proposed semantics and illustrative syntax only; no implementation or experiment is claimed.

Give Emergent an operation through which several nodes can establish a persistent, resource-funded scope. The scope owns a rule set, a treasury, and precisely bounded authority delegated by its members. It can survive the loss of its founders, reproduce with variation, and dissolve without necessarily killing its members.

The inspiration is institutional economics and incorporation: an organization can persist, hold assets, and make decisions even as its participants change. Ostrom's work on self-organized governance is particularly relevant to the idea that arrangements for managing resources can themselves be objects of investigation. This proposal borrows that question; it does not claim that one institutional arrangement is universally best. [Ostrom's Nobel interview](https://www.nobelprize.org/prizes/economic-sciences/2009/ostrom/164465-ostrom-williamson-interview-transcript/).

The language-level idea is that **a scope becomes something a program can construct, sustain, reproduce, and lose**. Its existence is part of the simulation, alongside the existence of organisms. This changes what can evolve rather than how an existing run is analyzed.

Important novelty qualification: recursive agents, executable institutions, resource-funded groups, and transitions in digital individuality have all been studied. I found no exact match for the complete semantics below, but several predecessors are close. The proposal is a candidate integration, not a defensible claim to have invented collective agents or evolving governance.

**Why this belongs in the discussion of the core.**

[core.md](../core.md) currently assigns an owned rule set to one node. A rule may rewrite its own node's rules and those of a node it creates; laws have broader authority. The graph already supports cooperation, resource sharing, and multicellular arrangements. A new scope is therefore not necessary just to represent a group.

What is missing as an explicit contract is the ability for existing, independently owned programs to establish a new durable owner of selected resources and permissions. A set of bonded nodes does not by itself say who can make a binding allocation decision, how that decision survives a member's mutation, or what happens to the arrangement when its coordinator dies.

A world author could implement much of this using trusted laws and special node kinds. The proposal earns native support only if the compiler can enforce useful guarantees that would otherwise be reimplemented and audited in every world. It is an ownership and lifecycle abstraction, not a claim about greater computability.

**One concrete example.**

Suppose a world allows energy specialists and nutrient specialists. Both kinds can live independently, but each encounters a bottleneck the other can alleviate. A pair could establish a scope that holds contributed resources and allocates them under a shared rule set. Its rules might accumulate reserves, fund one member's reproduction, or help the member currently short of energy.

Members do not receive an automatic cooperation bonus. Sharing must pay after the costs of transport, administration, and delayed access to contributed resources. A badly governed scope may go bankrupt, lose members, or be outcompeted. A useful one may recruit members and establish descendants with related allocation rules.

This does not mean the existing Dish already contains the necessary ecology. The first experiment should construct complementary resource constraints deliberately and then ask whether scopes help. It should not assume that adding an organizational primitive fixes test 9.

Illustrative syntax:

```text
scope MutualPool {
    members: Critter;
    capacity: 16;
    treasury: [energy, nitrate];
    owned rules (instances: 16, signals: 8, modules: 4);
    uses: [reserve, allocate, admit, bud];

    administration {
        pay: energy;
        base: 2mu;
        per_member: 1mu;
        becomes: heat;
        vent: void;
    }

    admission: explicit_acceptance;
    authority: contributed_inventory;
    exit: at_lease_end;
    on_failure: dissolve;
    on_dissolve: residual_by_share else void;
}
```

These numbers are illustrative. This is not current `.em` syntax, and the primitive menu names stand for library templates that would need specification. The universe author declares the available scope kinds and their limits. Evolving nodes choose whether to instantiate or join one and evolve the scope's permitted policy parameters and rule instances.

In the smallest useful version, a scope controls only resources already contributed to its treasury. There are no promised future payments, unsecured debts, or authority to rewrite another node's arbitrary code. Richer delegated control can be added only when its semantics are justified.

**What a scope actually contains.**

A scope has a persistent identity, a governor node with an owned rule set, membership records, a physical treasury, and a lifecycle state. A membership record identifies the admitted node, the accepted policy/grant version, its lease endpoint, and any residual claim. Nested scopes could be members later; the first implementation should be flat.

The treasury is actual inventory in the graph. It is not a virtual sum of every member's holdings. A contribution debits one real store and credits another through the ordinary transfer mechanism. Membership does not teleport materials, create a transport edge, or bypass capacity and arbitration.

Similarly, membership does not grant global vision. The governor reads only information permitted by declared interfaces and the world's locality rules. A distant member's private signals remain private unless an explicit interface makes them accessible. Otherwise organizational size would confer free sensing and communication advantages.

A scope's policy is expressed through the same bounded templates as ordinary owned rules. It has no source access and cannot repeal conservation, dissipation, cardinality, or the global schedule. The author remains responsible for the physical laws; the scope has authority only over its own treasury and any explicitly granted effect rights.

**Authority must not multiply when code does.**

This is the most important semantic distinction. A rule-set copy is information. A grant to control an existing resource is not merely information.

For an exclusive grant concerning target slot `s`, the invariant would be:

```text
number_of_current_holders(exclusive_grant(s)) <= 1
```

Creating a new node can create legitimate authority over that new node's own slots. Therefore the global number of rights is not a conserved physical quantity. The guarantee is nonduplication of authority over an existing target, and no escalation beyond the rights the issuer can delegate.

Copying a policy copies neither balances nor grants. A child scope must receive real resources and new or transferred membership grants. A governor that reproduces cannot thereby acquire two independently spendable claims on the same treasury.

The initial treasury-only design gets much of this behavior through ordinary inventory separation. If future versions add delegation over member actions, the restriction must attach to semantic effect permissions, not gene names. Otherwise a member could mutate a second gene that performs the same action and bypass its promise. Changing a label or rewriting local code must not mint authority.

Universal laws still apply. A contract does not confer physical invulnerability: if the world's laws permit an outsider to steal from a treasury, membership does not magically stop that. Defense would require ordinary mechanisms. Likewise, scope policy cannot command a nonmember simply because that would improve collective performance.

**Formation, exit, and failure are part of the abstraction.**

Formation should be a bounded atomic restructuring operation. Candidate members accept the exact scope identity and grant version. Formation requires their admission conditions, required edges, capacities, and the world node limit to remain valid when it commits. Simultaneous incompatible applications are resolved deterministically; a node cannot accidentally make the same exclusive grant to two scopes.

Because current owned rules create only by copying or splitting their own node, establishing a new governor requires explicit world authorization. A trusted standard-library law could settle joint formation requests. The feature must not quietly turn every owned rule into an unrestricted node factory.

Initial resources should be contributed through ordinary Move operations to a pending governor or escrow store. Pending objects need a bounded lifetime and explicit refund or disposal routes. Activation in Restructure consumes the recorded acceptances atomically. A failed activation cannot strand resources indefinitely. This avoids inventing an unpriced resource transfer inside an arbitrary graph rewrite.

Contributions to the spendable treasury are not guaranteed refundable deposits. If members want a guaranteed refundable bond, it must be separately earmarked and unavailable for ordinary spending. A residual claim on a treasury is a claim on whatever remains, not a second copy of that inventory. Split or transfer of claim shares must preserve the total existing entitlement; reproduction cannot manufacture extra redemption claims.

Leases define when delegated authority expires. In the first version, expiration restores access to delegated rights at a tick boundary and creates no new liability. Members can leave while a scope continues. A more ambitious model can allow irreversible commitments, but that is a world-level option with different evolutionary consequences.

When the scope cannot pay administration, it stops acting and dissolves under its declared route. Its members can remain alive and resume independent behavior. An empty treasury should not erase them, while a surviving member should not keep a dead governor's authority alive accidentally.

The disposal route distributes remaining assets under the fixed residual-share rule, subject to ordinary destination capacity and fallback sinks. Claims belonging to dead or unavailable recipients need a declared fallback. The first implementation should have no unsecured creditors: otherwise insolvency ordering becomes another large semantic subsystem.

**Fit with the tick pipeline.**

Freeze membership and grant validity at the tick snapshot. Compute member and governor requests from that snapshot under their respective rule sets. Settle governor administration as an ordinary declared charge before its resource moves. A governor unable to pay is failing; its owned requests are dropped as they are for other failing nodes.

Transfers from the treasury still enter the same arbitration as every other transfer. Transform does not gain special group chemistry. Restructure commits membership changes, dissolutions, and successful offspring creation, with effects visible in the next tick.

Restoring a member's expired or revoked authority halfway through Move would create a second chance to issue effects and complicate replay. Avoid that: lifecycle changes take effect at the boundary. Scope failure must not mean that two different owners can spend the same inventory during one tick.

For an initial implementation, activation, dissolution, and budding involving overlapping members or grants should be mutually exclusive in a tick and settled by a specified persistent-ID order. Scope deletion should reuse the core's deletion routes where possible; additional grant revocation and claim settlement must be part of its atomic change. No dangling membership should depend on a deleted owner.

Administration here is an explicit simulation cost, not the measured CPU time of enforcement. It must be paid using a valid conversion and sink route. A world may make it zero, but that makes organizational authority cheaper and changes the model. Distinguish that choice from a claim that real coordination costs exactly so many energy units.

**How inheritance could work without copying a whole organization for free.**

A scope can reproduce by budding: partition an authorized subset of members into a new scope, move a specified share of treasury assets, and copy the governor's rules with permitted mutation. Membership grants must explicitly allow such succession, or each affected member must accept the new governor.

Both parent and child must have valid memberships, required edges, capacities, and funded governors. Failure is atomic: no half-created child, double-assigned member, or duplicated balance. Any attempt fee follows the ordinary fee rules and is not confused with resource allocation to the offspring.

The policy information can be copied. Actual members must be partitioned or created by their own legitimate reproduction. Treasury amounts must be partitioned or separately funded. Authority over existing objects must move or be reissued by a valid issuer, never be duplicated by policy copying.

This gives the scope a lineage that differs from the lineage of any member. A scope can survive member replacement; one member lineage can participate in several successive scopes. The birth log would therefore need separate ancestry for member reproduction, governor reproduction, and membership succession. A nested membership tree alone is not an evolutionary family tree.

Mutation initially happens at budding. Live amendment can come later. An amended policy must remain within existing grants and accepted bounds; a mutation that requests more authority is a proposal requiring new acceptance, not an automatic constitutional privilege. That preserves the meaningful distinction between changing a program and acquiring control over somebody else's resources.

**Why this could matter for open-endedness.**

The current substrate supports evolving behavior within an owner. Living scopes add a way for the identity of the owner to change through endogenous formation and succession. A resource-sharing arrangement can become a persistent, inheritable program even if no individual member contains all of its behavior.

Possible outcomes include specialization, common reserves, delayed reproduction, selective admission, freeloading, exploitative governors, and organizational collapse. None should be hard-coded as a success. Survival, recruitment, and budding must arise from the world's actual constraints.

This is a mechanism for studying transitions in organization; it is not evidence that true biological individuality will emerge. A treasury with several members is not automatically an organism. Stronger claims would require coordinated reproduction, heritable collective traits, and evidence about the relevant selection processes.

Outside artificial life, the same abstraction could model resource pools, autonomous service consortia, cooperative logistics, or capacity-sharing networks. Those applications would need their own laws for transport, contracts, and failure. Host-system permissions should never be inferred from simulated authority.

**Problems that could make the proposal fail.**

The largest conceptual risk is giving agents a powerful cooperation mechanism and then mistaking its consequences for an unexplained emergence of cooperation. Binding commitments and enforceable treasuries are part of the substrate. Experiments must say so and compare them with ordinary voluntary sharing under equal resource conditions.

There is also a danger that the mechanism merely subsidizes large organizations. A fixed administration fee amortizes over more members; free communication and global access would make growth even cheaper. Use costs that expose membership and communication burden, restrict transport and sensing normally, and test several cost functions rather than tuning one to obtain a desired outcome.

| Failure mode | Why it matters | Initial response |
| :-- | :-- | :-- |
| Scope ownership bypassed by mutated aliases | Commitments become meaningless | Enforce permissions on target resources/effects, independently of rule labels |
| New members or births multiply voting and payout claims | Reproduction becomes a control exploit | Do not make one-node-one-vote or one-node-one-share a hidden default; partition existing entitlements explicitly |
| Policy mutation expands old grants | Evolution can manufacture authority | Freeze accepted bounds; new authority requires fresh acceptance |
| Free or unlimited nesting | Organizational depth becomes a cheap source of power | Start flat; later cap depth and charge actual declared operations |
| Cyclic ownership or leases | Control resolution and dissolution can deadlock | Start with exclusive ownership and an acyclic scope relation |
| Treasury protected from outsiders without a mechanism | Membership quietly becomes a physical shield | Apply ordinary world interaction laws to treasury stores |
| Death, exit, and budding overlap | Assets or rights can be lost or duplicated | Atomic lifecycle transitions, explicit ordering, conflict rejection |
| Abstract membership creates transport shortcuts | Large organizations defeat spatial physics | Separate membership from transport and communication edges |
| All policies converge to dictatorship | The feature may have a narrow attractor | Measure it as a model outcome; test ecology and costs without inserting an unannounced fairness objective |
| Complexity overwhelms GPU execution | The system loses its original performance advantage | Bounded membership, flat scopes, template-based governors, CPU semantics first |

Even correct enforcement does not make a contract attractive or fair. An agent can voluntarily accept a terrible arrangement because its evolved decision rule is poor. “Acceptance” is a simulated transition, not a claim about human consent or political legitimacy.

**Research: close predecessors, not just distant analogies.**

| Source | Established overlap | What it means for novelty |
| :-- | :-- | :-- |
| [SARL agent reference](https://www.sarl.io/docs/official/lang/aop/Agent.html) | Agents can contain other agents through contexts; holonic composition is native | A collective agent or recursive scope is not new |
| [Jamali and Zhao, Scalable Hierarchical Coordination of Multi-Agent Resource Usage](https://agents.usask.ca/Papers/jamali-zhao-mmas04.pdf) | CyberOrgs combine groups, resources, control hierarchies, isolation, and assimilation | This is especially close; budgets and control transfer do not establish novelty |
| [Minsky and Ungureanu, Law-Governed Interaction, 2000](https://doi.org/10.1145/352591.352592) | Explicitly specified interaction laws enforced through decentralized controllers | Executable institutional rules have a long history |
| [Move resources](https://move-language.github.io/move/structs-and-resources.html) | Resources can disallow copying and dropping and require ownership transfer | Nonduplicable assets and ownership-transfer guarantees are established techniques |
| [Moreno and Ofria, DISHTINY, 2019](https://mmore500.com/pubs/moreno2019toward) | Digital organisms can unite into higher-level individuals, with mechanisms intended to support subsequent transitions | Evolution of collective individuality itself is not a new target |
| [Kumar et al., Evolving Interpretable Constitutions for Multi-Agent Coordination, 2026](https://arxiv.org/abs/2602.00755) | Genetic programming evolves natural-language constitutions for LLM agents against a societal objective | “Let constitutions evolve” is already being investigated; this proposal instead specifies executable ownership and resource semantics within the evolving world |
| [P systems with mutative membrane structures, 2025](https://doi.org/10.1016/j.ic.2025.105277) | Computational compartments can bud, fuse, divide, and dissolve | Dynamic boundaries and compartment reproduction are also prior art |

The plausible distinction is the combination of heritable scope policies, administration paid from the same conserved resources as the population, and succession that partitions assets and existing authority while allowing members to survive scope death. I did not find the complete combination in the sources examined. Because CyberOrgs and digital-individuality research are close, the honest claim is a potentially useful language synthesis whose novelty still needs formal comparison, not an entirely unprecedented paradigm.

**A small experiment before changing the language.**

Implement a treasury-only scope as a standard-library prototype on the CPU runner: flat membership capped at 16, explicit contributions, one governor, fixed-duration membership, ordinary resource moves, and atomic dissolution. Add budding only after assets and membership transitions have unambiguous semantics.

Construct a small world with complementary resource access and compare independent agents, voluntary resource exchange, and funded scopes. Hold physical capabilities and total initial resources constant. Measure member survival, scope lifetime, transport and administration costs, specialization, recruitment, budding, and within-scope conflict. Record negative results; do not introduce a group reward just to make scopes spread.

Meaningful correctness fixtures include a duplicated authority attempt, simultaneous joins using the same grant, an empty treasury during a lifecycle transition, a member dying during dissolution, full payout destinations, a rejected bud, and a policy mutation requesting more authority than its grants allow. Every fixture must preserve the resource ledger and leave each exclusive grant with at most one valid holder.

If scopes repeatedly produce useful organization that ordinary sharing cannot sustain under the same conditions, and the ownership contract substantially simplifies world code, promote the abstraction into the language. If they are merely a complicated spelling of a controller node with no additional enforceable guarantees, keep them in the library.
