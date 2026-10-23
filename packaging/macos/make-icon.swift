// Draws the app icon (same artwork as packaging/dev.proxy.Proxy.svg) into an .iconset.
// Usage: swift make-icon.swift <out.iconset>
import AppKit

let out = CommandLine.arguments[1]
try FileManager.default.createDirectory(atPath: out, withIntermediateDirectories: true)

func render(_ px: Int) -> Data {
    let rep = NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: px, pixelsHigh: px, bitsPerSample: 8,
                               samplesPerPixel: 4, hasAlpha: true, isPlanar: false, colorSpaceName: .deviceRGB,
                               bytesPerRow: 0, bitsPerPixel: 0)!
    NSGraphicsContext.saveGraphicsState()
    NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: rep)
    let s = CGFloat(px) / 128
    let tile = NSBezierPath(roundedRect: NSRect(x: 8 * s, y: 8 * s, width: 112 * s, height: 112 * s), xRadius: 26 * s, yRadius: 26 * s)
    NSGradient(starting: NSColor(srgbRed: 0x4D / 255, green: 0xA3 / 255, blue: 1, alpha: 1),
               ending: NSColor(srgbRed: 0x0A / 255, green: 0x6C / 255, blue: 1, alpha: 1))!.draw(in: tile, angle: -90)
    func stroke(_ pts: [(CGFloat, CGFloat)], alpha: CGFloat) {
        let p = NSBezierPath()
        // SVG y runs down; flip into AppKit's y-up space.
        p.move(to: NSPoint(x: pts[0].0 * s, y: (128 - pts[0].1) * s))
        for q in pts.dropFirst() { p.line(to: NSPoint(x: q.0 * s, y: (128 - q.1) * s)) }
        p.lineWidth = 9 * s
        p.lineCapStyle = .round
        p.lineJoinStyle = .round
        NSColor.white.withAlphaComponent(alpha).setStroke()
        p.stroke()
    }
    stroke([(34, 50), (82, 50)], alpha: 1)
    stroke([(70, 36), (84, 50), (70, 64)], alpha: 1)
    stroke([(94, 78), (46, 78)], alpha: 0.72)
    stroke([(58, 64), (44, 78), (58, 92)], alpha: 0.72)
    NSGraphicsContext.restoreGraphicsState()
    return rep.representation(using: .png, properties: [:])!
}

for base in [16, 32, 128, 256, 512] {
    try render(base).write(to: URL(fileURLWithPath: "\(out)/icon_\(base)x\(base).png"))
    try render(base * 2).write(to: URL(fileURLWithPath: "\(out)/icon_\(base)x\(base)@2x.png"))
}
