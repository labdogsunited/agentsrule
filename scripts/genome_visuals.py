"""
SVG Visual Diagrams for Emerge Genomes Documentation.
Generates responsive, Houdini-themed vector graphics explaining:
1. The Computational Genome Network (Node Graph Visual)
2. The Four Gene Forms Architecture & Data Loop
3. Internal Signal Memory Dynamics (keep factor decay curves)
4. Module Duplication, Rewiring & Evolutionary Divergence
5. Genome Profile & Fixed-Stride GPU Memory Layout
"""

import xml.etree.ElementTree as ET

def generate_network_diagram():
    return """<figure class="diagram-figure">
  <div class="diagram-header">
    <span>Figure 1: The Computational Genome Network (Node Graph View)</span>
    <span class="diagram-tag">Drifter Seed Genome</span>
  </div>
  <div class="diagram-svg-wrap">
    <svg viewBox="0 0 960 480" width="100%" style="max-width: 960px; height: auto;" xmlns="http://www.w3.org/2000/svg" font-family="ui-sans-serif, system-ui, -apple-system, sans-serif">
      <defs>
        <!-- Background Grid -->
        <pattern id="grid-dots" width="20" height="20" patternUnits="userSpaceOnUse">
          <circle cx="2" cy="2" r="1" fill="#242933" />
        </pattern>
        <!-- Glow filters for wires -->
        <filter id="glow-blue" x="-20%" y="-20%" width="140%" height="140%">
          <feGaussianBlur stdDeviation="2" result="blur" />
          <feMerge><feMergeNode in="blur" /><feMergeNode in="SourceGraphic" /></feMerge>
        </filter>
        <filter id="glow-purple" x="-20%" y="-20%" width="140%" height="140%">
          <feGaussianBlur stdDeviation="2" result="blur" />
          <feMerge><feMergeNode in="blur" /><feMergeNode in="SourceGraphic" /></feMerge>
        </filter>
        <filter id="glow-orange" x="-20%" y="-20%" width="140%" height="140%">
          <feGaussianBlur stdDeviation="2" result="blur" />
          <feMerge><feMergeNode in="blur" /><feMergeNode in="SourceGraphic" /></feMerge>
        </filter>
      </defs>

      <!-- Background -->
      <rect width="960" height="480" fill="#13161c" />
      <rect width="960" height="480" fill="url(#grid-dots)" />

      <!-- Column Phase Headers -->
      <g opacity="0.85">
        <rect x="35" y="14" width="260" height="22" rx="3" fill="#1c2230" stroke="#2b364a" />
        <text x="165" y="29" fill="#7fa6d6" font-size="11" font-weight="700" text-anchor="middle" letter-spacing="0.05em">PHASE 1: SENSES (READ SNAPSHOT)</text>

        <rect x="350" y="14" width="260" height="22" rx="3" fill="#261d33" stroke="#3d2d54" />
        <text x="480" y="29" fill="#c39ee3" font-size="11" font-weight="700" text-anchor="middle" letter-spacing="0.05em">PHASE 2: MIND (REGULATION STEP)</text>

        <rect x="665" y="14" width="260" height="22" rx="3" fill="#182c28" stroke="#244a42" />
        <text x="795" y="29" fill="#62c7b5" font-size="11" font-weight="700" text-anchor="middle" letter-spacing="0.05em">PHASE 3: PHYSICAL ACT &amp; REACT</text>
      </g>

      <!-- MODULE 0: FORAGE -->
      <g>
        <rect x="25" y="48" width="910" height="236" rx="8" fill="#181c24" stroke="#2d3442" stroke-dasharray="4 4" />
        <rect x="35" y="56" width="130" height="20" rx="4" fill="#2d3545" />
        <text x="45" y="70" fill="#f38230" font-size="11" font-weight="800" letter-spacing="0.04em">MODULE forage</text>
        <text x="175" y="70" fill="#8b95a5" font-size="11">Active (1 expressed module, 4 genes)</text>

        <!-- SENSE Gene: gradient -->
        <g id="node-gradient">
          <rect x="45" y="90" width="240" height="82" rx="6" fill="#1c2028" stroke="#3d495c" stroke-width="1.5" />
          <path d="M 45 96 A 6 6 0 0 1 51 90 L 279 90 A 6 6 0 0 1 285 96 L 285 114 L 45 114 Z" fill="#3a669b" />
          <text x="55" y="107" fill="#ffffff" font-size="12" font-weight="700">sense gradient</text>
          <text x="275" y="107" fill="#cbe1fb" font-size="10" text-anchor="end">@food_dir</text>
          <text x="55" y="130" fill="#9db0c6" font-size="11" font-family="ui-monospace, monospace">q: cells.body</text>
          <text x="55" y="146" fill="#6d7a8c" font-size="10">Spatial chemical probe</text>
          <text x="55" y="162" fill="#586576" font-size="9">Upkeep: 5mu/tick</text>
          <!-- Out Ports -->
          <circle cx="285" cy="128" r="5" fill="#4a88c7" stroke="#1c2028" stroke-width="1.5" />
          <text x="274" y="131" fill="#7fa6d6" font-size="10" text-anchor="end" font-family="ui-monospace, monospace">food_x</text>
          <circle cx="285" cy="152" r="5" fill="#4a88c7" stroke="#1c2028" stroke-width="1.5" />
          <text x="274" y="155" fill="#7fa6d6" font-size="10" text-anchor="end" font-family="ui-monospace, monospace">food_y</text>
        </g>

        <!-- REGULATE Gene: weigh -->
        <g id="node-weigh">
          <rect x="360" y="90" width="240" height="82" rx="6" fill="#201b28" stroke="#4d3b63" stroke-width="1.5" />
          <path d="M 360 96 A 6 6 0 0 1 366 90 L 594 90 A 6 6 0 0 1 600 90 L 600 114 L 360 114 Z" fill="#7c4ca8" />
          <text x="370" y="107" fill="#ffffff" font-size="12" font-weight="700">regulate weigh</text>
          <text x="590" y="107" fill="#ecdcfb" font-size="10" text-anchor="end">@weigh_steer</text>
          <!-- In Port -->
          <circle cx="360" cy="138" r="5" fill="#9d6ec9" stroke="#201b28" stroke-width="1.5" />
          <text x="372" y="141" fill="#c39ee3" font-size="10" font-family="ui-monospace, monospace">[10.0 * food_y]</text>
          <text x="372" y="157" fill="#937ca8" font-size="10">curve: tanh, bias: 0.0</text>
          <!-- Out Port -->
          <circle cx="600" cy="138" r="5" fill="#9d6ec9" stroke="#201b28" stroke-width="1.5" />
          <text x="588" y="141" fill="#c39ee3" font-size="10" text-anchor="end" font-family="ui-monospace, monospace">steer</text>
        </g>

        <!-- ACTION Gene: turn -->
        <g id="node-turn">
          <rect x="680" y="85" width="235" height="72" rx="6" fill="#182422" stroke="#2c524a" stroke-width="1.5" />
          <path d="M 680 91 A 6 6 0 0 1 686 85 L 909 85 A 6 6 0 0 1 915 85 L 915 109 L 680 109 Z" fill="#2f8a7a" />
          <text x="690" y="102" fill="#ffffff" font-size="12" font-weight="700">action turn</text>
          <text x="905" y="102" fill="#d2f5ee" font-size="10" text-anchor="end">@turn_to_food</text>
          <!-- In Port -->
          <circle cx="680" cy="128" r="5" fill="#3eb49f" stroke="#182422" stroke-width="1.5" />
          <text x="692" y="132" fill="#62c7b5" font-size="10" font-family="ui-monospace, monospace">&lt;- steer</text>
          <text x="692" y="147" fill="#759b93" font-size="10">angle_deg: 30.0 (steers body)</text>
        </g>

        <!-- ACTION Gene: move -->
        <g id="node-move">
          <rect x="680" y="172" width="235" height="72" rx="6" fill="#182422" stroke="#2c524a" stroke-width="1.5" />
          <path d="M 680 178 A 6 6 0 0 1 686 172 L 909 172 A 6 6 0 0 1 915 172 L 915 196 L 680 196 Z" fill="#2f8a7a" />
          <text x="690" y="189" fill="#ffffff" font-size="12" font-weight="700">action move</text>
          <text x="905" y="189" fill="#d2f5ee" font-size="10" text-anchor="end">@go</text>
          <text x="690" y="215" fill="#62c7b5" font-size="10" font-family="ui-monospace, monospace">speed: 1.0 (unconditional 1.0)</text>
          <text x="690" y="230" fill="#759b93" font-size="10">Translates along heading @P</text>
        </g>

        <!-- Wires in Forage -->
        <!-- food_y wire -->
        <path d="M 285 152 C 320 152, 325 138, 360 138" fill="none" stroke="#4a88c7" stroke-width="2.5" filter="url(#glow-blue)" />
        <rect x="306" y="137" width="34" height="14" rx="2" fill="#161e2b" stroke="#36587e" />
        <text x="323" y="148" fill="#8cb7e8" font-size="9" text-anchor="middle" font-family="ui-monospace, monospace">food_y</text>

        <!-- steer wire -->
        <path d="M 600 138 C 640 138, 640 128, 680 128" fill="none" stroke="#9d6ec9" stroke-width="2.5" filter="url(#glow-purple)" />
        <rect x="626" y="125" width="28" height="14" rx="2" fill="#241b2e" stroke="#563b72" />
        <text x="640" y="136" fill="#cda8f3" font-size="9" text-anchor="middle" font-family="ui-monospace, monospace">steer</text>
      </g>

      <!-- MODULE 1: METABOLISM -->
      <g>
        <rect x="25" y="296" width="910" height="168" rx="8" fill="#181c24" stroke="#2d3442" stroke-dasharray="4 4" />
        <rect x="35" y="304" width="160" height="20" rx="4" fill="#2d3545" />
        <text x="45" y="318" fill="#f38230" font-size="11" font-weight="800" letter-spacing="0.04em">MODULE metabolism</text>
        <text x="205" y="318" fill="#8b95a5" font-size="11">Active (1 signal, 1 reaction, 1 action)</text>

        <!-- SIGNAL hunger Declaration Box with Leaky Integrator Loop -->
        <g id="node-signal-hunger">
          <rect x="45" y="336" width="240" height="106" rx="6" fill="#241e17" stroke="#664626" stroke-width="1.5" />
          <path d="M 45 342 A 6 6 0 0 1 51 336 L 279 336 A 6 6 0 0 1 285 336 L 285 360 L 45 360 Z" fill="#9e5618" />
          <text x="55" y="353" fill="#ffffff" font-size="12" font-weight="700">signal hunger</text>
          <text x="275" y="353" fill="#fed6b0" font-size="10" text-anchor="end">keep 0.9</text>
          <text x="55" y="377" fill="#e0a458" font-size="11" font-weight="700">Leaky Memory Integrator</text>
          <text x="55" y="393" fill="#9f7b53" font-size="10">Decays 10% per tick: S = 0.9 · S</text>
          <text x="55" y="408" fill="#9f7b53" font-size="10">Upkeep: 1mu/tick (signal in use)</text>
          <!-- Memory Loop visual indicator -->
          <path d="M 70 422 C 40 422, 40 434, 70 434 L 140 434 C 170 434, 170 422, 140 422" fill="none" stroke="#e0a458" stroke-width="1.5" stroke-dasharray="3 2" />
          <polygon points="75,419 75,425 67,422" fill="#e0a458" />
          <text x="105" y="430" fill="#e0a458" font-size="8" text-anchor="middle">S(t+1) = 0.9 S(t)</text>
          <!-- Out Port -->
          <circle cx="285" cy="385" r="5" fill="#f38230" stroke="#241e17" stroke-width="1.5" />
          <text x="274" y="388" fill="#f38230" font-size="10" text-anchor="end" font-family="ui-monospace, monospace">hunger</text>
        </g>

        <!-- REACTION Gene: photosynthesis -->
        <g id="node-photosynthesis">
          <rect x="360" y="336" width="240" height="92" rx="6" fill="#251d18" stroke="#5a3c26" stroke-width="1.5" />
          <path d="M 360 342 A 6 6 0 0 1 366 336 L 594 336 A 6 6 0 0 1 600 336 L 600 360 L 360 360 Z" fill="#a0571d" />
          <text x="370" y="353" fill="#ffffff" font-size="12" font-weight="700">reaction photosynthesis</text>
          <text x="590" y="353" fill="#fed6b0" font-size="10" text-anchor="end">@photo</text>
          <!-- In Port -->
          <circle cx="360" cy="385" r="5" fill="#e0a050" stroke="#251d18" stroke-width="1.5" />
          <text x="372" y="388" fill="#e0a050" font-size="10" font-family="ui-monospace, monospace">&lt;- hunger</text>
          <text x="372" y="405" fill="#c49b71" font-size="10" font-family="ui-monospace, monospace">light + carbon -&gt; sugar</text>
          <text x="372" y="419" fill="#8e7357" font-size="9">Rate: floor(1u · clamp(hunger, 0, 1))</text>
        </g>

        <!-- ACTION Gene: take -->
        <g id="node-take">
          <rect x="680" y="336" width="235" height="82" rx="6" fill="#182422" stroke="#2c524a" stroke-width="1.5" />
          <path d="M 680 342 A 6 6 0 0 1 686 336 L 909 336 A 6 6 0 0 1 915 336 L 915 360 L 680 360 Z" fill="#2f8a7a" />
          <text x="690" y="353" fill="#ffffff" font-size="12" font-weight="700">action take</text>
          <text x="905" y="353" fill="#d2f5ee" font-size="10" text-anchor="end">@eat</text>
          <text x="690" y="378" fill="#62c7b5" font-size="10" font-family="ui-monospace, monospace">s: body, max: 2u</text>
          <text x="690" y="394" fill="#759b93" font-size="10">Harvests food from local cell store</text>
          <text x="690" y="408" fill="#58766f" font-size="9">Attempt fee: 5mu sugar upkeep</text>
        </g>

        <!-- Wire in Metabolism: hunger -->
        <path d="M 285 385 L 360 385" fill="none" stroke="#f38230" stroke-width="2.5" filter="url(#glow-orange)" />
        <rect x="306" y="377" width="36" height="14" rx="2" fill="#241b14" stroke="#683d1c" />
        <text x="324" y="388" fill="#fbb67e" font-size="9" text-anchor="middle" font-family="ui-monospace, monospace">hunger</text>
      </g>
    </svg>
  </div>
  <figcaption class="diagram-caption">
    <strong>The Drifter Genome Signal Network:</strong> Genes are grouped inside modular frames. Sensory readings write internal floating-point signals (<span style="color:#7fa6d6;">blue</span>), which are transformed by neural regulation curves (<span style="color:#c39ee3;">purple</span>) and leaky memory signals (<span style="color:#f38230;">orange</span>), ultimately firing physical motor actions and biochemical metabolic reactions (<span style="color:#62c7b5;">teal</span>).
  </figcaption>
</figure>"""

