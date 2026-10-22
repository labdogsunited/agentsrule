"""
Data definitions for Emerge Language Documentation
Covers all built-in functions, statements, attributes, diagnostics, and language guides.
"""

import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from genome_visuals import (
    generate_network_diagram,
    generate_four_gene_kinds_diagram,
    generate_signal_memory_diagram,
    generate_evolution_specialization_diagram,
    generate_gpu_memory_layout_diagram,
)

FUNCTIONS = [
    # --- Substance Flow ---
    {
        "id": "transfer",
        "name": "transfer",
        "category": "Substance Flow",
        "summary": "Transfers an amount of conserved substance between two stores with pro-rata shortfall arbitration.",
        "return_type": "void (effect)",
        "context": "Transfer rules, Genesis",
        "phase": "Move",
        "conserved": True,
        "gpu": True,
        "synopsis": """void transfer(substance s, store from, store to, amount[s] amount [, sink overflow]) @label;""",
        "params": [
            {"name": "s", "type": "substance", "req": "Yes", "desc": "The declared conserved substance to move (e.g. carbon, light, body)."},
            {"name": "from", "type": "store | source", "req": "Yes", "desc": "Origin store: source (rock, sun), self (the current cell), or a neighbour cell (above, below, left, right)."},
            {"name": "to", "type": "store | sink", "req": "Yes", "desc": "Destination store: self, a neighbour cell, or a declared sink."},
            {"name": "amount", "type": "amount[s]", "req": "Yes", "desc": "Quantity to transfer, strictly typed by substance s (e.g. 20mu, 5u). Amounts are unsigned saturating micro-units."},
            {"name": "overflow", "type": "sink", "req": "No", "desc": "Optional fallback sink (e.g. void). If the destination lacks capacity, excess is vented to this sink instead of remaining at origin."}
        ],
        "returns": "Produces a transfer effect submitted to the Move phase arbitration pool.",
        "details": """
`transfer` is the fundamental mechanism by which physical matter moves across the simulation. 
Because substances are strictly conserved, matter is never created or destroyed.

### Two-Stage Pro-Rata Arbitration
All transfers across all cells, rules, and creatures are arbitrated concurrently in the **Move phase**:
1. **Supply Contention:** If the total transfer requests drawn from a store exceed its available snapshot balance, each request receives a proportional fraction (floored). Leftover micro-units are distributed one by one to the largest fractional parts.
2. **Capacity Contention:** If the incoming amount exceeds the destination store's remaining headroom (`cap(s) - balance`), incoming transfers are scaled down pro-rata.
3. **Overflow:** If an `overflow` sink is provided, rejected headroom surplus is diverted into that sink. Without an overflow sink, surplus remains safely at the origin store.

Transfers between cells in world rules (`from: self, to: above`) do not accept an overflow argument.
""",
        "example": """// Inflow from source to cell with overflow sink
rule weather on cells {
    transfer(carbon, from: rock, to: self, amount: 20mu, overflow: void) @transfer_carbon;
    
    // Transfer from self to neighbour below
    when self.grain > 0u and below.grain < cap(grain) {
        transfer(grain, from: self, to: below, amount: 1u) @transfer_grain;
    }
}""",
        "see_also": ["diffuse_conserved", "react", "take", "give", "cap"]
    },
    {
        "id": "react",
        "name": "react",
        "category": "Substance Flow",
        "summary": "Executes a declared chemical reaction up to a specified extent rate, bounded by inputs and output headroom.",
        "return_type": "void (effect)",
        "context": "Reaction rules, Genomes",
        "phase": "Transform",
        "conserved": True,
        "gpu": True,
        "synopsis": """void react(reaction r, rate: amount) @label;""",
        "params": [
            {"name": "r", "type": "reaction", "req": "Yes", "desc": "The declared or generated reaction identifier (e.g. rot, respiration, build)."},
            {"name": "rate", "type": "amount", "req": "Yes", "desc": "Maximum stoichiometric extent of the reaction per tick (e.g. 50bp as rate or 1u of extent)."}
        ],
        "returns": "Consumes reaction inputs and produces reaction outputs according to balanced stoichiometry.",
        "details": """
`react` drives biochemical transformation. Reactions run strictly downhill according to the world's `second_law { min_drop: ... }` rule.

### Execution Constraints:
- Reactions run in the **Transform phase**, after all spatial transfers and movements in the Move phase have concluded.
- In each cell or creature, reactions execute sequentially in declaration order.
- The actual extent achieved is clamped by:
  1. The available inventory of all reactant inputs.
  2. The available headroom of all product stores (cannot exceed substance cap).
  3. The requested `rate`.
- If coupled with a catalyst (`catalyze iron(half: 100mu, boost: 10.0)`), reaction throughput scales non-linearly with catalyst presence.
""",
        "example": """world Dish {
    reaction rot : body -> carbon + nitrate;
    
    rule rotting on cells {
        react(rot, rate: 50bp) @react_rot;
    }
}""",
        "see_also": ["transfer", "diffuse_conserved", "take", "give"]
    },
    {
        "id": "diffuse_conserved",
        "name": "diffuse_conserved",
        "category": "Substance Flow",
        "summary": "Evenly diffuses substances to 4 orthogonal von Neumann neighbours with exact mass conservation.",
        "return_type": "void (effect)",
        "context": "Transfer rules",
        "phase": "Move",
        "conserved": True,
        "gpu": True,
        "synopsis": """void diffuse_conserved(list[substance] substances, rate: rate) @label;""",
        "params": [
            {"name": "substances", "type": "list[substance]", "req": "Yes", "desc": "Bracketed list of conserved substances to diffuse, e.g. [sugar, carbon, nitrate]."},
            {"name": "rate", "type": "rate", "req": "Yes", "desc": "Diffusion fraction in basis points (e.g. 300bp = 3.00%). Max 10_000bp."}
        ],
        "returns": "Generates conservative transfer flows from the cell to North, East, South, and West neighbours.",
        "details": """
`diffuse_conserved` provides physical spatial diffusion without numerical drift or matter loss.

### Mechanics:
- For each substance, `diffuse_conserved` computes:
  `offered = floor(current_amount * rate / 10_000)`
- The offered amount is split equally among the 4 von Neumann neighbours:
  `each_neighbour = floor(offered / 4)`
- Any remainder from the integer divisions stays in the origin cell.
- If the world space wraps (`wrap: true`), edge cells diffuse across toroidal boundaries; otherwise boundary outflows are clipped.
""",
        "example": """rule spread on cells {
    diffuse_conserved([sugar, carbon, nitrate], rate: 300bp) @diffuse_conserved;
}""",
        "see_also": ["transfer", "blur", "cap"]
    },
    {
        "id": "take",
        "name": "take",
        "category": "Substance Flow",
        "summary": "VEX-style expression to pull an amount of conserved substance from an external store into the current element.",
        "return_type": "amount[s]",
        "context": "VEX context(agent), context(cells)",
        "phase": "Move",
        "conserved": True,
        "gpu": True,
        "synopsis": """@substance += take(store, amount);""",
        "params": [
            {"name": "store", "type": "store | source", "req": "Yes", "desc": "Target store (e.g. world.rock, cell)."},
            {"name": "amount", "type": "amount[s]", "req": "Yes", "desc": "Amount requested to extract."}
        ],
        "returns": "Lowers to an arbitrated transfer request in the Move phase.",
        "details": """
In VEX-like `.em` code, substances cannot be assigned directly (`@nitrate += 10u` is an error: `E0302`) because substances are strictly conserved.
Instead, matter must be explicitly drawn from an identified store using `take(store, amount)`.
""",
        "example": """context(agents) {
    if (@producer) {
        @nitrate += take(cell, 250mu);
        @energy += take(world.spark, 5u);
    }
}""",
        "see_also": ["give", "transfer", "move_to"]
    },
    {
        "id": "give",
        "name": "give",
        "category": "Substance Flow",
        "summary": "VEX-style expression to deliver an amount of substance from the current element to a target store.",
        "return_type": "amount[s]",
        "context": "VEX context(agent)",
        "phase": "Move",
        "conserved": True,
        "gpu": True,
        "synopsis": """@substance -= give(store, amount);""",
        "params": [
            {"name": "store", "type": "store | neighbour", "req": "Yes", "desc": "Target destination (e.g. cell, neighbour)."},
            {"name": "amount", "type": "amount[s]", "req": "Yes", "desc": "Amount to transfer."}
        ],
        "returns": "Delivers substance into the target store subject to headroom.",
        "details": """
`give` safely transfers matter out of an agent's inventory into a destination store or neighbouring creature.
""",
        "example": """context(agents) {
    if (@energy > 50u) {
        @energy -= give(cell, 10u);
    }
}""",
        "see_also": ["take", "transfer", "drop"]
    },
    {
        "id": "move_to",
        "name": "move_to",
        "category": "Substance Flow",
        "summary": "Vents or disposes of excess matter directly into a designated sink boundary.",
        "return_type": "void",
        "context": "Transfer rules, Patches, Genesis",
        "phase": "Move",
        "conserved": True,
        "gpu": True,
        "synopsis": """move_to(sink, amount);""",
        "params": [
            {"name": "sink", "type": "sink", "req": "Yes", "desc": "The sink boundary that accepts the substance (e.g. void, vent)."},
            {"name": "amount", "type": "amount[s]", "req": "Yes", "desc": "Amount of substance to vent."}
        ],
        "returns": "Vents the substance out of the simulated system into the sink boundary.",
        "details": """
Used in world rules, patches (`remove substances.ozone move_to: void;`), and cleanup blocks.
The target sink must declare that it accepts the substance (`sink void : any;` or `sink vent : [heat];`).
""",
        "example": """// In world upkeep or layer body
move_to(world.vent, @heat);""",
        "see_also": ["transfer", "waste"]
    },

    # --- Math & Numeric Built-ins ---
    {
        "id": "fit",
        "name": "fit",
        "category": "Math & Numeric",
        "summary": "Re-maps a value from a source interval [omin, omax] into a target interval [nmin, nmax], clamped.",
        "return_type": "float",
        "context": "VEX layer bodies, View contexts",
        "phase": "Draw",
        "conserved": False,
        "gpu": True,
        "synopsis": """float fit(float val, float omin, float omax, float nmin, float nmax);""",
        "params": [
            {"name": "val", "type": "float", "req": "Yes", "desc": "Input value to re-map."},
            {"name": "omin", "type": "float", "req": "Yes", "desc": "Minimum bound of original range."},
            {"name": "omax", "type": "float", "req": "Yes", "desc": "Maximum bound of original range."},
            {"name": "nmin", "type": "float", "req": "Yes", "desc": "Minimum bound of target range."},
            {"name": "nmax", "type": "float", "req": "Yes", "desc": "Maximum bound of target range."}
        ],
        "returns": "Clamped, linearly interpolated value in [nmin, nmax].",
        "details": """
Identical to the classic Houdini VEX `fit()` function.
Calculates `nmin + ((val - omin) / (omax - omin)) * (nmax - nmin)` and clamps the result between `nmin` and `nmax`.
If `omin == omax`, returns `nmin`.
""",
        "example": """layer life = context(agents) {
    // Map agent energy (0u to 20u) to particle scale (0.2 to 1.5)
    @pscale = fit(units(@energy), 0.0, 20.0, 0.2, 1.5);
    @Alpha = fit(float(@age), 0.0, 10_000.0, 1.0, 0.1);
}""",
        "see_also": ["fit_log", "clamp", "lerp", "scale"]
    },
    {
        "id": "fit_log",
        "name": "fit_log",
        "category": "Math & Numeric",
        "summary": "Logarithmically maps a positive value from [lo, hi] into the normalized unit interval [0.0, 1.0].",
        "return_type": "float",
        "context": "VEX layer bodies, Views",
        "phase": "Draw",
        "conserved": False,
        "gpu": True,
        "synopsis": """float fit_log(float val, float lo, float hi);""",
        "params": [
            {"name": "val", "type": "float", "req": "Yes", "desc": "Positive scalar value to map."},
            {"name": "lo", "type": "float", "req": "Yes", "desc": "Lower logarithmic domain bound (> 0)."},
            {"name": "hi", "type": "float", "req": "Yes", "desc": "Upper logarithmic domain bound (> lo)."}
        ],
        "returns": "Normalized scalar in [0.0, 1.0].",
        "details": """
Useful for visualizing substance concentrations and population numbers that vary across multiple orders of magnitude.
Matches `scale(log, range: lo..hi)`. Both `lo` and `hi` must be greater than zero.
""",
        "example": """layer ground = context(cells) {
    float t = fit_log(units(@sugar), 1.0, 5_000.0);
    @Cd = ramp("viridis", t);
}""",
        "see_also": ["fit", "ramp", "scale"]
    },
    {
        "id": "clamp",
        "name": "clamp",
        "category": "Math & Numeric",
        "summary": "Constrains a value to lie between a specified lower and upper bound.",
        "return_type": "float",
        "context": "VEX bodies, View layers, Math expressions",
        "phase": "All",
        "conserved": False,
        "gpu": True,
        "synopsis": """float clamp(float val, float min_val, float max_val);""",
        "params": [
            {"name": "val", "type": "float", "req": "Yes", "desc": "Value to clamp."},
            {"name": "min_val", "type": "float", "req": "Yes", "desc": "Minimum allowed value."},
            {"name": "max_val", "type": "float", "req": "Yes", "desc": "Maximum allowed value."}
        ],
        "returns": "`min_val` if `val < min_val`, `max_val` if `val > max_val`, otherwise `val`.",
        "details": "Standard numeric clamping function. Follows IEEE floating point standards; NaN clamps to 0.0.",
        "example": """float fuel = clamp(units(@energy) / 20.0, 0.0, 1.0);""",
        "see_also": ["fit", "min", "max"]
    },
    {
        "id": "lerp",
        "name": "lerp",
        "category": "Math & Numeric",
        "summary": "Computes linear interpolation between two values according to fraction t.",
        "return_type": "float",
        "context": "VEX bodies, Views",
        "phase": "Draw",
        "conserved": False,
        "gpu": True,
        "synopsis": """float lerp(float a, float b, float t);""",
        "params": [
            {"name": "a", "type": "float", "req": "Yes", "desc": "Start value (when t = 0.0)."},
            {"name": "b", "type": "float", "req": "Yes", "desc": "End value (when t = 1.0)."},
            {"name": "t", "type": "float", "req": "Yes", "desc": "Interpolation factor (0.0 to 1.0)."}
        ],
        "returns": "`a * (1.0 - t) + b * t`.",
        "details": "Linear blend between two scalar quantities.",
        "example": """float base_sz = 1.0;
float max_sz = 3.5;
@pscale = lerp(base_sz, max_sz, @ready);""",
        "see_also": ["fit", "clamp"]
    },
    {
        "id": "min",
        "name": "min",
        "category": "Math & Numeric",
        "summary": "Returns the smaller of two numbers or the minimum value over a neighbourhood collection.",
        "return_type": "float | element type",
        "context": "Rules, VEX bodies, Genesis",
        "phase": "All",
        "conserved": False,
        "gpu": True,
        "synopsis": """float min(float a, float b);
amount min(collection c);""",
        "params": [
            {"name": "a / c", "type": "float | collection", "req": "Yes", "desc": "Two values or a collection (e.g. moore.carbon)."}
        ],
        "returns": "The minimum value found.",
        "details": "When applied to a collection such as `moore.sugar`, returns the lowest value among the 8 surrounding cells.",
        "example": """// In a rule condition
when min(moore.carbon) > 5u {
    transfer(carbon, from: self, to: below, amount: 1u) @flow;
}""",
        "see_also": ["max", "sum", "mean"]
    },
    {
        "id": "max",
        "name": "max",
        "category": "Math & Numeric",
        "summary": "Returns the larger of two numbers or the maximum value over a neighbourhood collection.",
        "return_type": "float | element type",
        "context": "Rules, VEX bodies, Genesis",
        "phase": "All",
        "conserved": False,
        "gpu": True,
        "synopsis": """float max(float a, float b);
amount max(collection c);""",
        "params": [
            {"name": "a / c", "type": "float | collection", "req": "Yes", "desc": "Two values or a collection (e.g. vonneumann.heat)."}
        ],
        "returns": "The maximum value found.",
        "details": "When applied to a collection, evaluates across neighbouring grid cells on the start-of-tick snapshot.",
        "example": """let peak = max(moore.value);""",
        "see_also": ["min", "sum", "mean"]
    },
    {
        "id": "abs",
        "name": "abs",
        "category": "Math & Numeric",
        "summary": "Returns the absolute magnitude of a floating-point number.",
        "return_type": "f32",
        "context": "All contexts",
        "phase": "All",
        "conserved": False,
        "gpu": True,
        "synopsis": """f32 abs(f32 x);""",
        "params": [
            {"name": "x", "type": "f32", "req": "Yes", "desc": "Input scalar."}
        ],
        "returns": "Non-negative magnitude `|x|`.",
        "details": "Standard mathematical absolute value.",
        "example": """where abs($bias) > 4.0""",
        "see_also": ["sqrt"]
    },
    {
        "id": "sqrt",
        "name": "sqrt",
        "category": "Math & Numeric",
        "summary": "Returns the positive square root of a floating-point number.",
        "return_type": "f32",
        "context": "Rules, VEX bodies, Patterns",
        "phase": "All",
        "conserved": False,
        "gpu": True,
        "synopsis": """f32 sqrt(f32 x);""",
        "params": [
            {"name": "x", "type": "f32", "req": "Yes", "desc": "Non-negative scalar."}
        ],
        "returns": "Square root of x. Returns 0.0 if x < 0.0.",
        "details": "Protects against domain errors by clamping negative arguments to 0.0.",
        "example": """float dist = sqrt(dx*dx + dy*dy);""",
        "see_also": ["abs", "log"]
    },
    {
        "id": "sin",
        "name": "sin",
        "category": "Math & Numeric",
        "summary": "Computes the trigonometric sine of an angle given in radians.",
        "return_type": "f32",
        "context": "All contexts",
        "phase": "All",
        "conserved": False,
        "gpu": True,
        "synopsis": """f32 sin(f32 angle_rad);""",
        "params": [
            {"name": "angle_rad", "type": "f32", "req": "Yes", "desc": "Angle in radians."}
        ],
        "returns": "Sine value in range [-1.0, 1.0].",
        "details": "Calculates trigonometric sine using WGSL/Rust standard math.",
        "example": """float wave = sin(float(world.tick) * 0.05);""",
        "see_also": ["tanh"]
    },
    {
        "id": "tanh",
        "name": "tanh",
        "category": "Math & Numeric",
        "summary": "Computes the hyperbolic tangent of a float, commonly used as a neural regulation activation curve.",
        "return_type": "f32",
        "context": "All contexts, Regulation genes",
        "phase": "All",
        "conserved": False,
        "gpu": True,
        "synopsis": """f32 tanh(f32 x);""",
        "params": [
            {"name": "x", "type": "f32", "req": "Yes", "desc": "Activation input."}
        ],
        "returns": "Smooth sigmoid S-curve output in range [-1.0, 1.0].",
        "details": "Standard smooth non-linear activation curve for neural regulation in minds.",
        "example": """regulate weigh(curve: tanh) <- [10.0 * food_y] -> steer @weigh_steer;""",
        "see_also": ["sin"]
    },
    {
        "id": "log",
        "name": "log",
        "category": "Math & Numeric",
        "summary": "Calculates the natural logarithm (base e) of a positive float.",
        "return_type": "f32",
        "context": "VEX bodies, Patterns, Rules",
        "phase": "All",
        "conserved": False,
        "gpu": True,
        "synopsis": """f32 log(f32 x);""",
        "params": [
            {"name": "x", "type": "f32", "req": "Yes", "desc": "Input value (> 0.0)."}
        ],
        "returns": "Natural logarithm. Returns 0.0 if x <= 0.0.",
        "details": "Non-finite numbers (NaN, Inf) become 0.0 according to numeric contract §10.1.",
        "example": """float log_density = log(float(@population) + 1.0);""",
        "see_also": ["fit_log", "sqrt"]
    },
    {
        "id": "uniform",
        "name": "uniform",
        "category": "Math & Numeric",
        "summary": "Draws a uniform pseudorandom 2D vector or float within specified bounds using the Philox PRNG stream.",
        "return_type": "vec2 | f32",
        "context": "Genesis only",
        "phase": "Restructure / Genesis",
        "conserved": False,
        "gpu": True,
        "synopsis": """vec2 uniform(x: float..float, y: float..float);
f32 uniform(range: float..float);""",
        "params": [
            {"name": "x", "type": "range", "req": "No", "desc": "Horizontal coordinate range, e.g. 0.0..512.0."},
            {"name": "y", "type": "range", "req": "No", "desc": "Vertical coordinate range, e.g. 0.0..512.0."},
            {"name": "range", "type": "range", "req": "No", "desc": "Single scalar range."}
        ],
        "returns": "Random 2D position vector or random scalar.",
        "details": """
Allowed only inside `genesis { ... }` blocks. Uses deterministic Philox 4x32-10 PRNG.
Guarantees identical placement across CPU reference and GPU contract runs.
""",
        "example": """genesis {
    count: 2_000;
    position = uniform(x: 0.0..512.0, y: 0.0..512.0);
    heading = uniform(range: -3.14159..3.14159);
}""",
        "see_also": ["jitter", "scatter"]
    },
    {
        "id": "jitter",
        "name": "jitter",
        "category": "Math & Numeric",
        "summary": "Perturbs a numeric value with Gaussian noise drawn from the mutation stream.",
        "return_type": "f32",
        "context": "Rulesets / Mutation patterns only",
        "phase": "Mutate",
        "conserved": False,
        "gpu": False,
        "synopsis": """f32 jitter(f32 val, sigma: f32);""",
        "params": [
            {"name": "val", "type": "f32", "req": "Yes", "desc": "Base parameter value."},
            {"name": "sigma", "type": "f32", "req": "Yes", "desc": "Standard deviation of the perturbation."}
        ],
        "returns": "`val + Normal(0, sigma)`.",
        "details": "Used exclusively in `ruleset` genome rewrite patterns to mutate numeric parameters during reproduction.",
        "example": """ruleset Tinker {
    rule nudge weight 0.3 : sense $kind(dist: $d) -> sense $kind(dist: jitter($d, sigma: 0.1));
}""",
        "see_also": ["uniform"]
    },

    # --- Type Conversions ---
    {
        "id": "units",
        "name": "units",
        "category": "Type Conversions",
        "summary": "Converts a substance-parameterized amount[s] into a dimensionless f32 in whole units (1u = 1.0).",
        "return_type": "f32",
        "context": "All contexts",
        "phase": "All",
        "conserved": False,
        "gpu": True,
        "synopsis": """f32 units(amount[s] a);""",
        "params": [
            {"name": "a", "type": "amount[s]", "req": "Yes", "desc": "Typed substance amount (e.g. self.carbon, @body)."}
        ],
        "returns": "Dimensionless float representation where 1u = 1.0 and 1mu = 1e-6.",
        "details": """
Emerge enforces strict dimensional typing: you cannot mix substances (`self.carbon + self.light` is `E0201`), nor compare amounts directly with floats.
`units()` strips the substance tag and scales micro-units to floating point for mathematical calculations and visual styling.
""",
        "example": """// Convert body substance amount to float for visual glyph sizing
float body_units = units(@body);
@pscale = 1.0 + body_units / 4.0;""",
        "see_also": ["amount", "float"]
    },
    {
        "id": "amount",
        "name": "amount",
        "category": "Type Conversions",
        "summary": "Constructs a typed amount[s] from a floating-point unit count.",
        "return_type": "amount[s]",
        "context": "All contexts",
        "phase": "All",
        "conserved": False,
        "gpu": True,
        "synopsis": """amount[s] amount(substance s, units: f32);""",
        "params": [
            {"name": "s", "type": "substance", "req": "Yes", "desc": "The declared substance that tags the resulting amount."},
            {"name": "units", "type": "f32", "req": "Yes", "desc": "Float quantity in units."}
        ],
        "returns": "Substance-typed amount (clamped to whole micro-units, saturated to u32 max).",
        "details": "Inverse of `units()`. Floors to nearest integer micro-unit (µ). Negative values become `0mu`.",
        "example": """amount[carbon] req = amount(carbon, units: 2.5); // 2_500_000mu""",
        "see_also": ["units", "float"]
    },
    {
        "id": "float",
        "name": "float",
        "category": "Type Conversions",
        "summary": "Explicitly casts an integer (i32) to a floating-point number (f32).",
        "return_type": "f32",
        "context": "All contexts",
        "phase": "All",
        "conserved": False,
        "gpu": True,
        "synopsis": """f32 float(int i);""",
        "params": [
            {"name": "i", "type": "int", "req": "Yes", "desc": "Integer value."}
        ],
        "returns": "32-bit floating-point value.",
        "details": "Because Emerge has no implicit type casting (`E0104`), `float()` is required to convert integers (e.g. `@age`, `tick`, `count`).",
        "example": """size: 1.0 + float(expressed_genes) / 4.0;""",
        "see_also": ["units", "amount"]
    },

    # --- Spatial & Grid Analysis ---
    {
        "id": "cap",
        "name": "cap",
        "category": "Spatial & Grid",
        "summary": "Queries the maximum capacity cap declared for a substance on the rule's field.",
        "return_type": "amount[s]",
        "context": "World rules, Genesis",
        "phase": "All",
        "conserved": True,
        "gpu": True,
        "synopsis": """amount[s] cap(substance s);""",
        "params": [
            {"name": "s", "type": "substance", "req": "Yes", "desc": "Substance name."}
        ],
        "returns": "Maximum amount of substance s a single cell of this field can hold.",
        "details": "Useful in rule conditions to avoid pushing matter into full cells.",
        "example": """when below.grain < cap(grain) {
    transfer(grain, from: self, to: below, amount: 1u) @transfer_grain;
}""",
        "see_also": ["transfer", "units"]
    },
    {
        "id": "argmin",
        "name": "argmin",
        "category": "Spatial & Grid",
        "summary": "Finds the neighbouring cell in a collection with the smallest value.",
        "return_type": "cell store",
        "context": "World rules",
        "phase": "Move",
        "conserved": False,
        "gpu": True,
        "synopsis": """store argmin(collection c);""",
        "params": [
            {"name": "c", "type": "collection", "req": "Yes", "desc": "Neighbourhood collection (e.g. moore.height)."}
        ],
        "returns": "Cell reference pointing to the neighbour with minimum value.",
        "details": "Ties are broken deterministically in fixed cardinal order: North, East, South, West, North-East, South-East, South-West, North-West.",
        "example": """let lowest = argmin(moore.height);
transfer(water, from: self, to: lowest, amount: 1u) @flow_downhill;""",
        "see_also": ["argmax", "min"]
    },
    {
        "id": "argmax",
        "name": "argmax",
        "category": "Spatial & Grid",
        "summary": "Finds the neighbouring cell in a collection with the largest value.",
        "return_type": "cell store",
        "context": "World rules",
        "phase": "Move",
        "conserved": False,
        "gpu": True,
        "synopsis": """store argmax(collection c);""",
        "params": [
            {"name": "c", "type": "collection", "req": "Yes", "desc": "Neighbourhood collection (e.g. moore.light)."}
        ],
        "returns": "Cell reference pointing to the neighbour with maximum value.",
        "details": "Follows deterministic cardinal tie-breaking order.",
        "example": """let brightest = argmax(moore.light);""",
        "see_also": ["argmin", "max"]
    },
    {
        "id": "blur",
        "name": "blur",
        "category": "Spatial & Grid",
        "summary": "Applies a spatial 2D smoothing filter across von Neumann neighbours on a signal field.",
        "return_type": "f32",
        "context": "Signal rules",
        "phase": "Set",
        "conserved": False,
        "gpu": True,
        "synopsis": """f32 blur(signal_field f, amount: f32);""",
        "params": [
            {"name": "f", "type": "field", "req": "Yes", "desc": "The signal field identifier."},
            {"name": "amount", "type": "f32", "req": "Yes", "desc": "Filter strength between 0.0 (no blur) and 1.0 (full neighbour average)."}
        ],
        "returns": "` (1.0 - amount) * self.value + amount * mean(von Neumann neighbours) `.",
        "details": "Reads the start-of-rule snapshot state. Wraps edges if the space declares `wrap: true`.",
        "example": """rule smell on scent {
    self.value = fade(blur(scent, amount: 0.2), keep: 0.9);
}""",
        "see_also": ["fade", "diffuse_conserved"]
    },
    {
        "id": "fade",
        "name": "fade",
        "category": "Spatial & Grid",
        "summary": "Multiplies a signal by an attenuation keep factor, simulating exponential decay.",
        "return_type": "f32",
        "context": "Signal rules",
        "phase": "Set",
        "conserved": False,
        "gpu": True,
        "synopsis": """f32 fade(f32 val, keep: f32);""",
        "params": [
            {"name": "val", "type": "f32", "req": "Yes", "desc": "Input signal value."},
            {"name": "keep", "type": "f32", "req": "Yes", "desc": "Retention factor (0.0 to 1.0)."}
        ],
        "returns": "`val * keep`.",
        "details": "Convenience helper for signal dissipation over successive ticks.",
        "example": """self.value = fade(self.value, keep: 0.95);""",
        "see_also": ["blur"]
    },

    # --- Reductions ---
    {
        "id": "sum",
        "name": "sum",
        "category": "Reductions",
        "summary": "Calculates the total sum over all items in a neighbourhood collection.",
        "return_type": "element type",
        "context": "World rules",
        "phase": "All",
        "conserved": False,
        "gpu": True,
        "synopsis": """f32 sum(collection c);""",
        "params": [
            {"name": "c", "type": "collection", "req": "Yes", "desc": "Moore (8) or von Neumann (4) neighbourhood collection (e.g. moore.value, vonneumann.carbon)."}
        ],
        "returns": "Sum of all surrounding cells.",
        "details": "Evaluates concurrently over the snapshot grid. If the space does not wrap, cells outside the boundary contribute 0.",
        "example": """rule life on board {
    let n = sum(moore.value);
    self.value = select(when: n == 3.0 or (self.value > 0.5 and n == 2.0), then: 1.0, else: 0.0);
}""",
        "see_also": ["mean", "count", "min", "max"]
    },
    {
        "id": "mean",
        "name": "mean",
        "category": "Reductions",
        "summary": "Calculates the arithmetic average of values in a neighbourhood collection.",
        "return_type": "f32",
        "context": "World rules",
        "phase": "All",
        "conserved": False,
        "gpu": True,
        "synopsis": """f32 mean(collection c);""",
        "params": [
            {"name": "c", "type": "collection", "req": "Yes", "desc": "Neighbourhood collection."}
        ],
        "returns": "Average value of surrounding cells.",
        "details": "Calculates sum divided by the number of valid present neighbour cells.",
        "example": """let avg_temp = mean(moore.heat);""",
        "see_also": ["sum", "min", "max"]
    },
    {
        "id": "count",
        "name": "count",
        "category": "Reductions",
        "summary": "Counts how many neighbours in a collection satisfy a boolean predicate condition.",
        "return_type": "int",
        "context": "World rules",
        "phase": "All",
        "conserved": False,
        "gpu": True,
        "synopsis": """int count(collection_condition cond);""",
        "params": [
            {"name": "cond", "type": "bool expression", "req": "Yes", "desc": "Condition evaluated per neighbour, e.g. `moore.value > 0.5` or `moore.carbon > 10u`."}
        ],
        "returns": "Integer count of matching cells (0 to 8 for Moore, 0 to 4 for von Neumann).",
        "details": "Evaluates in exact integers. Widely used for cellular automata and clustering rules.",
        "example": """let live_neighbours = count(moore.value > 0.5);""",
        "see_also": ["sum", "life_like"]
    },

    # --- Cellular Automata & State Fields ---
    {
        "id": "life_like",
        "name": "life_like",
        "category": "Cellular Automata",
        "summary": "Executes an outer-totalistic cellular automaton update rule on a 2-state discrete field.",
        "return_type": "void (effect)",
        "context": "State field rules",
        "phase": "Set",
        "conserved": False,
        "gpu": True,
        "synopsis": """void life_like(born: list[int], survives: list[int]) @label;""",
        "params": [
            {"name": "born", "type": "list[int]", "req": "Yes", "desc": "List of live Moore neighbour counts (0 to 8) that cause a dead cell to become alive."},
            {"name": "survives", "type": "list[int]", "req": "Yes", "desc": "List of live Moore neighbour counts (0 to 8) that allow an alive cell to remain alive."}
        ],
        "returns": "Transitions cell state between dead and alive according to standard Conway B.../S... rules.",
        "details": """
Operates on state fields declared with two states: `field life : Grid(...) states [dead, alive];`.
Runs in the **Set phase** alongside signal rules. Integer arithmetic only, guaranteeing bit-exact execution.
""",
        "example": """// Standard Conway's Game of Life (B3/S23)
rule conway on life {
    life_like(born: [3], survives: [2, 3]) @life;
}""",
        "see_also": ["pattern", "scatter", "count"]
    },
    {
        "id": "pattern",
        "name": "pattern",
        "category": "Cellular Automata",
        "summary": "Initializes a state field with a standard Run-Length Encoded (RLE) bit pattern during genesis.",
        "return_type": "void",
        "context": "Genesis only",
        "phase": "Genesis",
        "conserved": False,
        "gpu": True,
        "synopsis": """pattern(field f, x: int, y: int, rle: string, to: state) @label;""",
        "params": [
            {"name": "f", "type": "field", "req": "Yes", "desc": "Target state field."},
            {"name": "x", "type": "int", "req": "Yes", "desc": "Top-left grid column coordinate."},
            {"name": "y", "type": "int", "req": "Yes", "desc": "Top-left grid row coordinate."},
            {"name": "rle", "type": "string", "req": "Yes", "desc": "RLE string ('b' off, 'o' on, '$' next line, '!' end)."},
            {"name": "to", "type": "state", "req": "Yes", "desc": "State to apply to 'o' cells (e.g. alive)."}
        ],
        "returns": "Sets matching grid cells to the specified state.",
        "details": "Parses standard Life RLE strings at time zero.",
        "example": """genesis {
    // Inject a glider at (10, 10)
    pattern(life, x: 10, y: 10, rle: "bo$2bo$3o!", to: alive) @glider;
}""",
        "see_also": ["scatter", "life_like"]
    },
    {
        "id": "scatter",
        "name": "scatter",
        "category": "Cellular Automata",
        "summary": "Randomly populates state field cells with a designated state at prescribed probability density.",
        "return_type": "void",
        "context": "Genesis only",
        "phase": "Genesis",
        "conserved": False,
        "gpu": True,
        "synopsis": """scatter(field f, density: f32, to: state) @label;""",
        "params": [
            {"name": "f", "type": "field", "req": "Yes", "desc": "Target state field."},
            {"name": "density", "type": "f32", "req": "Yes", "desc": "Probability between 0.0 and 1.0."},
            {"name": "to", "type": "state", "req": "Yes", "desc": "Target state token (e.g. alive)."}
        ],
        "returns": "Sets cells to target state with probability density.",
        "details": "Uses deterministic Philox random draws per cell during genesis.",
        "example": """genesis {
    scatter(life, density: 0.35, to: alive) @random_soup;
}""",
        "see_also": ["pattern", "life_like"]
    },

    # --- Logic & Control Flow ---
    {
        "id": "select",
        "name": "select",
        "category": "Logic & Control",
        "summary": "Functional ternary conditional selection expression.",
        "return_type": "type of then",
        "context": "Signal rules, Expressions",
        "phase": "All",
        "conserved": False,
        "gpu": True,
        "synopsis": """val select(when: bool, then: val, else: val);""",
        "params": [
            {"name": "when", "type": "bool", "req": "Yes", "desc": "Condition expression."},
            {"name": "then", "type": "any", "req": "Yes", "desc": "Value returned when condition is true."},
            {"name": "else", "type": "any", "req": "Yes", "desc": "Value returned when condition is false."}
        ],
        "returns": "Either the then or else value. Both arms must have matching types.",
        "details": "Pure straight-line functional conditional without branching control flow.",
        "example": """self.value = select(when: n == 3.0, then: 1.0, else: 0.0);""",
        "see_also": ["when", "if"]
    },
    {
        "id": "when",
        "name": "when",
        "category": "Logic & Control",
        "summary": "Block-level conditional statement in world rules and genesis.",
        "return_type": "statement",
        "context": "World rules, Genesis",
        "phase": "All",
        "conserved": False,
        "gpu": True,
        "synopsis": """when (condition) {
    statements;
} else {
    statements;
}""",
        "params": [
            {"name": "condition", "type": "bool", "req": "Yes", "desc": "Condition evaluated on start-of-tick snapshot."}
        ],
        "returns": "Executes enclosed statements of the rule's phase if true.",
        "details": """
All reads in the condition see the snapshot. Branches must contain statements matching the enclosing rule's single kind (e.g. transfers, or reactions).
Cannot mix kinds inside branches (`E0303`).
""",
        "example": """when self.grain > 0u and below.grain < cap(grain) {
    transfer(grain, from: self, to: below, amount: 1u) @transfer_grain;
}""",
        "see_also": ["select", "if"]
    },
    {
        "id": "if",
        "name": "if",
        "category": "Logic & Control",
        "summary": "Imperative conditional statement in VEX layer bodies.",
        "return_type": "statement",
        "context": "VEX layer bodies",
        "phase": "Draw",
        "conserved": False,
        "gpu": True,
        "synopsis": """if (condition) {
    statements;
} else {
    statements;
}""",
        "params": [
            {"name": "condition", "type": "bool", "req": "Yes", "desc": "Boolean test expression."}
        ],
        "returns": "Conditionally executes statements.",
        "details": "Standard C/VEX-style branching in visualization layers.",
        "example": """if (@age > 500) {
    @Cd = {0.8, 0.2, 0.2};
} else {
    @Cd = {0.2, 0.8, 0.3};
}""",
        "see_also": ["when", "select"]
    },

    # --- Agent Senses ---
    {
        "id": "inventory",
        "name": "inventory",
        "category": "Agent Senses",
        "summary": "Senses the current agent's stored internal inventory of a specified substance.",
        "return_type": "f32 (in units)",
        "context": "Sense menus, Genomes",
        "phase": "Set",
        "conserved": False,
        "gpu": True,
        "synopsis": """sense inventory(s: substance) -> signal;""",
        "params": [
            {"name": "s", "type": "substance", "req": "Yes", "desc": "Target substance (e.g. body, energy, carbon)."}
        ],
        "returns": "Amount held in inventory scaled to units float.",
        "details": "Reads the agent's start-of-tick snapshot inventory.",
        "example": """sense inventory(s: body) -> fullness @inventory_fullness;""",
        "see_also": ["gradient", "probe", "neighbour"]
    },
    {
        "id": "gradient",
        "name": "gradient",
        "category": "Agent Senses",
        "summary": "Senses the spatial gradient of an environmental substance or signal field relative to agent orientation.",
        "return_type": "signal_x, signal_y",
        "context": "Sense menus, Genomes",
        "phase": "Set",
        "conserved": False,
        "gpu": True,
        "synopsis": """sense gradient(q: quantity) -> forward_sig, left_sig;""",
        "params": [
            {"name": "q", "type": "quantity", "req": "Yes", "desc": "Target field quantity (e.g. cells.body, scent)."}
        ],
        "returns": "Two signals: `.x` along the forward heading, `.y` towards the creature's left.",
        "details": "Enables chemotaxis and gradient ascent/descent behaviour.",
        "example": """sense gradient(q: cells.body) -> food_x, food_y @food_dir;""",
        "see_also": ["probe", "inventory"]
    },
    {
        "id": "probe",
        "name": "probe",
        "category": "Agent Senses",
        "summary": "Samples an environmental quantity at a polar offset raycast from the creature.",
        "return_type": "signal",
        "context": "Sense menus, Genomes",
        "phase": "Set",
        "conserved": False,
        "gpu": True,
        "synopsis": """sense probe(q: quantity, angle_deg: range, dist: range) -> signal;""",
        "params": [
            {"name": "q", "type": "quantity", "req": "Yes", "desc": "Target field quantity."},
            {"name": "angle_deg", "type": "range", "req": "Yes", "desc": "Polar angle offset relative to heading (-180.0..180.0)."},
            {"name": "dist", "type": "range", "req": "Yes", "desc": "Sampling distance radius (e.g. 0.0..8.0)."}
        ],
        "returns": "Bilinearly interpolated field value at the probe coordinate.",
        "details": "Simulates directional antenna or eye sensors.",
        "example": """sense probe(q: cells.light, angle_deg: 20.0, dist: 4.0) -> light_ahead @probe_light;""",
        "see_also": ["gradient", "crowd"]
    },
    {
        "id": "crowd",
        "name": "crowd",
        "category": "Agent Senses",
        "summary": "Senses the local agent density (number of other creatures) within an interaction radius.",
        "return_type": "signal",
        "context": "Sense menus, Genomes",
        "phase": "Set",
        "conserved": False,
        "gpu": True,
        "synopsis": """sense crowd(radius: range) -> signal;""",
        "params": [
            {"name": "radius", "type": "range", "req": "Yes", "desc": "Detection radius (0.0..6.0)."}
        ],
        "returns": "Count of nearby creatures as a float signal.",
        "details": "Used for flocking, avoidance, and density-dependent reproduction control.",
        "example": """sense crowd(radius: 4.0) -> density @sense_density;""",
        "see_also": ["neighbour", "probe"]
    },
    {
        "id": "neighbour",
        "name": "neighbour",
        "category": "Agent Senses",
        "summary": "Inspects the inventory of nearby creatures ordered by proximity rank.",
        "return_type": "signal",
        "context": "Sense menus, Genomes",
        "phase": "Set",
        "conserved": False,
        "gpu": True,
        "synopsis": """sense neighbour(s: substance, rank: 0..3) -> signal;""",
        "params": [
            {"name": "s", "type": "substance", "req": "Yes", "desc": "Substance to inspect."},
            {"name": "rank", "type": "int", "req": "Yes", "desc": "0 for closest neighbour, 1 for second closest, etc."}
        ],
        "returns": "Target creature's substance amount in units float.",
        "details": "Enables predatory target assessment and symbiotic resource exchange.",
        "example": """sense neighbour(s: energy, rank: 0) -> prey_energy @inspect_target;""",
        "see_also": ["crowd", "steal"]
    },
    {
        "id": "clock",
        "name": "clock",
        "category": "Agent Senses",
        "summary": "Provides a periodic cyclical biological clock signal oscillation.",
        "return_type": "signal",
        "context": "Sense menus, Genomes",
        "phase": "Set",
        "conserved": False,
        "gpu": True,
        "synopsis": """sense clock(period: 1..1000) -> signal;""",
        "params": [
            {"name": "period", "type": "int", "req": "Yes", "desc": "Period in ticks."}
        ],
        "returns": "Periodic triangular or sinusoidal signal in range [0.0, 1.0].",
        "details": "Allows evolved organisms to synchronize metabolic or locomotor cycles.",
        "example": """sense clock(period: 100) -> pace @internal_clock;""",
        "see_also": ["delta"]
    },
    {
        "id": "delta",
        "name": "delta",
        "category": "Agent Senses",
        "summary": "Senses the signed rate of change in an internal quantity between ticks.",
        "return_type": "signal",
        "context": "Sense menus, Genomes",
        "phase": "Set",
        "conserved": False,
        "gpu": True,
        "synopsis": """sense delta(q: quantity) -> signal;""",
        "params": [
            {"name": "q", "type": "quantity", "req": "Yes", "desc": "Quantity to differentiate."}
        ],
        "returns": "Signed change `q(t) - q(t-1)` as float.",
        "details": "Evaluates exact difference in contract mode.",
        "example": """sense delta(body) -> body_growth @growth_rate;""",
        "see_also": ["clock", "inventory"]
    },
    {
        "id": "has_gene",
        "name": "has_gene",
        "category": "Agent Senses",
        "summary": "Queries whether the current entity expresses a gene of a given kind.",
        "return_type": "bool",
        "context": "VEX contexts, Queries",
        "phase": "All",
        "conserved": False,
        "gpu": True,
        "synopsis": """bool has_gene(react: reaction_name);
bool has_gene(take: substance_name);""",
        "params": [
            {"name": "kind", "type": "named reference", "req": "Yes", "desc": "Gene type and parameter."}
        ],
        "returns": "True if an active, non-dormant gene matching the query is present.",
        "details": "Allows classification of agents based on genetic capabilities.",
        "example": """if (has_gene(react: build)) {
    @Cd = {0.2, 0.8, 0.2}; // green for builders
}""",
        "see_also": ["genes", "active_genes", "made"]
    },
    {
        "id": "genes",
        "name": "genes",
        "category": "Agent Senses",
        "summary": "Returns the total number of genes in the creature's genome.",
        "return_type": "int",
        "context": "VEX contexts, Views",
        "phase": "All",
        "conserved": False,
        "gpu": True,
        "synopsis": """int genes();""",
        "params": [],
        "returns": "Total gene count (including dormant genes).",
        "details": "Useful for complexity metrics and visual scaling.",
        "example": """int total = genes();""",
        "see_also": ["active_genes", "has_gene"]
    },
    {
        "id": "active_genes",
        "name": "active_genes",
        "category": "Agent Senses",
        "summary": "Returns the number of expressed (non-dormant) genes in the creature's genome.",
        "return_type": "int",
        "context": "VEX contexts, Views",
        "phase": "All",
        "conserved": False,
        "gpu": True,
        "synopsis": """int active_genes();""",
        "params": [],
        "returns": "Active gene count. Matches `@expressed_genes`.",
        "details": "Directly correlates with upkeep cost (`upkeep.per_gene`).",
        "example": """@pscale = 1.0 + float(active_genes()) * 0.1;""",
        "see_also": ["genes", "has_gene"]
    },
    {
        "id": "made",
        "name": "made",
        "category": "Agent Senses",
        "summary": "Queries the quantity of a substance synthesized by the agent's own reactions over a recent sliding window.",
        "return_type": "amount[s]",
        "context": "VEX contexts, Views",
        "phase": "Draw / Query",
        "conserved": True,
        "gpu": True,
        "synopsis": """amount[s] made(substance s, window: int);""",
        "params": [
            {"name": "s", "type": "substance", "req": "Yes", "desc": "Counted product substance."},
            {"name": "window", "type": "int", "req": "Yes", "desc": "Tick count window (e.g. 1000)."}
        ],
        "returns": "Total micro-units of substance s synthesized by the creature's reactions during the window.",
        "details": "Distinguishes true primary producers from scavengers and parasites.",
        "example": """bool is_producer = units(made(body, window: 1000)) > 0.0;""",
        "see_also": ["has_gene", "inventory"]
    },
    {
        "id": "lineage",
        "name": "lineage",
        "category": "Agent Senses",
        "summary": "Returns the founding ancestral lineage identifier of the entity.",
        "return_type": "lineage_id",
        "context": "VEX contexts, Views",
        "phase": "All",
        "conserved": False,
        "gpu": True,
        "synopsis": """lineage lineage();
lineage lineage(id entity_id);""",
        "params": [
            {"name": "entity_id", "type": "id", "req": "No", "desc": "Entity ID (defaults to current creature)."}
        ],
        "returns": "Lineage token (e.g. Drifter). Matches `@lineage`.",
        "details": "Persists across asexual reproduction generations.",
        "example": """@Cd = palette(categorical, lineage());""",
        "see_also": ["made", "has_gene"]
    },

    # --- Agent Actions ---
    {
        "id": "move",
        "name": "move",
        "category": "Agent Actions",
        "summary": "Propels the creature forward along its heading, paying an action fee proportional to distance.",
        "return_type": "action",
        "context": "Action menus, Genomes",
        "phase": "Move",
        "conserved": False,
        "gpu": True,
        "synopsis": """action move(speed: range) cost fee per distance;""",
        "params": [
            {"name": "speed", "type": "range", "req": "Yes", "desc": "Forward velocity (e.g. 0.0..2.0)."},
            {"name": "fee", "type": "amount", "req": "Yes", "desc": "Upkeep pay substance charged per unit distance."}
        ],
        "returns": "Displaces creature position at step 2(d) of Move phase.",
        "details": "If the entity cannot afford the action fee, the intent is dropped and no movement occurs.",
        "example": """action move(speed: 1.0) @go;""",
        "see_also": ["turn", "take", "drop"]
    },
    {
        "id": "turn",
        "name": "turn",
        "category": "Agent Actions",
        "summary": "Rotates the creature's heading orientation.",
        "return_type": "action",
        "context": "Action menus, Genomes",
        "phase": "Move",
        "conserved": False,
        "gpu": True,
        "synopsis": """action turn(angle_deg: range) cost fee;""",
        "params": [
            {"name": "angle_deg", "type": "range", "req": "Yes", "desc": "Maximum angular deflection in degrees (0.0..30.0)."},
            {"name": "cost", "type": "amount", "req": "Yes", "desc": "Fixed action fee."}
        ],
        "returns": "Rotates heading vector.",
        "details": "Driven by an input steering signal.",
        "example": """action turn(angle_deg: 30.0) <- steer @turn_to_food;""",
        "see_also": ["move"]
    },
    {
        "id": "take_action",
        "name": "take (action)",
        "category": "Agent Actions",
        "summary": "Harvests an amount of substance from the underlying grid cell into the creature's inventory.",
        "return_type": "action",
        "context": "Action menus, Genomes",
        "phase": "Move",
        "conserved": True,
        "gpu": True,
        "synopsis": """action take(s: substance, max: range) cost fee;""",
        "params": [
            {"name": "s", "type": "substance", "req": "Yes", "desc": "Target substance to harvest."},
            {"name": "max", "type": "range", "req": "Yes", "desc": "Maximum amount to take per tick (e.g. 0u..10u)."},
            {"name": "cost", "type": "amount", "req": "Yes", "desc": "Action fee paid in upkeep currency."}
        ],
        "returns": "Submits a transfer request from cell to entity in the Move phase.",
        "details": """
Arbitrated concurrently with all other cell transfers.
The entity takes from the cell it started the tick in (start-of-tick snapshot).
""",
        "example": """action take(s: carbon, max: 2u) cost 5mu @eat_carbon;""",
        "see_also": ["drop", "give_action", "steal"]
    },
    {
        "id": "drop",
        "name": "drop",
        "category": "Agent Actions",
        "summary": "Discharges an amount of substance from inventory into the underlying grid cell.",
        "return_type": "action",
        "context": "Action menus, Genomes",
        "phase": "Move",
        "conserved": True,
        "gpu": True,
        "synopsis": """action drop(s: substance, max: range) cost fee;""",
        "params": [
            {"name": "s", "type": "substance", "req": "Yes", "desc": "Substance to drop."},
            {"name": "max", "type": "range", "req": "Yes", "desc": "Maximum quantity to deposit."}
        ],
        "returns": "Transfers substance from entity inventory to cell.",
        "details": "If the destination cell lacks capacity, unshed matter stays with the creature.",
        "example": """action drop(s: waste, max: 5u) cost 0u @deposit_waste;""",
        "see_also": ["take_action", "give_action"]
    },
    {
        "id": "give_action",
        "name": "give (action)",
        "category": "Agent Actions",
        "summary": "Transfers substance directly from inventory into a targeted neighbour creature.",
        "return_type": "action",
        "context": "Action menus, Genomes",
        "phase": "Move",
        "conserved": True,
        "gpu": True,
        "synopsis": """action give(s: substance, max: range, to: neighbour) cost fee;""",
        "params": [
            {"name": "s", "type": "substance", "req": "Yes", "desc": "Substance to donate."},
            {"name": "max", "type": "range", "req": "Yes", "desc": "Maximum donation ceiling."}
        ],
        "returns": "Transfers matter to neighbour in Move phase.",
        "details": "If the target neighbour is dying or lacks inventory headroom, the transfer is clipped.",
        "example": """action give(s: energy, max: 2u, to: neighbour) cost 5mu @share_food;""",
        "see_also": ["take_action", "steal"]
    },
    {
        "id": "steal",
        "name": "steal",
        "category": "Agent Actions",
        "summary": "Forcibly extracts substance from a targeted neighbour creature into the actor's inventory.",
        "return_type": "action",
        "context": "Action menus, Genomes",
        "phase": "Move",
        "conserved": True,
        "gpu": True,
        "synopsis": """action steal(s: substance, max: range) cost fee;""",
        "params": [
            {"name": "s", "type": "substance", "req": "Yes", "desc": "Target substance to siphon."},
            {"name": "max", "type": "range", "req": "Yes", "desc": "Maximum stolen quantity."}
        ],
        "returns": "Transfers matter from victim to thief.",
        "details": "Fails completely if the victim is currently firing an active `guard(s)` action.",
        "example": """action steal(s: energy, max: 1u) cost 20mu @predate;""",
        "see_also": ["guard", "take_action"]
    },
    {
        "id": "guard",
        "name": "guard",
        "category": "Agent Actions",
        "summary": "Active defense mechanism shielding the creature from theft attempts of a specified substance.",
        "return_type": "action",
        "context": "Action menus, Genomes",
        "phase": "Move",
        "conserved": False,
        "gpu": True,
        "synopsis": """action guard(s: substance) cost fee;""",
        "params": [
            {"name": "s", "type": "substance", "req": "Yes", "desc": "Protected substance."}
        ],
        "returns": "Shields substance for the duration of the tick.",
        "details": "While guard is active, all `steal` actions targeting this substance on the entity fail.",
        "example": """action guard(s: energy) cost 20mu @defend;""",
        "see_also": ["steal"]
    },
    {
        "id": "divide",
        "name": "divide",
        "category": "Agent Actions",
        "summary": "Triggers asexual fission reproduction, partitioning inventory with an offspring.",
        "return_type": "action",
        "context": "Action menus, Genomes",
        "phase": "Restructure",
        "conserved": True,
        "gpu": True,
        "synopsis": """action divide(split: range) cost fee requires condition;""",
        "params": [
            {"name": "split", "type": "range", "req": "Yes", "desc": "Fraction of inventory bestowed to child (e.g. 0.0..1.0, typically 0.5)."},
            {"name": "requires", "type": "bool expression", "req": "No", "desc": "Precondition check (e.g. `requires body >= 10u`)."}
        ],
        "returns": "Spawns a new creature entity in Phase 4 (Restructure).",
        "details": """
Division checks inventory after all reactions in Transform have completed.
The child gets `floor(balance * split)` of every substance; the parent retains the rest.
The child is placed at distance `child_offset` behind the parent.
""",
        "example": """action divide(split: 0.5) cost 0u requires body >= 10u @split;""",
        "see_also": ["move", "drop"]
    },
    {
        "id": "emit",
        "name": "emit",
        "category": "Agent Actions",
        "summary": "Deposits a non-conserved value onto a signal field at the creature's location.",
        "return_type": "action",
        "context": "Action menus, Genomes",
        "phase": "Set",
        "conserved": False,
        "gpu": False,
        "synopsis": """action emit(sig: signal_field, max: range) cost fee;""",
        "params": [
            {"name": "sig", "type": "field", "req": "Yes", "desc": "Target signal field."},
            {"name": "max", "type": "range", "req": "Yes", "desc": "Maximum signal magnitude."}
        ],
        "returns": "Writes to the signal field using the field's declared combine rule (`max`, `sum`, `min`).",
        "details": "Simulates pheromone trails and broadcasting beacons.",
        "example": """action emit(sig: scent, max: 10.0) cost 2mu @leave_scent;""",
        "see_also": ["blur", "fade"]
    },

    # --- Visualization & Render ---
    {
        "id": "ramp",
        "name": "ramp",
        "category": "Visualization",
        "summary": "Evaluates a procedural color colormap at normalized position t.",
        "return_type": "color",
        "context": "VEX layer bodies",
        "phase": "Draw",
        "conserved": False,
        "gpu": True,
        "synopsis": """color ramp(string palette_name, float t);""",
        "params": [
            {"name": "palette_name", "type": "string", "req": "Yes", "desc": "Palette name: viridis, magma, inferno, cividis, greens, blues, reds, rdbu, phase."},
            {"name": "t", "type": "float", "req": "Yes", "desc": "Normalized position in [0.0, 1.0]."}
        ],
        "returns": "RGB color vector `{r, g, b}`.",
        "details": "Matches Houdini VEX color ramps.",
        "example": """@Cd = ramp("magma", fit(units(@energy), 0.0, 20.0, 0.0, 1.0));""",
        "see_also": ["palette", "fit"]
    },
    {
        "id": "palette",
        "name": "palette",
        "category": "Visualization",
        "summary": "Selects a color palette generator for pipeline pipes in view layers.",
        "return_type": "pipeline call",
        "context": "View layer declarations",
        "phase": "Draw",
        "conserved": False,
        "gpu": True,
        "synopsis": """color value |> palette(palette_name);""",
        "params": [
            {"name": "palette_name", "type": "ident", "req": "Yes", "desc": "Palette identifier."}
        ],
        "returns": "Color mapping stage in view pipeline.",
        "details": "Available palettes: sequential (greens, blues, reds, grays, viridis, magma, inferno, cividis), diverging (rdbu), cyclic (phase), categorical.",
        "example": """color: value |> scale(log, range: 1.0..5_000.0) |> palette(greens);""",
        "see_also": ["ramp", "scale"]
    },
    {
        "id": "scale",
        "name": "scale",
        "category": "Visualization",
        "summary": "Re-maps a numeric value domain linearly or logarithmically into [0.0, 1.0].",
        "return_type": "pipeline call",
        "context": "View layer declarations",
        "phase": "Draw",
        "conserved": False,
        "gpu": True,
        "synopsis": """float value |> scale(kind, range: min..max);""",
        "params": [
            {"name": "kind", "type": "ident", "req": "Yes", "desc": "linear or log."},
            {"name": "range", "type": "range", "req": "Yes", "desc": "Input domain interval. If log, range must be > 0.0."}
        ],
        "returns": "Normalized scalar stage.",
        "details": "Pipeline syntax equivalent of `fit` / `fit_log`.",
        "example": """scale(linear, range: 0.0..10.0)""",
        "see_also": ["fit", "palette"]
    },
    {
        "id": "arrow",
        "name": "arrow",
        "category": "Visualization",
        "summary": "Specifies an oriented arrow glyph for rendering agents in view layers.",
        "return_type": "glyph",
        "context": "Agent view layers",
        "phase": "Draw",
        "conserved": False,
        "gpu": True,
        "synopsis": """glyph arrow(dir: heading);""",
        "params": [
            {"name": "dir", "type": "direction", "req": "Yes", "desc": "Heading orientation angle or vector."}
        ],
        "returns": "Arrow geometry glyph.",
        "details": "Renders an instanced triangle/arrow pointing along the agent's heading.",
        "example": """glyph: arrow(dir: heading);""",
        "see_also": ["dot"]
    },
    {
        "id": "dot",
        "name": "dot",
        "category": "Visualization",
        "summary": "Specifies a circular dot glyph for rendering agents.",
        "return_type": "glyph",
        "context": "Agent view layers",
        "phase": "Draw",
        "conserved": False,
        "gpu": True,
        "synopsis": """glyph dot();""",
        "params": [],
        "returns": "Circular disc glyph.",
        "details": "Fastest rendering primitive for high agent counts (100k+).",
        "example": """glyph: dot();""",
        "see_also": ["arrow"]
    },
    {
        "id": "field_view",
        "name": "field",
        "category": "Visualization",
        "summary": "Declares a raster background visualization layer over a grid field.",
        "return_type": "layer",
        "context": "View declarations",
        "phase": "Draw",
        "conserved": False,
        "gpu": True,
        "synopsis": """layer name = field(cells.substance) { ... }""",
        "params": [
            {"name": "quantity", "type": "quantity", "req": "Yes", "desc": "Substance or signal field to display."}
        ],
        "returns": "2D background raster layer.",
        "details": "Evaluates per-pixel or per-cell on the display frame.",
        "example": """layer ground = field(cells.sugar) {
    color: value |> scale(log, range: 1.0..5_000.0) |> palette(greens);
}""",
        "see_also": ["agents_view"]
    },
    {
        "id": "agents_view",
        "name": "agents",
        "category": "Visualization",
        "summary": "Declares a foreground particle instance visualization layer over live creatures.",
        "return_type": "layer",
        "context": "View declarations",
        "phase": "Draw",
        "conserved": False,
        "gpu": True,
        "synopsis": """layer name = agents(where: condition) { ... }""",
        "params": [
            {"name": "where", "type": "bool expression", "req": "No", "desc": "Optional filter condition."}
        ],
        "returns": "Foreground entity rendering pass.",
        "details": "Provides access to agent attributes (`@P`, `@heading`, `@id`, inventory).",
        "example": """layer life = agents(where: units(body) > 5.0) {
    glyph: arrow(dir: heading);
    color: species_id |> palette(categorical);
}""",
        "see_also": ["field_view"]
    },
    {
        "id": "line",
        "name": "line",
        "category": "Visualization",
        "summary": "Plots a time-series metric line on the HUD chart overlay.",
        "return_type": "chart entry",
        "context": "Chart declarations",
        "phase": "Draw",
        "conserved": False,
        "gpu": True,
        "synopsis": """line(series [, by: grouping]);""",
        "params": [
            {"name": "series", "type": "metric", "req": "Yes", "desc": "population, births, deaths, or metrics.*."},
            {"name": "by", "type": "ident", "req": "No", "desc": "Breakdown attribute (e.g. lineage)."}
        ],
        "returns": "Time-series strip chart line.",
        "details": "Drawn in the telemetry strip below the main viewport.",
        "example": """chart Health for Dish {
    line(population, by: lineage);
    line(births);
    line(deaths);
}""",
        "see_also": ["ledger"]
    },
    {
        "id": "ledger",
        "name": "ledger",
        "category": "Visualization",
        "summary": "Renders an audit bar of system-wide conservation accounting.",
        "return_type": "chart entry",
        "context": "Chart declarations",
        "phase": "Draw",
        "conserved": True,
        "gpu": True,
        "synopsis": """ledger(elements | substances);""",
        "params": [
            {"name": "target", "type": "ident", "req": "Yes", "desc": "elements or substances."}
        ],
        "returns": "Mass balance verification bar.",
        "details": "Verifies that total elemental inventory equals initial genesis plus inflows minus outflows.",
        "example": """ledger(elements);""",
        "see_also": ["line"]
    },

    # --- Exploration & Sweeps ---
    {
        "id": "linspace",
        "name": "linspace",
        "category": "Exploration",
        "summary": "Generates a sequence of evenly spaced numeric values in linear space.",
        "return_type": "generator",
        "context": "Explore declarations",
        "phase": "Explore",
        "conserved": False,
        "gpu": False,
        "synopsis": """linspace(from: val, to: val, steps: int);""",
        "params": [
            {"name": "from", "type": "val", "req": "Yes", "desc": "Start value."},
            {"name": "to", "type": "val", "req": "Yes", "desc": "End value."},
            {"name": "steps", "type": "int", "req": "Yes", "desc": "Number of sample points."}
        ],
        "returns": "List of sampled values.",
        "details": "Used for parameter sweeps across simulation variants.",
        "example": """vary upkeep.base in linspace(from: 10mu, to: 50mu, steps: 5);""",
        "see_also": ["logspace", "enumerate"]
    },
    {
        "id": "logspace",
        "name": "logspace",
        "category": "Exploration",
        "summary": "Generates a sequence of logarithmically spaced values.",
        "return_type": "generator",
        "context": "Explore declarations",
        "phase": "Explore",
        "conserved": False,
        "gpu": False,
        "synopsis": """logspace(from: val, to: val, steps: int);""",
        "params": [
            {"name": "from", "type": "val", "req": "Yes", "desc": "Start value (> 0)."},
            {"name": "to", "type": "val", "req": "Yes", "desc": "End value (> 0)."},
            {"name": "steps", "type": "int", "req": "Yes", "desc": "Number of logarithmic steps."}
        ],
        "returns": "Logarithmically spaced points.",
        "details": "Ideal for varying rates across orders of magnitude.",
        "example": """vary Dish.rules.rotting@react_rot.rate in logspace(from: 10bp, to: 500bp, steps: 8);""",
        "see_also": ["linspace", "top"]
    },
    {
        "id": "enumerate",
        "name": "enumerate",
        "category": "Exploration",
        "summary": "Iterates through discrete alternatives in an exploration sweep.",
        "return_type": "generator",
        "context": "Explore declarations",
        "phase": "Explore",
        "conserved": False,
        "gpu": False,
        "synopsis": """enumerate(space);""",
        "params": [
            {"name": "space", "type": "list", "req": "Yes", "desc": "Discrete candidate list."}
        ],
        "returns": "Alternative configurations.",
        "details": "Permutes discrete settings.",
        "example": """vary reaction_space.generate.max_coeff in [1, 2, 3];""",
        "see_also": ["top", "atlas"]
    },
    {
        "id": "top",
        "name": "top",
        "category": "Exploration",
        "summary": "Filters the top N highest-ranking runs in an exploration sweep by metric.",
        "return_type": "setting",
        "context": "Explore declarations",
        "phase": "Explore",
        "conserved": False,
        "gpu": False,
        "synopsis": """top(count, by: metric);""",
        "params": [
            {"name": "count", "type": "int", "req": "Yes", "desc": "Number of runs to retain."},
            {"name": "by", "type": "ident", "req": "Yes", "desc": "Ranking metric (novelty, activity, species)."}
        ],
        "returns": "Selects candidate variants for export.",
        "details": "Reduces large parameter sweep datasets to the most interesting clades.",
        "example": """keep: top(12, by: novelty);""",
        "see_also": ["atlas"]
    },
    {
        "id": "atlas",
        "name": "atlas",
        "category": "Exploration",
        "summary": "Schedules periodic visual frame captures during an exploration sweep.",
        "return_type": "setting",
        "context": "Explore declarations",
        "phase": "Explore",
        "conserved": False,
        "gpu": False,
        "synopsis": """atlas(view: view_name, every: ticks);""",
        "params": [
            {"name": "view", "type": "view", "req": "Yes", "desc": "Target view specification."},
            {"name": "every", "type": "int", "req": "Yes", "desc": "Capture interval in ticks."}
        ],
        "returns": "Generates image atlas gallery.",
        "details": "Produces gallery contact sheets for comparing simulation runs.",
        "example": """atlas: atlas(view: Main, every: 5_000);""",
        "see_also": ["top"]
    }
]

