import SwiftUI
import UniformTypeIdentifiers

struct ActionBuilderView: View {
    @Binding var action: Action
    var browsers: [String]
    var fileApps: [String]
    var onFilePathChanged: () -> Void

    @State private var showFilePicker = false
    @State private var showAppPicker = false

    private var actionType: ActionTypeTag {
        switch action {
        case .openUrl: return .openUrl
        case .openFile: return .openFile
        case .openApp: return .openApp
        case .runCommand: return .runCommand
        case .notify: return .notify
        case .webhook: return .webhook
        case .settings: return .settings
        }
    }

    var body: some View {
        VStack(spacing: 16) {
            // Type selector
            Picker("Type", selection: Binding(
                get: { actionType },
                set: { handleTypeChange($0) }
            )) {
                ForEach(ActionTypeTag.allCases) { tag in
                    Text(tag.label).tag(tag)
                }
            }
            .pickerStyle(.segmented)
            .labelsHidden()

            // Type-specific fields
            switch action {
            case .openFile:
                openFileFields
            case .openUrl:
                openUrlFields
            case .openApp:
                openAppFields
            case .runCommand:
                runCommandFields
            case .notify:
                notifyFields
            case .webhook:
                webhookFields
            case .settings:
                settingsFields
            }

            // Post-shortcuts for file/url/app types
            if hasPostShortcuts {
                postShortcutsSection
            }
        }
    }

    // MARK: - OpenFile

    @ViewBuilder
    private var openFileFields: some View {
        if case .openFile(let path, let app, let shortcuts, let delay) = action {
            VStack(alignment: .leading, spacing: 12) {
                VStack(alignment: .leading, spacing: 4) {
                    Text("File path")
                        .font(.system(size: 12, weight: .medium))
                    HStack {
                        TextField("/path/to/file", text: Binding(
                            get: { path },
                            set: {
                                action = .openFile(path: $0, app: app, postShortcuts: shortcuts, shortcutDelaySecs: delay)
                                onFilePathChanged()
                            }
                        ))
                        .textFieldStyle(.roundedBorder)
                        .font(.system(size: 12, design: .monospaced))
                        Button {
                            showFilePicker = true
                        } label: {
                            Image(systemName: "folder")
                        }
                        .fileImporter(isPresented: $showFilePicker, allowedContentTypes: [.item]) { result in
                            if let url = try? result.get() {
                                action = .openFile(path: url.path, app: app, postShortcuts: shortcuts, shortcutDelaySecs: delay)
                                onFilePathChanged()
                            }
                        }
                    }
                }

                VStack(alignment: .leading, spacing: 4) {
                    HStack(spacing: 4) {
                        Text("Open with")
                            .font(.system(size: 12, weight: .medium))
                        Text("(optional)")
                            .font(.system(size: 12))
                            .foregroundStyle(.secondary)
                    }
                    HStack {
                        Picker("", selection: Binding(
                            get: { app ?? "__default__" },
                            set: {
                                let newApp: String? = $0 == "__default__" ? nil : $0
                                action = .openFile(path: path, app: newApp, postShortcuts: shortcuts, shortcutDelaySecs: delay)
                            }
                        )) {
                            Text("Default app").tag("__default__")
                            ForEach(fileApps, id: \.self) { a in
                                Text(a).tag(a)
                            }
                        }
                        .labelsHidden()

                        Button {
                            showAppPicker = true
                        } label: {
                            Image(systemName: "folder")
                        }
                        .fileImporter(isPresented: $showAppPicker, allowedContentTypes: [.application]) { result in
                            if let url = try? result.get() {
                                let name = url.deletingPathExtension().lastPathComponent
                                action = .openFile(path: path, app: name, postShortcuts: shortcuts, shortcutDelaySecs: delay)
                            }
                        }
                    }
                }
            }
        }
    }

    // MARK: - OpenUrl

    @ViewBuilder
    private var openUrlFields: some View {
        if case .openUrl(let url, let browser, let shortcuts, let delay) = action {
            VStack(alignment: .leading, spacing: 12) {
                VStack(alignment: .leading, spacing: 4) {
                    Text("URL")
                        .font(.system(size: 12, weight: .medium))
                    TextField("https://example.com", text: Binding(
                        get: { url },
                        set: { action = .openUrl(url: $0, browser: browser, postShortcuts: shortcuts, shortcutDelaySecs: delay) }
                    ))
                    .textFieldStyle(.roundedBorder)
                }

                VStack(alignment: .leading, spacing: 4) {
                    HStack(spacing: 4) {
                        Text("Browser")
                            .font(.system(size: 12, weight: .medium))
                        Text("(optional)")
                            .font(.system(size: 12))
                            .foregroundStyle(.secondary)
                    }
                    Picker("", selection: Binding(
                        get: { browser ?? "__default__" },
                        set: {
                            let newBrowser: String? = $0 == "__default__" ? nil : $0
                            action = .openUrl(url: url, browser: newBrowser, postShortcuts: shortcuts, shortcutDelaySecs: delay)
                        }
                    )) {
                        Text("Default browser").tag("__default__")
                        ForEach(browsers, id: \.self) { b in
                            Text(b).tag(b)
                        }
                    }
                    .labelsHidden()
                }
            }
        }
    }

