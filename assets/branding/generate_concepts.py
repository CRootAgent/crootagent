#!/usr/bin/env python3
"""
Fix exact 45-degree angle math in Concept A and evenodd hole in Concept C.
"""
import os

OUT_DIR = os.path.dirname(os.path.abspath(__file__))

# Concept A: exact 45/135 degree chevron math:
# (142, 50) -> (214, 122) (dx=72, dy=72 -> exact 45.0 deg)
# (214, 134) -> (142, 206) (dx=-72, dy=72 -> exact 135.0 deg)
# (142, 174) -> (188, 128) (dx=46, dy=-46 -> exact -45.0 deg)
# (188, 128) -> (142, 82) (dx=-46, dy=-46 -> exact -135.0 deg)
concept_a_svg = """<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 256 256" width="256" height="256">
  <title>Lontar CLI Logo - Manuscript Chevron</title>
  <g fill="#0F172A">
    <!-- Left Leaf Spine ('L' Anchor) -->
    <path d="M 40 44 C 40 36 46 32 54 32 L 88 32 C 96 32 102 38 102 46 L 102 164 L 140 164 C 148 164 154 170 154 178 L 154 210 C 154 218 148 224 140 224 L 54 224 C 46 224 40 218 40 210 Z" />
    <!-- Forward Prompt Blade (Exact 45.0 and 135.0 degree chevron) -->
    <path d="M 142 50 C 134 42 122 47 122 58 L 122 82 L 168 128 L 122 174 L 122 198 C 122 209 134 214 142 206 L 214 134 C 218 130 218 126 214 122 Z" />
    <!-- Grounding Dot (Knowledge Seed / Cursor Pulse) -->
    <circle cx="188" cy="196" r="14" />
  </g>
</svg>"""

concept_b_svg = """<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 256 256" width="256" height="256">
  <title>Lontar CLI Logo - Folio Stack</title>
  <g fill="#0F172A">
    <!-- Top Folio Sheet -->
    <rect x="36" y="40" width="184" height="42" rx="12" />
    <!-- Middle Folio Sheet (Shifted context layer) -->
    <rect x="48" y="104" width="172" height="42" rx="12" />
    <!-- Bottom Lontar Base (L-Anchor) -->
    <rect x="36" y="168" width="128" height="42" rx="12" />
    <!-- Terminal Prompt Node -->
    <polygon points="184,168 220,189 184,210" />
  </g>
</svg>"""

concept_c_svg = """<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 256 256" width="256" height="256">
  <title>Lontar CLI Logo - Sacred Tablet</title>
  <g fill="#0F172A" fill-rule="evenodd">
    <!-- Outer Rounded Tablet with True Inner Cutout Hole -->
    <path d="M 72 36 L 184 36 C 204 36 220 52 220 72 L 220 184 C 220 204 204 220 184 220 L 72 220 C 52 220 36 204 36 184 L 36 72 C 36 52 52 36 72 36 Z
             M 80 72 L 80 184 L 168 184 L 168 152 L 112 152 L 112 72 Z" />
    <!-- Terminal Prompt Chevron Inset -->
    <polygon points="132,84 164,108 132,132" />
  </g>
</svg>"""

def make_lockup(symbol_inner, title="Lontar Logo Lockup"):
    return f"""<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 768 256" width="768" height="256">
  <title>{title}</title>
  <g transform="translate(16, 0)">
    {symbol_inner}
  </g>
  <!-- Bespoke Vector Wordmark 'LONTAR' -->
  <g fill="#0F172A">
    <!-- L -->
    <path d="M 304 80 L 328 80 L 328 156 L 368 156 L 368 176 L 304 176 Z" />
    <!-- O -->
    <path fill-rule="evenodd" d="M 388 128 C 388 101 408 80 435 80 C 462 80 482 101 482 128 C 482 155 462 176 435 176 C 408 176 388 155 388 128 Z M 412 128 C 412 142 422 154 435 154 C 448 154 458 142 458 128 C 458 114 448 102 435 102 C 422 102 412 114 412 128 Z" />
    <!-- N -->
    <path d="M 504 80 L 526 80 L 558 136 L 558 80 L 580 80 L 580 176 L 558 176 L 526 120 L 526 176 L 504 176 Z" />
    <!-- T -->
    <path d="M 598 80 L 662 80 L 662 100 L 642 100 L 642 176 L 618 176 L 618 100 L 598 100 Z" />
    <!-- A -->
    <path fill-rule="evenodd" d="M 686 80 L 710 80 L 736 176 L 712 176 L 706 154 L 690 154 L 684 176 L 660 176 Z M 695 134 L 701 110 L 703 110 L 709 134 Z" />
    <!-- R -->
    <path fill-rule="evenodd" d="M 744 80 L 780 80 C 794 80 804 90 804 104 C 804 116 796 124 786 126 L 806 176 L 782 176 L 764 130 L 758 130 L 758 176 L 736 176 L 736 80 Z M 758 100 L 758 114 L 776 114 C 782 114 786 110 786 107 C 786 103 782 100 776 100 Z" />
  </g>
  <!-- Subtitle Tagline Pill 'NOTEBOOKLM CLI' -->
  <g fill="#0284C7" transform="translate(306, 196)">
    <rect x="0" y="0" width="144" height="26" rx="6" fill="#0284C7" fill-opacity="0.12" />
    <text x="10" y="17" font-family="-apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif" font-size="11" font-weight="700" letter-spacing="1.5" fill="#0284C7">NOTEBOOKLM CLI</text>
  </g>
</svg>"""

def get_body(svg_str):
    start = svg_str.find("<g")
    end = svg_str.rfind("</svg>")
    return svg_str[start:end]

with open(f"{OUT_DIR}/concept-a-symbol.svg", "w") as f:
    f.write(concept_a_svg)

with open(f"{OUT_DIR}/concept-b-symbol.svg", "w") as f:
    f.write(concept_b_svg)

with open(f"{OUT_DIR}/concept-c-symbol.svg", "w") as f:
    f.write(concept_c_svg)

with open(f"{OUT_DIR}/concept-a-lockup.svg", "w") as f:
    f.write(make_lockup(get_body(concept_a_svg), "Lontar Logo Concept A"))

with open(f"{OUT_DIR}/concept-b-lockup.svg", "w") as f:
    f.write(make_lockup(get_body(concept_b_svg), "Lontar Logo Concept B"))

with open(f"{OUT_DIR}/concept-c-lockup.svg", "w") as f:
    f.write(make_lockup(get_body(concept_c_svg), "Lontar Logo Concept C"))

print("Clean SVG files regenerated.")
