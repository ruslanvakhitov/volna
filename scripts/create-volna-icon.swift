import AppKit
import Foundation
let root = URL(fileURLWithPath: CommandLine.arguments[1])
func wave(_ rect: CGRect, color: NSColor, width: CGFloat) {
    color.setStroke()
    for offset in [-0.18,0.18] {
        let p = NSBezierPath()
        func pt(_ x: Double, _ y: Double) -> NSPoint { NSPoint(x: rect.minX + x * rect.width, y: rect.minY + (y + offset) * rect.height) }
        p.move(to:pt(0.0,0.5)); p.curve(to:pt(0.5,0.5),controlPoint1:pt(0.2,0.98),controlPoint2:pt(0.3,0.02))
        p.curve(to:pt(1.0,0.5),controlPoint1:pt(0.7,0.98),controlPoint2:pt(0.8,0.02))
        p.lineWidth=width;p.lineCapStyle = .round;p.stroke()
    }
}
func render(_ size: Int, menu: Bool, recording: Bool = false) -> Data {
    let bitmap=NSBitmapImageRep(bitmapDataPlanes:nil,pixelsWide:size,pixelsHigh:size,bitsPerSample:8,samplesPerPixel:4,hasAlpha:true,isPlanar:false,colorSpaceName:.deviceRGB,bytesPerRow:0,bitsPerPixel:0)!
    NSGraphicsContext.saveGraphicsState()
    NSGraphicsContext.current=NSGraphicsContext(bitmapImageRep:bitmap)!
    let n=CGFloat(size)
    if !menu {
        let tile=NSBezierPath(roundedRect:NSRect(x:n*0.08,y:n*0.08,width:n*0.84,height:n*0.84),xRadius:n*0.20,yRadius:n*0.20)
        let shadow=NSShadow();shadow.shadowOffset=NSSize(width:0,height:-n*0.014);shadow.shadowBlurRadius=n*0.035;shadow.shadowColor=NSColor.black.withAlphaComponent(0.22);shadow.set()
        NSGradient(starting:NSColor(calibratedRed:0.035,green:0.15,blue:0.19,alpha:1),ending:NSColor(calibratedRed:0.06,green:0.38,blue:0.40,alpha:1))!.draw(in:tile,angle:60)
        NSShadow().set();NSColor.white.withAlphaComponent(0.12).setStroke();tile.lineWidth=n*0.002;tile.stroke()
    }
    wave(CGRect(x:n*(menu ? 0.14 : 0.25),y:n*(menu ? 0.25 : 0.31),width:n*(menu ? 0.72 : 0.50),height:n*(menu ? 0.5 : 0.38)),color:menu ? .black : NSColor(calibratedRed:0.65,green:0.98,blue:0.87,alpha:1),width:n*(menu ? 0.075 : 0.048))
    if recording { NSColor.black.setFill();NSBezierPath(ovalIn:NSRect(x:n*0.76,y:n*0.73,width:n*0.18,height:n*0.18)).fill() }
    NSGraphicsContext.restoreGraphicsState()
    return bitmap.representation(using:.png,properties:[:])!
}
let iconset=root.appendingPathComponent("assets/volna/Volna.iconset")
try FileManager.default.createDirectory(at:iconset,withIntermediateDirectories:true)
for size in [16,32,128,256,512] {
    try render(size,menu:false).write(to:iconset.appendingPathComponent("icon_\(size)x\(size).png"))
    try render(size*2,menu:false).write(to:iconset.appendingPathComponent("icon_\(size)x\(size)@2x.png"))
}
try render(1024,menu:false).write(to:root.appendingPathComponent("assets/volna/icon.png"))
try render(36,menu:true).write(to:root.appendingPathComponent("assets/icons/menubar-idle@2x.png"))
try render(36,menu:true,recording:true).write(to:root.appendingPathComponent("assets/icons/menubar-recording@2x.png"))