    // MARK: - OpenApp

    @ViewBuilder
    private var openAppFields: some View {
        if case .openApp(let appPath, let shortcuts, let delay) = action {
            VStack(alignment: .leading, spacing: 4) {
                Text("Application")
                    .font(.system(size: 12, weight: .medium))
                HStack {
                    TextField("/Applications/App.app", text: Binding(
                        get: { appPath },
                        set: { action = .openApp(appPath: $0, postShortcuts: shortcuts, shortcutDelaySecs: delay) }
                    ))
                    .textFieldStyle(.roundedBorder)
                    .font(.system(size: 12, design: .monospaced))
                    Button {
                        showAppPicker = true
                    } label: {
                        Image(systemName: "folder")
                    }
                    .fileImporter(isPresented: $showAppPicker, allowedContentTypes: [.application]) { result in
                        if let url = try? result.get() {
                            action = .openApp(appPath: url.path, postShortcuts: shortcuts, shortcutDelaySecs: delay)
                        }
                    }
                }
            }
        }
    }

    // MARK: - RunCommand

    @ViewBuilder
    private var runCommandFields: some View {
        if case .runCommand(let command, let args, let shell) = action {
            VStack(alignment: .leading, spacing: 12) {
                VStack(alignment: .leading, spacing: 4) {
                    Text("Shell")
                        .font(.system(size: 12, weight: .medium))
                    Picker("", selection: Binding(
                        get: { shell },
                        set: { action = .runCommand(command: command, args: args, shell: $0) }
                    )) {
                        Text("sh").tag(Shell.sh)
                        Text("bash").tag(Shell.bash)
                        Text("zsh").tag(Shell.zsh)
                        Text("python").tag(Shell.python)
                        Text("AppleScript").tag(Shell.appleScript)
                    }
                    .labelsHidden()
                }

                VStack(alignment: .leading, spacing: 4) {
                    Text("Command")
                        .font(.system(size: 12, weight: .medium))
                    TextField("echo hello", text: Binding(
                        get: { command },
                        set: { action = .runCommand(command: $0, args: args, shell: shell) }
                    ))
                    .textFieldStyle(.roundedBorder)
                    .font(.system(size: 12, design: .monospaced))
                }

                VStack(alignment: .leading, spacing: 4) {
                    HStack(spacing: 4) {
                        Text("Arguments")
                            .font(.system(size: 12, weight: .medium))
                        Text("(one per line)")
                            .font(.system(size: 12))
                            .foregroundStyle(.secondary)
                    }
                    TextEditor(text: Binding(
                        get: { args.joined(separator: "\n") },
                        set: {
                            let newArgs = $0.split(separator: "\n", omittingEmptySubsequences: false)
                                .map { $0.trimmingCharacters(in: .whitespaces) }
                                .filter { !$0.isEmpty }
                            action = .runCommand(command: command, args: newArgs, shell: shell)
                        }
                    ))
                    .font(.system(size: 12, design: .monospaced))
                    .frame(minHeight: 60)
                    .scrollContentBackground(.hidden)
                    .padding(4)
                    .background(Color(nsColor: .textBackgroundColor))
                    .clipShape(RoundedRectangle(cornerRadius: 6))
                    .overlay(
                        RoundedRectangle(cornerRadius: 6)
                            .stroke(Color(nsColor: .separatorColor), lineWidth: 1)
                    )
                }
            }
        }
    }

    // MARK: - Notify

