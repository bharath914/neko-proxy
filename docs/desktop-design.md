# Desktop app design

Built to the `apple-design` skill (`.claude/skills/apple-design/SKILL.md`), translated from web to a native immediate-mode UI.

| Principle | Where it shows up |
|---|---|
| Respond on pointer-down | Buttons, rows, tabs and the record pill show their pressed state on the press frame, not on release. Rows select on press |
| Springs, critically damped by default | `src/motion.rs`: response and damping ratio, `stiffness = (2π / response)²`. All state changes use `SMOOTH`, `MOVE` or `QUICK` at damping 1.0 |
| Interruptible, start from the live value | Every animated value is a persisted spring that retargets with its current position and velocity. Click a segmented control twice quickly and the thumb bends, it does not restart |
| Direct manipulation | The detail divider follows the pointer 1:1, resists past its bounds with the rubber-band function, and settles on release |
| Momentum | A hard flick on the divider projects where it would land (`project()`, 0.998) and snaps to the nearest of min, default or max on the under-damped spring. A slow release stays put |
| Materials and depth | Tonal layers by role (sidebar, toolbar, content, grouped detail pane, card). White cards lift off the grouped background in light; dark uses elevated greys and a deeper code surface. Shadows deepen with surface size. Scroll-edge fade under the table header instead of a rule. No true backdrop blur: egui has none |
| Color | Apple system colors: the accessible deepened variants for text on light, the vivid ones on dark. Badge and banner washes use `Palette::tint`, which takes the brighter system hue so they never go muddy. Light, dark and Auto are one click away in the sidebar; switching eases the palette over about 300 ms instead of jumping in brightness |
| Code and markup | One syntax palette (`Palette::syn`) for JSON, markup and plain text: cyan/blue keys, coral/vermilion strings, mint/green numbers, pink/magenta literals and tags. Line-number gutter, indent guides, folds as pills. The viewer always gets real height: at least 200 px, and up to the pane's height, scrolling inside itself |
| HTML | The core scans markup once into a simplified preview (headings, paragraphs, lists, links, code, quotes, image placeholders) and highlighted source. Scripts, styles and remote resources are never executed or fetched. Links copy on click. Preview and Source are one toggle apart |
| Typography | San Francisco (SF Pro Text and SF Mono) when installed in a system font folder or `PROXY_FONT_DIR`, since Apple's license does not allow shipping it; otherwise Inter and JetBrains Mono, bundled because egui cannot select weights from a variable system font. Tracking is per role: tight for titles, open for small caps labels |
| Reduced motion, contrast | Reads GNOME `enable-animations` and `high-contrast`. Reduced motion replaces springs with immediate values or short fades; high contrast hardens separators and secondary text. Overridable in Settings |
| Agency, forgiveness | Destructive actions confirm once. System-changing commands are shown, then run only on confirmation. Nothing changes system proxy settings |
| Wayfinding | The sidebar always shows where you are, core state, proxy address and certificate trust. Empty states say what to do next |
| Simplicity | One list, one inspector. Advanced options (raw view, body modes) are one level down |

Known gaps: no backdrop blur, no custom title bar (native decorations are kept), egui has no IME composition polish, and the layout is tuned for 960 px and wider.
