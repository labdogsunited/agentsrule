# Emerge (agentsrule)

A rule-based language and engine for open-ended evolution. A world is a few lines of rules over conserved substances (`.em` files); creatures with evolvable genomes live in it; conservation, dissipation and the cost of living are laws that cannot be broken. It runs on the CPU (the reference) and on the GPU (equal word for word in contract mode), and has a browser editor.

## Try it

```sh
ln -s "$PWD/scripts/agentsrule" ~/.local/bin/agentsrule   # once
agentsrule web                                            # builds, starts the viewer, opens the browser
```

`agentsrule web [--port N] [--no-open] [--journal-dir DIR]` takes the first free port from 7878 (or the one you give), prints the address, and has a **Restart server** and a **Stop server** button; Ctrl-C stops it too. In the page:

| | |
| :-- | :-- |
| **New world** | start from the reference dish, Game of Life or the test-9 dish; run on the CPU or the GPU |
| **Picture** | the live view; wheel zooms, drag pans; press **i** for an info picker (cell contents, nearby creatures, open a creature's genome) |
| **Graph** | the world as boxes and wires (Houdini style): edit a parameter, add, wire and delete boxes, type into a code box, Ctrl+Z to undo; every edit is a live hot-swap |
| **Genome** | one creature's genes in module frames, signals as wires |
| **Code** | the complete source of the running world; edit a copy, open a `.em` file, make a new world from it |

## Command line

```sh
cargo build --release -p emerge-ref -p emerge-server -p emerge-gpu    # emerge, emerge-server, emerge-gpu
target/release/emerge check  examples/dish.em       # static checks
target/release/emerge order  examples/dish.em       # the steps of one tick, each rule placed
target/release/emerge run    examples/dish.em --lineages Drifter --ticks 1000
target/release/emerge-server render examples/dish.em --lineages Drifter --tick 300 --out dish.png
target/release/emerge-gpu run examples/dish.em Drifter 1000 1 100
```

Run the tests in release mode: `cargo test --release --workspace --no-fail-fast` (the GPU tests crash a debug build).

## Where things are

| | |
| :-- | :-- |
| `language.md` | the `.em` language, normative for syntax |
| `spec-v0.7.9.md` | runtime behaviour, the engine API, the roadmap; `core.md` is the core language it is being restructured around |
| `proposals/plan-next.md` | the active plan; `proposals/decisions-for-review.md` is the queue of decisions and what was decided |
| `proposals/*.md` | one document per design (node editor, live viewer, rule conditions, checked schedule IR, catalyst experiment, …) with its pass rules and results |
| [`proposals/world-presets.md`](proposals/world-presets.md) | proposed world presets, their visual character, controls, capability requirements, and validation plan |
| [`proposals/science-panels.md`](proposals/science-panels.md) | proposed rule, lineage, genome, resource, and experiment panels with observation requirements |
| [`proposals/genome-editor.md`](proposals/genome-editor.md) | genome editor design: seed authoring, signal wiring, modules, captures, and test runs |
| `crates/emerge-lang` | parser, checker, formatter, patches, the checked schedule, the graph model |
| `crates/emerge-ref` | the CPU reference runner |
| `crates/emerge-gpu` | the GPU runtime |
| `crates/emerge-server` | the engine API (JSON-RPC over WebSocket, MCP), journals, the browser viewer |
| `crates/emerge-view` | the renderer |
| `docs/` | Houdini VEX-style HTML help system and function reference; open `docs/index.html` or view via `/help` in the browser viewer |
| `examples/`, `gallery/` | worlds and pictures |
| `archive/` | old specs, reviews and finished experiments |

## Rules of the house

Conservation, determinism, tick ordering, default evolutionary behaviour, success criteria and stop rules are the scientific contract: changes to them are proposed with their evidence and consequence first, and each experiment fixes its pass and stop rules before it runs. Everything is committed locally; pushing waits for review.