    @ViewBuilder
    private var notifyFields: some View {
        if case .notify(let title, let body, let sound) = action {
            VStack(alignment: .leading, spacing: 12) {
                VStack(alignment: .leading, spacing: 4) {
                    Text("Title")
                        .font(.system(size: 12, weight: .medium))
                    TextField("Notification title", text: Binding(
                        get: { title },
                        set: { action = .notify(title: $0, body: body, sound: sound) }
                    ))
                    .textFieldStyle(.roundedBorder)
                }

                VStack(alignment: .leading, spacing: 4) {
                    Text("Body")
                        .font(.system(size: 12, weight: .medium))
                    TextEditor(text: Binding(
                        get: { body },
                        set: { action = .notify(title: title, body: $0, sound: sound) }
                    ))
                    .font(.system(size: 13))
                    .frame(minHeight: 60)
                    .scrollContentBackground(.hidden)
                    .padding(4)
                    .background(Color(nsColor: .textBackgroundColor))
                    .clipShape(RoundedRectangle(cornerRadius: 6))
                    .overlay(
                        RoundedRectangle(cornerRadius: 6)
                            .stroke(Color(nsColor: .separatorColor), lineWidth: 1)
                    )
                }

                Toggle("Play sound", isOn: Binding(
                    get: { sound },
                    set: { action = .notify(title: title, body: body, sound: $0) }
                ))
                .font(.system(size: 13))
            }
        }
    }

    // MARK: - Webhook

    @ViewBuilder
    private var webhookFields: some View {
        if case .webhook(let url, let method, let headers, let body) = action {
            VStack(alignment: .leading, spacing: 12) {
                HStack(spacing: 8) {
                    VStack(alignment: .leading, spacing: 4) {
                        Text("Method")
                            .font(.system(size: 12, weight: .medium))
                        Picker("", selection: Binding(
                            get: { method },
                            set: { action = .webhook(url: url, method: $0, headers: headers, body: body) }
                        )) {
                            Text("GET").tag(HttpMethod.get)
                            Text("POST").tag(HttpMethod.post)
                            Text("PUT").tag(HttpMethod.put)
                            Text("PATCH").tag(HttpMethod.patch)
                            Text("DELETE").tag(HttpMethod.delete)
                        }
                        .labelsHidden()
                        .frame(width: 90)
                    }
                    VStack(alignment: .leading, spacing: 4) {
                        Text("URL")
                            .font(.system(size: 12, weight: .medium))
                        TextField("https://api.example.com/hook", text: Binding(
                            get: { url },
                            set: { action = .webhook(url: $0, method: method, headers: headers, body: body) }
                        ))
                        .textFieldStyle(.roundedBorder)
                    }
                }

                VStack(alignment: .leading, spacing: 4) {
                    HStack(spacing: 4) {
                        Text("Headers")
                            .font(.system(size: 12, weight: .medium))
                        Text("(key=value per line)")
                            .font(.system(size: 12))
                            .foregroundStyle(.secondary)
                    }
                    TextEditor(text: Binding(
                        get: { headersToText(headers) },
                        set: { action = .webhook(url: url, method: method, headers: textToHeaders($0), body: body) }
                    ))
                    .font(.system(size: 12, design: .monospaced))
                    .frame(minHeight: 60)
                    .scrollContentBackground(.hidden)
                    .padding(4)
                    .background(Color(nsColor: .textBackgroundColor))
                    .clipShape(RoundedRectangle(cornerRadius: 6))
                    .overlay(
                        RoundedRectangle(cornerRadius: 6)
                            .stroke(Color(nsColor: .separatorColor), lineWidth: 1)
                    )
                }

                VStack(alignment: .leading, spacing: 4) {
                    HStack(spacing: 4) {
                        Text("Body")
                            .font(.system(size: 12, weight: .medium))
                        Text("(optional)")
                            .font(.system(size: 12))
                            .foregroundStyle(.secondary)
                    }
                    TextEditor(text: Binding(
                        get: { body ?? "" },
                        set: { action = .webhook(url: url, method: method, headers: headers, body: $0.isEmpty ? nil : $0) }
                    ))
                    .font(.system(size: 12, design: .monospaced))
                    .frame(minHeight: 60)
                    .scrollContentBackground(.hidden)
                    .padding(4)
                    .background(Color(nsColor: .textBackgroundColor))
                    .clipShape(RoundedRectangle(cornerRadius: 6))
                    .overlay(
                        RoundedRectangle(cornerRadius: 6)
                            .stroke(Color(nsColor: .separatorColor), lineWidth: 1)
                    )
                }
            }
        }
    }

    // MARK: - Settings