GUIDE_TOPICS = [
    {
        "id": "overview",
        "title": "Language Overview & Design Goals",
        "summary": "Core architectural principles of the Emerge (.em) simulation and evolutionary language.",
        "content": """
# The `.em` Language Reference

Emerge is a rule-based language and high-performance simulation engine designed for open-ended evolutionary biology, artificial life, and procedural world synthesis. A world is specified in a few lines of rules over conserved physical substances; autonomous creatures with evolvable genomes inhabit the environment; and thermodynamic conservation, dissipation, and the metabolic cost of living are inviolable laws enforced by the type system.

## 1. Key Design Principles

### 1. One Spelling Per Meaning
The canonical output of `emerge fmt` is the only accepted source form. There are no optional semicolons, redundant syntactic synonyms, or ambiguous formatting orders. Programs are byte-for-byte identical across developers and tools.

### 2. Laws in the Type System
Thermodynamic conservation and entropy dissipation are not runtime suggestions—they are static compile-time contracts:
- **Typed Amounts:** An amount of carbon cannot be added to an amount of light (`self.carbon + self.light` is rejected at compile time with `E0201`).
- **No Free Creation:** There is no assignment to an amount anywhere (`self.carbon = 5u` is `E0302`). Matter moves strictly through `transfer`, `react`, and `diffuse_conserved`, or via explicit agent intents.
- **Second Law of Thermodynamics:** Reactions must declare elemental formulas and potentials; any reaction that is not strictly downhill by at least `min_drop` potential is rejected with `E0402`.

### 3. Small, Strictly Checked Contexts
Every block of code lives in exactly one execution context (Declaration, Transfer Rule, Reaction Rule, Signal Rule, Genesis, Genome, VEX View Layer). Each context has an explicit white-list of what it may read and what physical effects it may produce.

### 4. Direct Lowering to WGSL Straight-Line Code
Simulation rules and neural minds compile to straight-line code with bounded execution. There are no user-written dynamic loops or recursion in anything evaluated during simulation ticks, allowing Emerge worlds to execute with massive parallelism across CPU and GPU hardware.

### 5. Persistent Identity and Live Hot-Swapping
Every declaration, statement, and gene has a readable path and a persistent 64-bit ID. World rules, upkeep rates, and fees can be hot-swapped mid-simulation without invalidating state or restarting the run.
"""
    },
    {
        "id": "lexical",
        "title": "Lexical Structure & Formatting",
        "summary": "Tokens, literals, substance amounts (u/mu), rates (bp), labels (@), and canonical formatting.",
        "content": """
# Lexical Structure & Tokens

Emerge files (`.em`) are UTF-8 encoded text files. The grammar is designed for unambiguous lexical scanning.

## Literals & Formats

| Element | Syntax Form | Meaning & Rules | Examples |
| :--- | :--- | :--- | :--- |
| **Comments** | `// text` | • Single-line comments to end of line<br>• No block comments (`/* */`) | `// rot returns nutrients` |
| **Identifiers** | `[A-Za-z_][A-Za-z0-9_]*` | • Case-sensitive names<br>• Cannot collide with keywords or built-ins | • `photosynthesis`<br>• `Dish`<br>• `max_age` |
| **Amounts** | `<num>u` or `<num>mu` | • Fixed-point unsigned micro-units (`µ` / `mu`)<br>• 1u = 1,000,000mu<br>• Must be whole micro-units (no fractional mu)<br>• Always unsigned and non-negative | • `5u`<br>• `250mu`<br>• `0.5u` (= 500_000mu) |
| **Rates** | `<int>bp` | • Basis points (10_000bp = 100%, 50bp = 0.50%)<br>• Always non-negative integer | • `50bp`<br>• `10_000bp` |
| **Integers** | Digits with `_` separators | • 32-bit signed (`i32`), checked as `i64`<br>• Division by zero evaluates to 0 | • `3`<br>• `20_000` |
| **Floats** | Digits with `.` or exponent | • 32-bit float (`f32`)<br>• Must contain `.` or exponent | • `0.5`<br>• `1e-3`<br>• `30.0` |
| **Ranges** | `bound .. bound` | • Inclusive interval bounds<br>• Used in menus, explore, and parameter limits | • `0..512`<br>• `-30.0..30.0`<br>• `0u..10u` |
| **Labels** | `@identifier` | • Stable target identifier for statements and genes<br>• Used for addressing, hot-swapping, and patching | • `@eat_light`<br>• `@transfer_carbon` |
| **Captures** | `$identifier` | • Parameter capture variable in rewrite rulesets | • `$kind`<br>• `$bias` |
| **Arrows** | `->` and `<-` | • `->` indicates outgoing flow<br>• `<-` indicates incoming flow / driving signal | • `-> steer`<br>• `<- hunger` |

### Rules for Literals
1. **Whole Micro-Units:** An amount must represent a whole number of micro-units (`mu`). `0.5u` is `500mu` and valid; `0.0005u` is `0.5mu` (fractional) and rejected with `E0101`.
2. **Amounts are Non-Negative:** Unary minus is allowed only on `int` and `f32`. `-5u` and `-50bp` are compilation errors (`E0203`).
3. **Digit Grouping:** `emerge fmt` groups digits in threes for numbers of four digits or more (`20_000`, `5_000.0`).
"""
    },
    {
        "id": "types",
        "title": "Value & Reference Types",
        "summary": "Substance-parameterized amounts, rates, primitive numbers, reference types, and saturation rules.",
        "content": """
# Type System

Emerge features a dual type system: **Value Types** representing quantities computed during simulation, and **Reference Types** identifying physical declarations within the world.

## Value Types

| Type | Representation | Literals | Permitted Operations |
| :--- | :--- | :--- | :--- |
| `amount[s]` | `u32` micro-units (`mu`) | • `5u`<br>• `250mu` | • `+`, `-` (with same substance `s`)<br>• `* rate`<br>• `/ int`<br>• Comparisons (`==`, `!=`, `<`, `<=`, `>`, `>=`) |
| `rate` | `u32` basis points (`bp`) | • `50bp`<br>• `1000bp` | • `amount * rate`<br>• Parameter passing in menus and rules |
| `int` | `i32` (folded as `i64`) | • `3`<br>• `20_000` | • Arithmetic: `+`, `-`, `*`, `/`<br>• Comparisons (`==`, `!=`, `<`, `<=`, `>`, `>=`) |
| `f32` | 32-bit float | • `0.5`<br>• `1e-3` | • Arithmetic: `+`, `-`, `*`, `/`<br>• Math built-ins (`sin`, `tanh`, `fit`, `clamp`, `lerp`)<br>• Comparisons |
| `bool` | Boolean truth value | • `true`<br>• `false` | • Logical operators: `and`, `or`, `not`<br>• Conditional branches (`select`, `when`, `if`) |
| `vec2` | 2D vector coordinate | From `uniform(x: ..., y: ...)` | • Assignment to `@P` / `position`<br>• Vector components `.x`, `.y` |
| `string` | Single-line UTF-8 text | `"bo$2bo$3o!"` | • Pattern RLE string input<br>• Palette selection names in `ramp()` |

## Reference Types

| Type | Target Declared | Example | Used In |
| :--- | :--- | :--- | :--- |
| `substance` | A declared physical substance | • `carbon`<br>• `sugar` | • Menu parameters<br>• Transfers<br>• Reactions |
| `reaction` | A declared chemical reaction | • `rot`<br>• `build` | • Reaction rules<br>• Reaction genes |
| `field` | A spatial grid or signal field | • `cells`<br>• `scent`<br>• `life` | • Rules (`on field`)<br>• Senses<br>• Views |
| `quantity` | Substance in field, or signal field | • `cells.light`<br>• `scent` | • Sense probes<br>• Gradients<br>• View layers |
| `source` / `sink` | Environmental boundary | • `rock`<br>• `sun`<br>• `void` | • Physical transfer origins<br>• Overflows |
| `store` | Physical container | • `self`<br>• `above`<br>• `below`<br>• `cell` | • Transfers<br>• Arbitration targets |
| `neighbour` | Ranked nearby entity | Selected by proximity | • Social actions (`give`, `steal`, `guard`) |

### Substance-Parameterized Amounts
Amounts carry the substance name in their type signature:
```
self.carbon + self.carbon; // Valid: amount[carbon]
self.carbon + self.light;  // Compile Error E0201: mismatched substances!
self.carbon > 0u;          // Valid: 0u is universal zero
self.carbon * 50bp;        // Valid: yields amount[carbon], floored
```

### No Implicit Conversions
There are no automatic type coercions. Conversions are explicit built-in functions:
- `units(amount[s]) -> f32`: Converts amount to dimensionless floating-point units.
- `amount(substance, units: f32) -> amount[s]`: Converts float units into typed substance amount.
- `float(int) -> f32`: Converts integer to float.

### Numeric Contract
- **Amount & Integer Arithmetic Saturates:** Overflow stops at type limits; underflow stops at `0mu`.
- **Division by Zero:** Division by zero returns `0` (or `0.0` for floats).
- **Non-Finite Floats:** NaN and ±Inf are clamped to `0.0`.
"""
    },
    {
        "id": "contexts",
        "title": "Contexts & Execution Scopes",
        "summary": "Execution permissions matrix: what code in each block may read and physical effects it may produce.",
        "content": """
# Contexts & Execution Scopes

To ensure deterministic scheduling and preserve physical conservation, each code block belongs to exactly one execution context. Code attempting an unauthorized read or side-effect is rejected with `E0301`.

## Context Permissions Matrix

| Context | Where it Appears | What it May Read | Permitted Physical Effects |
| :--- | :--- | :--- | :--- |
| **Declaration** | • `world` top level<br>• `lineage` settings<br>• Configuration blocks | • Constants (`const`) | • Declare world topology<br>• Define elements & substances<br>• Configure reactions & upkeep |
| **Transfer Rule** | • `rule ... on field` containing transfers | • Current cell snapshot<br>• Neighbour cells (`above`, `below`, `left`, `right`)<br>• Neighbourhood collections (`moore`, `vonneumann`)<br>• Current `tick` | • `transfer(...)`<br>• `diffuse_conserved(...)` |
| **Reaction Rule** | • `rule ... on field` containing reactions | • Current cell snapshot<br>• Cell substance amounts<br>• Current `tick` | • `react(...)` (up to specified rate) |
| **Signal Rule** | • `rule ... on field` containing signal writes | • Snapshot cell values<br>• Surrounding neighbourhood fields<br>• Other fields at same location | • Assign non-conserved field values: `self.value = ...` |
| **Genesis** | • `genesis { ... }` in world or lineage | • Snapshot cell stores<br>• `uniform()` PRNG draws | • `transfer(...)` from sources to cells/creatures<br>• Set initial `position` and `heading`<br>• `pattern(...)` & `scatter(...)` on state fields |
| **Requirement** | • `requires` clause in action menus | • Entity's own current inventory amounts | • Pure boolean expression check |
| **Genome** | • `seed genome { ... }` in lineages | • Creature's own declared signals | • Declare modules and signal connections<br>• Specify sense, regulate, reaction, and action genes |
| **Pattern** | • `ruleset { ... }` mutation patterns | • Gene AST parameters being rewritten | • Emit replacement gene AST with `jitter()` |
| **VEX View Layer**| • `layer = context(...) { ... }` | • Agent attributes (`@P`, `@heading`, `@age`, `@made`)<br>• Cell attributes (`@ix`, `@iy`, `@value`)<br>• World attributes (`@tick`, `@population`) | • Write visual attributes: `@Cd`, `@Alpha`, `@pscale`, `@shape`, `@N`<br>• (Cannot modify world simulation state) |
| **Explore** | • `explore { ... }` sweeps | • Parameter paths into base world | • Vary parameters across ranges with `linspace` / `logspace` |


### Fundamental Consequences:
1. **No Mixed Rule Kinds:** A world rule holds exactly one kind of physical effect (Transfers, Reactions, or Signals). Mixing kinds is error `E0303`.
2. **Genomes Have No Direct Physical Effects:** Evolved genes only read and write non-conserved signals; the engine runtime translates action genes into physical intents evaluated under conservation laws.
3. **Views Cannot Alter Physics:** Visualization and VEX layer code cannot write simulation state or consume simulation PRNG streams.
"""
    },
    {
        "id": "tick-pipeline",
        "title": "The Tick Pipeline & Clock",
        "summary": "The 4 execution phases of a simulation tick: Set, Move, Transform, Restructure, and shortfall arbitration.",
        "content": """
# The Tick Pipeline & Physical Clock

Every simulation tick advances the world state across four strictly sequential phases:

```
┌─────────────────────────────────────────────────────────────┐
│ 1. SET: Senses read snapshot -> Minds update -> Intents     │
├─────────────────────────────────────────────────────────────┤
│ 2. MOVE: Upkeep -> Fees -> Pro-Rata Transfers -> Movement   │
├─────────────────────────────────────────────────────────────┤
│ 3. TRANSFORM: Entity reactions -> World reactions -> Waste  │
├─────────────────────────────────────────────────────────────┤
│ 4. RESTRUCTURE: Cull dying -> Asexual division -> Offspring │
└─────────────────────────────────────────────────────────────┘
```

## 4-Phase Simulation Clock

| Phase | What Happens | Evaluation Rules | Key Invariants |
| :--- | :--- | :--- | :--- |
| **1. Set** | • Senses read snapshot<br>• Minds update signals<br>• Action intents formed<br>• Signal rules execute | • Reads see start-of-tick snapshot<br>• Signals update straight-line (`mind_steps: 1`) | • No physical substance movements occur yet |
| **2. Move** | • Upkeep paid<br>• Action fees paid<br>• Transfers arbitrated<br>• Physical movement | • Unpaid upkeep marks entity dying<br>• Transfers settled pro-rata concurrently<br>• Unaffordable action intents dropped | • Upkeep comes before reactions<br>• Two-stage pro-rata shortfall sharing<br>• Fees paid for attempts |
| **3. Transform** | • Creature reactions run<br>• World reactions run<br>• Waste substances vented | • Reactions run in genome / declaration order<br>• Clamped by reactant supplies and headroom<br>• Waste vented to declared sinks | • Reactions chain within tick in genome order<br>• ΔG must be strictly downhill |
| **4. Restructure** | • Dying & over-age creatures culled<br>• Asexual division occurs<br>• Offspring placed | • Dead inventories return to cell (overflow to `remains`)<br>• Children receive `split` share of inventory | • Children placed at `child_offset`<br>• Newborns act starting from next tick |

## Detailed Phase Breakdown

### Phase 1: Set
- Senses read the **start-of-tick snapshot**.
- Minds update neural signals according to their wiring (`mind_steps: 1`).
- Action genes formulate physical intents (e.g. move, turn, take, divide); reaction genes set their speed.
- World signal rules execute and update signal fields.

### Phase 2: Move
1. **Upkeep:** Entities pay metabolic upkeep from the `pay` substance (e.g. `energy`). If an entity cannot pay, it is marked **dying**; dying entities do nothing else this tick, participate in no transfers, and are removed at Restructure.
2. **Action Fees:** Fees for chosen actions are paid in genome order. Unaffordable actions are dropped.
3. **Transfers Arbitrated Concurrently:** All transfers (entity `take`, `drop`, `give`, `steal`, and all world transfer rules) are arbitrated together.
   - **Supply Shortfall (Pro-Rata):** If total requests on a store exceed its balance, each request gets an equal fraction floored. Remainder micro-units go one by one to largest fractional parts.
   - **Headroom Capacity:** If incoming matter exceeds destination cap, transfers scale down pro-rata.
4. **Locomotion:** Entities that paid movement fees advance their physical coordinates.

### Phase 3: Transform
- Entity chemical reactions execute in genome module order.
- World reaction rules execute in declaration order.
- Waste substances (e.g. `heat`) are vented to declared sinks (`waste heat -> void;`).

### Phase 4: Restructure
- Dying entities and creatures exceeding `max_age` are removed. Their remaining inventory returns to their cell up to capacity; remaining excess goes to `remains`.
- Reproducing entities execute `divide`: children receive their `split` share and are placed behind the parent at `child_offset`.
- Newborns begin active life from the next tick.
"""
    },
    {
        "id": "world-declarations",
        "title": "World Declarations & Chemistry",
        "summary": "Defining continuous space, chemical elements, thermodynamic dissipation, fields, boundaries, and upkeep.",
        "content": """
# World Declarations & Chemistry

A `world` declaration defines the spatial domain, physical chemistry, environmental fields, boundaries, and biological laws.

```
world Dish {
    // 1. Spatial Geometry
    space: Continuous2D(w: 512.0, h: 512.0, wrap: true, max_interaction_radius: 4.0);

    // 2. Elements & Second Law
    element C;
    element N;
    element E;
    second_law { min_drop: 1; }

    // 3. Conserved Substances
    substance light   = E potential 10;
    substance sugar   = C + E potential 9;
    substance body    = C + N potential 5;
    substance waste   = E potential 1;

    // 4. Fields
    field cells : Grid(w: 256, h: 256) holds [light: 2_000u, sugar: 2_000u, body: 2_000u];
    field scent : Grid(w: 256, h: 256) signal combine max;

    // 5. Boundaries & Waste
    source sun  : [light];
    source rock : [body];
    sink void   : any;
    waste waste -> void;
    remains: void;

    // 6. Chemical Reactions
    reaction photosynthesis : light -> sugar;
    reaction respiration    : sugar -> waste;
    reaction build          : sugar + body -> 2 body;

    // 7. Upkeep & Limits
    upkeep {
        pay: sugar;
        becomes: waste;
        vent: void;
        base: 20mu;
        per_gene: 5mu;
    }

    limits {
        max_entities: 100_000;
        inventory_cap: 100u;
    }
}
```

### Chemistry Rules:
- **Mass Balance:** Reactions must balance elements identically on both sides (`E0401`).
- **Thermodynamic Dissipation:** Input potentials minus output potentials (`ΔG`) must be downhill by at least `min_drop` (`E0402`).
- **Canonical Form:** Terms must appear in substance declaration order without redundant common factors.
"""
    },
    {
        "id": "genomes",
        "title": "Genomes, Lineages & Evolution",
        "summary": "Lineages, seed genomes, modules, signals, gene forms, regulation kinds, and mutation operators.",
        "content": f"""
# Genomes, Lineages & Evolution

In Emerge, autonomous creatures possess no centralized brain or scripted state machines. Instead, a creature's mind is a **computational network formed by genes over internal signals**.

Creatures belong to a founding `lineage` that specifies genome capacity bounds (`profile`), mutation operator probabilities (`mutation`), a `seed genome`, and initial population spawning (`genesis`).

## 1. Anatomy of a Seed Genome

A seed genome defines the ancestral starting state for a lineage. It declares persistent internal signals, groups functional genes into discrete **modules**, and binds sensory inputs to physical effectors:

```
lineage Drifter in Dish {{
    profile standard {{
        genes: 32;
        signals: 16;
        modules: 8;
    }}

    mutation {{
        rate: 0.02;
        param_sigma: 0.1;
        rewire: 0.01;
        duplicate_gene: 0.005;
        delete_gene: 0.005;
    }}

    seed genome {{
        signal hunger keep 0.9;

        module forage {{
            sense gradient(q: cells.body) -> food_x, food_y @food_dir;
            regulate weigh(curve: tanh) <- [10.0 * food_y] -> steer @weigh_steer;
            action move(speed: 1.0) @go;
            action turn(angle_deg: 30.0) <- steer @turn_to_food;
        }}

        module metabolism {{
            reaction photosynthesis rate 1u <- hunger @photo;
            action take(s: body, max: 2u) @eat;
        }}
    }}

    genesis {{
        count: 1_000;
        position = uniform(x: 0.0..512.0, y: 0.0..512.0);
        transfer(sugar, from: rock, to: self, amount: 5u) @seed_fuel;
    }}
}}
```

The node diagram below illustrates the exact computational dataflow constructed by this seed genome:

{generate_network_diagram()}

### How a Genome Executes Each Tick
Each simulation tick proceeds through a deterministic multi-stage pipeline:
- **1. Senses Read Snapshot:** In the `Set` phase, all active `sense` genes sample the immutable start-of-tick snapshot (environmental fields, nearby cells, inventory levels, or chemical gradients) and write floating-point readings into internal signals.
- **2. The Mind Step Evaluates:** Active `regulate` genes compute their non-linear output curves from current signals. Each internal signal updates via its decay factor: `keep · s + inputs`.
- **3. Driving Reactions & Actions:** Active `reaction` genes scale their chemical flux by their driving signals. Active `action` genes whose driving signals exceed their threshold emit physical intents (e.g. `move`, `turn`, `take`) into the global arbitration pool.

## 2. The Four Gene Forms

Every gene belongs to one of four fundamental classes, selected from the world declarations:

{generate_four_gene_kinds_diagram()}

| Gene Kind | What it Reads | What it Writes | World Menu | Primary Function |
| :--- | :--- | :--- | :--- | :--- |
| **`sense`** | • Spatial fields (`Grid`)<br>• Local cell inventory<br>• Creature inventory<br>• Distance &amp; angle probes | • 1 or 2 internal signals (`f32`) | `senses {{ ... }}` | • Converts continuous world geometry and chemistry into informational signals |
| **`regulate`** | • 1 to 4 internal signals<br>• Sensed values<br>• Weighted coefficients | • 1 internal signal (`f32`) | `regulation {{ ... }}` | • Neural / mathematical signal processing (e.g. `weigh(bias, curve)`) |
| **`reaction`** | • 1 driving signal<br>• Conserved inventory | • Chemical transformation of physical inventory | `reaction_space {{ ... }}` | • Modulates metabolic conversion rate (`floor(rate · clamp(s, 0, 1))`) |
| **`action`** | • 1 driving signal (`<- drive`)<br>• Activation threshold | • Physical intent emitted to Set/Move phase | `actions {{ ... }}` | • Drives locomotion (`move`, `turn`), substance exchange (`take`, `drop`, `give`, `steal`), and division (`divide`) |

### Gene Execution Invariants
- **Unconditional Drives Default to 1.0:** Reaction and action genes that omit a driving signal (`<- drive`) evaluate with a constant signal input of `1.0`. They fire at full capacity or run at full rate.
- **Arbitration of Duplicate Actions:** If multiple active genes emit `move`, `turn`, or `divide`, only the **first firing gene in genome order** takes effect; subsequent ones are dropped before fees. In contrast, substance actions (`take`, `drop`, `give`, `steal`) and `emit` all execute independently, each paying its attempt fee.
- **Gene Expression:** Every gene has an `expressed` bitflag. Genes marked `dormant` are retained in the chromosome and inherited across generations, but pay **0 upkeep** and emit no effects.

## 3. Internal Signals & Leaky Memory Dynamics

A creature's internal signals are fundamentally distinct from physical substances:
- **Substances are Conserved Physical Matter:** Substances have elemental composition, follow mass conservation, dissipate free energy downhill, and are subject to physical store caps.
- **Signals are Non-Conserved Information:** Signals are element-free floating-point numbers (`f32`). They can be amplified, decayed, duplicated, or cleared without thermodynamic penalty.

{generate_signal_memory_diagram()}

### The Signal Update Law
During each mind step, every internal signal $S$ is updated according to its declared `keep` factor:

`S[t+1] = keep * S[t] + sum(Sense Readings) + sum(Regulation Outputs)`

| `keep` Value | Behavioral Dynamic | Biological Analogy | Typical Use Cases |
| :--- | :--- | :--- | :--- |
| **`keep 0.0`** | • Pure reflex arc<br>• Signal resets to `0.0` immediately on the next tick | • Direct neural reflex | • Steering toward a momentary gradient<br>• Obstacle collision avoidance |
| **`keep 0.5`** | • Rapid exponential decay<br>• Half-life = 1 simulation tick | • Short-term sensory persistence | • Turn dampening<br>• Transient motion smoothing |
| **`keep 0.9`** | • Leaky integration<br>• Retains 90% of value per tick | • Hormonal &amp; metabolic drive | • Hunger accumulation<br>• Danger / fear arousal states<br>• Cumulative evidence integration |
| **`keep 1.0`** | • Perfect accumulator<br>• Retains value indefinitely without loss | • Counting &amp; state registers | • Developmental stage toggling<br>• Step counters<br>• Finite state transitions |

### Derived Signal Scoping: Private vs Interface
In Emerge, signal scope is **derived from graph topology**, not manually declared:
- **Private Signals:** If a signal is only read and written by genes within a single module, it is scoped as **private** to that module.
- **Interface Signals:** If a signal connects genes across different modules, it is an **interface signal**.
- When evolution duplicates a module, private signals are automatically assigned fresh identifiers, completely preventing accidental cross-talk between the original module and its duplicate!

## 4. Regulation Gene Types & Activation Functions

Worlds declare what kinds of regulation genes creatures can evolve in their `regulation {{ ... }}` block:

```
regulation {{
    gene weigh(
        bias: -16.0..16.0,
        curve: [linear, step, sigmoid, tanh]
    ) inputs 1..4 weight -16.0..16.0;
    gene product() inputs 2..2;
}}
```

### The `weigh` Gene Kind
A `weigh` gene calculates a weighted linear combination of up to 4 input signals, adds an evolvable bias, and passes the result through an activation curve:

`Output = curve(bias + w1*s1 + w2*s2 + ...)`

| Curve Form | Mathematical Definition | Output Range | Functional Behavior |
| :--- | :--- | :--- | :--- |
| **`linear`** | • `f(x) = x` | • `(-inf, +inf)` | • Proportional gain and scalar amplification |
| **`step`** | • `f(x) = x > 0 ? 1 : 0` | • `0` or `1` | • Boolean binary threshold switch |
| **`sigmoid`** | • `f(x) = 1 / (1 + exp(-x))` | • `(0.0, 1.0)` | • Smooth unipolar probabilistic activation |
| **`tanh`** | • `f(x) = tanh(x)` | • `(-1.0, 1.0)` | • Smooth bipolar steering and motor control |

## 5. Evolutionary Specialization & Module Duplication

Novel complex behaviors evolve through the duplication and divergence of functional modules:

{generate_evolution_specialization_diagram()}

### How Evolution Specializes Functional Modules
1. **Whole-Module Duplication:** A mutation event (`duplicate_module`) copies an entire functional module (e.g. `forage`).
2. **Private Signal Re-mapping:** The engine automatically re-maps all private internal signals inside the duplicated module to fresh, unused signal IDs. Interface signals (such as master sensory inputs or global clocks) remain shared.
3. **Neutral Redundancy:** Immediately after duplication, the creature's behavior is intact. The duplicate module serves as a redundant, functional backup.
4. **Adaptive Divergence:** Subsequent point mutations alter the duplicated module's sensor target (e.g. from `cells.light` to `cells.sugar`) or invert regulation weights. The creature acquires specialized multi-cue foraging without ever passing through a non-viable intermediate state.

## 6. Genome Profiles, Capacity & Fixed-Stride GPU Memory

Every lineage declares a `profile` that fixes the maximum capacity of its chromosomes:

{generate_gpu_memory_layout_diagram()}

| Profile Type | Capacity Limits | Execution Strategy | Primary Domain |
| :--- | :--- | :--- | :--- |
| **`profile fast`** | • 32 genes max<br>• 16 signals max<br>• 8 modules max | • Fixed-stride contiguous GPU arrays<br>• SIMD lockstep execution | • Massive ecologies (100,000+ creatures)<br>• High-framerate real-time simulation |
| **`profile free`** | • 256 genes max<br>• 64 signals max<br>• 32 modules max | • Arena-allocated variable-length memory<br>• CPU evaluation (GPU support in Phase 1) | • Open-ended macro-evolution<br>• Deep neural phylogenetic trees |

### Thermodynamic Upkeep Accounting
To prevent runaway genome bloat, the type system enforces living metabolic upkeep costs charged every tick:
- **Base Upkeep (`base`):** Constant survival fee (e.g. `20mu` energy/sugar) paid just to keep the entity alive.
- **Active Gene Fee (`per_gene`):** Each expressed, active gene incurs an ongoing metabolic toll (e.g. `5mu` per tick).
- **Signal in Use Fee (`per_signal`):** Signals with at least one active writer and one active reader cost `1mu` per tick.
- **Carried Mass Fee (`per_carried_u`):** Weight tax proportional to stored physical inventory.
- **Dormant Genes Cost Zero:** Inactive and dormant genes incur **0 upkeep fee**, allowing lineages to maintain a cryptic reservoir of neutral genetic variation that can be reactivated by future environmental shifts!
"""
    },
    {
        "id": "vex-layers",
        "title": "VEX-Style Layer Bodies & Attributes",
        "summary": "Procedural visualization, @ attributes, local variables, and imperative VEX shading logic.",
        "content": """
# VEX-Style Layer Bodies & Visual Language

Emerge incorporates a VEX-inspired visual language for configuring rendering and data visualization layers.
Just as a Houdini Point Wrangle runs over every point in parallel, an Emerge `context(agents)` or `context(cells)` block runs concurrently over every simulated element.

## VEX Context Syntax

```
view Main for Dish {
    // 1. Cell Field Raster Layer
    layer ground = context(cells) {
        float t = fit_log(units(@sugar), 1.0, 5_000.0);
        @Cd = ramp("viridis", t);
    }

    // 2. Creature Particle Layer
    layer creatures = context(agents) {
        // Local typed variables
        float fuel = clamp(units(@energy) / 20.0, 0.0, 1.0);
        
        // Output visual attributes
        @Cd = ramp("magma", fuel);
        @pscale = fit(units(@body), 0.0, 10.0, 0.4, 2.0);
        @shape = "arrow";
        @N = @heading;
        
        if (@age > 10_000) {
            @Alpha = 0.3; // fading elder
        } else {
            @Alpha = 1.0;
        }
    }
}
```

## Reserved Visual Output Attributes

| Attribute | Type | Meaning in Renderer | Valid Values / Ranges |
| :--- | :--- | :--- | :--- |
| `@Cd` | `color` / `vector3` | • Primary RGB surface color | `{r, g, b}` each in range `[0.0, 1.0]` |
| `@Alpha` | `float` | • Transparency opacity factor | `0.0` (transparent) to `1.0` (opaque) |
| `@pscale` | `float` | • Particle glyph size diameter | Positive scalar (typically `0.2` to `3.0`) |
| `@shape` | `string` | • Primitive visual geometry glyph | • `"arrow"` (directional triangle)<br>• `"dot"` (circular disc) |
| `@N` | `vector2` / `float` | • Visual orientation direction normal | Direction vector or radian angle |
"""
    },
    {
        "id": "grammar",
        "title": "Grammar Specification",
        "summary": "Formal EBNF grammar fragments for files, declarations, statements, expressions, and tokens.",
        "content": """
# Emerge Formal Grammar

```ebnf
file        := item*
item        := world | lineage | ruleset | view | chart | explore | patch

world       := 'world' IDENT '{' wdecl* '}'
wdecl       := const | element | substance | field | source | sink | waste 
             | reaction | menu | rule | genesis | block_cfg | setting

const       := 'const' IDENT '=' expr ';'
element     := 'element' IDENT ';'
substance   := 'substance' IDENT '=' comp 'potential' '-'? INT ';'
comp        := cterm ('+' cterm)*
cterm       := INT? IDENT

field       := 'field' IDENT ':' call ( 
                 'holds' '[' sep(IDENT ':' AMOUNT) ']' 
               | 'signal' ( 'combine' IDENT )? 
               | 'states' '[' sep(IDENT) ']' 
               ) ';'

rule        := 'rule' IDENT 'on' IDENT block
genesis     := 'genesis' block
block       := '{' stmt* '}'

stmt        := 'let' IDENT '=' expr ';'
             | 'when' expr block ('else' block)?
             | 'for' IDENT 'in' IDENT block
             | path '=' expr ';'
             | call LABEL? ';'

lineage     := 'lineage' IDENT ('extends' IDENT)? 'in' IDENT '{' ldecl* '}'
seed        := 'seed' 'genome' '{' sigdecl* module+ '}'
module      := 'dormant'? 'module' IDENT '{' gene* '}'
gene        := 'dormant'? gbody LABEL? ('after' LABEL)? ';'

view        := 'view' IDENT 'for' IDENT '{' layer* '}'
layer       := 'layer' IDENT '=' call '{' entry* '}'
```
"""
    },
    {
        "id": "diagnostics",
        "title": "Compiler Diagnostics & Error Codes",
        "summary": "Exhaustive reference of E00xx through E10xx compiler codes with explanations and automated fix strategies.",
        "content": """
# Compiler Diagnostics & Error Reference

`emerge check` reports structured JSON diagnostics with stable error codes, file positions, explanations, and automated patches.

| Code Range | Area | Description |
| :--- | :--- | :--- |
| **`E0001`–`E0007`** | Syntax | • Missing braces or delimiters<br>• Unexpected tokens<br>• Chained comparisons |
| **`E0101`–`E0108`** | Lexical | • Invalid amount precision<br>• Unclosed string literals<br>• Invalid units |
| **`E0110`–`E0119`** | Names & Constants | • Colliding names or duplicate definitions<br>• Reserved keyword usage<br>• Circular constants (`E0119`) |
| **`E0201`–`E0205`** | Types | • Mismatched substances (`E0201`)<br>• Invalid casts or arithmetic<br>• Rate overflows (`E0204`) |
| **`E0301`–`E0305`** | Contexts | • Effect in unauthorized context (`E0301`)<br>• Assignment to amount (`E0302`)<br>• Mixed rule kinds (`E0303`) |
| **`E0401`–`E0406`** | Chemistry | • Unbalanced reaction (`E0401`)<br>• Reaction not downhill (`E0402`)<br>• Missing potential (`E0403`) |
| **`E0501`–`E0502`** | Menus | • Unknown action or sense (`E0501`)<br>• Invalid parameter argument (`E0502`) |
| **`E0601`–`E0605`** | Genomes | • Out-of-bounds weight (`E0601`)<br>• Unwritten signal read by mind (`E0605`) |
| **`E0701`–`E0707`** | Routes & Caps | • Invalid upkeep payment route (`E0701`)<br>• Cap exceeded<br>• Unaccepted sink substance |
| **`E0801`–`E0803`** | Capacity Limits | • Seed genome exceeds profile size (`E0801`)<br>• Population limit exceeded |
| **`E0901`–`E0904`** | Patches | • Stale patch base revision<br>• Patch missing destination `move_to:` |
| **`E1001`–`E1002`** | Runner Support | • Valid language feature not yet supported by current backend runner |
| **`W1001`, `W1201`**| Warnings | • Partial execution omission (`W1001`)<br>• Uncomputed view metric (`W1201`) |
| **`N1101`, `N1102`**| Static Notes | • Dead substances or unexecutable reactions proved unreachable by static closure |
"""
    },
    {
        "id": "vex-comparison",
        "title": "Houdini VEX vs Emerge Comparison",
        "summary": "Rosetta stone comparing Houdini VEX concepts, syntax, attributes, and execution to Emerge (.em).",
        "content": """
# Houdini VEX vs Emerge Language Guide

If you are familiar with SideFX Houdini's Vector Expression language (VEX), you will find Emerge's architecture remarkably familiar.

## Conceptual Mapping

| Houdini VEX Concept | Emerge Equivalent | Architectural Similarity | Key Difference |
| :--- | :--- | :--- | :--- |
| **Point/Primitive Wrangle** | • `context(agents) { ... }`<br>• `context(cells) { ... }` | Evaluates once per element in parallel across SIMD / GPU compute threads. | Emerge enforces physical mass conservation on substance attributes. |
| **Input vs Output Geometry** | • Snapshot (Input)<br>• Tick Phase Effects (Output) | Inputs are read-only from the start-of-tick snapshot; writes are collected and arbitrated. | Intermediate writes in Emerge cannot feed each other within the same tick. |
| **Visual Attributes (`@P`, `@Cd`, `@N`)** | • `@P`, `@Cd`, `@Alpha`<br>• `@pscale`, `@shape`, `@N` | Attribute reads inspect element state; attribute writes directly control rendering passes. | Unqualified `@x` references the current element; outer elements are referenced explicitly (`world.rock`, `cell`). |
| **Contexts (SOP, CVEX, COP, CHOP)** | • Transfer Context<br>• Reaction Context<br>• Signal Context<br>• VEX View Layer | Enclosing block strictly determines permitted operations and physical effects. | Checked at compile time (`E0301`). Code cannot mix physical effect kinds in a single rule (`E0303`). |
| **Channel Parameters (`ch()`, `chf()`)** | • Constants (`const`)<br>• Menu parameters<br>• Sensed inputs | Parameters can be bound, varied, and manipulated live in the node network. | Constants are folded at compile time; runtime parameters are hot-swapped via JSON patches. |
| **VDB / Volume Voxel Operations** | • `field cells : Grid(...)`<br>• `cell.substance`<br>• `above`, `below`, `moore` | 2D spatial grid querying with neighbourhood stencils and bilinear sampling. | Conservation is strictly enforced when moving voxels (`diffuse_conserved`). |
| **`removepoint()` / `addpoint()`** | • `kill()` / Restructure cull<br>• `divide()` / `spawn()` | Creation and removal are isolated to a single deterministic restructure phase. | Dead entities return their remaining substance inventory back to the local cell store. |

## The Critical Distinction: Thermodynamic Conservation
In standard Houdini VEX:
```c
// Houdini VEX: matter is free
@nitrate += 10.0;
```
In Emerge:
```emerge
// Emerge: mass conservation is enforced
@nitrate += take(world.rock, 10u);
```
Direct assignment to a conserved substance is a compile error (`E0302`). Every transfer must explicitly declare the supplying store or source, and all competing requests are settled through fair pro-rata arbitration.
"""
    }
]