def generate_four_gene_kinds_diagram():
    return """<figure class="diagram-figure">
  <div class="diagram-header">
    <span>Figure 2: The Four Gene Forms &amp; Data Pipeline</span>
    <span class="diagram-tag">Computational Contract</span>
  </div>
  <div class="diagram-svg-wrap">
    <svg viewBox="0 0 940 370" width="100%" style="max-width: 940px; height: auto;" xmlns="http://www.w3.org/2000/svg" font-family="ui-sans-serif, system-ui, -apple-system, sans-serif">
      <rect width="940" height="370" fill="#13161c" />

      <!-- 1. SENSE CARD -->
      <g transform="translate(20, 20)">
        <rect width="210" height="330" rx="6" fill="#181e28" stroke="#31435e" stroke-width="1.5" />
        <path d="M 0 6 A 6 6 0 0 1 6 0 L 204 0 A 6 6 0 0 1 210 0 L 210 38 L 0 38 Z" fill="#315c8e" />
        <text x="15" y="24" fill="#ffffff" font-size="14" font-weight="800">1. SENSE</text>
        <text x="195" y="24" fill="#bed8fa" font-size="10" text-anchor="end">Input Transducer</text>
        
        <!-- Icon / Graphic -->
        <circle cx="105" cy="75" r="24" fill="#1e2c40" stroke="#4a88c7" stroke-width="2" />
        <!-- Antenna radar arcs -->
        <path d="M 93 68 A 16 16 0 0 1 117 68" fill="none" stroke="#7fa6d6" stroke-width="2" />
        <path d="M 87 62 A 25 25 0 0 1 123 62" fill="none" stroke="#7fa6d6" stroke-width="1.5" />
        <circle cx="105" cy="77" r="3" fill="#f38230" />

        <text x="15" y="125" fill="#7fa6d6" font-size="11" font-weight="700">READS:</text>
        <text x="15" y="142" fill="#c3d2e6" font-size="11">• Environmental fields</text>
        <text x="15" y="158" fill="#c3d2e6" font-size="11">• Cell substance stores</text>
        <text x="15" y="174" fill="#c3d2e6" font-size="11">• Entity inventory / clocks</text>

        <line x1="15" y1="188" x2="195" y2="188" stroke="#253245" />

        <text x="15" y="206" fill="#7fa6d6" font-size="11" font-weight="700">WRITES:</text>
        <text x="15" y="224" fill="#c3d2e6" font-size="11">• 1 or 2 internal signals</text>
        <text x="15" y="240" fill="#9db0c6" font-size="10">(f32 values added into signal)</text>

        <rect x="15" y="260" width="180" height="52" rx="4" fill="#121720" stroke="#253245" />
        <text x="22" y="278" fill="#bed8fa" font-size="10" font-family="ui-monospace, monospace">sense gradient(q) -&gt;</text>
        <text x="22" y="296" fill="#bed8fa" font-size="10" font-family="ui-monospace, monospace">  sig1, sig2 @label;</text>
      </g>

      <!-- 2. REGULATION CARD -->
      <g transform="translate(250, 20)">
        <rect width="210" height="330" rx="6" fill="#1e1828" stroke="#483361" stroke-width="1.5" />
        <path d="M 0 6 A 6 6 0 0 1 6 0 L 204 0 A 6 6 0 0 1 210 0 L 210 38 L 0 38 Z" fill="#6d3f96" />
        <text x="15" y="24" fill="#ffffff" font-size="14" font-weight="800">2. REGULATE</text>
        <text x="195" y="24" fill="#eccdfe" font-size="10" text-anchor="end">Neural / Math Mind</text>

        <!-- Neural Node Icon -->
        <circle cx="75" cy="65" r="8" fill="#3a254e" stroke="#9d6ec9" stroke-width="1.5" />
        <circle cx="75" cy="85" r="8" fill="#3a254e" stroke="#9d6ec9" stroke-width="1.5" />
        <circle cx="135" cy="75" r="10" fill="#6d3f96" stroke="#c39ee3" stroke-width="2" />
        <line x1="83" y1="65" x2="125" y2="75" stroke="#9d6ec9" stroke-width="1.5" />
        <line x1="83" y1="85" x2="125" y2="75" stroke="#9d6ec9" stroke-width="1.5" />

        <text x="15" y="125" fill="#c39ee3" font-size="11" font-weight="700">READS:</text>
        <text x="15" y="142" fill="#e0cee8" font-size="11">• 1 to 4 internal signals</text>
        <text x="15" y="158" fill="#e0cee8" font-size="11">• Sensed input values</text>
        <text x="15" y="174" fill="#e0cee8" font-size="11">• Weighted coefficients</text>

        <line x1="15" y1="188" x2="195" y2="188" stroke="#362547" />

        <text x="15" y="206" fill="#c39ee3" font-size="11" font-weight="700">COMPUTES &amp; WRITES:</text>
        <text x="15" y="224" fill="#e0cee8" font-size="11">• curve(bias + Σ w·s)</text>
        <text x="15" y="240" fill="#aa93be" font-size="10">(tanh, sigmoid, step, linear)</text>

        <rect x="15" y="260" width="180" height="52" rx="4" fill="#15111c" stroke="#362547" />
        <text x="22" y="278" fill="#eccdfe" font-size="10" font-family="ui-monospace, monospace">regulate weigh(...) &lt;-</text>
        <text x="22" y="296" fill="#eccdfe" font-size="10" font-family="ui-monospace, monospace">  [w * sig] -&gt; out;</text>
      </g>

      <!-- 3. REACTION CARD -->
      <g transform="translate(480, 20)">
        <rect width="210" height="330" rx="6" fill="#221b16" stroke="#5c3c21" stroke-width="1.5" />
        <path d="M 0 6 A 6 6 0 0 1 6 0 L 204 0 A 6 6 0 0 1 210 0 L 210 38 L 0 38 Z" fill="#914d17" />
        <text x="15" y="24" fill="#ffffff" font-size="14" font-weight="800">3. REACTION</text>
        <text x="195" y="24" fill="#fcd7b2" font-size="10" text-anchor="end">Metabolic Flux</text>

        <!-- Chemical Retort Icon -->
        <path d="M 98 60 L 112 60 L 112 70 L 122 88 A 6 6 0 0 1 117 96 L 93 96 A 6 6 0 0 1 88 88 L 98 70 Z" fill="#3a2517" stroke="#e0a050" stroke-width="1.5" />
        <circle cx="102" cy="88" r="2" fill="#f38230" />
        <circle cx="108" cy="85" r="1.5" fill="#f38230" />

        <text x="15" y="125" fill="#e0a050" font-size="11" font-weight="700">READS &amp; GATES:</text>
        <text x="15" y="142" fill="#eddcd0" font-size="11">• Driving signal (s)</text>
        <text x="15" y="158" fill="#eddcd0" font-size="11">• Conserved substances</text>
        <text x="15" y="174" fill="#eddcd0" font-size="11">• Enzyme rate cap</text>

        <line x1="15" y1="188" x2="195" y2="188" stroke="#3d281a" />

        <text x="15" y="206" fill="#e0a050" font-size="11" font-weight="700">CONVERTS (CONSERVED):</text>
        <text x="15" y="224" fill="#eddcd0" font-size="11">• In-place chemical shift</text>
        <text x="15" y="240" fill="#a48166" font-size="10">(e.g. sugar -&gt; energy + waste)</text>

        <rect x="15" y="260" width="180" height="52" rx="4" fill="#17120e" stroke="#3d281a" />
        <text x="22" y="278" fill="#fcd7b2" font-size="10" font-family="ui-monospace, monospace">reaction photo rate 1u</text>
        <text x="22" y="296" fill="#fcd7b2" font-size="10" font-family="ui-monospace, monospace">  &lt;- hunger @photo;</text>
      </g>

      <!-- 4. ACTION CARD -->
      <g transform="translate(710, 20)">
        <rect width="210" height="330" rx="6" fill="#162220" stroke="#255249" stroke-width="1.5" />
        <path d="M 0 6 A 6 6 0 0 1 6 0 L 204 0 A 6 6 0 0 1 210 0 L 210 38 L 0 38 Z" fill="#247568" />
        <text x="15" y="24" fill="#ffffff" font-size="14" font-weight="800">4. ACTION</text>
        <text x="195" y="24" fill="#bdf4eb" font-size="10" text-anchor="end">Physical Effector</text>

        <!-- Motor Thrust Icon -->
        <polygon points="105,58 120,86 105,80 90,86" fill="#1d3833" stroke="#3eb49f" stroke-width="2" />
        <line x1="105" y1="80" x2="105" y2="94" stroke="#f38230" stroke-width="2" stroke-dasharray="2 2" />

        <text x="15" y="125" fill="#3eb49f" font-size="11" font-weight="700">GATED BY:</text>
        <text x="15" y="142" fill="#cbe7e2" font-size="11">• Drive signal &gt; threshold</text>
        <text x="15" y="158" fill="#cbe7e2" font-size="11">• Attempt fees (paid in Move)</text>
        <text x="15" y="174" fill="#cbe7e2" font-size="11">• Physical bounds / targets</text>

        <line x1="15" y1="188" x2="195" y2="188" stroke="#1d3a34" />

        <text x="15" y="206" fill="#3eb49f" font-size="11" font-weight="700">PHYSICAL EFFECTS:</text>
        <text x="15" y="224" fill="#cbe7e2" font-size="11">• move, turn, take, drop</text>
        <text x="15" y="240" fill="#7ba39b" font-size="10">• give, steal, emit, divide</text>

        <rect x="15" y="260" width="180" height="52" rx="4" fill="#101817" stroke="#1d3a34" />
        <text x="22" y="278" fill="#bdf4eb" font-size="10" font-family="ui-monospace, monospace">action turn(30.0)</text>
        <text x="22" y="296" fill="#bdf4eb" font-size="10" font-family="ui-monospace, monospace">  &lt;- steer @turn;</text>
      </g>
    </svg>
  </div>
  <figcaption class="diagram-caption">
    <strong>The Four Gene Forms:</strong> Every gene maps into one of four functional roles. Senses read continuous space; Regulation performs neural tensor transformations; Reactions drive enzyme-controlled chemistry; and Actions arbitrate physical locomotion and ecological contact.
  </figcaption>
</figure>"""

