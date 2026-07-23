import AppKit
import CoreGraphics
import Foundation

/// Observes native macOS eligibility and exposes lock-protected snapshots to
/// Rust scheduler threads. AppKit queries stay on main; notification callbacks
/// update scalar losses and capture recovery boundaries synchronously.
final class UserActivityMonitor: @unchecked Sendable {

    /// Owns AppKit/Foundation resources that must be released on the main thread.
    /// The unchecked conformance is limited to transferring an immutable cleanup
    /// payload to the main queue; resource access itself remains main-thread-only.
    private final class MainThreadResources: @unchecked Sendable {
        private var timer: Timer?
        private var workspaceObserverTokens: [NSObjectProtocol] = []
        private var applicationObserverTokens: [NSObjectProtocol] = []
        private var distributedObserverTokens: [NSObjectProtocol] = []
        private let lock = NSLock()
        private var cleanupStarted = false

        func retainTimer(_ timer: Timer) {
            dispatchPrecondition(condition: .onQueue(.main))
            self.timer = timer
        }

        func retainWorkspaceObserver(_ token: NSObjectProtocol) {
            dispatchPrecondition(condition: .onQueue(.main))
            workspaceObserverTokens.append(token)
        }

        func retainApplicationObserver(_ token: NSObjectProtocol) {
            dispatchPrecondition(condition: .onQueue(.main))
            applicationObserverTokens.append(token)
        }

        func retainDistributedObserver(_ token: NSObjectProtocol) {
            dispatchPrecondition(condition: .onQueue(.main))
            distributedObserverTokens.append(token)
        }

        func cleanup() {
            let payload: MainThreadCleanup?

            lock.lock()
            if cleanupStarted {
                payload = nil
            } else {
                cleanupStarted = true
                payload = MainThreadCleanup(
                    timer: timer,
                    workspaceObserverTokens: workspaceObserverTokens,
                    applicationObserverTokens: applicationObserverTokens,
                    distributedObserverTokens: distributedObserverTokens
                )
                timer = nil
                workspaceObserverTokens.removeAll()
                applicationObserverTokens.removeAll()
                distributedObserverTokens.removeAll()
            }
            lock.unlock()

            payload?.perform()
        }

        deinit {
            cleanup()
        }
    }

    /// Sendable transfer object. Its contents are consumed once on the main
    /// thread, where timer invalidation and observer removal are valid.
    private final class MainThreadCleanup: @unchecked Sendable {
        private let timer: Timer?
        private let workspaceObserverTokens: [NSObjectProtocol]
        private let applicationObserverTokens: [NSObjectProtocol]
        private let distributedObserverTokens: [NSObjectProtocol]

        init(
            timer: Timer?,
            workspaceObserverTokens: [NSObjectProtocol],
            applicationObserverTokens: [NSObjectProtocol],
            distributedObserverTokens: [NSObjectProtocol]
        ) {
            self.timer = timer
            self.workspaceObserverTokens = workspaceObserverTokens
            self.applicationObserverTokens = applicationObserverTokens
            self.distributedObserverTokens = distributedObserverTokens
        }

        func perform() {
            let payload = self
            DispatchQueue.main.async {
                payload.performOnMainThread()
            }
        }

        private func performOnMainThread() {
            dispatchPrecondition(condition: .onQueue(.main))

            timer?.invalidate()

            let workspaceCenter = NSWorkspace.shared.notificationCenter
            workspaceObserverTokens.forEach(workspaceCenter.removeObserver)

            let applicationCenter = NotificationCenter.default
            applicationObserverTokens.forEach(applicationCenter.removeObserver)

            let distributedCenter = DistributedNotificationCenter.default()
            distributedObserverTokens.forEach(distributedCenter.removeObserver)
        }
    }

    // macOS does not publish supported screen saver or lock notification names.
    // These de facto distributed names only accelerate updates; initialization
    // and recovery notifications always re-read current session/display state.
    private enum DistributedName {
        static let screenSaverDidStart = Notification.Name("com.apple.screensaver.didstart")
        static let screenSaverDidStop = Notification.Name("com.apple.screensaver.didstop")
        static let screenDidLock = Notification.Name("com.apple.screenIsLocked")
        static let screenDidUnlock = Notification.Name("com.apple.screenIsUnlocked")
    }

    private static let screenSaverBundleIdentifier = "com.apple.ScreenSaver.Engine"
    private static let stateRefreshInterval: TimeInterval = 10

    private struct InputObservation {
        let eventCount: UInt64
        let lastInputAtUnixMillis: Int64?
    }

