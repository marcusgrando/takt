import SwiftUI
import AppKit
import Carbon.HIToolbox

/// A "press to record" keyboard shortcut field.
/// Click to start recording, press any key combo, and it captures modifiers + key.
struct ShortcutRecorderView: NSViewRepresentable {
    @Binding var combo: KeyCombo
    var onChanged: () -> Void

    func makeNSView(context: Context) -> ShortcutField {
        let field = ShortcutField()
        field.delegate = context.coordinator
        field.alignment = .center
        field.font = .systemFont(ofSize: 11)
        field.isEditable = false
        field.isSelectable = false
        field.isBordered = true
        field.isBezeled = true
        field.bezelStyle = .roundedBezel
        field.focusRingType = .exterior
        field.placeholderString = "Click to record"
        field.stringValue = combo.displayString
        field.target = context.coordinator
        field.action = #selector(Coordinator.fieldClicked(_:))
        return field
    }

    func updateNSView(_ nsView: ShortcutField, context: Context) {
        if !nsView.isRecording {
            nsView.stringValue = combo.displayString
        }
    }

    func makeCoordinator() -> Coordinator {
        Coordinator(self)
    }

    class Coordinator: NSObject, NSTextFieldDelegate {
        let parent: ShortcutRecorderView
        var monitor: Any?

        init(_ parent: ShortcutRecorderView) {
            self.parent = parent
        }

        @objc func fieldClicked(_ sender: ShortcutField) {
            guard !sender.isRecording else { return }
            sender.isRecording = true
            sender.stringValue = "Press shortcut..."
            sender.window?.makeFirstResponder(sender)

            // Local key-down monitor captures the next keystroke
            monitor = NSEvent.addLocalMonitorForEvents(matching: .keyDown) { [weak self] event in
                guard let self, let field = sender as ShortcutField? else { return event }

                // Escape cancels recording
                if event.keyCode == UInt16(kVK_Escape) {
                    self.stopRecording(field)
                    return nil
                }

                let modifiers = self.extractModifiers(event.modifierFlags)
                let key = self.keyName(for: event)

                // Only accept if there's an actual key (not just modifier press)
                if !key.isEmpty {
                    self.parent.combo = KeyCombo(modifiers: modifiers, key: key)
                    self.parent.onChanged()
                    self.stopRecording(field)
                }
                return nil // consume the event
            }
        }

        private func stopRecording(_ field: ShortcutField) {
            field.isRecording = false
            field.stringValue = parent.combo.displayString
            if let monitor {
                NSEvent.removeMonitor(monitor)
            }
            monitor = nil
        }

        private func extractModifiers(_ flags: NSEvent.ModifierFlags) -> [Modifier] {
            var mods: [Modifier] = []
            if flags.contains(.command) { mods.append(.cmd) }
            if flags.contains(.shift) { mods.append(.shift) }
            if flags.contains(.option) { mods.append(.opt) }
            if flags.contains(.control) { mods.append(.ctrl) }
            return mods
        }

        private func keyName(for event: NSEvent) -> String {
            // Try characters first for printable keys
            if let chars = event.charactersIgnoringModifiers?.lowercased(), !chars.isEmpty {
                let c = chars.first!
                // Filter out non-printable control characters
                if c.isLetter || c.isNumber || c.isPunctuation || c.isSymbol || c == " " {
                    return specialKeyName(chars) ?? chars
                }
            }

            // Fall back to keyCode for non-printable keys
            return keyCodeName(event.keyCode) ?? ""
        }

        private func specialKeyName(_ chars: String) -> String? {
            switch chars {
            case " ": return "Space"
            case "\t": return "Tab"
            case "\r": return "Return"
            default: return nil
            }
        }

        private func keyCodeName(_ keyCode: UInt16) -> String? {
            switch Int(keyCode) {
            case kVK_Return: return "Return"
            case kVK_Tab: return "Tab"
            case kVK_Space: return "Space"
            case kVK_Delete: return "Delete"
            case kVK_ForwardDelete: return "ForwardDelete"
            case kVK_LeftArrow: return "Left"
            case kVK_RightArrow: return "Right"
            case kVK_UpArrow: return "Up"
            case kVK_DownArrow: return "Down"
            case kVK_Home: return "Home"
            case kVK_End: return "End"
            case kVK_PageUp: return "PageUp"
            case kVK_PageDown: return "PageDown"
            case kVK_F1: return "F1"
            case kVK_F2: return "F2"
            case kVK_F3: return "F3"
            case kVK_F4: return "F4"
            case kVK_F5: return "F5"
            case kVK_F6: return "F6"
            case kVK_F7: return "F7"
            case kVK_F8: return "F8"
            case kVK_F9: return "F9"
            case kVK_F10: return "F10"
            case kVK_F11: return "F11"
            case kVK_F12: return "F12"
            default: return nil
            }
        }

        deinit {
            if let monitor {
                NSEvent.removeMonitor(monitor)
            }
        }
    }
}

/// Custom NSTextField that tracks recording state and accepts first mouse click.
class ShortcutField: NSTextField {
    var isRecording = false

    override func mouseDown(with event: NSEvent) {
        // Trigger the action (fieldClicked) on click
        sendAction(action, to: target)
    }

    override var acceptsFirstResponder: Bool { true }

    override func acceptsFirstMouse(for event: NSEvent?) -> Bool { true }
}

// MARK: - Display helpers

extension KeyCombo {
    var displayString: String {
        if key.isEmpty && modifiers.isEmpty { return "" }
        let modStr = modifiers.map(\.symbol).joined()
        return modStr + key.uppercased()
    }
}

extension Modifier {
    var symbol: String {
        switch self {
        case .cmd: return "⌘"
        case .shift: return "⇧"
        case .opt: return "⌥"
        case .ctrl: return "⌃"
        }
    }
}