def generate_signal_memory_diagram():
    return """<figure class="diagram-figure">
  <div class="diagram-header">
    <span>Figure 3: Internal Signal Memory Dynamics (keep Factor Decay Curves)</span>
    <span class="diagram-tag">Mathematical Contract</span>
  </div>
  <div class="diagram-svg-wrap">
    <svg viewBox="0 0 940 380" width="100%" style="max-width: 940px; height: auto;" xmlns="http://www.w3.org/2000/svg" font-family="ui-sans-serif, system-ui, -apple-system, sans-serif">
      <defs>
        <linearGradient id="eq-grad" x1="0" y1="0" x2="1" y2="0">
          <stop offset="0%" stop-color="#241e30" />
          <stop offset="100%" stop-color="#1c242c" />
        </linearGradient>
      </defs>
      <rect width="940" height="380" fill="#13161c" />

      <!-- Left Side: Mathematical Formula Box -->
      <g transform="translate(24, 24)">
        <rect width="320" height="332" rx="6" fill="url(#eq-grad)" stroke="#384357" stroke-width="1.5" />
        <text x="20" y="32" fill="#f38230" font-size="13" font-weight="800" letter-spacing="0.04em">THE SIGNAL UPDATE LAW</text>
        
        <!-- Formula Box -->
        <rect x="20" y="48" width="280" height="54" rx="4" fill="#0d1117" stroke="#252d3a" />
        <text x="160" y="80" fill="#ffffff" font-size="15" font-family="ui-monospace, monospace" font-weight="700" text-anchor="middle">
          S<tspan font-size="10" dy="4">t+1</tspan><tspan font-size="15" dy="-4"> = keep · S</tspan><tspan font-size="10" dy="4">t</tspan><tspan font-size="15" dy="-4"> + Σ inputs</tspan><tspan font-size="10" dy="4">t</tspan>
        </text>

        <text x="20" y="130" fill="#b0bccd" font-size="11" font-weight="700">Memory Regimes:</text>

        <!-- Legend Items -->
        <circle cx="28" cy="154" r="5" fill="#f87171" />
        <text x="42" y="158" fill="#ffffff" font-size="11" font-weight="700">keep = 0.0</text>
        <text x="114" y="158" fill="#8d99ab" font-size="11">Pure Reflex (Immediate clear)</text>

        <circle cx="28" cy="186" r="5" fill="#fbbf24" />
        <text x="42" y="190" fill="#ffffff" font-size="11" font-weight="700">keep = 0.5</text>
        <text x="114" y="190" fill="#8d99ab" font-size="11">Short Memory (Half-life = 1 tick)</text>

        <circle cx="28" cy="218" r="5" fill="#34d399" />
        <text x="42" y="222" fill="#ffffff" font-size="11" font-weight="700">keep = 0.9</text>
        <text x="114" y="222" fill="#8d99ab" font-size="11">Leaky Integrator (Hunger/Drive)</text>

        <circle cx="28" cy="250" r="5" fill="#a78bfa" />
        <text x="42" y="254" fill="#ffffff" font-size="11" font-weight="700">keep = 1.0</text>
        <text x="114" y="254" fill="#8d99ab" font-size="11">Perfect Counter (Zero loss)</text>

        <!-- Thermodynamic Note -->
        <rect x="20" y="276" width="280" height="66" rx="4" fill="#141a22" stroke="#253549" />
        <text x="28" y="294" fill="#60a5fa" font-size="10" font-weight="700">NON-CONSERVED INFORMATION</text>
        <text x="28" y="310" fill="#9db0c6" font-size="10">Signals are weightless and free. Unlike</text>
        <text x="28" y="324" fill="#9db0c6" font-size="10">conserved substances, signals can multiply,</text>
        <text x="28" y="336" fill="#9db0c6" font-size="10">decay, or reset without violating physics.</text>
      </g>

      <!-- Right Side: Decay Curves Coordinate Graph -->
      <g transform="translate(370, 24)">
        <!-- Graph Area Background -->
        <rect width="546" height="332" rx="6" fill="#161b22" stroke="#2d3748" />

        <!-- Grid Lines -->
        <line x1="50" y1="40" x2="520" y2="40" stroke="#242c38" stroke-dasharray="3 3" />
        <text x="42" y="44" fill="#627084" font-size="10" text-anchor="end">1.0</text>

        <line x1="50" y1="100" x2="520" y2="100" stroke="#242c38" stroke-dasharray="3 3" />
        <text x="42" y="104" fill="#627084" font-size="10" text-anchor="end">0.75</text>

        <line x1="50" y1="160" x2="520" y2="160" stroke="#242c38" stroke-dasharray="3 3" />
        <text x="42" y="164" fill="#627084" font-size="10" text-anchor="end">0.50</text>

        <line x1="50" y1="220" x2="520" y2="220" stroke="#242c38" stroke-dasharray="3 3" />
        <text x="42" y="224" fill="#627084" font-size="10" text-anchor="end">0.25</text>

        <line x1="50" y1="280" x2="520" y2="280" stroke="#3b4759" />
        <text x="42" y="284" fill="#627084" font-size="10" text-anchor="end">0.0</text>

        <!-- Vertical Grid Lines & Ticks -->
        <line x1="50" y1="280" x2="50" y2="40" stroke="#3b4759" />
        <text x="50" y="300" fill="#627084" font-size="10" text-anchor="middle">t=0</text>
        <line x1="128" y1="280" x2="128" y2="40" stroke="#242c38" stroke-dasharray="3 3" />
        <text x="128" y="300" fill="#627084" font-size="10" text-anchor="middle">t=2</text>
        <line x1="206" y1="280" x2="206" y2="40" stroke="#242c38" stroke-dasharray="3 3" />
        <text x="206" y="300" fill="#627084" font-size="10" text-anchor="middle">t=4</text>
        <line x1="284" y1="280" x2="284" y2="40" stroke="#242c38" stroke-dasharray="3 3" />
        <text x="284" y="300" fill="#627084" font-size="10" text-anchor="middle">t=6</text>
        <line x1="362" y1="280" x2="362" y2="40" stroke="#242c38" stroke-dasharray="3 3" />
        <text x="362" y="300" fill="#627084" font-size="10" text-anchor="middle">t=8</text>
        <line x1="440" y1="280" x2="440" y2="40" stroke="#242c38" stroke-dasharray="3 3" />
        <text x="440" y="300" fill="#627084" font-size="10" text-anchor="middle">t=10</text>
        <line x1="518" y1="280" x2="518" y2="40" stroke="#242c38" stroke-dasharray="3 3" />
        <text x="518" y="300" fill="#627084" font-size="10" text-anchor="middle">t=12</text>

        <!-- Impulse Marker at t=1 -->
        <rect x="85" y="40" width="8" height="240" fill="rgba(243, 130, 48, 0.12)" />
        <text x="89" y="30" fill="#f38230" font-size="9" text-anchor="middle" font-weight="700">Impulse</text>

        <!-- Curve 1: keep = 1.0 (Purple) -->
        <path d="M 50 280 L 89 40 L 518 40" fill="none" stroke="#a78bfa" stroke-width="3" />
        <circle cx="518" cy="40" r="4" fill="#a78bfa" />
        <text x="500" y="32" fill="#c4b5fd" font-size="10" font-weight="700">keep: 1.0</text>

        <!-- Curve 2: keep = 0.9 (Green) -->
        <!-- Points: t=1: 40; t=2: 64; t=3: 86; t=4: 105; t=5: 123; t=6: 138; t=7: 153; t=8: 165; t=9: 177; t=10: 187; t=12: 205 -->
        <path d="M 50 280 L 89 40 Q 150 90, 220 140 T 362 205 T 518 245" fill="none" stroke="#34d399" stroke-width="3" />
        <circle cx="518" cy="245" r="4" fill="#34d399" />
        <text x="500" y="238" fill="#6ee7b7" font-size="10" font-weight="700">keep: 0.9</text>

        <!-- Curve 3: keep = 0.5 (Yellow) -->
        <!-- Points: t=1: 40; t=2: 160; t=3: 220; t=4: 250; t=5: 265; t=6: 272; t=7: 280 -->
        <path d="M 50 280 L 89 40 Q 105 130, 128 160 Q 160 220, 206 265 L 518 280" fill="none" stroke="#fbbf24" stroke-width="3" />
        <circle cx="206" cy="265" r="4" fill="#fbbf24" />
        <text x="180" y="240" fill="#fde68a" font-size="10" font-weight="700">keep: 0.5</text>

        <!-- Curve 4: keep = 0.0 (Red) -->
        <path d="M 50 280 L 89 40 L 128 280 L 518 280" fill="none" stroke="#f87171" stroke-width="3" />
        <circle cx="128" cy="280" r="4" fill="#f87171" />
        <text x="135" y="260" fill="#fca5a5" font-size="10" font-weight="700">keep: 0.0</text>

        <!-- Axis Titles -->
        <text x="280" y="324" fill="#9db0c6" font-size="11" text-anchor="middle">Simulation Time Steps (Ticks)</text>
      </g>
    </svg>
  </div>
  <figcaption class="diagram-caption">
    <strong>Internal Signal Persistence Dynamics:</strong> Signal values decay geometrically according to their declared <code>keep</code> factor. A factor of <code>0.0</code> models instantaneous reflex arcs, while <code>0.9</code> provides a smooth leaky integrator ideal for metabolic hunger, threat alarms, or cumulative sensory evidence.
  </figcaption>
</figure>"""

