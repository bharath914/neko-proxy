// Synthesizes the soundtrack (music + effects) synced to concept.html.
// swiftc -O synth.swift -o synth && ./synth out.wav
import Foundation

let sr = 48000.0
let dur = 20.4
let n = Int(sr * dur)
var L = [Float](repeating: 0, count: n)
var R = [Float](repeating: 0, count: n)
let tau = 2.0 * Double.pi
// MUSIC=1 adds the background track; by default only the effects play.
let music = ProcessInfo.processInfo.environment["MUSIC"] == "1"
func idx(_ t: Double) -> Int { max(0, min(n, Int(t * sr))) }
func mtof(_ m: Double) -> Double { 440 * pow(2, (m - 69) / 12) }
var seed: UInt64 = 0x9E3779B97F4A7C15
func noise() -> Double { seed ^= seed << 13; seed ^= seed >> 7; seed ^= seed << 17; return Double(seed % 1_000_000) / 500_000 - 1 }

// Add a mono signal, panned (-1 left … 1 right).
func add(_ t0: Double, _ len: Double, _ gain: Double, pan: Double = 0, into b: inout ([Float], [Float]), _ f: (Double) -> Double) {
    let gl = gain * sqrt(0.5 * (1 - pan)), gr = gain * sqrt(0.5 * (1 + pan))
    let a = idx(t0), z = idx(t0 + len)
    guard a < z else { return }
    for i in a..<z {
        let v = f(Double(i) / sr - t0)
        b.0[i] += Float(v * gl); b.1[i] += Float(v * gr)
    }
}
func bus() -> ([Float], [Float]) { ([Float](repeating: 0, count: n), [Float](repeating: 0, count: n)) }
func mix(_ b: ([Float], [Float]), _ g: Double) { for i in 0..<n { L[i] += b.0[i] * Float(g); R[i] += b.1[i] * Float(g) } }
// One-pole low-pass, in place, with a cutoff that can move over time.
func lowpass(_ x: inout [Float], _ cutoff: (Double) -> Double) {
    var y: Double = 0
    for i in 0..<n { let a = 1 - exp(-tau * cutoff(Double(i) / sr) / sr); y += a * (Double(x[i]) - y); x[i] = Float(y) }
}
func highpass(_ x: inout [Float], _ cutoff: Double) {
    let a = exp(-tau * cutoff / sr); var lp: Double = 0
    for i in 0..<n { lp = (1 - a) * Double(x[i]) + a * lp; x[i] = Float(Double(x[i]) - lp) }
}
func env(_ t: Double, _ a: Double, _ d: Double) -> Double { t < a ? t / a : exp(-(t - a) / d) }
func saw(_ ph: Double) -> Double { 2 * (ph - floor(ph + 0.5)) }

// ---------- Music: 120 BPM, D major, chords every 2 s ----------
let beat = 0.5
let chords: [[Double]] = [
    [50, 54, 57, 61, 64],  // Dmaj9
    [47, 50, 54, 57, 61],  // Bm9
    [43, 47, 50, 54, 57],  // Gmaj7
    [45, 52, 57, 59, 61],  // A add9
    [50, 54, 57, 61, 64],  // Dmaj9
    [47, 50, 54, 57, 61],  // Bm9
    [43, 50, 54, 57, 62],  // Gmaj7 → end card
]
func chord(_ t: Double) -> [Double] { chords[min(chords.count - 1, Int(t / 2))] }

// Pad: detuned saws through a slowly opening low-pass.
var pad = bus()
for (c, notes) in chords.enumerated() {
    let t0 = Double(c) * 2 - 0.15, len = c == chords.count - 1 ? 2.3 : 2.45
    for (k, m) in notes.enumerated() {
        let f = mtof(m + 12)
        for (d, pan) in [(-0.11, -0.7), (0.0, 0.0), (0.12, 0.7)] {
            let fr = f * pow(2, d / 12), ph0 = Double(k) * 0.37 + d
            add(max(0, t0), len, 0.05, pan: pan, into: &pad) { t in
                let e = min(1, t / 0.35) * min(1, max(0, (len - t) / 0.4))
                return saw(ph0 + fr * t) * e
            }
        }
    }
}
lowpass(&pad.0) { t in 500 + 1400 * min(1, t / 8) + (t > 12.4 ? 900 : 0) }
lowpass(&pad.1) { t in 500 + 1400 * min(1, t / 8) + (t > 12.4 ? 900 : 0) }
if music { mix(pad, 0.9) }

