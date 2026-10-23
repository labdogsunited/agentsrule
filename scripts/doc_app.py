"""Guide topics for using the app (the browser viewer, the command line, the engine API).

Read by generate_docs.py next to the language guide in doc_data.py. Each topic is markdown in the same
dialect the language guide uses. `scripts/check_docs.py` checks that every editor the viewer has is named in
these pages, so adding an editor without documenting it fails the check.
"""

SECTION = "Using Emerge"

APP_GUIDE_TOPICS = [
    {
        "id": "using-overview",
        "section": SECTION,
        "title": "Using the viewer",
        "summary": "Start the viewer, create a world, and find your way around the panes, desktops and keys.",
        "content": """# Using the viewer

The viewer is a page in your browser, served by the engine on your own machine. A world is a `.em` file; the viewer starts one from an example, runs it, shows it, and lets you change it while it runs.

## Start it

```sh
agentsrule web
```

This builds what it needs, starts the server on the first free port from 7878, prints the address, and opens your browser. The address carries a key: open the address the server printed, not just the port. **Restart server** starts it again (worlds in memory are lost); **Stop server** stops it. `agentsrule web --port N --no-open --journal-dir DIR` changes the port, keeps the browser closed, and keeps a journal of every world so it can be replayed. What outlives the server (each world's network organisation and view document by the world's name, the template and operator library) is kept in `--state-dir DIR`, by default `~/.local/share/emerge`.

## The header

| Control | What it does |
| :--- | :--- |
| **World** | the running world every pane acts on; several can exist at once |
| **New world…** | pick an example (the reference dish, a preset, Game of Life), the lineages that start in it, the seed, the size, and the backend (CPU or GPU) |
| **Desktop** | a saved arrangement of the panes; **Save as…** keeps the present one under a name |
| **Docs** | opens this documentation (also **F1**, which opens the page for the pane you are in) |

## Desktops and panes

The window is split into panes, and each pane holds tabs. Drag a tab to move it, drag a divider to resize, and use the pane's menu to split it or make it fill the window. Five desktops come with the viewer:

| Desktop | For |
| :--- | :--- |
| **Simulate** | watching and running a world |
| **Rules** | editing a world's rules as a network, with the source and the inspector beside it |
| **Genome** | one creature's genes and signals |
| **Science** | the panels that answer questions about a run |
| **Track** | following creatures: the Viewport with Details under it, the Inspector, the Network (for the Observe context) and Watches (see *Tracking creatures*) |

## The panes (editors)

| Editor | What it shows |
| :--- | :--- |
| **Viewport** | the live picture of the world: wheel zooms, drag pans, a click selects a creature (see *Tracking creatures*), **i** picks what is under the pointer (cell contents, nearby creatures, open a creature's genome), **g** shows the cell grid |
| **Parameters** | the box selected in the Network: its parameters, its text, and one edit for several selected boxes |
| **Network** | the world's rules as boxes and wires (see *The network editor*) |
| **Genome** | one living creature's genes in module frames, with the signals between them as wires |
| **Look** | how the picture is drawn, as a network of the view's pieces with typed sockets, previews and a display flag (see *The Look network*) |
| **Details** | a table of agents: an even sample, all of them paged, the selection, the tracked ones or one tracker's members, with a condition the server evaluates; click selects, double-click opens the genome, the star tracks |
| **Inspector** | what the selected box *is*, what the world holds in it, and what the runner counted for it, kept apart (see *The network editor*); for a selected creature, one exact reading of it, which can be pinned; for a selected cell, what it holds and what touches it |
| **Source** | the complete source of the running world; edit a copy, open a file, make a new world from it |
| **Rule Lens** | the checked schedule: the steps of one tick and the rules and statements in each; also a read/write matrix, per-statement activity and a trace |
| **Lineage History** | population by lineage over time; drag on the chart to brush a span of ticks |
| **Genome Census** | every living creature's active genes by lineage, and a comparison of two creatures |
| **Resource Budget** | where the upkeep substance came from and went, by lineage, closed to the integer µ |
| **Ancestry** | one creature's ancestors and descendants |
| **Compare Runs** | two worlds side by side, on the ticks both sampled |
| **Region** | creatures and amounts inside a rectangle of the world, read in one call |
| **Flow Map** | the reactions of the world: declared, measured, and which outcomes blocked them |
| **Life Histories** | ages and carried stock of the living, and one birth cohort |
| **Notebook** | bookmarks that carry copies of their evidence, and a timeline of edits |
| **Watches** | every tracker of the world: watches and sets, their recordings, exports and nodes (see *Tracking creatures*) |

## Keys

| Key | Where | Does |
| :--- | :--- | :--- |
| Space | anywhere | run or pause the world |
| i | Viewport | the info picker |
| click | Viewport | select the creature under the pointer, or the cell where there is none |
| T, F | Viewport | track (or stop tracking) the selection; frame it |
| Esc | Viewport | close the list of overlapping creatures, leave Track on click, clear the selection |
| g | Viewport / Network | the cell grid / frame the selection |
| H | Network | frame everything |
| Enter, U | Network | go into a statement's rule / go up |
| Tab | Network | add a box at the pointer; while pulling a wire, create a connected statement |
| Ctrl+Z, Ctrl+Shift+Z | Network | undo, redo an edit |
| Ctrl+D, Ctrl+C, Ctrl+V | Network | duplicate, copy, paste statements (into another world too) |
| Shift-click, Shift-drag | Network | select several |
| double-click on a wire | Network | add a routing dot |
| Ctrl+Space, F12 | code box | complete a name; go to where a name is declared |
| F1 | anywhere | the documentation page for this pane |

## What is read and what is changed

Almost everything in the viewer only **reads**: it asks the engine questions and draws the answers, and the world runs exactly as it would without the question. What changes the world is an edit (in the Network, in Source, or a patch through the API): it is checked, then swapped into the running world as one numbered **revision**, and the world keeps running. A refused edit changes nothing and says why. Positions, notes, boxes, cameras and selections are only presentation: they never make a revision.
""",
    },
    {
        "id": "using-network",
        "section": SECTION,
        "title": "The network editor",
        "summary": "Edit a world's rules as boxes and wires: ports, contexts, statements inside a when, the code box, organising, and the inspector.",
        "content": """# The network editor

The Network shows a world's source as boxes and wires. It is a **projection of the text**: every edit becomes a patch to the source, checked and swapped into the running world as one revision, and the Source pane shows the result. Nothing is stored in the graph itself, so it cannot drift from the source.

## Boxes, ports and wires

Sources and reactions are on the left, the statements of the rules in the middle (in the order of the tick's steps), stores on the right. A box has **named sockets**: a transfer has `from` and `to`, a react has the reaction it runs, a `when` and a signal statement have `reads` (and `writes`). A socket is filled when a wire is attached; a required socket that is empty is amber. A wire ends at the socket it stands for.

Wires come in three kinds, told apart by their line: a **resource flow** (an amount moves), a **declaration reference** (a `react` names its reaction; dashed) and a **value dependency** (what a condition or a signal statement reads and writes; dotted). Hover a wire to see its kind and what editing it would write. A box whose statement is not followed through completely (a `let`, a call the analysis does not know) carries a **?**: what it touches beyond the wires drawn is unknown, not absent.

- **Connect**: pull from an output socket and drop on a box. While you pull, the box under the pointer turns **green** and says what the connection would write, or **red** with the reason. The answer is the engine's own check of the edit, applied nowhere until you let go.
- **Create connected**: press **Tab** while pulling a wire from a source or a reaction. The menu offers what can be made and creates it already connected, in one revision.
- **Add**: the palette (transfer, react, diffuse, code) or **Tab** at the pointer adds a box. Tab also lists the library's templates and operators (see *Templates and operators*).
- **Drop on a wire**: drag a sink over the wire from a transfer to its destination; the wire lights up and the hint says what a drop writes: the sink becomes the transfer's `overflow`. That is the one insertion defined; anywhere else a drop only moves the box.
- **Delete** a box from its Parameters pane; **Undo** and **Redo** (Ctrl+Z, Ctrl+Shift+Z) step through your edits.

## Parameters and the code box

Select a box and the Parameters pane says where it runs (its rule, the field, the kind of rule and the step), what it reads and writes, and shows its arguments with their units. An edit is sent when you leave the field or press Enter; Escape puts the applied value back. A value the checker refuses stays as a draft with the reason. An unlabelled statement can be given a **name** (a label), so a patch can find it whatever is inserted before it.

The statement's text is a **code box**:

- **Completion**: typing a name offers what the statement may use (the world's substances, fields, sources, sinks, reactions and constants, then directions, functions and keywords); **Ctrl+Space** opens the list; Enter or Tab takes one.
- **Jump to a declaration**: **F12** (or Ctrl+click) on a name selects the box that declares it.
- **Checked while typing**: a moment after you stop, the engine checks the draft as a swap would (✓ it would be accepted, or ✗ and why); nothing is applied until **Apply** (Ctrl+Enter).

Select several boxes (Shift-click, or Shift-drag a rectangle) and the pane shows the parameters they share. One edit sets all of them: **one revision and one undo**, and a value one of them refuses leaves all of them as they were.

**Duplicate** (Ctrl+D) copies the selected statements, each into a rule of its own with a fresh name; a copy runs as well, so the effect happens twice. **Ctrl+C** copies statements with the field their rule works on; **Ctrl+V** in the same world duplicates them, and in **another world** pastes their text into new rules there, checked by that world (a world without the field, or without a substance they name, refuses them with the reason).

## Contexts

The whole network is the default picture. Press **Enter** (or double-click) on a statement to go into its rule: its statements, with dashed **ghosts** of the sources and stores they touch (double-click a ghost to go to the real thing). **U**, the breadcrumb or the menu next to it go back up. Each network keeps its own camera and positions, and the page comes back to the network and camera you left after a reload. **Rules as boxes** draws the whole network with one box a rule (its statement count, kinds and steps), the wires of its statements on it; Enter on one goes into it.

A statement inside a `when` or `else` is drawn inside a dashed frame that says when it runs, with its own sockets and wires, and is edited and rewired like any statement: the engine rebuilds the `when` that holds it in one patch. Selecting a `when` shows its **condition**, set as one edit, and **Add inside** puts statements at the end of its `then` (or of its `else`, made if it has none).

## Organising

**Find** fades what does not match a name, type, text, substance or step (Enter selects and frames the matches). **H** frames everything, **G** frames the selection. **Note** adds a sticky note; **Box** puts the selected nodes in a labelled box that moves them together. **Arrange…** aligns the selection's edges or centres, spreads it evenly, or lays it out in its columns. A **double-click on a wire** puts a routing dot on it; drag a dot to route the wire, double-click it to remove it. All of this is presentation: kept beside the world, never a revision. With a state directory (`agentsrule web` keeps one), it is kept by the world's name and comes back when the same world is created again, after a restart too.

## The inspector

The **Inspector** follows the selection and keeps three kinds of information apart, each with its own stamp:

| Kind | Comes from | Stamp |
| :--- | :--- | :--- |
| **Declared** | the source: the text, the step it runs in, what it reads and writes, its connections, what can be done to it and why not | the revision |
| **Observed** | one reading of the running world: for a field, the amount of each substance over all cells, read in one call | the tick and revision it was read at |
| **Measured** | what the runner counted over an interval: for a world statement (inside a `when` too), the cells it ran in, the amounts asked and delivered, the requests that were cut; for a reaction, the extent run; for an instance, the statements it writes out | the tick counting began and the tick it was read at |

A transfer statement's Measured section can **record its requests for 10 ticks**: a bounded recording from now on (the past cannot be recovered), which says how many it kept and whether it is complete.

**A cell**: click the picture where no creature is, and the Inspector shows that cell: what it holds at one tick, the statements that touch its field (each a click away in the Network), and **Record what moves through this cell** for 10 ticks (CPU worlds). A reading the backend cannot give says so (a GPU world cannot read cell amounts) and is never drawn as zero. **Hold this observation** keeps the numbers on screen and says how many ticks the world has moved on; **Release** reads again.

## Limits

A text edit of a statement inside a `when` takes one statement (Add inside takes several). Insertion on a wire is defined for a sink on a transfer's destination wire only. A rule with a loop over names is counted by its written-out copies, so a statement inside it has no count of its own. Wires of a written-out instance are shown in its own network, not in the world's.
""",
    },
    {
        "id": "using-look",
        "section": SECTION,
        "title": "The Look network",
        "summary": "How the picture is drawn, as a network of the view's own pieces: typed sockets, previews, the display flag, checked edits.",
        "content": """# The Look network

The **Look** pane shows the viewport's view as a network of its own pieces. Each node is a piece of the view's text, and its socket says the type of what it gives, from the view checker:

```text
field(cells.carbon) → read value → scale(linear, 0..150) → palette(cividis) → color → layer ground ┐
agents(where: true) → units(energy) → scale(…) → palette(inferno) → color → layer life ───────────┴→ view
```

| Node | Is |
| :--- | :--- |
| **source** | a layer's source: `field(cells.carbon)` gives *cells*, `agents(where: …)` gives *agents* |
| **read** / **expression** | the head of an entry: a name it reads (`value`, `lineage_id`) or an expression |
| **stage** | each `|>` call (`scale`, `palette`, `units`, …), with the type it gives (*number*, *scaled*, *colour*, *identity*, …) |
| **entry** | `color`, `size`, `glyph`, `opacity` |
| **body** | a layer's VEX-like statements |
| **layer** | the layer, with its blend and opacity |
| **view** | the composite of the layers, the last on top |

## Previews and the display flag

With **Previews on**, each node shows a small picture of what it gives, drawn by the engine from the current frame: a colour as it is; a scaled value through a gray ramp; an identity by category; a direction by hue; a plain number or amount through a gray ramp over the range this frame holds (the preview says which range). The ◉ **display flag** on a node makes the viewport show that node instead of the whole view; a bar over the picture says what it shows and when its range was measured, and **Whole view** takes it back.

## Editing

Select a node to edit it on the right: a scale's kind and range, a palette from its list (with its colours), any piece as text, a layer's blend, opacity and order, an entry added or removed, a stage put after a node or taken out, a layer added from the view node. Every edit is **checked against the world before it is kept**: a refused one leaves the last valid view on screen and says why. Edits change the world's **view document** on the server, which has its own revision and **Undo**/**Redo** and never changes the world. With a state directory it is kept by the world's name as `views/<name>.em`, an ordinary file of `view` items that `emerge-server render --views` reads too. A view that is part of the world's own source is read-only here: **Edit a copy** makes an editable copy in the view document.
""",
    },
    {
        "id": "using-reuse",
        "section": SECTION,
        "title": "Templates and operators",
        "summary": "Keep statements as a template, make rules a reusable operator, use it in other worlds, set its parameters, detach, upgrade.",
        "content": """# Templates and operators

Two ways to reuse what you built (`proposals/reusable-operators.md`):

| | Template | Operator |
| :--- | :--- | :--- |
| Make one | select statements, **Template…** | select statements, **Operator…**: their rules become a definition, and an instance takes their place |
| Use one | **Tab**: `template NAME` | **Tab**: `operator NAME vN` |
| What the world gets | ordinary rules, no link back | the definition (in the world) and an instance box |
| Change it later | edit the rules | set the instance's parameters, detach it, edit it as a new version, upgrade it |

Both are kept in the **library**, in the state directory (`library/templates/`, `library/operators/NAME@VERSION.em`), so they outlive the server and reach every world.

## Templates

A template keeps the selected statements with the field each rule works on. The declarations they name become **bindings** (each defaulting to the name it had) and the amounts and rates of labelled statements **controls**. Inserting one asks for the bindings (from this world's declarations of that kind) and the controls, and writes the rules with fresh names in one revision.

## Operators

An operator is a definition in the world, `operator NAME version N { param…; rule… }`, and each `use NAME version N as INSTANCE(…)` writes its rules out before anything checks or runs the world (`language.md` §6.9). An instance is one box: its bindings are its sockets (wire a source or a store to it to bind the one parameter of that kind), its value parameters are in Parameters (with the definition's defaults shown), and **Enter** shows the rules it writes out, read-only, named as the runner counts them. The world runs exactly as with the same rules written by hand.

- **Detach** replaces the instance by its rules, in its place: ordinary rules from then on.
- **Edit as a new version** copies the definition as its next version with your change and moves only this instance to it. No other instance, and no other world, changes.
- **Upgrade…** compares the instance's version with the newest one (in the world or the library), side by side, and moves it only when you say so; a version no instance uses any more goes in the same patch.
- **Publish** keeps the instance's version in the library. A version, once kept, does not change: the same content is the same version, other content needs a new version.

Inserting an operator from the library copies its definition into the world, so a saved world holds what reproduces it. A world that already has that name and version with other content refuses it.
""",
    },
    {
        "id": "using-science",
        "section": SECTION,
        "title": "Science panels",
        "summary": "Panels that answer questions about a run: what each shows, what its stamp means, and what a GPU world can answer.",
        "content": """# Science panels

The Science desktop holds panels for asking what a run did. They share a few rules:

- **Every answer carries a stamp**: the world, the tick, the revision and the backend it was read at. An answer for a past interval says what scope it really has (the engine cannot re-observe a tick that has passed).
- **Reading never changes the run.** Asking and not asking give the same world, tick for tick.
- **One selection across panels.** Brush a span of ticks on Lineage History, or select a lineage or a creature, and Life Histories, the census, Ancestry, the Budget and the Flow Map follow it, or say they cannot.
- **A blank is not a zero.** Where a panel could not find something, it says "none found", and where the backend cannot answer it says why.

## The panels

| Panel | Question it answers |
| :--- | :--- |
| **Rule Lens** | what runs in a tick, in what order? Modes: *Lens* (the checked schedule), *Matrix* (which statements read or write which declaration), *Activity* (what each statement did), *Trace* (a bounded recording of transfer requests) |
| **Lineage History** | how did each lineage's population change? |
| **Genome Census** | what do the living carry? Active genes by lineage, a comparison of two creatures, and where each gene is carried |
| **Resource Budget** | where did the upkeep substance come from and go? Stock then and now, every line with its sign, and an *unreconciled* amount that is shown and never forced to zero |
| **Ancestry** | who are this creature's ancestors and descendants? |
| **Life Histories** | how old are the living and what do they carry? How did a birth cohort fare? |
| **Region** | what is in this rectangle of the world, in one call? |
| **Flow Map** | which reactions ran, where, and what blocked them? |
| **Compare Runs** | do two worlds differ, and could the setup explain it? Identical setups are said to be repeated trials of one question |
| **Notebook** | bookmarks that copy their evidence, edits and their timing, exportable as JSON |

## What the GPU can answer

A world on the GPU answers these questions with the same numbers as the CPU in contract mode. What differs:

| | CPU | GPU |
| :--- | :--- | :--- |
| Ancestry, life histories, clades | from tick 0 | from the start of the run (a full log counts what it dropped) |
| Resource budget | by lineage, by clade, split by who makes a substance | by lineage only |
| Where the dead's stock went | yes | no |
| Per-statement activity | yes | yes |
| Transfer trace | up to 100,000 records, the first ones in request order | up to 32,768 records; once full it counts what it dropped, and which requests got a record is not defined |
| Region cell amounts | yes | no |
| Blocked-outcome counters in the Flow Map | yes | no |

## Reading a budget

The budget of a lineage closes: its stock changed by what its creatures made, used, took, stole, gave, dropped and lost, minus upkeep and fees. If the lines do not explain the change, the difference is shown as **unreconciled**. Stock that creatures bring in by genesis or injection after the start is not counted, and shows up there.
""",
    },
    {
        "id": "using-tracking",
        "section": SECTION,
        "title": "Tracking creatures",
        "summary": "Click a creature to select it, Track it to keep watching it, follow it with the camera, record it, and build trackers in the Observe network.",
        "content": """# Tracking creatures

Tracking only observes. Nothing here is part of the source: selecting, tracking, markers, trails and recordings change no revision and no tick, and the run goes the same way with or without them.

## Select

Click a creature in the **Viewport**. The click is matched against the picture on screen, not the world as it is a moment later, so it selects the creature that picture showed. It gets a white ring and a label, and keeps them as it moves. Where several creatures overlap, a short list offers them, nearest first; hovering one previews its ring. A drag still pans and selects nothing. **Esc** clears the selection. The `i` tool still shows the cell under the pointer; its pinned panel has **Select** and **Track** for each creature near it.

The selection bar on the picture has **Track**, **Frame** (move the camera to it once), **Follow camera**, **Genome** and **Ancestry**. With the Viewport focused, **T** tracks or stops tracking the selection and **F** frames it.

## Track

**Track** keeps watching a creature while you select others: it gets a ring in its tracker's colour, a star in Details, and an entry in **Watches**. At most 32 creatures are watched at once. **Track on click** (◎ beside the picture, or the button in Watches) makes every click select a creature and add it to one *Watch set*; Esc leaves the tool and keeps the watches.

A watched creature stays watched when it divides: its child is a new creature, not the same one (track the family with a Descendants tracker). When it dies it stays in the list, marked with the tick and cause of death where the engine recorded them, and a cross marks where it was last seen. Nothing jumps to another creature by itself.

## Follow camera

**Follow camera** keeps the creature inside the middle of the Viewport. Moving the camera by hand pauses it (**Resume** picks it up); zooming does not. **Stop** is always on screen while it follows. The picture is one image of a wrapping world, so at an edge the camera jumps across with the creature.

## Details

**Details** is a table of agents with a scope: *Population sample* (an even sample), *All agents* (sorted and paged on the server, all pages from one reading of the world; Refresh reads again), *Selected*, *Tracked* (every watched creature, the dead too) and *Tracker output* (one tracker's members, each with how long it has been in). A condition such as `age > 50 and carried.body >= 1000`, `region x 0..100 y 0..100` or `near #1042 within 20` is evaluated by the server over the whole population. Type an ID to select a creature that is in no sample. The selected and watched creatures always show in a strip above the rows. Click selects; double-click opens the genome; the star tracks.

## Inspector

With a creature selected, the **Inspector** shows it as one exact reading: age, position, heading, active genes, what it carries, its birth (its parent, or *founder*), its death if it died, and what tracks it, stamped with the tick, revision and backend. **Pin Inspector** keeps it on that creature while you select others; **Hold this observation** keeps the values on screen while the world runs on.

## Watches and recordings

**Watches** lists every tracker: its meaning in words, how many it holds now and ever, and buttons to **Show** or hide its markers, add a short **Trail** (positions from the frames the page received, not a recording), **Follow camera**, **Record**, export **CSV** or **JSON**, open its **Node** or its members in **Details**, **Stop observing** (the definition and data stay), and **Remove**.

**Record** samples the members (at most 32) every chosen number of ticks on the server, at the end of each tick. It goes on whatever the page does: closing the tab or a slow connection changes nothing in it. It keeps up to 4096 samples per creature, then stops or drops the oldest, and the export says which, with the run, seed, source hash, cadence and the rule that chose the members. *Creature interactions* adds the give and steal requests that involve the members, from the run's one transfer recording, which it shares with the Rule Lens and never takes over silently.

## Trackers in the Observe network

In the **Network**, the context menu has **Observe**: *Run agents* feed each **Tracker**, and its outputs (current members, recorded samples, events) feed what shows them: *Marker* and *Trail* in the Viewport, *Samples* and *Event table* in Details and exports. A watch is a Tracker node too: renaming or changing one in Parameters changes the same object Watches shows. Each node has its **display flag** (◉) and a red dot while it records. Changes have their own undo and revision; one made against a tracker that changed elsewhere meanwhile is refused as stale. Moving a node is layout only.

**Add** in Observe (and **New tracker…** in Watches) makes a tracker over a set:

| Kind | Members |
| :--- | :--- |
| Capture the matches now | the creatures matching a condition at this tick, kept as fixed individuals (*Viewport's view* fills in the rectangle the picture shows) |
| The selection and the watched | those individuals, fixed |
| Live query | whoever matches at each evaluation: members enter and leave |
| Latched query | everyone seen matching between two ticks, kept afterwards |
| Descendants | a creature and every child born to its members (optionally the children already born, from the events) |
| Representative | the one creature first by a ranking (oldest, most genes, …); each handoff is kept |

A query is evaluated every chosen number of ticks: a match that begins and ends between two evaluations can be missed. A tracker holds at most 4096 creatures and says how many it had to leave out.

## Limits

- A world on the GPU records every 10 ticks or more (each sample reads the entity table back from the device).
- A recording keeps at most 32 creatures; members over that are counted, not sampled.
- A position history exists only from when a recording started: the past is not recorded.
- Trails between two samples far apart on a wrapping world are split where they cross an edge, not joined across the map.
""",
    },
    {
        "id": "using-worlds",
        "section": SECTION,
        "title": "Worlds, presets and backends",
        "summary": "Starting a world: the examples and presets, lineages, seeds, CPU and GPU, contract mode, and journals.",
        "content": """# Worlds, presets and backends

## Starting a world

**New world…** offers the examples that come with the engine. A world is created from a source, the **lineages** that start in it (the creature kinds the file defines), a **seed** (the same seed gives the same run), a **size**, and a **backend**. Mutation is on unless the example turns it off.

| Example | What it is for |
| :--- | :--- |
| **Reference dish** | `examples/dish.em`: chemistry, a conserved field, world rules, menus and two lineages (Drifter, Builder); a new world is easiest written as a changed copy of it |
| **Game of Life (glider gun)** | a state-field automaton with no creatures (CPU only) |
| **Soil (reusable operators)** | `examples/operators.em`: an operator definition and two instances of it; no creatures, so it runs on the CPU |
| **Sunlit Nursery** | a luminous ecosystem with a working metabolism (Builder, mutation off): what limits population growth? |
| **The Long Night** | sunlight stops for ticks 300 to 399: who survives on stored resources? |
| **The Long Night (harsh)** | the same world with a night to tick 449: extinction |
| **Lean Times** | Builder against Bloated (Builder plus eight unused senses): what does unneeded machinery cost? |
| **Automata Cabinet** (three soups) | Conway's rules, one extra birth count (HighLife) and Seeds from a random soup on a wrapping 64 x 64 grid (CPU only) |

Each preset says what to watch, which controls it has, and what was verified about it (and what was not), in its description.

## CPU and GPU

The **CPU** runner is the reference: it defines what a world means. The **GPU** runner runs the same tick on a graphics card, roughly 20 to 60 times faster than one CPU core on the same world. In **contract mode**, which the GPU always uses and the CPU can be asked to use, the two give the same state word for word on one driver, so a run can be compared across them. Without it, results differ slightly (transcendental functions are not bit-identical), so runs are only compared within one mode.

A world that uses state fields (Game of Life) runs on the CPU only; the GPU refuses it, and says why.

## Editing a running world

An edit is a **hot-swap**: the source is checked, the change is applied between two ticks, and the world goes on with the same creatures. A swap is refused, with the reason, when it would change something the engine cannot swap into a running world (substances, elements, menus other than their fees, genomes). Create a new world from the changed source for those.

## Journals

With `--journal-dir`, every world writes a journal: its creation and every swap, with its reason. `emerge-server replay JOURNAL` plays one back to the same state, tick for tick, so an experiment can be reproduced from the journal alone.

## Seeds and trials

A repeated seed is the same trial, not a new one. Compare Runs says so when two worlds have identical setups, and calls setups that differ single trials: one run of each is a hint, not a result.
""",
    },
    {
        "id": "using-cli",
        "section": SECTION,
        "title": "Command line",
        "summary": "emerge, emerge-server, emerge-gpu and the agentsrule launcher: the commands and what they print.",
        "content": """# Command line

Build once in release mode (the GPU tests crash a debug build):

```sh
cargo build --release -p emerge-ref -p emerge-server -p emerge-gpu
```

## emerge

```sh
target/release/emerge check  examples/dish.em        # static checks, every diagnostic it can find
target/release/emerge fmt    examples/dish.em        # the one canonical text; --check only says if it differs
target/release/emerge expand examples/dish.em        # lineages written out, operators' instances and loops written out, constants replaced
target/release/emerge order  examples/dish.em        # the steps of one tick, each rule placed
target/release/emerge run    examples/dish.em --lineages Drifter --ticks 1000
```

`check` prints `file:line:col: severity[code]: path: message`, or one JSON object per line with `--json`. `run` prints a row every `--every` ticks; its other options (`--flux`, `--stop-when-sustained`, `--accounts`, `--ancestry`, `--contract gpu`, …) are listed by `emerge` with no arguments. Every command also takes `--patch FILE` and `--set TARGET.PATH=VALUE` after the file, applied in order, to run a variant without copying the file.

## emerge-server

```sh
target/release/emerge-server serve --viewer [--port N] [--journal-dir DIR] [--state-dir DIR]   # the engine API and the viewer
target/release/emerge-server mcp [--journal-dir DIR]                          # the same API as MCP over stdio
target/release/emerge-server render examples/dish.em --lineages Drifter --tick 300 --out dish.png
target/release/emerge-server replay JOURNAL
```

## emerge-gpu

```sh
target/release/emerge-gpu run examples/dish.em Drifter 1000 1 100    # file, lineage, ticks, seed, row every N
```

Environment variables: `EMERGE_GPU_MUTATION=1` turns mutation on, `EMERGE_GPU_POOL_WORDS=N` sizes the genome pools (large populations need more), `EMERGE_GPU_AUDIT=1` audits element conservation every tick, `EMERGE_GPU_ACCOUNTS=1` and `EMERGE_GPU_LAWSTATS=1` keep the per-lineage accounts and per-statement counters (about 3 % slower with both on).

## agentsrule

`agentsrule web` builds what is stale and starts the viewer (see *Using the viewer*). Link `scripts/agentsrule` into your path once: `ln -s "$PWD/scripts/agentsrule" ~/.local/bin/agentsrule`.

## Tests

```sh
cargo test --release --workspace --no-fail-fast
cargo test --release -p emerge-gpu -- --test-threads=1     # parallel GPU tests run out of device memory
python3 scripts/check_docs.py                               # the documentation against the code
node scripts/browser-check.mjs URL scripts/browser/network.mjs   # the viewer, in a real Chromium
```
""",
    },
    {
        "id": "using-api",
        "section": SECTION,
        "title": "The engine API and MCP",
        "summary": "Drive a world from a program: JSON-RPC over WebSocket, the MCP adapter, and the methods.",
        "content": """# The engine API and MCP

Everything the viewer does goes through the engine API, so a program can do it too: create worlds, run them, edit them, and ask the science questions.

## Connecting

`emerge-server serve` listens for JSON-RPC 2.0 on a WebSocket on `127.0.0.1` and prints the address and a key. The key goes in the WebSocket subprotocol (`emerge.k.<key>`), and a request without it is refused. `emerge-server mcp` serves the same methods over stdio as MCP tools, with `_` for the dot in a name (`world_create` calls `world.create`), for a client that wants a tool list.

```json
{"jsonrpc": "2.0", "id": 1, "method": "world.create",
 "params": {"source": "…", "lineages": ["Drifter"], "seed": 1, "backend": "cpu"}}
```

A request that fails answers with an error code and a message that says why; an edit that is refused changes nothing.

## Methods

| Group | Methods |
| :--- | :--- |
| Worlds | `world.create`, `world.list`, `world.source`, `world.validate` |
| Running | `run.step`, `run.pause`, `run.resume`, `stats.query`, `entities.query`, `view.snapshot`, `view.pick`, `view.palettes` |
| Editing | `world.hotswap` (a patch), `world.graph`, `world.graph_op`, `world.graph_edit`, `world.undo`, `world.redo`, `world.layout_set`, `world.names` |
| Views (the Look network) | `views.get`, `views.set`, `views.undo`, `views.redo`, `view.graph`, `view.edit`, `view.copy`, `view.preview`, `view.display` |
| Reuse | `library.list`, `template.save`, `template.insert`, `operator.publish`, `operator.insert`, `operator.upgrade` |
| Rules | `rules.lens`, `rules.matrix`, `rules.activity`, `trace.start`, `trace.query`, `reactions.flow`, `flow.blocked_start` |
| Genomes | `genome.explain`, `genome.graph`, `genome.census`, `genome.atlas`, `genome.compare` |
| Lineages | `history.query`, `ancestry.query`, `life.query`, `region.query` |
| Budgets | `accounts.start`, `accounts.query`, `accounts.import` |
| Runs | `runs.compare`, `runs.outcomes`, `runs.pair`, `export.provenance`, `journal.read` |
| Notebook | `notebook.add`, `notebook.list`, `notebook.events`, `notebook.export` |
| Tracking | `entities.get`, `entities.find`, `tracker.create`, `tracker.update`, `tracker.stop`, `tracker.resume`, `tracker.remove`, `tracker.undo`, `tracker.redo`, `tracker.list`, `tracker.query`, `tracker.record_start`, `tracker.record_stop`, `tracker.samples`, `tracker.export`, `tracker.interactions` |
| Frames (WebSocket only) | `view.subscribe` (with `pick` and `watch`), `view.unsubscribe`, `view.watch`, `view.pick_frame`, `view.record`, `view.recordings` |
| Help | `docs.query`, `docs.examples`, `docs.example` |

Answers to questions about the world carry an `observation` object: the token, world, run, tick, revision and backend they were read at.

## Creatures, runs and IDs

A creature is named by its run and its entity ID. `world.create` answers a `run`: an ID for this world in this server process, never given again. A call may pass `run` next to `world`; if the world handle now names another run (a restarted server gives out `w1` again) the call is refused with code `-32012` instead of answering about a different creature. Entity IDs in the tracking methods are decimal strings, because a JSON number read by JavaScript is not exact above 2^53; an ID may be sent as a number only below that.

`entities.get` reads creatures by exact ID, whatever any sample holds: each answer says `alive`, `dead` (with the tick and cause from the death events), `not living` (not in the complete lookup, and no death recorded) or `unknown` (no creature of this run had that ID). `entities.find` evaluates a condition over every living creature at one tick, sorts on the server and pages from that one stored result (a `cursor`), so pages never skip or repeat a creature; `sample` gives an even sample instead. A condition is `{"attr", "op", "value"}` (attributes `id`, `lineage`, `age`, `x`, `y`, `active_genes`, `carried.SUBSTANCE`), `{"all": [...]}`, `{"any": [...]}`, `{"region": {"x": [a, b], "y": [c, d]}}` or `{"near": {"id", "radius"}}`. A condition that names a substance the world does not have is refused, never read as zero.

## Trackers

A tracker is a named set of creatures with a membership rule: `fixed` (these individuals), `live` (whoever matches at each evaluation), `latched` (everyone seen matching during a window, kept afterwards), `descendants` (a root and the children born to its members) or `representative` (one creature first by a ranking; each handoff is kept). A `watch` is a tracker made by Track (at most 32 creatures in all). Changes to a tracker have their own definition revision and undo (`tracker.undo`), and `base` makes a change made against an older definition fail as stale. A death never removes a member.

`tracker.record_start` samples the members (at most 32) at completed tick boundaries, every `every` ticks, on the server: what a browser receives changes nothing. It keeps at most `cap` samples per creature and then stops or drops the oldest (`policy`), and says so in its coverage. `tracker.samples` pages by sequence number (`after`), so a client that reconnects asks after the last one it has. `interactions: true` shares the run's one transfer recording: it attaches to one that keeps creatures' actions, starts one if none is open, and is refused if another owner holds it; `trace.start` likewise refuses to replace an open recording someone else started unless `replace: true`.

## Frames

`view.subscribe` with `pick: true` keeps an index of the agents each sent frame drew. Each frame header carries `frame`, `run`, the map's size, and `agents`: the selected creatures (`view.watch`) and every visible tracker's members, with their position in that same frame. `view.pick_frame` names the creatures under a point of a frame the subscription was sent; once the frame is no longer kept it answers `frame expired` (code `-32011`) rather than picking from the present.

## Edits and revisions

`world.graph_op` takes `base_revision` and one `op`: `set_param`, `wire`, `delete`, `add_rule`, `add_connected`, `duplicate`, `label`, `unwire`, `paste` (statements as text into new rules), `insert` (a sink on a transfer's destination wire), `add_inside` and `condition` (a `when`), `detach`, `fork` and `upgrade` (an operator's instance), or `batch`. If the world has moved on since `base_revision` the edit is refused as stale. With `"dry_run": true` it answers what the edit would write and checks it as a swap would, applying nothing; `world.graph_edit` takes `dry_run` too (the code box's inline check). A `batch` is one revision and one undo. `world.hotswap` takes a patch in the language of `language.md` §10. `world.layout_set` takes `positions`, `notes`, `frames` and `routes` (routing dots); none of them is a revision. `trace.start` takes `cells` (cell indices) to keep only the requests through those cells (CPU worlds).

`view.edit` changes the world's view document (`set_stage`, `insert_stage`, `remove_stage`, `set_entry`, `set_body`, `set_source`, `add_layer`, `remove_layer`, `move_layer`), checked against the world before it is kept; it has its own `base` revision and undo. `view.preview` answers a PNG (base64) of one node of a view, `view.display` the view the viewport shows for it.
""",
    },
]
