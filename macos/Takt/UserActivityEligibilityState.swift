import Foundation

enum UserActivityEligibilitySource: CaseIterable, Hashable, Sendable {
    case workspaceSession
    case distributedScreenLock
    case systemSleep
    case displaySleep
    case screenSaver
}

struct UserActivityRecoveryToken: Equatable, Sendable {
    fileprivate let source: UserActivityEligibilitySource
    fileprivate let epoch: UInt64
    fileprivate let sequence: UInt64
}

struct UserActivityEligibilitySnapshot: Sendable {
    let sessionActive: Bool
    let eligibilityGeneration: UInt64
    let inputEventCount: UInt64
    let eligibilityInputEventCount: UInt64
}

/// Owns source eligibility and recovery epochs under one lock.
final class UserActivityEligibilityState: @unchecked Sendable {
    typealias InputEventCountSource = @Sendable () -> UInt64

    private struct RecoveryBoundary {
        let token: UserActivityRecoveryToken
        let inputEventCount: UInt64
    }

    private struct SourceState {
        var eligible: Bool
        var epoch: UInt64 = 0
        var recoveryBoundary: RecoveryBoundary?
    }

    private let lock = NSLock()
    private let inputEventCount: InputEventCountSource
    private var sources: [UserActivityEligibilitySource: SourceState] = [
        .workspaceSession: SourceState(eligible: false),
        .distributedScreenLock: SourceState(eligible: false),
        .systemSleep: SourceState(eligible: true),
        .displaySleep: SourceState(eligible: false),
        .screenSaver: SourceState(eligible: false),
    ]
    private var generation: UInt64 = 0
    private var eligibilityInputEventCount: UInt64 = 0
    private var recoverySequence: UInt64 = 0

    init(inputEventCount: @escaping InputEventCountSource) {
        self.inputEventCount = inputEventCount
    }

    func signalLoss(_ source: UserActivityEligibilitySource) {
        lock.lock()
        signalLossLocked(source)
        lock.unlock()
    }

    func captureRecovery(
        _ source: UserActivityEligibilitySource
    ) -> UserActivityRecoveryToken? {
        lock.lock()
        defer { lock.unlock() }
        return captureRecoveryLocked(source)
    }

    func applyNotifiedObservation(
        _ token: UserActivityRecoveryToken,
        systemAwake: Bool? = nil,
        sessionActive: Bool,
        screenSaverActive: Bool,
        anyDisplayAwake: Bool
    ) {
        lock.lock()
        defer { lock.unlock() }

        guard boundaryLocked(for: token) != nil else { return }

        let observation = NativeObservation(
            systemAwake: systemAwake,
            sessionActive: sessionActive,
            screenSaverActive: screenSaverActive,
            anyDisplayAwake: anyDisplayAwake
        )
        guard observation.isEligible(token.source) else {
            signalLossLocked(token.source)
            return
        }

        applyObservedLossesLocked(observation)
        guard boundaryLocked(for: token) != nil else { return }

        for source in token.source.reconciledRecoverySources {
            setEligibleLocked(source)
        }
        finalizeEligibilityTransitionLocked()
    }

    func applyPolledObservation(
        systemAwake: Bool? = nil,
        sessionActive: Bool,
        screenSaverActive: Bool,
        anyDisplayAwake: Bool
    ) {
        lock.lock()
        defer { lock.unlock() }

        let observation = NativeObservation(
            systemAwake: systemAwake,
            sessionActive: sessionActive,
            screenSaverActive: screenSaverActive,
            anyDisplayAwake: anyDisplayAwake
        )
        applyObservedLossesLocked(observation)

        for source in observation.observedSources where observation.isEligible(source) {
            guard sources[source]!.eligible == false else { continue }
            if sources[source]!.recoveryBoundary == nil {
                _ = captureRecoveryLocked(source)
            }
            setEligibleLocked(source)
        }
        finalizeEligibilityTransitionLocked()
    }

