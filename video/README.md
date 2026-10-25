# LinkedIn / social video

A 1080x1080, 30 fps, about 24 s montage. `opener.html` (5 s, both features at a glance) comes first, then cuts of `concept.html` (agent and rules, requests, Composer, Native. Fast. Simple., end card) and `hook.html` (camera moves over real Composer screenshots in `shots/`). `montage.mjs` holds the cut list (page, source start, source end); every page is a pure function of time, so cuts can be re-ordered or trimmed freely.

Scenes: AI agent talks to the proxy (MCP) · rules and breakpoints · see every request · compose requests and test them (Composer) · Native. Fast. Simple. · end card.

```
cd video && npm install && npx playwright install chromium   # once; needs ffmpeg (brew install ffmpeg)
./build.sh neko-proxy.mp4 17                                  # effects + montage render (about 2 min)
node frames.mjs keys 12.6,14.6                                # check single frames of concept.html -> keys/k<t>.png
node frames_hook.mjs keysh 2,4,6                              # same for the hook (hook.html)
```

Editing rules: change timings in `concept.html` and the matching times in `synth.swift` together (sound effects for the montage are in `montage_fx.swift`, keyed to output times and kept quiet; update them when the cut list changes; `synth.swift` is the older full-length track), and `window.DURATION` plus `let dur` in the synth. Look at key frames before the full render. Keep the copy per the brief: no app metrics; say native, fast, simple UI, open source, connects to Claude via MCP.