// Arp: plucked chord tones in 8ths from the first typed key, with a ping-pong echo.
var arp = bus()
var t = 0.9 - 0.9.truncatingRemainder(dividingBy: 0.25)
var step = 0
while t < 12.3 {
    let c = chord(t), m = c[[1, 2, 3, 4, 3, 2][step % 6]] + 12
    let f = mtof(m), v = step % 4 == 0 ? 1.0 : 0.7
    add(t, 0.5, 0.11 * v, pan: step % 2 == 0 ? -0.3 : 0.3, into: &arp) { s in
        (sin(tau * f * s) + 0.25 * sin(tau * 2 * f * s) + 0.1 * sin(tau * 3 * f * s)) * env(s, 0.004, 0.13)
    }
    t += 0.25; step += 1
}
let dly = Int(0.375 * sr)
for i in dly..<n { arp.0[i] += arp.1[i - dly] * 0.38; arp.1[i] += arp.0[i - dly] * 0.38 }
if music { mix(arp, 1) }

// Bass: a soft sine on the root, pulsing on beats 1 and 3, from the second headline.
var bass = bus()
t = 3.0
while t < 12.3 {
    let f = mtof(chord(t)[0] - 12)
    add(t, 0.95, 0.22, into: &bass) { s in (sin(tau * f * s) + 0.15 * sin(tau * 2 * f * s)) * min(1, s / 0.01) * exp(-s / 0.45) }
    t += 1
}
if music { mix(bass, 1) }

// Drums: kick on every beat and offbeat hats from the second headline; quiet during scene changes.
var drums = bus()
t = 3.0
while t < 12.3 {
    let gap = (t > 8.3 && t < 8.7) || (t > 10.7 && t < 10.9)
    if !gap {
        add(t, 0.4, 0.42, into: &drums) { s in
            let f = 45 + 95 * exp(-s / 0.03)
            return sin(tau * f * s + 2 * exp(-s / 0.03)) * exp(-s / 0.16)
        }
    }
    t += beat
}
var hats = bus()
t = 4.25
var h = 0
while t < 12.3 {
    if !((t > 8.3 && t < 8.7) || (t > 10.7 && t < 10.9)) {
        add(t, 0.06, h % 2 == 0 ? 0.13 : 0.08, pan: h % 2 == 0 ? 0.25 : -0.25, into: &hats) { s in noise() * exp(-s / 0.012) }
    }
    t += beat / 2; h += 1
}
highpass(&hats.0, 7000); highpass(&hats.1, 7000)
if music { mix(drums, 1); mix(hats, 1) }

