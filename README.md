# Emerge (agentsrule)

A rule-based language and engine for open-ended evolution. A world is a few lines of rules over conserved substances (`.em` files); creatures with evolvable genomes live in it; conservation, dissipation and the cost of living are laws that cannot be broken. It runs on the CPU (the reference) and on the GPU (equal word for word in contract mode), and has a browser editor.

![Interactive Web Viewer](gallery/viewer-l3.png)

## Screenshots

### Application Panes & Desktop Layouts

| Node Network Editor (`Rules`) | Creature Genome Editor (`Genome`) |
| :---: | :---: |
| ![Node Network Editor](gallery/app-node-network.png) | ![Genome Editor](gallery/app-genome-editor.png) |
| *Houdini-like network of rules as boxes, ports, and wires* | *Evolvable modules, signal wiring, and parameter cards* |

| Science Panels & Rule Lens (`Science`) | Agent Tracking & Inspector (`Track`) |
| :---: | :---: |
| ![Science Panels](gallery/app-science-lens.png) | ![Agent Tracking](gallery/app-agent-tracking.png) |
| *Rule execution schedule, lineage history, and accounts* | *Live creature tracking, follow camera, and entity inspect* |

### Simulation Runs

| Sunlit Nursery (`examples/presets/nursery.em`) | Emerging Producers (`archive/test9-dish.em`) |
| :---: | :---: |
| ![Sunlit Nursery](gallery/preset-nursery-seed1-tick300.png) | ![Producers Spreading](gallery/test9-seed4-tick5000.png) |
| *Biolume view: 5,978 creatures in a layered cellular dish* | *Producers spreading through a Drifter population over nitrate* |


## Prerequisites

Before installing, ensure you have:

* **Rust Toolchain**: Rust 1.75+ (edition 2021). Install via [rustup](https://rustup.rs/):
  ```sh
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
  ```
* **Python 3**: Python 3.10+ (used by helper scripts and documentation generator).
* **Modern Web Browser**: Chrome, Brave, Firefox, Edge, or Safari with WebGL/WebSocket support.
* **GPU Drivers (Optional)**: Vulkan, Metal, or DirectX 12 drivers if running the GPU backend (`emerge-gpu` via `wgpu`). The CPU reference backend (`emerge-ref`) has no GPU dependency.
* **FFmpeg (Optional)**: Required only if recording and exporting simulation journals to `.mov` videos.

## Installation & Quick Start

### 1. Clone the repository

```sh
git clone https://github.com/labdogsunited/agentsrule.git
cd agentsrule
```

### 2. Build the binaries

Build the core binaries in release mode:

```sh
cargo build --release -p emerge-ref -p emerge-server -p emerge-gpu
```

This builds:
* `target/release/emerge` — CPU runner, compiler, and CLI checker
* `target/release/emerge-server` — Engine API server (JSON-RPC over WebSocket) and renderer
* `target/release/emerge-gpu` — GPU accelerated runner

### 3. Launch the Web Viewer

Link the helper script and launch the interactive viewer:

```sh
ln -s "$PWD/scripts/agentsrule" ~/.local/bin/agentsrule   # once
agentsrule web                                            # builds, starts the viewer, opens the browser
```

`agentsrule web [--port N] [--no-open] [--journal-dir DIR]` binds to the first free port starting from 7878, prints the local address, and opens your browser. Ctrl-C stops the server.

In the viewer:

The viewer's panes, in short (the guide *Using the viewer*, in the **Docs** button or `docs/index.html`, has all of them):

| | |
| :-- | :-- |
| **New world…** | start from an example or preset (the reference dish, Sunlit Nursery, The Long Night, Lean Times, an automata cabinet), with the lineages, seed, size and backend (CPU or GPU) you choose |
| **Viewport** | the live picture; wheel zooms, drag pans; press **i** for an info picker (cell contents, nearby creatures, open a creature's genome) |
| **Network** | the world's rules as boxes and wires: named sockets, a live preview of a connection, contexts (Enter goes into a rule, U comes back), duplicate and paste, one edit for several statements, Ctrl+Z to undo; every edit is a checked hot-swap into the running world |
| **Inspector** | what the selected box is (declared), what the world holds in it (observed) and what the runner counted for it (measured), each with its tick and revision |
| **Genome** | one creature's genes in module frames, signals as wires |
| **Source** | the complete source of the running world; edit a copy, open a `.em` file, make a new world from it |
| **Science panels** | Rule Lens, Lineage History, Genome Census, Resource Budget, Ancestry, Life Histories, Region, Flow Map, Compare Runs, Notebook: questions about a run, each answer stamped with the tick, revision and backend it was read at (the GPU answers them too) |
| **Docs** (F1) | this documentation: the app guide, the language guide, the function and attribute reference, the diagnostics |

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
| `crates/emerge-lang` | parser, checker, formatter, patches, the checked schedule, the graph model |
| `crates/emerge-ref` | the CPU reference runner |
| `crates/emerge-gpu` | the GPU runtime |
| `crates/emerge-server` | the engine API (JSON-RPC over WebSocket, MCP), journals, the browser viewer |
| `crates/emerge-view` | the renderer |
| `docs/` | the help site (generated by `scripts/generate_docs.py` from `scripts/doc_data.py` and `scripts/doc_app.py`): open `docs/index.html`, or press **F1** in the viewer; `python3 scripts/check_docs.py` checks it against the code |
| `examples/`, `gallery/` | worlds and pictures |
| `archive/` | old specs, reviews and finished experiments |

## Rules of the house

Conservation, determinism, tick ordering, default evolutionary behaviour, success criteria and stop rules are the scientific contract: changes to them are proposed with their evidence and consequence first, and each experiment fixes its pass and stop rules before it runs. Everything is committed locally; pushing waits for review.

## License

This project is licensed under the GNU Affero General Public License v3.0 (AGPL-3.0-or-later) - see the [LICENSE](LICENSE) file for details.
Copyright (C) 2026 Lab Dogs United.

