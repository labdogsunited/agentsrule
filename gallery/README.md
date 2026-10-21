# Gallery

A few chosen pictures of real runs, each with what is needed to draw it again (`archive/questions-2026-10-21.md`, question 14). Everything else is drawn on demand with `emerge-server render`. Large recordings and image sequences stay outside Git.

| Picture | What it shows | Command | Source and settings |
| :-- | :-- | :-- | :-- |
| `test9-seed4-tick5000.png` | Exit test 9, seed 4: producers (agents whose own reactions made body in ticks 4_000–5_000) spreading through a `Drifter` population, over nitrate in the cells. 11_104 of 18_731 agents are producers, equal to the archived run's row (59.28%, `~/test9dup/drifter-4.csv`) | `emerge-server render archive/test9-dish.em --lineages Drifter --seed 4 --tick 5000 --made body --window 1000 --views examples/views/dish.em --view Producers --out gallery/test9-seed4-tick5000.png` | `archive/test9-dish.em`, seed 4, tick 5_000, CPU, default mode, mutation on, view `Producers`, commit `6645166`; 2 min 43 s |
| `viewer-l3.png` | The live viewer (`proposals/live-viewer.md` L3) in a real browser (Brave, headless, driven over the DevTools protocol, 5 s after loading): a running reference-dish world, view `Lineages`, frames pushed every 5 ticks | `emerge-server serve --viewer`, a world created and resumed over the API, then the printed address opened | `examples/dish.em` at 96 × 96, `Drifter,Builder`, seed 3, CPU, default mode, commit after `9203f29` |
