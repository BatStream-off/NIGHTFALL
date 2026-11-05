// Integrated title bar (prototype M-0005): drives the real NIGHTFALL window
// with synthetic mouse events on macOS (lock screen) and checks that the
// frameless window moves when its top band is dragged, and that a double
// click on the band zooms it. Prints "NFTITLEBAR ..." lines.
import CoreGraphics
import Foundation

func say(_ s: String) { print("NFTITLEBAR \(s)"); fflush(stdout) }
var failed = false
func check(_ name: String, _ ok: Bool, _ detail: String) {
    say((ok ? "OK   " : "ECHEC ") + name + " " + detail)
    if !ok { failed = true }
}

func nightfallWindow() -> CGRect? {
    guard let list = CGWindowListCopyWindowInfo([.optionOnScreenOnly, .excludeDesktopElements], kCGNullWindowID) as? [[String: Any]] else { return nil }
    for w in list {
        let owner = (w[kCGWindowOwnerName as String] as? String ?? "").lowercased()
        let layer = w[kCGWindowLayer as String] as? Int ?? -1
        guard owner.contains("nightfall"), layer == 0, let b = w[kCGWindowBounds as String] as? [String: Any] else { continue }
        let r = CGRect(dictionaryRepresentation: b as CFDictionary) ?? .zero
        if r.width > 300 && r.height > 200 { return r }
    }
    return nil
}

func post(_ type: CGEventType, _ p: CGPoint, clicks: Int64 = 1) {
    let e = CGEvent(mouseEventSource: nil, mouseType: type, mouseCursorPosition: p, mouseButton: .left)
    e?.setIntegerValueField(.mouseEventClickState, value: clicks)
    e?.post(tap: .cghidEventTap)
    usleep(30_000)
}

func drag(from a: CGPoint, to b: CGPoint) {
    post(.mouseMoved, a); usleep(150_000)
    post(.leftMouseDown, a); usleep(150_000)
    for i in 1...12 {
        let t = CGFloat(i) / 12
        post(.leftMouseDragged, CGPoint(x: a.x + (b.x - a.x) * t, y: a.y + (b.y - a.y) * t))
    }
    usleep(150_000); post(.leftMouseUp, b); usleep(800_000)
}

say("accès accessibilité (AXIsProcessTrusted non vérifiable ici) ; événements postés sur cghidEventTap")
guard let r0 = nightfallWindow() else { say("ECHEC fenêtre introuvable"); exit(1) }
say("position initiale \(r0)")
let band = CGPoint(x: r0.midX, y: r0.minY + 18)
drag(from: band, to: CGPoint(x: band.x - 140, y: band.y + 60))
let r1 = nightfallWindow() ?? .zero
check("déplacement par glisser", r1.minX - r0.minX <= -100 && r1.minY - r0.minY >= 40, "\(r1)")

// Double click on the band: macOS "zoom" (fills the screen, not fullscreen).
let p = CGPoint(x: r1.midX, y: r1.minY + 18)
post(.mouseMoved, p); usleep(150_000)
post(.leftMouseDown, p, clicks: 1); post(.leftMouseUp, p, clicks: 1)
post(.leftMouseDown, p, clicks: 2); post(.leftMouseUp, p, clicks: 2)
sleep(2)
let r2 = nightfallWindow() ?? .zero
check("double-clic : zoom", r2.width > r1.width + 50 || r2.height > r1.height + 50, "\(r2)")
exit(failed ? 1 : 0)