    private static let settingsPresets: [(label: String, url: String)] = [
        ("General", "x-apple.systempreferences:com.apple.settings.General"),
        ("Accessibility", "x-apple.systempreferences:com.apple.Accessibility-Settings.extension"),
        ("Battery", "x-apple.systempreferences:com.apple.settings.Battery"),
        ("Bluetooth", "x-apple.systempreferences:com.apple.BluetoothSettings"),
        ("Displays", "x-apple.systempreferences:com.apple.Displays-Settings.extension"),
        ("Keyboard", "x-apple.systempreferences:com.apple.Keyboard-Settings.extension"),
        ("Network", "x-apple.systempreferences:com.apple.Network-Settings.extension"),
        ("Notifications", "x-apple.systempreferences:com.apple.Notifications-Settings.extension"),
        ("Privacy & Security", "x-apple.systempreferences:com.apple.settings.PrivacySecurity.extension"),
        ("Sound", "x-apple.systempreferences:com.apple.Sound-Settings.extension"),
        ("Software Update", "x-apple.systempreferences:com.apple.Software-Update-Settings.extension"),
        ("Storage", "x-apple.systempreferences:com.apple.settings.Storage"),
        ("Time Machine", "x-apple.systempreferences:com.apple.Time-Machine-Settings.extension"),
        ("Users", "x-apple.systempreferences:com.apple.settings.Users"),
        ("Wi-Fi", "x-apple.systempreferences:com.apple.wifi-settings-extension"),
    ]

    @ViewBuilder
    private var settingsFields: some View {
        if case .settings(let paneUrl) = action {
            VStack(alignment: .leading, spacing: 4) {
                Text("Settings panel")
                    .font(.system(size: 12, weight: .medium))
                Picker("", selection: Binding(
                    get: { paneUrl },
                    set: { action = .settings(paneUrl: $0) }
                )) {
                    ForEach(Self.settingsPresets, id: \.url) { preset in
                        Text(preset.label).tag(preset.url)
                    }
                }
                .labelsHidden()
            }
        }
    }

    // MARK: - Post Shortcuts

    private var hasPostShortcuts: Bool {
        switch action {
        case .openFile, .openUrl, .openApp: return true
        default: return false
        }
    }

    private var currentShortcuts: [KeyCombo] {
        switch action {
        case .openFile(_, _, let s, _): return s
        case .openUrl(_, _, let s, _): return s
        case .openApp(_, let s, _): return s
        default: return []
        }
    }

    private var currentDelay: UInt64 {
        switch action {
        case .openFile(_, _, _, let d): return d
        case .openUrl(_, _, _, let d): return d
        case .openApp(_, _, let d): return d
        default: return 1
        }
    }

    @ViewBuilder
    private var postShortcutsSection: some View {
        let shortcuts = currentShortcuts

        VStack(alignment: .leading, spacing: 8) {
            Divider()

            if shortcuts.isEmpty {
                Button("+ Run shortcuts after open") {
                    addShortcut()
                }
                .font(.system(size: 13))
                .buttonStyle(.borderless)
            } else {
                Text("SHORTCUTS AFTER OPEN")
                    .font(.system(size: 10, weight: .medium))
                    .tracking(1.5)
                    .foregroundStyle(.secondary)

                ForEach(Array(shortcuts.enumerated()), id: \.offset) { index, combo in
                    HStack(spacing: 6) {
                        ForEach([Modifier.cmd, .shift, .opt, .ctrl], id: \.self) { mod in
                            Button(modLabel(mod)) {
                                toggleModifier(at: index, mod: mod)
                            }
                            .buttonStyle(.bordered)
                            .tint(combo.modifiers.contains(mod) ? .accentColor : nil)
                            .controlSize(.small)
                            .font(.system(size: 10))
                        }

                        TextField("key", text: Binding(
                            get: { combo.key },
                            set: { updateShortcutKey(at: index, key: $0) }
                        ))
                        .textFieldStyle(.roundedBorder)
                        .frame(width: 60)
                        .font(.system(size: 12, design: .monospaced))

                        Button {
                            removeShortcut(at: index)
                        } label: {
                            Image(systemName: "xmark")
                                .font(.system(size: 10))
                                .foregroundStyle(.secondary)
                        }
                        .buttonStyle(.borderless)
                    }
                }

                Button {
                    addShortcut()
                } label: {
                    Label("Add shortcut", systemImage: "plus")
                        .font(.system(size: 12))
                }
                .buttonStyle(.bordered)
                .controlSize(.small)
                .frame(maxWidth: .infinity)

                // Delay
                HStack(spacing: 4) {
                    Text("Wait")
                        .font(.system(size: 11))
                        .foregroundStyle(.secondary)
                    TextField("", value: Binding(
                        get: { Int(currentDelay) },
                        set: { setDelay(UInt64(max(0, $0))) }
                    ), format: .number)
                    .textFieldStyle(.roundedBorder)
                    .frame(width: 50)
                    .multilineTextAlignment(.center)
                    Text("seconds between each step")
                        .font(.system(size: 11))
                        .foregroundStyle(.secondary)
                }
            }
        }
    }

