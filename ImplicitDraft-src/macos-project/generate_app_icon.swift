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
color(0x000000).setFill()
canvas.fill()

let plateRect = canvas.insetBy(dx: 72, dy: 72)
let plate = NSBezierPath(roundedRect: plateRect, xRadius: 220, yRadius: 220)
color(0x000000).setFill()
plate.fill()

let pageRect = NSRect(x: 226, y: 158, width: 572, height: 708)
let page = NSBezierPath(roundedRect: pageRect, xRadius: 102, yRadius: 102)
color(0x090c10).setFill()
page.fill()

let foldPath = NSBezierPath()
foldPath.move(to: NSPoint(x: pageRect.maxX - 146, y: pageRect.maxY))
foldPath.line(to: NSPoint(x: pageRect.maxX, y: pageRect.maxY - 146))
foldPath.line(to: NSPoint(x: pageRect.maxX, y: pageRect.maxY))
foldPath.close()
color(0x58a6ff, alpha: 0.9).setFill()
foldPath.fill()

let foldShadow = NSBezierPath()
foldShadow.move(to: NSPoint(x: pageRect.maxX - 116, y: pageRect.maxY))
foldShadow.line(to: NSPoint(x: pageRect.maxX, y: pageRect.maxY - 116))
foldShadow.line(to: NSPoint(x: pageRect.maxX, y: pageRect.maxY - 28))
foldShadow.line(to: NSPoint(x: pageRect.maxX - 28, y: pageRect.maxY))
foldShadow.close()
color(0x0d1117, alpha: 0.24).setFill()
foldShadow.fill()

func line(_ x: CGFloat, _ y: CGFloat, _ width: CGFloat, _ height: CGFloat, _ fill: NSColor) {
    let rect = NSRect(x: x, y: y, width: width, height: height)
    let path = NSBezierPath(roundedRect: rect, xRadius: height / 2, yRadius: height / 2)
    fill.setFill()
    path.fill()
}

line(pageRect.minX + 120, pageRect.maxY - 208, 280, 36, color(0xe6edf3))
line(pageRect.minX + 120, pageRect.maxY - 286, 370, 30, color(0xa4acb8))
line(pageRect.minX + 120, pageRect.maxY - 356, 318, 30, color(0xa4acb8))
line(pageRect.minX + 120, pageRect.maxY - 462, 230, 50, color(0x58a6ff))
line(pageRect.minX + 120, pageRect.maxY - 544, 332, 30, color(0xa4acb8))
line(pageRect.minX + 120, pageRect.maxY - 614, 268, 30, color(0xa4acb8))

let dotRect = NSRect(x: pageRect.maxX - 130, y: pageRect.minY + 82, width: 34, height: 34)
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