    // Use input-start and movement events only. Release events can arrive after
    // an unlock/wake notification for the same gesture and must not look like a
    // separate post-eligibility input.
    private static let inputEventTypes: [CGEventType] = [
        .keyDown,
        .leftMouseDown,
        .rightMouseDown,
        .mouseMoved,
        .leftMouseDragged,
        .rightMouseDragged,
        .otherMouseDown,
        .otherMouseDragged,
        .scrollWheel,
    ]

    private let resources = MainThreadResources()
    private let eligibilityState: UserActivityEligibilityState

    init(
        inputEventCount: @escaping UserActivityEligibilityState.InputEventCountSource = {
            UserActivityMonitor.currentInputEventCount()
        }
    ) {
        dispatchPrecondition(condition: .onQueue(.main))
        eligibilityState = UserActivityEligibilityState(inputEventCount: inputEventCount)
        registerObservers()
        refreshCurrentState()
        startStateRefreshTimer()
    }

    deinit {
        resources.cleanup()
    }

    func snapshot() -> UserActivitySnapshot {
        let eligibility = eligibilityState.snapshot()
        let input = Self.currentInputObservation(eventCount: eligibility.inputEventCount)
        return UserActivitySnapshot(
            sessionActive: eligibility.sessionActive,
            eligibilityGeneration: eligibility.eligibilityGeneration,
            inputEventCount: input.eventCount,
            eligibilityInputEventCount: eligibility.eligibilityInputEventCount,
            lastInputAtUnixMillis: input.lastInputAtUnixMillis
        )
    }

    private func observeEligibilityLoss(_ source: UserActivityEligibilitySource) {
        eligibilityState.signalLoss(source)
    }

    private func observeEligibilityRecovery(_ source: UserActivityEligibilitySource) {
        guard let token = eligibilityState.captureRecovery(source) else { return }

        // queue:nil makes capture synchronous. Source epochs follow the order in
        // which callbacks enter the state lock; no ordering across centers is used.
        DispatchQueue.main.async { [weak self] in
            self?.refreshCurrentState(
                recoveryToken: token,
                systemAwake: source == .systemSleep ? true : nil
            )
        }
    }

    private func observeScreenParametersChanged() {
        let token = eligibilityState.captureRecovery(.displaySleep)
        DispatchQueue.main.async { [weak self] in
            self?.refreshCurrentState(recoveryToken: token)
        }
    }

    private func registerObservers() {
        dispatchPrecondition(condition: .onQueue(.main))

        let workspace = NSWorkspace.shared
        let workspaceCenter = workspace.notificationCenter

        resources.retainWorkspaceObserver(workspaceCenter.addObserver(
            forName: NSWorkspace.sessionDidResignActiveNotification,
            object: workspace,
            queue: nil
        ) { [weak self] _ in
            self?.observeEligibilityLoss(.workspaceSession)
        })

        resources.retainWorkspaceObserver(workspaceCenter.addObserver(
            forName: NSWorkspace.sessionDidBecomeActiveNotification,
            object: workspace,
            queue: nil
        ) { [weak self] _ in
            self?.observeEligibilityRecovery(.workspaceSession)
        })

        resources.retainWorkspaceObserver(workspaceCenter.addObserver(
            forName: NSWorkspace.screensDidSleepNotification,
            object: workspace,
            queue: nil
        ) { [weak self] _ in
            self?.observeEligibilityLoss(.displaySleep)
        })

        resources.retainWorkspaceObserver(workspaceCenter.addObserver(
            forName: NSWorkspace.screensDidWakeNotification,
            object: workspace,
            queue: nil
        ) { [weak self] _ in
            self?.observeEligibilityRecovery(.displaySleep)
        })

        resources.retainWorkspaceObserver(workspaceCenter.addObserver(
            forName: NSWorkspace.willSleepNotification,
            object: workspace,
            queue: nil
        ) { [weak self] _ in
            self?.observeEligibilityLoss(.systemSleep)
            self?.observeEligibilityLoss(.displaySleep)
        })

        resources.retainWorkspaceObserver(workspaceCenter.addObserver(
            forName: NSWorkspace.didWakeNotification,
            object: workspace,
            queue: nil
        ) { [weak self] _ in
            self?.observeEligibilityRecovery(.systemSleep)
        })

        let applicationCenter = NotificationCenter.default
        resources.retainApplicationObserver(applicationCenter.addObserver(
            forName: NSApplication.didChangeScreenParametersNotification,
            object: nil,
            queue: nil
        ) { [weak self] _ in
            self?.observeScreenParametersChanged()
        })

        let distributedCenter = DistributedNotificationCenter.default()
        resources.retainDistributedObserver(distributedCenter.addObserver(
            forName: DistributedName.screenDidLock,
            object: nil,
            queue: nil
        ) { [weak self] _ in
            self?.observeEligibilityLoss(.distributedScreenLock)
        })

        resources.retainDistributedObserver(distributedCenter.addObserver(
            forName: DistributedName.screenDidUnlock,
            object: nil,
            queue: nil
        ) { [weak self] _ in
            self?.observeEligibilityRecovery(.distributedScreenLock)
        })

        resources.retainDistributedObserver(distributedCenter.addObserver(
            forName: DistributedName.screenSaverDidStart,
            object: nil,
            queue: nil
        ) { [weak self] _ in
            self?.observeEligibilityLoss(.screenSaver)
        })

        resources.retainDistributedObserver(distributedCenter.addObserver(
            forName: DistributedName.screenSaverDidStop,
            object: nil,
            queue: nil
        ) { [weak self] _ in
            self?.observeEligibilityRecovery(.screenSaver)
        })
    }

