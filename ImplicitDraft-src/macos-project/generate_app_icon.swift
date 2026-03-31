import AppKit

let outputDir = URL(fileURLWithPath: "/Users/ryancummins/Developer/ImplicitDraft/ImplicitDraft-src/macos-project/implicit/implicit/Assets.xcassets/AppIcon.appiconset", isDirectory: true)
let sourceOutput = URL(fileURLWithPath: "/Users/ryancummins/Developer/ImplicitDraft/ImplicitDraft-tmp/icon-work/implicit-icon-source-1024.png")

func color(_ hex: UInt32, alpha: CGFloat = 1.0) -> NSColor {
    NSColor(
        srgbRed: CGFloat((hex >> 16) & 0xff) / 255.0,
        green: CGFloat((hex >> 8) & 0xff) / 255.0,
        blue: CGFloat(hex & 0xff) / 255.0,
        alpha: alpha
    )
}

let canvasSize = NSSize(width: 1024, height: 1024)
let image = NSImage(size: canvasSize)
image.lockFocus()

let canvas = NSRect(origin: .zero, size: canvasSize)
NSColor.clear.setFill()
canvas.fill()

let plateRect = canvas.insetBy(dx: 72, dy: 72)
let plate = NSBezierPath(roundedRect: plateRect, xRadius: 220, yRadius: 220)
color(0x000000).setFill()
plate.fill()

let contentRect = NSRect(x: 226, y: 158, width: 572, height: 708)

let accentRect = NSRect(
    x: contentRect.minX - 6,
    y: contentRect.minY + 54,
    width: 58,
    height: contentRect.height - 108
)
let accent = NSBezierPath(roundedRect: accentRect, xRadius: 29, yRadius: 29)
color(0x2ea043).setFill()
accent.fill()

func line(_ x: CGFloat, _ y: CGFloat, _ width: CGFloat, _ height: CGFloat, _ fill: NSColor) {
    let rect = NSRect(x: x, y: y, width: width, height: height)
    let path = NSBezierPath(roundedRect: rect, xRadius: height / 2, yRadius: height / 2)
    fill.setFill()
    path.fill()
}

line(contentRect.minX + 110, contentRect.maxY - 204, 290, 38, color(0xe6edf3))
line(contentRect.minX + 110, contentRect.maxY - 286, 382, 30, color(0xa4acb8))
line(contentRect.minX + 110, contentRect.maxY - 356, 326, 30, color(0xa4acb8))
line(contentRect.minX + 110, contentRect.maxY - 468, 236, 52, color(0x58a6ff))
line(contentRect.minX + 110, contentRect.maxY - 554, 340, 30, color(0xa4acb8))
line(contentRect.minX + 110, contentRect.maxY - 624, 274, 30, color(0xa4acb8))

let dotRect = NSRect(x: contentRect.maxX - 128, y: contentRect.minY + 84, width: 34, height: 34)
let dot = NSBezierPath(ovalIn: dotRect)
color(0xe6edf3, alpha: 0.9).setFill()
dot.fill()

image.unlockFocus()

guard let tiff = image.tiffRepresentation,
      let bitmap = NSBitmapImageRep(data: tiff),
      let png = bitmap.representation(using: .png, properties: [:]) else {
    fatalError("failed to create png")
}

try FileManager.default.createDirectory(at: sourceOutput.deletingLastPathComponent(), withIntermediateDirectories: true)
try png.write(to: sourceOutput)

let specs: [(Int, String)] = [
    (16, "icon_16x16.png"),
    (32, "icon_16x16@2x.png"),
    (32, "icon_32x32.png"),
    (64, "icon_32x32@2x.png"),
    (128, "icon_128x128.png"),
    (256, "icon_128x128@2x.png"),
    (256, "icon_256x256.png"),
    (512, "icon_256x256@2x.png"),
    (512, "icon_512x512.png"),
    (1024, "icon_512x512@2x.png"),
]

for (size, name) in specs {
    guard let rep = NSBitmapImageRep(data: png) else { continue }
    rep.size = NSSize(width: size, height: size)
    let resized = NSImage(size: NSSize(width: size, height: size))
    resized.lockFocus()
    NSGraphicsContext.current?.imageInterpolation = .high
    NSImage(data: png)?.draw(in: NSRect(x: 0, y: 0, width: size, height: size))
    resized.unlockFocus()
    guard let resizedTiff = resized.tiffRepresentation,
          let resizedRep = NSBitmapImageRep(data: resizedTiff),
          let resizedPng = resizedRep.representation(using: .png, properties: [:]) else {
        continue
    }
    try resizedPng.write(to: outputDir.appendingPathComponent(name))
}

print("generated app icon set at \(outputDir.path)")