def generate_evolution_specialization_diagram():
    return """<figure class="diagram-figure">
  <div class="diagram-header">
    <span>Figure 4: Evolutionary Specialization (Module Duplication &amp; Divergence)</span>
    <span class="diagram-tag">Adaptive Radiation</span>
  </div>
  <div class="diagram-svg-wrap">
    <svg viewBox="0 0 940 370" width="100%" style="max-width: 940px; height: auto;" xmlns="http://www.w3.org/2000/svg" font-family="ui-sans-serif, system-ui, -apple-system, sans-serif">
      <defs>
        <marker id="arr-orange" viewBox="0 0 10 10" refX="6" refY="5" markerWidth="6" markerHeight="6" orient="auto-start-reverse">
          <path d="M 0 1 L 10 5 L 0 9 z" fill="#f38230" />
        </marker>
      </defs>
      <rect width="940" height="370" fill="#13161c" />

      <!-- Step 1: Ancestral Module -->
      <g transform="translate(20, 25)">
        <rect width="250" height="320" rx="8" fill="#181d26" stroke="#2d3748" stroke-width="1.5" />
        <rect x="15" y="15" width="220" height="24" rx="4" fill="#2d3545" />
        <text x="25" y="31" fill="#f38230" font-size="11" font-weight="800">1. ANCESTRAL MODULE</text>

        <rect x="15" y="52" width="220" height="250" rx="6" fill="#141820" stroke="#263040" />
        
        <!-- Interface Signal Badge -->
        <rect x="25" y="65" width="200" height="42" rx="4" fill="#1c2636" stroke="#3b5376" />
        <text x="35" y="82" fill="#7fa6d6" font-size="10" font-weight="700">INTERFACE SIGNAL:</text>
        <text x="35" y="98" fill="#ffffff" font-size="11" font-family="ui-monospace, monospace">light_dir (Shared Input)</text>

        <!-- Private Signal Badge -->
        <rect x="25" y="120" width="200" height="42" rx="4" fill="#261d33" stroke="#4a3563" />
        <text x="35" y="137" fill="#c39ee3" font-size="10" font-weight="700">PRIVATE SIGNAL:</text>
        <text x="35" y="153" fill="#ffffff" font-size="11" font-family="ui-monospace, monospace">steer_priv (Module Internal)</text>

        <!-- Gene Summary -->
        <text x="25" y="185" fill="#9db0c6" font-size="10" font-weight="700">GENES IN MODULE:</text>
        <text x="25" y="204" fill="#e2e8f0" font-size="11">• sense gradient(light) -&gt; light_dir</text>
        <text x="25" y="222" fill="#e2e8f0" font-size="11">• regulate weigh &lt;- light_dir -&gt; steer</text>
        <text x="25" y="240" fill="#e2e8f0" font-size="11">• action turn &lt;- steer_priv</text>

        <rect x="25" y="260" width="200" height="30" rx="3" fill="#102521" stroke="#214b42" />
        <text x="125" y="279" fill="#5fbfae" font-size="10" font-weight="700" text-anchor="middle">Function: Phototaxis (Sunlight)</text>
      </g>

      <!-- Arrow 1 -> 2 -->
      <path d="M 280 185 L 320 185" fill="none" stroke="#f38230" stroke-width="3" marker-end="url(#arr-orange)" />
      <text x="300" y="175" fill="#f38230" font-size="10" font-weight="700" text-anchor="middle">Duplicate</text>

      <!-- Step 2: Duplication & Re-wiring -->
      <g transform="translate(335, 25)">
        <rect width="260" height="320" rx="8" fill="#181d26" stroke="#f38230" stroke-width="1.5" />
        <rect x="15" y="15" width="230" height="24" rx="4" fill="#3a2517" />
        <text x="25" y="31" fill="#f38230" font-size="11" font-weight="800">2. DUPLICATION MUTATION</text>

        <rect x="15" y="52" width="230" height="250" rx="6" fill="#141820" stroke="#263040" />

        <!-- Interface Signal: Preserved -->
        <rect x="25" y="65" width="210" height="42" rx="4" fill="#1c2636" stroke="#3b5376" />
        <text x="35" y="82" fill="#7fa6d6" font-size="10" font-weight="700">SHARED INTERFACE:</text>
        <text x="35" y="98" fill="#9db0c6" font-size="11" font-family="ui-monospace, monospace">light_dir (Preserved connection)</text>

        <!-- Private Signal: Re-mapped! -->
        <rect x="25" y="120" width="210" height="52" rx="4" fill="#2d1c16" stroke="#7a3f1d" />
        <text x="35" y="137" fill="#f38230" font-size="10" font-weight="700">RE-MAPPED PRIVATE SIGNAL:</text>
        <text x="35" y="153" fill="#ffffff" font-size="11" font-family="ui-monospace, monospace">steer_priv_2 (Fresh ID)</text>
        <text x="35" y="165" fill="#e0a458" font-size="9">✓ Prevents destructive cross-talk!</text>

        <text x="25" y="195" fill="#9db0c6" font-size="10" font-weight="700">GENOME INTEGRITY:</text>
        <text x="25" y="214" fill="#e2e8f0" font-size="11">• Full working copy is formed</text>
        <text x="25" y="232" fill="#e2e8f0" font-size="11">• Original behavior undamaged</text>
        <text x="25" y="250" fill="#e2e8f0" font-size="11">• Free to mutate neutrally</text>

        <rect x="25" y="265" width="210" height="25" rx="3" fill="#2a1e12" stroke="#4d321c" />
        <text x="130" y="281" fill="#e0a458" font-size="9" font-weight="700" text-anchor="middle">Redundant Backup Sub-network</text>
      </g>

      <!-- Arrow 2 -> 3 -->
      <path d="M 605 185 L 645 185" fill="none" stroke="#f38230" stroke-width="3" marker-end="url(#arr-orange)" />
      <text x="625" y="175" fill="#f38230" font-size="10" font-weight="700" text-anchor="middle">Mutate</text>

      <!-- Step 3: Functional Divergence -->
      <g transform="translate(660, 25)">
        <rect width="260" height="320" rx="8" fill="#181d26" stroke="#255249" stroke-width="1.5" />
        <rect x="15" y="15" width="230" height="24" rx="4" fill="#1a3430" />
        <text x="25" y="31" fill="#3eb49f" font-size="11" font-weight="800">3. ADAPTIVE DIVERGENCE</text>

        <rect x="15" y="52" width="230" height="250" rx="6" fill="#141820" stroke="#263040" />

        <!-- Mutated Input -->
        <rect x="25" y="65" width="210" height="42" rx="4" fill="#182c28" stroke="#2a6659" />
        <text x="35" y="82" fill="#5fbfae" font-size="10" font-weight="700">MUTATED RE-WIRED SENSOR:</text>
        <text x="35" y="98" fill="#ffffff" font-size="11" font-family="ui-monospace, monospace">cells.sugar (Chemotaxis)</text>

        <!-- Mutated Weights -->
        <rect x="25" y="120" width="210" height="42" rx="4" fill="#182c28" stroke="#2a6659" />
        <text x="35" y="137" fill="#5fbfae" font-size="10" font-weight="700">SPECIALIZED WEIGHTS:</text>
        <text x="35" y="153" fill="#ffffff" font-size="11" font-family="ui-monospace, monospace">weight: -14.0 (Inverted curve)</text>

        <text x="25" y="185" fill="#9db0c6" font-size="10" font-weight="700">EVOLVED CAPABILITY:</text>
        <text x="25" y="204" fill="#e2e8f0" font-size="11">• Module 1: Tracks sunlight</text>
        <text x="25" y="222" fill="#e2e8f0" font-size="11">• Module 2: Tracks glucose food</text>
        <text x="25" y="240" fill="#e2e8f0" font-size="11">• Dual-competency emergence</text>

        <rect x="25" y="260" width="210" height="30" rx="3" fill="#102c25" stroke="#206153" />
        <text x="130" y="279" fill="#5fbfae" font-size="10" font-weight="700" text-anchor="middle">Specialized Food-Seeking Organ</text>
      </g>
    </svg>
  </div>
  <figcaption class="diagram-caption">
    <strong>Module Duplication and Divergence:</strong> Functional sub-networks grow by whole-module duplication. Private internal signals are automatically re-mapped to fresh identifiers, isolating the duplicate from destructive interference while point mutations adapt sensor targets and weights.
  </figcaption>
</figure>"""

