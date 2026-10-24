// Cuts the squircle out of source.png (white corners become transparent) and writes the shipped icons:
//   neko-1024.png  macOS master: artwork on Apple's 824/1024 grid
//   neko-512.png   Linux hicolor icon: artwork nearly full-bleed
// Usage: swift packaging/icon/prepare.swift packaging/icon/source.png packaging/icon
import AppKit

let args = CommandLine.arguments
let src = NSBitmapImageRep(data: try! Data(contentsOf: URL(fileURLWithPath: args[1])))!
let outDir = args[2]
let w = src.pixelsWide, h = src.pixelsHigh

var px = [UInt8](repeating: 0, count: w * h * 4)
for y in 0..<h { for x in 0..<w {
    let c = src.colorAt(x: x, y: y)!.usingColorSpace(.sRGB)!
    let i = (y * w + x) * 4
    px[i] = UInt8(c.redComponent * 255); px[i + 1] = UInt8(c.greenComponent * 255)
    px[i + 2] = UInt8(c.blueComponent * 255); px[i + 3] = 255
} }

// Background is near-white; the squircle's lightest edge is far bluer (R about 134).
func isBg(_ i: Int) -> Bool { px[i * 4] >= 195 && px[i * 4 + 1] >= 210 }
var bg = [Bool](repeating: false, count: w * h)
var stack: [Int] = []
func seed(_ x: Int, _ y: Int) { let i = y * w + x; if !bg[i], isBg(i) { bg[i] = true; stack.append(i) } }
for x in 0..<w { seed(x, 0); seed(x, h - 1) }
for y in 0..<h { seed(0, y); seed(w - 1, y) }
while let i = stack.popLast() {
    let x = i % w, y = i / w
    if x > 0 { seed(x - 1, y) }; if x < w - 1 { seed(x + 1, y) }
    if y > 0 { seed(x, y - 1) }; if y < h - 1 { seed(x, y + 1) }
}
// Erode a few pixels to drop the pale fringe left by antialiasing and the shadow.
for _ in 0..<3 {
    var next = bg
    for y in 1..<(h - 1) { for x in 1..<(w - 1) where !bg[y * w + x] {
        if bg[y * w + x - 1] || bg[y * w + x + 1] || bg[(y - 1) * w + x] || bg[(y + 1) * w + x] { next[y * w + x] = true }
    } }
    bg = next
}
var minX = w, minY = h, maxX = 0, maxY = 0
for y in 0..<h { for x in 0..<w {
    let i = y * w + x
    if bg[i] { px[i * 4 + 3] = 0; px[i * 4] = 0; px[i * 4 + 1] = 0; px[i * 4 + 2] = 0 }
    else { minX = min(minX, x); maxX = max(maxX, x); minY = min(minY, y); maxY = max(maxY, y) }
} }

let cs = CGColorSpace(name: CGColorSpace.sRGB)!
let cut = CGContext(data: &px, width: w, height: h, bitsPerComponent: 8, bytesPerRow: w * 4, space: cs,
                    bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!
// px holds straight colour with alpha 0 or 255, which is valid premultiplied data too.
let art = cut.makeImage()!.cropping(to: CGRect(x: minX, y: minY, width: maxX - minX + 1, height: maxY - minY + 1))!

func write(_ name: String, canvas: Int, artSize: Int) {
    let ctx = CGContext(data: nil, width: canvas, height: canvas, bitsPerComponent: 8, bytesPerRow: 0, space: cs,
                        bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!
    ctx.interpolationQuality = .high
    let o = CGFloat(canvas - artSize) / 2
    ctx.draw(art, in: CGRect(x: o, y: o, width: CGFloat(artSize), height: CGFloat(artSize)))
    let rep = NSBitmapImageRep(cgImage: ctx.makeImage()!)
    try! rep.representation(using: .png, properties: [:])!.write(to: URL(fileURLWithPath: "\(outDir)/\(name)"))
}
write("neko-1024.png", canvas: 1024, artSize: 824)
write("neko-512.png", canvas: 512, artSize: 488)
print("art bbox \(minX),\(minY) – \(maxX),\(maxY)")