// ---------- Effects ----------
var fx = bus()
// Swells into each scene change.
for at in [2.95, 8.6, 10.9, 15.7, 18.3] {
    var sw = bus()
    add(at - 0.55, 0.75, 0.32, into: &sw) { s in
        let k = s / 0.55
        return noise() * (k < 1 ? pow(k, 2.2) : exp(-(s - 0.55) / 0.06))
    }
    highpass(&sw.0, 900); highpass(&sw.1, 900)
    lowpass(&sw.0) { tt in 1500 + 6000 * max(0, min(1, (tt - at + 0.55) / 0.55)) }
    lowpass(&sw.1) { tt in 1500 + 6000 * max(0, min(1, (tt - at + 0.55) / 0.55)) }
    mix(sw, 1)
}
// Typing: one key tick per character.
for i in 0..<34 {
    let at = 0.9 + Double(i) / 34 * 1.0 + noise() * 0.006
    let g = 0.08 + 0.04 * abs(noise())
    let f = 2400 + 600 * noise()
    add(at, 0.03, g, pan: 0.45, into: &fx) { s in (noise() * 0.6 + sin(tau * f * s) * 0.4) * exp(-s / 0.004) }
}
// Tool calls: a rising two-note blip. Results: a single soft ping.
func blip(_ at: Double, _ m1: Double, _ m2: Double, _ g: Double, pan: Double) {
    for (k, m) in [m1, m2].enumerated() {
        let f = mtof(m)
        add(at + Double(k) * 0.06, 0.3, g, pan: pan, into: &fx) { s in sin(tau * f * s) * env(s, 0.003, 0.07) }
    }
}
for at in [2.1, 3.05, 5.5] { blip(at, 81, 88, 0.12, pan: 0.4) }
for at in [2.45, 4.05, 5.85] { blip(at, 93, 93, 0.07, pan: 0.4) }
// Taps: a click plus a little body.
for (at, pan) in [(3.85, -0.2), (4.85, -0.5), (6.3, -0.5)] {
    add(at, 0.08, 0.35, pan: pan, into: &fx) { s in (sin(tau * 1700 * s) * 0.5 + noise() * 0.5) * exp(-s / 0.006) + sin(tau * 160 * s) * exp(-s / 0.025) }
}
// Approval card appears.
add(3.3, 0.5, 0.1, into: &fx) { s in sin(tau * (600 + 500 * min(1, s / 0.2)) * s) * env(s, 0.03, 0.12) }
// Allowed: a bright two-note bell.
for (k, m) in [85.0, 90.0].enumerated() {
    let f = mtof(m)
    add(3.95 + Double(k) * 0.09, 1.2, 0.13, into: &fx) { s in (sin(tau * f * s) + 0.3 * sin(tau * f * 2.76 * s) * exp(-s / 0.1)) * env(s, 0.002, 0.35) }
}
// Error on the phone: two falling notes.
for (k, m) in [69.0, 65.0].enumerated() {
    let f = mtof(m)
    add(5.15 + Double(k) * 0.13, 0.5, 0.13, pan: -0.5, into: &fx) { s in (sin(tau * f * s) + 0.3 * saw(f * s)) * env(s, 0.005, 0.12) }
}
// Held at the breakpoint: a low wooden tock and a held tone that fades.
add(6.55, 0.2, 0.3, into: &fx) { s in sin(tau * 220 * s) * exp(-s / 0.03) + sin(tau * 660 * s) * exp(-s / 0.012) * 0.4 }
add(6.6, 0.9, 0.06, pan: 0.4, into: &fx) { s in sin(tau * mtof(81) * s) * env(s, 0.01, 0.3) }
// Agent's closing line: a little sparkle.
for k in 0..<4 {
    let f = mtof([86.0, 90, 93, 98][k])
    add(7.0 + Double(k) * 0.05, 0.6, 0.05, pan: 0.5, into: &fx) { s in sin(tau * f * s) * env(s, 0.002, 0.15) }
}
// List rows: rising ticks.
for i in 0..<6 {
    let f = mtof(74 + Double([0, 2, 4, 7, 9, 12][i]))
    add(8.95 + Double(i) * 0.2, 0.25, 0.08, into: &fx) { s in sin(tau * f * s) * env(s, 0.002, 0.05) }
}
// Composer: typing, send, response, three passing tests, feature chips.
for i in 0..<20 {
    let at = 11.45 + Double(i) / 20 * 0.9 + noise() * 0.006
    let f = 2400 + 600 * noise()
    add(at, 0.03, 0.09 + 0.04 * abs(noise()), pan: 0.2, into: &fx) { s in (noise() * 0.6 + sin(tau * f * s) * 0.4) * exp(-s / 0.004) }
}
add(10.95, 0.4, 0.1, into: &fx) { s in sin(tau * (500 + 400 * min(1, s / 0.2)) * s) * env(s, 0.03, 0.12) }
add(12.55, 0.08, 0.35, pan: 0.3, into: &fx) { s in (sin(tau * 1700 * s) * 0.5 + noise() * 0.5) * exp(-s / 0.006) + sin(tau * 160 * s) * exp(-s / 0.025) }
add(12.62, 0.35, 0.09, pan: 0.2, into: &fx) { s in sin(tau * mtof(79) * s) * env(s, 0.003, 0.12) }
for (k, m) in [81.0, 85.0, 88.0].enumerated() {
    let f = mtof(m)
    add(13.0 + Double(k) * 0.2, 0.7, 0.11, pan: -0.2 + Double(k) * 0.2, into: &fx) { s in (sin(tau * f * s) + 0.25 * sin(tau * f * 2.76 * s) * exp(-s / 0.08)) * env(s, 0.002, 0.25) }
}
for (i, at) in [13.8, 13.92, 14.04, 14.16].enumerated() {
    let f = mtof([79.0, 83, 86, 91][i])
    add(at, 0.3, 0.07, pan: (i % 2 == 0 ? -0.3 : 0.3), into: &fx) { s in (sin(tau * f * s) + 0.2 * sin(tau * 2 * f * s)) * env(s, 0.002, 0.07) }
}
// Native. Fast. Simple.: a soft thump per word, then a pop per card.
for at in [15.65, 15.85, 16.05] {
    add(at, 0.35, 0.3, into: &fx) { s in sin(tau * (60 + 80 * exp(-s / 0.02)) * s) * exp(-s / 0.12) }
}
for (i, at) in [16.3, 16.42, 16.54, 16.66, 16.9].enumerated() {
    let f = mtof([79.0, 83, 86, 88, 91][i])
    add(at, 0.3, i == 4 ? 0.11 : 0.08, pan: i == 4 ? 0 : (i % 2 == 0 ? -0.35 : 0.35), into: &fx) { s in (sin(tau * f * s) + 0.2 * sin(tau * 2 * f * s)) * env(s, 0.002, 0.07) }
}
// End card: a bell bloom on D.
for (k, m) in [74.0, 78, 81, 86, 90].enumerated() {
    let f = mtof(m)
    add(18.3 + Double(k) * 0.07, 1.8, 0.07, pan: Double(k - 2) * 0.3, into: &fx) { s in (sin(tau * f * s) + 0.2 * sin(tau * f * 2.76 * s) * exp(-s / 0.15)) * env(s, 0.003, 0.6) }
}
mix(fx, 1)