    // MARK: - Shortcut Helpers

    private func addShortcut() {
        let newCombo = KeyCombo(modifiers: [], key: "")
        updateShortcuts(currentShortcuts + [newCombo])
    }

    private func removeShortcut(at index: Int) {
        var s = currentShortcuts
        s.remove(at: index)
        updateShortcuts(s)
    }

    private func toggleModifier(at index: Int, mod: Modifier) {
        var s = currentShortcuts
        var combo = s[index]
        if combo.modifiers.contains(mod) {
            combo.modifiers.removeAll { $0 == mod }
        } else {
            combo.modifiers.append(mod)
        }
        s[index] = combo
        updateShortcuts(s)
    }

    private func updateShortcutKey(at index: Int, key: String) {
        var s = currentShortcuts
        s[index] = KeyCombo(modifiers: s[index].modifiers, key: key)
        updateShortcuts(s)
    }

    private func updateShortcuts(_ shortcuts: [KeyCombo]) {
        switch action {
        case .openFile(let path, let app, _, let delay):
            action = .openFile(path: path, app: app, postShortcuts: shortcuts, shortcutDelaySecs: delay)
        case .openUrl(let url, let browser, _, let delay):
            action = .openUrl(url: url, browser: browser, postShortcuts: shortcuts, shortcutDelaySecs: delay)
        case .openApp(let appPath, _, let delay):
            action = .openApp(appPath: appPath, postShortcuts: shortcuts, shortcutDelaySecs: delay)
        default: break
        }
    }

    private func setDelay(_ delay: UInt64) {
        switch action {
        case .openFile(let path, let app, let shortcuts, _):
            action = .openFile(path: path, app: app, postShortcuts: shortcuts, shortcutDelaySecs: delay)
        case .openUrl(let url, let browser, let shortcuts, _):
            action = .openUrl(url: url, browser: browser, postShortcuts: shortcuts, shortcutDelaySecs: delay)
        case .openApp(let appPath, let shortcuts, _):
            action = .openApp(appPath: appPath, postShortcuts: shortcuts, shortcutDelaySecs: delay)
        default: break
        }
    }

    // MARK: - Type Change

    private func handleTypeChange(_ tag: ActionTypeTag) {
        switch tag {
        case .openFile:
            action = .openFile(path: "", app: nil, postShortcuts: [], shortcutDelaySecs: 1)
        case .openUrl:
            action = .openUrl(url: "", browser: nil, postShortcuts: [], shortcutDelaySecs: 1)
        case .openApp:
            action = .openApp(appPath: "", postShortcuts: [], shortcutDelaySecs: 1)
        case .runCommand:
            action = .runCommand(command: "", args: [], shell: .zsh)
        case .notify:
            action = .notify(title: "", body: "", sound: true)
        case .webhook:
            action = .webhook(url: "", method: .get, headers: [:], body: nil)
        case .settings:
            action = .settings(paneUrl: "x-apple.systempreferences:com.apple.settings.General")
        }
    }

    // MARK: - Header Helpers

    private func headersToText(_ headers: [String: String]) -> String {
        headers.map { "\($0.key)=\($0.value)" }.joined(separator: "\n")
    }

    private func textToHeaders(_ text: String) -> [String: String] {
        var result: [String: String] = [:]
        for line in text.split(separator: "\n", omittingEmptySubsequences: false) {
            if let idx = line.firstIndex(of: "="), idx > line.startIndex {
                let key = line[line.startIndex..<idx].trimmingCharacters(in: .whitespaces)
                let value = line[line.index(after: idx)...].trimmingCharacters(in: .whitespaces)
                if !key.isEmpty { result[key] = value }
            }
        }
        return result
    }

    private func modLabel(_ mod: Modifier) -> String {
        switch mod {
        case .cmd: return "Cmd"
        case .shift: return "Shift"
        case .opt: return "Opt"
        case .ctrl: return "Ctrl"
        }
    }
}

// MARK: - ActionTypeTag

enum ActionTypeTag: String, CaseIterable, Identifiable {
    case openUrl, openFile, openApp, runCommand, notify, webhook, settings

    var id: String { rawValue }

    var label: String {
        switch self {
        case .openUrl: return "URL"
        case .openFile: return "File"
        case .openApp: return "App"
        case .runCommand: return "Cmd"
        case .notify: return "Notify"
        case .webhook: return "Hook"
        case .settings: return "Settings"
        }
    }
}
