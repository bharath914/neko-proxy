// Montage sound effects, kept subtle: a soft low pulse on each cut, a few quiet ticks and chimes.
// Times are output times; they follow the CUTS list in montage.mjs.
var fx = bus()
// A soft low pulse with a faint glassy tail, landing on each cut.
func pulse(_ at: Double, _ g: Double) {
    add(at, 0.4, g, into: &fx) { s in sin(tau * (70 + 90 * exp(-s / 0.018)) * s) * exp(-s / 0.1) }
    add(at, 0.35, g * 0.18, pan: 0.2, into: &fx) { s in sin(tau * mtof(98) * s) * env(s, 0.002, 0.07) }
}
func tick(_ at: Double, _ m: Double, _ g: Double, pan: Double = 0) {
    let f = mtof(m)
    add(at, 0.35, g, pan: pan, into: &fx) { s in (sin(tau * f * s) + 0.2 * sin(tau * 2 * f * s)) * env(s, 0.003, 0.08) }
}
for c in [5.0, 9.0, 13.0, 15.0, 19.0, 21.65] { pulse(c, 0.16) }
pulse(0.1, 0.12)
// Opener: cards and chips.
tick(0.5, 79, 0.05, pan: -0.3); tick(0.75, 83, 0.05, pan: 0.3)
for (i, at) in [1.9, 2.1, 2.4].enumerated() { tick(at, [86.0, 88, 91][i], 0.035) }
// Composer: the Send click, then three soft checks.
add(6.65, 0.08, 0.14, into: &fx) { s in (sin(tau * 1700 * s) * 0.5 + noise() * 0.5) * exp(-s / 0.006) + sin(tau * 160 * s) * exp(-s / 0.025) }
for (k, m) in [81.0, 85.0, 88.0].enumerated() { tick(7.1 + Double(k) * 0.2, m, 0.045, pan: -0.2 + Double(k) * 0.2) }
// Agent: the approval chime.
for (k, m) in [85.0, 90.0].enumerated() {
    let f = mtof(m)
    add(9.95 + Double(k) * 0.09, 1.0, 0.06, into: &fx) { s in (sin(tau * f * s) + 0.3 * sin(tau * f * 2.76 * s) * exp(-s / 0.1)) * env(s, 0.002, 0.3) }
}
// End card: a quiet bell.
for (k, m) in [74.0, 78, 81, 86].enumerated() {
    let f = mtof(m)
    add(21.7 + Double(k) * 0.07, 1.6, 0.03, pan: Double(k - 2) * 0.3, into: &fx) { s in (sin(tau * f * s) + 0.2 * sin(tau * f * 2.76 * s) * exp(-s / 0.15)) * env(s, 0.003, 0.5) }
}
mix(fx, 1)