ATTRIBUTES = [
    {
        "name": "@P",
        "type": "vector2",
        "domain": "Agent",
        "desc": "Current 2D spatial position vector (x, y) in continuous world space."
    },
    {
        "name": "@heading",
        "type": "vector2 / float",
        "domain": "Agent",
        "desc": "Forward orientation heading vector or radian angle."
    },
    {
        "name": "@id",
        "type": "u64",
        "domain": "Agent",
        "desc": "Unique 64-bit persistent entity identity token."
    },
    {
        "name": "@lineage",
        "type": "ident",
        "domain": "Agent",
        "desc": "Founding ancestral lineage identifier (e.g. Drifter)."
    },
    {
        "name": "@age",
        "type": "int",
        "domain": "Agent",
        "desc": "Age of the creature measured in integer simulation ticks."
    },
    {
        "name": "@expressed_genes",
        "type": "int",
        "domain": "Agent",
        "desc": "Number of currently active, non-dormant expressed genes."
    },
    {
        "name": "@made",
        "type": "amount[s]",
        "domain": "Agent",
        "desc": "Quantity of counted substance synthesized by creature's own reactions in sliding window."
    },
    {
        "name": "@<substance>",
        "type": "amount[s]",
        "domain": "Agent / Cell",
        "desc": "Stored quantity of specified conserved substance (e.g. @energy, @carbon, @body)."
    },
    {
        "name": "@<signal>",
        "type": "float",
        "domain": "Agent",
        "desc": "Internal neural signal value declared by the creature's genome (e.g. @steer, @hunger)."
    },
    {
        "name": "@ix, @iy",
        "type": "int",
        "domain": "Cell",
        "desc": "Integer discrete 2D grid cell column and row coordinates."
    },
    {
        "name": "@value",
        "type": "float / state",
        "domain": "Cell",
        "desc": "Current scalar value on a signal field or discrete state token on a state field."
    },
    {
        "name": "@tick",
        "type": "int",
        "domain": "World",
        "desc": "Current global simulation tick number."
    },
    {
        "name": "@population",
        "type": "int",
        "domain": "World",
        "desc": "Total number of living entities currently alive in the world."
    },
    {
        "name": "@Cd",
        "type": "color",
        "domain": "Visual Output",
        "desc": "Surface RGB display color vector {r, g, b} in [0.0, 1.0]."
    },
    {
        "name": "@Alpha",
        "type": "float",
        "domain": "Visual Output",
        "desc": "Opacity factor in [0.0, 1.0] (1.0 = fully opaque)."
    },
    {
        "name": "@pscale",
        "type": "float",
        "domain": "Visual Output",
        "desc": "Particle glyph size diameter scale factor."
    },
    {
        "name": "@shape",
        "type": "string",
        "domain": "Visual Output",
        "desc": "Primitive visual geometry glyph: \"arrow\" or \"dot\"."
    },
    {
        "name": "@N",
        "type": "vector2 / float",
        "domain": "Visual Output",
        "desc": "Visual orientation normal vector for rotating glyphs."
    }
]