    func snapshot() -> UserActivityEligibilitySnapshot {
        lock.lock()
        defer { lock.unlock() }

        return UserActivityEligibilitySnapshot(
            sessionActive: isEligibleLocked,
            eligibilityGeneration: generation,
            inputEventCount: inputEventCount(),
            eligibilityInputEventCount: eligibilityInputEventCount
        )
    }

    private var isEligibleLocked: Bool {
        UserActivityEligibilitySource.allCases.allSatisfy { sources[$0]!.eligible }
    }

    private func signalLossLocked(_ source: UserActivityEligibilitySource) {
        let aggregateWasEligible = isEligibleLocked
        var state = sources[source]!

        guard state.eligible || state.recoveryBoundary != nil else { return }

        state.eligible = false
        state.epoch = incremented(state.epoch)
        state.recoveryBoundary = nil
        sources[source] = state

        if aggregateWasEligible {
            generation = incremented(generation)
        }
    }

    private func captureRecoveryLocked(
        _ source: UserActivityEligibilitySource
    ) -> UserActivityRecoveryToken? {
        var state = sources[source]!
        guard !state.eligible, state.recoveryBoundary == nil else { return nil }

        recoverySequence = incremented(recoverySequence)
        let token = UserActivityRecoveryToken(
            source: source,
            epoch: state.epoch,
            sequence: recoverySequence
        )
        state.recoveryBoundary = RecoveryBoundary(
            token: token,
            inputEventCount: inputEventCount()
        )
        sources[source] = state
        return token
    }

    private func boundaryLocked(
        for token: UserActivityRecoveryToken
    ) -> RecoveryBoundary? {
        let state = sources[token.source]!
        guard state.epoch == token.epoch,
              state.recoveryBoundary?.token == token else {
            return nil
        }
        return state.recoveryBoundary
    }

    private func applyObservedLossesLocked(_ observation: NativeObservation) {
        for source in observation.observedSources where !observation.isEligible(source) {
            signalLossLocked(source)
        }
    }

    private func setEligibleLocked(_ source: UserActivityEligibilitySource) {
        var state = sources[source]!
        state.eligible = true
        sources[source] = state
    }

    private func finalizeEligibilityTransitionLocked() {
        guard isEligibleLocked,
              let latestBoundary = sources.values
                  .compactMap(\.recoveryBoundary)
                  .max(by: { lhs, rhs in lhs.token.sequence < rhs.token.sequence }) else {
            return
        }
        eligibilityInputEventCount = latestBoundary.inputEventCount

        for source in UserActivityEligibilitySource.allCases {
            sources[source]!.recoveryBoundary = nil
        }
    }

    private func incremented(_ value: UInt64) -> UInt64 {
        value == UInt64.max ? UInt64.max : value + 1
    }
}

private struct NativeObservation {
    let systemAwake: Bool?
    let sessionActive: Bool
    let screenSaverActive: Bool
    let anyDisplayAwake: Bool

    var observedSources: [UserActivityEligibilitySource] {
        var result: [UserActivityEligibilitySource] = [
            .workspaceSession,
            .distributedScreenLock,
            .displaySleep,
            .screenSaver,
        ]
        if systemAwake != nil {
            result.append(.systemSleep)
        }
        return result
    }

    func isEligible(_ source: UserActivityEligibilitySource) -> Bool {
        switch source {
        case .workspaceSession, .distributedScreenLock:
            sessionActive
        case .systemSleep:
            systemAwake == true
        case .displaySleep:
            anyDisplayAwake
        case .screenSaver:
            !screenSaverActive
        }
    }
}

private extension UserActivityEligibilitySource {
    var reconciledRecoverySources: [UserActivityEligibilitySource] {
        switch self {
        case .workspaceSession, .distributedScreenLock:
            [.workspaceSession, .distributedScreenLock]
        case .systemSleep:
            [.systemSleep]
        case .displaySleep:
            [.displaySleep]
        case .screenSaver:
            [.screenSaver]
        }
    }
}
