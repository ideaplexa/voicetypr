import Cocoa
// Usage: hidkey <keycode> [ctrl] [alt] [cmd] [shift] — posts key down/up at the HID tap.
let args = CommandLine.arguments.dropFirst()
guard let code = args.first.flatMap({ CGKeyCode($0) }) else { exit(2) }
var flags: CGEventFlags = []
if args.contains("ctrl") { flags.insert(.maskControl) }
if args.contains("alt") { flags.insert(.maskAlternate) }
if args.contains("cmd") { flags.insert(.maskCommand) }
if args.contains("shift") { flags.insert(.maskShift) }
let src = CGEventSource(stateID: .hidSystemState)
let mods: [(CGEventFlags, CGKeyCode)] = [(.maskControl, 59), (.maskAlternate, 58), (.maskCommand, 55), (.maskShift, 56)]
var held: CGEventFlags = []
for (f, k) in mods where flags.contains(f) { held.insert(f); let e = CGEvent(keyboardEventSource: src, virtualKey: k, keyDown: true)!; e.flags = held; e.post(tap: .cghidEventTap); usleep(20000) }
let down = CGEvent(keyboardEventSource: src, virtualKey: code, keyDown: true)!; down.flags = flags; down.post(tap: .cghidEventTap); usleep(40000)
let up = CGEvent(keyboardEventSource: src, virtualKey: code, keyDown: false)!; up.flags = flags; up.post(tap: .cghidEventTap); usleep(20000)
for (f, k) in mods.reversed() where flags.contains(f) { held.remove(f); let e = CGEvent(keyboardEventSource: src, virtualKey: k, keyDown: false)!; e.flags = held; e.post(tap: .cghidEventTap); usleep(20000) }