DIAGNOSTICS = [
    {"code": "E0001", "severity": "error", "area": "Syntax", "desc": "Missing or unexpected token in declaration or statement."},
    {"code": "E0002", "severity": "error", "area": "Syntax", "desc": "Unclosed delimiter or bracket pair."},
    {"code": "E0003", "severity": "error", "area": "Syntax", "desc": "Unexpected end of file while parsing block."},
    {"code": "E0004", "severity": "error", "area": "Syntax", "desc": "Chained comparison operators (e.g. `a < b < c`). Use `and` instead."},
    {"code": "E0007", "severity": "error", "area": "Syntax", "desc": "Trailing comment at the end of a block violating formatter rules."},
    {"code": "E0101", "severity": "error", "area": "Lexical", "desc": "Amount is not a whole number of micro-units (`mu`). E.g. `0.0005u` is 0.5mu."},
    {"code": "E0102", "severity": "error", "area": "Lexical", "desc": "Malformed numeric integer or float literal."},
    {"code": "E0103", "severity": "error", "area": "Lexical", "desc": "Malformed rate literal. Rates must be integer basis points (`bp`)."},
    {"code": "E0104", "severity": "error", "area": "Lexical", "desc": "Float literal expected where integer was provided (e.g. `0` instead of `0.0`)."},
    {"code": "E0108", "severity": "error", "area": "Lexical", "desc": "Unclosed string literal or string containing backslash escape sequences."},
    {"code": "E0110", "severity": "error", "area": "Names", "desc": "Declared name collides with a reserved hard keyword or contextual keyword."},
    {"code": "E0111", "severity": "error", "area": "Names", "desc": "Duplicate declared identifier name within the same world namespace."},
    {"code": "E0112", "severity": "error", "area": "Names", "desc": "Signal name in genome collides with an existing declared world name."},
    {"code": "E0113", "severity": "error", "area": "Names", "desc": "Reserved generated identifier pattern (e.g. `s17`, `m3`) used in seed genome."},
    {"code": "E0114", "severity": "error", "area": "Names", "desc": "Unresolved reference: unknown substance, field, reaction, source, sink, or lineage."},
    {"code": "E0115", "severity": "error", "area": "Names", "desc": "Unknown world setting or configuration block name."},
    {"code": "E0116", "severity": "error", "area": "Declarations", "desc": "Required declaration or mandatory parameter argument is missing."},
    {"code": "E0117", "severity": "error", "area": "Lineages", "desc": "Invalid `extends` target: base must be in same world and cannot extend another."},
    {"code": "E0118", "severity": "error", "area": "Lineages", "desc": "Extension collision: duplicate signal name, repeated label, or `after` outside `extends`."},
    {"code": "E0119", "severity": "error", "area": "Constants", "desc": "Circular dependency loop detected between constants (`const A = B; const B = A;`)."},
    {"code": "E0201", "severity": "error", "area": "Types", "desc": "Substance mismatch in amount arithmetic (e.g. `self.carbon + self.light`)."},
    {"code": "E0202", "severity": "error", "area": "Types", "desc": "Wrong substance amount passed where another substance was expected."},
    {"code": "E0203", "severity": "error", "area": "Values", "desc": "Value outside valid range (e.g. `second_law.min_drop` < 1 or negative amount)."},
    {"code": "E0204", "severity": "error", "area": "Values", "desc": "Rate exceeds maximum 10_000bp (100%) or amount exceeds exact f32 precision."},
    {"code": "E0205", "severity": "error", "area": "Types", "desc": "Expression evaluates to an invalid type or unexpected kind."},
    {"code": "E0301", "severity": "error", "area": "Contexts", "desc": "Physical effect, mutation, or read operation not permitted in this context."},
    {"code": "E0302", "severity": "error", "area": "Contexts", "desc": "Assignment to a conserved substance amount (`self.carbon = 5u`). Matter is conserved."},
    {"code": "E0303", "severity": "error", "area": "Contexts", "desc": "Mixed physical effect kinds in a rule. Rules must be purely transfers, reactions, or signals."},
    {"code": "E0304", "severity": "error", "area": "Contexts", "desc": "Rule contains no physical effect statements."},
    {"code": "E0305", "severity": "error", "area": "Contexts", "desc": "Signal field written by multiple rules without a declared `combine` policy."},
    {"code": "E0401", "severity": "error", "area": "Chemistry", "desc": "Chemical reaction is not stoichiometrically balanced across elemental compositions."},
    {"code": "E0402", "severity": "error", "area": "Chemistry", "desc": "Reaction is not strictly downhill: ΔG must decrease by at least `min_drop`."},
    {"code": "E0403", "severity": "error", "area": "Chemistry", "desc": "Substance declared without specifying an elemental formula or thermodynamic potential."},
    {"code": "E0404", "severity": "error", "area": "Chemistry", "desc": "Reaction formula not in canonical declaration order. Run `emerge fmt` to fix."},
    {"code": "E0406", "severity": "error", "area": "Chemistry", "desc": "Generated reaction space exceeds maximum capacity limit."},
    {"code": "E0501", "severity": "error", "area": "Menus", "desc": "Unknown sense, action, or regulation kind not declared in world menus."},
    {"code": "E0502", "severity": "error", "area": "Menus", "desc": "Argument name is not a valid parameter of the target menu item."},
    {"code": "E0601", "severity": "error", "area": "Genomes", "desc": "Gene parameter, weight, or input count is outside the menu's permitted range."},
    {"code": "E0602", "severity": "error", "area": "Genomes", "desc": "Reaction gene references a reaction not present in the reaction space `include` list."},
    {"code": "E0605", "severity": "error", "area": "Genomes", "desc": "Signal is read by a gene but never written by any gene in the seed genome."},
    {"code": "E0701", "severity": "error", "area": "Routes", "desc": "Invalid upkeep route: `pay` and `becomes` composition mismatch, or sink rejects."},
    {"code": "E0702", "severity": "error", "area": "Routes", "desc": "Upkeep `base` fee must be strictly greater than 0mu."},
    {"code": "E0703", "severity": "error", "area": "Routes", "desc": "Source boundary emits a substance it did not declare in its emission list."},
    {"code": "E0704", "severity": "error", "area": "Routes", "desc": "Sink boundary rejects the substance directed to it."},
    {"code": "E0705", "severity": "error", "area": "Routes", "desc": "`remains` sink does not accept all world substances."},
    {"code": "E0706", "severity": "error", "area": "Routes", "desc": "Conserved field `holds` list does not declare capacity for this substance."},
    {"code": "E0707", "severity": "error", "area": "Routes", "desc": "Genesis initial endowment exceeds cell storage capacity cap."},
    {"code": "E0801", "severity": "error", "area": "Limits", "desc": "Seed genome size exceeds the limits configured in the lineage profile."},
    {"code": "E0802", "severity": "error", "area": "Limits", "desc": "Active population count exceeds the world `limits.max_entities` ceiling."},
    {"code": "E0803", "severity": "error", "area": "Limits", "desc": "Genesis entity placement coordinates lie outside space bounds."},
    {"code": "E0901", "severity": "error", "area": "Patches", "desc": "Patch removes a substance or lowers a cap without specifying `move_to:` sink."},
    {"code": "E0902", "severity": "error", "area": "Patches", "desc": "Redundant full path prefix inside relative `patch World { ... }` block."},
    {"code": "E0903", "severity": "error", "area": "Patches", "desc": "File provided to `--patch` contains non-patch declarations."},
    {"code": "E0904", "severity": "error", "area": "Patches", "desc": "Patch operation not currently supported on static file patches."},
    {"code": "E1001", "severity": "error", "area": "Runner", "desc": "Valid language construct not yet supported by current runner execution engine."},
    {"code": "E1002", "severity": "error", "area": "Runner", "desc": "Simulation cannot execute on this runner backend without `--partial` flag."},
    {"code": "W1001", "severity": "warning", "area": "Runner", "desc": "Partial execution: unsupported language constructs omitted during run."},
    {"code": "W1201", "severity": "warning", "area": "Views", "desc": "View or chart references valid attribute not yet computed by simulation."},
    {"code": "N1101", "severity": "note", "area": "Analysis", "desc": "Static proof: substance can never appear (no store starts with it and no inflow adds it)."},
    {"code": "N1102", "severity": "note", "area": "Analysis", "desc": "Static proof: reaction can never execute because required reactants can never appear."}
]