def generate_gpu_memory_layout_diagram():
    return """<figure class="diagram-figure">
  <div class="diagram-header">
    <span>Figure 5: Genome Profile &amp; Fixed-Stride GPU Array Layout</span>
    <span class="diagram-tag">SIMD / Lockstep Storage</span>
  </div>
  <div class="diagram-svg-wrap">
    <svg viewBox="0 0 940 310" width="100%" style="max-width: 940px; height: auto;" xmlns="http://www.w3.org/2000/svg" font-family="ui-sans-serif, system-ui, -apple-system, sans-serif">
      <rect width="940" height="310" fill="#13161c" />

      <!-- Profile Header -->
      <g transform="translate(24, 20)">
        <rect width="892" height="36" rx="4" fill="#1c212c" stroke="#313d52" />
        <text x="16" y="23" fill="#f38230" font-size="13" font-weight="800" font-family="ui-monospace, monospace">profile standard { genes: 32; signals: 16; modules: 8; }</text>
        <text x="876" y="23" fill="#8da0b8" font-size="11" text-anchor="end">GPU Warp Lockstep Structure</text>
      </g>

      <!-- Gene Array Strip -->
      <g transform="translate(24, 72)">
        <text x="0" y="16" fill="#ffffff" font-size="12" font-weight="700">1. GENE ARRAY SLOTS (32 contiguous fixed-stride slots per creature)</text>
        
        <!-- Slots Visual -->
        <g transform="translate(0, 26)">
          <!-- Active Gene Slots (0..3) -->
          <rect x="0" y="0" width="80" height="44" rx="3" fill="#1e3450" stroke="#3b689e" />
          <text x="40" y="18" fill="#7fa6d6" font-size="10" font-weight="700" text-anchor="middle">Slot 0</text>
          <text x="40" y="32" fill="#ffffff" font-size="9" text-anchor="middle">sense: active</text>

          <rect x="85" y="0" width="80" height="44" rx="3" fill="#322246" stroke="#6f4c9c" />
          <text x="125" y="18" fill="#c39ee3" font-size="10" font-weight="700" text-anchor="middle">Slot 1</text>
          <text x="125" y="32" fill="#ffffff" font-size="9" text-anchor="middle">regulate: on</text>

          <rect x="170" y="0" width="80" height="44" rx="3" fill="#1c3833" stroke="#3b8275" />
          <text x="210" y="18" fill="#62c7b5" font-size="10" font-weight="700" text-anchor="middle">Slot 2</text>
          <text x="210" y="32" fill="#ffffff" font-size="9" text-anchor="middle">action: on</text>

          <rect x="255" y="0" width="80" height="44" rx="3" fill="#1c3833" stroke="#3b8275" />
          <text x="295" y="18" fill="#62c7b5" font-size="10" font-weight="700" text-anchor="middle">Slot 3</text>
          <text x="295" y="32" fill="#ffffff" font-size="9" text-anchor="middle">action: on</text>

          <rect x="340" y="0" width="80" height="44" rx="3" fill="#3b2719" stroke="#875129" />
          <text x="380" y="18" fill="#e0a458" font-size="10" font-weight="700" text-anchor="middle">Slot 4</text>
          <text x="380" y="32" fill="#ffffff" font-size="9" text-anchor="middle">reaction: on</text>

          <!-- Dormant Gene Slot (5) -->
          <rect x="425" y="0" width="80" height="44" rx="3" fill="#1e2229" stroke="#475263" stroke-dasharray="3 3" />
          <text x="465" y="18" fill="#8d99ab" font-size="10" font-weight="700" text-anchor="middle">Slot 5</text>
          <text x="465" y="32" fill="#fbbf24" font-size="9" text-anchor="middle">dormant (0 fee)</text>

          <!-- Empty Slots (6..31) -->
          <rect x="510" y="0" width="382" height="44" rx="3" fill="#14171d" stroke="#272e38" />
          <text x="701" y="27" fill="#586374" font-size="11" text-anchor="middle">Slots 6 .. 31 : Empty Profile Capacity Headroom (No Upkeep Cost)</text>
        </g>
      </g>

      <!-- Signal & Module Row -->
      <g transform="translate(24, 175)">
        <!-- Signals (Left) -->
        <g>
          <text x="0" y="16" fill="#ffffff" font-size="12" font-weight="700">2. SIGNAL TABLE (16 Slots)</text>
          <g transform="translate(0, 26)">
            <rect x="0" y="0" width="105" height="38" rx="3" fill="#1c2636" stroke="#3b5882" />
            <text x="52" y="16" fill="#7fa6d6" font-size="9" font-weight="700" text-anchor="middle">hunger (keep 0.9)</text>
            <text x="52" y="30" fill="#ffffff" font-size="9" text-anchor="middle">val: f32 | active</text>

            <rect x="110" y="0" width="105" height="38" rx="3" fill="#1c2636" stroke="#3b5882" />
            <text x="162" y="16" fill="#7fa6d6" font-size="9" font-weight="700" text-anchor="middle">steer (keep 0.0)</text>
            <text x="162" y="30" fill="#ffffff" font-size="9" text-anchor="middle">val: f32 | active</text>

            <rect x="220" y="0" width="200" height="38" rx="3" fill="#14171d" stroke="#272e38" />
            <text x="320" y="24" fill="#586374" font-size="10" text-anchor="middle">Signals 2 .. 15 : Available</text>
          </g>
        </g>

        <!-- Upkeep Ledger (Right) -->
        <g transform="translate(460, 0)">
          <text x="0" y="16" fill="#f38230" font-size="12" font-weight="700">3. THERMODYNAMIC UPKEEP CALCULATION</text>
          <g transform="translate(0, 26)">
            <rect x="0" y="0" width="432" height="65" rx="4" fill="#19171f" stroke="#403352" />
            <text x="14" y="20" fill="#e2e8f0" font-size="11">Base Creature Living Cost: <tspan fill="#f38230" font-weight="700">20mu</tspan></text>
            <text x="14" y="38" fill="#e2e8f0" font-size="11">Active Genes: 5 × 5mu = <tspan fill="#f38230" font-weight="700">25mu</tspan> | Dormant: <tspan fill="#34d399">0mu (free neutral cache)</tspan></text>
            <text x="14" y="54" fill="#e2e8f0" font-size="11">Active Signals: 2 × 1mu = <tspan fill="#f38230" font-weight="700">2mu</tspan> | Total Upkeep: <tspan fill="#f38230" font-weight="800">47mu / tick</tspan></text>
          </g>
        </g>
      </g>
    </svg>
  </div>
  <figcaption class="diagram-caption">
    <strong>Profile Memory Model:</strong> Lineages pre-allocate fixed-size gene, signal, and module tables, allowing parallel GPU execution across 100,000+ creatures. Inactive and dormant genes cost 0 upkeep, providing a zero-cost reservoir for neutral genetic drift and evolutionary novelty.
  </figcaption>
</figure>"""

if __name__ == "__main__":
    for name, gen in [
        ("Network", generate_network_diagram),
        ("Four Genes", generate_four_gene_kinds_diagram),
        ("Signal Memory", generate_signal_memory_diagram),
        ("Evolution", generate_evolution_specialization_diagram),
        ("GPU Memory", generate_gpu_memory_layout_diagram),
    ]:
        html = gen()
        # Extract SVG and test XML validity
        svg_start = html.find("<svg")
        svg_end = html.find("</svg>") + 6
        svg_str = html[svg_start:svg_end]
        ET.fromstring(svg_str)
        print(f"✓ {name} SVG is valid XML!")