// ---------- Master: fade in/out, soft clip, normalize, 16-bit WAV ----------
for i in 0..<n {
    let tt = Double(i) / sr
    let g = Float(min(1, tt / 0.25) * min(1, (dur - tt) / 0.7))
    L[i] = tanh(L[i] * 1.1) * g; R[i] = tanh(R[i] * 1.1) * g
}
let peak = max(L.map(abs).max()!, R.map(abs).max()!)
let norm = 0.89 / peak
var data = Data()
func u32(_ v: UInt32) { withUnsafeBytes(of: v.littleEndian) { data.append(contentsOf: $0) } }
func u16(_ v: UInt16) { withUnsafeBytes(of: v.littleEndian) { data.append(contentsOf: $0) } }
data.append("RIFF".data(using: .ascii)!); u32(UInt32(36 + n * 4)); data.append("WAVEfmt ".data(using: .ascii)!)
u32(16); u16(1); u16(2); u32(UInt32(sr)); u32(UInt32(sr) * 4); u16(4); u16(16)
data.append("data".data(using: .ascii)!); u32(UInt32(n * 4))
var pcm = [Int16](repeating: 0, count: n * 2)
for i in 0..<n { pcm[2 * i] = Int16(max(-1, min(1, L[i] * norm)) * 32767); pcm[2 * i + 1] = Int16(max(-1, min(1, R[i] * norm)) * 32767) }
pcm.withUnsafeBytes { data.append(contentsOf: $0) }
try! data.write(to: URL(fileURLWithPath: CommandLine.arguments[1]))
print("wrote \(CommandLine.arguments[1]): \(dur) s, peak \(peak) → 0.89")