    private func startStateRefreshTimer() {
        dispatchPrecondition(condition: .onQueue(.main))

        let timer = Timer(timeInterval: Self.stateRefreshInterval, repeats: true) { [weak self] _ in
            self?.refreshCurrentState()
        }
        RunLoop.main.add(timer, forMode: .common)
        resources.retainTimer(timer)
    }

    private func refreshCurrentState(
        recoveryToken: UserActivityRecoveryToken? = nil,
        systemAwake: Bool? = nil
    ) {
        dispatchPrecondition(condition: .onQueue(.main))

        let sessionActive = Self.currentSessionIsActive()
        let screenSaverActive = Self.currentScreenSaverIsActive()
        let anyDisplayAwake = Self.currentDisplayIsAwake()

        if let recoveryToken {
            eligibilityState.applyNotifiedObservation(
                recoveryToken,
                systemAwake: systemAwake,
                sessionActive: sessionActive,
                screenSaverActive: screenSaverActive,
                anyDisplayAwake: anyDisplayAwake
            )
        } else {
            eligibilityState.applyPolledObservation(
                systemAwake: systemAwake,
                sessionActive: sessionActive,
                screenSaverActive: screenSaverActive,
                anyDisplayAwake: anyDisplayAwake
            )
        }
    }

    private static func currentSessionIsActive() -> Bool {
        guard let session = CGSessionCopyCurrentDictionary() as? [String: Any],
              session["kCGSSessionOnConsoleKey"] as? Bool == true,
              session["kCGSessionLoginDoneKey"] as? Bool == true,
              session["kCGSSessionUserIDKey"] as? NSNumber != nil,
              let userName = session["kCGSSessionUserNameKey"] as? String,
              !userName.isEmpty else {
            return false
        }

        // CGSSessionScreenIsLocked is not a public SDK constant. WindowServer
        // omits it while unlocked and reports true while the session is locked.
        if let lockValue = session["CGSSessionScreenIsLocked"] {
            guard let isLocked = lockValue as? Bool else { return false }
            return !isLocked
        }
        return true
    }

    private static func currentScreenSaverIsActive() -> Bool {
        !NSRunningApplication.runningApplications(
            withBundleIdentifier: screenSaverBundleIdentifier
        ).isEmpty
    }

    private static func currentDisplayIsAwake() -> Bool {
        var displayCount: UInt32 = 0
        guard CGGetOnlineDisplayList(0, nil, &displayCount) == .success,
              displayCount > 0 else {
            return false
        }

        var displays = Array(repeating: CGDirectDisplayID(), count: Int(displayCount))
        guard CGGetOnlineDisplayList(displayCount, &displays, &displayCount) == .success else {
            return false
        }

        return displays.prefix(Int(displayCount)).contains {
            CGDisplayIsOnline($0) != 0 && CGDisplayIsAsleep($0) == 0
        }
    }

    private static func currentInputObservation(eventCount: UInt64) -> InputObservation {
        let sampledAtUnixSeconds = Date().timeIntervalSince1970
        let lastInputAge = inputEventTypes.lazy
            .map {
                CGEventSource.secondsSinceLastEventType(
                    .combinedSessionState,
                    eventType: $0
                )
            }
            .filter { $0.isFinite && $0 >= 0 }
            .min()
        let lastInputAtUnixMillis = lastInputAge.flatMap { age -> Int64? in
            let milliseconds = (sampledAtUnixSeconds - age) * 1_000
            guard milliseconds.isFinite,
                  milliseconds >= Double(Int64.min),
                  milliseconds <= Double(Int64.max) else {
                return nil
            }
            return Int64(milliseconds.rounded(.down))
        }

        return InputObservation(
            eventCount: eventCount,
            lastInputAtUnixMillis: lastInputAtUnixMillis
        )
    }

    private static func currentInputEventCount() -> UInt64 {
        inputEventTypes.reduce(into: UInt64(0)) { count, eventType in
            count += UInt64(CGEventSource.counterForEventType(
                .combinedSessionState,
                eventType: eventType
            ))
        }
    }
}
